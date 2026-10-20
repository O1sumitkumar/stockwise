use anyhow::{Context, Result, bail};
use reqwest::header;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36";
const MAX_RETRIES: u32 = 3;
const MIN_REQUEST_INTERVAL_MS: u64 = 200;
const REDIS_KEY_PREFIX: &str = "stockwise:";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6379/";
/// Never sleep longer than this for a single retry, whatever Retry-After says.
const MAX_RETRY_DELAY: Duration = Duration::from_secs(10);

/// Connect to the optional Redis cache.
///
/// `STOCKWISE_REDIS_URL` picks the server (default: local Redis); set it to
/// `off` to disable. Short timeouts mean a missing or hung Redis never slows
/// a command down — StockWise falls back to its in-memory cache.
pub async fn connect_redis() -> Option<redis::aio::MultiplexedConnection> {
    let url = std::env::var("STOCKWISE_REDIS_URL").unwrap_or_else(|_| DEFAULT_REDIS_URL.into());
    if url.is_empty() || url.eq_ignore_ascii_case("off") {
        return None;
    }
    let client = redis::Client::open(url).ok()?;
    let config = redis::AsyncConnectionConfig::new()
        .set_connection_timeout(Duration::from_millis(300))
        .set_response_timeout(Duration::from_millis(500));
    client
        .get_multiplexed_async_connection_with_config(&config)
        .await
        .ok()
}

/// Cache key for a request URL: the per-session `crumb` parameter is dropped
/// so cached quotes survive across runs.
fn cache_key(url: &str) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(url) else {
        return url.to_string();
    };
    let kept: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| k != "crumb")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        parsed.set_query(None);
    } else {
        parsed.query_pairs_mut().clear().extend_pairs(kept);
    }
    parsed.to_string()
}

/// Build `base` + `/segment` + `?query`, percent-encoding every part, so a
/// symbol like `M&M.NS` or a search for "AT&T" can't break the URL.
pub fn build_url(base: &str, segment: Option<&str>, query: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse(base).expect("static base URL is valid");
    if let Some(seg) = segment {
        url.path_segments_mut()
            .expect("base URL can have path segments")
            .push(seg);
    }
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    url.to_string()
}

/// Delay before retry `attempt` (0-based), honouring a server Retry-After.
fn retry_delay(attempt: u32, retry_after: Option<&header::HeaderValue>, base_ms: u64) -> Duration {
    let from_header = retry_after
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    from_header
        .unwrap_or_else(|| Duration::from_millis(base_ms * 2u64.pow(attempt)))
        .min(MAX_RETRY_DELAY)
}

// ── Hybrid Cache: Redis (primary) + In-Memory (fallback) ──

struct CacheEntry {
    data: String,
    expires: Instant,
}

struct HybridCache {
    redis: Option<redis::aio::MultiplexedConnection>,
    memory: HashMap<String, CacheEntry>,
}

impl HybridCache {
    async fn new() -> Self {
        Self {
            redis: connect_redis().await,
            memory: HashMap::new(),
        }
    }

    fn has_redis(&self) -> bool {
        self.redis.is_some()
    }

    async fn get(&mut self, key: &str) -> Option<String> {
        let redis_key = format!("{}{}", REDIS_KEY_PREFIX, key);

        // Try Redis first
        if let Some(ref mut conn) = self.redis {
            match redis::cmd("GET")
                .arg(&redis_key)
                .query_async::<Option<String>>(conn)
                .await
            {
                Ok(Some(v)) => return Some(v),
                Ok(None) => {}
                // Timed out or dropped: stop using Redis for this run.
                Err(_) => self.redis = None,
            }
        }

        // Fallback to memory
        self.memory.get(key).and_then(|e| {
            if Instant::now() < e.expires {
                Some(e.data.clone())
            } else {
                None
            }
        })
    }

    async fn set(&mut self, key: String, data: String, ttl: Duration) {
        let ttl_secs = ttl.as_secs() as i64;
        let redis_key = format!("{}{}", REDIS_KEY_PREFIX, key);

        // Write to Redis
        if let Some(ref mut conn) = self.redis {
            let _: Result<(), _> = redis::cmd("SETEX")
                .arg(&redis_key)
                .arg(ttl_secs.max(1))
                .arg(&data)
                .query_async(conn)
                .await;
        }

        // Also write to memory (fast path for same-session reads)
        if self.memory.len() > 200 {
            let now = Instant::now();
            self.memory.retain(|_, v| now < v.expires);
        }
        self.memory.insert(
            key,
            CacheEntry {
                data,
                expires: Instant::now() + ttl,
            },
        );
    }
}

// ── Data Models ──

#[derive(Debug, Deserialize)]
pub struct QuoteResponse {
    #[serde(rename = "quoteResponse")]
    pub quote_response: QuoteResponseInner,
}

#[derive(Debug, Deserialize)]
pub struct QuoteResponseInner {
    pub result: Vec<Quote>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Quote {
    pub symbol: Option<String>,
    #[serde(rename = "shortName")]
    pub short_name: Option<String>,
    #[serde(rename = "longName")]
    pub long_name: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    pub regular_market_price: Option<f64>,
    #[serde(rename = "regularMarketChange")]
    pub regular_market_change: Option<f64>,
    #[serde(rename = "regularMarketChangePercent")]
    pub regular_market_change_percent: Option<f64>,
    #[serde(rename = "regularMarketVolume")]
    pub regular_market_volume: Option<u64>,
    #[serde(rename = "regularMarketOpen")]
    pub regular_market_open: Option<f64>,
    #[serde(rename = "regularMarketDayHigh")]
    pub regular_market_day_high: Option<f64>,
    #[serde(rename = "regularMarketDayLow")]
    pub regular_market_day_low: Option<f64>,
    #[serde(rename = "regularMarketPreviousClose")]
    pub regular_market_previous_close: Option<f64>,
    #[serde(rename = "fiftyDayAverage")]
    pub fifty_day_average: Option<f64>,
    #[serde(rename = "twoHundredDayAverage")]
    pub two_hundred_day_average: Option<f64>,
    #[serde(rename = "fiftyTwoWeekHigh")]
    pub fifty_two_week_high: Option<f64>,
    #[serde(rename = "fiftyTwoWeekLow")]
    pub fifty_two_week_low: Option<f64>,
    #[serde(rename = "marketCap")]
    pub market_cap: Option<f64>,
    #[serde(rename = "trailingPE")]
    pub trailing_pe: Option<f64>,
    #[serde(rename = "forwardPE")]
    pub forward_pe: Option<f64>,
    #[serde(rename = "trailingAnnualDividendYield")]
    pub trailing_annual_dividend_yield: Option<f64>,
    #[serde(rename = "epsTrailingTwelveMonths")]
    pub eps_trailing_twelve_months: Option<f64>,
    #[serde(rename = "epsForward")]
    pub eps_forward: Option<f64>,
    #[serde(rename = "priceToBook")]
    pub price_to_book: Option<f64>,
    #[serde(rename = "bookValue")]
    pub book_value: Option<f64>,
    pub exchange: Option<String>,
    pub currency: Option<String>,
    #[serde(rename = "averageDailyVolume3Month")]
    pub average_daily_volume_3_month: Option<u64>,
    #[serde(rename = "enterpriseToRevenue")]
    pub enterprise_to_revenue: Option<f64>,
    #[serde(rename = "enterpriseToEbitda")]
    pub enterprise_to_ebitda: Option<f64>,
    #[serde(rename = "revenuePerShare")]
    pub revenue_per_share: Option<f64>,
    #[serde(rename = "returnOnEquity")]
    pub return_on_equity: Option<f64>,
    #[serde(rename = "profitMargins")]
    pub profit_margins: Option<f64>,
    #[serde(rename = "debtToEquity")]
    pub debt_to_equity: Option<f64>,
    #[serde(rename = "currentRatio")]
    pub current_ratio: Option<f64>,
    #[serde(rename = "earningsQuarterlyGrowth")]
    pub earnings_quarterly_growth: Option<f64>,
    #[serde(rename = "revenueGrowth")]
    pub revenue_growth: Option<f64>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    #[serde(rename = "targetMeanPrice")]
    pub target_mean_price: Option<f64>,
    #[serde(rename = "recommendationMean")]
    pub recommendation_mean: Option<f64>,
    #[serde(rename = "recommendationKey")]
    pub recommendation_key: Option<String>,
    #[serde(rename = "numberOfAnalystOpinions")]
    pub number_of_analyst_opinions: Option<u32>,
    pub beta: Option<f64>,
    #[serde(rename = "dividendYield")]
    pub dividend_yield: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ChartResponse {
    pub chart: ChartResult,
}

#[derive(Debug, Deserialize)]
pub struct ChartResult {
    pub result: Option<Vec<ChartData>>,
}

#[derive(Debug, Deserialize)]
pub struct ChartData {
    pub meta: Option<ChartMeta>,
    pub timestamp: Option<Vec<i64>>,
    pub indicators: Indicators,
}

#[derive(Debug, Deserialize)]
pub struct ChartMeta {
    #[serde(rename = "regularMarketPrice")]
    pub regular_market_price: Option<f64>,
    #[serde(rename = "chartPreviousClose")]
    pub chart_previous_close: Option<f64>,
    pub symbol: Option<String>,
    #[serde(rename = "longName")]
    pub long_name: Option<String>,
    #[serde(rename = "shortName")]
    pub short_name: Option<String>,
    #[serde(rename = "regularMarketDayHigh")]
    pub regular_market_day_high: Option<f64>,
    #[serde(rename = "regularMarketDayLow")]
    pub regular_market_day_low: Option<f64>,
    #[serde(rename = "regularMarketVolume")]
    pub regular_market_volume: Option<u64>,
    #[serde(rename = "fiftyTwoWeekHigh")]
    pub fifty_two_week_high: Option<f64>,
    #[serde(rename = "fiftyTwoWeekLow")]
    pub fifty_two_week_low: Option<f64>,
    pub currency: Option<String>,
    #[serde(rename = "exchangeName")]
    pub exchange_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Indicators {
    pub quote: Vec<QuoteIndicator>,
}

#[derive(Debug, Deserialize)]
pub struct QuoteIndicator {
    pub open: Option<Vec<Option<f64>>>,
    pub high: Option<Vec<Option<f64>>>,
    pub low: Option<Vec<Option<f64>>>,
    pub close: Option<Vec<Option<f64>>>,
    pub volume: Option<Vec<Option<u64>>>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub quotes: Vec<SearchResult>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResult {
    pub symbol: Option<String>,
    #[serde(rename = "shortname")]
    pub short_name: Option<String>,
    #[serde(rename = "longname")]
    pub long_name: Option<String>,
    #[serde(rename = "quoteType")]
    pub quote_type: Option<String>,
    pub exchange: Option<String>,
}

// ── Client ──

pub struct YahooClient {
    client: reqwest::Client,
    crumb: String,
    cookie: String,
    last_request: Mutex<Instant>,
    cache: tokio::sync::Mutex<HybridCache>,
}

impl YahooClient {
    pub async fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .cookie_store(true)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        // Step 1: Get cookies — retry up to 3 times
        let mut cookie_str = String::new();
        for attempt in 0..MAX_RETRIES {
            match client.get("https://fc.yahoo.com").send().await {
                Ok(resp) => {
                    let cookies: Vec<String> = resp
                        .headers()
                        .get_all(header::SET_COOKIE)
                        .iter()
                        .filter_map(|v: &header::HeaderValue| v.to_str().ok())
                        .map(|v: &str| v.split(';').next().unwrap_or("").to_string())
                        .collect();
                    cookie_str = cookies.join("; ");
                    break;
                }
                Err(_e) if attempt < MAX_RETRIES - 1 => {
                    let delay = Duration::from_millis(500 * 2u64.pow(attempt));
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => bail!(
                    "Cannot reach Yahoo Finance ({}). Check your internet connection.",
                    e
                ),
            }
        }

        // Step 2: Get crumb — retry
        let mut crumb = String::new();
        for attempt in 0..MAX_RETRIES {
            match client
                .get("https://query2.finance.yahoo.com/v1/test/getcrumb")
                .header(header::COOKIE, &cookie_str)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    crumb = resp.text().await.context("Failed to read crumb")?;
                    // A real crumb is a short token; anything else is an error page.
                    if !crumb.is_empty() && crumb.len() < 64 && !crumb.contains(char::is_whitespace)
                    {
                        break;
                    }
                    if attempt == MAX_RETRIES - 1 {
                        bail!(
                            "Yahoo Finance returned an invalid session token. Try again shortly."
                        );
                    }
                    tokio::time::sleep(retry_delay(attempt, None, 500)).await;
                }
                Ok(resp) => {
                    let status = resp.status();
                    if attempt == MAX_RETRIES - 1 {
                        bail!(
                            "Yahoo Finance refused to start a session (HTTP {}). {}",
                            status,
                            if status.as_u16() == 429 {
                                "You are being rate limited — wait a minute and retry."
                            } else {
                                "Try again shortly."
                            }
                        );
                    }
                    let delay = retry_delay(attempt, resp.headers().get(header::RETRY_AFTER), 500);
                    tokio::time::sleep(delay).await;
                }
                Err(_e) if attempt < MAX_RETRIES - 1 => {
                    tokio::time::sleep(retry_delay(attempt, None, 500)).await;
                }
                Err(e) => bail!(
                    "Cannot reach Yahoo Finance ({}). Check your internet connection.",
                    e
                ),
            }
        }

        let cache = HybridCache::new().await;

        Ok(Self {
            client,
            crumb,
            cookie: cookie_str,
            last_request: Mutex::new(Instant::now() - Duration::from_secs(1)),
            cache: tokio::sync::Mutex::new(cache),
        })
    }

    /// Rate-limited GET with retry and caching (Redis + in-memory)
    async fn fetch(&self, url: &str, cache_ttl: Duration) -> Result<String> {
        // Check cache first (Redis → memory)
        {
            let mut cache = self.cache.lock().await;
            if let Some(cached) = cache.get(&cache_key(url)).await {
                return Ok(cached);
            }
        }

        // Rate limit — reserve the next slot while holding the lock so
        // concurrent callers serialize on a monotonically increasing schedule
        // (the std Mutex is released before the await, never held across it).
        let sleep_until = {
            let mut last = self.last_request.lock().unwrap();
            let min_interval = Duration::from_millis(MIN_REQUEST_INTERVAL_MS);
            let next = (*last + min_interval).max(Instant::now());
            *last = next;
            next
        };
        let now = Instant::now();
        if sleep_until > now {
            tokio::time::sleep(sleep_until - now).await;
        }

        // Yahoo session cookies go to Yahoo only (news also comes from Google).
        let is_yahoo = reqwest::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.host_str()
                    .map(|h| h == "yahoo.com" || h.ends_with(".yahoo.com"))
            })
            .unwrap_or(false);

        // Fetch with retry + exponential backoff
        let mut last_status = None;
        for attempt in 0..MAX_RETRIES {
            let last_attempt = attempt == MAX_RETRIES - 1;
            let mut req = self.client.get(url);
            if is_yahoo {
                req = req.header(header::COOKIE, &self.cookie);
            }
            let result = req.send().await;

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body = resp.text().await.context("Failed to read response body")?;
                        // Cache to Redis + memory
                        let mut cache = self.cache.lock().await;
                        cache.set(cache_key(url), body.clone(), cache_ttl).await;
                        return Ok(body);
                    }
                    last_status = Some(status);
                    if status.as_u16() == 429 {
                        if last_attempt {
                            break;
                        }
                        let delay =
                            retry_delay(attempt + 1, resp.headers().get(header::RETRY_AFTER), 1000);
                        eprintln!(
                            "  Rate limited by Yahoo Finance. Waiting {}s...",
                            delay.as_secs()
                        );
                        tokio::time::sleep(delay).await;
                    } else if status.is_server_error() {
                        if last_attempt {
                            break;
                        }
                        tokio::time::sleep(retry_delay(attempt, None, 500)).await;
                    } else {
                        bail!("Yahoo Finance returned HTTP {}", status);
                    }
                }
                Err(_e) if !last_attempt => {
                    tokio::time::sleep(retry_delay(attempt, None, 500)).await;
                }
                Err(e) => bail!(
                    "Request to Yahoo Finance failed after {} attempts: {}",
                    MAX_RETRIES,
                    e
                ),
            }
        }
        match last_status {
            Some(s) if s.as_u16() == 429 => {
                bail!("Yahoo Finance is rate limiting requests. Wait a minute and try again.")
            }
            Some(s) => bail!(
                "Yahoo Finance returned HTTP {} after {} attempts",
                s,
                MAX_RETRIES
            ),
            None => bail!(
                "Request to Yahoo Finance failed after {} attempts",
                MAX_RETRIES
            ),
        }
    }

    pub async fn get_quote(&self, symbols: &[&str]) -> Result<Vec<Quote>> {
        let joined = symbols.join(",");
        let url = build_url(
            "https://query2.finance.yahoo.com/v7/finance/quote",
            None,
            &[("symbols", &joined), ("crumb", &self.crumb)],
        );

        let body = self.fetch(&url, Duration::from_secs(30)).await?; // cache 30s for quotes
        let resp: QuoteResponse =
            serde_json::from_str(&body).context("Failed to parse quote response")?;
        Ok(resp.quote_response.result)
    }

    pub async fn get_chart(&self, symbol: &str, range: &str, interval: &str) -> Result<ChartData> {
        let url = build_url(
            "https://query1.finance.yahoo.com/v8/finance/chart",
            Some(symbol),
            &[("range", range), ("interval", interval)],
        );

        // Cache chart data longer for bigger timeframes
        let ttl = match range {
            "1d" | "5d" => Duration::from_secs(60),
            "1mo" | "3mo" => Duration::from_secs(300),
            _ => Duration::from_secs(600), // 10 min for 1y+
        };

        let body = self.fetch(&url, ttl).await?;
        let resp: ChartResponse =
            serde_json::from_str(&body).context("Failed to parse chart response")?;

        resp.chart
            .result
            .and_then(|mut r| {
                if r.is_empty() {
                    None
                } else {
                    Some(r.remove(0))
                }
            })
            .context("No chart data returned")
    }

    pub fn crumb(&self) -> &str {
        &self.crumb
    }

    pub async fn raw_get(&self, url: &str) -> Result<serde_json::Value> {
        let body = self.fetch(url, Duration::from_secs(60)).await?;
        let resp: serde_json::Value =
            serde_json::from_str(&body).context("Failed to parse JSON")?;
        Ok(resp)
    }

    /// Fetch raw text (for RSS/XML endpoints)
    pub async fn raw_get_text(&self, url: &str) -> Result<String> {
        self.fetch(url, Duration::from_secs(300)).await
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let url = build_url(
            "https://query2.finance.yahoo.com/v1/finance/search",
            None,
            &[("q", query), ("quotesCount", "10"), ("newsCount", "0")],
        );
        let body = self.fetch(&url, Duration::from_secs(120)).await?;
        let resp: SearchResponse =
            serde_json::from_str(&body).context("Failed to parse search response")?;
        Ok(resp.quotes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_encodes_ampersand_in_symbols() {
        let url = build_url(
            "https://query2.finance.yahoo.com/v7/finance/quote",
            None,
            &[("symbols", "M&M.NS,TCS.NS"), ("crumb", "abc")],
        );
        assert!(url.contains("symbols=M%26M.NS%2CTCS.NS"), "{url}");
        let parsed = reqwest::Url::parse(&url).unwrap();
        let symbols = parsed
            .query_pairs()
            .find(|(k, _)| k == "symbols")
            .unwrap()
            .1;
        assert_eq!(symbols, "M&M.NS,TCS.NS");
    }

    #[test]
    fn build_url_encodes_path_segment() {
        let url = build_url(
            "https://query1.finance.yahoo.com/v8/finance/chart",
            Some("M&M.NS"),
            &[("range", "1mo")],
        );
        let parsed = reqwest::Url::parse(&url).unwrap();
        assert_eq!(
            parsed.path_segments().unwrap().next_back().unwrap(),
            "M&M.NS"
        );
        assert!(url.starts_with("https://query1.finance.yahoo.com/v8/finance/chart/"));
    }

    #[test]
    fn cache_key_drops_crumb() {
        let a = cache_key("https://x.yahoo.com/q?symbols=AAPL&crumb=one");
        let b = cache_key("https://x.yahoo.com/q?symbols=AAPL&crumb=two");
        assert_eq!(a, b);
        assert!(!a.contains("crumb"));
        assert!(a.contains("symbols=AAPL"));
    }

    #[test]
    fn retry_delay_honours_retry_after_but_caps_it() {
        let ra = header::HeaderValue::from_static("3");
        assert_eq!(retry_delay(0, Some(&ra), 500), Duration::from_secs(3));
        let huge = header::HeaderValue::from_static("3600");
        assert_eq!(retry_delay(0, Some(&huge), 500), MAX_RETRY_DELAY);
        assert_eq!(retry_delay(1, None, 500), Duration::from_millis(1000));
    }
}

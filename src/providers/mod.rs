//! Market-data providers behind one trait.
//!
//! Yahoo Finance (`api::YahooClient`) is the default and is always there:
//! it fills every field another provider lacks (fundamentals, 52-week range,
//! names), serves symbols a provider doesn't cover (NSE stocks, indices),
//! and takes over when a provider errors or runs out of free-tier calls.
//!
//! An alternative provider is opt-in: `STOCKWISE_DATA_PROVIDER` names it and
//! its API key comes from the environment. Commands keep calling
//! `YahooClient::get_quote` / `get_chart` / `search`; the client routes to
//! the provider underneath.
//!
//! | Provider      | `STOCKWISE_DATA_PROVIDER` | Key variable          | Covers                         |
//! |---------------|---------------------------|-----------------------|--------------------------------|
//! | Alpha Vantage | `alphavantage`            | `ALPHAVANTAGE_API_KEY`| US, BSE (`.BO`), FX, crypto    |
//! | Massive       | `massive` / `polygon`     | `MASSIVE_API_KEY`     | US stocks, FX, crypto          |
//! | Twelve Data   | `twelvedata`              | `TWELVEDATA_API_KEY`  | US stocks, FX, crypto          |

pub mod alphavantage;
pub mod massive;
pub mod twelvedata;

use crate::api::{ChartData, ChartMeta, Indicators, Quote, QuoteIndicator, SearchResult};
use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

/// A source of quotes, price history and symbol search. Symbols are always
/// Yahoo-style (`AAPL`, `RELIANCE.BO`, `BTC-USD`, `USDINR=X`); each provider
/// maps them to its own syntax and back.
pub trait MarketData {
    /// Human-readable provider name.
    fn name(&self) -> &'static str;

    /// Latest quotes. Symbols the provider can't price are left out.
    fn quotes(&self, symbols: &[&str]) -> impl Future<Output = Result<Vec<Quote>>> + Send;

    /// Price history for a Yahoo `range` (`1mo`, `1y`, ...) and `interval`
    /// (`1d`, `1wk`, `1mo`, ...).
    fn chart(
        &self,
        symbol: &str,
        range: &str,
        interval: &str,
    ) -> impl Future<Output = Result<ChartData>> + Send;

    /// Find symbols by name or ticker.
    fn search(&self, query: &str) -> impl Future<Output = Result<Vec<SearchResult>>> + Send;
}

/// The alternative providers. Yahoo is not listed: it is the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    AlphaVantage,
    Massive,
    TwelveData,
}

impl ProviderKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_', ' '], "")
            .as_str()
        {
            "alphavantage" | "av" => Some(ProviderKind::AlphaVantage),
            "massive" | "polygon" | "polygonio" => Some(ProviderKind::Massive),
            "twelvedata" | "12data" => Some(ProviderKind::TwelveData),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ProviderKind::AlphaVantage => "Alpha Vantage",
            ProviderKind::Massive => "Massive (Polygon.io)",
            ProviderKind::TwelveData => "Twelve Data",
        }
    }

    /// Variables checked for the API key, preferred first.
    pub fn key_envs(self) -> &'static [&'static str] {
        match self {
            ProviderKind::AlphaVantage => &["ALPHAVANTAGE_API_KEY", "ALPHA_VANTAGE_API_KEY"],
            ProviderKind::Massive => &["MASSIVE_API_KEY", "POLYGON_API_KEY"],
            ProviderKind::TwelveData => &["TWELVEDATA_API_KEY", "TWELVE_DATA_API_KEY"],
        }
    }

    pub fn key_env(self) -> &'static str {
        self.key_envs()[0]
    }

    /// Provider requests one command may make before the rest of its
    /// symbols go to Yahoo. Free tiers are tiny (Alpha Vantage: 25 a day;
    /// Massive: 5 a minute; Twelve Data: 8 a minute), so a scan of 30 stocks
    /// must not burn — or wait on — the whole allowance.
    fn per_run_budget(self) -> u32 {
        match self {
            ProviderKind::AlphaVantage => 5,
            ProviderKind::Massive => 5,
            ProviderKind::TwelveData => 8,
        }
    }
}

/// What `STOCKWISE_DATA_PROVIDER` asks for.
pub enum Selection {
    /// Not set (or `yahoo`): Yahoo only.
    Yahoo,
    Ready(Provider),
    /// A provider is named but its key variable is empty.
    MissingKey(ProviderKind),
    /// The value names no provider.
    Unknown(String),
}

impl Selection {
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    fn from_lookup(env: impl Fn(&str) -> Option<String>) -> Self {
        let raw = env("STOCKWISE_DATA_PROVIDER").unwrap_or_default();
        let raw = raw.trim();
        if raw.is_empty() || raw.eq_ignore_ascii_case("yahoo") {
            return Selection::Yahoo;
        }
        let Some(kind) = ProviderKind::parse(raw) else {
            return Selection::Unknown(raw.to_string());
        };
        let key = kind
            .key_envs()
            .iter()
            .filter_map(|k| env(k))
            .map(|k| k.trim().to_string())
            .find(|k| !k.is_empty());
        match key {
            Some(key) => Selection::Ready(Provider::new(kind, key, None)),
            None => Selection::MissingKey(kind),
        }
    }
}

/// A configured alternative provider.
pub enum Provider {
    AlphaVantage(alphavantage::AlphaVantage),
    Massive(massive::Massive),
    TwelveData(twelvedata::TwelveData),
}

impl Provider {
    /// `base_url` overrides the API endpoint (tests use a local mock).
    pub fn new(kind: ProviderKind, key: String, base_url: Option<String>) -> Self {
        let http = |default: &str| {
            Http::new(
                kind,
                key.clone(),
                base_url.clone().unwrap_or_else(|| default.into()),
            )
        };
        match kind {
            ProviderKind::AlphaVantage => Provider::AlphaVantage(alphavantage::AlphaVantage::new(
                http(alphavantage::BASE_URL),
            )),
            ProviderKind::Massive => {
                Provider::Massive(massive::Massive::new(http(massive::BASE_URL)))
            }
            ProviderKind::TwelveData => {
                Provider::TwelveData(twelvedata::TwelveData::new(http(twelvedata::BASE_URL)))
            }
        }
    }

    fn http(&self) -> &Http {
        match self {
            Provider::AlphaVantage(p) => &p.http,
            Provider::Massive(p) => &p.http,
            Provider::TwelveData(p) => &p.http,
        }
    }

    pub fn kind(&self) -> ProviderKind {
        self.http().kind
    }

    /// Share the Redis/in-memory cache, so free-tier calls survive between
    /// runs.
    pub fn with_cache(mut self, cache: crate::api::SharedCache) -> Self {
        match &mut self {
            Provider::AlphaVantage(p) => p.http.cache = Some(cache),
            Provider::Massive(p) => p.http.cache = Some(cache),
            Provider::TwelveData(p) => p.http.cache = Some(cache),
        }
        self
    }

    /// Whether this provider can price `symbol` at all.
    pub fn supports(&self, symbol: &str) -> bool {
        match self {
            Provider::AlphaVantage(_) => alphavantage::target(symbol).is_some(),
            Provider::Massive(_) => massive::ticker(symbol).is_some(),
            Provider::TwelveData(_) => twelvedata::target(symbol).is_some(),
        }
    }

    /// Requests left before this run falls back to Yahoo.
    pub fn remaining(&self) -> u32 {
        let http = self.http();
        if http.exhausted.load(Ordering::Relaxed) {
            0
        } else {
            http.budget.load(Ordering::Relaxed)
        }
    }

    /// A symbol `doctor` asks for to show the provider answers.
    pub fn probe_symbol(&self) -> &'static str {
        "AAPL"
    }
}

impl MarketData for Provider {
    fn name(&self) -> &'static str {
        self.kind().name()
    }

    async fn quotes(&self, symbols: &[&str]) -> Result<Vec<Quote>> {
        let mut out = Vec::new();
        for symbol in symbols {
            let quote = match self {
                Provider::AlphaVantage(p) => p.quote(symbol).await,
                Provider::Massive(p) => p.quote(symbol).await,
                Provider::TwelveData(p) => p.quote(symbol).await,
            };
            match quote {
                Ok(Some(mut q)) => {
                    q.symbol = Some(symbol.to_string());
                    q.data_source = Some(self.name().to_string());
                    out.push(q);
                }
                Ok(None) => {}
                Err(e) => {
                    // Rate limit, refused key or budget gone: keep what we
                    // have, the caller asks Yahoo for the rest.
                    if self.remaining() == 0 {
                        if out.is_empty() {
                            return Err(e);
                        }
                        break;
                    }
                    // A single symbol is worth an error (doctor shows it);
                    // in a list, one bad symbol is skipped.
                    if symbols.len() == 1 {
                        return Err(e);
                    }
                }
            }
        }
        Ok(out)
    }

    async fn chart(&self, symbol: &str, range: &str, interval: &str) -> Result<ChartData> {
        let Some(step) = Resample::for_interval(interval) else {
            bail!("{} serves daily, weekly and monthly bars only", self.name());
        };
        let start = range_start(range, chrono::Utc::now().date_naive());
        let series = match self {
            Provider::AlphaVantage(p) => p.daily(symbol, start).await?,
            Provider::Massive(p) => p.daily(symbol, start).await?,
            Provider::TwelveData(p) => p.daily(symbol, start).await?,
        };
        let series = series.since(start);
        if !series.covers(start) {
            bail!(
                "{} has no history back to {} for {} on this plan",
                self.name(),
                start.map_or("the start".into(), |d| d.to_string()),
                symbol
            );
        }
        Ok(series.resample(step).into_chart(symbol))
    }

    async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        match self {
            Provider::AlphaVantage(p) => p.search(query).await,
            Provider::Massive(p) => p.search(query).await,
            Provider::TwelveData(p) => p.search(query).await,
        }
    }
}

/// Say once per run, on stderr, that a provider stopped answering.
fn notice_once(flag: &AtomicBool, message: impl FnOnce() -> String) {
    if !flag.swap(true, Ordering::Relaxed) {
        eprintln!("  \u{24d8} {}", message());
    }
}

/// Overlay a provider quote on Yahoo's quote for the same symbol. Price and
/// day fields come from the provider; everything it lacks (name, 52-week
/// range, fundamentals, currency) stays Yahoo's. If the provider gave a price
/// but no change, the change is recomputed against Yahoo's previous close so
/// price and change never disagree.
pub fn merge_quote(provider: Quote, yahoo: Option<Quote>) -> Quote {
    let Some(mut q) = yahoo else {
        return provider;
    };
    macro_rules! take {
        ($($field:ident),*) => {
            $(if provider.$field.is_some() { q.$field = provider.$field.clone(); })*
        };
    }
    take!(
        regular_market_price,
        regular_market_open,
        regular_market_day_high,
        regular_market_day_low,
        regular_market_volume,
        regular_market_previous_close,
        fifty_two_week_high,
        fifty_two_week_low,
        average_daily_volume_3_month
    );
    if provider.regular_market_change.is_some() {
        q.regular_market_change = provider.regular_market_change;
        q.regular_market_change_percent = provider.regular_market_change_percent;
    } else if let (Some(price), Some(prev)) =
        (q.regular_market_price, q.regular_market_previous_close)
        && prev != 0.0
    {
        q.regular_market_change = Some(price - prev);
        q.regular_market_change_percent = Some((price - prev) / prev * 100.0);
    }
    if q.currency.is_none() {
        q.currency = provider.currency;
    }
    if q.short_name.is_none() && q.long_name.is_none() {
        q.short_name = provider.short_name;
    }
    if q.exchange.is_none() {
        q.exchange = provider.exchange;
    }
    q.data_source = provider.data_source;
    q
}

// ── HTTP ──

/// HTTP for one provider: shared cache, per-run request budget, and an
/// off-switch once the provider reports its limit. The API key never ends up
/// in a cache key or an error message.
pub struct Http {
    kind: ProviderKind,
    client: reqwest::Client,
    base: String,
    key: String,
    cache: Option<crate::api::SharedCache>,
    budget: AtomicU32,
    exhausted: AtomicBool,
    noticed: AtomicBool,
}

/// How a provider wants its key.
pub enum Auth {
    /// `?name=KEY` (Alpha Vantage has no header option).
    Query(&'static str),
    /// `name: prefix KEY`.
    Header(&'static str, &'static str),
}

impl Http {
    fn new(kind: ProviderKind, key: String, base: String) -> Self {
        Self {
            kind,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(20))
                .build()
                .expect("HTTP client builds"),
            base: base.trim_end_matches('/').to_string(),
            key,
            cache: None,
            budget: AtomicU32::new(kind.per_run_budget()),
            exhausted: AtomicBool::new(false),
            noticed: AtomicBool::new(false),
        }
    }

    /// GET `path?query` and return the JSON body. `check` inspects a parsed
    /// body for the provider's in-band errors (HTTP 200 with an error
    /// message); only bodies that pass are cached.
    pub async fn get_json(
        &self,
        path: &str,
        query: &[(&str, &str)],
        auth: Auth,
        ttl: Duration,
        check: fn(&serde_json::Value) -> Result<(), ProviderError>,
    ) -> Result<serde_json::Value> {
        let name = self.kind.name();
        let mut url = reqwest::Url::parse(&format!("{}{}", self.base, path))
            .with_context(|| format!("bad {} URL", name))?;
        url.query_pairs_mut().extend_pairs(query);
        let cache_key = format!("provider:{}:{}", self.kind.key_env(), url);

        if let Some(cache) = &self.cache
            && let Some(body) = cache.lock().await.get(&cache_key).await
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&body)
        {
            return Ok(json);
        }

        if self.exhausted.load(Ordering::Relaxed) {
            bail!("{} limit reached for this run", name);
        }
        if self
            .budget
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |b| b.checked_sub(1))
            .is_err()
        {
            bail!("{} per-run request budget used up", name);
        }

        let mut send_url = url;
        if let Auth::Query(param) = auth {
            send_url.query_pairs_mut().append_pair(param, &self.key);
        }
        let mut req = self.client.get(send_url);
        if let Auth::Header(header, prefix) = auth {
            req = req.header(header, format!("{}{}", prefix, self.key));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("{} request failed: {}", name, e.without_url()))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("{} response unreadable: {}", name, e.without_url()))?;
        let json: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);

        let verdict = if status.as_u16() == 429 {
            Err(ProviderError::RateLimited(
                error_text(&json).unwrap_or("HTTP 429").into(),
            ))
        } else if status.as_u16() == 401 || status.as_u16() == 403 {
            Err(ProviderError::Denied(
                error_text(&json)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("HTTP {}", status.as_u16())),
            ))
        } else if !status.is_success() {
            Err(ProviderError::Other(
                error_text(&json)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("HTTP {}", status.as_u16())),
            ))
        } else if json.is_null() {
            Err(ProviderError::Other("response is not JSON".into()))
        } else {
            check(&json)
        };

        match verdict {
            Ok(()) => {
                if let Some(cache) = &self.cache {
                    cache.lock().await.set(cache_key, body, ttl).await;
                }
                Ok(json)
            }
            Err(ProviderError::Other(msg)) => bail!("{}: {}", name, redact(&msg, &self.key)),
            Err(e) => {
                // Rate limit or refused key: stop asking for the rest of the run.
                self.exhausted.store(true, Ordering::Relaxed);
                let msg = redact(&e.to_string(), &self.key);
                notice_once(&self.noticed, || {
                    format!("{}: {} — using Yahoo Finance instead", name, msg)
                });
                bail!("{}: {}", name, msg)
            }
        }
    }
}

/// Errors a provider can report inside an HTTP 200 body, or by status.
#[derive(Debug)]
pub enum ProviderError {
    RateLimited(String),
    Denied(String),
    Other(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::RateLimited(m) => write!(f, "rate limit reached ({})", m),
            ProviderError::Denied(m) => write!(f, "access denied ({})", m),
            ProviderError::Other(m) => write!(f, "{}", m),
        }
    }
}

/// A message from a provider error body: `message`, `error`, or the
/// Alpha Vantage `Information` / `Note` / `Error Message` keys.
fn error_text(json: &serde_json::Value) -> Option<&str> {
    ["message", "error", "Information", "Note", "Error Message"]
        .iter()
        .find_map(|k| json[*k].as_str())
}

fn redact(text: &str, key: &str) -> String {
    if key.len() >= 4 {
        text.replace(key, "<key>")
    } else {
        text.to_string()
    }
}

/// Parse a JSON number or numeric string (`"236.3900"`, `"1.83%"`).
fn num(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| {
            v.as_str()
                .and_then(|s| s.trim().trim_end_matches('%').parse::<f64>().ok())
        })
        .filter(|x| x.is_finite())
}

fn int(v: &serde_json::Value) -> Option<u64> {
    num(v).filter(|x| *x >= 0.0).map(|x| x as u64)
}

// ── Daily series → ChartData ──

/// One daily bar, dated (no time of day).
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<u64>,
}

/// Daily bars in date order, plus what the provider said about currency.
#[derive(Clone, Debug, Default)]
pub struct Series {
    pub bars: Vec<Bar>,
    pub currency: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Resample {
    Day,
    Week,
    Month,
}

impl Resample {
    /// Provider charts are built from daily bars; intraday intervals stay on
    /// Yahoo (free tiers don't serve them).
    fn for_interval(interval: &str) -> Option<Self> {
        match interval {
            "1d" => Some(Resample::Day),
            "5d" | "1wk" => Some(Resample::Week),
            "1mo" | "3mo" => Some(Resample::Month),
            _ => None,
        }
    }
}

/// First day a Yahoo `range` covers, or `None` for `max`.
pub fn range_start(range: &str, today: NaiveDate) -> Option<NaiveDate> {
    let days = match range {
        "1d" => 1,
        "5d" | "1w" => 7,
        "1mo" => 31,
        "3mo" => 92,
        "6mo" => 183,
        "1y" => 366,
        "2y" => 731,
        "5y" => 1827,
        "10y" => 3653,
        "ytd" => return NaiveDate::from_ymd_opt(today.year(), 1, 1),
        _ => return None,
    };
    Some(today - chrono::Duration::days(days))
}

impl Series {
    pub fn sorted(mut self) -> Self {
        self.bars.sort_by_key(|b| b.date);
        self.bars.dedup_by_key(|b| b.date);
        self
    }

    fn since(mut self, start: Option<NaiveDate>) -> Self {
        if let Some(start) = start {
            self.bars.retain(|b| b.date >= start);
        }
        self
    }

    /// Whether the bars reach back to `start`. A week of slack covers
    /// weekends and holidays at the start of a range.
    fn covers(&self, start: Option<NaiveDate>) -> bool {
        let Some(first) = self.bars.first() else {
            return false;
        };
        match start {
            Some(start) => first.date <= start + chrono::Duration::days(7),
            None => true,
        }
    }

    fn resample(self, step: Resample) -> Self {
        if step == Resample::Day {
            return self;
        }
        let bucket = |d: NaiveDate| match step {
            Resample::Week => d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64),
            _ => NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d),
        };
        let mut out: Vec<Bar> = Vec::new();
        for bar in self.bars {
            match out.last_mut() {
                Some(last) if bucket(last.date) == bucket(bar.date) => {
                    last.high = max_opt(last.high, bar.high);
                    last.low = min_opt(last.low, bar.low);
                    last.close = bar.close.or(last.close);
                    last.volume = match (last.volume, bar.volume) {
                        (Some(a), Some(b)) => Some(a + b),
                        (a, b) => a.or(b),
                    };
                }
                _ => out.push(bar),
            }
        }
        Self {
            bars: out,
            currency: self.currency,
        }
    }

    /// Yahoo-shaped chart data. Each bar is stamped at 12:00 UTC on its date,
    /// so it shows as the same day in every time zone from UTC-12 to UTC+12.
    fn into_chart(self, symbol: &str) -> ChartData {
        let noon = |d: NaiveDate| {
            d.and_hms_opt(12, 0, 0)
                .map(|t| t.and_utc().timestamp())
                .unwrap_or_default()
        };
        let last_close = self.bars.iter().rev().find_map(|b| b.close);
        ChartData {
            meta: Some(ChartMeta {
                regular_market_price: last_close,
                chart_previous_close: None,
                symbol: Some(symbol.to_string()),
                long_name: None,
                short_name: None,
                regular_market_day_high: self.bars.last().and_then(|b| b.high),
                regular_market_day_low: self.bars.last().and_then(|b| b.low),
                regular_market_volume: self.bars.last().and_then(|b| b.volume),
                fifty_two_week_high: None,
                fifty_two_week_low: None,
                currency: self.currency,
                exchange_name: None,
            }),
            timestamp: Some(self.bars.iter().map(|b| noon(b.date)).collect()),
            indicators: Indicators {
                quote: vec![QuoteIndicator {
                    open: Some(self.bars.iter().map(|b| b.open).collect()),
                    high: Some(self.bars.iter().map(|b| b.high).collect()),
                    low: Some(self.bars.iter().map(|b| b.low).collect()),
                    close: Some(self.bars.iter().map(|b| b.close).collect()),
                    volume: Some(self.bars.iter().map(|b| b.volume).collect()),
                }],
            },
        }
    }
}

fn max_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

fn min_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    }
}

/// A quote from the last two daily bars: the latest bar's prices, with the
/// change against the bar before it.
fn quote_from_bars(series: &Series) -> Option<Quote> {
    let last = series.bars.last()?;
    let prev_close = series
        .bars
        .len()
        .checked_sub(2)
        .and_then(|i| series.bars[i].close);
    let price = last.close?;
    let mut q = Quote {
        regular_market_price: Some(price),
        regular_market_open: last.open,
        regular_market_day_high: last.high,
        regular_market_day_low: last.low,
        regular_market_volume: last.volume,
        regular_market_previous_close: prev_close,
        currency: series.currency.clone(),
        ..Default::default()
    };
    if let Some(prev) = prev_close
        && prev != 0.0
    {
        q.regular_market_change = Some(price - prev);
        q.regular_market_change_percent = Some((price - prev) / prev * 100.0);
    }
    Some(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn bar(date: &str, close: f64) -> Bar {
        Bar {
            date: d(date),
            open: Some(close - 1.0),
            high: Some(close + 2.0),
            low: Some(close - 2.0),
            close: Some(close),
            volume: Some(100),
        }
    }

    #[test]
    fn selection_reads_provider_and_key() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(matches!(Selection::from_lookup(env(&[])), Selection::Yahoo));
        assert!(matches!(
            Selection::from_lookup(env(&[("STOCKWISE_DATA_PROVIDER", "Yahoo")])),
            Selection::Yahoo
        ));
        assert!(matches!(
            Selection::from_lookup(env(&[("STOCKWISE_DATA_PROVIDER", "polygon")])),
            Selection::MissingKey(ProviderKind::Massive)
        ));
        match Selection::from_lookup(env(&[
            ("STOCKWISE_DATA_PROVIDER", "alpha-vantage"),
            ("ALPHA_VANTAGE_API_KEY", " k123 "),
        ])) {
            Selection::Ready(p) => assert_eq!(p.kind(), ProviderKind::AlphaVantage),
            _ => panic!("expected a ready provider"),
        }
        assert!(matches!(
            Selection::from_lookup(env(&[("STOCKWISE_DATA_PROVIDER", "bloomberg")])),
            Selection::Unknown(v) if v == "bloomberg"
        ));
    }

    #[test]
    fn merge_keeps_yahoo_fundamentals_and_takes_provider_prices() {
        let yahoo = Quote {
            symbol: Some("AAPL".into()),
            short_name: Some("Apple Inc.".into()),
            regular_market_price: Some(230.0),
            regular_market_change: Some(1.0),
            regular_market_change_percent: Some(0.4),
            regular_market_previous_close: Some(229.0),
            trailing_pe: Some(35.0),
            currency: Some("USD".into()),
            ..Default::default()
        };
        let provider = Quote {
            symbol: Some("AAPL".into()),
            regular_market_price: Some(236.39),
            regular_market_change: Some(4.25),
            regular_market_change_percent: Some(1.83),
            data_source: Some("Alpha Vantage".into()),
            ..Default::default()
        };
        let q = merge_quote(provider, Some(yahoo.clone()));
        assert_eq!(q.regular_market_price, Some(236.39));
        assert_eq!(q.regular_market_change, Some(4.25));
        assert_eq!(q.trailing_pe, Some(35.0));
        assert_eq!(q.short_name.as_deref(), Some("Apple Inc."));
        assert_eq!(q.data_source.as_deref(), Some("Alpha Vantage"));

        // A provider price without a change is measured against Yahoo's
        // previous close, never left paired with Yahoo's stale change.
        let price_only = Quote {
            regular_market_price: Some(231.29),
            data_source: Some("X".into()),
            ..Default::default()
        };
        let q = merge_quote(price_only, Some(yahoo));
        assert!((q.regular_market_change.unwrap() - 2.29).abs() < 1e-9);
        assert!((q.regular_market_change_percent.unwrap() - 1.0).abs() < 1e-9);

        // No Yahoo quote: the provider's quote stands alone.
        let alone = merge_quote(
            Quote {
                regular_market_price: Some(1.0),
                ..Default::default()
            },
            None,
        );
        assert_eq!(alone.regular_market_price, Some(1.0));
    }

    #[test]
    fn range_start_maps_yahoo_ranges() {
        let today = d("2026-10-21");
        assert_eq!(range_start("1mo", today), Some(d("2026-09-20")));
        assert_eq!(range_start("ytd", today), Some(d("2026-01-01")));
        assert_eq!(range_start("1y", today), Some(d("2025-10-20")));
        assert_eq!(range_start("max", today), None);
    }

    #[test]
    fn coverage_check_detects_short_history() {
        let series = Series {
            bars: vec![bar("2026-06-01", 10.0), bar("2026-10-20", 12.0)],
            currency: None,
        };
        assert!(series.covers(Some(d("2026-05-30"))));
        assert!(!series.covers(Some(d("2025-10-20"))));
        assert!(series.covers(None));
        assert!(!Series::default().covers(None));
    }

    #[test]
    fn resample_builds_weekly_and_monthly_bars() {
        // Mon 2026-10-12 .. Fri 2026-10-16, then Mon 2026-10-19.
        let series = Series {
            bars: vec![
                bar("2026-10-12", 10.0),
                bar("2026-10-14", 14.0),
                bar("2026-10-16", 12.0),
                bar("2026-10-19", 20.0),
            ],
            currency: Some("USD".into()),
        };
        let weekly = series.clone().resample(Resample::Week);
        assert_eq!(weekly.bars.len(), 2);
        let w = &weekly.bars[0];
        assert_eq!(w.date, d("2026-10-12"));
        assert_eq!(w.open, Some(9.0));
        assert_eq!(w.close, Some(12.0));
        assert_eq!(w.high, Some(16.0));
        assert_eq!(w.low, Some(8.0));
        assert_eq!(w.volume, Some(300));
        let monthly = series.resample(Resample::Month);
        assert_eq!(monthly.bars.len(), 1);
        assert_eq!(monthly.bars[0].close, Some(20.0));
    }

    #[test]
    fn chart_is_yahoo_shaped_with_noon_utc_timestamps() {
        let series = Series {
            bars: vec![bar("2026-10-19", 10.0), bar("2026-10-20", 11.0)],
            currency: Some("USD".into()),
        };
        let chart = series.into_chart("AAPL");
        let ts = chart.timestamp.unwrap();
        assert_eq!(ts.len(), 2);
        let first = chrono::DateTime::from_timestamp(ts[0], 0).unwrap();
        assert_eq!(first.to_rfc3339(), "2026-10-19T12:00:00+00:00");
        let meta = chart.meta.unwrap();
        assert_eq!(meta.regular_market_price, Some(11.0));
        assert_eq!(meta.currency.as_deref(), Some("USD"));
        assert_eq!(
            chart.indicators.quote[0].close.as_ref().unwrap()[1],
            Some(11.0)
        );
    }

    #[test]
    fn quote_from_bars_uses_the_last_two_closes() {
        let series = Series {
            bars: vec![bar("2026-10-19", 100.0), bar("2026-10-20", 102.0)],
            currency: Some("USD".into()),
        };
        let q = quote_from_bars(&series).unwrap();
        assert_eq!(q.regular_market_price, Some(102.0));
        assert_eq!(q.regular_market_previous_close, Some(100.0));
        assert_eq!(q.regular_market_change, Some(2.0));
        assert_eq!(q.regular_market_change_percent, Some(2.0));
        assert!(quote_from_bars(&Series::default()).is_none());
    }

    #[test]
    fn numbers_parse_from_strings_and_percentages() {
        assert_eq!(num(&serde_json::json!("236.3900")), Some(236.39));
        assert_eq!(num(&serde_json::json!("1.8308%")), Some(1.8308));
        assert_eq!(num(&serde_json::json!(4.5)), Some(4.5));
        assert_eq!(num(&serde_json::json!("n/a")), None);
        assert_eq!(int(&serde_json::json!("4908740")), Some(4_908_740));
    }

    #[test]
    fn redact_hides_the_key() {
        assert_eq!(
            redact("bad key sk-abcdef here", "sk-abcdef"),
            "bad key <key> here"
        );
    }
}

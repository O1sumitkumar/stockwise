//! What the core commands show, gathered into plain structs.
//!
//! The CLI printers (`commands/`), the TUI and the HTTP server all build on
//! these functions, so `stockwise quote --json` and `GET /api/quote` return
//! the same shape. Nothing here prints. Market data and local files come
//! through [`DataSource`], which tests replace with fixtures.

use crate::alerts::AlertStore;
use crate::api::{ChartData, Quote, SearchResult, YahooClient};
use crate::daemon_status::DaemonStatus;
use crate::market::{self, Market};
use crate::portfolio::Portfolio;
use crate::watchlist::Watchlist;
use anyhow::{Result, bail};
use serde::Serialize;
use std::future::Future;

/// Where market data and the local data files come from: Yahoo Finance and
/// the data directory in production, fixtures in tests.
pub trait DataSource: Send + Sync {
    fn quotes(&self, symbols: &[&str]) -> impl Future<Output = Result<Vec<Quote>>> + Send;
    fn chart(
        &self,
        symbol: &str,
        range: &str,
        interval: &str,
    ) -> impl Future<Output = Result<ChartData>> + Send;
    fn search(&self, query: &str) -> impl Future<Output = Result<Vec<SearchResult>>> + Send;

    fn portfolio(&self) -> Result<Portfolio> {
        Portfolio::load()
    }
    fn watchlist(&self) -> Result<Watchlist> {
        Watchlist::load()
    }
    fn alerts(&self) -> Result<AlertStore> {
        AlertStore::load()
    }
    /// The intraday daemon's last saved state, if it has run.
    fn daemon_status(&self) -> Option<DaemonStatus> {
        crate::daemon_status::load()
    }
}

impl DataSource for YahooClient {
    async fn quotes(&self, symbols: &[&str]) -> Result<Vec<Quote>> {
        self.get_quote(symbols).await
    }
    async fn chart(&self, symbol: &str, range: &str, interval: &str) -> Result<ChartData> {
        self.get_chart(symbol, range, interval).await
    }
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        YahooClient::search(self, query).await
    }
}

/// A [`YahooClient`] opened on first use. Long-running modes (`tui`,
/// `serve`) start without the network and retry the Yahoo session on the
/// next request if opening it fails. All requests share one client, so they
/// share its rate limiter and cache.
#[derive(Default)]
pub struct LiveSource {
    client: tokio::sync::OnceCell<YahooClient>,
}

impl LiveSource {
    pub fn new() -> Self {
        Self::default()
    }

    async fn client(&self) -> Result<&YahooClient> {
        self.client.get_or_try_init(YahooClient::new).await
    }
}

impl DataSource for LiveSource {
    async fn quotes(&self, symbols: &[&str]) -> Result<Vec<Quote>> {
        self.client().await?.get_quote(symbols).await
    }
    async fn chart(&self, symbol: &str, range: &str, interval: &str) -> Result<ChartData> {
        self.client()
            .await?
            .get_chart(symbol, range, interval)
            .await
    }
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        self.client().await?.search(query).await
    }
}

/// "This symbol has no data" — as opposed to Yahoo being unreachable. The
/// server answers 404 for it instead of 502.
#[derive(Debug)]
pub struct NotFound(pub String);

impl std::fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotFound {}

fn find_quote<'a>(quotes: &'a [Quote], symbol: &str) -> Option<&'a Quote> {
    quotes.iter().find(|q| q.symbol.as_deref() == Some(symbol))
}

// ── quote ──

/// Quotes for already-resolved symbols, plus the symbols Yahoo silently
/// dropped (unknown tickers).
pub struct QuoteSet {
    pub quotes: Vec<Quote>,
    pub missing: Vec<String>,
}

pub async fn quotes(src: &impl DataSource, resolved: &[String]) -> Result<QuoteSet> {
    let refs: Vec<&str> = resolved.iter().map(String::as_str).collect();
    let quotes = src.quotes(&refs).await?;
    let missing = resolved
        .iter()
        .filter(|r| {
            !quotes.iter().any(|q| {
                q.symbol
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(r))
            })
        })
        .cloned()
        .collect();
    Ok(QuoteSet { quotes, missing })
}

// ── markets ──

/// One index in `stockwise markets --json` (field order = key order).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IndexRow {
    pub region: String,
    pub symbol: String,
    pub name: String,
    pub price: Option<f64>,
    pub change: Option<f64>,
    pub change_percent: Option<f64>,
    pub currency: Option<String>,
}

/// Index lists in display order: the selected market first.
pub fn index_sections(
    market: Market,
) -> [(&'static str, &'static [(&'static str, &'static str)]); 2] {
    match market {
        Market::Us => [
            ("US Markets", market::US_INDICES),
            ("Indian Markets", market::INDIA_INDICES),
        ],
        Market::In => [
            ("Indian Markets", market::INDIA_INDICES),
            ("US Markets", market::US_INDICES),
        ],
    }
}

/// Rows for one region, in the order Yahoo returned them. Labels are matched
/// by symbol — Yahoo may reorder or drop symbols, so pairing by position
/// could mislabel an index.
pub fn index_rows(region: &str, indices: &[(&str, &str)], quotes: &[Quote]) -> Vec<IndexRow> {
    quotes
        .iter()
        .map(|q| {
            let q_sym = q.symbol.as_deref().unwrap_or("");
            let name = indices
                .iter()
                .find(|(sym, _)| *sym == q_sym)
                .map(|(_, name)| *name)
                .unwrap_or(q_sym);
            IndexRow {
                region: region.to_string(),
                symbol: q_sym.to_string(),
                name: name.to_string(),
                price: q.regular_market_price,
                change: q.regular_market_change,
                change_percent: q.regular_market_change_percent,
                currency: q.currency.clone(),
            }
        })
        .collect()
}

/// Both regions' indices, one request per region, selected market first.
pub async fn market_sections(
    src: &impl DataSource,
    market: Market,
) -> Result<Vec<(&'static str, Vec<IndexRow>)>> {
    let mut out = Vec::new();
    for (title, indices) in index_sections(market) {
        let symbols: Vec<&str> = indices.iter().map(|(sym, _)| *sym).collect();
        let quotes = src.quotes(&symbols).await?;
        out.push((title, index_rows(title, indices, &quotes)));
    }
    Ok(out)
}

pub async fn markets(src: &impl DataSource, market: Market) -> Result<Vec<IndexRow>> {
    Ok(market_sections(src, market)
        .await?
        .into_iter()
        .flat_map(|(_, rows)| rows)
        .collect())
}

// ── history ──

/// `stockwise history --json` (field order = key order).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct History {
    pub symbol: String,
    pub currency: Option<String>,
    pub period: String,
    pub interval: String,
    pub bars: Vec<Bar>,
}

/// One OHLCV bar of `history --json/--csv`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Bar {
    /// RFC 3339 UTC timestamp of the bar.
    pub time: Option<String>,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<u64>,
    /// Unix seconds, for printers; not part of the JSON.
    #[serde(skip)]
    pub ts: i64,
}

/// Yahoo range and bar interval for a `history` period.
pub fn history_range(period: &str) -> (&str, &'static str) {
    match period {
        "1d" => ("1d", "5m"),
        "5d" | "1w" => ("5d", "15m"),
        "1mo" => ("1mo", "1d"),
        "3mo" | "6mo" => (period, "1d"),
        "1y" | "2y" => (period, "1wk"),
        "5y" | "10y" | "max" => (period, "1mo"),
        _ => (period, "1d"),
    }
}

/// Bars from a chart response: one per timestamp, missing values as None.
pub fn history_from_chart(chart: ChartData, symbol: &str, period: &str, interval: &str) -> History {
    let ind = chart.indicators.quote.into_iter().next();
    let (open, high, low, close, volume) = match ind {
        Some(q) => (
            q.open.unwrap_or_default(),
            q.high.unwrap_or_default(),
            q.low.unwrap_or_default(),
            q.close.unwrap_or_default(),
            q.volume.unwrap_or_default(),
        ),
        None => Default::default(),
    };
    let at = |v: &[Option<f64>], i: usize| v.get(i).copied().flatten();
    let bars = chart
        .timestamp
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, ts)| Bar {
            time: chrono::DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()),
            open: at(&open, i),
            high: at(&high, i),
            low: at(&low, i),
            close: at(&close, i),
            volume: volume.get(i).copied().flatten(),
            ts,
        })
        .collect();
    History {
        symbol: symbol.to_string(),
        currency: chart.meta.and_then(|m| m.currency),
        period: period.to_string(),
        interval: interval.to_string(),
        bars,
    }
}

/// Price history for a resolved symbol. No bars is a [`NotFound`] error.
pub async fn history(
    src: &impl DataSource,
    resolved: &str,
    period: &str,
    market: Market,
) -> Result<History> {
    let (range, interval) = history_range(period);
    let chart = src.chart(resolved, range, interval).await?;
    let hist = history_from_chart(chart, resolved, period, interval);
    if hist.bars.is_empty() {
        return Err(NotFound(format!(
            "No price history for {}{}",
            resolved,
            market::not_found_hint(market)
        ))
        .into());
    }
    Ok(hist)
}

// ── portfolio, watchlist, alerts ──
//
// The row builders live with the `portfolio`, `watch` and `alert` printers
// (`commands::holdings`); these wrap them so `/api/*` and the TUI return
// exactly what `--json` prints.

pub(crate) use crate::commands::{AlertCheckRow, HoldingRow, PortfolioTotals, WatchRow};

/// `portfolio --json`: holdings, totals over the priced holdings, and the
/// symbols left out for lack of a price (never valued at 0).
#[derive(Debug, Default, Serialize)]
pub struct PortfolioView {
    pub holdings: Vec<HoldingRow>,
    pub totals: PortfolioTotals,
    pub unpriced: Vec<String>,
}

/// Value `portfolio` at `quotes`. Only the holdings' own quotes are used, so
/// the totals' currency is a holding's, whatever else `quotes` contains.
pub fn portfolio_view(portfolio: &Portfolio, quotes: &[Quote]) -> PortfolioView {
    let own: Vec<Quote> = quotes
        .iter()
        .filter(|q| {
            portfolio
                .holdings
                .iter()
                .any(|h| q.symbol.as_deref() == Some(h.symbol.as_str()))
        })
        .cloned()
        .collect();
    let (holdings, totals) = crate::commands::holding_rows(&portfolio.holdings, &own);
    let unpriced = holdings
        .iter()
        .filter(|r| r.price.is_none())
        .map(|r| r.symbol.clone())
        .collect();
    PortfolioView {
        holdings,
        totals,
        unpriced,
    }
}

/// A local data file (portfolio, watchlist, alerts) could not be read — as
/// opposed to market data. The server answers 500 for it instead of 502.
#[derive(Debug)]
pub struct LocalData;

impl std::fmt::Display for LocalData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Could not read local data")
    }
}

fn local<T>(r: Result<T>) -> Result<T> {
    r.map_err(|e| e.context(LocalData))
}

fn unique_symbols<'a>(groups: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for s in groups {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// Fetch quotes only when there is something to price.
async fn quotes_for(src: &impl DataSource, symbols: &[&str]) -> Result<Vec<Quote>> {
    if symbols.is_empty() {
        Ok(Vec::new())
    } else {
        src.quotes(symbols).await
    }
}

pub async fn portfolio(src: &impl DataSource) -> Result<PortfolioView> {
    let p = local(src.portfolio())?;
    let symbols = unique_symbols(p.holdings.iter().map(|h| h.symbol.as_str()));
    let quotes = quotes_for(src, &symbols).await?;
    Ok(portfolio_view(&p, &quotes))
}

/// `watch --json`: one row per watched symbol, in watchlist order.
pub async fn watchlist(src: &impl DataSource) -> Result<Vec<WatchRow>> {
    let wl = local(src.watchlist())?;
    let symbols = unique_symbols(wl.symbols.iter().map(String::as_str));
    let quotes = quotes_for(src, &symbols).await?;
    Ok(crate::commands::watch_rows(&wl.symbols, &quotes))
}

/// `alert check --json`: every alert with its live price and a `status` of
/// `triggered`, `waiting` or `no_price`.
pub async fn alerts(src: &impl DataSource) -> Result<Vec<AlertCheckRow>> {
    let store = local(src.alerts())?;
    let symbols = unique_symbols(store.alerts.iter().map(|a| a.symbol.as_str()));
    let quotes = quotes_for(src, &symbols).await?;
    Ok(crate::commands::alert_check_rows(&store.alerts, &quotes))
}

// ── dashboard snapshot ──

/// Everything the dashboard shows, priced from a single quote request.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub indices: Vec<IndexRow>,
    pub watchlist: Vec<WatchRow>,
    pub portfolio: PortfolioView,
    pub alerts: Vec<AlertCheckRow>,
}

/// Home-market indices (in list order, unpriced ones kept), watchlist,
/// portfolio and alerts in one batched quote request.
pub async fn snapshot(src: &impl DataSource, market: Market) -> Result<Snapshot> {
    let (region, indices) = index_sections(market)[0];
    let p = local(src.portfolio())?;
    let wl = local(src.watchlist())?;
    let store = local(src.alerts())?;
    let symbols = unique_symbols(
        indices
            .iter()
            .map(|(s, _)| *s)
            .chain(wl.symbols.iter().map(String::as_str))
            .chain(p.holdings.iter().map(|h| h.symbol.as_str()))
            .chain(store.alerts.iter().map(|a| a.symbol.as_str())),
    );
    let quotes = src.quotes(&symbols).await?;
    let indices = indices
        .iter()
        .map(|(sym, name)| {
            let q = find_quote(&quotes, sym);
            IndexRow {
                region: region.to_string(),
                symbol: sym.to_string(),
                name: name.to_string(),
                price: q.and_then(|q| q.regular_market_price),
                change: q.and_then(|q| q.regular_market_change),
                change_percent: q.and_then(|q| q.regular_market_change_percent),
                currency: q.and_then(|q| q.currency.clone()),
            }
        })
        .collect();
    Ok(Snapshot {
        indices,
        watchlist: crate::commands::watch_rows(&wl.symbols, &quotes),
        portfolio: portfolio_view(&p, &quotes),
        alerts: crate::commands::alert_check_rows(&store.alerts, &quotes),
    })
}

/// Validate a `history` period the way the CLI does.
pub fn check_period(period: &str) -> Result<()> {
    if crate::PERIODS.contains(&period) {
        Ok(())
    } else {
        bail!(
            "Unknown period '{}' (use one of {})",
            period,
            crate::PERIODS.join(", ")
        )
    }
}

#[cfg(test)]
pub mod fixtures {
    //! Canned market data for tests of the views, the server and the TUI.

    use super::*;
    use crate::alerts::AlertCondition;
    use crate::api::{ChartMeta, Indicators, QuoteIndicator};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub fn quote(symbol: &str, price: Option<f64>, change_pct: f64, currency: &str) -> Quote {
        Quote {
            symbol: Some(symbol.to_string()),
            short_name: Some(format!("{} Ltd", symbol.trim_end_matches(".NS"))),
            regular_market_price: price,
            regular_market_change: price.map(|p| p * change_pct / 100.0),
            regular_market_change_percent: Some(change_pct),
            regular_market_volume: Some(1_234_567),
            currency: Some(currency.to_string()),
            ..Default::default()
        }
    }

    pub fn chart(closes: &[f64], start_ts: i64, step: i64, currency: &str) -> ChartData {
        let some = |v: &[f64]| Some(v.iter().map(|c| Some(*c)).collect::<Vec<_>>());
        ChartData {
            meta: Some(ChartMeta {
                regular_market_price: closes.last().copied(),
                chart_previous_close: None,
                symbol: None,
                long_name: None,
                short_name: None,
                regular_market_day_high: None,
                regular_market_day_low: None,
                regular_market_volume: None,
                fifty_two_week_high: None,
                fifty_two_week_low: None,
                currency: Some(currency.to_string()),
                exchange_name: None,
            }),
            timestamp: Some(
                (0..closes.len() as i64)
                    .map(|i| start_ts + i * step)
                    .collect(),
            ),
            indicators: Indicators {
                quote: vec![QuoteIndicator {
                    open: some(closes),
                    high: some(&closes.iter().map(|c| c + 1.0).collect::<Vec<_>>()),
                    low: some(&closes.iter().map(|c| c - 1.0).collect::<Vec<_>>()),
                    close: some(closes),
                    volume: Some(closes.iter().map(|_| Some(1000)).collect()),
                }],
            },
        }
    }

    /// A market with a few Indian stocks and the index symbols. Unknown
    /// symbols are dropped, like Yahoo does.
    pub struct Fixture {
        pub quotes: Vec<Quote>,
        pub portfolio: Portfolio,
        pub watchlist: Watchlist,
        pub alerts: AlertStore,
        pub daemon: Option<DaemonStatus>,
        /// Number of quote requests served.
        pub quote_calls: AtomicUsize,
        /// When set, every network call fails with this message.
        pub fail: Mutex<Option<String>>,
    }

    impl Default for Fixture {
        fn default() -> Self {
            let mut portfolio = Portfolio::default();
            portfolio.add("TCS.NS", 10.0, 3500.0);
            portfolio.add("INFY.NS", 5.0, 1600.0);
            portfolio.add("DELISTED.NS", 3.0, 100.0);
            let mut watchlist = Watchlist::default();
            watchlist.add("RELIANCE.NS");
            watchlist.add("TCS.NS");
            let mut alerts = AlertStore::default();
            alerts.add("TCS.NS", AlertCondition::Above, 4000.0);
            alerts.add("RELIANCE.NS", AlertCondition::Below, 1000.0);
            alerts.add("DELISTED.NS", AlertCondition::Below, 1_000_000.0);
            Self {
                quotes: vec![
                    quote("^NSEI", Some(24800.0), 0.5, "INR"),
                    quote("^BSESN", Some(81200.0), 0.4, "INR"),
                    quote("^NSEBANK", Some(52000.0), -0.3, "INR"),
                    quote("^CNXIT", Some(41000.0), 1.1, "INR"),
                    quote("^GSPC", Some(5800.0), 0.2, "USD"),
                    quote("TCS.NS", Some(4100.0), 1.5, "INR"),
                    quote("INFY.NS", Some(1500.0), -2.0, "INR"),
                    quote("RELIANCE.NS", Some(1300.0), 0.8, "INR"),
                ],
                portfolio,
                watchlist,
                alerts,
                daemon: None,
                quote_calls: AtomicUsize::new(0),
                fail: Mutex::new(None),
            }
        }
    }

    impl Fixture {
        fn check(&self) -> Result<()> {
            match self.fail.lock().unwrap().as_ref() {
                Some(msg) => bail!("{}", msg),
                None => Ok(()),
            }
        }
    }

    impl DataSource for Fixture {
        async fn quotes(&self, symbols: &[&str]) -> Result<Vec<Quote>> {
            self.check()?;
            self.quote_calls.fetch_add(1, Ordering::SeqCst);
            Ok(symbols
                .iter()
                .filter_map(|s| find_quote(&self.quotes, s).cloned())
                .collect())
        }
        async fn chart(&self, symbol: &str, _range: &str, interval: &str) -> Result<ChartData> {
            self.check()?;
            if find_quote(&self.quotes, symbol).is_none() {
                bail!("Yahoo Finance returned HTTP 404 Not Found");
            }
            let step = if interval == "1d" { 86_400 } else { 300 };
            Ok(chart(
                &[100.0, 102.0, 101.0, 105.0, 107.5],
                1_700_000_000,
                step,
                "INR",
            ))
        }
        async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
            self.check()?;
            Ok(self
                .quotes
                .iter()
                .filter(|q| {
                    q.symbol
                        .as_deref()
                        .is_some_and(|s| s.to_lowercase().contains(&query.to_lowercase()))
                })
                .map(|q| SearchResult {
                    symbol: q.symbol.clone(),
                    short_name: q.short_name.clone(),
                    long_name: None,
                    quote_type: Some("EQUITY".into()),
                    exchange: Some("NSI".into()),
                })
                .collect())
        }
        fn portfolio(&self) -> Result<Portfolio> {
            Ok(Portfolio {
                holdings: self.portfolio.holdings.clone(),
            })
        }
        fn watchlist(&self) -> Result<Watchlist> {
            Ok(Watchlist {
                symbols: self.watchlist.symbols.clone(),
            })
        }
        fn alerts(&self) -> Result<AlertStore> {
            Ok(AlertStore {
                alerts: self.alerts.alerts.clone(),
            })
        }
        fn daemon_status(&self) -> Option<DaemonStatus> {
            self.daemon.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn block<F: Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn quote_json_keeps_yahoo_field_names() {
        // `quote --json` prints Yahoo's own camelCase quote objects.
        let v = serde_json::to_value(quote("TCS.NS", Some(4100.0), 1.5, "INR")).unwrap();
        for key in [
            "symbol",
            "shortName",
            "longName",
            "regularMarketPrice",
            "regularMarketChange",
            "regularMarketChangePercent",
            "regularMarketVolume",
            "fiftyTwoWeekHigh",
            "marketCap",
            "trailingPE",
            "currency",
        ] {
            assert!(v.get(key).is_some(), "missing key {key}");
        }
        assert_eq!(v["regularMarketPrice"], 4100.0);
        assert!(v["longName"].is_null());
    }

    #[test]
    fn quotes_reports_missing_symbols() {
        let f = Fixture::default();
        let set = block(quotes(&f, &["TCS.NS".into(), "NOPE.NS".into()])).unwrap();
        assert_eq!(set.quotes.len(), 1);
        assert_eq!(set.missing, vec!["NOPE.NS".to_string()]);
    }

    #[test]
    fn markets_json_shape_is_pinned() {
        let rows = block(markets(&Fixture::default(), Market::In)).unwrap();
        let first = serde_json::to_string(&rows[0]).unwrap();
        assert_eq!(
            first,
            r#"{"region":"Indian Markets","symbol":"^NSEI","name":"NIFTY 50","price":24800.0,"change":124.0,"change_percent":0.5,"currency":"INR"}"#
        );
        // India first for the default market; US indices the fixture lacks are dropped.
        assert_eq!(
            rows.iter().filter(|r| r.region == "Indian Markets").count(),
            4
        );
        assert_eq!(rows.last().unwrap().symbol, "^GSPC");
        assert_eq!(rows.last().unwrap().region, "US Markets");
    }

    #[test]
    fn history_json_shape_is_pinned() {
        let f = Fixture::default();
        let h = block(history(&f, "TCS.NS", "1mo", Market::In)).unwrap();
        assert_eq!(h.interval, "1d");
        let v = serde_json::to_value(&h).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["symbol", "currency", "period", "interval", "bars"]);
        let bar = serde_json::to_string(&h.bars[0]).unwrap();
        assert_eq!(
            bar,
            r#"{"time":"2023-11-14T22:13:20+00:00","open":100.0,"high":101.0,"low":99.0,"close":100.0,"volume":1000}"#
        );
        assert_eq!(h.bars.len(), 5);
    }

    #[test]
    fn history_of_unknown_symbol_fails() {
        let f = Fixture::default();
        assert!(block(history(&f, "NOPE.NS", "1mo", Market::In)).is_err());
        let empty = ChartData {
            timestamp: None,
            ..chart(&[], 0, 1, "INR")
        };
        let h = history_from_chart(empty, "X", "1mo", "1d");
        assert!(h.bars.is_empty());
    }

    #[test]
    fn history_range_matches_cli_periods() {
        assert_eq!(history_range("1w"), ("5d", "15m"));
        assert_eq!(history_range("1d"), ("1d", "5m"));
        assert_eq!(history_range("2y"), ("2y", "1wk"));
        assert_eq!(history_range("max"), ("max", "1mo"));
        assert!(check_period("ytd").is_ok());
        assert!(check_period("7x").is_err());
    }

    #[test]
    fn portfolio_view_excludes_unpriced_holdings_from_totals() {
        let v = block(portfolio(&Fixture::default())).unwrap();
        assert_eq!(v.holdings.len(), 3);
        // TCS 10 @ 3500 → 4100; INFY 5 @ 1600 → 1500.
        assert_eq!(v.totals.cost_basis, 35000.0 + 8000.0);
        assert_eq!(v.totals.value, 41000.0 + 7500.0);
        assert_eq!(v.totals.pnl, 5500.0);
        assert_eq!(v.totals.pnl_percent, 12.79);
        assert_eq!(v.unpriced, vec!["DELISTED.NS".to_string()]);
        let delisted = &v.holdings[2];
        assert_eq!(
            (delisted.price, delisted.value, delisted.pnl),
            (None, None, None)
        );
        assert_eq!(v.totals.currency.as_deref(), Some("INR"));
    }

    #[test]
    fn portfolio_json_matches_portfolio_json_flag() {
        // Same keys as `portfolio --json` (pinned in commands/output_tests.rs).
        let v = serde_json::to_value(block(portfolio(&Fixture::default())).unwrap()).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["holdings", "totals", "unpriced"]);
        let totals: Vec<&str> = v["totals"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            totals,
            ["cost_basis", "value", "pnl", "pnl_percent", "currency"]
        );
        assert_eq!(v["holdings"][0]["symbol"], "TCS.NS");
        assert_eq!(v["holdings"][0]["pnl"], 6000.0);
        assert!(v["holdings"][2]["price"].is_null());
    }

    #[test]
    fn totals_currency_comes_from_a_holding() {
        // A US portfolio viewed with Indian indices in the same quote batch.
        let mut p = Portfolio::default();
        p.add("AAPL", 2.0, 100.0);
        let quotes = vec![
            quote("^NSEI", Some(24800.0), 0.5, "INR"),
            quote("AAPL", Some(150.0), 1.0, "USD"),
        ];
        let v = portfolio_view(&p, &quotes);
        assert_eq!(v.totals.currency.as_deref(), Some("USD"));
        assert_eq!(v.totals.value, 300.0);
    }

    #[test]
    fn empty_portfolio_needs_no_network() {
        let f = Fixture {
            portfolio: Portfolio::default(),
            ..Default::default()
        };
        *f.fail.lock().unwrap() = Some("offline".into());
        let v = block(portfolio(&f)).unwrap();
        assert!(v.holdings.is_empty());
        assert_eq!(v.totals.value, 0.0);
    }

    #[test]
    fn watchlist_keeps_order_and_unknown_symbols() {
        let mut f = Fixture::default();
        f.watchlist.add("NOPE.NS");
        let rows = block(watchlist(&f)).unwrap();
        let symbols: Vec<&str> = rows.iter().map(|r| r.symbol.as_str()).collect();
        assert_eq!(symbols, ["RELIANCE.NS", "TCS.NS", "NOPE.NS"]);
        assert_eq!(rows[0].price, Some(1300.0));
        assert_eq!(rows[2].price, None);
    }

    #[test]
    fn alerts_never_fire_without_a_price() {
        let rows = block(alerts(&Fixture::default())).unwrap();
        let status: Vec<&str> = rows.iter().map(|r| r.status).collect();
        // TCS 4100 >= 4000; RELIANCE 1300 is not below 1000; no price for DELISTED.
        assert_eq!(status, ["triggered", "waiting", "no_price"]);
        assert_eq!(rows[2].price, None);
        assert_eq!(rows[0].condition, "above");
        assert_eq!(rows[1].index, 1);
    }

    #[test]
    fn local_file_errors_are_tagged() {
        struct Broken;
        impl DataSource for Broken {
            async fn quotes(&self, _: &[&str]) -> Result<Vec<Quote>> {
                Ok(Vec::new())
            }
            async fn chart(&self, _: &str, _: &str, _: &str) -> Result<ChartData> {
                bail!("unused")
            }
            async fn search(&self, _: &str) -> Result<Vec<SearchResult>> {
                Ok(Vec::new())
            }
            fn portfolio(&self) -> Result<Portfolio> {
                bail!("/home/me/portfolio.json is not valid JSON")
            }
        }
        let err = block(portfolio(&Broken)).unwrap_err();
        assert!(err.downcast_ref::<LocalData>().is_some());
        // Market data errors are not tagged.
        let f = Fixture::default();
        *f.fail.lock().unwrap() = Some("offline".into());
        let err = block(watchlist(&f)).unwrap_err();
        assert!(err.downcast_ref::<LocalData>().is_none());
    }

    #[test]
    fn snapshot_uses_one_quote_request() {
        let f = Fixture::default();
        let s = block(snapshot(&f, Market::In)).unwrap();
        assert_eq!(f.quote_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let names: Vec<&str> = s.indices.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["NIFTY 50", "SENSEX", "BANK NIFTY", "NIFTY IT"]);
        assert_eq!(s.watchlist.len(), 2);
        assert_eq!(s.portfolio.holdings.len(), 3);
        assert_eq!(s.portfolio.totals.currency.as_deref(), Some("INR"));
        assert_eq!(
            s.alerts.iter().filter(|a| a.status == "triggered").count(),
            1
        );
    }

    #[test]
    fn snapshot_keeps_unpriced_us_indices() {
        let s = block(snapshot(&Fixture::default(), Market::Us)).unwrap();
        assert_eq!(s.indices.len(), market::US_INDICES.len());
        assert_eq!(s.indices[0].price, Some(5800.0));
        assert_eq!(s.indices[1].price, None);
    }
}

//! What the core commands show, gathered into plain structs.
//!
//! The CLI printers (`commands/`), the TUI and the HTTP server all build on
//! these functions, so `stockwise quote --json` and `GET /api/quote` return
//! the same shape. Nothing here prints. Market data and local files come
//! through [`DataSource`], which tests replace with fixtures.

use crate::alerts::{AlertCondition, AlertStore};
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

/// One index in `stockwise markets --json`. Fields are in alphabetical order
/// because that is the key order the command has always printed.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IndexRow {
    pub change: Option<f64>,
    pub change_percent: Option<f64>,
    pub currency: Option<String>,
    pub name: String,
    pub price: Option<f64>,
    pub region: String,
    pub symbol: String,
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
                change: q.regular_market_change,
                change_percent: q.regular_market_change_percent,
                currency: q.currency.clone(),
                name: name.to_string(),
                price: q.regular_market_price,
                region: region.to_string(),
                symbol: q_sym.to_string(),
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

/// `stockwise history --json`. Field order is the key order the command has
/// always printed (alphabetical).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct History {
    pub bars: Vec<Bar>,
    pub currency: Option<String>,
    pub interval: String,
    pub period: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Bar {
    pub close: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub open: Option<f64>,
    /// RFC 3339 timestamp of the bar.
    pub time: Option<String>,
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
            close: at(&close, i),
            high: at(&high, i),
            low: at(&low, i),
            open: at(&open, i),
            time: chrono::DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()),
            volume: volume.get(i).copied().flatten(),
            ts,
        })
        .collect();
    History {
        bars,
        currency: chart.meta.and_then(|m| m.currency),
        interval: interval.to_string(),
        period: period.to_string(),
        symbol: symbol.to_string(),
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

// ── portfolio ──

/// One holding valued at the live price. Without a price, the value and P&L
/// are None and the holding is left out of the totals — never valued at 0.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HoldingRow {
    pub symbol: String,
    pub shares: f64,
    pub avg_cost: f64,
    pub price: Option<f64>,
    pub value: Option<f64>,
    pub pnl: Option<f64>,
    pub pnl_percent: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PortfolioView {
    pub holdings: Vec<HoldingRow>,
    /// Cost of the priced holdings.
    pub total_cost: f64,
    pub total_value: f64,
    pub total_pnl: f64,
    pub total_pnl_percent: f64,
    /// Currency of the totals (that of the first quote; mixed-currency
    /// portfolios are summed as plain numbers).
    pub currency: Option<String>,
    /// Holdings without a live price, excluded from the totals.
    pub unpriced: Vec<String>,
}

fn pct(part: f64, whole: f64) -> f64 {
    if whole > 0.0 {
        (part / whole) * 100.0
    } else {
        0.0
    }
}

pub fn portfolio_view(portfolio: &Portfolio, quotes: &[Quote]) -> PortfolioView {
    let mut view = PortfolioView {
        currency: quotes.first().and_then(|q| q.currency.clone()),
        ..Default::default()
    };
    for h in &portfolio.holdings {
        let quote = find_quote(quotes, &h.symbol);
        let price = quote.and_then(|q| q.regular_market_price);
        let mut row = HoldingRow {
            symbol: h.symbol.clone(),
            shares: h.shares,
            avg_cost: h.avg_cost,
            price,
            value: None,
            pnl: None,
            pnl_percent: None,
            currency: quote.and_then(|q| q.currency.clone()),
        };
        match price {
            Some(price) => {
                let value = h.shares * price;
                let cost = h.shares * h.avg_cost;
                row.value = Some(value);
                row.pnl = Some(value - cost);
                row.pnl_percent = Some(pct(value - cost, cost));
                view.total_cost += cost;
                view.total_value += value;
            }
            None => view.unpriced.push(h.symbol.clone()),
        }
        view.holdings.push(row);
    }
    view.total_pnl = view.total_value - view.total_cost;
    view.total_pnl_percent = pct(view.total_pnl, view.total_cost);
    view
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
    let p = src.portfolio()?;
    let symbols = unique_symbols(p.holdings.iter().map(|h| h.symbol.as_str()));
    let quotes = quotes_for(src, &symbols).await?;
    Ok(portfolio_view(&p, &quotes))
}

// ── watchlist ──

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WatchRow {
    pub symbol: String,
    pub name: Option<String>,
    pub price: Option<f64>,
    pub change: Option<f64>,
    pub change_percent: Option<f64>,
    pub volume: Option<u64>,
    pub currency: Option<String>,
}

/// One row per watched symbol, in watchlist order; unknown symbols keep
/// their row with no price.
pub fn watch_rows(symbols: &[String], quotes: &[Quote]) -> Vec<WatchRow> {
    symbols
        .iter()
        .map(|s| {
            let q = find_quote(quotes, s);
            WatchRow {
                symbol: s.clone(),
                name: q.and_then(|q| q.short_name.clone().or_else(|| q.long_name.clone())),
                price: q.and_then(|q| q.regular_market_price),
                change: q.and_then(|q| q.regular_market_change),
                change_percent: q.and_then(|q| q.regular_market_change_percent),
                volume: q.and_then(|q| q.regular_market_volume),
                currency: q.and_then(|q| q.currency.clone()),
            }
        })
        .collect()
}

pub async fn watchlist(src: &impl DataSource) -> Result<Vec<WatchRow>> {
    let wl = src.watchlist()?;
    let symbols = unique_symbols(wl.symbols.iter().map(String::as_str));
    let quotes = quotes_for(src, &symbols).await?;
    Ok(watch_rows(&wl.symbols, &quotes))
}

// ── alerts ──

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AlertRow {
    /// Number used by `stockwise alert remove`.
    pub index: usize,
    pub symbol: String,
    /// `above` or `below`.
    pub condition: String,
    pub target: f64,
    pub created_at: String,
    pub price: Option<f64>,
    /// False when there is no live price: an alert never fires on a missing quote.
    pub triggered: bool,
}

pub fn alert_rows(store: &AlertStore, quotes: &[Quote]) -> Vec<AlertRow> {
    store
        .alerts
        .iter()
        .enumerate()
        .map(|(index, a)| {
            let price = find_quote(quotes, &a.symbol).and_then(|q| q.regular_market_price);
            let triggered = price.is_some_and(|p| match a.condition {
                AlertCondition::Above => p >= a.target,
                AlertCondition::Below => p <= a.target,
            });
            AlertRow {
                index,
                symbol: a.symbol.clone(),
                condition: a.condition.to_string(),
                target: a.target,
                created_at: a.created_at.clone(),
                price,
                triggered,
            }
        })
        .collect()
}

pub async fn alerts(src: &impl DataSource) -> Result<Vec<AlertRow>> {
    let store = src.alerts()?;
    let symbols = unique_symbols(store.alerts.iter().map(|a| a.symbol.as_str()));
    let quotes = quotes_for(src, &symbols).await?;
    Ok(alert_rows(&store, &quotes))
}

// ── dashboard snapshot ──

/// Everything the dashboard shows, priced from a single quote request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub indices: Vec<IndexRow>,
    pub watchlist: Vec<WatchRow>,
    pub portfolio: PortfolioView,
    pub alerts: Vec<AlertRow>,
}

/// Home-market indices (in list order, unpriced ones kept), watchlist,
/// portfolio and alerts in one batched quote request.
pub async fn snapshot(src: &impl DataSource, market: Market) -> Result<Snapshot> {
    let (region, indices) = index_sections(market)[0];
    let p = src.portfolio()?;
    let wl = src.watchlist()?;
    let store = src.alerts()?;
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
                change: q.and_then(|q| q.regular_market_change),
                change_percent: q.and_then(|q| q.regular_market_change_percent),
                currency: q.and_then(|q| q.currency.clone()),
                name: name.to_string(),
                price: q.and_then(|q| q.regular_market_price),
                region: region.to_string(),
                symbol: sym.to_string(),
            }
        })
        .collect();
    Ok(Snapshot {
        indices,
        watchlist: watch_rows(&wl.symbols, &quotes),
        portfolio: portfolio_view(&p, &quotes),
        alerts: alert_rows(&store, &quotes),
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
        // Same keys, order and values as the json! object 0.3 printed.
        let rows = block(markets(&Fixture::default(), Market::In)).unwrap();
        let first = serde_json::to_string(&rows[0]).unwrap();
        assert_eq!(
            first,
            r#"{"change":124.0,"change_percent":0.5,"currency":"INR","name":"NIFTY 50","price":24800.0,"region":"Indian Markets","symbol":"^NSEI"}"#
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
        assert_eq!(keys, ["bars", "currency", "interval", "period", "symbol"]);
        let bar = serde_json::to_string(&h.bars[0]).unwrap();
        assert_eq!(
            bar,
            r#"{"close":100.0,"high":101.0,"low":99.0,"open":100.0,"time":"2023-11-14T22:13:20+00:00","volume":1000}"#
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
        assert_eq!(v.total_cost, 35000.0 + 8000.0);
        assert_eq!(v.total_value, 41000.0 + 7500.0);
        assert_eq!(v.total_pnl, 5500.0);
        assert!((v.total_pnl_percent - 5500.0 / 43000.0 * 100.0).abs() < 1e-9);
        assert_eq!(v.unpriced, vec!["DELISTED.NS".to_string()]);
        let delisted = &v.holdings[2];
        assert_eq!(
            (delisted.price, delisted.value, delisted.pnl),
            (None, None, None)
        );
        assert_eq!(v.currency.as_deref(), Some("INR"));
    }

    #[test]
    fn portfolio_json_shape() {
        let v = serde_json::to_value(block(portfolio(&Fixture::default())).unwrap()).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "currency",
                "holdings",
                "total_cost",
                "total_pnl",
                "total_pnl_percent",
                "total_value",
                "unpriced"
            ]
        );
        assert_eq!(v["holdings"][0]["symbol"], "TCS.NS");
        assert_eq!(v["holdings"][0]["pnl"], 6000.0);
        assert!(v["holdings"][2]["price"].is_null());
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
        assert_eq!(v.total_value, 0.0);
    }

    #[test]
    fn watch_rows_keep_watchlist_order_and_unknown_symbols() {
        let quotes = vec![quote("B.NS", Some(2.0), 1.0, "INR")];
        let rows = watch_rows(&["A.NS".into(), "B.NS".into()], &quotes);
        assert_eq!(rows[0].symbol, "A.NS");
        assert_eq!(rows[0].price, None);
        assert_eq!(rows[1].price, Some(2.0));
        assert_eq!(rows[1].name.as_deref(), Some("B Ltd"));
    }

    #[test]
    fn alerts_never_fire_without_a_price() {
        let rows = block(alerts(&Fixture::default())).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].triggered, "TCS 4100 >= 4000");
        assert!(!rows[1].triggered, "RELIANCE 1300 is not below 1000");
        assert!(!rows[2].triggered, "no price for DELISTED");
        assert_eq!(rows[2].price, None);
        assert_eq!(rows[0].condition, "above");
        assert_eq!(rows[1].index, 1);
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
        assert_eq!(s.alerts.iter().filter(|a| a.triggered).count(), 1);
    }

    #[test]
    fn snapshot_keeps_unpriced_us_indices() {
        let s = block(snapshot(&Fixture::default(), Market::Us)).unwrap();
        assert_eq!(s.indices.len(), market::US_INDICES.len());
        assert_eq!(s.indices[0].price, Some(5800.0));
        assert_eq!(s.indices[1].price, None);
    }
}

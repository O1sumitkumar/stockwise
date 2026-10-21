//! Option chains: the data model, parsers for Yahoo Finance (US) and NSE
//! (India) responses, and the chain-wide numbers traders read — put/call
//! ratio, max pain, open-interest walls — plus per-contract IV and greeks
//! from `greeks.rs`.
//!
//! NSE's option-chain API serves browser sessions: `NseClient` loads the
//! public option-chain page once for cookies, then makes at most two API
//! calls, spaced out, and gives up with a clear error instead of retrying.

use crate::api::{YahooClient, build_url};
use crate::greeks::{self, BsInput, Greeks, OptionKind};
use crate::market::{self, Market};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

/// NSE index option underlyings (everything else is a stock).
pub const NSE_INDICES: &[&str] = &["NIFTY", "BANKNIFTY", "FINNIFTY", "MIDCPNIFTY", "NIFTYNXT50"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChainSource {
    Nse,
    Yahoo,
}

/// One call or put at one strike. Missing fields stay `None`; NSE reports
/// "no trade" as 0, which the parser turns into `None` for prices and IV.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Contract {
    pub last: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub volume: Option<u64>,
    pub open_interest: Option<u64>,
    pub oi_change: Option<i64>,
    /// Implied volatility as reported by the source, as a decimal.
    pub source_iv: Option<f64>,
}

impl Contract {
    /// Mid of a two-sided market, else the last traded price.
    pub fn market_price(&self) -> Option<f64> {
        match (self.bid, self.ask) {
            (Some(b), Some(a)) if b > 0.0 && a >= b => Some((a + b) / 2.0),
            _ => self.last.filter(|p| *p > 0.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChainRow {
    pub strike: f64,
    pub call: Option<Contract>,
    pub put: Option<Contract>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OptionChain {
    pub symbol: String,
    pub source: ChainSource,
    pub underlying: Option<f64>,
    pub currency: Option<String>,
    pub expiry: NaiveDate,
    pub expiries: Vec<NaiveDate>,
    /// When the source priced the chain (NSE's timestamp, Yahoo's quote time).
    pub as_of: Option<DateTime<Utc>>,
    pub rows: Vec<ChainRow>,
}

impl OptionChain {
    /// Years from the data timestamp (or now, if unknown) to expiry: the
    /// prices are as of that moment, so IV and greeks are too.
    pub fn years_to_expiry(&self, now: DateTime<Utc>) -> f64 {
        let at = self.as_of.filter(|t| *t < now).unwrap_or(now);
        years_to_expiry(self.expiry, self.source, at)
    }
}

/// NSE's `19-Oct-2026 15:40:00` timestamps (IST) in UTC.
pub fn parse_nse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    let ist = chrono::FixedOffset::east_opt(5 * 3600 + 1800)?;
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%d-%b-%Y %H:%M:%S")
        .ok()?
        .and_local_timezone(ist)
        .single()
        .map(|t| t.with_timezone(&Utc))
}

/// Where an option chain comes from for a user-typed symbol.
#[derive(Debug, PartialEq)]
pub enum Route {
    /// NSE symbol (`NIFTY`, `RELIANCE`) and whether it is an index.
    Nse { symbol: String, index: bool },
    /// Yahoo Finance symbol (`AAPL`, `SPY`).
    Yahoo(String),
}

/// Indian market → NSE (`RELIANCE`, `RELIANCE.NS`, `^NSEI` → `NIFTY`);
/// US market or a non-NSE suffix → Yahoo.
pub fn route(symbol: &str, market: Market) -> Route {
    let upper = symbol.trim().to_uppercase();
    if market == Market::Us {
        return Route::Yahoo(market::resolve_symbol(&upper, market));
    }
    let nse = match upper.as_str() {
        "^NSEI" | "NIFTY50" | "NIFTY 50" => "NIFTY".to_string(),
        "^NSEBANK" | "BANKNIFTY" | "NIFTYBANK" | "NIFTY BANK" => "BANKNIFTY".to_string(),
        s if s.ends_with(".NS") => s.trim_end_matches(".NS").to_string(),
        s if !s.contains(['.', '^', '=']) => s.to_string(),
        _ => return Route::Yahoo(market::resolve_symbol(&upper, market)),
    };
    let index = NSE_INDICES.contains(&nse.as_str());
    Route::Nse { symbol: nse, index }
}

/// Parse `2026-10-27` or NSE's `27-Oct-2026`.
pub fn parse_expiry(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%d-%b-%Y"))
        .ok()
}

/// When an expiry stops trading, in UTC: 15:30 IST on NSE, 16:00 New York
/// (taken as 20:00 UTC) for US listings.
pub fn expiry_close(expiry: NaiveDate, source: ChainSource) -> DateTime<Utc> {
    let hour = match source {
        ChainSource::Nse => 10,
        ChainSource::Yahoo => 20,
    };
    expiry
        .and_hms_opt(hour, 0, 0)
        .expect("valid time")
        .and_utc()
}

/// Years from `now` to the expiry close (negative once expired).
pub fn years_to_expiry(expiry: NaiveDate, source: ChainSource, now: DateTime<Utc>) -> f64 {
    (expiry_close(expiry, source) - now).num_seconds() as f64 / (365.0 * 86_400.0)
}

/// The requested expiry if listed, else the nearest one still trading.
pub fn pick_expiry(
    expiries: &[NaiveDate],
    requested: Option<NaiveDate>,
    source: ChainSource,
    now: DateTime<Utc>,
) -> Result<NaiveDate> {
    if expiries.is_empty() {
        bail!("no expiries listed");
    }
    if let Some(want) = requested {
        if expiries.contains(&want) {
            return Ok(want);
        }
        let listed: Vec<String> = expiries.iter().take(8).map(|d| d.to_string()).collect();
        bail!(
            "No expiry on {}. Listed expiries: {}{}",
            want,
            listed.join(", "),
            if expiries.len() > 8 { ", ..." } else { "" }
        );
    }
    expiries
        .iter()
        .copied()
        .filter(|d| expiry_close(*d, source) > now)
        .min()
        .context("every listed expiry has passed")
}

// ── Parsers ──

fn f(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64).filter(|x| x.is_finite())
}

/// Positive number or `None` (NSE writes 0 for "no price").
fn pos(v: &Value, key: &str) -> Option<f64> {
    f(v, key).filter(|x| *x > 0.0)
}

fn count(v: &Value, key: &str) -> Option<u64> {
    v.get(key)
        .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f.max(0.0) as u64)))
}

fn yahoo_contract(c: &Value) -> Contract {
    Contract {
        last: pos(c, "lastPrice"),
        bid: pos(c, "bid"),
        ask: pos(c, "ask"),
        volume: count(c, "volume"),
        open_interest: count(c, "openInterest"),
        oi_change: None,
        // Yahoo reports placeholder IVs like 1e-5 for dead strikes.
        source_iv: f(c, "impliedVolatility").filter(|v| *v > 0.001),
    }
}

/// Merge call and put lists into rows sorted by strike.
fn merge_rows(calls: Vec<(f64, Contract)>, puts: Vec<(f64, Contract)>) -> Vec<ChainRow> {
    let mut rows: Vec<ChainRow> = Vec::new();
    let find = |rows: &mut Vec<ChainRow>, strike: f64| -> usize {
        match rows.iter().position(|r| (r.strike - strike).abs() < 1e-9) {
            Some(i) => i,
            None => {
                rows.push(ChainRow {
                    strike,
                    call: None,
                    put: None,
                });
                rows.len() - 1
            }
        }
    };
    for (strike, c) in calls {
        let i = find(&mut rows, strike);
        rows[i].call = Some(c);
    }
    for (strike, p) in puts {
        let i = find(&mut rows, strike);
        rows[i].put = Some(p);
    }
    rows.sort_by(|a, b| a.strike.total_cmp(&b.strike));
    rows
}

fn ts_to_date(ts: i64) -> Option<NaiveDate> {
    DateTime::from_timestamp(ts, 0).map(|d| d.date_naive())
}

/// Parse a Yahoo `v7/finance/options` response.
pub fn parse_yahoo_chain(v: &Value) -> Result<OptionChain> {
    let result = v
        .pointer("/optionChain/result/0")
        .context("Yahoo returned no option data")?;
    let symbol = result
        .get("underlyingSymbol")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let expiries: Vec<NaiveDate> = result
        .get("expirationDates")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_i64)
                .filter_map(ts_to_date)
                .collect()
        })
        .unwrap_or_default();
    let quote = result.get("quote");
    let underlying = quote.and_then(|q| pos(q, "regularMarketPrice"));
    let currency = quote
        .and_then(|q| q.get("currency"))
        .and_then(Value::as_str)
        .map(String::from);

    let opts = result
        .pointer("/options/0")
        .filter(|_| !expiries.is_empty())
        .with_context(|| format!("Yahoo Finance lists no options for {symbol}"))?;
    let expiry = opts
        .get("expirationDate")
        .and_then(Value::as_i64)
        .and_then(ts_to_date)
        .context("option data has no expiration date")?;
    let side = |key: &str| -> Vec<(f64, Contract)> {
        opts.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| Some((f(c, "strike")?, yahoo_contract(c))))
                    .collect()
            })
            .unwrap_or_default()
    };
    let as_of = quote
        .and_then(|q| q.get("regularMarketTime"))
        .and_then(Value::as_i64)
        .and_then(|t| DateTime::from_timestamp(t, 0));

    Ok(OptionChain {
        symbol,
        source: ChainSource::Yahoo,
        underlying,
        currency,
        expiry,
        expiries,
        as_of,
        rows: merge_rows(side("calls"), side("puts")),
    })
}

fn nse_contract(c: &Value) -> Contract {
    Contract {
        last: pos(c, "lastPrice"),
        bid: pos(c, "buyPrice1").or_else(|| pos(c, "bidprice")),
        ask: pos(c, "sellPrice1").or_else(|| pos(c, "askPrice")),
        volume: count(c, "totalTradedVolume"),
        open_interest: count(c, "openInterest"),
        oi_change: f(c, "changeinOpenInterest").map(|x| x as i64),
        source_iv: pos(c, "impliedVolatility").map(|iv| iv / 100.0),
    }
}

/// Expiry dates from NSE's `option-chain-contract-info` response.
pub fn parse_nse_expiries(v: &Value) -> Vec<NaiveDate> {
    let mut out: Vec<NaiveDate> = v
        .get("expiryDates")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter_map(parse_expiry)
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// Parse NSE's `option-chain-v3` response, keeping rows for `expiry`.
pub fn parse_nse_chain(v: &Value, symbol: &str, expiry: NaiveDate) -> Result<OptionChain> {
    let records = v.get("records").context("NSE returned no option records")?;
    let data = records
        .get("data")
        .and_then(Value::as_array)
        .context("NSE returned no option rows")?;
    let mut calls = Vec::new();
    let mut puts = Vec::new();
    for row in data {
        let row_expiry = row
            .get("expiryDates")
            .or_else(|| row.get("expiryDate"))
            .and_then(Value::as_str)
            .and_then(parse_expiry);
        if row_expiry.is_some_and(|d| d != expiry) {
            continue;
        }
        let Some(strike) = f(row, "strikePrice") else {
            continue;
        };
        if let Some(ce) = row.get("CE").filter(|c| c.is_object()) {
            calls.push((strike, nse_contract(ce)));
        }
        if let Some(pe) = row.get("PE").filter(|c| c.is_object()) {
            puts.push((strike, nse_contract(pe)));
        }
    }
    let rows = merge_rows(calls, puts);
    if rows.is_empty() {
        bail!("NSE returned no {symbol} options for {expiry}");
    }
    let underlying = pos(records, "underlyingValue").or_else(|| {
        data.iter().find_map(|r| {
            r.get("CE")
                .or(r.get("PE"))
                .and_then(|c| pos(c, "underlyingValue"))
        })
    });
    let mut expiries = parse_nse_expiries(records);
    if expiries.is_empty() {
        expiries.push(expiry);
    }
    Ok(OptionChain {
        symbol: symbol.to_string(),
        source: ChainSource::Nse,
        underlying,
        currency: Some("INR".into()),
        expiry,
        expiries,
        as_of: records
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_nse_timestamp),
        rows,
    })
}

// ── Chain analytics ──

/// Rows within `n` strikes of the at-the-money strike (all rows for n = 0).
pub fn near_the_money(rows: &[ChainRow], spot: Option<f64>, n: usize) -> &[ChainRow] {
    let (Some(spot), true) = (spot, n > 0) else {
        return rows;
    };
    let Some(atm) = atm_index(rows, spot) else {
        return rows;
    };
    let lo = atm.saturating_sub(n);
    let hi = (atm + n + 1).min(rows.len());
    &rows[lo..hi]
}

/// Index of the strike closest to `spot`.
pub fn atm_index(rows: &[ChainRow], spot: f64) -> Option<usize> {
    rows.iter()
        .enumerate()
        .min_by(|a, b| {
            (a.1.strike - spot)
                .abs()
                .total_cmp(&(b.1.strike - spot).abs())
        })
        .map(|(i, _)| i)
}

/// Open interest and volume totals, and the put/call ratios.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Pcr {
    pub call_oi: u64,
    pub put_oi: u64,
    pub call_volume: u64,
    pub put_volume: u64,
    /// Put OI / call OI; `None` when there is no call OI.
    pub oi_ratio: Option<f64>,
    /// Put volume / call volume; `None` when no calls traded.
    pub volume_ratio: Option<f64>,
}

pub fn pcr(rows: &[ChainRow]) -> Pcr {
    let sum = |pick: fn(&ChainRow) -> Option<&Contract>, field: fn(&Contract) -> Option<u64>| {
        rows.iter().filter_map(pick).filter_map(field).sum::<u64>()
    };
    fn call(r: &ChainRow) -> Option<&Contract> {
        r.call.as_ref()
    }
    fn put(r: &ChainRow) -> Option<&Contract> {
        r.put.as_ref()
    }
    let (call_oi, put_oi) = (
        sum(call, |c| c.open_interest),
        sum(put, |c| c.open_interest),
    );
    let (call_volume, put_volume) = (sum(call, |c| c.volume), sum(put, |c| c.volume));
    let ratio = |a: u64, b: u64| (b > 0).then(|| a as f64 / b as f64);
    Pcr {
        call_oi,
        put_oi,
        call_volume,
        put_volume,
        oi_ratio: ratio(put_oi, call_oi),
        volume_ratio: ratio(put_volume, call_volume),
    }
}

/// The `n` strikes with the most open interest on one side, largest first.
pub fn top_oi(rows: &[ChainRow], kind: OptionKind, n: usize) -> Vec<(f64, u64)> {
    let mut v: Vec<(f64, u64)> = rows
        .iter()
        .filter_map(|r| {
            let c = match kind {
                OptionKind::Call => r.call.as_ref(),
                OptionKind::Put => r.put.as_ref(),
            }?;
            Some((r.strike, c.open_interest.filter(|oi| *oi > 0)?))
        })
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.total_cmp(&b.0)));
    v.truncate(n);
    v
}

/// What option buyers would collect if the underlying settled at `strike`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PainPoint {
    pub strike: f64,
    pub call_pain: f64,
    pub put_pain: f64,
    pub total: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MaxPain {
    /// Settlement strike at which option holders collect the least.
    pub strike: f64,
    /// Payout at every listed strike, in underlying points × open interest.
    pub curve: Vec<PainPoint>,
}

/// Max pain: the listed strike minimising the total intrinsic value paid to
/// holders of open calls and puts at expiry. `None` without any open interest.
pub fn max_pain(rows: &[ChainRow]) -> Option<MaxPain> {
    let oi = |c: &Option<Contract>| c.as_ref().and_then(|c| c.open_interest).unwrap_or(0) as f64;
    if rows.iter().all(|r| oi(&r.call) == 0.0 && oi(&r.put) == 0.0) {
        return None;
    }
    let curve: Vec<PainPoint> = rows
        .iter()
        .map(|settle| {
            let s = settle.strike;
            let call_pain: f64 = rows
                .iter()
                .map(|r| oi(&r.call) * (s - r.strike).max(0.0))
                .sum();
            let put_pain: f64 = rows
                .iter()
                .map(|r| oi(&r.put) * (r.strike - s).max(0.0))
                .sum();
            PainPoint {
                strike: s,
                call_pain,
                put_pain,
                total: call_pain + put_pain,
            }
        })
        .collect();
    let strike = curve
        .iter()
        .min_by(|a, b| {
            a.total
                .total_cmp(&b.total)
                .then(a.strike.total_cmp(&b.strike))
        })?
        .strike;
    Some(MaxPain { strike, curve })
}

/// One side of a chain row with the IV and greeks used for display.
#[derive(Clone, Debug, Serialize)]
pub struct ContractView {
    #[serde(flatten)]
    pub contract: Contract,
    /// IV used for the greeks: solved from the market price when possible,
    /// otherwise the source's figure.
    pub iv: Option<f64>,
    pub iv_from: Option<&'static str>,
    pub greeks: Option<Greeks>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RowView {
    pub strike: f64,
    pub call: Option<ContractView>,
    pub put: Option<ContractView>,
}

/// Market inputs shared by every contract in a chain.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Assumptions {
    pub spot: f64,
    pub years: f64,
    pub rate: f64,
    pub div_yield: f64,
}

pub fn contract_view(c: &Contract, kind: OptionKind, strike: f64, a: &Assumptions) -> ContractView {
    let base = BsInput {
        spot: a.spot,
        strike,
        t: a.years,
        rate: a.rate,
        div_yield: a.div_yield,
        vol: 0.0,
    };
    let solved = c
        .market_price()
        .and_then(|p| greeks::implied_vol(kind, p, &base));
    let (iv, iv_from) = match (solved, c.source_iv) {
        (Some(v), _) => (Some(v), Some("solved")),
        (None, Some(v)) => (Some(v), Some("source")),
        (None, None) => (None, None),
    };
    let greeks = iv.and_then(|vol| greeks::greeks(kind, &BsInput { vol, ..base }));
    ContractView {
        contract: c.clone(),
        iv,
        iv_from,
        greeks,
    }
}

pub fn row_views(rows: &[ChainRow], a: &Assumptions) -> Vec<RowView> {
    rows.iter()
        .map(|r| RowView {
            strike: r.strike,
            call: r
                .call
                .as_ref()
                .map(|c| contract_view(c, OptionKind::Call, r.strike, a)),
            put: r
                .put
                .as_ref()
                .map(|c| contract_view(c, OptionKind::Put, r.strike, a)),
        })
        .collect()
}

// ── Fetching ──

const NSE_BASE: &str = "https://www.nseindia.com";
const NSE_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                      (KHTML, like Gecko) Chrome/129.0 Safari/537.36";

fn nse_refused(status: reqwest::StatusCode) -> anyhow::Error {
    anyhow::anyhow!(
        "NSE refused the option-chain request (HTTP {status}). NSE's API serves browser \
         sessions and sometimes blocks scripts — wait a minute and retry, or open \
         nseindia.com/option-chain. US options (`-m us`) come from Yahoo Finance instead."
    )
}

/// Minimal NSE session: one page load for cookies, then JSON API calls
/// spaced 500 ms apart. No retry loops.
pub struct NseClient {
    client: reqwest::Client,
}

impl NseClient {
    pub async fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(NSE_UA)
            .cookie_store(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .context("Failed to build HTTP client")?;
        let resp = client
            .get(format!("{NSE_BASE}/option-chain"))
            .header(reqwest::header::ACCEPT, "text/html")
            .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
            .send()
            .await
            .context("Cannot reach nseindia.com. Check your internet connection")?;
        if !resp.status().is_success() {
            return Err(nse_refused(resp.status()));
        }
        Ok(Self { client })
    }

    async fn get_json(&self, url: &str) -> Result<Value> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::REFERER, format!("{NSE_BASE}/option-chain"))
            .send()
            .await
            .context("Request to nseindia.com failed")?;
        if !resp.status().is_success() {
            return Err(nse_refused(resp.status()));
        }
        let body = resp.text().await.context("Failed to read NSE response")?;
        serde_json::from_str(&body).map_err(|_| nse_refused(reqwest::StatusCode::OK))
    }

    pub async fn expiries(&self, symbol: &str) -> Result<Vec<NaiveDate>> {
        let url = build_url(
            &format!("{NSE_BASE}/api/option-chain-contract-info"),
            None,
            &[("symbol", symbol)],
        );
        Ok(parse_nse_expiries(&self.get_json(&url).await?))
    }

    pub async fn chain(&self, symbol: &str, index: bool, expiry: NaiveDate) -> Result<Value> {
        let expiry = expiry.format("%d-%b-%Y").to_string();
        let url = build_url(
            &format!("{NSE_BASE}/api/option-chain-v3"),
            None,
            &[
                ("type", if index { "Indices" } else { "Equity" }),
                ("symbol", symbol),
                ("expiry", &expiry),
            ],
        );
        self.get_json(&url).await
    }
}

/// Option chain for `symbol` at `expiry` (default: nearest live expiry).
pub async fn fetch_chain(
    symbol: &str,
    market: Market,
    expiry: Option<NaiveDate>,
) -> Result<OptionChain> {
    let now = Utc::now();
    match route(symbol, market) {
        Route::Nse { symbol, index } => {
            let nse = NseClient::new().await?;
            let expiries = nse.expiries(&symbol).await?;
            if expiries.is_empty() {
                bail!(
                    "NSE lists no options for {symbol}. Only F&O stocks and the indices {} \
                     have options on NSE; for US options add `-m us`",
                    NSE_INDICES.join(", ")
                );
            }
            let expiry = pick_expiry(&expiries, expiry, ChainSource::Nse, now)?;
            let raw = nse.chain(&symbol, index, expiry).await?;
            parse_nse_chain(&raw, &symbol, expiry)
        }
        Route::Yahoo(sym) => {
            let client = YahooClient::new().await?;
            let base = "https://query2.finance.yahoo.com/v7/finance/options";
            let first_url = build_url(base, Some(&sym), &[("crumb", client.crumb())]);
            let first = client.raw_get(&first_url).await?;
            let chain = parse_yahoo_chain(&first).with_context(|| {
                format!(
                    "No option chain for {sym}{}",
                    market::not_found_hint(market)
                )
            })?;
            let want = pick_expiry(&chain.expiries, expiry, ChainSource::Yahoo, now)?;
            if want == chain.expiry {
                return Ok(chain);
            }
            let ts = want
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc()
                .timestamp()
                .to_string();
            let url = build_url(
                base,
                Some(&sym),
                &[("date", &ts), ("crumb", client.crumb())],
            );
            parse_yahoo_chain(&client.raw_get(&url).await?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(strike: f64, call_oi: u64, put_oi: u64) -> ChainRow {
        let c = |oi| Contract {
            open_interest: Some(oi),
            ..Default::default()
        };
        ChainRow {
            strike,
            call: Some(c(call_oi)),
            put: Some(c(put_oi)),
        }
    }

    #[test]
    fn max_pain_on_fixture_chain() {
        // Settle at 100: calls 90 pay 10*100=1000; puts 110 pay 10*300=3000 → 4000
        // Settle at 110: calls 90 → 20*100, 100 → 10*500 = 7000; puts 0 → 7000
        // Settle at  90: calls 0; puts 100 → 10*200, 110 → 20*300 = 8000
        let rows = vec![
            row(90.0, 100, 50),
            row(100.0, 500, 200),
            row(110.0, 80, 300),
        ];
        let mp = max_pain(&rows).unwrap();
        assert_eq!(mp.strike, 100.0);
        let totals: Vec<f64> = mp.curve.iter().map(|p| p.total).collect();
        assert_eq!(totals, vec![8000.0, 4000.0, 7000.0]);
        assert_eq!(mp.curve[1].call_pain, 1000.0);
        assert_eq!(mp.curve[1].put_pain, 3000.0);
    }

    #[test]
    fn max_pain_needs_open_interest() {
        assert!(max_pain(&[row(100.0, 0, 0)]).is_none());
        assert!(max_pain(&[]).is_none());
    }

    #[test]
    fn pcr_and_oi_walls() {
        let mut rows = vec![
            row(90.0, 100, 50),
            row(100.0, 500, 200),
            row(110.0, 80, 300),
        ];
        rows[0].call.as_mut().unwrap().volume = Some(40);
        rows[1].put.as_mut().unwrap().volume = Some(10);
        let p = pcr(&rows);
        assert_eq!((p.call_oi, p.put_oi), (680, 550));
        assert!((p.oi_ratio.unwrap() - 550.0 / 680.0).abs() < 1e-12);
        assert_eq!(p.volume_ratio, Some(0.25));
        assert_eq!(
            top_oi(&rows, OptionKind::Put, 2),
            vec![(110.0, 300), (100.0, 200)]
        );
        assert_eq!(top_oi(&rows, OptionKind::Call, 1), vec![(100.0, 500)]);
        assert_eq!(pcr(&[]).oi_ratio, None);
    }

    #[test]
    fn routes_symbols_to_the_right_exchange() {
        let nse = |s: &str, index| Route::Nse {
            symbol: s.into(),
            index,
        };
        assert_eq!(route("nifty", Market::In), nse("NIFTY", true));
        assert_eq!(route("^NSEI", Market::In), nse("NIFTY", true));
        assert_eq!(route("^NSEBANK", Market::In), nse("BANKNIFTY", true));
        assert_eq!(route("RELIANCE.NS", Market::In), nse("RELIANCE", false));
        assert_eq!(route("M&M", Market::In), nse("M&M", false));
        assert_eq!(route("AAPL", Market::Us), Route::Yahoo("AAPL".into()));
        assert_eq!(route("VOD.L", Market::In), Route::Yahoo("VOD.L".into()));
    }

    #[test]
    fn expiry_parsing_and_picking() {
        let d = |s| parse_expiry(s).unwrap();
        assert_eq!(d("27-Oct-2026"), d("2026-10-27"));
        assert!(parse_expiry("27/10/2026").is_none());

        let list = vec![d("2026-10-20"), d("2026-10-27"), d("2026-11-03")];
        // 2026-10-20 11:00 UTC is after the 15:30 IST close that day.
        let now = DateTime::parse_from_rfc3339("2026-10-20T11:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            pick_expiry(&list, None, ChainSource::Nse, now).unwrap(),
            d("2026-10-27")
        );
        // ...but before the 16:00 New York close.
        assert_eq!(
            pick_expiry(&list, None, ChainSource::Yahoo, now).unwrap(),
            d("2026-10-20")
        );
        assert_eq!(
            pick_expiry(&list, Some(d("2026-11-03")), ChainSource::Nse, now).unwrap(),
            d("2026-11-03")
        );
        let err = pick_expiry(&list, Some(d("2026-12-31")), ChainSource::Nse, now)
            .unwrap_err()
            .to_string();
        assert!(err.contains("2026-10-27"), "{err}");
        let t = years_to_expiry(d("2026-10-27"), ChainSource::Nse, now);
        assert!((t * 365.0 - 6.958_333).abs() < 1e-3, "{t}");
    }

    #[test]
    fn parses_nse_v3_fixture() {
        let v = json!({
            "records": {
                "timestamp": "19-Oct-2026 15:40:00",
                "underlyingValue": 22888.95,
                "expiryDates": ["19-Oct-2026", "27-Oct-2026"],
                "data": [
                    {"strikePrice": 22900, "expiryDates": "27-Oct-2026",
                     "CE": {"lastPrice": 210.5, "buyPrice1": 209.0, "sellPrice1": 211.0,
                            "totalTradedVolume": 1500, "openInterest": 12000,
                            "changeinOpenInterest": -300, "impliedVolatility": 14.2},
                     "PE": {"lastPrice": 0, "buyPrice1": 0, "sellPrice1": 0,
                            "totalTradedVolume": 0, "openInterest": 0,
                            "changeinOpenInterest": 0, "impliedVolatility": 0}},
                    {"strikePrice": 22800, "expiryDates": "27-Oct-2026",
                     "PE": {"lastPrice": 150.0, "totalTradedVolume": 900,
                            "openInterest": 8000, "impliedVolatility": 15.0}},
                    {"strikePrice": 22800, "expiryDates": "03-Nov-2026",
                     "CE": {"lastPrice": 999.0, "openInterest": 1}}
                ]
            }
        });
        let chain = parse_nse_chain(&v, "NIFTY", parse_expiry("2026-10-27").unwrap()).unwrap();
        assert_eq!(chain.underlying, Some(22888.95));
        assert_eq!(chain.rows.len(), 2);
        assert_eq!(chain.rows[0].strike, 22800.0);
        assert!(chain.rows[0].call.is_none(), "other expiry must be dropped");
        let ce = chain.rows[1].call.as_ref().unwrap();
        assert_eq!(ce.market_price(), Some(210.0));
        assert_eq!(ce.oi_change, Some(-300));
        assert!((ce.source_iv.unwrap() - 0.142).abs() < 1e-12);
        let pe = chain.rows[1].put.as_ref().unwrap();
        assert_eq!(
            (pe.last, pe.source_iv, pe.open_interest),
            (None, None, Some(0))
        );
        assert_eq!(chain.expiries.len(), 2);
        let as_of = chain.as_of.unwrap();
        assert_eq!(as_of.to_rfc3339(), "2026-10-19T10:10:00+00:00");
        // Valued at the data time, not at a later "now".
        let later = DateTime::parse_from_rfc3339("2026-10-21T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let days = chain.years_to_expiry(later) * 365.0;
        assert!((days - (8.0 - 10.0 / 1440.0)).abs() < 1e-6, "{days}");
    }

    #[test]
    fn parses_yahoo_fixture() {
        let v = json!({"optionChain": {"result": [{
            "underlyingSymbol": "AAPL",
            "expirationDates": [1792540800, 1793145600],
            "quote": {"regularMarketPrice": 340.0, "currency": "USD"},
            "options": [{
                "expirationDate": 1792540800,
                "calls": [{"strike": 340.0, "lastPrice": 4.1, "bid": 4.0, "ask": 4.2,
                           "volume": 1200, "openInterest": 5000, "impliedVolatility": 0.21}],
                "puts": [{"strike": 335.0, "lastPrice": 2.0, "impliedVolatility": 0.00001},
                         {"strike": 340.0, "lastPrice": 3.9, "openInterest": 4000}]
            }]
        }]}});
        let chain = parse_yahoo_chain(&v).unwrap();
        assert_eq!(chain.symbol, "AAPL");
        assert_eq!(chain.expiry, parse_expiry("2026-10-21").unwrap());
        assert_eq!(chain.expiries.len(), 2);
        assert_eq!(chain.rows.len(), 2);
        assert!(chain.rows[0].call.is_none());
        assert_eq!(chain.rows[0].put.as_ref().unwrap().source_iv, None);
        assert_eq!(chain.rows[1].call.as_ref().unwrap().volume, Some(1200));

        let empty = json!({"optionChain": {"result": [{
            "underlyingSymbol": "RELIANCE.NS", "expirationDates": [], "options": []
        }]}});
        let err = parse_yahoo_chain(&empty).unwrap_err().to_string();
        assert!(err.contains("no options"), "{err}");
    }

    #[test]
    fn views_solve_iv_and_greeks() {
        let a = Assumptions {
            spot: 100.0,
            years: 30.0 / 365.0,
            rate: 0.05,
            div_yield: 0.0,
        };
        let inp = BsInput {
            spot: 100.0,
            strike: 105.0,
            t: a.years,
            rate: a.rate,
            div_yield: 0.0,
            vol: 0.25,
        };
        let fair = greeks::price(OptionKind::Call, &inp).unwrap();
        let c = Contract {
            bid: Some(fair - 0.01),
            ask: Some(fair + 0.01),
            source_iv: Some(0.9),
            ..Default::default()
        };
        let v = contract_view(&c, OptionKind::Call, 105.0, &a);
        assert!((v.iv.unwrap() - 0.25).abs() < 1e-6);
        assert_eq!(v.iv_from, Some("solved"));
        assert!(v.greeks.unwrap().delta > 0.0);

        // No price → falls back to the source IV; nothing → no greeks.
        let src = Contract {
            source_iv: Some(0.3),
            ..Default::default()
        };
        assert_eq!(
            contract_view(&src, OptionKind::Put, 95.0, &a).iv_from,
            Some("source")
        );
        assert!(
            contract_view(&Contract::default(), OptionKind::Put, 95.0, &a)
                .greeks
                .is_none()
        );
    }

    #[test]
    fn near_the_money_window() {
        let rows: Vec<ChainRow> = (0..10)
            .map(|i| row(100.0 + 10.0 * i as f64, 1, 1))
            .collect();
        let w = near_the_money(&rows, Some(141.0), 2);
        let strikes: Vec<f64> = w.iter().map(|r| r.strike).collect();
        assert_eq!(strikes, vec![120.0, 130.0, 140.0, 150.0, 160.0]);
        assert_eq!(near_the_money(&rows, Some(141.0), 0).len(), 10);
        assert_eq!(near_the_money(&rows, None, 2).len(), 10);
        assert_eq!(near_the_money(&rows, Some(50.0), 1).len(), 2);
    }
}

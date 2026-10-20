//! Broker integrations behind one [`Broker`] trait.
//!
//! Angel One is the original integration. Zerodha Kite, Upstox, Dhan and
//! Fyers are **experimental**: built from each broker's public API docs and
//! tested against handwritten fixtures and a local mock server, never against
//! a live account.
//!
//! Every order — any broker, any order type, GTT included — goes through
//! `commands::trade::place_live_order` (preview, `max_order_value`, typed
//! `yes` / `--yes`, `--dry-run`). Nothing else calls [`Broker::place_order`]
//! or [`Broker::gtt_create`].

pub mod angel;
mod app;
pub mod dhan;
pub mod fyers;
mod http;
#[cfg(test)]
pub mod mock;
pub mod secrets;
pub mod upstox;
pub mod zerodha;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// The brokers `trade --broker` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BrokerKind {
    /// Angel One SmartAPI
    Angel,
    /// Zerodha Kite Connect (experimental)
    #[value(alias = "kite")]
    Zerodha,
    /// Upstox API v2 (experimental)
    Upstox,
    /// Dhan API v2 (experimental)
    Dhan,
    /// Fyers API v3 (experimental)
    Fyers,
}

impl BrokerKind {
    pub const ALL: [BrokerKind; 5] = [
        BrokerKind::Angel,
        BrokerKind::Zerodha,
        BrokerKind::Upstox,
        BrokerKind::Dhan,
        BrokerKind::Fyers,
    ];

    /// The `--broker` spelling.
    pub fn id(self) -> &'static str {
        match self {
            BrokerKind::Angel => "angel",
            BrokerKind::Zerodha => "zerodha",
            BrokerKind::Upstox => "upstox",
            BrokerKind::Dhan => "dhan",
            BrokerKind::Fyers => "fyers",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BrokerKind::Angel => "Angel One",
            BrokerKind::Zerodha => "Zerodha Kite",
            BrokerKind::Upstox => "Upstox",
            BrokerKind::Dhan => "Dhan",
            BrokerKind::Fyers => "Fyers",
        }
    }

    /// Built from public docs only — never run against a live account.
    pub fn experimental(self) -> bool {
        self != BrokerKind::Angel
    }

    /// Credentials file in the data directory.
    pub fn config_file(self) -> &'static str {
        match self {
            BrokerKind::Angel => angel::CONFIG_FILE,
            BrokerKind::Zerodha => zerodha::CONFIG_FILE,
            BrokerKind::Upstox => upstox::CONFIG_FILE,
            BrokerKind::Dhan => dhan::CONFIG_FILE,
            BrokerKind::Fyers => fyers::CONFIG_FILE,
        }
    }

    /// Config fields that are secrets (kept in the OS keyring when possible).
    pub fn secret_keys(self) -> &'static [&'static str] {
        match self {
            BrokerKind::Angel => angel::SECRET_KEYS,
            BrokerKind::Zerodha => zerodha::SECRET_KEYS,
            BrokerKind::Upstox => upstox::SECRET_KEYS,
            BrokerKind::Dhan => dhan::SECRET_KEYS,
            BrokerKind::Fyers => fyers::SECRET_KEYS,
        }
    }

    /// "Angel One" or "Zerodha Kite (experimental)".
    pub fn display(self) -> String {
        if self.experimental() {
            format!("{} (experimental)", self.label())
        } else {
            self.label().to_string()
        }
    }
}

/// The one-line warning shown wherever an experimental broker is used.
pub const EXPERIMENTAL_NOTE: &str = "experimental — untested against a live account";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        }
    }
}

/// What kind of order to place. Prices are per share in the instrument's
/// currency (₹ for NSE).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderKind {
    Market,
    Limit {
        price: f64,
    },
    /// SL: once `trigger` trades, a limit order at `price` is placed.
    StopLimit {
        trigger: f64,
        price: f64,
    },
    /// SL-M: once `trigger` trades, a market order is placed.
    StopMarket {
        trigger: f64,
    },
}

impl OrderKind {
    /// Build from the `--limit` / `--trigger` flags.
    pub fn from_prices(limit: Option<f64>, trigger: Option<f64>) -> Self {
        match (limit, trigger) {
            (None, None) => OrderKind::Market,
            (Some(price), None) => OrderKind::Limit { price },
            (Some(price), Some(trigger)) => OrderKind::StopLimit { trigger, price },
            (None, Some(trigger)) => OrderKind::StopMarket { trigger },
        }
    }

    /// The conventional Indian order-type code.
    pub fn code(&self) -> &'static str {
        match self {
            OrderKind::Market => "MARKET",
            OrderKind::Limit { .. } => "LIMIT",
            OrderKind::StopLimit { .. } => "SL",
            OrderKind::StopMarket { .. } => "SL-M",
        }
    }

    pub fn price(&self) -> Option<f64> {
        match *self {
            OrderKind::Limit { price } | OrderKind::StopLimit { price, .. } => Some(price),
            _ => None,
        }
    }

    pub fn trigger(&self) -> Option<f64> {
        match *self {
            OrderKind::StopLimit { trigger, .. } | OrderKind::StopMarket { trigger } => {
                Some(trigger)
            }
            _ => None,
        }
    }

    pub fn is_stop(&self) -> bool {
        self.trigger().is_some()
    }
}

/// A broker instrument resolved from a user-typed symbol. Always an exact
/// match — never a fuzzy first hit.
#[derive(Debug, Clone, PartialEq)]
pub struct Instrument {
    /// Plain exchange symbol, e.g. `RELIANCE`.
    pub symbol: String,
    /// The broker's trading symbol, e.g. `RELIANCE-EQ` (Angel, Fyers) or
    /// `RELIANCE` (Kite).
    pub tradingsymbol: String,
    /// The broker's instrument id: Angel symbol token, Kite instrument token,
    /// Upstox instrument key, Dhan security id, Fyers symbol.
    pub token: String,
    pub exchange: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderRequest {
    pub instrument: Instrument,
    pub side: Side,
    pub qty: u32,
    pub kind: OrderKind,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrderAck {
    pub order_id: Option<String>,
}

/// A single-leg GTT: when `trigger` trades, a LIMIT order at `limit` is
/// placed for `qty` shares. Delivery (CNC) product.
#[derive(Debug, Clone, PartialEq)]
pub struct GttRequest {
    pub instrument: Instrument,
    pub side: Side,
    pub qty: u32,
    pub trigger: f64,
    pub limit: f64,
    /// Last traded price when the GTT was created (some brokers require it).
    pub last_price: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Gtt {
    pub id: String,
    pub symbol: String,
    pub side: Option<String>,
    pub qty: Option<f64>,
    pub trigger: Option<f64>,
    pub limit: Option<f64>,
    pub status: Option<String>,
}

/// A delivery holding. Missing prices are `None`, never 0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Holding {
    /// Plain exchange symbol (series suffix removed), e.g. `RELIANCE`.
    pub symbol: String,
    pub exchange: Option<String>,
    pub isin: Option<String>,
    pub qty: f64,
    pub avg_price: Option<f64>,
    pub ltp: Option<f64>,
    pub pnl: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub exchange: Option<String>,
    pub product: Option<String>,
    /// Net quantity: negative for a short position.
    pub qty: f64,
    pub avg_price: Option<f64>,
    pub ltp: Option<f64>,
    pub pnl: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub side: Option<String>,
    pub order_type: Option<String>,
    pub qty: Option<f64>,
    pub price: Option<f64>,
    pub trigger: Option<f64>,
    pub status: Option<String>,
    pub message: Option<String>,
}

/// One logged-in broker session.
#[async_trait]
pub trait Broker: Send + Sync {
    fn kind(&self) -> BrokerKind;
    async fn holdings(&self) -> Result<Vec<Holding>>;
    async fn positions(&self) -> Result<Vec<Position>>;
    /// Today's order book.
    async fn orders(&self) -> Result<Vec<Order>>;
    /// The exact cash-equity instrument for `symbol` on `exchange` (`NSE`),
    /// or an error. Never a partial or first-of-many match.
    async fn resolve_instrument(&self, symbol: &str, exchange: &str) -> Result<Instrument>;
    /// Send one order. Only `place_live_order` may call this.
    async fn place_order(&self, order: &OrderRequest) -> Result<OrderAck>;
    /// Whether the GTT methods below are implemented for this broker.
    fn supports_gtt(&self) -> bool {
        false
    }
    /// Create a GTT; returns its id. Only `place_live_order` may call this.
    async fn gtt_create(&self, _gtt: &GttRequest) -> Result<String> {
        bail!("{} GTT orders are not supported yet", self.kind().label())
    }
    async fn gtt_list(&self) -> Result<Vec<Gtt>> {
        bail!("{} GTT orders are not supported yet", self.kind().label())
    }
    async fn gtt_cancel(&self, _id: &str) -> Result<()> {
        bail!("{} GTT orders are not supported yet", self.kind().label())
    }
}

/// Log in (Angel One) or load the saved session (the others) for `kind`.
pub async fn connect(kind: BrokerKind) -> Result<Box<dyn Broker>> {
    Ok(match kind {
        BrokerKind::Angel => Box::new(angel::AngelClient::new().await?),
        BrokerKind::Zerodha => Box::new(zerodha::KiteClient::connect()?),
        BrokerKind::Upstox => Box::new(upstox::UpstoxClient::connect()?),
        BrokerKind::Dhan => Box::new(dhan::DhanClient::connect()?),
        BrokerKind::Fyers => Box::new(fyers::FyersClient::connect()?),
    })
}

/// Interactive login for brokers whose sessions expire daily.
pub async fn login(kind: BrokerKind) -> Result<()> {
    match kind {
        BrokerKind::Angel => {
            // Angel One logs in with password + TOTP on every command.
            angel::AngelClient::new().await?;
            println!(
                "\n  Angel One logs in automatically on every command (PIN + TOTP); nothing to save.\n"
            );
            Ok(())
        }
        BrokerKind::Zerodha => zerodha::login().await,
        BrokerKind::Upstox => upstox::login().await,
        BrokerKind::Dhan => dhan::login().await,
        BrokerKind::Fyers => fyers::login().await,
    }
}

/// What `trade config` and `doctor` show about a broker's credentials.
#[derive(Debug, Clone)]
pub struct ConfigStatus {
    pub path: std::path::PathBuf,
    pub exists: bool,
    /// All required credentials are filled in.
    pub configured: bool,
    /// Non-secret identity, e.g. "client A123" or "api key kitefront".
    pub summary: String,
    pub storage: secrets::Storage,
    /// For brokers with daily sessions: when `trade login` last succeeded.
    pub session: Option<String>,
}

/// Read a broker's config. With `migrate`, plaintext secrets move into the
/// OS keyring when one is available.
pub fn config_status(kind: BrokerKind, migrate: bool) -> Result<ConfigStatus> {
    match kind {
        BrokerKind::Angel => angel::status(migrate),
        BrokerKind::Zerodha => zerodha::status(migrate),
        BrokerKind::Upstox => upstox::status(migrate),
        BrokerKind::Dhan => dhan::status(migrate),
        BrokerKind::Fyers => fyers::status(migrate),
    }
}

/// Write a credentials template with placeholder values (owner-only).
pub fn write_template(kind: BrokerKind) -> Result<()> {
    match kind {
        BrokerKind::Angel => angel::AngelConfig::template().save(),
        BrokerKind::Zerodha => zerodha::write_template(),
        BrokerKind::Upstox => upstox::write_template(),
        BrokerKind::Dhan => dhan::write_template(),
        BrokerKind::Fyers => fyers::write_template(),
    }
}

/// Numbered setup steps for `trade setup`.
pub fn setup_steps(kind: BrokerKind) -> &'static [&'static str] {
    match kind {
        BrokerKind::Angel => angel::SETUP,
        BrokerKind::Zerodha => zerodha::SETUP,
        BrokerKind::Upstox => upstox::SETUP,
        BrokerKind::Dhan => dhan::SETUP,
        BrokerKind::Fyers => fyers::SETUP,
    }
}

/// Today's date in India (sessions of Indian brokers roll over in IST).
pub fn ist_now() -> chrono::DateTime<chrono::FixedOffset> {
    let ist = chrono::FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("valid offset");
    chrono::Utc::now().with_timezone(&ist)
}

/// Build a [`ConfigStatus`] from a loaded config.
fn status_of(
    kind: BrokerKind,
    configured: bool,
    summary: String,
    storage: secrets::Storage,
    session: Option<String>,
) -> Result<ConfigStatus> {
    let path = crate::store::path(kind.config_file())?;
    Ok(ConfigStatus {
        exists: path.exists(),
        path,
        configured,
        summary,
        storage,
        session,
    })
}

/// Error for a broker that has no saved credentials yet.
fn not_configured(kind: BrokerKind) -> anyhow::Error {
    let path = crate::store::path(kind.config_file())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| kind.config_file().to_string());
    anyhow::anyhow!(
        "{} is not configured. Run `stockwise trade setup --broker {}` for instructions, then edit {}",
        kind.label(),
        kind.id(),
        path
    )
}

/// Error for a broker whose daily session has not been created yet.
fn not_logged_in(kind: BrokerKind) -> anyhow::Error {
    anyhow::anyhow!(
        "No {} session. Log in with `stockwise trade login --broker {}` (sessions expire daily).",
        kind.label(),
        kind.id()
    )
}

/// Order guards shared by every broker, read from its config file.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrderLimits {
    /// Refuse any single order whose estimated value exceeds this (₹).
    pub max_order_value: Option<f64>,
    /// `"enable_orders": true` in the config file: the user opted in to
    /// order paths that were never run against a live account (every order
    /// on an experimental broker; stop-loss and GTT orders on Angel One).
    pub enable_orders: bool,
}

/// Fields every broker config file may carry, besides its credentials.
#[derive(Debug, Clone, Default, Deserialize)]
struct CommonSettings {
    #[serde(default)]
    max_order_value: Option<f64>,
    #[serde(default)]
    enable_orders: bool,
}

/// Read the order guards for `kind` without touching its secrets.
pub fn order_limits(kind: BrokerKind) -> Result<OrderLimits> {
    let path = crate::store::path(kind.config_file())?;
    let common: CommonSettings = crate::store::load_from(&path)?;
    Ok(OrderLimits {
        max_order_value: common.max_order_value,
        enable_orders: common.enable_orders,
    })
}

// ── Order validation (pure; tested below) ──

fn check_price(name: &str, v: f64) -> Result<()> {
    if !(v.is_finite() && v > 0.0) {
        bail!("{name} must be a positive number");
    }
    Ok(())
}

/// A limit more than this far from its trigger is treated as a typo.
const MAX_LIMIT_TRIGGER_GAP: f64 = 0.20;

fn check_limit_near_trigger(limit: f64, trigger: f64) -> Result<()> {
    let gap = (limit - trigger).abs() / trigger;
    if gap > MAX_LIMIT_TRIGGER_GAP {
        bail!(
            "Limit ₹{limit:.2} is {:.0}% away from the trigger ₹{trigger:.2} — check for a typo \
             (StockWise refuses gaps above {:.0}%)",
            gap * 100.0,
            MAX_LIMIT_TRIGGER_GAP * 100.0
        );
    }
    Ok(())
}

/// Check prices and, for stop-loss orders, the trigger against the limit,
/// the side and the last traded price. Stop orders need a last price: a
/// trigger on the wrong side of the market would fire immediately.
pub fn validate_order(side: Side, kind: &OrderKind, last: Option<f64>) -> Result<()> {
    if let Some(p) = kind.price() {
        check_price("Limit price", p)?;
    }
    let Some(trigger) = kind.trigger() else {
        return Ok(());
    };
    check_price("Trigger price", trigger)?;
    if let Some(price) = kind.price() {
        match side {
            Side::Buy if price < trigger => bail!(
                "For a stop-loss BUY the limit (₹{price:.2}) must be at or above the trigger (₹{trigger:.2})"
            ),
            Side::Sell if price > trigger => bail!(
                "For a stop-loss SELL the limit (₹{price:.2}) must be at or below the trigger (₹{trigger:.2})"
            ),
            _ => {}
        }
        check_limit_near_trigger(price, trigger)?;
    }
    let Some(last) = last else {
        bail!(
            "Cannot fetch the last price, so the trigger can't be checked against the market. \
             Try again in a moment."
        );
    };
    match side {
        Side::Buy if trigger <= last => bail!(
            "A stop-loss BUY fires when the price rises to the trigger, so the trigger (₹{trigger:.2}) \
             must be above the last price (₹{last:.2}). To buy at or below the market, use --limit."
        ),
        Side::Sell if trigger >= last => bail!(
            "A stop-loss SELL fires when the price falls to the trigger, so the trigger (₹{trigger:.2}) \
             must be below the last price (₹{last:.2}). To sell at or above the market, use --limit."
        ),
        _ => Ok(()),
    }
}

/// Check a single-leg GTT. Either direction is valid (a BUY above the market
/// is a breakout entry, below it a dip buy; a SELL above is a target, below
/// a stop-loss), but the trigger must not sit on the last price and the
/// limit must be near the trigger. A stop GTT (SELL below the market, BUY
/// above it) fires into a moving price, so its limit must be marketable at
/// the trigger: at or below it for a SELL, at or above it for a BUY —
/// otherwise the order could rest unfilled and leave the position open.
pub fn validate_gtt(side: Side, trigger: f64, limit: f64, last: Option<f64>) -> Result<()> {
    check_price("Trigger price", trigger)?;
    check_price("Limit price", limit)?;
    check_limit_near_trigger(limit, trigger)?;
    let Some(last) = last else {
        bail!("Cannot fetch the last price, which a GTT needs. Try again in a moment.");
    };
    if ((trigger - last) / last).abs() < 0.0025 {
        bail!(
            "The trigger (₹{trigger:.2}) is within 0.25% of the last price (₹{last:.2}); \
             it would fire immediately. Place a regular order instead."
        );
    }
    match side {
        Side::Sell if trigger < last && limit > trigger => bail!(
            "A stop-loss SELL GTT (trigger ₹{trigger:.2} below the market) needs its limit at or \
             below the trigger, or the sell may never fill; got ₹{limit:.2}"
        ),
        Side::Buy if trigger > last && limit < trigger => bail!(
            "A stop BUY GTT (trigger ₹{trigger:.2} above the market) needs its limit at or above \
             the trigger, or the buy may never fill; got ₹{limit:.2}"
        ),
        _ => Ok(()),
    }
}

/// Prices with more than 2 decimals: refused rather than silently rounded,
/// so what the preview shows is what is sent.
pub fn check_paise(name: &str, v: f64) -> Result<()> {
    let paise = v * 100.0;
    if (paise - paise.round()).abs() > 1e-6 {
        bail!("{name} ₹{v} has more than 2 decimals");
    }
    Ok(())
}

/// Estimated order value for the preview and the `max_order_value` cap.
/// Uses the limit price when there is one; SL-M uses the higher of trigger
/// and last price, since it fills at market after triggering.
pub fn estimate_value(kind: &OrderKind, qty: u32, last: Option<f64>) -> Option<f64> {
    let per_share = match *kind {
        OrderKind::Market => last?,
        OrderKind::Limit { price } | OrderKind::StopLimit { price, .. } => price,
        OrderKind::StopMarket { trigger } => last.map_or(trigger, |l| l.max(trigger)),
    };
    Some(per_share * qty as f64)
}

/// Drop an NSE/BSE series suffix from a broker trading symbol:
/// `RELIANCE-EQ` / `CDSL-BE` → `RELIANCE` / `CDSL`. Hyphenated names such
/// as `BAJAJ-AUTO` are left alone.
pub fn strip_series(tradingsymbol: &str) -> &str {
    const SERIES: [&str; 9] = ["EQ", "BE", "BZ", "BL", "SM", "ST", "IL", "IQ", "RL"];
    match tradingsymbol.rsplit_once('-') {
        Some((base, series)) if !base.is_empty() && SERIES.contains(&series) => base,
        _ => tradingsymbol,
    }
}

/// `RELIANCE.NS` / `reliance-eq` / ` Reliance ` → `RELIANCE`. Other
/// suffixes (`.BO`) are kept.
pub fn plain_symbol(symbol: &str) -> String {
    let s = symbol.trim().to_uppercase();
    let s = s.strip_suffix(".NS").unwrap_or(&s);
    s.strip_suffix("-EQ").unwrap_or(s).to_string()
}

/// The NSE symbol to trade. Orders go to NSE only, so a BSE (`.BO`) or other
/// suffixed ticker is refused instead of quietly trading the NSE listing.
pub fn nse_symbol(symbol: &str) -> Result<String> {
    let s = plain_symbol(symbol);
    if s.is_empty() {
        bail!("Symbol required");
    }
    if s.contains('.') {
        bail!("StockWise places orders on NSE only; use the NSE symbol (e.g. RELIANCE), not {s}");
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_kind_from_flags() {
        assert_eq!(OrderKind::from_prices(None, None), OrderKind::Market);
        assert_eq!(OrderKind::from_prices(Some(10.0), None).code(), "LIMIT");
        assert_eq!(OrderKind::from_prices(Some(10.0), Some(9.0)).code(), "SL");
        assert_eq!(OrderKind::from_prices(None, Some(9.0)).code(), "SL-M");
    }

    #[test]
    fn stop_loss_sell_needs_trigger_below_market_and_limit_below_trigger() {
        let ok = OrderKind::StopLimit {
            trigger: 95.0,
            price: 94.5,
        };
        assert!(validate_order(Side::Sell, &ok, Some(100.0)).is_ok());
        // Trigger above the market would fire at once.
        let above = OrderKind::StopMarket { trigger: 101.0 };
        assert!(validate_order(Side::Sell, &above, Some(100.0)).is_err());
        // Limit above the trigger is not a stop-loss sell.
        let inverted = OrderKind::StopLimit {
            trigger: 95.0,
            price: 96.0,
        };
        assert!(validate_order(Side::Sell, &inverted, Some(100.0)).is_err());
    }

    #[test]
    fn stop_loss_buy_needs_trigger_above_market_and_limit_above_trigger() {
        let ok = OrderKind::StopLimit {
            trigger: 105.0,
            price: 105.5,
        };
        assert!(validate_order(Side::Buy, &ok, Some(100.0)).is_ok());
        let below = OrderKind::StopMarket { trigger: 99.0 };
        assert!(validate_order(Side::Buy, &below, Some(100.0)).is_err());
        let inverted = OrderKind::StopLimit {
            trigger: 105.0,
            price: 104.0,
        };
        assert!(validate_order(Side::Buy, &inverted, Some(100.0)).is_err());
    }

    #[test]
    fn stop_orders_need_a_last_price() {
        let sl = OrderKind::StopMarket { trigger: 95.0 };
        assert!(validate_order(Side::Sell, &sl, None).is_err());
        // Plain limit/market orders don't.
        assert!(validate_order(Side::Buy, &OrderKind::Limit { price: 10.0 }, None).is_ok());
        assert!(validate_order(Side::Buy, &OrderKind::Market, None).is_ok());
    }

    #[test]
    fn typo_sized_gap_between_limit_and_trigger_is_refused() {
        let typo = OrderKind::StopLimit {
            trigger: 950.0,
            price: 95.0,
        };
        assert!(validate_order(Side::Sell, &typo, Some(1000.0)).is_err());
        assert!(validate_gtt(Side::Sell, 1200.0, 120.0, Some(1300.0)).is_err());
    }

    #[test]
    fn bad_numbers_are_refused() {
        for bad in [0.0, -5.0, f64::NAN, f64::INFINITY] {
            assert!(validate_order(Side::Buy, &OrderKind::Limit { price: bad }, None).is_err());
            assert!(validate_gtt(Side::Buy, bad, 100.0, Some(120.0)).is_err());
        }
    }

    #[test]
    fn gtt_trigger_must_not_sit_on_the_market() {
        assert!(validate_gtt(Side::Sell, 1200.0, 1195.0, Some(1300.0)).is_ok());
        assert!(validate_gtt(Side::Buy, 1400.0, 1405.0, Some(1300.0)).is_ok());
        assert!(validate_gtt(Side::Sell, 1300.5, 1300.0, Some(1300.0)).is_err());
        assert!(validate_gtt(Side::Sell, 1200.0, 1195.0, None).is_err());
    }

    #[test]
    fn stop_gtts_need_a_marketable_limit() {
        // Stop-loss SELL below the market with the limit above the trigger:
        // the sell could rest unfilled as the price falls.
        assert!(validate_gtt(Side::Sell, 1200.0, 1230.0, Some(1300.0)).is_err());
        // Stop BUY above the market with the limit below the trigger.
        assert!(validate_gtt(Side::Buy, 1400.0, 1390.0, Some(1300.0)).is_err());
        // Targets and dip buys may rest beyond the trigger (Kite's own examples do).
        assert!(validate_gtt(Side::Sell, 1400.0, 1405.0, Some(1300.0)).is_ok());
        assert!(validate_gtt(Side::Buy, 1200.0, 1195.0, Some(1300.0)).is_ok());
    }

    #[test]
    fn prices_with_more_than_two_decimals_are_refused() {
        assert!(check_paise("Limit price", 1300.45).is_ok());
        assert!(check_paise("Limit price", 0.05).is_ok());
        assert!(check_paise("Limit price", 1300.456).is_err());
    }

    #[test]
    fn value_estimates() {
        assert_eq!(
            estimate_value(&OrderKind::Market, 10, Some(100.0)),
            Some(1000.0)
        );
        assert_eq!(estimate_value(&OrderKind::Market, 10, None), None);
        assert_eq!(
            estimate_value(&OrderKind::Limit { price: 50.0 }, 2, None),
            Some(100.0)
        );
        // SL-M buy above market: trigger is the higher price.
        assert_eq!(
            estimate_value(&OrderKind::StopMarket { trigger: 110.0 }, 1, Some(100.0)),
            Some(110.0)
        );
        // SL-M sell below market: last price is the higher one.
        assert_eq!(
            estimate_value(&OrderKind::StopMarket { trigger: 90.0 }, 1, Some(100.0)),
            Some(100.0)
        );
    }

    #[test]
    fn strip_series_keeps_hyphenated_names() {
        assert_eq!(strip_series("RELIANCE-EQ"), "RELIANCE");
        assert_eq!(strip_series("CDSL-BE"), "CDSL");
        assert_eq!(strip_series("BAJAJ-AUTO"), "BAJAJ-AUTO");
        assert_eq!(strip_series("BAJAJ-AUTO-EQ"), "BAJAJ-AUTO");
        assert_eq!(strip_series("INFY"), "INFY");
    }

    #[test]
    fn plain_symbol_strips_suffixes() {
        assert_eq!(plain_symbol(" reliance.ns "), "RELIANCE");
        assert_eq!(plain_symbol("TCS-EQ"), "TCS");
        assert_eq!(plain_symbol("M&M"), "M&M");
        assert_eq!(plain_symbol("INFY.BO"), "INFY.BO");
        assert_eq!(nse_symbol("infy.ns").unwrap(), "INFY");
        assert!(nse_symbol("INFY.BO").is_err());
        assert!(nse_symbol(" ").is_err());
    }

    #[test]
    fn only_angel_is_not_experimental() {
        for k in BrokerKind::ALL {
            assert_eq!(k.experimental(), k != BrokerKind::Angel);
            assert!(k.config_file().ends_with("_config.json"));
        }
    }
}

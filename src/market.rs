use crate::i18n::{Msg, t};
use clap::ValueEnum;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum Market {
    /// US stock market (NYSE/NASDAQ)
    #[value(alias = "US")]
    Us,
    /// Indian stock market (NSE/BSE)
    #[value(alias = "IN", alias = "india")]
    In,
    /// Crypto, trades 24/7: BTC → BTC-USD (BTC-INR with --quote-currency inr)
    #[value(alias = "CRYPTO")]
    Crypto,
    /// Currency pairs, 24x5: USDINR → USDINR=X
    #[value(alias = "FX", alias = "forex")]
    Fx,
}

/// What kind of instrument a Yahoo symbol is. Decides how prices are
/// formatted (crypto and FX need more decimals) and whether exchange hours
/// apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetClass {
    Stock,
    Index,
    Crypto,
    Fx,
}

/// Currencies a crypto pair can be quoted in on Yahoo (`BTC-USD`, `ETH-INR`).
const CRYPTO_QUOTES: &[&str] = &["USD", "INR", "EUR", "GBP", "JPY", "USDT", "BTC", "ETH"];

impl AssetClass {
    pub fn of(symbol: &str) -> Self {
        let upper = symbol.trim().to_ascii_uppercase();
        if upper.starts_with('^') {
            return AssetClass::Index;
        }
        if upper.ends_with("=X") {
            return AssetClass::Fx;
        }
        // `BTC-USD` is crypto, `BRK-B` and `BAJAJ-AUTO.NS` are stocks.
        if !upper.contains('.')
            && let Some((base, quote)) = upper.rsplit_once('-')
            && !base.is_empty()
            && CRYPTO_QUOTES.contains(&quote)
        {
            return AssetClass::Crypto;
        }
        AssetClass::Stock
    }

    /// Whether the instrument trades around the clock (no exchange session).
    pub fn trades_24x7(self) -> bool {
        self == AssetClass::Crypto
    }
}

static QUOTE_CURRENCY: OnceLock<String> = OnceLock::new();

/// Set the currency bare crypto tickers are quoted in (`--quote-currency`).
/// Only the first call wins; it is made once at startup.
pub fn set_quote_currency(code: &str) {
    let code = code.trim().to_ascii_uppercase();
    if !code.is_empty() {
        let _ = QUOTE_CURRENCY.set(code);
    }
}

/// Currency for bare crypto tickers: `USD` unless `--quote-currency` or
/// `STOCKWISE_QUOTE_CURRENCY` says otherwise.
pub fn quote_currency() -> &'static str {
    QUOTE_CURRENCY.get().map(String::as_str).unwrap_or("USD")
}

/// Normalise a user-typed symbol for Yahoo Finance.
///
/// In the Indian market a bare ticker gets the NSE suffix (`TCS` → `TCS.NS`).
/// Symbols that already say where they trade are passed through untouched:
/// an exchange suffix (`RELIANCE.BO`, `VOD.L`), an index (`^NSEI`), or a
/// currency/futures pair (`USDINR=X`, `BTC-USD`).
///
/// In the crypto market a bare coin gets the quote currency (`BTC` →
/// `BTC-USD`); in the FX market a pair gets Yahoo's `=X` suffix (`USDINR`,
/// `usd/inr` → `USDINR=X`).
pub fn resolve_symbol(symbol: &str, market: Market) -> String {
    resolve_symbol_in(symbol, market, quote_currency())
}

fn resolve_symbol_in(symbol: &str, market: Market, crypto_quote: &str) -> String {
    let upper = symbol.trim().to_uppercase();
    let explicit = upper.contains('.')
        || upper.starts_with('^')
        || upper.contains('=')
        || upper.ends_with("-USD")
        || upper.ends_with("-INR");
    match market {
        Market::Us => upper,
        Market::In if explicit => upper,
        Market::In => format!("{}.NS", upper),
        Market::Crypto if upper.contains('-') || explicit => upper,
        Market::Crypto => format!("{}-{}", upper, crypto_quote),
        Market::Fx if upper.contains('=') || upper.starts_with('^') => upper,
        Market::Fx => {
            let pair: String = upper
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect();
            format!("{}=X", pair)
        }
    }
}

/// Hint shown when a symbol is not found, pointing at the most common cause:
/// a US ticker looked up in the default Indian market.
pub fn not_found_hint(market: Market) -> &'static str {
    t(match market {
        Market::In => Msg::HintNotFoundIn,
        Market::Us => Msg::HintNotFoundUs,
        Market::Crypto => Msg::HintNotFoundCrypto,
        Market::Fx => Msg::HintNotFoundFx,
    })
}

impl Market {
    /// Short name for titles: "India", "US", "Crypto", "FX".
    pub fn label(self) -> &'static str {
        match self {
            Market::In => "India",
            Market::Us => "US",
            Market::Crypto => "Crypto",
            Market::Fx => "FX",
        }
    }

    /// Whether this is a stock exchange (with sessions, sectors, fundamentals).
    pub fn is_stock(self) -> bool {
        matches!(self, Market::In | Market::Us)
    }

    /// Fail clearly for commands built on stock-exchange concepts — trading
    /// sessions, sectors, fundamentals — instead of running them on coins or
    /// currency pairs.
    pub fn require_stocks(self, command: &str) -> anyhow::Result<()> {
        if self.is_stock() {
            return Ok(());
        }
        anyhow::bail!(
            "`{}` works on stock markets only (-m in or -m us); {} {}",
            command,
            self.label(),
            if self == Market::Crypto {
                "trades 24/7 without sessions, sectors or fundamentals"
            } else {
                "trades 24x5 without exchange sessions, sectors or fundamentals"
            }
        )
    }

    /// Currency symbol for amounts the user types (capital, SIP budget).
    pub fn currency_symbol(self) -> &'static str {
        match self {
            Market::In => "\u{20B9}",
            Market::Us | Market::Fx => "$",
            Market::Crypto => currency_symbol(Some(quote_currency())),
        }
    }

    /// Symbols scanned by movers, screeners and the bots.
    pub fn universe(self) -> Vec<String> {
        match self {
            Market::In => INDIA_POPULAR.iter().map(|s| s.to_string()).collect(),
            Market::Us => US_POPULAR.iter().map(|s| s.to_string()).collect(),
            Market::Crypto => crypto_majors().into_iter().map(|(s, _)| s).collect(),
            Market::Fx => FX_MAJORS.iter().map(|(s, _)| s.to_string()).collect(),
        }
    }

    /// Sector indices or ETFs; empty for crypto and FX.
    pub fn sectors(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Market::In => INDIA_SECTOR_INDICES,
            Market::Us => US_SECTOR_ETFS,
            Market::Crypto | Market::Fx => &[],
        }
    }
}

/// The largest coins, quoted in the active quote currency.
pub fn crypto_majors() -> Vec<(String, &'static str)> {
    let quote = quote_currency();
    CRYPTO_MAJORS
        .iter()
        .map(|(coin, name)| (format!("{}-{}", coin, quote), *name))
        .collect()
}

/// A one-line note on trading hours for 24/7 and 24x5 instruments, or
/// `None` for exchange-listed ones (whose sessions Yahoo already reflects).
pub fn session_note(class: AssetClass, now: chrono::DateTime<chrono::Utc>) -> Option<Msg> {
    use chrono::{Datelike, Timelike, Weekday};
    match class {
        AssetClass::Crypto => Some(Msg::SessionCrypto),
        AssetClass::Fx => {
            // The FX week runs from Sunday ~21:00 UTC to Friday ~21:00 UTC.
            let closed = match now.weekday() {
                Weekday::Sat => true,
                Weekday::Fri => now.hour() >= 21,
                Weekday::Sun => now.hour() < 21,
                _ => false,
            };
            Some(if closed {
                Msg::SessionFxWeekend
            } else {
                Msg::SessionFxOpen
            })
        }
        AssetClass::Stock | AssetClass::Index => None,
    }
}

/// Get the currency symbol character for display
pub fn currency_symbol(currency: Option<&str>) -> &'static str {
    match currency {
        Some("INR") => "\u{20B9}",
        Some("EUR") => "\u{20AC}",
        Some("GBP") => "\u{00A3}",
        Some("JPY") => "\u{00A5}",
        _ => "$",
    }
}

/// Check if currency is INR (for lakhs/crores formatting)
pub fn is_inr(currency: Option<&str>) -> bool {
    currency == Some("INR")
}

pub const US_INDICES: &[(&str, &str)] = &[
    ("^GSPC", "S&P 500"),
    ("^DJI", "Dow Jones"),
    ("^IXIC", "NASDAQ"),
    ("^RUT", "Russell 2000"),
    ("^VIX", "VIX"),
];

pub const INDIA_INDICES: &[(&str, &str)] = &[
    ("^NSEI", "NIFTY 50"),
    ("^BSESN", "SENSEX"),
    ("^NSEBANK", "BANK NIFTY"),
    ("^CNXIT", "NIFTY IT"),
];

pub const US_SECTOR_ETFS: &[(&str, &str)] = &[
    ("XLK", "Technology"),
    ("XLF", "Financials"),
    ("XLV", "Healthcare"),
    ("XLY", "Cons. Discret."),
    ("XLP", "Cons. Staples"),
    ("XLE", "Energy"),
    ("XLI", "Industrials"),
    ("XLB", "Materials"),
    ("XLU", "Utilities"),
    ("XLRE", "Real Estate"),
    ("XLC", "Communication"),
];

pub const INDIA_SECTOR_INDICES: &[(&str, &str)] = &[
    ("^CNXIT", "IT"),
    ("^NSEBANK", "Bank"),
    ("^CNXPHARMA", "Pharma"),
    ("^CNXAUTO", "Auto"),
    ("^CNXFMCG", "FMCG"),
    ("^CNXMETAL", "Metal"),
    ("^CNXREALTY", "Realty"),
    ("^CNXENERGY", "Energy"),
];

/// Popular large-cap symbols for the movers feature
pub const US_POPULAR: &[&str] = &[
    "AAPL", "MSFT", "GOOG", "AMZN", "NVDA", "META", "TSLA", "BRK-B", "JPM", "V", "UNH", "XOM",
    "JNJ", "WMT", "MA", "PG", "HD", "COST", "MRK", "ABBV", "CRM", "AVGO", "PEP", "KO", "TMO",
    "ADBE", "NFLX", "CSCO", "ACN", "AMD",
];

pub const INDIA_POPULAR: &[&str] = &[
    "RELIANCE.NS",
    "TCS.NS",
    "HDFCBANK.NS",
    "INFY.NS",
    "ICICIBANK.NS",
    "HINDUNILVR.NS",
    "SBIN.NS",
    "BHARTIARTL.NS",
    "ITC.NS",
    "KOTAKBANK.NS",
    "LT.NS",
    "AXISBANK.NS",
    "ASIANPAINT.NS",
    "MARUTI.NS",
    "TITAN.NS",
    "SUNPHARMA.NS",
    "TMPV.NS", // Tata Motors Passenger Vehicles; TATAMOTORS.NS no longer quotes after the demerger
    "BAJFINANCE.NS",
    "WIPRO.NS",
    "HCLTECH.NS",
    "ONGC.NS",
    "NTPC.NS",
    "POWERGRID.NS",
    "TATASTEEL.NS",
    "ADANIENT.NS",
    "ULTRACEMCO.NS",
    "JSWSTEEL.NS",
    "TECHM.NS",
    "INDUSINDBK.NS",
    "DRREDDY.NS",
];

/// Coins for `markets --crypto` and the crypto universe, without the quote
/// currency (added by `crypto_majors`).
pub const CRYPTO_MAJORS: &[(&str, &str)] = &[
    ("BTC", "Bitcoin"),
    ("ETH", "Ethereum"),
    ("SOL", "Solana"),
    ("XRP", "XRP"),
    ("BNB", "BNB"),
    ("DOGE", "Dogecoin"),
    ("ADA", "Cardano"),
    ("TRX", "TRON"),
];

/// Currency pairs for `markets --fx` and the FX universe.
pub const FX_MAJORS: &[(&str, &str)] = &[
    ("USDINR=X", "USD/INR"),
    ("EURINR=X", "EUR/INR"),
    ("GBPINR=X", "GBP/INR"),
    ("EURUSD=X", "EUR/USD"),
    ("GBPUSD=X", "GBP/USD"),
    ("USDJPY=X", "USD/JPY"),
];

/// Predefined screener categories
pub const SCREENER_CATEGORIES: &[&str] =
    &["undervalued", "growth", "dividend", "momentum", "bluechip"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_symbol_us() {
        assert_eq!(resolve_symbol("aapl", Market::Us), "AAPL");
        assert_eq!(resolve_symbol("MSFT", Market::Us), "MSFT");
    }

    #[test]
    fn test_resolve_symbol_india() {
        assert_eq!(resolve_symbol("reliance", Market::In), "RELIANCE.NS");
        assert_eq!(resolve_symbol("TCS", Market::In), "TCS.NS");
        // Already suffixed
        assert_eq!(resolve_symbol("RELIANCE.NS", Market::In), "RELIANCE.NS");
        assert_eq!(resolve_symbol("RELIANCE.BO", Market::In), "RELIANCE.BO");
    }

    #[test]
    fn test_currency_symbol() {
        assert_eq!(currency_symbol(Some("INR")), "\u{20B9}");
        assert_eq!(currency_symbol(Some("USD")), "$");
        assert_eq!(currency_symbol(None), "$");
    }

    #[test]
    fn test_is_inr() {
        assert!(is_inr(Some("INR")));
        assert!(!is_inr(Some("USD")));
        assert!(!is_inr(None));
    }

    #[test]
    fn test_resolve_symbol_india_passes_through_explicit_symbols() {
        assert_eq!(resolve_symbol("^nsei", Market::In), "^NSEI");
        assert_eq!(resolve_symbol("^GSPC", Market::In), "^GSPC");
        assert_eq!(resolve_symbol("usdinr=x", Market::In), "USDINR=X");
        assert_eq!(resolve_symbol("btc-usd", Market::In), "BTC-USD");
        assert_eq!(resolve_symbol("vod.l", Market::In), "VOD.L");
        assert_eq!(resolve_symbol(" m&m ", Market::In), "M&M.NS");
        assert_eq!(resolve_symbol("BAJAJ-AUTO", Market::In), "BAJAJ-AUTO.NS");
    }

    #[test]
    fn test_resolve_symbol_us_lowercase() {
        assert_eq!(resolve_symbol("tsla", Market::Us), "TSLA");
    }

    #[test]
    fn test_resolve_symbol_india_bo_suffix() {
        // .BO suffix preserved
        assert_eq!(resolve_symbol("RELIANCE.BO", Market::In), "RELIANCE.BO");
    }

    #[test]
    fn test_resolve_symbol_india_lowercase_with_ns() {
        // If already has .NS, don't double-append
        assert_eq!(resolve_symbol("tcs.ns", Market::In), "TCS.NS");
    }

    #[test]
    fn test_resolve_symbol_us_already_upper() {
        assert_eq!(resolve_symbol("AAPL", Market::Us), "AAPL");
    }

    #[test]
    fn test_currency_symbol_other_currencies() {
        assert_eq!(currency_symbol(Some("GBP")), "\u{00A3}");
        assert_eq!(currency_symbol(Some("EUR")), "\u{20AC}");
        assert_eq!(currency_symbol(Some("JPY")), "\u{00A5}");
        // Unknown currencies fall back to $
        assert_eq!(currency_symbol(Some("CHF")), "$");
    }

    #[test]
    fn test_popular_lists_non_empty() {
        assert!(!US_POPULAR.is_empty());
        assert!(!INDIA_POPULAR.is_empty());
    }

    #[test]
    fn test_india_popular_has_ns_suffix() {
        // All India popular symbols should have .NS suffix
        for sym in INDIA_POPULAR {
            assert!(sym.ends_with(".NS"), "{} missing .NS suffix", sym);
        }
    }

    #[test]
    fn test_screener_categories_non_empty() {
        assert!(!SCREENER_CATEGORIES.is_empty());
        assert!(SCREENER_CATEGORIES.contains(&"undervalued"));
        assert!(SCREENER_CATEGORIES.contains(&"dividend"));
    }

    #[test]
    fn crypto_resolves_bare_coins_to_the_quote_currency() {
        assert_eq!(resolve_symbol_in("btc", Market::Crypto, "USD"), "BTC-USD");
        assert_eq!(resolve_symbol_in("ETH", Market::Crypto, "INR"), "ETH-INR");
        // Full pairs and other explicit symbols pass through.
        assert_eq!(
            resolve_symbol_in("btc-inr", Market::Crypto, "USD"),
            "BTC-INR"
        );
        assert_eq!(
            resolve_symbol_in("ETH-EUR", Market::Crypto, "USD"),
            "ETH-EUR"
        );
        assert_eq!(resolve_symbol_in("^GSPC", Market::Crypto, "USD"), "^GSPC");
    }

    #[test]
    fn fx_resolves_pairs_to_yahoo_symbols() {
        assert_eq!(resolve_symbol_in("USDINR", Market::Fx, "USD"), "USDINR=X");
        assert_eq!(resolve_symbol_in("usd/inr", Market::Fx, "USD"), "USDINR=X");
        assert_eq!(resolve_symbol_in("eur-usd", Market::Fx, "USD"), "EURUSD=X");
        assert_eq!(resolve_symbol_in("USDINR=X", Market::Fx, "USD"), "USDINR=X");
        assert_eq!(resolve_symbol_in("INR", Market::Fx, "USD"), "INR=X");
    }

    #[test]
    fn asset_class_from_symbol() {
        assert_eq!(AssetClass::of("BTC-USD"), AssetClass::Crypto);
        assert_eq!(AssetClass::of("eth-inr"), AssetClass::Crypto);
        assert_eq!(AssetClass::of("USDINR=X"), AssetClass::Fx);
        assert_eq!(AssetClass::of("^NSEI"), AssetClass::Index);
        assert_eq!(AssetClass::of("BRK-B"), AssetClass::Stock);
        assert_eq!(AssetClass::of("BAJAJ-AUTO.NS"), AssetClass::Stock);
        assert_eq!(AssetClass::of("TCS.NS"), AssetClass::Stock);
        assert!(AssetClass::of("BTC-USD").trades_24x7());
        assert!(!AssetClass::of("USDINR=X").trades_24x7());
    }

    #[test]
    fn stock_only_commands_reject_crypto_and_fx() {
        assert!(Market::In.require_stocks("sectors").is_ok());
        assert!(Market::Us.require_stocks("sectors").is_ok());
        let err = Market::Crypto
            .require_stocks("sectors")
            .unwrap_err()
            .to_string();
        assert!(err.contains("24/7"), "{err}");
        assert!(Market::Fx.require_stocks("intraday").is_err());
        assert!(Market::Crypto.sectors().is_empty());
    }

    #[test]
    fn universes_match_the_market() {
        assert!(Market::In.universe().iter().all(|s| s.ends_with(".NS")));
        assert!(Market::Fx.universe().iter().all(|s| s.ends_with("=X")));
        assert!(
            Market::Crypto
                .universe()
                .iter()
                .all(|s| AssetClass::of(s) == AssetClass::Crypto)
        );
    }

    #[test]
    fn session_notes_know_crypto_and_fx_hours() {
        use chrono::TimeZone;
        let saturday = chrono::Utc
            .with_ymd_and_hms(2026, 10, 24, 12, 0, 0)
            .unwrap();
        let tuesday = chrono::Utc
            .with_ymd_and_hms(2026, 10, 20, 12, 0, 0)
            .unwrap();
        let sunday_evening = chrono::Utc
            .with_ymd_and_hms(2026, 10, 25, 22, 0, 0)
            .unwrap();
        assert_eq!(
            session_note(AssetClass::Fx, saturday),
            Some(Msg::SessionFxWeekend)
        );
        assert_eq!(
            session_note(AssetClass::Fx, tuesday),
            Some(Msg::SessionFxOpen)
        );
        assert_eq!(
            session_note(AssetClass::Fx, sunday_evening),
            Some(Msg::SessionFxOpen)
        );
        assert_eq!(
            session_note(AssetClass::Crypto, saturday),
            Some(Msg::SessionCrypto)
        );
        assert_eq!(session_note(AssetClass::Stock, tuesday), None);
    }
}

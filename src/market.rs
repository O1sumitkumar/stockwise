use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum Market {
    /// US stock market (NYSE/NASDAQ)
    #[value(alias = "US")]
    Us,
    /// Indian stock market (NSE/BSE)
    #[value(alias = "IN", alias = "india")]
    In,
}

/// Normalise a user-typed symbol for Yahoo Finance.
///
/// In the Indian market a bare ticker gets the NSE suffix (`TCS` → `TCS.NS`).
/// Symbols that already say where they trade are passed through untouched:
/// an exchange suffix (`RELIANCE.BO`, `VOD.L`), an index (`^NSEI`), or a
/// currency/futures pair (`USDINR=X`, `BTC-USD`).
pub fn resolve_symbol(symbol: &str, market: Market) -> String {
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
    }
}

/// Hint shown when a symbol is not found, pointing at the most common cause:
/// a US ticker looked up in the default Indian market.
pub fn not_found_hint(market: Market) -> &'static str {
    match market {
        Market::In => {
            " (Indian market is the default — for US stocks add `-m us`; use `stockwise search <name>` to find a ticker)"
        }
        Market::Us => {
            " (use `stockwise search <name>` to find a ticker; Indian stocks need `-m in` or a .NS suffix)"
        }
    }
}

/// Get the currency symbol character for display
pub fn currency_symbol(currency: Option<&str>) -> &'static str {
    match currency {
        Some("INR") => "\u{20B9}",
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
    fn test_currency_symbol_gbp() {
        // Non-INR currencies all return $
        assert_eq!(currency_symbol(Some("GBP")), "$");
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
}

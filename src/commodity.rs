//! Gold and silver: international spot in ₹ per 10 g / per kg, COMEX
//! futures, Indian gold/silver ETFs and Sovereign Gold Bonds.
//!
//! Spot is the COMEX front-month future (Yahoo `GC=F`, `SI=F`, USD per troy
//! ounce) converted at `USDINR=X`. Yahoo has no free spot quote, and MCX
//! futures are not available from a public feed, so every row says where
//! its number comes from. Indian retail and MCX prices include import duty
//! and GST; these international prices do not.
//!
//! The functions here turn quotes into rows and never fetch anything, so
//! they are tested with fixtures.

use crate::api::Quote;
use serde::Serialize;

/// Grams in a troy ounce.
pub const TROY_OUNCE_GRAMS: f64 = 31.103_476_8;

pub const GOLD_FUTURE: &str = "GC=F";
pub const SILVER_FUTURE: &str = "SI=F";
pub const USDINR: &str = "USDINR=X";

/// COMEX contracts on Yahoo: (symbol, metal, contract).
pub const FUTURES: &[(&str, &str, &str)] = &[
    ("GC=F", "gold", "COMEX Gold (100 oz)"),
    ("MGC=F", "gold", "COMEX Micro Gold (10 oz)"),
    ("SI=F", "silver", "COMEX Silver (5,000 oz)"),
    ("SIL=F", "silver", "COMEX Micro Silver (1,000 oz)"),
];

/// NSE-listed gold and silver ETFs: (symbol, metal, name). Checked
/// against Yahoo Finance; prices are ₹ per unit.
pub const ETFS: &[(&str, &str, &str)] = &[
    ("GOLDBEES.NS", "gold", "Nippon India ETF Gold BeES"),
    ("SETFGOLD.NS", "gold", "SBI Gold ETF"),
    ("HDFCGOLD.NS", "gold", "HDFC Gold ETF"),
    ("GOLDIETF.NS", "gold", "ICICI Prudential Gold ETF"),
    ("GOLD1.NS", "gold", "Kotak Gold ETF"),
    ("GOLDCASE.NS", "gold", "Zerodha Gold ETF"),
    ("SILVERBEES.NS", "silver", "Nippon India Silver ETF"),
    ("SILVERIETF.NS", "silver", "ICICI Prudential Silver ETF"),
    ("HDFCSILVER.NS", "silver", "HDFC Silver ETF"),
    ("SILVER1.NS", "silver", "Kotak Silver ETF"),
];

/// Where spot prices come from, for notes and JSON.
pub const SPOT_SOURCE: &str = "COMEX front-month futures (Yahoo GC=F / SI=F) × USDINR=X";

/// The note every ₹ spot price carries.
pub const DUTY_NOTE: &str = "International price converted at the USD/INR rate: excludes Indian \
     import duty and GST, so jewellers' and MCX prices are higher.";

/// `usd_per_oz` in rupees for `grams` of metal.
pub fn inr_for_grams(usd_per_oz: f64, usdinr: f64, grams: f64) -> f64 {
    usd_per_oz * usdinr / TROY_OUNCE_GRAMS * grams
}

fn find<'a>(quotes: &'a [Quote], symbol: &str) -> Option<&'a Quote> {
    quotes.iter().find(|q| {
        q.symbol
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(symbol))
    })
}

/// Today's price and the previous close, from a quote.
fn price_and_previous(q: &Quote) -> Option<(f64, Option<f64>)> {
    let price = q.regular_market_price?;
    let previous = q
        .regular_market_previous_close
        .or_else(|| q.regular_market_change.map(|c| price - c))
        .filter(|p| *p > 0.0);
    Some((price, previous))
}

/// One line of `commodity spot` (field order = JSON key order).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpotRow {
    /// `gold` or `silver`.
    pub metal: String,
    /// `USD/oz`, `INR/g`, `INR/10g` or `INR/kg`.
    pub unit: String,
    pub price: Option<f64>,
    pub currency: String,
    pub change_percent: Option<f64>,
    pub source: String,
}

impl crate::output::CsvRow for SpotRow {
    const COLUMNS: &'static [&'static str] = &[
        "metal",
        "unit",
        "price",
        "currency",
        "change_percent",
        "source",
    ];
}

/// Spot rows for gold (USD/oz, ₹/g, ₹/10 g) and silver (USD/oz, ₹/g,
/// ₹/kg). A missing quote gives a row without a price, never a 0.
pub fn spot_rows(quotes: &[Quote]) -> Vec<SpotRow> {
    let fx = find(quotes, USDINR).and_then(price_and_previous);
    let mut rows = Vec::new();
    for (metal, future, inr_units) in [
        (
            "gold",
            GOLD_FUTURE,
            &[("INR/g", 1.0), ("INR/10g", 10.0)][..],
        ),
        (
            "silver",
            SILVER_FUTURE,
            &[("INR/g", 1.0), ("INR/kg", 1000.0)][..],
        ),
    ] {
        let q = find(quotes, future);
        let usd = q.and_then(price_and_previous);
        rows.push(SpotRow {
            metal: metal.into(),
            unit: "USD/oz".into(),
            price: usd.map(|(p, _)| p),
            currency: "USD".into(),
            change_percent: q.and_then(|q| q.regular_market_change_percent),
            source: format!("COMEX {future}"),
        });
        for (unit, grams) in inr_units {
            let (price, change) = match (usd, fx) {
                (Some((p, prev_p)), Some((r, prev_r))) => {
                    let now = inr_for_grams(p, r, *grams);
                    let change = prev_p
                        .zip(prev_r)
                        .map(|(pp, pr)| (now / inr_for_grams(pp, pr, *grams) - 1.0) * 100.0);
                    (Some(now), change)
                }
                _ => (None, None),
            };
            rows.push(SpotRow {
                metal: metal.into(),
                unit: (*unit).into(),
                price,
                currency: "INR".into(),
                change_percent: change,
                source: format!("COMEX {future} × {USDINR}"),
            });
        }
    }
    rows
}

/// Gold in ₹ per gram (international, before duty), if both quotes exist.
pub fn gold_inr_per_gram(quotes: &[Quote]) -> Option<f64> {
    let usd = find(quotes, GOLD_FUTURE)?.regular_market_price?;
    let fx = find(quotes, USDINR)?.regular_market_price?;
    Some(inr_for_grams(usd, fx, 1.0))
}

/// One instrument in `commodity futures` / `commodity etfs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InstrumentRow {
    pub symbol: String,
    pub metal: String,
    pub name: String,
    /// Yahoo's short name: the contract month for futures ("Gold Dec 26").
    pub contract: Option<String>,
    pub price: Option<f64>,
    pub change: Option<f64>,
    pub change_percent: Option<f64>,
    pub volume: Option<u64>,
    pub currency: Option<String>,
}

impl crate::output::CsvRow for InstrumentRow {
    const COLUMNS: &'static [&'static str] = &[
        "symbol",
        "metal",
        "name",
        "contract",
        "price",
        "change",
        "change_percent",
        "volume",
        "currency",
    ];
}

/// Rows for a fixed list, in list order; instruments without a quote keep
/// their row with empty prices.
pub fn instrument_rows(list: &[(&str, &str, &str)], quotes: &[Quote]) -> Vec<InstrumentRow> {
    list.iter()
        .map(|(symbol, metal, name)| {
            let q = find(quotes, symbol);
            InstrumentRow {
                symbol: symbol.to_string(),
                metal: metal.to_string(),
                name: name.to_string(),
                contract: q.and_then(|q| q.short_name.clone()),
                price: q.and_then(|q| q.regular_market_price),
                change: q.and_then(|q| q.regular_market_change),
                change_percent: q.and_then(|q| q.regular_market_change_percent),
                volume: q.and_then(|q| q.regular_market_volume),
                currency: q.and_then(|q| q.currency.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn quote(symbol: &str, price: f64, previous: f64, currency: &str) -> Quote {
        Quote {
            symbol: Some(symbol.to_string()),
            short_name: Some(format!("{symbol} name")),
            regular_market_price: Some(price),
            regular_market_previous_close: Some(previous),
            regular_market_change: Some(price - previous),
            regular_market_change_percent: Some((price / previous - 1.0) * 100.0),
            currency: Some(currency.to_string()),
            ..Default::default()
        }
    }

    pub fn fixture() -> Vec<Quote> {
        vec![
            quote("GC=F", 4000.0, 4050.0, "USD"),
            quote("SI=F", 50.0, 50.0, "USD"),
            quote("USDINR=X", 95.0, 94.0, "INR"),
        ]
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn ounces_to_grams_in_rupees() {
        // $3110.34768/oz at ₹100/$ = ₹10,000 a gram.
        assert!(close(inr_for_grams(3_110.347_68, 100.0, 1.0), 10_000.0));
        assert!(close(inr_for_grams(3_110.347_68, 100.0, 10.0), 100_000.0));
        assert!(close(inr_for_grams(31.103_476_8, 100.0, 1000.0), 100_000.0));
    }

    #[test]
    fn spot_rows_convert_and_combine_changes() {
        let rows = spot_rows(&fixture());
        let units: Vec<_> = rows
            .iter()
            .map(|r| format!("{}:{}", r.metal, r.unit))
            .collect();
        assert_eq!(
            units,
            [
                "gold:USD/oz",
                "gold:INR/g",
                "gold:INR/10g",
                "silver:USD/oz",
                "silver:INR/g",
                "silver:INR/kg"
            ]
        );
        let ten_g = &rows[2];
        let want = 4000.0 * 95.0 / TROY_OUNCE_GRAMS * 10.0;
        assert!(close(ten_g.price.unwrap(), want));
        assert_eq!(ten_g.currency, "INR");
        // Gold −1.2346% in USD, rupee +1.0638% weaker: (4000×95)/(4050×94) − 1.
        let change = ten_g.change_percent.unwrap();
        assert!(close(
            change,
            (4000.0 * 95.0 / (4050.0 * 94.0) - 1.0) * 100.0
        ));
        // Silver flat in USD: the ₹ change is the currency move alone.
        let kg = &rows[5];
        assert!(close(
            kg.change_percent.unwrap(),
            (95.0 / 94.0 - 1.0) * 100.0
        ));
        assert!(close(
            gold_inr_per_gram(&fixture()).unwrap(),
            4000.0 * 95.0 / TROY_OUNCE_GRAMS
        ));
    }

    #[test]
    fn missing_quotes_never_become_zero() {
        // No exchange rate: USD rows keep their price, ₹ rows have none.
        let quotes = vec![quote("GC=F", 4000.0, 4050.0, "USD")];
        let rows = spot_rows(&quotes);
        assert_eq!(rows[0].price, Some(4000.0));
        assert!(rows[1..3].iter().all(|r| r.price.is_none()));
        assert!(rows[3..].iter().all(|r| r.price.is_none()));
        assert_eq!(gold_inr_per_gram(&quotes), None);
        let etfs = instrument_rows(ETFS, &quotes);
        assert_eq!(etfs.len(), ETFS.len());
        assert!(etfs.iter().all(|r| r.price.is_none()));
    }

    #[test]
    fn instrument_rows_follow_the_list() {
        let quotes = vec![quote("SI=F", 50.0, 49.0, "USD")];
        let rows = instrument_rows(FUTURES, &quotes);
        assert_eq!(rows[2].symbol, "SI=F");
        assert_eq!(rows[2].price, Some(50.0));
        assert_eq!(rows[2].contract.as_deref(), Some("SI=F name"));
        assert_eq!(rows[0].price, None);
        assert!(ETFS.iter().all(|(s, ..)| s.ends_with(".NS")));
    }
}

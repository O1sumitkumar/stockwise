//! Holdings imports beyond the Kite / INDmoney / generic CSV parsers in
//! `holdings.rs`: Groww and Upstox holdings exports, and CAMS / KFintech
//! consolidated account statements (CAS) in casparser's JSON or CSV form.
//!
//! Parsers are pure (text in, rows out) so they can be tested with fixtures;
//! `import_rows` applies the rows with the same snapshot semantics as the
//! other imports.

use super::holdings::{find_col, parse_csv_header, parse_csv_row};
use super::*;

/// One holding read from an import file.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ImportRow {
    /// Portfolio symbol (`RELIANCE.NS`, `MF:118955`), or empty when only an
    /// ISIN is known and it still has to be looked up.
    pub symbol: String,
    pub isin: Option<String>,
    /// Display name (company or scheme), when the file has one.
    pub name: Option<String>,
    pub qty: f64,
    /// Average cost per unit. `None` when the file has no cost (a demat or
    /// summary CAS): an existing holding then gets its quantity updated and
    /// keeps its cost; a new one is skipped.
    pub avg_cost: Option<f64>,
    /// "shares" or "units".
    pub unit: &'static str,
}

/// A row that could not be imported, and why.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Skipped {
    pub what: String,
    pub why: String,
}

#[derive(Debug, Default)]
pub(super) struct Parsed {
    pub rows: Vec<ImportRow>,
    pub skipped: Vec<Skipped>,
}

/// Parse a number as exported by Indian brokers: `1,234.50`, `₹ 1,234.50`,
/// `"(12.5)"`, `-`.
fn number(s: &str) -> Option<f64> {
    let t: String = s
        .trim()
        .trim_matches('"')
        .chars()
        .filter(|c| !matches!(c, ',' | '₹' | ' ' | '\u{a0}'))
        .collect();
    let (neg, t) = match t.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        Some(inner) => (true, inner.to_string()),
        None => (false, t),
    };
    let v: f64 = t.parse().ok()?;
    v.is_finite().then_some(if neg { -v } else { v })
}

fn is_isin(s: &str) -> bool {
    let s = s.trim();
    s.len() == 12
        && s[..2].chars().all(|c| c.is_ascii_uppercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Find the header row: exports often start with a few lines of account
/// details (name, client code, statement date) before the table.
fn find_header(lines: &[&str], groups: &[&[&str]]) -> Option<(usize, Vec<String>)> {
    lines.iter().take(40).enumerate().find_map(|(i, line)| {
        let cols = parse_csv_header(line);
        groups
            .iter()
            .all(|names| find_col(&cols, names).is_some())
            .then_some((i, cols))
    })
}

/// Column layout of a stock holdings export.
struct StockLayout {
    broker: &'static str,
    /// Expected columns, for the error message.
    expected: &'static str,
    symbol: &'static [&'static str],
    isin: &'static [&'static str],
    name: &'static [&'static str],
    qty: &'static [&'static str],
    avg: &'static [&'static str],
}

/// Groww → Stocks → Holdings → Download, then save the sheet as CSV.
const GROWW: StockLayout = StockLayout {
    broker: "Groww",
    expected: "Stock Name, ISIN, Quantity, Average buy price",
    symbol: &["symbol", "nse symbol", "trading symbol"],
    isin: &["isin"],
    name: &["stock name", "company name", "name"],
    qty: &["quantity", "qty", "shares"],
    avg: &[
        "average buy price",
        "avg buy price",
        "average price",
        "avg. buy price",
        "avg price",
        "buy price",
    ],
};

/// Upstox → Portfolio → Holdings → Download.
const UPSTOX: StockLayout = StockLayout {
    broker: "Upstox",
    expected: "Symbol / Scrip Name / ISIN, Qty, Avg. Price",
    symbol: &["symbol", "trading symbol", "tradingsymbol", "scrip code"],
    isin: &["isin"],
    name: &[
        "scrip name",
        "company name",
        "instrument",
        "stock name",
        "name",
    ],
    qty: &["quantity", "qty", "net qty", "shares"],
    avg: &[
        "avg. price",
        "avg price",
        "average price",
        "avg. cost",
        "avg cost",
        "average cost",
        "buy avg",
    ],
};

fn parse_stock_export(content: &str, layout: &StockLayout, market: Market) -> Result<Parsed> {
    let lines: Vec<&str> = content
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}'))
        .collect();
    let any_symbol: Vec<&str> = layout
        .symbol
        .iter()
        .chain(layout.isin)
        .chain(layout.name)
        .copied()
        .collect();
    let Some((h, cols)) = find_header(&lines, &[&any_symbol, layout.qty, layout.avg]) else {
        bail!(
            "Could not detect {} columns. Expected: {}. If you downloaded an .xlsx file, open it \
             and save it as CSV first.",
            layout.broker,
            layout.expected
        );
    };
    let sym_col = find_col(&cols, layout.symbol);
    let isin_col = find_col(&cols, layout.isin);
    let name_col = find_col(&cols, layout.name);
    let qty_col = find_col(&cols, layout.qty).expect("checked by find_header");
    let avg_col = find_col(&cols, layout.avg).expect("checked by find_header");

    let mut out = Parsed::default();
    for line in &lines[h + 1..] {
        let fields = parse_csv_row(line);
        let get = |c: Option<usize>| {
            c.and_then(|i| fields.get(i))
                .map(|f| f.trim().trim_matches('"').trim().to_string())
                .filter(|f| !f.is_empty() && f != "-")
        };
        let name = get(name_col);
        let isin = get(isin_col).filter(|s| is_isin(s));
        // A name column that holds a ticker (no spaces) is a symbol too.
        let symbol = get(sym_col).or_else(|| {
            name.clone()
                .filter(|n| isin.is_none() && !n.contains(' ') && n.len() <= 20)
        });
        if symbol.is_none() && isin.is_none() && name.is_none() {
            continue; // blank or footer line
        }
        let label = name
            .clone()
            .or_else(|| symbol.clone())
            .or_else(|| isin.clone())
            .unwrap_or_default();
        // Totals, and Groww's "NA" filler rows.
        if label.to_lowercase().starts_with("total") || label.eq_ignore_ascii_case("NA") {
            continue;
        }
        let qty = get(Some(qty_col)).and_then(|q| number(&q)).unwrap_or(0.0);
        let avg = get(Some(avg_col)).and_then(|a| number(&a)).unwrap_or(0.0);
        if qty <= 0.0 || avg <= 0.0 {
            out.skipped.push(Skipped {
                what: label,
                why: "no positive quantity and average price".into(),
            });
            continue;
        }
        if symbol.is_none() && isin.is_none() {
            out.skipped.push(Skipped {
                what: label,
                why: "no symbol or ISIN — add it with `portfolio add`".into(),
            });
            continue;
        }
        out.rows.push(ImportRow {
            symbol: symbol
                .map(|s| market::resolve_symbol(&crate::brokers::plain_symbol(&s), market))
                .unwrap_or_default(),
            isin,
            name,
            qty,
            avg_cost: Some(avg),
            unit: "shares",
        });
    }
    Ok(out)
}

pub(super) fn parse_groww(content: &str, market: Market) -> Result<Parsed> {
    parse_stock_export(content, &GROWW, market)
}

pub(super) fn parse_upstox(content: &str, market: Market) -> Result<Parsed> {
    parse_stock_export(content, &UPSTOX, market)
}

// ── CAS (CAMS / KFintech) ──

/// Portfolio symbol for a mutual-fund scheme: `MF:<AMFI code>`, else
/// `MF:<ISIN>`. Never a Yahoo ticker, so it can't be confused with a stock.
fn mf_symbol(amfi: Option<&str>, isin: Option<&str>) -> Option<String> {
    fn pick(s: Option<&str>) -> Option<&str> {
        s.map(str::trim).filter(|s| !s.is_empty())
    }
    pick(amfi)
        .or_else(|| pick(isin))
        .map(|id| format!("MF:{}", id.to_uppercase()))
}

/// Running average-cost book for one scheme: purchases add units and cost,
/// redemptions remove units at the current average cost.
#[derive(Debug, Default, Clone, Copy)]
struct CostBook {
    units: f64,
    cost: f64,
}

impl CostBook {
    /// Apply one transaction. `units` is signed (negative for redemptions
    /// and switch-outs); `amount` is the cash value. Rows without units
    /// (stamp duty, STT, TDS, dividend payouts) only adjust cost when they are
    /// stamp duty, which is part of the purchase cost.
    fn apply(&mut self, kind: &str, units: Option<f64>, amount: Option<f64>, nav: Option<f64>) {
        match units {
            Some(u) if u > 0.0 => {
                let value = amount
                    .map(f64::abs)
                    .filter(|a| *a > 0.0)
                    .or_else(|| nav.map(|n| n * u))
                    .unwrap_or(0.0);
                self.units += u;
                self.cost += value;
            }
            Some(u) if u < 0.0 => {
                let u = (-u).min(self.units);
                if self.units > 0.0 {
                    self.cost -= self.cost / self.units * u;
                }
                self.units -= u;
                if self.units < 1e-6 {
                    self.units = 0.0;
                    self.cost = 0.0;
                }
            }
            _ => {
                if kind.to_ascii_uppercase().contains("STAMP")
                    && let Some(a) = amount
                {
                    self.cost += a.abs();
                }
            }
        }
    }

    fn avg(&self) -> Option<f64> {
        (self.units > 0.0 && self.cost > 0.0).then(|| self.cost / self.units)
    }

    /// Average cost, but only if the transactions explain the whole closing
    /// balance. A statement that starts with units bought before its period
    /// has no cost for them, and averaging only the later purchases would be
    /// wrong.
    fn avg_for(&self, closing_units: f64) -> Option<f64> {
        let tolerance = 0.002_f64.max(closing_units * 1e-4);
        ((self.units - closing_units).abs() <= tolerance)
            .then(|| self.avg())
            .flatten()
    }
}

fn json_num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => number(s),
        _ => None,
    }
}

fn json_str(v: Option<&serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// casparser JSON (`casparser CAS.pdf -p PASSWORD -o cas.json`).
pub(super) fn parse_cas_json(content: &str) -> Result<Parsed> {
    let doc: serde_json::Value =
        serde_json::from_str(content).context("Not valid JSON — is this casparser output?")?;
    if let Some(accounts) = doc.get("accounts").and_then(|a| a.as_array()) {
        return Ok(parse_demat_accounts(accounts));
    }
    let Some(folios) = doc.get("folios").and_then(|f| f.as_array()) else {
        bail!(
            "No `folios` or `accounts` in this JSON. Expected casparser output for a CAMS / \
             KFintech / NSDL / CDSL CAS (`casparser CAS.pdf -o cas.json`)."
        );
    };
    let mut out = Parsed::default();
    for folio in folios {
        let folio_no = json_str(folio.get("folio")).unwrap_or_default();
        let schemes = folio
            .get("schemes")
            .and_then(|s| s.as_array())
            .map(Vec::as_slice)
            .unwrap_or_default();
        for scheme in schemes {
            let name = json_str(scheme.get("scheme")).unwrap_or_else(|| "Unknown scheme".into());
            let what = format!("{name} (folio {folio_no})");
            let isin = json_str(scheme.get("isin"));
            let amfi = json_str(scheme.get("amfi"));
            let units = json_num(scheme.get("close"))
                .or_else(|| json_num(scheme.get("close_calculated")))
                .unwrap_or(0.0);
            if units <= 1e-6 {
                continue; // fully redeemed
            }
            let Some(symbol) = mf_symbol(amfi.as_deref(), isin.as_deref()) else {
                out.skipped.push(Skipped {
                    what,
                    why: "no AMFI code or ISIN".into(),
                });
                continue;
            };
            let valuation = scheme.get("valuation");
            let stated_cost = json_num(valuation.and_then(|v| v.get("cost"))).filter(|c| *c > 0.0);
            let avg = match stated_cost {
                Some(c) => Some(c / units),
                None => {
                    let mut book = CostBook::default();
                    for t in scheme
                        .get("transactions")
                        .and_then(|t| t.as_array())
                        .map(Vec::as_slice)
                        .unwrap_or_default()
                    {
                        book.apply(
                            &json_str(t.get("type")).unwrap_or_default(),
                            json_num(t.get("units")),
                            json_num(t.get("amount")),
                            json_num(t.get("nav")),
                        );
                    }
                    book.avg_for(units)
                }
            };
            // No complete cost: update the quantity of a fund already tracked.
            out.rows.push(ImportRow {
                symbol,
                isin,
                name: Some(name),
                qty: units,
                avg_cost: avg,
                unit: "units",
            });
        }
    }
    Ok(out)
}

/// NSDL / CDSL (demat) CAS from casparser: `accounts[].equities[]` and
/// `accounts[].mutual_funds[]`. Equities carry no purchase cost, so they
/// only update quantities of holdings you already track.
fn parse_demat_accounts(accounts: &[serde_json::Value]) -> Parsed {
    let mut out = Parsed::default();
    let items = |a: &serde_json::Value, key: &str| {
        a.get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    for account in accounts {
        for e in items(account, "equities") {
            let isin = json_str(e.get("isin"));
            let name = json_str(e.get("name"));
            let qty = json_num(e.get("num_shares")).unwrap_or(0.0);
            if qty <= 0.0 {
                continue;
            }
            // casparser back-fills the NSE ticker when it knows it.
            let symbol = match (json_str(e.get("symbol")), json_str(e.get("exchange"))) {
                (Some(s), Some(x)) if x.eq_ignore_ascii_case("BSE") => {
                    format!("{}.BO", s.to_uppercase())
                }
                (Some(s), _) => format!("{}.NS", s.to_uppercase()),
                (None, _) => String::new(),
            };
            if symbol.is_empty() && isin.is_none() {
                continue;
            }
            out.rows.push(ImportRow {
                symbol,
                isin,
                name,
                qty,
                avg_cost: None,
                unit: "shares",
            });
        }
        for mf in items(account, "mutual_funds") {
            let isin = json_str(mf.get("isin"));
            let amfi = json_str(mf.get("amfi"));
            let units = json_num(mf.get("balance")).unwrap_or(0.0);
            let Some(symbol) = mf_symbol(amfi.as_deref(), isin.as_deref()) else {
                continue;
            };
            if units <= 1e-6 {
                continue;
            }
            let avg = json_num(mf.get("avg_cost"))
                .filter(|c| *c > 0.0)
                .or_else(|| {
                    json_num(mf.get("total_cost"))
                        .filter(|c| *c > 0.0)
                        .map(|c| c / units)
                });
            out.rows.push(ImportRow {
                symbol,
                isin,
                name: json_str(mf.get("name")),
                qty: units,
                avg_cost: avg,
                unit: "units",
            });
        }
    }
    out
}

/// casparser summary CSV (`-s`): one row per scheme with closing units and
/// no cost.
fn parse_cas_summary_csv(lines: &[&str], h: usize, cols: &[String]) -> Parsed {
    let col = |name: &str| cols.iter().position(|c| c == name);
    let (scheme_c, isin_c, amfi_c, close_c, folio_c) = (
        col("scheme"),
        col("isin"),
        col("amfi"),
        col("close"),
        col("folio"),
    );
    let mut out = Parsed::default();
    for line in &lines[h + 1..] {
        let f = parse_csv_row(line);
        let get = |c: Option<usize>| {
            c.and_then(|i| f.get(i))
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(scheme) = get(scheme_c) else {
            continue;
        };
        let units = get(close_c).and_then(|v| number(&v)).unwrap_or(0.0);
        if units <= 1e-6 {
            continue;
        }
        let (isin, amfi) = (get(isin_c), get(amfi_c));
        let Some(symbol) = mf_symbol(amfi.as_deref(), isin.as_deref()) else {
            out.skipped.push(Skipped {
                what: format!("{scheme} (folio {})", get(folio_c).unwrap_or_default()),
                why: "no AMFI code or ISIN".into(),
            });
            continue;
        };
        out.rows.push(ImportRow {
            symbol,
            isin,
            name: Some(scheme),
            qty: units,
            avg_cost: None,
            unit: "units",
        });
    }
    out
}

/// casparser CSV (`casparser CAS.pdf -p PASSWORD -o cas.csv`): one row per
/// transaction; holdings are rebuilt per folio and scheme. The summary
/// layout (`-s`) is accepted too.
pub(super) fn parse_cas_csv(content: &str) -> Result<Parsed> {
    let lines: Vec<&str> = content
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}'))
        .collect();
    if let Some((h, cols)) = find_header(&lines, &[&["scheme"], &["close"], &["transactions"]]) {
        return Ok(parse_cas_summary_csv(&lines, h, &cols));
    }
    let Some((h, cols)) = find_header(
        &lines,
        &[&["scheme"], &["units"], &["balance"], &["amount"]],
    ) else {
        bail!(
            "Could not detect casparser CSV columns (scheme, isin, amfi, units, balance, amount). \
             Export with `casparser CAS.pdf -o cas.csv`, or use JSON."
        );
    };
    let col = |name: &str| cols.iter().position(|c| c == name);
    let (folio_c, scheme_c, isin_c, amfi_c) =
        (col("folio"), col("scheme"), col("isin"), col("amfi"));
    let (units_c, balance_c, amount_c, nav_c, type_c) = (
        col("units"),
        col("balance"),
        col("amount"),
        col("nav"),
        col("type"),
    );

    // Keyed by folio + scheme, in first-seen order.
    let mut order: Vec<(String, String)> = Vec::new();
    let mut books: std::collections::HashMap<
        (String, String),
        (CostBook, Option<f64>, Option<String>, Option<String>),
    > = Default::default();
    for line in &lines[h + 1..] {
        let f = parse_csv_row(line);
        let get = |c: Option<usize>| {
            c.and_then(|i| f.get(i))
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(scheme) = get(scheme_c) else {
            continue;
        };
        let key = (get(folio_c).unwrap_or_default(), scheme);
        let entry = books.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            (CostBook::default(), None, get(isin_c), get(amfi_c))
        });
        let num = |c| get(c).and_then(|v| number(&v));
        entry.0.apply(
            &get(type_c).unwrap_or_default(),
            num(units_c),
            num(amount_c),
            num(nav_c),
        );
        if let Some(b) = num(balance_c) {
            entry.1 = Some(b);
        }
    }

    let mut out = Parsed::default();
    for key in order {
        let (book, balance, isin, amfi) = &books[&key];
        let what = format!("{} (folio {})", key.1, key.0);
        let units = balance.unwrap_or(book.units);
        if units <= 1e-6 {
            continue;
        }
        let Some(symbol) = mf_symbol(amfi.as_deref(), isin.as_deref()) else {
            out.skipped.push(Skipped {
                what,
                why: "no AMFI code or ISIN".into(),
            });
            continue;
        };
        // Opening units of unknown cost (a period statement): quantity only.
        out.rows.push(ImportRow {
            symbol,
            isin: isin.clone(),
            name: Some(key.1.clone()),
            qty: units,
            avg_cost: book.avg_for(units),
            unit: "units",
        });
    }
    Ok(out)
}

/// casparser JSON or CSV, by content.
pub(super) fn parse_cas(content: &str) -> Result<Parsed> {
    let trimmed = content.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with("%PDF") {
        bail!(
            "This is the CAS PDF itself. StockWise reads casparser's output: install it \
             (`pip install casparser`), run `casparser CAS.pdf -p <password> -o cas.json`, \
             then `stockwise import cas cas.json`."
        );
    }
    if trimmed.starts_with('{') {
        parse_cas_json(trimmed)
    } else {
        parse_cas_csv(trimmed)
    }
}

/// Look up the NSE (else BSE) ticker for an ISIN via Yahoo search. Only a
/// single unambiguous equity result is accepted. `Ok(None)` means the search
/// worked and found no listing; a failed search is an error, so a network
/// blip never changes what a row is imported as.
async fn ticker_for_isin(yahoo: &YahooClient, isin: &str) -> Result<Option<String>> {
    let hits = yahoo.search(isin).await?;
    let pick = |suffix: &str| {
        let mut found = hits.iter().filter_map(|h| {
            let s = h.symbol.as_deref()?;
            let equity = h
                .quote_type
                .as_deref()
                .is_none_or(|t| t.eq_ignore_ascii_case("EQUITY") || t.eq_ignore_ascii_case("ETF"));
            (equity && s.ends_with(suffix)).then(|| s.to_uppercase())
        });
        let first = found.next()?;
        found.all(|s| s == first).then_some(first)
    };
    Ok(pick(".NS").or_else(|| pick(".BO")))
}

/// Apply rows with snapshot semantics: the first row for a symbol replaces
/// the holding (so re-importing never doubles it); further rows for the
/// same symbol in this file are extra lots and are averaged in. Rows
/// without a cost only update holdings that already exist. Returns the rows
/// applied, with the cost used, and the rows skipped.
pub(super) fn apply_rows(
    portfolio: &mut Portfolio,
    rows: &[ImportRow],
) -> (Vec<(ImportRow, f64)>, Vec<Skipped>) {
    let mut seen = std::collections::HashSet::new();
    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    for r in rows {
        let sym = r.symbol.to_uppercase();
        let existing = portfolio
            .holdings
            .iter()
            .find(|h| h.symbol == sym)
            .map(|h| h.avg_cost);
        let Some(avg) = r.avg_cost.or(existing) else {
            skipped.push(Skipped {
                what: r.name.clone().unwrap_or_else(|| sym.clone()),
                why: format!(
                    "the statement has no complete purchase cost (units bought before its period, \
                     or a summary/demat statement) — add it once with `portfolio add {sym} <qty> <cost>`; \
                     later imports keep its quantity in sync"
                ),
            });
            continue;
        };
        if seen.insert(sym.clone()) {
            portfolio.set(&sym, r.qty, avg);
        } else {
            portfolio.add(&sym, r.qty, avg);
        }
        applied.push((r.clone(), avg));
    }
    (applied, skipped)
}

/// Resolve ISIN-only rows to tickers, apply everything, and report.
pub(super) async fn import_rows(source: &str, parsed: Parsed) -> Result<()> {
    let Parsed {
        mut rows,
        mut skipped,
    } = parsed;

    // Exports that only carry ISINs (Groww) need a ticker lookup.
    if rows.iter().any(|r| r.symbol.is_empty()) {
        let yahoo = YahooClient::new().await;
        for r in rows.iter_mut().filter(|r| r.symbol.is_empty()) {
            let isin = r.isin.clone().unwrap_or_default();
            let what = r.name.clone().unwrap_or(isin.clone());
            let lookup = match &yahoo {
                Ok(y) => ticker_for_isin(y, &isin).await,
                Err(e) => Err(anyhow::anyhow!("{e:#}")),
            };
            match lookup {
                Ok(Some(t)) => r.symbol = t,
                // INF… ISINs with no exchange listing are mutual funds.
                Ok(None) if isin.starts_with("INF") => {
                    r.symbol = mf_symbol(None, Some(&isin)).unwrap_or_default();
                    r.unit = "units";
                }
                Ok(None) => skipped.push(Skipped {
                    what,
                    why: format!(
                        "could not find one NSE/BSE ticker for ISIN {isin} — add it with `portfolio add`"
                    ),
                }),
                Err(e) => skipped.push(Skipped {
                    what,
                    why: format!("ticker lookup failed ({e:#}) — run the import again later"),
                }),
            }
        }
        rows.retain(|r| !r.symbol.is_empty());
    }

    let mut portfolio = Portfolio::load()?;
    let (applied, not_applied) = apply_rows(&mut portfolio, &rows);
    skipped.extend(not_applied);
    for (r, avg) in &applied {
        let name = r
            .name
            .as_deref()
            .filter(|n| !r.symbol.contains(&n.to_uppercase()))
            .map(|n| format!(" {}", n.dimmed()))
            .unwrap_or_default();
        let kept = if r.avg_cost.is_none() {
            " (quantity only; cost kept)".dimmed().to_string()
        } else {
            String::new()
        };
        println!(
            "  {} {}{} — {} {} @ ₹{:.2}{}",
            "✓".green(),
            r.symbol.cyan(),
            name,
            r.qty,
            r.unit,
            avg,
            kept
        );
    }
    for s in &skipped {
        println!("  {} {} — {}", "!".yellow(), s.what, s.why.dimmed());
    }

    if applied.is_empty() {
        println!(
            "\n  {} No holdings imported from the {} file. Check the format.\n",
            "✗".red(),
            source
        );
        return Ok(());
    }
    portfolio.save()?;
    println!(
        "\n  {} Imported {} holdings into portfolio.",
        "✓".green().bold(),
        applied.len()
    );
    if applied.iter().any(|(r, _)| r.symbol.starts_with("MF:")) {
        println!(
            "  {}",
            "Mutual funds are stored as MF:<AMFI code or ISIN>; StockWise can't price them live yet."
                .dimmed()
        );
    }
    println!(
        "  Run {} to see your portfolio.\n",
        "stockwise portfolio show".cyan()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_in_indian_exports() {
        assert_eq!(number("1,234.50"), Some(1234.5));
        assert_eq!(number("₹ 2,00,000"), Some(200000.0));
        assert_eq!(number("\"12\""), Some(12.0));
        assert_eq!(number("(5.5)"), Some(-5.5));
        assert_eq!(number("-"), None);
        assert_eq!(number(""), None);
    }

    #[test]
    fn isin_shape() {
        assert!(is_isin("INE002A01018"));
        assert!(is_isin("INF179K01UT0"));
        assert!(!is_isin("RELIANCE"));
        assert!(!is_isin("ine002a01018"));
    }

    #[test]
    fn cost_book_average_cost_method() {
        let mut b = CostBook::default();
        b.apply("PURCHASE", Some(10.0), Some(1000.0), Some(100.0));
        b.apply("PURCHASE_SIP", Some(10.0), Some(2000.0), Some(200.0));
        b.apply("STAMP_DUTY_TAX", None, Some(0.15), None);
        // avg = 3000.15 / 20; selling 5 keeps the average.
        b.apply("REDEMPTION", Some(-5.0), Some(-1250.0), Some(250.0));
        assert!((b.units - 15.0).abs() < 1e-9);
        assert!((b.avg().unwrap() - 3000.15 / 20.0).abs() < 1e-9);
        b.apply("REDEMPTION", Some(-15.0), Some(-4000.0), Some(266.0));
        assert_eq!(b.avg(), None);
    }

    #[test]
    fn mf_symbols_prefer_amfi_code() {
        assert_eq!(
            mf_symbol(Some("118955"), Some("INF179K01UT0")).as_deref(),
            Some("MF:118955")
        );
        assert_eq!(
            mf_symbol(None, Some("inf179k01ut0")).as_deref(),
            Some("MF:INF179K01UT0")
        );
        assert_eq!(mf_symbol(Some(" "), None), None);
    }

    fn row(symbol: &str, qty: f64, avg: Option<f64>) -> ImportRow {
        ImportRow {
            symbol: symbol.into(),
            isin: None,
            name: None,
            qty,
            avg_cost: avg,
            unit: "shares",
        }
    }

    #[test]
    fn groww_export_skips_preamble_and_na_rows() {
        let p = parse_groww(include_str!("fixtures/groww_holdings.csv"), Market::In).unwrap();
        assert_eq!(p.rows.len(), 2, "{:?}", p.rows);
        assert!(p.skipped.is_empty(), "{:?}", p.skipped);
        let asian = &p.rows[0];
        // Groww has no ticker column: the ISIN is looked up later.
        assert_eq!(asian.symbol, "");
        assert_eq!(asian.isin.as_deref(), Some("INE021A01026"));
        assert_eq!((asian.qty, asian.avg_cost), (100.0, Some(2410.04)));
        assert_eq!(p.rows[1].isin.as_deref(), Some("INF204KB14I2"));
    }

    #[test]
    fn upstox_export_reads_symbols_and_indian_number_format() {
        let p = parse_upstox(include_str!("fixtures/upstox_holdings.csv"), Market::In).unwrap();
        assert_eq!(p.rows.len(), 2, "{:?}", p.rows);
        assert_eq!(p.rows[0].symbol, "YESBANK.NS");
        assert_eq!(p.rows[1].symbol, "BAJAJ-AUTO.NS");
        assert_eq!((p.rows[1].qty, p.rows[1].avg_cost), (1000.0, Some(8912.4)));
    }

    #[test]
    fn unknown_layout_is_a_clear_error() {
        let err = parse_groww("a,b,c\n1,2,3\n", Market::In)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Groww") && err.contains("CSV"), "{err}");
    }

    #[test]
    fn cas_json_uses_stated_cost_then_transactions() {
        let p = parse_cas(include_str!("fixtures/cas_cams.json")).unwrap();
        assert_eq!(p.rows.len(), 2, "{:?}", p.rows);
        let hdfc = &p.rows[0];
        assert_eq!(hdfc.symbol, "MF:118834");
        assert_eq!(hdfc.qty, 23.412);
        assert!((hdfc.avg_cost.unwrap() - 3499.75 / 23.412).abs() < 1e-9);
        // No valuation.cost and numbers instead of strings (older casparser).
        let ppfas = &p.rows[1];
        assert_eq!(ppfas.symbol, "MF:122639");
        assert!((ppfas.avg_cost.unwrap() - 6000.0 / 100.5).abs() < 1e-9);
        // The fully redeemed liquid fund is not a holding.
        assert!(p.rows.iter().all(|r| r.symbol != "MF:119091"));
    }

    #[test]
    fn cas_csv_rebuilds_holdings_per_folio() {
        let p = parse_cas(include_str!("fixtures/cas_cams.csv")).unwrap();
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.rows[0].qty, 23.412);
        let expected = 5000.0 * (23.412 / 33.412);
        assert!((p.rows[0].avg_cost.unwrap() * 23.412 - expected).abs() < 1e-6);
        assert_eq!(
            (p.rows[1].qty, p.rows[1].avg_cost),
            (6.5, Some(1000.0 / 6.5))
        );

        // Two folios of one scheme add up; re-importing never doubles them.
        let mut pf = Portfolio::default();
        apply_rows(&mut pf, &p.rows);
        apply_rows(&mut pf, &p.rows);
        assert_eq!(pf.holdings.len(), 1);
        assert!((pf.holdings[0].shares - 29.912).abs() < 1e-9);
    }

    #[test]
    fn units_bought_before_the_statement_period_have_no_cost() {
        // 100 units carried in, 10 bought in the period: averaging only the
        // 10 would misstate the cost of all 110.
        let json = r#"{"folios":[{"folio":"1","schemes":[{"scheme":"X Fund","amfi":"1","isin":"INF000000001",
            "open":"100","close":"110","close_calculated":"110","valuation":{"date":"2025-03-31","nav":"100","value":"11000"},
            "transactions":[{"date":"2024-05-01","description":"Purchase","amount":"1000","units":"10","nav":"100","balance":"110","type":"PURCHASE"}]}]}]}"#;
        let p = parse_cas(json).unwrap();
        assert_eq!((p.rows[0].qty, p.rows[0].avg_cost), (110.0, None));

        let csv = "amc,folio,pan,scheme,advisor,isin,amfi,date,description,amount,units,nav,balance,type,dividend\n\
X AMC,1,P,X Fund,,INF000000001,1,2024-05-01,Purchase,1000,10,100,110,PURCHASE,\n";
        let p = parse_cas(csv).unwrap();
        assert_eq!((p.rows[0].qty, p.rows[0].avg_cost), (110.0, None));
    }

    #[test]
    fn cas_summary_csv_has_no_cost() {
        let p = parse_cas(include_str!("fixtures/cas_summary.csv")).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!((p.rows[0].qty, p.rows[0].avg_cost), (23.412, None));
    }

    #[test]
    fn demat_cas_equities_update_quantities_only() {
        let p = parse_cas(include_str!("fixtures/cas_nsdl.json")).unwrap();
        assert_eq!(p.rows.len(), 3);
        assert_eq!(p.rows[0].symbol, "HDFCBANK.NS");
        assert_eq!(p.rows[0].avg_cost, None);
        assert_eq!(p.rows[2].symbol, "MF:122639");
        assert_eq!(p.rows[2].avg_cost, Some(60.0));

        let mut pf = Portfolio::default();
        pf.add("HDFCBANK.NS", 90.0, 1500.0);
        let (applied, skipped) = apply_rows(&mut pf, &p.rows);
        // HDFCBANK: quantity synced, cost kept. RELIANCE: no cost, not held → skipped.
        assert_eq!(applied.len(), 2);
        assert_eq!(skipped.len(), 1);
        assert!(
            skipped[0].why.contains("portfolio add RELIANCE.NS"),
            "{:?}",
            skipped
        );
        let h = pf
            .holdings
            .iter()
            .find(|h| h.symbol == "HDFCBANK.NS")
            .unwrap();
        assert_eq!((h.shares, h.avg_cost), (100.0, 1500.0));
    }

    #[test]
    fn apply_rows_is_idempotent() {
        let rows = vec![
            row("TCS.NS", 10.0, Some(3500.0)),
            row("INFY.NS", 5.0, Some(1500.0)),
        ];
        let mut pf = Portfolio::default();
        apply_rows(&mut pf, &rows);
        let first: Vec<_> = pf
            .holdings
            .iter()
            .map(|h| (h.symbol.clone(), h.shares, h.avg_cost))
            .collect();
        apply_rows(&mut pf, &rows);
        let second: Vec<_> = pf
            .holdings
            .iter()
            .map(|h| (h.symbol.clone(), h.shares, h.avg_cost))
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn pdf_input_explains_casparser() {
        let err = parse_cas("%PDF-1.7 ...").unwrap_err().to_string();
        assert!(err.contains("casparser"), "{err}");
    }
}

//! F&O: option chains with IV and greeks, put/call ratio, max pain, and
//! payoff analysis of multi-leg strategies. NSE for Indian symbols, BSE for
//! SENSEX/BANKEX and `.BO` stocks, Yahoo Finance for US ones (see
//! `derivatives.rs`). The expected-moves view (`options moves`) lives in
//! `risk.rs`.

use super::*;
use crate::derivatives::{self, Assumptions, ChainSource, ContractView, OptionChain};
use crate::greeks::OptionKind;

/// Default risk-free rates (annual, in percent) when `--rate` is not given:
/// roughly the Indian 91-day T-bill and US 3-month Treasury yields.
const DEFAULT_RATE_IN: f64 = 6.5;
const DEFAULT_RATE_US: f64 = 4.0;

fn source_label(chain: &OptionChain) -> &'static str {
    match chain.source {
        ChainSource::Nse => "NSE",
        ChainSource::Bse => "BSE",
        ChainSource::Yahoo => "Yahoo Finance",
    }
}

/// `--rate` default for the chain's market, in percent.
fn default_rate_pct(chain: &OptionChain) -> f64 {
    match chain.source {
        ChainSource::Nse | ChainSource::Bse => DEFAULT_RATE_IN,
        ChainSource::Yahoo => DEFAULT_RATE_US,
    }
}

fn days_to_expiry(chain: &OptionChain) -> f64 {
    chain.years_to_expiry(chrono::Utc::now()) * 365.0
}

fn chain_header(title: &str, chain: &OptionChain) {
    let cur = chain.currency.as_deref();
    print_header(&format!(
        "{}: {} — expiry {}",
        title,
        chain.symbol,
        chain.expiry.format("%d %b %Y")
    ));
    print_kv(
        "Underlying",
        &chain
            .underlying
            .map(|p| format_price(p, cur).bold().to_string())
            .unwrap_or_else(|| "N/A".dimmed().to_string()),
    );
    let days = days_to_expiry(chain);
    print_kv(
        "Time to expiry",
        &if days <= 0.0 {
            "expired".into()
        } else if chain.as_of.is_some() {
            format!("{:.1} days (from the data time)", days)
        } else {
            format!("{:.1} days", days)
        },
    );
    print_kv(
        "Source",
        &format!(
            "{}{}",
            source_label(chain),
            chain
                .as_of
                .map(|t| format!(
                    " · as of {}",
                    t.with_timezone(&chrono::Local).format("%d %b %Y %H:%M")
                ))
                .unwrap_or_default()
        ),
    );
}

fn expiries_line(chain: &OptionChain) -> String {
    let today = chrono::Utc::now().date_naive();
    let upcoming: Vec<String> = chain
        .expiries
        .iter()
        .filter(|d| **d >= today)
        .take(6)
        .map(|d| d.to_string())
        .collect();
    upcoming.join(", ")
}

fn opt_num(v: Option<f64>, decimals: usize, width: usize) -> String {
    match v {
        Some(x) => format!("{:>width$.decimals$}", x),
        None => format!("{:>width$}", "—"),
    }
}

fn opt_count(v: Option<u64>, width: usize) -> String {
    match v {
        Some(x) => format!("{:>width$}", format_volume(x)),
        None => format!("{:>width$}", "—"),
    }
}

fn print_side(
    title: &str,
    rows: &[(f64, Option<&ContractView>)],
    atm: Option<f64>,
    oi_change: bool,
) {
    print_section(title);
    println!(
        "  {:>10} {:>9} {:>8} {:>8} {:>8} {:>7} {:>7} {:>8} {:>8} {:>7} {:>7}",
        "Strike".bold(),
        "LTP".bold(),
        "OI".bold(),
        if oi_change { "ΔOI" } else { "" }.bold(),
        "Volume".bold(),
        "IV".bold(),
        "Delta".bold(),
        "Gamma".bold(),
        "Theta".bold(),
        "Vega".bold(),
        "Rho".bold()
    );
    println!("  {}", "─".repeat(100).dimmed());
    for (strike, view) in rows {
        let strike_txt = format!("{:>10.2}", strike);
        let strike_cell = if Some(*strike) == atm {
            strike_txt.bold().yellow().to_string()
        } else {
            strike_txt
        };
        let Some(v) = view else {
            println!("  {} {}", strike_cell, "no contract".dimmed());
            continue;
        };
        let c = &v.contract;
        let g = v.greeks;
        let oi_change = match c.oi_change {
            Some(x) if oi_change => {
                let s = format!("{:>8}", format!("{:+}", x));
                if x >= 0 {
                    s.green().to_string()
                } else {
                    s.red().to_string()
                }
            }
            _ => format!("{:>8}", ""),
        };
        println!(
            "  {} {} {} {} {} {} {} {} {} {} {}",
            strike_cell,
            opt_num(c.last, 2, 9),
            opt_count(c.open_interest, 8),
            oi_change,
            opt_count(c.volume, 8),
            match v.iv {
                Some(iv) => format!("{:>6.1}%", iv * 100.0),
                None => format!("{:>7}", "—"),
            },
            opt_num(g.map(|g| g.delta), 3, 7),
            opt_num(g.map(|g| g.gamma), 4, 8),
            opt_num(g.map(|g| g.theta), 2, 8),
            opt_num(g.map(|g| g.vega), 2, 7),
            opt_num(g.map(|g| g.rho), 2, 7),
        );
    }
}

/// `options chain` — strikes around the money with OI, volume, IV and greeks.
pub async fn cmd_options_chain(
    symbol: &str,
    expiry: Option<chrono::NaiveDate>,
    strikes: usize,
    rate_pct: Option<f64>,
    div_yield_pct: f64,
    market: Market,
    json: bool,
) -> Result<()> {
    let chain = derivatives::fetch_chain(symbol, market, expiry).await?;
    let rate = rate_pct.unwrap_or(default_rate_pct(&chain)) / 100.0;
    let years = chain.years_to_expiry(chrono::Utc::now());
    // Greeks need a live spot and time left; without them the chain still prints.
    let assumptions = chain
        .underlying
        .filter(|_| years > 0.0)
        .map(|spot| Assumptions {
            spot,
            years,
            rate,
            div_yield: div_yield_pct / 100.0,
        });
    let rows = derivatives::near_the_money(&chain.rows, chain.underlying, strikes);
    // A zero spot makes the IV solver and greeks return None per contract.
    let views = derivatives::row_views(
        rows,
        &assumptions.unwrap_or(Assumptions {
            spot: 0.0,
            years: 0.0,
            rate,
            div_yield: 0.0,
        }),
    );

    if json {
        let out = serde_json::json!({
            "symbol": chain.symbol,
            "source": chain.source,
            "underlying": chain.underlying,
            "currency": chain.currency,
            "as_of": chain.as_of,
            "expiry": chain.expiry,
            "expiries": chain.expiries,
            "lot_size": chain.lot_size,
            "days_to_expiry": years * 365.0,
            "rate": rate,
            "div_yield": div_yield_pct / 100.0,
            "rows": views,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    chain_header("Option chain", &chain);
    print_kv(
        "Rate / dividend yield",
        &format!(
            "{:.2}% / {:.2}%{}",
            rate * 100.0,
            div_yield_pct,
            if rate_pct.is_none() {
                " (default; set --rate)"
            } else {
                ""
            }
        ),
    );
    print_kv("Upcoming expiries", &expiries_line(&chain));
    if let Some(lot) = chain.lot_size {
        print_kv("Lot size", &lot.to_string());
    }
    if assumptions.is_none() {
        println!(
            "\n  {}",
            "No live underlying price or time left to expiry: IV is the source's and greeks are skipped."
                .yellow()
        );
    }

    let atm = chain
        .underlying
        .and_then(|s| derivatives::atm_index(rows, s))
        .map(|i| rows[i].strike);
    // NSE and BSE report the day's change in open interest; Yahoo does not.
    let oi_change = chain.source != ChainSource::Yahoo;
    let calls: Vec<(f64, Option<&ContractView>)> =
        views.iter().map(|r| (r.strike, r.call.as_ref())).collect();
    let puts: Vec<(f64, Option<&ContractView>)> =
        views.iter().map(|r| (r.strike, r.put.as_ref())).collect();
    print_side("Calls", &calls, atm, oi_change);
    print_side("Puts", &puts, atm, oi_change);

    if rows.len() < chain.rows.len() {
        println!(
            "\n  {}",
            format!(
                "Showing {} of {} strikes around the money (-n 0 for all).",
                rows.len(),
                chain.rows.len()
            )
            .dimmed()
        );
    }
    println!(
        "  {}",
        "IV is solved from the bid/ask mid (else last price) with Black-Scholes at the data time;\n  theta is per calendar day, vega and rho per 1 point."
            .dimmed()
    );
    if chain.source == ChainSource::Yahoo {
        println!(
            "  {}",
            "US equity options are American-style; these greeks are the European approximation."
                .dimmed()
        );
    }
    println!();
    Ok(())
}

fn pcr_reading(ratio: f64) -> &'static str {
    if ratio < 0.7 {
        "call-heavy — more open calls than puts"
    } else if ratio <= 1.3 {
        "balanced"
    } else {
        "put-heavy — more open puts than calls (often hedging)"
    }
}

/// `options pcr` — put/call ratio and the strikes with the most open interest.
pub async fn cmd_options_pcr(
    symbol: &str,
    expiry: Option<chrono::NaiveDate>,
    market: Market,
    json: bool,
) -> Result<()> {
    let chain = derivatives::fetch_chain(symbol, market, expiry).await?;
    let p = derivatives::pcr(&chain.rows);
    let top_calls = derivatives::top_oi(&chain.rows, OptionKind::Call, 3);
    let top_puts = derivatives::top_oi(&chain.rows, OptionKind::Put, 3);

    if json {
        let walls = |v: &[(f64, u64)]| -> Vec<serde_json::Value> {
            v.iter()
                .map(|(s, oi)| serde_json::json!({"strike": s, "open_interest": oi}))
                .collect()
        };
        let out = serde_json::json!({
            "symbol": chain.symbol,
            "source": chain.source,
            "underlying": chain.underlying,
            "expiry": chain.expiry,
            "pcr": p,
            "top_call_oi": walls(&top_calls),
            "top_put_oi": walls(&top_puts),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    chain_header("Put/call ratio", &chain);
    print_section("Open interest");
    print_kv("Calls", &format_volume(p.call_oi));
    print_kv("Puts", &format_volume(p.put_oi));
    print_kv(
        "PCR (OI)",
        &p.oi_ratio
            .map(|r| format!("{} — {}", format!("{:.2}", r).bold(), pcr_reading(r)))
            .unwrap_or_else(|| "N/A (no call open interest)".dimmed().to_string()),
    );
    print_section("Volume");
    print_kv("Calls", &format_volume(p.call_volume));
    print_kv("Puts", &format_volume(p.put_volume));
    print_kv(
        "PCR (volume)",
        &p.volume_ratio
            .map(|r| format!("{:.2}", r))
            .unwrap_or_else(|| "N/A (no call volume)".dimmed().to_string()),
    );

    let cur = chain.currency.as_deref();
    let wall = |v: &[(f64, u64)]| -> String {
        if v.is_empty() {
            return "—".into();
        }
        v.iter()
            .map(|(s, oi)| format!("{} ({})", format_price(*s, cur), format_volume(*oi)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    print_section("Largest open interest");
    print_kv("Calls (resistance)", &wall(&top_calls));
    print_kv("Puts (support)", &wall(&top_puts));
    println!(
        "\n  {}",
        "Descriptive positioning for this expiry, not a trading signal.".dimmed()
    );
    println!();
    Ok(())
}

/// `options maxpain` — the settlement price where option buyers collect least.
pub async fn cmd_options_maxpain(
    symbol: &str,
    expiry: Option<chrono::NaiveDate>,
    market: Market,
    json: bool,
) -> Result<()> {
    let chain = derivatives::fetch_chain(symbol, market, expiry).await?;
    let mp = derivatives::max_pain(&chain.rows).with_context(|| {
        format!(
            "No open interest for {} on {} — max pain needs open contracts",
            chain.symbol, chain.expiry
        )
    })?;

    if json {
        let out = serde_json::json!({
            "symbol": chain.symbol,
            "source": chain.source,
            "underlying": chain.underlying,
            "expiry": chain.expiry,
            "max_pain": mp.strike,
            "curve": mp.curve,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let cur = chain.currency.as_deref();
    chain_header("Max pain", &chain);
    print_kv(
        "Max pain strike",
        &format_price(mp.strike, cur).bold().yellow().to_string(),
    );
    if let Some(spot) = chain.underlying {
        let gap = (mp.strike - spot) / spot * 100.0;
        print_kv("Distance from spot", &format!("{:+.2}%", gap));
    }

    print_section("Option holders' payout if expiry settles at each strike");
    let i = mp
        .curve
        .iter()
        .position(|p| p.strike == mp.strike)
        .unwrap_or(0);
    let lo = i.saturating_sub(7);
    let hi = (i + 8).min(mp.curve.len());
    let window = &mp.curve[lo..hi];
    let max = window.iter().map(|p| p.total).fold(0.0, f64::max);
    for p in window {
        let len = if max > 0.0 {
            ((p.total / max) * 40.0).round() as usize
        } else {
            0
        };
        let bar = "█".repeat(len.max(1));
        let strike = format!("{:>10.2}", p.strike);
        let (strike, bar) = if p.strike == mp.strike {
            (strike.bold().yellow().to_string(), bar.yellow().to_string())
        } else {
            (strike, bar.dimmed().to_string())
        };
        println!(
            "  {} {} {}",
            strike,
            bar,
            format_volume(p.total as u64).dimmed()
        );
    }
    println!(
        "\n  {}",
        "Payout = Σ open interest × intrinsic value (in underlying points). Max pain assumes open\n  interest stays where it is; it describes positioning and is not a price forecast."
            .dimmed()
    );
    println!();
    Ok(())
}

//! Your data: watchlist, alerts, import/export, wealth history, rebalancing, tax, and dividends.

use super::portfolio::{ESTIMATED_DATE_HINT, portfolio_title, value_holdings};
use super::*;
use crate::portfolio::{self as pf, ImportOptions, ImportOutcome, ImportRow, SyncChange};
use crate::tax::{self, Country};

pub async fn cmd_watchlist(action: &str, symbols: &[String], market: Market) -> Result<()> {
    let mut wl = Watchlist::load()?;

    match action {
        "add" => {
            for sym in symbols {
                let resolved = market::resolve_symbol(sym, market);
                if wl.add(&resolved) {
                    println!("  {} Added {} to watchlist", "✓".green(), resolved.cyan());
                } else {
                    println!("  {} {} already in watchlist", "·".dimmed(), resolved);
                }
            }
            wl.save()?;
        }
        "remove" | "rm" => {
            for sym in symbols {
                let resolved = market::resolve_symbol(sym, market);
                if wl.remove(&resolved) {
                    println!("  {} Removed {}", "✓".green(), resolved);
                } else {
                    println!("  {} {} not found", "✗".red(), resolved);
                }
            }
            wl.save()?;
        }
        "show" | "list" | "" => {
            if wl.symbols.is_empty() {
                println!(
                    "\n  {}",
                    "Watchlist is empty. Use 'stockwise watch add <SYMBOL>' to add.".dimmed()
                );
                return Ok(());
            }

            let client = YahooClient::new().await?;
            let sym_refs: Vec<&str> = wl.symbols.iter().map(|s| s.as_str()).collect();
            let quotes = client.get_quote(&sym_refs).await?;

            print_header("Watchlist");
            println!(
                "  {:<12} {:>10} {:>12} {:>10} {:>14}",
                "Symbol".bold(),
                "Price".bold(),
                "Change".bold(),
                "Change %".bold(),
                "Volume".bold(),
            );
            println!("  {}", "─".repeat(62).dimmed());

            for q in &quotes {
                let sym = q.symbol.as_deref().unwrap_or("???");
                let cur = q.currency.as_deref();
                let price = q.regular_market_price.unwrap_or(0.0);
                let change = q.regular_market_change.unwrap_or(0.0);
                let pct = q.regular_market_change_percent.unwrap_or(0.0);
                let vol = q.regular_market_volume.unwrap_or(0);
                let change_str = if change >= 0.0 {
                    format!("+{:.2}", change).green().to_string()
                } else {
                    format!("{:.2}", change).red().to_string()
                };
                let pct_str = if pct >= 0.0 {
                    format!("+{:.2}%", pct).green().to_string()
                } else {
                    format!("{:.2}%", pct).red().to_string()
                };
                println!(
                    "  {:<12} {:>10} {:>12} {:>10} {:>14}",
                    sym.cyan(),
                    format_price(price, cur),
                    change_str,
                    pct_str,
                    format_volume(vol),
                );
            }
            println!();
        }
        _ => bail!(
            "Unknown watch action '{}'. Use add, remove, or show",
            action
        ),
    }
    Ok(())
}

// ── Movers ──

pub async fn cmd_alert(
    action: &str,
    symbol: Option<&str>,
    condition: Option<&str>,
    target: Option<f64>,
    market: Market,
) -> Result<()> {
    let mut store = AlertStore::load()?;
    match action {
        "add" => {
            let sym = symbol.context("Symbol is required")?;
            let resolved = market::resolve_symbol(sym, market);
            let cond_str = condition.context("Condition required (above/below)")?;
            let cond = match cond_str {
                "above" | ">" => AlertCondition::Above,
                "below" | "<" => AlertCondition::Below,
                _ => {
                    println!("  Condition must be 'above' or 'below'");
                    return Ok(());
                }
            };
            let tgt = target.context("Target price is required")?;
            store.add(&resolved, cond.clone(), tgt);
            store.save()?;
            println!(
                "  {} Alert: {} {} {:.2}",
                "✓".green(),
                resolved.cyan(),
                cond,
                tgt
            );
        }
        "remove" | "rm" => {
            let idx_str = symbol.context("Alert index required (see 'alert list')")?;
            let idx: usize = idx_str.parse().context("Invalid index")?;
            if store.remove_by_index(idx) {
                store.save()?;
                println!("  {} Removed alert #{}", "✓".green(), idx);
            } else {
                bail!("No alert #{} — see `stockwise alert list`", idx);
            }
        }
        "check" => {
            if store.alerts.is_empty() {
                println!("  {}", "No alerts configured.".dimmed());
                return Ok(());
            }
            let a_syms: Vec<String> = store.alerts.iter().map(|a| a.symbol.clone()).collect();
            let mut unique: Vec<&str> = a_syms.iter().map(|s| s.as_str()).collect();
            unique.sort();
            unique.dedup();
            let client = YahooClient::new().await?;
            let quotes = client.get_quote(&unique).await?;
            print_header("Alert Check");
            let mut triggered = 0;
            for alert in &store.alerts {
                let Some(price) = quotes
                    .iter()
                    .find(|q| q.symbol.as_deref() == Some(alert.symbol.as_str()))
                    .and_then(|q| q.regular_market_price)
                else {
                    println!(
                        "  {} {} — no live price, skipped",
                        "?".yellow(),
                        alert.symbol.cyan()
                    );
                    continue;
                };
                let cur = quotes
                    .iter()
                    .find(|q| q.symbol.as_deref() == Some(alert.symbol.as_str()))
                    .and_then(|q| q.currency.as_deref());
                let is_hit = match alert.condition {
                    AlertCondition::Above => price >= alert.target,
                    AlertCondition::Below => price <= alert.target,
                };
                let status = if is_hit {
                    triggered += 1;
                    "TRIGGERED".red().bold().to_string()
                } else {
                    "waiting".dimmed().to_string()
                };
                println!(
                    "  {} {} {:.2} | Now: {} | {}",
                    alert.symbol.cyan(),
                    alert.condition,
                    alert.target,
                    format_price(price, cur),
                    status
                );
            }
            if triggered == 0 {
                println!("\n  {}", "No alerts triggered.".dimmed());
            } else {
                println!("\n  {} {} alert(s) triggered!", "⚠".yellow(), triggered);
            }
            println!();
        }
        "list" | "show" | "" => {
            if store.alerts.is_empty() {
                println!(
                    "\n  {}",
                    "No alerts. Use 'stockwise alert add <SYMBOL> above/below <PRICE>'.".dimmed()
                );
                return Ok(());
            }
            print_header("Price Alerts");
            for (i, a) in store.alerts.iter().enumerate() {
                println!(
                    "  #{} {} {} {:.2}  (set {})",
                    i,
                    a.symbol.cyan(),
                    a.condition,
                    a.target,
                    a.created_at.dimmed()
                );
            }
            println!();
        }
        _ => bail!(
            "Unknown alert action '{}'. Use add, remove, check, or list",
            action
        ),
    }
    Ok(())
}

// ── Screener ──

pub async fn cmd_export(what: &str, _market: Market) -> Result<()> {
    match what {
        "portfolio" => {
            let portfolio = Portfolio::load()?;
            if portfolio.holdings.is_empty() {
                println!("  {}", "Portfolio is empty.".dimmed());
                return Ok(());
            }
            let client = YahooClient::new().await?;
            let symbols: Vec<String> = portfolio
                .holdings
                .iter()
                .map(|h| h.symbol.clone())
                .collect();
            let sym_refs: Vec<&str> = symbols.iter().map(|s| s.as_str()).collect();
            let quotes = client.get_quote(&sym_refs).await?;
            let path = "portfolio_export.csv";
            let mut csv = String::from("Symbol,Shares,Avg Cost,Current Price,Value,P&L,P&L %\n");
            for h in &portfolio.holdings {
                let price = quotes
                    .iter()
                    .find(|q| q.symbol.as_deref() == Some(&h.symbol))
                    .and_then(|q| q.regular_market_price)
                    .unwrap_or(0.0);
                let value = h.shares * price;
                let cost = h.shares * h.avg_cost;
                let pnl = value - cost;
                let pnl_pct = if cost > 0.0 {
                    (pnl / cost) * 100.0
                } else {
                    0.0
                };
                csv.push_str(&format!(
                    "{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}%\n",
                    h.symbol, h.shares, h.avg_cost, price, value, pnl, pnl_pct
                ));
            }
            std::fs::write(path, &csv)?;
            println!("  {} Exported portfolio to {}", "✓".green(), path.cyan());
        }
        "watchlist" => {
            let wl = Watchlist::load()?;
            if wl.symbols.is_empty() {
                println!("  {}", "Watchlist is empty.".dimmed());
                return Ok(());
            }
            let client = YahooClient::new().await?;
            let sym_refs: Vec<&str> = wl.symbols.iter().map(|s| s.as_str()).collect();
            let quotes = client.get_quote(&sym_refs).await?;
            let path = "watchlist_export.csv";
            let mut csv = String::from("Symbol,Price,Change,Change %,Volume,Market Cap\n");
            for q in &quotes {
                csv.push_str(&format!(
                    "{},{:.2},{:.2},{:.2}%,{},{:.0}\n",
                    q.symbol.as_deref().unwrap_or(""),
                    q.regular_market_price.unwrap_or(0.0),
                    q.regular_market_change.unwrap_or(0.0),
                    q.regular_market_change_percent.unwrap_or(0.0),
                    q.regular_market_volume.unwrap_or(0),
                    q.market_cap.unwrap_or(0.0)
                ));
            }
            std::fs::write(path, &csv)?;
            println!("  {} Exported watchlist to {}", "✓".green(), path.cyan());
        }
        _ => bail!("Unknown export '{}'. Use portfolio or watchlist", what),
    }
    Ok(())
}

// ── Alerts ──

pub async fn cmd_import(source: &str, file: &str, replace: bool, market: Market) -> Result<()> {
    let path = std::path::Path::new(file);
    if !path.exists() {
        println!();
        println!("  {} How to export your holdings:", "→".cyan());
        match source {
            "kite" | "zerodha" => {
                println!(
                    "    1. Login to {} → Portfolio → Holdings",
                    "kite.zerodha.com".cyan()
                );
                println!(
                    "    2. Click the {} icon (top-right of holdings table)",
                    "download/export".bold()
                );
                println!("    3. Save the CSV file");
                println!("    4. Run: stockwise import kite <path-to-file.csv>");
            }
            "indmoney" => {
                println!("    1. Open {} app → Portfolio → Stocks", "IndMoney".cyan());
                println!(
                    "    2. Tap {} → Download Statement / Export",
                    "⋮ (menu)".bold()
                );
                println!("    3. Save the CSV/Excel file");
                println!("    4. Run: stockwise import indmoney <path-to-file.csv>");
            }
            "groww" => {
                println!("    1. Open {} → Stocks → Holdings", "groww.in".cyan());
                println!(
                    "    2. Click {} to get the holdings statement",
                    "Download".bold()
                );
                println!("    3. Open the .xlsx file and save it as CSV");
                println!("    4. Run: stockwise import groww <path-to-file.csv>");
            }
            "upstox" => {
                println!(
                    "    1. Open {} → Portfolio → Holdings",
                    "pro.upstox.com".cyan()
                );
                println!("    2. Click {} and save the CSV file", "Download".bold());
                println!("    3. Run: stockwise import upstox <path-to-file.csv>");
            }
            "cas" => {
                println!(
                    "    1. Request a detailed CAS from {} or {} (emailed as a password-protected PDF)",
                    "camsonline.com".cyan(),
                    "kfintech.com".cyan()
                );
                println!(
                    "    2. Convert it with casparser: {}",
                    "pip install casparser && casparser CAS.pdf -p <password> -o cas.json".bold()
                );
                println!("    3. Run: stockwise import cas cas.json");
            }
            _ => {
                println!("    Provide a CSV with columns: Symbol, Quantity, Average Price");
                println!("    Run: stockwise import csv <path-to-file.csv>");
            }
        }
        bail!("File not found: {}", file);
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", file))?;
    match source {
        "groww" => {
            return super::imports::import_rows(
                "groww",
                file,
                replace,
                // Groww and Upstox list Indian stocks, whatever -m says.
                super::imports::parse_groww(&content, Market::In)?,
            )
            .await;
        }
        "upstox" => {
            return super::imports::import_rows(
                "upstox",
                file,
                replace,
                super::imports::parse_upstox(&content, Market::In)?,
            )
            .await;
        }
        "cas" => {
            return super::imports::import_rows(
                "cas",
                file,
                replace,
                super::imports::parse_cas(&content)?,
            )
            .await;
        }
        _ => {}
    }
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() < 2 {
        bail!("{} is empty or has no data rows", file);
    }

    let header = lines[0].to_lowercase();
    let cols = parse_csv_header(&header);
    // Column names per format, most specific first (matched as substrings).
    let (sym_names, qty_names, price_names, expected): (&[&str], &[&str], &[&str], &str) =
        match source {
            // Kite holdings: Instrument, Qty., Avg. cost, LTP, ...
            // Kite tradebook: symbol, trade_date, trade_type, quantity, price, ...
            "kite" | "zerodha" => (
                &["instrument", "tradingsymbol", "symbol", "stock"],
                &["qty", "qty.", "quantity", "shares"],
                &[
                    "avg. cost",
                    "avg cost",
                    "average cost",
                    "avg_price",
                    "average price",
                    "buy avg",
                    "buy avg.",
                    "price",
                ],
                "Kite CSV columns. Expected: Instrument, Qty., Avg. cost",
            ),
            // IndMoney: Stock Name, Symbol/ISIN, Quantity, Avg Buy Price, ...
            "indmoney" => (
                &[
                    "symbol",
                    "isin",
                    "stock symbol",
                    "scrip",
                    "stock name",
                    "name",
                ],
                &["quantity", "qty", "shares", "units"],
                &[
                    "avg buy price",
                    "avg price",
                    "average price",
                    "buy price",
                    "avg. buy price",
                    "average cost",
                ],
                "IndMoney CSV columns. Expected: Symbol/Stock Name, Quantity, Avg Buy Price",
            ),
            // "csv" or any unrecognized format: generic columns.
            _ => (
                &[
                    "symbol",
                    "stock",
                    "instrument",
                    "name",
                    "ticker",
                    "scrip",
                    "tradingsymbol",
                ],
                &["quantity", "qty", "qty.", "shares", "units"],
                &[
                    "avg price",
                    "avg. price",
                    "average price",
                    "avg cost",
                    "avg. cost",
                    "buy price",
                    "price",
                    "cost",
                ],
                "CSV columns. Your CSV needs Symbol, Quantity and Average Price columns",
            ),
        };
    let (Some(si), Some(qi), Some(pi)) = (
        find_col(&cols, sym_names),
        find_col(&cols, qty_names),
        find_col(&cols, price_names),
    ) else {
        bail!("Could not detect {}. Found: {}", expected, header);
    };
    // Optional: a purchase/trade date per row, and buy/sell for tradebooks.
    let date_col = find_col_exact(
        &cols,
        &[
            "trade_date",
            "trade date",
            "buy date",
            "buy_date",
            "purchase date",
            "purchase_date",
            "date of purchase",
            "acquisition date",
            "date acquired",
            "date",
        ],
    );
    let side_col = find_col_exact(
        &cols,
        &[
            "trade_type",
            "trade type",
            "transaction type",
            "transaction_type",
            "type",
            "side",
            "buy/sell",
            "action",
        ],
    );
    let month_first = market == Market::Us;
    let field = |fields: &[String], i: usize| -> String {
        fields
            .get(i)
            .map(|f| f.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let number = |raw: String| raw.replace(',', "").parse::<f64>().unwrap_or(0.0);

    let mut rows = Vec::new();
    let mut bad_dates = Vec::new();
    for line in &lines[1..] {
        let fields = parse_csv_row(line);
        if fields.len() <= si.max(qi).max(pi) {
            continue;
        }
        let symbol_raw = field(&fields, si);
        if symbol_raw.is_empty() {
            continue;
        }
        let qty = number(field(&fields, qi));
        let price = number(field(&fields, pi));
        // "NaN"/"inf" parse as f64 but can't be saved as JSON.
        if !(qty.is_finite() && qty > 0.0 && price.is_finite() && price > 0.0) {
            continue;
        }
        let date = match date_col.map(|i| field(&fields, i)) {
            Some(raw) if !raw.is_empty() => match pf::parse_trade_date(&raw, month_first) {
                Some(d) => Some(d.to_string()),
                None => {
                    bad_dates.push(raw);
                    None
                }
            },
            _ => None,
        };
        let sell = side_col.is_some_and(|i| {
            matches!(
                field(&fields, i).to_lowercase().as_str(),
                "sell" | "s" | "sold"
            )
        });
        rows.push(ImportRow {
            symbol: market::resolve_symbol(&symbol_raw, market),
            qty,
            price,
            date,
            sell,
        });
    }
    if rows.is_empty() {
        bail!("No holdings found in {}. Check the format.", file);
    }
    // A wrong date would silently change tax and XIRR: refuse the file.
    if !bad_dates.is_empty() {
        bail!(
            "Unrecognized dates in {}: {}. Use YYYY-MM-DD (or DD-MM-YYYY), then import again — nothing was changed.",
            file,
            bad_dates.join(", ")
        );
    }

    apply_import_rows(source, file, rows, side_col.is_some(), replace).await
}

/// Apply parsed import rows (from any source) to the portfolio and report
/// what changed: dated rows rebuild a symbol's lots, undated rows sync its
/// quantity and keep existing lots. Re-importing the same rows changes
/// nothing.
pub(super) async fn apply_import_rows(
    source: &str,
    file: &str,
    rows: Vec<ImportRow>,
    tradebook: bool,
    replace: bool,
) -> Result<()> {
    let mut portfolio = Portfolio::load()?;
    let dated = |sym: &str| {
        rows.iter()
            .any(|r| r.symbol == sym && (r.date.is_some() || r.sell))
    };
    let mut symbols: Vec<String> = rows.iter().map(|r| r.symbol.clone()).collect();
    symbols.dedup();
    symbols.sort();
    symbols.dedup();

    // A tradebook's quantities are as traded: fetch each symbol's splits so
    // sells after a split or bonus match the right number of shares.
    let mut splits = Vec::new();
    if tradebook {
        let client = YahooClient::new().await?;
        for sym in symbols.iter().filter(|s| dated(s)) {
            let since = rows
                .iter()
                .filter(|r| &r.symbol == sym)
                .filter_map(|r| r.date.clone())
                .min()
                .unwrap_or_else(pf::today);
            let (s, _, _) = super::portfolio::fetch_actions(&client, sym, &since)
                .await
                .with_context(|| {
                    format!(
                        "Could not check {} for splits, which a tradebook needs; nothing was changed. Try again when online",
                        sym
                    )
                })?;
            splits.push((sym.clone(), s));
        }
    }

    // A holdings snapshot with more shares than StockWise has may just be a
    // split not yet applied to your lots: leave those for `portfolio actions`.
    let snapshot: Vec<(String, f64)> = symbols
        .iter()
        .filter(|s| !dated(s))
        .map(|s| {
            let qty = rows.iter().filter(|r| &r.symbol == s).map(|r| r.qty).sum();
            (s.clone(), qty)
        })
        .collect();
    let blocked = super::portfolio::blocked_by_pending_splits(&portfolio, &snapshot).await;
    let rows: Vec<ImportRow> = rows
        .into_iter()
        .filter(|r| !blocked.iter().any(|(s, _)| *s == r.symbol))
        .collect();

    let outcomes = portfolio.apply_import(
        &rows,
        ImportOptions {
            source: &format!("import:{}", source),
            replace,
            tradebook,
            splits: &splits,
        },
    );
    let updated = outcomes
        .iter()
        .filter(|(_, o)| {
            !matches!(
                o,
                ImportOutcome::Unmatched { .. } | ImportOutcome::WouldReplace
            )
        })
        .count();
    if updated > 0 {
        portfolio.save()?;
    }

    let mut undated = false;
    for (sym, outcome) in &outcomes {
        let cur = pf::symbol_currency(sym);
        let now = portfolio.holding(sym).map_or_else(
            || "none left".to_string(),
            |h| format!("{} shares @ {}", h.shares, format_price(h.avg_cost, cur)),
        );
        match outcome {
            ImportOutcome::Synced(SyncChange::New) => {
                undated = true;
                println!("  {} {} — {} (new)", "✓".green(), sym.cyan(), now);
            }
            ImportOutcome::Synced(SyncChange::Unchanged) => {
                println!("  {} {} — {}, unchanged", "·".dimmed(), sym, now);
            }
            ImportOutcome::Synced(SyncChange::Added(n)) => {
                undated = true;
                println!(
                    "  {} {} — +{} shares, now {}",
                    "✓".green(),
                    sym.cyan(),
                    n,
                    now
                );
            }
            ImportOutcome::Synced(SyncChange::Reduced(n)) => println!(
                "  {} {} — {} fewer shares (taken from the oldest lots, not recorded as a sale), now {}",
                "✓".green(),
                sym.cyan(),
                n,
                now
            ),
            ImportOutcome::Lots {
                lots,
                sales,
                splits,
            } => {
                let mut extra = String::new();
                if *sales > 0 {
                    extra.push_str(&format!(", {} sale(s) recorded", sales));
                }
                if *splits > 0 {
                    extra.push_str(&format!(", adjusted for {} split(s)/bonus(es)", splits));
                }
                println!(
                    "  {} {} — {} lot(s){}, now {}",
                    "✓".green(),
                    sym.cyan(),
                    lots,
                    extra,
                    now
                );
            }
            ImportOutcome::Unmatched { unmatched } => println!(
                "  {} {} — skipped: sells of {} shares have no earlier buy in this file. Import a \
                 tradebook that covers every buy, or record the sale with `stockwise portfolio sell`.",
                "!".yellow(),
                sym,
                unmatched
            ),
            ImportOutcome::WouldReplace => println!(
                "  {} {} — skipped: it has lots or sales from elsewhere (manual entries, another \
                 import) that this file would replace. Re-run with --replace to rebuild it from the file.",
                "!".yellow(),
                sym
            ),
        }
    }
    for (sym, reason) in &blocked {
        println!("  {} {} — skipped: {}", "!".yellow(), sym, reason);
    }

    if updated == 0 {
        bail!(
            "Nothing imported from {} — see the skipped holdings above.",
            file
        );
    }
    println!(
        "\n  {} Imported {} rows: {} holdings updated.",
        "✓".green().bold(),
        rows.len(),
        updated
    );
    if undated {
        println!(
            "  {}",
            "This file has no purchase dates, so new shares are dated today (marked *). Add a \
             date column, or fix dates with `stockwise portfolio lots SYMBOL --lot N --date YYYY-MM-DD`."
                .dimmed()
        );
    }
    println!(
        "  Run {} to see your portfolio.",
        "stockwise portfolio show".cyan()
    );
    println!();
    Ok(())
}

pub(super) fn parse_csv_header(header: &str) -> Vec<String> {
    parse_csv_row(header)
        .iter()
        .map(|s| s.trim().trim_matches('"').to_lowercase())
        .collect()
}

pub(super) fn parse_csv_row(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

/// Like [`find_col`] but the column name must match exactly, for short
/// names such as "date" or "type" that would match too much as substrings.
pub(super) fn find_col_exact(cols: &[String], names: &[&str]) -> Option<usize> {
    names
        .iter()
        .find_map(|name| cols.iter().position(|c| c == name))
}

pub(super) fn find_col(cols: &[String], names: &[&str]) -> Option<usize> {
    for name in names {
        if let Some(i) = cols.iter().position(|c| c.contains(name)) {
            return Some(i);
        }
    }
    None
}

// ── Angel One Trading ──

pub async fn cmd_wealth(market: Market) -> Result<()> {
    let portfolio = Portfolio::load()?;
    let mut history = WealthHistory::load()?;

    if portfolio.holdings.is_empty() {
        println!("\n  {}", "Portfolio is empty. Add holdings first.".dimmed());
        return Ok(());
    }

    // Fetch current values and snapshot
    let symbols: Vec<String> = portfolio
        .holdings
        .iter()
        .map(|h| h.symbol.clone())
        .collect();
    let client = YahooClient::new().await?;
    // Always the market's currency, so the history never mixes currencies
    // as holdings come and go.
    let base = super::portfolio::market_currency(market);
    let val = value_holdings(&client, &symbols, market, Some(base)).await?;
    let totals = val.totals(&portfolio);
    let (total_value, total_cost) = (totals.value, totals.cost);
    // A snapshot with a holding valued at 0 would show as a fake crash in the
    // history forever, so refuse to save one.
    if !totals.unpriced.is_empty() || !totals.unconverted.is_empty() {
        let missing: Vec<String> = totals
            .unpriced
            .iter()
            .chain(&totals.unconverted)
            .cloned()
            .collect();
        bail!(
            "No live price or exchange rate for {}; snapshot not saved. Fix or remove the holding, then retry.",
            missing.join(", ")
        );
    }

    // Save today's snapshot
    history.add_snapshot(total_value, total_cost, portfolio.holdings.len());
    history.save()?;

    let cur = Some(val.base());
    let csym = market::currency_symbol(cur);

    print_header("Wealth Tracker");

    if history.snapshots.len() >= 2 {
        let values: Vec<f64> = history.snapshots.iter().map(|s| s.total_value).collect();
        let color = if *values.last().unwrap() >= values[0] {
            "green"
        } else {
            "red"
        };
        for line in charts::line_chart(&values, 55, 8, color, "Portfolio Value Over Time") {
            println!("{}", line);
        }
    }

    print_section("Current Snapshot");
    print_kv(
        "Total Value",
        &format!("{}{:.2}", csym, total_value).bold().to_string(),
    );
    print_kv("Total Invested", &format!("{}{:.2}", csym, total_cost));
    let pnl = total_value - total_cost;
    let pnl_pct = if total_cost > 0.0 {
        (pnl / total_cost) * 100.0
    } else {
        0.0
    };
    let pnl_str = if pnl >= 0.0 {
        format!("+{}{:.2} (+{:.2}%)", csym, pnl, pnl_pct)
            .green()
            .bold()
            .to_string()
    } else {
        format!("-{}{:.2} ({:.2}%)", csym, pnl.abs(), pnl_pct)
            .red()
            .bold()
            .to_string()
    };
    print_kv("Total P&L", &pnl_str);
    if let Some(note) = val.fx_note() {
        println!("  {}", note.dimmed());
    }
    print_kv("Snapshots", &history.snapshots.len().to_string());

    if history.snapshots.len() >= 2 {
        let first = &history.snapshots[0];
        let growth = ((total_value / first.total_value) - 1.0) * 100.0;
        print_kv(
            "Since First Track",
            &format!("{:+.2}% (from {})", growth, first.date),
        );
    }

    println!(
        "\n  {}",
        "Run 'stockwise wealth' daily to build your growth chart."
            .dimmed()
            .italic()
    );
    println!();
    Ok(())
}

// ── Rebalance ──

pub async fn cmd_rebalance(targets: &[(String, f64)], market: Market) -> Result<()> {
    let portfolio = Portfolio::load()?;
    if portfolio.holdings.is_empty() {
        println!("\n  {}", "Portfolio is empty.".dimmed());
        return Ok(());
    }

    let symbols: Vec<String> = portfolio
        .holdings
        .iter()
        .map(|h| h.symbol.clone())
        .collect();
    let client = YahooClient::new().await?;
    let val = value_holdings(&client, &symbols, market, None).await?;

    // Weights only make sense in one currency: value everything in the base.
    let mut total_value = 0.0;
    let mut holdings_value: Vec<(String, f64, f64)> = Vec::new(); // symbol, value, price (base currency)
    for h in &portfolio.holdings {
        let Some(price) = val.price(&h.symbol) else {
            bail!(
                "No live price for {} — fix or remove it before rebalancing",
                h.symbol
            );
        };
        let Some(price) = val.to_base(&h.symbol, price) else {
            bail!(
                "No exchange rate for {} — can't weigh it against your other holdings",
                h.symbol
            );
        };
        let value = h.shares * price;
        total_value += value;
        holdings_value.push((h.symbol.clone(), value, price));
    }

    let cur = Some(val.base());
    let csym = market::currency_symbol(cur);

    print_header("Portfolio Rebalance");
    print_kv("Total Value", &format!("{}{:.2}", csym, total_value));

    // If no targets given, show equal-weight rebalance
    let target_pct = if targets.is_empty() {
        let equal = 100.0 / holdings_value.len() as f64;
        holdings_value
            .iter()
            .map(|(s, _, _)| (s.clone(), equal))
            .collect::<Vec<_>>()
    } else {
        targets.to_vec()
    };

    println!();
    println!(
        "  {:<14} {:>10} {:>8} {:>8} {:>12}",
        "Symbol".bold(),
        "Current".bold(),
        "Now%".bold(),
        "Target%".bold(),
        "Action".bold()
    );
    println!("  {}", "─".repeat(56).dimmed());

    for (sym, value, price) in &holdings_value {
        let current_pct = (value / total_value) * 100.0;
        let tgt = target_pct
            .iter()
            .find(|(s, _)| s == sym)
            .map(|(_, p)| *p)
            .unwrap_or(100.0 / holdings_value.len() as f64);
        let target_value = total_value * (tgt / 100.0);
        let diff = target_value - value;
        let diff_shares = if *price > 0.0 { diff / price } else { 0.0 };

        let action = if diff.abs() < 1.0 {
            "OK".green().to_string()
        } else if diff > 0.0 {
            format!("BUY {:.0} shares", diff_shares.ceil())
                .green()
                .to_string()
        } else {
            format!("SELL {:.0} shares", diff_shares.abs().floor())
                .red()
                .to_string()
        };

        println!(
            "  {:<14} {:>10} {:>8} {:>8} {:>12}",
            sym.cyan(),
            format!("{}{:.2}", csym, value),
            format!("{:.1}%", current_pct),
            format!("{:.1}%", tgt),
            action
        );
    }
    println!();
    Ok(())
}

// ── Earnings Calendar ──

pub async fn cmd_harvest(_market: Market) -> Result<()> {
    let portfolio = Portfolio::load()?;
    if portfolio.holdings.is_empty() {
        println!("\n  {}", "Portfolio is empty.".dimmed());
        return Ok(());
    }

    let symbols: Vec<String> = portfolio
        .holdings
        .iter()
        .map(|h| h.symbol.clone())
        .collect();
    let sym_refs: Vec<&str> = symbols.iter().map(|s| s.as_str()).collect();
    let client = YahooClient::new().await?;
    let quotes = client.get_quote(&sym_refs).await?;

    print_header("Tax-Loss Harvesting Opportunities");
    let mut found = false;

    println!(
        "  {:<14} {:>10} {:>10} {:>12} {:>10}",
        "Symbol".bold(),
        "Avg Cost".bold(),
        "Price".bold(),
        "Loss".bold(),
        "Loss %".bold()
    );
    println!("  {}", "─".repeat(60).dimmed());

    for h in &portfolio.holdings {
        let quote = quotes
            .iter()
            .find(|q| q.symbol.as_deref() == Some(&h.symbol));
        // Never suggest selling something just because its price is missing.
        let Some(price) = quote.and_then(|q| q.regular_market_price) else {
            continue;
        };
        let cur = quote.and_then(|q| q.currency.as_deref());
        let csym = market::currency_symbol(cur);
        let pnl = (price - h.avg_cost) * h.shares;
        let pnl_pct = if h.avg_cost > 0.0 {
            ((price / h.avg_cost) - 1.0) * 100.0
        } else {
            0.0
        };

        if pnl < 0.0 {
            found = true;
            println!(
                "  {:<14} {:>10} {:>10} {:>12} {:>10}",
                h.symbol.cyan(),
                format!("{}{:.2}", csym, h.avg_cost),
                format!("{}{:.2}", csym, price),
                format!("-{}{:.2}", csym, pnl.abs()).red(),
                format!("{:.2}%", pnl_pct).red(),
            );
        }
    }

    if !found {
        println!(
            "\n  {} No losing positions — nothing to harvest!",
            "✓".green()
        );
    } else {
        println!(
            "\n  {} Sell these positions to realize losses and offset capital gains.",
            "→".yellow()
        );
        println!(
            "  {}",
            "Consult a tax advisor. Wash sale rules may apply."
                .dimmed()
                .italic()
        );
    }
    println!();
    Ok(())
}

// ── Tax ──

fn money(csym: &str, amount: f64) -> String {
    if amount < 0.0 {
        format!("-{}{:.0}", csym, -amount)
    } else {
        format!("{}{:.0}", csym, amount)
    }
}

fn signed_tax_money(csym: &str, amount: f64) -> String {
    if amount >= 0.0 {
        format!("+{}{:.0}", csym, amount).green().to_string()
    } else {
        format!("-{}{:.0}", csym, -amount).red().to_string()
    }
}

/// Print the set-off and tax for one set of gains.
fn print_tax_estimate(country: Country, b: &tax::Buckets) {
    let e = tax::estimate(country, *b);
    let c = country.currency_symbol();
    let m = |x: f64| format!("{}{:.2}", c, x);
    if country.long_term_months().is_some() {
        print_kv(
            "Short-term gains / losses",
            &format!("{} / {}", m(b.st_gain), m(b.st_loss)),
        );
        print_kv(
            "Long-term gains / losses",
            &format!("{} / {}", m(b.lt_gain), m(b.lt_loss)),
        );
        if e.st_loss_against_lt > 0.0 {
            print_kv("ST loss set off vs LTCG", &m(e.st_loss_against_lt));
        }
        if e.lt_loss_against_st > 0.0 {
            print_kv("LT loss set off vs STCG", &m(e.lt_loss_against_st));
        }
        print_kv("Taxable STCG", &m(e.st_taxable));
        if country.exemption() > 0.0 {
            print_kv("LTCG after set-off", &m(e.lt_net));
            print_kv(
                &format!("LTCG exemption ({} a year)", money(c, country.exemption())),
                &format!("-{}", m(e.exemption_used)),
            );
        }
        print_kv("Taxable LTCG", &m(e.lt_taxable));
        print_kv(&format!("STCG tax ({}%)", country.st_rate()), &m(e.st_tax));
        print_kv(&format!("LTCG tax ({}%)", country.lt_rate()), &m(e.lt_tax));
    } else {
        print_kv(
            "Gains / losses",
            &format!(
                "{} / {}",
                m(b.st_gain + b.lt_gain),
                m(b.st_loss + b.lt_loss)
            ),
        );
        print_kv("Annual exempt amount used", &m(e.exemption_used));
        print_kv("Taxable gain", &m(e.st_taxable));
        print_kv(&format!("CGT ({}%)", country.st_rate()), &m(e.st_tax));
    }
    let total = e.total();
    let total_str = if total > 0.0 {
        m(total).red().bold().to_string()
    } else {
        m(total).green().to_string()
    };
    print_kv("TOTAL TAX", &total_str);
    if e.st_carry + e.lt_carry > 0.0 {
        print_kv(
            "Loss to carry forward",
            &if country.long_term_months().is_some() {
                format!("short-term {}, long-term {}", m(e.st_carry), m(e.lt_carry))
            } else {
                m(e.st_carry)
            },
        );
    }
}

/// Capital-gains estimate for the current tax year: gains realized by
/// `portfolio sell` (or a tradebook import) this year, plus what selling
/// everything today would add. Every lot uses its own purchase date.
pub async fn cmd_tax(country: &str, _market: Market) -> Result<()> {
    let Some(country) = Country::parse(country) else {
        bail!("Unsupported country '{}'. Use in, us, or uk", country);
    };
    let portfolio = Portfolio::load()?;
    let today = chrono::Local::now().date_naive();
    let (year_start, year_end) = country.tax_year(today);
    let in_year = |d: &str| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .is_ok_and(|d| d >= year_start && d <= year_end)
    };
    let year_sales: Vec<&pf::Sale> = portfolio
        .realized
        .iter()
        .filter(|s| in_year(&s.sell_date))
        .collect();
    if portfolio.holdings.is_empty() && year_sales.is_empty() {
        println!(
            "\n  {}",
            "Portfolio is empty and no sales are recorded this tax year.".dimmed()
        );
        return Ok(());
    }

    let quotes = if portfolio.holdings.is_empty() {
        Vec::new()
    } else {
        let symbols: Vec<&str> = portfolio
            .holdings
            .iter()
            .map(|h| h.symbol.as_str())
            .collect();
        YahooClient::new().await?.get_quote(&symbols).await?
    };
    let quote = |sym: &str| quotes.iter().find(|q| q.symbol.as_deref() == Some(sym));
    let currency = |sym: &str| {
        quote(sym)
            .and_then(|q| q.currency.clone())
            .or_else(|| pf::symbol_currency(sym).map(str::to_string))
    };
    let c = country.currency_symbol();
    let terms = country.long_term_months().is_some();
    let date = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
    let mut other_currency: Vec<String> = Vec::new();
    let mut estimated = false;

    print_header(&portfolio_title(&format!(
        "Tax Estimate — {} ({})",
        country.name(),
        country.tax_year_label(today)
    )));

    // ── Realized this tax year ──
    let mut realized = tax::Buckets::default();
    print_section(&format!(
        "Realized — sales {} to {}",
        year_start.format("%d %b %Y"),
        year_end.format("%d %b %Y")
    ));
    // Sales in this country's currency; others are named at the end.
    let mut taxed_sales = Vec::new();
    for s in &year_sales {
        match currency(&s.symbol).and_then(|cur| country.currency_scale(&cur)) {
            Some(scale) => taxed_sales.push((*s, scale)),
            None => other_currency.push(s.symbol.clone()),
        }
    }
    if taxed_sales.is_empty() {
        println!(
            "  {}",
            "No sales recorded this tax year. Record one with `stockwise portfolio sell`.".dimmed()
        );
    } else {
        println!(
            "  {:<14} {:>9} {:<12} {:<12} {:>12} {:>5}",
            "Symbol".bold(),
            "Qty".bold(),
            "Bought".bold(),
            "Sold".bold(),
            "Gain".bold(),
            if terms { "Term" } else { "" }.bold(),
        );
        for &(s, scale) in &taxed_sales {
            let long = match (date(&s.buy_date), date(&s.sell_date)) {
                (Some(b), Some(d)) => country.is_long_term(b, d),
                _ => false,
            };
            let gain = s.gain() * scale;
            realized.add(gain, long);
            estimated |= s.buy_date_estimated;
            println!(
                "  {:<14} {:>9.2} {:<12} {:<12} {:>12} {:>5}",
                s.symbol.cyan(),
                s.qty,
                format!(
                    "{}{}",
                    s.buy_date,
                    if s.buy_date_estimated { "*" } else { "" }
                ),
                s.sell_date,
                signed_tax_money(c, gain),
                if !terms {
                    ""
                } else if long {
                    "LT"
                } else {
                    "ST"
                },
            );
        }
    }

    // ── Unrealized: every open lot at today's price ──
    let mut unrealized = tax::Buckets::default();
    let mut unpriced = Vec::new();
    if !portfolio.holdings.is_empty() {
        print_section("Unrealized — if you sold everything today");
        if terms {
            println!(
                "  {:<14} {:>12} {:>12} {:>12} {:>12}",
                "Symbol".bold(),
                "Cost".bold(),
                "Value".bold(),
                "Short-term".bold(),
                "Long-term".bold(),
            );
        } else {
            println!(
                "  {:<14} {:>12} {:>12} {:>12}",
                "Symbol".bold(),
                "Cost".bold(),
                "Value".bold(),
                "Gain".bold(),
            );
        }
        for h in &portfolio.holdings {
            let Some(scale) = currency(&h.symbol).and_then(|cur| country.currency_scale(&cur))
            else {
                other_currency.push(h.symbol.clone());
                continue;
            };
            // A missing price must not count as a 100% capital loss.
            let Some(price) = quote(&h.symbol).and_then(|q| q.regular_market_price) else {
                unpriced.push(h.symbol.clone());
                continue;
            };
            let (mut st, mut lt) = (0.0, 0.0);
            for l in &h.lots {
                let gain = l.qty * (price - l.price) * scale;
                let long = date(&l.date).is_some_and(|d| country.is_long_term(d, today));
                unrealized.add(gain, long);
                if long {
                    lt += gain;
                } else {
                    st += gain;
                }
            }
            let mut label = h.symbol.clone();
            if h.has_estimated_dates() {
                label.push('*');
                estimated = true;
            }
            let cost = h.cost_basis() * scale;
            let value = h.shares * price * scale;
            if terms {
                println!(
                    "  {:<14} {:>12} {:>12} {:>12} {:>12}",
                    label.cyan(),
                    money(c, cost),
                    money(c, value),
                    signed_tax_money(c, st),
                    signed_tax_money(c, lt),
                );
            } else {
                println!(
                    "  {:<14} {:>12} {:>12} {:>12}",
                    label.cyan(),
                    money(c, cost),
                    money(c, value),
                    signed_tax_money(c, st + lt),
                );
            }
        }
    }

    print_section(&format!(
        "Estimated tax on gains realized in {}",
        country.tax_year_label(today)
    ));
    print_tax_estimate(country, &realized);
    if !portfolio.holdings.is_empty() {
        print_section("If you also sold everything today");
        print_tax_estimate(country, &realized.plus(unrealized));
    }

    if country == Country::India {
        let used = tax::estimate(country, realized).exemption_used;
        let left = country.exemption() - used;
        if left > 0.0 && unrealized.lt_gain > 0.0 {
            println!(
                "\n  {} {} of this year's {} LTCG exemption is unused — booking up to that much \
                 long-term gain this FY is tax-free.",
                "→".cyan(),
                money(c, left),
                money(c, country.exemption())
            );
        }
    }

    println!();
    other_currency.sort();
    other_currency.dedup();
    if !other_currency.is_empty() {
        println!(
            "  {} Not included (not in {}): {}. Other countries' shares follow other rules — in India, \
             foreign shares are long-term only after 24 months, and short-term gains are taxed at your slab rate.",
            "!".yellow(),
            country.tax_currency(),
            other_currency.join(", ")
        );
    }
    if !unpriced.is_empty() {
        println!(
            "  {} No live price for {} — excluded from this estimate.",
            "!".yellow(),
            unpriced.join(", ")
        );
    }
    if estimated {
        println!("  {}", ESTIMATED_DATE_HINT.dimmed());
    }
    let caveat = match country {
        Country::India => {
            "ESTIMATE for listed shares with STT paid (Sec. 111A/112A rates from 23 Jul 2024), FIFO lots. \
             Excludes cess and surcharge, pre-2018 grandfathering, brought-forward losses, and bonus-share \
             cost rules. Consult a chartered accountant."
        }
        Country::Us => {
            "ESTIMATE at top federal rates (37% short-term, 20% long-term), FIFO lots. Excludes NIIT, \
             state tax, wash sales and the $3,000 loss deduction. Consult a CPA."
        }
        Country::Uk => {
            "ESTIMATE at the 24% higher rate (18% for basic-rate taxpayers). HMRC uses share pooling \
             (Section 104) and the 30-day rule rather than FIFO. Consult an accountant."
        }
    };
    println!("  {}", caveat.dimmed().italic());
    println!();
    Ok(())
}

// ── Import from brokers ──

pub async fn cmd_divcal(market: Market) -> Result<()> {
    let client = YahooClient::new().await?;
    let portfolio = Portfolio::load()?;
    let wl = Watchlist::load()?;
    let mut all: Vec<String> = portfolio
        .holdings
        .iter()
        .map(|h| h.symbol.clone())
        .collect();
    for s in &wl.symbols {
        if !all.contains(s) {
            all.push(s.clone());
        }
    }
    if all.is_empty() {
        all = market.universe();
    }

    let refs: Vec<&str> = all.iter().map(|s| s.as_str()).collect();
    let quotes = client.get_quote(&refs).await?;

    print_header("Dividend Dashboard");
    print_dividends_received(&portfolio);
    let mut payers = Vec::new();
    for q in &quotes {
        let dy = q.trailing_annual_dividend_yield.unwrap_or(0.0);
        if dy > 0.001 {
            payers.push((
                q.symbol.as_deref().unwrap_or("???").to_string(),
                dy,
                q.regular_market_price.unwrap_or(0.0),
                q.currency.clone(),
            ));
        }
    }
    payers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if payers.is_empty() {
        println!("  {}", "No dividend-paying stocks found.".dimmed());
        return Ok(());
    }

    let csym = market.currency_symbol();
    println!(
        "  {:<14} {:>8} {:>12} {:>14}",
        "Symbol".bold(),
        "Yield".bold(),
        "Annual/Sh".bold(),
        "Income/₹1L".bold()
    );
    println!("  {}", "─".repeat(52).dimmed());

    let mut total_yield_weighted = 0.0;
    for (sym, dy, price, _cur) in &payers {
        let annual = dy * price;
        let income_1l = 100000.0 * dy;
        println!(
            "  {:<14} {:>8} {:>12} {:>14}",
            sym.cyan(),
            format!("{:.2}%", dy * 100.0).green(),
            format!("{}{:.2}", csym, annual),
            format!("{}{:.0}/yr", csym, income_1l)
        );
        total_yield_weighted += dy;
    }

    print_section("Portfolio Dividend Summary");
    let avg_yield = if !payers.is_empty() {
        total_yield_weighted / payers.len() as f64
    } else {
        0.0
    };
    print_kv(
        "Dividend Payers",
        &format!("{}/{}", payers.len(), all.len()),
    );
    print_kv("Avg Yield", &format!("{:.2}%", avg_yield * 100.0));
    print_kv(
        "Income on ₹10L",
        &format!(
            "{}{:.0}/year ({}{:.0}/month)",
            csym,
            1000000.0 * avg_yield,
            csym,
            1000000.0 * avg_yield / 12.0
        ),
    );
    println!();
    Ok(())
}

/// Dividends recorded by `portfolio actions --apply`, per currency: this
/// year (last 12 months) and in total.
fn print_dividends_received(portfolio: &Portfolio) {
    if portfolio.dividends.is_empty() {
        if !portfolio.holdings.is_empty() {
            println!(
                "  {}\n",
                "Record the dividends your holdings paid with `stockwise portfolio actions --apply`."
                    .dimmed()
            );
        }
        return;
    }
    let year_ago = (chrono::Local::now().date_naive() - chrono::Duration::days(365)).to_string();
    let mut by_currency: Vec<(Option<String>, f64, f64, usize)> = Vec::new();
    for d in &portfolio.dividends {
        let cur = d
            .currency
            .clone()
            .or_else(|| pf::symbol_currency(&d.symbol).map(str::to_string));
        let recent = if d.ex_date > year_ago { d.amount } else { 0.0 };
        match by_currency.iter_mut().find(|(c, ..)| *c == cur) {
            Some(row) => {
                row.1 += recent;
                row.2 += d.amount;
                row.3 += 1;
            }
            None => by_currency.push((cur, recent, d.amount, 1)),
        }
    }
    print_section("Dividends Received (recorded)");
    for (cur, recent, total, n) in &by_currency {
        // Label each currency when there is more than one.
        let suffix = match (by_currency.len(), cur) {
            (2.., Some(c)) => format!(" ({})", c),
            _ => String::new(),
        };
        print_kv(
            &format!("Last 12 months{}", suffix),
            &format_price(*recent, cur.as_deref()).green().to_string(),
        );
        print_kv(
            &format!("All recorded{}", suffix),
            &format!(
                "{} from {} payouts",
                format_price(*total, cur.as_deref()),
                n
            ),
        );
    }
    println!();
}

// ── Portfolio Correlation Dashboard ──

pub async fn cmd_pcorr(_market: Market) -> Result<()> {
    let portfolio = Portfolio::load()?;
    if portfolio.holdings.len() < 2 {
        println!("\n  {}", "Need 2+ holdings for correlation.".dimmed());
        return Ok(());
    }

    let client = YahooClient::new().await?;
    let mut all_closes: Vec<(String, Vec<f64>)> = Vec::new();
    for h in &portfolio.holdings {
        if let Ok(chart) = client.get_chart(&h.symbol, "6mo", "1d").await {
            let closes: Vec<f64> = chart
                .indicators
                .quote
                .first()
                .and_then(|q| q.close.as_ref())
                .map(|c| c.iter().filter_map(|v| *v).collect())
                .unwrap_or_default();
            if closes.len() > 20 {
                all_closes.push((h.symbol.clone(), closes));
            }
        }
    }

    if all_closes.len() < 2 {
        println!("  {}", "Not enough data.".dimmed());
        return Ok(());
    }

    print_header("Portfolio Correlation (6M)");
    print!("  {:<12}", "");
    for (sym, _) in &all_closes {
        print!(" {:>10}", sym.cyan());
    }
    println!();
    println!("  {}", "─".repeat(12 + all_closes.len() * 11).dimmed());

    let mut high_corr_pairs = Vec::new();
    for (i, (sym_a, ca)) in all_closes.iter().enumerate() {
        print!("  {:<12}", sym_a.cyan());
        for (j, (sym_b, cb)) in all_closes.iter().enumerate() {
            if i == j {
                print!(" {:>10}", "1.000".bold());
            } else {
                let corr = technical::correlation(ca, cb).unwrap_or(0.0);
                if corr > 0.7 && i < j {
                    high_corr_pairs.push((sym_a.clone(), sym_b.clone(), corr));
                }
                let c = if corr > 0.7 {
                    format!("{:.3}", corr).red().to_string()
                } else if corr > 0.3 {
                    format!("{:.3}", corr).yellow().to_string()
                } else {
                    format!("{:.3}", corr).to_string()
                };
                print!(" {:>10}", c);
            }
        }
        println!();
    }

    if !high_corr_pairs.is_empty() {
        print_section("Diversification Warning");
        for (a, b, c) in &high_corr_pairs {
            println!(
                "  {} {} + {} correlation {:.2} — consider reducing one",
                "⚠".yellow(),
                a.cyan(),
                b.cyan(),
                c
            );
        }
    } else {
        println!("\n  {} Portfolio is well-diversified.", "✓".green());
    }
    println!();
    Ok(())
}

// ── Returns Calculator ──

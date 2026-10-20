//! Your data: portfolio, watchlist, alerts, import/export, wealth history, rebalancing, tax, and dividends.

use super::*;

pub async fn cmd_portfolio(
    action: &str,
    symbol: Option<&str>,
    shares: Option<f64>,
    cost: Option<f64>,
    market: Market,
) -> Result<()> {
    let mut portfolio = Portfolio::load()?;

    match action {
        "add" => {
            let sym = symbol.context("Symbol is required")?;
            let resolved = market::resolve_symbol(sym, market);
            let shares = shares.context("Number of shares is required")?;
            let cost = cost.context("Cost per share is required")?;
            portfolio.add(&resolved, shares, cost);
            portfolio.save()?;
            println!(
                "  {} Added {} shares of {} at {:.2}",
                "✓".green(),
                shares,
                resolved,
                cost
            );
        }
        "remove" => {
            let sym = symbol.context("Symbol is required")?;
            let resolved = market::resolve_symbol(sym, market);
            if portfolio.remove(&resolved) {
                portfolio.save()?;
                println!("  {} Removed {} from portfolio", "✓".green(), resolved);
            } else {
                bail!("{} is not in your portfolio", resolved);
            }
        }
        "show" | "list" => {
            if portfolio.holdings.is_empty() {
                println!("\n  {}", t(Msg::PortfolioEmpty).dimmed());
                return Ok(());
            }

            // Fetch current prices for all holdings
            let symbols: Vec<String> = portfolio
                .holdings
                .iter()
                .map(|h| h.symbol.clone())
                .collect();
            let sym_refs: Vec<&str> = symbols.iter().map(|s| s.as_str()).collect();
            let client = YahooClient::new().await?;
            let quotes = client.get_quote(&sym_refs).await?;

            print_header(t(Msg::YourPortfolio));
            println!(
                "  {} {} {} {} {} {} {}",
                pad_end(t(Msg::Symbol), 12).bold(),
                pad_start(t(Msg::Shares), 8).bold(),
                pad_start(t(Msg::AvgCost), 10).bold(),
                pad_start(t(Msg::Price), 10).bold(),
                pad_start(t(Msg::Value), 12).bold(),
                pad_start(t(Msg::PnL), 10).bold(),
                pad_start(t(Msg::PnLPct), 10).bold(),
            );
            println!("  {}", "─".repeat(76).dimmed());

            let mut total_cost = 0.0;
            let mut total_value = 0.0;

            let mut unpriced: Vec<String> = Vec::new();
            for holding in &portfolio.holdings {
                let quote = quotes
                    .iter()
                    .find(|q| q.symbol.as_deref() == Some(&holding.symbol));
                let current_price = quote.and_then(|q| q.regular_market_price);
                let cur = quote.and_then(|q| q.currency.as_deref());
                let csym = market::currency_symbol(cur);

                // No live price (delisted, renamed, or a bad symbol): show N/A
                // and leave it out of the totals rather than valuing it at 0.
                let Some(price) = current_price else {
                    println!(
                        "  {:<12} {:>8.2} {:>10} {:>10} {:>12} {:>10} {:>10}",
                        holding.symbol.yellow(),
                        holding.shares,
                        format_price(holding.avg_cost, cur),
                        "N/A",
                        "N/A",
                        "—",
                        "—",
                    );
                    unpriced.push(holding.symbol.clone());
                    continue;
                };
                let value = holding.shares * price;
                let cost_basis = holding.shares * holding.avg_cost;
                let pnl = value - cost_basis;
                let pnl_pct = if cost_basis > 0.0 {
                    (pnl / cost_basis) * 100.0
                } else {
                    0.0
                };

                total_cost += cost_basis;
                total_value += value;

                let pnl_str = if pnl >= 0.0 {
                    format!("+{}{:.2}", csym, pnl).green().to_string()
                } else {
                    format!("-{}{:.2}", csym, pnl.abs()).red().to_string()
                };
                let pnl_pct_str = if pnl_pct >= 0.0 {
                    format!("+{:.2}%", pnl_pct).green().to_string()
                } else {
                    format!("{:.2}%", pnl_pct).red().to_string()
                };

                println!(
                    "  {:<12} {:>8.2} {:>10} {:>10} {:>12} {:>10} {:>10}",
                    holding.symbol.cyan(),
                    holding.shares,
                    format_price(holding.avg_cost, cur),
                    format_price(price, cur),
                    format_price(value, cur),
                    pnl_str,
                    pnl_pct_str,
                );
            }

            let total_pnl = total_value - total_cost;
            let total_pnl_pct = if total_cost > 0.0 {
                (total_pnl / total_cost) * 100.0
            } else {
                0.0
            };

            // Use first holding's currency for totals (mixed-currency portfolios just show numbers)
            let total_cur = quotes.first().and_then(|q| q.currency.as_deref());
            let tcsym = market::currency_symbol(total_cur);

            println!("  {}", "─".repeat(76).dimmed());
            let total_pnl_str = if total_pnl >= 0.0 {
                format!("+{}{:.2}", tcsym, total_pnl)
                    .green()
                    .bold()
                    .to_string()
            } else {
                format!("-{}{:.2}", tcsym, total_pnl.abs())
                    .red()
                    .bold()
                    .to_string()
            };
            let total_pnl_pct_str = if total_pnl_pct >= 0.0 {
                format!("+{:.2}%", total_pnl_pct).green().bold().to_string()
            } else {
                format!("{:.2}%", total_pnl_pct).red().bold().to_string()
            };
            println!(
                "  {} {:>8} {:>10} {:>10} {:>12} {:>10} {:>10}",
                pad_end(t(Msg::Total), 12).bold(),
                "",
                format_price(total_cost, total_cur),
                "",
                format_price(total_value, total_cur).bold(),
                total_pnl_str,
                total_pnl_pct_str,
            );
            if !unpriced.is_empty() {
                println!(
                    "\n  {} No live price for {} — excluded from totals. Check the symbol with `stockwise search`.",
                    "!".yellow(),
                    unpriced.join(", ")
                );
            }

            // Allocation chart
            if portfolio.holdings.len() > 1 {
                print_section("Allocation");
                let alloc_items: Vec<(&str, f64)> = portfolio
                    .holdings
                    .iter()
                    .map(|h| {
                        let price = quotes
                            .iter()
                            .find(|q| q.symbol.as_deref() == Some(&h.symbol))
                            .and_then(|q| q.regular_market_price)
                            .unwrap_or(0.0);
                        (h.symbol.as_str(), h.shares * price)
                    })
                    .collect();
                for line in charts::allocation_chart(&alloc_items) {
                    println!("{}", line);
                }
            }
            println!();
        }
        _ => bail!(
            "Unknown portfolio action '{}'. Use add, remove, or show",
            action
        ),
    }

    Ok(())
}

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

pub async fn cmd_import(source: &str, file: &str, market: Market) -> Result<()> {
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
            _ => {
                println!("    Provide a CSV with columns: Symbol, Quantity, Average Price");
                println!("    Run: stockwise import csv <path-to-file.csv>");
            }
        }
        bail!("File not found: {}", file);
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", file))?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() < 2 {
        bail!("{} is empty or has no data rows", file);
    }

    let header = lines[0].to_lowercase();
    let mut portfolio = Portfolio::load()?;
    let mut imported = 0;
    // An export is a full snapshot: the first row for a symbol replaces the
    // existing holding (so re-importing never doubles it); further rows for the
    // same symbol in this file are extra lots and are averaged in.
    let mut seen = std::collections::HashSet::new();

    match source {
        "kite" | "zerodha" => {
            // Kite CSV: Instrument, Qty., Avg. cost, LTP, Cur. val, P&L, Net chg., Day chg.
            let cols = parse_csv_header(&header);
            let sym_col = find_col(&cols, &["instrument", "tradingsymbol", "symbol", "stock"]);
            let qty_col = find_col(&cols, &["qty", "qty.", "quantity", "shares"]);
            let avg_col = find_col(
                &cols,
                &[
                    "avg. cost",
                    "avg cost",
                    "average cost",
                    "avg_price",
                    "average price",
                    "buy avg",
                    "buy avg.",
                ],
            );

            if sym_col.is_none() || qty_col.is_none() || avg_col.is_none() {
                bail!(
                    "Could not detect Kite CSV columns. Expected: Instrument, Qty., Avg. cost. Found: {}",
                    header
                );
            }
            let (si, qi, ai) = (sym_col.unwrap(), qty_col.unwrap(), avg_col.unwrap());

            for line in &lines[1..] {
                let fields = parse_csv_row(line);
                if fields.len() <= si.max(qi).max(ai) {
                    continue;
                }
                let symbol_raw = fields[si].trim().trim_matches('"');
                if symbol_raw.is_empty() {
                    continue;
                }
                let qty: f64 = fields[qi]
                    .trim()
                    .trim_matches('"')
                    .replace(',', "")
                    .parse()
                    .unwrap_or(0.0);
                let avg: f64 = fields[ai]
                    .trim()
                    .trim_matches('"')
                    .replace(',', "")
                    .parse()
                    .unwrap_or(0.0);
                if qty <= 0.0 || avg <= 0.0 {
                    continue;
                }

                let resolved = market::resolve_symbol(symbol_raw, market);
                if seen.insert(resolved.clone()) {
                    portfolio.set(&resolved, qty, avg);
                } else {
                    portfolio.add(&resolved, qty, avg);
                }
                imported += 1;
                println!(
                    "  {} {} — {} shares @ ₹{:.2}",
                    "✓".green(),
                    resolved.cyan(),
                    qty,
                    avg
                );
            }
        }
        "indmoney" => {
            // IndMoney: Stock Name, Symbol/ISIN, Quantity, Avg Buy Price, Current Price, ...
            let cols = parse_csv_header(&header);
            let sym_col = find_col(
                &cols,
                &[
                    "symbol",
                    "isin",
                    "stock symbol",
                    "scrip",
                    "stock name",
                    "name",
                ],
            );
            let qty_col = find_col(&cols, &["quantity", "qty", "shares", "units"]);
            let avg_col = find_col(
                &cols,
                &[
                    "avg buy price",
                    "avg price",
                    "average price",
                    "buy price",
                    "avg. buy price",
                    "average cost",
                ],
            );

            if sym_col.is_none() || qty_col.is_none() || avg_col.is_none() {
                bail!(
                    "Could not detect IndMoney CSV columns. Expected: Symbol/Stock Name, Quantity, Avg Buy Price. Found: {}",
                    header
                );
            }
            let (si, qi, ai) = (sym_col.unwrap(), qty_col.unwrap(), avg_col.unwrap());

            for line in &lines[1..] {
                let fields = parse_csv_row(line);
                if fields.len() <= si.max(qi).max(ai) {
                    continue;
                }
                let symbol_raw = fields[si].trim().trim_matches('"');
                if symbol_raw.is_empty() {
                    continue;
                }
                let qty: f64 = fields[qi]
                    .trim()
                    .trim_matches('"')
                    .replace(',', "")
                    .parse()
                    .unwrap_or(0.0);
                let avg: f64 = fields[ai]
                    .trim()
                    .trim_matches('"')
                    .replace(',', "")
                    .parse()
                    .unwrap_or(0.0);
                if qty <= 0.0 || avg <= 0.0 {
                    continue;
                }

                let resolved = market::resolve_symbol(symbol_raw, market);
                if seen.insert(resolved.clone()) {
                    portfolio.set(&resolved, qty, avg);
                } else {
                    portfolio.add(&resolved, qty, avg);
                }
                imported += 1;
                println!(
                    "  {} {} — {} shares @ ₹{:.2}",
                    "✓".green(),
                    resolved.cyan(),
                    qty,
                    avg
                );
            }
        }
        // "csv" or any unrecognized format falls back to generic CSV parsing
        _ => {
            // Generic CSV: try to find Symbol, Quantity, Price columns
            let cols = parse_csv_header(&header);
            let sym_col = find_col(
                &cols,
                &[
                    "symbol",
                    "stock",
                    "instrument",
                    "name",
                    "ticker",
                    "scrip",
                    "tradingsymbol",
                ],
            );
            let qty_col = find_col(&cols, &["quantity", "qty", "qty.", "shares", "units"]);
            let avg_col = find_col(
                &cols,
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
            );

            if sym_col.is_none() || qty_col.is_none() || avg_col.is_none() {
                bail!(
                    "Could not auto-detect CSV columns. Your CSV needs Symbol, Quantity and Average Price columns. Found: {}",
                    header
                );
            }
            let (si, qi, ai) = (sym_col.unwrap(), qty_col.unwrap(), avg_col.unwrap());

            for line in &lines[1..] {
                let fields = parse_csv_row(line);
                if fields.len() <= si.max(qi).max(ai) {
                    continue;
                }
                let symbol_raw = fields[si].trim().trim_matches('"');
                if symbol_raw.is_empty() {
                    continue;
                }
                let qty: f64 = fields[qi]
                    .trim()
                    .trim_matches('"')
                    .replace(',', "")
                    .parse()
                    .unwrap_or(0.0);
                let avg: f64 = fields[ai]
                    .trim()
                    .trim_matches('"')
                    .replace(',', "")
                    .parse()
                    .unwrap_or(0.0);
                if qty <= 0.0 || avg <= 0.0 {
                    continue;
                }

                let resolved = market::resolve_symbol(symbol_raw, market);
                if seen.insert(resolved.clone()) {
                    portfolio.set(&resolved, qty, avg);
                } else {
                    portfolio.add(&resolved, qty, avg);
                }
                imported += 1;
                println!(
                    "  {} {} — {} shares @ {:.2}",
                    "✓".green(),
                    resolved.cyan(),
                    qty,
                    avg
                );
            }
        }
    }

    if imported > 0 {
        portfolio.save()?;
        println!(
            "\n  {} Imported {} holdings into portfolio.",
            "✓".green().bold(),
            imported
        );
        println!(
            "  Run {} to see your portfolio.",
            "stockwise portfolio show".cyan()
        );
    } else {
        println!(
            "\n  {} No holdings found in file. Check the format.",
            "✗".red()
        );
    }
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

pub(super) fn find_col(cols: &[String], names: &[&str]) -> Option<usize> {
    for name in names {
        if let Some(i) = cols.iter().position(|c| c.contains(name)) {
            return Some(i);
        }
    }
    None
}

// ── Angel One Trading ──

pub async fn cmd_wealth(_market: Market) -> Result<()> {
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
    let sym_refs: Vec<&str> = symbols.iter().map(|s| s.as_str()).collect();
    let client = YahooClient::new().await?;
    let quotes = client.get_quote(&sym_refs).await?;

    let mut total_value = 0.0;
    let mut total_cost = 0.0;
    let mut unpriced = Vec::new();
    for h in &portfolio.holdings {
        match quotes
            .iter()
            .find(|q| q.symbol.as_deref() == Some(&h.symbol))
            .and_then(|q| q.regular_market_price)
        {
            Some(price) => {
                total_value += h.shares * price;
                total_cost += h.shares * h.avg_cost;
            }
            None => unpriced.push(h.symbol.as_str()),
        }
    }
    // A snapshot with a holding valued at 0 would show as a fake crash in the
    // history forever, so refuse to save one.
    if !unpriced.is_empty() {
        bail!(
            "No live price for {}; snapshot not saved. Fix or remove the holding, then retry.",
            unpriced.join(", ")
        );
    }

    // Save today's snapshot
    history.add_snapshot(total_value, total_cost, portfolio.holdings.len());
    history.save()?;

    let cur = quotes.first().and_then(|q| q.currency.as_deref());
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

pub async fn cmd_rebalance(targets: &[(String, f64)], _market: Market) -> Result<()> {
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

    let mut total_value = 0.0;
    let mut holdings_value: Vec<(String, f64, f64)> = Vec::new(); // symbol, value, price
    for h in &portfolio.holdings {
        let Some(price) = quotes
            .iter()
            .find(|q| q.symbol.as_deref() == Some(&h.symbol))
            .and_then(|q| q.regular_market_price)
        else {
            bail!(
                "No live price for {} — fix or remove it before rebalancing",
                h.symbol
            );
        };
        let value = h.shares * price;
        total_value += value;
        holdings_value.push((h.symbol.clone(), value, price));
    }

    let cur = quotes.first().and_then(|q| q.currency.as_deref());
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

// ── Wealth Tracker ──

/// Label for the tax year containing `today`. India runs April–March
/// ("FY 2026-27"); the UK year starts on 6 April; the US uses the calendar year.
pub(super) fn tax_year_label(country: &str, today: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    let (y, m, d) = (today.year(), today.month(), today.day());
    match country {
        "us" | "usa" => format!("Tax year {}", y),
        "uk" => {
            let start = if (m, d) >= (4, 6) { y } else { y - 1 };
            format!("Tax year {}-{:02}", start, (start + 1) % 100)
        }
        _ => {
            let start = if m >= 4 { y } else { y - 1 };
            format!("FY {}-{:02}", start, (start + 1) % 100)
        }
    }
}

pub async fn cmd_tax(country: &str, _market: Market) -> Result<()> {
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

    let mut total_stcg = 0.0_f64; // short-term capital gains
    let mut total_ltcg = 0.0_f64; // long-term capital gains
    let mut total_stcl = 0.0_f64;
    let mut total_ltcl = 0.0_f64;

    let (country_name, stcg_rate, ltcg_rate, ltcg_exempt, holding_months, currency_sym) =
        match country.to_lowercase().as_str() {
            "in" | "india" => ("India", 20.0, 12.5, 125000.0, 12, "₹"),
            "us" | "usa" => ("United States", 37.0, 20.0, 0.0, 12, "$"),
            "uk" => ("United Kingdom", 20.0, 20.0, 3000.0, 12, "£"),
            _ => bail!("Unsupported country '{}'. Use in, us, or uk", country),
        };

    print_header(&format!(
        "Tax Report — {} ({})",
        country_name,
        tax_year_label(country, chrono::Local::now().date_naive())
    ));

    println!(
        "  {:<14} {:>10} {:>10} {:>12} {:>8} {:>8}",
        "Symbol".bold(),
        "Cost".bold(),
        "Value".bold(),
        "Gain/Loss".bold(),
        "Type".bold(),
        "Tax".bold()
    );
    println!("  {}", "─".repeat(68).dimmed());

    let mut unpriced = Vec::new();
    for h in &portfolio.holdings {
        // A missing price must not count as a 100% capital loss.
        let Some(price) = quotes
            .iter()
            .find(|q| q.symbol.as_deref() == Some(&h.symbol))
            .and_then(|q| q.regular_market_price)
        else {
            unpriced.push(h.symbol.as_str());
            continue;
        };
        let cost = h.shares * h.avg_cost;
        let value = h.shares * price;
        let gain = value - cost;

        // Determine holding period from added_at
        let is_longterm = chrono::NaiveDate::parse_from_str(&h.added_at, "%Y-%m-%d")
            .ok()
            .map(|d| {
                let now = chrono::Local::now().date_naive();
                // Long-term = held strictly longer than the holding period.
                // Compare real dates so the day-of-month is respected.
                let threshold = d
                    .checked_add_months(chrono::Months::new(holding_months as u32))
                    .unwrap_or(d);
                now > threshold
            })
            .unwrap_or(false);

        let hold_type = if is_longterm { "LTCG" } else { "STCG" };
        let tax_rate = if is_longterm { ltcg_rate } else { stcg_rate };

        match (gain >= 0.0, is_longterm) {
            (true, true) => total_ltcg += gain,
            (true, false) => total_stcg += gain,
            (false, true) => total_ltcl += gain.abs(),
            (false, false) => total_stcl += gain.abs(),
        }

        let gain_str = if gain >= 0.0 {
            format!("+{}{:.2}", currency_sym, gain).green().to_string()
        } else {
            format!("-{}{:.2}", currency_sym, gain.abs())
                .red()
                .to_string()
        };
        let tax = if gain > 0.0 {
            gain * tax_rate / 100.0
        } else {
            0.0
        };
        let tax_str = if tax > 0.0 {
            format!("{}{:.0}", currency_sym, tax)
        } else {
            "—".dimmed().to_string()
        };

        println!(
            "  {:<14} {:>10} {:>10} {:>12} {:>8} {:>8}",
            h.symbol.cyan(),
            format!("{}{:.0}", currency_sym, cost),
            format!("{}{:.0}", currency_sym, value),
            gain_str,
            hold_type,
            tax_str
        );
    }

    println!("  {}", "─".repeat(68).dimmed());

    print_section("Tax Summary");
    print_kv(
        "Short-Term Gains",
        &format!("{}{:.2}", currency_sym, total_stcg),
    );
    print_kv(
        "Short-Term Losses",
        &format!("{}{:.2}", currency_sym, total_stcl),
    );
    print_kv(
        "Long-Term Gains",
        &format!("{}{:.2}", currency_sym, total_ltcg),
    );
    print_kv(
        "Long-Term Losses",
        &format!("{}{:.2}", currency_sym, total_ltcl),
    );

    // Net gains after offsetting losses
    let net_stcg = (total_stcg - total_stcl).max(0.0);
    let net_ltcg = (total_ltcg - total_ltcl - ltcg_exempt).max(0.0);

    let stcg_tax = net_stcg * stcg_rate / 100.0;
    let ltcg_tax = net_ltcg * ltcg_rate / 100.0;
    let total_tax = stcg_tax + ltcg_tax;

    print_section("Estimated Tax Liability");
    print_kv(
        &format!("STCG Tax ({}%)", stcg_rate),
        &format!("{}{:.2}", currency_sym, stcg_tax),
    );
    if ltcg_exempt > 0.0 {
        print_kv(
            "LTCG Exemption",
            &format!("{}{:.0}", currency_sym, ltcg_exempt),
        );
    }
    print_kv(
        &format!("LTCG Tax ({}%)", ltcg_rate),
        &format!("{}{:.2}", currency_sym, ltcg_tax),
    );
    println!("  {}", "─".repeat(40).dimmed());
    let total_str = if total_tax > 0.0 {
        format!("{}{:.2}", currency_sym, total_tax)
            .red()
            .bold()
            .to_string()
    } else {
        format!("{}{:.2}", currency_sym, total_tax)
            .green()
            .to_string()
    };
    print_kv("TOTAL TAX", &total_str);

    if total_stcl + total_ltcl > 0.0 {
        println!();
        print_kv(
            "Tax Saved (losses)",
            &format!(
                "{}{:.2}",
                currency_sym,
                (total_stcl * stcg_rate / 100.0 + total_ltcl * ltcg_rate / 100.0)
            )
            .green()
            .to_string(),
        );
    }

    println!(
        "\n  {}",
        "Tax rates are approximate. Consult a chartered accountant / CPA."
            .dimmed()
            .italic()
    );
    println!();
    if !unpriced.is_empty() {
        println!(
            "  {} No live price for {} — excluded from this estimate.\n",
            "!".yellow(),
            unpriced.join(", ")
        );
    }
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

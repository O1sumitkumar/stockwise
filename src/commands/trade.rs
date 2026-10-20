//! Live trading through Angel One, with order previews and confirmation.

use super::*;

/// Safety switches for commands that place live orders.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrderOpts {
    /// Skip the interactive confirmation (required when stdin is not a terminal).
    pub yes: bool,
    /// Resolve and preview the order, but never send it.
    pub dry_run: bool,
}

/// Resolve, preview, confirm, and place one live order. Each guard exists
/// because a typo in a shell command could otherwise spend real money.
pub(super) async fn place_live_order(
    side: &str,
    symbol: &str,
    qty: u32,
    limit: Option<f64>,
    opts: OrderOpts,
) -> Result<()> {
    if qty == 0 {
        bail!("Quantity must be at least 1");
    }
    if let Some(p) = limit
        && !(p.is_finite() && p > 0.0)
    {
        bail!("Limit price must be a positive number");
    }
    let sym = symbol.trim().to_uppercase();
    let sym = sym.trim_end_matches(".NS").trim_end_matches("-EQ");

    let client = crate::angel::AngelClient::new().await?;
    let instrument = client.resolve_equity(sym, "NSE").await?;

    // Reference price for the value estimate: the limit price, else last trade.
    let yahoo_sym = format!("{}.NS", sym);
    let last = match YahooClient::new().await {
        Ok(y) => y
            .get_quote(&[yahoo_sym.as_str()])
            .await
            .ok()
            .and_then(|q| q.first().and_then(|q| q.regular_market_price)),
        Err(_) => None,
    };
    let est_value = limit.or(last).map(|p| p * qty as f64);

    print_header(&format!("{} order — REAL MONEY", side));
    print_kv(
        "Instrument",
        &format!(
            "{} ({}, token {})",
            instrument.tradingsymbol, instrument.exchange, instrument.token
        ),
    );
    print_kv("Side", side);
    print_kv("Quantity", &qty.to_string());
    match limit {
        Some(p) => print_kv("Type", &format!("LIMIT @ ₹{:.2}", p)),
        None => print_kv("Type", "MARKET"),
    }
    if let Some(l) = last {
        print_kv("Last price", &format!("₹{:.2}", l));
    }
    print_kv(
        "Est. value",
        &est_value.map_or("unknown".to_string(), |v| format!("₹{:.2}", v)),
    );
    println!();

    if let Some(cap) = crate::angel::AngelConfig::load()?.max_order_value {
        match est_value {
            Some(v) if v <= cap => {}
            Some(v) => bail!(
                "Estimated value ₹{:.2} exceeds max_order_value ₹{:.2} in your Angel One config",
                v,
                cap
            ),
            None => bail!(
                "Cannot estimate the order value, so max_order_value (₹{:.2}) cannot be checked. Use a limit order.",
                cap
            ),
        }
    }

    if opts.dry_run {
        println!("  {} Dry run — no order sent.\n", "✓".green());
        return Ok(());
    }
    if !opts.yes && !confirm_live_order()? {
        println!("  Cancelled — no order sent.\n");
        return Ok(());
    }

    let order_type = if limit.is_some() { "LIMIT" } else { "MARKET" };
    let result = client
        .place_order(
            &instrument,
            side,
            qty,
            order_type,
            limit.unwrap_or(0.0),
            0.0,
        )
        .await?;

    match &result.orderid {
        Some(id) => println!("  {} Order placed! ID: {}", "✓".green().bold(), id.cyan()),
        None => println!(
            "  {} Order accepted but no order id was returned — {}",
            "⚠".yellow().bold(),
            "do NOT resubmit; verify with `stockwise trade orders`.".yellow()
        ),
    }
    println!();
    Ok(())
}

/// Ask the user to type `yes`. Refuses (errors) when stdin is not a terminal,
/// so scripts must opt in with `--yes`.
pub(super) fn confirm_live_order() -> Result<bool> {
    use std::io::{BufRead, IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        bail!(
            "Refusing to place a live order without confirmation. Run it in a terminal, or pass --yes."
        );
    }
    print!(
        "  Type {} to place this order (anything else cancels): ",
        "yes".bold()
    );
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim() == "yes")
}

pub async fn cmd_trade(
    action: &str,
    symbol: Option<&str>,
    qty: Option<u32>,
    price: Option<f64>,
    opts: OrderOpts,
    _market: Market,
) -> Result<()> {
    match action {
        "setup" => {
            let path = crate::store::path(crate::angel::CONFIG_FILE)?;
            println!();
            print_header("Angel One SmartAPI Setup");
            println!("  To connect your Angel One account, you need:");
            println!();
            println!(
                "  1. Go to {} and create an app",
                "smartapi.angelone.in".cyan()
            );
            println!(
                "  2. Note your {} (from the app dashboard)",
                "API Key".bold()
            );
            println!("  3. Your {} (Angel One login ID)", "Client ID".bold());
            println!("  4. Your {} (Angel One login PIN)", "Password".bold());
            println!(
                "  5. Your {} (from Authenticator app / Angel One TOTP setup)",
                "TOTP Secret".bold()
            );
            println!();
            println!("  Then run:");
            println!("    {} stockwise trade config", "$".dimmed());
            println!("  and fill in the file it creates:");
            println!("    {}", path.display().to_string().cyan());
            println!();
            println!(
                "  {}",
                "Never pass credentials as command-line arguments — they end up in shell history."
                    .dimmed()
            );
            println!(
                "  {}",
                "The file is readable only by you and is sent nowhere except Angel One's API."
                    .dimmed()
            );
            println!(
                "  {}",
                "Optional: add \"max_order_value\": 50000 to cap the value of any single order."
                    .dimmed()
            );
            println!();
            Ok(())
        }
        "config" => {
            let path = crate::store::path(crate::angel::CONFIG_FILE)?;
            let config = crate::angel::AngelConfig::load()?;
            println!();
            if config.is_configured() {
                println!(
                    "  {} Configured for client {} — {}",
                    "✓".green(),
                    config.client_id.cyan(),
                    path.display()
                );
            } else {
                if !path.exists() {
                    crate::angel::AngelConfig::template().save()?;
                    println!(
                        "  {} Created a credentials template (owner-only access).",
                        "✓".green()
                    );
                } else {
                    println!(
                        "  {} Credentials file still has placeholder or empty values.",
                        "⚠".yellow()
                    );
                }
                println!("  Edit it with your API key, client ID, PIN and TOTP secret:");
                println!("    {}", path.display().to_string().cyan());
            }
            println!();
            Ok(())
        }
        "holdings" => {
            let client = crate::angel::AngelClient::new().await?;
            let holdings = client.get_holdings().await?;

            if holdings.is_empty() {
                println!("\n  {}", "No holdings found.".dimmed());
                return Ok(());
            }

            print_header("Angel One — Holdings (Live)");
            println!(
                "  {:<14} {:>8} {:>10} {:>10} {:>12} {:>10}",
                "Symbol".bold(),
                "Qty".bold(),
                "Avg Cost".bold(),
                "LTP".bold(),
                "P&L".bold(),
                "P&L %".bold()
            );
            println!("  {}", "─".repeat(68).dimmed());

            let mut total_pnl = 0.0_f64;
            let mut total_value = 0.0_f64;
            let mut total_cost = 0.0_f64;

            for h in &holdings {
                let sym = h.tradingsymbol.as_deref().unwrap_or("???");
                let qty = h.quantity.unwrap_or(0);
                let avg = h.averageprice.unwrap_or(0.0);
                let ltp = h.ltp.unwrap_or(0.0);
                let pnl = h.pnl.unwrap_or(0.0);
                let pnl_pct = h.pnlpercentage.unwrap_or(0.0);
                total_pnl += pnl;
                total_value += ltp * qty as f64;
                total_cost += avg * qty as f64;

                let pnl_str = if pnl >= 0.0 {
                    format!("+₹{:.2}", pnl).green().to_string()
                } else {
                    format!("-₹{:.2}", pnl.abs()).red().to_string()
                };
                let pct_str = if pnl_pct >= 0.0 {
                    format!("+{:.2}%", pnl_pct).green().to_string()
                } else {
                    format!("{:.2}%", pnl_pct).red().to_string()
                };

                println!(
                    "  {:<14} {:>8} {:>10} {:>10} {:>12} {:>10}",
                    sym.cyan(),
                    qty,
                    format!("₹{:.2}", avg),
                    format!("₹{:.2}", ltp),
                    pnl_str,
                    pct_str
                );
            }

            println!("  {}", "─".repeat(68).dimmed());
            let total_str = if total_pnl >= 0.0 {
                format!("+₹{:.2}", total_pnl).green().bold().to_string()
            } else {
                format!("-₹{:.2}", total_pnl.abs()).red().bold().to_string()
            };
            println!(
                "  {:<14} {:>8} {:>10} {:>10} {:>12}",
                "TOTAL".bold(),
                "",
                format!("₹{:.0}", total_cost),
                format!("₹{:.0}", total_value),
                total_str
            );

            // Also sync to stockwise portfolio
            let mut portfolio = Portfolio::load()?;
            for h in &holdings {
                let sym = h.tradingsymbol.as_deref().unwrap_or("");
                let qty = h.quantity.unwrap_or(0) as f64;
                let avg = h.averageprice.unwrap_or(0.0);
                if !sym.is_empty() && qty > 0.0 && avg > 0.0 {
                    // Broker symbols carry a series suffix (RELIANCE-EQ); Yahoo
                    // wants RELIANCE.NS.
                    let base = sym.rsplit_once('-').map_or(sym, |(b, _)| b);
                    let resolved = if base.contains('.') {
                        base.to_string()
                    } else {
                        format!("{}.NS", base)
                    };
                    // Overwrite quantity/cost but keep the original added date.
                    portfolio.set(&resolved, qty, avg);
                }
            }
            portfolio.save()?;
            println!(
                "\n  {} Holdings synced to StockWise portfolio.",
                "✓".green()
            );
            println!();
            Ok(())
        }
        "positions" => {
            let client = crate::angel::AngelClient::new().await?;
            let positions = client.get_positions().await?;

            if positions.is_empty() {
                println!("\n  {}", "No open positions.".dimmed());
                return Ok(());
            }

            print_header("Angel One — Open Positions");
            println!(
                "  {:<14} {:>8} {:>10} {:>10} {:>12}",
                "Symbol".bold(),
                "Qty".bold(),
                "Buy Avg".bold(),
                "LTP".bold(),
                "P&L".bold()
            );
            println!("  {}", "─".repeat(58).dimmed());

            for p in &positions {
                let sym = p.tradingsymbol.as_deref().unwrap_or("???");
                let qty = p.quantity.as_deref().unwrap_or("0");
                let avg = p.buyavgprice.as_deref().unwrap_or("0");
                let ltp = p.ltp.as_deref().unwrap_or("0");
                let pnl = p.pnl.as_deref().unwrap_or("0");
                let pnl_f: f64 = pnl.parse().unwrap_or(0.0);
                let pnl_str = if pnl_f >= 0.0 {
                    format!("+₹{:.2}", pnl_f).green().to_string()
                } else {
                    format!("-₹{:.2}", pnl_f.abs()).red().to_string()
                };
                println!(
                    "  {:<14} {:>8} {:>10} {:>10} {:>12}",
                    sym.cyan(),
                    qty,
                    format!("₹{}", avg),
                    format!("₹{}", ltp),
                    pnl_str
                );
            }
            println!();
            Ok(())
        }
        "buy" | "sell" | "limit" => {
            let sym = symbol.context("Symbol required, e.g. stockwise trade buy RELIANCE 10")?;
            let quantity =
                qty.context("Quantity required, e.g. stockwise trade buy RELIANCE 10")?;
            let (side, limit) = match action {
                "buy" => ("BUY", None),
                "sell" => ("SELL", None),
                _ => (
                    "BUY",
                    Some(price.context(
                        "Limit price required, e.g. stockwise trade limit RELIANCE 10 1300",
                    )?),
                ),
            };
            place_live_order(side, sym, quantity, limit, opts).await
        }
        "orders" => {
            let client = crate::angel::AngelClient::new().await?;
            let orders = client.get_order_book().await?;

            if orders.is_empty() {
                println!("\n  {}", "No orders today.".dimmed());
                return Ok(());
            }

            print_header("Angel One — Order Book");
            println!(
                "  {:<12} {:<14} {:>6} {:>8} {:>10} {:>12}",
                "Order ID".bold(),
                "Symbol".bold(),
                "Type".bold(),
                "Qty".bold(),
                "Price".bold(),
                "Status".bold()
            );
            println!("  {}", "─".repeat(66).dimmed());

            for o in &orders {
                let id = o.orderid.as_deref().unwrap_or("???");
                let sym = o.tradingsymbol.as_deref().unwrap_or("???");
                let txn = o.transactiontype.as_deref().unwrap_or("?");
                let qty = o.quantity.as_deref().unwrap_or("0");
                let price = o.price.as_deref().unwrap_or("0");
                let status = o.status.as_deref().unwrap_or("???");
                let txn_str = if txn == "BUY" {
                    txn.green().to_string()
                } else {
                    txn.red().to_string()
                };
                let status_str = match status {
                    "complete" => status.green().bold().to_string(),
                    "rejected" => status.red().to_string(),
                    "open" | "pending" => status.yellow().to_string(),
                    _ => status.to_string(),
                };
                println!(
                    "  {:<12} {:<14} {:>6} {:>8} {:>10} {:>12}",
                    id.dimmed(),
                    sym.cyan(),
                    txn_str,
                    qty,
                    format!("₹{}", price),
                    status_str
                );
            }
            println!();
            Ok(())
        }
        _ => {
            println!();
            println!("  {} Angel One Trading Commands:", "→".cyan());
            println!();
            println!(
                "    {} — Set up Angel One API credentials",
                "stockwise trade setup".bold()
            );
            println!(
                "    {} — Create config file to edit",
                "stockwise trade config".bold()
            );
            println!(
                "    {} — View your live holdings",
                "stockwise trade holdings".bold()
            );
            println!(
                "    {} — View open intraday positions",
                "stockwise trade positions".bold()
            );
            println!(
                "    {} — Place a market buy order",
                "stockwise trade buy RELIANCE 10".bold()
            );
            println!(
                "    {} — Place a market sell order",
                "stockwise trade sell RELIANCE 10".bold()
            );
            println!(
                "    {} — Place a limit buy order",
                "stockwise trade limit RELIANCE 10 1300".bold()
            );
            println!(
                "    {} — View today's order book",
                "stockwise trade orders".bold()
            );
            println!();
            Ok(())
        }
    }
}

// ── Support & Resistance ──

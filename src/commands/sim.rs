//! Paper-trading simulator.

use super::*;

pub async fn cmd_sim(
    action: &str,
    amount: Option<f64>,
    target: Option<f64>,
    market: Market,
) -> Result<()> {
    let csym = match market {
        Market::In => "₹",
        Market::Us => "$",
    };

    match action {
        "start" => {
            let capital = amount.unwrap_or(25000.0);
            let target_pct = target.unwrap_or(2.0);
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();

            let mut history = SimHistory::load()?;
            if history
                .sessions
                .iter()
                .any(|s| s.date == today && !s.settled)
            {
                println!(
                    "  {} Already have an open simulation for today.",
                    "!".yellow()
                );
                println!(
                    "  Run {} to settle it, or {} to check status.",
                    "stockwise sim settle".cyan(),
                    "stockwise sim status".cyan()
                );
                return Ok(());
            }

            print_header("Paper Trading Simulation — Starting");
            println!(
                "  Date: {}  |  Capital: {}{:.0}  |  Target: {}%\n",
                today.bold(),
                csym,
                capital,
                target_pct
            );
            println!("  {} Running intraday bot scan...\n", "⟳".yellow());

            let client = YahooClient::new().await?;
            let signals = intraday::scan_intraday(&client, market).await?;
            let plans = intraday::generate_trade_plans(&signals, capital, target_pct, 1.0);

            if plans.is_empty() {
                println!(
                    "  {} No trades qualified today. Try tomorrow.",
                    "→".yellow()
                );
                return Ok(());
            }

            let sim_trades: Vec<SimTrade> = plans
                .iter()
                .take(5)
                .map(|p| SimTrade {
                    symbol: p.signal.symbol.clone(),
                    direction: p.signal.direction.to_string(),
                    entry_price: p.entry,
                    target1: p.target1,
                    target2: p.target2,
                    stop_loss: p.stop_loss,
                    qty: p.qty,
                    capital: p.capital_required,
                    score: p.signal.score,
                    confidence: p.signal.confidence.to_string(),
                    strategies: p
                        .signal
                        .strategies
                        .iter()
                        .map(|s| s.name.to_string())
                        .collect(),
                    exit_price: None,
                    pnl: None,
                    pnl_pct: None,
                    hit_target: None,
                    hit_stop: None,
                })
                .collect();

            println!("  {} Simulation trades locked in:\n", "✓".green().bold());
            println!(
                "  {:<14} {:>6} {:>10} {:>10} {:>10} {:>6}",
                "Symbol".bold(),
                "Qty".bold(),
                "Entry".bold(),
                "Target".bold(),
                "Stop".bold(),
                "Score".bold()
            );
            println!("  {}", "─".repeat(60).dimmed());
            for t in &sim_trades {
                println!(
                    "  {:<14} {:>6} {:>10} {:>10} {:>10} {:>6}",
                    t.symbol.cyan(),
                    t.qty,
                    format!("{}{:.2}", csym, t.entry_price),
                    format!("{}{:.2}", csym, t.target2).green(),
                    format!("{}{:.2}", csym, t.stop_loss).red(),
                    format!("{:.0}", t.score)
                );
            }

            history.sessions.push(SimSession {
                date: today,
                market: match market {
                    Market::In => "IN",
                    Market::Us => "US",
                }
                .into(),
                capital,
                target_pct,
                trades: sim_trades,
                total_pnl: None,
                total_pnl_pct: None,
                win_count: None,
                loss_count: None,
                settled: false,
            });
            history.save()?;

            println!(
                "\n  {} Trades recorded. Run {} at EOD to see results.",
                "✓".green(),
                "stockwise sim settle".cyan()
            );
            println!(
                "  {} Run {} anytime to check live P&L.",
                "→".dimmed(),
                "stockwise sim status".cyan()
            );
            println!();
        }
        "status" => {
            let history = SimHistory::load()?;
            let open: Vec<&SimSession> = history.sessions.iter().filter(|s| !s.settled).collect();
            if open.is_empty() {
                println!(
                    "\n  {} No open simulations. Run {}.",
                    "→".dimmed(),
                    "stockwise sim start".cyan()
                );
                return Ok(());
            }

            let client = YahooClient::new().await?;
            for session in &open {
                print_header(&format!("Simulation Status — {}", session.date));
                println!(
                    "  Capital: {}{:.0}  |  Target: {}%\n",
                    csym, session.capital, session.target_pct
                );
                let syms: Vec<&str> = session.trades.iter().map(|t| t.symbol.as_str()).collect();
                let quotes = client.get_quote(&syms).await?;

                println!(
                    "  {:<14} {:>6} {:>10} {:>10} {:>12} {:>10}",
                    "Symbol".bold(),
                    "Qty".bold(),
                    "Entry".bold(),
                    "Now".bold(),
                    "P&L".bold(),
                    "Status".bold()
                );
                println!("  {}", "─".repeat(66).dimmed());

                let mut total_pnl = 0.0_f64;
                for t in &session.trades {
                    let current = quotes
                        .iter()
                        .find(|q| q.symbol.as_deref() == Some(&t.symbol))
                        .and_then(|q| q.regular_market_price)
                        .unwrap_or(t.entry_price);
                    let is_long = t.direction == "BUY";
                    let pnl = if is_long {
                        (current - t.entry_price) * t.qty as f64
                    } else {
                        (t.entry_price - current) * t.qty as f64
                    };
                    total_pnl += pnl;
                    let hit_t2 = if is_long {
                        current >= t.target2
                    } else {
                        current <= t.target2
                    };
                    let hit_stop = if is_long {
                        current <= t.stop_loss
                    } else {
                        current >= t.stop_loss
                    };
                    let hit_t1 = if is_long {
                        current >= t.target1
                    } else {
                        current <= t.target1
                    };
                    let status = if hit_t2 {
                        "TARGET HIT".green().bold().to_string()
                    } else if hit_stop {
                        "STOPPED".red().bold().to_string()
                    } else if hit_t1 {
                        "T1 hit".green().to_string()
                    } else {
                        "Open".yellow().to_string()
                    };
                    let pnl_str = if pnl >= 0.0 {
                        format!("+{}{:.0}", csym, pnl).green().to_string()
                    } else {
                        format!("-{}{:.0}", csym, pnl.abs()).red().to_string()
                    };
                    println!(
                        "  {:<14} {:>6} {:>10} {:>10} {:>12} {:>10}",
                        t.symbol.cyan(),
                        t.qty,
                        format!("{}{:.2}", csym, t.entry_price),
                        format!("{}{:.2}", csym, current),
                        pnl_str,
                        status
                    );
                }
                println!("  {}", "─".repeat(66).dimmed());
                let total_str = if total_pnl >= 0.0 {
                    format!("+{}{:.0}", csym, total_pnl)
                        .green()
                        .bold()
                        .to_string()
                } else {
                    format!("-{}{:.0}", csym, total_pnl.abs())
                        .red()
                        .bold()
                        .to_string()
                };
                let target_profit = session.capital * (session.target_pct / 100.0);
                let on_track = if total_pnl >= target_profit {
                    "TARGET MET".green().bold().to_string()
                } else {
                    format!("{}{:.0} to go", csym, target_profit - total_pnl)
                        .yellow()
                        .to_string()
                };
                println!("  Total: {}  |  {}", total_str, on_track);
            }
            println!();
        }
        "settle" => {
            let mut history = SimHistory::load()?;
            let client = YahooClient::new().await?;
            let mut settled_any = false;

            for session in history.sessions.iter_mut().filter(|s| !s.settled) {
                let syms: Vec<&str> = session.trades.iter().map(|t| t.symbol.as_str()).collect();
                let quotes = client.get_quote(&syms).await?;
                let mut total_pnl = 0.0_f64;
                let (mut wins, mut losses) = (0u32, 0u32);

                print_header(&format!("Settling — {}", session.date));
                println!(
                    "  {:<14} {:>10} {:>10} {:>12} {:>8}",
                    "Symbol".bold(),
                    "Entry".bold(),
                    "Close".bold(),
                    "P&L".bold(),
                    "Result".bold()
                );
                println!("  {}", "─".repeat(58).dimmed());

                for trade in session.trades.iter_mut() {
                    // Settling at a made-up price would write a fake result
                    // into the track record; nothing is saved before this loop.
                    let Some(current) = quotes
                        .iter()
                        .find(|q| q.symbol.as_deref() == Some(&trade.symbol))
                        .and_then(|q| q.regular_market_price)
                    else {
                        bail!(
                            "No live price for {}; session not settled. Try again later.",
                            trade.symbol
                        );
                    };
                    let is_long = trade.direction == "BUY";
                    let pnl = if is_long {
                        (current - trade.entry_price) * trade.qty as f64
                    } else {
                        (trade.entry_price - current) * trade.qty as f64
                    };
                    let pnl_pct = if trade.entry_price != 0.0 {
                        let move_pct = (current - trade.entry_price) / trade.entry_price * 100.0;
                        if is_long { move_pct } else { -move_pct }
                    } else {
                        0.0
                    };
                    let hit_target = if is_long {
                        current >= trade.target1
                    } else {
                        current <= trade.target1
                    };
                    let hit_stop = if is_long {
                        current <= trade.stop_loss
                    } else {
                        current >= trade.stop_loss
                    };
                    trade.exit_price = Some(current);
                    trade.pnl = Some(pnl);
                    trade.pnl_pct = Some(pnl_pct);
                    trade.hit_target = Some(hit_target);
                    trade.hit_stop = Some(hit_stop);
                    total_pnl += pnl;
                    if pnl > 0.0 {
                        wins += 1;
                    } else {
                        losses += 1;
                    }
                    let result = if hit_target {
                        "WIN".green().bold().to_string()
                    } else if hit_stop {
                        "STOPPED".red().to_string()
                    } else if pnl > 0.0 {
                        "Profit".green().to_string()
                    } else {
                        "Loss".red().to_string()
                    };
                    let pnl_str = if pnl >= 0.0 {
                        format!("+{}{:.0}", csym, pnl).green().to_string()
                    } else {
                        format!("-{}{:.0}", csym, pnl.abs()).red().to_string()
                    };
                    println!(
                        "  {:<14} {:>10} {:>10} {:>12} {:>8}",
                        trade.symbol.cyan(),
                        format!("{}{:.2}", csym, trade.entry_price),
                        format!("{}{:.2}", csym, current),
                        pnl_str,
                        result
                    );
                }

                let total_pnl_pct = (total_pnl / session.capital) * 100.0;
                session.total_pnl = Some(total_pnl);
                session.total_pnl_pct = Some(total_pnl_pct);
                session.win_count = Some(wins);
                session.loss_count = Some(losses);
                session.settled = true;
                settled_any = true;

                println!("  {}", "─".repeat(58).dimmed());
                let verdict = if total_pnl >= session.capital * (session.target_pct / 100.0) {
                    format!(
                        "TARGET MET — {}{:.0} ({:+.2}%)",
                        csym, total_pnl, total_pnl_pct
                    )
                    .green()
                    .bold()
                    .to_string()
                } else if total_pnl > 0.0 {
                    format!(
                        "Partial win — {}{:.0} ({:+.2}%)",
                        csym, total_pnl, total_pnl_pct
                    )
                    .yellow()
                    .to_string()
                } else {
                    format!(
                        "Loss — {}{:.0} ({:.2}%)",
                        csym,
                        total_pnl.abs(),
                        total_pnl_pct
                    )
                    .red()
                    .to_string()
                };
                println!("  {} {}", "→".bold(), verdict);
            }
            if !settled_any {
                println!("\n  {} No open simulations.", "→".dimmed());
            } else {
                history.save()?;
                println!(
                    "\n  {} Run {} for stats.",
                    "→".dimmed(),
                    "stockwise sim history".cyan()
                );
            }
            println!();
        }
        "history" | "stats" => {
            let history = SimHistory::load()?;
            let stats = history.stats();
            if stats.total_days == 0 {
                println!(
                    "\n  {} No history. Run {}.",
                    "→".dimmed(),
                    "stockwise sim start".cyan()
                );
                return Ok(());
            }

            print_header("Paper Trading Performance");
            let daily_pnls: Vec<f64> = history
                .sessions
                .iter()
                .filter(|s| s.settled)
                .filter_map(|s| s.total_pnl)
                .collect();
            if daily_pnls.len() > 1 {
                let mut equity = vec![0.0_f64];
                for &pnl in &daily_pnls {
                    equity.push(equity.last().unwrap() + pnl);
                }
                let color = if *equity.last().unwrap() >= 0.0 {
                    "green"
                } else {
                    "red"
                };
                for line in charts::line_chart(&equity, 50, 8, color, "Cumulative P&L") {
                    println!("{}", line);
                }
                println!();
                println!("  {}", "Daily P&L".bold());
                for s in history.sessions.iter().filter(|s| s.settled) {
                    let pnl = s.total_pnl.unwrap_or(0.0);
                    let bar_len = (pnl.abs()
                        / stats
                            .best_day
                            .unwrap_or(1.0)
                            .abs()
                            .max(stats.worst_day.unwrap_or(1.0).abs())
                        * 20.0)
                        .min(20.0) as usize;
                    let bar = if pnl >= 0.0 {
                        "█".repeat(bar_len.max(1)).green().to_string()
                    } else {
                        "█".repeat(bar_len.max(1)).red().to_string()
                    };
                    let ps = if pnl >= 0.0 {
                        format!("+{}{:.0}", csym, pnl).green().to_string()
                    } else {
                        format!("-{}{:.0}", csym, pnl.abs()).red().to_string()
                    };
                    println!("  {} {} {}", s.date.dimmed(), bar, ps);
                }
            }

            print_section("Performance");
            print_kv("Days Simulated", &stats.total_days.to_string());
            print_kv(
                "Winning Days",
                &format!("{} ({:.0}%)", stats.winning_days, stats.day_win_rate),
            );
            let ts = if stats.total_pnl >= 0.0 {
                format!("+{}{:.0}", csym, stats.total_pnl)
                    .green()
                    .bold()
                    .to_string()
            } else {
                format!("-{}{:.0}", csym, stats.total_pnl.abs())
                    .red()
                    .bold()
                    .to_string()
            };
            print_kv("Total P&L", &ts);
            print_kv(
                "Avg Daily P&L",
                &format!("{}{:.0}", csym, stats.avg_daily_pnl),
            );
            if let Some(b) = stats.best_day {
                print_kv(
                    "Best Day",
                    &format!("+{}{:.0}", csym, b).green().to_string(),
                );
            }
            if let Some(w) = stats.worst_day {
                print_kv("Worst Day", &format!("{}{:.0}", csym, w).red().to_string());
            }
            print_kv("Total Trades", &stats.total_trades.to_string());
            print_kv("Trade Win Rate", &format!("{:.0}%", stats.trade_win_rate));
            print_kv("Max Win Streak", &format!("{} days", stats.max_win_streak));
            print_kv(
                "Max Loss Streak",
                &format!("{} days", stats.max_loss_streak),
            );
            if let Some(s) = stats.sharpe {
                print_kv("Sharpe (ann.)", &format!("{:.2}", s));
            }

            print_section("Bot Confidence");
            if stats.total_days >= 5 {
                let conf = if stats.day_win_rate >= 65.0
                    && stats.trade_win_rate >= 55.0
                    && stats.total_pnl > 0.0
                {
                    "HIGH — Bot is consistently profitable. Consider going live."
                        .green()
                        .bold()
                        .to_string()
                } else if stats.day_win_rate >= 50.0 && stats.total_pnl > 0.0 {
                    "MODERATE — Profitable but inconsistent. Keep testing."
                        .yellow()
                        .to_string()
                } else {
                    "LOW — Underperforming. Review strategy.".red().to_string()
                };
                println!("  {}", conf);
            } else {
                println!(
                    "  {} Need 5+ days for confidence rating ({}/5)",
                    "→".dimmed(),
                    stats.total_days
                );
            }
            println!();
        }
        "reset" => return cmd_sim_reset(false),
        _ => {
            println!();
            println!("  {} Paper Trading Simulator:", "→".cyan());
            println!();
            println!(
                "    {} — Lock in today's bot trades",
                "stockwise sim start [AMOUNT] [TARGET%]".bold()
            );
            println!("    {} — Check live P&L", "stockwise sim status".bold());
            println!(
                "    {} — Settle at end-of-day",
                "stockwise sim settle".bold()
            );
            println!(
                "    {} — View cumulative stats + confidence",
                "stockwise sim history".bold()
            );
            println!("    {} — Clear all data", "stockwise sim reset".bold());
            println!();
            println!(
                "  {} Morning: {} → Track: {} → EOD: {}",
                "Workflow:".dimmed(),
                "sim start".cyan(),
                "sim status".cyan(),
                "sim settle".cyan()
            );
            println!();
        }
    }
    Ok(())
}

// ── Deep Dive: Everything about a stock ──

/// Clear the simulator history after confirmation. The old file is kept as a
/// timestamped backup so a mistaken reset can be undone by hand.
pub fn cmd_sim_reset(yes: bool) -> Result<()> {
    let history = SimHistory::load()?;
    if history.sessions.is_empty() {
        println!("  Simulator history is already empty.\n");
        return Ok(());
    }
    if !yes {
        use std::io::{BufRead, IsTerminal, Write};
        if !std::io::stdin().is_terminal() {
            bail!("Refusing to delete simulator history without confirmation. Pass --yes.");
        }
        print!(
            "  Delete {} simulator session(s)? Type {} to confirm: ",
            history.sessions.len(),
            "yes".bold()
        );
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        if line.trim() != "yes" {
            println!("  Cancelled — nothing was deleted.\n");
            return Ok(());
        }
    }
    let path = crate::store::path("sim_history.json")?;
    let backup = path.with_extension(format!(
        "json.{}.bak",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::copy(&path, &backup)
        .with_context(|| format!("Could not back up {}", path.display()))?;
    SimHistory::default().save()?;
    println!(
        "  {} Simulator history cleared. Backup: {}\n",
        "✓".green(),
        backup.display()
    );
    Ok(())
}

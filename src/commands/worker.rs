//! `daemon` workers: the intraday paper-trading loop and the daily long-term scan.

use super::*;

pub async fn cmd_daemon(mode: &str, amount: Option<f64>, market: Market) -> Result<()> {
    // Both loops follow exchange sessions; crypto and FX have none.
    market.require_stocks("daemon")?;
    let csym = market.currency_symbol();

    match mode {
        "intraday" => {
            let capital = amount.unwrap_or(25000.0);
            let target_pct = 2.0;
            let mut risk = crate::daemon::RiskState::new(capital);
            let mut positions: Vec<crate::daemon::LivePosition> = Vec::new();
            let mut entered = false;
            let mut tick_count = 0u32;
            let mut last_phase = crate::daemon::Phase::Closed;

            print_header("Intraday Daemon — Starting");
            println!(
                "  Capital: {}{:.0}  |  Target: {}%  |  Max Risk: {}% daily",
                csym, capital, target_pct, risk.daily_loss_limit_pct
            );
            println!(
                "  Max {} positions  |  Paper trading mode",
                risk.max_positions
            );
            println!("  {}", "─".repeat(60).dimmed());
            println!(
                "  {} Press Ctrl+C to stop. Positions will be squared off.\n",
                "→".dimmed()
            );

            loop {
                let phase = crate::daemon::current_phase_india();

                // Phase transition announcements
                if phase != last_phase {
                    let phase_str = match phase {
                        crate::daemon::Phase::PreMarket => "PRE-MARKET — Scanning..."
                            .to_string()
                            .yellow()
                            .bold()
                            .to_string(),
                        crate::daemon::Phase::Opening => "MARKET OPEN — Entering positions"
                            .to_string()
                            .green()
                            .bold()
                            .to_string(),
                        crate::daemon::Phase::Active => "ACTIVE TRADING — Monitoring"
                            .to_string()
                            .cyan()
                            .bold()
                            .to_string(),
                        crate::daemon::Phase::WindDown => {
                            "WIND-DOWN — Tightening stops, no new entries"
                                .to_string()
                                .yellow()
                                .to_string()
                        }
                        crate::daemon::Phase::SquareOff => "SQUARE OFF — Closing all positions"
                            .to_string()
                            .red()
                            .bold()
                            .to_string(),
                        crate::daemon::Phase::PostMarket => {
                            "POST-MARKET — Settling".to_string().dimmed().to_string()
                        }
                        crate::daemon::Phase::Closed => {
                            "MARKET CLOSED".to_string().dimmed().to_string()
                        }
                    };
                    println!(
                        "\n  {} [{}] {}",
                        "▶".bold(),
                        chrono::Local::now().format("%H:%M:%S"),
                        phase_str
                    );
                    last_phase = phase;
                }

                match phase {
                    crate::daemon::Phase::Closed => {
                        println!(
                            "  {} Market is closed. Daemon will wait for market hours.",
                            "→".dimmed()
                        );
                        println!("  {} NSE: Mon-Fri 9:00 AM – 3:30 PM IST", "→".dimmed());
                        // Wait 5 minutes before checking again
                        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                        continue;
                    }

                    crate::daemon::Phase::PreMarket => {
                        if !entered {
                            println!("  {} Scanning sectors and candidates...", "⟳".yellow());
                            let client = YahooClient::new().await?;
                            // Sector heat
                            if let Ok(heats) = intraday::scan_sector_heat(&client).await {
                                let hot: Vec<_> = heats.iter().filter(|h| h.hot).collect();
                                if !hot.is_empty() {
                                    println!(
                                        "  🔥 Hot sectors: {}",
                                        hot.iter()
                                            .map(|h| format!("{} ({:+.1}%)", h.name, h.change_pct))
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    );
                                }
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    }

                    crate::daemon::Phase::Opening => {
                        if !entered && !risk.killed {
                            println!("  {} Running 9-strategy scan...", "⟳".yellow());
                            let client = YahooClient::new().await?;
                            let signals = intraday::scan_intraday(&client, market).await?;
                            let plans =
                                intraday::generate_trade_plans(&signals, capital, target_pct, 1.0);

                            if !plans.is_empty() {
                                positions = crate::daemon::plans_to_positions(
                                    &plans[..plans.len().min(risk.max_positions)],
                                );
                                entered = true;

                                println!(
                                    "  {} {} positions entered:\n",
                                    "✓".green().bold(),
                                    positions.len()
                                );
                                for p in &positions {
                                    println!(
                                        "    {} {} {} × {} @ {}{:.2}  T1:{}{:.2}  T2:{}{:.2}  SL:{}{:.2}",
                                        "→".green(),
                                        p.direction,
                                        p.symbol.cyan(),
                                        p.qty,
                                        csym,
                                        p.entry_price,
                                        csym,
                                        p.target1,
                                        csym,
                                        p.target2,
                                        csym,
                                        p.stop_loss
                                    );
                                }
                            } else {
                                println!("  {} No qualified trades. Watching...", "→".yellow());
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    }

                    crate::daemon::Phase::Active | crate::daemon::Phase::WindDown => {
                        if !positions.is_empty() {
                            // Fetch live prices
                            let client = YahooClient::new().await?;
                            let syms: Vec<&str> =
                                positions.iter().map(|p| p.symbol.as_str()).collect();
                            let quotes = client.get_quote(&syms).await?;

                            // Update each position
                            let mut total_pnl = 0.0_f64;
                            let mut any_change = false;

                            for pos in positions.iter_mut() {
                                let old_status = pos.status;
                                if let Some(q) = quotes
                                    .iter()
                                    .find(|q| q.symbol.as_deref() == Some(&pos.symbol))
                                    && let Some(price) = q.regular_market_price
                                {
                                    crate::daemon::update_position(pos, price, phase);
                                }
                                total_pnl += pos.pnl;

                                if pos.status != old_status {
                                    any_change = true;
                                    let alert = match pos.status {
                                        crate::daemon::PositionStatus::T1Hit => format!(
                                            "T1 HIT — booked 50% of {} at {}{:.2}",
                                            pos.symbol, csym, pos.current_price
                                        )
                                        .green()
                                        .bold()
                                        .to_string(),
                                        crate::daemon::PositionStatus::T2Hit => format!(
                                            "TARGET HIT — {} fully closed at {}{:.2}",
                                            pos.symbol, csym, pos.current_price
                                        )
                                        .green()
                                        .bold()
                                        .to_string(),
                                        crate::daemon::PositionStatus::StopHit => {
                                            risk.record_exit(pos.pnl);
                                            format!(
                                                "STOPPED — {} at {}{:.2} (P&L: {}{:.0})",
                                                pos.symbol, csym, pos.current_price, csym, pos.pnl
                                            )
                                            .red()
                                            .bold()
                                            .to_string()
                                        }
                                        crate::daemon::PositionStatus::SquaredOff => {
                                            risk.record_exit(pos.pnl);
                                            format!(
                                                "SQUARED OFF — {} at {}{:.2}",
                                                pos.symbol, csym, pos.current_price
                                            )
                                            .yellow()
                                            .to_string()
                                        }
                                        _ => String::new(),
                                    };
                                    if !alert.is_empty() {
                                        println!(
                                            "  ⚡ [{}] {}",
                                            chrono::Local::now().format("%H:%M:%S"),
                                            alert
                                        );
                                    }
                                }
                            }

                            // Check daily loss limit
                            let unrealized: f64 = positions
                                .iter()
                                .filter(|p| {
                                    p.status == crate::daemon::PositionStatus::Open
                                        || p.status == crate::daemon::PositionStatus::T1Hit
                                })
                                .map(|p| p.pnl)
                                .sum();
                            if risk.check_daily_limit(unrealized) {
                                println!(
                                    "  🛑 [{}] {} Daily loss limit breached! Closing all positions.",
                                    chrono::Local::now().format("%H:%M:%S"),
                                    "KILL SWITCH".red().bold()
                                );
                                for pos in positions.iter_mut() {
                                    if pos.status == crate::daemon::PositionStatus::Open
                                        || pos.status == crate::daemon::PositionStatus::T1Hit
                                    {
                                        pos.status = crate::daemon::PositionStatus::SquaredOff;
                                        risk.record_exit(pos.pnl);
                                    }
                                }
                            }

                            // Status update every 5 ticks (~5 minutes)
                            tick_count += 1;
                            if tick_count.is_multiple_of(5) || any_change {
                                let open_count = positions
                                    .iter()
                                    .filter(|p| {
                                        matches!(
                                            p.status,
                                            crate::daemon::PositionStatus::Open
                                                | crate::daemon::PositionStatus::T1Hit
                                        )
                                    })
                                    .count();
                                let total_str = if total_pnl >= 0.0 {
                                    format!("+{}{:.0}", csym, total_pnl).green().to_string()
                                } else {
                                    format!("-{}{:.0}", csym, total_pnl.abs()).red().to_string()
                                };
                                println!(
                                    "  {} [{}] {} | {} open | P&L: {} | Phase: {}",
                                    "●".dimmed(),
                                    chrono::Local::now().format("%H:%M:%S"),
                                    format!("Tick #{}", tick_count).dimmed(),
                                    open_count,
                                    total_str,
                                    phase
                                );
                            }

                            // All positions closed? Stop monitoring
                            if positions.iter().all(|p| {
                                matches!(
                                    p.status,
                                    crate::daemon::PositionStatus::T2Hit
                                        | crate::daemon::PositionStatus::StopHit
                                        | crate::daemon::PositionStatus::SquaredOff
                                )
                            }) {
                                println!(
                                    "\n  {} All positions closed. Waiting for post-market.",
                                    "✓".green().bold()
                                );
                                // Skip to post-market wait
                                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                                continue;
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    }

                    crate::daemon::Phase::SquareOff => {
                        // Force close everything
                        for pos in positions.iter_mut() {
                            if pos.status == crate::daemon::PositionStatus::Open
                                || pos.status == crate::daemon::PositionStatus::T1Hit
                            {
                                pos.status = crate::daemon::PositionStatus::SquaredOff;
                                risk.record_exit(pos.pnl);
                                println!(
                                    "  {} Squared off {} at {}{:.2}",
                                    "→".yellow(),
                                    pos.symbol.cyan(),
                                    csym,
                                    pos.current_price
                                );
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    }

                    crate::daemon::Phase::PostMarket => {
                        // Settlement
                        if entered {
                            println!("\n  {}", "─".repeat(60).dimmed());
                            print_header("End of Day Settlement");

                            let total_pnl: f64 = positions.iter().map(|p| p.pnl).sum();
                            let wins = positions.iter().filter(|p| p.pnl > 0.0).count();
                            let losses = positions.iter().filter(|p| p.pnl <= 0.0).count();

                            println!(
                                "  {:<14} {:>10} {:>10} {:>12} {:>10}",
                                "Symbol".bold(),
                                "Entry".bold(),
                                "Exit".bold(),
                                "P&L".bold(),
                                "Status".bold()
                            );
                            println!("  {}", "─".repeat(60).dimmed());
                            for p in &positions {
                                let pnl_str = if p.pnl >= 0.0 {
                                    format!("+{}{:.0}", csym, p.pnl).green().to_string()
                                } else {
                                    format!("-{}{:.0}", csym, p.pnl.abs()).red().to_string()
                                };
                                println!(
                                    "  {:<14} {:>10} {:>10} {:>12} {:>10}",
                                    p.symbol.cyan(),
                                    format!("{}{:.2}", csym, p.entry_price),
                                    format!("{}{:.2}", csym, p.current_price),
                                    pnl_str,
                                    p.status
                                );
                            }
                            println!("  {}", "─".repeat(60).dimmed());
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
                            println!(
                                "  Total: {}  |  {} wins, {} losses",
                                total_str, wins, losses
                            );

                            // Save to sim history
                            let mut history = crate::simulator::SimHistory::load()?;
                            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                            let trades: Vec<crate::simulator::SimTrade> = positions
                                .iter()
                                .map(|p| crate::simulator::SimTrade {
                                    symbol: p.symbol.clone(),
                                    direction: p.direction.clone(),
                                    entry_price: p.entry_price,
                                    target1: p.target1,
                                    target2: p.target2,
                                    stop_loss: p.stop_loss,
                                    qty: p.qty,
                                    capital: p.entry_price * p.qty as f64,
                                    score: p.score,
                                    confidence: "DAEMON".into(),
                                    strategies: p.strategies.clone(),
                                    exit_price: Some(p.current_price),
                                    pnl: Some(p.pnl),
                                    pnl_pct: Some(p.pnl_pct),
                                    hit_target: Some(
                                        p.status == crate::daemon::PositionStatus::T2Hit,
                                    ),
                                    hit_stop: Some(
                                        p.status == crate::daemon::PositionStatus::StopHit,
                                    ),
                                })
                                .collect();
                            let total_pnl_pct = (total_pnl / capital) * 100.0;
                            history.sessions.push(crate::simulator::SimSession {
                                date: today,
                                market: "IN".into(),
                                capital,
                                target_pct,
                                trades,
                                total_pnl: Some(total_pnl),
                                total_pnl_pct: Some(total_pnl_pct),
                                win_count: Some(wins as u32),
                                loss_count: Some(losses as u32),
                                settled: true,
                            });
                            history.save()?;
                            println!("  {} Saved to simulation history.", "✓".green());

                            // AI report if available
                            let mut ai = crate::ai::AiClient::connect().await;
                            if ai.is_available().await {
                                println!("  {} Generating AI EOD report...", "⟳".yellow());
                                let mut data = format!(
                                    "Intraday results: P&L={}{:.0}, {} wins {} losses\n",
                                    csym, total_pnl, wins, losses
                                );
                                for p in &positions {
                                    data.push_str(&format!(
                                        "{}: entry={:.2}, exit={:.2}, pnl={:.0}, status={}\n",
                                        p.symbol, p.entry_price, p.current_price, p.pnl, p.status
                                    ));
                                }
                                if let Ok(report) = ai.generate_intraday_report(&data).await {
                                    print_section("AI EOD Analysis");
                                    for line in report.lines() {
                                        println!("  {}", line);
                                    }
                                }
                            }

                            println!(
                                "\n  {} Daemon complete for today. Exiting.",
                                "✓".green().bold()
                            );
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    }
                }
            }
        }

        "longterm" => {
            print_header("Long-Term Daemon — Daily Run");
            println!("  {} Running post-market analysis...\n", "⟳".yellow());

            let client = YahooClient::new().await?;

            // 1. Score all stocks
            println!("  [1/5] Scoring stocks...");
            let symbols = market.universe();
            let sym_refs: Vec<&str> = symbols.iter().map(String::as_str).collect();
            let quotes = client.get_quote(&sym_refs).await?;
            let mut scores: Vec<crate::longterm::LongTermScore> = Vec::new();
            for q in &quotes {
                let sym = q.symbol.as_deref().unwrap_or("");
                let hist = client.get_chart(sym, "1y", "1d").await.ok().and_then(|c| {
                    c.indicators
                        .quote
                        .first()
                        .and_then(|qi| qi.close.as_ref())
                        .map(|c| c.iter().filter_map(|v| *v).collect::<Vec<f64>>())
                });
                scores.push(crate::longterm::score_for_longterm(q, hist.as_deref()));
            }
            scores.sort_by(|a, b| {
                b.total_score
                    .partial_cmp(&a.total_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            println!(
                "    Top 3: {}",
                scores
                    .iter()
                    .take(3)
                    .map(|s| format!("{} ({:.0})", s.symbol, s.total_score))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            // 2. Check portfolio
            println!("  [2/5] Checking portfolio...");
            let portfolio = Portfolio::load()?;
            if !portfolio.holdings.is_empty() {
                let p_syms: Vec<String> = portfolio
                    .holdings
                    .iter()
                    .map(|h| h.symbol.clone())
                    .collect();
                let p_refs: Vec<&str> = p_syms.iter().map(|s| s.as_str()).collect();
                let p_quotes = client.get_quote(&p_refs).await?;
                let mut total_value = 0.0_f64;
                let mut total_cost = 0.0_f64;
                let mut unpriced = 0;
                for h in &portfolio.holdings {
                    match p_quotes
                        .iter()
                        .find(|q| q.symbol.as_deref() == Some(&h.symbol))
                        .and_then(|q| q.regular_market_price)
                    {
                        Some(price) => {
                            total_value += h.shares * price;
                            total_cost += h.shares * h.avg_cost;
                        }
                        None => unpriced += 1,
                    }
                }
                let pnl = total_value - total_cost;
                let pnl_str = if pnl >= 0.0 {
                    format!("+{}{:.0}", csym, pnl).green().to_string()
                } else {
                    format!("-{}{:.0}", csym, pnl.abs()).red().to_string()
                };
                println!(
                    "    Portfolio: {}{:.0} ({}) | {} holdings",
                    csym,
                    total_value,
                    pnl_str,
                    portfolio.holdings.len()
                );

                // Wealth snapshot — skipped when a price is missing, so the
                // history never records a fake drop.
                if unpriced == 0 {
                    let mut wealth = crate::wealth::WealthHistory::load()?;
                    wealth.add_snapshot(total_value, total_cost, portfolio.holdings.len());
                    wealth.save()?;
                } else {
                    println!(
                        "    {} {} holding(s) without a live price — wealth snapshot skipped.",
                        "!".yellow(),
                        unpriced
                    );
                }
            } else {
                println!("    No portfolio holdings.");
            }

            // 3. Check alerts
            println!("  [3/5] Checking alerts...");
            let store = crate::alerts::AlertStore::load()?;
            if !store.alerts.is_empty() {
                let a_syms: Vec<String> = store.alerts.iter().map(|a| a.symbol.clone()).collect();
                let mut unique: Vec<&str> = a_syms.iter().map(|s| s.as_str()).collect();
                unique.sort();
                unique.dedup();
                let a_quotes = client.get_quote(&unique).await?;
                let mut triggered = 0;
                for alert in &store.alerts {
                    let Some(price) = a_quotes
                        .iter()
                        .find(|q| q.symbol.as_deref() == Some(alert.symbol.as_str()))
                        .and_then(|q| q.regular_market_price)
                    else {
                        continue;
                    };
                    let hit = match alert.condition {
                        crate::alerts::AlertCondition::Above => price >= alert.target,
                        crate::alerts::AlertCondition::Below => price <= alert.target,
                    };
                    if hit {
                        triggered += 1;
                        println!(
                            "    {} {} {} {:.2} — TRIGGERED (now {}{:.2})",
                            "⚠".yellow(),
                            alert.symbol,
                            alert.condition,
                            alert.target,
                            csym,
                            price
                        );
                    }
                }
                if triggered == 0 {
                    println!("    No alerts triggered.");
                }
            }

            // 4. Tax harvest check
            println!("  [4/5] Tax harvest scan...");
            if !portfolio.holdings.is_empty() {
                let p_syms: Vec<String> = portfolio
                    .holdings
                    .iter()
                    .map(|h| h.symbol.clone())
                    .collect();
                let p_refs: Vec<&str> = p_syms.iter().map(|s| s.as_str()).collect();
                let p_quotes = client.get_quote(&p_refs).await?;
                let losers: Vec<_> = portfolio
                    .holdings
                    .iter()
                    .filter(|h| {
                        p_quotes
                            .iter()
                            .find(|q| q.symbol.as_deref() == Some(&h.symbol))
                            .and_then(|q| q.regular_market_price)
                            .is_some_and(|price| price < h.avg_cost)
                    })
                    .collect();
                if !losers.is_empty() {
                    println!(
                        "    {} positions with losses (harvest candidates): {}",
                        losers.len(),
                        losers
                            .iter()
                            .map(|h| h.symbol.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                } else {
                    println!("    No tax-loss harvest opportunities.");
                }
            }

            // 5. AI report
            println!("  [5/5] Generating AI report...");
            let mut ai = crate::ai::AiClient::connect().await;
            if ai.is_available().await {
                let mut data = String::new();
                for s in scores.iter().take(10) {
                    data.push_str(&format!(
                        "{}: score={:.0}, moat={}, 5Y={:+.0}%, risk={}\n",
                        s.symbol, s.total_score, s.moat, s.projected_5y_return, s.risk_tier
                    ));
                }
                if let Ok(report) = ai.generate_longterm_report(&data).await {
                    print_section("AI Investment Memo");
                    for line in report.lines() {
                        println!("  {}", line);
                    }
                }
            } else {
                println!("    Ollama not running. Skipping AI report.");
            }

            println!("\n  {} Long-term daemon complete.", "✓".green().bold());
        }

        _ => {
            println!();
            println!("  {} Daemon Modes:", "→".cyan());
            println!();
            println!(
                "    {} — Continuous intraday monitor (9 AM – 3:30 PM)",
                "stockwise daemon intraday [CAPITAL]".bold()
            );
            println!(
                "    {} — Daily long-term analysis (run after market close)",
                "stockwise daemon longterm".bold()
            );
            println!();
            println!(
                "  {} The intraday daemon runs in paper trading mode by default.",
                "→".dimmed()
            );
            println!(
                "  {} It scans, enters, monitors, and squares off automatically.",
                "→".dimmed()
            );
            println!(
                "  {} Results are saved to simulation history.",
                "→".dimmed()
            );
            println!();
        }
    }
    Ok(())
}

// ── Multi-timeframe Chart ──

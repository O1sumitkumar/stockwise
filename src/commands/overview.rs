//! Overview commands: quotes, indices, dashboard, search, and `doctor`.

use super::*;

pub async fn cmd_quote(symbols: &[String], market: Market, json: bool) -> Result<()> {
    let resolved = resolve_symbols(symbols, market);
    let client = YahooClient::new().await?;
    // Yahoo silently drops unknown symbols; name them instead.
    let views::QuoteSet { quotes, missing } = views::quotes(&client, &resolved).await?;
    let missing_err = || {
        anyhow::anyhow!(
            "No quote for {}{}",
            missing.join(", "),
            market::not_found_hint(market)
        )
    };

    if quotes.is_empty() {
        return Err(missing_err());
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&quotes)?);
        return if missing.is_empty() {
            Ok(())
        } else {
            Err(missing_err())
        };
    }

    for q in &quotes {
        let symbol = q.symbol.as_deref().unwrap_or("???");
        let name = q
            .long_name
            .as_deref()
            .or(q.short_name.as_deref())
            .unwrap_or("Unknown");
        let price = q.regular_market_price.unwrap_or(0.0);
        let change = q.regular_market_change.unwrap_or(0.0);
        let change_pct = q.regular_market_change_percent.unwrap_or(0.0);
        let cur = q.currency.as_deref();

        print_header(&format!("{} — {}", symbol, name));

        println!(
            "  {}  {}",
            format_price(price, cur).bold(),
            format_change(change, change_pct)
        );

        println!();
        print_kv(
            "Open",
            &format_price(q.regular_market_open.unwrap_or(0.0), cur),
        );
        print_kv(
            "Day Range",
            &format!(
                "{} — {}",
                format_price(q.regular_market_day_low.unwrap_or(0.0), cur),
                format_price(q.regular_market_day_high.unwrap_or(0.0), cur)
            ),
        );
        print_kv(
            "52-Week Range",
            &format!(
                "{} — {}",
                format_price(q.fifty_two_week_low.unwrap_or(0.0), cur),
                format_price(q.fifty_two_week_high.unwrap_or(0.0), cur)
            ),
        );
        print_kv(
            "Volume",
            &format_volume(q.regular_market_volume.unwrap_or(0)),
        );
        print_kv(
            "Avg Volume (3M)",
            &format_volume(q.average_daily_volume_3_month.unwrap_or(0)),
        );
        print_kv(
            "Market Cap",
            &q.market_cap
                .map_or("N/A".to_string(), |v| format_large_number(v, cur)),
        );
        print_kv("P/E (TTM)", &format_optional_f64(q.trailing_pe, ""));
        print_kv(
            "EPS (TTM)",
            &format_optional_f64(q.eps_trailing_twelve_months, ""),
        );
        print_kv(
            "50-Day MA",
            &format_optional_price(q.fifty_day_average, cur),
        );
        print_kv(
            "200-Day MA",
            &format_optional_price(q.two_hundred_day_average, cur),
        );

        if let Some(div) = q.trailing_annual_dividend_yield
            && div > 0.0
        {
            print_kv("Dividend Yield", &format!("{:.2}%", div * 100.0));
        }
    }
    println!();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing_err())
    }
}

pub async fn cmd_markets(market: Market, json: bool) -> Result<()> {
    let client = YahooClient::new().await?;
    let sections = views::market_sections(&client, market).await?;

    if json {
        let rows: Vec<&views::IndexRow> = sections.iter().flat_map(|(_, rows)| rows).collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    for (title, rows) in &sections {
        print_header(title);
        for r in rows {
            let price = r.price.unwrap_or(0.0);
            let change = r.change.unwrap_or(0.0);
            let change_pct = r.change_percent.unwrap_or(0.0);
            let cur = r.currency.as_deref();

            println!(
                "  {:<16} {}  {}",
                r.name.bold(),
                format_price(price, cur).bold(),
                format_change(change, change_pct)
            );
        }
    }
    println!();
    Ok(())
}

// ── Watchlist ──

pub async fn cmd_dashboard(market: Market) -> Result<()> {
    let client = YahooClient::new().await?;
    let (indices, market_title) = match market {
        Market::Us => (market::US_INDICES, "US"),
        Market::In => (market::INDIA_INDICES, "India"),
    };
    let idx_syms: Vec<&str> = indices.iter().map(|(s, _)| *s).collect();
    let idx_quotes = client.get_quote(&idx_syms).await?;

    print_header(&format!("Dashboard — {}", market_title));
    print_section("Market Indices");
    for q in &idx_quotes {
        // Match by symbol: Yahoo may reorder or drop symbols.
        let q_sym = q.symbol.as_deref().unwrap_or("");
        let label = indices
            .iter()
            .find(|(sym, _)| *sym == q_sym)
            .map(|(_, n)| *n)
            .unwrap_or(q_sym);
        let cur = q.currency.as_deref();
        println!(
            "  {:<16} {}  {}",
            label,
            format_price(q.regular_market_price.unwrap_or(0.0), cur).bold(),
            format_change(
                q.regular_market_change.unwrap_or(0.0),
                q.regular_market_change_percent.unwrap_or(0.0)
            )
        );
    }

    let portfolio = Portfolio::load()?;
    if !portfolio.holdings.is_empty() {
        let p_syms: Vec<String> = portfolio
            .holdings
            .iter()
            .map(|h| h.symbol.clone())
            .collect();
        let p_refs: Vec<&str> = p_syms.iter().map(|s| s.as_str()).collect();
        let p_quotes = client.get_quote(&p_refs).await?;
        let mut total_cost = 0.0_f64;
        let mut total_value = 0.0_f64;
        for h in &portfolio.holdings {
            // Skip holdings without a live price instead of valuing them at 0.
            if let Some(price) = p_quotes
                .iter()
                .find(|q| q.symbol.as_deref() == Some(&h.symbol))
                .and_then(|q| q.regular_market_price)
            {
                total_cost += h.shares * h.avg_cost;
                total_value += h.shares * price;
            }
        }
        let pnl = total_value - total_cost;
        let pnl_pct = if total_cost > 0.0 {
            (pnl / total_cost) * 100.0
        } else {
            0.0
        };
        let cur = p_quotes.first().and_then(|q| q.currency.as_deref());
        let csym = market::currency_symbol(cur);
        print_section(&format!(
            "Portfolio ({} holdings)",
            portfolio.holdings.len()
        ));
        print_kv("Total Value", &format_price(total_value, cur));
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
        print_kv("P&L", &pnl_str);
    }

    let wl = Watchlist::load()?;
    if !wl.symbols.is_empty() {
        let w_refs: Vec<&str> = wl.symbols.iter().map(|s| s.as_str()).collect();
        let w_quotes = client.get_quote(&w_refs).await?;
        print_section(&format!("Watchlist ({} symbols)", wl.symbols.len()));
        for q in &w_quotes {
            let cur = q.currency.as_deref();
            let pct = q.regular_market_change_percent.unwrap_or(0.0);
            let pct_str = if pct >= 0.0 {
                format!("+{:.2}%", pct).green().to_string()
            } else {
                format!("{:.2}%", pct).red().to_string()
            };
            println!(
                "  {:<12} {}  {}",
                q.symbol.as_deref().unwrap_or("???"),
                format_price(q.regular_market_price.unwrap_or(0.0), cur),
                pct_str
            );
        }
    }

    let store = AlertStore::load()?;
    if !store.alerts.is_empty() {
        let a_syms: Vec<String> = store.alerts.iter().map(|a| a.symbol.clone()).collect();
        let mut unique: Vec<&str> = a_syms.iter().map(|s| s.as_str()).collect();
        unique.sort();
        unique.dedup();
        let a_quotes = client.get_quote(&unique).await?;
        let mut triggered = Vec::new();
        for alert in &store.alerts {
            // No live price: never report it as triggered.
            let Some(price) = a_quotes
                .iter()
                .find(|q| q.symbol.as_deref() == Some(alert.symbol.as_str()))
                .and_then(|q| q.regular_market_price)
            else {
                continue;
            };
            let hit = match alert.condition {
                AlertCondition::Above => price >= alert.target,
                AlertCondition::Below => price <= alert.target,
            };
            if hit {
                triggered.push(format!(
                    "{} {} {:.2}",
                    alert.symbol, alert.condition, alert.target
                ));
            }
        }
        if !triggered.is_empty() {
            print_section("Triggered Alerts");
            for t in &triggered {
                println!("  {} {}", "⚠".yellow(), t.red());
            }
        }
    }
    println!();
    if portfolio.holdings.is_empty() && wl.symbols.is_empty() && store.alerts.is_empty() {
        print_section("Get started");
        println!(
            "  stockwise portfolio add TCS 10 3500     track a holding (symbol, shares, cost)"
        );
        println!("  stockwise import kite holdings.csv      or import from your broker");
        println!("  stockwise watch add INFY HDFCBANK       watch a few stocks");
        println!("  stockwise alert add RELIANCE below 1200 get alerted on a price level");
        println!();
    }
    Ok(())
}

// ── Search ──

pub async fn cmd_search(query: &str, json: bool) -> Result<()> {
    let client = YahooClient::new().await?;
    let results = client.search(query).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }
    if results.is_empty() {
        bail!("No tickers match '{}'", query);
    }
    print_header(&format!("Search: {}", query));
    println!(
        "  {:<14} {:<30} {:>10} {:>10}",
        "Symbol".bold(),
        "Name".bold(),
        "Type".bold(),
        "Exchange".bold()
    );
    println!("  {}", "─".repeat(68).dimmed());
    for r in &results {
        println!(
            "  {:<14} {:<30} {:>10} {:>10}",
            r.symbol.as_deref().unwrap_or("???").cyan(),
            r.long_name
                .as_deref()
                .or(r.short_name.as_deref())
                .unwrap_or("—"),
            r.quote_type.as_deref().unwrap_or("—").dimmed(),
            r.exchange.as_deref().unwrap_or("—").dimmed()
        );
    }
    println!();
    Ok(())
}

// ── Intraday Bot ──

/// `stockwise doctor` — one screen that answers "is my setup working?".
pub async fn cmd_doctor(market: Market) -> Result<()> {
    let ok = |s: &str| format!("{} {}", "✓".green(), s);
    let warn = |s: &str| format!("{} {}", "!".yellow(), s);
    let bad = |s: &str| format!("{} {}", "✗".red(), s);

    print_header(&format!("StockWise {} — doctor", env!("CARGO_PKG_VERSION")));

    // Defaults
    let market_src = if std::env::var_os("STOCKWISE_MARKET").is_some() {
        "from STOCKWISE_MARKET"
    } else {
        "default; change with -m or STOCKWISE_MARKET"
    };
    let market_name = match market {
        Market::In => "in (India, NSE)",
        Market::Us => "us",
    };
    print_kv("Market", &format!("{} — {}", market_name, market_src));

    // Data directory
    match crate::store::data_dir() {
        Ok(dir) => {
            print_kv("Data directory", &ok(&dir.display().to_string()));
            let files = [
                "portfolio.json",
                "watchlist.json",
                "alerts.json",
                "sim_history.json",
                "wealth_history.json",
            ];
            let present: Vec<&str> = files
                .iter()
                .copied()
                .filter(|f| dir.join(f).exists())
                .collect();
            print_kv(
                "Data files",
                &if present.is_empty() {
                    "none yet (created on first use)".to_string()
                } else {
                    present.join(", ")
                },
            );
        }
        Err(e) => print_kv("Data directory", &bad(&format!("{:#}", e))),
    }

    // Market data
    let started = std::time::Instant::now();
    let yahoo = match YahooClient::new().await {
        Ok(client) => client
            .get_quote(&["^GSPC"])
            .await
            .map(|q| !q.is_empty())
            .unwrap_or(false),
        Err(_) => false,
    };
    print_kv(
        "Yahoo Finance",
        &if yahoo {
            ok(&format!("reachable ({} ms)", started.elapsed().as_millis()))
        } else {
            bad("unreachable — check your internet connection or proxy")
        },
    );

    // Cache
    let redis = crate::api::connect_redis().await.is_some();
    print_kv(
        "Redis cache",
        &if redis {
            ok("connected")
        } else {
            "not used (optional; in-memory cache instead)"
                .dimmed()
                .to_string()
        },
    );

    // AI
    let ai = crate::ai::AiClient::connect().await;
    print_kv(
        "Ollama (AI)",
        &match ai.has_model().await {
            Some(true) => ok(&format!(
                "model {} ready — use `report` or add --ai",
                ai.model()
            )),
            Some(false) => warn(&format!(
                "running, but model {} is not pulled — run `ollama pull {}`",
                ai.model(),
                ai.model()
            )),
            None => "not running (optional; needed for `report` and --ai)"
                .dimmed()
                .to_string(),
        },
    );

    // Broker
    let broker = match crate::angel::AngelConfig::load() {
        Ok(c) if c.is_configured() => ok(&format!(
            "configured for {}{}",
            c.client_id,
            c.max_order_value
                .map(|v| format!(", max order ₹{:.0}", v))
                .unwrap_or_default()
        )),
        Ok(_) => "not configured (optional; `trade setup` for live trading)"
            .dimmed()
            .to_string(),
        Err(e) => warn(&format!("{:#}", e)),
    };
    print_kv("Angel One", &broker);
    println!();

    if !yahoo {
        bail!("Market data is unreachable");
    }
    Ok(())
}

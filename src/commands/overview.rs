//! Overview commands: quotes, indices, dashboard, search, and `doctor`.

use super::*;

/// Localized name of a market, for titles.
fn market_title(market: Market) -> &'static str {
    t(match market {
        Market::In => Msg::MarketIndia,
        Market::Us => Msg::MarketUs,
        Market::Crypto => Msg::MarketCrypto,
        Market::Fx => Msg::MarketFx,
    })
}

/// A price with the precision its asset class needs, or a dimmed N/A —
/// never a made-up 0.
fn price_or_na(val: Option<f64>, currency: Option<&str>, symbol: &str) -> String {
    val.map_or_else(
        || "N/A".dimmed().to_string(),
        |v| format_asset_price(v, currency, symbol),
    )
}

/// `▲ 1.23 (0.45%)`, with as many decimals as the price needs.
fn change_for(q: &crate::api::Quote, symbol: &str) -> String {
    let price = q.regular_market_price.unwrap_or(0.0);
    format_change_dp(
        q.regular_market_change.unwrap_or(0.0),
        q.regular_market_change_percent.unwrap_or(0.0),
        price_decimals(market::AssetClass::of(symbol), price),
    )
}

pub async fn cmd_quote(symbols: &[String], market: Market, json: bool) -> Result<()> {
    let resolved = resolve_symbols(symbols, market);
    let client = YahooClient::new().await?;
    let sym_refs: Vec<&str> = resolved.iter().map(|s| s.as_str()).collect();
    let quotes = client.get_quote(&sym_refs).await?;

    // Yahoo silently drops unknown symbols; name them instead.
    let missing: Vec<&str> = resolved
        .iter()
        .filter(|r| {
            !quotes.iter().any(|q| {
                q.symbol
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(r))
            })
        })
        .map(|s| s.as_str())
        .collect();
    let missing_err = || {
        anyhow::anyhow!(
            "{}",
            tf(
                Msg::NoQuoteFor,
                &[&missing.join(", "), &market::not_found_hint(market)]
            )
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

    let now = chrono::Utc::now();
    for q in &quotes {
        let symbol = q.symbol.as_deref().unwrap_or("???");
        let name = q
            .long_name
            .as_deref()
            .or(q.short_name.as_deref())
            .unwrap_or(t(Msg::Unknown));
        let cur = q.currency.as_deref();
        let px = |v: Option<f64>| price_or_na(v, cur, symbol);
        let na = || "N/A".dimmed().to_string();

        print_header(&format!("{} — {}", symbol, name));

        println!(
            "  {}  {}",
            px(q.regular_market_price).bold(),
            change_for(q, symbol)
        );
        if let Some(note) = market::session_note(market::AssetClass::of(symbol), now) {
            println!("  {}", t(note).dimmed());
        }

        println!();
        print_kv(t(Msg::Open), &px(q.regular_market_open));
        print_kv(
            t(Msg::DayRange),
            &format!(
                "{} — {}",
                px(q.regular_market_day_low),
                px(q.regular_market_day_high)
            ),
        );
        print_kv(
            t(Msg::WeekRange52),
            &format!(
                "{} — {}",
                px(q.fifty_two_week_low),
                px(q.fifty_two_week_high)
            ),
        );
        // FX has no exchange volume (Yahoo reports 0).
        if market::AssetClass::of(symbol) != market::AssetClass::Fx {
            print_kv(
                t(Msg::Volume),
                &q.regular_market_volume.map_or_else(na, format_volume),
            );
            print_kv(
                t(Msg::AvgVolume3M),
                &q.average_daily_volume_3_month
                    .map_or_else(na, format_volume),
            );
        }
        print_kv(
            t(Msg::MarketCap),
            &q.market_cap
                .map_or_else(na, |v| format_large_number(v, cur)),
        );
        print_kv(t(Msg::PeTtm), &format_optional_f64(q.trailing_pe, ""));
        print_kv(
            t(Msg::EpsTtm),
            &format_optional_f64(q.eps_trailing_twelve_months, ""),
        );
        print_kv(t(Msg::Ma50), &px(q.fifty_day_average));
        print_kv(t(Msg::Ma200), &px(q.two_hundred_day_average));

        if let Some(div) = q.trailing_annual_dividend_yield
            && div > 0.0
        {
            print_kv(t(Msg::DividendYield), &format!("{:.2}%", div * 100.0));
        }
        if let Some(source) = &q.data_source {
            println!("\n  {}", tf(Msg::DataFrom, &[source]).dimmed());
        }
    }
    println!();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing_err())
    }
}

/// One block of `markets` output. `region` is the stable English key used
/// in `--json`; `title` is what people read.
struct MarketSection {
    region: &'static str,
    title: &'static str,
    items: Vec<(String, String)>,
}

fn market_section(kind: Market) -> MarketSection {
    let owned = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|(s, n)| (s.to_string(), n.to_string()))
            .collect()
    };
    match kind {
        Market::In => MarketSection {
            region: "Indian Markets",
            title: t(Msg::SectionIndianMarkets),
            items: owned(market::INDIA_INDICES),
        },
        Market::Us => MarketSection {
            region: "US Markets",
            title: t(Msg::SectionUsMarkets),
            items: owned(market::US_INDICES),
        },
        Market::Crypto => MarketSection {
            region: "Crypto",
            title: t(Msg::SectionCrypto),
            items: market::crypto_majors()
                .into_iter()
                .map(|(s, n)| (s, n.to_string()))
                .collect(),
        },
        Market::Fx => MarketSection {
            region: "FX",
            title: t(Msg::SectionFx),
            items: owned(market::FX_MAJORS),
        },
    }
}

/// Sections for `markets`: the active market first, then the other stock
/// market; crypto and FX are added on request (or lead when selected).
fn market_sections(market: Market, crypto: bool, fx: bool) -> Vec<Market> {
    let mut order = match market {
        Market::Us => vec![Market::Us, Market::In],
        Market::In => vec![Market::In, Market::Us],
        Market::Crypto => vec![Market::Crypto, Market::In, Market::Us],
        Market::Fx => vec![Market::Fx, Market::In, Market::Us],
    };
    for (wanted, kind) in [(crypto, Market::Crypto), (fx, Market::Fx)] {
        if wanted && !order.contains(&kind) {
            order.push(kind);
        }
    }
    order
}

pub async fn cmd_markets(market: Market, json: bool, crypto: bool, fx: bool) -> Result<()> {
    let client = YahooClient::new().await?;

    let mut rows = Vec::new();
    for section in market_sections(market, crypto, fx)
        .into_iter()
        .map(market_section)
    {
        let symbols: Vec<&str> = section.items.iter().map(|(sym, _)| sym.as_str()).collect();
        let quotes = client.get_quote(&symbols).await?;
        // Match the label by symbol — Yahoo may reorder or drop symbols, so
        // pairing by position can mislabel an index.
        let label_for = |q_sym: &str| -> String {
            section
                .items
                .iter()
                .find(|(sym, _)| sym.eq_ignore_ascii_case(q_sym))
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| q_sym.to_string())
        };

        if json {
            for q in &quotes {
                let q_sym = q.symbol.as_deref().unwrap_or("");
                rows.push(serde_json::json!({
                    "region": section.region,
                    "symbol": q_sym,
                    "name": label_for(q_sym),
                    "price": q.regular_market_price,
                    "change": q.regular_market_change,
                    "change_percent": q.regular_market_change_percent,
                    "currency": q.currency,
                }));
            }
            continue;
        }

        print_header(section.title);

        for q in &quotes {
            let q_sym = q.symbol.as_deref().unwrap_or("");
            let cur = q.currency.as_deref();
            println!(
                "  {} {}  {}",
                pad_end(&label_for(q_sym), 16).bold(),
                price_or_na(q.regular_market_price, cur, q_sym).bold(),
                change_for(q, q_sym)
            );
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    println!();
    Ok(())
}

// ── Watchlist ──

pub async fn cmd_dashboard(market: Market) -> Result<()> {
    let client = YahooClient::new().await?;
    let section = market_section(market);
    let idx_syms: Vec<&str> = section.items.iter().map(|(s, _)| s.as_str()).collect();
    let idx_quotes = client.get_quote(&idx_syms).await?;

    print_header(&tf(Msg::DashboardTitle, &[&market_title(market)]));
    print_section(t(match market {
        Market::In | Market::Us => Msg::MarketIndices,
        Market::Crypto => Msg::TopCoins,
        Market::Fx => Msg::MajorPairs,
    }));
    for q in &idx_quotes {
        // Match by symbol: Yahoo may reorder or drop symbols.
        let q_sym = q.symbol.as_deref().unwrap_or("");
        let label = section
            .items
            .iter()
            .find(|(sym, _)| sym.eq_ignore_ascii_case(q_sym))
            .map(|(_, n)| n.as_str())
            .unwrap_or(q_sym);
        let cur = q.currency.as_deref();
        println!(
            "  {} {}  {}",
            pad_end(label, 16),
            price_or_na(q.regular_market_price, cur, q_sym).bold(),
            change_for(q, q_sym)
        );
    }

    let portfolio = Portfolio::load()?;
    if !portfolio.holdings.is_empty() {
        let p_syms: Vec<String> = portfolio
            .holdings
            .iter()
            .map(|h| h.symbol.clone())
            .collect();
        // Totals in one currency; holdings without a price or rate are skipped
        // instead of being valued at 0 or added in the wrong currency.
        let val = super::portfolio::value_holdings(&client, &p_syms, market, None).await?;
        let totals = val.totals(&portfolio);
        let (total_cost, total_value) = (totals.cost, totals.value);
        let pnl = total_value - total_cost;
        let pnl_pct = totals.pnl_pct();
        let cur = Some(val.base());
        let csym = market::currency_symbol(cur);
        print_section(&tf(Msg::PortfolioCount, &[&portfolio.holdings.len()]));
        print_kv(t(Msg::TotalValue), &format_price(total_value, cur));
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
        print_kv(t(Msg::PnL), &pnl_str);
        if let Some(note) = val.fx_note() {
            println!("  {}", note.dimmed());
        }
        totals.print_exclusions();
    }

    let wl = Watchlist::load()?;
    if !wl.symbols.is_empty() {
        let w_refs: Vec<&str> = wl.symbols.iter().map(|s| s.as_str()).collect();
        let w_quotes = client.get_quote(&w_refs).await?;
        print_section(&tf(Msg::WatchlistCount, &[&wl.symbols.len()]));
        for q in &w_quotes {
            let cur = q.currency.as_deref();
            let sym = q.symbol.as_deref().unwrap_or("???");
            let pct = q.regular_market_change_percent.unwrap_or(0.0);
            let pct_str = if pct >= 0.0 {
                format!("+{:.2}%", pct).green().to_string()
            } else {
                format!("{:.2}%", pct).red().to_string()
            };
            println!(
                "  {:<12} {}  {}",
                sym,
                price_or_na(q.regular_market_price, cur, sym),
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
            print_section(t(Msg::TriggeredAlerts));
            for line in &triggered {
                println!("  {} {}", "⚠".yellow(), line.red());
            }
        }
    }
    println!();
    if portfolio.holdings.is_empty() && wl.symbols.is_empty() && store.alerts.is_empty() {
        print_section(t(Msg::GetStarted));
        for (cmd, what) in [
            ("stockwise portfolio add TCS 10 3500", Msg::GetStartedTrack),
            ("stockwise import kite holdings.csv", Msg::GetStartedImport),
            ("stockwise watch add INFY HDFCBANK", Msg::GetStartedWatch),
            (
                "stockwise alert add RELIANCE below 1200",
                Msg::GetStartedAlert,
            ),
        ] {
            println!("  {:<39} {}", cmd, t(what));
        }
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
        bail!("{}", tf(Msg::NoTickersMatch, &[&query]));
    }
    print_header(&tf(Msg::SearchTitle, &[&query]));
    println!(
        "  {} {} {} {}",
        pad_end(t(Msg::Symbol), 14).bold(),
        pad_end(t(Msg::Name), 30).bold(),
        pad_start(t(Msg::Type), 10).bold(),
        pad_start(t(Msg::Exchange), 10).bold()
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

/// `stockwise doctor` — one screen that answers "is my setup working?".
pub async fn cmd_doctor(market: Market) -> Result<()> {
    let ok = |s: &str| format!("{} {}", "✓".green(), s);
    let warn = |s: &str| format!("{} {}", "!".yellow(), s);
    let bad = |s: &str| format!("{} {}", "✗".red(), s);

    print_header(&tf(Msg::DoctorTitle, &[&env!("CARGO_PKG_VERSION")]));

    // Defaults
    let market_src = if std::env::var_os("STOCKWISE_MARKET").is_some() {
        t(Msg::DoctorMarketFromEnv)
    } else {
        t(Msg::DoctorMarketDefault)
    };
    let market_name = match market {
        Market::In => "in (India, NSE)",
        Market::Us => "us",
        Market::Crypto => "crypto",
        Market::Fx => "fx",
    };
    let market_name = if market == Market::Crypto {
        format!("{} (vs {})", market_name, market::quote_currency())
    } else {
        market_name.to_string()
    };
    print_kv(
        t(Msg::DoctorMarket),
        &format!("{} — {}", market_name, market_src),
    );
    let lang_src = t(match crate::i18n::lang_source() {
        crate::i18n::LangSource::Flag => Msg::DoctorLangFlag,
        crate::i18n::LangSource::StockwiseLang => Msg::DoctorLangEnv,
        crate::i18n::LangSource::Locale => Msg::DoctorLangLocale,
        crate::i18n::LangSource::Default => Msg::DoctorLangDefault,
    });
    print_kv(
        t(Msg::DoctorLanguage),
        &format!("{} — {}", crate::i18n::lang().code(), lang_src),
    );

    // Data directory
    match crate::store::data_dir() {
        Ok(dir) => {
            print_kv(t(Msg::DoctorDataDir), &ok(&dir.display().to_string()));
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
                t(Msg::DoctorDataFiles),
                &if present.is_empty() {
                    t(Msg::DoctorNoFilesYet).to_string()
                } else {
                    present.join(", ")
                },
            );
        }
        Err(e) => print_kv(t(Msg::DoctorDataDir), &bad(&format!("{:#}", e))),
    }

    // Market data: Yahoo is always needed (fundamentals, news, fallback).
    let started = std::time::Instant::now();
    let yahoo = match YahooClient::new().await {
        Ok(client) => client
            .yahoo_quote(&["^GSPC"])
            .await
            .map(|q| !q.is_empty())
            .unwrap_or(false),
        Err(_) => false,
    };
    print_kv(
        "Yahoo Finance",
        &if yahoo {
            ok(&tf(Msg::DoctorReachable, &[&started.elapsed().as_millis()]))
        } else {
            bad(t(Msg::DoctorYahooUnreachable))
        },
    );

    // Optional data provider
    use crate::providers::{MarketData, Selection};
    let provider_line = match Selection::from_env() {
        Selection::Yahoo => t(Msg::DoctorProviderDefault).dimmed().to_string(),
        Selection::Unknown(value) => warn(&tf(Msg::DoctorProviderUnknown, &[&value])),
        Selection::MissingKey(kind) => warn(&tf(
            Msg::DoctorProviderNoKey,
            &[&kind.name(), &kind.key_env()],
        )),
        Selection::Ready(provider) => {
            let probe = provider.probe_symbol();
            let started = std::time::Instant::now();
            match provider.quotes(&[probe]).await {
                Ok(q) if q.iter().any(|q| q.regular_market_price.is_some()) => ok(&tf(
                    Msg::DoctorProviderAnswered,
                    &[&provider.name(), &probe, &started.elapsed().as_millis()],
                )),
                Ok(_) => warn(&tf(
                    Msg::DoctorProviderFellBack,
                    &[&provider.name(), &"no price returned"],
                )),
                Err(e) => warn(&tf(
                    Msg::DoctorProviderFellBack,
                    &[&provider.name(), &format!("{:#}", e)],
                )),
            }
        }
    };
    print_kv(t(Msg::DoctorDataProvider), &provider_line);

    // Cache
    let redis = crate::api::connect_redis().await.is_some();
    print_kv(
        t(Msg::DoctorRedis),
        &if redis {
            ok(t(Msg::DoctorConnected))
        } else {
            t(Msg::DoctorRedisNotUsed).dimmed().to_string()
        },
    );

    // AI — provider and model, never the key itself.
    let ai = crate::ai::AiClient::connect().await;
    let ai_line = if ai.config_error().is_some() {
        let value = std::env::var("STOCKWISE_AI_PROVIDER").unwrap_or_default();
        bad(&tf(Msg::DoctorAiUnknown, &[&value.trim()]))
    } else {
        let provider = ai.provider();
        match provider.key_env() {
            Some(key_env) if ai.has_api_key() => ok(&tf(
                Msg::DoctorCloudReady,
                &[&provider.name(), &ai.model(), &key_env],
            )),
            Some(key_env) => warn(&tf(Msg::DoctorCloudNoKey, &[&provider.name(), &key_env])),
            None => match ai.has_model().await {
                Some(true) => ok(&tf(Msg::DoctorOllamaReady, &[&ai.model()])),
                Some(false) => warn(&tf(Msg::DoctorOllamaNoModel, &[&ai.model()])),
                None => t(Msg::DoctorOllamaDown).dimmed().to_string(),
            },
        }
    };
    print_kv(t(Msg::DoctorAi), &ai_line);

    // Brokers: Angel One always; the others once they have a config file.
    // Read-only: doctor never moves secrets into the keyring.
    let backend = crate::brokers::secrets::backend();
    print_kv(
        t(Msg::DoctorKeyring),
        &match backend.available() {
            Ok(()) => ok(&tf(Msg::DoctorKeyringAvailable, &[&backend.name()])),
            Err(reason) => tf(Msg::DoctorKeyringNotUsed, &[&reason])
                .dimmed()
                .to_string(),
        },
    );
    for kind in crate::brokers::BrokerKind::ALL {
        let line = match crate::brokers::config_status(kind, false) {
            Ok(st) if !st.exists && kind != crate::brokers::BrokerKind::Angel => continue,
            Ok(st) if st.configured => {
                let limits = crate::brokers::order_limits(kind).unwrap_or_default();
                let mut extra = limits
                    .max_order_value
                    .map(|v| tf(Msg::DoctorAngelMaxOrder, &[&format!("{:.0}", v)]))
                    .unwrap_or_default();
                if let Some(s) = &st.session {
                    extra.push_str(&format!(", {s}"));
                }
                match (kind.experimental(), limits.enable_orders) {
                    (true, true) => extra.push_str(&format!(", {}", t(Msg::DoctorBrokerOrdersOn))),
                    (true, false) => {
                        extra.push_str(&format!(", {}", t(Msg::DoctorBrokerOrdersOff)))
                    }
                    (false, true) => extra.push_str(&format!(", {}", t(Msg::DoctorAngelStopGttOn))),
                    (false, false) => {}
                }
                extra.push_str(&format!(
                    ", {}",
                    crate::brokers::secrets::describe(&st.storage, backend)
                ));
                ok(&tf(Msg::DoctorAngelConfigured, &[&st.summary, &extra]))
            }
            Ok(st) if st.exists => warn(&tf(
                Msg::DoctorBrokerPlaceholder,
                &[&st.path.display(), &kind.id()],
            )),
            Ok(_) => t(Msg::DoctorAngelNotConfigured).dimmed().to_string(),
            Err(e) => warn(&format!("{:#}", e)),
        };
        let label = if kind == crate::brokers::BrokerKind::Angel {
            t(Msg::DoctorAngel).to_string()
        } else {
            kind.display()
        };
        print_kv(&label, &line);
    }
    println!();

    if !yahoo {
        bail!("{}", t(Msg::DoctorMarketDataDown));
    }
    Ok(())
}

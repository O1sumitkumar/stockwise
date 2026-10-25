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

/// One quote as `quote --csv` prints it (`quote --json` keeps Yahoo's
/// camelCase fields).
#[derive(Debug, Serialize)]
pub(crate) struct QuoteRow {
    pub(crate) symbol: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) currency: Option<String>,
    pub(crate) price: Option<f64>,
    pub(crate) change: Option<f64>,
    pub(crate) change_percent: Option<f64>,
    pub(crate) open: Option<f64>,
    pub(crate) day_low: Option<f64>,
    pub(crate) day_high: Option<f64>,
    pub(crate) year_low: Option<f64>,
    pub(crate) year_high: Option<f64>,
    pub(crate) volume: Option<u64>,
    pub(crate) avg_volume_3m: Option<u64>,
    pub(crate) market_cap: Option<f64>,
    pub(crate) pe_ttm: Option<f64>,
    pub(crate) eps_ttm: Option<f64>,
    pub(crate) dividend_yield: Option<f64>,
}

impl CsvRow for QuoteRow {
    const COLUMNS: &'static [&'static str] = &[
        "symbol",
        "name",
        "currency",
        "price",
        "change",
        "change_percent",
        "open",
        "day_low",
        "day_high",
        "year_low",
        "year_high",
        "volume",
        "avg_volume_3m",
        "market_cap",
        "pe_ttm",
        "eps_ttm",
        "dividend_yield",
    ];
}

impl From<&crate::api::Quote> for QuoteRow {
    fn from(q: &crate::api::Quote) -> Self {
        Self {
            symbol: q.symbol.clone(),
            name: q.long_name.clone().or_else(|| q.short_name.clone()),
            currency: q.currency.clone(),
            price: q.regular_market_price,
            change: q.regular_market_change,
            change_percent: q.regular_market_change_percent,
            open: q.regular_market_open,
            day_low: q.regular_market_day_low,
            day_high: q.regular_market_day_high,
            year_low: q.fifty_two_week_low,
            year_high: q.fifty_two_week_high,
            volume: q.regular_market_volume,
            avg_volume_3m: q.average_daily_volume_3_month,
            market_cap: q.market_cap,
            pe_ttm: q.trailing_pe,
            eps_ttm: q.eps_trailing_twelve_months,
            dividend_yield: q.trailing_annual_dividend_yield,
        }
    }
}

pub async fn cmd_quote(symbols: &[String], market: Market, fmt: Format) -> Result<()> {
    let resolved = resolve_symbols(symbols, market);
    let client = YahooClient::new().await?;
    // Yahoo silently drops unknown symbols; name them instead.
    let views::QuoteSet { quotes, missing } = views::quotes(&client, &resolved).await?;
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

    if !fmt.is_table() {
        if fmt == Format::Json {
            output::print_json(&quotes)?;
        } else {
            let rows: Vec<QuoteRow> = quotes.iter().map(QuoteRow::from).collect();
            output::print_csv(&rows)?;
        }
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

/// `markets --csv` columns; the rows come from [`views::market_sections`].
impl CsvRow for views::IndexRow {
    const COLUMNS: &'static [&'static str] = &[
        "region",
        "symbol",
        "name",
        "price",
        "change",
        "change_percent",
        "currency",
    ];
}

/// `▲ 1.23 (0.45%)` for an index row, with as many decimals as the price needs.
fn row_change(r: &views::IndexRow) -> String {
    format_change_dp(
        r.change.unwrap_or(0.0),
        r.change_percent.unwrap_or(0.0),
        price_decimals(market::AssetClass::of(&r.symbol), r.price.unwrap_or(0.0)),
    )
}

pub async fn cmd_markets(market: Market, fmt: Format, crypto: bool, fx: bool) -> Result<()> {
    let client = YahooClient::new().await?;
    let sections = views::market_sections(&client, market, crypto, fx).await?;

    if !fmt.is_table() {
        let rows: Vec<views::IndexRow> = sections.into_iter().flat_map(|s| s.rows).collect();
        return match fmt {
            Format::Csv => output::print_csv(&rows),
            _ => output::print_json(&rows),
        };
    }

    for section in &sections {
        print_header(section.title);
        for r in &section.rows {
            let cur = r.currency.as_deref();
            println!(
                "  {} {}  {}",
                pad_end(&r.name, 16).bold(),
                price_or_na(r.price, cur, &r.symbol).bold(),
                row_change(r)
            );
        }
    }
    println!();
    Ok(())
}

// ── Watchlist ──

pub async fn cmd_dashboard(market: Market) -> Result<()> {
    let client = YahooClient::new().await?;
    let section = views::section_spec(market);
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
        // Missing data (no price, too little history) never reads as triggered.
        let rows = views::alert_rows(&client, &store.alerts, &a_quotes, chrono::Utc::now()).await;
        let triggered: Vec<String> = rows
            .iter()
            .zip(&store.alerts)
            .filter(|(row, _)| row.status == "triggered")
            .map(|(_, alert)| format!("{} {}", alert.symbol, alert.describe(crate::i18n::lang())))
            .collect();
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

/// One search hit as `search --csv` prints it (`search --json` keeps
/// Yahoo's field names).
#[derive(Debug, Serialize)]
pub(crate) struct SearchRow {
    pub(crate) symbol: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) quote_type: Option<String>,
    pub(crate) exchange: Option<String>,
}

impl CsvRow for SearchRow {
    const COLUMNS: &'static [&'static str] = &["symbol", "name", "quote_type", "exchange"];
}

pub async fn cmd_search(query: &str, fmt: Format) -> Result<()> {
    let client = YahooClient::new().await?;
    let results = client.search(query).await?;
    match fmt {
        Format::Json => return output::print_json(&results),
        Format::Csv => {
            let rows: Vec<SearchRow> = results
                .iter()
                .map(|r| SearchRow {
                    symbol: r.symbol.clone(),
                    name: r.long_name.clone().or_else(|| r.short_name.clone()),
                    quote_type: r.quote_type.clone(),
                    exchange: r.exchange.clone(),
                })
                .collect();
            return output::print_csv(&rows);
        }
        Format::Table => {}
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
pub async fn cmd_doctor(market: Market, sources: crate::config::CliSources) -> Result<()> {
    let ok = |s: &str| format!("{} {}", "✓".green(), s);
    let warn = |s: &str| format!("{} {}", "!".yellow(), s);
    let bad = |s: &str| format!("{} {}", "✗".red(), s);

    print_header(&tf(Msg::DoctorTitle, &[&env!("CARGO_PKG_VERSION")]));

    // Config file and defaults
    let cfg = crate::config::state();
    let cfg_path = cfg
        .and_then(|s| s.path.as_ref())
        .map_or("unknown (no config directory)".to_string(), |p| {
            p.display().to_string()
        });
    let cfg_invalid = match cfg.map(|s| &s.status) {
        Some(crate::config::Status::Loaded) => {
            print_kv("Config file", &ok(&format!("{} (loaded)", cfg_path)));
            if let Some(p) = crate::config::exposed_secrets() {
                println!(
                    "  {:<28} {}",
                    "",
                    warn(&format!(
                        "holds notification secrets and other users can read it — run `chmod 600 {}`",
                        p.display()
                    ))
                );
            }
            None
        }
        Some(crate::config::Status::Invalid(msg)) => {
            print_kv("Config file", &bad(&format!("{} (invalid)", cfg_path)));
            for line in msg.lines() {
                println!("  {:<28} {}", "", line.red());
            }
            Some(cfg_path)
        }
        _ => {
            print_kv(
                "Config file",
                &format!(
                    "{} {}",
                    cfg_path,
                    "(none; optional — `stockwise config init` creates one)".dimmed()
                ),
            );
            None
        }
    };
    let market_src = match sources.market {
        crate::config::Source::Flag => "from -m/--market",
        crate::config::Source::Env => t(Msg::DoctorMarketFromEnv),
        crate::config::Source::File => "from the config file",
        crate::config::Source::Default => t(Msg::DoctorMarketDefault),
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
        crate::i18n::LangSource::Config => Msg::DoctorLangConfig,
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

    if let Some(path) = cfg_invalid {
        bail!(
            "Config file {} is invalid — fix it, or replace it with `stockwise config init --force`",
            path
        );
    }
    if !yahoo {
        bail!("{}", t(Msg::DoctorMarketDataDown));
    }
    Ok(())
}

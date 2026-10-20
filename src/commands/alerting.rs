//! `alert watch` — a loop that checks price alerts while their market is
//! open and notifies once per trigger — and `notify`, which shows and tests
//! the notification channels.

use super::worker::Feed;
use super::*;
use crate::clock;
use crate::daemon::Shutdown;
use crate::notify::{Channel, Message, Notifier};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Remembers which alerts already fired, across restarts.
const WATCH_STATE: &str = "alert_watch.json";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Fired {
    /// Exchange-local date of the last notification.
    last: NaiveDate,
    /// The condition has been false since the last notification.
    armed: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WatchState {
    fired: HashMap<String, Fired>,
}

impl WatchState {
    /// Whether an alert whose condition is `hit` should notify now. An alert
    /// notifies when its condition becomes true, re-arms once the condition
    /// has been false, and notifies at most once a day.
    fn should_notify(&mut self, key: &str, hit: bool, today: NaiveDate) -> bool {
        match (hit, self.fired.get_mut(key)) {
            (true, None) => {
                self.fired.insert(
                    key.to_string(),
                    Fired {
                        last: today,
                        armed: false,
                    },
                );
                true
            }
            (true, Some(f)) if f.armed && f.last != today => {
                *f = Fired {
                    last: today,
                    armed: false,
                };
                true
            }
            (true, Some(_)) => false,
            (false, Some(f)) => {
                f.armed = true;
                false
            }
            (false, None) => false,
        }
    }

    /// Forget alerts that were removed.
    fn retain(&mut self, keys: &[String]) {
        self.fired.retain(|k, _| keys.contains(k));
    }
}

fn alert_key(a: &crate::alerts::Alert) -> String {
    format!("{}|{}|{}|{}", a.symbol, a.condition, a.target, a.created_at)
}

/// The exchange whose hours apply to a symbol, or None for instruments that
/// trade around the clock (crypto, FX) and other exchanges' listings.
fn alert_market(symbol: &str) -> Option<Market> {
    let s = symbol.to_uppercase();
    if s.ends_with("-USD") || s.ends_with("-INR") || s.contains('=') {
        None
    } else if s.ends_with(".NS")
        || s.ends_with(".BO")
        || s.starts_with("^NSE")
        || s.starts_with("^CNX")
        || s == "^BSESN"
    {
        Some(Market::In)
    } else if s.contains('.') {
        None
    } else {
        Some(Market::Us)
    }
}

fn is_due(symbol: &str, now: DateTime<Utc>) -> bool {
    alert_market(symbol).is_none_or(|m| clock::is_open(m, now))
}

/// How long to sleep when nothing is due: until the earliest next open of
/// the markets the alerts trade on, in chunks of at most 30 minutes.
fn wait_for_open(markets: &[Market], now: DateTime<Utc>, interval: Duration) -> Duration {
    markets
        .iter()
        .filter_map(|m| clock::next_open(*m, now))
        .min()
        .and_then(|t| (t - now).to_std().ok())
        .unwrap_or(interval)
        .clamp(interval, Duration::from_secs(1800))
}

struct Trigger {
    symbol: String,
    condition: String,
    target: f64,
    price: f64,
    currency: Option<String>,
}

impl Trigger {
    fn text(&self) -> String {
        format!(
            "{} {} {:.2} — now {}",
            self.symbol,
            self.condition,
            self.target,
            plain_price(self.price, self.currency.as_deref())
        )
    }
}

fn plain_price(price: f64, currency: Option<&str>) -> String {
    format!("{}{:.2}", market::currency_symbol(currency), price)
}

fn trigger_message(triggers: &[Trigger]) -> Option<Message> {
    match triggers {
        [] => None,
        [t] => Some(Message::new(format!("StockWise alert: {}", t.text()), "")),
        many => Some(Message::new(
            format!("StockWise: {} alerts triggered", many.len()),
            many.iter()
                .map(Trigger::text)
                .collect::<Vec<_>>()
                .join("\n"),
        )),
    }
}

/// The result of one pass over the alerts.
struct Pass {
    /// Alerts configured.
    total: usize,
    /// Alerts whose market was open (all of them with `--once`).
    due: usize,
    /// Markets the alerts trade on (round-the-clock symbols excluded).
    markets: Vec<Market>,
    triggers: Vec<Trigger>,
}

/// One pass over the alerts that are due (all of them if `all`).
async fn check_alerts(
    feed: &mut Feed,
    state: &mut WatchState,
    market: Market,
    now: DateTime<Utc>,
    all: bool,
    verbose: bool,
) -> Result<Pass> {
    let store = AlertStore::load()?;
    let keys: Vec<String> = store.alerts.iter().map(alert_key).collect();
    state.retain(&keys);
    let mut markets = Vec::new();
    for m in store.alerts.iter().filter_map(|a| alert_market(&a.symbol)) {
        if !markets.contains(&m) {
            markets.push(m);
        }
    }
    let due: Vec<&crate::alerts::Alert> = store
        .alerts
        .iter()
        .filter(|a| all || is_due(&a.symbol, now))
        .collect();
    let mut pass = Pass {
        total: store.alerts.len(),
        due: due.len(),
        markets,
        triggers: Vec::new(),
    };
    if due.is_empty() {
        return Ok(pass);
    }
    let mut symbols: Vec<&str> = due.iter().map(|a| a.symbol.as_str()).collect();
    symbols.sort();
    symbols.dedup();
    let quotes = feed.client().await?.get_quote(&symbols).await?;

    for alert in due {
        let quote = quotes
            .iter()
            .find(|q| q.symbol.as_deref() == Some(alert.symbol.as_str()));
        let Some(price) = quote.and_then(|q| q.regular_market_price) else {
            if verbose {
                println!(
                    "  {} {} — no live price, skipped",
                    "?".yellow(),
                    alert.symbol.cyan()
                );
            }
            continue;
        };
        let currency = quote.and_then(|q| q.currency.clone());
        let hit = match alert.condition {
            AlertCondition::Above => price >= alert.target,
            AlertCondition::Below => price <= alert.target,
        };
        let today = clock::session_date(alert_market(&alert.symbol).unwrap_or(market), now);
        let fire = state.should_notify(&alert_key(alert), hit, today);
        if verbose {
            let status = if fire {
                "TRIGGERED".red().bold().to_string()
            } else if hit {
                "triggered (already notified)".yellow().to_string()
            } else {
                "waiting".dimmed().to_string()
            };
            println!(
                "  {} {} {:.2} | Now: {} | {}",
                alert.symbol.cyan(),
                alert.condition,
                alert.target,
                format_price(price, currency.as_deref()),
                status
            );
        }
        if fire {
            pass.triggers.push(Trigger {
                symbol: alert.symbol.clone(),
                condition: alert.condition.to_string(),
                target: alert.target,
                price,
                currency,
            });
        }
    }
    Ok(pass)
}

/// `alert watch`: check alerts every `interval` seconds while their market
/// is open; notify each trigger once. `once` runs a single pass over every
/// alert regardless of market hours (for cron).
pub async fn cmd_alert_watch(interval: u64, once: bool, market: Market) -> Result<()> {
    let interval = Duration::from_secs(interval);
    let notifier = Notifier::from_env();
    let mut state: WatchState = crate::store::load(WATCH_STATE)?;
    let mut feed = Feed::default();

    if once {
        print_header("Alert Watch — single pass");
        let now = Utc::now();
        let pass = check_alerts(&mut feed, &mut state, market, now, true, true).await?;
        crate::store::save(WATCH_STATE, &state, false)?;
        if pass.total == 0 {
            println!(
                "  {}",
                "No alerts. Use 'stockwise alert add <SYMBOL> above/below <PRICE>'.".dimmed()
            );
        } else {
            if pass.triggers.is_empty() {
                println!("\n  {}", "No new triggers.".dimmed());
            }
            report_triggers(&pass.triggers, market, &notifier).await;
        }
        println!();
        return Ok(());
    }

    let mut shutdown = Shutdown::install("Alert state is saved after every check.");
    print_header("Alert Watch");
    println!(
        "  Checking every {}s while each alert's market is open (NSE 09:15–15:30 IST, NYSE 09:30–16:00 ET).",
        interval.as_secs()
    );
    let channels = notifier.config().channels();
    if channels.is_empty() {
        println!(
            "  {} No notification channels configured — triggers print here only. See {}.",
            "→".dimmed(),
            "stockwise notify".cyan()
        );
    } else {
        let names: Vec<String> = channels.iter().map(Channel::to_string).collect();
        println!("  Notifications: {}", names.join(", "));
    }
    println!("  {} Press Ctrl+C to stop.\n", "→".dimmed());

    let mut passes = 0u32;
    let mut idle_note: Option<String> = None;
    loop {
        if shutdown.requested() {
            println!("\n  {} Alert watch stopped.", "✓".green());
            return Ok(());
        }
        let now = Utc::now();
        let pass = tokio::select! {
            r = check_alerts(&mut feed, &mut state, market, now, false, false) => Some(r),
            _ = shutdown.wait() => None,
        };
        let Some(pass) = pass else { continue };
        let wait = match pass {
            Ok(pass) => {
                feed.succeeded();
                if let Err(e) = crate::store::save(WATCH_STATE, &state, false) {
                    eprintln!("  {} Could not save {}: {:#}", "!".yellow(), WATCH_STATE, e);
                }
                report_triggers(&pass.triggers, market, &notifier).await;
                if pass.total == 0 {
                    note_once(
                        &mut idle_note,
                        "No alerts yet — add one with `stockwise alert add`; watching for new ones.",
                    );
                    interval
                } else if pass.due == 0 {
                    let next = pass
                        .markets
                        .iter()
                        .filter_map(|m| {
                            clock::next_open(*m, now).map(|t| clock::format_local(*m, t))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    note_once(
                        &mut idle_note,
                        &format!("Markets closed — next open: {}.", next),
                    );
                    wait_for_open(&pass.markets, now, interval)
                } else {
                    idle_note = None;
                    passes += 1;
                    if passes.is_multiple_of(15) {
                        println!(
                            "  {} [{}] {} alert(s) checked, all quiet",
                            "●".dimmed(),
                            clock::local(market, now).format("%H:%M:%S"),
                            pass.due
                        );
                    }
                    interval
                }
            }
            Err(e) => {
                let wait = feed.failed();
                eprintln!(
                    "  {} [{}] Check failed: {:#} — retrying in {}s",
                    "!".yellow(),
                    clock::local(market, Utc::now()).format("%H:%M:%S"),
                    e,
                    wait.as_secs()
                );
                wait
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.wait() => {}
        }
    }
}

fn note_once(last: &mut Option<String>, note: &str) {
    if last.as_deref() != Some(note) {
        println!("  {} {}", "→".dimmed(), note);
        *last = Some(note.to_string());
    }
}

async fn report_triggers(triggers: &[Trigger], market: Market, notifier: &Notifier) {
    if triggers.is_empty() {
        return;
    }
    let now = clock::local(market, Utc::now()).format("%H:%M:%S");
    for t in triggers {
        println!(
            "  {} [{}] {} {} {:.2} — now {}",
            "⚠".yellow().bold(),
            now,
            t.symbol.cyan().bold(),
            t.condition,
            t.target,
            format_price(t.price, t.currency.as_deref()).bold()
        );
    }
    if let Some(msg) = trigger_message(triggers) {
        notifier.send_logged(&msg).await;
    }
}

/// `notify` (status) and `notify test`.
pub async fn cmd_notify(action: &str) -> Result<()> {
    let notifier = Notifier::from_env();
    let cfg = notifier.config();
    print_header("Notifications");
    for channel in [
        Channel::Telegram,
        Channel::Slack,
        Channel::Discord,
        Channel::Email,
        Channel::Desktop,
    ] {
        match cfg.destination(channel) {
            Some(dest) if cfg.channels().contains(&channel) => {
                println!("  {} {:<9} {}", "✓".green(), channel, dest)
            }
            _ => println!(
                "  {} {:<9} {}",
                "·".dimmed(),
                channel,
                "not configured".dimmed()
            ),
        }
    }
    for p in &cfg.problems {
        println!("  {} {}", "!".yellow(), p);
    }

    if cfg.channels().is_empty() {
        if action == "test" && !cfg.problems.is_empty() {
            bail!("No channel is fully configured — fix the problems above.");
        }
        println!("\n  Set any of these environment variables to turn a channel on:\n");
        for (var, what) in crate::notify::ENV_VARS {
            println!("    {:<28} {}", var.bold(), what.dimmed());
        }
        println!(
            "\n  Then run {} to send a test message. Alerts ({}) and the intraday daemon use every configured channel.\n",
            "stockwise notify test".cyan(),
            "stockwise alert watch".cyan()
        );
        return Ok(());
    }
    if action != "test" {
        println!(
            "\n  {} Run {} to send a test message.\n",
            "→".dimmed(),
            "stockwise notify test".cyan()
        );
        return Ok(());
    }

    println!();
    let results = notifier
        .send(&Message::new(
            "StockWise test notification",
            "If you can read this, StockWise can reach you here.",
        ))
        .await;
    let mut failed = 0;
    for (channel, result) in &results {
        match result {
            Ok(()) => println!("  {} {:<9} sent", "✓".green(), channel),
            Err(e) => {
                failed += 1;
                println!("  {} {:<9} {:#}", "✗".red(), channel, e);
            }
        }
    }
    println!();
    if failed > 0 {
        bail!("{} of {} channel(s) failed.", failed, results.len());
    }
    if !cfg.problems.is_empty() {
        bail!("Sent, but some channels are misconfigured — see the problems above.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    #[test]
    fn an_alert_notifies_once_until_it_rearms() {
        let mut s = WatchState::default();
        assert!(!s.should_notify("a", false, day(21)));
        assert!(s.should_notify("a", true, day(21)));
        // Still above the level: no repeat.
        assert!(!s.should_notify("a", true, day(21)));
        assert!(!s.should_notify("a", true, day(22)));
        // Dips back, crosses again the same day: still quiet (once a day).
        assert!(!s.should_notify("a", false, day(22)));
        assert!(s.should_notify("a", true, day(23)));
        assert!(!s.should_notify("a", false, day(23)));
        assert!(!s.should_notify("a", true, day(23)));
    }

    #[test]
    fn removed_alerts_are_forgotten() {
        let mut s = WatchState::default();
        s.should_notify("a", true, day(21));
        s.should_notify("b", true, day(21));
        s.retain(&["b".to_string()]);
        assert!(s.fired.contains_key("b") && !s.fired.contains_key("a"));
        // A re-added alert with the same key fires again.
        assert!(s.should_notify("a", true, day(21)));
    }

    #[test]
    fn alert_markets_follow_the_symbol() {
        assert_eq!(alert_market("TCS.NS"), Some(Market::In));
        assert_eq!(alert_market("RELIANCE.BO"), Some(Market::In));
        assert_eq!(alert_market("^NSEI"), Some(Market::In));
        assert_eq!(alert_market("AAPL"), Some(Market::Us));
        assert_eq!(alert_market("^GSPC"), Some(Market::Us));
        assert_eq!(alert_market("BTC-USD"), None);
        assert_eq!(alert_market("USDINR=X"), None);
    }

    #[test]
    fn alerts_are_due_only_while_their_market_is_open() {
        // 10:00 IST on Wed 21 Oct 2026 = 00:30 ET: NSE open, NYSE closed.
        let t = clock::at(
            Market::In,
            day(21),
            chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
        );
        assert!(is_due("TCS.NS", t));
        assert!(!is_due("AAPL", t));
        assert!(is_due("BTC-USD", t));
        // Dussehra: NSE closed all day.
        let holiday = clock::at(
            Market::In,
            day(20),
            chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
        );
        assert!(!is_due("TCS.NS", holiday));
    }

    #[test]
    fn closed_markets_sleep_until_the_open_within_bounds() {
        let interval = Duration::from_secs(60);
        // 09:10 IST → 5 minutes to the NSE open.
        let t = clock::at(
            Market::In,
            day(21),
            chrono::NaiveTime::from_hms_opt(9, 10, 0).unwrap(),
        );
        assert_eq!(
            wait_for_open(&[Market::In], t, interval),
            Duration::from_secs(300)
        );
        // Saturday: capped at 30 minutes.
        let sat = clock::at(
            Market::In,
            day(24),
            chrono::NaiveTime::from_hms_opt(9, 10, 0).unwrap(),
        );
        assert_eq!(
            wait_for_open(&[Market::In, Market::Us], sat, interval),
            Duration::from_secs(1800)
        );
    }

    #[test]
    fn several_triggers_become_one_message() {
        let t = |s: &str| Trigger {
            symbol: s.into(),
            condition: "above".into(),
            target: 100.0,
            price: 101.5,
            currency: Some("INR".into()),
        };
        let one = trigger_message(&[t("TCS.NS")]).unwrap();
        assert_eq!(
            one.title,
            "StockWise alert: TCS.NS above 100.00 — now ₹101.50"
        );
        let two = trigger_message(&[t("TCS.NS"), t("INFY.NS")]).unwrap();
        assert_eq!(two.title, "StockWise: 2 alerts triggered");
        assert_eq!(two.body.lines().count(), 2);
        assert!(trigger_message(&[]).is_none());
    }
}

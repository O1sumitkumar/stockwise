//! A historical replay: trade a past market one day at a time with paper
//! money, without seeing what comes next.
//!
//! The current day is `calendar[day]`; its close is the latest price the
//! player knows. Orders placed today fill at the next trading day's open,
//! paying slippage and the backtest cost model's charges. After the fills,
//! stops and targets are checked against that day's low and high (a gap
//! through the level fills at the open; when both are touched the stop is
//! assumed first, as in the backtester). Every view built here reads prices
//! through [`Tape::upto`] at the current day.

use super::challenge::{self, Active, Facts, Outcome, Violation};
use super::scenario::Scenario;
use super::score::{self, BuyFact, RunFacts, RunInput, Scorecard};
use super::tape::Tape;
use crate::backtest::costs::{CostModel, CostPreset, Side};
use crate::output::round;
use anyhow::{Result, bail};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FILE: &str = "sim_replay.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub id: u32,
    pub side: OrderSide,
    pub symbol: String,
    pub qty: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<f64>,
    pub placed: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub qty: u64,
    /// Average fill price (after slippage, before charges).
    pub avg_price: f64,
    pub stop: Option<f64>,
    pub target: Option<f64>,
    pub opened: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillReason {
    Order,
    StopLoss,
    Target,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub date: NaiveDate,
    pub symbol: String,
    pub side: OrderSide,
    pub qty: u64,
    pub price: f64,
    pub charges: f64,
    pub reason: FillReason,
    /// Buys: whether the position had a stop-loss after the fill.
    #[serde(default)]
    pub with_stop: bool,
    /// Buys: the position's share of the previous close's equity.
    #[serde(default)]
    pub weight_percent: f64,
    /// Sells: realised profit against the average price, after the sell's
    /// charges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pnl: Option<f64>,
}

/// The account at one close.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub date: NaiveDate,
    pub equity: f64,
    pub invested: f64,
    /// Money added before this day's open (SIP contributions).
    pub flow: f64,
    pub benchmark: f64,
    pub positions: usize,
    pub sectors: usize,
}

/// Something that happened while stepping a day.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Contribution {
        amount: f64,
    },
    Fill(Fill),
    Rejected {
        symbol: String,
        side: OrderSide,
        qty: u64,
        reason: String,
    },
    /// The stock did not trade today; the order waits for the next day.
    Waiting {
        symbol: String,
        side: OrderSide,
    },
    RuleBroken(Violation),
    ChallengeOver(Outcome),
    /// The replay reached its last day.
    LastDay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replay {
    pub scenario: Scenario,
    /// Trading days from the scenario's start to its end (the benchmark's).
    pub calendar: Vec<NaiveDate>,
    /// Index of the current day in `calendar`.
    pub day: usize,
    pub capital: f64,
    pub cash: f64,
    /// Cost preset: auto, none, in-delivery, in-intraday or us.
    pub costs: String,
    pub positions: BTreeMap<String, Position>,
    pub orders: Vec<Order>,
    pub fills: Vec<Fill>,
    pub marks: Vec<Mark>,
    pub costs_paid: f64,
    pub next_id: u32,
    #[serde(default)]
    pub challenge: Option<Active>,
    #[serde(default)]
    pub finished_challenges: Vec<Outcome>,
    /// Added on the first trading day of each month (SIP challenge).
    #[serde(default)]
    pub contribution: Option<f64>,
}

pub fn cost_preset(name: &str) -> Option<CostPreset> {
    Some(match name {
        "auto" => CostPreset::Auto,
        "none" => CostPreset::None,
        "in-delivery" => CostPreset::InDelivery,
        "in-intraday" => CostPreset::InIntraday,
        "us" => CostPreset::Us,
        _ => return None,
    })
}

impl Replay {
    /// A replay over `tape`, on the first trading day of the scenario.
    pub fn new(scenario: Scenario, tape: &Tape, capital: f64, costs: &str) -> Result<Replay> {
        let Some(preset) = cost_preset(costs) else {
            bail!("Unknown cost model {costs:?}: use auto, none, in-delivery, in-intraday or us");
        };
        for m in &scenario.universe {
            if tape.has(&m.symbol) {
                preset
                    .model_for(&m.symbol, tape.currency(&m.symbol))
                    .map_err(anyhow::Error::msg)?;
            }
        }
        let calendar = tape.dates(&scenario.benchmark, scenario.from, scenario.to);
        if calendar.len() < 2 {
            bail!(
                "No {} prices between {} and {} — nothing to replay",
                scenario.benchmark_name,
                scenario.from,
                scenario.to
            );
        }
        let mut r = Replay {
            scenario,
            calendar,
            day: 0,
            capital,
            cash: capital,
            costs: costs.to_string(),
            positions: BTreeMap::new(),
            orders: Vec::new(),
            fills: Vec::new(),
            marks: Vec::new(),
            costs_paid: 0.0,
            next_id: 1,
            challenge: None,
            finished_challenges: Vec::new(),
            contribution: None,
        };
        r.mark(tape, 0.0);
        Ok(r)
    }

    pub fn load() -> Result<Option<Replay>> {
        crate::store::load::<Option<Replay>>(FILE)
    }

    pub fn save(&self) -> Result<()> {
        crate::store::save(FILE, self, false)
    }

    pub fn date(&self) -> NaiveDate {
        self.calendar[self.day]
    }

    pub fn is_last_day(&self) -> bool {
        self.day + 1 >= self.calendar.len()
    }

    fn model(&self, tape: &Tape, symbol: &str) -> CostModel {
        cost_preset(&self.costs)
            .unwrap_or(CostPreset::Auto)
            .model_for(symbol, tape.currency(symbol))
            .unwrap_or_else(|_| CostModel::none())
    }

    /// The cost model in words, e.g. "India delivery — brokerage ₹20 or
    /// 0.1%, lower, STT, ...".
    pub fn cost_summary(&self, tape: &Tape) -> String {
        let market =
            <crate::market::Market as clap::ValueEnum>::from_str(&self.scenario.market, true)
                .unwrap_or(crate::market::Market::In);
        let csym = market.currency_symbol();
        self.scenario
            .universe
            .first()
            .map(|m| self.model(tape, &m.symbol).describe(csym))
            .unwrap_or_default()
    }

    /// Equity at the current close.
    pub fn equity(&self) -> f64 {
        self.marks.last().map_or(self.cash, |m| m.equity)
    }

    /// Cash left for a new buy at the last close: cash, plus what pending
    /// sells bring in (they fill first), minus what pending buys take.
    fn free_cash(&self, tape: &Tape) -> f64 {
        let date = self.date();
        let pending: f64 = self
            .orders
            .iter()
            .map(|o| {
                let px = tape.last_close(&o.symbol, date).unwrap_or(0.0);
                let model = self.model(tape, &o.symbol);
                match o.side {
                    OrderSide::Buy => -buy_cost(&model, px, o.qty),
                    OrderSide::Sell => {
                        let turnover = model.fill_price(Side::Sell, px) * o.qty as f64;
                        turnover - model.charges(Side::Sell, turnover).total()
                    }
                }
            })
            .sum();
        self.cash + pending
    }

    /// Queue an order for the next open. A hard challenge rule it would
    /// break refuses it; soft ones are recorded and returned.
    pub fn place(
        &mut self,
        tape: &Tape,
        side: OrderSide,
        symbol: &str,
        qty: u64,
        stop: Option<f64>,
        target: Option<f64>,
    ) -> Result<(Order, Vec<Violation>)> {
        if self.is_last_day() {
            bail!(
                "This is the replay's last day: nothing fills any more. Run `stockwise sim replay end`."
            );
        }
        if self.scenario.sector(symbol).is_none() {
            bail!(
                "{symbol} is not in this replay. Pick one of: {}",
                self.scenario.symbols().join(", ")
            );
        }
        if qty == 0 {
            bail!("Quantity must be at least 1");
        }
        let date = self.date();
        let Some(close) = tape.last_close(symbol, date) else {
            bail!("{symbol} has no price yet on {date}");
        };
        let mut warnings = Vec::new();
        match side {
            OrderSide::Buy => {
                if let Some(s) = stop
                    && !(s > 0.0 && s < close)
                {
                    bail!("A stop-loss must be below the last close ({close:.2})");
                }
                if let Some(t) = target
                    && t <= close
                {
                    bail!("A target must be above the last close ({close:.2})");
                }
                let need = buy_cost(&self.model(tape, symbol), close, qty);
                let free = self.free_cash(tape);
                if need > free {
                    bail!(
                        "{qty} × {symbol} costs about {need:.2} with charges; you have {free:.2} free. \
                         At most {} at the last close.",
                        affordable(&self.model(tape, symbol), close, free)
                    );
                }
                if let Some(ch) = &self.challenge {
                    let held = self.positions.get(symbol).map_or(0, |p| p.qty);
                    let queued: u64 = self
                        .orders
                        .iter()
                        .filter(|o| o.side == OrderSide::Buy && o.symbol == symbol)
                        .map(|o| o.qty)
                        .sum();
                    let weight = held.saturating_add(queued).saturating_add(qty) as f64 * close
                        / self.equity()
                        * 100.0;
                    let has_stop = stop.is_some()
                        || self.positions.get(symbol).is_some_and(|p| p.stop.is_some());
                    let broken = ch.check_buy(date, symbol, has_stop, weight);
                    if let Some(v) = broken.iter().find(|v| v.hard) {
                        bail!(
                            "This order breaks a hard rule of the {} challenge — {}: {}. Change the order, \
                             or run `stockwise sim challenge end` to give up the challenge.",
                            ch.def.title,
                            v.rule,
                            v.detail
                        );
                    }
                    warnings = broken;
                }
            }
            OrderSide::Sell => {
                if stop.is_some() || target.is_some() {
                    bail!("--stop and --target go with a buy");
                }
                let held = self.positions.get(symbol).map_or(0, |p| p.qty);
                let queued: u64 = self
                    .orders
                    .iter()
                    .filter(|o| o.side == OrderSide::Sell && o.symbol == symbol)
                    .map(|o| o.qty)
                    .sum();
                if qty.saturating_add(queued) > held {
                    bail!(
                        "You hold {held} {symbol}{} — you can't sell {qty}. Short selling is not part of the replay.",
                        if queued > 0 {
                            format!(" ({queued} already queued to sell)")
                        } else {
                            String::new()
                        }
                    );
                }
            }
        }
        if let Some(ch) = &mut self.challenge {
            ch.violations.extend(warnings.iter().cloned());
        }
        let order = Order {
            id: self.next_id,
            side,
            symbol: symbol.to_string(),
            qty,
            stop,
            target,
            placed: date,
        };
        self.next_id += 1;
        self.orders.push(order.clone());
        Ok((order, warnings))
    }

    /// Cancel a pending order (`None` = all). Returns how many went.
    pub fn cancel(&mut self, id: Option<u32>) -> usize {
        let before = self.orders.len();
        self.orders.retain(|o| id.is_some_and(|i| o.id != i));
        before - self.orders.len()
    }

    /// Change the stop-loss or target of a position.
    pub fn protect(
        &mut self,
        tape: &Tape,
        symbol: &str,
        stop: Option<f64>,
        target: Option<f64>,
    ) -> Result<()> {
        let date = self.date();
        let close = tape.last_close(symbol, date).unwrap_or(0.0);
        let Some(pos) = self.positions.get_mut(symbol) else {
            bail!("You don't hold {symbol}");
        };
        if let Some(s) = stop {
            if !(s > 0.0 && s < close) {
                bail!("A stop-loss must be below the last close ({close:.2})");
            }
            pos.stop = Some(s);
        }
        if let Some(t) = target {
            if t <= close {
                bail!("A target must be above the last close ({close:.2})");
            }
            pos.target = Some(t);
        }
        Ok(())
    }

    /// Move to the next trading day: contributions, fills at the open,
    /// stops and targets, the close, then the challenge's rules.
    pub fn step(&mut self, tape: &Tape) -> Result<Vec<Event>> {
        if self.is_last_day() {
            bail!("The replay is on its last day. Run `stockwise sim replay end` for your score.");
        }
        let prev = self.date();
        self.day += 1;
        let date = self.date();
        let mut events = Vec::new();

        let mut flow = 0.0;
        if let Some(amount) = self.contribution
            && date.month() != prev.month()
        {
            self.cash += amount;
            flow = amount;
            events.push(Event::Contribution { amount });
        }

        // Sells first so their cash is there for the buys.
        let mut pending = std::mem::take(&mut self.orders);
        pending.sort_by_key(|o| (o.side == OrderSide::Buy, o.id));
        for order in pending {
            match tape.bar_on(&order.symbol, date) {
                None => {
                    events.push(Event::Waiting {
                        symbol: order.symbol.clone(),
                        side: order.side,
                    });
                    self.orders.push(order);
                }
                Some(bar) => events.push(self.fill_order(tape, &order, bar.open, date)),
            }
        }
        self.orders.sort_by_key(|o| o.id);

        // Stops and targets on today's range.
        let held: Vec<String> = self.positions.keys().cloned().collect();
        for symbol in held {
            let Some(bar) = tape.bar_on(&symbol, date) else {
                continue;
            };
            let pos = &self.positions[&symbol];
            let exit = match (pos.stop, pos.target) {
                (Some(s), _) if bar.open <= s => Some((bar.open, FillReason::StopLoss)),
                (Some(s), _) if bar.low <= s => Some((s, FillReason::StopLoss)),
                (_, Some(t)) if bar.open >= t => Some((bar.open, FillReason::Target)),
                (_, Some(t)) if bar.high >= t => Some((t, FillReason::Target)),
                _ => None,
            };
            if let Some((price, reason)) = exit {
                let qty = pos.qty;
                let fill = self.sell(tape, &symbol, qty, price, reason, date);
                events.push(Event::Fill(fill));
            }
        }

        self.mark(tape, flow);
        events.extend(self.check_challenge(prev));
        if self.is_last_day() {
            events.push(Event::LastDay);
        }
        Ok(events)
    }

    fn fill_order(&mut self, tape: &Tape, order: &Order, open: f64, date: NaiveDate) -> Event {
        let rejected = |reason: &str| Event::Rejected {
            symbol: order.symbol.clone(),
            side: order.side,
            qty: order.qty,
            reason: reason.to_string(),
        };
        let model = self.model(tape, &order.symbol);
        match order.side {
            OrderSide::Sell => {
                let held = self.positions.get(&order.symbol).map_or(0, |p| p.qty);
                if held == 0 {
                    return rejected("no longer held (stopped out or sold)");
                }
                Event::Fill(self.sell(
                    tape,
                    &order.symbol,
                    order.qty.min(held),
                    open,
                    FillReason::Order,
                    date,
                ))
            }
            OrderSide::Buy => {
                let price = model.fill_price(Side::Buy, open);
                let qty = order.qty.min(affordable(&model, open, self.cash));
                if qty == 0 {
                    return rejected("not enough cash at the open");
                }
                let charges = model.charges(Side::Buy, price * qty as f64).total();
                self.cash -= price * qty as f64 + charges;
                self.costs_paid += charges + (price - open) * qty as f64;
                let equity = self.equity();
                let pos = self
                    .positions
                    .entry(order.symbol.clone())
                    .or_insert(Position {
                        qty: 0,
                        avg_price: 0.0,
                        stop: None,
                        target: None,
                        opened: date,
                    });
                pos.avg_price =
                    (pos.avg_price * pos.qty as f64 + price * qty as f64) / (pos.qty + qty) as f64;
                pos.qty += qty;
                if order.stop.is_some() {
                    pos.stop = order.stop;
                }
                if order.target.is_some() {
                    pos.target = order.target;
                }
                // Size on the basis the order was judged on: the last close
                // before this fill, over that close's equity.
                let known = self.calendar[self.day.saturating_sub(1)];
                let ref_px = tape.last_close(&order.symbol, known).unwrap_or(price);
                let weight = if equity > 0.0 {
                    pos.qty as f64 * ref_px / equity * 100.0
                } else {
                    100.0
                };
                let fill = Fill {
                    date,
                    symbol: order.symbol.clone(),
                    side: OrderSide::Buy,
                    qty,
                    price: round(price, 4),
                    charges: round(charges, 2),
                    reason: FillReason::Order,
                    with_stop: pos.stop.is_some(),
                    weight_percent: round(weight, 2),
                    pnl: None,
                };
                self.fills.push(fill.clone());
                Event::Fill(fill)
            }
        }
    }

    /// Sell `qty` of a position at `level` (slippage and charges applied).
    fn sell(
        &mut self,
        tape: &Tape,
        symbol: &str,
        qty: u64,
        level: f64,
        reason: FillReason,
        date: NaiveDate,
    ) -> Fill {
        let model = self.model(tape, symbol);
        let price = model.fill_price(Side::Sell, level);
        let charges = model.charges(Side::Sell, price * qty as f64).total();
        let pos = self.positions.get_mut(symbol).expect("position exists");
        let pnl = (price - pos.avg_price) * qty as f64 - charges;
        pos.qty -= qty;
        if pos.qty == 0 {
            self.positions.remove(symbol);
        }
        self.cash += price * qty as f64 - charges;
        self.costs_paid += charges + (level - price) * qty as f64;
        let fill = Fill {
            date,
            symbol: symbol.to_string(),
            side: OrderSide::Sell,
            qty,
            price: round(price, 4),
            charges: round(charges, 2),
            reason,
            with_stop: false,
            weight_percent: 0.0,
            pnl: Some(round(pnl, 2)),
        };
        self.fills.push(fill.clone());
        fill
    }

    /// Value the account at the current close.
    fn mark(&mut self, tape: &Tape, flow: f64) {
        let date = self.date();
        let mut invested = 0.0;
        let mut sectors: Vec<&str> = Vec::new();
        for (symbol, pos) in &self.positions {
            let close = tape.last_close(symbol, date).unwrap_or(pos.avg_price);
            invested += close * pos.qty as f64;
            if let Some(s) = self.scenario.sector(symbol)
                && !sectors.contains(&s)
            {
                sectors.push(s);
            }
        }
        let benchmark = tape
            .last_close(&self.scenario.benchmark, date)
            .or_else(|| self.marks.last().map(|m| m.benchmark))
            .unwrap_or(0.0);
        self.marks.push(Mark {
            date,
            equity: self.cash + invested,
            invested,
            flow,
            benchmark,
            positions: self.positions.len(),
            sectors: sectors.len(),
        });
    }

    /// Start a challenge on the current day.
    pub fn start_challenge(&mut self, tape: &Tape, def: challenge::Challenge) -> Result<()> {
        if let Some(ch) = &self.challenge {
            bail!(
                "The {} challenge is already running. Finish it, or `stockwise sim challenge end`.",
                ch.def.title
            );
        }
        if !def.modes.contains(&challenge::Mode::Replay) {
            bail!(
                "The {} challenge runs over live paper sessions, not a replay.",
                def.title
            );
        }
        if self.is_last_day() {
            bail!("The replay is on its last day — no room for a challenge.");
        }
        let left = self.calendar.len() - 1 - self.day;
        if def.days > left {
            bail!(
                "The {} challenge lasts {} trading days and this replay has {left} left. \
                 Start it earlier in a replay.",
                def.title,
                def.days
            );
        }
        let end_day = match def.days {
            0 => self.calendar.len() - 1,
            n => self.day + n,
        };
        let mut active = Active::new(def, self.day, self.date(), end_day);
        if let Some(amount) = active.def.monthly_contribution {
            if self.day != 0 || !self.fills.is_empty() || !self.orders.is_empty() {
                bail!(
                    "A SIP challenge starts with an empty account: start it on the replay's first day, \
                     before any order (`stockwise sim replay start ...` again)."
                );
            }
            self.capital = amount;
            self.cash = amount;
            self.contribution = Some(amount);
            self.marks.clear();
            self.mark(tape, 0.0);
            active.contributed = amount;
            let bench = self.marks[0].benchmark;
            if bench > 0.0 {
                active.sip_units = amount / bench;
            }
        }
        self.challenge = Some(active);
        Ok(())
    }

    fn check_challenge(&mut self, prev: NaiveDate) -> Vec<Event> {
        let mut events = Vec::new();
        let date = self.date();
        let (start, flow, bench) = match &self.challenge {
            Some(ch) => (
                ch.start_day,
                self.marks.last().map_or(0.0, |m| m.flow),
                self.marks.last().map_or(0.0, |m| m.benchmark),
            ),
            None => return events,
        };
        let nav = self.nav_since(start);
        let month_end = (date.month() != prev.month())
            .then(|| self.marks.iter().rev().nth(1))
            .flatten()
            .filter(|m| m.date > self.calendar[start] && m.equity > 0.0)
            .map(|m| m.invested / m.equity * 100.0);
        let ch = self.challenge.as_mut().expect("checked above");
        if flow > 0.0 && bench > 0.0 {
            ch.sip_units += flow / bench;
            ch.contributed += flow;
        }
        let dd = score::current_drawdown_percent(&nav);
        for v in ch.check_day(date, dd, month_end) {
            events.push(Event::RuleBroken(v));
        }
        let over = ch.hard_broken().is_some() || self.day >= ch.end_day;
        if over {
            let outcome = self.finish_challenge(false);
            events.push(Event::ChallengeOver(outcome));
        }
        events
    }

    /// End the running challenge now (`abandoned` = before its last day).
    pub fn finish_challenge(&mut self, abandoned: bool) -> Outcome {
        let ch = self.challenge.take().expect("a challenge is running");
        let facts = self.facts_since(&ch);
        let outcome = ch.finish(self.date(), &facts, abandoned);
        if ch.def.monthly_contribution.is_some() {
            // The SIP ends with its challenge.
            self.contribution = None;
        }
        self.finished_challenges.push(outcome.clone());
        outcome
    }

    fn facts_since(&self, ch: &Active) -> Facts {
        let marks = &self.marks[ch.start_day.min(self.marks.len() - 1)..];
        let nav = self.nav_since(ch.start_day);
        let bench: Vec<f64> = marks.iter().map(|m| m.benchmark).collect();
        let shares: Vec<f64> = marks.iter().skip(1).map(exposure).collect();
        let first = ch.start_date;
        Facts {
            return_percent: change(&nav),
            benchmark_return_percent: change(&bench),
            exposure_percent: if shares.is_empty() {
                0.0
            } else {
                shares.iter().sum::<f64>() / shares.len() as f64 * 100.0
            },
            max_drawdown_percent: score::max_drawdown_percent(&nav),
            benchmark_max_drawdown_percent: score::max_drawdown_percent(&bench),
            max_positions: marks.iter().map(|m| m.positions).max().unwrap_or(0),
            buys: self
                .fills
                .iter()
                .filter(|f| f.side == OrderSide::Buy && f.date > first)
                .count(),
            equity: self.equity(),
            sip_value: (ch.sip_units > 0.0)
                .then(|| ch.sip_units * marks.last().map_or(0.0, |m| m.benchmark)),
        }
    }

    /// Time-weighted value from replay day `start` to today.
    fn nav_since(&self, start: usize) -> Vec<f64> {
        let marks = &self.marks[start.min(self.marks.len().saturating_sub(1))..];
        let equity: Vec<f64> = marks.iter().map(|m| m.equity).collect();
        let flows: Vec<f64> = marks.iter().map(|m| m.flow).collect();
        score::nav(&equity, &flows)
    }

    /// Return since the start, time-weighted, in percent.
    pub fn return_percent(&self) -> f64 {
        change(&self.nav_since(0))
    }

    pub fn benchmark_return_percent(&self) -> f64 {
        let bench: Vec<f64> = self.marks.iter().map(|m| m.benchmark).collect();
        change(&bench)
    }

    pub fn max_drawdown_percent(&self) -> f64 {
        score::max_drawdown_percent(&self.nav_since(0))
    }

    /// The score input for every day played so far.
    pub fn run_input(&self) -> RunInput {
        let mut held: Vec<&str> = self.fills.iter().map(|f| f.symbol.as_str()).collect();
        held.sort();
        held.dedup();
        let violations = self
            .finished_challenges
            .iter()
            .flat_map(|o| o.violations.iter())
            .chain(self.challenge.iter().flat_map(|c| c.violations.iter()));
        let (mut hard, mut soft) = (false, 0);
        for v in violations {
            if v.hard {
                hard = true;
            } else {
                soft += 1;
            }
        }
        RunInput {
            nav: self.nav_since(0),
            benchmark: self.marks.iter().map(|m| m.benchmark).collect(),
            exposure: self.marks.iter().skip(1).map(exposure).collect(),
            buys: self
                .fills
                .iter()
                .filter(|f| f.side == OrderSide::Buy)
                .map(|f| BuyFact {
                    with_stop: f.with_stop,
                    weight_percent: f.weight_percent,
                })
                .collect(),
            max_sectors: self.marks.iter().map(|m| m.sectors).max().unwrap_or(0),
            symbols_held: held.len(),
            hard_break: hard,
            soft_violations: soft,
            periods_per_year: 252.0,
        }
    }

    pub fn run_facts(&self, input: &RunInput) -> RunFacts {
        RunFacts {
            buys: input.buys.len(),
            buys_with_stop: input.buys.iter().filter(|b| b.with_stop).count(),
            hard_break: input.hard_break,
            max_sectors: input.max_sectors,
            challenge_passed: self.finished_challenges.iter().any(|o| o.passed),
            sip_clean: self.finished_challenges.iter().any(|o| o.sip),
            full_distance: self.is_last_day(),
        }
    }

    pub fn scorecard(&self) -> Scorecard {
        score::score(&self.run_input())
    }
}

fn exposure(m: &Mark) -> f64 {
    if m.equity > 0.0 {
        m.invested / m.equity
    } else {
        0.0
    }
}

fn change(curve: &[f64]) -> f64 {
    match (curve.first(), curve.last()) {
        (Some(&a), Some(&b)) if a > 0.0 => (b / a - 1.0) * 100.0,
        _ => 0.0,
    }
}

/// Cash for `qty` at `price` with slippage and charges.
fn buy_cost(model: &CostModel, price: f64, qty: u64) -> f64 {
    let px = model.fill_price(Side::Buy, price);
    let turnover = px * qty as f64;
    turnover + model.charges(Side::Buy, turnover).total()
}

/// The most shares `cash` buys at `price`, charges included.
pub fn affordable(model: &CostModel, price: f64, cash: f64) -> u64 {
    if price <= 0.0 || cash <= 0.0 {
        return 0;
    }
    let mut q = (cash / model.fill_price(Side::Buy, price)).floor() as u64;
    while q > 0 && buy_cost(model, price, q) > cash {
        q -= 1;
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::scenario::Member;
    use crate::practice::tape::tests::{d, flat};

    fn scenario(symbols: &[(&str, &str)]) -> Scenario {
        Scenario {
            name: "test".into(),
            title: "Test".into(),
            market: "us".into(),
            from: d("2020-01-01"),
            to: d("2020-01-10"),
            benchmark: "^IDX".into(),
            benchmark_name: "Index".into(),
            capital: 10_000.0,
            description: String::new(),
            debrief: String::new(),
            universe: symbols
                .iter()
                .map(|(s, sec)| Member {
                    symbol: s.to_string(),
                    sector: sec.to_string(),
                })
                .collect(),
        }
    }

    /// AAA and BBB with the given closes from 2020-01-01; the index is flat.
    fn setup(a: &[f64], b: &[f64]) -> (Replay, Tape) {
        let mut tape = Tape::default();
        tape.insert("AAA", Some("USD".into()), flat("2020-01-01", a));
        tape.insert("BBB", Some("USD".into()), flat("2020-01-01", b));
        tape.insert("^IDX", None, flat("2020-01-01", &vec![100.0; a.len()]));
        let s = scenario(&[("AAA", "Tech"), ("BBB", "Energy")]);
        let r = Replay::new(s, &tape, 10_000.0, "none").unwrap();
        (r, tape)
    }

    #[test]
    fn buy_fills_at_the_next_open_and_marks_at_the_close() {
        let (mut r, tape) = setup(&[10.0, 11.0, 12.0, 13.0], &[5.0; 4]);
        r.place(&tape, OrderSide::Buy, "AAA", 100, None, None)
            .unwrap();
        assert_eq!(r.cash, 10_000.0); // nothing fills on the day it is placed
        let ev = r.step(&tape).unwrap();
        let Event::Fill(f) = &ev[0] else {
            panic!("{ev:?}")
        };
        assert_eq!((f.price, f.qty, f.date), (11.0, 100, d("2020-01-02")));
        assert_eq!(r.cash, 8_900.0);
        assert_eq!(r.equity(), 10_000.0);
        r.step(&tape).unwrap();
        assert_eq!(r.equity(), 10_100.0); // 100 × 12 + 8,900
        assert!((r.return_percent() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn costs_come_from_the_backtest_models() {
        let mut tape = Tape::default();
        tape.insert(
            "TCS.NS",
            Some("INR".into()),
            flat("2020-01-01", &[1000.0; 3]),
        );
        tape.insert("^NSEI", None, flat("2020-01-01", &[100.0; 3]));
        let mut s = scenario(&[("TCS.NS", "IT")]);
        s.market = "in".into();
        s.benchmark = "^NSEI".into();
        let mut r = Replay::new(s, &tape, 200_000.0, "auto").unwrap();
        r.place(&tape, OrderSide::Buy, "TCS.NS", 100, None, None)
            .unwrap();
        r.step(&tape).unwrap();
        let f = &r.fills[0];
        let m = CostModel::in_delivery();
        let px = m.fill_price(Side::Buy, 1000.0);
        assert!((f.price - px).abs() < 1e-6);
        let charges = m.charges(Side::Buy, px * 100.0).total();
        assert!((f.charges - round(charges, 2)).abs() < 1e-9);
        assert!((r.cash - (200_000.0 - px * 100.0 - charges)).abs() < 1e-6);
        assert!(Replay::new(r.scenario.clone(), &tape, 1.0, "fancy").is_err());
    }

    #[test]
    fn stop_fills_at_the_level_or_the_gap() {
        let mut tape = Tape::default();
        let mut bars = flat("2020-01-01", &[100.0, 100.0, 100.0, 80.0]);
        bars[2].low = 90.0; // day 3 trades through the stop at 95
        tape.insert("AAA", Some("USD".into()), bars);
        tape.insert("BBB", Some("USD".into()), flat("2020-01-01", &[5.0; 4]));
        tape.insert("^IDX", None, flat("2020-01-01", &[100.0; 4]));
        let mut r = Replay::new(
            scenario(&[("AAA", "Tech"), ("BBB", "Energy")]),
            &tape,
            10_000.0,
            "none",
        )
        .unwrap();
        r.place(&tape, OrderSide::Buy, "AAA", 10, Some(95.0), Some(130.0))
            .unwrap();
        r.step(&tape).unwrap();
        assert!(r.fills[0].with_stop);
        let ev = r.step(&tape).unwrap();
        let Event::Fill(f) = &ev[0] else {
            panic!("{ev:?}")
        };
        assert_eq!((f.reason, f.price), (FillReason::StopLoss, 95.0));
        assert_eq!(f.pnl, Some(-50.0));
        assert!(r.positions.is_empty());

        // A gap below the stop fills at the open.
        let (mut r, mut tape) = setup(&[100.0; 4], &[5.0; 4]);
        let mut bars = flat("2020-01-01", &[100.0, 100.0, 100.0, 80.0]);
        bars[3].high = 85.0;
        tape.insert("AAA", Some("USD".into()), bars);
        r.place(&tape, OrderSide::Buy, "AAA", 10, Some(95.0), None)
            .unwrap();
        r.step(&tape).unwrap();
        r.step(&tape).unwrap();
        let ev = r.step(&tape).unwrap();
        assert!(
            matches!(&ev[0], Event::Fill(f) if f.price == 80.0),
            "{ev:?}"
        );
        assert!(ev.contains(&Event::LastDay));
        assert!(r.step(&tape).is_err());
        assert!(
            r.place(&tape, OrderSide::Buy, "AAA", 1, None, None)
                .is_err()
        );
    }

    #[test]
    fn orders_are_checked() {
        let (mut r, tape) = setup(&[10.0; 5], &[5.0; 5]);
        assert!(
            r.place(&tape, OrderSide::Buy, "ZZZ", 1, None, None)
                .is_err()
        );
        assert!(
            r.place(&tape, OrderSide::Buy, "AAA", 0, None, None)
                .is_err()
        );
        assert!(
            r.place(&tape, OrderSide::Buy, "AAA", 1, Some(11.0), None)
                .is_err()
        );
        assert!(
            r.place(&tape, OrderSide::Buy, "AAA", 1, None, Some(9.0))
                .is_err()
        );
        let e = r
            .place(&tape, OrderSide::Buy, "AAA", 2_000, None, None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("At most 1000"), "{e}");
        // Pending buys hold their cash.
        r.place(&tape, OrderSide::Buy, "AAA", 600, None, None)
            .unwrap();
        assert!(
            r.place(&tape, OrderSide::Buy, "BBB", 900, None, None)
                .is_err()
        );
        assert!(
            r.place(&tape, OrderSide::Sell, "AAA", 1, None, None)
                .is_err()
        );
        assert_eq!(r.cancel(None), 1);
        assert!(r.orders.is_empty());
    }

    #[test]
    fn sells_fill_before_buys_and_free_their_cash() {
        let (mut r, tape) = setup(&[10.0; 5], &[10.0; 5]);
        r.place(&tape, OrderSide::Buy, "AAA", 1_000, None, None)
            .unwrap();
        r.step(&tape).unwrap();
        assert_eq!(r.cash, 0.0);
        assert!(
            r.place(&tape, OrderSide::Buy, "BBB", 500, None, None)
                .is_err()
        );
        // A pending sell's proceeds pay for a buy placed the same day.
        r.place(&tape, OrderSide::Sell, "AAA", 1_000, None, None)
            .unwrap();
        r.place(&tape, OrderSide::Buy, "BBB", 500, None, None)
            .unwrap();
        r.step(&tape).unwrap();
        assert_eq!(r.cash, 5_000.0);
        assert_eq!(r.positions["BBB"].qty, 500);
    }

    #[test]
    fn hard_rules_refuse_orders_and_drawdown_ends_the_challenge() {
        let (mut r, tape) = setup(&[10.0, 10.0, 8.0, 8.0, 8.0, 8.0], &[5.0; 6]);
        r.start_challenge(&tape, short("stop-loss", 3)).unwrap();
        let e = r
            .place(&tape, OrderSide::Buy, "AAA", 10, None, None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("stop-loss"), "{e}");
        r.challenge = None;

        r.start_challenge(&tape, short("steady-hands", 4)).unwrap();
        r.place(&tape, OrderSide::Buy, "AAA", 900, None, None)
            .unwrap();
        r.step(&tape).unwrap();
        let ev = r.step(&tape).unwrap(); // AAA −20%: drawdown 18%
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::RuleBroken(v) if v.hard)),
            "{ev:?}"
        );
        let Some(Event::ChallengeOver(out)) = ev.last() else {
            panic!("{ev:?}")
        };
        assert!(!out.passed);
        assert!(r.challenge.is_none());
        assert!(r.run_input().hard_break);
    }

    /// A built-in challenge shortened to fit the test tapes.
    fn short(name: &str, days: usize) -> challenge::Challenge {
        let mut c = challenge::load(name).unwrap();
        c.days = days;
        c
    }

    #[test]
    fn a_challenge_must_fit_in_the_days_left() {
        let (mut r, tape) = setup(&[10.0; 6], &[5.0; 6]);
        let e = r
            .start_challenge(&tape, challenge::load("steady-hands").unwrap())
            .unwrap_err()
            .to_string();
        assert!(e.contains("30 trading days") && e.contains("5 left"), "{e}");
        r.start_challenge(&tape, short("steady-hands", 5)).unwrap();
        assert_eq!(r.challenge.as_ref().unwrap().end_day, 5);
    }

    #[test]
    fn fill_size_is_judged_like_the_order() {
        // 199 × 10 is 19.9% of 10,000 at the close; the next open gaps to 10.5.
        let mut tape = Tape::default();
        let mut a = flat("2020-01-01", &[10.0; 4]);
        a[1].open = 10.5;
        a[1].high = 10.5;
        tape.insert("AAA", None, a);
        tape.insert("BBB", None, flat("2020-01-01", &[5.0; 4]));
        tape.insert("^IDX", None, flat("2020-01-01", &[100.0; 4]));
        let mut r = Replay::new(
            scenario(&[("AAA", "Tech"), ("BBB", "Energy")]),
            &tape,
            10_000.0,
            "none",
        )
        .unwrap();
        r.start_challenge(&tape, short("position-limit", 3))
            .unwrap();
        r.place(&tape, OrderSide::Buy, "AAA", 199, Some(9.0), None)
            .unwrap();
        r.step(&tape).unwrap();
        assert_eq!(r.fills[0].price, 10.5);
        assert!((r.fills[0].weight_percent - 19.9).abs() < 1e-9);
        assert!(score::disciplined(
            &r.run_input(),
            &Scorecard {
                score: Some(50),
                ..Scorecard::default()
            }
        ));
    }

    #[test]
    fn month_end_check_skips_the_start_day() {
        // Start on 31 Jan with an investing rule: nothing could be bought by
        // the January close, so February's first step breaks nothing.
        let mut tape = Tape::default();
        tape.insert("AAA", None, flat("2020-01-31", &[10.0; 4]));
        tape.insert("BBB", None, flat("2020-01-31", &[5.0; 4]));
        tape.insert("^IDX", None, flat("2020-01-31", &[100.0; 4]));
        let mut s = scenario(&[("AAA", "Tech"), ("BBB", "Energy")]);
        s.from = d("2020-01-31");
        s.to = d("2020-02-29");
        let mut r = Replay::new(s, &tape, 10_000.0, "none").unwrap();
        let mut def = short("steady-hands", 3);
        def.rules = vec![challenge::Rule::InvestEachMonth {
            percent: 90.0,
            hard: true,
        }];
        r.start_challenge(&tape, def).unwrap();
        let ev = r.step(&tape).unwrap();
        assert!(ev.is_empty(), "{ev:?}");
        assert!(r.challenge.is_some());
    }

    #[test]
    fn huge_quantities_do_not_overflow() {
        let (mut r, tape) = setup(&[10.0; 4], &[5.0; 4]);
        r.place(&tape, OrderSide::Buy, "AAA", 10, None, None)
            .unwrap();
        r.step(&tape).unwrap();
        r.place(&tape, OrderSide::Sell, "AAA", 5, None, None)
            .unwrap();
        assert!(
            r.place(&tape, OrderSide::Sell, "AAA", u64::MAX, None, None)
                .is_err()
        );
    }

    #[test]
    fn soft_position_rule_is_recorded_not_refused() {
        let (mut r, tape) = setup(&[10.0; 6], &[5.0; 6]);
        let mut def = short("position-limit", 5);
        def.rules[0] = challenge::Rule::MaxPosition {
            percent: 20.0,
            hard: false,
        };
        r.start_challenge(&tape, def).unwrap();
        let (_, warn) = r
            .place(&tape, OrderSide::Buy, "AAA", 300, None, None)
            .unwrap();
        assert_eq!(warn.len(), 1);
        assert_eq!(r.run_input().soft_violations, 1);
    }

    #[test]
    fn sip_challenge_adds_money_monthly_and_compares_with_a_plain_sip() {
        let mut tape = Tape::default();
        // Jan 30 .. Feb 4: the index doubles on the first February day.
        let start = "2020-01-30";
        tape.insert("AAA", Some("USD".into()), flat(start, &[10.0; 6]));
        tape.insert("BBB", Some("USD".into()), flat(start, &[5.0; 6]));
        tape.insert(
            "^IDX",
            None,
            flat(start, &[100.0, 100.0, 200.0, 200.0, 200.0, 200.0]),
        );
        let mut s = scenario(&[("AAA", "Tech"), ("BBB", "Energy")]);
        s.from = d(start);
        s.to = d("2020-02-29");
        let mut r = Replay::new(s, &tape, 50_000.0, "none").unwrap();
        r.start_challenge(&tape, challenge::load("sip-discipline").unwrap())
            .unwrap();
        assert_eq!(r.cash, 10_000.0);
        r.place(&tape, OrderSide::Buy, "AAA", 1_000, None, None)
            .unwrap();
        let mut events = Vec::new();
        while !r.is_last_day() {
            events.extend(r.step(&tape).unwrap());
        }
        assert!(events.contains(&Event::Contribution { amount: 10_000.0 }));
        assert_eq!(r.equity(), 20_000.0);
        // Money added is not return.
        assert!(r.return_percent().abs() < 1e-9);
        let Some(Event::ChallengeOver(out)) = events
            .iter()
            .find(|e| matches!(e, Event::ChallengeOver(_)))
            .cloned()
        else {
            panic!("{events:?}")
        };
        // The plain SIP bought 100 index units in January, then 50 at 200:
        // 150 × 200 = 30,000 beats 20,000.
        assert!(!out.passed);
        assert!(
            out.goals.iter().any(|g| g.detail.contains("30000")),
            "{out:?}"
        );
        // SIP needs a fresh account.
        let (mut r, tape) = setup(&[10.0; 4], &[5.0; 4]);
        r.place(&tape, OrderSide::Buy, "AAA", 1, None, None)
            .unwrap();
        assert!(
            r.start_challenge(&tape, challenge::load("sip-discipline").unwrap())
                .is_err()
        );
    }

    #[test]
    fn missing_bar_keeps_the_order_waiting() {
        let mut tape = Tape::default();
        let mut a = flat("2020-01-01", &[10.0; 4]);
        a.remove(1); // no AAA bar on the 2nd
        tape.insert("AAA", None, a);
        tape.insert("BBB", None, flat("2020-01-01", &[5.0; 4]));
        tape.insert("^IDX", None, flat("2020-01-01", &[100.0; 4]));
        let mut r = Replay::new(
            scenario(&[("AAA", "Tech"), ("BBB", "Energy")]),
            &tape,
            1_000.0,
            "none",
        )
        .unwrap();
        r.place(&tape, OrderSide::Buy, "AAA", 10, None, None)
            .unwrap();
        let ev = r.step(&tape).unwrap();
        assert!(matches!(ev[0], Event::Waiting { .. }));
        assert_eq!(r.orders.len(), 1);
        r.step(&tape).unwrap();
        assert!(r.orders.is_empty());
    }

    #[test]
    fn affordable_includes_charges() {
        let m = CostModel::in_delivery();
        let q = affordable(&m, 1000.0, 100_000.0);
        assert!(buy_cost(&m, 1000.0, q) <= 100_000.0);
        assert!(buy_cost(&m, 1000.0, q + 1) > 100_000.0);
        assert_eq!(affordable(&m, 0.0, 100.0), 0);
    }
}

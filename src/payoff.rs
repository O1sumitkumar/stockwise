//! Multi-leg option positions: payoff at expiry and before it, net premium,
//! maximum profit and loss, breakevens, probability of profit under a
//! lognormal price, and net greeks. Pure functions over legs the caller has
//! priced (`commands/fno.rs` prices them from the live chain).
//!
//! Every amount here is per unit of the underlying for one strategy unit;
//! the caller multiplies by the lot size and the number of lots.

use crate::greeks::{self, BsInput, Greeks, OptionKind};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    fn sign(self) -> f64 {
        match self {
            Side::Buy => 1.0,
            Side::Sell => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Instrument {
    Call,
    Put,
    /// The stock, index future or ETF itself (for covered calls).
    Underlying,
}

/// A strike as typed: a price, or a number of listed strikes from the
/// at-the-money one (`atm`, `atm+2`, `atm-1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StrikeSpec {
    Price(f64),
    Atm(i32),
}

/// One `--leg` as typed, before it is priced from the chain.
#[derive(Clone, Debug, PartialEq)]
pub struct LegSpec {
    pub side: Side,
    pub qty: u32,
    pub instrument: Instrument,
    pub strike: Option<StrikeSpec>,
    /// Price per unit given after `@`, instead of the chain's.
    pub price: Option<f64>,
}

const LEG_HELP: &str = "use `buy|sell [QTY] call|put STRIKE [@PRICE]` or `buy|sell [QTY] stock \
                        [@PRICE]`, e.g. \"buy call 24500\", \"sell 2 put atm-1\", \"sell call 24800 @85\"";

fn parse_strike(tok: &str) -> Option<StrikeSpec> {
    if let Some(rest) = tok.strip_prefix("atm") {
        if rest.is_empty() {
            return Some(StrikeSpec::Atm(0));
        }
        let n: i32 = rest.strip_prefix('+').unwrap_or(rest).parse().ok()?;
        return Some(StrikeSpec::Atm(n));
    }
    let v: f64 = tok.replace(',', "").parse().ok()?;
    (v.is_finite() && v > 0.0).then_some(StrikeSpec::Price(v))
}

/// Parse a `--leg` such as `buy call 24500`, `sell 2 put atm-1 @85.5` or
/// `buy stock`.
pub fn parse_leg(s: &str) -> Result<LegSpec, String> {
    let text = s.trim().to_lowercase().replace('@', " @ ");
    let mut toks: Vec<&str> = text.split_whitespace().collect();
    let bad = |why: &str| format!("`{}`: {why}; {LEG_HELP}", s.trim());

    // Optional trailing price: `@ 85.5` or `at 85.5`.
    let mut price = None;
    if let Some(i) = toks.iter().position(|t| *t == "@" || *t == "at") {
        let p: f64 = toks
            .get(i + 1)
            .and_then(|t| t.parse().ok())
            .filter(|p: &f64| p.is_finite() && *p >= 0.0)
            .ok_or_else(|| bad("the price after @ must be a number"))?;
        if toks.len() > i + 2 {
            return Err(bad("nothing may follow the price"));
        }
        price = Some(p);
        toks.truncate(i);
    }

    let mut it = toks.into_iter().peekable();
    let side = match it.next() {
        Some("buy" | "long" | "b") => Side::Buy,
        Some("sell" | "short" | "write" | "s") => Side::Sell,
        _ => return Err(bad("start with buy or sell")),
    };
    let mut qty = 1;
    if let Some(n) = it
        .peek()
        .map(|t| t.trim_end_matches('x').trim_start_matches('x'))
        .and_then(|t| t.parse::<u32>().ok())
    {
        if n == 0 {
            return Err(bad("quantity must be at least 1"));
        }
        qty = n;
        it.next();
    }
    let instrument = match it.next() {
        Some("call" | "calls" | "c" | "ce") => Instrument::Call,
        Some("put" | "puts" | "p" | "pe") => Instrument::Put,
        Some("stock" | "shares" | "underlying" | "spot" | "fut" | "future" | "futures") => {
            Instrument::Underlying
        }
        _ => return Err(bad("say call, put or stock")),
    };
    let strike = match (instrument, it.next()) {
        (Instrument::Underlying, None) => None,
        (Instrument::Underlying, Some(_)) => return Err(bad("stock legs take no strike")),
        (_, Some(tok)) => {
            Some(parse_strike(tok).ok_or_else(|| bad("strike must be a price or atm±N"))?)
        }
        (_, None) => return Err(bad("options need a strike")),
    };
    if it.next().is_some() {
        return Err(bad("unexpected words"));
    }
    Ok(LegSpec {
        side,
        qty,
        instrument,
        strike,
        price,
    })
}

/// Ready-made strategies around the at-the-money strike.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// Buy ATM call, sell call `width` above.
    BullCall,
    /// Buy ATM put, sell put `width` below.
    BearPut,
    /// Buy ATM call and ATM put.
    Straddle,
    /// Buy put `width` below and call `width` above.
    Strangle,
    /// Sell put/call `width` from ATM, buy wings `2 × width` away (credit).
    IronCondor,
    /// Buy calls `width` either side, sell two ATM calls.
    Butterfly,
    /// Own the underlying, sell a call `width` above.
    CoveredCall,
}

/// A preset's legs: side, quantity, instrument and strike (`None` for the
/// underlying). `atm` is the at-the-money strike; strikes are not snapped
/// to listed ones here.
pub fn preset_legs(
    preset: Preset,
    atm: f64,
    width: f64,
) -> Vec<(Side, u32, Instrument, Option<f64>)> {
    use Instrument::{Call, Put, Underlying};
    use Side::{Buy, Sell};
    match preset {
        Preset::BullCall => vec![
            (Buy, 1, Call, Some(atm)),
            (Sell, 1, Call, Some(atm + width)),
        ],
        Preset::BearPut => vec![(Buy, 1, Put, Some(atm)), (Sell, 1, Put, Some(atm - width))],
        Preset::Straddle => vec![(Buy, 1, Call, Some(atm)), (Buy, 1, Put, Some(atm))],
        Preset::Strangle => vec![
            (Buy, 1, Put, Some(atm - width)),
            (Buy, 1, Call, Some(atm + width)),
        ],
        Preset::IronCondor => vec![
            (Buy, 1, Put, Some(atm - 2.0 * width)),
            (Sell, 1, Put, Some(atm - width)),
            (Sell, 1, Call, Some(atm + width)),
            (Buy, 1, Call, Some(atm + 2.0 * width)),
        ],
        Preset::Butterfly => vec![
            (Buy, 1, Call, Some(atm - width)),
            (Sell, 2, Call, Some(atm)),
            (Buy, 1, Call, Some(atm + width)),
        ],
        Preset::CoveredCall => vec![
            (Buy, 1, Underlying, None),
            (Sell, 1, Call, Some(atm + width)),
        ],
    }
}

/// A priced leg. `price` is per unit of the underlying (the option premium,
/// or the entry price of the underlying); `iv` drives the before-expiry
/// value and greeks.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Leg {
    pub side: Side,
    pub qty: u32,
    pub instrument: Instrument,
    pub strike: Option<f64>,
    pub price: f64,
    pub iv: Option<f64>,
}

impl Leg {
    fn signed_qty(&self) -> f64 {
        self.side.sign() * self.qty as f64
    }

    fn kind(&self) -> Option<OptionKind> {
        match self.instrument {
            Instrument::Call => Some(OptionKind::Call),
            Instrument::Put => Some(OptionKind::Put),
            Instrument::Underlying => None,
        }
    }

    /// What the leg is worth at expiry with the underlying at `s`.
    pub fn expiry_value(&self, s: f64) -> f64 {
        let k = self.strike.unwrap_or(0.0);
        match self.instrument {
            Instrument::Call => (s - k).max(0.0),
            Instrument::Put => (k - s).max(0.0),
            Instrument::Underlying => s,
        }
    }

    /// Profit or loss at expiry.
    pub fn pl_at_expiry(&self, s: f64) -> f64 {
        self.signed_qty() * (self.expiry_value(s) - self.price)
    }
}

/// Market inputs for valuing legs before expiry.
#[derive(Clone, Copy, Debug)]
pub struct Market {
    /// Years to expiry.
    pub t: f64,
    pub rate: f64,
    pub div_yield: f64,
}

/// Premium paid (positive, a debit) or received (negative, a credit).
pub fn net_premium(legs: &[Leg]) -> f64 {
    legs.iter()
        .filter(|l| l.instrument != Instrument::Underlying)
        .map(|l| l.signed_qty() * l.price)
        .sum()
}

pub fn pl_at_expiry(legs: &[Leg], s: f64) -> f64 {
    legs.iter().map(|l| l.pl_at_expiry(s)).sum()
}

/// Profit or loss if the underlying were at `s` now, with each option
/// valued by Black-Scholes at its own IV. `None` if an option has no IV.
pub fn pl_before_expiry(legs: &[Leg], s: f64, m: &Market) -> Option<f64> {
    if m.t <= 0.0 {
        return Some(pl_at_expiry(legs, s));
    }
    legs.iter()
        .map(|l| {
            let value = match l.kind() {
                None => s,
                Some(kind) => greeks::price(
                    kind,
                    &BsInput {
                        spot: s,
                        strike: l.strike?,
                        t: m.t,
                        rate: m.rate,
                        div_yield: m.div_yield,
                        vol: l.iv?,
                    },
                )
                .or_else(|| (s <= 0.0).then(|| l.expiry_value(0.0)))?,
            };
            Some(l.signed_qty() * (value - l.price))
        })
        .sum()
}

/// Sorted, distinct strikes: where the expiry payoff bends.
fn kinks(legs: &[Leg]) -> Vec<f64> {
    let mut k: Vec<f64> = legs.iter().filter_map(|l| l.strike).collect();
    k.sort_by(f64::total_cmp);
    k.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    k
}

/// Slope of the expiry payoff above the highest strike: calls and the
/// underlying gain one per unit, puts are flat.
fn slope_above(legs: &[Leg]) -> f64 {
    legs.iter()
        .filter(|l| l.instrument != Instrument::Put)
        .map(Leg::signed_qty)
        .sum()
}

/// Largest gain and largest loss at expiry. `None` means unlimited: the
/// payoff keeps rising (or falling) as the underlying rises. Below the
/// lowest strike the price can only fall to 0, so that side is finite.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Extremes {
    pub max_profit: Option<f64>,
    /// The worst profit-or-loss (negative for a loss).
    pub max_loss: Option<f64>,
}

pub fn extremes(legs: &[Leg]) -> Extremes {
    let points: Vec<f64> = std::iter::once(0.0).chain(kinks(legs)).collect();
    let values: Vec<f64> = points.iter().map(|s| pl_at_expiry(legs, *s)).collect();
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let slope = slope_above(legs);
    Extremes {
        max_profit: (slope <= 1e-9).then_some(max),
        max_loss: (slope >= -1e-9).then_some(min),
    }
}

/// Underlying prices at expiry where the position breaks even, ascending.
pub fn breakevens(legs: &[Leg]) -> Vec<f64> {
    const EPS: f64 = 1e-9;
    let points: Vec<f64> = std::iter::once(0.0).chain(kinks(legs)).collect();
    let f = |s: f64| pl_at_expiry(legs, s);
    let mut out: Vec<f64> = Vec::new();
    let mut push = |x: f64| {
        if x > 0.0 && !out.iter().any(|y| (x - y).abs() < 1e-6 * x.max(1.0)) {
            out.push(x);
        }
    };
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (fa, fb) = (f(a), f(b));
        if fa.abs() < EPS {
            push(a);
        }
        if fa * fb < 0.0 && fa.abs() >= EPS && fb.abs() >= EPS {
            push(a + (b - a) * fa / (fa - fb));
        }
    }
    let last = *points.last().expect("points start with 0");
    let fl = f(last);
    if fl.abs() < EPS {
        push(last);
    }
    let slope = slope_above(legs);
    if slope.abs() > EPS && fl.abs() >= EPS && fl.signum() != slope.signum() {
        push(last - fl / slope);
    }
    out.sort_by(f64::total_cmp);
    out
}

/// Price ranges at expiry where the position makes money: `(from, to)`,
/// `to = None` for "and above".
pub fn profit_ranges(legs: &[Leg]) -> Vec<(f64, Option<f64>)> {
    let mut bounds = vec![0.0];
    bounds.extend(breakevens(legs));
    let mut out: Vec<(f64, Option<f64>)> = Vec::new();
    for (i, &lo) in bounds.iter().enumerate() {
        let hi = bounds.get(i + 1).copied();
        let probe = match hi {
            Some(hi) => (lo + hi) / 2.0,
            None => lo * 1.5 + 1.0,
        };
        if pl_at_expiry(legs, probe) > 1e-9 {
            // Merge with the previous range when they touch.
            match out.last_mut() {
                Some(prev) if prev.1 == Some(lo) => prev.1 = hi,
                _ => out.push((lo, hi)),
            }
        }
    }
    out
}

/// Probability that the position is profitable at expiry when the
/// underlying is lognormal with volatility `vol` from `spot` (risk-neutral
/// drift `rate − div_yield`). `None` without a volatility or time left.
pub fn probability_of_profit(legs: &[Leg], spot: f64, vol: f64, m: &Market) -> Option<f64> {
    if !(spot > 0.0 && vol > 0.0 && m.t > 0.0) {
        return None;
    }
    let sd = vol * m.t.sqrt();
    let mean = spot.ln() + (m.rate - m.div_yield - 0.5 * vol * vol) * m.t;
    let cdf = |x: f64| {
        if x <= 0.0 {
            0.0
        } else {
            greeks::norm_cdf((x.ln() - mean) / sd)
        }
    };
    let p: f64 = profit_ranges(legs)
        .iter()
        .map(|&(lo, hi)| hi.map_or(1.0, cdf) - cdf(lo))
        .sum();
    Some(p.clamp(0.0, 1.0))
}

/// Position greeks per unit of the underlying (the underlying leg counts as
/// delta 1). `None` if an option leg has no IV or no time is left.
pub fn net_greeks(legs: &[Leg], spot: f64, m: &Market) -> Option<Greeks> {
    let mut total = Greeks::default();
    for l in legs {
        let q = l.signed_qty();
        let g = match l.kind() {
            None => Greeks {
                delta: 1.0,
                ..Default::default()
            },
            Some(kind) => greeks::greeks(
                kind,
                &BsInput {
                    spot,
                    strike: l.strike?,
                    t: m.t,
                    rate: m.rate,
                    div_yield: m.div_yield,
                    vol: l.iv?,
                },
            )?,
        };
        total.delta += q * g.delta;
        total.gamma += q * g.gamma;
        total.theta += q * g.theta;
        total.vega += q * g.vega;
        total.rho += q * g.rho;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(side: Side, qty: u32, instrument: Instrument, strike: Option<f64>, price: f64) -> Leg {
        Leg {
            side,
            qty,
            instrument,
            strike,
            price,
            iv: Some(0.2),
        }
    }

    fn call(side: Side, strike: f64, price: f64) -> Leg {
        leg(side, 1, Instrument::Call, Some(strike), price)
    }

    fn put(side: Side, strike: f64, price: f64) -> Leg {
        leg(side, 1, Instrument::Put, Some(strike), price)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn assert_breakevens(legs: &[Leg], want: &[f64]) {
        let got = breakevens(legs);
        assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
        for (g, w) in got.iter().zip(want) {
            assert!(close(*g, *w), "{got:?} vs {want:?}");
        }
        for b in &got {
            assert!(pl_at_expiry(legs, *b).abs() < 1e-9, "P/L at {b} is not 0");
        }
    }

    use Side::{Buy, Sell};

    #[test]
    fn bull_call_spread() {
        let legs = [call(Buy, 100.0, 5.0), call(Sell, 110.0, 2.0)];
        assert!(close(net_premium(&legs), 3.0), "debit");
        assert!(close(pl_at_expiry(&legs, 90.0), -3.0));
        assert!(close(pl_at_expiry(&legs, 105.0), 2.0));
        assert!(close(pl_at_expiry(&legs, 150.0), 7.0));
        let e = extremes(&legs);
        assert_eq!(e.max_profit, Some(7.0));
        assert_eq!(e.max_loss, Some(-3.0));
        assert_breakevens(&legs, &[103.0]);
        assert_eq!(profit_ranges(&legs), vec![(103.0, None)]);
    }

    #[test]
    fn bear_put_spread() {
        let legs = [put(Buy, 100.0, 6.0), put(Sell, 90.0, 2.0)];
        assert!(close(net_premium(&legs), 4.0));
        let e = extremes(&legs);
        assert_eq!(e.max_profit, Some(6.0));
        assert_eq!(e.max_loss, Some(-4.0));
        assert_breakevens(&legs, &[96.0]);
        assert_eq!(profit_ranges(&legs), vec![(0.0, Some(96.0))]);
    }

    #[test]
    fn long_straddle_and_strangle() {
        let straddle = [call(Buy, 100.0, 5.0), put(Buy, 100.0, 4.0)];
        assert!(close(net_premium(&straddle), 9.0));
        let e = extremes(&straddle);
        assert_eq!(e.max_profit, None, "unlimited upside");
        assert_eq!(e.max_loss, Some(-9.0));
        assert_breakevens(&straddle, &[91.0, 109.0]);
        assert_eq!(
            profit_ranges(&straddle),
            vec![(0.0, Some(91.0)), (109.0, None)]
        );

        let strangle = [put(Buy, 95.0, 2.0), call(Buy, 105.0, 3.0)];
        let e = extremes(&strangle);
        assert_eq!((e.max_profit, e.max_loss), (None, Some(-5.0)));
        assert_breakevens(&strangle, &[90.0, 110.0]);
        // Below the put the gain is capped by a price of 0.
        assert!(close(pl_at_expiry(&strangle, 0.0), 90.0));
    }

    #[test]
    fn short_iron_condor() {
        let legs = [
            put(Buy, 80.0, 0.5),
            put(Sell, 90.0, 2.0),
            call(Sell, 110.0, 2.5),
            call(Buy, 120.0, 1.0),
        ];
        assert!(close(net_premium(&legs), -3.0), "credit");
        let e = extremes(&legs);
        assert_eq!(e.max_profit, Some(3.0));
        assert_eq!(e.max_loss, Some(-7.0));
        assert_breakevens(&legs, &[87.0, 113.0]);
        assert_eq!(profit_ranges(&legs), vec![(87.0, Some(113.0))]);
        assert!(close(pl_at_expiry(&legs, 50.0), -7.0));
        assert!(close(pl_at_expiry(&legs, 200.0), -7.0));
    }

    #[test]
    fn long_call_butterfly() {
        let legs = [
            call(Buy, 90.0, 12.0),
            leg(Sell, 2, Instrument::Call, Some(100.0), 5.0),
            call(Buy, 110.0, 1.5),
        ];
        assert!(close(net_premium(&legs), 3.5));
        let e = extremes(&legs);
        assert_eq!(e.max_profit, Some(6.5));
        assert_eq!(e.max_loss, Some(-3.5));
        assert_breakevens(&legs, &[93.5, 106.5]);
    }

    #[test]
    fn covered_call() {
        let legs = [
            leg(Buy, 1, Instrument::Underlying, None, 100.0),
            call(Sell, 110.0, 3.0),
        ];
        assert!(close(net_premium(&legs), -3.0), "the stock is not premium");
        let e = extremes(&legs);
        assert_eq!(e.max_profit, Some(13.0));
        assert_eq!(e.max_loss, Some(-97.0), "the stock can fall to 0");
        assert_breakevens(&legs, &[97.0]);
        // Stock alone: breakeven at the entry, unlimited upside.
        let stock = [leg(Buy, 1, Instrument::Underlying, None, 100.0)];
        assert_breakevens(&stock, &[100.0]);
        assert_eq!(extremes(&stock).max_profit, None);
    }

    #[test]
    fn short_options_have_unlimited_or_floor_losses() {
        let short_straddle = [call(Sell, 100.0, 5.0), put(Sell, 100.0, 4.0)];
        let e = extremes(&short_straddle);
        assert_eq!((e.max_profit, e.max_loss), (Some(9.0), None));
        let short_put = [put(Sell, 100.0, 4.0)];
        let e = extremes(&short_put);
        assert_eq!((e.max_profit, e.max_loss), (Some(4.0), Some(-96.0)));
        assert_breakevens(&short_put, &[96.0]);
        // Lots of the same leg scale linearly.
        let two = [leg(Sell, 2, Instrument::Put, Some(100.0), 4.0)];
        assert_eq!(extremes(&two).max_loss, Some(-192.0));
    }

    #[test]
    fn breakeven_on_a_strike_and_flat_positions() {
        // Pays exactly the premium back at the short strike.
        let legs = [call(Buy, 100.0, 10.0), call(Sell, 110.0, 0.0)];
        assert_breakevens(&legs, &[110.0]);
        assert!(profit_ranges(&legs).is_empty(), "never above 0");
        // A free call never loses: no breakeven below the strike.
        let free = [call(Buy, 100.0, 0.0)];
        assert_eq!(extremes(&free).max_loss, Some(0.0));
    }

    #[test]
    fn probability_of_profit_is_lognormal() {
        let m = Market {
            t: 30.0 / 365.0,
            rate: 0.065,
            div_yield: 0.0,
        };
        let (spot, vol): (f64, f64) = (100.0, 0.2);
        let sd = vol * m.t.sqrt();
        let mean = spot.ln() + (m.rate - 0.5 * vol * vol) * m.t;
        let below = |x: f64| greeks::norm_cdf((x.ln() - mean) / sd);

        let straddle = [call(Buy, 100.0, 5.0), put(Buy, 100.0, 4.0)];
        let p = probability_of_profit(&straddle, spot, vol, &m).unwrap();
        assert!((p - (below(91.0) + 1.0 - below(109.0))).abs() < 1e-12);

        let condor = [
            put(Buy, 80.0, 0.5),
            put(Sell, 90.0, 2.0),
            call(Sell, 110.0, 2.5),
            call(Buy, 120.0, 1.0),
        ];
        let p = probability_of_profit(&condor, spot, vol, &m).unwrap();
        assert!((p - (below(113.0) - below(87.0))).abs() < 1e-12);
        assert!(p > 0.5, "a wide condor usually wins: {p}");

        // A long call far out of the money rarely pays.
        let otm = [call(Buy, 150.0, 0.01)];
        assert!(probability_of_profit(&otm, spot, vol, &m).unwrap() < 0.001);
        assert!(probability_of_profit(&otm, spot, 0.0, &m).is_none());
        let expired = Market { t: 0.0, ..m };
        assert!(probability_of_profit(&otm, spot, vol, &expired).is_none());
    }

    #[test]
    fn value_before_expiry_and_net_greeks() {
        let m = Market {
            t: 0.25,
            rate: 0.05,
            div_yield: 0.0,
        };
        let c = call(Buy, 100.0, 4.0);
        let fair = greeks::price(
            OptionKind::Call,
            &BsInput {
                spot: 100.0,
                strike: 100.0,
                t: 0.25,
                rate: 0.05,
                div_yield: 0.0,
                vol: 0.2,
            },
        )
        .unwrap();
        let now = pl_before_expiry(std::slice::from_ref(&c), 100.0, &m).unwrap();
        assert!(close(now, fair - 4.0));
        // At expiry the curve is the payoff.
        let at_expiry = Market { t: 0.0, ..m };
        assert!(close(
            pl_before_expiry(std::slice::from_ref(&c), 120.0, &at_expiry).unwrap(),
            16.0
        ));

        let straddle = [call(Buy, 100.0, 5.0), put(Buy, 100.0, 4.0)];
        let g = net_greeks(&straddle, 100.0, &m).unwrap();
        let gc = greeks::greeks(
            OptionKind::Call,
            &BsInput {
                spot: 100.0,
                strike: 100.0,
                t: 0.25,
                rate: 0.05,
                div_yield: 0.0,
                vol: 0.2,
            },
        )
        .unwrap();
        assert!(
            close(g.delta, 2.0 * gc.delta - 1.0),
            "put delta = call delta - 1"
        );
        assert!(close(g.gamma, 2.0 * gc.gamma));
        assert!(g.theta < 0.0 && g.vega > 0.0);

        let covered = [
            leg(Buy, 1, Instrument::Underlying, None, 100.0),
            call(Sell, 110.0, 3.0),
        ];
        let g = net_greeks(&covered, 100.0, &m).unwrap();
        assert!(g.delta > 0.0 && g.delta < 1.0 && g.theta > 0.0);

        let mut no_iv = c.clone();
        no_iv.iv = None;
        assert!(net_greeks(&[no_iv.clone()], 100.0, &m).is_none());
        assert!(pl_before_expiry(&[no_iv], 100.0, &m).is_none());
    }

    #[test]
    fn presets_build_their_legs() {
        let strikes = |p: Preset| -> Vec<(Side, u32, Instrument, Option<f64>)> {
            preset_legs(p, 24500.0, 100.0)
        };
        use Instrument::{Call, Put, Underlying};
        assert_eq!(
            strikes(Preset::BullCall),
            vec![
                (Buy, 1, Call, Some(24500.0)),
                (Sell, 1, Call, Some(24600.0))
            ]
        );
        assert_eq!(
            strikes(Preset::BearPut),
            vec![(Buy, 1, Put, Some(24500.0)), (Sell, 1, Put, Some(24400.0))]
        );
        assert_eq!(strikes(Preset::Straddle).len(), 2);
        assert_eq!(
            strikes(Preset::Strangle),
            vec![(Buy, 1, Put, Some(24400.0)), (Buy, 1, Call, Some(24600.0))]
        );
        assert_eq!(
            strikes(Preset::IronCondor),
            vec![
                (Buy, 1, Put, Some(24300.0)),
                (Sell, 1, Put, Some(24400.0)),
                (Sell, 1, Call, Some(24600.0)),
                (Buy, 1, Call, Some(24700.0)),
            ]
        );
        assert_eq!(
            strikes(Preset::Butterfly)[1],
            (Sell, 2, Call, Some(24500.0))
        );
        assert_eq!(
            strikes(Preset::CoveredCall),
            vec![(Buy, 1, Underlying, None), (Sell, 1, Call, Some(24600.0))]
        );
    }

    #[test]
    fn parses_leg_specs() {
        let p = |s: &str| parse_leg(s).unwrap();
        assert_eq!(
            p("buy call 24500"),
            LegSpec {
                side: Buy,
                qty: 1,
                instrument: Instrument::Call,
                strike: Some(StrikeSpec::Price(24500.0)),
                price: None,
            }
        );
        let l = p("SELL 2 PE 24,000 @ 85.5");
        assert_eq!(
            (l.side, l.qty, l.instrument, l.strike, l.price),
            (
                Sell,
                2,
                Instrument::Put,
                Some(StrikeSpec::Price(24000.0)),
                Some(85.5)
            )
        );
        assert_eq!(p("short 3x put atm-2").strike, Some(StrikeSpec::Atm(-2)));
        assert_eq!(p("long c atm+1@10").strike, Some(StrikeSpec::Atm(1)));
        assert_eq!(p("buy call atm").strike, Some(StrikeSpec::Atm(0)));
        let s = p("buy stock @ 2450");
        assert_eq!(
            (s.instrument, s.strike, s.price),
            (Instrument::Underlying, None, Some(2450.0))
        );
        for bad in [
            "",
            "hold call 100",
            "buy call",
            "buy 0 call 100",
            "buy straddle 100",
            "buy call -5",
            "buy call 100 @",
            "buy call 100 @ x",
            "buy call 100 @ 5 extra",
            "buy call 100 200",
            "buy stock 100",
        ] {
            let err = parse_leg(bad).unwrap_err();
            assert!(err.contains("buy|sell"), "{bad}: {err}");
        }
    }
}

//! Walk-forward testing: tune parameters on a rolling train window, then
//! trade the next test window — data the tuning never saw — with the winner.
//! Only the stitched test windows count as results. A strategy that shines in
//! training and fades out of sample is probably fitted to noise.

use super::costs::CostModel;
use super::engine::{self, Asset, Metrics, SimConfig, SimResult, Trade};
use super::rules::{Signals, Strategy, StrategyDef};
use super::{Bars, date};
use anyhow::{Result, bail};
use serde::Serialize;
use std::collections::BTreeMap;

/// Parse a window length into trading days: `90d`, `13w`, `6mo`, `1y`.
pub fn parse_window(s: &str) -> Result<usize, String> {
    let t = s.trim().to_ascii_lowercase();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let n: usize = num
        .parse()
        .map_err(|_| format!("`{s}`: use a length like 90d, 13w, 6mo or 1y"))?;
    let per = match unit {
        "d" => 1,
        "w" => 5,
        "mo" | "m" => 21,
        "y" => 252,
        _ => return Err(format!("`{s}`: use a length like 90d, 13w, 6mo or 1y")),
    };
    match n * per {
        0 => Err(format!("`{s}` is empty")),
        days => Ok(days),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowStats {
    pub start: String,
    pub end: String,
    pub bars: usize,
    #[serde(rename = "return_percent")]
    pub return_pct: f64,
    #[serde(rename = "annualised_percent")]
    pub annualised_pct: Option<f64>,
    pub sharpe: Option<f64>,
    #[serde(rename = "max_drawdown_percent")]
    pub max_drawdown_pct: f64,
    #[serde(rename = "trade_count")]
    pub trades: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Fold {
    /// Parameters that won the train window.
    pub params: BTreeMap<String, f64>,
    pub in_sample: WindowStats,
    pub out_of_sample: WindowStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Out-of-sample results hold up against in-sample.
    Robust,
    /// Out-of-sample is noticeably weaker but still positive.
    Degraded,
    /// The edge mostly or entirely disappears on unseen data.
    LikelyOverfit,
    /// Loses money even on the data it was tuned on.
    NoEdge,
    /// Too few folds or trades to judge.
    Inconclusive,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalkForward {
    #[serde(rename = "train_days")]
    pub train_bars: usize,
    #[serde(rename = "test_days")]
    pub test_bars: usize,
    /// Parameter combinations tried on every train window.
    pub combos: usize,
    pub folds: Vec<Fold>,
    /// Stitched out-of-sample result, after costs.
    pub out_of_sample: Metrics,
    #[serde(rename = "out_of_sample_gross_return_percent")]
    pub out_of_sample_gross_return_pct: f64,
    #[serde(rename = "out_of_sample_annualised_percent")]
    pub out_of_sample_annualised_pct: Option<f64>,
    /// Averages over the train windows of the winning parameters.
    #[serde(rename = "in_sample_annualised_percent")]
    pub in_sample_annualised_pct: Option<f64>,
    pub in_sample_sharpe: Option<f64>,
    /// Out-of-sample ÷ in-sample annualised return (walk-forward efficiency).
    pub efficiency: Option<f64>,
    pub verdict: Verdict,
    pub verdict_reason: String,
    pub trades: Vec<Trade>,
    #[serde(skip)]
    pub times: Vec<i64>,
    #[serde(skip)]
    pub equity: Vec<f64>,
}

fn sim(symbol: &str, bars: &Bars, signals: &Signals, cfg: &SimConfig) -> SimResult {
    engine::simulate(
        &[Asset {
            symbol,
            bars,
            signals,
        }],
        cfg,
    )
}

fn stats(bars: &Bars, r: &SimResult) -> WindowStats {
    let m = r.metrics();
    WindowStats {
        start: date(bars.time[0]),
        end: date(bars.time[bars.len() - 1]),
        bars: bars.len(),
        return_pct: m.total_return_pct,
        annualised_pct: engine::annualised_pct(m.total_return_pct, bars.len()),
        sharpe: m.sharpe,
        max_drawdown_pct: m.max_drawdown_pct,
        trades: m.trades,
    }
}

/// Train windows rank parameters by Sharpe; those that never trade rank last.
fn score(s: &WindowStats) -> (bool, f64, f64) {
    (
        s.trades > 0,
        s.sharpe.unwrap_or(f64::NEG_INFINITY),
        s.return_pct,
    )
}

fn mean(v: impl Iterator<Item = f64>) -> Option<f64> {
    let v: Vec<f64> = v.collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

pub fn run(
    def: &StrategyDef,
    symbol: &str,
    bars: &Bars,
    train: usize,
    test: usize,
    cfg: &SimConfig,
) -> Result<WalkForward> {
    let n = bars.len();
    if train < 20 {
        bail!("--train must cover at least 20 trading days (e.g. 6mo or 1y)");
    }
    if test < 5 {
        bail!("--test must cover at least 5 trading days (e.g. 1mo or 3mo)");
    }
    if n < train + test {
        bail!(
            "Not enough history for walk-forward: {n} daily bars, need {train} (train) + {test} \
             (test). Use a longer period (-p 10y or max) or a shorter --train/--test"
        );
    }

    // Signals depend only on past bars, so compute each parameter set once
    // over the whole history and simulate windows of it.
    let candidates: Vec<(Strategy, Signals)> = def
        .grid_combos()
        .iter()
        .map(|p| {
            let s = def.compile_with(p)?;
            let sig = s.signals(bars);
            Ok((s, sig))
        })
        .collect::<Result<_>>()?;

    let mut windows = Vec::new();
    let mut start = 0;
    while start + train < n {
        let test_end = (start + train + test).min(n);
        if test_end - (start + train) < (test / 2).max(5) && !windows.is_empty() {
            break;
        }
        windows.push((start..start + train, start + train..test_end));
        start += test;
    }

    let (mut capital, mut gross_capital) = (cfg.capital, cfg.capital);
    let (mut times, mut equity, mut invested, mut trades) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut costs = 0.0;
    let mut folds = Vec::new();

    for (train_r, test_r) in windows {
        let train_bars = bars.slice(train_r.clone());
        let mut best: Option<(usize, WindowStats)> = None;
        for (i, (strategy, signals)) in candidates.iter().enumerate() {
            let c = SimConfig {
                stop_loss_pct: strategy.stop_loss_pct,
                take_profit_pct: strategy.take_profit_pct,
                ..cfg.clone()
            };
            let r = sim(symbol, &train_bars, &signals.slice(train_r.clone()), &c);
            let s = stats(&train_bars, &r);
            let better = best.as_ref().is_none_or(|(_, b)| {
                score(&s).partial_cmp(&score(b)) == Some(std::cmp::Ordering::Greater)
            });
            if better {
                best = Some((i, s));
            }
        }
        let (i, in_sample) = best.expect("at least one parameter combination");
        let (strategy, signals) = &candidates[i];
        let test_bars = bars.slice(test_r.clone());
        let test_signals = signals.slice(test_r.clone());
        let c = SimConfig {
            capital,
            stop_loss_pct: strategy.stop_loss_pct,
            take_profit_pct: strategy.take_profit_pct,
            ..cfg.clone()
        };
        let r = sim(symbol, &test_bars, &test_signals, &c);
        let g = sim(
            symbol,
            &test_bars,
            &test_signals,
            &SimConfig {
                capital: gross_capital,
                costs: CostModel::none(),
                ..c.clone()
            },
        );
        capital = r.equity.last().copied().unwrap_or(capital);
        gross_capital = g.equity.last().copied().unwrap_or(gross_capital);
        costs += r.costs.total;
        times.extend_from_slice(&r.times);
        equity.extend_from_slice(&r.equity);
        invested.extend_from_slice(&r.invested);
        trades.extend(r.trades.iter().cloned());
        folds.push(Fold {
            params: strategy.params.clone(),
            in_sample,
            out_of_sample: stats(&test_bars, &r),
        });
    }

    let oos = engine::metrics(cfg.capital, &times, &equity, &invested, &trades, costs);
    let oos_ann = engine::annualised_pct(oos.total_return_pct, equity.len());
    let is_ann = mean(folds.iter().filter_map(|f| f.in_sample.annualised_pct));
    let is_sharpe = mean(folds.iter().filter_map(|f| f.in_sample.sharpe));
    let efficiency = match (oos_ann, is_ann) {
        (Some(o), Some(i)) if i > 0.0 => Some(o / i),
        _ => None,
    };
    let (verdict, verdict_reason) = judge(
        folds.len(),
        oos.trades,
        is_ann,
        oos_ann,
        is_sharpe,
        oos.sharpe,
        efficiency,
    );

    Ok(WalkForward {
        train_bars: train,
        test_bars: test,
        combos: candidates.len(),
        folds,
        out_of_sample_gross_return_pct: (gross_capital / cfg.capital - 1.0) * 100.0,
        out_of_sample: oos,
        out_of_sample_annualised_pct: oos_ann,
        in_sample_annualised_pct: is_ann,
        in_sample_sharpe: is_sharpe,
        efficiency,
        verdict,
        verdict_reason,
        trades,
        times,
        equity,
    })
}

fn judge(
    folds: usize,
    oos_trades: usize,
    is_ann: Option<f64>,
    oos_ann: Option<f64>,
    is_sharpe: Option<f64>,
    oos_sharpe: Option<f64>,
    efficiency: Option<f64>,
) -> (Verdict, String) {
    let pct = |v: f64| format!("{v:+.1}%/yr");
    if folds < 2 || oos_trades == 0 {
        return (
            Verdict::Inconclusive,
            format!(
                "{folds} test window(s) and {oos_trades} out-of-sample trade(s) are too few to \
                 judge — use a longer period or shorter windows"
            ),
        );
    }
    let (Some(is_ann), Some(oos_ann)) = (is_ann, oos_ann) else {
        return (
            Verdict::Inconclusive,
            "returns could not be annualised".into(),
        );
    };
    if is_ann <= 0.0 {
        return (
            Verdict::NoEdge,
            format!(
                "loses money even on the windows it was tuned on ({})",
                pct(is_ann)
            ),
        );
    }
    if oos_ann <= 0.0 {
        return (
            Verdict::LikelyOverfit,
            format!(
                "made {} in-sample but {} on unseen data",
                pct(is_ann),
                pct(oos_ann)
            ),
        );
    }
    let gap = match (is_sharpe, oos_sharpe) {
        (Some(i), Some(o)) => i - o,
        _ => 0.0,
    };
    let eff = efficiency.unwrap_or(0.0);
    let detail = format!(
        "out-of-sample keeps {:.0}% of the in-sample return ({} vs {}); Sharpe gap {:.2}",
        eff * 100.0,
        pct(oos_ann),
        pct(is_ann),
        gap
    );
    if eff < 0.3 || gap > 1.0 {
        (Verdict::LikelyOverfit, detail)
    } else if eff < 0.6 || gap > 0.5 {
        (Verdict::Degraded, detail)
    } else {
        (Verdict::Robust, detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::engine::Fill;
    use crate::backtest::rules;

    fn cyclic(n: usize) -> Bars {
        let closes: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64;
                100.0 + (t * 0.09).sin() * 10.0 + (t * 0.37).sin() * 3.0 + t * 0.01
            })
            .collect();
        let mut b = Bars::from_closes(&closes);
        b.open = std::iter::once(closes[0])
            .chain(closes.iter().copied())
            .take(n)
            .collect();
        for i in 0..n {
            b.high[i] = b.open[i].max(b.close[i]) * 1.005;
            b.low[i] = b.open[i].min(b.close[i]) * 0.995;
        }
        b
    }

    #[test]
    fn window_lengths() {
        assert_eq!(parse_window("1y"), Ok(252));
        assert_eq!(parse_window("6mo"), Ok(126));
        assert_eq!(parse_window("3M"), Ok(63));
        assert_eq!(parse_window("13w"), Ok(65));
        assert_eq!(parse_window("90d"), Ok(90));
        assert!(parse_window("0d").is_err());
        assert!(parse_window("1q").is_err());
        assert!(parse_window("y").is_err());
    }

    #[test]
    fn folds_roll_forward_and_only_test_windows_count() {
        let bars = cyclic(600);
        let def = rules::load("rsi").unwrap();
        let cfg = SimConfig::single(100_000.0, Fill::NextOpen, CostModel::in_delivery());
        let wf = run(&def, "SYN", &bars, 252, 63, &cfg).unwrap();
        assert_eq!(wf.combos, 9);
        // 600 bars: tests start at 252, 315, 378, 441, 504, 567 (33 bars ≥ 31).
        assert_eq!(wf.folds.len(), 6);
        assert_eq!(wf.equity.len(), 600 - 252);
        assert!(
            wf.folds
                .windows(2)
                .all(|f| f[0].out_of_sample.end < f[1].out_of_sample.start)
        );
        assert!(
            wf.folds
                .iter()
                .all(|f| f.in_sample.end < f.out_of_sample.start)
        );
        // Every winning parameter set came from the grid.
        for f in &wf.folds {
            assert!([25.0, 30.0, 35.0].contains(&f.params["oversold"]));
        }
        // Costs only make things worse.
        assert!(wf.out_of_sample_gross_return_pct >= wf.out_of_sample.total_return_pct);
    }

    #[test]
    fn too_short_history_is_an_error() {
        let bars = cyclic(200);
        let def = rules::load("rsi").unwrap();
        let cfg = SimConfig::single(100_000.0, Fill::NextOpen, CostModel::none());
        let e = run(&def, "SYN", &bars, 252, 63, &cfg).unwrap_err();
        assert!(e.to_string().contains("Not enough history"));
    }

    #[test]
    fn verdicts() {
        let j = |is: f64, oos: f64, gap: f64| {
            judge(
                4,
                10,
                Some(is),
                Some(oos),
                Some(1.0 + gap),
                Some(1.0),
                Some(oos / is),
            )
            .0
        };
        assert_eq!(j(20.0, 18.0, 0.1), Verdict::Robust);
        assert_eq!(j(20.0, 10.0, 0.1), Verdict::Degraded);
        assert_eq!(j(20.0, 4.0, 0.1), Verdict::LikelyOverfit);
        assert_eq!(j(20.0, 18.0, 1.5), Verdict::LikelyOverfit);
        assert_eq!(j(20.0, -5.0, 0.0), Verdict::LikelyOverfit);
        assert_eq!(j(-3.0, -5.0, 0.0), Verdict::NoEdge);
        assert_eq!(
            judge(1, 10, Some(5.0), Some(5.0), None, None, Some(1.0)).0,
            Verdict::Inconclusive
        );
    }
}

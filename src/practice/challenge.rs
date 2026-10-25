//! Goal-based challenges: data-driven rules checked as you trade.
//!
//! Built-ins live in `src/practice/challenges.toml` (embedded); `file:PATH`
//! loads one `[[challenge]]` table from your own file. Constraint rules are
//! checked when you place an order or after every replay day; a broken hard
//! rule ends the challenge, a broken soft one costs discipline points. Goal
//! rules are checked when the challenge ends. The functions here work on
//! plain facts, so a replay and live paper sessions share them.

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

const BUILT_IN: &str = include_str!("challenges.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Over a historical replay (`sim replay`).
    Replay,
    /// Over live paper sessions (`sim start` / `sim settle`).
    Live,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    /// The drawdown from the challenge's peak stays at or under `percent`.
    MaxDrawdown {
        percent: f64,
        #[serde(default)]
        hard: bool,
    },
    /// No single position above `percent` of equity, checked when an order
    /// is placed (at the last close).
    MaxPosition {
        percent: f64,
        #[serde(default)]
        hard: bool,
    },
    /// Every buy carries a stop-loss.
    StopLossRequired {
        #[serde(default)]
        hard: bool,
    },
    /// At each month end, at least `percent` of equity is invested.
    InvestEachMonth {
        percent: f64,
        #[serde(default)]
        hard: bool,
    },
    /// Goal: a higher time-weighted return than the benchmark (plus a margin).
    BeatBenchmark {
        #[serde(default)]
        margin_percent: f64,
    },
    /// Goal: end with more than a plain monthly SIP into the benchmark.
    BeatSip,
    /// Goal: average share of equity invested.
    MinExposure { percent: f64 },
    /// Goal: hold at least `count` positions at one close.
    MinPositions { count: usize },
    /// Goal: at least `count` buys filled.
    MinTrades { count: usize },
    /// Goal: your worst drawdown at most `ratio` × the benchmark's.
    DrawdownVsBenchmark { ratio: f64 },
}

impl Rule {
    /// Plain-English summary, e.g. "max drawdown 10% (hard)".
    pub fn label(&self) -> String {
        let hard = |h: bool| if h { " (hard)" } else { "" };
        match self {
            Rule::MaxDrawdown { percent, hard: h } => {
                format!("drawdown at most {percent}%{}", hard(*h))
            }
            Rule::MaxPosition { percent, hard: h } => {
                format!("no position above {percent}% of capital{}", hard(*h))
            }
            Rule::StopLossRequired { hard: h } => format!("every buy has a stop-loss{}", hard(*h)),
            Rule::InvestEachMonth { percent, hard: h } => {
                format!("at least {percent}% invested at each month end{}", hard(*h))
            }
            Rule::BeatBenchmark { margin_percent } if *margin_percent > 0.0 => {
                format!("beat the benchmark by {margin_percent} points")
            }
            Rule::BeatBenchmark { .. } => "beat the benchmark".into(),
            Rule::BeatSip => "beat a plain monthly SIP into the benchmark".into(),
            Rule::MinExposure { percent } => format!("on average at least {percent}% invested"),
            Rule::MinPositions { count } => format!("hold {count} or more stocks at once"),
            Rule::MinTrades { count } => format!("make {count} or more buys"),
            Rule::DrawdownVsBenchmark { ratio } => {
                format!("worst drawdown under {ratio}× the benchmark's")
            }
        }
    }

    pub fn is_goal(&self) -> bool {
        matches!(
            self,
            Rule::BeatBenchmark { .. }
                | Rule::BeatSip
                | Rule::MinExposure { .. }
                | Rule::MinPositions { .. }
                | Rule::MinTrades { .. }
                | Rule::DrawdownVsBenchmark { .. }
        )
    }

    pub fn is_hard(&self) -> bool {
        matches!(
            self,
            Rule::MaxDrawdown { hard: true, .. }
                | Rule::MaxPosition { hard: true, .. }
                | Rule::StopLossRequired { hard: true }
                | Rule::InvestEachMonth { hard: true, .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge {
    pub name: String,
    pub title: String,
    pub description: String,
    /// Trading days (replay) or settled sessions (live); 0 = until the
    /// replay ends.
    #[serde(default)]
    pub days: usize,
    pub modes: Vec<Mode>,
    /// Money added on the first trading day of each month (SIP challenges).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_contribution: Option<f64>,
    pub rules: Vec<Rule>,
}

#[derive(Deserialize)]
struct File {
    challenge: Vec<Challenge>,
}

fn parse_file(text: &str) -> Result<Vec<Challenge>> {
    let f: File = toml::from_str(text).context("Not a valid challenge file")?;
    for c in &f.challenge {
        if c.rules.is_empty() || c.modes.is_empty() {
            bail!("Challenge {}: needs at least one rule and one mode", c.name);
        }
        if c.days == 0 && !c.modes.contains(&Mode::Replay) {
            bail!("Challenge {}: days = 0 only works over a replay", c.name);
        }
        let sip = c.rules.contains(&Rule::BeatSip);
        if sip != c.monthly_contribution.is_some_and(|a| a > 0.0) {
            bail!(
                "Challenge {}: beat_sip needs monthly_contribution, and the other way round",
                c.name
            );
        }
        if sip && c.modes.contains(&Mode::Live) {
            bail!(
                "Challenge {}: SIP challenges run over a replay only",
                c.name
            );
        }
    }
    Ok(f.challenge)
}

pub fn built_in() -> Vec<Challenge> {
    parse_file(BUILT_IN).expect("built-in challenges are valid")
}

/// A built-in challenge by name, or `file:PATH` (its first challenge).
pub fn load(name: &str) -> Result<Challenge> {
    if let Some(path) = name.strip_prefix("file:") {
        let text = std::fs::read_to_string(path).with_context(|| format!("Cannot read {path}"))?;
        return parse_file(&text)
            .with_context(|| format!("In {path}"))?
            .into_iter()
            .next()
            .with_context(|| format!("{path} has no [[challenge]]"));
    }
    let key = name.trim().to_ascii_lowercase();
    let all = built_in();
    let names: Vec<String> = all.iter().map(|c| c.name.clone()).collect();
    all.into_iter().find(|c| c.name == key).with_context(|| {
        format!(
            "No challenge called {name:?}. Built-in: {}. Or use file:PATH.toml",
            names.join(", ")
        )
    })
}

/// A rule broken during a challenge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    pub date: NaiveDate,
    pub rule: String,
    pub detail: String,
    pub hard: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalResult {
    pub goal: String,
    pub met: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub name: String,
    pub title: String,
    pub passed: bool,
    /// Why it ended: "completed", "abandoned", or the hard rule broken.
    pub reason: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub goals: Vec<GoalResult>,
    pub violations: Vec<Violation>,
    /// Never missed a month-end investing check (SIP challenges).
    #[serde(default)]
    pub sip: bool,
}

/// A running challenge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Active {
    pub def: Challenge,
    /// Replay day index the challenge started on (0 for live).
    pub start_day: usize,
    pub start_date: NaiveDate,
    /// Replay day index it ends on (inclusive); live: sessions needed.
    pub end_day: usize,
    pub violations: Vec<Violation>,
    /// Benchmark units bought by the plain SIP it is compared with.
    #[serde(default)]
    pub sip_units: f64,
    #[serde(default)]
    pub contributed: f64,
}

impl Active {
    pub fn new(def: Challenge, start_day: usize, start_date: NaiveDate, end_day: usize) -> Self {
        Active {
            def,
            start_day,
            start_date,
            end_day,
            violations: Vec::new(),
            sip_units: 0.0,
            contributed: 0.0,
        }
    }

    pub fn hard_broken(&self) -> Option<&Violation> {
        self.violations.iter().find(|v| v.hard)
    }

    pub fn soft_violations(&self) -> usize {
        self.violations.iter().filter(|v| !v.hard).count()
    }

    fn already(&self, rule: &str) -> bool {
        self.violations.iter().any(|v| v.rule == rule)
    }

    /// Rules an order breaks. `weight_percent` is the position's share of
    /// equity after the order, at the last close.
    pub fn check_buy(
        &self,
        date: NaiveDate,
        symbol: &str,
        with_stop: bool,
        weight_percent: f64,
    ) -> Vec<Violation> {
        let mut out = Vec::new();
        for rule in &self.def.rules {
            match rule {
                Rule::StopLossRequired { hard } if !with_stop => out.push(Violation {
                    date,
                    rule: rule.label(),
                    detail: format!("buy of {symbol} without --stop"),
                    hard: *hard,
                }),
                Rule::MaxPosition { percent, hard } if weight_percent > *percent + 1e-9 => {
                    out.push(Violation {
                        date,
                        rule: rule.label(),
                        detail: format!(
                            "{symbol} would be {weight_percent:.1}% of your capital (limit {percent}%)"
                        ),
                        hard: *hard,
                    })
                }
                _ => {}
            }
        }
        out
    }

    /// Check the day that just closed: `drawdown_percent` from the
    /// challenge's peak, and — when a month just ended — the invested share
    /// at that month's last close. Returns the new violations (once per rule,
    /// except one per month for the investing rule).
    pub fn check_day(
        &mut self,
        date: NaiveDate,
        drawdown_percent: f64,
        month_end_invested_percent: Option<f64>,
    ) -> Vec<Violation> {
        let mut new = Vec::new();
        for rule in &self.def.rules {
            match rule {
                Rule::MaxDrawdown { percent, hard }
                    if drawdown_percent > *percent + 1e-9 && !self.already(&rule.label()) =>
                {
                    new.push(Violation {
                        date,
                        rule: rule.label(),
                        detail: format!("drawdown reached {drawdown_percent:.1}%"),
                        hard: *hard,
                    })
                }
                Rule::InvestEachMonth { percent, hard } => {
                    if let Some(inv) = month_end_invested_percent
                        && inv + 1e-9 < *percent
                    {
                        new.push(Violation {
                            date,
                            rule: rule.label(),
                            detail: format!("only {inv:.0}% invested at the month end"),
                            hard: *hard,
                        })
                    }
                }
                _ => {}
            }
        }
        self.violations.extend(new.iter().cloned());
        new
    }

    /// End the challenge: a hard break fails it; otherwise every goal must
    /// be met. `abandoned` ends it before its last day (a fail).
    pub fn finish(&self, to: NaiveDate, facts: &Facts, abandoned: bool) -> Outcome {
        let goals: Vec<GoalResult> = self
            .def
            .rules
            .iter()
            .filter(|r| r.is_goal())
            .map(|r| goal(r, facts))
            .collect();
        let (passed, reason) = if let Some(v) = self.hard_broken() {
            (false, format!("broke a hard rule: {}", v.rule))
        } else if abandoned {
            (false, "abandoned".to_string())
        } else {
            (goals.iter().all(|g| g.met), "completed".to_string())
        };
        let month_rule = self
            .def
            .rules
            .iter()
            .find(|r| matches!(r, Rule::InvestEachMonth { .. }))
            .map(Rule::label);
        let sip = month_rule.is_some_and(|l| !self.violations.iter().any(|v| v.rule == l));
        Outcome {
            name: self.def.name.clone(),
            title: self.def.title.clone(),
            passed,
            reason,
            from: self.start_date,
            to,
            goals,
            violations: self.violations.clone(),
            sip: sip && passed,
        }
    }
}

/// What the goals are judged on, over the challenge's days only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub return_percent: f64,
    pub benchmark_return_percent: f64,
    pub exposure_percent: f64,
    pub max_drawdown_percent: f64,
    pub benchmark_max_drawdown_percent: f64,
    pub max_positions: usize,
    pub buys: usize,
    pub equity: f64,
    /// Value of the plain SIP (SIP challenges).
    pub sip_value: Option<f64>,
}

fn goal(rule: &Rule, f: &Facts) -> GoalResult {
    let (met, detail) = match rule {
        Rule::BeatBenchmark { margin_percent } => (
            f.return_percent > f.benchmark_return_percent + margin_percent,
            format!(
                "you {:+.1}%, benchmark {:+.1}%",
                f.return_percent, f.benchmark_return_percent
            ),
        ),
        Rule::BeatSip => match f.sip_value {
            Some(sip) => (
                f.equity > sip,
                format!("you {:.0}, plain SIP {:.0}", f.equity, sip),
            ),
            None => (false, "no SIP to compare with".into()),
        },
        Rule::MinExposure { percent } => (
            f.exposure_percent + 1e-9 >= *percent,
            format!("{:.0}% invested on average", f.exposure_percent),
        ),
        Rule::MinPositions { count } => (
            f.max_positions >= *count,
            format!("at most {} held at once", f.max_positions),
        ),
        Rule::MinTrades { count } => (f.buys >= *count, format!("{} buys", f.buys)),
        Rule::DrawdownVsBenchmark { ratio } => {
            let limit = f.benchmark_max_drawdown_percent * ratio;
            (
                f.benchmark_max_drawdown_percent > 0.0 && f.max_drawdown_percent < limit,
                format!(
                    "your worst drawdown {:.1}%, benchmark {:.1}%",
                    f.max_drawdown_percent, f.benchmark_max_drawdown_percent
                ),
            )
        }
        _ => (true, String::new()),
    };
    GoalResult {
        goal: rule.label(),
        met,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::tape::tests::d;

    #[test]
    fn built_ins_parse_with_the_documented_rules() {
        let all = built_in();
        let names: Vec<&str> = all.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "beat-the-index",
                "steady-hands",
                "position-limit",
                "stop-loss",
                "sip-discipline",
                "crash-survivor"
            ]
        );
        let sip = load("SIP-discipline").unwrap();
        assert_eq!(sip.monthly_contribution, Some(10_000.0));
        assert_eq!(sip.modes, [Mode::Replay]);
        assert!(
            load("nope")
                .unwrap_err()
                .to_string()
                .contains("steady-hands")
        );
        let steady = load("steady-hands").unwrap();
        assert_eq!(
            steady.rules[0],
            Rule::MaxDrawdown {
                percent: 10.0,
                hard: true
            }
        );
        assert!(steady.rules[0].is_hard() && !steady.rules[1].is_hard());
        assert_eq!(steady.rules[0].label(), "drawdown at most 10% (hard)");
    }

    #[test]
    fn invalid_challenge_files_are_refused() {
        let ok = r#"
            [[challenge]]
            name = "x"
            title = "X"
            description = "d"
            days = 10
            modes = ["live"]
            [[challenge.rules]]
            kind = "min_trades"
            count = 2
        "#;
        assert_eq!(parse_file(ok).unwrap().len(), 1);
        assert!(parse_file(&ok.replace("min_trades", "max_fun")).is_err());
        assert!(parse_file(&ok.replace("days = 10", "days = 0")).is_err());
        assert!(parse_file(&ok.replace("count = 2", "count = 2\nextra = 1")).is_err());
        let sip_without_money = ok.replace(
            "kind = \"min_trades\"\n            count = 2",
            "kind = \"beat_sip\"",
        );
        assert!(parse_file(&sip_without_money).is_err());
    }

    fn active(rules: Vec<Rule>) -> Active {
        let def = Challenge {
            name: "t".into(),
            title: "T".into(),
            description: String::new(),
            days: 5,
            modes: vec![Mode::Replay],
            monthly_contribution: None,
            rules,
        };
        Active::new(def, 0, d("2020-01-01"), 5)
    }

    #[test]
    fn order_rules() {
        let a = active(vec![
            Rule::StopLossRequired { hard: true },
            Rule::MaxPosition {
                percent: 20.0,
                hard: false,
            },
        ]);
        let day = d("2020-01-02");
        assert!(a.check_buy(day, "TCS.NS", true, 20.0).is_empty());
        let v = a.check_buy(day, "TCS.NS", false, 25.0);
        assert_eq!(v.len(), 2);
        assert!(v[0].hard && !v[1].hard);
        assert!(v[1].detail.contains("25.0%"));
    }

    #[test]
    fn day_rules_fire_once_and_hard_breaks_fail() {
        let mut a = active(vec![
            Rule::MaxDrawdown {
                percent: 10.0,
                hard: true,
            },
            Rule::InvestEachMonth {
                percent: 90.0,
                hard: false,
            },
            Rule::MinTrades { count: 1 },
        ]);
        assert!(a.check_day(d("2020-01-02"), 9.0, None).is_empty());
        assert_eq!(a.check_day(d("2020-01-03"), 11.0, Some(50.0)).len(), 2);
        // The drawdown rule is already broken; the month rule fires again.
        assert_eq!(a.check_day(d("2020-02-03"), 12.0, Some(10.0)).len(), 1);
        assert_eq!(a.soft_violations(), 2);
        let facts = Facts {
            buys: 3,
            ..Facts::default()
        };
        let out = a.finish(d("2020-02-03"), &facts, false);
        assert!(!out.passed);
        assert!(out.reason.contains("drawdown"), "{}", out.reason);
        assert!(out.goals[0].met);
    }

    #[test]
    fn goals_decide_a_clean_challenge() {
        let a = active(vec![
            Rule::BeatBenchmark {
                margin_percent: 0.0,
            },
            Rule::DrawdownVsBenchmark { ratio: 0.5 },
            Rule::MinExposure { percent: 30.0 },
        ]);
        let facts = Facts {
            return_percent: -5.0,
            benchmark_return_percent: -20.0,
            exposure_percent: 40.0,
            max_drawdown_percent: 9.0,
            benchmark_max_drawdown_percent: 30.0,
            ..Facts::default()
        };
        let won = a.finish(d("2020-03-01"), &facts, false);
        assert!(won.passed, "{won:?}");
        assert_eq!(won.reason, "completed");
        let lost = a.finish(
            d("2020-03-01"),
            &Facts {
                max_drawdown_percent: 16.0,
                ..facts.clone()
            },
            false,
        );
        assert!(!lost.passed);
        assert!(!a.finish(d("2020-03-01"), &facts, true).passed);
    }

    #[test]
    fn sip_goal() {
        let a = active(vec![Rule::BeatSip]);
        let facts = Facts {
            equity: 31_000.0,
            sip_value: Some(30_000.0),
            ..Facts::default()
        };
        assert!(a.finish(d("2020-03-01"), &facts, false).passed);
        let behind = Facts {
            sip_value: Some(32_000.0),
            ..facts
        };
        assert!(!a.finish(d("2020-03-01"), &behind, false).passed);
    }
}

//! Your practice record: badges, streaks, best scores and recent runs, kept
//! in `sim_profile.json` in the data directory.

use super::challenge::Outcome;
use super::score::{self, Scorecard};
use anyhow::Result;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FILE: &str = "sim_profile.json";
/// Runs kept for `sim profile` and `sim card`.
const KEEP_RUNS: usize = 50;
/// Disciplined runs in a row for the streak badge.
const STREAK_BADGE: u32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Earned {
    pub id: String,
    pub date: NaiveDate,
    /// The run or challenge that earned it.
    pub run: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Streak {
    pub current: u32,
    pub best: u32,
}

impl Streak {
    fn hit(&mut self, kept: bool) {
        if kept {
            self.current += 1;
            self.best = self.best.max(self.current);
        } else {
            self.current = 0;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Streaks {
    /// Scored runs in a row with every buy stopped and sized, no rule broken.
    pub disciplined: Streak,
    /// Challenges passed in a row.
    pub challenges: Streak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Best {
    pub score: u32,
    pub grade: String,
    pub date: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeSummary {
    pub name: String,
    pub title: String,
    pub passed: bool,
    pub reason: String,
}

impl From<&Outcome> for ChallengeSummary {
    fn from(o: &Outcome) -> Self {
        ChallengeSummary {
            name: o.name.clone(),
            title: o.title.clone(),
            passed: o.passed,
            reason: o.reason.clone(),
        }
    }
}

/// One finished replay or live challenge, as the card shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    /// `replay` or `live`.
    pub kind: String,
    /// Scenario name, or `live`.
    pub name: String,
    pub title: String,
    /// `in` or `us`.
    pub market: String,
    pub benchmark_name: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub finished: NaiveDate,
    pub scorecard: Scorecard,
    /// Badges this run qualified for.
    pub badges: Vec<String>,
    /// Badges earned for the first time by this run.
    pub new_badges: Vec<String>,
    pub disciplined: bool,
    pub challenges: Vec<ChallengeSummary>,
    /// Account and benchmark rebased to 100, thinned for the card.
    pub curve: Vec<f64>,
    pub benchmark_curve: Vec<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub badges: Vec<Earned>,
    #[serde(default)]
    pub streaks: Streaks,
    /// Best score per scenario (and `live`).
    #[serde(default)]
    pub best: BTreeMap<String, Best>,
    /// Oldest first.
    #[serde(default)]
    pub runs: Vec<RunResult>,
    #[serde(default)]
    pub challenges_passed: u32,
    #[serde(default)]
    pub challenges_failed: u32,
}

impl Profile {
    pub fn load() -> Result<Profile> {
        crate::store::load(FILE)
    }

    pub fn save(&self) -> Result<()> {
        crate::store::save(FILE, self, false)
    }

    pub fn has(&self, id: &str) -> bool {
        self.badges.iter().any(|b| b.id == id)
    }

    /// Give a badge once. True if it is new.
    fn award(&mut self, id: &str, date: NaiveDate, run: &str) -> bool {
        if self.has(id) {
            return false;
        }
        self.badges.push(Earned {
            id: id.to_string(),
            date,
            run: run.to_string(),
        });
        true
    }

    /// A challenge ended: update the streak and award its badges. Returns
    /// the badges earned for the first time.
    pub fn record_challenge(&mut self, outcome: &Outcome, today: NaiveDate) -> Vec<String> {
        self.streaks.challenges.hit(outcome.passed);
        if outcome.passed {
            self.challenges_passed += 1;
        } else {
            self.challenges_failed += 1;
        }
        let mut new = Vec::new();
        for (id, earned) in [
            ("challenge-complete", outcome.passed),
            ("sip-discipline", outcome.sip),
        ] {
            if earned && self.award(id, today, &outcome.title) {
                new.push(id.to_string());
            }
        }
        new
    }

    /// A run ended: update the discipline streak, best score and badges.
    /// Returns the run as stored, with `new_badges` filled in.
    pub fn record_run(&mut self, mut run: RunResult, today: NaiveDate) -> RunResult {
        if run.scorecard.score.is_some() {
            self.streaks.disciplined.hit(run.disciplined);
            if self.streaks.disciplined.current >= STREAK_BADGE
                && !run.badges.iter().any(|b| b == "three-in-a-row")
            {
                run.badges.push("three-in-a-row".into());
            }
        }
        run.new_badges = run
            .badges
            .iter()
            .filter(|id| score::badge(id).is_some())
            .filter(|id| self.award(id, today, &run.title))
            .cloned()
            .collect();
        if let (Some(points), Some(grade)) = (run.scorecard.score, &run.scorecard.grade)
            && self.best.get(&run.name).is_none_or(|b| points > b.score)
        {
            self.best.insert(
                run.name.clone(),
                Best {
                    score: points,
                    grade: grade.clone(),
                    date: today,
                },
            );
        }
        run.finished = today;
        self.runs.push(run.clone());
        if self.runs.len() > KEEP_RUNS {
            self.runs.drain(..self.runs.len() - KEEP_RUNS);
        }
        run
    }

    pub fn last_run(&self) -> Option<&RunResult> {
        self.runs.last()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::practice::tape::tests::d;

    pub fn run(name: &str, score: Option<u32>, disciplined: bool, badges: &[&str]) -> RunResult {
        RunResult {
            kind: "replay".into(),
            name: name.into(),
            title: format!("Run {name}"),
            market: "in".into(),
            benchmark_name: "NIFTY 50".into(),
            from: d("2020-02-03"),
            to: d("2020-06-30"),
            finished: d("2026-01-01"),
            scorecard: Scorecard {
                days: 100,
                score,
                grade: score.map(|s| score::grade(s).to_string()),
                ..Scorecard::default()
            },
            badges: badges.iter().map(|b| b.to_string()).collect(),
            new_badges: Vec::new(),
            disciplined,
            challenges: Vec::new(),
            curve: vec![100.0, 101.0],
            benchmark_curve: vec![100.0, 90.0],
        }
    }

    #[test]
    fn streaks_badges_and_best_scores() {
        let mut p = Profile::default();
        let today = d("2026-10-25");
        let r = p.record_run(run("a", Some(70), true, &["kept-your-stops"]), today);
        assert_eq!(r.new_badges, ["kept-your-stops"]);
        assert_eq!(r.finished, today);
        let r = p.record_run(run("a", Some(60), true, &["kept-your-stops"]), today);
        assert!(r.new_badges.is_empty());
        assert_eq!(p.best["a"].score, 70);
        // An unscored run leaves the streak alone.
        p.record_run(run("b", None, false, &[]), today);
        assert_eq!(p.streaks.disciplined.current, 2);
        let r = p.record_run(run("b", Some(81), true, &[]), today);
        assert_eq!(r.new_badges, ["three-in-a-row"]);
        assert_eq!(p.best["b"].grade, "A");
        p.record_run(run("c", Some(40), false, &[]), today);
        assert_eq!(
            p.streaks.disciplined,
            Streak {
                current: 0,
                best: 3
            }
        );
        assert_eq!(p.last_run().unwrap().name, "c");
        // Unknown badge ids are not stored.
        let r = p.record_run(run("d", Some(50), false, &["moon"]), today);
        assert!(r.new_badges.is_empty() && !p.has("moon"));
    }

    #[test]
    fn challenges_update_their_streak() {
        let mut p = Profile::default();
        let out = |passed: bool, sip: bool| Outcome {
            name: "x".into(),
            title: "X".into(),
            passed,
            reason: String::new(),
            from: d("2020-01-01"),
            to: d("2020-02-01"),
            goals: Vec::new(),
            violations: Vec::new(),
            sip,
        };
        let today = d("2026-10-25");
        assert_eq!(
            p.record_challenge(&out(true, true), today),
            ["challenge-complete", "sip-discipline"]
        );
        assert!(p.record_challenge(&out(true, false), today).is_empty());
        p.record_challenge(&out(false, false), today);
        assert_eq!(
            p.streaks.challenges,
            Streak {
                current: 0,
                best: 2
            }
        );
        assert_eq!((p.challenges_passed, p.challenges_failed), (2, 1));
    }

    #[test]
    fn keeps_the_last_runs_only() {
        let mut p = Profile::default();
        for i in 0..(KEEP_RUNS + 5) {
            p.record_run(run(&i.to_string(), Some(50), false, &[]), d("2026-01-01"));
        }
        assert_eq!(p.runs.len(), KEEP_RUNS);
        assert_eq!(p.runs[0].name, "5");
    }
}

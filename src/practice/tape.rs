//! A replay's price data, fetched once when the replay starts.
//!
//! The tape holds every daily bar of the scenario, the future included, and
//! is the only place those bars live. Code outside `practice` reads them
//! through [`Tape::upto`] and [`Tape::last_close`], which stop at a date it
//! passes — the replay's current day — so nothing a command prints, charts
//! or serialises could not have been known on that day. Only the replay
//! engine reads the next day's bar, while it steps onto that day.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FILE: &str = "sim_replay_tape.json";

/// One daily bar.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DayBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Series {
    currency: Option<String>,
    /// Oldest first, one bar per date.
    bars: Vec<DayBar>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tape {
    series: BTreeMap<String, Series>,
}

impl Tape {
    /// Add `symbol`'s bars (any order; a repeated date keeps the last bar).
    pub fn insert(&mut self, symbol: &str, currency: Option<String>, mut bars: Vec<DayBar>) {
        bars.sort_by_key(|b| b.date);
        let mut out: Vec<DayBar> = Vec::with_capacity(bars.len());
        for b in bars {
            match out.last_mut() {
                Some(last) if last.date == b.date => *last = b,
                _ => out.push(b),
            }
        }
        self.series.insert(
            symbol.to_string(),
            Series {
                currency,
                bars: out,
            },
        );
    }

    /// Daily bars from the backtest loader (dated in UTC, which is the
    /// exchange's own date for NSE and NYSE daily bars).
    pub fn day_bars(bars: &crate::backtest::Bars) -> Vec<DayBar> {
        (0..bars.len())
            .filter_map(|i| {
                Some(DayBar {
                    date: chrono::DateTime::from_timestamp(bars.time[i], 0)?.date_naive(),
                    open: bars.open[i],
                    high: bars.high[i],
                    low: bars.low[i],
                    close: bars.close[i],
                })
            })
            .collect()
    }

    pub fn has(&self, symbol: &str) -> bool {
        self.series.contains_key(symbol)
    }

    pub fn currency(&self, symbol: &str) -> Option<&str> {
        self.series.get(symbol)?.currency.as_deref()
    }

    /// Bars of `symbol` dated `date` or earlier.
    pub fn upto(&self, symbol: &str, date: NaiveDate) -> &[DayBar] {
        let Some(s) = self.series.get(symbol) else {
            return &[];
        };
        let n = s.bars.partition_point(|b| b.date <= date);
        &s.bars[..n]
    }

    /// The latest close on or before `date`.
    pub fn last_close(&self, symbol: &str, date: NaiveDate) -> Option<f64> {
        self.upto(symbol, date).last().map(|b| b.close)
    }

    /// The bar of exactly `date` — for the engine as it steps onto `date`.
    pub(super) fn bar_on(&self, symbol: &str, date: NaiveDate) -> Option<DayBar> {
        let s = self.series.get(symbol)?;
        s.bars
            .binary_search_by_key(&date, |b| b.date)
            .ok()
            .map(|i| s.bars[i])
    }

    /// Dates `symbol` traded between `from` and `to`, inclusive: the
    /// replay's calendar when `symbol` is the benchmark.
    pub(super) fn dates(&self, symbol: &str, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        self.series
            .get(symbol)
            .map(|s| {
                s.bars
                    .iter()
                    .map(|b| b.date)
                    .filter(|d| *d >= from && *d <= to)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn load() -> anyhow::Result<Tape> {
        crate::store::load(FILE)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        crate::store::save(FILE, self, false)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Flat bars (open = high = low = close) on consecutive days.
    pub fn flat(start: &str, closes: &[f64]) -> Vec<DayBar> {
        let start = d(start);
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| DayBar {
                date: start + chrono::Duration::days(i as i64),
                open: c,
                high: c,
                low: c,
                close: c,
            })
            .collect()
    }

    #[test]
    fn upto_never_returns_a_later_bar() {
        let mut t = Tape::default();
        t.insert("A", None, flat("2020-01-01", &[1.0, 2.0, 3.0, 4.0]));
        assert_eq!(t.upto("A", d("2019-12-31")).len(), 0);
        assert_eq!(t.upto("A", d("2020-01-02")).len(), 2);
        assert_eq!(t.last_close("A", d("2020-01-02")), Some(2.0));
        assert_eq!(t.last_close("A", d("2030-01-01")), Some(4.0));
        assert!(t.upto("B", d("2020-01-02")).is_empty());
        assert_eq!(t.bar_on("A", d("2020-01-03")).unwrap().close, 3.0);
        assert!(t.bar_on("A", d("2020-01-09")).is_none());
        assert_eq!(t.dates("A", d("2020-01-02"), d("2020-01-03")).len(), 2);
    }

    #[test]
    fn insert_sorts_and_keeps_one_bar_per_day() {
        let mut t = Tape::default();
        let mut bars = flat("2020-01-01", &[1.0, 2.0]);
        bars.push(flat("2020-01-01", &[9.0])[0]);
        bars.reverse();
        t.insert("A", Some("INR".into()), bars);
        let all = t.upto("A", d("2020-12-31"));
        assert_eq!(all.len(), 2);
        assert!(all[0].date < all[1].date);
        assert_eq!(t.currency("A"), Some("INR"));
    }
}

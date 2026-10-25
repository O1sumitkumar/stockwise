//! Replay scenarios: a market, a date range, a universe of stocks with
//! sectors, and a benchmark. The built-ins are TOML files in
//! `src/practice/scenarios/`, embedded in the binary; `file:PATH.toml`
//! loads your own in the same format.

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Built-in scenarios, in the order `sim replay list` shows them.
const BUILT_IN: &[(&str, &str)] = &[
    (
        "covid-crash-2020",
        include_str!("scenarios/covid-crash-2020.toml"),
    ),
    ("gfc-2008", include_str!("scenarios/gfc-2008.toml")),
    ("adani-2023", include_str!("scenarios/adani-2023.toml")),
    ("dotcom-2000", include_str!("scenarios/dotcom-2000.toml")),
    (
        "bull-run-2021",
        include_str!("scenarios/bull-run-2021.toml"),
    ),
    (
        "rate-hikes-2022",
        include_str!("scenarios/rate-hikes-2022.toml"),
    ),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    /// Yahoo symbol, e.g. `RELIANCE.NS` or `AAPL`.
    pub symbol: String,
    pub sector: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub title: String,
    /// `in` or `us`.
    pub market: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Index the player is measured against, e.g. `^NSEI`.
    pub benchmark: String,
    pub benchmark_name: String,
    #[serde(default = "default_capital")]
    pub capital: f64,
    /// Shown before and during the replay: must not give away the future.
    pub description: String,
    /// Shown once the replay has ended.
    #[serde(default)]
    pub debrief: String,
    pub universe: Vec<Member>,
}

fn default_capital() -> f64 {
    100_000.0
}

impl Scenario {
    pub fn parse(text: &str) -> Result<Scenario> {
        let s: Scenario = toml::from_str(text).context("Not a valid scenario file")?;
        s.validate()?;
        Ok(s)
    }

    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("Scenario has no name");
        }
        if !matches!(self.market.as_str(), "in" | "us") {
            bail!(
                "Scenario {}: market must be \"in\" or \"us\", not {:?}",
                self.name,
                self.market
            );
        }
        if self.to <= self.from {
            bail!("Scenario {}: `to` must be after `from`", self.name);
        }
        if !(self.capital.is_finite() && self.capital > 0.0) {
            bail!("Scenario {}: capital must be positive", self.name);
        }
        if self.universe.is_empty() {
            bail!("Scenario {}: the universe is empty", self.name);
        }
        for (i, m) in self.universe.iter().enumerate() {
            if self.universe[..i].iter().any(|o| o.symbol == m.symbol) {
                bail!("Scenario {}: {} is listed twice", self.name, m.symbol);
            }
        }
        Ok(())
    }

    /// The sector of `symbol`, if it is in the universe.
    pub fn sector(&self, symbol: &str) -> Option<&str> {
        self.universe
            .iter()
            .find(|m| m.symbol == symbol)
            .map(|m| m.sector.as_str())
    }

    pub fn symbols(&self) -> Vec<String> {
        self.universe.iter().map(|m| m.symbol.clone()).collect()
    }
}

/// Every built-in scenario.
pub fn built_in() -> Vec<Scenario> {
    BUILT_IN
        .iter()
        .map(|(name, text)| {
            Scenario::parse(text).unwrap_or_else(|e| panic!("built-in scenario {name}: {e:#}"))
        })
        .collect()
}

/// A built-in scenario by name, or `file:PATH` / `PATH.toml`.
pub fn load(name: &str) -> Result<Scenario> {
    let path = name
        .strip_prefix("file:")
        .or_else(|| name.ends_with(".toml").then_some(name));
    if let Some(path) = path {
        let text = std::fs::read_to_string(path).with_context(|| format!("Cannot read {path}"))?;
        return Scenario::parse(&text).with_context(|| format!("In {path}"));
    }
    let key = name.trim().to_ascii_lowercase();
    match built_in().into_iter().find(|s| s.name == key) {
        Some(s) => Ok(s),
        None => bail!(
            "No scenario called {name:?}. Built-in: {}. Or use file:PATH.toml",
            BUILT_IN
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_parse_and_are_valid() {
        let all = built_in();
        assert_eq!(all.len(), BUILT_IN.len());
        for (s, (key, _)) in all.iter().zip(BUILT_IN) {
            assert_eq!(&s.name, key, "file name and `name` must match");
            assert!(s.universe.len() >= 10, "{}: small universe", s.name);
            assert!(!s.debrief.is_empty(), "{}: no debrief", s.name);
            assert!(s.benchmark.starts_with('^'), "{}: benchmark", s.name);
            for m in &s.universe {
                let suffix = m.symbol.ends_with(".NS");
                assert_eq!(suffix, s.market == "in", "{}: {}", s.name, m.symbol);
                assert!(!m.sector.is_empty());
            }
        }
        // Distinct sectors make the diversification badge reachable.
        for s in &all {
            let mut sectors: Vec<&str> = s.universe.iter().map(|m| m.sector.as_str()).collect();
            sectors.sort();
            sectors.dedup();
            assert!(sectors.len() >= 5, "{}: {} sectors", s.name, sectors.len());
        }
    }

    #[test]
    fn load_by_name_and_reject_unknown() {
        let s = load("COVID-crash-2020").unwrap();
        assert_eq!(s.benchmark, "^NSEI");
        assert_eq!(s.sector("TCS.NS"), Some("IT"));
        assert!(s.sector("AAPL").is_none());
        let e = load("moon-2030").unwrap_err().to_string();
        assert!(e.contains("covid-crash-2020"), "{e}");
    }

    #[test]
    fn invalid_files_are_refused() {
        let base = r#"
            name = "x"
            title = "X"
            market = "in"
            from = "2020-01-01"
            to = "2020-02-01"
            benchmark = "^NSEI"
            benchmark_name = "NIFTY 50"
            description = "d"
            [[universe]]
            symbol = "TCS.NS"
            sector = "IT"
        "#;
        assert!(Scenario::parse(base).is_ok());
        assert!(Scenario::parse(&base.replace("\"in\"", "\"jp\"")).is_err());
        assert!(Scenario::parse(&base.replace("2020-02-01", "2019-12-01")).is_err());
        let twice = format!("{base}\n[[universe]]\nsymbol = \"TCS.NS\"\nsector = \"IT\"\n");
        assert!(Scenario::parse(&twice).is_err());
        assert!(Scenario::parse(&format!("{base}\nsurprise = 1\n")).is_err());
    }
}

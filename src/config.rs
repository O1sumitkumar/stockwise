//! Optional config file for defaults: `config.toml` in the platform config
//! directory (`dirs::config_dir()/stockwise`), or the file named by
//! `STOCKWISE_CONFIG`.
//!
//! Precedence for every setting: command-line flag > environment variable >
//! config file > built-in default. `main` turns the file's `market`, `ai` and
//! `[periods]` values into clap defaults, so `--help` shows them and flags and
//! env vars still win. Other modules read the rest through [`setting`].

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Env var that points at another config file.
pub const ENV_VAR: &str = "STOCKWISE_CONFIG";

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Market for bare tickers: `in` or `us`.
    pub market: Option<String>,
    /// Add the local-AI section to analysis commands.
    pub ai: Option<bool>,
    /// Ollama model name.
    pub model: Option<String>,
    /// Ollama server (`host:port` or a URL).
    pub ollama_host: Option<String>,
    /// Redis cache URL, or `off`.
    pub redis_url: Option<String>,
    /// Data directory; a leading `~` means the home directory.
    pub home: Option<String>,
    /// Default `--period` per command, e.g. `technical = "6mo"`.
    #[serde(default)]
    pub periods: BTreeMap<String, String>,
}

static EMPTY: Config = Config {
    market: None,
    ai: None,
    model: None,
    ollama_host: None,
    redis_url: None,
    home: None,
    periods: BTreeMap::new(),
};

/// Where the config file was looked for and what came of it.
#[derive(Debug)]
pub struct State {
    pub path: Option<PathBuf>,
    pub config: Config,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// No file at the path: built-in defaults apply.
    Missing,
    Loaded,
    /// The file could not be read or is invalid. The message names the file
    /// and the offending key.
    Invalid(String),
}

/// Where an effective setting came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Flag,
    Env,
    File,
    Default,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Source::Flag => "flag",
            Source::Env => "env",
            Source::File => "config file",
            Source::Default => "default",
        })
    }
}

/// Where the global `--market` and `--ai` values came from (clap knows
/// whether a flag or env var was used; `main` fills this in).
#[derive(Debug, Clone, Copy)]
pub struct CliSources {
    pub market: Source,
    pub ai: Source,
}

/// Config file path: `STOCKWISE_CONFIG`, else `<config dir>/stockwise/config.toml`.
pub fn path() -> Option<PathBuf> {
    match std::env::var_os(ENV_VAR) {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
        _ => dirs::config_dir().map(|d| d.join("stockwise").join("config.toml")),
    }
}

/// Find and parse the config file. Never fails: problems are reported in
/// [`State::status`] so `doctor` and `config` can still run.
/// `period_commands` are the commands that take `--period`.
pub fn load(period_commands: &[&str]) -> State {
    let Some(path) = path() else {
        return State {
            path: None,
            config: Config::default(),
            status: Status::Missing,
        };
    };
    let explicit = std::env::var_os(ENV_VAR).is_some_and(|p| !p.is_empty());
    let (config, status) = match read(&path, period_commands) {
        Ok(Some(config)) => (config, Status::Loaded),
        Ok(None) if explicit => (
            Config::default(),
            Status::Invalid(format!(
                "{} points to {}, which does not exist (create it with `stockwise config init`)",
                ENV_VAR,
                path.display()
            )),
        ),
        Ok(None) => (Config::default(), Status::Missing),
        Err(e) => (Config::default(), Status::Invalid(format!("{:#}", e))),
    };
    State {
        path: Some(path),
        config,
        status,
    }
}

/// Read and validate the file at `path`; `None` if it does not exist.
pub fn read(path: &Path, period_commands: &[&str]) -> Result<Option<Config>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(e).with_context(|| format!("Cannot read config file {}", path.display()));
        }
    };
    parse(&text, period_commands)
        .map(Some)
        .with_context(|| format!("Invalid config file {}", path.display()))
}

/// Parse and validate config text.
pub fn parse(text: &str, period_commands: &[&str]) -> Result<Config> {
    let config: Config = toml::from_str(text)?;
    config.validate(period_commands)?;
    Ok(config)
}

impl Config {
    fn validate(&self, period_commands: &[&str]) -> Result<()> {
        if let Some(m) = &self.market {
            market_value(m)?;
        }
        for (key, value) in [
            ("model", &self.model),
            ("ollama_host", &self.ollama_host),
            ("redis_url", &self.redis_url),
            ("home", &self.home),
        ] {
            if value.as_deref().is_some_and(|v| v.trim().is_empty()) {
                bail!("`{}` is empty — remove the line to use the default", key);
            }
        }
        for (command, period) in &self.periods {
            if !period_commands.contains(&command.as_str()) {
                bail!(
                    "`periods.{}`: `{}` has no --period option. Commands with one: {}",
                    command,
                    command,
                    period_commands.join(", ")
                );
            }
            period_value(period).with_context(|| format!("`periods.{}`", command))?;
        }
        Ok(())
    }
}

/// The clap spelling (`in` / `us`) of a market name from the file.
pub fn market_value(s: &str) -> Result<&'static str> {
    match crate::market::Market::from_str(s, true) {
        Ok(crate::market::Market::In) => Ok("in"),
        Ok(crate::market::Market::Us) => Ok("us"),
        Err(_) => bail!("`market` = \"{}\" is not a market; use \"in\" or \"us\"", s),
    }
}

/// The matching entry of [`crate::PERIODS`], so it can be a clap default.
pub fn period_value(s: &str) -> Result<&'static str> {
    crate::PERIODS
        .iter()
        .copied()
        .find(|p| *p == s)
        .with_context(|| {
            format!(
                "\"{}\" is not a period; use one of {}",
                s,
                crate::PERIODS.join(", ")
            )
        })
}

static STATE: OnceLock<State> = OnceLock::new();

/// Make `state` the process-wide config. Later calls are ignored.
pub fn install(state: State) -> &'static State {
    let _ = STATE.set(state);
    STATE.get().expect("just set")
}

pub fn state() -> Option<&'static State> {
    STATE.get()
}

/// The loaded config, or an empty one (no file, invalid file, or tests).
pub fn get() -> &'static Config {
    STATE
        .get()
        .filter(|s| s.status == Status::Loaded)
        .map(|s| &s.config)
        .unwrap_or(&EMPTY)
}

/// Env var (if set and non-empty) over the config file's value.
pub fn resolve(env: Option<String>, file: Option<&str>) -> Option<(String, Source)> {
    match env {
        Some(v) if !v.trim().is_empty() => Some((v, Source::Env)),
        _ => file.map(|v| (v.to_string(), Source::File)),
    }
}

/// A setting from `env` or, failing that, the config file. `None` means
/// "use the built-in default".
pub fn setting(env: &str, pick: impl FnOnce(&Config) -> Option<&String>) -> Option<String> {
    resolve(std::env::var(env).ok(), pick(get()).map(String::as_str)).map(|(v, _)| v)
}

/// Where [`setting`] would take `env`'s value from.
pub fn setting_source(env: &str, pick: impl FnOnce(&Config) -> Option<&String>) -> Source {
    resolve(std::env::var(env).ok(), pick(get()).map(String::as_str))
        .map_or(Source::Default, |(_, s)| s)
}

/// Expand a leading `~` to the home directory.
pub fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') => {
            match dirs::home_dir() {
                Some(home) => home.join(rest.trim_start_matches(['/', '\\'])),
                None => PathBuf::from(p),
            }
        }
        _ => PathBuf::from(p),
    }
}

/// Commented template written by `stockwise config init`. Everything is
/// commented out, so a fresh file changes nothing until a line is edited.
pub const TEMPLATE: &str = r#"# StockWise configuration.
#
# Precedence: command-line flag > environment variable > this file > built-in default.
# Uncomment a line to change a default; `stockwise config show` prints the
# effective values and where each one comes from.

# Market for bare tickers: "in" (India, NSE — TCS means TCS.NS) or "us".
# Env: STOCKWISE_MARKET. Flag: -m / --market.
# market = "in"

# Add a local-AI (Ollama) section to analysis commands (slow: 30-60s per call).
# Env: STOCKWISE_AI. Flags: --ai / --no-ai.
# ai = false

# Ollama model and server. Env: STOCKWISE_MODEL, OLLAMA_HOST.
# model = "qwen3:14b"
# ollama_host = "http://localhost:11434"

# Redis cache server, or "off" for the in-memory cache only.
# Env: STOCKWISE_REDIS_URL.
# redis_url = "redis://127.0.0.1:6379/"

# Data directory for portfolio, watchlist, alerts and history ("~" = your home).
# Env: STOCKWISE_HOME. Default: shown by `stockwise doctor`.
# home = "~/stockwise"

# Default --period per command. Periods: 1d 5d 1w 1mo 3mo 6mo ytd 1y 2y 5y 10y max.
[periods]
# deep = "3mo"
# technical = "3mo"
# history = "1mo"
# risk = "1y"
# patterns = "1mo"
# support = "1mo"
# fibs = "1mo"
# volume = "1mo"
# gaps = "1mo"
# stoploss = "1mo"
# forecast = "1mo"
# options = "1mo"
# backtest = "2y"
# sip = "5y"
"#;

/// Write [`TEMPLATE`] to `path`, refusing to replace an existing file
/// unless `force`.
pub fn init(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists — edit it, or pass --force to replace it with the template",
            path.display()
        );
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Cannot create directory {}", dir.display()))?;
    }
    std::fs::write(path, TEMPLATE).with_context(|| format!("Cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMDS: &[&str] = &[
        "deep",
        "technical",
        "history",
        "risk",
        "patterns",
        "support",
        "fibs",
        "volume",
        "gaps",
        "stoploss",
        "forecast",
        "options",
        "backtest",
        "sip",
    ];

    fn err(text: &str) -> String {
        format!("{:#}", parse(text, CMDS).unwrap_err())
    }

    #[test]
    fn template_parses_to_defaults() {
        assert_eq!(parse(TEMPLATE, CMDS).unwrap(), Config::default());
    }

    #[test]
    fn template_lists_every_period_command() {
        for cmd in CMDS {
            assert!(
                TEMPLATE.contains(&format!("# {} = ", cmd)),
                "template is missing periods.{cmd}"
            );
        }
    }

    #[test]
    fn full_config_parses() {
        let c = parse(
            r#"
            market = "US"
            ai = true
            model = "llama3.1:8b"
            ollama_host = "gpu-box:11434"
            redis_url = "off"
            home = "~/sw"
            [periods]
            technical = "6mo"
            backtest = "5y"
            "#,
            CMDS,
        )
        .unwrap();
        assert_eq!(c.market.as_deref(), Some("US"));
        assert_eq!(market_value(c.market.as_deref().unwrap()).unwrap(), "us");
        assert_eq!(c.ai, Some(true));
        assert_eq!(c.periods["technical"], "6mo");
        assert_eq!(c.periods["backtest"], "5y");
    }

    #[test]
    fn unknown_key_is_named() {
        let e = err("markt = \"us\"");
        assert!(e.contains("markt"), "{e}");
    }

    #[test]
    fn wrong_type_shows_the_line() {
        let e = err("ai = \"yes\"");
        assert!(e.contains("ai = \"yes\""), "{e}");
    }

    #[test]
    fn bad_market_is_named() {
        let e = err("market = \"uk\"");
        assert!(e.contains("`market`") && e.contains("uk"), "{e}");
    }

    #[test]
    fn bad_period_value_names_the_key() {
        let e = err("[periods]\ntechnical = \"7x\"");
        assert!(e.contains("periods.technical") && e.contains("7x"), "{e}");
    }

    #[test]
    fn period_for_command_without_period_is_rejected() {
        let e = err("[periods]\nquote = \"1y\"");
        assert!(e.contains("periods.quote"), "{e}");
    }

    #[test]
    fn empty_string_is_rejected() {
        let e = err("model = \"\"");
        assert!(e.contains("`model`"), "{e}");
    }

    #[test]
    fn read_names_the_file() {
        let dir = std::env::temp_dir().join(format!("stockwise-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        assert!(
            read(&path, CMDS).unwrap().is_none(),
            "missing file is no config"
        );
        std::fs::write(&path, "market = 1").unwrap();
        let e = format!("{:#}", read(&path, CMDS).unwrap_err());
        assert!(e.contains("config.toml"), "{e}");
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() {
        let dir = std::env::temp_dir().join(format!("stockwise-init-{}", std::process::id()));
        let path = dir.join("nested").join("config.toml");
        let _ = std::fs::remove_file(&path);
        init(&path, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TEMPLATE);
        std::fs::write(&path, "market = \"us\"").unwrap();
        assert!(init(&path, false).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "market = \"us\"");
        init(&path, true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TEMPLATE);
    }

    #[test]
    fn env_beats_file_beats_default() {
        assert_eq!(
            resolve(Some("env".into()), Some("file")),
            Some(("env".into(), Source::Env))
        );
        assert_eq!(
            resolve(None, Some("file")),
            Some(("file".into(), Source::File))
        );
        assert_eq!(
            resolve(Some("  ".into()), Some("file")),
            Some(("file".into(), Source::File)),
            "an empty env var counts as unset"
        );
        assert_eq!(resolve(None, None), None);
    }

    #[test]
    fn home_expands_tilde_only_at_the_start() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expand_home("~"), home);
            assert_eq!(expand_home("~/sw"), home.join("sw"));
        }
        assert_eq!(expand_home("/data/sw"), PathBuf::from("/data/sw"));
        assert_eq!(expand_home("~bob/sw"), PathBuf::from("~bob/sw"));
    }
}

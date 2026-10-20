//! Strategy rules: the TOML strategy format behind `backtest -s` and the
//! intraday bot's `--strategy` signal.
//!
//! ```toml
//! name = "rsi"
//! description = "Buy oversold, sell overbought"
//! entry = "rsi($period) < $oversold"
//! exit = "rsi($period) > $overbought"
//! stop_loss_pct = 8          # optional
//! take_profit_pct = 15       # optional
//!
//! [params]                   # defaults, referenced as $name
//! period = 14
//! oversold = 30
//! overbought = 70
//!
//! [grid]                     # values `backtest walkforward` tries
//! oversold = [25, 30, 35]
//! ```
//!
//! `entry` and `exit` are a condition or a list of conditions that must all
//! hold. A condition compares two values with `<`, `<=`, `>`, `>=`,
//! `crosses_above` or `crosses_below`, and combines with `and`, `or`, `not`
//! and parentheses. Values are numbers, `$params`, price fields, the
//! indicators of `technical.rs`, and `+ - * /` between them. The six
//! built-in strategies are bundled from `examples/strategies/`.

use super::Bars;
use super::indicators::{self, Series};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Most parameter combinations a `[grid]` may expand to.
pub const MAX_GRID: usize = 500;

/// Built-in strategies: canonical name, aliases, definition.
const BUILTINS: &[(&str, &[&str], &str)] = &[
    (
        "rsi",
        &[],
        include_str!("../../examples/strategies/rsi.toml"),
    ),
    (
        "macd",
        &[],
        include_str!("../../examples/strategies/macd.toml"),
    ),
    (
        "sma",
        &["sma_crossover"],
        include_str!("../../examples/strategies/sma.toml"),
    ),
    (
        "bb",
        &["bollinger"],
        include_str!("../../examples/strategies/bb.toml"),
    ),
    (
        "vwap",
        &[],
        include_str!("../../examples/strategies/vwap.toml"),
    ),
    (
        "mr",
        &["mean_reversion"],
        include_str!("../../examples/strategies/mr.toml"),
    ),
];

/// Indicators a condition can use, with their arguments.
const INDICATORS: &[(&str, &str)] = &[
    ("close", ""),
    ("open", ""),
    ("high", ""),
    ("low", ""),
    ("volume", ""),
    ("sma", "(period)"),
    ("ema", "(period)"),
    ("rsi", "(period)"),
    ("atr", "(period)"),
    ("macd", "[(fast, slow, signal)]"),
    ("macd_signal", "[(fast, slow, signal)]"),
    ("macd_hist", "[(fast, slow, signal)]"),
    ("bb_upper", "(period[, k])"),
    ("bb_middle", "(period)"),
    ("bb_lower", "(period[, k])"),
    ("vwap", ""),
    ("highest", "(period)"),
    ("lowest", "(period)"),
];

/// Comma-separated indicator list for error messages and docs.
pub fn indicator_help() -> String {
    INDICATORS
        .iter()
        .map(|(n, a)| format!("{n}{a}"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ── File format ──

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrategyFile {
    name: Option<String>,
    description: Option<String>,
    entry: Conditions,
    exit: Option<Conditions>,
    stop_loss_pct: Option<f64>,
    take_profit_pct: Option<f64>,
    #[serde(default)]
    params: BTreeMap<String, f64>,
    #[serde(default)]
    grid: BTreeMap<String, Vec<f64>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Conditions {
    One(String),
    All(Vec<String>),
}

impl Conditions {
    fn into_vec(self) -> Vec<String> {
        match self {
            Conditions::One(s) => vec![s],
            Conditions::All(v) => v,
        }
    }
}

/// A validated strategy definition, before parameters are bound.
#[derive(Debug, Clone, Serialize)]
pub struct StrategyDef {
    pub name: String,
    pub description: String,
    /// "built-in" or the file it was loaded from.
    pub source: String,
    pub entry: Vec<String>,
    pub exit: Vec<String>,
    pub stop_loss_pct: Option<f64>,
    pub take_profit_pct: Option<f64>,
    pub params: BTreeMap<String, f64>,
    pub grid: BTreeMap<String, Vec<f64>>,
    #[serde(skip)]
    entry_ast: Vec<BoolExpr>,
    #[serde(skip)]
    exit_ast: Vec<BoolExpr>,
}

impl StrategyDef {
    /// Parse and fully validate a strategy file: syntax, parameters, the grid,
    /// and every indicator for the defaults and each grid combination.
    pub fn parse(text: &str, default_name: &str, source: &str) -> Result<StrategyDef> {
        let file: StrategyFile = toml::from_str(text).map_err(|e| anyhow!("{source}: {e}"))?;
        let name = file
            .name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| default_name.to_string());
        let entry = file.entry.into_vec();
        let exit = file.exit.map(Conditions::into_vec).unwrap_or_default();
        if entry.is_empty() {
            bail!("{source}: `entry` needs at least one condition");
        }
        if let Some(sl) = file.stop_loss_pct
            && !(sl > 0.0 && sl < 100.0)
        {
            bail!("{source}: `stop_loss_pct` must be between 0 and 100, got {sl}");
        }
        if let Some(tp) = file.take_profit_pct
            && !(tp > 0.0 && tp.is_finite())
        {
            bail!("{source}: `take_profit_pct` must be above 0, got {tp}");
        }

        let parse_all = |label: &str, conds: &[String]| -> Result<Vec<BoolExpr>> {
            conds
                .iter()
                .map(|c| parse_condition(c).with_context(|| format!("{source}: {label} \"{c}\"")))
                .collect()
        };
        let entry_ast = parse_all("entry", &entry)?;
        let exit_ast = parse_all("exit", &exit)?;
        if exit.is_empty() && file.stop_loss_pct.is_none() && file.take_profit_pct.is_none() {
            bail!(
                "{source}: add an `exit` condition, `stop_loss_pct` or `take_profit_pct` — \
                 otherwise a position never closes"
            );
        }

        for (k, v) in &file.params {
            if !is_identifier(k) {
                bail!("{source}: parameter name `{k}` must be letters, digits and _");
            }
            if !v.is_finite() {
                bail!("{source}: parameter `{k}` must be a number");
            }
        }
        let mut used = Vec::new();
        for e in entry_ast.iter().chain(&exit_ast) {
            e.params(&mut used);
        }
        for p in &used {
            if !file.params.contains_key(p) {
                bail!("{source}: `${p}` is used in a condition but not defined under [params]");
            }
        }
        let mut combos = 1usize;
        for (k, values) in &file.grid {
            if !file.params.contains_key(k) {
                bail!("{source}: [grid] `{k}` is not a parameter — add a default under [params]");
            }
            if values.is_empty() {
                bail!("{source}: [grid] `{k}` needs at least one value");
            }
            combos = combos.saturating_mul(values.len());
        }
        if combos > MAX_GRID {
            bail!("{source}: [grid] expands to {combos} combinations; keep it under {MAX_GRID}");
        }

        let def = StrategyDef {
            name,
            description: file.description.unwrap_or_default(),
            source: source.to_string(),
            entry,
            exit,
            stop_loss_pct: file.stop_loss_pct,
            take_profit_pct: file.take_profit_pct,
            params: file.params,
            grid: file.grid,
            entry_ast,
            exit_ast,
        };
        def.compile()?;
        for combo in def.grid_combos() {
            def.compile_with(&combo)
                .with_context(|| format!("[grid] combination {}", format_params(&combo)))?;
        }
        Ok(def)
    }

    /// The same strategy with some default parameters replaced. A replaced
    /// parameter is also pinned in the grid, so walk-forward keeps it fixed.
    pub fn with_params(&self, overrides: &[(String, f64)]) -> Result<StrategyDef> {
        let mut def = self.clone();
        for (k, v) in overrides {
            if def.params.is_empty() {
                bail!("strategy `{}` has no parameters to set", def.name);
            }
            if !def.params.contains_key(k) {
                bail!(
                    "strategy `{}` has no parameter `{k}` (it has: {})",
                    def.name,
                    def.params.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            def.params.insert(k.clone(), *v);
            if let Some(values) = def.grid.get_mut(k) {
                *values = vec![*v];
            }
        }
        def.compile()?;
        Ok(def)
    }

    /// Every parameter set in the grid (the defaults when there is no grid).
    pub fn grid_combos(&self) -> Vec<BTreeMap<String, f64>> {
        let mut combos = vec![self.params.clone()];
        for (k, values) in &self.grid {
            combos = combos
                .into_iter()
                .flat_map(|c| {
                    values.iter().map(move |v| {
                        let mut c = c.clone();
                        c.insert(k.clone(), *v);
                        c
                    })
                })
                .collect();
        }
        combos
    }

    pub fn compile(&self) -> Result<Strategy> {
        self.compile_with(&self.params)
    }

    /// Bind `params` into the conditions.
    pub fn compile_with(&self, params: &BTreeMap<String, f64>) -> Result<Strategy> {
        let mut inds = Vec::new();
        let mut bind_all = |label: &str, src: &[String], asts: &[BoolExpr]| -> Result<Vec<Cond>> {
            src.iter()
                .zip(asts)
                .map(|(s, e)| {
                    bind_cond(e, params, &mut inds)
                        .with_context(|| format!("{}: {label} \"{s}\"", self.source))
                })
                .collect()
        };
        let entry = bind_all("entry", &self.entry, &self.entry_ast)?;
        let exit = bind_all("exit", &self.exit, &self.exit_ast)?;
        Ok(Strategy {
            name: self.name.clone(),
            params: params.clone(),
            stop_loss_pct: self.stop_loss_pct,
            take_profit_pct: self.take_profit_pct,
            inds,
            entry,
            exit,
        })
    }
}

/// A condition with its `$params` replaced by their values, for display:
/// `rsi($period) < $oversold` → `rsi(14) < 30`.
pub fn render(cond: &str, params: &BTreeMap<String, f64>) -> String {
    let mut out = String::new();
    let mut rest = cond;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        match params.get(&tail[..end]) {
            Some(v) => out.push_str(&v.to_string()),
            None => out.push_str(&rest[i..i + 1 + end]),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// `period=14 oversold=30`
pub fn format_params(params: &BTreeMap<String, f64>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a `-P name=value` override.
pub fn parse_param_override(s: &str) -> Result<(String, f64), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("`{s}` should look like name=value, e.g. oversold=25"))?;
    let v: f64 = v
        .trim()
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite())
        .ok_or_else(|| format!("`{}` is not a number", v.trim()))?;
    Ok((k.trim().to_string(), v))
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ── Loading ──

/// Where `-s NAME` looks for your own strategies: `$STOCKWISE_HOME/strategies`
/// when STOCKWISE_HOME is set, else `$XDG_CONFIG_HOME/stockwise/strategies`,
/// else `~/.config/stockwise/strategies`.
pub fn strategies_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    if let Some(home) = env("STOCKWISE_HOME") {
        return Some(PathBuf::from(home).join("strategies"));
    }
    if let Some(xdg) = env("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("stockwise").join("strategies"));
    }
    Some(
        dirs::home_dir()?
            .join(".config")
            .join("stockwise")
            .join("strategies"),
    )
}

/// Canonical names of the built-in strategies.
pub fn builtin_names() -> Vec<&'static str> {
    BUILTINS.iter().map(|(n, ..)| *n).collect()
}

/// Built-in strategies with their aliases.
pub fn builtins() -> Vec<(StrategyDef, &'static [&'static str])> {
    BUILTINS
        .iter()
        .map(|(name, aliases, text)| {
            let def = StrategyDef::parse(text, name, "built-in")
                .expect("bundled strategies are tested to parse");
            (def, *aliases)
        })
        .collect()
}

fn builtin(spec: &str) -> Option<(&'static str, &'static str)> {
    let spec = spec.to_ascii_lowercase();
    BUILTINS
        .iter()
        .find(|(n, aliases, _)| *n == spec || aliases.contains(&spec.as_str()))
        .map(|(n, _, text)| (*n, *text))
}

/// Resolve `-s`: a built-in name, `file:PATH`, a path to a `.toml` file, or
/// the name of a file in [`strategies_dir`].
pub fn load(spec: &str) -> Result<StrategyDef> {
    let spec = spec.trim();
    if let Some((name, text)) = builtin(spec) {
        return StrategyDef::parse(text, name, "built-in");
    }
    if let Some(path) = spec.strip_prefix("file:") {
        return load_file(Path::new(path));
    }
    if spec.ends_with(".toml") || spec.contains('/') || spec.contains('\\') {
        return load_file(Path::new(spec));
    }
    let dir = strategies_dir();
    if let Some(dir) = &dir {
        let path = dir.join(format!("{spec}.toml"));
        if path.is_file() {
            return load_file(&path);
        }
    }
    let mut msg = format!(
        "Unknown strategy '{spec}'. Built-in: {}",
        builtin_names().join(", ")
    );
    if let Some(dir) = &dir {
        let mine: Vec<String> = user_strategy_files(dir)
            .iter()
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        if mine.is_empty() {
            msg.push_str(&format!(". No strategy files in {}", dir.display()));
        } else {
            msg.push_str(&format!(". Yours ({}): {}", dir.display(), mine.join(", ")));
        }
    }
    msg.push_str(". Or pass -s file:path/to/strategy.toml");
    bail!(msg)
}

pub fn load_file(path: &Path) -> Result<StrategyDef> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read strategy file {}", path.display()))?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "custom".into());
    StrategyDef::parse(&text, &stem, &path.display().to_string())
}

/// `*.toml` files in `dir`, sorted. A missing directory has none.
pub fn user_strategy_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "toml") && p.is_file())
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// Whether a file stem is shadowed by a built-in name or alias.
pub fn is_builtin_name(name: &str) -> bool {
    builtin(name).is_some()
}

// ── Compiled strategy ──

/// Entry and exit signals, one per bar, evaluated at the bar's close.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Signals {
    pub entry: Vec<bool>,
    pub exit: Vec<bool>,
}

impl Signals {
    pub fn slice(&self, range: std::ops::Range<usize>) -> Signals {
        Signals {
            entry: self.entry[range.clone()].to_vec(),
            exit: self.exit[range].to_vec(),
        }
    }
}

/// A strategy with its parameters bound, ready to evaluate.
#[derive(Debug, Clone)]
pub struct Strategy {
    pub name: String,
    pub params: BTreeMap<String, f64>,
    pub stop_loss_pct: Option<f64>,
    pub take_profit_pct: Option<f64>,
    inds: Vec<Ind>,
    entry: Vec<Cond>,
    exit: Vec<Cond>,
}

impl Strategy {
    /// Evaluate entry and exit at every bar. A condition is false while any
    /// indicator it uses is still warming up.
    pub fn signals(&self, bars: &Bars) -> Signals {
        let data: Vec<Series> = self.inds.iter().map(|i| i.compute(bars)).collect();
        let all = |conds: &[Cond], i: usize| {
            !conds.is_empty() && conds.iter().all(|c| c.eval(i, &data) == Some(true))
        };
        let n = bars.len();
        Signals {
            entry: (0..n).map(|i| all(&self.entry, i)).collect(),
            exit: (0..n).map(|i| all(&self.exit, i)).collect(),
        }
    }

    /// Bars of history the slowest indicator needs before its first value.
    pub fn lookback(&self) -> usize {
        self.inds.iter().map(Ind::lookback).max().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Ind {
    Open,
    High,
    Low,
    Close,
    Volume,
    Sma(usize),
    Ema(usize),
    Rsi(usize),
    Atr(usize),
    Macd(usize, usize, usize),
    MacdSignal(usize, usize, usize),
    MacdHist(usize, usize, usize),
    BbUpper(usize, f64),
    BbMiddle(usize),
    BbLower(usize, f64),
    Vwap,
    Highest(usize),
    Lowest(usize),
}

impl Ind {
    fn compute(&self, b: &Bars) -> Series {
        let wrap = |v: &[f64]| v.iter().map(|x| Some(*x)).collect();
        match *self {
            Ind::Open => wrap(&b.open),
            Ind::High => wrap(&b.high),
            Ind::Low => wrap(&b.low),
            Ind::Close => wrap(&b.close),
            Ind::Volume => b.volume.iter().map(|v| Some(*v as f64)).collect(),
            Ind::Sma(p) => indicators::sma(&b.close, p),
            Ind::Ema(p) => indicators::ema(&b.close, p),
            Ind::Rsi(p) => indicators::rsi(&b.close, p),
            Ind::Atr(p) => indicators::atr(&b.high, &b.low, &b.close, p),
            Ind::Macd(f, s, g) => indicators::macd(&b.close, f, s, g).0,
            Ind::MacdSignal(f, s, g) => indicators::macd(&b.close, f, s, g).1,
            Ind::MacdHist(f, s, g) => indicators::macd(&b.close, f, s, g).2,
            Ind::BbUpper(p, k) => indicators::bollinger(&b.close, p, k).0,
            Ind::BbMiddle(p) => indicators::bollinger(&b.close, p, 2.0).1,
            Ind::BbLower(p, k) => indicators::bollinger(&b.close, p, k).2,
            Ind::Vwap => indicators::vwap(&b.high, &b.low, &b.close, &b.volume),
            Ind::Highest(p) => indicators::highest(&b.high, p),
            Ind::Lowest(p) => indicators::lowest(&b.low, p),
        }
    }

    fn lookback(&self) -> usize {
        match *self {
            Ind::Sma(p) | Ind::Ema(p) | Ind::BbUpper(p, _) | Ind::BbMiddle(p) => p,
            Ind::BbLower(p, _) => p,
            Ind::Rsi(p) | Ind::Atr(p) | Ind::Highest(p) | Ind::Lowest(p) => p + 1,
            Ind::Macd(f, s, _) => f.max(s),
            Ind::MacdSignal(f, s, g) | Ind::MacdHist(f, s, g) => f.max(s) + g,
            _ => 1,
        }
    }

    fn make(name: &str, args: &[f64]) -> Result<Ind> {
        let period = |v: f64| -> Result<usize> {
            if v.fract() != 0.0 || !(1.0..=1000.0).contains(&v) {
                bail!("`{name}` period must be a whole number from 1 to 1000, got {v}");
            }
            Ok(v as usize)
        };
        let k = |v: f64| -> Result<f64> {
            if !(v > 0.0 && v <= 10.0) {
                bail!("`{name}` band width k must be between 0 and 10, got {v}");
            }
            Ok(v)
        };
        let macd = |a: &[f64]| -> Result<(usize, usize, usize)> {
            match a {
                [] => Ok((12, 26, 9)),
                [f, s, g] => Ok((period(*f)?, period(*s)?, period(*g)?)),
                _ => bail!("`{name}` takes no arguments or (fast, slow, signal)"),
            }
        };
        Ok(match (name, args) {
            ("close", []) => Ind::Close,
            ("open", []) => Ind::Open,
            ("high", []) => Ind::High,
            ("low", []) => Ind::Low,
            ("volume", []) => Ind::Volume,
            ("vwap", []) => Ind::Vwap,
            ("sma", [p]) => Ind::Sma(period(*p)?),
            ("ema", [p]) => Ind::Ema(period(*p)?),
            ("rsi", [p]) => Ind::Rsi(period(*p)?),
            ("atr", [p]) => Ind::Atr(period(*p)?),
            ("highest", [p]) => Ind::Highest(period(*p)?),
            ("lowest", [p]) => Ind::Lowest(period(*p)?),
            ("macd", a) => {
                let (f, s, g) = macd(a)?;
                Ind::Macd(f, s, g)
            }
            ("macd_signal", a) => {
                let (f, s, g) = macd(a)?;
                Ind::MacdSignal(f, s, g)
            }
            ("macd_hist", a) => {
                let (f, s, g) = macd(a)?;
                Ind::MacdHist(f, s, g)
            }
            ("bb_middle", [p]) => Ind::BbMiddle(period(*p)?),
            ("bb_upper", [p]) => Ind::BbUpper(period(*p)?, 2.0),
            ("bb_upper", [p, w]) => Ind::BbUpper(period(*p)?, k(*w)?),
            ("bb_lower", [p]) => Ind::BbLower(period(*p)?, 2.0),
            ("bb_lower", [p, w]) => Ind::BbLower(period(*p)?, k(*w)?),
            _ => match INDICATORS.iter().find(|(n, _)| *n == name) {
                Some((n, "")) => bail!("`{n}` takes no arguments"),
                Some((n, usage)) => bail!("`{n}` is used as {n}{usage}"),
                None => bail!(
                    "unknown indicator `{name}`. Available: {}",
                    indicator_help()
                ),
            },
        })
    }
}

// ── Expressions ──

#[derive(Debug, Clone, Copy, PartialEq)]
enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    CrossAbove,
    CrossBelow,
}

/// A value expression as written, before parameters are bound.
#[derive(Debug, Clone)]
enum Expr {
    Num(f64),
    Param(String),
    Call(String, Vec<Expr>),
    Neg(Box<Expr>),
    Bin(ArithOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
enum BoolExpr {
    Cmp(CmpOp, Expr, Expr),
    And(Box<BoolExpr>, Box<BoolExpr>),
    Or(Box<BoolExpr>, Box<BoolExpr>),
    Not(Box<BoolExpr>),
}

impl Expr {
    fn params(&self, out: &mut Vec<String>) {
        match self {
            Expr::Num(_) => {}
            Expr::Param(p) => out.push(p.clone()),
            Expr::Call(_, args) => args.iter().for_each(|a| a.params(out)),
            Expr::Neg(e) => e.params(out),
            Expr::Bin(_, a, b) => {
                a.params(out);
                b.params(out);
            }
        }
    }
}

impl BoolExpr {
    fn params(&self, out: &mut Vec<String>) {
        match self {
            BoolExpr::Cmp(_, a, b) => {
                a.params(out);
                b.params(out);
            }
            BoolExpr::And(a, b) | BoolExpr::Or(a, b) => {
                a.params(out);
                b.params(out);
            }
            BoolExpr::Not(e) => e.params(out),
        }
    }
}

/// A value with parameters bound and indicators resolved to slots.
#[derive(Debug, Clone)]
enum Value {
    Const(f64),
    Ind(usize),
    Neg(Box<Value>),
    Bin(ArithOp, Box<Value>, Box<Value>),
}

#[derive(Debug, Clone)]
enum Cond {
    Cmp(CmpOp, Value, Value),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
}

fn arith(op: ArithOp, a: f64, b: f64) -> f64 {
    match op {
        ArithOp::Add => a + b,
        ArithOp::Sub => a - b,
        ArithOp::Mul => a * b,
        ArithOp::Div => a / b,
    }
}

fn const_eval(e: &Expr, params: &BTreeMap<String, f64>) -> Result<f64> {
    Ok(match e {
        Expr::Num(v) => *v,
        Expr::Param(p) => *params
            .get(p)
            .ok_or_else(|| anyhow!("`${p}` is not defined under [params]"))?,
        Expr::Neg(e) => -const_eval(e, params)?,
        Expr::Bin(op, a, b) => arith(*op, const_eval(a, params)?, const_eval(b, params)?),
        Expr::Call(name, _) => {
            bail!("indicator arguments must be numbers or $parameters, not `{name}`")
        }
    })
}

fn bind_value(e: &Expr, params: &BTreeMap<String, f64>, inds: &mut Vec<Ind>) -> Result<Value> {
    Ok(match e {
        Expr::Num(_) | Expr::Param(_) => Value::Const(const_eval(e, params)?),
        Expr::Neg(e) => match bind_value(e, params, inds)? {
            Value::Const(v) => Value::Const(-v),
            v => Value::Neg(Box::new(v)),
        },
        Expr::Bin(op, a, b) => match (bind_value(a, params, inds)?, bind_value(b, params, inds)?) {
            (Value::Const(x), Value::Const(y)) => Value::Const(arith(*op, x, y)),
            (x, y) => Value::Bin(*op, Box::new(x), Box::new(y)),
        },
        Expr::Call(name, args) => {
            let args = args
                .iter()
                .map(|a| const_eval(a, params))
                .collect::<Result<Vec<_>>>()?;
            let ind = Ind::make(name, &args)?;
            let slot = match inds.iter().position(|i| *i == ind) {
                Some(slot) => slot,
                None => {
                    inds.push(ind);
                    inds.len() - 1
                }
            };
            Value::Ind(slot)
        }
    })
}

fn bind_cond(e: &BoolExpr, params: &BTreeMap<String, f64>, inds: &mut Vec<Ind>) -> Result<Cond> {
    Ok(match e {
        BoolExpr::Cmp(op, a, b) => {
            let (a, b) = (bind_value(a, params, inds)?, bind_value(b, params, inds)?);
            if let (Value::Const(_), Value::Const(_)) = (&a, &b) {
                bail!("compares two constants — it is always true or always false");
            }
            Cond::Cmp(*op, a, b)
        }
        BoolExpr::And(a, b) => Cond::And(
            Box::new(bind_cond(a, params, inds)?),
            Box::new(bind_cond(b, params, inds)?),
        ),
        BoolExpr::Or(a, b) => Cond::Or(
            Box::new(bind_cond(a, params, inds)?),
            Box::new(bind_cond(b, params, inds)?),
        ),
        BoolExpr::Not(a) => Cond::Not(Box::new(bind_cond(a, params, inds)?)),
    })
}

impl Value {
    fn at(&self, i: usize, data: &[Series]) -> Option<f64> {
        let v = match self {
            Value::Const(c) => *c,
            Value::Ind(slot) => data[*slot].get(i).copied().flatten()?,
            Value::Neg(v) => -v.at(i, data)?,
            Value::Bin(op, a, b) => arith(*op, a.at(i, data)?, b.at(i, data)?),
        };
        v.is_finite().then_some(v)
    }
}

impl Cond {
    /// Three-valued: `None` while an indicator has no value yet.
    fn eval(&self, i: usize, data: &[Series]) -> Option<bool> {
        match self {
            Cond::Cmp(op, a, b) => {
                let (x, y) = (a.at(i, data)?, b.at(i, data)?);
                Some(match op {
                    CmpOp::Lt => x < y,
                    CmpOp::Le => x <= y,
                    CmpOp::Gt => x > y,
                    CmpOp::Ge => x >= y,
                    CmpOp::CrossAbove | CmpOp::CrossBelow => {
                        let j = i.checked_sub(1)?;
                        let (px, py) = (a.at(j, data)?, b.at(j, data)?);
                        if *op == CmpOp::CrossAbove {
                            x > y && px <= py
                        } else {
                            x < y && px >= py
                        }
                    }
                })
            }
            Cond::And(a, b) => match (a.eval(i, data), b.eval(i, data)) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            Cond::Or(a, b) => match (a.eval(i, data), b.eval(i, data)) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
            Cond::Not(a) => a.eval(i, data).map(|v| !v),
        }
    }
}

// ── Parser ──

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Param(String),
    LParen,
    RParen,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Cmp(CmpOp),
    And,
    Or,
    Not,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Num(v) => write!(f, "`{v}`"),
            Tok::Ident(s) => write!(f, "`{s}`"),
            Tok::Param(s) => write!(f, "`${s}`"),
            Tok::LParen => write!(f, "`(`"),
            Tok::RParen => write!(f, "`)`"),
            Tok::Comma => write!(f, "`,`"),
            Tok::Plus => write!(f, "`+`"),
            Tok::Minus => write!(f, "`-`"),
            Tok::Star => write!(f, "`*`"),
            Tok::Slash => write!(f, "`/`"),
            Tok::Cmp(op) => write!(
                f,
                "`{}`",
                match op {
                    CmpOp::Lt => "<",
                    CmpOp::Le => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::Ge => ">=",
                    CmpOp::CrossAbove => "crosses_above",
                    CmpOp::CrossBelow => "crosses_below",
                }
            ),
            Tok::And => write!(f, "`and`"),
            Tok::Or => write!(f, "`or`"),
            Tok::Not => write!(f, "`not`"),
        }
    }
}

fn tokenize(s: &str) -> Result<Vec<Tok>> {
    let chars: Vec<char> = s.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let word = |i: &mut usize| {
        let start = *i;
        while *i < chars.len() && (chars[*i].is_ascii_alphanumeric() || chars[*i] == '_') {
            *i += 1;
        }
        chars[start..*i].iter().collect::<String>()
    };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            c if c.is_whitespace() => i += 1,
            c if c.is_ascii_digit() || (c == '.' && next.is_some_and(|n| n.is_ascii_digit())) => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let v = text
                    .parse()
                    .map_err(|_| anyhow!("`{text}` is not a number"))?;
                toks.push(Tok::Num(v));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let w = word(&mut i).to_ascii_lowercase();
                let after_crosses = toks.last() == Some(&Tok::Ident("crosses".into()));
                let tok = match w.as_str() {
                    "and" => Tok::And,
                    "or" => Tok::Or,
                    "not" => Tok::Not,
                    "crosses_above" => Tok::Cmp(CmpOp::CrossAbove),
                    "crosses_below" => Tok::Cmp(CmpOp::CrossBelow),
                    "above" if after_crosses => Tok::Cmp(CmpOp::CrossAbove),
                    "below" if after_crosses => Tok::Cmp(CmpOp::CrossBelow),
                    _ => Tok::Ident(w),
                };
                if matches!(tok, Tok::Cmp(_)) && after_crosses {
                    toks.pop();
                }
                toks.push(tok);
            }
            '$' => {
                i += 1;
                let w = word(&mut i);
                if w.is_empty() {
                    bail!("`$` must be followed by a parameter name, e.g. $period");
                }
                toks.push(Tok::Param(w));
            }
            '(' | ')' | ',' | '+' | '-' | '*' | '/' => {
                toks.push(match c {
                    '(' => Tok::LParen,
                    ')' => Tok::RParen,
                    ',' => Tok::Comma,
                    '+' => Tok::Plus,
                    '-' => Tok::Minus,
                    '*' => Tok::Star,
                    _ => Tok::Slash,
                });
                i += 1;
            }
            '<' | '>' => {
                let eq = next == Some('=');
                toks.push(Tok::Cmp(match (c, eq) {
                    ('<', true) => CmpOp::Le,
                    ('<', false) => CmpOp::Lt,
                    ('>', true) => CmpOp::Ge,
                    _ => CmpOp::Gt,
                }));
                i += if eq { 2 } else { 1 };
            }
            '&' if next == Some('&') => {
                toks.push(Tok::And);
                i += 2;
            }
            '|' if next == Some('|') => {
                toks.push(Tok::Or);
                i += 2;
            }
            '=' | '!' => bail!(
                "`{c}` is not supported — compare with <, <=, >, >=, crosses_above or \
                 crosses_below, and negate with `not`"
            ),
            c => bail!("unexpected character `{c}`"),
        }
    }
    if toks.contains(&Tok::Ident("crosses".into())) {
        bail!("write `crosses_above` or `crosses_below` (or `crosses above`)");
    }
    Ok(toks)
}

fn parse_condition(s: &str) -> Result<BoolExpr> {
    let toks = tokenize(s)?;
    if toks.is_empty() {
        bail!("empty condition");
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.or()?;
    if let Some(t) = p.peek() {
        bail!("unexpected {t} after a complete condition");
    }
    Ok(e)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn found(&self) -> String {
        self.peek()
            .map_or("the end of the condition".into(), |t| t.to_string())
    }

    fn or(&mut self) -> Result<BoolExpr> {
        let mut l = self.and()?;
        while self.eat(&Tok::Or) {
            l = BoolExpr::Or(Box::new(l), Box::new(self.and()?));
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<BoolExpr> {
        let mut l = self.not()?;
        while self.eat(&Tok::And) {
            l = BoolExpr::And(Box::new(l), Box::new(self.not()?));
        }
        Ok(l)
    }

    fn not(&mut self) -> Result<BoolExpr> {
        if self.eat(&Tok::Not) {
            return Ok(BoolExpr::Not(Box::new(self.not()?)));
        }
        // `(` opens either a grouped condition or a grouped value; try the
        // condition first and fall back to a comparison.
        if self.peek() == Some(&Tok::LParen) {
            let save = self.pos;
            self.pos += 1;
            if let Ok(e) = self.or()
                && self.eat(&Tok::RParen)
                && !matches!(
                    self.peek(),
                    Some(Tok::Cmp(_) | Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash)
                )
            {
                return Ok(e);
            }
            self.pos = save;
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<BoolExpr> {
        let l = self.sum()?;
        let op = match self.peek() {
            Some(Tok::Cmp(op)) => *op,
            _ => bail!(
                "expected <, <=, >, >=, crosses_above or crosses_below, found {}",
                self.found()
            ),
        };
        self.pos += 1;
        let r = self.sum()?;
        Ok(BoolExpr::Cmp(op, l, r))
    }

    fn sum(&mut self) -> Result<Expr> {
        let mut l = self.product()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => ArithOp::Add,
                Some(Tok::Minus) => ArithOp::Sub,
                _ => return Ok(l),
            };
            self.pos += 1;
            l = Expr::Bin(op, Box::new(l), Box::new(self.product()?));
        }
    }

    fn product(&mut self) -> Result<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => ArithOp::Mul,
                Some(Tok::Slash) => ArithOp::Div,
                _ => return Ok(l),
            };
            self.pos += 1;
            l = Expr::Bin(op, Box::new(l), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr> {
        if self.eat(&Tok::Minus) {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat(&Tok::Plus) {
            return self.unary();
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr> {
        let tok = self.peek().cloned();
        match tok {
            Some(Tok::Num(v)) => {
                self.pos += 1;
                Ok(Expr::Num(v))
            }
            Some(Tok::Param(p)) => {
                self.pos += 1;
                Ok(Expr::Param(p))
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                let mut args = Vec::new();
                if self.eat(&Tok::LParen) && !self.eat(&Tok::RParen) {
                    loop {
                        args.push(self.sum()?);
                        if self.eat(&Tok::RParen) {
                            break;
                        }
                        if !self.eat(&Tok::Comma) {
                            bail!("expected `,` or `)` in {name}(...), found {}", self.found());
                        }
                    }
                }
                Ok(Expr::Call(name, args))
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                let e = self.sum()?;
                if !self.eat(&Tok::RParen) {
                    bail!("expected `)`, found {}", self.found());
                }
                Ok(e)
            }
            _ => bail!(
                "expected a number, $parameter or indicator, found {}",
                self.found()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(toml: &str) -> Result<StrategyDef> {
        StrategyDef::parse(toml, "test", "test.toml")
    }

    fn err(toml: &str) -> String {
        format!("{:#}", def(toml).unwrap_err())
    }

    fn bars(closes: &[f64]) -> Bars {
        Bars::from_closes(closes)
    }

    #[test]
    fn every_bundled_strategy_parses() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/strategies");
        let files = user_strategy_files(&dir);
        assert!(files.len() >= BUILTINS.len());
        for f in files {
            let d = load_file(&f).unwrap_or_else(|e| panic!("{}: {e:#}", f.display()));
            assert!(!d.description.is_empty(), "{} needs a description", d.name);
        }
        for (name, ..) in BUILTINS {
            let d = load(name).unwrap();
            assert_eq!(d.name, *name);
            assert_eq!(d.source, "built-in");
        }
    }

    #[test]
    fn aliases_and_case_resolve_to_builtins() {
        assert_eq!(load("bollinger").unwrap().name, "bb");
        assert_eq!(load("MEAN_REVERSION").unwrap().name, "mr");
        assert_eq!(load("sma_crossover").unwrap().name, "sma");
        assert!(format!("{:#}", load("nope").unwrap_err()).contains("Built-in: rsi, macd"));
    }

    #[test]
    fn parses_operators_and_precedence() {
        let d = def(r#"
            entry = "close > sma(20) and (rsi(14) < 30 or not macd_hist > 0)"
            exit = ["(close - sma(20)) / sma(20) > 0.05", "close crosses below ema(10)"]
        "#)
        .unwrap();
        assert_eq!(d.entry.len(), 1);
        assert_eq!(d.exit.len(), 2);
        // Keywords and indicators are case-insensitive; && and || work too.
        def(r#"entry = "CLOSE > SMA(5) && (RSI(14) < 30 || close < 1)"
               exit = "close crosses_above bb_upper(20, 2.5)""#)
        .unwrap();
    }

    #[test]
    fn helpful_errors() {
        assert!(
            err(r#"entry = "rsx(14) < 30"
                       exit = "close > 1""#)
            .contains("unknown indicator `rsx`")
        );
        assert!(
            err(r#"entry = "rsi < 30"
                       exit = "close > 1""#)
            .contains("rsi(period)")
        );
        assert!(
            err(r#"entry = "rsi(14.5) < 30"
                       exit = "close > 1""#)
            .contains("whole number")
        );
        assert!(
            err(r#"entry = "rsi(14) = 30"
                       exit = "close > 1""#)
            .contains("not supported")
        );
        assert!(
            err(r#"entry = "rsi(14) 30"
                       exit = "close > 1""#)
            .contains("expected <, <=, >, >=")
        );
        assert!(
            err(r#"entry = "rsi($n) < 30"
                       exit = "close > 1""#)
            .contains("`$n` is used in a condition but not defined")
        );
        assert!(err(r#"entry = "close > 1""#).contains("never closes"));
        assert!(
            err(r#"entry = "close > 1"
                       exit = "close < 1"
                       stop_loss = 5"#)
            .contains("unknown field `stop_loss`")
        );
        assert!(
            err(r#"entry = "close > 1"
                       exit = "close < 1"
                       [grid]
                       x = [1, 2]"#)
            .contains("[grid] `x` is not a parameter")
        );
        assert!(
            err(r#"entry = "1 < 2"
                       exit = "close < 1""#)
            .contains("two constants")
        );
        assert!(
            err(r#"entry = "sma($p) < close"
                       exit = "close < 1"
                       [params]
                       p = 10
                       [grid]
                       p = [5, 0]"#)
            .contains("p=0")
        );
        // TOML syntax errors carry the file name.
        assert!(err("entry = ").contains("test.toml"));
    }

    #[test]
    fn crosses_need_both_bars() {
        let s = def(r#"entry = "close crosses_above 10"
                       exit = "close crosses_below 10""#)
        .unwrap()
        .compile()
        .unwrap();
        let sig = s.signals(&bars(&[9.0, 11.0, 12.0, 9.0, 10.0, 10.5]));
        assert_eq!(sig.entry, vec![false, true, false, false, false, true]);
        assert_eq!(sig.exit, vec![false, false, false, true, false, false]);
    }

    #[test]
    fn warmup_is_false_even_under_not() {
        let s = def(r#"entry = "not sma(3) > 100"
                       stop_loss_pct = 5"#)
        .unwrap()
        .compile()
        .unwrap();
        let sig = s.signals(&bars(&[1.0, 2.0, 3.0, 4.0]));
        assert_eq!(sig.entry, vec![false, false, true, true]);
        assert!(sig.exit.iter().all(|e| !e));
        assert_eq!(s.lookback(), 3);
    }

    #[test]
    fn params_overrides_and_grid() {
        let d = load("rsi").unwrap();
        let combos = d.grid_combos();
        assert_eq!(combos.len(), 9);
        assert!(combos.iter().all(|c| c["period"] == 14.0));
        let d2 = d.with_params(&[("oversold".into(), 25.0)]).unwrap();
        assert_eq!(d2.params["oversold"], 25.0);
        assert_eq!(d2.grid["oversold"], vec![25.0]);
        assert_eq!(d2.grid_combos().len(), 3);
        assert!(d.with_params(&[("nope".into(), 1.0)]).is_err());
        assert!(d.with_params(&[("period".into(), 0.0)]).is_err());
        assert_eq!(
            render("rsi($period) < $oversold and $nope", &d2.params),
            "rsi(14) < 25 and $nope"
        );
        assert_eq!(
            parse_param_override("oversold = 25").unwrap(),
            ("oversold".into(), 25.0)
        );
        assert!(parse_param_override("oversold").is_err());
        assert!(parse_param_override("x=abc").is_err());
    }

    /// The original hard-coded strategies, kept to prove the bundled TOML
    /// versions signal on the same bars.
    mod legacy {
        use crate::technical;

        pub fn rsi(c: &[f64]) -> Vec<i8> {
            let mut s = vec![0; c.len()];
            for i in 15..c.len() {
                if let Some(r) = technical::rsi(&c[..=i], 14) {
                    s[i] = if r < 30.0 {
                        1
                    } else if r > 70.0 {
                        -1
                    } else {
                        0
                    };
                }
            }
            s
        }

        pub fn macd(c: &[f64]) -> Vec<i8> {
            let mut s = vec![0; c.len()];
            let mut prev = 0.0;
            for i in 35..c.len() {
                if let Some((_, _, h)) = technical::macd(&c[..=i]) {
                    if h > 0.0 && prev <= 0.0 {
                        s[i] = 1;
                    } else if h < 0.0 && prev >= 0.0 {
                        s[i] = -1;
                    }
                    prev = h;
                }
            }
            s
        }

        pub fn sma(c: &[f64]) -> Vec<i8> {
            let mut s = vec![0; c.len()];
            let mut prev = false;
            for i in 50..c.len() {
                if let (Some(a), Some(b)) =
                    (technical::sma(&c[..=i], 20), technical::sma(&c[..=i], 50))
                {
                    let above = a > b;
                    if above && !prev {
                        s[i] = 1;
                    } else if !above && prev {
                        s[i] = -1;
                    }
                    prev = above;
                }
            }
            s
        }

        pub fn bb(c: &[f64]) -> Vec<i8> {
            let mut s = vec![0; c.len()];
            for i in 20..c.len() {
                if let Some((u, _, l)) = technical::bollinger_bands(&c[..=i], 20) {
                    s[i] = if c[i] < l {
                        1
                    } else if c[i] > u {
                        -1
                    } else {
                        0
                    };
                }
            }
            s
        }

        pub fn vwap(c: &[f64], h: &[f64], l: &[f64], v: &[u64]) -> Vec<i8> {
            let mut s = vec![0; c.len()];
            let mut prev = false;
            for i in 10..c.len() {
                if let Some(w) = technical::vwap(&h[..=i], &l[..=i], &c[..=i], &v[..=i]) {
                    let above = c[i] > w;
                    if above && !prev {
                        s[i] = 1;
                    } else if !above && prev {
                        s[i] = -1;
                    }
                    prev = above;
                }
            }
            s
        }

        pub fn mr(c: &[f64]) -> Vec<i8> {
            let mut s = vec![0; c.len()];
            for i in 20..c.len() {
                if let Some(m) = technical::sma(&c[..=i], 20) {
                    let dev = (c[i] - m) / m * 100.0;
                    s[i] = if dev < -3.0 {
                        1
                    } else if dev > 3.0 {
                        -1
                    } else {
                        0
                    };
                }
            }
            s
        }
    }

    #[test]
    fn bundled_strategies_match_the_original_hard_coded_ones() {
        let closes: Vec<f64> = (0..400)
            .map(|i| {
                let t = i as f64;
                100.0 + (t * 0.07).sin() * 12.0 + (t * 0.31).sin() * 4.0 + t * 0.02
            })
            .collect();
        let mut b = bars(&closes);
        b.high = closes.iter().map(|c| c * 1.01).collect();
        b.low = closes.iter().map(|c| c * 0.99).collect();
        b.volume = (0..400).map(|i| 1_000 + (i * 53 % 700) as u64).collect();

        // Legacy loops seeded their "previous" state with a guess on the first
        // bar; compare from the bar after that.
        let cases: [(&str, Vec<i8>, usize); 6] = [
            ("rsi", legacy::rsi(&closes), 15),
            ("macd", legacy::macd(&closes), 36),
            ("sma", legacy::sma(&closes), 51),
            ("bb", legacy::bb(&closes), 20),
            (
                "vwap",
                legacy::vwap(&closes, &b.high, &b.low, &b.volume),
                11,
            ),
            ("mr", legacy::mr(&closes), 20),
        ];
        for (name, expected, from) in cases {
            let sig = load(name).unwrap().compile().unwrap().signals(&b);
            let mut signals = 0;
            for i in from..closes.len() {
                let got = if sig.entry[i] && !sig.exit[i] {
                    1
                } else if sig.exit[i] {
                    -1
                } else {
                    0
                };
                assert_eq!(got, expected[i], "{name} differs at bar {i}");
                signals += (got != 0) as usize;
            }
            assert!(signals > 2, "{name}: test series should trigger signals");
        }
    }
}

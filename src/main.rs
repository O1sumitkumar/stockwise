// StockWise is an application crate. The lints below are silenced deliberately:
// - dead_code: helpers, broker-response fields, and stubs kept for upcoming
//   ROADMAP features and to document the shape of deserialized API responses.
// - the clippy lints are stylistic/structural choices in chart rendering, data
//   alignment loops, and test fixtures — not correctness issues.
#![allow(dead_code)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::if_same_then_else)]

mod ai;
mod alerts;
mod angel;
mod api;
mod backtest;
mod charts;
mod commands;
mod config;
mod daemon;
mod display;
mod insights;
mod intraday;
mod longterm;
mod market;
mod output;
mod portfolio;
mod simulator;
mod store;
mod technical;
mod watchlist;
mod wealth;

use clap::parser::ValueSource;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use market::Market;
use output::Format;

/// Periods Yahoo Finance understands. Short periods give too few bars for
/// some indicators; those commands say so instead of guessing.
const PERIODS: [&str; 12] = [
    "1d", "5d", "1w", "1mo", "3mo", "6mo", "ytd", "1y", "2y", "5y", "10y", "max",
];

#[derive(Parser)]
#[command(
    name = "stockwise",
    version,
    about = "Stock market toolkit for your terminal — US and Indian markets",
    long_about = "StockWise — quotes, analysis, screeners, backtests, paper trading, and \
portfolio tracking for US and Indian (NSE/BSE) stocks, from your terminal.\n\n\
The Indian market is the default: `stockwise quote TCS` looks up TCS.NS. \
Use `-m us` (or set STOCKWISE_MARKET=us) for US stocks.\n\n\
Not financial advice. See DISCLAIMER.md.",
    after_help = "Examples:\n  \
stockwise quote RELIANCE TCS          Indian stocks (default market)\n  \
stockwise quote AAPL NVDA -m us       US stocks\n  \
stockwise deep HDFCBANK               everything about one stock\n  \
stockwise backtest INFY -s macd       test a strategy on history\n  \
stockwise quote AAPL -m us --json     machine-readable output\n  \
stockwise portfolio --csv > p.csv     spreadsheet-ready CSV\n\n\
Run `stockwise doctor` to check your setup, `stockwise config init` to set your own \
defaults, `stockwise <command> --help` for details."
)]
struct Cli {
    /// Market for bare tickers: `in` (India, adds .NS) or `us`
    #[arg(
        short = 'm',
        long = "market",
        global = true,
        default_value = "in",
        env = "STOCKWISE_MARKET",
        ignore_case = true
    )]
    market: Market,

    /// Add a local-AI (Ollama) section to analysis commands. Slow: 30-60s per call
    #[arg(
        long,
        global = true,
        env = "STOCKWISE_AI",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with = "no_ai"
    )]
    ai: bool,

    /// Turn the AI section off, even if STOCKWISE_AI or the config file turn it on
    #[arg(long, global = true, overrides_with = "ai")]
    no_ai: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // ── Start here ──
    /// Real-time quote for one or more stocks
    #[command(alias = "q")]
    Quote {
        /// Tickers, e.g. RELIANCE TCS (India) or AAPL MSFT with -m us
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Everything about a stock in one command
    Deep {
        symbol: String,
        #[arg(short, long, default_value = "3mo", value_parser = PERIODS)]
        period: String,
    },

    /// Major market indices (US + India)
    Markets {
        #[command(flatten)]
        out: TableOutput,
    },

    /// Markets + portfolio + watchlist + alerts on one screen
    #[command(alias = "d")]
    Dashboard,

    /// Find a ticker by company name
    Search {
        /// Company name or partial ticker, e.g. "tata motors"
        query: String,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Check your setup: data directory, network, cache, AI, broker
    Doctor,

    // ── Analysis ──
    /// Fundamental analysis with plain-English insights
    #[command(alias = "a")]
    Analyze { symbol: String },

    /// Technical analysis (RSI, MACD, Bollinger Bands, ...)
    #[command(alias = "t")]
    Technical {
        symbol: String,
        #[arg(short, long, default_value = "3mo", value_parser = PERIODS)]
        period: String,
        #[command(flatten)]
        out: JsonOutput,
    },

    /// Price history with a terminal chart and volume bars
    History {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
        /// Print every OHLCV bar as JSON instead of a chart
        #[arg(long, conflicts_with = "csv")]
        json: bool,
        /// Print every OHLCV bar as CSV instead of a chart
        #[arg(long)]
        csv: bool,
    },

    /// Charts for 1W, 1M, 3M and 1Y at once
    Chart { symbol: String },

    /// Compare stocks side by side
    #[command(alias = "c")]
    Compare {
        /// Two or more tickers
        #[arg(required = true, num_args = 2..)]
        symbols: Vec<String>,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Risk metrics: volatility, Sharpe, max drawdown, VaR
    Risk {
        symbol: String,
        /// History used for the metrics (daily bars; needs ~30+ trading days)
        #[arg(short, long, default_value = "1y", value_parser = PERIODS)]
        period: String,
        #[command(flatten)]
        out: JsonOutput,
    },

    /// Price correlation matrix between stocks
    #[command(alias = "corr")]
    Correlate {
        /// Two or more tickers
        #[arg(required = true, num_args = 2..)]
        symbols: Vec<String>,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Trend and momentum across timeframes at a glance
    #[command(alias = "mtf")]
    Timeframes { symbol: String },

    /// Returns over 1W, 1M, 3M, 6M, 1Y, 3Y, 5Y
    Returns {
        symbol: String,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Candlestick pattern detection
    Patterns {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    /// Support & resistance levels (pivot points + Fibonacci)
    Support {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    /// Fibonacci retracement levels
    Fibs {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    /// Volume profile (OBV, A/D line, volume stats)
    #[command(alias = "vol")]
    Volume {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    /// Detect price gaps (filled and unfilled)
    Gaps {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    /// Stop-loss levels (ATR, percentage, Chandelier exit)
    Stoploss {
        symbol: String,
        /// Your entry price (default: last price)
        #[arg(short, long)]
        entry: Option<f64>,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    /// Trend projection with confidence bands (not a prediction)
    Forecast {
        symbol: String,
        /// Days to project forward
        #[arg(short, long, default_value = "30")]
        days: usize,
        /// History used to fit the trend
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    /// Expected moves and strike ideas from volatility
    Options {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    // ── Market ──
    /// Top gainers and losers today
    Movers {
        #[command(flatten)]
        out: TableOutput,
    },
    /// Sector performance with bars
    Sectors {
        #[command(flatten)]
        out: TableOutput,
    },
    /// Market heatmap
    Heatmap,
    /// Fear & Greed style sentiment gauge
    Sentiment,
    /// Recent news headlines for a stock
    News { symbol: String },
    /// Sector rotation — which sectors lead and lag
    Rotation,
    /// Compare two sectors head to head (e.g. IT Bank)
    Sectorcmp { sector1: String, sector2: String },
    /// Compare against industry peers
    Peers { symbol: String },
    /// Insider activity and analyst consensus
    Insider { symbol: String },
    /// IPO calendar and news
    Ipo,
    /// Earnings overview for your portfolio and watchlist
    Earnings,
    /// Earnings momentum — accelerating vs decelerating
    Surprise,
    /// Quality vs growth quadrants
    Matrix,
    /// Top-scored stocks from the large-cap universe
    Picks,
    /// Screen large caps by a preset
    Screen {
        #[arg(value_enum, ignore_case = true)]
        category: ScreenCategory,
        #[command(flatten)]
        out: TableOutput,
    },
    /// Dividend yield, income projection and DRIP growth
    Dividends {
        symbol: String,
        #[command(flatten)]
        out: JsonOutput,
    },

    // ── Strategies & bots ──
    /// Backtest a trading strategy on history
    Backtest {
        symbol: String,
        #[arg(short, long, value_enum, ignore_case = true, default_value = "rsi")]
        strategy: Strategy,
        #[arg(short, long, default_value = "2y", value_parser = PERIODS)]
        period: String,
        /// Print the result and every trade as JSON
        #[arg(long, conflicts_with = "csv")]
        json: bool,
        /// Print every trade as CSV
        #[arg(long)]
        csv: bool,
    },

    /// SIP / DCA simulator — what if you invested X every month
    Sip {
        symbol: String,
        /// Amount invested each month
        amount: f64,
        #[arg(short, long, default_value = "5y", value_parser = PERIODS)]
        period: String,
        #[command(flatten)]
        out: JsonOutput,
    },

    /// What if you had invested X on a past date
    Whatif {
        symbol: String,
        /// Amount invested
        amount: f64,
        /// Date in YYYY-MM-DD format
        date: String,
        #[command(flatten)]
        out: JsonOutput,
    },

    /// Intraday trade ideas sized for your capital (signals only, no orders)
    #[command(alias = "bot")]
    Intraday {
        /// Capital to deploy
        amount: f64,
        /// Target return for the day, in percent (e.g. 1.5)
        target: f64,
    },

    /// Long-term picks for a monthly SIP budget
    Longterm {
        /// Monthly SIP budget (default 10000)
        amount: Option<f64>,
    },

    /// Background worker: intraday paper-trading loop or daily long-term scan
    Daemon {
        #[arg(value_enum, ignore_case = true)]
        mode: DaemonMode,
        /// Paper capital for intraday mode (default 25000)
        amount: Option<f64>,
    },

    /// Paper-trading simulator — practise without real money
    Sim {
        #[command(subcommand)]
        action: Option<SimCmd>,
    },

    /// AI report via local Ollama: intraday, longterm, portfolio, or a ticker
    Report {
        /// `intraday`, `longterm`, `portfolio`, or a ticker such as RELIANCE
        what: String,
    },

    // ── Portfolio ──
    /// Track holdings and P&L (default: show)
    #[command(alias = "p", args_conflicts_with_subcommands = true)]
    Portfolio {
        #[command(subcommand)]
        action: Option<PortfolioCmd>,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Manage your watchlist (default: show)
    #[command(alias = "w", args_conflicts_with_subcommands = true)]
    Watch {
        #[command(subcommand)]
        action: Option<WatchCmd>,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Price alerts (default: list)
    #[command(args_conflicts_with_subcommands = true)]
    Alert {
        #[command(subcommand)]
        action: Option<AlertCmd>,
        #[command(flatten)]
        out: TableOutput,
    },

    /// Export portfolio or watchlist to CSV in the current directory
    Export {
        #[arg(value_enum, ignore_case = true)]
        what: ExportWhat,
    },

    /// Import holdings from a broker CSV export
    Import {
        #[arg(value_enum, ignore_case = true)]
        source: ImportSource,
        /// Path to the exported CSV file
        file: String,
    },

    /// Snapshot portfolio value over time (run daily)
    Wealth,
    /// Trades to rebalance your portfolio to equal weight
    Rebalance,
    /// Tax-loss harvesting opportunities
    Harvest,
    /// Capital-gains tax estimate for your portfolio
    Tax {
        #[arg(value_enum, ignore_case = true)]
        country: TaxCountry,
        #[command(flatten)]
        out: TableOutput,
    },
    /// Dividend calendar for portfolio + watchlist
    Divcal,
    /// Correlation between your holdings — find redundant positions
    Pcorr,

    // ── Live trading ──
    /// Live trading via Angel One (real money — try `sim` first)
    Trade {
        #[command(subcommand)]
        action: TradeCmd,
    },

    // ── Setup ──
    /// Your defaults: config file path, effective settings, template (default: show)
    Config {
        #[command(subcommand)]
        action: Option<ConfigCmd>,
    },

    /// Print a shell completion script (bash, zsh, fish, powershell, elvish)
    Completions {
        #[arg(value_enum, ignore_case = true)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum PortfolioCmd {
    /// Add shares (averages into an existing holding)
    Add {
        symbol: String,
        /// Number of shares
        shares: f64,
        /// Price paid per share
        cost: f64,
    },
    /// Remove a holding
    #[command(alias = "rm")]
    Remove { symbol: String },
    /// Show holdings with live P&L
    #[command(alias = "list")]
    Show {
        #[command(flatten)]
        out: TableOutput,
    },
}

#[derive(Subcommand)]
enum WatchCmd {
    /// Add tickers to the watchlist
    Add {
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,
    },
    /// Remove tickers from the watchlist
    #[command(alias = "rm")]
    Remove {
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,
    },
    /// Show the watchlist with live prices
    #[command(alias = "list")]
    Show {
        #[command(flatten)]
        out: TableOutput,
    },
}

#[derive(Subcommand)]
enum AlertCmd {
    /// Alert when a price crosses a level, e.g. `alert add TCS above 4200`
    Add {
        symbol: String,
        #[arg(value_enum, ignore_case = true)]
        condition: AlertWhen,
        /// Price level
        target: f64,
    },
    /// Remove an alert by its number from `alert list`
    #[command(alias = "rm")]
    Remove { index: usize },
    /// Check all alerts against live prices
    Check {
        #[command(flatten)]
        out: TableOutput,
    },
    /// List alerts
    #[command(alias = "show")]
    List {
        #[command(flatten)]
        out: TableOutput,
    },
}

#[derive(Subcommand)]
enum SimCmd {
    /// Start a paper session with the bot's picks
    Start {
        /// Paper capital (default 25000)
        amount: Option<f64>,
        /// Target return in percent
        target: Option<f64>,
    },
    /// Live P&L of the open session
    Status,
    /// Close the session at current prices and record the result
    Settle,
    /// Past sessions and the bot's confidence rating
    #[command(alias = "stats")]
    History {
        #[command(flatten)]
        out: JsonOutput,
    },
    /// Delete all simulator history (a backup is kept)
    Reset {
        /// Don't ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Print the config file path (it may not exist yet)
    Path,
    /// Effective settings and where each comes from: flag, env, config file, or default
    Show {
        #[command(flatten)]
        out: JsonOutput,
    },
    /// Write a commented config template to the config file path
    Init {
        /// Replace an existing config file
        #[arg(long)]
        force: bool,
    },
}

/// `--json` / `--csv` for commands that print a table.
#[derive(Args, Clone, Copy, Default)]
struct TableOutput {
    /// Print JSON instead of a table
    #[arg(long, conflicts_with = "csv")]
    json: bool,
    /// Print CSV (RFC 4180, header row first) instead of a table
    #[arg(long)]
    csv: bool,
}

/// `--json` for commands whose output is more than one table.
#[derive(Args, Clone, Copy, Default)]
struct JsonOutput {
    /// Print JSON instead of text
    #[arg(long)]
    json: bool,
}

impl From<TableOutput> for Format {
    fn from(o: TableOutput) -> Self {
        Format::from_flags(o.json, o.csv)
    }
}

impl From<JsonOutput> for Format {
    fn from(o: JsonOutput) -> Self {
        Format::from_flags(o.json, false)
    }
}

impl TableOutput {
    /// Flags given on either the parent (`portfolio --json`) or the action
    /// (`portfolio show --json`).
    fn or(self, other: TableOutput) -> Format {
        Format::from_flags(self.json || other.json, self.csv || other.csv)
    }
}

#[derive(Args, Clone, Copy)]
struct OrderFlags {
    /// Place the order without the interactive confirmation
    #[arg(short, long)]
    yes: bool,
    /// Resolve and preview the order, but don't send it
    #[arg(long)]
    dry_run: bool,
}

impl From<OrderFlags> for commands::OrderOpts {
    fn from(f: OrderFlags) -> Self {
        Self {
            yes: f.yes,
            dry_run: f.dry_run,
        }
    }
}

#[derive(Subcommand)]
enum TradeCmd {
    /// How to connect your Angel One account
    Setup,
    /// Create the credentials file, or show whether it is configured
    Config,
    /// Live holdings (also synced into `portfolio`)
    Holdings,
    /// Open intraday positions
    Positions,
    /// Today's order book
    Orders,
    /// Market BUY on NSE (asks for confirmation)
    Buy {
        symbol: String,
        /// Number of shares
        qty: u32,
        #[command(flatten)]
        flags: OrderFlags,
    },
    /// Market SELL on NSE (asks for confirmation)
    Sell {
        symbol: String,
        /// Number of shares
        qty: u32,
        #[command(flatten)]
        flags: OrderFlags,
    },
    /// Limit BUY on NSE (asks for confirmation)
    Limit {
        symbol: String,
        /// Number of shares
        qty: u32,
        /// Limit price
        price: f64,
        #[command(flatten)]
        flags: OrderFlags,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ScreenCategory {
    /// P/E below 15
    Undervalued,
    /// Forward EPS above trailing EPS
    Growth,
    /// Dividend yield above 2%
    Dividend,
    /// Top daily gainers
    Momentum,
    /// Market cap above $100B / ₹5L Cr
    Bluechip,
}

#[derive(Clone, Copy, ValueEnum)]
enum Strategy {
    /// RSI oversold/overbought
    Rsi,
    /// MACD crossover
    Macd,
    /// SMA crossover
    #[value(alias = "sma_crossover")]
    Sma,
    /// Bollinger Band reversion
    #[value(alias = "bollinger")]
    Bb,
    /// VWAP crossover
    Vwap,
    /// Mean reversion
    #[value(alias = "mean_reversion")]
    Mr,
}

#[derive(Clone, Copy, ValueEnum)]
enum DaemonMode {
    /// 9:00-15:30 IST paper-trading loop
    Intraday,
    /// Daily post-market long-term scan
    Longterm,
}

#[derive(Clone, Copy, ValueEnum)]
enum ExportWhat {
    Portfolio,
    Watchlist,
}

#[derive(Clone, Copy, ValueEnum)]
enum ImportSource {
    /// Zerodha Kite holdings CSV
    #[value(alias = "zerodha")]
    Kite,
    /// INDmoney holdings CSV
    Indmoney,
    /// Any CSV with symbol, quantity and average-price columns
    Csv,
}

#[derive(Clone, Copy, ValueEnum)]
enum TaxCountry {
    /// India (STCG/LTCG)
    #[value(alias = "india")]
    In,
    /// United States
    #[value(alias = "usa")]
    Us,
    /// United Kingdom
    Uk,
}

#[derive(Clone, Copy, ValueEnum)]
enum AlertWhen {
    #[value(alias = ">")]
    Above,
    #[value(alias = "<")]
    Below,
}

/// The CLI spelling of a ValueEnum variant (e.g. `Strategy::Bb` → "bb"),
/// which is what the command handlers match on.
fn value_name<T: ValueEnum>(v: T) -> String {
    v.to_possible_value()
        .expect("no skipped variants")
        .get_name()
        .to_string()
}

/// Top-level help groups. Every subcommand must appear exactly once (tested).
const GROUPS: &[(&str, &[&str])] = &[
    (
        "Start here",
        &["quote", "deep", "markets", "dashboard", "search", "doctor"],
    ),
    (
        "Analysis",
        &[
            "analyze",
            "technical",
            "history",
            "chart",
            "compare",
            "risk",
            "correlate",
            "timeframes",
            "returns",
            "patterns",
            "support",
            "fibs",
            "volume",
            "gaps",
            "stoploss",
            "forecast",
            "options",
        ],
    ),
    (
        "Market",
        &[
            "movers",
            "sectors",
            "heatmap",
            "sentiment",
            "news",
            "rotation",
            "sectorcmp",
            "peers",
            "insider",
            "ipo",
            "earnings",
            "surprise",
            "matrix",
            "picks",
            "screen",
            "dividends",
        ],
    ),
    (
        "Strategies & bots",
        &[
            "backtest", "sip", "whatif", "intraday", "longterm", "daemon", "sim", "report",
        ],
    ),
    (
        "Portfolio",
        &[
            "portfolio",
            "watch",
            "alert",
            "export",
            "import",
            "wealth",
            "rebalance",
            "harvest",
            "tax",
            "divcal",
            "pcorr",
        ],
    ),
    ("Live trading", &["trade"]),
    ("Setup", &["config", "completions", "help"]),
];

/// Top-level help with commands grouped by purpose (clap would otherwise list
/// 60 commands flat). Descriptions come from the doc comments above.
fn grouped_help_template(cmd: &clap::Command) -> String {
    use colored::Colorize;
    let mut out = String::from("{before-help}{about-with-newline}\n{usage-heading} {usage}\n");
    for (group, names) in GROUPS {
        out.push_str(&format!("\n{}\n", format!("{}:", group).bold().underline()));
        for name in *names {
            let about = cmd
                .find_subcommand(name)
                .and_then(|c| c.get_about())
                .map(|a| a.to_string())
                .unwrap_or_else(|| "Print help for a command".into());
            // Pad before styling so ANSI codes don't break the alignment.
            out.push_str(&format!("  {} {}\n", format!("{:<12}", name).bold(), about));
        }
    }
    out.push_str(&format!(
        "\n{}\n{{options}}{{after-help}}",
        "Options:".bold().underline()
    ));
    out
}

fn build_cli() -> clap::Command {
    let cmd = Cli::command();
    let template = grouped_help_template(&cmd);
    cmd.help_template(template)
}

/// Commands that take `-p/--period`, with their current default period.
fn period_defaults(cmd: &clap::Command) -> Vec<(String, String)> {
    cmd.get_subcommands()
        .filter_map(|sc| {
            let arg = sc.get_arguments().find(|a| a.get_id() == "period")?;
            let default = arg
                .get_default_values()
                .first()
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default();
            Some((sc.get_name().to_string(), default))
        })
        .collect()
}

/// Make the config file's values the clap defaults, so flags and env vars
/// still win and `--help` shows the effective default. `cfg` is validated.
fn with_config_defaults(mut cmd: clap::Command, cfg: &config::Config) -> clap::Command {
    if let Some(m) = cfg
        .market
        .as_deref()
        .and_then(|m| config::market_value(m).ok())
    {
        cmd = cmd.mut_arg("market", |a| a.default_value(m));
    }
    if cfg.ai == Some(true) {
        cmd = cmd.mut_arg("ai", |a| a.default_value("true"));
    }
    for (name, period) in &cfg.periods {
        if let Ok(p) = config::period_value(period) {
            cmd = cmd.mut_subcommand(name, |sc| sc.mut_arg("period", |a| a.default_value(p)));
        }
    }
    cmd
}

/// Where a global option's value came from.
fn arg_source(matches: &clap::ArgMatches, id: &str, in_file: bool) -> config::Source {
    match matches.value_source(id) {
        Some(ValueSource::CommandLine) => config::Source::Flag,
        Some(ValueSource::EnvVariable) => config::Source::Env,
        _ if in_file => config::Source::File,
        _ => config::Source::Default,
    }
}

/// Exit quietly when stdout is closed early (`stockwise ... | head`) instead
/// of panicking with "failed printing to stdout: Broken pipe".
fn exit_quietly_on_broken_pipe() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.contains("Broken pipe") {
            std::process::exit(0);
        }
        default_hook(info);
    }));
}

#[tokio::main]
async fn main() {
    exit_quietly_on_broken_pipe();
    let cmd = build_cli();
    let period_cmds: Vec<String> = period_defaults(&cmd).into_iter().map(|(c, _)| c).collect();
    let period_refs: Vec<&str> = period_cmds.iter().map(String::as_str).collect();
    let cfg_state = config::install(config::load(&period_refs));
    let cmd = with_config_defaults(cmd, config::get());
    let periods = period_defaults(&cmd);
    let matches = cmd.get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    // A broken config file stops everything except the commands that help fix it.
    if let config::Status::Invalid(msg) = &cfg_state.status
        && !matches!(
            cli.command,
            Commands::Config { .. } | Commands::Doctor | Commands::Completions { .. }
        )
    {
        eprintln!("\n  {} {}\n", colored::Colorize::red("Error:"), msg);
        std::process::exit(1);
    }

    let cfg = config::get();
    let sources = config::CliSources {
        market: arg_source(&matches, "market", cfg.market.is_some()),
        ai: if matches.value_source("no_ai") == Some(ValueSource::CommandLine) {
            config::Source::Flag
        } else {
            arg_source(&matches, "ai", cfg.ai.is_some())
        },
    };
    let m = cli.market;
    ai::set_auto_ai(cli.ai && !cli.no_ai);

    let result = match cli.command {
        Commands::Quote { symbols, out } => commands::cmd_quote(&symbols, m, out.into()).await,
        Commands::Deep { symbol, period } => commands::cmd_deep(&symbol, &period, m).await,
        Commands::Markets { out } => commands::cmd_markets(m, out.into()).await,
        Commands::Dashboard => commands::cmd_dashboard(m).await,
        Commands::Search { query, out } => commands::cmd_search(&query, out.into()).await,
        Commands::Doctor => commands::cmd_doctor(m, sources).await,

        Commands::Analyze { symbol } => commands::cmd_analyze(&symbol, m).await,
        Commands::Technical {
            symbol,
            period,
            out,
        } => commands::cmd_technical(&symbol, &period, m, out.into()).await,
        Commands::History {
            symbol,
            period,
            json,
            csv,
        } => commands::cmd_history(&symbol, &period, m, Format::from_flags(json, csv)).await,
        Commands::Chart { symbol } => commands::cmd_chart(&symbol, m).await,
        Commands::Compare { symbols, out } => commands::cmd_compare(&symbols, m, out.into()).await,
        Commands::Risk {
            symbol,
            period,
            out,
        } => commands::cmd_risk(&symbol, &period, m, out.into()).await,
        Commands::Correlate { symbols, out } => {
            commands::cmd_correlate(&symbols, m, out.into()).await
        }
        Commands::Timeframes { symbol } => commands::cmd_multitimeframe(&symbol, m).await,
        Commands::Returns { symbol, out } => commands::cmd_returns(&symbol, m, out.into()).await,
        Commands::Patterns { symbol, period } => commands::cmd_patterns(&symbol, &period, m).await,
        Commands::Support { symbol, period } => commands::cmd_support(&symbol, &period, m).await,
        Commands::Fibs { symbol, period } => commands::cmd_fibs(&symbol, &period, m).await,
        Commands::Volume { symbol, period } => commands::cmd_volume(&symbol, &period, m).await,
        Commands::Gaps { symbol, period } => commands::cmd_gaps(&symbol, &period, m).await,
        Commands::Stoploss {
            symbol,
            entry,
            period,
        } => commands::cmd_stoploss(&symbol, entry, &period, m).await,
        Commands::Forecast {
            symbol,
            days,
            period,
        } => commands::cmd_forecast(&symbol, days, &period, m).await,
        Commands::Options { symbol, period } => commands::cmd_options(&symbol, &period, m).await,

        Commands::Movers { out } => commands::cmd_movers(m, out.into()).await,
        Commands::Sectors { out } => commands::cmd_sectors(m, out.into()).await,
        Commands::Heatmap => commands::cmd_heatmap(m).await,
        Commands::Sentiment => commands::cmd_sentiment().await,
        Commands::News { symbol } => commands::cmd_news(&symbol, m).await,
        Commands::Rotation => commands::cmd_rotation(m).await,
        Commands::Sectorcmp { sector1, sector2 } => {
            commands::cmd_sectorcmp(&sector1, &sector2, m).await
        }
        Commands::Peers { symbol } => commands::cmd_peers(&symbol, m).await,
        Commands::Insider { symbol } => commands::cmd_insider(&symbol, m).await,
        Commands::Ipo => commands::cmd_ipo().await,
        Commands::Earnings => commands::cmd_earnings(m).await,
        Commands::Surprise => commands::cmd_surprise(m).await,
        Commands::Matrix => commands::cmd_matrix(m).await,
        Commands::Picks => commands::cmd_picks(m).await,
        Commands::Screen { category, out } => {
            commands::cmd_screen(&value_name(category), m, out.into()).await
        }
        Commands::Dividends { symbol, out } => {
            commands::cmd_dividends(&symbol, m, out.into()).await
        }

        Commands::Backtest {
            symbol,
            strategy,
            period,
            json,
            csv,
        } => {
            let fmt = Format::from_flags(json, csv);
            commands::cmd_backtest(&symbol, &value_name(strategy), &period, m, fmt).await
        }
        Commands::Sip {
            symbol,
            amount,
            period,
            out,
        } => commands::cmd_sip(&symbol, amount, &period, m, out.into()).await,
        Commands::Whatif {
            symbol,
            amount,
            date,
            out,
        } => commands::cmd_whatif(&symbol, amount, &date, m, out.into()).await,
        Commands::Intraday { amount, target } => commands::cmd_intraday(amount, target, m).await,
        Commands::Longterm { amount } => commands::cmd_longterm(amount, m).await,
        Commands::Daemon { mode, amount } => {
            commands::cmd_daemon(&value_name(mode), amount, m).await
        }
        Commands::Sim { action } => match action {
            Some(SimCmd::Reset { yes }) => commands::cmd_sim_reset(yes),
            Some(SimCmd::Start { amount, target }) => {
                commands::cmd_sim("start", amount, target, m).await
            }
            Some(SimCmd::Status) => commands::cmd_sim("status", None, None, m).await,
            Some(SimCmd::Settle) => commands::cmd_sim("settle", None, None, m).await,
            Some(SimCmd::History { out }) => commands::cmd_sim_history(m, out.into()),
            None => commands::cmd_sim("help", None, None, m).await,
        },
        Commands::Report { what } => commands::cmd_report(&what, m).await,

        Commands::Portfolio { action, out } => match action {
            Some(PortfolioCmd::Add {
                symbol,
                shares,
                cost,
            }) => commands::cmd_portfolio("add", Some(&symbol), Some(shares), Some(cost), m).await,
            Some(PortfolioCmd::Remove { symbol }) => {
                commands::cmd_portfolio("remove", Some(&symbol), None, None, m).await
            }
            Some(PortfolioCmd::Show { out: show }) => {
                commands::cmd_portfolio_show(out.or(show)).await
            }
            None => commands::cmd_portfolio_show(out.into()).await,
        },
        Commands::Watch { action, out } => match action {
            Some(WatchCmd::Add { symbols }) => commands::cmd_watchlist("add", &symbols, m).await,
            Some(WatchCmd::Remove { symbols }) => {
                commands::cmd_watchlist("remove", &symbols, m).await
            }
            Some(WatchCmd::Show { out: show }) => commands::cmd_watchlist_show(out.or(show)).await,
            None => commands::cmd_watchlist_show(out.into()).await,
        },
        Commands::Alert { action, out } => match action {
            Some(AlertCmd::Add {
                symbol,
                condition,
                target,
            }) => {
                let condition = value_name(condition);
                commands::cmd_alert("add", Some(&symbol), Some(&condition), Some(target), m).await
            }
            Some(AlertCmd::Remove { index }) => {
                let index = index.to_string();
                commands::cmd_alert("remove", Some(&index), None, None, m).await
            }
            Some(AlertCmd::Check { out: check }) => commands::cmd_alert_check(out.or(check)).await,
            Some(AlertCmd::List { out: list }) => commands::cmd_alert_list(out.or(list)),
            None => commands::cmd_alert_list(out.into()),
        },
        Commands::Export { what } => commands::cmd_export(&value_name(what), m).await,
        Commands::Import { source, file } => {
            commands::cmd_import(&value_name(source), &file, m).await
        }
        Commands::Wealth => commands::cmd_wealth(m).await,
        Commands::Rebalance => commands::cmd_rebalance(&[], m).await,
        Commands::Harvest => commands::cmd_harvest(m).await,
        Commands::Tax { country, out } => {
            commands::cmd_tax(&value_name(country), m, out.into()).await
        }
        Commands::Divcal => commands::cmd_divcal(m).await,
        Commands::Pcorr => commands::cmd_pcorr(m).await,

        Commands::Trade { action } => {
            let (name, symbol, qty, price, flags) = match action {
                TradeCmd::Setup => ("setup", None, None, None, None),
                TradeCmd::Config => ("config", None, None, None, None),
                TradeCmd::Holdings => ("holdings", None, None, None, None),
                TradeCmd::Positions => ("positions", None, None, None, None),
                TradeCmd::Orders => ("orders", None, None, None, None),
                TradeCmd::Buy { symbol, qty, flags } => {
                    ("buy", Some(symbol), Some(qty), None, Some(flags))
                }
                TradeCmd::Sell { symbol, qty, flags } => {
                    ("sell", Some(symbol), Some(qty), None, Some(flags))
                }
                TradeCmd::Limit {
                    symbol,
                    qty,
                    price,
                    flags,
                } => ("limit", Some(symbol), Some(qty), Some(price), Some(flags)),
            };
            let opts = flags.map(Into::into).unwrap_or_default();
            commands::cmd_trade(name, symbol.as_deref(), qty, price, opts, m).await
        }

        Commands::Config { action } => match action {
            Some(ConfigCmd::Path) => commands::cmd_config_path(),
            Some(ConfigCmd::Init { force }) => commands::cmd_config_init(force),
            Some(ConfigCmd::Show { out }) => {
                commands::cmd_config_show(m, sources, &periods, out.into())
            }
            None => commands::cmd_config_show(m, sources, &periods, Format::Table),
        },

        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "stockwise",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("\n  {} {:#}\n", colored::Colorize::red("Error:"), e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_command_is_in_exactly_one_help_group() {
        let cmd = Cli::command();
        let mut names: Vec<String> = cmd
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect();
        names.push("help".into());
        for name in &names {
            let hits = GROUPS
                .iter()
                .filter(|(_, cmds)| cmds.contains(&name.as_str()))
                .count();
            assert_eq!(hits, 1, "`{name}` must be in exactly one help group");
        }
        let grouped: usize = GROUPS.iter().map(|(_, cmds)| cmds.len()).sum();
        assert_eq!(grouped, names.len(), "help groups list an unknown command");
    }

    #[test]
    fn quote_requires_a_symbol() {
        assert!(Cli::try_parse_from(["stockwise", "quote"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "quote", "TCS"]).is_ok());
    }

    #[test]
    fn compare_requires_two_symbols() {
        assert!(Cli::try_parse_from(["stockwise", "compare", "AAPL"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "compare", "AAPL", "MSFT"]).is_ok());
    }

    #[test]
    fn bad_period_is_rejected_at_parse_time() {
        assert!(Cli::try_parse_from(["stockwise", "history", "TCS", "-p", "7x"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "history", "TCS", "-p", "6mo"]).is_ok());
    }

    #[test]
    fn legacy_positional_forms_still_parse() {
        for args in [
            vec!["stockwise", "portfolio", "add", "TCS", "10", "3500"],
            vec!["stockwise", "portfolio", "show"],
            vec!["stockwise", "watch", "add", "TCS", "INFY"],
            vec!["stockwise", "alert", "add", "TCS", "above", "4200"],
            vec!["stockwise", "alert", "add", "TCS", ">", "4200"],
            vec!["stockwise", "sim", "start", "100000", "2"],
            vec!["stockwise", "trade", "buy", "RELIANCE", "10"],
            vec!["stockwise", "trade", "limit", "RELIANCE", "10", "1300"],
            vec!["stockwise", "import", "zerodha", "holdings.csv"],
            vec!["stockwise", "tax", "india"],
            vec!["stockwise", "daemon", "intraday", "25000"],
            vec!["stockwise", "backtest", "TCS", "-s", "bb"],
            vec!["stockwise", "backtest", "TCS", "-s", "bollinger"],
            vec!["stockwise", "sim", "start"],
            vec!["stockwise", "tax", "US"],
            vec!["stockwise", "tax", "India"],
            vec!["stockwise", "screen", "Growth"],
            vec!["stockwise", "history", "TCS", "-p", "ytd"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} should parse");
        }
    }

    #[test]
    fn typos_in_actions_are_rejected() {
        assert!(Cli::try_parse_from(["stockwise", "portfolio", "ad", "TCS", "1", "1"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "screen", "growht"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "trade", "by", "TCS", "1"]).is_err());
    }

    fn parses(args: &[&str]) -> bool {
        Cli::try_parse_from(args).is_ok()
    }

    #[test]
    fn json_and_csv_flags_parse_where_supported() {
        for args in [
            vec!["stockwise", "quote", "TCS", "--csv"],
            vec!["stockwise", "portfolio", "--json"],
            vec!["stockwise", "portfolio", "--csv"],
            vec!["stockwise", "portfolio", "show", "--csv"],
            vec!["stockwise", "p", "list", "--json"],
            vec!["stockwise", "watch", "--csv"],
            vec!["stockwise", "watch", "show", "--json"],
            vec!["stockwise", "alert", "--json"],
            vec!["stockwise", "alert", "list", "--csv"],
            vec!["stockwise", "alert", "check", "--json"],
            vec!["stockwise", "technical", "TCS", "--json"],
            vec!["stockwise", "history", "TCS", "-p", "6mo", "--csv"],
            vec!["stockwise", "risk", "TCS", "--json"],
            vec!["stockwise", "backtest", "TCS", "-s", "macd", "--csv"],
            vec!["stockwise", "backtest", "TCS", "--json"],
            vec!["stockwise", "screen", "growth", "--csv"],
            vec!["stockwise", "movers", "--csv"],
            vec!["stockwise", "sectors", "--json"],
            vec!["stockwise", "compare", "TCS", "INFY", "--csv"],
            vec!["stockwise", "correlate", "TCS", "INFY", "--csv"],
            vec!["stockwise", "sip", "TCS", "5000", "--json"],
            vec![
                "stockwise",
                "whatif",
                "TCS",
                "100000",
                "2024-01-15",
                "--json",
            ],
            vec!["stockwise", "returns", "TCS", "--csv"],
            vec!["stockwise", "dividends", "ITC", "--json"],
            vec!["stockwise", "tax", "in", "--csv"],
            vec!["stockwise", "sim", "history", "--json"],
            vec!["stockwise", "config", "show", "--json"],
        ] {
            assert!(parses(&args), "{args:?} should parse");
        }
    }

    #[test]
    fn json_and_csv_are_mutually_exclusive() {
        for args in [
            vec!["stockwise", "quote", "TCS", "--json", "--csv"],
            vec!["stockwise", "portfolio", "--json", "--csv"],
            vec!["stockwise", "history", "TCS", "--csv", "--json"],
            vec!["stockwise", "backtest", "TCS", "--json", "--csv"],
        ] {
            assert!(!parses(&args), "{args:?} should be rejected");
        }
    }

    #[test]
    fn output_flags_only_where_they_make_sense() {
        // CSV only for tables; no output flags on actions that change data.
        assert!(!parses(&["stockwise", "technical", "TCS", "--csv"]));
        assert!(!parses(&["stockwise", "risk", "TCS", "--csv"]));
        assert!(!parses(&[
            "stockwise",
            "portfolio",
            "add",
            "TCS",
            "1",
            "1",
            "--json"
        ]));
        assert!(!parses(&[
            "stockwise",
            "portfolio",
            "--json",
            "add",
            "TCS",
            "1",
            "1"
        ]));
        assert!(!parses(&["stockwise", "watch", "add", "TCS", "--csv"]));
    }

    #[test]
    fn output_flags_map_to_formats() {
        let cli = Cli::try_parse_from(["stockwise", "portfolio", "show", "--csv"]).unwrap();
        let Commands::Portfolio {
            action: Some(PortfolioCmd::Show { out: show }),
            out,
        } = cli.command
        else {
            panic!("expected portfolio show");
        };
        assert_eq!(out.or(show), Format::Csv);
        assert_eq!(Format::from(JsonOutput { json: true }), Format::Json);
        assert_eq!(Format::from(TableOutput::default()), Format::Table);
    }

    #[test]
    fn config_subcommands_parse() {
        assert!(parses(&["stockwise", "config"]));
        assert!(parses(&["stockwise", "config", "path"]));
        assert!(parses(&["stockwise", "config", "init", "--force"]));
        assert!(!parses(&["stockwise", "config", "path", "--json"]));
    }

    fn with_config(text: &str) -> clap::Command {
        let cmd = build_cli();
        let names: Vec<String> = period_defaults(&cmd).into_iter().map(|(c, _)| c).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let cfg = config::parse(text, &refs).unwrap();
        with_config_defaults(cmd, &cfg)
    }

    fn matches(cmd: clap::Command, args: &[&str]) -> Cli {
        Cli::from_arg_matches(&cmd.try_get_matches_from(args).unwrap()).unwrap()
    }

    #[test]
    fn config_file_sets_defaults_and_flags_still_win() {
        let cfg = "market = \"us\"\nai = true\n[periods]\ntechnical = \"6mo\"\n";

        let cli = matches(with_config(cfg), &["stockwise", "technical", "AAPL"]);
        assert_eq!(cli.market, Market::Us, "config beats the built-in default");
        assert!(cli.ai && !cli.no_ai);
        let Commands::Technical { period, .. } = cli.command else {
            panic!("expected technical");
        };
        assert_eq!(period, "6mo");

        let cli = matches(
            with_config(cfg),
            &[
                "stockwise",
                "-m",
                "in",
                "--no-ai",
                "technical",
                "TCS",
                "-p",
                "1y",
            ],
        );
        assert_eq!(cli.market, Market::In, "a flag beats the config file");
        assert!(cli.no_ai);
        let Commands::Technical { period, .. } = cli.command else {
            panic!("expected technical");
        };
        assert_eq!(period, "1y");

        // Other commands keep their built-in default period.
        let cli = matches(with_config(cfg), &["stockwise", "risk", "TCS"]);
        let Commands::Risk { period, .. } = cli.command else {
            panic!("expected risk");
        };
        assert_eq!(period, "1y");
    }

    #[test]
    fn help_shows_the_config_default() {
        let mut cmd = with_config("[periods]\nbacktest = \"5y\"\n");
        let help = cmd
            .find_subcommand_mut("backtest")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(help.contains("[default: 5y]"), "{help}");
    }

    #[test]
    fn period_defaults_cover_every_period_command() {
        let names: Vec<String> = period_defaults(&build_cli())
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        for cmd in ["technical", "history", "risk", "backtest", "sip", "deep"] {
            assert!(names.iter().any(|n| n == cmd), "{cmd} takes --period");
        }
        assert!(!names.iter().any(|n| n == "quote"));
    }

    #[test]
    fn value_names_match_handler_strings() {
        assert_eq!(value_name(Strategy::Bb), "bb");
        assert_eq!(value_name(ImportSource::Kite), "kite");
        assert_eq!(value_name(TaxCountry::In), "in");
        assert_eq!(value_name(AlertWhen::Above), "above");
        assert_eq!(value_name(DaemonMode::Longterm), "longterm");
    }
}

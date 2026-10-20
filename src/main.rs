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
mod daemon;
mod display;
mod insights;
mod intraday;
mod longterm;
mod market;
mod portfolio;
mod simulator;
mod store;
mod technical;
mod watchlist;
mod wealth;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use market::Market;

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
stockwise quote AAPL -m us --json     machine-readable output\n\n\
Run `stockwise doctor` to check your setup, `stockwise <command> --help` for details."
)]
struct Cli {
    /// Market for bare tickers: `in` (India, adds .NS — the default) or `us`
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
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    ai: bool,

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
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },

    /// Everything about a stock in one command
    Deep {
        symbol: String,
        #[arg(short, long, default_value = "3mo", value_parser = PERIODS)]
        period: String,
    },

    /// Major market indices (US + India)
    Markets {
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },

    /// Markets + portfolio + watchlist + alerts on one screen
    #[command(alias = "d")]
    Dashboard,

    /// Find a ticker by company name
    Search {
        /// Company name or partial ticker, e.g. "tata motors"
        query: String,
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
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
    },

    /// Price history with a terminal chart and volume bars
    History {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
        /// Print JSON (date, open, high, low, close, volume) instead of a chart
        #[arg(long)]
        json: bool,
    },

    /// Charts for 1W, 1M, 3M and 1Y at once
    Chart { symbol: String },

    /// Compare stocks side by side
    #[command(alias = "c")]
    Compare {
        /// Two or more tickers
        #[arg(required = true, num_args = 2..)]
        symbols: Vec<String>,
    },

    /// Risk metrics: volatility, Sharpe, max drawdown, VaR
    Risk {
        symbol: String,
        /// History used for the metrics (daily bars; needs ~30+ trading days)
        #[arg(short, long, default_value = "1y", value_parser = PERIODS)]
        period: String,
    },

    /// Price correlation matrix between stocks
    #[command(alias = "corr")]
    Correlate {
        /// Two or more tickers
        #[arg(required = true, num_args = 2..)]
        symbols: Vec<String>,
    },

    /// Trend and momentum across timeframes at a glance
    #[command(alias = "mtf")]
    Timeframes { symbol: String },

    /// Returns over 1W, 1M, 3M, 6M, 1Y, 3Y, 5Y
    Returns { symbol: String },

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
    Movers,
    /// Sector performance with bars
    Sectors,
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
    },
    /// Dividend yield, income projection and DRIP growth
    Dividends { symbol: String },

    // ── Strategies & bots ──
    /// Backtest a trading strategy on history
    Backtest {
        symbol: String,
        #[arg(short, long, value_enum, ignore_case = true, default_value = "rsi")]
        strategy: Strategy,
        #[arg(short, long, default_value = "2y", value_parser = PERIODS)]
        period: String,
    },

    /// SIP / DCA simulator — what if you invested X every month
    Sip {
        symbol: String,
        /// Amount invested each month
        amount: f64,
        #[arg(short, long, default_value = "5y", value_parser = PERIODS)]
        period: String,
    },

    /// What if you had invested X on a past date
    Whatif {
        symbol: String,
        /// Amount invested
        amount: f64,
        /// Date in YYYY-MM-DD format
        date: String,
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
    #[command(alias = "p")]
    Portfolio {
        #[command(subcommand)]
        action: Option<PortfolioCmd>,
    },

    /// Manage your watchlist (default: show)
    #[command(alias = "w")]
    Watch {
        #[command(subcommand)]
        action: Option<WatchCmd>,
    },

    /// Price alerts (default: list)
    Alert {
        #[command(subcommand)]
        action: Option<AlertCmd>,
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
    /// Print a shell completion script (bash, zsh, fish, powershell, elvish)
    Completions {
        #[arg(value_enum, ignore_case = true)]
        shell: clap_complete::Shell,
    },
    /// Print the man page, or write one page per command with --dir
    #[command(after_help = "Examples:\n  \
stockwise man > stockwise.1 && man ./stockwise.1\n  \
stockwise man --dir ~/.local/share/man/man1    then: man stockwise-backtest")]
    Man {
        /// Write stockwise.1 and a stockwise-<command>.1 page per command into DIR
        #[arg(long, value_name = "DIR")]
        dir: Option<std::path::PathBuf>,
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
    Show,
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
    Show,
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
    Check,
    /// List alerts
    #[command(alias = "show")]
    List,
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
    History,
    /// Delete all simulator history (a backup is kept)
    Reset {
        /// Don't ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
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
    ("Setup", &["completions", "man", "help"]),
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

/// `stockwise man`: roff rendered from the clap definition, so the pages always
/// match the binary. Release archives ship the `--dir` output under `man/`.
fn cmd_man(dir: Option<&std::path::Path>) -> anyhow::Result<()> {
    use anyhow::Context;
    let Some(dir) = dir else {
        let mut page = Vec::new();
        clap_mangen::Man::new(Cli::command()).render(&mut page)?;
        print!("{}", String::from_utf8_lossy(&page));
        return Ok(());
    };
    std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    clap_mangen::generate_to(Cli::command(), dir)
        .with_context(|| format!("could not write man pages to {}", dir.display()))?;
    println!(
        "Wrote stockwise.1 and one page per command to {}",
        dir.display()
    );
    Ok(())
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
    let matches = build_cli().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let m = cli.market;
    ai::set_auto_ai(cli.ai);

    let result = match cli.command {
        Commands::Quote { symbols, json } => commands::cmd_quote(&symbols, m, json).await,
        Commands::Deep { symbol, period } => commands::cmd_deep(&symbol, &period, m).await,
        Commands::Markets { json } => commands::cmd_markets(m, json).await,
        Commands::Dashboard => commands::cmd_dashboard(m).await,
        Commands::Search { query, json } => commands::cmd_search(&query, json).await,
        Commands::Doctor => commands::cmd_doctor(m).await,

        Commands::Analyze { symbol } => commands::cmd_analyze(&symbol, m).await,
        Commands::Technical { symbol, period } => {
            commands::cmd_technical(&symbol, &period, m).await
        }
        Commands::History {
            symbol,
            period,
            json,
        } => commands::cmd_history(&symbol, &period, m, json).await,
        Commands::Chart { symbol } => commands::cmd_chart(&symbol, m).await,
        Commands::Compare { symbols } => commands::cmd_compare(&symbols, m).await,
        Commands::Risk { symbol, period } => commands::cmd_risk(&symbol, &period, m).await,
        Commands::Correlate { symbols } => commands::cmd_correlate(&symbols, m).await,
        Commands::Timeframes { symbol } => commands::cmd_multitimeframe(&symbol, m).await,
        Commands::Returns { symbol } => commands::cmd_returns(&symbol, m).await,
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

        Commands::Movers => commands::cmd_movers(m).await,
        Commands::Sectors => commands::cmd_sectors(m).await,
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
        Commands::Screen { category } => commands::cmd_screen(&value_name(category), m).await,
        Commands::Dividends { symbol } => commands::cmd_dividends(&symbol, m).await,

        Commands::Backtest {
            symbol,
            strategy,
            period,
        } => commands::cmd_backtest(&symbol, &value_name(strategy), &period, m).await,
        Commands::Sip {
            symbol,
            amount,
            period,
        } => commands::cmd_sip(&symbol, amount, &period, m).await,
        Commands::Whatif {
            symbol,
            amount,
            date,
        } => commands::cmd_whatif(&symbol, amount, &date, m).await,
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
            Some(SimCmd::History) => commands::cmd_sim("history", None, None, m).await,
            None => commands::cmd_sim("help", None, None, m).await,
        },
        Commands::Report { what } => commands::cmd_report(&what, m).await,

        Commands::Portfolio { action } => match action {
            Some(PortfolioCmd::Add {
                symbol,
                shares,
                cost,
            }) => commands::cmd_portfolio("add", Some(&symbol), Some(shares), Some(cost), m).await,
            Some(PortfolioCmd::Remove { symbol }) => {
                commands::cmd_portfolio("remove", Some(&symbol), None, None, m).await
            }
            Some(PortfolioCmd::Show) | None => {
                commands::cmd_portfolio("show", None, None, None, m).await
            }
        },
        Commands::Watch { action } => match action {
            Some(WatchCmd::Add { symbols }) => commands::cmd_watchlist("add", &symbols, m).await,
            Some(WatchCmd::Remove { symbols }) => {
                commands::cmd_watchlist("remove", &symbols, m).await
            }
            Some(WatchCmd::Show) | None => commands::cmd_watchlist("show", &[], m).await,
        },
        Commands::Alert { action } => match action {
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
            Some(AlertCmd::Check) => commands::cmd_alert("check", None, None, None, m).await,
            Some(AlertCmd::List) | None => commands::cmd_alert("list", None, None, None, m).await,
        },
        Commands::Export { what } => commands::cmd_export(&value_name(what), m).await,
        Commands::Import { source, file } => {
            commands::cmd_import(&value_name(source), &file, m).await
        }
        Commands::Wealth => commands::cmd_wealth(m).await,
        Commands::Rebalance => commands::cmd_rebalance(&[], m).await,
        Commands::Harvest => commands::cmd_harvest(m).await,
        Commands::Tax { country } => commands::cmd_tax(&value_name(country), m).await,
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

        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "stockwise",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        Commands::Man { dir } => cmd_man(dir.as_deref()),
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
    fn man_page_lists_every_command() {
        let mut page = Vec::new();
        clap_mangen::Man::new(Cli::command())
            .render(&mut page)
            .unwrap();
        let page = String::from_utf8(page).unwrap();
        for sub in Cli::command().get_subcommands() {
            let reference = format!("stockwise\\-{}(1)", sub.get_name());
            assert!(page.contains(&reference), "man page is missing {reference}");
        }
    }

    #[test]
    fn man_dir_writes_a_page_per_command() {
        let dir = std::env::temp_dir().join(format!("stockwise-man-{}", std::process::id()));
        cmd_man(Some(&dir)).unwrap();
        for page in [
            "stockwise.1",
            "stockwise-quote.1",
            "stockwise-portfolio-add.1",
            "stockwise-trade-buy.1",
        ] {
            assert!(dir.join(page).is_file(), "{page} was not written");
        }
        std::fs::remove_dir_all(&dir).unwrap();
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

    #[test]
    fn value_names_match_handler_strings() {
        assert_eq!(value_name(Strategy::Bb), "bb");
        assert_eq!(value_name(ImportSource::Kite), "kite");
        assert_eq!(value_name(TaxCountry::In), "in");
        assert_eq!(value_name(AlertWhen::Above), "above");
        assert_eq!(value_name(DaemonMode::Longterm), "longterm");
    }
}

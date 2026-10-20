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
mod clock;
mod commands;
mod daemon;
mod derivatives;
mod display;
mod funds;
mod greeks;
mod i18n;
mod insights;
mod intraday;
mod longterm;
mod market;
mod notify;
mod portfolio;
mod providers;
mod simulator;
mod store;
mod tax;
mod technical;
#[cfg(test)]
mod testutil;
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

    /// Add an AI section to analysis commands (local Ollama unless STOCKWISE_AI_PROVIDER picks a cloud model)
    #[arg(
        long,
        global = true,
        env = "STOCKWISE_AI",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    ai: bool,

    /// Language for tables and messages: `en` or `hi` (Hindi). Default: your locale
    #[arg(
        long,
        global = true,
        env = "STOCKWISE_LANG",
        ignore_case = true,
        value_name = "LANG"
    )]
    lang: Option<i18n::Lang>,

    /// Currency bare crypto tickers are quoted in with `-m crypto` (BTC → BTC-INR)
    #[arg(
        long,
        global = true,
        env = "STOCKWISE_QUOTE_CURRENCY",
        default_value = "usd",
        value_name = "CODE",
        value_parser = parse_currency_code
    )]
    quote_currency: String,
    /// Portfolio to use: `default` (portfolio.json) or a name such as `long-term`
    #[arg(
        long,
        global = true,
        env = "STOCKWISE_PORTFOLIO",
        value_name = "NAME",
        value_parser = portfolio_name
    )]
    portfolio: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

fn portfolio_name(s: &str) -> Result<String, String> {
    portfolio::normalize_name(Some(s))
        .map(|n| n.unwrap_or_else(|| "default".into()))
        .map_err(|e| e.to_string())
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
        /// Add the largest cryptocurrencies (trade 24/7)
        #[arg(long)]
        crypto: bool,
        /// Add major currency pairs (USD/INR, EUR/USD, ...)
        #[arg(long)]
        fx: bool,
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

    // ── Funds & F&O ──
    /// Mutual funds (AMFI NAVs): search, nav, history, sip, screen, overlap
    Mf {
        #[command(subcommand)]
        action: MfCmd,
    },

    /// Option chains with greeks, PCR and max pain; `options SYMBOL` shows expected moves
    #[command(
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true,
        after_help = "Examples:\n  \
stockwise options chain NIFTY             NSE index options, nearest expiry\n  \
stockwise options chain AAPL -m us -n 5   US options from Yahoo, 5 strikes each side\n  \
stockwise options pcr BANKNIFTY           put/call ratio and open-interest walls\n  \
stockwise options maxpain RELIANCE -e 2026-10-27\n  \
stockwise options TCS                     expected moves (same as `options moves TCS`)"
    )]
    Options {
        #[command(subcommand)]
        action: Option<OptionsCmd>,
        /// Ticker for expected moves (same as `options moves SYMBOL`)
        #[arg(required = true)]
        symbol: Option<String>,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },

    // ── Strategies & bots ──
    /// Backtest a strategy: one stock, a portfolio, or walk-forward
    Backtest(Box<BacktestArgs>),

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
        /// Add a strategy's entry/exit rule as an extra signal: a built-in, a file in your
        /// strategies folder, or file:PATH.toml (as in `backtest -s`)
        #[arg(short, long)]
        strategy: Option<String>,
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
    /// Track holdings, lots and P&L (default: show)
    #[command(alias = "p")]
    Portfolio {
        /// Currency for totals when holdings mix INR and USD (default: the market's)
        #[arg(long, global = true, value_enum, ignore_case = true)]
        base: Option<BaseCurrency>,
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
        /// Rebuild holdings from a dated file even if that replaces lots or
        /// sales recorded another way (manual entries, another import)
        #[arg(long)]
        replace: bool,
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
    /// Notification channels for alerts and the daemon (default: status)
    Notify {
        #[command(subcommand)]
        action: Option<NotifyCmd>,
    },

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
    /// Add a purchase as a new lot
    Add {
        symbol: String,
        /// Number of shares
        shares: f64,
        /// Price paid per share
        cost: f64,
        /// Purchase date, YYYY-MM-DD (default: today) — used for LTCG/STCG and XIRR
        #[arg(long)]
        date: Option<String>,
    },
    /// Record a sale: oldest lots first (FIFO), realized P&L kept for `tax`
    Sell {
        symbol: String,
        /// Number of shares sold
        shares: f64,
        /// Sale price per share (default: the live price; required with a past --date)
        price: Option<f64>,
        /// Sale date, YYYY-MM-DD (default: today)
        #[arg(long)]
        date: Option<String>,
    },
    /// A holding's lots, sales and dividends; --lot N --date fixes a purchase date
    Lots {
        symbol: String,
        /// Lot number (from the list) whose purchase date to correct
        #[arg(long, requires = "date")]
        lot: Option<usize>,
        /// Correct purchase date for --lot, YYYY-MM-DD
        #[arg(long, requires = "lot")]
        date: Option<String>,
    },
    /// Splits, bonuses and dividends since you bought; --apply records them
    Actions {
        /// Only this holding (default: all)
        symbol: Option<String>,
        /// Adjust lots and record dividends (otherwise only shows them)
        #[arg(long)]
        apply: bool,
    },
    /// Remove a holding and its lots without recording a sale
    #[command(alias = "rm")]
    Remove { symbol: String },
    /// Show holdings with live P&L and XIRR
    Show,
    /// List your portfolios (see --portfolio)
    List,
}

#[derive(Clone, Copy, ValueEnum)]
enum BaseCurrency {
    /// Indian rupee
    Inr,
    /// US dollar
    Usd,
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
    /// Keep checking alerts while their market is open; notify each trigger once
    Watch {
        /// Seconds between checks
        #[arg(long, default_value = "60", value_parser = clap::value_parser!(u64).range(10..))]
        interval: u64,
        /// Check every alert once (any time of day) and exit — for cron
        #[arg(long)]
        once: bool,
    },
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
    /// Settle open sessions from each day's 5-minute bars (stop or target first?)
    Settle {
        /// Settle today's session before the close, at the latest bar
        #[arg(long)]
        now: bool,
    },
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

/// Months for an MF period such as `6mo`, `3y` or `max` (validated here,
/// converted again in the handler).
fn mf_period(s: &str) -> Result<String, String> {
    funds::parse_period(s).map(|_| s.trim().to_lowercase())
}

fn expiry_date(s: &str) -> Result<chrono::NaiveDate, String> {
    derivatives::parse_expiry(s).ok_or_else(|| format!("`{s}`: use YYYY-MM-DD, e.g. 2026-10-27"))
}

#[derive(Subcommand)]
enum MfCmd {
    /// Find schemes by name, fund house or category, e.g. `mf search parag flexi direct`
    Search {
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// Maximum schemes to show
        #[arg(short = 'n', long, default_value_t = 25)]
        limit: usize,
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Latest NAV from AMFI for one or more scheme codes
    Nav {
        /// AMFI scheme codes (from `mf search`)
        #[arg(required = true, num_args = 1..)]
        codes: Vec<u32>,
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// NAV chart, 1M–10Y trailing returns (CAGR from 1Y) and drawdown
    History {
        /// AMFI scheme code
        code: u32,
        /// Chart period: 6mo, 1y, 3y, 5y, 10y, max, ...
        #[arg(short, long, default_value = "1y", value_parser = mf_period)]
        period: String,
        /// Print JSON (returns and every NAV in the period)
        #[arg(long)]
        json: bool,
    },
    /// Monthly SIP into a scheme from NAV history, with XIRR
    Sip {
        /// AMFI scheme code
        code: u32,
        /// Amount invested each month (₹)
        amount: f64,
        /// How far back the SIP starts: 1y, 3y, 5y, 10y, max, ...
        #[arg(short, long, default_value = "5y", value_parser = mf_period)]
        period: String,
        /// Print JSON instead of a chart
        #[arg(long)]
        json: bool,
    },
    /// Rank a category's growth funds by 1Y/3Y/5Y return (bare: list categories)
    Screen {
        /// Category words, e.g. `flexi cap`, `elss`, `liquid`, `equity etf`
        category: Vec<String>,
        /// Which plans to include
        #[arg(long, value_enum, ignore_case = true, default_value = "direct")]
        plan: MfPlan,
        /// Return horizon to rank by
        #[arg(short, long, value_enum, default_value = "3y")]
        sort: MfSort,
        /// Funds to show
        #[arg(short = 'n', long, default_value_t = 15)]
        limit: usize,
        /// Most funds to fetch history for (one request each, cached 12h)
        #[arg(long, default_value_t = 100)]
        max: usize,
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Holdings and sector overlap of two funds/ETFs (Yahoo data: mainly US funds)
    Overlap {
        /// First fund or ETF ticker, e.g. SPY (with -m us)
        a: String,
        /// Second fund or ETF ticker, e.g. QQQ
        b: String,
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum MfPlan {
    /// Direct plans (lower expense ratio)
    Direct,
    /// Regular plans (sold through distributors)
    Regular,
    /// Both
    All,
}

#[derive(Clone, Copy, ValueEnum)]
enum MfSort {
    #[value(name = "1y")]
    Y1,
    #[value(name = "3y")]
    Y3,
    #[value(name = "5y")]
    Y5,
}

#[derive(Subcommand)]
enum OptionsCmd {
    /// Option chain: OI, volume, IV and greeks (NSE for India, Yahoo for -m us)
    Chain {
        /// NIFTY, BANKNIFTY, an F&O stock such as RELIANCE, or a US ticker with -m us
        symbol: String,
        /// Expiry date (default: nearest)
        #[arg(short, long, value_parser = expiry_date)]
        expiry: Option<chrono::NaiveDate>,
        /// Strikes each side of at-the-money (0 = all)
        #[arg(short = 'n', long, default_value_t = 10)]
        strikes: usize,
        /// Risk-free rate in percent for greeks (default 6.5 India, 4.0 US)
        #[arg(long)]
        rate: Option<f64>,
        /// Dividend yield in percent for greeks
        #[arg(long, default_value_t = 0.0)]
        div_yield: f64,
        /// Print JSON instead of tables
        #[arg(long)]
        json: bool,
    },
    /// Put/call ratio by open interest and volume, with the biggest OI strikes
    Pcr {
        symbol: String,
        /// Expiry date (default: nearest)
        #[arg(short, long, value_parser = expiry_date)]
        expiry: Option<chrono::NaiveDate>,
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Max pain: the expiry price at which option buyers collect least
    #[command(alias = "max-pain")]
    Maxpain {
        symbol: String,
        /// Expiry date (default: nearest)
        #[arg(short, long, value_parser = expiry_date)]
        expiry: Option<chrono::NaiveDate>,
        /// Print JSON instead of a chart
        #[arg(long)]
        json: bool,
    },
    /// Expected moves and strike ideas from realized volatility
    Moves {
        symbol: String,
        #[arg(short, long, default_value = "1mo", value_parser = PERIODS)]
        period: String,
    },
}

#[derive(Subcommand)]
enum NotifyCmd {
    /// Show which channels are configured
    #[command(alias = "show")]
    Status,
    /// Send a test message to every configured channel
    Test,
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

/// `backtest SYMBOL` runs one stock; the subcommands run the other forms.
#[derive(Args)]
#[command(
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    after_help = "Examples:\n  \
stockwise backtest INFY -s macd                       one stock, Indian costs, next-open fills\n  \
stockwise backtest TCS -s rsi -P oversold=25 --json   override a parameter, JSON output\n  \
stockwise backtest portfolio TCS INFY HDFCBANK -s sma --rebalance monthly\n  \
stockwise backtest walkforward RELIANCE -s rsi --train 1y --test 3mo\n  \
stockwise backtest strategies                         built-in and your own strategies\n\n\
Strategy files, costs and fills: docs/backtesting.md"
)]
struct BacktestArgs {
    #[command(subcommand)]
    action: Option<BacktestCmd>,
    /// Ticker, e.g. INFY (India) or AAPL with -m us
    #[arg(required = true)]
    symbol: Option<String>,
    #[arg(short, long, default_value = "2y", value_parser = PERIODS)]
    period: String,
    #[command(flatten)]
    flags: BacktestFlags,
}

#[derive(Subcommand)]
enum BacktestCmd {
    /// One strategy across several stocks sharing one pot of capital
    Portfolio {
        /// Tickers, e.g. TCS INFY HDFCBANK
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,
        /// Capital split: equal, inverse-vol, or one weight per symbol (0.5,0.3,0.2)
        #[arg(long, default_value = "equal", value_parser = backtest::engine::Weights::parse)]
        weights: backtest::engine::Weights,
        /// Bring every symbol back to its weight each month or quarter
        #[arg(long, value_enum, ignore_case = true, default_value = "none")]
        rebalance: RebalanceArg,
        #[arg(short, long, default_value = "2y", value_parser = PERIODS)]
        period: String,
        #[command(flatten)]
        flags: BacktestFlags,
    },
    /// Tune parameters on rolling train windows; report only the unseen test windows
    #[command(alias = "wf")]
    Walkforward {
        /// Ticker, e.g. RELIANCE
        symbol: String,
        /// Train window, in trading time: 6mo, 1y, 2y, 90d
        #[arg(long, default_value = "1y", value_parser = backtest::walkforward::Window::parse)]
        train: backtest::walkforward::Window,
        /// Test window after each train window: 1mo, 3mo, 6mo
        #[arg(long, default_value = "3mo", value_parser = backtest::walkforward::Window::parse)]
        test: backtest::walkforward::Window,
        #[arg(short, long, default_value = "5y", value_parser = PERIODS)]
        period: String,
        #[command(flatten)]
        flags: BacktestFlags,
    },
    /// List built-in and your own strategies, or check strategy files for errors
    Strategies {
        /// Strategy files to check instead of listing everything
        files: Vec<String>,
        /// Print JSON instead of a list
        #[arg(long)]
        json: bool,
    },
}

/// Options shared by every form of `backtest`.
#[derive(Args, Clone)]
struct BacktestFlags {
    /// Strategy: rsi, macd, sma, bb, vwap, mr, a file in your strategies folder, or file:PATH.toml
    #[arg(short, long, default_value = "rsi")]
    strategy: String,
    /// Set a strategy parameter, e.g. -P oversold=25 (repeatable)
    #[arg(
        short = 'P',
        long = "param",
        value_name = "NAME=VALUE",
        value_parser = backtest::rules::parse_param_override
    )]
    params: Vec<(String, f64)>,
    /// Trading costs; auto = India delivery for INR stocks, crypto/FX for coins and pairs, US otherwise
    #[arg(long, value_enum, ignore_case = true, default_value = "auto")]
    costs: CostsArg,
    /// Brokerage per order: 20 (flat), 0.03% or 20,0.03% (whichever is lower)
    #[arg(long, value_parser = backtest::costs::Brokerage::parse)]
    brokerage: Option<backtest::costs::Brokerage>,
    /// Slippage per fill, in basis points, 0-1000 [default: 5, or 0 with --costs none]
    #[arg(long, value_name = "BPS", value_parser = slippage_bps)]
    slippage_bps: Option<f64>,
    /// When a signal fills: the next bar's open, or the signal bar's close (look-ahead)
    #[arg(long, value_enum, ignore_case = true, default_value = "next-open")]
    fill: FillArg,
    /// Starting capital, in the stock's currency
    #[arg(long, default_value = "100000", value_parser = capital)]
    capital: f64,
    /// Print JSON instead of a report
    #[arg(long)]
    json: bool,
}

impl From<BacktestFlags> for commands::BacktestOpts {
    fn from(f: BacktestFlags) -> Self {
        use backtest::costs::CostPreset;
        Self {
            strategy: f.strategy,
            params: f.params,
            costs: match f.costs {
                CostsArg::Auto => CostPreset::Auto,
                CostsArg::None => CostPreset::None,
                CostsArg::InDelivery => CostPreset::InDelivery,
                CostsArg::InIntraday => CostPreset::InIntraday,
                CostsArg::Us => CostPreset::Us,
                CostsArg::Crypto => CostPreset::Crypto,
                CostsArg::Fx => CostPreset::Fx,
            },
            brokerage: f.brokerage,
            slippage_bps: f.slippage_bps,
            fill: match f.fill {
                FillArg::NextOpen => backtest::engine::Fill::NextOpen,
                FillArg::Close => backtest::engine::Fill::Close,
            },
            capital: f.capital,
            json: f.json,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CostsArg {
    /// India delivery for INR stocks, US for the rest
    Auto,
    /// No costs or slippage
    None,
    /// NSE delivery: brokerage, STT 0.1% both sides, stamp duty, DP charge
    #[value(alias = "delivery")]
    InDelivery,
    /// NSE intraday: brokerage, STT 0.025% on sells, stamp duty
    #[value(alias = "intraday")]
    InIntraday,
    /// US: commission-free, slippage only
    Us,
    /// Crypto exchanges: 0.1% fee per order, 10 bps slippage
    Crypto,
    /// Spot FX: 2 bps slippage for the spread
    Fx,
}

#[derive(Clone, Copy, ValueEnum)]
enum FillArg {
    /// Next bar's open (no look-ahead)
    NextOpen,
    /// Signal bar's close (how StockWise 0.3 and earlier filled)
    Close,
}

#[derive(Clone, Copy, ValueEnum)]
enum RebalanceArg {
    None,
    Monthly,
    Quarterly,
}

impl From<RebalanceArg> for backtest::engine::Rebalance {
    fn from(r: RebalanceArg) -> Self {
        match r {
            RebalanceArg::None => Self::None,
            RebalanceArg::Monthly => Self::Monthly,
            RebalanceArg::Quarterly => Self::Quarterly,
        }
    }
}

fn capital(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if v > 0.0 && v <= 1e12 => Ok(v),
        _ => Err(format!("`{s}` must be a number above 0 and at most 1e12")),
    }
}

fn slippage_bps(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if (0.0..=1000.0).contains(&v) => Ok(v),
        _ => Err(format!(
            "`{s}` must be from 0 to 1000 basis points (1000 = 10%)"
        )),
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum DaemonMode {
    /// Paper-trading loop during market hours (IST; ET with `-m us`)
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

/// A 3-letter currency code such as `usd` or `INR`, upper-cased.
fn parse_currency_code(value: &str) -> Result<String, String> {
    let code = value.trim().to_ascii_uppercase();
    if code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(code)
    } else {
        Err(format!(
            "`{value}` is not a 3-letter currency code (e.g. usd, inr)"
        ))
    }
}

/// The CLI spelling of a ValueEnum variant (e.g. `CostsArg::InDelivery` → "in-delivery"),
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
    ("Funds & F&O", &["mf", "options"]),
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
    ("Setup", &["notify", "completions", "man", "help"]),
];

/// The catalog entry for a help group title. Titles without one (e.g. a
/// newly added group) show in English.
fn group_msg(english: &str) -> Option<i18n::Msg> {
    use i18n::Msg;
    Some(match english {
        "Start here" => Msg::GroupStartHere,
        "Analysis" => Msg::GroupAnalysis,
        "Market" => Msg::GroupMarket,
        "Strategies & bots" => Msg::GroupStrategies,
        "Funds & F&O" => Msg::GroupFunds,
        "Portfolio" => Msg::GroupPortfolio,
        "Live trading" => Msg::GroupLiveTrading,
        "Setup" => Msg::GroupSetup,
        _ => return None,
    })
}

/// A help group title in the active language.
fn group_title(english: &str) -> &str {
    match group_msg(english) {
        Some(msg) => i18n::t(msg),
        None => english,
    }
}

/// Translated one-line descriptions for the most-used commands; the rest
/// show their English doc comment.
fn localized_about(name: &str) -> Option<&'static str> {
    use i18n::Msg;
    if i18n::lang() == i18n::Lang::En {
        return None;
    }
    let msg = match name {
        "quote" => Msg::AboutQuote,
        "deep" => Msg::AboutDeep,
        "markets" => Msg::AboutMarkets,
        "dashboard" => Msg::AboutDashboard,
        "search" => Msg::AboutSearch,
        "doctor" => Msg::AboutDoctor,
        "portfolio" => Msg::AboutPortfolio,
        "watch" => Msg::AboutWatch,
        "alert" => Msg::AboutAlert,
        "completions" => Msg::AboutCompletions,
        "help" => Msg::HelpPrintHelp,
        _ => return None,
    };
    Some(i18n::t(msg))
}

/// Top-level help with commands grouped by purpose (clap would otherwise list
/// 60 commands flat). Descriptions come from the doc comments above.
fn grouped_help_template(cmd: &clap::Command) -> String {
    use colored::Colorize;
    let mut out = format!(
        "{{before-help}}{{about-with-newline}}\n{} {{usage}}\n",
        i18n::t(i18n::Msg::HelpUsage).bold().underline()
    );
    for (group, names) in GROUPS {
        out.push_str(&format!(
            "\n{}\n",
            format!("{}:", group_title(group)).bold().underline()
        ));
        for name in *names {
            let about = localized_about(name)
                .map(str::to_string)
                .or_else(|| {
                    cmd.find_subcommand(name)
                        .and_then(|c| c.get_about())
                        .map(|a| a.to_string())
                })
                .unwrap_or_else(|| i18n::t(i18n::Msg::HelpPrintHelp).into());
            // Pad before styling so ANSI codes don't break the alignment.
            out.push_str(&format!("  {} {}\n", format!("{:<12}", name).bold(), about));
        }
    }
    out.push_str(&format!(
        "\n{}\n{{options}}{{after-help}}",
        i18n::t(i18n::Msg::HelpOptions).bold().underline()
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
    // The language is needed before parsing: the help text is built from it.
    // args_os: a non-UTF-8 argument is clap's to report, not a panic here.
    let args: Vec<String> = std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let (lang, source) = i18n::detect(&args, |k| std::env::var(k).ok());
    i18n::set_lang(lang, source);
    let matches = build_cli().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let m = cli.market;
    ai::set_auto_ai(cli.ai);
    market::set_quote_currency(&cli.quote_currency);
    if let Err(e) = portfolio::select(cli.portfolio.as_deref()) {
        eprintln!("\n  {} {:#}\n", colored::Colorize::red("Error:"), e);
        std::process::exit(2);
    }

    let result = match cli.command {
        Commands::Quote { symbols, json } => commands::cmd_quote(&symbols, m, json).await,
        Commands::Deep { symbol, period } => commands::cmd_deep(&symbol, &period, m).await,
        Commands::Markets { json, crypto, fx } => commands::cmd_markets(m, json, crypto, fx).await,
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

        Commands::Mf { action } => match action {
            MfCmd::Search { query, limit, json } => {
                commands::cmd_mf_search(&query.join(" "), limit, json).await
            }
            MfCmd::Nav { codes, json } => commands::cmd_mf_nav(&codes, json).await,
            MfCmd::History { code, period, json } => {
                commands::cmd_mf_history(code, &period, json).await
            }
            MfCmd::Sip {
                code,
                amount,
                period,
                json,
            } => commands::cmd_mf_sip(code, amount, &period, json).await,
            MfCmd::Screen {
                category,
                plan,
                sort,
                limit,
                max,
                json,
            } => {
                let plan = match plan {
                    MfPlan::Direct => commands::PlanFilter::Direct,
                    MfPlan::Regular => commands::PlanFilter::Regular,
                    MfPlan::All => commands::PlanFilter::All,
                };
                let months = match sort {
                    MfSort::Y1 => 12,
                    MfSort::Y3 => 36,
                    MfSort::Y5 => 60,
                };
                commands::cmd_mf_screen(&category.join(" "), plan, months, limit, max, json).await
            }
            MfCmd::Overlap { a, b, json } => commands::cmd_mf_overlap(&a, &b, m, json).await,
        },
        Commands::Options {
            action,
            symbol,
            period,
        } => match action {
            Some(OptionsCmd::Chain {
                symbol,
                expiry,
                strikes,
                rate,
                div_yield,
                json,
            }) => {
                commands::cmd_options_chain(&symbol, expiry, strikes, rate, div_yield, m, json)
                    .await
            }
            Some(OptionsCmd::Pcr {
                symbol,
                expiry,
                json,
            }) => commands::cmd_options_pcr(&symbol, expiry, m, json).await,
            Some(OptionsCmd::Maxpain {
                symbol,
                expiry,
                json,
            }) => commands::cmd_options_maxpain(&symbol, expiry, m, json).await,
            Some(OptionsCmd::Moves { symbol, period }) => {
                commands::cmd_options(&symbol, &period, m).await
            }
            None => {
                let symbol = symbol.expect("clap requires SYMBOL without a subcommand");
                commands::cmd_options(&symbol, &period, m).await
            }
        },

        Commands::Backtest(args) => match args.action {
            Some(BacktestCmd::Portfolio {
                symbols,
                weights,
                rebalance,
                period,
                flags,
            }) => {
                commands::cmd_backtest_portfolio(
                    &symbols,
                    weights,
                    rebalance.into(),
                    &period,
                    flags.into(),
                    m,
                )
                .await
            }
            Some(BacktestCmd::Walkforward {
                symbol,
                train,
                test,
                period,
                flags,
            }) => {
                commands::cmd_backtest_walkforward(&symbol, train, test, &period, flags.into(), m)
                    .await
            }
            Some(BacktestCmd::Strategies { files, json }) => {
                commands::cmd_backtest_strategies(&files, json)
            }
            None => {
                let symbol = args
                    .symbol
                    .expect("clap requires a symbol without a subcommand");
                commands::cmd_backtest(&symbol, &args.period, args.flags.into(), m).await
            }
        },
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
        Commands::Intraday {
            amount,
            target,
            strategy,
        } => commands::cmd_intraday(amount, target, strategy.as_deref(), m).await,
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
            Some(SimCmd::Settle { now }) => commands::cmd_sim_settle(now).await,
            Some(SimCmd::History) => commands::cmd_sim("history", None, None, m).await,
            None => commands::cmd_sim("help", None, None, m).await,
        },
        Commands::Report { what } => commands::cmd_report(&what, m).await,

        Commands::Portfolio { base, action } => match action {
            Some(PortfolioCmd::Add {
                symbol,
                shares,
                cost,
                date,
            }) => commands::cmd_portfolio_add(&symbol, shares, cost, date.as_deref(), m).await,
            Some(PortfolioCmd::Sell {
                symbol,
                shares,
                price,
                date,
            }) => commands::cmd_portfolio_sell(&symbol, shares, price, date.as_deref(), m).await,
            Some(PortfolioCmd::Lots { symbol, lot, date }) => {
                commands::cmd_portfolio_lots(&symbol, lot, date.as_deref(), m).await
            }
            Some(PortfolioCmd::Actions { symbol, apply }) => {
                commands::cmd_portfolio_actions(symbol.as_deref(), apply, m).await
            }
            Some(PortfolioCmd::Remove { symbol }) => {
                commands::cmd_portfolio_remove(&symbol, m).await
            }
            Some(PortfolioCmd::List) => commands::cmd_portfolio_list(),
            Some(PortfolioCmd::Show) | None => {
                let base = base.map(|b| value_name(b).to_uppercase());
                commands::cmd_portfolio_show(m, base.as_deref()).await
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
            Some(AlertCmd::Watch { interval, once }) => {
                commands::cmd_alert_watch(interval, once, m).await
            }
            Some(AlertCmd::List) | None => commands::cmd_alert("list", None, None, None, m).await,
        },
        Commands::Export { what } => commands::cmd_export(&value_name(what), m).await,
        Commands::Import {
            source,
            file,
            replace,
        } => commands::cmd_import(&value_name(source), &file, replace, m).await,
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

        Commands::Notify { action } => match action {
            Some(NotifyCmd::Test) => commands::cmd_notify("test").await,
            Some(NotifyCmd::Status) | None => commands::cmd_notify("status").await,
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
        Commands::Man { dir } => cmd_man(dir.as_deref()),
    };

    if let Err(e) = result {
        eprintln!(
            "\n  {} {:#}\n",
            colored::Colorize::red(i18n::t(i18n::Msg::ErrorPrefix)),
            e
        );
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
    fn crypto_fx_lang_and_quote_currency_flags_parse() {
        for args in [
            vec!["stockwise", "quote", "BTC", "-m", "crypto"],
            vec![
                "stockwise",
                "quote",
                "ETH",
                "-m",
                "crypto",
                "--quote-currency",
                "inr",
            ],
            vec!["stockwise", "quote", "USDINR", "-m", "fx"],
            vec!["stockwise", "quote", "USDINR", "-m", "forex"],
            vec!["stockwise", "markets", "--crypto", "--fx"],
            vec!["stockwise", "--lang", "hi", "dashboard"],
            vec!["stockwise", "portfolio", "--lang", "HI"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} should parse");
        }
        assert!(
            Cli::try_parse_from(["stockwise", "quote", "BTC", "--quote-currency", "rupees"])
                .is_err()
        );
        assert!(Cli::try_parse_from(["stockwise", "--lang", "fr", "quote", "TCS"]).is_err());
        let cli =
            Cli::try_parse_from(["stockwise", "quote", "BTC", "--quote-currency", "inr"]).unwrap();
        assert_eq!(cli.quote_currency, "INR");
    }

    #[test]
    fn help_template_translates_group_titles() {
        let template = grouped_help_template(&Cli::command());
        // English unless set: tests never call i18n::set_lang.
        assert!(template.contains("Start here"));
        for (group, _) in GROUPS {
            let msg = group_msg(group).unwrap_or_else(|| panic!("`{group}` has no translation"));
            assert_eq!(msg.text(i18n::Lang::En), *group);
        }
        assert_eq!(group_title("Some new group"), "Some new group");
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
            vec!["stockwise", "intraday", "50000", "1.5", "-s", "rsi"],
            vec!["stockwise", "sim", "start"],
            vec!["stockwise", "tax", "US"],
            vec!["stockwise", "tax", "India"],
            vec!["stockwise", "screen", "Growth"],
            vec!["stockwise", "history", "TCS", "-p", "ytd"],
            vec!["stockwise", "options", "TCS"],
            vec!["stockwise", "options", "AAPL", "-p", "3mo", "-m", "us"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} should parse");
        }
    }

    #[test]
    fn backtest_forms_parse() {
        let parse = |args: &[&str]| Cli::try_parse_from(args).map(|c| c.command);

        // The original single-symbol form still works.
        match parse(&["stockwise", "backtest", "TCS", "-s", "rsi"]) {
            Ok(Commands::Backtest(a)) => {
                assert!(a.action.is_none());
                assert_eq!(a.symbol.as_deref(), Some("TCS"));
                assert_eq!(a.flags.strategy, "rsi");
                assert_eq!(a.period, "2y");
            }
            _ => panic!("backtest TCS -s rsi should parse as a single-symbol backtest"),
        }
        assert!(parse(&["stockwise", "backtest"]).is_err());

        match parse(&[
            "stockwise",
            "backtest",
            "portfolio",
            "TCS",
            "INFY",
            "-s",
            "sma",
            "--weights",
            "0.6,0.4",
            "--rebalance",
            "monthly",
            "--json",
        ]) {
            Ok(Commands::Backtest(a)) => match a.action {
                Some(BacktestCmd::Portfolio {
                    symbols,
                    weights,
                    flags,
                    ..
                }) => {
                    assert_eq!(symbols, ["TCS", "INFY"]);
                    assert_eq!(weights, backtest::engine::Weights::Fixed(vec![0.6, 0.4]));
                    assert!(flags.json);
                }
                _ => panic!("expected backtest portfolio"),
            },
            _ => panic!("backtest portfolio should parse"),
        }
        assert!(parse(&["stockwise", "backtest", "portfolio"]).is_err());

        match parse(&[
            "stockwise",
            "backtest",
            "walkforward",
            "INFY",
            "--train",
            "1y",
            "--test",
            "3mo",
        ]) {
            Ok(Commands::Backtest(a)) => match a.action {
                Some(BacktestCmd::Walkforward { train, test, .. }) => {
                    assert_eq!((train.bars(252.0), test.bars(252.0)), (252, 63));
                }
                _ => panic!("expected backtest walkforward"),
            },
            _ => panic!("backtest walkforward should parse"),
        }

        for args in [
            vec!["stockwise", "backtest", "strategies"],
            vec!["stockwise", "backtest", "strategies", "my.toml", "--json"],
            vec!["stockwise", "backtest", "wf", "TCS"],
            vec![
                "stockwise",
                "backtest",
                "TCS",
                "-s",
                "file:my.toml",
                "-P",
                "oversold=25",
            ],
            vec![
                "stockwise",
                "backtest",
                "TCS",
                "--costs",
                "in-intraday",
                "--fill",
                "close",
            ],
            vec![
                "stockwise",
                "backtest",
                "TCS",
                "--costs",
                "none",
                "--capital",
                "50000",
            ],
            vec!["stockwise", "backtest", "TCS", "--brokerage", "20,0.03%"],
            vec!["stockwise", "backtest", "AAPL", "-m", "us", "--costs", "us"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} should parse");
        }
        for args in [
            vec!["stockwise", "backtest", "TCS", "--costs", "zerodha"],
            vec!["stockwise", "backtest", "TCS", "--fill", "open"],
            vec!["stockwise", "backtest", "TCS", "--capital", "0"],
            vec!["stockwise", "backtest", "TCS", "--capital", "1e300"],
            vec!["stockwise", "backtest", "TCS", "--slippage-bps", "20000"],
            vec!["stockwise", "backtest", "TCS", "--slippage-bps", "-1"],
            vec!["stockwise", "backtest", "TCS", "--brokerage", "lots"],
            vec!["stockwise", "backtest", "TCS", "-P", "oversold"],
            vec![
                "stockwise",
                "backtest",
                "walkforward",
                "TCS",
                "--train",
                "1q",
            ],
            vec![
                "stockwise",
                "backtest",
                "portfolio",
                "A",
                "B",
                "--weights",
                "x",
            ],
            vec!["stockwise", "backtest", "TCS", "portfolio"],
        ] {
            assert!(Cli::try_parse_from(&args).is_err(), "{args:?} should fail");
        }
    }

    #[test]
    fn bots_and_alerts_forms_parse() {
        for args in [
            vec!["stockwise", "sim", "settle"],
            vec!["stockwise", "sim", "settle", "--now"],
            vec!["stockwise", "alert", "watch"],
            vec!["stockwise", "alert", "watch", "--interval", "30", "--once"],
            vec!["stockwise", "notify"],
            vec!["stockwise", "notify", "test"],
            vec!["stockwise", "-m", "us", "daemon", "intraday", "5000"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} should parse");
        }
        // Faster than every 10 seconds would hammer Yahoo.
        assert!(Cli::try_parse_from(["stockwise", "alert", "watch", "--interval", "5"]).is_err());
    }

    #[test]
    fn portfolio_lot_commands_parse() {
        for args in [
            vec![
                "stockwise",
                "portfolio",
                "add",
                "TCS",
                "10",
                "3500",
                "--date",
                "2023-04-12",
            ],
            vec!["stockwise", "portfolio", "sell", "TCS", "5"],
            vec![
                "stockwise",
                "portfolio",
                "sell",
                "TCS",
                "5",
                "3900",
                "--date",
                "2024-01-02",
            ],
            vec!["stockwise", "portfolio", "lots", "TCS"],
            vec![
                "stockwise",
                "portfolio",
                "lots",
                "TCS",
                "--lot",
                "1",
                "--date",
                "2022-01-03",
            ],
            vec!["stockwise", "portfolio", "actions"],
            vec!["stockwise", "portfolio", "actions", "RELIANCE", "--apply"],
            vec!["stockwise", "portfolio", "list"],
            vec!["stockwise", "portfolio", "--base", "usd"],
            vec!["stockwise", "portfolio", "show", "--base", "INR"],
            vec!["stockwise", "--portfolio", "long-term", "portfolio"],
            vec!["stockwise", "tax", "in", "--portfolio", "us"],
            vec!["stockwise", "import", "csv", "tradebook.csv", "--replace"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} should parse");
        }
        // --lot and --date only make sense together.
        assert!(
            Cli::try_parse_from(["stockwise", "portfolio", "lots", "TCS", "--lot", "1"]).is_err()
        );
        assert!(Cli::try_parse_from(["stockwise", "portfolio", "--base", "eur"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "--portfolio", "../x", "portfolio"]).is_err());
    }

    #[test]
    fn portfolio_name_flag_is_normalized() {
        let cli = Cli::try_parse_from(["stockwise", "--portfolio", "Long-Term", "doctor"]).unwrap();
        assert_eq!(cli.portfolio.as_deref(), Some("long-term"));
    }

    #[test]
    fn typos_in_actions_are_rejected() {
        assert!(Cli::try_parse_from(["stockwise", "portfolio", "ad", "TCS", "1", "1"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "screen", "growht"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "trade", "by", "TCS", "1"]).is_err());
    }

    fn options_cmd(args: &[&str]) -> (Option<OptionsCmd>, Option<String>, String) {
        let cli = Cli::try_parse_from(args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
        match cli.command {
            Commands::Options {
                action,
                symbol,
                period,
            } => (action, symbol, period),
            _ => panic!("not the options command"),
        }
    }

    #[test]
    fn options_keeps_the_bare_expected_moves_form() {
        let (action, symbol, period) = options_cmd(&["stockwise", "options", "TCS", "-p", "3mo"]);
        assert!(action.is_none());
        assert_eq!(symbol.as_deref(), Some("TCS"));
        assert_eq!(period, "3mo");

        let (action, ..) = options_cmd(&["stockwise", "options", "moves", "TCS"]);
        assert!(matches!(action, Some(OptionsCmd::Moves { symbol, .. }) if symbol == "TCS"));

        let (action, ..) = options_cmd(&[
            "stockwise",
            "options",
            "chain",
            "NIFTY",
            "-e",
            "2026-10-27",
            "-n",
            "5",
        ]);
        match action {
            Some(OptionsCmd::Chain {
                symbol,
                expiry,
                strikes,
                ..
            }) => {
                assert_eq!(symbol, "NIFTY");
                assert_eq!(expiry, chrono::NaiveDate::from_ymd_opt(2026, 10, 27));
                assert_eq!(strikes, 5);
            }
            _ => panic!("expected options chain"),
        }
        let (action, ..) = options_cmd(&["stockwise", "options", "max-pain", "NIFTY"]);
        assert!(matches!(action, Some(OptionsCmd::Maxpain { .. })));
        let (action, ..) = options_cmd(&["stockwise", "options", "pcr", "AAPL", "--json"]);
        assert!(matches!(action, Some(OptionsCmd::Pcr { json: true, .. })));

        assert!(Cli::try_parse_from(["stockwise", "options"]).is_err());
        assert!(
            Cli::try_parse_from(["stockwise", "options", "chain", "NIFTY", "-e", "27/10"]).is_err()
        );
    }

    #[test]
    fn mf_commands_parse() {
        for args in [
            vec!["stockwise", "mf", "search", "parag", "parikh", "flexi"],
            vec!["stockwise", "mf", "nav", "122639", "120465", "--json"],
            vec!["stockwise", "mf", "history", "122639", "-p", "3y"],
            vec!["stockwise", "mf", "sip", "122639", "5000", "-p", "max"],
            vec!["stockwise", "mf", "screen"],
            vec![
                "stockwise",
                "mf",
                "screen",
                "flexi",
                "cap",
                "--sort",
                "5y",
                "--plan",
                "all",
            ],
            vec!["stockwise", "mf", "overlap", "SPY", "QQQ", "-m", "us"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} should parse");
        }
        assert!(Cli::try_parse_from(["stockwise", "mf", "nav", "abc"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "mf", "history", "1", "-p", "3x"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "mf", "screen", "--sort", "2y"]).is_err());
        assert!(Cli::try_parse_from(["stockwise", "mf"]).is_err());
    }

    #[test]
    fn value_names_match_handler_strings() {
        assert_eq!(value_name(CostsArg::InDelivery), "in-delivery");
        assert_eq!(value_name(FillArg::NextOpen), "next-open");
        assert_eq!(value_name(ImportSource::Kite), "kite");
        assert_eq!(value_name(TaxCountry::In), "in");
        assert_eq!(value_name(AlertWhen::Above), "above");
        assert_eq!(value_name(DaemonMode::Longterm), "longterm");
    }
}

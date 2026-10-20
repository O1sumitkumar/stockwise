# Commands Reference

`stockwise --help` lists every command by group; `stockwise <command> --help`
shows its arguments. This page adds detail.

## Global options

| Option | Env var | Description |
|--------|---------|-------------|
| `-m, --market <in\|us>` | `STOCKWISE_MARKET` | Market for bare tickers. Default `in`: `TCS` → `TCS.NS`. Symbols with a suffix (`RELIANCE.BO`, `VOD.L`), indices (`^NSEI`), FX (`USDINR=X`) and crypto (`BTC-USD`) pass through unchanged |
| `--ai` | `STOCKWISE_AI` | Add a local-AI section to `analyze`, `technical`, `compare`, `screen`, `backtest`, `deep` (slow; see [ai.md](ai.md)) |
| `--portfolio <NAME>` | `STOCKWISE_PORTFOLIO` | Use a named portfolio (`portfolios/NAME.json`) instead of the default `portfolio.json`. Applies to `portfolio`, `import`, `tax`, `wealth`, `dashboard`, `trade holdings` and every other command that reads holdings |

Other environment variables: `STOCKWISE_HOME` (data directory),
`STOCKWISE_REDIS_URL` (cache server, or `off`), `STOCKWISE_MODEL` and
`OLLAMA_HOST` (AI). `NO_COLOR` disables colour; colour is also off when output
is piped.

**Periods** (`-p`): `1d`, `5d`, `1w` (same as `5d`), `1mo`, `3mo`, `6mo`, `ytd`, `1y`, `2y`, `5y`, `10y`, `max`.
Value names (`screen growth`, `tax in`, `-s macd`) are case-insensitive.

**Exit codes**: `0` success, `1` runtime error (unknown symbol, no data,
network), `2` invalid arguments.

## Start here

| Command | Description |
|---------|-------------|
| `quote <SYMBOLS...> [--json]` (`q`) | Price, change, day/52-week range, volume, market cap, P/E, EPS, moving averages. Unknown symbols are named in the error |
| `deep <SYMBOL> [-p PERIOD]` | Everything about one stock: fundamentals, technicals, support/resistance, Fibonacci, risk, gaps, volume, insights. Default `3mo` |
| `markets [--json]` | US + India indices (S&P 500, Dow, NASDAQ, Russell, VIX, NIFTY 50, SENSEX, Bank NIFTY, NIFTY IT) |
| `dashboard` (`d`) | Indices + portfolio P&L + watchlist + triggered alerts; first-run hints when empty |
| `search <QUERY> [--json]` | Find a ticker by company name |
| `doctor` | Check data directory, Yahoo reachability, Redis, Ollama model, broker config |

## Analysis

| Command | Description |
|---------|-------------|
| `analyze <SYMBOL>` (`a`) | Valuation, profitability, growth, analyst ratings, plain-English verdict |
| `technical <SYMBOL> [-p]` (`t`) | RSI, MACD, Bollinger Bands, ATR, VWAP, SMA/EMA, braille charts. Default `3mo` |
| `history <SYMBOL> [-p] [--json]` | Braille price chart, volume bars, last 20 bars. `--json` emits every OHLCV bar. Default `1mo` |
| `chart <SYMBOL>` | 1W, 1M, 3M and 1Y charts at once |
| `compare <SYMBOLS...>` (`c`) | Two or more stocks side by side across 16 metrics |
| `risk <SYMBOL> [-p]` | Annualized volatility, Sharpe/Sortino/Calmar, max drawdown, VaR 95/99% — always from daily bars. Default `1y` |
| `correlate <SYMBOLS...>` (`corr`) | Correlation matrix of daily returns (two or more symbols) |
| `timeframes <SYMBOL>` (`mtf`) | 1M–5Y return, volatility, RSI, SMA/MACD signals |
| `returns <SYMBOL>` | 1W–5Y returns with CAGR |
| `patterns <SYMBOL> [-p]` | Candlestick patterns (doji, hammer, engulfing, morning star) |
| `support <SYMBOL> [-p]` | Pivot points (R1–R3, S1–S3) + Fibonacci levels |
| `fibs <SYMBOL> [-p]` | Fibonacci retracement with a "you are here" marker |
| `volume <SYMBOL> [-p]` (`vol`) | OBV, A/D line, volume ratio, volume bars |
| `gaps <SYMBOL> [-p]` | Filled and unfilled price gaps |
| `stoploss <SYMBOL> [-e PRICE] [-p]` | ATR (1.5x/2x/3x), percentage and Chandelier stops, R:R targets |
| `forecast <SYMBOL> [-d DAYS] [-p]` | Linear trend projection with 95% bands — a projection, not a prediction |
| `options <SYMBOL> [-p]` | Expected 1/5/10/30-day moves and OTM strike ideas from realized volatility |

## Market

| Command | Description |
|---------|-------------|
| `movers` | Top gainers and losers among 30 large caps |
| `sectors` | Sector ETF/index performance with bars |
| `heatmap` | Heatmap of the 30-stock universe |
| `sentiment` | Fear & Greed style gauge from VIX, momentum and trend |
| `news <SYMBOL>` | Headlines collated from Yahoo Finance and Google News |
| `rotation` | Sector rotation vs 50-day average |
| `sectorcmp <SECTOR1> <SECTOR2>` | Two sectors head to head |
| `peers <SYMBOL>` | Same-sector peers on P/E, P/B, market cap |
| `insider <SYMBOL>` | Analyst consensus + insider-related news |
| `ipo` | IPO calendar and news |
| `earnings` | Earnings growth for portfolio and watchlist |
| `surprise` | Earnings momentum — accelerating vs decelerating |
| `matrix` | Quality vs growth quadrants |
| `picks` | Large caps ranked on 8 fundamental + technical factors |
| `screen <CATEGORY>` | `undervalued` (P/E<15), `growth` (forward EPS > trailing), `dividend` (yield>2%), `momentum` (top gainers), `bluechip` (>$100B / >₹5L Cr) |
| `dividends <SYMBOL>` | Yield, income projection, 10-year DRIP growth |

## Strategies & bots

| Command | Description |
|---------|-------------|
| `backtest <SYMBOL> [-s STRATEGY] [-p]` | `rsi` (default), `macd`, `sma`, `bb`, `vwap`, `mr`. Equity curve, alpha, per-trade Sharpe, drawdown. Default `2y`. No commissions or slippage are modelled |
| `sip <SYMBOL> <AMOUNT> [-p]` | Monthly SIP/DCA simulation with value-vs-invested chart and XIRR. Default `5y` |
| `whatif <SYMBOL> <AMOUNT> <YYYY-MM-DD>` | What a lump sum on a past date is worth today |
| `intraday <CAPITAL> <TARGET%>` (`bot`) | 9-strategy intraday ideas with Kelly sizing and dual targets. Signals only |
| `longterm [MONTHLY_BUDGET]` | 6-pillar scoring, Monte Carlo, SIP allocation |
| `daemon intraday [CAPITAL]` | Paper-trading loop, 9:00–15:30 IST: scan, enter, trail stops every 60s, book half at T1, square off 15:15, settle into `sim` history. **Never sends broker orders** |
| `daemon longterm` | Post-market scan: scores, portfolio check, alerts, tax-harvest ideas |
| `sim start [CAPITAL] [TARGET%]` | Start a paper session with the bot's picks (default ₹25,000, 2%) |
| `sim status` / `settle` / `history` | Live P&L / close at current prices / record + confidence rating |
| `sim reset [--yes]` | Delete simulator history after confirmation (a timestamped backup is kept) |
| `report <intraday\|longterm\|portfolio\|SYMBOL>` | AI report via local Ollama |

## Portfolio

Data lives in the directory shown by `stockwise doctor`.

| Command | Description |
|---------|-------------|
| `portfolio [show] [--base inr\|usd]` (`p`) | Holdings with live P&L, XIRR and allocation; holdings without a live price show N/A. Mixed INR/USD holdings are totalled in `--base` (default: the market's currency) |
| `portfolio add <SYMBOL> <SHARES> <COST> [--date YYYY-MM-DD]` | Add a purchase as a new lot (default date: today) |
| `portfolio sell <SYMBOL> <SHARES> [PRICE] [--date YYYY-MM-DD]` | Record a sale, oldest lots first (FIFO). Price defaults to the live price; a past `--date` needs a price |
| `portfolio lots <SYMBOL> [--lot N --date YYYY-MM-DD]` | Lots, sales, dividends and XIRR of one holding; `--lot N --date` corrects a purchase date |
| `portfolio actions [SYMBOL] [--apply]` | Splits, bonuses and dividends since your purchases (from Yahoo). Shows them; `--apply` adjusts lots and records dividends, and is safe to repeat |
| `portfolio list` | Your portfolios: the default one and every named one |
| `portfolio remove <SYMBOL>` | Remove a holding and its lots without recording a sale |
| `watch [show]` / `watch add <SYMBOLS...>` / `watch remove <SYMBOLS...>` (`w`) | Watchlist |
| `alert [list]` / `alert add <SYMBOL> <above\|below> <PRICE>` / `alert check` / `alert remove <N>` | Price alerts |
| `import <kite\|indmoney\|csv> <FILE>` | Import broker holdings or a tradebook; re-importing updates instead of doubling (see below) |
| `export <portfolio\|watchlist>` | Write a CSV to the current directory |
| `wealth` | Save today's portfolio value and show growth (run daily) |
| `rebalance` | Trades to reach equal weight |
| `harvest` | Tax-loss harvesting candidates |
| `tax <in\|us\|uk>` | Capital-gains estimate for the current tax year: realized sales and unrealized lots, each with its own holding period (see below) |
| `divcal` | Dividend ranking and income for portfolio + watchlist |
| `pcorr` | Correlation between holdings, diversification warnings |

### Lots, sales and tax

Every purchase is a lot: quantity, price, date and where it came from
(`manual`, `import:kite`, `broker:angel`, ...). `portfolio sell` takes shares
from the oldest lots first — FIFO, as Indian demat accounts and the IRS
default work — and keeps each slice as a realized sale.

`tax` classifies every lot by its own purchase date (long-term after more than
12 months for India and the US) and shows realized gains (sales this tax year)
separately from unrealized ones (if you sold everything today). For India it
applies the set-off rules: gains and losses of the same kind net first; a
short-term loss is set off against short-term gains, then long-term gains; a
long-term loss only against long-term gains; then ₹1.25 lakh of LTCG is exempt
and STCG is taxed at 20%, LTCG at 12.5%. Losses left over are shown as carry
forward. Holdings in another currency (e.g. US shares in `tax in`) are listed
but not included. It is an estimate — no cess, surcharge, grandfathering or
brought-forward losses.

XIRR (in `portfolio` and `portfolio lots`) is the money-weighted annual return
from each lot's purchase, each sale, recorded dividends and today's value. It
shows `—` with under 30 days of history. The portfolio figure includes closed
positions and converts other currencies at today's rate.

Lots marked `*` have an estimated date: holdings from before lot tracking
(dated when they were added to StockWise) or imported from a file without
dates. Fix one with `portfolio lots SYMBOL --lot N --date YYYY-MM-DD`.

**Corporate actions.** `portfolio actions` lists splits, bonus issues (Yahoo
reports a 1:1 bonus as a 2:1 split) and dividends since each holding's first
purchase. Nothing changes until `--apply`, which multiplies the quantity and
divides the price of lots bought before the split (cost and date unchanged)
and records each dividend on the shares held at its ex-date. Each lot
remembers the splits applied to it, so running it again is a no-op. Indian tax
treats bonus shares as bought at zero cost on the allotment date — check bonus
lots against your tax statement.

**Imports.** A holdings export (Kite, INDmoney) has no purchase dates: existing
lots are kept and only the quantity is matched — more shares add a lot dated
today, fewer shares come out of the oldest lots (not recorded as a sale).
`trade holdings` syncs the same way. A file with a date column (`Date`,
`Purchase Date`, `Buy Date`, `trade_date`, ...) is a list of lots: each
symbol's lots are rebuilt from it. With a buy/sell column (`trade_type`,
`Type`, `Side`) it is a tradebook — such as Zerodha Console's — and sells come
out FIFO as realized sales. A symbol whose sells have no earlier buy in the
file is left unchanged. Dates may be `YYYY-MM-DD`, `DD-MM-YYYY`, `DD/MM/YYYY`
(`MM/DD/YYYY` with `-m us`) or `15-Mar-2024`.

**Upgrading.** Portfolios saved before lot tracking are upgraded on first
load: each holding becomes one lot dated when it was added, and the original
file is kept once as `portfolio.json.bak`.

## Live trading (Angel One)

See [angel-one.md](angel-one.md). Orders are previewed and need a typed `yes`.

| Command | Description |
|---------|-------------|
| `trade setup` / `trade config` | How to connect / create the owner-only credentials file |
| `trade holdings` / `positions` / `orders` | Live holdings (synced into `portfolio`, keeping your lots), open positions, order book |
| `trade buy <SYMBOL> <QTY> [--dry-run] [--yes]` | Market buy on NSE |
| `trade sell <SYMBOL> <QTY> [--dry-run] [--yes]` | Market sell on NSE |
| `trade limit <SYMBOL> <QTY> <PRICE> [--dry-run] [--yes]` | Limit buy on NSE |

## Setup

| Command | Description |
|---------|-------------|
| `completions <bash\|zsh\|fish\|powershell\|elvish>` | Print a completion script, e.g. `stockwise completions zsh > ~/.zfunc/_stockwise` |

## Scripting with `--json`

```bash
stockwise quote TCS INFY --json | jq '.[] | {symbol, regularMarketPrice}'
stockwise history AAPL -m us -p 6mo --json | jq '.bars | map(.close) | max'
stockwise markets --json | jq '.[] | select(.change_percent < -1)'
```

`quote --json` returns Yahoo's quote fields (camelCase). `history --json`
returns `{symbol, currency, period, interval, bars: [{time, open, high, low, close, volume}]}`.

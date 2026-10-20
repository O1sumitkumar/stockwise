# Commands Reference

`stockwise --help` lists every command by group; `stockwise <command> --help`
shows its arguments. This page adds detail.

## Global options

| Option | Env var | Description |
|--------|---------|-------------|
| `-m, --market <in\|us>` | `STOCKWISE_MARKET` | Market for bare tickers. Default `in`: `TCS` → `TCS.NS`. Symbols with a suffix (`RELIANCE.BO`, `VOD.L`), indices (`^NSEI`), FX (`USDINR=X`) and crypto (`BTC-USD`) pass through unchanged |
| `--ai` | `STOCKWISE_AI` | Add a local-AI section to `analyze`, `technical`, `compare`, `screen`, `backtest`, `deep` (slow; see [ai.md](ai.md)) |

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

## Funds & F&O

| Command | Description |
|---------|-------------|
| `mf search <QUERY...> [-n N] [--json]` | Find Indian mutual-fund schemes by words in the name, fund house, category or ISIN; add `direct growth` to narrow. Shows the AMFI scheme code the other `mf` commands take |
| `mf nav <CODES...> [--json]` | Latest NAV and date, fund house, category, ISINs |
| `mf history <CODE> [-p PERIOD] [--json]` | NAV chart for the period (default `1y`), trailing 1M–10Y returns (annualized from 1Y), since-inception CAGR, max drawdown. `--json` includes every NAV in the period |
| `mf sip <CODE> <AMOUNT> [-p PERIOD] [--json]` | Monthly SIP from NAV history (default `5y`): value-vs-invested chart, gain, units, and XIRR on the actual purchase dates |
| `mf screen [CATEGORY...] [--plan direct\|regular\|all] [-s 1y\|3y\|5y] [-n N] [--max N] [--json]` | Bare: list categories with their active scheme counts. With words (`flexi cap`, `elss`, `liquid`, `equity etf`): rank the category's growth-option schemes by return (default 3Y CAGR, direct plans) |
| `mf overlap <A> <B> [--json]` | Shared top holdings, holdings and sector overlap, and expense ratios of two funds or ETFs, e.g. `mf overlap SPY QQQ -m us` |
| `options chain <SYMBOL> [-e DATE] [-n STRIKES] [--rate %] [--div-yield %] [--json]` | Calls and puts around the money: last price, open interest (and change on NSE), volume, IV, delta, gamma, theta, vega, rho. Default: nearest expiry, 10 strikes each side (`-n 0` for all) |
| `options pcr <SYMBOL> [-e DATE] [--json]` | Put/call ratio by open interest and by volume, and the three strikes with the most call and put open interest |
| `options maxpain <SYMBOL> [-e DATE] [--json]` (`max-pain`) | The settlement strike at which open options pay their holders least, with the payout curve around it |
| `options <SYMBOL> [-p]` / `options moves <SYMBOL> [-p]` | Expected 1/5/10/30-day moves and OTM strike ideas from realized volatility |

MF periods (`-p`) are `<n>mo`, `<n>y` or `max`, e.g. `6mo`, `3y`, `10y`. Expiries
(`-e`) are `YYYY-MM-DD`.

**Where the data comes from, and what has none**

- **NAVs**: AMFI's public [NAVAll.txt](https://portal.amfiindia.com/spages/NAVAll.txt),
  every Indian scheme, cached in the data directory (`cache/`) and re-downloaded
  after 12 hours. NAV history: [api.mfapi.in](https://www.mfapi.in), a free
  community mirror of AMFI data, cached the same way. If a download fails, a
  stale cached copy is used with a warning on stderr.
- **Returns** are point-to-point NAV returns of the growth option, before tax
  and exit load. `mf sip` buys at the first NAV on or after each monthly due date.
- **Expense ratios (TER)** of Indian schemes are not in AMFI's NAV file, so
  `mf screen` cannot rank by them; compare TERs on the AMC's site. `mf overlap`
  shows Yahoo's expense ratio for the funds it covers.
- **Holdings** (`mf overlap`) come from Yahoo Finance, which publishes the top 10
  holdings and sector weights for US ETFs and mutual funds (SPY, QQQ, VTI,
  VFIAX, ... with `-m us`) and for some funds under Yahoo's `0P…` IDs. NSE-listed
  ETFs such as NIFTYBEES and AMFI scheme codes have no holdings data — StockWise
  says so instead of guessing. With only the top 10 known, the holdings overlap
  is a lower bound; sector weights cover the whole fund.
- **Option chains**: in the Indian market (the default), NSE's public
  option-chain API — index options on NIFTY, BANKNIFTY, FINNIFTY, MIDCPNIFTY and
  NIFTYNXT50 (`^NSEI` and `^NSEBANK` work too) and NSE F&O stocks such as
  RELIANCE. NSE serves browser sessions only: StockWise loads its option-chain
  page once for cookies, makes two API calls, and stops with a clear error if NSE
  refuses — wait a minute and retry. BSE options (SENSEX, BANKEX) are not
  supported. With `-m us`, Yahoo Finance (delayed quotes) for US stocks and ETFs.
- **IV and greeks**: Black-Scholes-Merton. IV is solved from the bid/ask mid
  (else the last trade) at the data timestamp; when that fails, the source's IV
  is shown (`iv_from` in JSON). Risk-free rate defaults to 6.5% (India) / 4.0%
  (US) — pass `--rate` with the current T-bill yield. Theta is per calendar day,
  vega and rho per percentage point. NSE options are European; US equity options
  are American, so their greeks are the usual European approximation.
- **PCR and max pain** describe positioning for one expiry; they are not
  forecasts.

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
| `portfolio [show]` (`p`) | Holdings with live P&L and allocation; holdings without a live price show N/A |
| `portfolio add <SYMBOL> <SHARES> <COST>` | Add shares (averages into an existing holding) |
| `portfolio remove <SYMBOL>` | Remove a holding |
| `watch [show]` / `watch add <SYMBOLS...>` / `watch remove <SYMBOLS...>` (`w`) | Watchlist |
| `alert [list]` / `alert add <SYMBOL> <above\|below> <PRICE>` / `alert check` / `alert remove <N>` | Price alerts |
| `import <kite\|indmoney\|csv> <FILE>` | Import broker holdings; re-importing updates instead of doubling |
| `export <portfolio\|watchlist>` | Write a CSV to the current directory |
| `wealth` | Save today's portfolio value and show growth (run daily) |
| `rebalance` | Trades to reach equal weight |
| `harvest` | Tax-loss harvesting candidates |
| `tax <in\|us\|uk>` | Capital-gains estimate for the current tax year |
| `divcal` | Dividend ranking and income for portfolio + watchlist |
| `pcorr` | Correlation between holdings, diversification warnings |

## Live trading (Angel One)

See [angel-one.md](angel-one.md). Orders are previewed and need a typed `yes`.

| Command | Description |
|---------|-------------|
| `trade setup` / `trade config` | How to connect / create the owner-only credentials file |
| `trade holdings` / `positions` / `orders` | Live holdings (synced into `portfolio`), open positions, order book |
| `trade buy <SYMBOL> <QTY> [--dry-run] [--yes]` | Market buy on NSE |
| `trade sell <SYMBOL> <QTY> [--dry-run] [--yes]` | Market sell on NSE |
| `trade limit <SYMBOL> <QTY> <PRICE> [--dry-run] [--yes]` | Limit buy on NSE |

## Setup

| Command | Description |
|---------|-------------|
| `completions <bash\|zsh\|fish\|powershell\|elvish>` | Print a completion script, e.g. `stockwise completions zsh > ~/.zfunc/_stockwise` |
| `man [--dir <DIR>]` | Print the `stockwise.1` man page, or write it plus one `stockwise-<command>.1` page per command into `DIR`, e.g. `stockwise man --dir ~/.local/share/man/man1` |

## Scripting with `--json`

```bash
stockwise quote TCS INFY --json | jq '.[] | {symbol, regularMarketPrice}'
stockwise history AAPL -m us -p 6mo --json | jq '.bars | map(.close) | max'
stockwise markets --json | jq '.[] | select(.change_percent < -1)'
stockwise mf sip 122639 5000 -p 3y --json | jq '{invested, value, xirr}'
stockwise options chain NIFTY -n 2 --json | jq '.rows[] | {strike, delta: .call.greeks.delta}'
```

`quote --json` returns Yahoo's quote fields (camelCase). `history --json`
returns `{symbol, currency, period, interval, bars: [{time, open, high, low, close, volume}]}`.
Fractions in `mf` and `options` JSON are decimals (`0.1234` = 12.34%); missing
values are `null`, never 0.

# Commands Reference

`stockwise --help` lists every command by group; `stockwise <command> --help`
shows its arguments. This page adds detail.

## Global options

| Option | Env var | Description |
|--------|---------|-------------|
| `-m, --market <in\|us\|crypto\|fx>` | `STOCKWISE_MARKET` | Market for bare tickers. Default `in`: `TCS` → `TCS.NS`. `crypto`: `BTC` → `BTC-USD`. `fx` (or `forex`): `USDINR`, `usd/inr` → `USDINR=X`. Symbols with a suffix (`RELIANCE.BO`, `VOD.L`), indices (`^NSEI`), FX (`USDINR=X`) and crypto pairs (`BTC-INR`) pass through unchanged |
| `--quote-currency <CODE>` | `STOCKWISE_QUOTE_CURRENCY` | Currency bare crypto tickers are quoted in with `-m crypto`. Default `usd`; `inr` gives `BTC-INR` |
| `--lang <en\|hi>` | `STOCKWISE_LANG` | Language for help groups, tables and messages. Default: your locale (`LC_ALL` / `LC_MESSAGES` / `LANG` starting with `hi` picks Hindi), else English. `--json` and `--csv` output is never translated |
| `--ai` | `STOCKWISE_AI` | Add an AI section to `analyze`, `technical`, `compare`, `screen`, `backtest`, `deep` (slow with a local model; see [ai.md](ai.md)) |
| `--no-ai` | | Turn the AI section off for one run, even if `STOCKWISE_AI` or the config file turn it on |
| `--portfolio <NAME>` | `STOCKWISE_PORTFOLIO` | Use a named portfolio (`portfolios/NAME.json`) instead of the default `portfolio.json`. Applies to `portfolio`, `import`, `tax`, `wealth`, `dashboard`, `trade holdings` and every other command that reads holdings |

Other environment variables: `STOCKWISE_HOME` (data directory),
`STOCKWISE_REDIS_URL` (cache server, or `off`), `STOCKWISE_DATA_PROVIDER`
and its API key ([data-providers.md](data-providers.md)),
`STOCKWISE_AI_PROVIDER`, `STOCKWISE_MODEL`, `OLLAMA_HOST` and the cloud AI keys
([ai.md](ai.md)), `STOCKWISE_CONFIG` (config file path), `STOCKWISE_BIND` and
`STOCKWISE_API_TOKEN` (`serve`), and the notification variables
([bots.md](bots.md#notifications)). `NO_COLOR` disables colour; colour is also
off when output is piped.

Defaults for most of these, and for each command's `-p`, can live in a
[config file](#configuration-file). Precedence: command-line flag >
environment variable > config file > built-in default. API keys and the server
token are read from the environment only.

**Crypto and FX**: prices show with the precision they need (FX pairs 4
decimals, coins under 1 up to 8) and `quote` notes trading hours (crypto 24/7;
FX 24 hours Monday–Friday, closed at weekends). Commands built on exchange
sessions, sectors or fundamentals — `sectors`, `rotation`, `sectorcmp`,
`screen`, `picks`, `matrix`, `surprise`, `peers`, `intraday`, `longterm`,
`daemon`, `sim start` — refuse `-m crypto` / `-m fx` with an explanation;
`movers`, `heatmap`, `history`, `technical`, `backtest` and friends work.

**Hindi** (`--lang hi`): help group titles and the main command descriptions,
`quote`, `markets`, `dashboard`, `portfolio`, `search` and `doctor` labels,
and common errors. Other commands are still English. Indian amounts keep
lakh/crore grouping (`₹16.27 लाख करोड़`).

**Periods** (`-p`): `1d`, `5d`, `1w` (same as `5d`), `1mo`, `3mo`, `6mo`, `ytd`, `1y`, `2y`, `5y`, `10y`, `max`.
Value names (`screen growth`, `tax in`, `-s macd`) are case-insensitive.

**Exit codes**: `0` success, `1` runtime error (unknown symbol, no data,
network, invalid config file), `2` invalid arguments.

**Machine-readable output**: commands marked `[--json]` print JSON, and
`[--json\|--csv]` print JSON or CSV, instead of a table. See
[Scripting](#scripting-with---json-and---csv).

## Start here

| Command | Description |
|---------|-------------|
| `quote <SYMBOLS...> [--json\|--csv]` (`q`) | Price, change, day/52-week range, volume, market cap, P/E, EPS, moving averages. Unknown symbols are named in the error |
| `deep <SYMBOL> [-p PERIOD]` | Everything about one stock: fundamentals, technicals, support/resistance, Fibonacci, risk, gaps, volume, insights. Default `3mo` |
| `markets [--crypto] [--fx] [--json\|--csv]` | US + India indices (S&P 500, Dow, NASDAQ, Russell, VIX, NIFTY 50, SENSEX, Bank NIFTY, NIFTY IT); `--crypto` adds the largest coins, `--fx` major pairs (USD/INR, EUR/USD, ...). With `-m crypto` / `-m fx` that section comes first. `--json` rows carry `region`: `Indian Markets`, `US Markets`, `Crypto` or `FX` |
| `dashboard` (`d`) | Indices + portfolio P&L + watchlist + triggered alerts; first-run hints when empty |
| `search <QUERY> [--json\|--csv]` | Find a ticker by company name |
| `doctor` | Check the config file, market and language, data directory, Yahoo reachability, the data provider that answered, Redis, AI provider and model (never the key), broker config. Exits 1 if market data is unreachable or the config file is invalid |

## Analysis

| Command | Description |
|---------|-------------|
| `analyze <SYMBOL>` (`a`) | Valuation, profitability, growth, analyst ratings, plain-English verdict |
| `technical <SYMBOL> [-p] [--json]` (`t`) | RSI, MACD, Bollinger Bands, ATR, VWAP, SMA/EMA, braille charts. Default `3mo` |
| `history <SYMBOL> [-p] [--json\|--csv]` | Braille price chart, volume bars, last 20 bars. `--json`/`--csv` emit every OHLCV bar. Default `1mo` |
| `chart <SYMBOL>` | 1W, 1M, 3M and 1Y charts at once |
| `compare <SYMBOLS...> [--json\|--csv]` (`c`) | Two or more stocks side by side across 16 metrics |
| `risk <SYMBOL> [-p] [--json]` | Annualized volatility, Sharpe/Sortino/Calmar, max drawdown, VaR 95/99% — always from daily bars. Default `1y` |
| `correlate <SYMBOLS...> [--json\|--csv]` (`corr`) | Correlation matrix of daily returns (two or more symbols) |
| `timeframes <SYMBOL>` (`mtf`) | 1M–5Y return, volatility, RSI, SMA/MACD signals |
| `returns <SYMBOL> [--json\|--csv]` | 1W–5Y returns with CAGR |
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
| `movers [--json\|--csv]` | Top gainers and losers among 30 large caps |
| `sectors [--json\|--csv]` | Sector ETF/index performance with bars |
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
| `screen <CATEGORY> [--json\|--csv]` | `undervalued` (P/E<15), `growth` (forward EPS > trailing), `dividend` (yield>2%), `momentum` (top gainers), `bluechip` (>$100B / >₹5L Cr) |
| `dividends <SYMBOL> [--json]` | Yield, income projection, 10-year DRIP growth |

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
| `backtest <SYMBOL> [-s STRATEGY] [-P NAME=VALUE] [-p] [--json\|--csv]` | Backtest one stock after costs: net and gross return, buy & hold, CAGR, drawdown, Sharpe/Sortino, exposure, trades. Fills at the next bar's open. Strategies: `rsi` (default), `macd`, `sma`, `bb`, `vwap`, `mr`, or your own TOML file. Default `2y` |
| `backtest portfolio <SYMBOLS...> [-s] [--weights equal\|inverse-vol\|0.5,0.3,...] [--rebalance none\|monthly\|quarterly] [--json\|--csv]` | One strategy across several stocks sharing the capital; combined equity curve and per-symbol contribution |
| `backtest walkforward <SYMBOL> [-s] [--train 1y] [--test 3mo] [--json\|--csv]` (`wf`) | Tunes the strategy's `[grid]` on rolling train windows and reports only the unseen test windows, with an overfitting verdict. Default `5y` |
| `backtest strategies [FILES...] [--json]` | List built-in and your own strategies, or check strategy files (exit 1 on errors) |
| `sip <SYMBOL> <AMOUNT> [-p] [--json]` | Monthly SIP/DCA simulation with value-vs-invested chart and XIRR. Default `5y` |
| `whatif <SYMBOL> <AMOUNT> <YYYY-MM-DD> [--json]` | What a lump sum on a past date is worth today |
| `intraday <CAPITAL> <TARGET%> [-s STRATEGY]` (`bot`) | 9-strategy intraday ideas with Kelly sizing and dual targets. `-s` adds a strategy file's rules as a tenth signal. Signals only |
| `longterm [MONTHLY_BUDGET]` | 6-pillar scoring, Monte Carlo, SIP allocation |
| `daemon intraday [CAPITAL]` | Paper-trading loop on the NSE clock (IST, holidays included; `-m us` uses NYSE hours in ET): scan, enter at the open, trail stops every 60s, book half at T1, square off 15 minutes before the close, settle into `sim` history. Survives network blips, resumes the day after a restart, and squares off on Ctrl+C. **Never sends broker orders** |
| `daemon longterm` | Post-market scan: scores, portfolio check, alerts, tax-harvest ideas |
| `sim start [CAPITAL] [TARGET%]` | Start a paper session with the bot's picks (default ₹25,000, 2%) on a trading day before the close |
| `sim status` / `history [--json]` | Live P&L / record + confidence rating |
| `sim settle [--now]` | Settle open sessions from that day's 5-minute bars: stop or target first, else exit at the 15:15 bar. Today's session waits for the close unless `--now` |
| `sim reset [--yes]` | Delete simulator history after confirmation (a timestamped backup is kept) |
| `sim replay list [--json]` (`scenarios`) | Built-in historical replays: `covid-crash-2020`, `gfc-2008`, `adani-2023`, `dotcom-2000`, `bull-run-2021`, `rate-hikes-2022`. See [simulator.md](simulator.md) |
| `sim replay start SCENARIO\|file:PATH.toml [--capital] [--costs] [--json]` | Start a replay: daily history is fetched once, then you trade day by day and only ever see bars up to the replay date |
| `sim replay start --from DATE --to DATE --symbols A,B,... [--benchmark SYM]` | Your own replay window in the active market (`-m in`/`-m us`); benchmark NIFTY 50 / S&P 500 by default |
| `sim replay buy SYMBOL QTY [--stop P] [--target P]` / `sell SYMBOL [QTY]` | Queue an order for the next day's open (backtest cost models; long only). `sell` without a quantity sells the whole position |
| `sim replay next [N]` / `status` / `chart SYMBOL\|index [--days]` | Move N trading days (stops and targets on each day's range); account, positions, orders and the market on the replay date; a chart up to it |
| `sim replay cancel [ID]` / `set SYMBOL [--stop P] [--target P]` | Cancel pending orders; move a position's stop or target |
| `sim replay end [--json]` | Finish: 0–100 score and grade, badges, streaks, and what actually happened |
| `sim challenge list\|start NAME\|status\|end [--json]` | Goal-based challenges with hard and soft rules over the running replay or your live paper sessions: `beat-the-index`, `steady-hands`, `position-limit`, `stop-loss`, `sip-discipline`, `crash-survivor`, or `file:PATH.toml` |
| `sim profile [--json]` | Badges (earned and how to earn the rest), streaks, best score per scenario, recent runs |
| `sim card [--svg FILE] [--text]` | Your last run as a text card, or a 640 × 360 SVG image — with the paper-trading disclaimer |
| `report <intraday\|longterm\|portfolio\|SYMBOL>` | AI report via local Ollama |

Every `backtest` form also takes:

| Option | Default | |
|--------|---------|---|
| `-s, --strategy` | `rsi` | A built-in, a file in your strategies folder, or `file:PATH.toml` |
| `-P, --param NAME=VALUE` | | Override a strategy parameter (repeatable) |
| `--costs` | `auto` | `auto`, `in-delivery`, `in-intraday`, `us`, `crypto`, `fx`, `none`. `auto` = crypto/FX models for coins and pairs, India delivery for INR stocks, US otherwise |
| `--brokerage` | preset | Per order: `20` (flat), `0.03%`, or `20,0.03%` (whichever is lower) |
| `--slippage-bps` | `5` | Slippage per fill, in basis points |
| `--fill` | `next-open` | `next-open` or `close` (fills at the signal bar's close — look-ahead) |
| `--capital` | `100000` | Starting capital in the stock's currency |
| `--json` / `--csv` | | Print the whole result as JSON, or the trades as CSV |

Costs, fills, the metrics, walk-forward and the strategy file format are in
[backtesting.md](backtesting.md).

## Portfolio

Data lives in the directory shown by `stockwise doctor`.

| Command | Description |
|---------|-------------|
| `portfolio [show] [--base inr\|usd] [--json\|--csv]` (`p`) | Holdings with live P&L, XIRR and allocation; holdings without a live price show N/A. Mixed INR/USD holdings are totalled in `--base` (default: the market's currency) |
| `portfolio add <SYMBOL> <SHARES> <COST> [--date YYYY-MM-DD]` | Add a purchase as a new lot (default date: today) |
| `portfolio sell <SYMBOL> <SHARES> [PRICE] [--date YYYY-MM-DD]` | Record a sale, oldest lots first (FIFO). Price defaults to the live price; a past `--date` needs a price. Refused while a split or bonus before the sale isn't applied to your lots |
| `portfolio lots <SYMBOL> [--lot N --date YYYY-MM-DD]` | Lots, sales, dividends and XIRR of one holding; `--lot N --date` corrects a purchase date |
| `portfolio actions [SYMBOL] [--apply]` | Splits, bonuses and dividends since your purchases, including positions you have sold (from Yahoo). Shows them; `--apply` adjusts lots and records dividends, and is safe to repeat |
| `portfolio list` | Your portfolios: the default one and every named one |
| `portfolio remove <SYMBOL>` | Remove a holding and its lots without recording a sale |
| `watch [show] [--json\|--csv]` / `watch add <SYMBOLS...>` / `watch remove <SYMBOLS...>` (`w`) | Watchlist |
| `alert [list] [--json\|--csv]` / `alert add <SYMBOL> <above\|below> <PRICE>` / `alert check [--json\|--csv]` / `alert remove <N>` | Price alerts |
| `alert watch [--interval 60] [--once]` | Check alerts while their market is open and notify each trigger once (see [notifications](bots.md#notifications)). `--once` checks every alert once and exits, for cron |
| `import <kite\|indmoney\|groww\|upstox\|cas\|csv> <FILE> [--replace]` | Import broker holdings, a lot list, a tradebook or a CAS (casparser JSON/CSV); re-importing updates instead of doubling (see below and [brokers.md](brokers.md#importing-holdings)) |
| `export <portfolio\|watchlist>` | Write a CSV to the current directory |
| `wealth` | Save today's portfolio value and show growth (run daily) |
| `rebalance` | Trades to reach equal weight |
| `harvest` | Tax-loss harvesting candidates |
| `tax <in\|us\|uk> [--json\|--csv]` | Capital-gains estimate for the current tax year: realized sales and unrealized lots, each with its own holding period (see below) |
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
`trade holdings` syncs the same way. When the snapshot has more shares and a
split or bonus isn't applied to your lots yet, that holding is skipped until
you run `portfolio actions --apply` (otherwise the bonus shares would be
counted twice). A file with a date column (`Date`, `Purchase Date`,
`Buy Date`, `trade_date`, ...) is a list of lots: each symbol's lots are
rebuilt from it. With a buy/sell column (`trade_type`, `Type`, `Side`) it is a
tradebook — such as Zerodha Console's — sells come out FIFO as realized sales,
and splits and bonuses are applied as they happened (this needs a network
connection). A symbol is left unchanged when its sells have no earlier buy in
the file, or when it already has lots or sales from elsewhere (manual entries,
another import) — `--replace` rebuilds it from the file anyway. Dates may be
`YYYY-MM-DD`, `DD-MM-YYYY`, `DD/MM/YYYY` (`MM/DD/YYYY` with `-m us`) or
`15-Mar-2024`; a file with an unrecognized date is refused.

**Groww, Upstox, CAS.** Groww and Upstox exports are snapshots (quantity
sync, like Kite). A CAS whose transactions cover a fund's whole balance
rebuilds it as dated lots, with redemptions as realized sales; otherwise only
the quantity is synced. See [brokers.md](brokers.md#importing-holdings).

**Upgrading.** Portfolios saved before lot tracking are upgraded on first
load: each holding becomes one lot dated when it was added, and the original
file is kept once as `portfolio.json.bak`.

## Interfaces

Both are read-only and share the data layer with the commands above, so they
show the same numbers. They are on by default and can be left out of a build
with `--no-default-features` (cargo features `tui` and `server`).

| Command | Description |
|---------|-------------|
| `tui [-i SECONDS]` | Full-screen live dashboard: indices, watchlist, portfolio P&L, alerts and a chart of the selected symbol. Refreshes every 30s by default (10-3600). `?` shows the keys. See [tui.md](tui.md) |
| `serve [--bind ADDR] [--allow-remote] [--open]` | Web dashboard at `/` (`--open` opens it in your browser), JSON API (`/api/quote`, `/api/history/{symbol}`, `/api/markets`, `/api/search`, `/api/portfolio`, `/api/watchlist`, `/api/alerts`), `/healthz` and Prometheus `/metrics`. Listens on `127.0.0.1:8787`; other addresses need `--allow-remote` and `STOCKWISE_API_TOKEN`. See [server.md](server.md) |

## Live trading

See [brokers.md](brokers.md). Orders are previewed and need a typed `yes`.
Every `trade` command takes `--broker angel|zerodha|upstox|dhan|fyers`
(default `angel`, or `STOCKWISE_BROKER`); all but `angel` are experimental —
untested against a live account.

| Command | Description |
|---------|-------------|
| `trade setup` / `trade config [--keyring]` | How to connect / create the owner-only credentials file, move secrets into the OS keyring |
| `trade login` | Daily login (Zerodha, Upstox, Fyers) or paste a Dhan token |
| `trade holdings` / `positions` / `orders` | Live holdings (synced into `portfolio`, keeping your lots), open positions, order book |
| `trade buy <SYMBOL> <QTY> [--limit P] [--trigger T] [--dry-run] [--yes]` | Buy on NSE: MARKET, LIMIT, SL-M (`--trigger`) or SL (both) |
| `trade sell <SYMBOL> <QTY> [--limit P] [--trigger T] [--dry-run] [--yes]` | Sell on NSE, same order types |
| `trade limit <SYMBOL> <QTY> <PRICE> [--dry-run] [--yes]` | Limit buy on NSE |
| `trade gtt <buy\|sell> <SYMBOL> <QTY> --trigger T --limit P [--dry-run] [--yes]` | Single-leg GTT (not Upstox) |
| `trade gtt list` / `trade gtt cancel <ID>` | List / cancel GTTs (cancel asks for confirmation) |

## Setup

| Command | Description |
|---------|-------------|
| `config [show] [--json]` | Effective settings and where each comes from (flag, env, config file, default), plus the config file's status |
| `config path` | Print the config file path (it may not exist yet) |
| `config init [--force]` | Write a commented template to the config file path; refuses to replace an existing file without `--force` |
| `notify [status]` / `notify test` | Show the configured notification channels (Telegram, Slack, Discord, email, desktop) / send a test message. See [notifications](bots.md#notifications) |
| `completions <bash\|zsh\|fish\|powershell\|elvish>` | Print a completion script, e.g. `stockwise completions zsh > ~/.zfunc/_stockwise` |
| `man [--dir <DIR>]` | Print the `stockwise.1` man page, or write it plus one `stockwise-<command>.1` page per command into `DIR`, e.g. `stockwise man --dir ~/.local/share/man/man1` |

## Scripting with `--json` and `--csv`

```bash
stockwise portfolio --json | jq '.totals'
stockwise portfolio --json | jq -r '.holdings | sort_by(.pnl_percent) | .[:3][] | "\(.symbol) \(.pnl_percent)%"'
stockwise portfolio --csv > portfolio.csv                 # open in a spreadsheet
stockwise history TCS -p 1y --csv > tcs.csv               # every daily bar
stockwise alert check --json | jq -r '.[] | select(.status == "triggered") | .symbol'
stockwise screen dividend --csv | column -s, -t
stockwise tax in --json | jq '.summary.total_tax'
for s in rsi macd sma bb vwap mr; do
  stockwise backtest INFY -s "$s" --json | jq -r '"\(.strategy)\t\(.total_return_percent)"'
done
for s in $(stockwise watch --json | jq -r '.[].symbol'); do
  stockwise technical "$s" --json | jq -r '"\(.symbol)\t\(.rsi_14)\t\(.bias.verdict)"'
done
stockwise quote TCS INFY --json | jq '.[] | {symbol, regularMarketPrice}'
stockwise markets --json | jq '.[] | select(.change_percent < -1)'
stockwise mf sip 122639 5000 -p 3y --json | jq '{invested, value, xirr}'
stockwise options chain NIFTY -n 2 --json | jq '.rows[] | {strike, delta: .call.greeks.delta}'
```

**Rules**

- stdout holds only the JSON or CSV; errors go to stderr with exit code 1, so
  `set -e` and `||` work. `--json` and `--csv` can't be combined.
- Empty states are not errors: an empty portfolio prints `{"holdings": [], ...}`
  or a CSV header with no rows.
- Keys are stable `snake_case`. The exceptions are `quote --json` and
  `search --json`, which pass Yahoo's own fields through (camelCase, unchanged
  since 0.3); their `--csv` uses snake_case columns.
- A value that isn't available is `null` in JSON and an empty cell in CSV —
  never `0`. A holding without a live price keeps its row but is left out of
  totals and listed in `unpriced`.
- Fields ending in `_percent` are percentages (`12.5` = 12.5%). `dividend_yield`
  in `quote`, `compare` and `screen` is a fraction as Yahoo reports it (`0.012`
  = 1.2%). Prices are as Yahoo returns them; derived money values are rounded
  to 2 decimals, indicators to 4.
- Dates are `YYYY-MM-DD`; `history` bar times are RFC 3339 in UTC.
- CSV follows RFC 4180 quoting (fields with a comma, quote or newline are
  quoted, quotes doubled), starts with a header row, and ends lines with `\n`.
  CSV columns are the same names as the JSON row keys.
- The `--ai` section is never part of JSON or CSV output.
- `mf` and `options` have their own `--json` (no `--csv`); fractions there are
  decimals (`0.1234` = 12.34%), and missing values are `null`, never 0.

**Shapes** (`[...]` is an array; `row` is described next to it)

| Command | `--json` | `--csv` — one row per |
|---------|----------|-----------------------|
| `quote` | `[quote]` — Yahoo's quote fields, plus `dataSource` when another provider supplied the price | quote: `symbol, name, currency, price, change, change_percent, open, day_low, day_high, year_low, year_high, volume, avg_volume_3m, market_cap, pe_ttm, eps_ttm, dividend_yield` |
| `markets` | `[{region, symbol, name, price, change, change_percent, currency}]` — `region` is `Indian Markets`, `US Markets`, `Crypto` or `FX` | index |
| `search` | `[hit]` — Yahoo's search fields | hit: `symbol, name, quote_type, exchange` |
| `history` | `{symbol, currency, period, interval, bars: [{time, open, high, low, close, volume}]}` | bar |
| `technical` | `{symbol, currency, period, interval, bars, price, sma_10, sma_20, sma_50, sma_100, sma_200, ema_12, ema_26, rsi_14, rsi_signal, macd: {line, signal, histogram, trend}, bollinger: {upper, middle, lower, position}, atr_14, atr_percent, vwap, bias: {verdict, bullish, bearish}}` — `position` is `above_upper`, `below_lower` or `within`; `verdict` is `bullish`, `bearish` or `neutral` | — |
| `risk` | `{symbol, currency, period, interval, bars, price, return_percent, volatility_percent, risk_level, sharpe, sortino, calmar, max_drawdown: {percent, peak_price, trough_price, trading_days}, var_95_percent, var_99_percent}` — `risk_level` is `low`, `moderate`, `high` or `very_high` | — |
| `compare` | `[{symbol, name, currency, price, change_percent, market_cap, pe_ttm, pe_forward, eps_ttm, price_to_book, dividend_yield, profit_margin, revenue_growth, debt_to_equity, beta, year_high, year_low, sma_50, analyst_rating}]` — the table's transpose | stock |
| `correlate` | `{period, interval, symbols: [..], matrix: [[..]]}` — `matrix[i][j]` pairs `symbols[i]` and `symbols[j]` | symbol: `symbol, <SYM1>, <SYM2>, ...` |
| `returns` | `{symbol, currency, price, returns: [{period, label, start_price, end_price, return_percent, annualized_percent, growth_of_100000}]}` | period |
| `movers` | `{gainers: [row], losers: [row]}`, row = `{side, rank, symbol, price, change_percent, volume, currency}` | stock, gainers then losers |
| `sectors` | `[{sector, symbol, price, change_percent, currency}]`, best first | sector |
| `screen` | `[{symbol, name, price, change_percent, pe, market_cap, dividend_yield, currency}]` | match |
| `dividends` | `{symbol, currency, price, pays_dividend, dividend_yield_percent, annual_dividend_per_share, income: [{invested, shares, annual_income, monthly_income}], drip: [{year, shares, growth_percent}]}` | — |
| `backtest` | `{symbol, currency, strategy, period, interval, bars, start, end, fill, capital, start_value, end_value, total_return_percent, cagr_percent, max_drawdown_percent, sharpe, sortino, exposure_percent, trade_count, win_rate_percent, avg_win_percent, avg_loss_percent, profit_factor, costs_paid, gross_return_percent, buy_hold_return_percent, alpha_percent, skipped_entries, costs, cost_model, rules, trades: [trade], equity: [{date, value}]}` — returns are after costs except `gross_return_percent`; `portfolio` and `walkforward` shapes are in [backtesting.md](backtesting.md#json-output). trade = `{symbol, entry_date, exit_date, qty, entry_price, exit_price, gross_pnl, costs, net_pnl, pnl_percent, bars_held, exit_reason}` | trade (every form) |
| `sip` | `{symbol, currency, period, interval, monthly_amount, months, total_invested, final_value, profit, total_return_percent, xirr_percent, total_units, avg_cost, current_price}` | — |
| `whatif` | `{symbol, currency, amount, date, buy_date, buy_price, shares, invested, current_price, value, pnl, pnl_percent, cagr_percent, days, years}` — `buy_date` is the trading day closest to `date` | — |
| `portfolio` | `{holdings: [holding], totals: {cost_basis, value, pnl, pnl_percent, currency}, unpriced: [symbol], unconverted: [symbol]}`, holding = `{symbol, shares, avg_cost, cost_basis, price, value, pnl, pnl_percent, weight_percent, xirr_percent, currency}` — a holding's money values are in its own currency; `totals` and `weight_percent` are in the base currency (`--base`); `unconverted` holdings had no exchange rate | holding |
| `watch` | `[{symbol, price, change, change_percent, volume, currency}]`, in watchlist order | symbol |
| `alert list` | `[{index, symbol, condition, target, created_at}]` — `index` is what `alert remove` takes | alert |
| `alert check` | `[{index, symbol, condition, target, price, currency, status}]` — `status` is `triggered`, `waiting` or `no_price` | alert |
| `tax` | `{country, tax_year, currency, sales: [{symbol, qty, bought, bought_estimated, sold, gain, term}], holdings: [{symbol, cost, value, short_term_gain, long_term_gain, estimated_dates}], realized: estimate, if_sold_today: estimate, ltcg_exemption_left, unpriced: [symbol], other_currency: [symbol]}`, estimate = `{short_term_gains, short_term_losses, long_term_gains, long_term_losses, taxable_short_term, taxable_long_term, exemption_used, short_term_tax, long_term_tax, total_tax, loss_carried_forward}` — `term` is `short`, `long`, or null (UK); `ltcg_exemption_left` is India only | open holding |
| `sim history` | `{stats: {days, winning_days, day_win_rate_percent, total_pnl, avg_daily_pnl, best_day, worst_day, trades, winning_trades, losing_trades, trade_win_rate_percent, sharpe, max_win_streak, max_loss_streak}, confidence, sessions: [{date, market, capital, target_percent, settled, trades, pnl, pnl_percent, wins, losses}]}` — `confidence` is `high`, `moderate`, `low`, or null before 5 settled days | — |
| `sim replay list` | `[{name, title, market, from, to, benchmark, benchmark_name, capital, stocks, description}]` | — |
| `sim replay status` | `{scenario, title, market, date, day, days, last_day, capital, cash, invested, equity, return_percent, max_drawdown_percent, costs_paid, cost_model, benchmark: {symbol, name, close, return_percent}, positions: [{symbol, sector, qty, avg_price, close, value, pnl, pnl_percent, weight_percent, stop, target, opened}], orders: [order], universe: [{symbol, sector, close, change_percent, change_20d_percent}], challenge, finished_challenges: [outcome], disclaimer}` — only data known at the replay date's close; `sim replay start` adds `description` and `skipped`. order = `{id, side, symbol, qty, stop, target, placed}`; challenge = `{name, title, description, day, days, started, rules: [{rule, goal, hard}], violations: [{date, rule, detail, hard}]}`; outcome = `{name, title, passed, reason, from, to, goals: [{goal, met, detail}], violations, sip}` | — |
| `sim replay next` | `{days: [{date, events: [event]}], new_badges, status}` — `event` is `fill` (`{date, symbol, side, qty, price, charges, reason, with_stop, weight_percent, pnl}`, `reason` = `order`, `stop-loss` or `target`), `contribution` (`{amount}`), `rejected` (`{symbol, side, qty, reason}`), `waiting` (`{symbol, side}`), `rule_broken` (a violation), `challenge_over` (an outcome) or `last_day` | — |
| `sim replay buy` / `sell` | `{order, warnings: [violation]}` — warnings are soft challenge rules the order breaks | — |
| `sim replay end` | `{kind, name, title, market, benchmark_name, from, to, finished, scorecard: {days, score, grade, return_percent, benchmark_return_percent, max_drawdown_percent, benchmark_max_drawdown_percent, sharpe, benchmark_sharpe, sortino, benchmark_sortino, exposure_percent, parts: {risk_adjusted, drawdown, discipline, diversification}}, badges, new_badges, disciplined, challenges: [{name, title, passed, reason}], curve, benchmark_curve, debrief, disclaimer}` — `score`/`grade` are null under 5 trading days; curves are rebased to 100 | — |
| `sim challenge list` | `[{name, title, description, days, modes, monthly_contribution, rules: [{kind, ...}]}]` — `days` 0 = until the replay ends | — |
| `sim challenge status` | `{mode, challenge, ...}` — `mode` `replay` adds `finished: [outcome]`; `live` adds `sessions, complete, return_percent, benchmark_return_percent, max_drawdown_percent, violations`; `challenge` is null when none runs | — |
| `sim challenge end` | replay: `{outcome, new_badges}`; live: the `sim replay end` shape plus `outcome` | — |
| `sim profile` | `{badges: [{id, title, how, earned, run}], streaks: {disciplined: {current, best}, challenges: {current, best}}, best: {NAME: {score, grade, date}}, challenges_passed, challenges_failed, runs: [run], disclaimer}` — `earned` is null for badges not earned yet; run = the `sim replay end` shape without `debrief`/`disclaimer` | — |
| `config show` | `{path, status, error, settings: [{key, value, source}]}` — `status` is `loaded`, `missing` or `invalid`; `source` is `flag`, `env`, `file` or `default` | — |

## Configuration file

Everything works without one. To set your own defaults:

```bash
stockwise config init     # write a commented template
stockwise config path     # where it is
stockwise config show     # every effective setting and where it came from
```

The file is `config.toml` in your platform's config directory —
`~/Library/Application Support/stockwise/` on macOS, `~/.config/stockwise/`
on Linux (`$XDG_CONFIG_HOME`), `%APPDATA%\stockwise\` on Windows — or the
file named by `STOCKWISE_CONFIG`.

```toml
market = "us"                       # in | us | crypto | fx
lang = "hi"                         # en | hi
quote_currency = "inr"              # BTC → BTC-INR with -m crypto
portfolio = "long-term"             # a named portfolio
ai = false
ai_provider = "ollama"              # ollama | openai | anthropic | gemini
model = "llama3.1:8b"
ollama_host = "http://gpu-box:11434"
data_provider = "yahoo"             # yahoo | alphavantage | massive | twelvedata
redis_url = "off"
home = "~/Dropbox/stockwise"        # data directory; ~ is your home

[periods]                           # default -p per command
technical = "6mo"
backtest = "5y"
sip = "10y"

[notify]                            # STOCKWISE_* notification variables, lower-case, no prefix
telegram_chat_id = "123456789"
smtp_host = "smtp.gmail.com"
email_to = "me@example.com"
```

| Key | Overridden by | Built-in default |
|-----|---------------|------------------|
| `market` | `-m`, `STOCKWISE_MARKET` | `in` |
| `ai` | `--ai` / `--no-ai`, `STOCKWISE_AI` | `false` |
| `model` | `STOCKWISE_MODEL` | `qwen3:14b` |
| `ollama_host` | `OLLAMA_HOST` | `http://localhost:11434` |
| `redis_url` | `STOCKWISE_REDIS_URL` | `redis://127.0.0.1:6379/` |
| `home` | `STOCKWISE_HOME` | platform data directory (see `doctor`) |
| `lang` | `--lang`, `STOCKWISE_LANG` | your locale, else `en` |
| `quote_currency` | `--quote-currency`, `STOCKWISE_QUOTE_CURRENCY` | `usd` |
| `portfolio` | `--portfolio`, `STOCKWISE_PORTFOLIO` | `default` |
| `ai_provider` | `STOCKWISE_AI_PROVIDER` | `ollama` |
| `data_provider` | `STOCKWISE_DATA_PROVIDER` | `yahoo` |
| `[periods] <command>` | `-p` | per command, see `<command> --help` |
| `[notify] <key>` | `STOCKWISE_<KEY>` | none (see [notifications](bots.md#notifications)) |

`[periods]` accepts the commands that take `-p`: `deep`, `technical`,
`history`, `risk`, `patterns`, `support`, `fibs`, `volume`, `gaps`,
`stoploss`, `forecast`, `options`, `backtest`, `sip`. `--help` shows the
default from your file, e.g. `[default: 6mo]`.

A file that doesn't parse — an unknown key, a wrong type, an unknown market or
period — stops every command with exit code 1 and an error naming the file and
the key. `config`, `doctor` and `completions` still run, so you can inspect it
or replace it with `stockwise config init --force`.

API keys (`OPENAI_API_KEY`, the data provider's key, ...) and
`STOCKWISE_API_TOKEN` are read from the environment only. Notification tokens
and passwords may go in `[notify]`; if they do, keep the file private
(`chmod 600`) — `config show` and `doctor` warn when other users can read it.

`stockwise serve` answers with the same shapes over HTTP — see
[server.md](server.md).

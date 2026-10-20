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
| `--portfolio <NAME>` | `STOCKWISE_PORTFOLIO` | Use a named portfolio (`portfolios/NAME.json`) instead of the default `portfolio.json`. Applies to `portfolio`, `import`, `tax`, `wealth`, `dashboard`, `trade holdings` and every other command that reads holdings |

Other environment variables: `STOCKWISE_HOME` (data directory),
`STOCKWISE_REDIS_URL` (cache server, or `off`), `STOCKWISE_DATA_PROVIDER`
and its API key ([data-providers.md](data-providers.md)),
`STOCKWISE_AI_PROVIDER`, `STOCKWISE_MODEL`, `OLLAMA_HOST` and the cloud AI keys
([ai.md](ai.md)). `NO_COLOR` disables colour; colour is also off when output
is piped.

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
network), `2` invalid arguments.

## Start here

| Command | Description |
|---------|-------------|
| `quote <SYMBOLS...> [--json]` (`q`) | Price, change, day/52-week range, volume, market cap, P/E, EPS, moving averages. Unknown symbols are named in the error |
| `deep <SYMBOL> [-p PERIOD]` | Everything about one stock: fundamentals, technicals, support/resistance, Fibonacci, risk, gaps, volume, insights. Default `3mo` |
| `markets [--crypto] [--fx] [--json]` | US + India indices (S&P 500, Dow, NASDAQ, Russell, VIX, NIFTY 50, SENSEX, Bank NIFTY, NIFTY IT); `--crypto` adds the largest coins, `--fx` major pairs (USD/INR, EUR/USD, ...). With `-m crypto` / `-m fx` that section comes first. `--json` rows carry `region`: `Indian Markets`, `US Markets`, `Crypto` or `FX` |
| `dashboard` (`d`) | Indices + portfolio P&L + watchlist + triggered alerts; first-run hints when empty |
| `search <QUERY> [--json]` | Find a ticker by company name |
| `doctor` | Check market and language, data directory, Yahoo reachability, the data provider that answered, Redis, AI provider and model (never the key), broker config |

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
| `daemon intraday [CAPITAL]` | Paper-trading loop on the NSE clock (IST, holidays included; `-m us` uses NYSE hours in ET): scan, enter at the open, trail stops every 60s, book half at T1, square off 15 minutes before the close, settle into `sim` history. Survives network blips, resumes the day after a restart, and squares off on Ctrl+C. **Never sends broker orders** |
| `daemon longterm` | Post-market scan: scores, portfolio check, alerts, tax-harvest ideas |
| `sim start [CAPITAL] [TARGET%]` | Start a paper session with the bot's picks (default ₹25,000, 2%) on a trading day before the close |
| `sim status` / `history` | Live P&L / record + confidence rating |
| `sim settle [--now]` | Settle open sessions from that day's 5-minute bars: stop or target first, else exit at the 15:15 bar. Today's session waits for the close unless `--now` |
| `sim reset [--yes]` | Delete simulator history after confirmation (a timestamped backup is kept) |
| `report <intraday\|longterm\|portfolio\|SYMBOL>` | AI report via local Ollama |

## Portfolio

Data lives in the directory shown by `stockwise doctor`.

| Command | Description |
|---------|-------------|
| `portfolio [show] [--base inr\|usd]` (`p`) | Holdings with live P&L, XIRR and allocation; holdings without a live price show N/A. Mixed INR/USD holdings are totalled in `--base` (default: the market's currency) |
| `portfolio add <SYMBOL> <SHARES> <COST> [--date YYYY-MM-DD]` | Add a purchase as a new lot (default date: today) |
| `portfolio sell <SYMBOL> <SHARES> [PRICE] [--date YYYY-MM-DD]` | Record a sale, oldest lots first (FIFO). Price defaults to the live price; a past `--date` needs a price. Refused while a split or bonus before the sale isn't applied to your lots |
| `portfolio lots <SYMBOL> [--lot N --date YYYY-MM-DD]` | Lots, sales, dividends and XIRR of one holding; `--lot N --date` corrects a purchase date |
| `portfolio actions [SYMBOL] [--apply]` | Splits, bonuses and dividends since your purchases, including positions you have sold (from Yahoo). Shows them; `--apply` adjusts lots and records dividends, and is safe to repeat |
| `portfolio list` | Your portfolios: the default one and every named one |
| `portfolio remove <SYMBOL>` | Remove a holding and its lots without recording a sale |
| `watch [show]` / `watch add <SYMBOLS...>` / `watch remove <SYMBOLS...>` (`w`) | Watchlist |
| `alert [list]` / `alert add <SYMBOL> <above\|below> <PRICE>` / `alert check` / `alert remove <N>` | Price alerts |
| `alert watch [--interval 60] [--once]` | Check alerts while their market is open and notify each trigger once (see [notifications](bots.md#notifications)). `--once` checks every alert once and exits, for cron |
| `import <kite\|indmoney\|groww\|upstox\|cas\|csv> <FILE> [--replace]` | Import broker holdings, a lot list, a tradebook or a CAS (casparser JSON/CSV); re-importing updates instead of doubling (see below and [brokers.md](brokers.md#importing-holdings)) |
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
| `notify [status]` / `notify test` | Show the configured notification channels (Telegram, Slack, Discord, email, desktop) / send a test message. See [notifications](bots.md#notifications) |
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

`quote --json` returns Yahoo's quote fields (camelCase), plus `dataSource` when
another provider supplied the price. `history --json`
returns `{symbol, currency, period, interval, bars: [{time, open, high, low, close, volume}]}`.
Fractions in `mf` and `options` JSON are decimals (`0.1234` = 12.34%); missing
values are `null`, never 0.

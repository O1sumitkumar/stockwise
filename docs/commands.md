# Commands Reference

`stockwise --help` lists every command by group; `stockwise <command> --help`
shows its arguments. This page adds detail.

## Global options

| Option | Env var | Description |
|--------|---------|-------------|
| `-m, --market <in\|us>` | `STOCKWISE_MARKET` | Market for bare tickers. Default `in`: `TCS` → `TCS.NS`. Symbols with a suffix (`RELIANCE.BO`, `VOD.L`), indices (`^NSEI`), FX (`USDINR=X`) and crypto (`BTC-USD`) pass through unchanged |
| `--ai` | `STOCKWISE_AI` | Add a local-AI section to `analyze`, `technical`, `compare`, `screen`, `backtest`, `deep` (slow; see [ai.md](ai.md)) |
| `--no-ai` | | Turn the AI section off for one run, even if `STOCKWISE_AI` or the config file turn it on |

Other environment variables: `STOCKWISE_HOME` (data directory),
`STOCKWISE_REDIS_URL` (cache server, or `off`), `STOCKWISE_MODEL` and
`OLLAMA_HOST` (AI), `STOCKWISE_CONFIG` (config file path). `NO_COLOR` disables
colour; colour is also off when output is piped.

Defaults for all of these, and for each command's `-p`, can live in a
[config file](#configuration-file). Precedence: command-line flag >
environment variable > config file > built-in default.

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
| `markets [--json\|--csv]` | US + India indices (S&P 500, Dow, NASDAQ, Russell, VIX, NIFTY 50, SENSEX, Bank NIFTY, NIFTY IT) |
| `dashboard` (`d`) | Indices + portfolio P&L + watchlist + triggered alerts; first-run hints when empty |
| `search <QUERY> [--json\|--csv]` | Find a ticker by company name |
| `doctor` | Check the config file, data directory, Yahoo reachability, Redis, Ollama model, broker config. Exits 1 if market data is unreachable or the config file is invalid |

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
| `options <SYMBOL> [-p]` | Expected 1/5/10/30-day moves and OTM strike ideas from realized volatility |

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

## Strategies & bots

| Command | Description |
|---------|-------------|
| `backtest <SYMBOL> [-s STRATEGY] [-p] [--json\|--csv]` | `rsi` (default), `macd`, `sma`, `bb`, `vwap`, `mr`. Equity curve, alpha, per-trade Sharpe, drawdown. Default `2y`. No commissions or slippage are modelled |
| `sip <SYMBOL> <AMOUNT> [-p] [--json]` | Monthly SIP/DCA simulation with value-vs-invested chart and XIRR. Default `5y` |
| `whatif <SYMBOL> <AMOUNT> <YYYY-MM-DD> [--json]` | What a lump sum on a past date is worth today |
| `intraday <CAPITAL> <TARGET%>` (`bot`) | 9-strategy intraday ideas with Kelly sizing and dual targets. Signals only |
| `longterm [MONTHLY_BUDGET]` | 6-pillar scoring, Monte Carlo, SIP allocation |
| `daemon intraday [CAPITAL]` | Paper-trading loop, 9:00–15:30 IST: scan, enter, trail stops every 60s, book half at T1, square off 15:15, settle into `sim` history. **Never sends broker orders** |
| `daemon longterm` | Post-market scan: scores, portfolio check, alerts, tax-harvest ideas |
| `sim start [CAPITAL] [TARGET%]` | Start a paper session with the bot's picks (default ₹25,000, 2%) |
| `sim status` / `settle` / `history [--json]` | Live P&L / close at current prices / record + confidence rating |
| `sim reset [--yes]` | Delete simulator history after confirmation (a timestamped backup is kept) |
| `report <intraday\|longterm\|portfolio\|SYMBOL>` | AI report via local Ollama |

## Portfolio

Data lives in the directory shown by `stockwise doctor`.

| Command | Description |
|---------|-------------|
| `portfolio [show] [--json\|--csv]` (`p`) | Holdings with live P&L and allocation; holdings without a live price show N/A |
| `portfolio add <SYMBOL> <SHARES> <COST>` | Add shares (averages into an existing holding) |
| `portfolio remove <SYMBOL>` | Remove a holding |
| `watch [show] [--json\|--csv]` / `watch add <SYMBOLS...>` / `watch remove <SYMBOLS...>` (`w`) | Watchlist |
| `alert [list] [--json\|--csv]` / `alert add <SYMBOL> <above\|below> <PRICE>` / `alert check [--json\|--csv]` / `alert remove <N>` | Price alerts |
| `import <kite\|indmoney\|csv> <FILE>` | Import broker holdings; re-importing updates instead of doubling |
| `export <portfolio\|watchlist>` | Write a CSV to the current directory |
| `wealth` | Save today's portfolio value and show growth (run daily) |
| `rebalance` | Trades to reach equal weight |
| `harvest` | Tax-loss harvesting candidates |
| `tax <in\|us\|uk> [--json\|--csv]` | Capital-gains estimate for the current tax year |
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
| `config [show] [--json]` | Effective settings and where each comes from (flag, env, config file, default), plus the config file's status |
| `config path` | Print the config file path (it may not exist yet) |
| `config init [--force]` | Write a commented template to the config file path; refuses to replace an existing file without `--force` |
| `completions <bash\|zsh\|fish\|powershell\|elvish>` | Print a completion script, e.g. `stockwise completions zsh > ~/.zfunc/_stockwise` |

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

**Shapes** (`[...]` is an array; `row` is described next to it)

| Command | `--json` | `--csv` — one row per |
|---------|----------|-----------------------|
| `quote` | `[quote]` — Yahoo's quote fields | quote: `symbol, name, currency, price, change, change_percent, open, day_low, day_high, year_low, year_high, volume, avg_volume_3m, market_cap, pe_ttm, eps_ttm, dividend_yield` |
| `markets` | `[{region, symbol, name, price, change, change_percent, currency}]` | index |
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
| `backtest` | `{symbol, currency, strategy, period, interval, bars, total_return_percent, buy_hold_return_percent, alpha_percent, trade_count, win_rate_percent, avg_win_percent, avg_loss_percent, max_drawdown_percent, sharpe, trades: [trade], equity_curve: [..]}`, trade = `{number, entry_date, exit_date, entry_price, exit_price, pnl_percent}` | trade |
| `sip` | `{symbol, currency, period, interval, monthly_amount, months, total_invested, final_value, profit, total_return_percent, xirr_percent, total_units, avg_cost, current_price}` | — |
| `whatif` | `{symbol, currency, amount, date, buy_date, buy_price, shares, invested, current_price, value, pnl, pnl_percent, cagr_percent, days, years}` — `buy_date` is the trading day closest to `date` | — |
| `portfolio` | `{holdings: [holding], totals: {cost_basis, value, pnl, pnl_percent, currency}, unpriced: [symbol]}`, holding = `{symbol, shares, avg_cost, cost_basis, price, value, pnl, pnl_percent, weight_percent, currency}` | holding |
| `watch` | `[{symbol, price, change, change_percent, volume, currency}]`, in watchlist order | symbol |
| `alert list` | `[{index, symbol, condition, target, created_at}]` — `index` is what `alert remove` takes | alert |
| `alert check` | `[{index, symbol, condition, target, price, currency, status}]` — `status` is `triggered`, `waiting` or `no_price` | alert |
| `tax` | `{country, tax_year, currency, holdings: [{symbol, bought, term, cost, value, gain, tax_rate_percent, tax}], summary: {short_term_gains, short_term_losses, long_term_gains, long_term_losses, stcg_rate_percent, ltcg_rate_percent, ltcg_exemption, stcg_tax, ltcg_tax, total_tax, tax_saved_by_losses}, unpriced: [symbol]}` — `term` is `short` or `long` | holding |
| `sim history` | `{stats: {days, winning_days, day_win_rate_percent, total_pnl, avg_daily_pnl, best_day, worst_day, trades, winning_trades, losing_trades, trade_win_rate_percent, sharpe, max_win_streak, max_loss_streak}, confidence, sessions: [{date, market, capital, target_percent, settled, trades, pnl, pnl_percent, wins, losses}]}` — `confidence` is `high`, `moderate`, `low`, or null before 5 settled days | — |
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
market = "us"                       # in | us
ai = false
model = "llama3.1:8b"
ollama_host = "http://gpu-box:11434"
redis_url = "off"
home = "~/Dropbox/stockwise"        # data directory; ~ is your home

[periods]                           # default -p per command
technical = "6mo"
backtest = "5y"
sip = "10y"
```

| Key | Overridden by | Built-in default |
|-----|---------------|------------------|
| `market` | `-m`, `STOCKWISE_MARKET` | `in` |
| `ai` | `--ai` / `--no-ai`, `STOCKWISE_AI` | `false` |
| `model` | `STOCKWISE_MODEL` | `qwen3:14b` |
| `ollama_host` | `OLLAMA_HOST` | `http://localhost:11434` |
| `redis_url` | `STOCKWISE_REDIS_URL` | `redis://127.0.0.1:6379/` |
| `home` | `STOCKWISE_HOME` | platform data directory (see `doctor`) |
| `[periods] <command>` | `-p` | per command, see `<command> --help` |

`[periods]` accepts the commands that take `-p`: `deep`, `technical`,
`history`, `risk`, `patterns`, `support`, `fibs`, `volume`, `gaps`,
`stoploss`, `forecast`, `options`, `backtest`, `sip`. `--help` shows the
default from your file, e.g. `[default: 6mo]`.

A file that doesn't parse — an unknown key, a wrong type, an unknown market or
period — stops every command with exit code 1 and an error naming the file and
the key. `config`, `doctor` and `completions` still run, so you can inspect it
or replace it with `stockwise config init --force`.

<p align="center">
  <img src="assets/banner.svg" alt="StockWise — stock market toolkit for your terminal" width="820">
</p>

<p align="center">
  <a href="https://github.com/iamsaquib8/stockwise/actions/workflows/ci.yml"><img src="https://github.com/iamsaquib8/stockwise/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/stockwise"><img src="https://img.shields.io/crates/v/stockwise.svg" alt="crates.io"></a>
  <a href="https://crates.io/crates/stockwise"><img src="https://img.shields.io/crates/d/stockwise.svg" alt="Downloads"></a>
  <a href="https://github.com/iamsaquib8/stockwise/releases/latest"><img src="https://img.shields.io/github/v/release/iamsaquib8/stockwise?label=binaries" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="#install"><img src="https://img.shields.io/badge/rustc-1.88+-blue.svg" alt="MSRV 1.88"></a>
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#tour">Tour</a> ·
  <a href="docs/commands.md">Commands</a> ·
  <a href="#configuration">Configuration</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="ROADMAP.md">Roadmap</a>
</p>

**StockWise is a stock market toolkit for your terminal, built for Indian
(NSE/BSE) and US stocks.** Quotes, charts, screeners, backtests, SIP math, paper
trading, and portfolio tracking in one fast Rust binary. No API keys, no
account, no browser tabs.

<p align="center">
  <img src="assets/history.svg" alt="stockwise history AAPL -m us -p 6mo: a braille price chart with volume bars in the terminal" width="760">
</p>

```bash
stockwise quote RELIANCE TCS            # Indian stocks are the default
stockwise quote AAPL NVDA -m us         # US stocks
stockwise deep HDFCBANK                 # everything about one stock
stockwise backtest INFY -s macd         # test a strategy on 2 years of data
stockwise sip NIFTYBEES 5000 -p 5y      # what a ₹5,000/month SIP became (XIRR)
```

> **Not financial advice.** StockWise is for education and information. See
> [DISCLAIMER.md](DISCLAIMER.md).

## Why StockWise

- **India-first, US too.** `TCS` means `TCS.NS`; rupee formatting in lakhs and
  crores; NIFTY/SENSEX sectors; Indian tax years and LTCG/STCG; imports from
  Zerodha Kite and INDmoney. Add `-m us` (or `STOCKWISE_MARKET=us`) for US stocks.
- **Zero setup.** Public market data, no API key or sign-up. One binary for
  macOS, Linux and Windows. `stockwise doctor` checks everything.
- **60 commands, one muscle memory.** Fundamentals, technicals, risk, screeners,
  sector rotation, backtests, SIP/what-if calculators, alerts, tax estimates.
- **Scriptable.** `--json` on `quote`, `history`, `markets` and `search`; real
  exit codes; shell completions.
- **Practice before money.** A paper-trading simulator and an intraday paper
  daemon with risk controls. Optional live orders via Angel One, behind a
  preview and a typed confirmation.
- **Private by design.** Your portfolio stays in local files. Optional AI runs
  on your own machine through [Ollama](https://ollama.com).

## Install

**macOS / Linux — prebuilt binary**

```bash
curl -fsSL https://raw.githubusercontent.com/iamsaquib8/stockwise/master/install.sh | sh
```

Installs to `~/.local/bin` (override with `STOCKWISE_INSTALL=/usr/local/bin`)
and verifies the release checksum. Windows: download the `.zip` from the
[latest release](https://github.com/iamsaquib8/stockwise/releases/latest).

**With Rust**

```bash
cargo binstall stockwise    # prebuilt binary via cargo-binstall
cargo install stockwise     # or build from source (Rust 1.88+)
```

Then:

```bash
stockwise doctor                                          # check your setup
stockwise completions zsh > ~/.zfunc/_stockwise           # optional: tab completion
```

## Tour

<p align="center">
  <img src="assets/markets.svg" alt="stockwise markets: Indian and US indices with daily change" width="49%">
  <img src="assets/backtest.svg" alt="stockwise backtest TCS -s macd: equity curve and trade statistics" width="49%">
</p>

| | Commands |
|---|---|
| **Start here** | `quote`, `deep`, `markets`, `dashboard`, `search`, `doctor` |
| **Analysis** | `analyze`, `technical`, `history`, `chart`, `compare`, `risk`, `correlate`, `timeframes`, `returns`, `patterns`, `support`, `fibs`, `volume`, `gaps`, `stoploss`, `forecast`, `options` |
| **Market** | `movers`, `sectors`, `heatmap`, `sentiment`, `news`, `rotation`, `sectorcmp`, `peers`, `insider`, `ipo`, `earnings`, `surprise`, `matrix`, `picks`, `screen`, `dividends` |
| **Strategies & bots** | `backtest`, `sip`, `whatif`, `intraday`, `longterm`, `daemon`, `sim`, `report` |
| **Portfolio** | `portfolio`, `watch`, `alert`, `import`, `export`, `wealth`, `rebalance`, `harvest`, `tax`, `divcal`, `pcorr` |
| **Live trading** | `trade` (Angel One) |

`stockwise --help` shows the same groups; [docs/commands.md](docs/commands.md)
has every option.

### Track a portfolio

```bash
stockwise import kite ~/Downloads/holdings.csv   # or: portfolio add TCS 10 3500
stockwise portfolio                              # live P&L + allocation
stockwise alert add RELIANCE below 1200
stockwise tax in                                 # LTCG/STCG estimate for this FY
stockwise dashboard                              # indices + portfolio + watchlist + alerts
```

### Script it

```bash
stockwise quote TCS INFY --json | jq '.[] | {symbol, regularMarketPrice}'
stockwise history AAPL -m us -p 6mo --json | jq '.bars | map(.close) | max'
stockwise quote ZZZZ || echo "lookup failed"     # non-zero exit on errors
```

### Paper trade, then decide

```bash
stockwise sim start 100000          # paper session with the bot's picks
stockwise sim status                # live P&L
stockwise daemon intraday 25000     # NSE-hours paper loop: 1% risk/trade, 3% daily stop
stockwise sim settle                # after the close: replay the day's 5-minute bars
stockwise sim history               # track record + confidence rating
```

The daemon never sends broker orders. It runs on the exchange clock (IST,
NSE holidays included), survives network blips, resumes after a restart, and
squares off on Ctrl+C. Read [docs/bot-design.md](docs/bot-design.md) for how
it decides.

### Alerts and notifications

```bash
stockwise alert add TCS above 4200
export STOCKWISE_TELEGRAM_TOKEN=... STOCKWISE_TELEGRAM_CHAT_ID=...
stockwise notify test               # Telegram, Slack, Discord, email or desktop
stockwise alert watch               # checks during market hours, notifies once per trigger
```

See [notifications](docs/bots.md#notifications) for every channel.

### Live orders (optional, Angel One)

```bash
stockwise trade setup                          # how to create owner-only credentials
stockwise trade buy RELIANCE 10 --dry-run      # resolve + preview, send nothing
stockwise trade buy RELIANCE 10                # preview, then type "yes" to send
```

Orders resolve the exact NSE instrument, show the estimated value, respect an
optional `max_order_value` cap, and refuse to run unattended without `--yes`.
See [docs/angel-one.md](docs/angel-one.md).

### Local AI (optional)

With [Ollama](https://ollama.com) running, `stockwise report TCS` writes an AI
stock report, and `--ai` adds commentary to `analyze`, `technical`, `compare`,
`screen`, `backtest` and `deep`. It's opt-in because local models take 30–60s.
See [docs/ai.md](docs/ai.md).

## Configuration

Everything works with no config. Optional environment variables:

| Variable | Purpose |
|----------|---------|
| `STOCKWISE_MARKET` | Default market: `in` (default) or `us` |
| `STOCKWISE_HOME` | Where portfolio, watchlist, alerts and history are stored |
| `STOCKWISE_AI` | `1` to add AI sections by default |
| `STOCKWISE_MODEL` / `OLLAMA_HOST` | Ollama model (default `qwen3:14b`) and server |
| `STOCKWISE_REDIS_URL` | Optional shared cache (`redis://…`), or `off` |
| `STOCKWISE_TELEGRAM_TOKEN`, `STOCKWISE_SLACK_WEBHOOK`, … | Notification channels — see [docs/bots.md](docs/bots.md#notifications) |
| `NO_COLOR` | Disable colours (also automatic when piping) |

## Docs

- [Commands reference](docs/commands.md)
- [Trading bots](docs/bots.md) and [bot design](docs/bot-design.md)
- [Angel One setup](docs/angel-one.md)
- [AI & Ollama](docs/ai.md)
- [Architecture](docs/architecture.md)

## Roadmap & contributing

Next up: Homebrew, a config file, more brokers (Zerodha, Upstox), and a TUI
dashboard — see the [ROADMAP](ROADMAP.md). Ideas and bug reports are
welcome in [Discussions](https://github.com/iamsaquib8/stockwise/discussions) and
[Issues](https://github.com/iamsaquib8/stockwise/issues); start with
[CONTRIBUTING.md](CONTRIBUTING.md).

If StockWise is useful to you, a ⭐ helps other people find it.

## License

[MIT](LICENSE) © iamsaquib8. Market data comes from Yahoo Finance and your
broker; StockWise is not affiliated with any exchange, broker, or data provider.
See [DISCLAIMER.md](DISCLAIMER.md).

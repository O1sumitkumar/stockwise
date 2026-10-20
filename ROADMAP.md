# Roadmap

Where StockWise is headed. Dates are intentionally omitted — this is a
direction, not a promise. Have an opinion? Open a
[Discussion](https://github.com/iamsaquib8/stockwise/discussions) or upvote an
[issue](https://github.com/iamsaquib8/stockwise/issues) — the roadmap follows
demand.

Legend: ✅ shipped · 🛠️ in progress · 📋 planned · 💡 exploring

## ✅ Recently shipped

**0.3** — usability and safety
- `doctor`, shell completions, grouped help, `--json` on core commands
- Typed subcommands with suggestions; real exit codes
- Live orders behind exact instrument matching, preview, and confirmation
- Opt-in local AI; owner-only credential storage; atomic data files
- One-line installer, `cargo binstall`, checksums, Linux ARM64 binaries

**0.2** — first public release
- 57 commands across analysis, screening, bots, trading, and portfolio
- Daemon workers, paper-trading simulator, Angel One live trading
- Broker import (Kite, INDmoney, CSV), Redis cache, braille charts

## 🛠️ Now — distribution, scripting & correctness (0.3.x)

**Install anywhere**
- 🛠️ Homebrew tap — `brew install iamsaquib8/tap/stockwise` (formula and release automation
  ready; waiting for the tap repository)
- 🛠️ Scoop / winget manifests for Windows; AUR package for Arch (manifests rendered on every
  release; Scoop installs from the release URL, winget and AUR submissions pending)
- ✅ Man page (`stockwise.1`) generated from the CLI definition

**Scripting**
- 📋 `--json` on more commands (`portfolio`, `technical`, `risk`, `backtest`, `screen`)
- 📋 Config file (`~/.config/stockwise/config.toml`) for market, model, and defaults
- 📋 `--csv` output for tables, for spreadsheets

**Correctness**
- 📋 Daemon resilience — survive network blips, square off on Ctrl+C, IST clock + exchange holidays
- 📋 Lot-level holdings with purchase dates, for accurate LTCG/STCG after imports
- 📋 Portfolio XIRR (money-weighted return) from lots
- 📋 `sim settle` from intraday bars (did the stop or target hit first?) instead of the closing price
- 📋 Intraday plans capped at total capital across positions

## 📋 Next — India depth, brokers & alerts (0.4)

**Brokers & data in**
- 📋 Native brokers: Zerodha Kite, Upstox, Dhan, Fyers — read-only holdings first; orders
  through the same preview → confirm flow as Angel One
- 📋 CAS import (CAMS / KFintech consolidated statements) for stocks + mutual funds
- 📋 Groww and Upstox CSV import
- 📋 Corporate actions — splits, bonuses, dividends adjust cost basis automatically

**Mutual funds & India specifics**
- ✅ Mutual funds via AMFI NAVs — `mf search`, `mf nav`, `mf history`, `mf sip` with XIRR
- 📋 NSE holiday calendar and an IST market clock for every time-aware command
- 📋 GTT / stop-loss orders with the same safety flow

**Portfolio & alerts**
- 📋 Named portfolios (`--portfolio long-term`)
- 📋 Secure credential storage via the OS keyring (no plaintext broker secrets)
- 📋 `alert watch` — a lightweight loop that checks alerts during market hours
- 📋 Notifications — Telegram, Slack, Discord, email, and desktop, for alerts and daemon events

**AI**
- 📋 Optional cloud AI providers (OpenAI, Anthropic, Gemini) alongside Ollama, strictly opt-in

## 💡 Later — depth (0.5+)

- 💡 Interactive TUI dashboard (ratatui) — live quotes, positions, P&L, alerts
- 💡 Portfolio-level and walk-forward backtesting with Indian costs (STT, brokerage, slippage)
- 💡 Pluggable strategy system for `backtest` and the intraday bot (TOML rules or WASM)
- ✅ F&O: option chains, greeks, IV, PCR, max pain — NSE index options and F&O stocks, US
  options via Yahoo (`options chain`, `options pcr`, `options maxpain`); BSE options not yet
- 💡 Crypto and forex as first-class markets (quotes already work: `BTC-USD`, `USDINR=X`)
- ✅ Mutual-fund / ETF screening and overlap analysis — `mf screen` ranks Indian schemes by
  category and returns (no expense-ratio data in AMFI's feed); `mf overlap` covers funds with
  Yahoo holdings data (mainly US ETFs/funds, top 10 holdings)
- 💡 Mixed-currency portfolios with INR/USD conversion
- 💡 Data-provider abstraction (official NSE feeds, Alpha Vantage, Polygon) beyond Yahoo
- 💡 Web/REST mode for remote dashboards; Prometheus metrics for the daemon
- ✅ Docker image and Nix flake
- 💡 Hindi localization

## Non-goals

- Becoming a brokerage or holding custody of funds
- Promising profits or providing personalized financial advice (see
  [DISCLAIMER.md](DISCLAIMER.md))
- High-frequency / co-located trading

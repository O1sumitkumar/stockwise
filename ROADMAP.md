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

## 🛠️ Now — distribution & scripting (0.3.x)

- 📋 Homebrew tap — `brew install iamsaquib8/tap/stockwise`
- 📋 `--json` on more commands (`portfolio`, `technical`, `risk`, `backtest`)
- 📋 Config file (`~/.config/stockwise/config.toml`) for market, model, and defaults
- 📋 Daemon resilience — survive network blips, square off on Ctrl+C, IST clock + exchange holidays
- 📋 Lot-level holdings with purchase dates, for accurate LTCG/STCG after imports

## 📋 Next — brokers & alerts (0.4)

- 📋 Native Zerodha Kite and Upstox brokers (beyond CSV import)
- 📋 Named portfolios
- 📋 Secure credential storage via the OS keyring (no plaintext broker secrets)
- 📋 Daemon notifications — Telegram, Slack, and email on entries/exits/alerts
- 📋 Optional cloud AI providers (OpenAI, Anthropic, Gemini) alongside Ollama

## 💡 Later — depth (0.5+)

- 💡 Interactive TUI dashboard (ratatui) — live quotes, positions, P&L
- 💡 Portfolio-level and walk-forward backtesting
- 💡 Pluggable strategy system for `backtest` and the intraday bot
- 💡 Crypto and forex symbols
- 💡 Mutual-fund / ETF screening; options-chain greeks
- 💡 Web/REST mode for remote dashboards

## Non-goals

- Becoming a brokerage or holding custody of funds
- Promising profits or providing personalized financial advice (see
  [DISCLAIMER.md](DISCLAIMER.md))
- High-frequency / co-located trading

# Roadmap

Where StockWise is headed. Dates are intentionally omitted — this is a
direction, not a promise. Have an opinion? Open a
[Discussion](https://github.com/iamsaquib8/stockwise/discussions) or upvote an
[issue](https://github.com/iamsaquib8/stockwise/issues) — the roadmap follows
demand.

Legend: ✅ shipped · 🛠️ in progress · 📋 planned · 💡 exploring

## ✅ Shipped

**0.4** — the whole 0.3 roadmap
- Markets: crypto and FX (`-m crypto`, `-m fx`), mutual funds via AMFI (`mf search/nav/history/sip/screen`),
  fund/ETF overlap, F&O option chains with greeks, IV, PCR and max pain (NSE + US)
- Portfolio: lots with purchase dates, FIFO sales, XIRR, splits/bonuses/dividends, named
  portfolios, INR/USD totals, Indian LTCG/STCG with set-off rules
- Brokers: Zerodha, Upstox, Dhan, Fyers (**experimental** — untested against live accounts)
  next to Angel One; stop-loss and GTT orders; OS keyring; Groww, Upstox and CAS imports
- Bots & alerts: resilient paper daemon (Ctrl+C square-off, saved state), IST/US exchange clock
  with holidays, `sim settle` from intraday bars, `alert watch`, Telegram/Slack/Discord/email/desktop
- Backtesting: Indian/US cost models, next-bar fills, portfolio and walk-forward backtests,
  TOML strategy files usable by the intraday bot
- Interfaces: `stockwise tui` dashboard, `stockwise serve` JSON API + Prometheus metrics
- Scripting: `--json` / `--csv` across commands, `config.toml`, man pages
- Data & AI: Alpha Vantage / Massive (Polygon) / Twelve Data providers, opt-in OpenAI /
  Anthropic / Gemini, Hindi (`--lang hi`, partial)
- Distribution: Docker image, Nix flake, Homebrew/Scoop/winget/AUR manifests

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

## 🛠️ Now — finish what 0.4 started (0.4.x)

- 🛠️ Homebrew tap, Scoop bucket, winget and AUR — manifests are generated on every release;
  publishing them needs the tap repository and package submissions
- 🛠️ Hindi for every command (today: help groups, `quote`, `markets`, `dashboard`,
  `portfolio`, `search`, `doctor`)
- 📋 Broker integrations verified against live accounts — community testers wanted for
  Zerodha, Upstox, Dhan and Fyers before the "experimental" label comes off
- 📋 BSE options in `options chain`
- 📋 Expense ratios for Indian mutual funds (AMFI TER disclosures) in `mf screen`

## 📋 Next — planning and reporting (0.5)

- ✅ Tax export: capital-gains schedule (ITR Schedule CG–ready CSV) from realized lots
  (`tax export in`, Form 8949 with `tax export us`)
- ✅ Benchmark comparison and attribution — portfolio vs NIFTY 50 / S&P 500, by holding and sector
  (`portfolio benchmark`)
- ✅ Goal planner — SIP needed for a target corpus, retirement and education calculators (`goal`)
- 📋 Technical alerts — RSI, moving-average crosses and breakouts, not only price levels
- 📋 Results and corporate-announcement calendar for holdings (NSE/BSE filings)
- 📋 More Indian languages: Marathi, Tamil, Bengali

## 💡 Later — exploring (0.6+)

- 💡 Official NSE / BSE data feeds as a data provider
- 💡 Broker-side GTT and order-status sync into the portfolio
- 💡 A small web dashboard served by `stockwise serve`
- 💡 WASM strategy plugins (TOML strategies cover most cases today)
- 💡 Options strategy builder — payoff charts for spreads and straddles from the live chain

## Non-goals

- Becoming a brokerage or holding custody of funds
- Promising profits or providing personalized financial advice (see
  [DISCLAIMER.md](DISCLAIMER.md))
- High-frequency / co-located trading

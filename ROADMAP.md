# Roadmap

Where StockWise is headed. Dates are intentionally omitted — this is a
direction, not a promise. Have an opinion? Open a
[Discussion](https://github.com/iamsaquib8/stockwise/discussions) or upvote an
[issue](https://github.com/iamsaquib8/stockwise/issues) — the roadmap follows
demand.

Legend: ✅ shipped · 🛠️ in progress · 📋 planned · 💡 exploring

## ✅ Shipped

**0.5** — planning, reporting and more markets
- Planning: capital-gains export (`tax export in|us`), portfolio vs benchmark with attribution,
  goal planner (`goal sip|retire|education`)
- Alerts: RSI, moving-average crosses, breakouts, day-change and volume spikes; results calendar
- Markets: UK, Germany, Japan, Hong Kong, Singapore, Canada, Australia from one table; gold and
  silver (spot ₹/10 g, ETFs, SGBs, COMEX); G-sec yields and a YTM calculator; NSE/BSE data provider
- Funds & options: AMFI expense ratios, BSE options, options strategy builder with payoff charts
- Simulator: historical replay with the future hidden, challenges, risk-adjusted scores, badges
- Brokers: read-only `trade selftest` + tester kit, `trade sync` of fills and GTT status
- Web dashboard in `stockwise serve`; experimental WASM strategy plugins; MSRV 1.97

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

## 🛠️ Now — finish what 0.5 started (0.5.x)

- 🛠️ Homebrew tap, Scoop bucket, winget and AUR — manifests are generated on every release;
  publishing them needs the tap repository and package submissions
- 📋 Hindi for every command (today: help groups, `quote`, `markets`, `dashboard`,
  `portfolio`, `search`, `doctor`) and more Indian languages: Marathi, Tamil, Bengali
- 🛠️ Broker integrations verified against live accounts — the read-only `trade selftest` and the
  [verification criteria](docs/brokers.md#help-verify-a-broker) are ready; we need 3 reports per
  broker plus one small live order a tester places themselves
- 📋 Check the NSE/BSE parsers against live responses and add SENSEX from BSE (the endpoints are
  unofficial; licensed exchange feeds stay out of scope)
- 📋 MCX gold/silver futures (no free public feed yet) and live prices for individual G-secs
  and T-bills (no documented public source yet)

## 📋 Next — your money, end to end (0.6)

- 📋 Rebalance to your own target allocation (today `rebalance` is equal-weight only)
- 📋 Dividend and interest income tracking, with TDS, across stocks, funds and bonds
- 📋 NPS, PPF and EPF balances alongside the portfolio for a full net-worth view
- 📋 Scheduled digests — a daily or weekly portfolio summary over Telegram, Slack or email
- 📋 Installable web dashboard (PWA) for phones, still read-only and local-first

## 💡 Later — exploring (0.7+)

- 💡 Simulator scenarios contributed by the community (TOML), with a scenario gallery
- 💡 Options strategy backtests on historical chains
- 💡 Tax-loss harvesting suggestions that respect the set-off rules and wash-sale periods

## Non-goals

- Becoming a brokerage or holding custody of funds
- Promising profits or providing personalized financial advice (see
  [DISCLAIMER.md](DISCLAIMER.md))
- High-frequency / co-located trading

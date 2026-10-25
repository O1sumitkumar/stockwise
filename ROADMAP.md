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

- 🛠️ **Mutual funds and SGBs in portfolio valuation** — price `MF:<AMFI code|ISIN>` holdings (from
  `import cas`) via AMFI and SGBs via NSE across `portfolio`, `dashboard`, `tui`, `serve`, `wealth`,
  `rebalance` and `tax`, and pass `MF:` symbols through `portfolio add` untouched
- 🛠️ **Direct `.xlsx` broker imports** — read Groww, Zerodha Console, Upstox and Angel One `.xlsx`
  holdings and tradebook exports directly with the built-in reader, without saving as CSV first
- 🛠️ **Homebrew tap, Scoop bucket, winget and AUR** — manifests are generated on every release;
  publishing them needs the tap repository and package submissions
- 🛠️ **Broker integrations verified against live accounts** — the read-only `trade selftest` and the
  [verification criteria](docs/brokers.md#help-verify-a-broker) are ready; we need 3 reports per
  broker plus one small live order a tester places themselves
- 📋 **Check the NSE/BSE parsers against live responses** and add SENSEX from BSE (the endpoints are
  unofficial; licensed exchange feeds stay out of scope)
- 📋 **Hindi (`--lang hi`) across core workflows** — extend beyond help groups, `quote`, `markets`,
  `dashboard`, `portfolio`, `search` and `doctor` to mutual funds, tax, goal planning, alerts and
  the web dashboard

## 📋 Next — your money, end to end, and more markets (0.6)

- 📋 **More markets and asset classes**:
  - Global stock exchanges in the market table: **South Korea** (`-m kr`, `.KS`), **Taiwan**
    (`-m tw`, `.TW`), **France** (`-m fr`, `.PA`), **Netherlands** (`-m nl`, `.AS`), **Switzerland**
    (`-m ch`, `.SW`), **China** (`-m cn`, `.SS`/`.SZ`) and **Brazil** (`-m br`, `.SA`)
  - Energy and industrial metals in `-m commodity`: **Brent and WTI crude oil** (`BZ=F`, `CL=F`, in
    USD and ₹/bbl), **natural gas** (`NG=F`), **copper** (`HG=F`) and **platinum** (`PL=F`)
  - Macro, FX and yield instruments: **US Treasury yields** (3M, 5Y, 10Y, 30Y and the India–US 10Y
    spread) in `bonds` and `markets`, **DXY** and NRI cross-rates (`AEDINR`, `SGDINR`, `JPYINR`,
    `AUDINR`, `CADINR`, `CHFINR`) in `-m fx`, and **Indian REITs and InvITs**
- 📋 **Target-allocation and inflow-only rebalancing** — saved target weights by stock, sector or
  asset class (equity / debt / gold / international), drift bands, and tax-free "buy-only"
  rebalancing for new cash or SIP inflows (today `rebalance` is equal-weight only)
- 📋 **Lot-level tax-gain and tax-loss harvesting** — use India's ₹1.25 lakh tax-free LTCG exemption
  before 31 March (gain harvesting), respect STCL/LTCL set-off rules, and flag 30-day US wash-sale
  windows
- 📋 **Complete Indian ITR toolkit** — automatic 31 Jan 2018 LTCG grandfathering, **ITR Schedule FA**
  (foreign assets peak and closing balance in INR) and Schedule FSI/TR for US stock holdings,
  asset-specific tax rules (debt MFs under Sec. 50AA, SGB maturity exemption, crypto VDA under
  Sec. 115BBH), and quarterly **Advance Tax** estimates
- 📋 **Dividend and interest income tracking**, with TDS, across stocks, mutual funds, SGBs, REITs
  and bonds
- 📋 **Full net-worth view and active SIP tracker** — NPS, PPF, EPF, bank FDs and cash alongside the
  portfolio; register running monthly SIPs and track live progress against `goal` targets
- 📋 **Portfolio X-Ray** — look-through overlap across stocks, ETFs and mutual funds, plus
  asset-class, market-cap (large/mid/small) and sector concentration
- 📋 **Scheduled morning and post-market digests** — daily or weekly portfolio summary, upcoming
  results/ex-dividend dates, and GTT changes over Telegram, Slack, Discord or email
- 📋 **Installable web dashboard (PWA) and richer TUI** — phone install, named portfolio switcher
  (`--portfolio`), lot/fundamental/technical drill-downs, and benchmark, calendar and tax tabs
  (still read-only and local-first)

## 💡 Later — exploring (0.7+)

- 💡 **Composable stock screener and broader NSE universes** — custom fundamental + technical filter
  expressions across NIFTY Midcap 150, Smallcap 250 and NIFTY 500; promoter holding and pledge %,
  daily FII/DII flows, and bulk/block deals
- 💡 **Trade journal, position sizing and OCO GTTs** — notes and tags on lots and sales, realized
  win-rate/expectancy analytics, R-multiple position sizer, and two-leg (stop-loss + target) GTT
  orders
- 💡 **More Indian languages** — Marathi, Tamil and Bengali locale catalogs
- 💡 **Gulf exchanges** — Saudi Tadawul (`-m sa`, `.SR`, Sun–Thu session) and UAE DFM/ADX (`-m ae`)
- 💡 **Simulator scenarios contributed by the community** (TOML), with a scenario gallery
- 💡 **Options strategy backtests** on historical chains
- 💡 **MCX gold/silver/crude futures and live individual G-sec / T-bill prices** — via optional
  configured broker feeds (no free public feed exists yet)

## Non-goals

- Becoming a brokerage or holding custody of funds
- Promising profits or providing personalized financial advice (see
  [DISCLAIMER.md](DISCLAIMER.md))
- High-frequency / co-located trading

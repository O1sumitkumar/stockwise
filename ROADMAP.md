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
- 🛠️ Broker integrations verified against live accounts — community testers wanted for
  Zerodha, Upstox, Dhan and Fyers before the "experimental" label comes off
  - ✅ Testing kit: `stockwise trade selftest` (read-only, redacted report), a broker test
    report issue template, and the [verification criteria](docs/brokers.md#help-verify-a-broker)
  - 🛠️ Reports from real accounts (3 per broker plus one small live order a tester places
    themselves)
- 📋 BSE options in `options chain`
- ✅ BSE options in `options chain`, `pcr` and `maxpain` — SENSEX, BANKEX, SENSEX50 and `.BO`
  stocks via BSE's public API; BSE sometimes refuses scripted requests (StockWise then says so)
- ✅ Expense ratios for Indian mutual funds (AMFI TER disclosures) in `mf screen`, `mf nav` and
  `mf history`, with `--sort ter` and `--max-ter`

## 📋 Next — planning and reporting (0.5)

- 🛠️ NSE / BSE data straight from the exchanges' websites (`STOCKWISE_DATA_PROVIDER=nse|bse|exchange`):
  quotes with circuit limits and delivery %, intraday and daily bars, search, and NIFTY index members
  for `--universe`. Shipped; still to do: check the parsers against live NSE/BSE responses (the
  test fixtures are modelled on the endpoints' known shapes) and add SENSEX from BSE. The
  endpoints are unofficial — licensed exchange feeds remain out of scope
- ✅ Tax export: capital-gains schedule (ITR Schedule CG–ready CSV) from realized lots
  (`tax export in`, Form 8949 with `tax export us`)
- ✅ Benchmark comparison and attribution — portfolio vs NIFTY 50 / S&P 500, by holding and sector
  (`portfolio benchmark`)
- ✅ Goal planner — SIP needed for a target corpus, retirement and education calculators (`goal`)
- ✅ Technical alerts — RSI, SMA/EMA crosses, N-day breakouts, day-change and volume-spike
  alerts next to price levels, plus `results-in N` before a results date
- ✅ Results and corporate calendar for holdings (`stockwise calendar`) — Yahoo earnings and
  dividend dates for US and Indian stocks; NSE board meetings and corporate actions (filed
  results dates, splits, bonus) for Indian stocks. BSE filings and advance US splits are not covered
- 📋 More Indian languages: Marathi, Tamil, Bengali
- 🛠️ Commodities: gold and silver — spot in ₹/10 g, gold/silver ETFs, Sovereign Gold Bonds and
  COMEX futures (`commodity`, `-m commodity`); still missing: MCX futures (no free public feed),
  and SGB prices only show when NSE accepts StockWise's session
- ✅ Global exchanges from one market table — UK, Germany, Japan, Hong Kong, Singapore, Canada,
  Australia (`-m uk|de|jp|hk|sg|ca|au`, `markets --global`)
- 🛠️ Indian government bonds and T-bills (G-secs) — benchmark yields, India 10Y in `markets`, the
  outstanding list and a YTM calculator (`bonds`); still missing: live prices of individual G-secs
  and T-bills (no documented public source)
- ✅ Simulator challenges — historical replay ("trade the 2020 crash" with the future hidden),
  goal-based challenges, risk-adjusted scores, badges and streaks, a shareable result card
  (`sim replay`, `sim challenge`, `sim profile`, `sim card`)

## 💡 Later — exploring (0.6+)

- ✅ Broker-side GTT and order-status sync into the portfolio — `trade sync` (fills → lots and
  FIFO sales, idempotent) and GTT status changes in `trade gtt list` (experimental on non-Angel
  brokers)
- ✅ A small web dashboard served by `stockwise serve` (`serve --open`)
- ✅ WASM strategy plugins (**experimental**; `--features wasm-plugins`, `backtest -s wasm:PATH`)
- ✅ Options strategy builder — `options payoff`: legs or presets (spreads, straddle, strangle,
  iron condor, butterfly, covered call) priced from the live chain, payoff chart, breakevens,
  probability of profit, net greeks

## Non-goals

- Becoming a brokerage or holding custody of funds
- Promising profits or providing personalized financial advice (see
  [DISCLAIMER.md](DISCLAIMER.md))
- High-frequency / co-located trading

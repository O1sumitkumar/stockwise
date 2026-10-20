# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `stockwise man` prints the `stockwise.1` man page; `stockwise man --dir DIR`
  writes it plus one `stockwise-<command>.1` page per command. The pages are
  generated from the CLI definition and ship in every release archive under
  `man/`.
- Docker image `ghcr.io/iamsaquib8/stockwise` for linux/amd64 and linux/arm64,
  published on every release. It runs as a non-root user and keeps data in the
  `/data` volume.
- Nix flake: `nix run github:iamsaquib8/stockwise`, `nix profile install`, and
  `nix develop` for a dev shell. Installs man pages and shell completions.
- Package manifests in `packaging/` for Homebrew, Scoop, winget and the AUR
  (`stockwise-bin`), filled in from each release's checksums by
  `packaging/render.sh`. Releases push the formula to the
  `iamsaquib8/homebrew-tap` tap when a `HOMEBREW_TAP_TOKEN` secret is set, and
  attach `stockwise.json` so `scoop install <release URL>` works.
- `mf` — Indian mutual funds from AMFI's public NAV file: `mf search`, `mf nav`,
  `mf history` (NAV chart, 1M–10Y trailing returns, since-inception CAGR, max
  drawdown) and `mf sip` (monthly SIP with XIRR on the actual purchase dates).
  NAV history comes from api.mfapi.in; both are cached in the data directory
  for 12 hours. `--json` on every subcommand.
- `mf screen` ranks a category's growth-option schemes by 1Y/3Y/5Y return
  (bare `mf screen` lists categories). Expense ratios are not in AMFI's feed,
  so they are not shown for Indian schemes.
- `mf overlap A B` — shared top holdings, holdings and sector overlap, and
  expense ratios for funds Yahoo Finance has holdings for (mainly US ETFs and
  mutual funds). Indian schemes and NSE ETFs report "no holdings data".
- `options chain` (OI, change in OI, volume, IV, delta, gamma, theta, vega,
  rho), `options pcr` (put/call ratio and open-interest walls) and
  `options maxpain`. Indian symbols use NSE's option chain (index options and
  F&O stocks), US symbols (`-m us`) use Yahoo Finance. Black-Scholes greeks
  with an implied-volatility solver, valued at the data timestamp.

### Changed
- `options` is now a command group. `stockwise options SYMBOL` still shows
  expected moves, also available as `options moves SYMBOL`. `mf` and
  `options` are listed under a new "Funds & F&O" help group.

### Fixed
- `options` (expected moves) treated a missing price as 0; it now reports an
  error.
- **Crypto and forex markets.** `-m crypto` resolves bare coins (`BTC` →
  `BTC-USD`; `--quote-currency inr` / `STOCKWISE_QUOTE_CURRENCY` for `BTC-INR`)
  and `-m fx` resolves pairs (`USDINR`, `usd/inr` → `USDINR=X`). Prices show
  4 decimals for FX and up to 8 for low-priced coins; `quote` notes 24/7
  (crypto) and weekday-only (FX) trading. `markets --crypto --fx` adds top
  coins and major pairs. Session-, sector- and fundamentals-based commands
  (`sectors`, `screen`, `intraday`, `daemon`, ...) explain that they need a
  stock market instead of running on coins.
- **Data providers.** `STOCKWISE_DATA_PROVIDER=alphavantage|massive|twelvedata`
  with the provider's API key routes quotes, daily history and search through
  Alpha Vantage, Massive (formerly Polygon.io) or Twelve Data. Yahoo fills the
  fields and symbols a provider lacks and takes over on errors or exhausted
  free tiers. `doctor` shows which provider answered; `quote` names it, and
  `quote --json` adds `dataSource`. See docs/data-providers.md.
- **Cloud AI, opt-in.** `STOCKWISE_AI_PROVIDER=openai|anthropic|gemini` with
  `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `GEMINI_API_KEY` uses a hosted model
  for `report` and `--ai` (same prompts as Ollama, which stays the default).
  The first request of a run prints a notice that data is being sent;
  `doctor` shows provider and model, never the key. `STOCKWISE_MODEL`
  overrides the default model (`gpt-6.1-sol`, `claude-opus-5-5`,
  `gemini-3.8-flash`). See docs/ai.md.
- **Hindi (partial).** `--lang hi`, `STOCKWISE_LANG=hi`, or a `hi_*` locale
  translates help group titles, the main command descriptions, `quote`,
  `markets`, `dashboard`, `portfolio`, `search`, `doctor` and common errors.
  Lakh/crore amounts read `₹16.27 लाख करोड़`. `--json` output is unchanged.
- `doctor` shows the active language and where it came from.

### Changed
- `quote`, `markets` and `dashboard` show `N/A` for a missing price or range
  instead of `0.00`.
- AI section titles name the provider (`AI Analysis (Ollama)`) instead of
  always saying Ollama.
- EUR, GBP and JPY amounts show `€`, `£` and `¥` instead of `$`.
- `stockwise alert watch [--interval 60] [--once]` — checks alerts while
  each alert's market is open and announces every trigger once (re-arms when
  the price moves back, at most once a day; remembered across restarts).
- Notifications for `alert watch` and the intraday daemon: Telegram, Slack
  and Discord webhooks, email over SMTP (rustls), and desktop pop-ups with
  the `desktop-notify` build feature. Configured with `STOCKWISE_*`
  environment variables; `stockwise notify` shows them and `stockwise notify
  test` sends a test message. Secrets are never printed or sent anywhere but
  their own endpoint.
- Exchange clock: IST for NSE and US Eastern (with daylight saving) for
  NYSE, with the NSE 2026 and NYSE 2026–2028 holiday calendars and NYSE
  early closes.
- `sim settle --now` settles today's session before the close.

### Changed
- `sim settle` replays each session's own 5-minute bars (falling back to
  hourly, then daily bars for old sessions): the stop or target that was hit
  first decides each trade, otherwise it exits at the 15:15 bar. It used to
  settle every session, from any date, at today's price. Today's session is
  refused until the market closes.
- `sim start` refuses to open a session when the market is closed for the
  day (weekend, holiday, or after the close).
- `daemon intraday` follows the exchange clock instead of the machine's time
  zone, skips exchange holidays, and with `-m us` trades NYSE hours.
- Intraday plans (`intraday`, `sim start`, the daemon) never need more than
  the capital in total, and the daemon's 30% per-position limit is applied.

### Fixed
- `daemon intraday` exited on the first network error, losing the day's
  paper positions. Errors are now logged and retried with backoff; the state
  is saved to `daemon_state.json` (`daemon_state_us.json` for `-m us`) every
  tick and resumed after a restart.
- Ctrl+C in `daemon intraday` now squares off at the last prices, records
  the day in the simulator history and exits 0, as the banner promised.
- Positions closed at the second target were not counted by the 3% daily
  loss kill switch; the kill switch also no longer re-fires every tick.
- The daemon kept waiting forever after a day with no qualified trades.
- Daemon P&L right after a first-target partial booking counted the booked
  half at the current price instead of the target price.
- Intraday plans could put a long's stop above the entry (support above the
  price) or a short's below it; such levels are now ignored.
- The daemon monitors positions from the moment it enters them, not only
  from 15 minutes after the open, and ignores zero quotes.

## [0.3.0] - 2026-10-21

Usability and safety release. Command-line forms from 0.2 keep working.

### Added
- `stockwise doctor` — checks the data directory, Yahoo reachability, Redis,
  the Ollama model, and broker config in one screen.
- `stockwise completions <shell>` for bash, zsh, fish, PowerShell and elvish.
- `--json` output for `quote`, `history`, `markets` and `search`.
- Grouped top-level `--help` with examples.
- `trade buy/sell/limit --dry-run` and `--yes`; optional `max_order_value` cap.
- Environment variables: `STOCKWISE_MARKET`, `STOCKWISE_HOME`, `STOCKWISE_AI`,
  `STOCKWISE_REDIS_URL`, and the standard `OLLAMA_HOST`.
- `install.sh` one-line installer, `cargo binstall` support, SHA-256 checksums
  and a Linux ARM64 binary on every release.
- Dashboard first-run hints when you have no portfolio, watchlist or alerts.

### Changed
- **Live orders need confirmation.** Every order resolves the exact NSE `-EQ`
  instrument, prints a preview, and requires typing `yes` (or `--yes`).
- **Local AI is opt-in** (`--ai` / `STOCKWISE_AI=1`). It made analysis commands
  take 40–60 seconds. `report` still always uses AI.
- Actions are real subcommands and value lists (`portfolio`, `watch`, `alert`,
  `sim`, `trade`, `screen`, `backtest -s`, `tax`, `import`, `export`,
  `daemon`): typos are rejected with suggestions, and help documents every
  argument. `portfolio`, `watch` and `alert` show their list when run bare.
- Errors go to stderr with exit code 1 (unknown symbol, no data, bad file);
  invalid arguments exit 2. Previously many printed a message and exited 0.
- Not-found errors name the symbol and suggest `-m us` for US tickers.
- `forecast` and `stoploss` accept `-p` like the other commands.
- `sim reset` asks for confirmation and keeps a timestamped backup.
- Local data files are written atomically, and parse errors name the file.
- Redis has 300/500 ms timeouts and no longer prints a banner on every run.

### Fixed
- `M&M` and other symbols/searches containing `&` were cut off (`M&M` showed
  Macy's); all request URLs are now percent-encoded.
- `risk`, `forecast`, `options` and `deep` annualized weekly or monthly bars as
  if they were daily (volatility and Sharpe about 2.2x too high with `-p 1y`).
- SIP "annualized" return was total return divided by years; it is now XIRR.
- A missing quote was valued at 0: portfolio showed -100%, `tax` booked a full
  loss, `harvest` suggested selling, `wealth` saved a fake crash, and alerts
  fired. These holdings now show N/A and are left out.
- Re-running `import` or `trade holdings` doubled positions and reset dates;
  broker symbols like `RELIANCE-EQ` became `RELIANCE-EQ.NS`.
- Angel One credentials were saved world-readable; they are now owner-only
  (0600) and existing files are tightened. `trade setup` no longer suggests
  passing secrets as command-line arguments.
- Broker holdings/positions/order-book failures showed "No holdings found"
  instead of an error; broker calls now have timeouts.
- Index, FX and crypto symbols (`^NSEI`, `USDINR=X`, `BTC-USD`) got `.NS`
  appended in the Indian market.
- The Yahoo session token is validated; a rate-limit page used to be stored
  as the token and break every later request. Retries honour `Retry-After`.
- Volume bars drew the same glyph on every row instead of stacking.
- `--help` said the default market was US (it is India).
- `screen bluechip` used a $100B threshold for INR market caps.
- `dashboard` labelled indices by position, not symbol.
- `tax` printed a fixed "FY 2025-26"; it now shows the current tax year.
- `TATAMOTORS.NS` (no longer quoted) replaced with `TMPV.NS` in the universe.
- Piping output into `head` no longer panics.

## [0.2.0] - 2026-06-24

First public release — published to crates.io with prebuilt binaries.

### Added
- **Packaging & distribution**: published to crates.io (`cargo install stockwise`),
  prebuilt binaries for macOS (x86_64/arm64), Linux, and Windows on each release.
- **CI**: GitHub Actions running `rustfmt`, `clippy -D warnings`, and the full test
  suite (213 tests) on Linux/macOS/Windows plus an MSRV (1.88) job.
- **Project docs**: `LICENSE` (MIT), `DISCLAIMER.md`, `ROADMAP.md`, `CHANGELOG.md`,
  `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, and issue/PR templates.
- Multi-source news collation for `news`, `insider`, `ipo`, and `sentiment`.

### Changed
- Migrated to Rust edition 2024; MSRV is now 1.88 (the codebase uses let-chains).
- Codebase is `rustfmt`- and `clippy`-clean (enforced in CI).
- `risk` now defaults to a `1y` period so it has enough data to run.
- Backtest Sharpe is now a per-trade ratio (no longer annualized from a
  trade-level equity curve, which produced meaningless values).

### Fixed
- Corrected README counts (57 commands, 213 tests; was "89 tests / 50+").
- **Short trades**: `sim` status/settle and the daemon now compute P&L and
  WIN/STOP/TARGET verdicts correctly for SELL positions (previously inverted).
- **Tax**: LTCG/STCG classification now compares real dates instead of
  year+month, so the day-of-month no longer flips ~11-month holds to long-term.
- **Daemon risk**: realized P&L from the booked half at T1 is now tracked, so
  the daily-loss kill switch and EOD totals are no longer understated; wind-down
  stop tightening and breakeven moves now handle short positions.
- **Panics**: `vwap`/`atr` no longer index out of bounds on ragged OHLCV data;
  every `partial_cmp().unwrap()` sort is now NaN-safe.
- **Division by zero / NaN**: guarded in `daily_returns`, `value_at_risk`,
  backtest returns, `sip`, `stoploss`, `rotation`, dividend/harvest P&L, and
  swing-point detection.
- `markets` now matches index labels to quotes by symbol (Yahoo can reorder
  results); `whatif` pairs timestamps with closes so the buy price stays aligned.
- Live order placement warns instead of going silent when no order id is
  returned, preventing accidental resubmits; `triggerprice` is only sent on
  stop-loss order types.
- `strip_think_tags` (AI output) no longer corrupts text when a stray closing
  tag precedes the opening one.

## [0.1.0] - Unreleased

Initial development. Not published. Highlights that landed during this phase:

### Added
- 57 commands spanning analysis, screening, trading bots, live trading, and
  portfolio management for US and Indian (NSE/BSE) markets.
- Daemon workers: continuous intraday bot (9 strategies, Kelly sizing, risk
  controls) and a daily long-term worker (6-pillar scoring + Monte Carlo).
- Paper-trading simulator with confidence rating.
- Live trading via Angel One; broker import for Zerodha Kite, INDmoney, and CSV.
- Local AI integration via Ollama, auto-injected into 6 commands.
- Redis response caching with in-memory fallback.
- Rate limiting, retry-with-backoff, and response caching for the Yahoo Finance
  data source.
- Multi-timeframe and braille terminal charts; `-p/--period` flag on chart commands.
- `deep` command — full report on a single stock in one shot.

[Unreleased]: https://github.com/iamsaquib8/stockwise/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/iamsaquib8/stockwise/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/iamsaquib8/stockwise/releases/tag/v0.2.0
[0.1.0]: https://github.com/iamsaquib8/stockwise/commits/master

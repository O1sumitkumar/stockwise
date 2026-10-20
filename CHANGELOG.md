# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `stockwise tui` — a full-screen, read-only live dashboard: indices,
  watchlist, portfolio P&L, triggered alerts and a chart of the selected
  symbol (periods 1d–5y), with symbol search to add to the watchlist. Refresh
  interval with `-i` (default 30s); works at 80×24 and degrades on smaller
  terminals; honours `NO_COLOR`. See [docs/tui.md](docs/tui.md).
- `stockwise serve` — a read-only JSON API (`/api/quote`, `/api/history/{symbol}`,
  `/api/markets`, `/api/search`, `/api/portfolio`, `/api/watchlist`,
  `/api/alerts`), `/healthz`, and Prometheus `/metrics` (request counts and
  latency, Yahoo errors, cache hits, and the intraday daemon's open paper
  positions, day P&L and kill switch). Loopback only unless `--allow-remote`
  with a `STOCKWISE_API_TOKEN` bearer token. See [docs/server.md](docs/server.md).
- Cargo features `tui` and `server` (both default); `--no-default-features`
  builds without ratatui and axum.
- `--json` on 17 more commands: `portfolio`, `watch`,
  `alert list`/`check`, `technical`, `risk`, `backtest`, `screen`, `movers`,
  `sectors`, `compare`, `correlate`, `sip`, `whatif`, `returns`, `dividends`,
  `tax` and `sim history`, alongside `quote`, `history`, `markets` and
  `search`. Keys are snake_case; shapes are documented in docs/commands.md.
- `--csv` (RFC 4180, header row first) for tables: `portfolio`, `watch`,
  `alert list`/`check`, `movers`, `screen`, `sectors`, `history` bars,
  `backtest` trades, `compare`, `correlate`, `returns`, `tax`, `quote`,
  `markets` and `search`. `--json` and `--csv` are mutually exclusive.
- Config file (`config.toml` in the platform config directory, or
  `STOCKWISE_CONFIG`) for `market`, `ai`, `model`, `ollama_host`,
  `redis_url`, `home`, and a default `-p` per command under `[periods]`.
  Precedence: flag > environment variable > config file > default, and
  `--help` shows defaults from the file.
- `stockwise config [show]`, `config path` and `config init [--force]`.
- `--no-ai` turns the AI section off when `STOCKWISE_AI` or the config file
  turn it on.
- `doctor` reports the config file and whether it parsed.

### Changed
- `backtest` with too little history is an error (exit 1) instead of a
  message with exit 0.
- `movers` leaves out stocks without a change today instead of ranking them
  at 0%; `correlate` shows N/A instead of 0 when two series can't be compared.
- `watch` lists symbols that have no live quote (as N/A) instead of hiding
  them; `dividends` errors when there is no live price instead of dividing by
  zero.
- An invalid config file stops every command except `config`, `doctor` and
  `completions`, with an error naming the file and the key.
- `quote`, `history` and `markets` gather their data through a shared layer
  (`src/views.rs`) that the TUI and the server also use, so `--json` and the
  HTTP API return the same shapes.

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

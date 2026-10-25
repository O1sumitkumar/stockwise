# Architecture

## Source Layout

```
src/
  main.rs            CLI definition (clap): 60 subcommands, grouped help, parse tests
  commands/          Command handlers — one file per area, each `cmd_*` re-exported
    mod.rs           Shared imports and helpers (period → interval, symbol resolution)
    overview.rs      quote, markets, dashboard, search, doctor
    setup.rs         config path/show/init
    fundamentals.rs  analyze, compare, peers, dividends, insider, deep
    technicals.rs    technical, history, chart, timeframes, patterns, support, fibs, volume, gaps, stoploss
    risk.rs          risk, correlate, returns, forecast, options moves
    funds.rs         mf search, nav, history, sip, screen, overlap
    fno.rs           options chain, pcr, maxpain
    markets.rs       movers, sectors, news, sentiment, screen, picks, earnings, rotation, matrix, surprise, heatmap, sectorcmp, ipo
    backtests.rs     backtest (single, portfolio, walkforward, strategies)
    strategies.rs    sip, whatif, intraday, longterm, report
    portfolio.rs     portfolio (show, add, sell, lots, actions, list) and valuation in a base currency
    holdings.rs      watch, alert, import, export, wealth, rebalance, harvest, tax, divcal, pcorr
    imports.rs       Groww / Upstox CSV and CAS (casparser JSON/CSV) import parsers
    sim.rs           paper-trading simulator (settle replays intraday bars)
    worker.rs        daemon intraday / longterm loops
    alerting.rs      alert watch loop, notify status/test
    trade.rs         Live orders for every broker: place_live_order (validation, preview, cap, confirmation)
    interfaces.rs    tui and serve (behind the `tui` / `server` cargo features)
  views.rs           Data behind quote/history/markets/portfolio/watchlist/alerts as structs,
                     shared by the CLI printers, the TUI and the server; `DataSource` trait
  tui/               `stockwise tui`: app.rs (state + key handling, no I/O), ui.rs (ratatui
                     drawing), mod.rs (event loop, fetches)
  server/            `stockwise serve`: axum routes, bearer auth, Prometheus metrics; web.rs serves
                     the dashboard (web/, embedded with include_str!) and sets CSP/security headers
  daemon_status.rs   Reads the intraday daemon's daemon_state.json for /metrics
  store.rs           Atomic JSON persistence, data directory (STOCKWISE_HOME)
  config.rs          Optional config.toml: defaults for market, AI, cache, data dir, periods
  output.rs          --json / --csv rendering (Format, CsvRow, RFC 4180 quoting)
  portfolio.rs       Lots, FIFO sales, splits/dividends, XIRR cash flows, FX, named portfolios
  tax.rs             Capital-gains rules: holding periods, tax years, set-off (India/US/UK)
  api.rs             Yahoo Finance client (session, quotes, charts, search, cache, retries); routes to a provider
  providers/         MarketData trait + opt-in Alpha Vantage, Massive (Polygon), Twelve Data; Yahoo fills gaps
  i18n.rs            Typed English/Hindi message catalog, --lang / STOCKWISE_LANG / locale detection
  brokers/           Broker trait + clients: angel (SmartAPI, TOTP), zerodha, upstox, dhan, fyers
                     (experimental), secrets.rs (OS keyring / owner-only file), mock.rs (test HTTP server)
  funds.rs           AMFI NAV file + api.mfapi.in history (cached), returns, SIP, ETF holdings overlap
  derivatives.rs     Option chains from NSE / Yahoo, PCR, max pain, per-contract IV and greeks
  greeks.rs          Black-Scholes pricing, greeks, implied-volatility solver
  ai.rs              AI: Ollama (default) or opt-in OpenAI / Anthropic / Gemini; prompts, opt-in flag, model checks
  clock.rs           Exchange clock: IST / US Eastern, NSE + NYSE holidays, session phases
  daemon.rs          Worker state: positions, risk manager, daemon_state.json, shutdown, retry
  notify.rs          Notifications: Telegram, Slack, Discord, SMTP email, desktop
  technical.rs       Indicators and statistics (SMA, EMA, RSI, MACD, BB, ATR, VWAP, OBV, VaR, Sharpe, XIRR, ...)
  backtest/          Backtesting engine (see docs/backtesting.md)
    mod.rs           Daily bars from Yahoo charts
    rules.rs         TOML strategy rules: parser, validation, signals; built-ins from examples/strategies/
    indicators.rs    Whole-series indicators matching technical.rs
    costs.rs         Indian delivery/intraday and US cost models, slippage
    engine.rs        Simulator (single symbol or portfolio, fills, stops, rebalancing) and metrics
    walkforward.rs   Rolling train/test optimisation and the overfitting verdict
  intraday.rs        Intraday bot (9 strategies, regime detection, Kelly sizing)
  longterm.rs        Long-term bot (6-pillar scoring, Monte Carlo, DRIP)
  simulator.rs       Paper-trading sessions and confidence rating
  insights.rs        Rule-based plain-English insights
  charts.rs          Braille line charts, volume bars, gauges
  display.rs         Currency- and asset-aware formatting (₹/$/€/£/¥, FX and crypto decimals), width-aware padding
  market.rs          Market enum (in/us/crypto/fx), asset classes, symbol resolution, index/sector/coin/pair lists
  alerts.rs          Alert conditions (price, RSI, MA crosses, breakouts, day change, volume, results)
                     and their evaluation on quotes / daily bars; persisted via store.rs
  calendar.rs        Results, dividend and split dates: Yahoo calendarEvents + NSE board meetings and
                     corporate actions (one rate-limited session), day cache in calendar_cache.json
  watchlist.rs, wealth.rs   Data models persisted via store.rs
```

Adding a command: define it in `Commands` in `main.rs` (and in the `GROUPS`
help table — a test enforces this), write `cmd_<name>` in the matching
`commands/*.rs` file, and dispatch to it in `main()`. For machine-readable
output, flatten `TableOutput` (`--json`/`--csv`) or `JsonOutput` (`--json`)
into the command, build the rows once as `Serialize` structs, and branch on
`output::Format`; shape tests live in `commands/output_tests.rs`.

## Configuration

`main()` loads the config file before parsing arguments (`config::load`) and
turns its `market`, `ai` and `[periods]` values into clap defaults, so flags and
env vars still win and `--help` shows the effective default. Other settings
are read through `config::setting(env_var, ...)`: env var first, then the file,
then the caller's built-in default. Precedence is always flag > env var >
config file > default.

## Data Flow

```
User Input → clap CLI → commands/* → views.rs → api.rs → Yahoo Finance
              tui/, server/ ───────────↗
                                   → funds.rs → AMFI NAVAll.txt, api.mfapi.in
                                   → derivatives.rs → NSE option chain (India), Yahoo (US)
                                            → providers/* → Alpha Vantage / Massive / Twelve Data (opt-in)
                                   → brokers/* → Angel One, Kite, Upstox, Dhan, Fyers
                                   → ai.rs → Local Ollama (or opt-in OpenAI / Anthropic / Gemini)
                                   → technical.rs (indicators)
                                   → intraday.rs / longterm.rs (bots)
                                   → charts.rs (visualization)
                                   → display.rs (formatting)
                                   → store.rs (portfolio, watchlist, alerts, history)
```

## Caching

Hybrid cache: Redis (primary) + in-memory (fallback).

```
Request → Check Redis → Check Memory → Fetch Yahoo → Store Redis + Memory → Return
```

| Data | TTL | Why |
|------|-----|-----|
| Quotes | 30s | Live prices, short freshness |
| Charts (1d/5d) | 60s | Intraday data |
| Charts (1mo/3mo) | 5 min | Historical, moderate |
| Charts (1y+) | 10 min | Rarely changes |
| Search | 2 min | Static-ish |

Redis keys use `stockwise:` prefix. Cache persists across CLI invocations — run `stockwise deep RELIANCE`, then `stockwise technical RELIANCE` and chart data comes from Redis.

```bash
brew services start redis       # Enable Redis caching
redis-cli keys "stockwise:*"    # See cached entries
redis-cli --scan --pattern 'stockwise:*' | xargs redis-cli del   # Clear only StockWise keys
```

Falls back to an in-memory cache if Redis isn't running. Connection and response timeouts
(300/500 ms) keep a missing or hung Redis from slowing commands. Point at another server with
`STOCKWISE_REDIS_URL` (or `redis_url` in the config file), or set it to `off`.

Rate limiting: 200ms minimum between requests (5 req/sec). Retry: 3 attempts with exponential backoff; HTTP 429 honours `Retry-After` (capped at 10s) and never sleeps after the last attempt.

## Data Storage

Location: `~/Library/Application Support/stockwise/` (macOS), `~/.local/share/stockwise/` (Linux), or
`%LOCALAPPDATA%\stockwise\` (Windows). Set `STOCKWISE_HOME` (or `home` in the config file) to use another directory;
`stockwise doctor` prints the active path. Files are written atomically (temp file + rename), and
broker config files are readable only by you (0600 on Unix). Broker secrets move into
the OS keyring when one is available (`STOCKWISE_KEYRING=off` keeps them in the file).

| File | Purpose |
|------|---------|
| `portfolio.json` | Default portfolio: holdings with lots (qty, price, purchase date, source), realized sales, dividends. Format version 2; older files are upgraded on load and kept once as `portfolio.json.bak` |
| `portfolios/NAME.json` | Named portfolios (`--portfolio NAME`), same format |
| `portfolios/NAME.wealth_history.json` | Wealth history of a named portfolio |
| `watchlist.json` | Watched symbols |
| `alerts.json` | Price alerts (symbol, condition, target) |
| `wealth_history.json` | Daily value snapshots of the default portfolio |
| `sim_history.json` | Paper trading sessions and results |
| `daemon_state.json` / `daemon_state_us.json` | The intraday daemon's current day (positions, risk), saved every tick |
| `alert_watch.json` | Which alerts `alert watch` already announced |
| `angel_config.json` | Angel One API credentials (secrets in the OS keyring when available) |
| `zerodha_config.json`, `upstox_config.json`, `dhan_config.json`, `fyers_config.json` | Experimental broker apps and daily sessions (secrets in the keyring when available) |
| `cache/` | AMFI NAV file and per-scheme NAV history, re-downloaded after 12 hours (safe to delete) |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing (derive macros) |
| `clap_complete` / `clap_mangen` | Shell completions and man pages generated from the CLI definition |
| `reqwest` | HTTP client (Yahoo Finance, brokers, Ollama) |
| `keyring` | OS keyring for broker secrets |
| `async-trait` | `dyn Broker` with async methods |
| `sha2` / `flate2` | Kite/Fyers login checksums; gzipped instrument lists |
| `rpassword` | Hidden prompt for pasted tokens |
| `redis` | Redis caching (optional, falls back to in-memory) |
| `tokio` | Async runtime |
| `serde` / `serde_json` | JSON serialization |
| `colored` | Terminal colors |
| `chrono` | Date handling |
| `dirs` | Platform-specific data directories |
| `anyhow` | Error handling |
| `lettre` | SMTP email notifications (rustls) |
| `notify-rust` | Desktop notifications (optional `desktop-notify` feature) |
| `ratatui` | Terminal UI for `tui` (re-exports crossterm; `tui` feature) |
| `axum` | HTTP server for `serve` (`server` feature) |

Both features are on by default; `cargo build --no-default-features` leaves
them out. CI-style checks should run clippy for both builds.

## Releases and packaging

Pushing a `v*` tag runs `.github/workflows/release.yml`: it builds the five release
archives (binary, docs, and `man/*.1` from `stockwise man --dir`), publishes the crate,
renders the Homebrew, Scoop, winget and AUR manifests in `packaging/` with the release's
checksums, updates the Homebrew tap when its token is configured, and pushes the
multi-arch `ghcr.io/iamsaquib8/stockwise` image built from `Dockerfile`. `flake.nix`
builds the same crate with Nix. See [packaging/README.md](../packaging/README.md).

## Testing

61 unit tests across 6 modules:

```bash
cargo test              # Run all tests
cargo test technical    # Run only technical indicator tests
cargo test longterm     # Run only longterm bot tests
```

Tested modules: `technical` (18), `backtest` (11), `display` (10), `charts` (9), `longterm` (9), `market` (4).

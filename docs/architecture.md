# Architecture

## Source Layout

```
src/
  main.rs            CLI definition (clap): 60 subcommands, grouped help, parse tests
  commands/          Command handlers — one file per area, each `cmd_*` re-exported
    mod.rs           Shared imports and helpers (period → interval, symbol resolution)
    overview.rs      quote, markets, dashboard, search, doctor
    fundamentals.rs  analyze, compare, peers, dividends, insider, deep
    technicals.rs    technical, history, chart, timeframes, patterns, support, fibs, volume, gaps, stoploss
    risk.rs          risk, correlate, returns, forecast, options
    markets.rs       movers, sectors, news, sentiment, screen, picks, earnings, rotation, matrix, surprise, heatmap, sectorcmp, ipo
    strategies.rs    backtest, sip, whatif, intraday, longterm, report
    holdings.rs      portfolio, watch, alert, import, export, wealth, rebalance, harvest, tax, divcal, pcorr
    imports.rs       Groww / Upstox CSV and CAS (casparser JSON/CSV) import parsers
    sim.rs           paper-trading simulator
    worker.rs        daemon intraday / longterm loops
    trade.rs         Live orders for every broker: place_live_order (validation, preview, cap, confirmation)
  store.rs           Atomic JSON persistence, data directory (STOCKWISE_HOME)
  api.rs             Yahoo Finance client (session, quotes, charts, search, cache, retries)
  brokers/           Broker trait + clients: angel (SmartAPI, TOTP), zerodha, upstox, dhan, fyers
                     (experimental), secrets.rs (OS keyring / owner-only file), mock.rs (test HTTP server)
  ai.rs              Ollama integration (prompts, opt-in flag, model checks)
  daemon.rs          Worker state: market phases, positions, risk manager
  technical.rs       Indicators and statistics (SMA, EMA, RSI, MACD, BB, ATR, VWAP, OBV, VaR, Sharpe, XIRR, ...)
  backtest.rs        Backtesting engine (6 strategies, equity curves)
  intraday.rs        Intraday bot (9 strategies, regime detection, Kelly sizing)
  longterm.rs        Long-term bot (6-pillar scoring, Monte Carlo, DRIP)
  simulator.rs       Paper-trading sessions and confidence rating
  insights.rs        Rule-based plain-English insights
  charts.rs          Braille line charts, volume bars, gauges
  display.rs         Currency-aware formatting (₹/$/£), colours
  market.rs          Market enum, symbol resolution, index/sector lists
  portfolio.rs, watchlist.rs, alerts.rs, wealth.rs   Data models persisted via store.rs
```

Adding a command: define it in `Commands` in `main.rs` (and in the `GROUPS`
help table — a test enforces this), write `cmd_<name>` in the matching
`commands/*.rs` file, and dispatch to it in `main()`.

## Data Flow

```
User Input → clap CLI → commands/* → api.rs → Yahoo Finance
                                   → brokers/* → Angel One, Kite, Upstox, Dhan, Fyers
                                   → ai.rs → Local Ollama
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
`STOCKWISE_REDIS_URL`, or set it to `off`.

Rate limiting: 200ms minimum between requests (5 req/sec). Retry: 3 attempts with exponential backoff; HTTP 429 honours `Retry-After` (capped at 10s) and never sleeps after the last attempt.

## Data Storage

Location: `~/Library/Application Support/stockwise/` (macOS), `~/.local/share/stockwise/` (Linux), or
`%LOCALAPPDATA%\stockwise\` (Windows). Set `STOCKWISE_HOME` to use another directory;
`stockwise doctor` prints the active path. Files are written atomically (temp file + rename), and
broker config files are readable only by you (0600 on Unix). Broker secrets move into
the OS keyring when one is available (`STOCKWISE_KEYRING=off` keeps them in the file).

| File | Purpose |
|------|---------|
| `portfolio.json` | Holdings (symbol, shares, avg cost, date) |
| `watchlist.json` | Watched symbols |
| `alerts.json` | Price alerts (symbol, condition, target) |
| `wealth_history.json` | Daily portfolio value snapshots |
| `sim_history.json` | Paper trading sessions and results |
| `angel_config.json` | Angel One API credentials (secrets in the OS keyring when available) |
| `zerodha_config.json`, `upstox_config.json`, `dhan_config.json`, `fyers_config.json` | Experimental broker apps and daily sessions (secrets in the keyring when available) |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing (derive macros) |
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

## Testing

61 unit tests across 6 modules:

```bash
cargo test              # Run all tests
cargo test technical    # Run only technical indicator tests
cargo test longterm     # Run only longterm bot tests
```

Tested modules: `technical` (18), `backtest` (11), `display` (10), `charts` (9), `longterm` (9), `market` (4).

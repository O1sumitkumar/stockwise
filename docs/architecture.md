# Architecture

## Source Layout

```
src/
  main.rs          CLI definition (clap) — 50+ subcommands
  commands.rs      All command implementations (~4000 lines)
  daemon.rs        Continuous market workers (phases, position tracking, risk manager)
  api.rs           Yahoo Finance client (cookie/crumb auth, quotes, charts, search)
  angel.rs         Angel One SmartAPI (login, TOTP, orders, holdings, positions)
  ai.rs            Ollama integration (6 prompt types, think-tag stripping, model config)
  technical.rs     30+ indicators (SMA, EMA, RSI, MACD, BB, ATR, VWAP, OBV, A/D, Fibonacci, pivots, VaR, Sharpe, Sortino, Calmar, correlation, gap detection)
  backtest.rs      Backtesting engine (6 strategies, trade simulation, equity curves)
  intraday.rs      Intraday bot (9 strategies, regime detection, Kelly sizing, sector heat)
  longterm.rs      Long-term bot (6-pillar scoring, moat analysis, Monte Carlo, DRIP)
  simulator.rs     Paper trading (sessions, daily P&L, cumulative stats, confidence rating)
  insights.rs      Rule-based investment insight generation
  charts.rs        Braille line charts, dual overlays, volume bars, bar charts, gauges
  display.rs       Currency-aware formatting (₹/$/£), sparklines, colors
  market.rs        Market enum, symbol resolver, sector/index definitions, stock lists
  portfolio.rs     Portfolio persistence (JSON)
  watchlist.rs     Watchlist persistence (JSON)
  alerts.rs        Price alert storage and checking
  wealth.rs        Daily portfolio snapshots
```

## Data Flow

```
User Input → clap CLI → commands.rs → api.rs → Yahoo Finance
                                   → angel.rs → Angel One
                                   → ai.rs → Local Ollama
                                   → technical.rs (indicators)
                                   → intraday.rs / longterm.rs (bots)
                                   → charts.rs (visualization)
                                   → display.rs (formatting)
                                   → portfolio.rs / watchlist.rs (persistence)
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
`angel_config.json` is readable only by you (0600 on Unix).

| File | Purpose |
|------|---------|
| `portfolio.json` | Holdings (symbol, shares, avg cost, date) |
| `watchlist.json` | Watched symbols |
| `alerts.json` | Price alerts (symbol, condition, target) |
| `wealth_history.json` | Daily portfolio value snapshots |
| `sim_history.json` | Paper trading sessions and results |
| `angel_config.json` | Angel One API credentials |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing (derive macros) |
| `reqwest` | HTTP client (Yahoo Finance, Angel One, Ollama) |
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

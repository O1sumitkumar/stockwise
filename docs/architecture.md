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
    risk.rs          risk, correlate, returns, forecast, options moves
    funds.rs         mf search, nav, history, sip, screen, overlap
    fno.rs           options chain, pcr, maxpain
    markets.rs       movers, sectors, news, sentiment, screen, picks, earnings, rotation, matrix, surprise, heatmap, sectorcmp, ipo
    strategies.rs    backtest, sip, whatif, intraday, longterm, report
    holdings.rs      portfolio, watch, alert, import, export, wealth, rebalance, harvest, tax, divcal, pcorr
    sim.rs           paper-trading simulator
    worker.rs        daemon intraday / longterm loops
    trade.rs         Angel One orders: instrument resolution, preview, confirmation
  store.rs           Atomic JSON persistence, data directory (STOCKWISE_HOME)
  api.rs             Yahoo Finance client (session, quotes, charts, search, cache, retries); routes to a provider
  providers/         MarketData trait + opt-in Alpha Vantage, Massive (Polygon), Twelve Data; Yahoo fills gaps
  i18n.rs            Typed English/Hindi message catalog, --lang / STOCKWISE_LANG / locale detection
  angel.rs           Angel One SmartAPI (login, TOTP, exact instrument lookup, orders)
  funds.rs           AMFI NAV file + api.mfapi.in history (cached), returns, SIP, ETF holdings overlap
  derivatives.rs     Option chains from NSE / Yahoo, PCR, max pain, per-contract IV and greeks
  greeks.rs          Black-Scholes pricing, greeks, implied-volatility solver
  ai.rs              AI: Ollama (default) or opt-in OpenAI / Anthropic / Gemini; prompts, opt-in flag, model checks
  daemon.rs          Worker state: market phases, positions, risk manager
  technical.rs       Indicators and statistics (SMA, EMA, RSI, MACD, BB, ATR, VWAP, OBV, VaR, Sharpe, XIRR, ...)
  backtest.rs        Backtesting engine (6 strategies, equity curves)
  intraday.rs        Intraday bot (9 strategies, regime detection, Kelly sizing)
  longterm.rs        Long-term bot (6-pillar scoring, Monte Carlo, DRIP)
  simulator.rs       Paper-trading sessions and confidence rating
  insights.rs        Rule-based plain-English insights
  charts.rs          Braille line charts, volume bars, gauges
  display.rs         Currency- and asset-aware formatting (₹/$/€/£/¥, FX and crypto decimals), width-aware padding
  market.rs          Market enum (in/us/crypto/fx), asset classes, symbol resolution, index/sector/coin/pair lists
  portfolio.rs, watchlist.rs, alerts.rs, wealth.rs   Data models persisted via store.rs
```

Adding a command: define it in `Commands` in `main.rs` (and in the `GROUPS`
help table — a test enforces this), write `cmd_<name>` in the matching
`commands/*.rs` file, and dispatch to it in `main()`.

## Data Flow

```
User Input → clap CLI → commands/* → api.rs → Yahoo Finance
                                   → funds.rs → AMFI NAVAll.txt, api.mfapi.in
                                   → derivatives.rs → NSE option chain (India), Yahoo (US)
                                            → providers/* → Alpha Vantage / Massive / Twelve Data (opt-in)
                                   → angel.rs → Angel One
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
| `cache/` | AMFI NAV file and per-scheme NAV history, re-downloaded after 12 hours (safe to delete) |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing (derive macros) |
| `clap_complete` / `clap_mangen` | Shell completions and man pages generated from the CLI definition |
| `reqwest` | HTTP client (Yahoo Finance, Angel One, Ollama) |
| `redis` | Redis caching (optional, falls back to in-memory) |
| `tokio` | Async runtime |
| `serde` / `serde_json` | JSON serialization |
| `colored` | Terminal colors |
| `chrono` | Date handling |
| `dirs` | Platform-specific data directories |
| `anyhow` | Error handling |

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

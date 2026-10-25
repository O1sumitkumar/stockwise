# HTTP API and metrics (`stockwise serve`)

`stockwise serve` exposes StockWise data as JSON for remote dashboards,
spreadsheets, Grafana or Home Assistant, plus Prometheus metrics for the
server and the intraday daemon.

```bash
stockwise serve                          # http://127.0.0.1:8787; Ctrl+C stops it
stockwise serve --bind 127.0.0.1:9000    # another port (or STOCKWISE_BIND)
stockwise serve -m us                    # bare tickers are US symbols
```

**Read-only.** There are no endpoints that trade, change your portfolio,
watchlist or alerts, or touch the broker. Broker credentials are never read.

## Endpoints

All responses are JSON except `/metrics`. Bare tickers follow the server's
market (`-m`, default India: `TCS` → `TCS.NS`); `quote`, `history`,
`markets` and `portfolio` accept `?market=in|us|crypto|fx` to override it per
request.

| Endpoint | Returns | Same as |
|----------|---------|---------|
| `GET /api/quote?symbols=RELIANCE,TCS` | Array of Yahoo quote objects (camelCase fields). Up to 50 symbols. Unknown symbols are listed in the `X-Stockwise-Missing` header; 404 if none are known | `quote --json` |
| `GET /api/history/{symbol}?period=1mo` | `{symbol, currency, period, interval, bars: [{time, open, high, low, close, volume}]}`. Periods as in the CLI; default `1mo` | `history --json` |
| `GET /api/markets?crypto=true&fx=true` | Array of `{region, symbol, name, price, change, change_percent, currency}`, your market first; `crypto` / `fx` add those sections | `markets --json` |
| `GET /api/search?q=tata motors` | Array of `{symbol, shortname, longname, quoteType, exchange}` | `search --json` |
| `GET /api/portfolio?base=usd` | Holdings (lots, FIFO cost) valued at live prices, totals in one currency (below); `base` is `inr` or `usd` | `portfolio --json` |
| `GET /api/watchlist` | Array of `{symbol, price, change, change_percent, volume, currency}` in watchlist order | `watch --json` |
| `GET /api/alerts` | Array of `{index, symbol, condition, target, period, description, price, currency, value, reference, status}`; `status` is `triggered`, `waiting`, `no_price` or `no_data` (technical alert with too little history) | `alert check --json` |
| `GET /healthz` | `{"status": "ok", "version": "..."}` — never needs a token | |
| `GET /metrics` | Prometheus text format (below) | |

`/api/portfolio`:

```json
{
  "holdings": [
    {"symbol": "TCS.NS", "shares": 10.0, "avg_cost": 3500.0, "cost_basis": 35000.0,
     "price": 4100.0, "value": 41000.0, "pnl": 6000.0, "pnl_percent": 17.14,
     "weight_percent": 100.0, "xirr_percent": 15.2, "currency": "INR"}
  ],
  "totals": {"cost_basis": 35000.0, "value": 41000.0, "pnl": 6000.0,
             "pnl_percent": 17.14, "currency": "INR"},
  "unpriced": [],
  "unconverted": []
}
```

A holding's money values are in its own currency; `totals` and
`weight_percent` are converted to the base currency at the live rate. A
holding without a live price has `null` price, value, P&L and weight, is
listed in `unpriced`, and is left out of the totals — it is never valued at 0.
One whose currency couldn't be converted is listed in `unconverted`. The
server uses the active portfolio (`--portfolio` / `STOCKWISE_PORTFOLIO` when
it was started).
Likewise an alert without a price has `"price": null, "status": "no_price"`
and never counts as triggered; a technical alert without enough daily bars
reads `"status": "no_data"`.

Errors are `{"error": "..."}` with status 400 (bad parameter), 401 (missing
or wrong token), 403 (request not addressed to localhost while no token is
set, see below), 404 (unknown symbol or endpoint), 405 (not GET), 500 (a local
data file such as `portfolio.json` could not be read; details only in the
server's log), or 502 (market data could not be fetched). Responses carry
`Cache-Control: no-store`.

```bash
curl -s 'localhost:8787/api/quote?symbols=RELIANCE,TCS' | jq '.[] | {symbol, regularMarketPrice}'
curl -s 'localhost:8787/api/history/INFY?period=6mo' | jq '.bars | map(.close) | max'
curl -s localhost:8787/api/portfolio | jq '.totals | {value, pnl_percent}'
curl -s localhost:8787/api/alerts | jq '.[] | select(.status == "triggered")'
```

## Prometheus metrics

```
stockwise_build_info{version="0.3.0"} 1
stockwise_uptime_seconds 3600.1
stockwise_http_requests_total{method="GET",route="/api/quote",status="200"} 42
stockwise_http_request_duration_seconds_bucket{route="/api/quote",le="0.25"} 40
stockwise_upstream_requests_total 57          # requests sent to Yahoo, retries included
stockwise_upstream_errors_total 1             # fetches that failed after all retries
stockwise_cache_hits_total{layer="redis"} 12
stockwise_cache_hits_total{layer="memory"} 30
stockwise_cache_misses_total 57
stockwise_daemon_up{market="IN"} 1
stockwise_daemon_up{market="US"} 0
stockwise_daemon_status_age_seconds{market="IN"} 41
stockwise_daemon_open_positions{market="IN"} 3
stockwise_daemon_day_pnl{market="IN"} 412.5
stockwise_daemon_realized_pnl{market="IN"} 120
stockwise_daemon_unrealized_pnl{market="IN"} 292.5
stockwise_daemon_capital{market="IN"} 25000
stockwise_daemon_kill_switch{market="IN"} 0
stockwise_daemon_settled{market="IN"} 0
```

`route` is the route template (`/api/history/{symbol}`), so label values stay
bounded; anything that matched no route is `unmatched`, and unusual HTTP
methods are counted as `OTHER`.

**Daemon metrics.** `stockwise daemon intraday` saves its day state in the
data directory after every tick — `daemon_state.json` for NSE,
`daemon_state_us.json` for `-m us` — and the server reads both, so run them
against the same data directory (`STOCKWISE_HOME`). Every daemon gauge has a
`market` label (`IN` or `US`). `stockwise_daemon_up` is 1 if that market's
state is less than 45 minutes old (while the market is closed the daemon
saves at least every 30 minutes); the other `stockwise_daemon_*` gauges appear
once a daemon has run. The daemon paper-trades only.

```yaml
# prometheus.yml
scrape_configs:
  - job_name: stockwise
    static_configs:
      - targets: ["127.0.0.1:8787"]
    # with a token:
    # authorization: { type: Bearer, credentials: "<STOCKWISE_API_TOKEN>" }
```

Example alert: `stockwise_daemon_kill_switch{market="IN"} == 1`, or
`stockwise_daemon_up{market="IN"} == 0` during NSE hours.

## Remote access

The API shows your holdings, so it listens on loopback only by default and
refuses any other address. Without a token it also only answers requests
whose `Host` is `localhost`, `*.localhost` or a loopback IP, so a web page
can't reach it through DNS rebinding.

```text
$ stockwise serve --bind 0.0.0.0:8787
Error: Refusing to listen on 0.0.0.0:8787: other machines could read your portfolio.
```

To serve other machines, set a bearer token (16+ characters) and pass
`--allow-remote`:

```bash
export STOCKWISE_API_TOKEN=$(openssl rand -hex 32)
stockwise serve --bind 0.0.0.0:8787 --allow-remote
curl -H "Authorization: Bearer $STOCKWISE_API_TOKEN" http://my-host:8787/api/portfolio
```

When `STOCKWISE_API_TOKEN` is set, every endpoint except `/healthz` requires
it — on loopback too. The server speaks plain HTTP: across an untrusted
network, put it behind a TLS reverse proxy (Caddy, nginx) or a private
network such as Tailscale or WireGuard, so the token isn't sent in clear text.

## Rate limits and caching

All requests share one Yahoo client, so they share its rate limiter (5
requests/second), retries, and cache (quotes 30s, charts 1-10 min; Redis when
available). The Yahoo session starts on the first request that needs it and is
retried on the next request if it fails, so the server starts without
network access.

## Building without it

The server is the `server` cargo feature (axum), on by default.
`cargo install stockwise --no-default-features` leaves it out; `stockwise
serve` then exits with a message saying so.

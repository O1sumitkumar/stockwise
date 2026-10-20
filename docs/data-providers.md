# Data providers

StockWise gets market data from **Yahoo Finance** by default — no account or
key needed. You can opt in to another provider for quotes, daily history and
symbol search. Yahoo stays in the loop either way: it fills every field the
other provider lacks and answers whatever the provider can't.

```bash
export STOCKWISE_DATA_PROVIDER=twelvedata
export TWELVEDATA_API_KEY=...
stockwise doctor            # "Twelve Data answered for AAPL (180 ms)"
stockwise quote AAPL -m us  # ends with "Data: Twelve Data"
```

## Providers

| Provider | `STOCKWISE_DATA_PROVIDER` | Key variable | Free tier | Covers on the free tier |
|----------|---------------------------|--------------|-----------|-------------------------|
| Yahoo Finance | unset or `yahoo` | — | unofficial, no key | Everything StockWise does |
| Alpha Vantage | `alphavantage` | `ALPHAVANTAGE_API_KEY` | 25 requests/day; end-of-day; last 100 daily bars | US stocks, BSE (`.BO`), LSE, TSX, XETRA, Shanghai/Shenzhen, FX, crypto |
| Massive (formerly Polygon.io) | `massive` or `polygon` | `MASSIVE_API_KEY` (or `POLYGON_API_KEY`) | 5 requests/min; end-of-day; 2 years of history | US stocks, FX, crypto |
| Twelve Data | `twelvedata` | `TWELVEDATA_API_KEY` | 8 credits/min, 800/day | US stocks, FX, crypto (NSE/BSE need the paid Grow plan) |

Free-tier limits are the providers' own and change over time; check their
pricing pages. Twelve Data's free plan is licensed for internal, non-display
use only.

**Indian stocks:** none of these free tiers carries NSE, and only Alpha
Vantage carries BSE. With the default Indian market, `TCS` (`TCS.NS`) keeps
coming from Yahoo whichever provider you pick; `RELIANCE.BO` goes to Alpha
Vantage. Indices (`^NSEI`, `^GSPC`) always come from Yahoo.

## What goes where

| Data | With a provider configured |
|------|----------------------------|
| Quotes (`quote`, `dashboard`, `portfolio`, ...) | Provider for price, day range, volume and change; Yahoo for name, 52-week range, fundamentals and anything else missing. `quote` says which provider answered |
| Bulk scans (`movers`, `screen`, `heatmap`, ...) | Yahoo, when the scan has more symbols than the provider's per-run budget, so one table never mixes sources |
| Daily, weekly, monthly history | Provider, if its plan reaches back over the whole period (weekly and monthly bars are built from daily ones); otherwise Yahoo |
| Intraday bars (`-p 1d`, `-p 5d`) | Yahoo (not on any free tier) |
| `search` | Provider's matches, mapped back to Yahoo symbols (`RELIANCE.BSE` → `RELIANCE.BO`, `X:BTCUSD` → `BTC-USD`); Yahoo's when it finds none |
| Fundamentals, news, insider, earnings | Yahoo |

## Limits and fallback

- Each command spends at most a small per-run budget of provider requests —
  5 for Alpha Vantage and Massive, 8 for Twelve Data — so one scan can't burn
  a day's allowance or wait a minute for the next slot.
- If a provider reports a rate limit or refuses the key, StockWise prints one
  `ⓘ` notice on stderr, stops asking it for the rest of the run, and uses
  Yahoo. `--json` output on stdout is unaffected.
- Provider answers share the Redis / in-memory cache (quotes 1–5 minutes,
  daily history 10 minutes), so repeating a command doesn't spend another request.
  API keys are never part of a cache key or an error message.
- An unknown provider name or a missing key is reported (by every command on
  stderr, and by `doctor`) and StockWise carries on with Yahoo.
- `doctor` asks the provider for one `AAPL` quote to show it answers — one
  request of your allowance per run.

`--json` quotes from another provider carry a `"dataSource"` field naming it;
Yahoo-only quotes don't have the field.

## Adding a provider

Implement the `MarketData` trait in `src/providers/` (quote, daily series,
search), map Yahoo-style symbols to the provider's syntax and back, and add a
`ProviderKind`. Parse responses in pure functions with fixture tests, and test
the HTTP path against `testutil::MockServer` — tests must never call a real
API or need a real key.

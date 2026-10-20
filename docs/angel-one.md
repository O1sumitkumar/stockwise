# Angel One Live Trading

StockWise integrates with Angel One SmartAPI (free) for live order placement.

## Setup

1. Create an account at [angelone.in](https://www.angelone.in/)
2. Register an app at [smartapi.angelone.in](https://smartapi.angelone.in/)
3. Enable TOTP on your account and note the base32 secret

```bash
stockwise trade config
```

`trade config` creates `angel_config.json` in your data directory (it prints the
path; `stockwise doctor` shows it too) with owner-only permissions. Replace the
placeholders:

```json
{
  "api_key": "YOUR_API_KEY",
  "client_id": "YOUR_CLIENT_ID",
  "password": "YOUR_PASSWORD_OR_PIN",
  "totp_secret": "YOUR_TOTP_SECRET",
  "max_order_value": 50000
}
```

`max_order_value` is optional: any single order whose estimated value is higher
is refused. Never pass credentials as command-line arguments — they end up in
your shell history.

## Commands

```bash
stockwise trade holdings                    # Live holdings (synced into `portfolio`)
stockwise trade positions                   # Open intraday positions
stockwise trade buy RELIANCE 10 --dry-run   # Resolve + preview, send nothing
stockwise trade buy RELIANCE 10             # Market buy (asks you to type "yes")
stockwise trade sell TCS 5                  # Market sell
stockwise trade limit INFY 20 1300          # Limit buy
stockwise trade orders                      # Today's order book
```

## Order safety

Every `buy`, `sell` and `limit`:

1. Resolves the **exact** NSE cash-equity instrument (`RELIANCE` → `RELIANCE-EQ`).
   Partial names or other series never match, so a typo can't buy a different stock.
2. Prints a preview: instrument, token, side, quantity, last price, estimated value.
3. Checks `max_order_value` if you set one.
4. Asks you to type `yes`. Without a terminal (scripts, cron) it refuses unless
   you pass `--yes`.

If the network fails while the order is in flight, StockWise reports the order
state as **unknown** — check the Angel One app or `stockwise trade orders`
before retrying, so you don't place it twice.

## Security

- Credentials stored locally only (owner-only file, written atomically), never sent except to Angel One's API
- Older world-readable config files are tightened to owner-only on first load
- TOTP generated locally via built-in HMAC-SHA1 (no external dependencies)
- No data leaves your machine except direct Angel One API calls

## Broker Import

Import holdings from other brokers without manual entry:

```bash
stockwise import kite ~/Downloads/holdings.csv       # Zerodha Kite
stockwise import indmoney ~/Downloads/portfolio.csv  # IndMoney
stockwise import csv ~/Downloads/stocks.csv          # Any CSV
```

Auto-detects column names: Symbol/Instrument, Quantity/Qty, Avg Cost/Average Price.
An export is treated as a snapshot: re-importing the same file updates holdings
instead of doubling them.

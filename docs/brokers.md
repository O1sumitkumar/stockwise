# Brokers: live trading and holdings import

StockWise can read your holdings and place orders through a broker's API, and
import holdings from broker exports and CAS statements.

| Broker | `--broker` | Status | Holdings / positions / orders | MARKET, LIMIT | SL, SL-M | GTT |
|--------|-----------|--------|-------------------------------|---------------|----------|-----|
| Angel One SmartAPI | `angel` (default) | original integration | yes | yes | experimental | experimental |
| Zerodha Kite Connect | `zerodha` / `kite` | **experimental** | yes | yes | yes | single-leg |
| Upstox API v2/v3 | `upstox` | **experimental** | yes | yes | yes | not supported |
| Dhan API v2 | `dhan` | **experimental** | yes | yes | yes | single Forever Order |
| Fyers API v3 | `fyers` | **experimental** | yes | yes | yes | single-leg |

> **Experimental — untested against a live account.** The Zerodha, Upstox,
> Dhan and Fyers integrations, and stop-loss / GTT orders on Angel One, were
> built from each broker's public API documentation and tested against recorded
> and handwritten responses on a local mock server. Nobody has run them against
> a real account yet. Until you opt in, they only read (holdings, positions,
> orders) and preview (`--dry-run`); see [Turning on live orders](#turning-on-live-orders).
> Reports from real accounts are very welcome — open an issue.

Pick the broker per command with `--broker`, or set a default with
`STOCKWISE_BROKER=zerodha`. Without either, `trade` uses Angel One, as before.

```bash
stockwise trade holdings --broker zerodha
STOCKWISE_BROKER=dhan stockwise trade orders
```

## Order safety

Every order — any broker, any order type, GTT included — goes through the same
steps:

1. **Exact instrument.** The symbol must match one NSE cash-equity (`EQ`
   series) instrument in the broker's own instrument list: Angel One's
   `searchScrip`, Kite's instrument dump, Upstox's NSE instrument file, Dhan's
   NSE_EQ scrip master, Fyers' NSE_CM symbol master. Partial names, other
   series (`-BE`), ETFs with a similar name, and F&O contracts never match; two
   matches are refused.
2. **Price checks.** Prices must be positive. For stop-loss orders the trigger
   must sit on the right side of the market (above it for a BUY, below it for a
   SELL — otherwise it would fire at once), and the limit must be at or beyond
   the trigger in the order's direction. A limit more than 20% away from its
   trigger is treated as a typo. A GTT trigger must be at least 0.25% away from
   the last price. Stop and GTT orders need a last price; if it can't be
   fetched the order is refused.
3. **Preview.** Instrument and broker token, side, quantity, order type,
   product (delivery / CNC), last price, estimated value.
4. **`max_order_value`.** If set in the broker's config file, any order whose
   estimated value is higher is refused (also on `--dry-run`).
5. **`--dry-run`** stops here: nothing is sent.
6. **Opt-in for experimental paths** (`"enable_orders": true`, below).
7. **Typed confirmation.** You type `yes`. Without a terminal (scripts, cron)
   the order is refused unless you pass `--yes`.

If the network fails while an order is in flight, StockWise says the order
state is **unknown** — check the broker app or `stockwise trade orders` before
retrying, so you don't place it twice. Cancelling a GTT goes through the same
preview, opt-in and confirmation.

The paper-trading daemon never sends broker orders.

## Order types

```bash
stockwise trade buy RELIANCE 10                              # MARKET
stockwise trade buy RELIANCE 10 --limit 1300                 # LIMIT
stockwise trade limit RELIANCE 10 1300                       # LIMIT buy (0.3 form)
stockwise trade sell RELIANCE 10 --trigger 1250              # SL-M: market sell once 1250 trades
stockwise trade sell RELIANCE 10 --trigger 1250 --limit 1245 # SL: limit sell at 1245 once 1250 trades
stockwise trade buy RELIANCE 10 --trigger 1400 --limit 1405  # SL buy (breakout entry)
```

All orders are delivery (CNC) on NSE with DAY validity. MARKET and SL-M orders
carry the broker's automatic market protection where the broker requires it
(Kite, Upstox).

### GTT (good till triggered)

A single-leg GTT places a LIMIT order when the trigger trades; it stays active
for up to a year.

```bash
stockwise trade gtt sell TCS 5 --trigger 3400 --limit 3390 --broker zerodha   # stop-loss below the market
stockwise trade gtt sell TCS 5 --trigger 4200 --limit 4190                    # target above it
stockwise trade gtt buy INFY 10 --trigger 1350 --limit 1355                   # buy the dip
stockwise trade gtt list
stockwise trade gtt cancel 112127
```

Two-leg (OCO) GTTs are not supported yet. Upstox GTT rules have no limit price,
so StockWise can't express "LIMIT at this price once triggered" there and
refuses GTTs for Upstox.

## Turning on live orders

Read-only commands work as soon as a broker is configured. To send real orders
through an experimental path, first compare a `--dry-run` preview with what the
broker app shows, then add to that broker's config file:

```json
"enable_orders": true
```

This applies to every order on Zerodha, Upstox, Dhan and Fyers, and to SL,
SL-M and GTT orders on Angel One. Angel One MARKET and LIMIT orders work as in
0.3 without it.

Most Indian brokers now accept API orders only from a **static IP registered
with your app** (Kite, Upstox, Dhan, Fyers and Angel One since April 2026).
Reads work from anywhere.

## Credentials and the OS keyring

`stockwise trade config --broker <name>` creates `<name>_config.json` in your
data directory (the path is printed; `stockwise doctor` shows it too) with
owner-only permissions (0600 on Unix). Fill in the placeholders and run the
command again.

On the next command, secrets in that file — passwords, TOTP secrets, API
secrets, access tokens — **move into the OS keyring** when one is available
(macOS Keychain, Windows Credential Manager, the Secret Service on Linux
desktops), and the file is rewritten without them. It keeps the non-secret
settings plus `"secret_store": "keyring"` and the keyring entry name. Each data
directory (`STOCKWISE_HOME`) gets its own keyring entry.

- `stockwise trade config --keyring` moves them now and says why if it can't.
- With no keyring (headless Linux, containers, CI) the secrets stay in the
  owner-only file. Set `STOCKWISE_KEYRING=off` to always use the file.
- To change a secret later, put the new value in the file; it moves into the
  keyring again on the next command.
- `stockwise doctor` shows, per broker, where its secrets live.
- On macOS a rebuilt or upgraded binary may trigger a Keychain prompt the first
  time it reads an entry; choose "Always Allow". Unattended runs right after an
  upgrade may wait for that prompt.

Never pass credentials as command-line arguments — they end up in your shell
history. StockWise never prints or logs them, and sends them only to the
broker's own API.

Common optional settings in every broker config: `"max_order_value": 50000`
and `"enable_orders": true`.

## Angel One

1. Create an account at [angelone.in](https://www.angelone.in/) and an app at
   [smartapi.angelone.in](https://smartapi.angelone.in/); note its API key.
2. Enable TOTP on your account and note the base32 secret.
3. `stockwise trade config`, then fill in:

```json
{
  "api_key": "YOUR_API_KEY",
  "client_id": "YOUR_CLIENT_ID",
  "password": "YOUR_PASSWORD_OR_PIN",
  "totp_secret": "YOUR_TOTP_SECRET",
  "max_order_value": 50000
}
```

Angel One logs in with your PIN and a TOTP generated locally (built-in
HMAC-SHA1) on every command, so there is no `trade login` step. Existing
`angel_config.json` files keep working; their secrets move into the keyring on
first use.

## Zerodha Kite (experimental)

1. Create a Kite Connect app at [developers.kite.trade](https://developers.kite.trade/)
   (a paid Kite Connect plan); note the **API key** and **API secret**. Set the
   redirect URL to anything you can copy from the address bar, e.g.
   `http://127.0.0.1/`.
2. `stockwise trade config --broker zerodha` and fill in `api_key` and
   `api_secret`.
3. Every trading day: `stockwise trade login --broker zerodha`. It prints the
   Kite login URL; log in in your browser, then paste the address the browser
   was redirected to (it contains `request_token=…`). StockWise exchanges it for
   an access token (`checksum = sha256(api_key + request_token + api_secret)`)
   and saves it. Kite sessions end at 06:00 IST.

Holdings include T1 shares. Selling holdings without DDPI/POA may need a CDSL
TPIN authorisation in the Kite app first (Kite answers HTTP 428).

## Upstox (experimental)

1. Create an app at [account.upstox.com/developer/apps](https://account.upstox.com/developer/apps);
   note the **API key** and **API secret** and set a redirect URL such as
   `http://127.0.0.1/`.
2. `stockwise trade config --broker upstox`; fill in `api_key`, `api_secret`
   and the same `redirect_uri`.
3. Every trading day: `stockwise trade login --broker upstox` — log in in the
   browser, paste the redirect address (it contains `code=…`). Sessions end at
   03:30 IST. Orders use the v3 endpoint on `api-hft.upstox.com`.

## Dhan (experimental)

1. On [web.dhan.co](https://web.dhan.co) open My Profile → Access DhanHQ APIs
   and generate an **access token** (valid 24 hours).
2. `stockwise trade config --broker dhan`; put your Dhan **client id** in
   `api_key`.
3. `stockwise trade login --broker dhan` and paste the token (input is hidden).
   It is checked against your Dhan profile and saved (in the keyring when
   available). Repeat when it expires.

GTT orders on Dhan are single "Forever Orders".

## Fyers (experimental)

1. Create an app at [myapi.fyers.in/dashboard](https://myapi.fyers.in/dashboard);
   note the **App ID** (e.g. `XXXX-100`) and **Secret ID**, and set a redirect
   URL such as `http://127.0.0.1/`. Only App IDs ending in `-200` can place
   orders.
2. `stockwise trade config --broker fyers`; put the App ID in `api_key`, the
   Secret ID in `api_secret`, and the `redirect_uri`.
3. Every trading day: `stockwise trade login --broker fyers` — log in in the
   browser and paste the redirect address (it contains `auth_code=…`).
   StockWise validates it with `appIdHash = sha256(app_id:secret)`. Sessions end
   around 06:30 IST.

## Commands

```bash
stockwise trade setup [--broker B]          # How to connect
stockwise trade config [--broker B] [--keyring]
stockwise trade login --broker B            # Zerodha, Upstox, Fyers, Dhan
stockwise trade holdings                    # Live holdings (synced into `portfolio`)
stockwise trade positions                   # Open positions
stockwise trade orders                      # Today's order book
stockwise trade buy RELIANCE 10 --dry-run   # Resolve + preview, send nothing
stockwise trade sell TCS 5 --trigger 3400   # SL-M sell
stockwise trade gtt list                    # GTT orders
```

## Importing holdings

Import holdings from other brokers without manual entry:

```bash
stockwise import kite ~/Downloads/holdings.csv       # Zerodha Kite
stockwise import indmoney ~/Downloads/portfolio.csv  # INDmoney
stockwise import groww ~/Downloads/holdings.csv      # Groww (save the .xlsx as CSV)
stockwise import upstox ~/Downloads/holdings.csv     # Upstox
stockwise import cas ~/Downloads/cas.json            # CAMS / KFintech / NSDL / CDSL CAS via casparser
stockwise import csv ~/Downloads/stocks.csv          # Any CSV
```

Columns are detected by name (Symbol/Instrument/Stock Name/ISIN,
Quantity/Qty, Avg Cost/Average buy price), and leading account-detail rows are
skipped. An export is treated as a snapshot: re-importing the same file
updates holdings instead of doubling them.

- **Groww** exports holdings as `.xlsx` with no ticker column: open it, save as
  CSV, and StockWise looks up each ISIN's NSE ticker (Yahoo search, one exact
  result only). Mutual-fund ISINs that don't trade on an exchange become
  `MF:<ISIN>`.
- **Upstox** holdings exports have no published layout; StockWise detects the
  usual columns (Symbol / Scrip Name / ISIN, Qty, Avg. Price). If it can't,
  use `import csv` with Symbol, Quantity and Average Price columns — and please
  open an issue with the header row.

### CAS (consolidated account statement)

CAS PDFs are password-protected and their tables don't survive plain text
extraction, so StockWise reads the output of the open-source
[casparser](https://github.com/codereverser/casparser) instead:

```bash
pip install casparser
casparser CAS.pdf -p <password> -o cas.json      # or -o cas.csv
stockwise import cas cas.json
```

- **CAMS / KFintech** (mutual funds): each scheme with units left becomes
  `MF:<AMFI code>` (or `MF:<ISIN>`). Cost comes from the statement's cost value,
  else from the transactions (average-cost method; stamp duty included). The
  JSON and both CSV forms (transactions, `-s` summary) are supported. A summary
  CSV has no cost, so it only updates quantities of funds you already track.
- **NSDL / CDSL** (demat): equities carry no purchase cost, so they only update
  the quantity of holdings already in your portfolio (keeping your cost);
  new ones are listed with a `portfolio add` hint. Mutual funds in a demat CAS
  are imported with their average cost when it is given.
- Feeding the PDF directly prints these instructions. Mutual funds have no live
  price in StockWise yet, so `portfolio` shows them as N/A and `wealth` won't
  snapshot until they're priced or removed.

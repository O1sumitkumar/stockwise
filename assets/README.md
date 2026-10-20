# Demo assets

## Screenshots (README)

`history.svg`, `markets.svg` and `backtest.svg` are real StockWise output
rendered to SVG by `ansi2svg.py` (no dependencies beyond Python 3). To
regenerate one:

```bash
cargo build --release
export CLICOLOR_FORCE=1   # keep colours when piping
(printf '$ stockwise markets\n'; ./target/release/stockwise markets) \
  | python3 assets/ansi2svg.py assets/markets.svg "stockwise — markets"
```

A first line starting with `$ ` is drawn as the typed prompt. Output depends on
live market data, so each capture differs.

## Animated demo (optional)

`demo.tape` records a GIF with [VHS](https://github.com/charmbracelet/vhs):

```bash
brew install vhs
cargo build --release
vhs assets/demo.tape      # writes assets/demo.gif
```

Both use **read-only** commands only — never live `trade` or `daemon` flows.

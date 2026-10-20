# Assets

## Brand

- `logo.svg` — square app icon (256×256).
- `banner.svg` — README header.
- `social-preview.svg` / `.png` — 1280×640 card for link previews. Upload the PNG
  in GitHub → Settings → General → Social preview.

To re-render the PNG after editing the SVG (needs Google Chrome):

```bash
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new \
  --hide-scrollbars --window-size=1280,640 \
  --screenshot=assets/social-preview.png "file://$PWD/assets/social-preview.svg"
```

## Screenshots (README)

`history.svg`, `markets.svg`, `backtest.svg` and `tui.svg` are real StockWise output
rendered to SVG by `ansi2svg.py` (no dependencies beyond Python 3). To
regenerate one:

```bash
cargo build --release
export CLICOLOR_FORCE=1   # keep colours when piping
(printf '$ stockwise markets\n'; ./target/release/stockwise markets) \
  | python3 assets/ansi2svg.py assets/markets.svg "stockwise — markets"
```

For the TUI, print one frame instead of running it interactively:

```bash
CLICOLOR_FORCE=1 ./target/release/stockwise tui --snapshot 100x30 \
  | python3 assets/ansi2svg.py assets/tui.svg "stockwise tui"
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

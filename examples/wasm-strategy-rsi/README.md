# RSI mean reversion — a StockWise WASM strategy plugin

A complete strategy plugin in Rust, with no dependencies: the built-in `rsi`
strategy (enter when RSI(14) falls below 30, exit above 70, Wilder smoothing)
compiled to WebAssembly. StockWise's tests check that it gives exactly the
built-in's signals for every grid combination.

It is its own Cargo project, not part of the StockWise build.

## Build

```bash
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release
```

The plugin is `target/wasm32-unknown-unknown/release/wasm_strategy_rsi.wasm`
(about 60 KB).

## Run

StockWise needs the `wasm-plugins` feature:

```bash
cargo install stockwise --features wasm-plugins

P=target/wasm32-unknown-unknown/release/wasm_strategy_rsi.wasm
stockwise backtest strategies $P                 # check it loads
stockwise backtest RELIANCE -s wasm:$P -P oversold=25
stockwise backtest walkforward TCS -s wasm:$P    # tunes oversold/overbought
```

Or copy it into your strategies folder (`stockwise backtest strategies` shows
where) as `rsiw.wasm` and run `-s rsiw`.

## How it works

`src/lib.rs` exports the plugin ABI (version 1):

- `stockwise_abi_version` returns 1.
- `stockwise_meta` returns the metadata JSON: name, description, parameter
  defaults, the walk-forward grid and the warm-up length.
- `stockwise_alloc` / `stockwise_init` receive the parameters as JSON, e.g.
  `{"overbought":70.0,"oversold":30.0,"period":14.0}`, and refuse bad ones with
  a message from `stockwise_error`.
- `stockwise_on_bar(time, open, high, low, close, volume)` updates the RSI
  and returns 1 (entry), 2 (exit) or 0.

Each price series runs in a fresh instance, so the thread-local state starts
over every time. The full ABI and the sandbox limits are in
[docs/backtesting.md](../../docs/backtesting.md#wasm-plugins).

Any language that targets `wasm32-unknown-unknown` (or produces a module with
no imports) works: C, Zig, AssemblyScript, TinyGo, or hand-written WAT like
[`src/backtest/fixtures/level.wat`](../../src/backtest/fixtures/level.wat).

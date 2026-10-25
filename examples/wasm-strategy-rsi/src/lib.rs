//! RSI mean reversion as a StockWise strategy plugin (WASM plugin ABI v1).
//!
//! The same rules as the built-in `rsi` strategy: enter when RSI(`period`)
//! falls below `oversold`, exit when it rises above `overbought`, with
//! Wilder's smoothing. No dependencies, no imports: the module exports the
//! functions below and nothing else.
//!
//! ```text
//! cargo build --target wasm32-unknown-unknown --release
//! stockwise backtest RELIANCE \
//!   -s wasm:target/wasm32-unknown-unknown/release/wasm_strategy_rsi.wasm -P oversold=25
//! ```
//!
//! StockWise starts a fresh instance for every price series, calls
//! `stockwise_init` once with the parameters, then `stockwise_on_bar` for
//! each daily bar, oldest first. State lives in a thread-local: a plugin
//! runs on one thread and never sees two series in one instance.

use std::cell::RefCell;

/// What the plugin declares about itself: read by `backtest strategies`,
/// `-P` and `backtest walkforward`. `lookback` is RSI(14)'s warm-up.
const META: &str = r#"{
  "name": "rsi-wasm",
  "description": "RSI(14) mean reversion as a WASM plugin: buy below 30, sell above 70",
  "params": { "period": 14, "oversold": 30, "overbought": 70 },
  "grid": { "oversold": [25, 30, 35], "overbought": [65, 70, 75] },
  "lookback": 15
}"#;

const NONE: i32 = 0;
const ENTRY: i32 = 1;
const EXIT: i32 = 2;

#[derive(Default)]
struct Rsi {
    period: usize,
    oversold: f64,
    overbought: f64,
    prev_close: Option<f64>,
    /// Price changes seen so far, up to `period`.
    changes: usize,
    gain_sum: f64,
    loss_sum: f64,
    avg_gain: f64,
    avg_loss: f64,
}

impl Rsi {
    /// RSI after this close, once `period` changes have been seen.
    fn update(&mut self, close: f64) -> Option<f64> {
        let prev = self.prev_close.replace(close)?;
        let change = close - prev;
        let gain = change.max(0.0);
        let loss = if change > 0.0 { 0.0 } else { change.abs() };
        let p = self.period as f64;
        if self.changes < self.period {
            self.changes += 1;
            self.gain_sum += gain;
            self.loss_sum += loss;
            if self.changes < self.period {
                return None;
            }
            self.avg_gain = self.gain_sum / p;
            self.avg_loss = self.loss_sum / p;
        } else {
            self.avg_gain = (self.avg_gain * (p - 1.0) + gain) / p;
            self.avg_loss = (self.avg_loss * (p - 1.0) + loss) / p;
        }
        Some(if self.avg_loss == 0.0 {
            100.0
        } else {
            100.0 - 100.0 / (1.0 + self.avg_gain / self.avg_loss)
        })
    }
}

thread_local! {
    static STATE: RefCell<Rsi> = RefCell::new(Rsi::default());
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

/// `(pointer << 32) | length`, how strings cross to the host.
fn pack(s: &[u8]) -> i64 {
    (((s.as_ptr() as usize as u64) << 32) | s.len() as u64) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn stockwise_abi_version() -> i32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn stockwise_meta() -> i64 {
    pack(META.as_bytes())
}

/// A buffer for the host to write the parameters into; `stockwise_init`
/// takes it back.
#[unsafe(no_mangle)]
pub extern "C" fn stockwise_alloc(len: i32) -> i32 {
    let buf = vec![0u8; len.max(0) as usize].into_boxed_slice();
    Box::into_raw(buf) as *mut u8 as usize as i32
}

/// The parameters as compact JSON, e.g.
/// `{"overbought":70.0,"oversold":30.0,"period":14.0}`. Returns 0, or 1
/// with a message for `stockwise_error`.
#[unsafe(no_mangle)]
pub extern "C" fn stockwise_init(ptr: i32, len: i32) -> i32 {
    // SAFETY: the host passes back the buffer `stockwise_alloc` returned.
    let buf = unsafe {
        Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            ptr as usize as *mut u8,
            len.max(0) as usize,
        ))
    };
    match configure(&String::from_utf8_lossy(&buf)) {
        Ok(rsi) => {
            STATE.with(|s| *s.borrow_mut() = rsi);
            0
        }
        Err(e) => {
            ERROR.with(|m| *m.borrow_mut() = e);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn stockwise_on_bar(
    _time: i64,
    _open: f64,
    _high: f64,
    _low: f64,
    close: f64,
    _volume: f64,
) -> i32 {
    STATE.with(|s| {
        let mut rsi = s.borrow_mut();
        match rsi.update(close) {
            Some(v) if v < rsi.oversold => ENTRY,
            Some(v) if v > rsi.overbought => EXIT,
            _ => NONE,
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stockwise_error() -> i64 {
    ERROR.with(|m| pack(m.borrow().as_bytes()))
}

/// Read and check the parameters. The host always sends a flat object of
/// names and numbers, so splitting on `,` and `:` is enough.
fn configure(json: &str) -> Result<Rsi, String> {
    let mut rsi = Rsi::default();
    let body = json.trim().trim_start_matches('{').trim_end_matches('}');
    for pair in body.split(',').filter(|p| !p.trim().is_empty()) {
        let (key, value) = pair
            .split_once(':')
            .ok_or_else(|| format!("bad parameter `{pair}`"))?;
        let value: f64 = value
            .trim()
            .parse()
            .map_err(|_| format!("bad number in `{pair}`"))?;
        match key.trim().trim_matches('"') {
            "period" => {
                if value.fract() != 0.0 || !(1.0..=1000.0).contains(&value) {
                    return Err(format!(
                        "period must be a whole number from 1 to 1000, got {value}"
                    ));
                }
                rsi.period = value as usize;
            }
            "oversold" => rsi.oversold = value,
            "overbought" => rsi.overbought = value,
            other => return Err(format!("unknown parameter `{other}`")),
        }
    }
    if rsi.period == 0 {
        return Err("period is missing".into());
    }
    if rsi.oversold >= rsi.overbought {
        return Err(format!(
            "oversold must be below overbought ({} >= {})",
            rsi.oversold, rsi.overbought
        ));
    }
    Ok(rsi)
}

;; The smallest complete StockWise strategy plugin (ABI v1), in WebAssembly
;; text. Entry when the close is above `level`, exit when it is below; the
;; first bar never signals (lookback 1). The tests load it, and so does
;; `stockwise backtest SYMBOL -s wasm:src/backtest/fixtures/level.wat`.
(module
  (memory (export "memory") 1)

  ;; stockwise_meta's JSON at offset 0 (124 bytes).
  (data (i32.const 0) "{\"name\":\"level\",\"description\":\"Close above a fixed level\",\"params\":{\"level\":100},\"grid\":{\"level\":[90,100,110]},\"lookback\":1}")
  ;; stockwise_error's message at offset 512 (36 bytes).
  (data (i32.const 512) "level must be a whole number above 0")

  (global $level (mut f64) (f64.const 0))
  (global $seen (mut i32) (i32.const 0))

  (func (export "stockwise_abi_version") (result i32)
    (i32.const 1))

  ;; (pointer << 32) | length
  (func (export "stockwise_meta") (result i64)
    (i64.const 124))

  (func (export "stockwise_error") (result i64)
    (i64.or (i64.shl (i64.const 512) (i64.const 32)) (i64.const 36)))

  ;; The parameters go in a fixed buffer at 1024.
  (func (export "stockwise_alloc") (param $len i32) (result i32)
    (i32.const 1024))

  ;; Parameters arrive as {"level":100.0}: read the whole number after the
  ;; colon, and refuse anything but a positive one.
  (func (export "stockwise_init") (param $ptr i32) (param $len i32) (result i32)
    (local $i i32) (local $end i32) (local $digit i32) (local $n i32) (local $digits i32)
    (local.set $end (i32.add (local.get $ptr) (local.get $len)))
    (local.set $i (local.get $ptr))
    (block $colon
      (loop $scan
        (br_if $colon (i32.ge_u (local.get $i) (local.get $end)))
        (br_if $colon (i32.eq (i32.load8_u (local.get $i)) (i32.const 58)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $scan)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (block $done
      (loop $digits
        (br_if $done (i32.ge_u (local.get $i) (local.get $end)))
        (local.set $digit (i32.sub (i32.load8_u (local.get $i)) (i32.const 48)))
        (br_if $done (i32.gt_u (local.get $digit) (i32.const 9)))
        (local.set $n (i32.add (i32.mul (local.get $n) (i32.const 10)) (local.get $digit)))
        (local.set $digits (i32.add (local.get $digits) (i32.const 1)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $digits)))
    (if (i32.or (i32.eqz (local.get $digits)) (i32.eqz (local.get $n)))
      (then (return (i32.const 1))))
    (global.set $level (f64.convert_i32_u (local.get $n)))
    (i32.const 0))

  ;; 1 = entry, 2 = exit, 0 = nothing.
  (func (export "stockwise_on_bar")
    (param $time i64) (param $open f64) (param $high f64) (param $low f64)
    (param $close f64) (param $volume f64) (result i32)
    (global.set $seen (i32.add (global.get $seen) (i32.const 1)))
    (if (i32.eq (global.get $seen) (i32.const 1))
      (then (return (i32.const 0))))
    (if (f64.gt (local.get $close) (global.get $level))
      (then (return (i32.const 1))))
    (if (f64.lt (local.get $close) (global.get $level))
      (then (return (i32.const 2))))
    (i32.const 0))
)

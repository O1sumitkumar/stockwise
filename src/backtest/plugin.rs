//! WASM strategy plugins: `backtest -s wasm:PATH` (the `wasm-plugins` feature).
//!
//! A plugin is a WebAssembly module that sees one bar at a time, oldest
//! first, and answers with a signal. It runs in the wasmi interpreter with no
//! imports — no WASI, files, network or clock — a memory cap, and a fuel
//! (instruction) budget per call, so a broken plugin ends in an error rather
//! than a panic or a hang. Every price series gets a fresh instance.
//!
//! ABI version 1 (docs/backtesting.md, "WASM plugins"):
//!
//! | export                  | signature                                  |
//! |-------------------------|--------------------------------------------|
//! | `stockwise_abi_version` | `() -> i32`, returns 1                     |
//! | `stockwise_on_bar`      | `(i64, f64, f64, f64, f64, f64) -> i32`    |
//! | `memory`                | needed by the optional exports below       |
//! | `stockwise_meta`        | `() -> i64`, packed pointer/length of JSON |
//! | `stockwise_alloc`       | `(i32) -> i32`, a buffer for the params    |
//! | `stockwise_init`        | `(i32, i32) -> i32`, params JSON, 0 = ok   |
//! | `stockwise_error`       | `() -> i64`, packed message of a failure   |
//!
//! `stockwise_on_bar(time, open, high, low, close, volume)` returns 0 (no
//! signal), 1 (entry), 2 (exit) or 3 (both), evaluated at the bar's close.

use super::Bars;
use super::rules::Signals;
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[cfg(feature = "wasm-plugins")]
pub use wasm::{Plugin, load};

#[cfg(not(feature = "wasm-plugins"))]
pub use disabled::{Plugin, load};

/// Whether this build can run plugins.
pub const SUPPORTED: bool = cfg!(feature = "wasm-plugins");

/// The plugin ABI this build speaks (`stockwise_abi_version`).
pub const ABI_VERSION: i32 = 1;

/// What `stockwise_meta` declares. Every field is optional; the strategy
/// name defaults to the file name.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub name: Option<String>,
    pub description: Option<String>,
    /// Parameter defaults, passed to `stockwise_init` and set with `-P`.
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
    /// Values `backtest walkforward` tries.
    #[serde(default)]
    pub grid: BTreeMap<String, Vec<f64>>,
    /// Bars before the first signal can appear.
    #[serde(default)]
    pub lookback: usize,
    pub stop_loss_pct: Option<f64>,
    pub take_profit_pct: Option<f64>,
}

#[cfg(feature = "wasm-plugins")]
mod wasm {
    use super::*;
    use anyhow::{Context, anyhow, bail};
    use serde::Serialize;
    use std::sync::Arc;
    use wasmi::{
        CompilationMode, Config, EnforcedLimits, Engine, Instance, Memory, Module, Store,
        StoreLimits, StoreLimitsBuilder, TrapCode, TypedFunc, WasmParams, WasmResults,
    };

    /// Largest plugin file accepted.
    const MAX_FILE_BYTES: u64 = 16 << 20;
    /// Linear memory per instance.
    const MAX_MEMORY_BYTES: usize = 64 << 20;
    /// Fuel budgets: about one unit per instruction executed.
    const FUEL: Fuel = Fuel {
        setup: 50_000_000,
        bar: 10_000_000,
    };
    /// Longest metadata or error message.
    const MAX_TEXT_BYTES: usize = 1 << 20;
    /// Most bars of warm-up a plugin may declare.
    const MAX_LOOKBACK: usize = 10_000;

    const ABI: &str = "stockwise_abi_version";
    const ON_BAR: &str = "stockwise_on_bar";
    const META: &str = "stockwise_meta";
    const ALLOC: &str = "stockwise_alloc";
    const INIT: &str = "stockwise_init";
    const ERROR: &str = "stockwise_error";

    type OnBar = TypedFunc<(i64, f64, f64, f64, f64, f64), i32>;

    /// Fuel for one call: when it runs out, the call traps.
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct Fuel {
        /// Instantiation (with any start function), and each call of
        /// `stockwise_meta`, `stockwise_alloc`, `stockwise_init` and
        /// `stockwise_error`.
        pub setup: u64,
        /// Each `stockwise_on_bar`.
        pub bar: u64,
    }

    #[derive(Clone, Copy)]
    enum Budget {
        Setup,
        Bar,
    }

    /// A loaded, checked plugin. Cheap to clone.
    #[derive(Clone)]
    pub struct Plugin(Arc<Inner>);

    struct Inner {
        engine: Engine,
        module: Module,
        /// The file, for error messages.
        source: String,
        fuel: Fuel,
        lookback: usize,
        has_init: bool,
    }

    impl std::fmt::Debug for Plugin {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Plugin")
                .field("lookback", &self.0.lookback)
                .field("has_init", &self.0.has_init)
                .finish_non_exhaustive()
        }
    }

    /// `"plugin": {"abi_version": 1, "lookback": 15}` in a strategy's JSON.
    impl Serialize for Plugin {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeStruct;
            let mut st = s.serialize_struct("Plugin", 2)?;
            st.serialize_field("abi_version", &ABI_VERSION)?;
            st.serialize_field("lookback", &self.0.lookback)?;
            st.end()
        }
    }

    /// Read, compile and check the plugin at `path`, and read its metadata.
    pub fn load(path: &Path) -> Result<(Meta, Plugin)> {
        let source = path.display().to_string();
        let size = std::fs::metadata(path)
            .with_context(|| format!("Cannot read WASM plugin {source}"))?
            .len();
        if size > MAX_FILE_BYTES {
            bail!(
                "{source}: the plugin is {:.1} MB; the limit is {} MB",
                size as f64 / 1_048_576.0,
                MAX_FILE_BYTES >> 20
            );
        }
        let bytes =
            std::fs::read(path).with_context(|| format!("Cannot read WASM plugin {source}"))?;
        from_bytes(&bytes, &source, FUEL).map_err(|e| anyhow!("{source}: {e:#}"))
    }

    /// Compile a plugin from `.wasm` bytes or WAT text.
    pub(crate) fn from_bytes(bytes: &[u8], source: &str, fuel: Fuel) -> Result<(Meta, Plugin)> {
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .compilation_mode(CompilationMode::Eager)
            .enforced_limits(EnforcedLimits::strict());
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(|e| {
            let text = bytes.trim_ascii_start();
            if bytes.starts_with(b"\0asm") || text.starts_with(b"(") || text.starts_with(b";;") {
                anyhow!("not a valid WebAssembly module: {e}")
            } else {
                anyhow!("not a WebAssembly module (no `\\0asm` header, and not WAT text)")
            }
        })?;
        if let Some(i) = module.imports().next() {
            bail!(
                "imports `{}::{}`, but plugins get no host functions (no WASI, files, network \
                 or clock). Build it for wasm32-unknown-unknown",
                i.module(),
                i.name()
            );
        }
        let mut inner = Inner {
            engine,
            module,
            source: source.to_string(),
            fuel,
            lookback: 0,
            has_init: false,
        };

        let mut live = Live::new(&inner)?;
        let abi = live
            .func::<(), i32>(ABI, "() -> i32")?
            .ok_or_else(|| anyhow!("no `{ABI}` export — is this a StockWise strategy plugin?"))?;
        let version = live
            .call(&abi, (), Budget::Setup)
            .with_context(|| format!("`{ABI}`"))?;
        if version != ABI_VERSION {
            bail!(
                "plugin ABI version {version}, but this StockWise runs version {ABI_VERSION} \
                 plugins"
            );
        }
        live.on_bar()?;
        let init = live.func::<(i32, i32), i32>(INIT, "(i32, i32) -> i32")?;
        let alloc = live.func::<i32, i32>(ALLOC, "(i32) -> i32")?;
        live.func::<(), i64>(ERROR, "() -> i64")?;
        if init.is_some() && (alloc.is_none() || live.memory.is_none()) {
            bail!("exports `{INIT}` without `{ALLOC}` and `memory` to pass parameters through");
        }
        let meta: Meta = match live.func::<(), i64>(META, "() -> i64")? {
            Some(f) => {
                let packed = live
                    .call(&f, (), Budget::Setup)
                    .with_context(|| format!("`{META}`"))?;
                let text = live.read_text(packed, META)?;
                serde_json::from_str(&text).map_err(|e| anyhow!("`{META}` JSON: {e}"))?
            }
            None => Meta::default(),
        };
        if !meta.params.is_empty() && init.is_none() {
            bail!("declares parameters but exports no `{INIT}` to receive them");
        }
        if meta.lookback > MAX_LOOKBACK {
            bail!(
                "`lookback` is {}; the most is {MAX_LOOKBACK} bars",
                meta.lookback
            );
        }
        inner.lookback = meta.lookback;
        inner.has_init = init.is_some();
        Ok((meta, Plugin(Arc::new(inner))))
    }

    impl Plugin {
        /// Bars of history the plugin needs before its first signal.
        pub fn lookback(&self) -> usize {
            self.0.lookback
        }

        /// Start an instance with `params`, so a plugin can refuse bad
        /// values before any data is fetched.
        pub fn check(&self, params: &BTreeMap<String, f64>) -> Result<()> {
            self.start(params)
                .map(drop)
                .map_err(|e| anyhow!("{}: {e:#}", self.0.source))
        }

        /// Run a fresh instance over `bars`, one `stockwise_on_bar` per bar.
        pub fn signals(&self, params: &BTreeMap<String, f64>, bars: &Bars) -> Result<Signals> {
            self.run(params, bars)
                .map_err(|e| anyhow!("{}: {e:#}", self.0.source))
        }

        fn run(&self, params: &BTreeMap<String, f64>, bars: &Bars) -> Result<Signals> {
            let mut live = self.start(params)?;
            let on_bar = live.on_bar()?;
            let n = bars.len();
            let mut out = Signals {
                entry: Vec::with_capacity(n),
                exit: Vec::with_capacity(n),
            };
            for i in 0..n {
                let at = || {
                    format!(
                        "`{ON_BAR}` on bar {} of {n} ({})",
                        i + 1,
                        super::super::date(bars.time[i])
                    )
                };
                let args = (
                    bars.time[i],
                    bars.open[i],
                    bars.high[i],
                    bars.low[i],
                    bars.close[i],
                    bars.volume[i] as f64,
                );
                let v = live.call(&on_bar, args, Budget::Bar).with_context(at)?;
                match decode_signal(v) {
                    Ok((entry, exit)) => {
                        out.entry.push(entry);
                        out.exit.push(exit);
                    }
                    Err(why) => bail!("{}: {why}{}", at(), live.error_message()),
                }
            }
            Ok(out)
        }

        /// A new instance, initialised with `params` when the plugin
        /// takes them.
        fn start(&self, params: &BTreeMap<String, f64>) -> Result<Live> {
            let mut live = Live::new(&self.0)?;
            if !self.0.has_init {
                return Ok(live);
            }
            let (Some(alloc), Some(init)) = (
                live.func::<i32, i32>(ALLOC, "(i32) -> i32")?,
                live.func::<(i32, i32), i32>(INIT, "(i32, i32) -> i32")?,
            ) else {
                bail!("`{INIT}` and `{ALLOC}` are missing");
            };
            let json = serde_json::to_string(params)?;
            let len = i32::try_from(json.len())?;
            let ptr = live
                .call(&alloc, len, Budget::Setup)
                .with_context(|| format!("`{ALLOC}`"))?;
            live.write(ptr, json.as_bytes())?;
            let status = live
                .call(&init, (ptr, len), Budget::Setup)
                .with_context(|| format!("`{INIT}`"))?;
            if status != 0 {
                bail!(
                    "the plugin rejected {json} (`{INIT}` returned {status}){}",
                    live.error_message()
                );
            }
            Ok(live)
        }
    }

    /// One running instance.
    struct Live {
        store: Store<StoreLimits>,
        instance: Instance,
        memory: Option<Memory>,
        fuel: Fuel,
    }

    impl Live {
        fn new(p: &Inner) -> Result<Live> {
            let limits = StoreLimitsBuilder::new()
                .memory_size(MAX_MEMORY_BYTES)
                .memories(1)
                .tables(4)
                .table_elements(100_000)
                .instances(1)
                .trap_on_grow_failure(true)
                .build();
            let mut store = Store::new(&p.engine, limits);
            store.limiter(|limits| limits);
            // A start function runs during instantiation, on this fuel.
            store.set_fuel(p.fuel.setup)?;
            let instance = Instance::new(&mut store, &p.module, &[])
                .map_err(|e| explain(e, p.fuel.setup))
                .context("starting the plugin")?;
            let memory = instance.get_memory(&store, "memory");
            Ok(Live {
                store,
                instance,
                memory,
                fuel: p.fuel,
            })
        }

        /// The export `name`, if present, checked against `sig`.
        fn func<P: WasmParams, R: WasmResults>(
            &self,
            name: &str,
            sig: &str,
        ) -> Result<Option<TypedFunc<P, R>>> {
            let Some(f) = self.instance.get_func(&self.store, name) else {
                return Ok(None);
            };
            f.typed::<P, R>(&self.store)
                .map(Some)
                .map_err(|_| anyhow!("`{name}` must have the signature {sig}"))
        }

        fn on_bar(&self) -> Result<OnBar> {
            const SIG: &str = "(i64, f64, f64, f64, f64, f64) -> i32";
            self.func(ON_BAR, SIG)?
                .ok_or_else(|| anyhow!("no `{ON_BAR}` export {SIG}"))
        }

        fn call<P: WasmParams, R: WasmResults>(
            &mut self,
            f: &TypedFunc<P, R>,
            args: P,
            budget: Budget,
        ) -> Result<R> {
            let fuel = match budget {
                Budget::Setup => self.fuel.setup,
                Budget::Bar => self.fuel.bar,
            };
            self.store.set_fuel(fuel)?;
            f.call(&mut self.store, args).map_err(|e| explain(e, fuel))
        }

        fn write(&mut self, ptr: i32, bytes: &[u8]) -> Result<()> {
            let mem = self
                .memory
                .ok_or_else(|| anyhow!("no `memory` export to write parameters into"))?;
            mem.write(&mut self.store, ptr as u32 as usize, bytes)
                .map_err(|_| {
                    anyhow!(
                        "`{ALLOC}` returned {ptr}, which has no room for {} bytes",
                        bytes.len()
                    )
                })
        }

        /// The UTF-8 text at a packed `(pointer << 32) | length`.
        fn read_text(&self, packed: i64, name: &str) -> Result<String> {
            let mem = self
                .memory
                .ok_or_else(|| anyhow!("no `memory` export to read `{name}` from"))?;
            let packed = packed as u64;
            let (ptr, len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
            if len > MAX_TEXT_BYTES {
                bail!("`{name}` returned {len} bytes; the limit is {MAX_TEXT_BYTES}");
            }
            let bytes = ptr
                .checked_add(len)
                .and_then(|end| mem.data(&self.store).get(ptr..end))
                .ok_or_else(|| {
                    anyhow!("`{name}` points outside the plugin's memory ({len} bytes at {ptr})")
                })?;
            String::from_utf8(bytes.to_vec()).map_err(|_| anyhow!("`{name}` is not UTF-8"))
        }

        /// `: <message>` from `stockwise_error`, or nothing.
        fn error_message(&mut self) -> String {
            let Ok(Some(f)) = self.func::<(), i64>(ERROR, "() -> i64") else {
                return String::new();
            };
            self.call(&f, (), Budget::Setup)
                .and_then(|packed| self.read_text(packed, ERROR))
                .ok()
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .map_or_else(String::new, |m| format!(": {m}"))
        }
    }

    /// Signal bits from `stockwise_on_bar`, or why the value is not one.
    fn decode_signal(v: i32) -> Result<(bool, bool), String> {
        match v {
            0..=3 => Ok((v & 1 != 0, v & 2 != 0)),
            _ if v < 0 => Err(format!("the plugin reported an error (returned {v})")),
            _ => Err(format!(
                "returned {v}; a signal is 0 (none), 1 (entry), 2 (exit) or 3 (both)"
            )),
        }
    }

    /// A trap in words.
    fn explain(e: wasmi::Error, fuel: u64) -> anyhow::Error {
        match e.as_trap_code() {
            Some(TrapCode::OutOfFuel) => anyhow!(
                "ran past its budget of {fuel} instructions for one call — an endless loop?"
            ),
            Some(TrapCode::GrowthOperationLimited) => anyhow!(
                "tried to use more than {} MB of memory",
                MAX_MEMORY_BYTES >> 20
            ),
            Some(TrapCode::StackOverflow) => {
                anyhow!("overflowed its call stack — runaway recursion?")
            }
            Some(TrapCode::UnreachableCodeReached) => {
                anyhow!("trapped (`unreachable`; a panic inside a Rust plugin ends this way)")
            }
            Some(code) => anyhow!("trapped: {code}"),
            None => anyhow!("{e}"),
        }
    }
}

#[cfg(not(feature = "wasm-plugins"))]
mod disabled {
    use super::*;
    use anyhow::bail;
    use std::convert::Infallible;

    /// Never constructed: this build cannot load plugins.
    #[derive(Debug, Clone)]
    pub struct Plugin(Infallible);

    impl serde::Serialize for Plugin {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            match self.0 {}
        }
    }

    pub fn load(path: &Path) -> Result<(Meta, Plugin)> {
        bail!(
            "{}: this build of stockwise has no WASM plugin support. Reinstall with \
             `cargo install stockwise --features wasm-plugins`",
            path.display()
        )
    }

    impl Plugin {
        pub fn lookback(&self) -> usize {
            match self.0 {}
        }

        pub fn check(&self, _: &BTreeMap<String, f64>) -> Result<()> {
            match self.0 {}
        }

        pub fn signals(&self, _: &BTreeMap<String, f64>, _: &Bars) -> Result<Signals> {
            match self.0 {}
        }
    }
}

#[cfg(all(test, feature = "wasm-plugins"))]
mod tests {
    use super::*;
    use crate::backtest::costs::CostModel;
    use crate::backtest::engine::{self, Asset, Fill, SimConfig};
    use crate::backtest::rules::{self, StrategyDef};
    use crate::backtest::walkforward;

    const LEVEL: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/backtest/fixtures/level.wat"
    );

    fn level() -> StrategyDef {
        rules::load(&format!("wasm:{LEVEL}")).unwrap()
    }

    fn err(r: Result<impl std::fmt::Debug>) -> String {
        format!("{:#}", r.unwrap_err())
    }

    /// A plugin with `body` added to the required exports. `on_bar` is the
    /// body of `stockwise_on_bar`.
    fn module(on_bar: &str, extra: &str) -> String {
        format!(
            r#"(module
                 (func (export "stockwise_abi_version") (result i32) (i32.const 1))
                 (func (export "stockwise_on_bar")
                   (param i64 f64 f64 f64 f64 f64) (result i32) {on_bar})
                 {extra})"#
        )
    }

    /// Small budgets keep the runaway cases quick in debug builds.
    const FUEL: wasm::Fuel = wasm::Fuel {
        setup: 2_000_000,
        bar: 200_000,
    };

    fn compile(wat: &str) -> Result<(Meta, Plugin)> {
        wasm::from_bytes(wat.as_bytes(), "test.wat", FUEL)
    }

    fn run(wat: &str, closes: &[f64]) -> Result<Signals> {
        let (meta, p) = compile(wat)?;
        p.signals(&meta.params, &Bars::from_closes(closes))
    }

    #[test]
    fn fixture_loads_with_its_metadata() {
        let d = level();
        assert_eq!(d.name, "level");
        assert_eq!(d.description, "Close above a fixed level");
        assert_eq!(d.params["level"], 100.0);
        assert_eq!(d.grid["level"], vec![90.0, 100.0, 110.0]);
        assert!(d.entry.is_empty() && d.exit.is_empty());
        let s = d.compile().unwrap();
        assert_eq!(s.lookback(), 1);
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["plugin"]["abi_version"], 1);
        assert_eq!(json["plugin"]["lookback"], 1);
        assert_eq!(json["source"], LEVEL);
        // TOML strategies have no "plugin" key.
        let rsi = serde_json::to_value(rules::load("rsi").unwrap()).unwrap();
        assert!(rsi.get("plugin").is_none());
    }

    #[test]
    fn signals_follow_the_plugin_and_each_series_starts_fresh() {
        let s = level().compile().unwrap();
        let bars = Bars::from_closes(&[120.0, 120.0, 100.0, 80.0, 101.0]);
        for _ in 0..2 {
            // The first bar is warm-up in every run: a reused instance
            // would signal on it the second time.
            let sig = s.signals(&bars).unwrap();
            assert_eq!(sig.entry, vec![false, true, false, false, true]);
            assert_eq!(sig.exit, vec![false, false, false, true, false]);
        }
    }

    #[test]
    fn params_reach_the_plugin_and_bad_ones_are_refused() {
        let def = level().with_params(&[("level".into(), 110.0)]).unwrap();
        assert_eq!(def.grid["level"], vec![110.0]);
        let sig = def
            .compile()
            .unwrap()
            .signals(&Bars::from_closes(&[100.0, 105.0, 115.0]))
            .unwrap();
        assert_eq!(sig.entry, vec![false, false, true]);
        assert_eq!(sig.exit, vec![false, true, false]);

        let e = err(level().with_params(&[("level".into(), 0.0)]));
        assert!(e.contains("rejected {\"level\":0.0}"), "{e}");
        assert!(e.contains("level must be a whole number above 0"), "{e}");
        let e = err(level().with_params(&[("nope".into(), 1.0)]));
        assert!(e.contains("has no parameter `nope` (it has: level)"), "{e}");
    }

    #[test]
    fn single_portfolio_and_walk_forward_runs() {
        let closes: Vec<f64> = (0..300)
            .map(|i| 100.0 + (i as f64 * 0.1).sin() * 15.0)
            .collect();
        let bars = Bars::from_closes(&closes);
        let def = level();
        let s = def.compile().unwrap();
        let cfg = SimConfig::single(100_000.0, Fill::NextOpen, CostModel::none());

        let (a, b) = (s.signals(&bars).unwrap(), s.signals(&bars).unwrap());
        let r = engine::simulate(
            &[
                Asset {
                    symbol: "A",
                    bars: &bars,
                    signals: &a,
                },
                Asset {
                    symbol: "B",
                    bars: &bars,
                    signals: &b,
                },
            ],
            &cfg,
        );
        assert!(!r.trades.is_empty());
        assert_eq!(r.assets.len(), 2);

        let wf = walkforward::run(&def, "SYN", &bars, 100, 50, &cfg).unwrap();
        assert_eq!(wf.combos, 3);
        assert!(!wf.folds.is_empty());
        for f in &wf.folds {
            assert!([90.0, 100.0, 110.0].contains(&f.params["level"]));
        }
    }

    #[test]
    fn spec_forms_resolve() {
        for spec in [
            format!("wasm:{LEVEL}"),
            format!("file:{LEVEL}"),
            LEVEL.to_string(),
        ] {
            let d = rules::load(&spec).unwrap();
            assert_eq!((d.name.as_str(), d.source.as_str()), ("level", LEVEL));
        }
        let e = err(rules::load("wasm:no/such/plugin.wasm"));
        assert!(
            e.contains("Cannot read WASM plugin no/such/plugin.wasm"),
            "{e}"
        );

        // The strategies folder lists plugins next to TOML files; a .wasm
        // file may even hold WAT text, since wasmi reads both.
        let dir = std::env::temp_dir().join(format!("sw-plugin-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(LEVEL, dir.join("lvl.wasm")).unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        let files = rules::user_strategy_files(&dir);
        assert_eq!(files, vec![dir.join("lvl.wasm")]);
        assert!(rules::is_plugin_file(&files[0]));
        assert_eq!(rules::load_file(&files[0]).unwrap().name, "level");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn minimal_plugin_needs_no_memory_or_metadata() {
        let wat = module("(i32.const 3)", "");
        let (meta, p) = compile(&wat).unwrap();
        assert!(meta.name.is_none() && meta.params.is_empty());
        assert_eq!(p.lookback(), 0);
        let sig = run(&wat, &[1.0, 2.0]).unwrap();
        assert_eq!(sig.entry, vec![true, true]);
        assert_eq!(sig.exit, vec![true, true]);
    }

    #[test]
    fn bars_arrive_in_order_with_every_field() {
        // Entry when time, OHLC and volume are what the host sent: the
        // second of two bars a day apart, volume 1000, all prices = close.
        let wat = module(
            "(i32.and
               (i32.and
                 (i64.eq (local.get 0) (i64.const 1704153600))
                 (f64.eq (local.get 5) (f64.const 1000)))
               (i32.and
                 (f64.eq (local.get 1) (local.get 4))
                 (i32.and (f64.eq (local.get 2) (local.get 4))
                          (f64.eq (local.get 3) (local.get 4)))))",
            "",
        );
        let sig = run(&wat, &[10.0, 11.0]).unwrap();
        assert_eq!(sig.entry, vec![false, true]);
    }

    #[test]
    fn misbehaving_plugins_fail_with_clear_errors() {
        let cases: &[(&str, String)] = &[
            (
                "budget of 200000 instructions",
                module("(loop $l (br $l)) (i32.const 0)", ""),
            ),
            ("`unreachable`", module("(unreachable)", "")),
            (
                "returned 7; a signal is 0 (none), 1 (entry), 2 (exit) or 3 (both)",
                module("(i32.const 7)", ""),
            ),
            (
                "the plugin reported an error (returned -1): no data for you",
                module(
                    "(i32.const -1)",
                    r#"(memory (export "memory") 1)
                       (data (i32.const 8) "no data for you")
                       (func (export "stockwise_error") (result i64)
                         (i64.or (i64.shl (i64.const 8) (i64.const 32)) (i64.const 15)))"#,
                ),
            ),
            (
                "more than 64 MB of memory",
                module(
                    "(drop (memory.grow (i32.const 2000))) (i32.const 0)",
                    r#"(memory 1)"#,
                ),
            ),
            (
                "call stack",
                module("(call $deep) (i32.const 0)", "(func $deep (call $deep))"),
            ),
            (
                "integer divide by zero",
                module("(i32.div_u (i32.const 1) (i32.const 0))", ""),
            ),
        ];
        for (want, wat) in cases {
            let e = err(run(wat, &[1.0, 2.0, 3.0]));
            assert!(e.contains(want), "want `{want}` in: {e}");
            assert!(e.contains("on bar 1 of 3 (2024-01-01)"), "{e}");
        }
    }

    #[test]
    fn bad_modules_are_refused_at_load() {
        let cases: &[(&str, String)] = &[
            (
                "not a WebAssembly module (no `\\0asm` header",
                "garbage".into(),
            ),
            (
                "not a valid WebAssembly module",
                "(module (func (oops)))".into(),
            ),
            ("not a valid WebAssembly module", "\0asm\u{2}\0\0\0".into()),
            (
                "imports `wasi_snapshot_preview1::fd_write`, but plugins get no host functions",
                r#"(module
                     (import "wasi_snapshot_preview1" "fd_write"
                       (func (param i32 i32 i32 i32) (result i32)))
                     (func (export "stockwise_abi_version") (result i32) (i32.const 1)))"#
                    .into(),
            ),
            (
                "no `stockwise_abi_version` export",
                "(module (func (export \"stockwise_on_bar\")))".into(),
            ),
            (
                "plugin ABI version 2, but this StockWise runs version 1",
                r#"(module (func (export "stockwise_abi_version") (result i32) (i32.const 2)))"#
                    .into(),
            ),
            (
                "no `stockwise_on_bar` export (i64, f64, f64, f64, f64, f64) -> i32",
                r#"(module (func (export "stockwise_abi_version") (result i32) (i32.const 1)))"#
                    .into(),
            ),
            (
                "`stockwise_on_bar` must have the signature (i64, f64, f64, f64, f64, f64) -> i32",
                r#"(module
                     (func (export "stockwise_abi_version") (result i32) (i32.const 1))
                     (func (export "stockwise_on_bar") (param f64) (result i32) (i32.const 0)))"#
                    .into(),
            ),
            (
                "budget of 2000000 instructions",
                module(
                    "(i32.const 0)",
                    "(func $spin (loop $l (br $l))) (start $spin)",
                ),
            ),
            (
                "starting the plugin",
                module("(i32.const 0)", r#"(memory (export "memory") 2000)"#),
            ),
            (
                "`stockwise_meta` JSON: unknown field `period`",
                module(
                    "(i32.const 0)",
                    r#"(memory (export "memory") 1)
                       (data (i32.const 0) "{\"period\":14}")
                       (func (export "stockwise_meta") (result i64) (i64.const 13))"#,
                ),
            ),
            (
                "`stockwise_meta` points outside the plugin's memory",
                module(
                    "(i32.const 0)",
                    r#"(memory (export "memory") 1)
                       (func (export "stockwise_meta") (result i64)
                         (i64.or (i64.shl (i64.const 65530) (i64.const 32)) (i64.const 100)))"#,
                ),
            ),
            (
                "declares parameters but exports no `stockwise_init`",
                module(
                    "(i32.const 0)",
                    r#"(memory (export "memory") 1)
                       (data (i32.const 0) "{\"params\":{\"n\":1}}")
                       (func (export "stockwise_meta") (result i64) (i64.const 18))"#,
                ),
            ),
            (
                "exports `stockwise_init` without `stockwise_alloc` and `memory`",
                module(
                    "(i32.const 0)",
                    r#"(func (export "stockwise_init") (param i32 i32) (result i32) (i32.const 0))"#,
                ),
            ),
        ];
        for (want, wat) in cases {
            let e = err(compile(wat));
            assert!(e.contains(want), "want `{want}` in: {e}");
        }
        // Grid values the plugin refuses fail when the strategy loads.
        let wat = std::fs::read_to_string(LEVEL)
            .unwrap()
            .replace("[90,100,110]", "[ 0,100,110]");
        let (meta, p) = compile(&wat).unwrap();
        let e = err(StrategyDef::from_plugin(meta, p, "x", "x.wat"));
        assert!(e.contains("[grid] combination level=0"), "{e}");
    }

    /// The example plugin in examples/wasm-strategy-rsi gives exactly the
    /// built-in `rsi` strategy's signals. Build it first:
    /// `cargo build --release --target wasm32-unknown-unknown
    ///  --manifest-path examples/wasm-strategy-rsi/Cargo.toml`
    #[test]
    #[ignore = "needs the example plugin built for wasm32-unknown-unknown"]
    fn example_rsi_plugin_matches_the_builtin() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/wasm-strategy-rsi/target/wasm32-unknown-unknown/release/wasm_strategy_rsi.wasm"
        );
        let plugin = rules::load(&format!("wasm:{path}")).unwrap();
        let builtin = rules::load("rsi").unwrap();
        assert_eq!(plugin.params, builtin.params);
        assert_eq!(plugin.grid, builtin.grid);
        let closes: Vec<f64> = (0..600)
            .map(|i| {
                let t = i as f64;
                100.0 + (t * 0.09).sin() * 10.0 + (t * 0.37).sin() * 3.0 + t * 0.01
            })
            .collect();
        let bars = Bars::from_closes(&closes);
        for params in builtin.grid_combos() {
            let want = builtin.compile_with(&params).unwrap();
            let got = plugin.compile_with(&params).unwrap();
            assert_eq!(got.lookback(), want.lookback());
            let (w, g) = (want.signals(&bars).unwrap(), got.signals(&bars).unwrap());
            assert!(w.entry.iter().any(|e| *e) && w.exit.iter().any(|e| *e));
            assert_eq!(g, w, "{}", rules::format_params(&params));
        }
        let e = err(plugin.with_params(&[("oversold".into(), 80.0)]));
        assert!(e.contains("oversold must be below overbought"), "{e}");
    }
}

#[cfg(all(test, not(feature = "wasm-plugins")))]
mod tests {
    #[test]
    fn plugins_need_the_feature() {
        let e = crate::backtest::rules::load("wasm:x.wasm").unwrap_err();
        assert!(format!("{e:#}").contains("--features wasm-plugins"));
    }
}

//! The JS API: what `wasm-bindgen` exports to the page. A thin shell over
//! [`Demo`]: JSON in, JSON out, events to one callback.

use std::sync::Arc;

use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::commands::{COMMANDS, Demo};
use crate::events::EventSink;
use crate::menu::DemoMenu;
use crate::tree::DemoTree;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = info)]
    fn console_info(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = debug)]
    fn console_debug(message: &str);
}

/// The engine's `log` lines, on the page's console.
struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("[{}] {}", record.target(), record.args());
        match record.level() {
            log::Level::Error => console_error(&line),
            log::Level::Warn => console_warn(&line),
            log::Level::Info => console_info(&line),
            log::Level::Debug | log::Level::Trace => console_debug(&line),
        }
    }

    fn flush(&self) {}
}

static CONSOLE_LOG: ConsoleLog = ConsoleLog;

/// Runs once when the module loads: panics and warnings land on the console.
#[wasm_bindgen(start)]
fn start() {
    std::panic::set_hook(Box::new(|info| {
        console_error(&format!("cmdr-listing-wasm panicked: {info}"))
    }));
    if log::set_logger(&CONSOLE_LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

/// The page's event callback, `(event: string, payload: unknown) => void`.
struct JsSink(js_sys::Function);

// SAFETY: this build is `wasm32-unknown-unknown` without the `atomics` target
// feature (the `cfg` below), which has exactly one thread, so the JS function is
// never touched from another. With atomics on, this doesn't compile, and the
// sink would need a real cross-thread design.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Send for JsSink {}
// SAFETY: as above, one thread.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Sync for JsSink {}

impl EventSink for JsSink {
    fn emit(&self, event: &str, payload: Value) {
        let payload = to_js(&payload);
        if let Err(e) = self.0.call2(&JsValue::NULL, &JsValue::from_str(event), &payload) {
            log::warn!(target: "listing", "the `{event}` handler threw: {e:?}");
        }
    }
}

/// The real listing engine over an in-memory file tree.
#[wasm_bindgen]
pub struct DemoListing {
    demo: Demo,
}

#[wasm_bindgen]
impl DemoListing {
    /// Builds the engine over `tree` (`{ volumeId, root, entries: [...] }`),
    /// calling `onEvent(eventName, payload)` for every listing event, with the
    /// same names and payloads as Tauri's (`listing-complete`, `directory-diff`, ...).
    #[wasm_bindgen(constructor)]
    pub fn new(tree: JsValue, on_event: js_sys::Function) -> Result<DemoListing, JsError> {
        let tree: DemoTree =
            serde_json::from_value(from_js(&tree)?).map_err(|e| JsError::new(&format!("demo tree: {e}")))?;
        let demo = Demo::new(tree, Arc::new(JsSink(on_event))).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(DemoListing { demo })
    }

    /// The Tauri command names [`invoke`](Self::invoke) answers.
    pub fn commands() -> Vec<String> {
        COMMANDS.iter().map(|c| (*c).to_string()).collect()
    }

    /// Whether [`invoke`](Self::invoke) answers `command`.
    pub fn handles(command: &str) -> bool {
        Demo::handles(command)
    }

    /// Runs a listing command like Tauri's `invoke(command, args)`: resolves
    /// with its answer, rejects with its error.
    pub fn invoke(&self, command: String, args: JsValue) -> js_sys::Promise {
        let demo = self.demo.clone();
        let args = from_js(&args);
        wasm_bindgen_futures::future_to_promise(async move {
            let args = args.map_err(JsValue::from)?;
            match demo.invoke(&command, args).await {
                Ok(answer) => Ok(to_js(&answer)),
                Err(error) => Err(to_js(&error)),
            }
        })
    }

    /// How much the engine logs to the console: `"error"`, `"warn"` (the
    /// default), `"info"`, `"debug"`, or `"trace"`.
    #[wasm_bindgen(js_name = setLogLevel)]
    pub fn set_log_level(level: &str) -> Result<(), JsError> {
        let level: log::LevelFilter = level.parse().map_err(|_| JsError::new("unknown log level"))?;
        log::set_max_level(level);
        Ok(())
    }
}

/// The macOS menu bar over the menu-state commands the app pushes (`menu.rs`).
#[wasm_bindgen]
pub struct DemoMenuBar {
    menu: DemoMenu,
}

#[wasm_bindgen]
impl DemoMenuBar {
    /// A bar as a fresh app builds it.
    #[wasm_bindgen(constructor)]
    pub fn new() -> DemoMenuBar {
        DemoMenuBar { menu: DemoMenu::new() }
    }

    /// Whether [`invoke`](Self::invoke) answers `command`.
    pub fn handles(command: &str) -> bool {
        DemoMenu::handles(command)
    }

    /// Stores one menu-state push like Tauri's `invoke(command, args)`: returns its answer
    /// (`null`), throws its error.
    pub fn invoke(&self, command: &str, args: JsValue) -> Result<JsValue, JsValue> {
        let args = from_js(&args).map_err(JsValue::from)?;
        match self.menu.invoke(command, args) {
            Ok(answer) => Ok(to_js(&answer)),
            Err(error) => Err(to_js(&error)),
        }
    }

    /// The bar as it stands: `[{ id, title, rows }]`, every row with its words, shortcut, and state.
    pub fn bar(&self) -> JsValue {
        serde_json::to_value(self.menu.bar()).map_or(JsValue::NULL, |bar| to_js(&bar))
    }

    /// What a click on `menu_id` asks the page to do: `{ kind: "emit", event, payload }`,
    /// `{ kind: "nativeEdit", action }`, `{ kind: "closeFocusedWindow" }`, or `{ kind: "nothing" }`.
    pub fn click(&self, menu_id: &str) -> JsValue {
        serde_json::to_value(self.menu.click(menu_id)).map_or(JsValue::NULL, |click| to_js(&click))
    }
}

impl Default for DemoMenuBar {
    fn default() -> Self {
        Self::new()
    }
}

/// A JS value as JSON, through `JSON.stringify`. `undefined` is `{}`, as a
/// Tauri command called with no arguments gets.
fn from_js(value: &JsValue) -> Result<Value, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    let json = js_sys::JSON::stringify(value).map_err(|_| JsError::new("arguments aren't JSON"))?;
    let json = json.as_string().unwrap_or_default();
    serde_json::from_str(&json).map_err(|e| JsError::new(&e.to_string()))
}

/// JSON as a JS value, through `JSON.parse`.
fn to_js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}

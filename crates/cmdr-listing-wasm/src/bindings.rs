//! The JS API: what `wasm-bindgen` exports to the page. A thin shell over
//! [`Demo`]: JSON in, JSON out, events to one callback.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::commands::Demo;
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

    fn emit_to(&self, window_label: &str, event: &str, payload: Value) {
        let payload = to_js(&payload);
        let target = JsValue::from_str(window_label);
        if let Err(e) = self
            .0
            .call3(&JsValue::NULL, &JsValue::from_str(event), &payload, &target)
        {
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
    /// A targeted event (`viewer-media-load`) adds a third argument, the window label it's
    /// for. `contents`, when given, is `{ [path]: Uint8Array }`: real bytes for those files.
    #[wasm_bindgen(constructor)]
    pub fn new(tree: JsValue, on_event: js_sys::Function, contents: JsValue) -> Result<DemoListing, JsError> {
        let mut tree: DemoTree =
            serde_json::from_value(from_js(&tree)?).map_err(|e| JsError::new(&format!("demo tree: {e}")))?;
        tree.contents = contents_from_js(&contents)?;
        let demo = Demo::new(tree, Arc::new(JsSink(on_event))).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(DemoListing { demo })
    }

    /// The Tauri command names [`invoke`](Self::invoke) answers.
    pub fn commands() -> Vec<String> {
        Demo::commands().map(str::to_string).collect()
    }

    /// The bytes behind a viewer media token, as `{ mime, bytes: Uint8Array }`, or
    /// `undefined` for a token that names nothing. The window `windowLabel` hears
    /// `viewer-media-load` `Done` for it, as the app's does.
    #[wasm_bindgen(js_name = mediaBody)]
    pub fn media_body(&self, token: &str, window_label: &str) -> JsValue {
        let Some(body) = self.demo.viewer().media_body(token, window_label) else {
            return JsValue::UNDEFINED;
        };
        let answer = js_sys::Object::new();
        let bytes = js_sys::Uint8Array::from(body.bytes.as_slice());
        // `Reflect::set` on a fresh plain object can't fail.
        let _ = js_sys::Reflect::set(&answer, &JsValue::from_str("mime"), &JsValue::from_str(&body.mime));
        let _ = js_sys::Reflect::set(&answer, &JsValue::from_str("bytes"), &bytes);
        answer.into()
    }

    /// Frees every viewer session the window `windowLabel` holds, as the app does when
    /// that window is destroyed.
    #[wasm_bindgen(js_name = closeViewerWindow)]
    pub fn close_viewer_window(&self, window_label: &str) {
        self.demo.viewer().close_window(window_label);
    }

    /// Whether [`invoke`](Self::invoke) answers `command`.
    pub fn handles(command: &str) -> bool {
        Demo::handles(command)
    }

    /// Plans a drop from the visitor's computer (`imports.rs`): `items` is
    /// `[{ path, kind: 'file' | 'folder', size?, modifiedMs? }]`. Resolves with the
    /// plan (`{ batch, accepted, roots, refusal }`), rejects with an `ImportError`.
    #[wasm_bindgen(js_name = importPlan)]
    pub fn import_plan(&self, items: JsValue) -> js_sys::Promise {
        let imports = Arc::clone(self.demo.imports());
        let items = from_js(&items);
        wasm_bindgen_futures::future_to_promise(async move {
            let items: Vec<crate::imports::ImportItem> = serde_json::from_value(items.map_err(JsValue::from)?)
                .map_err(|e| JsValue::from(JsError::new(&format!("import items: {e}"))))?;
            match imports.plan(&items).await {
                Ok(plan) => Ok(to_js(&serde_json::to_value(plan).unwrap_or(Value::Null))),
                Err(error) => Err(to_js(&serde_json::to_value(error).unwrap_or(Value::Null))),
            }
        })
    }

    /// Lands the bytes of the planned file `index` of `batch`. Resolves with
    /// `null`, rejects with an `ImportError`.
    #[wasm_bindgen(js_name = importFile)]
    pub fn import_file(&self, batch: u32, index: u32, bytes: js_sys::Uint8Array) -> js_sys::Promise {
        let imports = Arc::clone(self.demo.imports());
        let bytes = bytes.to_vec();
        wasm_bindgen_futures::future_to_promise(async move {
            match imports.file(u64::from(batch), index as usize, bytes).await {
                Ok(()) => Ok(JsValue::NULL),
                Err(error) => Err(to_js(&serde_json::to_value(error).unwrap_or(Value::Null))),
            }
        })
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

    /// What a click on `menu_id` asks the page to do: `{ kind: "emit", event, payload, to }`,
    /// `{ kind: "nativeEdit", action }`, `{ kind: "closeFocusedWindow" }`, or `{ kind: "nothing" }`.
    pub fn click(&self, menu_id: &str) -> JsValue {
        serde_json::to_value(self.menu.click(menu_id)).map_or(JsValue::NULL, |click| to_js(&click))
    }

    /// Whether [`popup`](Self::popup) answers `command` (a `show_*` context-menu command).
    #[wasm_bindgen(js_name = opensPopup)]
    pub fn opens_popup(command: &str) -> bool {
        DemoMenu::opens_popup(command)
    }

    /// Answers one `show_*` command opened over the window labeled `window`: `{ rows, anchor }`
    /// for the page to draw, or throws what the command's promise rejects with.
    pub fn popup(&self, command: &str, args: JsValue, window: &str) -> Result<JsValue, JsValue> {
        let args = from_js(&args).map_err(JsValue::from)?;
        match self.menu.popup(command, args, window) {
            Ok(popup) => Ok(serde_json::to_value(popup).map_or(JsValue::NULL, |popup| to_js(&popup))),
            Err(error) => Err(to_js(&error)),
        }
    }

    /// What a pick of `menu_id` from the last popup asks the page to do: a click, as
    /// [`click`](Self::click) answers, or `{ kind: "invoke", command, args }`.
    #[wasm_bindgen(js_name = contextClick)]
    pub fn context_click(&self, menu_id: &str) -> JsValue {
        serde_json::to_value(self.menu.context_click(menu_id)).map_or(JsValue::NULL, |click| to_js(&click))
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

/// `{ [path]: Uint8Array }` as stored contents; `undefined` is none.
fn contents_from_js(value: &JsValue) -> Result<HashMap<String, Vec<u8>>, JsError> {
    let mut contents = HashMap::new();
    if value.is_undefined() || value.is_null() {
        return Ok(contents);
    }
    for pair in js_sys::Object::entries(&js_sys::Object::from(value.clone())).iter() {
        let pair = js_sys::Array::from(&pair);
        let path = pair
            .get(0)
            .as_string()
            .ok_or_else(|| JsError::new("contents: a key isn't a path"))?;
        let bytes = pair.get(1);
        if !bytes.is_instance_of::<js_sys::Uint8Array>() {
            return Err(JsError::new(&format!("contents: `{path}` isn't a Uint8Array")));
        }
        contents.insert(path, js_sys::Uint8Array::from(bytes).to_vec());
    }
    Ok(contents)
}

/// JSON as a JS value, through `JSON.parse`.
fn to_js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}

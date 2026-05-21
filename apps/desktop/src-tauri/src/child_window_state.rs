//! In-session position+size cache for child windows (Settings, Debug).
//!
//! The window-state plugin persists only the main window across launches
//! (`with_filter(|label| label == "main")` in `lib.rs`). Child windows
//! intentionally start fresh each app launch — they're modal-feeling and
//! should reappear centered on the main window, not in a stale spot from
//! days ago.
//!
//! Within a single session, though, reopening Settings after closing it
//! should land back where the user last had it. That's what this cache is
//! for. It lives in `app.manage(...)` so it's wiped automatically when the
//! process exits; no disk involvement.

use std::collections::HashMap;
use std::sync::Mutex;

/// Logical-pixel rectangle. `f64` mirrors what Tauri's `LogicalPosition` /
/// `LogicalSize` use on the wire.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ChildWindowRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Mutex-guarded map keyed by window label.
#[derive(Default)]
pub struct ChildWindowRectStore(Mutex<HashMap<String, ChildWindowRect>>);

impl ChildWindowRectStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, label: &str) -> Option<ChildWindowRect> {
        self.0.lock().ok()?.get(label).copied()
    }

    pub fn set(&self, label: String, rect: ChildWindowRect) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(label, rect);
        }
    }
}

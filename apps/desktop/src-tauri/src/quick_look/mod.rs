//! Native macOS Quick Look panel integration.
//!
//! Owns a single `QuickLookController` registered as `tauri::State`. The controller
//! implements `QLPreviewPanelDataSource` and `QLPreviewPanelDelegate`, opens the
//! process-wide shared `QLPreviewPanel`, and emits two Tauri events:
//!
//! - `quick-look-key` — keyboard events the panel didn't want (we re-dispatch on
//!   the focused pane so arrow keys / Shift+Space still work while the panel is key)
//! - `quick-look-closed` — fires whenever the panel actually leaves the screen
//!   (✕, Esc, or our own `orderOut:`). Frontend uses it to flip `isOpen = false`.
//!
//! Three Tauri commands gate the controller from the frontend: `quick_look_open`,
//! `quick_look_set_path`, `quick_look_close`. All three hop to the AppKit main
//! thread via `app.run_on_main_thread()` + a one-shot `mpsc` channel (panel calls
//! have main-thread affinity), and the IPC layer wraps the await in a 2 s timeout
//! so a wedged AppKit pump never freezes the IPC pool.
//!
//! See `docs/specs/quick-look-plan.md` § "Why a singleton controller" for the
//! design rationale.

#[cfg(target_os = "macos")]
mod controller;

#[cfg(target_os = "macos")]
pub use controller::QuickLookController;

use serde::Serialize;
use std::sync::Mutex;

/// Wraps the controller in a `Mutex` for `tauri::State`. On non-macOS the inner
/// type is `()` so the same `app.manage(...)` call site compiles everywhere.
#[cfg(target_os = "macos")]
pub type QuickLookState = Mutex<QuickLookController>;
#[cfg(not(target_os = "macos"))]
pub type QuickLookState = Mutex<()>;

/// Initialize the empty controller. Call from `lib.rs::setup` after `MenuState`.
#[cfg(target_os = "macos")]
pub fn init_state() -> QuickLookState {
    Mutex::new(QuickLookController::new())
}
#[cfg(not(target_os = "macos"))]
pub fn init_state() -> QuickLookState {
    Mutex::new(())
}

/// Payload for the `quick-look-key` event emitted by the panel delegate.
/// Mirrors the relevant fields of a DOM `KeyboardEvent` so the frontend can
/// re-dispatch through the same primitives FilePane already uses.
///
/// Only constructed on macOS (the delegate that builds it is macOS-only).
/// On Linux the struct still exists for serde-shape symmetry across platforms
/// but no code emits it; `#[cfg_attr(...)] allow(dead_code)` silences
/// `#[deny(unused)]` on that platform.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "constructed only on macOS; struct exists on Linux for serde-shape symmetry"
    )
)]
pub struct QuickLookKeyEvent {
    /// `KeyboardEvent.key`. Matches DOM semantics (`'ArrowDown'`, `' '`, `'a'`).
    pub key: String,
    /// `KeyboardEvent.code`. Layout-independent physical-key id (`'KeyA'`,
    /// `'Space'`). Useful when the routed handler discriminates by physical key.
    pub code: String,
    pub shift_key: bool,
    pub meta_key: bool,
    pub alt_key: bool,
    pub ctrl_key: bool,
}

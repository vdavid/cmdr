//! Cross-platform appearance/system event payloads.
//!
//! These events are emitted from platform-gated modules (`accent_color.rs` /
//! `accent_color_linux.rs`, macOS-only `text_size.rs`), but `collect_events!`
//! in `ipc.rs` can't `#[cfg]`-gate inline, so their typed payload structs live
//! here in an always-compiled module. The emit sites just build and `.emit()`
//! them. Same pattern the MTP / network partitions use (structs in an
//! always-compiled module, emits behind `#[cfg]`).

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

/// `accent-color-changed`: the OS accent color (or light/dark appearance)
/// changed. `hex` is the new accent color as a `#rrggbb` string.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AccentColorChanged {
    pub hex: String,
}

/// `system-text-size-changed`: the macOS Accessibility > Display > Text Size
/// value changed. `multiplier` is the new system text-size multiplier (1.0 =
/// default).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SystemTextSizeChanged {
    pub multiplier: f32,
}

/// `drag-image-size`: the OS drag image's pixel dimensions, read on drag enter
/// (macOS swizzle in `drag_image_detection.rs`). Used to size / suppress the
/// DOM drag overlay.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DragImageSize {
    pub width: f64,
    pub height: f64,
}

/// `drag-modifiers`: the modifier-key state during a drag, emitted on drag
/// enter and whenever it changes (macOS swizzle in `drag_image_detection.rs`).
/// Drives copy/move intent without keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DragModifiers {
    pub alt_held: bool,
    pub cmd_held: bool,
    pub shift_held: bool,
}

/// `drag-out-session-started`: the FE raises a signs-of-life in-progress toast
/// when the FIRST fulfillment of a drag-out-to-Finder session begins (macOS,
/// `native_drag/promises.rs`). `total_items` is the top-level dragged-item
/// count. The struct lives here (always compiled) because `native_drag` is
/// macOS-only and `collect_events!` can't `#[cfg]`-gate inline; the macOS emit
/// site builds + `.emit()`s it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "drag-out-session-started")]
pub struct SessionStartedEvent {
    /// The drag sequence key, so the FE can key its in-progress toast and
    /// replace it in place with the completion toast under the same id.
    pub session_key: i64,
    /// Top-level dragged items in this session.
    pub total_items: usize,
}

/// `drag-out-session-complete`: the session drained (gesture ended AND no
/// in-flight fulfillment). The FE replaces the in-progress toast with a
/// completion / failure toast keyed by the same `session_key`, plus the folded
/// per-item outcome counts. Always-compiled for the same reason as
/// `SessionStartedEvent`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "drag-out-session-complete")]
pub struct SessionCompleteEvent {
    /// The drag sequence key (matches the started event's key).
    pub session_key: i64,
    /// Top-level files that landed successfully.
    pub files_succeeded: usize,
    /// Top-level folders that landed successfully.
    pub folders_succeeded: usize,
    /// Leaf names of items that failed (empty on full success).
    pub failures: Vec<String>,
}

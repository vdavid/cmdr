//! Clipboard file operations for copy/cut/paste workflows.
//!
//! macOS exposes three free functions backed by NSPasteboard (`pasteboard`)
//! in prod and by an in-process store (`mock`) under the `playwright-e2e`
//! Cargo feature. Both impls share the same `store` module so a runtime
//! `CMDR_CLIPBOARD_BACKEND=mock` env opt-out in the prod path delegates to
//! the same data the mock module reads. See `CLAUDE.md` for the rationale.

#[cfg(target_os = "macos")]
mod store;

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
mod pasteboard;

#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
mod mock;

mod state;

pub use state::clear_cut_state;
#[cfg(target_os = "macos")]
pub use state::{get_cut_state, set_cut_state};

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
pub use pasteboard::{read_file_urls_from_clipboard, read_text_from_clipboard, write_file_urls_to_clipboard};

#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
pub use mock::{read_file_urls_from_clipboard, read_text_from_clipboard, write_file_urls_to_clipboard};

/// E2E-only admin surface: returns a snapshot of the in-process clipboard
/// store. Used by Playwright specs to assert clipboard contents without
/// reading back through IPC (and without touching the real pasteboard).
#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
#[allow(
    dead_code,
    reason = "Exported for future Playwright specs that assert clipboard contents directly."
)]
pub fn snapshot_mock_clipboard() -> Option<store::ClipboardEntry> {
    store::snapshot()
}

/// E2E-only admin surface: clears the in-process clipboard store.
#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
#[allow(
    dead_code,
    reason = "Exported for future Playwright specs that reset clipboard state between tests."
)]
pub fn clear_mock_clipboard() {
    store::clear();
}

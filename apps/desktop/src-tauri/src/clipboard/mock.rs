//! In-process mock of the macOS pasteboard for E2E builds.
//!
//! Gated behind the `playwright-e2e` Cargo feature so prod and dev binaries
//! never link this code. The three exported functions match the signatures
//! of `pasteboard.rs` so the call sites in `commands/clipboard.rs` stay
//! identical between configurations.
//!
//! Backed by the shared `store` module; the prod module's runtime
//! `CMDR_CLIPBOARD_BACKEND=mock` override delegates to the same store, so
//! tests that read via the mock see what the prod-feature-built-with-env
//! path wrote.

use std::path::PathBuf;

use objc2::MainThreadMarker;

use super::store;

/// Stores file URLs in the in-process clipboard mock instead of NSPasteboard.
pub fn write_file_urls_to_clipboard(_mtm: MainThreadMarker, paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No paths to write to clipboard".to_string());
    }
    store::write_paths(paths);
    log::info!(target: "clipboard", "[mock] wrote {} file URL(s) to in-process clipboard", paths.len());
    Ok(())
}

/// Returns the most recently written file URLs, or an empty Vec when the store is empty.
pub fn read_file_urls_from_clipboard(_mtm: MainThreadMarker) -> Result<Vec<PathBuf>, String> {
    Ok(store::read_paths())
}

/// Returns the most recently written newline-joined paths as text.
pub fn read_text_from_clipboard(_mtm: MainThreadMarker) -> Option<String> {
    store::read_text()
}

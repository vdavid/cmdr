//! In-process record of external-open requests for the `playwright-e2e` build.
//!
//! Every action that hands a path to another app records here instead of
//! launching it: `open_path`, `open_in_editor` (`commands/file_actions.rs`), and
//! "open terminal here" (`file_system/terminal.rs`). The suite creates files and
//! opens them, and it has no way to close a TextEdit or terminal window, so real
//! launches would pile up unbounded across runs.
//!
//! Mirrors the clipboard mock (`crate::clipboard`): compiled only under the
//! feature, so prod and dev binaries never link it, and it never touches the OS.
//! Specs read it back through the `e2e_opened_paths` command.

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;

static OPENED: LazyLock<Mutex<Vec<PathBuf>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Records an open request without launching anything.
pub fn record(path: String) {
    log::info!(target: "file_actions", "[mock] external open recorded (not launched): {path}");
    OPENED.lock_ignore_poison().push(PathBuf::from(path));
}

/// Returns the paths opened so far, oldest first.
pub fn snapshot() -> Vec<PathBuf> {
    OPENED.lock_ignore_poison().clone()
}

/// Clears the recorded open requests (per-test reset).
pub fn clear() {
    OPENED.lock_ignore_poison().clear();
}

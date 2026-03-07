//! In-process cut state tracking.
//!
//! When the user cuts files, we write them to the system clipboard (same as copy)
//! and also record the source paths here. On paste, we check whether the clipboard
//! still matches the cut set to decide between copy and move semantics.

#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::{LazyLock, RwLock};

#[cfg(target_os = "macos")]
struct CutState {
    source_paths: Vec<PathBuf>,
}

#[cfg(target_os = "macos")]
static CUT_STATE: LazyLock<RwLock<Option<CutState>>> = LazyLock::new(|| RwLock::new(None));

#[cfg(target_os = "macos")]
pub fn set_cut_state(paths: Vec<PathBuf>) {
    let mut guard = CUT_STATE.write().unwrap_or_else(|e| e.into_inner());
    *guard = Some(CutState { source_paths: paths });
}

#[cfg(target_os = "macos")]
pub fn clear_cut_state() {
    let mut guard = CUT_STATE.write().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

#[cfg(not(target_os = "macos"))]
pub fn clear_cut_state() {}

#[cfg(target_os = "macos")]
pub fn get_cut_state() -> Option<Vec<PathBuf>> {
    let guard = CUT_STATE.read().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|s| s.source_paths.clone())
}

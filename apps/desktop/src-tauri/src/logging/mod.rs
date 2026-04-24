//! Logging support module.
//!
//! Owns the log pipeline end to end via a hand-rolled `fern` Dispatch tree
//! ([`dispatch::init`]). Replaced `tauri-plugin-log` so we get **per-output level
//! filtering**: file target stays at Debug (so error report bundles carry useful
//! context), terminal defaults to Info (less noise for `pnpm dev`).
//!
//! Public surface:
//!
//! - [`dispatch::init`] / [`dispatch::set_stdout_threshold`]: builds the tree + the
//!   verbose-toggle knob.
//! - **Resolved log dir cache** ([`set_log_dir`] / [`log_dir`]): the path is derived in
//!   `lib.rs` from `CMDR_LOG_DIR` / `CMDR_DATA_DIR` / the Tauri default. The error
//!   reporter bundle builder needs the same path without re-deriving the env-var logic.
//! - **Live keep-count** ([`set_keep_count`] / [`keep_count`]): the rotation keep-N value
//!   the file chain was built with. `file-rotate` is one-shot — changing this at runtime
//!   does NOT reconfigure the chain, but [`eager_prune`] uses it to delete excess
//!   archived files immediately when the user lowers the cap.
//! - **One-shot pruner** ([`eager_prune`]): for the user-lowered-the-cap case.
//! - **Listing helpers** ([`list_recent_log_files`], [`current_total_log_bytes`]): for
//!   bundle building and diagnostics.

pub mod dispatch;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
mod tests;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static KEEP_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Records the resolved log directory once, at plugin-build time.
///
/// Subsequent calls are no-ops — the path doesn't change without an app restart.
pub fn set_log_dir(path: PathBuf) {
    let _ = LOG_DIR.set(path);
}

/// Returns the resolved log directory if it was recorded.
///
/// Returns `None` when log storage is disabled (cap = 0) or before the plugin builder ran.
pub fn log_dir() -> Option<&'static Path> {
    LOG_DIR.get().map(PathBuf::as_path)
}

/// Records the keep-count the plugin was built with (`ceil(cap_mb / 50)`).
pub fn set_keep_count(n: usize) {
    KEEP_COUNT.store(n, Ordering::Relaxed);
}

/// Current keep-count.
///
/// Used by [`eager_prune`] callers and reported in diagnostics. `0` means "log storage
/// disabled" (the `Folder` target was dropped at build time).
pub fn keep_count() -> usize {
    KEEP_COUNT.load(Ordering::Relaxed)
}

/// Lists `*.log*` files in the log dir, newest first by mtime.
///
/// Returns an empty `Vec` if the directory doesn't exist or can't be read. The "live" file
/// (no rotation suffix) sorts ahead of archived `cmdr.log.YYYY-MM-DD-HH-MM-SS` siblings as
/// long as its mtime is fresher, which is the case during normal operation.
pub fn list_recent_log_files(log_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return Vec::new();
    };

    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            // Match anything containing ".log" — covers `cmdr.log` and rotated
            // `cmdr.log.2025-01-15-...` siblings without hard-coding a stem.
            let name = path.file_name()?.to_str()?;
            if !name.contains(".log") {
                return None;
            }
            let mtime = entry.metadata().ok()?.modified().ok()?;
            Some((path, mtime))
        })
        .collect();

    // Newest first — `Reverse` avoids the `sort_by`/`Ord` boilerplate clippy flags.
    files.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));
    files.into_iter().map(|(p, _)| p).collect()
}

/// Sums sizes of all `*.log*` files in the log dir. Returns `0` if the dir is missing.
#[allow(dead_code, reason = "Diagnostic helper; wired up by Phase 4 bundle manifest")]
pub fn current_total_log_bytes(log_dir: &Path) -> u64 {
    list_recent_log_files(log_dir)
        .into_iter()
        .filter_map(|p| std::fs::metadata(&p).ok().map(|m| m.len()))
        .sum()
}

/// Deletes all but the `keep_n` newest `*.log*` files in `log_dir`.
///
/// One-shot — call once after the user lowers the cap. Returns the number of files deleted.
/// A missing directory is not an error (returns `Ok(0)`). Per-file deletion errors are
/// logged but don't abort the sweep.
///
/// `keep_n == 0` keeps no files at all — used when the user disables log storage at runtime
/// (the live `cmdr.log` will be re-created by the plugin on the next write).
pub fn eager_prune(log_dir: &Path, keep_n: usize) -> std::io::Result<usize> {
    if !log_dir.exists() {
        return Ok(0);
    }
    let files = list_recent_log_files(log_dir);
    if files.len() <= keep_n {
        return Ok(0);
    }
    let mut deleted = 0usize;
    for path in files.into_iter().skip(keep_n) {
        match std::fs::remove_file(&path) {
            Ok(()) => deleted += 1,
            Err(err) => log::warn!(
                target: "cmdr_lib::logging",
                "Failed to prune log file {}: {err}",
                path.display(),
            ),
        }
    }
    Ok(deleted)
}

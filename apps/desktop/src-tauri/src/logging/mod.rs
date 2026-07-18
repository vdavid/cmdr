//! Logging support module.
//!
//! Owns the log pipeline end to end via a hand-rolled `fern` Dispatch tree
//! ([`dispatch::init`]). Replaced `tauri-plugin-log` so we get **per-output level
//! filtering**: file target stays at Debug (so error report bundles carry useful
//! context), terminal defaults to Info (less noise for `pnpm dev`).
//!
//! Public surface:
//!
//! - [`dispatch::init`] / [`dispatch::set_stdout_threshold`]: builds the tree + the verbose-toggle
//!   knob.
//! - **Resolved log dir cache** ([`set_log_dir`] / [`log_dir`]): the path is derived in `lib.rs`
//!   from `CMDR_LOG_DIR` / `CMDR_DATA_DIR` / the Tauri default. The error reporter bundle builder
//!   needs the same path without re-deriving the env-var logic.
//! - **Live keep-count** ([`set_keep_count`] / [`keep_count`]): the rotation keep-N value the file
//!   chain was built with. `file-rotate` is one-shot; changing this at runtime does NOT reconfigure
//!   the chain, but [`eager_prune`] uses it to delete excess archived files immediately when the
//!   user lowers the cap.
//! - **One-shot pruner** ([`eager_prune`]): for the user-lowered-the-cap case.
//! - **Listing helper** ([`list_recent_log_files`]): for bundle building and diagnostics.

mod coalesce;
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
/// Subsequent calls are no-ops; the path doesn't change without an app restart.
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

/// Returns `true` for filenames `file-rotate` actively manages: `cmdr.log` plus its
/// numeric-suffixed siblings `cmdr.log.1`, `cmdr.log.2`, ...
///
/// Match is case-insensitive so the same predicate works on macOS (where the on-disk
/// inode often shows up as `Cmdr.log` due to historical casing) and on Linux.
///
/// Specifically rejects the pre-`319d5d37` `tauri-plugin-log` rotation pattern
/// (`Cmdr_<timestamp>.log`) so legacy files left over after the fern refactor don't
/// pollute error report bundles. Those files are removed by [`cleanup_legacy_log_files`].
pub(crate) fn is_active_log_file(name: &str) -> bool {
    let Some(rest) = strip_prefix_ascii_case_insensitive(name, "cmdr.log") else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    let Some(suffix) = rest.strip_prefix('.') else {
        return false;
    };
    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
}

/// Returns `true` for the legacy `tauri-plugin-log` rotation pattern: `Cmdr_*.log`
/// (with an underscore separator after the stem and any non-empty middle segment).
/// Used by [`cleanup_legacy_log_files`] for the one-shot startup sweep.
pub(crate) fn is_legacy_log_file(name: &str) -> bool {
    let Some(rest) = strip_prefix_ascii_case_insensitive(name, "cmdr_") else {
        return false;
    };
    let Some(middle) = rest.strip_suffix(".log") else {
        return false;
    };
    !middle.is_empty()
}

fn strip_prefix_ascii_case_insensitive<'a>(haystack: &'a str, prefix: &str) -> Option<&'a str> {
    if haystack.len() < prefix.len() {
        return None;
    }
    let (head, tail) = haystack.split_at(prefix.len());
    if head.eq_ignore_ascii_case(prefix) {
        Some(tail)
    } else {
        None
    }
}

/// Lists active log files (`cmdr.log` plus `cmdr.log.<digits>` siblings) in the log dir,
/// newest first by mtime.
///
/// Returns an empty `Vec` if the directory doesn't exist or can't be read. The "live" file
/// (no numeric suffix) sorts ahead of rotated siblings as long as its mtime is fresher,
/// which is the case during normal operation.
pub fn list_recent_log_files(log_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return Vec::new();
    };

    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !is_active_log_file(name) {
                return None;
            }
            let mtime = entry.metadata().ok()?.modified().ok()?;
            Some((path, mtime))
        })
        .collect();

    // Newest first; `Reverse` avoids the `sort_by`/`Ord` boilerplate clippy flags.
    files.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));
    files.into_iter().map(|(p, _)| p).collect()
}

/// One-shot sweep: deletes `Cmdr_<timestamp>.log` rotation leftovers from the legacy
/// `tauri-plugin-log` setup. Idempotent; subsequent runs find nothing.
///
/// Logs one INFO line per deleted file so operators can see the cleanup happen on
/// upgrade. Per-file errors are logged at WARN and don't abort the sweep. Returns the
/// number of files removed.
pub fn cleanup_legacy_log_files(log_dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return 0;
    };
    let mut deleted = 0usize;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_legacy_log_file(name) {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {
                deleted += 1;
                log::info!(
                    target: "cmdr_lib::logging",
                    "Removed legacy log file {}",
                    path.display(),
                );
            }
            Err(err) => log::warn!(
                target: "cmdr_lib::logging",
                "Failed to remove legacy log file {}: {err}",
                path.display(),
            ),
        }
    }
    deleted
}

/// Deletes all but the `keep_n` newest `*.log*` files in `log_dir`.
///
/// One-shot: call once after the user lowers the cap. Returns the number of files deleted.
/// A missing directory is not an error (returns `Ok(0)`). Per-file deletion errors are
/// logged but don't abort the sweep.
///
/// `keep_n == 0` keeps no files at all; used when the user disables log storage at runtime
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

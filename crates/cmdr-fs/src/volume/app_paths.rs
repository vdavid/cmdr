//! Whether an app path sits under a volume root, and where it lands when that
//! root moves.
//!
//! An app path is what a pane, a tab, or a favorite holds: an OS path for a
//! mounted volume (`/Volumes/naspi/docs`), a `scheme://` path for one with no
//! mount (`sftp://ada@nas:22/srv/data`, `mtp://…`). Both spell their tree with
//! `/` separators under a root, so one segment-wise test serves both.
//!
//! ❗ **Whole segments, ❌ never a raw string prefix.** `sftp://ada@nas:22/srv/data`
//! would otherwise claim `sftp://ada@nas:22/srv/data-1`, a different tree, and
//! `/Volumes/naspi` would claim `/Volumes/naspi-1`, a different share.
//!
//! Lexical on purpose: no `stat`, no network. The callers (path resolution for
//! devices and servers, the favorites add gate and reach pass) run where a
//! syscall on a dead mount must never happen.

/// The part of `path` below `root`, without leading or trailing separators, or
/// `None` when `path` isn't `root` or inside it. `root` itself answers `Some("")`.
///
/// A trailing `/` on either side doesn't count, so `/` contains every absolute
/// path and `sftp://u@h:22/` is the same root as `sftp://u@h:22`.
pub fn path_under<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(root.trim_end_matches('/'))?;
    if rest.is_empty() {
        return Some("");
    }
    rest.starts_with('/').then(|| rest.trim_matches('/'))
}

/// Whether `path` is `root` or sits under it, by whole `/`-separated segments.
pub fn path_is_under(path: &str, root: &str) -> bool {
    path_under(path, root).is_some()
}

/// `path` moved from under `old_root` to the same place under `new_root`, or
/// `None` when it isn't under `old_root` at all.
///
/// For a mounted volume whose mount point moved (a share remounted at
/// `/Volumes/naspi-1`, a drive renamed): the folder inside it is the same one.
pub fn rebase(path: &str, old_root: &str, new_root: &str) -> Option<String> {
    let relative = path_under(path, old_root)?;
    if relative.is_empty() {
        return Some(new_root.to_string());
    }
    Some(format!("{}/{relative}", new_root.trim_end_matches('/')))
}

#[cfg(test)]
#[path = "app_paths_test.rs"]
mod app_paths_test;

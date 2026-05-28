//! Eligibility filter for downloads-watcher events.
//!
//! Decides whether a path observed under the watched root looks like a "real"
//! completed download we should surface. The watcher passes only paths that
//! are already under the resolved Downloads root, so this function doesn't
//! re-check that boundary.

use std::fs;
use std::path::{Component, Path};

/// Partial-download filename suffixes browsers emit while a transfer is
/// in flight. Case-sensitive: browsers always lowercase these.
const PARTIAL_SUFFIXES: &[&str] = &[".crdownload", ".part", ".download"];

/// Is `path` an eligible download to surface?
///
/// Returns `false` when any of:
///
/// - Any path component (basename or any ancestor) starts with `.` (hidden).
/// - The filename ends with a partial-download suffix
///   (`.crdownload`, `.part`, `.download`), case-sensitive.
/// - `path` refers to a directory.
/// - `path` is a broken symlink (errors swallowed, no propagation).
///
/// Returns `true` for regular files and symlinks resolving to a regular file.
pub fn is_eligible(path: &Path) -> bool {
    if has_hidden_component(path) {
        return false;
    }
    if has_partial_suffix(path) {
        return false;
    }
    // `fs::metadata` follows symlinks, so a symlink to a regular file is
    // treated as a regular file, and a broken symlink errors out and we
    // return `false` without propagating.
    match fs::metadata(path) {
        Ok(meta) => meta.is_file(),
        Err(_) => false,
    }
}

fn has_hidden_component(path: &Path) -> bool {
    path.components().any(|c| match c {
        Component::Normal(s) => s.to_str().is_some_and(|s| s.starts_with('.')),
        _ => false,
    })
}

fn has_partial_suffix(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    PARTIAL_SUFFIXES.iter().any(|suf| name.ends_with(suf))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use tempfile::TempDir;

    /// `tempfile::TempDir::new` on macOS creates a `.tmpXXXXX`-named
    /// directory inside `$TMPDIR` (the leading dot hides it in Finder).
    /// That dot trips our hidden-component check on the full path and
    /// shadows every other assertion in this file. Use a non-hidden
    /// prefix instead so positive-path assertions actually exercise the
    /// codepath we care about.
    fn unhidden_tempdir() -> TempDir {
        tempfile::Builder::new()
            .prefix("cmdr-downloads-test-")
            .tempdir()
            .unwrap()
    }

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, b"hi").unwrap();
        p
    }

    #[test]
    fn rejects_hidden_basename() {
        let td = unhidden_tempdir();
        let root = td.path();
        let p = touch(root, ".DS_Store");
        assert!(!is_eligible(&p));
    }

    #[test]
    fn rejects_hidden_ancestor() {
        let td = unhidden_tempdir();
        let root = td.path();
        let sub = root.join(".tmp");
        fs::create_dir(&sub).unwrap();
        let p = touch(&sub, "foo.zip");
        assert!(!is_eligible(&p));
    }

    #[test]
    fn rejects_partial_suffixes() {
        let td = unhidden_tempdir();
        let root = td.path();
        for name in ["foo.crdownload", "bar.part", "baz.download"] {
            let p = touch(root, name);
            assert!(!is_eligible(&p), "expected {name} to be ineligible");
        }
    }

    #[test]
    fn accepts_regular_file() {
        let td = unhidden_tempdir();
        let root = td.path();
        let p = touch(root, "foo.zip");
        assert!(is_eligible(&p));
    }

    #[test]
    fn rejects_directory() {
        let td = unhidden_tempdir();
        let root = td.path();
        let sub = root.join("subdir");
        fs::create_dir(&sub).unwrap();
        assert!(!is_eligible(&sub));
    }

    #[test]
    fn accepts_partial_looking_subname() {
        // Only literal trailing suffixes match. A file named
        // `foo.crdownload.zip` is final (the partial suffix is in the middle,
        // not at the end).
        let td = unhidden_tempdir();
        let root = td.path();
        let p = touch(root, "foo.crdownload.zip");
        assert!(is_eligible(&p));
    }

    #[test]
    fn broken_symlink_returns_false_without_panic() {
        let td = unhidden_tempdir();
        let root = td.path();
        let link = root.join("dangling");
        symlink(root.join("does-not-exist"), &link).unwrap();
        assert!(!is_eligible(&link));
    }

    #[test]
    fn case_sensitive_partial_suffix_check() {
        // Browsers emit lowercase only. Anything else is treated as a real
        // download (the user named it themselves).
        let td = unhidden_tempdir();
        let root = td.path();
        let p = touch(root, "foo.CRDOWNLOAD");
        assert!(is_eligible(&p));
    }
}

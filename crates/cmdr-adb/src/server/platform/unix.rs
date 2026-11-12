//! macOS and Linux: a file is runnable when any of its execute bits is set.

use std::path::Path;

/// A regular file with an execute bit.
pub(crate) fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

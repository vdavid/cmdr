//! Windows: `adb.exe`, with Android Studio's SDK under `%LOCALAPPDATA%`.

use std::path::Path;

pub(crate) const HOST_SEARCH: super::AdbSearch = super::AdbSearch::WINDOWS;

/// Windows runs a file by its extension, and every candidate ends in `.exe`, so a regular file
/// is enough.
pub(crate) fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

//! Where this OS keeps `adb`, and what makes a file runnable here. A platform seam
//! (`docs/guides/platform-seams.md`); the per-OS lists themselves are data in
//! `super::binary_search`, so every OS's list is tested on every OS.

use std::path::Path;

use super::binary_search::AdbSearch;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

/// What macOS and Linux share: the executable bit.
#[cfg(unix)]
mod unix;

const _: AdbSearch = HOST_SEARCH;
const _: fn(&Path) -> bool = is_executable;

/// This OS's search: its binary name, default SDK, and package-manager folders.
pub(super) use imp::HOST_SEARCH;

/// Whether `path` is a file this OS would run.
pub(super) use imp::is_executable;

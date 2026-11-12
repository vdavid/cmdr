//! macOS: Android Studio's SDK under `~/Library`, and Homebrew.

pub(crate) use super::unix::is_executable;

pub(crate) const HOST_SEARCH: super::AdbSearch = super::AdbSearch::MACOS;

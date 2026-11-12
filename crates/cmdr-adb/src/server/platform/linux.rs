//! Linux: Android Studio's SDK under `~/Android/Sdk`, and `/usr/local/bin`.

pub(crate) use super::unix::is_executable;

pub(crate) const HOST_SEARCH: super::AdbSearch = super::AdbSearch::LINUX;

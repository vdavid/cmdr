//! Windows: no native-crash capture yet (an unhandled-exception filter or `crash-handler`'s
//! minidumps is the one to add), so only panics are reported, and the process facts are unknown.

use std::path::Path;

/// Nothing to arm yet: a native crash goes unreported, a panic still doesn't.
pub(crate) fn install_native_crash_hooks(_raw_crash_path: &Path, _data_dir: &Path) {}

/// Unknown.
pub(crate) fn current_image_base() -> u64 {
    0
}

/// Unknown.
pub(crate) fn current_thread_count() -> usize {
    0
}

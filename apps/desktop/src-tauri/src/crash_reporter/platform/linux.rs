//! Linux: signal handlers; the thread count from `/proc/self/status`.

use std::path::Path;

pub(crate) use super::unix::current_image_base;

/// Signal handlers only: Linux has no second exception channel to catch.
pub(crate) fn install_native_crash_hooks(raw_crash_path: &Path, _data_dir: &Path) {
    super::unix::install_signal_handlers(raw_crash_path);
}

/// The `Threads:` line of `/proc/self/status`.
pub(crate) fn current_thread_count() -> usize {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(count) = line.strip_prefix("Threads:") {
                return count.trim().parse().unwrap_or(0);
            }
        }
    }
    0
}

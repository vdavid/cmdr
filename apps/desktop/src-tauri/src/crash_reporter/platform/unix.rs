use std::path::Path;

use super::super::signal_handler;

/// The async-signal-safe handlers for the fatal signals, writing raw addresses to a pre-opened
/// fd at `raw_crash_path`.
pub(crate) fn install_signal_handlers(raw_crash_path: &Path) {
    signal_handler::install(raw_crash_path);
}

/// Read through `dladdr` on the main image (`signal_handler.rs`).
pub(crate) fn current_image_base() -> u64 {
    signal_handler::current_image_base()
}

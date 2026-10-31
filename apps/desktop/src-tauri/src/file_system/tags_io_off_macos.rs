//! `tags_io` off macOS: Finder tags stop at the Mac, so what the rest of the app calls
//! on every platform answers "nothing to do". The real one: `tags_io.rs`.

use cmdr_fs::deadline::TimedOut;
use std::path::{Path, PathBuf};

const NOTHING_TO_DO: TimedOut<()> = TimedOut {
    data: (),
    timed_out: false,
};

/// The `enrich_tags` command. A row off macOS holds no tags to read.
pub async fn enrich_listing(_listing_id: String, _paths: Vec<String>) -> TimedOut<()> {
    NOTHING_TO_DO
}

/// A watcher saw `paths` change. No tags to read again.
pub fn refresh_rows(_volume_id: &str, _parent: &Path, _paths: Vec<PathBuf>) {}

/// The `toggle_tags` command. No tags to toggle.
pub async fn toggle_for_command(
    _app: tauri::AppHandle,
    _listing_id: String,
    _paths: Vec<String>,
    _color: u8,
) -> TimedOut<()> {
    NOTHING_TO_DO
}

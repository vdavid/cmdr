//! Where the app keeps Multi-Rename's state: the last settings and the field history
//! (one `crate::recents` file each), and the Results file's path. The presets file is
//! `preset_store`.

use std::path::PathBuf;

use crate::recents::{RecentEntry, RecentsFile};

use super::history::FieldHistoryEntry;
use super::presets::LastSpec;

/// The settings the sheet last closed with, so the next ⌃M opens where the last
/// one left off (TC keeps them too). One entry, in its own file.
pub static LAST_SPEC: RecentsFile<LastSpec> = RecentsFile::new();

impl RecentEntry for LastSpec {
    const FILENAME: &'static str = "multi-rename-last.json";
    const LOG_TARGET: &'static str = "multi_rename::presets";
    const LOG_NAME: &'static str = "multi-rename last settings";
    const STATE_FILE: crate::state_recovery::StateFile = crate::state_recovery::StateFile::MultiRenameLastSettings;
}

/// The history. Recorded by apply, never by typing, so it holds values that
/// renamed something.
pub static FIELD_HISTORY: RecentsFile<FieldHistoryEntry> = RecentsFile::new();

impl RecentEntry for FieldHistoryEntry {
    const FILENAME: &'static str = "multi-rename-history.json";
    const LOG_TARGET: &'static str = "multi_rename::history";
    const LOG_NAME: &'static str = "multi-rename field history";
    const STATE_FILE: crate::state_recovery::StateFile = crate::state_recovery::StateFile::MultiRenameFieldHistory;
}

/// The Results folder's name inside the app's cache dir.
const RESULTS_DIR_NAME: &str = "multi-rename";

/// Where session `session_id` writes its Results file: `multi-rename/` in the app's cache dir
/// (`config::app_cache_dir`, so a dev or E2E instance keeps its own), which "Delete all Cmdr
/// data" removes whole. `None` when the OS has no cache dir. The process id keeps two Cmdrs on
/// one cache dir off each other's files: session ids count from 1 in each.
pub fn results_path(session_id: &str) -> Option<PathBuf> {
    let name = format!("{}-{session_id}.txt", std::process::id());
    Some(crate::config::app_cache_dir()?.join(RESULTS_DIR_NAME).join(name))
}

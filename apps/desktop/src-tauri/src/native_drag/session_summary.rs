//! Pure per-session outcome accounting for drag-out file promises.
//!
//! The fulfillment service completes one item at a time on a session's serial
//! operation queue. To surface ONE completion toast per drag SESSION (not one
//! per file), the delegate records each item's outcome here as it finishes, and
//! the session-drain point folds them into a [`SessionSummary`] that crosses the
//! IPC boundary to the FE toast bridge.
//!
//! Counts are TOP-LEVEL dragged items, consistent with the selection-split
//! contract the transfer toasts use ("Copied 2 files and 1 folder."): one
//! dragged folder counts as one folder regardless of how many files it contains
//! (the recursive children land but never surface in the count). A failure
//! records the item's leaf name so the FE can name the file the user dropped.
//!
//! This module has NO AppKit and no Tauri dependency — it's plain data folding,
//! unit-tested in isolation. The delegate (in [`super::promises`]) owns the
//! `Send` storage that collects [`ItemOutcome`]s across the queue threads; this
//! module just summarizes them. [`super::promises::SessionCompleteEvent`] maps a
//! [`SessionSummary`] onto the camelCase wire payload the FE toast bridge reads.

/// The outcome of fulfilling one top-level dragged item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    /// The item streamed to its Finder destination. `is_dir` distinguishes a
    /// file from a folder for the selection-split count.
    Succeeded { is_dir: bool },
    /// The item failed (device unplugged, read error, unwritable dest, …). The
    /// partial destination has already been cleaned up by the fulfillment
    /// service; `leaf` is the filename to name in the failure toast.
    Failed { leaf: String },
}

/// Aggregate outcome of a whole drag session. Folded from per-item outcomes at
/// the session-drain point, then mapped onto
/// [`super::promises::SessionCompleteEvent`] (the camelCase wire payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// Top-level files that landed successfully.
    pub files_succeeded: usize,
    /// Top-level folders that landed successfully.
    pub folders_succeeded: usize,
    /// Leaf names of items that failed. Empty on a fully-successful session.
    pub failures: Vec<String>,
}

impl SessionSummary {
    /// Whether anything actually happened. `true` for a clean no-op session
    /// (dropped back into Cmdr, dropped on a terminal): no toast should show.
    /// The drain point uses this to suppress the completion event entirely.
    pub fn is_empty(&self) -> bool {
        self.files_succeeded == 0 && self.folders_succeeded == 0 && self.failures.is_empty()
    }
}

/// Folds a session's per-item outcomes into a [`SessionSummary`].
pub fn summarize(outcomes: &[ItemOutcome]) -> SessionSummary {
    let mut files_succeeded = 0;
    let mut folders_succeeded = 0;
    let mut failures = Vec::new();

    for outcome in outcomes {
        match outcome {
            ItemOutcome::Succeeded { is_dir: true } => folders_succeeded += 1,
            ItemOutcome::Succeeded { is_dir: false } => files_succeeded += 1,
            ItemOutcome::Failed { leaf } => failures.push(leaf.clone()),
        }
    }

    SessionSummary {
        files_succeeded,
        folders_succeeded,
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> ItemOutcome {
        ItemOutcome::Succeeded { is_dir: false }
    }
    fn folder() -> ItemOutcome {
        ItemOutcome::Succeeded { is_dir: true }
    }
    fn failed(leaf: &str) -> ItemOutcome {
        ItemOutcome::Failed { leaf: leaf.to_string() }
    }

    #[test]
    fn empty_session_summarizes_to_all_zero_and_is_empty() {
        let summary = summarize(&[]);
        assert_eq!(summary.files_succeeded, 0);
        assert_eq!(summary.folders_succeeded, 0);
        assert!(summary.failures.is_empty());
        assert!(
            summary.is_empty(),
            "a drained session with no items must read as empty (no toast)"
        );
    }

    #[test]
    fn single_file_success() {
        let summary = summarize(&[file()]);
        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(summary.folders_succeeded, 0);
        assert!(summary.failures.is_empty());
        assert!(!summary.is_empty());
    }

    #[test]
    fn mixed_files_and_folders_split_top_level_counts() {
        // Two files and one folder — the selection-split contract.
        let summary = summarize(&[file(), file(), folder()]);
        assert_eq!(summary.files_succeeded, 2);
        assert_eq!(summary.folders_succeeded, 1);
        assert!(summary.failures.is_empty());
    }

    #[test]
    fn failures_record_leaf_names_and_are_not_counted_as_successes() {
        let summary = summarize(&[file(), failed("video.mov"), folder()]);
        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(summary.folders_succeeded, 1);
        assert_eq!(summary.failures, vec!["video.mov".to_string()]);
    }

    #[test]
    fn all_failed_session_has_no_successes() {
        let summary = summarize(&[failed("a.jpg"), failed("b.jpg")]);
        assert_eq!(summary.files_succeeded, 0);
        assert_eq!(summary.folders_succeeded, 0);
        assert_eq!(summary.failures, vec!["a.jpg".to_string(), "b.jpg".to_string()]);
        assert!(
            !summary.is_empty(),
            "a session where everything failed still surfaces a failure toast"
        );
    }
}

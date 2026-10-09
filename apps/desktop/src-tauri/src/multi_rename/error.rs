//! Why a Multi-Rename session, preview, or apply didn't answer. Its own module so `session` and `run` don't depend on each other.

use serde::{Deserialize, Serialize};

use crate::file_system::write_operations::RenameStartError;

use super::plan::SpecError;

/// Why a session, a preview, or an apply didn't answer. Typed, so the frontend words it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MultiRenameError {
    /// The pane's listing is no longer cached (it moved on).
    Gone { listing_id: String },
    /// The pane's rows aren't the listing's state any more (a file came or went
    /// between the selection and the open): open again.
    SelectionChanged { listing_id: String },
    /// The session ended (the sheet closed, or it sat idle and another opened).
    SessionClosed,
    /// The spec doesn't parse; the sheet shows it under its field.
    Spec { error: SpecError },
    /// No row is ready to rename.
    NothingToRename,
    /// No volume answers for the folder (unplugged, disconnected).
    NotConnected { volume_id: String },
    /// The executor refused before renaming anything.
    CouldntStart { reason: RenameStartError },
    /// The folder changed since the preview the user started from, or a newer
    /// preview replaced it: re-preview.
    PreviewOutOfDate,
    /// The folder is read-only (inside an archive or a `.git` portal).
    ReadOnly,
    /// The work didn't finish within its deadline.
    TimedOut,
    /// The worker failed; `detail` is log text only.
    Internal { detail: String },
}

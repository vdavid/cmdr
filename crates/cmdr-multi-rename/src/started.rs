//! What a started apply answers. Its own module so the host trait can name it
//! (`MultiRenameHost::rename_applied`) without `host` reaching into `run`, which
//! reaches `session`, which reaches `host`: `module-cycles` holds that circle shut.
//! The public path stays `run::MultiRenameStarted`.

use serde::{Deserialize, Serialize};

/// A started rename.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameStarted {
    /// The operation, for its progress, the queue, and Undo.
    pub operation_id: String,
    /// How many rows it renames.
    pub renaming: usize,
    /// Rows that swap names with each other, which a batch that runs as a move
    /// (a rename that copies, on S3) leaves out: they keep their names.
    pub swaps_left_out: usize,
}

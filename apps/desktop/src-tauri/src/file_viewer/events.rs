//! The Tauri events a viewer window hears about its file's bytes on the way in: the app's
//! wire for the viewer's facts ([`MediaLoad`], [`PullProgress`]). Both go `emit_to` the
//! window that asked. Their JSON equals the facts' own, which the wire pins hold
//! (`wire_characterization_test.rs`).

use serde::{Deserialize, Serialize};

use super::media_volume::{MediaLoad, MediaLoadStage};
use super::pending_open::PullProgress;

/// `viewer-media-load`: how a volume-served picture's or PDF's request is going. Emitted
/// to the viewer window that asked, so it can show progress over an `<img>` that won't
/// say, and tell a dead server from a damaged file (an `<embed>` says neither).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ViewerMediaLoad {
    /// The token the request came in on, so a window ignores news of a closed session.
    pub token: String,
    pub stage: MediaLoadStage,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl From<MediaLoad> for ViewerMediaLoad {
    fn from(load: MediaLoad) -> Self {
        Self {
            token: load.token,
            stage: load.stage,
            bytes_done: load.bytes_done,
            bytes_total: load.bytes_total,
        }
    }
}

/// `viewer-pull-progress`: how much of the file a viewer has pulled so far. Emitted to
/// the viewer window that asked for the file, while its open pulls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ViewerPullProgress {
    pub bytes_done: u64,
    /// `None` when the source didn't say how big the file is.
    pub bytes_total: Option<u64>,
}

impl From<PullProgress> for ViewerPullProgress {
    fn from(progress: PullProgress) -> Self {
        Self {
            bytes_done: progress.bytes_done,
            bytes_total: progress.bytes_total,
        }
    }
}

//! Where a listing's rows came from, and how far they can be trusted: the
//! `coverage` block every `list_dir` answer carries (`tools/DETAILS.md`
//! § The honesty contract). The live read's own block is `live.rs`'s.

use serde::Serialize;

use super::VolumeView;
use crate::tools::resources::indexing::status_token;
use cmdr_index::Freshness;

/// Where a listing's rows came from. Every answer names it, so a read from
/// anywhere but the volume's live index can't pass for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ListSource {
    /// The volume's live drive index.
    Index,
    /// One readdir of the connected volume, just now: its index has no row here
    /// (or it has no index at all). `live.rs`.
    Live,
    /// The volume isn't connected: the index its last walk left on disk, read
    /// only, dated by `asOf`.
    OfflineIndex,
}

/// The index's honesty for a read: where the rows came from, its freshness token,
/// whether reads are authoritative (`fresh` only), and a plain-language caveat
/// when they aren't (or when the path isn't in the index).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub source: ListSource,
    /// `fresh` / `scanning` / `stale` / `off`. The shared token every surface uses.
    pub index_status: String,
    /// Reads are authoritative only when the index is `fresh` AND lists the folder.
    pub authoritative: bool,
    /// When an offline answer's index last finished a walk, spoken. Present only
    /// on `offlineIndex`, and absent there when that walk never finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Which copy of a volume's index a listing reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IndexReach {
    /// The volume is connected: its live index, if it has one (`enabled` and
    /// `freshness` say how it stands).
    Connected,
    /// The volume isn't connected: the database its last walk left answers.
    Offline {
        /// The volume's name, for the note.
        name: String,
        /// When that walk finished (unix seconds), `None` when it never did.
        as_of: Option<u64>,
    },
}

/// The coverage block for an index answer, from whichever copy of the index
/// [`VolumeView::reach`] says answered.
pub(super) fn index_coverage(volume: &VolumeView, indexed: bool) -> Coverage {
    match &volume.reach {
        IndexReach::Connected => coverage(volume.enabled, volume.freshness, indexed),
        IndexReach::Offline { name, as_of } => offline_coverage(name, *as_of),
    }
}

/// Build the coverage block. `indexed` is whether the path actually resolved in a
/// live index (so `false` distinguishes "not in the index" from an empty folder).
/// Reuses `status_token` + `Freshness::is_authoritative` so it can't drift from the
/// rest of the app.
pub(super) fn coverage(enabled: bool, freshness: Option<Freshness>, indexed: bool) -> Coverage {
    // A fresh index vouches only for the folders it lists: unlisted ground was read
    // by nobody, however fresh the volume.
    let authoritative = indexed && freshness.is_some_and(|f| f.is_authoritative());
    let note = if !indexed {
        Some(if enabled {
            "This folder isn't in the drive index — it may be new, hidden, or outside the indexed area.".to_string()
        } else {
            "This volume isn't indexed, so I can't read it from the index.".to_string()
        })
    } else if !authoritative {
        Some(match freshness {
            Some(Freshness::Scanning) => "The index is still scanning, so this may be incomplete.".to_string(),
            _ => "The index may have drifted since the last full scan, so treat this as best-effort.".to_string(),
        })
    } else {
        None
    };
    Coverage {
        source: ListSource::Index,
        index_status: status_token(enabled, freshness).to_string(),
        authoritative,
        as_of: None,
        note,
    }
}

/// The coverage block for a volume that isn't connected, answered from its index
/// on disk: `stale` plus `asOf`, the one offline vocabulary `list_volumes` speaks
/// too (`read/volumes.rs`), and ❌ never authoritative, since nothing has looked
/// at the volume since.
fn offline_coverage(name: &str, as_of: Option<u64>) -> Coverage {
    let note = match as_of {
        Some(at) => format!(
            "{name} isn't connected, so this is its drive index as of {}, and can't see changes since. Connect it for a live answer.",
            crate::search::format_timestamp(at)
        ),
        None => format!(
            "{name} isn't connected, so this is what its drive index holds, from a walk that never finished. Connect it for a live answer."
        ),
    };
    Coverage {
        source: ListSource::OfflineIndex,
        index_status: "stale".to_string(),
        authoritative: false,
        as_of: as_of.map(crate::search::format_timestamp),
        note: Some(note),
    }
}

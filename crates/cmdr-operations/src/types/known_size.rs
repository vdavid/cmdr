//! A size and how far it can be trusted, as the clash prompt and the
//! drive-index seam both speak it.

use serde::{Deserialize, Serialize};

/// A size, and how far it can be trusted: the whole total, or only a floor.
///
/// A folder's recursive size comes from the drive index, which knows when it
/// has counted only part of the subtree (a first index still running, an
/// unreadable subfolder). That count is a lower bound, and a person deciding
/// whether to replace the folder must never see it as the folder's size: a
/// 40 GB tree would read as 12 KB. Two variants, so no reader can take the
/// floor for the total without naming it. A size nothing can vouch for is
/// `None` around this, ❌ never a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum KnownSize {
    /// The whole size.
    Exact {
        /// Bytes.
        bytes: u64,
    },
    /// At least this much; the real size may be larger. Rendered with `≥`.
    AtLeast {
        /// Bytes counted so far.
        bytes: u64,
    },
}

impl KnownSize {
    /// The size when it's the whole total, `None` for a floor. Anything that
    /// compares two sizes (a "larger" badge, "overwrite smaller") reads this.
    pub fn exact(self) -> Option<u64> {
        match self {
            Self::Exact { bytes } => Some(bytes),
            Self::AtLeast { .. } => None,
        }
    }
}

//! What a volume can keep and report of a copy's metadata: the one place a
//! backend states it, so the engine knows which classes a copy is about to leave
//! behind, and the conformance layer holds every backend to its claim.

use super::{MetadataClasses, UserFlags};

/// Which metadata classes a volume keeps on what a copy writes, and which its
/// read streams report. mtime isn't a class here: every backend keeps a file's
/// (`VolumeReadStream::modified_at`); only a folder's is a capability.
///
/// A claim about the BACKEND, answered without touching a path, like every
/// predicate `capabilities()` folds. ❗ A backend claiming a class it doesn't
/// honor fails `cmdr_fs::volume::conformance::metadata`, and one that keeps a
/// class it doesn't claim makes every copy onto it report a loss that didn't
/// happen.
// DEFAULT-OK: nothing kept and nothing reported, the honest answer of a backend
// that stores no metadata beyond a file's mtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetadataCaps {
    /// What a FILE a copy writes here keeps: through `Volume::write_from_stream`,
    /// and for the user flags also through the `Volume::apply_metadata` the copy
    /// spends after the landing rename ([`Self::staged_flags`]).
    pub stores: MetadataClasses,
    /// What this volume's read streams report from what their open already
    /// holds (`VolumeReadStream::metadata`). A class not in here is unknown,
    /// never "absent": a copy off this volume never reports it lost.
    pub reports: MetadataClasses,
    /// What `Volume::apply_metadata` keeps by path: a FOLDER's set once its
    /// subtree lands (the copy's one path-based write today).
    pub folder_stores: MetadataClasses,
    /// Whether a folder created here keeps its source's modification date.
    pub folder_dates: bool,
    /// The user flags a FILE keeps through `write_from_stream` itself, on the
    /// staged temp, because they refuse no rename (SMB's Hidden rides the
    /// temp's SET_INFO). When a file carries any OTHER flag, the copy spends one
    /// `apply_metadata` after the landing with the whole set
    /// ([`CarriedMetadata::after_landing`](super::CarriedMetadata::after_landing)),
    /// so a destination that sets its flags as one word (SMB's attributes)
    /// keeps these too. Empty: every flag waits for the landing.
    pub staged_flags: UserFlags,
}

impl MetadataCaps {
    /// Keeps and reports nothing beyond a file's mtime: MTP and most WebDAV servers.
    pub const NONE: Self = Self {
        stores: MetadataClasses::NONE,
        reports: MetadataClasses::NONE,
        folder_stores: MetadataClasses::NONE,
        folder_dates: false,
        staged_flags: UserFlags::NONE,
    };

    /// Reports a mode and keeps nothing: a source whose files carry a real mode
    /// but nowhere a copy can store anything (an archive, a git snapshot, an
    /// Android phone over ADB).
    pub const MODE_REPORTER: Self = Self {
        reports: MetadataClasses::MODE,
        ..Self::NONE
    };

    /// Keeps and reports every class, on files and folders.
    pub const ALL: Self = Self {
        stores: MetadataClasses::ALL,
        reports: MetadataClasses::ALL,
        folder_stores: MetadataClasses::ALL,
        folder_dates: true,
        staged_flags: UserFlags::NONE,
    };

    /// The classes a FILE copied from `source` onto this volume can't keep,
    /// among those `source` reports: the ones a copy has to ask the source
    /// about, and report when it had one. Empty for every pair that loses
    /// nothing, so such a copy asks nothing.
    pub fn file_classes_at_risk_from(self, source: Self) -> MetadataClasses {
        source.reports.without(self.stores)
    }
}

/// What a copy will ask of a source's read stream beyond its bytes, told to the
/// source when it opens (`Volume::open_read_stream_for_copy`), so a backend whose
/// open can learn a class in a frame it already sends knows to, and one whose
/// open can't skips the cost for every copy that never asks.
// DEFAULT-OK: asks nothing, which is what any read outside a copy needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetadataAsk {
    /// The classes whose VALUES the destination writes
    /// (`VolumeReadStream::metadata`).
    pub values: MetadataClasses,
    /// The classes the loss note only needs to know the source HAS
    /// (`VolumeReadStream::metadata_outline`): an xattr's name, not its value.
    pub outline: MetadataClasses,
}

impl MetadataAsk {
    /// What a copy of a file from `source` onto `destination` asks: the values
    /// the destination keeps, and an outline of what it can't.
    pub fn for_copy(source: MetadataCaps, destination: MetadataCaps) -> Self {
        Self {
            values: source.reports.intersection(destination.stores),
            outline: destination.file_classes_at_risk_from(source),
        }
    }

    /// Whether the copy asks about `class` at all, values or outline.
    pub fn mentions(self, class: MetadataClasses) -> bool {
        self.values.union(self.outline).contains(class)
    }
}

#[cfg(test)]
#[path = "caps_tests.rs"]
mod tests;

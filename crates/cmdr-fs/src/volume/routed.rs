//! What a path-aware volume lookup answers: which volume serves a path, and
//! whether a route swapped it in.
//!
//! The host's registry (`VolumeManager::resolve` in the app) is what produces a
//! [`ResolvedVolume`]; the type lives here so a consumer that only READS the
//! answer (the listing engine's routing questions, a future non-Tauri host) can
//! name it without reaching into the app.

use super::Volume;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Which route swapped the volume in, when one did.
///
/// Every variant means the same thing to most readers — "this isn't the drive
/// the caller named, it's a volume mapping a namespace onto it" — so most sites
/// ask [`ResolvedVolume::is_routed`]. A write path asks the two capability
/// methods below, which match exhaustively so the next routed kind has to
/// answer them. Compare to ONE variant only where the answer is genuinely about
/// that backend (the archive-edit driver); a reader that cares whether an
/// archive serves the path, wherever it sits, asks [`RoutedKind::is_archive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutedKind {
    /// The path crossed a `.zip` boundary into a `cmdr_archive::ArchiveVolume`.
    Archive,
    /// The path reached into a repo's virtual `.git` trees, so a
    /// `cmdr_git::GitPortalVolume` serves it.
    GitPortal,
    /// The path reached into a restic repo's `Browse snapshots` view, so a
    /// `cmdr_restic::ResticVolume` serves it.
    Restic,
    /// The path crossed into an archive that sits INSIDE a restic snapshot, so a
    /// `cmdr_archive::ArchiveVolume` reading through the snapshot's view serves
    /// it. Browsable like any archive, but the snapshot can't be rewritten, so it
    /// takes no write and gives up no file.
    ArchiveInSnapshot,
}

impl RoutedKind {
    /// Whether a write may land inside this route: a copy, move, or new entry
    /// INTO it. Only an archive takes one, through its managed edit driver; a
    /// snapshot (git or restic) is history and refuses before anything is read.
    pub fn takes_writes(self) -> bool {
        match self {
            Self::Archive => true,
            Self::GitPortal | Self::Restic | Self::ArchiveInSnapshot => false,
        }
    }

    /// Whether a move may take files OUT of this route, which needs the source
    /// gone once the copy lands. An archive rewrites itself without them; a
    /// snapshot has no file to remove, so a move out of one is refused before
    /// anything is copied.
    pub fn can_give_up_files(self) -> bool {
        match self {
            Self::Archive => true,
            Self::GitPortal | Self::Restic | Self::ArchiveInSnapshot => false,
        }
    }

    /// Whether an `ArchiveVolume` serves the path, wherever the archive sits: what
    /// the archive preview, the viewer's error mapping, and `inspect_file` ask.
    pub fn is_archive(self) -> bool {
        match self {
            Self::Archive | Self::ArchiveInSnapshot => true,
            Self::GitPortal | Self::Restic => false,
        }
    }
}

/// Outcome of a path-aware volume lookup: the volume that should serve `path`.
///
/// `path` is ALWAYS the caller's input path unchanged. A routed resolve only
/// swaps in the read-only volume — which maps the full path into its own
/// namespace itself — and a passthrough returns the requested volume untouched.
/// Adoption sites read `resolved.path` so the "full path, unchanged" contract
/// lives in exactly one place.
pub struct ResolvedVolume {
    /// The volume to call, or `None` when `volume_id` isn't registered (an
    /// unmount race). Sites keep their existing `.ok_or_else(...)?` handling.
    pub volume: Option<Arc<dyn Volume>>,
    /// The path to pass to `volume`'s methods — the input path, verbatim.
    pub path: PathBuf,
    /// Which route swapped `volume` in, or `None` for a passthrough. Sites use
    /// it to skip drive-index enrich/verify and the read-only write guards.
    pub routed: Option<RoutedKind>,
    /// A route whose boundary is in `path` but couldn't be confirmed or ruled out
    /// (its parent failed or outran the deadline), on a passthrough. A read may go
    /// to the drive; ❗ a write must refuse, or it could write a real folder into
    /// what is a repo's read-only view (a real `Browse snapshots/` inside a
    /// restic repo).
    pub unconfirmed: Option<RoutedKind>,
}

impl ResolvedVolume {
    /// An unrouted resolve: the requested volume (if any), path unchanged.
    pub fn passthrough(volume: Option<Arc<dyn Volume>>, path: &Path) -> Self {
        Self {
            volume,
            path: path.to_path_buf(),
            routed: None,
            unconfirmed: None,
        }
    }

    /// A passthrough over a path where the `kind` route couldn't decide: reads
    /// go to the drive, writes refuse ([`unconfirmed`](Self::unconfirmed)).
    pub fn undecided(volume: Option<Arc<dyn Volume>>, path: &Path, kind: RoutedKind) -> Self {
        Self {
            unconfirmed: Some(kind),
            ..Self::passthrough(volume, path)
        }
    }

    /// Whether a route swapped the volume in. True for every read-only routed
    /// volume, whichever kind: a routed path has no drive-index entry and takes
    /// no writes, and both of those follow from being routed at all.
    pub fn is_routed(&self) -> bool {
        self.routed.is_some()
    }
}

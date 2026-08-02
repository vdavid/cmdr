//! A volume's identity in the index: its id and its kind.
//!
//! A leaf module on purpose. The registry in `lifecycle/state.rs` owns a
//! volume's *lifecycle*; these two describe *which* volume and *what sort*, and
//! everything from path routing to the scan transports needs them without
//! needing the registry. Same pattern as `metadata.rs`. Keep it free of
//! behavior: pure predicates only, nothing that reads state.

/// A volume's identity in the index registry (e.g. `"root"` for the local disk).
pub(crate) type VolumeId = String;

/// The local-disk volume id. The only volume registered when no network drive
/// is indexed.
pub const ROOT_VOLUME_ID: &str = "root";

/// How a volume's index is scanned, watched, rooted, and searched.
///
/// Four capabilities that move together for the three original kinds but pull
/// apart for [`LocalExternal`](IndexVolumeKind::LocalExternal), so each is an
/// explicit, orthogonal method rather than a single conflated predicate:
///
/// - [`uses_local_scanner`](Self::uses_local_scanner): the guarded walker + FSEvents pipeline
///   (`Local`, `LocalExternal`) vs the `Volume` trait scanner (`Smb`, `Mtp`).
///   Its exact complement is [`is_trait_scanned`](Self::is_trait_scanned).
/// - [`has_event_journal`](Self::has_event_journal): self-heals watch continuity
///   by replaying an FSEvents journal on launch. Only the boot disk (`Local`).
/// - [`mount_rooted`](Self::mount_rooted): the index `ROOT_ID` is the mount
///   (`/Volumes/X`), not `/`. True for `LocalExternal`, `Smb`, `Mtp`.
/// - [`feeds_search`](Self::feeds_search): the single volume whose writes back
///   the in-memory search index. Only the boot disk (`Local`).
///
/// The kinds:
///
/// - [`Local`](IndexVolumeKind::Local): the boot disk. The guarded walker's scan + FSEvents
///   journal, so a persisted index replays to **Fresh** on launch (continuity
///   self-heals). `/`-rooted and the sole search-feeding volume. The only kind
///   started when no network drive is indexed.
/// - [`LocalExternal`](IndexVolumeKind::LocalExternal): a plain local external
///   drive (USB stick, SD card, extra disk, mounted disk image). Uses the same
///   guarded walker + FSEvents pipeline as `Local`, but mount-rooted (`ROOT_ID` =
///   `/Volumes/X`). It has no FSEvents journal (external volumes carry no
///   `.fseventsd`), so a persisted index loads **Stale** on launch; live
///   FSEvents still fire while mounted, so a running watcher keeps it current.
///   Doesn't feed search.
/// - [`Smb`](IndexVolumeKind::Smb): an SMB share scanned over the `Volume` trait
///   (no guarded walker; `/Volumes/` is excluded from the local scanner). Mount-rooted.
///   No event journal, so a persisted index loads **Stale** on launch and the
///   live watcher is what keeps it Fresh while connected.
/// - [`Mtp`](IndexVolumeKind::Mtp): a phone/camera storage scanned over the same
///   `Volume` trait. Identical to `Smb` for indexing purposes (non-journaled,
///   mount-rooted, network/USB scan path, loads Stale on launch); the live PTP
///   event loop keeps it Fresh while the device is connected (D4). A distinct
///   variant only so the scan path and any future MTP-specific tuning have a
///   name to branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexVolumeKind {
    /// The boot disk.
    Local,
    /// A locally-attached external drive: scanned and watched like the boot disk,
    /// but it can vanish, so it's the one kind an unmount has to stop.
    LocalExternal,
    /// An SMB share, scanned through the `Volume` trait.
    Smb,
    /// An MTP device (a phone), scanned through the `Volume` trait.
    Mtp,
}

impl IndexVolumeKind {
    /// Whether this volume is scanned and watched by the local guarded walker + FSEvents
    /// pipeline rather than the `Volume` trait scanner. True for the boot disk
    /// and local external drives. Exact complement of
    /// [`is_trait_scanned`](Self::is_trait_scanned).
    pub fn uses_local_scanner(self) -> bool {
        matches!(self, IndexVolumeKind::Local | IndexVolumeKind::LocalExternal)
    }

    /// Whether this volume scans over the `Volume` trait (network/USB) rather
    /// than the local guarded walker. SMB and MTP both do. Exact complement of
    /// [`uses_local_scanner`](Self::uses_local_scanner).
    pub fn is_trait_scanned(self) -> bool {
        matches!(self, IndexVolumeKind::Smb | IndexVolumeKind::Mtp)
    }

    /// Whether this volume self-heals watch continuity from an event journal on
    /// launch. Only the local boot disk does (FSEvents replay). Feeds
    /// `freshness::initial_freshness_on_launch`. Local external drives carry no
    /// `.fseventsd`, and SMB and MTP have no journal.
    pub fn has_event_journal(self) -> bool {
        matches!(self, IndexVolumeKind::Local)
    }

    /// Whether the index's `ROOT_ID` is the volume's mount point (`/Volumes/X`)
    /// rather than `/`. True for every volume except the boot disk: local
    /// external drives, SMB shares, and MTP devices all index relative to their
    /// mount.
    ///
    /// Consumed by [`IndexPathSpace`](crate::indexing::IndexPathSpace) to decide
    /// whether the local scan/reconcile/live pipeline strips a mount root before
    /// `store::resolve_path`, and to pick the `scanner::exclusions::ExclusionScope`.
    pub fn mount_rooted(self) -> bool {
        matches!(
            self,
            IndexVolumeKind::LocalExternal | IndexVolumeKind::Smb | IndexVolumeKind::Mtp
        )
    }

    /// Whether this volume's writes back the single in-memory search index.
    /// Search is single-volume by construction (D7): only the boot disk
    /// (`Local`) feeds it. See `writer::WRITER_GENERATION`.
    pub fn feeds_search(self) -> bool {
        matches!(self, IndexVolumeKind::Local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `IndexVolumeKind`, so a new variant can't be added without deciding
    /// its capabilities here.
    const ALL_KINDS: [IndexVolumeKind; 4] = [
        IndexVolumeKind::Local,
        IndexVolumeKind::LocalExternal,
        IndexVolumeKind::Smb,
        IndexVolumeKind::Mtp,
    ];

    /// The five capability axes must match the plan's table exactly. Each tuple is
    /// `(uses_local_scanner, is_trait_scanned, has_event_journal, mount_rooted,
    /// feeds_search)`.
    #[test]
    fn capability_axes_match_the_table() {
        let expected = |kind: IndexVolumeKind| -> (bool, bool, bool, bool, bool) {
            (
                kind.uses_local_scanner(),
                kind.is_trait_scanned(),
                kind.has_event_journal(),
                kind.mount_rooted(),
                kind.feeds_search(),
            )
        };

        // (local_scanner, trait_scanned, event_journal, mount_rooted, feeds_search)
        assert_eq!(expected(IndexVolumeKind::Local), (true, false, true, false, true));
        assert_eq!(
            expected(IndexVolumeKind::LocalExternal),
            (true, false, false, true, false)
        );
        assert_eq!(expected(IndexVolumeKind::Smb), (false, true, false, true, false));
        assert_eq!(expected(IndexVolumeKind::Mtp), (false, true, false, true, false));
    }

    /// `uses_local_scanner` and `is_trait_scanned` are exact complements: every
    /// kind is scanned by exactly one of the two pipelines, so they can't silently
    /// drift (a new variant landing in neither, or both, fails here).
    #[test]
    fn scanner_axes_partition_the_enum() {
        for kind in ALL_KINDS {
            assert_ne!(
                kind.uses_local_scanner(),
                kind.is_trait_scanned(),
                "{kind:?} must be scanned by exactly one pipeline"
            );
        }
    }
}

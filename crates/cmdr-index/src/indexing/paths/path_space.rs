//! `IndexPathSpace`: how one volume's local scan / reconcile / live pipeline maps
//! between the absolute FS path and the index-relative path.
//!
//! This is the WRITE-pipeline seam: built once per scan or loop, then threaded
//! through the scanner, the reconciler, the watcher, and scan completion. The read
//! side has only a volume id at hand, so it asks [`routing`](super::routing)
//! (`index_read_path`). Both strip a mount root with the same
//! `transports::smb::watch::index_relative_path`, never a second copy.

use std::path::Path;

use rusqlite::Connection;

use crate::indexing::scanner::ExclusionScope;
use crate::indexing::store::{self, IndexStoreError};
use crate::indexing::volume::IndexVolumeKind;
use cmdr_fs::firmlinks;

/// How a volume's LOCAL scan/reconcile/live pipeline maps between the path spaces
/// its code touches, so the same code drives both the `/`-rooted boot disk and a
/// mount-rooted external drive without forking.
///
/// The pipeline handles the SAME path string in two spaces:
/// - **absolute FS path** — `read_dir`, `symlink_metadata`, `Path::exists`, and the
///   `DirsUpdated` emit (which must match pane paths);
/// - **index-relative path** — the argument `store::resolve_path` walks from
///   `ROOT_ID`.
///
/// For the boot disk the two coincide after firmlink normalization, so this is a
/// pass-through. For a `mount_rooted()` volume the index `ROOT_ID` is the mount
/// (`/Volumes/X`), so the mount root is stripped to reach the index-relative path —
/// via the SAME [`smb_watch::index_relative_path`](crate::indexing::transports::smb::watch::index_relative_path)
/// transform the SMB read/write sides funnel through, never a second copy.
///
/// **Discipline (the trap):** keep every path SET (`affected_paths`,
/// `pending_paths`, `new_dir_paths`) in the absolute space via [`absolute`]; apply
/// the mount-relative strip ONLY at the `store::resolve_path` argument via
/// [`resolve_abs`]. Stripping at set insertion breaks the FS reads and the FE emit;
/// omitting it breaks resolution.
///
/// [`absolute`]: IndexPathSpace::absolute
/// [`resolve_abs`]: IndexPathSpace::resolve_abs
#[derive(Clone, Debug)]
pub(crate) struct IndexPathSpace {
    /// Where this volume is rooted, held AS the [`ExclusionScope`] the pipeline
    /// gates paths with (`boot_disk` for `/`, `mount_rooted` for the prefix stripped
    /// to reach the index-relative path). One home for the mount root, so the space
    /// and the exclusion gate can't disagree about where the volume starts.
    scope: ExclusionScope,
    /// Whether this volume's filesystem inode is a trustworthy identity, resolved
    /// ONCE per scan from the volume's [`FilesystemKind`](cmdr_fs::filesystem_kind::FilesystemKind).
    /// `false` only for a local external drive on FAT/exFAT (derived, unstable
    /// inodes). When `false`, the local scan/reconcile/live pipeline stores
    /// `inode: None` for every entry so the rename pre-pass can never match — an
    /// inode-reused delete+create must not become a false `MoveEntryV2` (see
    /// `has_stable_inodes` and [`trust_inode`](Self::trust_inode)). The boot disk
    /// (APFS) and every trait-scanned volume (SMB/MTP, which don't run the local
    /// inode pre-pass) are `true`.
    inodes_trustworthy: bool,
}

impl IndexPathSpace {
    /// The `/`-rooted boot-disk space: absolute == index-relative (after firmlink
    /// normalization). The boot disk is APFS, so inodes are trustworthy.
    pub(crate) fn root() -> Self {
        Self {
            scope: ExclusionScope::boot_disk(),
            inodes_trustworthy: true,
        }
    }

    /// A mount-rooted space whose index `ROOT_ID` is `mount_root` (`/Volumes/X`).
    /// Defaults to trustworthy inodes; use [`for_volume`](Self::for_volume) (or
    /// [`with_inodes_trustworthy`](Self::with_inodes_trustworthy)) to carry a
    /// FAT/exFAT drive's untrusted-inode fact.
    pub(crate) fn mount_rooted(mount_root: impl Into<String>) -> Self {
        Self {
            scope: ExclusionScope::mount_rooted(mount_root),
            inodes_trustworthy: true,
        }
    }

    /// The mount root, or `None` for the `/`-rooted boot disk. Read back from the
    /// scope, which owns it.
    fn mount_root(&self) -> Option<&str> {
        self.scope.mount_root()
    }

    /// Whether this is the `/`-rooted boot disk (as opposed to a mount-rooted
    /// external drive). Read back from the scope, which owns the mount root.
    ///
    /// The shallow-`MustScanSubDirs` sweep window branches on this: the once-a-day
    /// window is boot-disk-only (see `reconcile/reconciler/rescan/route.rs`).
    pub(crate) fn is_boot_disk(&self) -> bool {
        self.mount_root().is_none()
    }

    /// Override the inode-trust flag (builder form). Used by `for_volume` and by
    /// tests exercising the FAT/exFAT nulling path.
    pub(crate) fn with_inodes_trustworthy(mut self, trustworthy: bool) -> Self {
        self.inodes_trustworthy = trustworthy;
        self
    }

    /// Swap the exclusion scope (tests only), so a walk can run against injected
    /// root probes without a real File Provider domain or Unix root on the
    /// machine. See `ExclusionScope::with_probes`.
    #[cfg(test)]
    pub(crate) fn with_exclusion_scope(mut self, scope: ExclusionScope) -> Self {
        self.scope = scope;
        self
    }

    /// Derive the space from a volume's kind + root path + inode trust: a
    /// `mount_rooted()` kind strips its mount, the boot disk passes through.
    /// `inodes_trustworthy` is resolved once per scan from the volume's
    /// filesystem (see `local_external_index::classify`); only a FAT/exFAT local
    /// external drive is `false`.
    pub(crate) fn for_volume(kind: IndexVolumeKind, volume_root: &Path, inodes_trustworthy: bool) -> Self {
        let base = if kind.mount_rooted() {
            Self::mount_rooted(volume_root.to_string_lossy().into_owned())
        } else {
            Self::root()
        };
        base.with_inodes_trustworthy(inodes_trustworthy)
    }

    /// Whether this volume's stored inodes are a trustworthy identity. See the
    /// field doc; `false` only for a FAT/exFAT local external drive.
    pub(crate) fn inodes_trustworthy(&self) -> bool {
        self.inodes_trustworthy
    }

    /// Map a freshly-stat'd inode to the value to STORE for this volume: the raw
    /// inode on a trustworthy filesystem, `None` on FAT/exFAT (where a derived,
    /// unstable inode must never reach the index and drive the rename pre-pass).
    /// The single choke point every local write path funnels a snapshot's inode
    /// through before persisting it.
    pub(crate) fn trust_inode(&self, raw: Option<u64>) -> Option<u64> {
        if self.inodes_trustworthy { raw } else { None }
    }

    /// The volume's root path as a string: `/Volumes/X` for a mount-rooted drive, `/`
    /// for the boot disk. Used for the stored `volume_path` meta.
    pub(crate) fn volume_root_string(&self) -> String {
        self.scope.volume_root().to_string()
    }

    /// The exclusion scope this volume's scan/live gate uses. A mount-rooted scan
    /// skips only the per-volume tier — under `BootDisk` its own `/Volumes/X`
    /// subtree would be excluded and the scan would falsely complete empty. The boot
    /// disk keeps the absolute-prefix tier. Either way the scope carries the volume
    /// ROOT, so the root-position pseudo-filesystem skip works on every volume.
    pub(crate) fn exclusion_scope(&self) -> &ExclusionScope {
        &self.scope
    }

    /// Canonicalize a raw FSEvents/`read_dir` path into the absolute path this
    /// volume uses everywhere EXCEPT the `resolve_path` argument (FS reads,
    /// exclusion checks, the FE emit). The boot disk firmlink-normalizes (`/private`
    /// symlinks, Data firmlinks); a mount-rooted external drive keeps the raw path —
    /// firmlink semantics are boot-disk-only and don't apply under `/Volumes`.
    pub(crate) fn absolute(&self, raw_path: &str) -> String {
        if self.mount_root().is_some() {
            raw_path.to_string()
        } else {
            firmlinks::normalize_path(raw_path)
        }
    }

    /// Map a canonical absolute path (from [`absolute`](Self::absolute)) into the
    /// index-relative path `store::resolve_path` walks from `ROOT_ID`: identity for
    /// the `/`-rooted boot disk, mount-root-stripped for a mount-rooted volume.
    ///
    /// `None` means the path lies outside this volume's mount root, so it has no
    /// place in this index at all — drop it rather than mis-root it at `ROOT_ID`.
    ///
    /// Use this only where an index-relative path is the value needed (a scan root
    /// handed to `resolve_scan_root`); to resolve an id, prefer
    /// [`resolve_abs`](Self::resolve_abs), which is this plus the lookup.
    pub(crate) fn index_relative(&self, absolute: &str) -> Option<String> {
        match self.mount_root() {
            None => Some(absolute.to_string()),
            Some(root) => crate::indexing::transports::smb::watch::index_relative_path(root, absolute),
        }
    }

    /// The inverse of [`index_relative`](Self::index_relative): where an index-relative
    /// path sits on disk. Identity for the boot disk, the mount root joined on for a
    /// mount-rooted volume.
    pub(crate) fn absolute_of(&self, relative: &str) -> String {
        join_volume_relative(&self.volume_root_string(), relative)
    }

    /// Resolve a canonical absolute path (from [`absolute`](Self::absolute)) to its
    /// index entry id, applying the mount-relative strip for a mount-rooted volume.
    ///
    /// `Ok(None)` means "not in this index" — the entry is genuinely absent OR the
    /// path lies outside the mount root — which every caller already treats as
    /// skip/no-op (mirrors the SMB read side dropping an off-volume path rather than
    /// mis-rooting it at `ROOT_ID`). Drop-in for a direct `store::resolve_path` call.
    pub(crate) fn resolve_abs(&self, conn: &Connection, absolute: &str) -> Result<Option<i64>, IndexStoreError> {
        match self.index_relative(absolute) {
            Some(rel) => store::resolve_path(conn, &rel),
            None => Ok(None),
        }
    }
}

/// Rebuild an absolute path from a volume root and an index-relative one. The
/// boot disk's index-relative paths are already absolute, so its root (`/`) has to
/// not double up the separator.
pub(crate) fn join_volume_relative(volume_root: &str, relative: &str) -> String {
    let trimmed_root = volume_root.trim_end_matches('/');
    let trimmed_relative = relative.trim_start_matches('/');
    if trimmed_relative.is_empty() {
        // The volume root itself, spelled without a trailing slash like every other
        // absolute path the pipeline hands around.
        return if trimmed_root.is_empty() {
            "/".to_string()
        } else {
            trimmed_root.to_string()
        };
    }
    format!("{trimmed_root}/{trimmed_relative}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::scanner::ExclusionTier;

    /// The `IndexPathSpace` seam: `root` is a pass-through (absolute == index-relative
    /// after firmlink normalization, `BootDisk` scope), while a mount-rooted space
    /// keeps the raw absolute path for FS/emit but resolves in the mount-relative
    /// space (`MountRooted` scope). The mount strip itself is `transports/smb/watch`'s and is
    /// unit-tested there; here we pin the root-vs-mount decision the pipeline branches on.
    #[test]
    fn index_path_space_root_vs_mount() {
        use std::path::Path;

        let root = IndexPathSpace::root();
        assert_eq!(root.exclusion_scope().tier(), ExclusionTier::BootDisk);
        assert_eq!(root.volume_root_string(), "/");
        // Root's `absolute` firmlink-normalizes; a plain path is already canonical.
        assert_eq!(root.absolute("/Users/me/x"), "/Users/me/x");

        let mount = IndexPathSpace::mount_rooted("/Volumes/NONAME");
        assert_eq!(mount.exclusion_scope().tier(), ExclusionTier::MountRooted);
        assert_eq!(mount.volume_root_string(), "/Volumes/NONAME");
        // A mount-rooted space keeps the raw absolute path (no firmlink normalization).
        assert_eq!(mount.absolute("/Volumes/NONAME/sub"), "/Volumes/NONAME/sub");

        // `for_volume` derives the space from the kind + root.
        assert_eq!(
            IndexPathSpace::for_volume(IndexVolumeKind::Local, Path::new("/"), true)
                .exclusion_scope()
                .tier(),
            ExclusionTier::BootDisk,
        );
        assert_eq!(
            IndexPathSpace::for_volume(IndexVolumeKind::LocalExternal, Path::new("/Volumes/NONAME"), true)
                .exclusion_scope()
                .tier(),
            ExclusionTier::MountRooted,
        );
    }

    /// The inode-trust axis: `trust_inode` passes a raw inode through on a
    /// trustworthy volume and nulls it on a FAT/exFAT one, so the local write
    /// paths store `inode: None` there and the rename pre-pass can never match.
    #[test]
    fn index_path_space_trust_inode_nulls_on_untrusted() {
        use std::path::Path;

        let trusted = IndexPathSpace::for_volume(IndexVolumeKind::LocalExternal, Path::new("/Volumes/USB"), true);
        assert!(trusted.inodes_trustworthy());
        assert_eq!(trusted.trust_inode(Some(42)), Some(42), "trusted keeps the inode");

        let untrusted = IndexPathSpace::for_volume(IndexVolumeKind::LocalExternal, Path::new("/Volumes/USB"), false);
        assert!(!untrusted.inodes_trustworthy());
        assert_eq!(untrusted.trust_inode(Some(42)), None, "FAT/exFAT nulls the inode");
        assert_eq!(untrusted.trust_inode(None), None);

        // The boot disk (APFS) is always trustworthy.
        assert!(IndexPathSpace::root().inodes_trustworthy());
    }
}

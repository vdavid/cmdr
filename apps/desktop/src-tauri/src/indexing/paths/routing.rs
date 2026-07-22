//! Path → volume routing and the read-path-space mapping.
//!
//! This module owns two related questions:
//!
//! - **Which volume does a path belong to?** `volume_id_for_local_path` maps a
//!   filesystem (or `mtp://`) path to the index volume id that owns it — an SMB
//!   mount to its `smb_volume_id`, an `mtp://` path to its `{device}:{storage}`
//!   id, a registered local external mount (`/Volumes/X`) to its own id, and the
//!   boot disk (plus cloud-drive folders `root`'s index owns) to `root`.
//! - **How does a read path map into that volume's index path space?**
//!   `index_read_path` (and the pure `index_read_path_pure` it wraps) translate a
//!   mount-absolute listing / dir-stats path into the mount-relative path the
//!   volume's index stores it under, so `store::resolve_path` (which walks from
//!   `ROOT_ID`) hits. Pass-through for `root`, mount-relative strip for SMB,
//!   scheme/storage strip for MTP.
//!
//! These are the read-side mirror of the write-side mount-relative transforms in
//! `smb_watch` / `mtp_watch`. They're kept here, separate from the lifecycle /
//! registry core in `state.rs`, because they're pure path arithmetic the read
//! query surface (`read/queries.rs`) and enrichment (`read/enrichment.rs`) both depend on.

use std::path::Path;

use rusqlite::Connection;

use super::firmlinks;
use crate::indexing::scanner::ExclusionScope;
#[cfg(test)]
use crate::indexing::scanner::ExclusionTier;
use crate::indexing::state::{IndexVolumeKind, ROOT_VOLUME_ID, VolumeId};
use crate::indexing::store::{self, IndexStoreError};

/// Resolve a filesystem path to its index volume id.
///
/// Four routing tiers, tried in order; each maps to the SAME id its volume and
/// index register under, so a read routes to the owning index (or skips cleanly
/// when that volume has no registered index — `get_read_pool_for` → `None` — so an
/// unindexed volume costs zero DB work):
///
/// - **SMB** (`/Volumes/<share>/…` on macOS, an `smbfs`/`cifs` mount on Linux) →
///   `smb_volume_id(server, port, share)`.
/// - **MTP** (`mtp://{device_id}/{storage_id}[/inner…]`) → `{device_id}:{storage_id}`.
/// - **Local external mount** (a registered `/Volumes/X` drive on macOS,
///   `/mnt`/`/media` on Linux) → the mount's registered id, so an external drive's
///   dir-stats and `cmdr://state` status come from ITS OWN index, not `root`'s. See
///   [`external_mount_volume_id_for_path`].
/// - **Everything else** (the boot disk, and cloud-drive folders in the home dir
///   that `root`'s index owns) → `root`.
pub(crate) fn volume_id_for_local_path(path: &str) -> VolumeId {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(smb_id) = crate::indexing::smb_index::smb_volume_id_for_path(path) {
        return smb_id;
    }
    if let Some(mtp_id) = mtp_volume_id_for_path(path) {
        return mtp_id;
    }
    if let Some(mount_id) = external_mount_volume_id_for_path(path) {
        return mount_id;
    }
    ROOT_VOLUME_ID.to_string()
}

/// Resolve a path on a registered local external mount to that mount's volume id,
/// or `None` for a path `root`'s index owns.
///
/// The fast-reject (a pure string check, no registry lock) is the load-bearing
/// distinction: ONLY a path under an excluded mount prefix (`is_on_mounted_external_volume`)
/// can belong to a separate per-mount index. A boot-disk path — and, crucially, a
/// registered cloud-drive folder in the home dir (`~/Library/CloudStorage/…`) — is
/// inside `root`'s indexed tree, so it bails here and stays on `root`, keeping its
/// recursive sizes. (A naive "any registered non-root volume" prefix match would
/// wrongly divert cloud drives to an index-less id and drop their sizes.)
///
/// Past the reject, the id comes from the `VolumeManager` registry (route by what's
/// REGISTERED, never a hardcoded path shape): the non-root volume whose mount root
/// is the longest ancestor of `path`. An external path with no registered mount
/// (drive not in the manager) yields `None` → falls back to `root`.
fn external_mount_volume_id_for_path(path: &str) -> Option<VolumeId> {
    if !crate::indexing::scanner::is_on_mounted_external_volume(path) {
        return None;
    }
    crate::file_system::get_volume_manager().mount_id_for_path(path)
}

/// Map an `mtp://{device_id}/{storage_id}[/…]` path to its MTP volume id
/// `{device_id}:{storage_id}`, or `None` for any non-MTP path. Pure string work
/// (no device lookup): the `mtp://` scheme + the first two segments fully
/// determine the volume id. The storage segment must be numeric so a malformed
/// `mtp://` path doesn't resolve to a bogus volume.
fn mtp_volume_id_for_path(path: &str) -> Option<VolumeId> {
    let without_scheme = path.strip_prefix("mtp://")?;
    let mut parts = without_scheme.splitn(3, '/');
    let device_id = parts.next().filter(|s| !s.is_empty())?;
    let storage = parts.next().filter(|s| s.parse::<u32>().is_ok())?;
    Some(format!("{device_id}:{storage}"))
}

/// The exclusion scope for a volume id on the READ side, where only the id is at
/// hand (enrichment). `root` is the boot disk; every other registered volume is
/// mount-rooted at its registered root, so its own subtree isn't excluded wholesale.
///
/// An UNREGISTERED non-root id yields an empty mount root: still mount-rooted (the
/// tier that matters), but no path can sit at that root, so the root-position
/// pseudo-filesystem rule simply never fires. That's inert in practice — the same
/// registry lookup in [`index_read_path`] runs a moment later and drops the path.
pub(crate) fn exclusion_scope_for_volume(volume_id: &str) -> ExclusionScope {
    if volume_id == ROOT_VOLUME_ID {
        return ExclusionScope::boot_disk();
    }
    let mount_root = crate::file_system::get_volume_manager()
        .get(volume_id)
        .map(|v| v.root().to_string_lossy().into_owned())
        .unwrap_or_default();
    ExclusionScope::mount_rooted(mount_root)
}

/// Map a listing/dir-stats path into the path space the volume's index stores it
/// under, so `store::resolve_path` (which walks component-by-component from
/// `ROOT_ID`) hits.
///
/// This is the READ-side mirror of `smb_watch`'s write-side mount-relative
/// transform, and it's load-bearing: an SMB index's `ROOT_ID` is the share's
/// **mount root** (the scanner maps the scan root to `ROOT_ID`), but enrichment
/// and `get_dir_stats` receive **mount-absolute** paths (`/Volumes/share/sub`).
/// Without stripping the mount root first, `resolve_path` tries to walk
/// `Volumes` / `share` as children of the share root and always misses, so an
/// indexed SMB folder shows no sizes. (Pure over `mount_root`; the live mount
/// lookup is in [`index_read_path`].)
///
/// - `root` (local disk): the index is rooted at `/`, so the firmlink-normalized
///   absolute path is already index-rooted — return it as-is (`mount_root` is
///   `None`). Firmlink normalization is local-only and must not touch virtual
///   SMB/MTP paths.
/// - A non-root volume with a known `mount_root` (SMB): strip the mount root to a
///   mount-relative path via the shared [`smb_watch::index_relative_path`]. A
///   path that isn't under the mount root yields `None` (drop it rather than
///   mis-root it at `ROOT_ID`).
fn index_read_path_pure(volume_id: &str, normalized_abs: &str, mount_root: Option<&str>) -> Option<String> {
    if volume_id == ROOT_VOLUME_ID {
        return Some(normalized_abs.to_string());
    }
    // MTP: the index `ROOT_ID` is the storage root and the volume namespace is
    // `mtp://{device}/{storage}[/inner…]`, so strip the scheme + device/storage
    // segments to the inner `/path` the index stores under (the read-side mirror
    // of how the MTP scan rooted the storage at `ROOT_ID`). A path on a DIFFERENT
    // MTP volume yields `None` (drop rather than mis-root), exactly like SMB.
    if crate::mtp::identity::is_mtp_volume_id(volume_id) {
        return mtp_index_relative_path(volume_id, normalized_abs);
    }
    let mount_root = mount_root?;
    crate::indexing::smb_watch::index_relative_path(mount_root, normalized_abs)
}

/// Strip an `mtp://{device}/{storage}[/inner…]` path to the inner index-relative
/// path (`/` for the storage root, `/DCIM/Camera` for a nested dir), but only if
/// the path belongs to `volume_id`'s device+storage. Pure string work, so the
/// MTP read-side mapping is unit-testable without a device.
///
/// Returns `None` if the path isn't an `mtp://` path for THIS volume (different
/// device/storage, or malformed) — the caller then skips, like an unindexed
/// volume. A plain `/inner` path (already storage-relative, e.g. from a
/// self-mutation notify) is accepted as-is for this MTP volume.
fn mtp_index_relative_path(volume_id: &str, abs_path: &str) -> Option<String> {
    // Already storage-relative (no scheme): trust it for this MTP volume.
    if !abs_path.starts_with("mtp://") {
        return abs_path.starts_with('/').then(|| abs_path.to_string());
    }
    // Scheme form: confirm the device/storage prefix matches this volume id, then
    // return the inner remainder rooted at `/`.
    let path_volume_id = mtp_volume_id_for_path(abs_path)?;
    if path_volume_id != volume_id {
        return None;
    }
    let without_scheme = abs_path.strip_prefix("mtp://")?;
    let mut parts = without_scheme.splitn(3, '/');
    let _device = parts.next()?;
    let _storage = parts.next()?;
    match parts.next() {
        Some(inner) if !inner.is_empty() => Some(format!("/{inner}")),
        _ => Some("/".to_string()),
    }
}

/// Live wrapper over [`index_read_path_pure`]: looks up a non-root volume's mount
/// root from the `VolumeManager` and maps `abs_path` into the volume's index path
/// space. `abs_path` should already be firmlink-normalized for `root`; for a
/// non-root SMB volume firmlinks don't apply (virtual path namespace), so we pass
/// it through unchanged.
///
/// `None` means "this path isn't resolvable in this volume's index" (unknown
/// volume, or a path outside the mount root) — the caller then skips, exactly
/// like an unindexed volume.
pub(crate) fn index_read_path(volume_id: &str, abs_path: &str) -> Option<String> {
    if volume_id == ROOT_VOLUME_ID {
        return Some(abs_path.to_string());
    }
    let mount_root = crate::file_system::get_volume_manager()
        .get(volume_id)
        .map(|v| v.root().to_string_lossy().into_owned());
    index_read_path_pure(volume_id, abs_path, mount_root.as_deref())
}

/// How a volume's LOCAL scan/reconcile/live pipeline maps between the path spaces
/// its code touches, so the same code drives both the `/`-rooted boot disk and a
/// mount-rooted external drive without forking.
///
/// The pipeline handles the SAME path string in two spaces:
/// - **absolute FS path** — `read_dir`, `symlink_metadata`, `Path::exists`, and the
///   `index-dir-updated` emit (which must match pane paths);
/// - **index-relative path** — the argument `store::resolve_path` walks from
///   `ROOT_ID`.
///
/// For the boot disk the two coincide after firmlink normalization, so this is a
/// pass-through. For a `mount_rooted()` volume the index `ROOT_ID` is the mount
/// (`/Volumes/X`), so the mount root is stripped to reach the index-relative path —
/// via the SAME [`smb_watch::index_relative_path`](crate::indexing::smb_watch::index_relative_path)
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
    /// ONCE per scan from the volume's [`FilesystemKind`](crate::file_system::filesystem_kind::FilesystemKind).
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
    /// window is boot-disk-only (see `reconcile/reconciler/rescan_route.rs`).
    pub(crate) fn is_boot_disk(&self) -> bool {
        self.mount_root().is_none()
    }

    /// Override the inode-trust flag (builder form). Used by `for_volume` and by
    /// tests exercising the FAT/exFAT nulling path.
    pub(crate) fn with_inodes_trustworthy(mut self, trustworthy: bool) -> Self {
        self.inodes_trustworthy = trustworthy;
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

    /// Resolve a canonical absolute path (from [`absolute`](Self::absolute)) to its
    /// index entry id, applying the mount-relative strip for a mount-rooted volume.
    ///
    /// `Ok(None)` means "not in this index" — the entry is genuinely absent OR the
    /// path lies outside the mount root — which every caller already treats as
    /// skip/no-op (mirrors the SMB read side dropping an off-volume path rather than
    /// mis-rooting it at `ROOT_ID`). Drop-in for a direct `store::resolve_path` call.
    pub(crate) fn resolve_abs(&self, conn: &Connection, absolute: &str) -> Result<Option<i64>, IndexStoreError> {
        match self.mount_root() {
            None => store::resolve_path(conn, absolute),
            Some(root) => match crate::indexing::smb_watch::index_relative_path(root, absolute) {
                Some(rel) => store::resolve_path(conn, &rel),
                None => Ok(None),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The read-side path transform routes by volume: `root` passes the absolute
    /// path through (its index is rooted at `/`), while a non-root SMB volume
    /// strips its mount root to a mount-relative path (its index `ROOT_ID` is the
    /// mount root). This is the pure decision the live `index_read_path` wraps; it
    /// fixes the SMB read-side gap where a mount-absolute path resolved to nothing.
    #[test]
    fn index_read_path_routes_root_vs_smb() {
        // Root: the absolute path is already index-rooted (no mount_root needed).
        assert_eq!(
            index_read_path_pure(ROOT_VOLUME_ID, "/Users/me/project", None),
            Some("/Users/me/project".to_string()),
            "root passes the absolute path straight through",
        );

        // SMB: strip the mount root to a mount-relative path.
        assert_eq!(
            index_read_path_pure("smb-nas", "/Volumes/share/sub/deep", Some("/Volumes/share")),
            Some("/sub/deep".to_string()),
            "an SMB volume maps the mount-absolute path to mount-relative",
        );
        assert_eq!(
            index_read_path_pure("smb-nas", "/Volumes/share", Some("/Volumes/share")),
            Some("/".to_string()),
            "the share mount root maps to the index ROOT_ID path",
        );

        // SMB with no known mount root (volume not registered) ⇒ can't map ⇒ None.
        assert_eq!(
            index_read_path_pure("smb-nas", "/Volumes/share/sub", None),
            None,
            "without a mount root an SMB path can't be index-rooted",
        );
        // A path outside the mount root ⇒ None (don't mis-root it at ROOT_ID).
        assert_eq!(
            index_read_path_pure("smb-nas", "/Volumes/other/x", Some("/Volumes/share")),
            None,
            "a path outside the mount root must not resolve",
        );
    }

    /// The MTP read-side transform: an `mtp://{device}/{storage}/inner` path maps
    /// to the inner `/inner` rooted at the storage `ROOT_ID`, but only when the
    /// device+storage match the volume id. A path on another MTP volume yields
    /// `None`. This is the MTP analogue of the SMB strip above.
    #[test]
    fn index_read_path_routes_mtp() {
        let vid = "mtp-PIXEL7:65537";

        // Storage root → "/".
        assert_eq!(
            index_read_path_pure(vid, "mtp://mtp-PIXEL7/65537", None),
            Some("/".to_string()),
            "the storage root maps to the index ROOT_ID path",
        );
        // Nested dir → the inner path rooted at "/".
        assert_eq!(
            index_read_path_pure(vid, "mtp://mtp-PIXEL7/65537/DCIM/Camera", None),
            Some("/DCIM/Camera".to_string()),
            "a nested MTP path maps to its storage-relative inner path",
        );
        // Already storage-relative (e.g. from a self-mutation notify) → as-is.
        assert_eq!(
            index_read_path_pure(vid, "/DCIM", None),
            Some("/DCIM".to_string()),
            "a storage-relative path is accepted for this MTP volume",
        );

        // A path on a DIFFERENT storage of the same device ⇒ None.
        assert_eq!(
            index_read_path_pure(vid, "mtp://mtp-PIXEL7/65538/DCIM", None),
            None,
            "a different storage must not resolve onto this volume's index",
        );
        // A path on a DIFFERENT device ⇒ None.
        assert_eq!(
            index_read_path_pure(vid, "mtp://mtp-OTHER/65537/DCIM", None),
            None,
            "a different device must not resolve onto this volume's index",
        );

        // A serial-based volume id with a `:` in the device part still round-trips
        // (the volume id and the path's device segment match verbatim).
        let colon_vid = "mtp-AA:BB:65537";
        assert_eq!(
            index_read_path_pure(colon_vid, "mtp://mtp-AA:BB/65537/Music", None),
            Some("/Music".to_string()),
            "a serial device id containing a colon maps correctly",
        );
    }

    /// The `IndexPathSpace` seam: `root` is a pass-through (absolute == index-relative
    /// after firmlink normalization, `BootDisk` scope), while a mount-rooted space
    /// keeps the raw absolute path for FS/emit but resolves in the mount-relative
    /// space (`MountRooted` scope). The mount strip itself is `smb_watch`'s and is
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

    /// A path under a REGISTERED external mount routes to that mount's volume id
    /// (so its own index owns its dir-stats + status), while a boot-disk path — and
    /// a registered cloud-drive folder that lives INSIDE root's indexed tree — stay
    /// on `root`. The cloud case is the trap: a cloud drive is a registered non-root
    /// volume too, but root's index owns it (it's not under an excluded mount
    /// prefix), so routing it away would drop its recursive sizes.
    #[test]
    fn volume_id_for_local_path_routes_registered_external_mount() {
        use std::sync::Arc;

        use crate::file_system::get_volume_manager;
        use crate::file_system::volume::LocalPosixVolume;

        // A platform-appropriate external mount root (macOS: `/Volumes/X`; Linux:
        // `/media/X`), plus a cloud-drive folder that sits inside the home dir.
        #[cfg(target_os = "macos")]
        let ext_root = "/Volumes/RoutingTestExt";
        #[cfg(not(target_os = "macos"))]
        let ext_root = "/media/RoutingTestExt";
        let cloud_root = "/Users/routingtest/Library/CloudStorage/RoutingTest";

        let manager = get_volume_manager();
        let ext_id = "volumes-routing-test-ext";
        let cloud_id = "cloud-routing-test";
        manager.register(ext_id, Arc::new(LocalPosixVolume::new("Ext", ext_root)));
        manager.register(cloud_id, Arc::new(LocalPosixVolume::new("Cloud", cloud_root)));

        // A path under the external mount → the mount's registered id.
        assert_eq!(
            volume_id_for_local_path(&format!("{ext_root}/sub/deep")),
            ext_id,
            "an external-mount path routes to the mount's own index",
        );
        // The mount root itself → the mount's id.
        assert_eq!(
            volume_id_for_local_path(ext_root),
            ext_id,
            "the mount root routes to its id"
        );
        // A boot-disk path → root.
        assert_eq!(
            volume_id_for_local_path("/Users/routingtest/project"),
            ROOT_VOLUME_ID,
            "a boot-disk path stays on root",
        );
        // A cloud-drive path (registered, but root's index owns it) → root, NOT the
        // cloud volume's id.
        assert_eq!(
            volume_id_for_local_path(&format!("{cloud_root}/x")),
            ROOT_VOLUME_ID,
            "a cloud-drive folder stays on root so its sizes survive",
        );

        manager.unregister(ext_id);
        manager.unregister(cloud_id);
    }

    /// `volume_id_for_local_path`'s pure MTP half: an `mtp://device/storage` path
    /// resolves to the `{device}:{storage}` volume id; non-MTP and malformed
    /// paths don't.
    #[test]
    fn mtp_volume_id_for_path_maps_scheme_paths() {
        assert_eq!(
            mtp_volume_id_for_path("mtp://mtp-PIXEL7/65537/DCIM/Camera"),
            Some("mtp-PIXEL7:65537".to_string()),
        );
        assert_eq!(
            mtp_volume_id_for_path("mtp://mtp-PIXEL7/65537"),
            Some("mtp-PIXEL7:65537".to_string()),
        );
        // A serial device id containing a colon round-trips into the volume id.
        assert_eq!(
            mtp_volume_id_for_path("mtp://mtp-AA:BB/65537/x"),
            Some("mtp-AA:BB:65537".to_string()),
        );
        // Non-MTP and malformed paths don't resolve.
        assert_eq!(mtp_volume_id_for_path("/Users/me"), None);
        assert_eq!(mtp_volume_id_for_path("mtp://mtp-PIXEL7/not-numeric/x"), None);
        assert_eq!(mtp_volume_id_for_path("mtp://"), None);
    }
}

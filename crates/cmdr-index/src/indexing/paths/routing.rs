//! Path → volume routing and the read-path-space mapping.
//!
//! This module owns two related questions:
//!
//! - **Which volume does a path belong to?** `volume_id_for_local_path` maps a
//!   filesystem (or `mtp://` / `adb://` / server) path to the volume id that owns
//!   it: an SMB mount to its `smb_volume_id`, an `mtp://` path to its
//!   `{device}:{storage}` id, an `adb://<serial>` path to that phone's
//!   `adb_volume_id`, an `sftp://` / `webdav://` path to that server account's id,
//!   a registered local external mount (`/Volumes/X`) to its own id, and the boot
//!   disk (plus cloud-drive folders `root`'s index owns) to `root`.
//! - **How does a read path map into that volume's index path space?**
//!   `index_read_path` (and the pure `index_read_path_pure` it wraps) translate a
//!   mount-absolute listing / dir-stats path into the mount-relative path the
//!   volume's index stores it under, so `store::resolve_path` (which walks from
//!   `ROOT_ID`) hits. Pass-through for `root`, mount-relative strip for SMB,
//!   scheme/storage strip for MTP, scheme/serial strip for ADB.
//!
//! These are the read-side mirror of the write-side mount-relative transforms in
//! `transports/smb/watch` / `transports/mtp/watch`. They're kept here, separate from the lifecycle /
//! registry core in `lifecycle/state.rs`, because they're pure path arithmetic the read
//! query surface (`read/queries.rs`) and enrichment (`read/enrichment.rs`) both depend on.
//!
//! Everything here asks by volume id. The scan / reconcile / live pipeline holds
//! its volume's space instead, as an `IndexPathSpace` ([`path_space`](super::path_space)).

use crate::indexing::host;
use crate::indexing::scanner::ExclusionScope;
use crate::indexing::volume::{ROOT_VOLUME_ID, VolumeId};
use cmdr_fs::firmlinks;

/// Resolve a filesystem path to its index volume id.
///
/// Seven routing tiers, tried in order; each maps to the SAME id its volume and
/// index register under, so a read routes to the owning index (or skips cleanly
/// when that volume has no registered index — `get_read_pool_for` → `None` — so an
/// unindexed volume costs zero DB work):
///
/// - **SMB** (`/Volumes/<share>/…` on macOS, an `smbfs`/`cifs` mount on Linux) →
///   `smb_volume_id(server, port, share)`.
/// - **MTP** (`mtp://{device_id}/{storage_id}[/inner…]`) → `{device_id}:{storage_id}`.
/// - **ADB** (`adb://<serial>[/device path…]`) → `adb_volume_id(serial)`.
/// - **Server** (`sftp://` / `webdav://<user>@<host>:<port>[/…]`) → that account's
///   `sftp_volume_id` / `webdav_volume_id`. No drive index serves a server, so the
///   id owns none; routing it anyway is what keeps a server's path off `root`.
/// - **Mount inside the boot tree** (a disk image, an sshfs / rclone / NFS mount,
///   pCloud's `~/pCloud Drive`) → the mount's registered id. See
///   [`boot_tree_mount_volume_id_for_path`].
/// - **Local external mount** (a registered `/Volumes/X` drive on macOS,
///   `/mnt`/`/media` on Linux) → the mount's registered id, so an external drive's
///   dir-stats and `cmdr://state` status come from ITS OWN index, not `root`'s. See
///   [`external_mount_volume_id_for_path`].
/// - **Everything else** (the boot disk, and cloud-drive folders in the home dir
///   that `root`'s index owns) → `root`.
pub(crate) fn volume_id_for_local_path(path: &str) -> VolumeId {
    if let Some(smb_id) = host::volumes::current().smb_volume_id_for_path(path) {
        return smb_id;
    }
    if let Some(mtp_id) = mtp_volume_id_for_path(path) {
        return mtp_id;
    }
    if let Some(adb_id) = adb_volume_id_for_path(path) {
        return adb_id;
    }
    // Pure, like the two device tiers: the account a server path names IS the
    // identity its id is minted from, so a saved server nobody has connected
    // routes the same as a live one.
    if let Some(server) = cmdr_fs::volume::server_of_path(path) {
        return server.volume_id;
    }
    if let Some(mount_id) = boot_tree_mount_volume_id_for_path(path) {
        return mount_id;
    }
    if let Some(mount_id) = external_mount_volume_id_for_path(path) {
        return mount_id;
    }
    ROOT_VOLUME_ID.to_string()
}

/// Resolve a path on a filesystem mounted inside the boot tree (an sshfs or rclone
/// mount in the home folder, pCloud's `~/pCloud Drive`, an NFS share or a disk
/// image mounted anywhere outside `/Volumes`) to that mount's registered id, or
/// `None` for a path `root`'s index owns.
///
/// The boot scan stops at every such mount (`scanner::boot_tree_mounts`), so
/// `root` holds none of its rows and routing here is what makes it read "not
/// indexed" instead of `root`'s `fresh`. Both ask the same cached mount table, so
/// they can't disagree about what's a mount. A registered folder that isn't a mount
/// point (a cloud drive under `~/Library/CloudStorage`) is never in that table and
/// stays on `root` with its sizes.
///
/// The registry is asked with the mount point as the TABLE spells it, which is how
/// the host registered it: a table can list a home-folder mount through the Data
/// volume while a pane names it by its firmlinked path. A mount the registry
/// hasn't adopted yet (the pane that lists it adopts it) stays on `root` until it
/// does.
fn boot_tree_mount_volume_id_for_path(path: &str) -> Option<VolumeId> {
    let normalized = firmlinks::normalize_path(path);
    let mounts = crate::indexing::scanner::boot_tree_mounts::current();
    let mount = mounts.covering(&normalized)?;
    host::volumes::current().mount_id_for_path(&mount.raw)
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
    host::volumes::current().mount_id_for_path(path)
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

/// Map an `adb://<serial>[/…]` path to that phone's volume id, or `None` for any
/// other path.
///
/// Pure, like the MTP tier and for the same reason: the serial the path names IS
/// the identity the id is minted from, so no registry lookup is needed, and none
/// is wanted. A phone's index outlives its volume (unplugging unregisters the
/// volume and keeps the index), and a registry match would send its paths to
/// `root` the moment the cable came out.
fn adb_volume_id_for_path(path: &str) -> Option<VolumeId> {
    cmdr_fs::volume::adb_serial_of_path(path).map(cmdr_fs::volume::adb_volume_id)
}

/// Strip an `adb://<serial>[/…]` path to the device path the phone's index stores
/// it under (`/` for the device root, `/sdcard/DCIM` below it), but only when the
/// serial mints `volume_id`.
///
/// `None` for another phone's path and for a bare `/sdcard/…`: in the app's
/// vocabulary a scheme-free path is the Mac's boot disk, never a phone (unlike
/// MTP, no self-mutation notify hands this index a bare path). The strip itself is
/// the shared mount-root one, against the root `adb_app_root` mints.
fn adb_index_relative_path(volume_id: &str, abs_path: &str) -> Option<String> {
    let serial = cmdr_fs::volume::adb_serial_of_path(abs_path)?;
    if cmdr_fs::volume::adb_volume_id(serial) != volume_id {
        return None;
    }
    crate::indexing::transports::smb::watch::index_relative_path(&cmdr_fs::volume::adb_app_root(serial), abs_path)
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
    let mount_root = host::volumes::current()
        .get(volume_id)
        .map(|v| v.root().to_string_lossy().into_owned())
        .unwrap_or_default();
    ExclusionScope::mount_rooted(mount_root)
}

/// Map a listing/dir-stats path into the path space the volume's index stores it
/// under, so `store::resolve_path` (which walks component-by-component from
/// `ROOT_ID`) hits.
///
/// This is the READ-side mirror of `transports/smb/watch`'s write-side mount-relative
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
///   mount-relative path via the shared [`index_relative_path`](crate::indexing::transports::smb::watch::index_relative_path). A
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
    if cmdr_fs::volume::mtp_ids::is_mtp_volume_id(volume_id) {
        return mtp_index_relative_path(volume_id, normalized_abs);
    }
    // ADB: the index `ROOT_ID` is the device's `/`, under `adb://<serial>`. Pure
    // over the path, so an unplugged phone's index still answers for its paths.
    if cmdr_fs::volume::VolumeScheme::of(volume_id) == cmdr_fs::volume::VolumeScheme::Adb {
        return adb_index_relative_path(volume_id, normalized_abs);
    }
    let mount_root = mount_root?;
    crate::indexing::transports::smb::watch::index_relative_path(mount_root, normalized_abs)
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
    let mount_root = host::volumes::current()
        .get(volume_id)
        .map(|v| v.root().to_string_lossy().into_owned());
    index_read_path_pure(volume_id, abs_path, mount_root.as_deref())
}

/// Where an index-relative path of `volume_id` sits on the local filesystem: the
/// inverse of [`index_read_path`] for a volume the local filesystem reaches (the
/// boot disk, a local drive). The boot disk's index paths already are absolute; a
/// drive's get the mount root the host serves it at RIGHT NOW joined on, so a
/// renamed drive answers at its new name.
///
/// `None` when the host has no root for the volume (it isn't mounted). macOS only, like its
/// one caller (`importance::last_used`'s Spotlight sampling).
#[cfg(target_os = "macos")]
pub(crate) fn local_path_of(volume_id: &str, index_relative: &str) -> Option<String> {
    if volume_id == ROOT_VOLUME_ID {
        return Some(index_relative.to_string());
    }
    let mount_root = host::volumes::current()
        .get(volume_id)?
        .root()
        .to_string_lossy()
        .into_owned();
    Some(crate::indexing::paths::path_space::join_volume_relative(
        &mount_root,
        index_relative,
    ))
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

    /// The ADB read-side transform: an `adb://<serial>/…` path maps to the device
    /// path the phone's index stores (its `ROOT_ID` is the device's `/`), but only
    /// when the serial mints THIS volume id. Pure, so it holds with no volume
    /// registered: an unplugged phone keeps its index.
    #[test]
    fn index_read_path_routes_adb() {
        use cmdr_fs::volume::{adb_app_root, adb_volume_id};

        let vid = adb_volume_id("R58M1");
        assert_eq!(
            index_read_path_pure(&vid, &adb_app_root("R58M1"), None),
            Some("/".to_string()),
            "the phone's root maps to the index ROOT_ID path",
        );
        assert_eq!(
            index_read_path_pure(&vid, "adb://R58M1/sdcard/DCIM", None),
            Some("/sdcard/DCIM".to_string()),
            "a nested phone path maps to its device path",
        );
        assert_eq!(
            index_read_path_pure(&vid, "adb://OTHER/sdcard/DCIM", None),
            None,
            "another phone's path must not resolve onto this index",
        );
        assert_eq!(
            index_read_path_pure(&vid, "adb://R58M10/sdcard", Some("adb://R58M1")),
            None,
            "a serial that merely starts with this one is another phone",
        );
        assert_eq!(
            index_read_path_pure(&vid, "/sdcard/DCIM", None),
            None,
            "a bare device path is the Mac's boot disk in app vocabulary, never this phone",
        );
    }

    /// `volume_id_for_local_path`'s ADB tier: an `adb://<serial>` path belongs to
    /// that phone's index, whether or not the phone is plugged in right now. Before
    /// this tier every phone path fell through to `root`, the Mac's boot disk.
    #[test]
    fn volume_id_for_local_path_routes_a_phone_path_to_its_own_index() {
        use cmdr_fs::volume::{adb_app_root, adb_volume_id};

        let _serialized = crate::indexing::handle::test_lock();
        let phone = adb_volume_id("R58M1");
        assert_eq!(volume_id_for_local_path(&adb_app_root("R58M1")), phone);
        assert_eq!(volume_id_for_local_path("adb://R58M1/sdcard/DCIM"), phone);
        assert_eq!(
            volume_id_for_local_path("adb://192.168.1.5:5555/sdcard"),
            adb_volume_id("192.168.1.5:5555"),
            "a wireless phone's serial carries its port",
        );
        assert_ne!(volume_id_for_local_path("adb://OTHER/sdcard"), phone);
        assert_eq!(
            volume_id_for_local_path("adb://"),
            ROOT_VOLUME_ID,
            "a path naming no phone is nobody's"
        );
    }

    /// `volume_id_for_local_path`'s server tier: an `sftp://` or `webdav://` path
    /// belongs to that server account's own id, registered or not. Nothing indexes
    /// a server, so the id owns no index and a read skips cleanly; what matters is
    /// that the path never falls through to `root`, where a search from a server
    /// pane silently searched the Mac's boot disk.
    #[test]
    fn volume_id_for_local_path_routes_a_server_path_to_its_own_id() {
        use cmdr_fs::volume::{sftp_app_root, sftp_volume_id, webdav_app_root, webdav_volume_id};

        let _serialized = crate::indexing::handle::test_lock();
        let sftp = sftp_volume_id("nas.local", 22, "ada");
        assert_eq!(volume_id_for_local_path(&sftp_app_root("nas.local", 22, "ada")), sftp);
        assert_eq!(
            volume_id_for_local_path(&format!("{}/srv/data/photos", sftp_app_root("nas.local", 22, "ada"))),
            sftp
        );
        assert_eq!(
            volume_id_for_local_path(&format!("{}/srv", sftp_app_root("NAS.local", 22, "ada"))),
            sftp,
            "the host folds the way the id folds it"
        );
        assert_ne!(
            volume_id_for_local_path(&format!("{}/srv", sftp_app_root("nas.local", 22, "Ada"))),
            sftp,
            "the account is case-sensitive, so another user is another volume"
        );
        assert_ne!(
            volume_id_for_local_path(&format!("{}/srv", sftp_app_root("nas.local", 2222, "ada"))),
            sftp,
            "another port is another server"
        );
        assert_eq!(
            volume_id_for_local_path(&format!("{}/dav/x", webdav_app_root("cloud.example", 443, "ada"))),
            webdav_volume_id("cloud.example", 443, "ada"),
        );
        for nobodys in [
            "sftp://",
            "sftp://nas.local:22/srv",
            "sftp://ada@nas.local/srv",
            "webdav://ada@:443",
        ] {
            assert_eq!(
                volume_id_for_local_path(nobodys),
                ROOT_VOLUME_ID,
                "a malformed server path ({nobodys}) names no account"
            );
        }
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

        use crate::indexing::host::volumes::{self, FakeVolumeProvider};
        use cmdr_fs::volume::InMemoryVolume;

        // A platform-appropriate external mount root (macOS: `/Volumes/X`; Linux:
        // `/media/X`), plus a cloud-drive folder that sits inside the home dir.
        #[cfg(target_os = "macos")]
        let ext_root = "/Volumes/RoutingTestExt";
        #[cfg(not(target_os = "macos"))]
        let ext_root = "/media/RoutingTestExt";
        let cloud_root = "/Users/routingtest/Library/CloudStorage/RoutingTest";

        let ext_id = "volumes-routing-test-ext";
        let cloud_id = "cloud-routing-test";
        let provider = FakeVolumeProvider::shared();
        provider
            .register(ext_id, Arc::new(InMemoryVolume::new("Ext").with_root(ext_root)))
            .register(cloud_id, Arc::new(InMemoryVolume::new("Cloud").with_root(cloud_root)));

        let _serialized = crate::indexing::handle::test_lock();
        let _installed = volumes::install_for_test(provider);

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

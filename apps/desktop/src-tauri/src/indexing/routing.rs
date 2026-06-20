//! Path → volume routing and the read-path-space mapping.
//!
//! This module owns two related questions:
//!
//! - **Which volume does a path belong to?** `volume_id_for_local_path` maps a
//!   filesystem (or `mtp://`) path to the index volume id that owns it — an SMB
//!   mount to its `smb_volume_id`, an `mtp://` path to its `{device}:{storage}`
//!   id, everything else to `root`.
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
//! query surface (`queries.rs`) and enrichment (`enrichment.rs`) both depend on.

use super::state::{ROOT_VOLUME_ID, VolumeId};

/// Resolve a filesystem path to its index volume id.
///
/// An SMB-mounted path (`/Volumes/<share>/…` on macOS, an `smbfs`/`cifs` mount
/// on Linux) maps to its `smb_volume_id(server, port, share)` — the SAME id the
/// `VolumeManager` and the SMB index register under — so a listing under that
/// share routes to the SMB volume's index, not `root`. Everything else (local
/// absolute paths) is `root`. The routed read paths still skip cleanly when the
/// resolved volume has no registered index (`get_read_pool_for` → `None`), so an
/// SMB share that isn't indexed costs zero DB work, exactly like before.
///
/// An `mtp://{device_id}/{storage_id}[/inner…]` virtual path maps to its MTP
/// volume id `{device_id}:{storage_id}` (the SAME id the `MtpConnectionManager`
/// registers the volume and its index under), so dir-stats / status reads on an
/// MTP path route to that device-storage's index. Everything else is `root`.
pub(super) fn volume_id_for_local_path(path: &str) -> VolumeId {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(smb_id) = super::smb_index::smb_volume_id_for_path(path) {
        return smb_id;
    }
    if let Some(mtp_id) = mtp_volume_id_for_path(path) {
        return mtp_id;
    }
    ROOT_VOLUME_ID.to_string()
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
    super::smb_watch::index_relative_path(mount_root, normalized_abs)
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

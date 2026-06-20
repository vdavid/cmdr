//! Resolve an opaque MTP/PTP object handle to its full virtual path.
//!
//! PTP change events ([`DeviceEvent::ObjectAdded`](mtp_rs::mtp::DeviceEvent) etc.) carry only an
//! `ObjectHandle` (a `u32`), never a path — this is a wire-format property of
//! the protocol (every event is `code + 3×u32`), not a library or Cmdr gap. To
//! turn a handle into a path we ask the device for the object's
//! [`ObjectInfo`](mtp_rs::ptp::ObjectInfo) (`{ parent, filename, .. }`) and walk
//! the `parent` chain up to the storage root, prepending each filename.
//!
//! Two things keep that cheap and robust:
//!
//! - **Reverse-cache short-circuit.** [`PathHandleCache`](super::cache::PathHandleCache)
//!   already maps `path → handle` for every directory the user has browsed; we
//!   also keep the reverse (`handle → path`), populated at the same sites. The
//!   walk stops the instant it hits a cached ancestor, so resolving a newly
//!   added file under an open folder is usually one `GetObjectInfo` round trip
//!   (the file itself), not a full walk to root.
//! - **A depth cap.** A device that returns a self-referential or cyclic parent
//!   chain (malformed firmware) can't wedge the walk: [`MAX_WALK_DEPTH`] bounds
//!   it and the resolve fails cleanly, letting the caller fall back to a blanket
//!   refresh.
//!
//! The pure walk ([`walk_handle_to_path`]) is generic over a `lookup` closure so
//! it unit-tests against an in-memory handle graph with no device. The async
//! [`MtpConnectionManager::resolve_handle_to_path`] wires that walk to real USB
//! `GetObjectInfo` calls under the device lock, populating the reverse cache as
//! it goes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use mtp_rs::{ObjectHandle, StorageId};

use super::errors::MtpConnectionError;
use super::{MTP_TIMEOUT_SECS, MtpConnectionManager, acquire_device_lock, map_mtp_error};

/// Upper bound on how many parent hops the walk will follow before giving up.
/// Real MTP trees are shallow (a handful of levels); this only exists to bound a
/// cyclic/self-referential parent chain from a malformed device.
const MAX_WALK_DEPTH: usize = 256;

/// A resolved MTP object for the index watch path: its storage-relative path plus
/// the metadata an index upsert needs. Built by
/// [`MtpConnectionManager::resolve_object_for_index`] from one extra
/// `GetObjectInfo` on top of the handle→path walk. The handle itself is known by
/// the caller (it's the event's handle), so it isn't repeated here.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedMtpObject {
    /// Full storage-relative path (leading `/`).
    pub path: PathBuf,
    pub is_directory: bool,
    /// Logical size in bytes (`None` for directories).
    pub size: Option<u64>,
    /// Modified time as a Unix timestamp, if the device reports one.
    pub modified_at: Option<u64>,
}

/// `true` when `handle` is a root sentinel: the walk stops here.
///
/// `ObjectHandle::ROOT` (`0`) is the spec value, but the Android root quirk means
/// a root-level object can report its parent as `ObjectHandle::ALL`
/// (`0xFFFFFFFF`) instead (mirrors the `AndroidRoot` filter in mtp-rs's
/// `list_objects`). Treat both as root so a top-level object resolves to
/// `/<name>` rather than failing the walk.
fn is_root_handle(handle: ObjectHandle) -> bool {
    handle == ObjectHandle::ROOT || handle == ObjectHandle::ALL
}

/// Walk a handle's parent chain into a full virtual path (leading `/`).
///
/// `reverse_cache` is the `handle → path` map for the storage; a hit on `start`
/// or any ancestor ends the walk early. `lookup(handle)` returns
/// `(parent_handle, filename)` for an uncached handle (a USB `GetObjectInfo` in
/// production, an in-memory map in tests).
///
/// Returns:
/// - `Some(path)` on success (including a root-level object → `/<name>`),
/// - `None` if a `lookup` returns `None` (handle gone/invalid), or the chain
///   exceeds [`MAX_WALK_DEPTH`] (cycle guard).
///
/// `start` being a root handle itself resolves to `/` (the storage root).
fn walk_handle_to_path(
    start: ObjectHandle,
    reverse_cache: &HashMap<ObjectHandle, PathBuf>,
    mut lookup: impl FnMut(ObjectHandle) -> Option<(ObjectHandle, String)>,
) -> Option<PathBuf> {
    if is_root_handle(start) {
        return Some(PathBuf::from("/"));
    }

    // Filenames from `start` up toward root, in child→ancestor order; reversed
    // into a path at the end.
    let mut names_leaf_first: Vec<String> = Vec::new();
    let mut current = start;

    for _ in 0..MAX_WALK_DEPTH {
        // Cached ancestor: splice the known prefix in front of what we've
        // collected and we're done — no more USB round trips.
        if let Some(cached) = reverse_cache.get(&current) {
            let mut path = cached.clone();
            for name in names_leaf_first.iter().rev() {
                path.push(name);
            }
            return Some(path);
        }

        let (parent, filename) = lookup(current)?;
        names_leaf_first.push(filename);

        if is_root_handle(parent) {
            // Reached the storage root: build `/<a>/<b>/…` from the names.
            let mut path = PathBuf::from("/");
            for name in names_leaf_first.iter().rev() {
                path.push(name);
            }
            return Some(path);
        }

        current = parent;
    }

    // Depth cap hit without reaching root or a cached ancestor: treat as a
    // malformed/cyclic chain and let the caller fall back.
    None
}

impl MtpConnectionManager {
    /// Resolve an `ObjectHandle` on `(device_id, storage_id)` to its full virtual
    /// path (leading `/`, e.g. `/DCIM/Camera/IMG_0001.jpg`).
    ///
    /// # Contract
    ///
    /// - Walks the object's `parent` chain to the storage root via
    ///   `GetObjectInfo`, short-circuiting on any ancestor already in the
    ///   reverse handle cache; newly seen `(handle, path)` pairs are added to the
    ///   cache as a side effect, so repeat resolves under the same folder get
    ///   cheaper.
    /// - A root-level object resolves to `/<name>`; the root handle itself
    ///   resolves to `/`.
    /// - Cancelable and bounded: every USB round trip is `MTP_TIMEOUT_SECS`-
    ///   capped via the same device-lock discipline as the rest of this module,
    ///   and the walk is depth-bounded against a cyclic parent chain.
    ///
    /// # Errors
    ///
    /// - [`MtpConnectionError::NotConnected`] if the device isn't in the registry.
    /// - [`MtpConnectionError::ObjectNotFound`] if the handle (or an ancestor) is
    ///   invalid/gone — expected for `ObjectRemoved`, whose object is already
    ///   deleted, so callers fall back to a blanket refresh there.
    /// - [`MtpConnectionError::Timeout`] / mapped protocol errors on USB failure.
    ///
    /// Intended for the live-pane targeted refresh ([`super::event_loop`]) and,
    /// later, the MTP index writer; the index will additionally store a handle
    /// per entry so it can resolve removals (which this can't, the object being
    /// gone).
    pub async fn resolve_handle_to_path(
        &self,
        device_id: &str,
        storage_id: u32,
        handle: ObjectHandle,
    ) -> Result<PathBuf, MtpConnectionError> {
        // Foreground priority: this resolve drives the live update of the
        // CURRENTLY-VISIBLE pane (the targeted-refresh path) and the not-scanning
        // index feed. Either way it's a small bounded walk that should preempt the
        // background scan so the open folder updates in ~1-2 s. (During a scan the
        // INDEX feed buffers the raw handle instead and never reaches here; this
        // guard then only affects the visible-pane refresh and the post-scan
        // replay, where nothing contends.)
        let _fg = self.foreground_guard(device_id).await;

        // Snapshot the reverse cache for this storage once, under the registry
        // lock, then drop the lock before any USB round trip.
        let (device_arc, reverse_cache) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;
            let reverse_cache = entry
                .path_cache
                .read()
                .ok()
                .and_then(|cache_map| cache_map.get(&storage_id).map(|sc| sc.handle_to_path.clone()))
                .unwrap_or_default();
            (std::sync::Arc::clone(&entry.device), reverse_cache)
        };

        // Phase 1 (async): pre-fetch `(parent, filename)` for the handles on the
        // chain into `memo`, stopping at a cached ancestor, a root sentinel, or
        // the depth cap. This is the only place that touches USB. Phase 2 (the
        // pure [`walk_handle_to_path`] below) then assembles the path from `memo`
        // + the cache — it owns the canonical stop/assembly logic, so phase 1
        // only needs to over-approximate "what might be needed", never to compute
        // the path. The device/storage open lazily on first miss, so a
        // fully-cached resolve issues zero USB calls.
        let memo = self
            .prefetch_handle_chain(device_id, storage_id, handle, &device_arc, &reverse_cache)
            .await?;

        let Some(path) = walk_handle_to_path(handle, &reverse_cache, |h| memo.get(&h).cloned()) else {
            // The chain couldn't be fully resolved (handle gone, or a cyclic
            // chain hit the depth cap). The caller falls back to a blanket refresh.
            return Err(MtpConnectionError::ObjectNotFound {
                device_id: device_id.to_string(),
                path: format!("handle {}", handle.0),
            });
        };

        // The leaf's full path is now known; record `(handle → path)` so a
        // follow-up resolve under the same folder short-circuits on it. We cache
        // only the leaf: re-deriving each ancestor's own full path to cache it
        // correctly isn't worth it, and the next browse of the folder repopulates
        // those anyway via `finalize_listing`. The root handle resolves to `/`
        // with no walk, so there's nothing handle-keyed to cache for it.
        if !is_root_handle(handle) {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                cache_map.entry(storage_id).or_default().insert(path.clone(), handle);
            }
        }

        Ok(path)
    }

    /// Resolve a PTP `ObjectAdded` / `ObjectInfoChanged` handle into the data an
    /// index upsert needs: its storage-relative path plus size / is-directory /
    /// modified time. Used by the MTP watch→index path (`indexing::mtp_watch`).
    ///
    /// Two USB-touching steps under the device lock: the handle→path walk
    /// ([`resolve_handle_to_path`](Self::resolve_handle_to_path), usually one
    /// round trip thanks to the reverse cache) plus one `GetObjectInfo` on the
    /// object itself for its metadata. Cancelable/bounded via the same
    /// `MTP_TIMEOUT_SECS` discipline.
    ///
    /// # Errors
    ///
    /// [`MtpConnectionError::ObjectNotFound`] if the handle is gone/invalid
    /// (expected for a removed object — but removals never call this; they resolve
    /// via the index's stored handle instead), plus the usual timeout / protocol
    /// errors. On any error the caller simply skips the index update for this
    /// event (the next scan reconciles).
    pub(crate) async fn resolve_object_for_index(
        &self,
        device_id: &str,
        storage_id: u32,
        handle: ObjectHandle,
    ) -> Result<ResolvedMtpObject, MtpConnectionError> {
        let path = self.resolve_handle_to_path(device_id, storage_id, handle).await?;

        // Fetch the object's own metadata for the upsert.
        let device_arc = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;
            std::sync::Arc::clone(&entry.device)
        };
        let device = acquire_device_lock(&device_arc, device_id, "resolve_object_for_index").await?;
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;
        let info = tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.get_object_info(handle))
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

        let is_directory = info.format == mtp_rs::ptp::ObjectFormatCode::Association;
        Ok(ResolvedMtpObject {
            path,
            is_directory,
            size: if is_directory { None } else { Some(info.size) },
            modified_at: info.modified.map(super::convert_mtp_datetime),
        })
    }

    /// Phase 1 of [`resolve_handle_to_path`](Self::resolve_handle_to_path):
    /// fetch `(parent, filename)` for each handle along `handle`'s parent chain
    /// into a memo map, stopping at a cached ancestor, a root sentinel, or
    /// [`MAX_WALK_DEPTH`]. The only USB-touching half; the device/storage open
    /// lazily on the first miss, so a fully-cached chain issues no USB calls. The
    /// device lock is held across the (few, shallow) round trips so an event
    /// burst doesn't interleave them and thrash the session.
    async fn prefetch_handle_chain(
        &self,
        device_id: &str,
        storage_id: u32,
        handle: ObjectHandle,
        device_arc: &std::sync::Arc<tokio::sync::Mutex<mtp_rs::MtpDevice>>,
        reverse_cache: &HashMap<ObjectHandle, PathBuf>,
    ) -> Result<HashMap<ObjectHandle, (ObjectHandle, String)>, MtpConnectionError> {
        let mut memo: HashMap<ObjectHandle, (ObjectHandle, String)> = HashMap::new();

        if is_root_handle(handle) || reverse_cache.contains_key(&handle) {
            // Answerable from the cache alone (or trivially root): no USB.
            return Ok(memo);
        }

        let device = acquire_device_lock(device_arc, device_id, "resolve_handle_to_path").await?;
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let mut current = handle;
        for _ in 0..MAX_WALK_DEPTH {
            if reverse_cache.contains_key(&current) {
                break;
            }
            let info = tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.get_object_info(current))
                .await
                .map_err(|_| MtpConnectionError::Timeout {
                    device_id: device_id.to_string(),
                })?
                .map_err(|e| map_mtp_error(e, device_id))?;
            let parent = info.parent;
            memo.insert(current, (parent, info.filename));
            if is_root_handle(parent) {
                break;
            }
            current = parent;
        }

        Ok(memo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `lookup` closure from a `handle -> (parent, filename)` graph, and
    /// a counter so tests can assert how many round trips the walk made.
    fn graph_lookup<'a>(
        graph: &'a HashMap<u32, (u32, &'static str)>,
        calls: &'a std::cell::RefCell<Vec<u32>>,
    ) -> impl FnMut(ObjectHandle) -> Option<(ObjectHandle, String)> + 'a {
        move |h: ObjectHandle| {
            calls.borrow_mut().push(h.0);
            graph
                .get(&h.0)
                .map(|(parent, name)| (ObjectHandle(*parent), (*name).to_string()))
        }
    }

    #[test]
    fn root_handle_resolves_to_slash() {
        let cache = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let graph = HashMap::new();
        let path = walk_handle_to_path(ObjectHandle::ROOT, &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, Some(PathBuf::from("/")));
        // No lookups for a root handle.
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn android_root_sentinel_also_resolves_to_slash() {
        let cache = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let graph = HashMap::new();
        let path = walk_handle_to_path(ObjectHandle::ALL, &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, Some(PathBuf::from("/")));
    }

    #[test]
    fn root_level_object_resolves_to_slash_name() {
        // handle 10 = /DCIM, parent is ROOT.
        let mut graph = HashMap::new();
        graph.insert(10u32, (0u32, "DCIM"));
        let cache = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let path = walk_handle_to_path(ObjectHandle(10), &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, Some(PathBuf::from("/DCIM")));
        assert_eq!(*calls.borrow(), vec![10]);
    }

    #[test]
    fn root_level_object_with_android_parent_sentinel() {
        // Some Android devices report a root child's parent as 0xFFFFFFFF.
        let mut graph = HashMap::new();
        graph.insert(10u32, (0xFFFF_FFFFu32, "Download"));
        let cache = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let path = walk_handle_to_path(ObjectHandle(10), &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, Some(PathBuf::from("/Download")));
    }

    #[test]
    fn full_walk_to_root_when_cache_empty() {
        // /DCIM/Camera/IMG.jpg : 30 -> 20 -> 10 -> ROOT
        let mut graph = HashMap::new();
        graph.insert(30u32, (20u32, "IMG.jpg"));
        graph.insert(20u32, (10u32, "Camera"));
        graph.insert(10u32, (0u32, "DCIM"));
        let cache = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let path = walk_handle_to_path(ObjectHandle(30), &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, Some(PathBuf::from("/DCIM/Camera/IMG.jpg")));
        // Walked all three levels.
        assert_eq!(*calls.borrow(), vec![30, 20, 10]);
    }

    #[test]
    fn cached_ancestor_short_circuits_the_walk() {
        // Same tree, but /DCIM/Camera (handle 20) is already cached. Resolving
        // the new file (30) should look up only 30, then splice the cached
        // prefix — no lookups for 20 or 10.
        let mut graph = HashMap::new();
        graph.insert(30u32, (20u32, "IMG.jpg"));
        graph.insert(20u32, (10u32, "Camera"));
        graph.insert(10u32, (0u32, "DCIM"));
        let mut cache = HashMap::new();
        cache.insert(ObjectHandle(20), PathBuf::from("/DCIM/Camera"));
        let calls = std::cell::RefCell::new(Vec::new());
        let path = walk_handle_to_path(ObjectHandle(30), &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, Some(PathBuf::from("/DCIM/Camera/IMG.jpg")));
        assert_eq!(*calls.borrow(), vec![30], "only the leaf should be looked up");
    }

    #[test]
    fn start_handle_already_cached_needs_no_lookup() {
        let mut cache = HashMap::new();
        cache.insert(ObjectHandle(42), PathBuf::from("/Music/song.mp3"));
        let graph = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let path = walk_handle_to_path(ObjectHandle(42), &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, Some(PathBuf::from("/Music/song.mp3")));
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn invalid_handle_returns_none() {
        // Handle not in the graph (gone/invalid) and not cached: lookup yields
        // None, so the walk fails — the caller falls back to a blanket refresh.
        let graph = HashMap::new();
        let cache = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let path = walk_handle_to_path(ObjectHandle(99), &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, None);
    }

    #[test]
    fn cyclic_parent_chain_is_bounded_not_hung() {
        // Malformed device: 1's parent is 2, 2's parent is 1. The depth cap must
        // stop this and fail cleanly rather than loop forever.
        let mut graph = HashMap::new();
        graph.insert(1u32, (2u32, "a"));
        graph.insert(2u32, (1u32, "b"));
        let cache = HashMap::new();
        let calls = std::cell::RefCell::new(Vec::new());
        let path = walk_handle_to_path(ObjectHandle(1), &cache, graph_lookup(&graph, &calls));
        assert_eq!(path, None);
        assert_eq!(calls.borrow().len(), MAX_WALK_DEPTH);
    }
}

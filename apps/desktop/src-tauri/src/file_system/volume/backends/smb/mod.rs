//! SMB volume implementation using direct smb2 protocol operations.
//!
//! Wraps an smb2 session to provide file system access through the Volume trait.
//! The share remains OS-mounted (for Finder/Terminal/drag-drop compatibility),
//! but all Cmdr file operations go through smb2's pipelined I/O for better
//! performance and fail-fast behavior.

#![allow(
    unused_imports,
    reason = "mod.rs holds the backend shared prelude and re-exports submodule internals so the sibling #[cfg(test)] modules resolve them through `super::*`"
)]
// Scaffolding only the sibling `#[cfg(test)] mod smb_*` modules call (`volume_id`,
// `PoolSlots::any_alive`, `with_smb_sync`), which `deny(unused)` flags against a
// non-test build. Scoped to this backend on purpose: every other backend under
// `backends/` compiles clean without it, so a new dead item here is a real finding
// rather than something the umbrella swallows.
#![allow(dead_code, reason = "reached only from the sibling #[cfg(test)] SMB modules")]

use super::{
    BatchScanResult, CopyScanResult, LaneKey, MutationEvent, ScanConflict, SmbConnectionState, SourceItemInfo,
    SpaceInfo, Volume, VolumeError, VolumeReadStream, WatchCoverage,
};
use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::try_get_authoritative_listing;
use crate::ignore_poison::RwLockIgnorePoison;
use log::{debug, info, trace, warn};
use smb2::client::tree::Tree;
use smb2::{ClientConfig, SmbClient};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock, RwLock as StdRwLock};
use std::time::Duration;
use tauri::AppHandle;

mod events;
mod foreground_yield;
mod mapping;
mod reconnect;
mod scan;
mod scan_pool;
mod session;
mod state;
mod streams;
mod volume_impl;

// Internal re-exports: pull submodule items into the `smb` root so the sibling
// `#[cfg(test)]` modules (declared below) reach them through `use super::*`,
// and so cross-module references resolve unqualified.
use events::emit_state_change;
use mapping::{directory_entry_to_file_entry, filetime_to_unix_secs, fs_info_to_space_info, map_smb_error};
use session::{CLIENT_LOCK_TICKET, build_session, refresh_credentials_from_store, update_state_on_smb_error};
use state::ConnectionState;
use streams::{
    ASSUMED_MAX_WRITE, InlineReadStream, SMB_STREAM_CHANNEL_CAPACITY, SmbReadStream, fits_one_compound_write,
};

// External surface: keep these paths stable at
// `crate::file_system::volume::backends::smb::<name>`.
pub use events::set_app_handle;
pub(crate) use reconnect::spawn_watcher_death_reconnect;

/// A volume backed by an SMB share, using smb2 for direct protocol access.
///
/// The share is also OS-mounted (at `mount_path`) for Finder/Terminal/drag-drop
/// compatibility, but Cmdr's own file operations go through the smb2 session
/// for better performance and fail-fast behavior.
///
/// # Thread safety & concurrency
///
/// The smb2 `SmbClient` is protected by a `tokio::sync::Mutex` because every
/// `SmbClient` method takes `&mut self`. The `Tree` lives in a separate
/// `tokio::sync::RwLock<Option<Arc<Tree>>>` so the hot read/write paths can
/// hold an `Arc<Tree>` without touching the client mutex. Concurrent copies
/// on a single volume briefly lock the client to clone its `Connection` (a
/// cheap `Arc::clone`), release the lock, and drive `Tree::download` /
/// `Tree::read_file_compound` / `Tree::write_file_compound` on the cloned
/// `Connection`, so N downloads run pipelined on one SMB session instead of
/// serializing through the mutex. The `watcher_cancel` field uses
/// `std::sync::Mutex` because it is only accessed briefly (no awaits while
/// held).
/// Connection parameters needed to (re-)establish the smb2 session.
///
/// Cached on the volume so `attempt_reconnect()` can rebuild the session in
/// place after a `ConnectionLost` / `SessionExpired` without going through the
/// mount flow again. Credentials are kept in memory for the lifetime of the
/// `SmbVolume` (no security concern: they're already in the process's
/// address space, used on every smb2 call). On auth failure we
/// re-pull from the secret store in case the user updated them.
#[derive(Debug, Clone)]
pub(crate) struct SmbConnectionParams {
    /// Resolved server address (IP or hostname, ready to pass to `build_smb_addr`).
    pub server: String,
    pub share_name: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

/// Hands out a fresh `SmbVolume::instance_id` per constructed volume.
static NEXT_INSTANCE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A `Volume` instance addressing one mount root of a share, over the shared
/// [`SmbVolumeInner`] every instance of that share rides.
///
/// The split is what makes [`Volume::rerooted`] free: a share reached through two
/// mount points is ONE session, and moving the registry's ID from a dead mount to
/// a live one only has to hand out another instance over the same
/// `Arc<SmbVolumeInner>`. Nothing re-authenticates, no transport is rebuilt, and
/// whoever still holds the old instance (a running copy, an open viewer stream)
/// keeps working at the root it was handed. See `../DETAILS.md` § "Re-rooting a
/// share".
pub struct SmbVolume {
    /// Display name (share name).
    name: String,
    /// The OS mount point THIS instance addresses (for example,
    /// "/Volumes/Documents"). Immutable: a different root means a different
    /// instance, so `root()` stays a plain `&Path` borrow.
    mount_path: PathBuf,
    /// Set once the registry has proven this instance's `mount_path` is gone and
    /// had no live sibling mount to move the ID to. The smb2 session is unaffected
    /// (Cmdr's own I/O never touches the mount), but a `file://` URL under a dead
    /// mount opens nowhere, so `paths_are_os_visible` has to stop claiming it can.
    /// One-way: a mount that comes back re-registers the volume from scratch.
    mount_root_gone: AtomicBool,
    /// The live transport and everything scoped to the SHARE rather than to one
    /// mount root, shared with every other instance of this share.
    inner: Arc<SmbVolumeInner>,
}

/// The share-scoped half of an [`SmbVolume`]: the smb2 session and the state that
/// must stay single across every mount root the share is reachable through.
pub(super) struct SmbVolumeInner {
    /// SMB share name. Mirrors `params.share_name`, kept here for cheap reads
    /// in log lines and hot paths without locking `params`.
    share_name: String,
    /// Volume ID for listing cache lookups (from `smb_volume_id(server, port, share)`).
    volume_id: String,
    /// Connection parameters (host, port, share, credentials) used to build the
    /// current session and to rebuild it on `attempt_reconnect`. `RwLock` because
    /// `attempt_reconnect` may update the credentials in place after a fresh
    /// secret-store lookup; reads (the watcher-restart path) are otherwise rare.
    params: Arc<tokio::sync::RwLock<SmbConnectionParams>>,
    /// smb2 client (owns the Connection). `None` when disconnected.
    ///
    /// Most methods still lock this mutex and call `client.stat(tree, ...)`
    /// etc. SmbClient's async methods need `&mut self` and these aren't
    /// hot-path parallel. The hot copy path (compound read/write, download
    /// stream) briefly locks just to clone the `Connection` (via
    /// `client.connection_mut().clone()`), releases the lock, and drives the
    /// op on the clone. This is what gives concurrency across files while the
    /// underlying SMB session multiplexes the frames.
    client: Arc<tokio::sync::Mutex<Option<SmbClient>>>,
    /// Tree (share connection), wrapped as `Arc<Tree>` so concurrent hot-path
    /// ops can hold a reference without serializing on the client mutex.
    /// `None` when disconnected. The `RwLock` is essentially uncontended (we
    /// only write on disconnect), so readers just clone the `Arc` out under a
    /// read guard and drop the guard immediately.
    tree: Arc<tokio::sync::RwLock<Option<Arc<Tree>>>>,
    /// Current connection health.
    /// Wrapped in `Arc` so background tasks (streaming read producer) can update
    /// the state on mid-stream connection loss.
    state: Arc<AtomicU8>,
    /// Cancel sender for the background watcher task. Send to stop watching.
    watcher_cancel: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Single-flight guard for `attempt_reconnect`. Concurrent callers (FE
    /// backoff cycle + lazy nav-time retry) wait on the same in-flight attempt
    /// instead of dog-piling the server.
    reconnect_lock: Arc<tokio::sync::Mutex<()>>,
    /// Set by `on_unmount` so that any in-flight `do_attempt_reconnect` can bail
    /// out without installing a fresh session into an orphaned volume.
    /// Once `true`, the volume is permanently dead; `attempt_reconnect` becomes
    /// a no-op error.
    unmounted: Arc<AtomicBool>,
    /// Set by `on_superseded` when a newer instance takes over this volume's id
    /// in the `VolumeManager`. The session stays up for whoever still holds this
    /// instance, but everything scoped to the ID (the watcher, the scan pool,
    /// `volume-connection-changed` events, the index-resume hook) belongs to the
    /// successor now and must go quiet here. See `DETAILS.md` § "Supersede vs.
    /// unmount".
    superseded: Arc<AtomicBool>,
    /// The per-scan connection pool: extra smb2 sessions that background bulk work
    /// (the index scan's listings, media enrichment's prefetch reads) spreads
    /// across, opened lazily on `begin_scan_session` and torn down when the LAST
    /// concurrent session ends. `None` between scans (steady-state footprint is
    /// just the one browsing session). See `scan_pool.rs`.
    scan_pool: tokio::sync::RwLock<Option<Arc<scan_pool::ScanPool>>>,
    /// Distinguishes this instance from any successor that later takes the same
    /// `volume_id`. The watcher carries it so its death path can tell "MY volume
    /// lost its connection" from "a volume that replaced me is fine" — without
    /// it, a retired watcher dying just after the swap marks the healthy
    /// successor disconnected. See `reconnect::spawn_watcher_death_reconnect`.
    instance_id: u64,
    /// How many scan sessions are open right now. Two background users can overlap
    /// (an index rescan kicked while an enrichment pass runs); the pool must
    /// survive until the LAST one ends, or one user's `end_scan_session` tears the
    /// pool out from under the other mid-flight. Saturating at 0 so an unmatched
    /// end (unmount teardown raced a pass) can't underflow.
    scan_session_refs: AtomicUsize,
    /// Where the share is mounted RIGHT NOW, as the registry last decided. The
    /// share-scoped watcher reads it per event batch to build the absolute paths
    /// its listing-cache notifications key on, so a promotion re-points it instead
    /// of leaving the watcher feeding a mount that's gone. ❌ Not a second source
    /// of truth for `root()`: an instance's own `mount_path` is what it addresses.
    active_mount_path: Arc<StdRwLock<PathBuf>>,
}

impl SmbVolume {
    /// Creates a new SMB volume with an established smb2 connection.
    ///
    /// # Arguments
    /// * `name` - Display name (typically the share name)
    /// * `mount_path` - OS mount point path
    /// * `volume_id` - Volume ID for listing cache lookups
    /// * `params` - Connection parameters (server, share, port, credentials) used to build the
    ///   current session and to rebuild it on `attempt_reconnect`
    /// * `client` - Connected `SmbClient`
    /// * `tree` - Connected `Tree` for the share
    pub fn new(
        name: impl Into<String>,
        mount_path: impl Into<PathBuf>,
        volume_id: impl Into<String>,
        params: SmbConnectionParams,
        client: SmbClient,
        tree: Tree,
    ) -> Self {
        let share_name = params.share_name.clone();
        let mount_path = mount_path.into();
        Self {
            name: name.into(),
            mount_path: mount_path.clone(),
            mount_root_gone: AtomicBool::new(false),
            inner: Arc::new(SmbVolumeInner {
                share_name,
                volume_id: volume_id.into(),
                params: Arc::new(tokio::sync::RwLock::new(params)),
                client: Arc::new(tokio::sync::Mutex::new(Some(client))),
                tree: Arc::new(tokio::sync::RwLock::new(Some(Arc::new(tree)))),
                state: Arc::new(AtomicU8::new(ConnectionState::Direct as u8)),
                watcher_cancel: std::sync::Mutex::new(None),
                reconnect_lock: Arc::new(tokio::sync::Mutex::new(())),
                unmounted: Arc::new(AtomicBool::new(false)),
                superseded: Arc::new(AtomicBool::new(false)),
                instance_id: NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
                scan_pool: tokio::sync::RwLock::new(None),
                scan_session_refs: AtomicUsize::new(0),
                active_mount_path: Arc::new(StdRwLock::new(mount_path)),
            }),
        }
    }

    /// Another instance of this SAME share, addressing `new_root`.
    ///
    /// The registry's promotion path (`manager/roots.rs`), through
    /// [`Volume::rerooted`]. Everything live is shared, so this is one allocation
    /// and no I/O; the share-scoped watcher is re-pointed at the new root so its
    /// listing-cache notifications keep landing where the panes are.
    fn instance_at_root(&self, new_root: &Path) -> Self {
        *self.inner.active_mount_path.write_ignore_poison() = new_root.to_path_buf();
        Self {
            name: self.name.clone(),
            mount_path: new_root.to_path_buf(),
            mount_root_gone: AtomicBool::new(false),
            inner: Arc::clone(&self.inner),
        }
    }

    /// Returns the volume ID (mirrors `smb_volume_id(server, port, share)`).
    pub(crate) fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// Test-only: drops the smb2 client session. After calling this, any code
    /// path that tries to acquire the client mutex sees `None` and returns
    /// `VolumeError::DeviceDisconnected`. Used by
    /// `smb_scan_oracle_tests::smb_scan_uses_oracle_on_hit_skips_stat_pipeline`
    /// to prove the oracle short-circuit doesn't touch the SMB session: if it
    /// did, the scan would fail with DeviceDisconnected after this call.
    #[cfg(test)]
    pub(in crate::file_system::volume) async fn detach_session_for_test(&self) {
        let mut client_guard = self.inner.client.lock().await;
        *client_guard = None;
    }
}

/// Creates an `SmbVolume` by connecting to a server and share.
///
/// Used by the mount flow to establish the smb2 session alongside the OS mount.
/// Also spawns a background watcher task for detecting external changes. The
/// credentials inside `params` are stored on the resulting `SmbVolume` so it
/// can rebuild its own session via `attempt_reconnect` after a transient
/// connection loss.
///
/// `volume_id` must match the key the caller will use to register the volume
/// with `VolumeManager`. Production callers derive it from the mount path via
/// `volume_id_for_mount` so the OS-event watcher and this path always agree;
/// tests typically pass `smb_volume_id(server, port, share)` directly.
pub async fn connect_smb_volume(
    name: &str,
    mount_path: &str,
    volume_id: &str,
    params: SmbConnectionParams,
) -> Result<SmbVolume, smb2::Error> {
    let (client, tree) = build_session(&params).await?;
    let vol = SmbVolume::new(name, mount_path, volume_id, params.clone(), client, tree);
    vol.spawn_watcher(&params);
    // PII-free analytics: a direct SMB connection succeeded. No host / share / credential
    // identifiers ever cross.
    crate::analytics::posthog::capture("smb_connected", serde_json::json!({}));
    Ok(vol)
}

impl SmbConnectionParams {
    /// Builds the params struct for an optionally-authenticated connection.
    ///
    /// `username = None` and `password = None` becomes a guest connection
    /// (`"Guest"` / empty password), matching the historical mount-time
    /// defaults. The fields are public so callers with explicit credentials
    /// in hand can build the struct directly.
    pub fn new(server: &str, share_name: &str, port: u16, username: Option<&str>, password: Option<&str>) -> Self {
        Self {
            server: server.to_string(),
            share_name: share_name.to_string(),
            port,
            username: username.unwrap_or("Guest").to_string(),
            password: password.unwrap_or("").to_string(),
        }
    }
}

// These test submodules live as sibling files next to this `smb/` directory
// (matching the `in_memory_test.rs` / `local_posix_test.rs` convention) but are
// children of the `smb` module so `super::*` reaches the backend private items.
// `#[path]` is relative to this `mod.rs`, so each points up into `backends/`.
#[cfg(test)]
#[path = "../smb_archive_integration_test.rs"]
mod smb_archive_integration_test;

#[cfg(test)]
#[path = "../smb_conformance_test.rs"]
mod smb_conformance_test;
#[cfg(test)]
#[path = "../smb_full_concurrency_test.rs"]
mod smb_full_concurrency_test;
#[cfg(test)]
#[path = "../smb_integration_test.rs"]
mod smb_integration_test;
#[cfg(test)]
#[path = "../smb_media_fetch_integration_test.rs"]
mod smb_media_fetch_integration_test;
#[cfg(test)]
#[path = "../smb_soak_test.rs"]
mod smb_soak_test;
#[cfg(test)]
#[path = "../smb_streaming_integration_test.rs"]
mod smb_streaming_integration_test;
#[cfg(test)]
#[path = "../smb_stress_test.rs"]
mod smb_stress_test;
#[cfg(test)]
#[path = "../smb_test.rs"]
mod smb_test;
#[cfg(test)]
#[path = "../smb_test_support.rs"]
mod smb_test_support;
#[cfg(test)]
#[path = "../smb_transfer_safety_test.rs"]
mod smb_transfer_safety_test;
#[cfg(test)]
#[path = "../smb_transfer_semantics_test.rs"]
mod smb_transfer_semantics_test;

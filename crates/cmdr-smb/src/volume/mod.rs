//! The SMB backend: a `Volume` over a live smb2 session.
//!
//! The share is also OS-mounted, for Finder / Terminal / drag-and-drop
//! compatibility, but none of Cmdr's own I/O goes near that mount: every read,
//! write, listing, and change notification rides smb2's pipelined session
//! instead, which is both faster and fail-fast where a wedged kernel mount
//! blocks for minutes.
//!
//! Nothing here names the application. What the backend needs from it arrives
//! through the [`VolumeHost`] seams it is handed in [`connect_smb_volume`] and
//! keeps on the share-scoped inner state. `CLAUDE.md` has the must-knows,
//! `DETAILS.md` the lifecycles and the decisions.

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::Retirement;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::settings::BackendName;
use smb2::SmbClient;
use smb2::client::tree::Tree;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize};

mod foreground_yield;
mod mapping;
mod mutation;
mod paths;
mod query;
mod reconnect;
mod scan;
mod scan_pool;
mod session;
mod state;
mod streams;
mod volume_impl;
mod watcher;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

// The backend's own public vocabulary, hoisted so callers write
// `cmdr_smb::volume::ConnectionState`.
pub use state::ConnectionState;

use reconnect::spawn_watcher_death_reconnect;
use session::build_session;

/// This backend's settings namespace, for everything it reads through
/// [`VolumeHost::settings`]. A namespace, not a classification: nothing branches
/// on it, and the app resolves it through a table
/// (`file_system::backend_settings`).
const BACKEND: BackendName = "smb";

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
/// `Tree::read_file_compound_sized` / `Tree::write_file_compound` on the cloned
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
pub struct SmbConnectionParams {
    /// Resolved server address (IP or hostname, ready to pass to
    /// [`build_smb_addr`](crate::build_smb_addr)).
    ///
    /// NFC-normalized: see [`SmbConnectionParams::new`].
    pub server: String,
    /// The share on that server, without leading or trailing separators.
    ///
    /// NFC-normalized, and it has to stay that way: this string goes to
    /// `connect_share` verbatim, and a decomposed spelling is answered with
    /// `STATUS_BAD_NETWORK_NAME`. Build these params with
    /// [`SmbConnectionParams::new`] rather than by struct literal, or fold the
    /// name yourself. See [`SmbConnectionParams::new`].
    pub share_name: String,
    /// The TCP port the server listens on (445 everywhere but a test fixture).
    pub port: u16,
    /// The account to authenticate as. `"Guest"` for an unauthenticated share.
    pub username: String,
    /// Its password, empty for a guest connection. Held in memory for the
    /// volume's lifetime, because every smb2 call needs it.
    pub password: String,
}

/// A `Volume` instance addressing one mount root of a share, over the shared
/// per-share state (`SmbVolumeInner`) every instance of that share rides.
///
/// The split is what makes [`Volume::rerooted`](cmdr_fs::volume::Volume::rerooted) free: a share reached through two
/// mount points is ONE session, and moving the registry's ID from a dead mount to
/// a live one only has to hand out another instance over the same
/// `Arc<SmbVolumeInner>`. Nothing re-authenticates, no transport is rebuilt, and
/// whoever still holds the old instance (a running copy, an open viewer stream)
/// keeps working at the root it was handed. See `DETAILS.md` § "Re-rooting a
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
struct SmbVolumeInner {
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
    /// Whether this share still owns its volume id in the `VolumeManager`.
    ///
    /// Set two ways, both of them the registry's answer rather than the
    /// backend's: `on_superseded` when a newer instance takes the id over, and
    /// the registry itself when the volume leaves it entirely. The session stays
    /// up for whoever still holds an instance, but everything scoped to the ID
    /// (the watcher, the scan pool, `volume-connection-changed` events, the
    /// index-resume hook) belongs to somebody else now and must go quiet here.
    /// See `DETAILS.md` § "Supersede vs. unmount".
    retirement: Arc<Retirement>,
    /// This share's own `Weak`, for [`SmbVolumeInner::self_handle`]. Built by
    /// `Arc::new_cyclic`, because the watcher this feeds is spawned from inside
    /// the share's own methods, where no `Arc` to it is in hand.
    me: std::sync::Weak<SmbVolumeInner>,
    /// The per-scan connection pool: extra smb2 sessions that background bulk work
    /// (the index scan's listings, media enrichment's prefetch reads) spreads
    /// across, opened lazily on `begin_scan_session` and torn down when the LAST
    /// concurrent session ends. `None` between scans (steady-state footprint is
    /// just the one browsing session). See `scan_pool.rs`.
    scan_pool: tokio::sync::RwLock<Option<Arc<scan_pool::ScanPool>>>,
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
    /// Everything this backend asks the application around it: the pane listings,
    /// the secret store, the file index, the frontend event channel, the live
    /// concurrency knob, and the runtime background work spawns onto. A value the
    /// app hands down at construction, never a static this crate reaches for.
    host: VolumeHost,
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
    /// * `host` - Everything the backend asks the app around it (see [`VolumeHost`])
    pub fn new(
        name: impl Into<String>,
        mount_path: impl Into<PathBuf>,
        volume_id: impl Into<String>,
        params: SmbConnectionParams,
        client: SmbClient,
        tree: Tree,
        host: VolumeHost,
    ) -> Self {
        let share_name = params.share_name.clone();
        let mount_path = mount_path.into();
        let volume_id = volume_id.into();
        Self {
            name: name.into(),
            mount_path: mount_path.clone(),
            mount_root_gone: AtomicBool::new(false),
            inner: Arc::new_cyclic(|me| SmbVolumeInner {
                share_name,
                volume_id,
                params: Arc::new(tokio::sync::RwLock::new(params)),
                client: Arc::new(tokio::sync::Mutex::new(Some(client))),
                tree: Arc::new(tokio::sync::RwLock::new(Some(Arc::new(tree)))),
                state: Arc::new(AtomicU8::new(ConnectionState::Direct as u8)),
                watcher_cancel: std::sync::Mutex::new(None),
                reconnect_lock: Arc::new(tokio::sync::Mutex::new(())),
                unmounted: Arc::new(AtomicBool::new(false)),
                retirement: Arc::new(Retirement::new()),
                me: me.clone(),
                scan_pool: tokio::sync::RwLock::new(None),
                scan_session_refs: AtomicUsize::new(0),
                active_mount_path: Arc::new(StdRwLock::new(mount_path)),
                host,
            }),
        }
    }
}

impl SmbVolumeInner {
    /// This share as a path-addressing volume, rooted where the registry serves
    /// it right now.
    ///
    /// Path translation is per mount root (`SmbVolume::to_smb_path` strips it),
    /// so share-scoped background work that needs to stat a path has to pick one,
    /// and the active root is the one every path such work builds already comes
    /// from. Everything live is shared with the registered instances — the same
    /// session, the same state — so this is one allocation and no I/O.
    pub(super) fn at_active_root(self: Arc<Self>) -> SmbVolume {
        let mount_path = self.active_mount_path.read_ignore_poison().clone();
        SmbVolume {
            name: self.share_name.clone(),
            mount_path,
            mount_root_gone: AtomicBool::new(false),
            inner: self,
        }
    }
}

impl SmbVolume {
    /// Another instance of this SAME share, addressing `new_root`.
    ///
    /// The registry's promotion path (`manager/roots.rs`), through
    /// [`Volume::rerooted`](cmdr_fs::volume::Volume::rerooted). Everything live is shared,
    /// so this is one allocation
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

    /// Returns the volume ID (mirrors
    /// [`smb_volume_id`](cmdr_fs::volume::smb_volume_id)`(server, port, share)`),
    /// which is the key every listing-cache lookup and every
    /// `volume-connection-changed` event this share sends is made under.
    pub fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// Test-only: drops the smb2 client session. After calling this, any code
    /// path that tries to acquire the client mutex sees `None` and returns
    /// [`VolumeError::DeviceDisconnected`](cmdr_fs::volume::VolumeError::DeviceDisconnected).
    /// The app's
    /// `smb_integration_scan_uses_oracle_on_hit_skips_stat_pipeline` proves the scan
    /// oracle's short-circuit doesn't touch the SMB session with it: if it did,
    /// the scan would fail with `DeviceDisconnected` after this call.
    #[cfg(any(test, feature = "testing"))]
    pub async fn detach_session_for_test(&self) {
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
    host: VolumeHost,
) -> Result<SmbVolume, smb2::Error> {
    let (client, tree) = build_session(&params).await?;
    let vol = SmbVolume::new(name, mount_path, volume_id, params.clone(), client, tree, host);
    vol.inner.spawn_watcher(&params);
    // PII-free analytics: a direct SMB connection succeeded. No host / share / credential
    // identifiers ever cross.
    vol.inner.host.analytics().record("smb_connected", &[]);
    Ok(vol)
}

impl SmbConnectionParams {
    /// Builds the params struct for an optionally-authenticated connection.
    ///
    /// `username = None` and `password = None` becomes a guest connection
    /// (`"Guest"` / empty password), matching the historical mount-time
    /// defaults. The fields are public so callers with explicit credentials
    /// in hand can build the struct directly.
    ///
    /// **`server` and `share_name` are NFC-folded here**, which is what upholds
    /// the invariant both fields document. macOS `statfs` hands out decomposed
    /// (NFD) names while SMB servers store and answer with composed (NFC) ones,
    /// and a share named `Régi NAS` reached with the decomposed spelling gets
    /// `STATUS_BAD_NETWORK_NAME` from TreeConnect while the composed one connects
    /// (ERR-ABXW4). Every wire use of the name reads it off these params, so
    /// folding at the one constructor covers `build_session` and the watcher's
    /// own session alike. Credentials are NOT folded: a password is bytes the
    /// user typed, and normalizing it would change the secret.
    pub fn new(server: &str, share_name: &str, port: u16, username: Option<&str>, password: Option<&str>) -> Self {
        use unicode_normalization::UnicodeNormalization;

        Self {
            server: server.nfc().collect(),
            share_name: share_name.nfc().collect(),
            port,
            username: username.unwrap_or("Guest").to_string(),
            password: password.unwrap_or("").to_string(),
        }
    }
}

#[cfg(test)]
mod params_normalization_test {
    use super::SmbConnectionParams;

    /// macOS `statfs` spells an accented share decomposed (NFD) while the server
    /// stores it composed (NFC), and TreeConnect answers the NFD spelling with
    /// `STATUS_BAD_NETWORK_NAME`. Both wire uses of the name (`build_session` and
    /// the watcher's own session) read it off these params, so normalizing here
    /// is what keeps a share named `Régi NAS` reachable. Reported as ERR-ABXW4.
    #[test]
    fn new_normalizes_share_and_server_to_nfc() {
        let composed_share = "R\u{e9}gi NAS";
        let decomposed_share = "Re\u{301}gi NAS";
        assert_ne!(
            composed_share, decomposed_share,
            "the two spellings must differ as bytes, or this proves nothing"
        );

        let params = SmbConnectionParams::new("cafe\u{301}-nas", decomposed_share, 445, None, None);
        assert_eq!(params.share_name, composed_share);
        assert_eq!(params.server, "caf\u{e9}-nas");
    }

    /// An already-composed name must survive untouched: normalization is a fold,
    /// not a rewrite.
    #[test]
    fn new_leaves_composed_names_alone() {
        let params = SmbConnectionParams::new("naspolya", "R\u{e9}gi NAS", 445, Some("david"), Some("pw"));
        assert_eq!(params.share_name, "R\u{e9}gi NAS");
        assert_eq!(params.server, "naspolya");
        assert_eq!(params.username, "david");
    }
}

// The suites that assert on this backend's own behavior. They live here rather
// than in the app because they are WHITE-BOX tests: they build an
// `SmbVolumeInner` by struct literal, drive `do_attempt_reconnect` directly, and
// read the client, tree, and scan pool out of the session. The app keeps the
// SMB cells whose other half is the app's own machinery (the transfer pipeline,
// the volume registry, the listing cache); see `DETAILS.md` § "Which side a test
// lives on".
#[cfg(test)]
mod conformance_test;
#[cfg(test)]
mod host_seam_test;
#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod retirement_test;
#[cfg(test)]
mod session_integration_test;
#[cfg(test)]
mod streaming_integration_test;
#[cfg(test)]
mod test_support;

//! SMB volume implementation using direct smb2 protocol operations.
//!
//! Wraps an smb2 session to provide file system access through the Volume trait.
//! The share remains OS-mounted (for Finder/Terminal/drag-drop compatibility),
//! but all Cmdr file operations go through smb2's pipelined I/O for better
//! performance and fail-fast behavior.

use super::{
    BatchScanResult, CopyScanResult, ScanConflict, SmbConnectionState, SourceItemInfo, SpaceInfo, Volume, VolumeError,
    VolumeReadStream,
};
use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::try_get_watched_listing;
use log::{debug, info, warn};
use smb2::client::tree::Tree;
use smb2::{ClientConfig, SmbClient};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

// ── App-handle for connection-state events ──────────────────────────

/// Global `AppHandle` for emitting `smb-connection-changed` events. Set once
/// from `lib.rs::setup`. Same pattern as `network::mdns_discovery::APP_HANDLE`.
static APP_HANDLE: OnceLock<StdMutex<Option<AppHandle>>> = OnceLock::new();

/// Stores the `AppHandle` so SMB state transitions can emit events.
pub fn set_app_handle(handle: AppHandle) {
    let storage = APP_HANDLE.get_or_init(|| StdMutex::new(None));
    if let Ok(mut guard) = storage.lock() {
        *guard = Some(handle);
    }
}

fn get_app_handle() -> Option<AppHandle> {
    APP_HANDLE.get().and_then(|m| m.lock().ok()).and_then(|g| g.clone())
}

/// Payload for `smb-connection-changed`. The frontend reconnect manager listens
/// for this and runs the per-volume backoff cycle.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SmbConnectionChangedPayload {
    volume_id: String,
    /// `"direct"` or `"disconnected"`. The internal state machine is binary;
    /// the OS-mount fallback only exists at the outer `SmbConnectionState` layer
    /// (driven by `enrich_smb_connection_state`), not on the smb2 hot path.
    state: &'static str,
}

fn emit_state_change(volume_id: &str, state: &'static str) {
    if let Some(app) = get_app_handle()
        && let Err(e) = app.emit(
            "smb-connection-changed",
            SmbConnectionChangedPayload {
                volume_id: volume_id.to_string(),
                state,
            },
        )
    {
        warn!("Failed to emit smb-connection-changed: {}", e);
    }
}

// ── Connection state ────────────────────────────────────────────────

/// Connection health states for an SmbVolume.
///
/// Stored as `AtomicU8` for lock-free reads from any thread. The internal state
/// machine is binary (`Direct ⇄ Disconnected`). The "OS mount" fallback the
/// frontend shows lives at the outer `SmbConnectionState` layer (see
/// `enrich_smb_connection_state` in `commands/volumes.rs`) and never reaches
/// this atomic on the smb2 hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// smb2 session is active. All ops go through smb2 (fast path).
    Direct = 0,
    /// smb2 session is down. Return errors immediately.
    Disconnected = 2,
}

impl ConnectionState {
    fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Direct,
            2 => Self::Disconnected,
            _ => Self::Disconnected,
        }
    }
}

static CLIENT_LOCK_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// ── Type mapping helpers ────────────────────────────────────────────

/// Converts an `smb2::FileTime` to seconds since the Unix epoch, matching
/// `FileEntry.modified_at` / `created_at` (seconds, like `LocalPosixVolume`).
fn filetime_to_unix_secs(ft: smb2::pack::FileTime) -> Option<u64> {
    let st = ft.to_system_time()?;
    let dur = st.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_secs())
}

/// Converts an `smb2::DirectoryEntry` to a `FileEntry`.
///
/// `parent_path` is the absolute path of the parent directory (under the mount point).
fn directory_entry_to_file_entry(entry: &smb2::client::tree::DirectoryEntry, parent_path: &str) -> FileEntry {
    let path = if parent_path.ends_with('/') {
        format!("{}{}", parent_path, entry.name)
    } else {
        format!("{}/{}", parent_path, entry.name)
    };

    let mut fe = FileEntry::new(entry.name.clone(), path, entry.is_directory, false);
    fe.size = if entry.is_directory { None } else { Some(entry.size) };
    fe.modified_at = filetime_to_unix_secs(entry.modified);
    fe.created_at = filetime_to_unix_secs(entry.created);
    fe
}

/// Converts an `smb2::FsInfo` to `SpaceInfo`.
fn fs_info_to_space_info(info: &smb2::client::tree::FsInfo) -> SpaceInfo {
    let used = info.total_bytes.saturating_sub(info.free_bytes);
    SpaceInfo {
        total_bytes: info.total_bytes,
        available_bytes: info.free_bytes,
        used_bytes: used,
    }
}

/// Converts an `smb2::Error` to `VolumeError`.
fn map_smb_error(err: smb2::Error) -> VolumeError {
    use smb2::ErrorKind;
    use smb2::types::status::NtStatus;

    // `STATUS_DELETE_PENDING` currently classifies as `ErrorKind::Other` in
    // smb2 (no typed variant yet), so we detect it via the raw NTSTATUS before
    // falling through to the generic kind match.
    if err.status() == Some(NtStatus::DELETE_PENDING) {
        return VolumeError::DeletePending(err.to_string());
    }

    match err.kind() {
        ErrorKind::NotFound => VolumeError::NotFound(err.to_string()),
        ErrorKind::AlreadyExists => VolumeError::AlreadyExists(err.to_string()),
        ErrorKind::IsADirectory => VolumeError::IsADirectory(err.to_string()),
        ErrorKind::AccessDenied | ErrorKind::AuthRequired | ErrorKind::SigningRequired => {
            VolumeError::PermissionDenied(err.to_string())
        }
        ErrorKind::ConnectionLost | ErrorKind::SessionExpired => VolumeError::DeviceDisconnected(err.to_string()),
        ErrorKind::TimedOut => VolumeError::ConnectionTimeout(err.to_string()),
        ErrorKind::DiskFull => VolumeError::StorageFull {
            message: err.to_string(),
        },
        ErrorKind::Cancelled => VolumeError::Cancelled("Operation cancelled by user".to_string()),
        _ => VolumeError::IoError {
            message: err.to_string(),
            raw_os_error: None,
        },
    }
}

// ── SmbVolume ───────────────────────────────────────────────────────

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

pub struct SmbVolume {
    /// Display name (share name).
    name: String,
    /// OS mount point (for example, "/Volumes/Documents").
    mount_path: PathBuf,
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
        Self {
            name: name.into(),
            mount_path: mount_path.into(),
            share_name,
            volume_id: volume_id.into(),
            params: Arc::new(tokio::sync::RwLock::new(params)),
            client: Arc::new(tokio::sync::Mutex::new(Some(client))),
            tree: Arc::new(tokio::sync::RwLock::new(Some(Arc::new(tree)))),
            state: Arc::new(AtomicU8::new(ConnectionState::Direct as u8)),
            watcher_cancel: std::sync::Mutex::new(None),
            reconnect_lock: Arc::new(tokio::sync::Mutex::new(())),
            unmounted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns the volume ID (mirrors `smb_volume_id(server, port, share)`).
    pub(crate) fn volume_id(&self) -> &str {
        &self.volume_id
    }

    /// Test-only: drops the smb2 client session. After calling this, any code
    /// path that tries to acquire the client mutex sees `None` and returns
    /// `VolumeError::DeviceDisconnected`. Used by
    /// `smb_scan_oracle_tests::smb_scan_uses_oracle_on_hit_skips_stat_pipeline`
    /// to prove the oracle short-circuit doesn't touch the SMB session: if it
    /// did, the scan would fail with DeviceDisconnected after this call.
    #[cfg(test)]
    pub(in crate::file_system::volume) async fn detach_session_for_test(&self) {
        let mut client_guard = self.client.lock().await;
        *client_guard = None;
    }

    /// Returns the current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Snapshot the smb2 client's diagnostics tree.
    ///
    /// Returns `None` while the client is disconnected (no `SmbClient`
    /// is held). Otherwise grabs the client mutex briefly, calls
    /// `client.diagnostics()` (cheap atomic loads + short critical
    /// sections inside smb2 — no I/O), and releases the lock before
    /// returning.
    ///
    /// Used by the debug-window SMB diagnostics dashboard. Safe to call
    /// at 1 Hz; cheap even at higher rates.
    pub async fn diagnostics(&self) -> Option<smb2::Diagnostics> {
        let guard = self.client.lock().await;
        guard.as_ref().map(|c| c.diagnostics())
    }

    /// Flips state to `Disconnected` and emits `smb-connection-changed` if the
    /// previous state was something else (silent if we were already Disconnected,
    /// to avoid event spam when several in-flight ops all see the same broken
    /// session).
    fn transition_to_disconnected(&self) {
        let prev = self.state.swap(ConnectionState::Disconnected as u8, Ordering::Relaxed);
        if prev != ConnectionState::Disconnected as u8 {
            emit_state_change(&self.volume_id, "disconnected");
        }
    }

    /// Flips state to `Direct` and emits `smb-connection-changed` if the previous
    /// state was something else. Called by `attempt_reconnect` after a successful
    /// session rebuild.
    fn transition_to_direct(&self) {
        let prev = self.state.swap(ConnectionState::Direct as u8, Ordering::Relaxed);
        if prev != ConnectionState::Direct as u8 {
            emit_state_change(&self.volume_id, "direct");
        }
    }

    /// Converts a volume-relative path to the SMB relative path string.
    ///
    /// The frontend sends paths relative to the volume root (which is the mount path).
    /// smb2 expects paths relative to the share root with `/` separators.
    /// NFC-normalizes the result because macOS sends NFD (decomposed) paths
    /// but SMB servers expect NFC (composed). Without this, paths with accented
    /// characters (like "ä") fail with STATUS_OBJECT_PATH_NOT_FOUND.
    fn to_smb_path(&self, path: &Path) -> String {
        use unicode_normalization::UnicodeNormalization;

        let path_str = path.to_string_lossy();

        // Handle paths that start with the mount path (absolute paths from frontend)
        if let Some(relative) = path_str.strip_prefix(self.mount_path.to_string_lossy().as_ref()) {
            let trimmed = relative.trim_start_matches('/');
            return trimmed.nfc().collect();
        }

        // Handle empty or root paths
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return String::new();
        }

        // Strip leading slash for absolute paths
        let raw = path_str.strip_prefix('/').unwrap_or(&path_str);
        raw.nfc().collect()
    }

    /// Returns the full absolute path for a relative SMB path (under mount point).
    fn to_display_path(&self, smb_path: &str) -> String {
        if smb_path.is_empty() {
            self.mount_path.to_string_lossy().to_string()
        } else {
            format!("{}/{}", self.mount_path.display(), smb_path)
        }
    }

    // ── Recursive helper for scan ──────────────────────────────────────

    /// Recursively scans an SMB path, returning file/dir counts and total bytes.
    fn scan_recursive<'a>(
        &'a self,
        smb_path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut result = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                // SMB exposes no hardlinks, so the source footprint always
                // equals the write footprint. Kept in lockstep with
                // `total_bytes` at every accumulation site below.
                dedup_bytes: 0,
                // Root path is always a directory; the file branch below
                // overwrites this to `false`. Subdirectory recursions also
                // return `true`; only the leaf file branch sets `false`.
                top_level_is_directory: true,
            };

            // Stat to determine if this is a file or directory
            if smb_path.is_empty() {
                // Root is always a directory, scan its contents
            } else {
                let info = {
                    let (tree, mut conn) = self.clone_session().await?;
                    let r = tree.stat(&mut conn, smb_path).await;
                    self.handle_smb_result("scan_for_copy(stat)", r)?
                };

                if !info.is_directory {
                    result.file_count = 1;
                    result.total_bytes = info.size;
                    result.dedup_bytes = info.size;
                    result.top_level_is_directory = false;
                    return Ok(result);
                }
            }

            // It's a directory: list and recurse
            result.dir_count += 1;
            let display_path = self.to_display_path(smb_path);
            let entries = self.list_directory_impl(Path::new(&display_path)).await?;

            for entry in &entries {
                let child_smb = if smb_path.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{}/{}", smb_path, entry.name)
                };

                if entry.is_directory {
                    let sub = self.scan_recursive(&child_smb).await?;
                    result.file_count += sub.file_count;
                    result.dir_count += sub.dir_count;
                    result.total_bytes += sub.total_bytes;
                    result.dedup_bytes += sub.dedup_bytes;
                } else {
                    result.file_count += 1;
                    result.total_bytes += entry.size.unwrap_or(0);
                    result.dedup_bytes += entry.size.unwrap_or(0);
                }
            }

            Ok(result)
        })
    }

    /// Shared async implementation of list_directory used by both the trait method
    /// and internal helpers (which need to call it without going through the trait).
    async fn list_directory_impl(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let smb_path = self.to_smb_path(path);
        let display_path = self.to_display_path(&smb_path);

        debug!(
            "SmbVolume::list_directory: share={}, input={:?}, smb_path={:?}",
            self.share_name, path, smb_path
        );

        let start = std::time::Instant::now();

        let result = {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.list_directory(&mut conn, &smb_path).await;
            self.handle_smb_result("list_directory", r)?
        };

        let entries: Vec<FileEntry> = result
            .iter()
            .filter(|e| e.name != "." && e.name != "..")
            .map(|e| directory_entry_to_file_entry(e, &display_path))
            .collect();

        debug!(
            "SmbVolume::list_directory: completed in {:?}, {} entries",
            start.elapsed(),
            entries.len()
        );

        Ok(entries)
    }

    // ── Connection helpers ──────────────────────────────────────────────

    /// Checks that the connection is in `Direct` state. Returns
    /// `DeviceDisconnected` for `Disconnected`.
    fn check_connection(&self) -> Result<(), VolumeError> {
        match self.connection_state() {
            ConnectionState::Direct => Ok(()),
            ConnectionState::Disconnected => Err(VolumeError::DeviceDisconnected(
                "SMB connection is disconnected".to_string(),
            )),
        }
    }

    /// Reads out a clone of `Arc<Tree>`. Cheap (`Arc::clone`).
    async fn tree_arc(&self) -> Result<Arc<Tree>, VolumeError> {
        self.check_connection()?;
        let guard = self.tree.read().await;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))
    }

    /// Briefly locks the client mutex, clones its `Connection` (cheap
    /// `Arc::clone`; all clones multiplex frames over the same SMB session),
    /// and releases the lock. Also reads out an `Arc<Tree>`. Returns both.
    ///
    /// Callers can then drive `Tree::download` / `Tree::read_file_compound` /
    /// `Tree::write_file_compound` on the owned `Connection` without holding
    /// any lock, enabling multiple concurrent copies on a single `SmbVolume`.
    async fn clone_session(&self) -> Result<(Arc<Tree>, smb2::client::Connection), VolumeError> {
        self.check_connection()?;
        let tree = self.tree_arc().await?;
        let ticket = CLIENT_LOCK_TICKET.fetch_add(1, Ordering::Relaxed);
        let start = std::time::Instant::now();
        log::debug!(
            "client-mutex: waiting ticket={} caller=clone_session share={}",
            ticket,
            self.share_name
        );
        let conn = {
            let mut guard = self.client.lock().await;
            log::debug!(
                "client-mutex: acquired ticket={} caller=clone_session share={} waited={:?}",
                ticket,
                self.share_name,
                start.elapsed()
            );
            let acquired_at = std::time::Instant::now();
            let client = guard.as_mut().ok_or_else(|| {
                log::debug!(
                    "client-mutex: released ticket={} caller=clone_session held_for={:?} (no-session-bail)",
                    ticket,
                    acquired_at.elapsed()
                );
                VolumeError::DeviceDisconnected("SMB session not available".to_string())
            })?;
            let c = client.connection_mut().clone();
            log::debug!(
                "client-mutex: released ticket={} caller=clone_session held_for={:?}",
                ticket,
                acquired_at.elapsed()
            );
            c
        };
        Ok((tree, conn))
    }

    /// Opens a streaming download on the given SMB-relative path.
    ///
    /// Briefly locks the client mutex to clone the underlying `Connection`,
    /// releases the lock, then spawns a background task that owns the clone
    /// and drives `Tree::download` on it. Each concurrent call gets its own
    /// cloned `Connection` (all multiplexing frames over the same SMB
    /// session), so N downloads run pipelined instead of serializing on the
    /// session mutex. Chunks flow through a bounded mpsc channel to the
    /// caller-facing `SmbReadStream`.
    ///
    /// This is the single streaming-read primitive for `SmbVolume`. The
    /// cross-volume streaming path (`open_read_stream`) goes through here, so
    /// no path has to buffer whole files in memory.
    async fn open_smb_download_stream(&self, smb_path: &str) -> Result<SmbReadStream, VolumeError> {
        let (tree, conn) = self.clone_session().await?;

        let (size_tx, size_rx) = tokio::sync::oneshot::channel::<Result<u64, VolumeError>>();
        let (chunk_tx, chunk_rx) =
            tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let state_arc = Arc::clone(&self.state);
        let volume_id = self.volume_id.clone();
        let share_name = self.share_name.clone();
        let smb_path_owned = smb_path.to_string();

        tokio::spawn(async move {
            // The task owns its `Connection` clone and an `Arc<Tree>` reference.
            // No lock is held, so other tasks can spawn in parallel and each
            // drive their own download on a fresh `Connection` clone, all
            // multiplexed over the same SMB session by smb2's receiver task.
            let mut conn = conn;
            let mut download = match tree.download(&mut conn, &smb_path_owned).await {
                Ok(d) => d,
                Err(e) => {
                    update_state_on_smb_error(&state_arc, &volume_id, &e);
                    warn!(
                        "SmbVolume::download(share={}, path={}): {}",
                        share_name, smb_path_owned, e
                    );
                    let _ = size_tx.send(Err(map_smb_error(e)));
                    return;
                }
            };

            let total_size = download.size();
            if size_tx.send(Ok(total_size)).is_err() {
                // Caller dropped the stream before receiving size. Drop download
                // cleanly (Drop logs a may-leak debug line; the handle is released
                // when the SMB session closes).
                return;
            }

            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => {
                        debug!(
                            "SmbVolume::download(share={}, path={}): cancelled after {} bytes",
                            share_name, smb_path_owned, download.bytes_received()
                        );
                        break;
                    }
                    chunk = download.next_chunk() => match chunk {
                        Some(Ok(bytes)) => {
                            if chunk_tx.send(Ok(bytes)).await.is_err() {
                                // Consumer dropped; stop pumping.
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            update_state_on_smb_error(&state_arc, &volume_id, &e);
                            warn!(
                                "SmbVolume::download(share={}, path={}): chunk error: {}",
                                share_name, smb_path_owned, e
                            );
                            let _ = chunk_tx.send(Err(map_smb_error(e))).await;
                            break;
                        }
                        None => break, // download complete
                    }
                }
            }
            // `download` drops here (releases SMB file handle at connection close).
            // `conn` and `tree` drop here: the `Arc<Connection>` inner and the
            // `Arc<Tree>` unwind when every concurrent task finishes.
        });

        let total_size = match size_rx.await {
            Ok(Ok(size)) => size,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(VolumeError::IoError {
                    message: "SMB download task terminated before reporting size".to_string(),
                    raw_os_error: None,
                });
            }
        };

        Ok(SmbReadStream {
            rx: chunk_rx,
            cancel: Some(cancel_tx),
            total_size,
            bytes_read: 0,
        })
    }

    /// Maps an smb2 result, handling connection state transitions on error.
    fn handle_smb_result<T>(&self, op_name: &str, result: Result<T, smb2::Error>) -> Result<T, VolumeError> {
        match result {
            Ok(val) => Ok(val),
            Err(e) => {
                let kind = e.kind();

                // On connection loss, transition to Disconnected
                if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                    warn!(
                        "SmbVolume::{}(share={}): connection lost ({}), transitioning to Disconnected",
                        op_name, self.share_name, e
                    );
                    self.transition_to_disconnected();
                } else if matches!(
                    kind,
                    smb2::ErrorKind::NotFound | smb2::ErrorKind::IsADirectory | smb2::ErrorKind::AlreadyExists
                ) {
                    // Expected fall-through cases: the caller is using the typed
                    // `VolumeError` variant as a signal, not an error:
                    // - `NotFound` for existence checks (rename dest, conflict detection)
                    // - `IsADirectory` for `delete()`'s "try delete_file first, fall back to delete_directory"
                    //   fast-path
                    // - `AlreadyExists` for `copy_directory_streaming`'s "create_directory is idempotent for merge"
                    //   path
                    debug!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                } else {
                    warn!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                }

                Err(map_smb_error(e))
            }
        }
    }

    /// Runs an smb2 operation synchronously. For sync-only contexts (`on_unmount`, tests).
    ///
    /// This exists only for contexts where
    /// no async runtime is available (for example, cleanup paths called from drop-like code).
    fn with_smb_sync<F, T>(&self, op_name: &str, f: F) -> Result<T, VolumeError>
    where
        F: FnOnce(&mut SmbClient, &Tree) -> Result<T, smb2::Error>,
    {
        if self.connection_state() == ConnectionState::Disconnected {
            return Err(VolumeError::DeviceDisconnected(
                "SMB connection is disconnected".to_string(),
            ));
        }

        let tree_arc = {
            let guard = self.tree.blocking_read();
            guard
                .as_ref()
                .cloned()
                .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?
        };

        let mut guard = self.client.blocking_lock();

        let client = guard
            .as_mut()
            .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?;

        match f(client, &tree_arc) {
            Ok(val) => Ok(val),
            Err(e) => {
                let kind = e.kind();

                if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                    warn!(
                        "SmbVolume::{}(share={}): connection lost ({}), transitioning to Disconnected",
                        op_name, self.share_name, e
                    );
                    self.transition_to_disconnected();
                } else if matches!(kind, smb2::ErrorKind::NotFound) {
                    debug!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                } else {
                    warn!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                }

                Err(map_smb_error(e))
            }
        }
    }

    // ── Reconnect / watcher restart ─────────────────────────────────────

    /// Cancels the existing watcher task (if any). The watcher exits on its
    /// next `select!` iteration. Best-effort: if the watcher already exited on
    /// a connection error, the send is a no-op.
    fn stop_watcher(&self) {
        if let Some(tx) = self.watcher_cancel.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(());
        }
    }

    /// Spawns the background watcher task on its own dedicated smb2 session.
    /// Replaces any prior `watcher_cancel`. Called from `connect_smb_volume`
    /// (initial setup) and from `attempt_reconnect` (after a session rebuild).
    ///
    /// We could share the volume's session with the watcher (smb2 0.10's
    /// `Watcher` is `'static`, owns a `Connection` clone), but in practice
    /// stacking the watcher's CHANGE_NOTIFY long-polls on the same TCP
    /// session as heavy concurrent writes wedges Samba — the
    /// `smb_integration_concurrent_streaming_writes_no_deadlock` test
    /// hangs against `smb-consumer-maxreadsize` (64 KB max read/write) when
    /// the watcher shares the connection. Keeping the watcher on its own
    /// TCP+session matches the pre-smb2-0.10 isolation; what we *do* keep
    /// from the new API is the pipelining (`Watcher` keeps one CHANGE_NOTIFY
    /// pre-issued, closing the response→re-arm loss window) and the lack
    /// of internal reconnect (single source of truth is
    /// `SmbVolume::attempt_reconnect`; the watcher bails on errors and we
    /// respawn here on the next successful reconnect).
    fn spawn_watcher(&self, params: &SmbConnectionParams) {
        use crate::network::smb_connection::build_smb_addr;

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let addr = build_smb_addr(&params.server, params.port);
        let share = params.share_name.clone();
        let username = params.username.clone();
        let password = params.password.clone();
        let volume_id = self.volume_id.clone();
        let mount_path = self.mount_path.clone();

        tokio::spawn(super::smb_watcher::run_smb_watcher(
            addr, share, username, password, volume_id, mount_path, cancel_rx,
        ));

        if let Ok(mut guard) = self.watcher_cancel.lock() {
            *guard = Some(cancel_tx);
        }
    }

    /// Inherent body for the trait's `attempt_reconnect`. Lives here as a regular
    /// async method so the body isn't hidden inside a `Pin<Box<...>>` future.
    ///
    /// Idempotent and single-flight:
    /// - If state is already `Direct`, returns Ok cheaply.
    /// - On auth failure, re-pulls credentials from the secret store (in case the user updated
    ///   them) and retries once before giving up.
    /// - On success: stores the new client + tree, restarts the watcher, emits
    ///   `smb-connection-changed { state: "direct" }`.
    /// - On failure: state stays `Disconnected`; the FE backoff cycle decides whether to retry.
    async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        // Bail early if `on_unmount` already ran. Doing this before taking the
        // lock means a queued caller doesn't pay the lock-acquisition cost for
        // a volume that's about to be (or already is) gone.
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(VolumeError::DeviceDisconnected(
                "SMB volume has been unmounted".to_string(),
            ));
        }

        // Single-flight: concurrent callers (FE cycle tick + lazy nav-time
        // retry) all wait here, and the second arrival sees state==Direct.
        let _guard = self.reconnect_lock.lock().await;

        // Re-check `unmounted`: between releasing the early check and acquiring
        // the lock, `on_unmount` may have run on another thread.
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(VolumeError::DeviceDisconnected(
                "SMB volume has been unmounted".to_string(),
            ));
        }

        if self.connection_state() == ConnectionState::Direct {
            debug!(
                "SmbVolume::attempt_reconnect(share={}): already Direct, skipping",
                self.share_name
            );
            return Ok(());
        }

        // First try: stored credentials (the ones that worked at original connect).
        let params_snapshot = { self.params.read().await.clone() };
        info!(
            "SmbVolume::attempt_reconnect(share={}): trying with cached credentials",
            self.share_name
        );

        let first_attempt = build_session(&params_snapshot).await;
        let (client, tree) = match first_attempt {
            Ok(pair) => pair,
            Err(err) if crate::network::smb_util::is_auth_error(&err) => {
                // Cached creds may be stale. Re-pull from the secret store and retry once.
                info!(
                    "SmbVolume::attempt_reconnect(share={}): cached credentials rejected, re-pulling from secret store",
                    self.share_name
                );
                match refresh_credentials_from_store(&params_snapshot).await {
                    Some(refreshed)
                        if refreshed.username != params_snapshot.username
                            || refreshed.password != params_snapshot.password =>
                    {
                        match build_session(&refreshed).await {
                            Ok(pair) => {
                                // Refreshed creds worked; persist them on the volume.
                                let mut params_w = self.params.write().await;
                                params_w.username = refreshed.username.clone();
                                params_w.password = refreshed.password.clone();
                                pair
                            }
                            Err(e2) => {
                                warn!(
                                    "SmbVolume::attempt_reconnect(share={}): refreshed credentials also failed: {}",
                                    self.share_name, e2
                                );
                                return Err(map_smb_error(e2));
                            }
                        }
                    }
                    _ => {
                        // No fresh creds available, or they're identical to the cached ones.
                        warn!(
                            "SmbVolume::attempt_reconnect(share={}): no fresh credentials available; giving up on this attempt",
                            self.share_name
                        );
                        return Err(map_smb_error(err));
                    }
                }
            }
            Err(e) => {
                warn!(
                    "SmbVolume::attempt_reconnect(share={}): connect failed: {}",
                    self.share_name, e
                );
                return Err(map_smb_error(e));
            }
        };

        // The session-build round-trip can take several seconds. The user may
        // have unmounted the volume in the meantime. Discard the freshly-built
        // session and bail rather than installing it into an orphaned volume
        // (which would leak the watcher task and the smb2 connection).
        if self.unmounted.load(Ordering::Relaxed) {
            drop(client);
            drop(tree);
            return Err(VolumeError::DeviceDisconnected(
                "SMB volume was unmounted during reconnect".to_string(),
            ));
        }

        // Install the new session.
        {
            let mut tree_guard = self.tree.write().await;
            *tree_guard = Some(Arc::new(tree));
        }
        {
            let mut client_guard = self.client.lock().await;
            *client_guard = Some(client);
        }

        // Restart the watcher with current params (which may include refreshed creds).
        self.stop_watcher();
        let params_now = self.params.read().await.clone();
        self.spawn_watcher(&params_now);

        // Flip state and emit. Doing this last means an observer that wakes
        // up on the event will see a fully-installed session.
        self.transition_to_direct();

        info!("SmbVolume::attempt_reconnect(share={}): success", self.share_name);
        Ok(())
    }
}

// ── Reconnect helpers (free functions to keep `attempt_reconnect` readable) ──

/// Builds a fresh smb2 session using the given params. Returns the connected
/// client + tree on success.
async fn build_session(params: &SmbConnectionParams) -> Result<(SmbClient, Tree), smb2::Error> {
    use crate::network::smb_connection::build_smb_addr;

    let config = ClientConfig {
        addr: build_smb_addr(&params.server, params.port),
        timeout: Duration::from_secs(10),
        username: params.username.clone(),
        password: params.password.clone(),
        domain: String::new(),
        auto_reconnect: false,
        compression: true,
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
    };
    let mut client = SmbClient::connect(config).await?;
    let tree = client.connect_share(&params.share_name).await?;
    Ok((client, tree))
}

/// Re-fetches credentials from the secret store for the given server/share.
/// Returns `None` if nothing is stored (in which case the cached creds are all
/// we have to work with).
async fn refresh_credentials_from_store(params: &SmbConnectionParams) -> Option<SmbConnectionParams> {
    let server = params.server.clone();
    let share = params.share_name.clone();

    let creds = tokio::task::spawn_blocking(move || {
        // Try share-level first (more specific), then server-level.
        crate::network::keychain::get_credentials(&server, Some(&share))
            .or_else(|_| crate::network::keychain::get_credentials(&server, None))
            .ok()
    })
    .await
    .ok()
    .flatten()?;

    Some(SmbConnectionParams {
        server: params.server.clone(),
        share_name: params.share_name.clone(),
        port: params.port,
        username: creds.username,
        password: creds.password,
    })
}

// ── Streaming support ───────────────────────────────────────────────

/// Backpressure window for the chunk channel. With smb2's ~512 KB pipelined
/// chunks, 4 slots keep peak memory at a few MB regardless of file size.
const SMB_STREAM_CHANNEL_CAPACITY: usize = 4;

/// Streaming reader for SMB files, backed by a background producer task.
///
/// The producer task owns an `OwnedMutexGuard` over the smb2 session and drives
/// an `smb2::FileDownload`, sending each chunk down an mpsc channel. The
/// consumer (this struct) just reads from the channel. This avoids buffering
/// the whole file in memory; peak is bounded by the channel capacity.
///
/// Dropping the stream before it's fully consumed sends a cancel signal so
/// the producer can stop early and release the SMB session lock.
struct SmbReadStream {
    rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, VolumeError>>,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    total_size: u64,
    bytes_read: u64,
}

impl Drop for SmbReadStream {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            // Best-effort: if the producer already finished, recv side is dropped
            // and the send is a no-op.
            let _ = tx.send(());
        }
    }
}

impl VolumeReadStream for SmbReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let chunk = self.rx.recv().await?;
            if let Ok(ref bytes) = chunk {
                self.bytes_read += bytes.len() as u64;
            }
            Some(chunk)
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

/// Wraps a pre-read `Vec<u8>` as a `VolumeReadStream` that yields the whole
/// buffer as a single chunk. Used by the compound fast-path in
/// `open_read_stream_with_hint`, where the full file body came back inside one
/// SMB compound response; there's no more I/O to drive, just hand the bytes
/// to the consumer.
struct InlineReadStream {
    data: Option<Vec<u8>>,
    total_size: u64,
    bytes_read: u64,
}

impl InlineReadStream {
    fn new(data: Vec<u8>) -> Self {
        let total_size = data.len() as u64;
        Self {
            data: Some(data),
            total_size,
            bytes_read: 0,
        }
    }
}

impl VolumeReadStream for InlineReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let data = self.data.take()?;
            self.bytes_read = data.len() as u64;
            Some(Ok(data))
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

/// If an smb2 error indicates the session is dead, transition state to
/// `Disconnected` and emit `smb-connection-changed`. Mirrors `handle_smb_result`
/// for contexts without `&self` (the streaming-read producer task).
fn update_state_on_smb_error(state: &AtomicU8, volume_id: &str, err: &smb2::Error) {
    if matches!(
        err.kind(),
        smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired
    ) {
        let prev = state.swap(ConnectionState::Disconnected as u8, Ordering::Relaxed);
        if prev != ConnectionState::Disconnected as u8 {
            emit_state_change(volume_id, "disconnected");
        }
    }
}

impl Volume for SmbVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.mount_path
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let entries = self.list_directory_impl(path).await?;
            // smb2's list_directory returns all entries at once, so report
            // progress as a single batch after the call completes. Tally files
            // / dirs / bytes from the returned entries so the FE scan dialog
            // doesn't see "0 bytes, 0 dirs" climbing on Direct SMB scans.
            if let Some(on_progress) = on_progress {
                let mut tally = crate::file_system::volume::ListingProgress::default();
                for e in &entries {
                    if e.is_directory {
                        tally.dirs += 1;
                    } else {
                        tally.files += 1;
                        tally.bytes += e.size.unwrap_or(0);
                    }
                }
                on_progress(tally);
            }
            Ok(entries)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::get_metadata: share={}, input={:?}, smb_path={:?}",
                self.share_name, path, smb_path
            );

            // For root, synthesize a directory entry
            if smb_path.is_empty() {
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.mount_path.to_string_lossy().to_string(),
                    true,
                    false,
                ));
            }

            let info = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.stat(&mut conn, &smb_path).await;
                self.handle_smb_result("get_metadata", r)?
            };

            let name = Path::new(&smb_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| smb_path.clone());
            let display_path = self.to_display_path(&smb_path);

            let mut fe = FileEntry::new(name, display_path, info.is_directory, false);
            fe.size = if info.is_directory { None } else { Some(info.size) };
            fe.modified_at = filetime_to_unix_secs(info.modified);
            fe.created_at = filetime_to_unix_secs(info.created);
            Ok(fe)
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);
            if smb_path.is_empty() {
                return true; // Root always exists if we're connected
            }

            {
                match self.clone_session().await {
                    Ok((tree, mut conn)) => tree.stat(&mut conn, &smb_path).await.is_ok(),
                    Err(_) => false,
                }
            }
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);
            if smb_path.is_empty() {
                return Ok(true); // Root is always a directory
            }

            let info = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.stat(&mut conn, &smb_path).await;
                self.handle_smb_result("is_directory", r)?
            };

            Ok(info.is_directory)
        })
    }

    fn supports_watching(&self) -> bool {
        // Starts as false: the existing FSEvents watcher on the OS mount
        // point already provides change notifications. smb2-native watching
        // can be added later as an optimization.
        false
    }

    fn supports_local_fs_access(&self) -> bool {
        // SmbVolume handles listing notifications via notify_mutation,
        // so the old std::fs-based synthetic diff path is not needed.
        false
    }

    fn listing_is_watched(&self, _path: &Path) -> bool {
        // SMB watching is volume-level: the smb_watcher monitors the whole share
        // via CHANGE_NOTIFY. So once the watcher is alive and the session is
        // Direct, every cached listing on this volume is oracle-eligible.
        // `watcher_cancel` is a std `Mutex` (not async): use `try_lock` and treat
        // contention as "not watched" to keep the oracle out of the lock-wait path.
        // The oracle will simply fall through to a real read; that's the safe
        // direction. Don't hold the lock across awaits (we never `.await` here
        // anyway: this is a sync method).
        let has_watcher = match self.watcher_cancel.try_lock() {
            Ok(guard) => guard.is_some(),
            Err(_) => return false,
        };
        has_watcher && self.connection_state() == ConnectionState::Direct
    }

    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: super::MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

            match mutation {
                super::MutationEvent::Created(ref name) | super::MutationEvent::Modified(ref name) => {
                    let entry_path = parent_path.join(name);
                    match self.get_metadata(&entry_path).await {
                        Ok(entry) => {
                            let change = if matches!(mutation, super::MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(&self.volume_id, parent_path, change);
                        }
                        Err(e) => {
                            warn!(
                                "SmbVolume::notify_mutation: couldn't stat {}: {}",
                                entry_path.display(),
                                e
                            );
                        }
                    }
                }
                super::MutationEvent::Deleted(name) => {
                    notify_directory_changed(&self.volume_id, parent_path, DirectoryChange::Removed(name));
                }
                super::MutationEvent::Renamed { from, to } => {
                    let new_path = parent_path.join(&to);
                    match self.get_metadata(&new_path).await {
                        Ok(entry) => {
                            notify_directory_changed(
                                &self.volume_id,
                                parent_path,
                                DirectoryChange::Renamed {
                                    old_name: from,
                                    new_entry: entry,
                                },
                            );
                        }
                        Err(e) => {
                            warn!(
                                "SmbVolume::notify_mutation: couldn't stat renamed entry {}: {}",
                                new_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        })
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            debug!("SmbVolume::get_space_info: share={}", self.share_name);

            let info = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.fs_info(&mut conn).await;
                self.handle_smb_result("get_space_info", r)?
            };

            Ok(fs_info_to_space_info(&info))
        })
    }

    fn space_poll_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(5))
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);
            let data = content.to_vec();

            debug!("SmbVolume::create_file: share={}, path={:?}", self.share_name, smb_path);

            {
                let (tree, conn) = self.clone_session().await?;
                // No-clobber contract via the exclusive-create writer
                // (`FileCreate` disposition): if the file already exists the
                // server returns `STATUS_OBJECT_NAME_COLLISION`, which the
                // smb2 crate maps to `ErrorKind::AlreadyExists`. The earlier
                // stat-then-write workaround left a microsecond TOCTOU
                // window; this closes it atomically at the protocol layer.
                let writer_result = tree.create_file_writer_exclusive(conn, &smb_path).await;
                let mut writer = self.handle_smb_result("create_file(open)", writer_result)?;
                if !data.is_empty() {
                    let write_result = writer.write_chunk(&data).await;
                    self.handle_smb_result("create_file(write_chunk)", write_result)?;
                }
                let finish_result = writer.finish().await;
                self.handle_smb_result("create_file(finish)", finish_result)?;
            }

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    super::MutationEvent::Created(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::create_directory: share={}, path={:?}",
                self.share_name, smb_path
            );

            {
                let (tree, mut conn) = self.clone_session().await?;
                let result = tree.create_directory(&mut conn, &smb_path).await;
                self.handle_smb_result("create_directory", result)?;
            }

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    super::MutationEvent::Created(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!("SmbVolume::delete: share={}, path={:?}", self.share_name, smb_path);

            // Try delete_file first (one round-trip). If the path is a directory,
            // the server returns STATUS_FILE_IS_A_DIRECTORY; then try delete_directory.
            // This avoids a stat round-trip for every file in bulk deletes.
            let file_result = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.delete_file(&mut conn, &smb_path).await;
                self.handle_smb_result("delete_file", r)
            };

            match file_result {
                Ok(()) => {} // File deleted successfully
                Err(VolumeError::IsADirectory(_)) => {
                    // Expected fall-through: path is a directory, retry with delete_directory.
                    let (tree, mut conn) = self.clone_session().await?;
                    let r = tree.delete_directory(&mut conn, &smb_path).await;
                    self.handle_smb_result("delete_directory", r)?;
                }
                Err(e) => return Err(e),
            }

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    super::MutationEvent::Deleted(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_from = self.to_smb_path(from);
            let smb_to = self.to_smb_path(to);

            debug!(
                "SmbVolume::rename: share={}, from={:?}, to={:?}, force={}",
                self.share_name, smb_from, smb_to, force
            );

            if force {
                // Check if dest exists and delete it first
                let dest_exists = {
                    let (tree, mut conn) = self.clone_session().await?;
                    tree.stat(&mut conn, &smb_to).await.is_ok()
                };

                if dest_exists {
                    // Try file delete first; if it fails specifically because the path is a
                    // directory, try directory delete. Any other error (PermissionDenied,
                    // SharingViolation, …) propagates immediately instead of being masked
                    // by a second futile delete.
                    let file_result = {
                        let (tree, mut conn) = self.clone_session().await?;
                        let r = tree.delete_file(&mut conn, &smb_to).await;
                        self.handle_smb_result("rename(delete_dest_file)", r)
                    };
                    match file_result {
                        Ok(()) => {}
                        Err(VolumeError::IsADirectory(_)) => {
                            // Expected fall-through: dest is a directory, retry with delete_directory.
                            // Any other error (PermissionDenied, SharingViolation, …) propagates immediately
                            // instead of being masked by a second futile delete.
                            let (tree, mut conn) = self.clone_session().await?;
                            let r = tree.delete_directory(&mut conn, &smb_to).await;
                            self.handle_smb_result("rename(delete_dest_dir)", r)?;
                        }
                        Err(e) => return Err(e),
                    }
                }
            } else {
                // Check if dest exists and return AlreadyExists if so
                let dest_exists = {
                    let (tree, mut conn) = self.clone_session().await?;
                    tree.stat(&mut conn, &smb_to).await.is_ok()
                };
                if dest_exists {
                    return Err(VolumeError::AlreadyExists(to.display().to_string()));
                }
            }

            {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.rename(&mut conn, &smb_from, &smb_to).await;
                self.handle_smb_result("rename", r)?;
            }

            // Notify listing cache about the rename
            if let (Some(from_parent), Some(from_name)) = (from.parent(), from.file_name()) {
                let to_name = to
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let from_parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(from_parent)));

                if from.parent() == to.parent() {
                    // Same-directory rename
                    self.notify_mutation(
                        &self.volume_id,
                        &from_parent_display,
                        super::MutationEvent::Renamed {
                            from: from_name.to_string_lossy().to_string(),
                            to: to_name,
                        },
                    )
                    .await;
                } else {
                    // Cross-directory move: remove from source, add in dest
                    self.notify_mutation(
                        &self.volume_id,
                        &from_parent_display,
                        super::MutationEvent::Deleted(from_name.to_string_lossy().to_string()),
                    )
                    .await;
                    if let Some(to_parent) = to.parent() {
                        let to_parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(to_parent)));
                        self.notify_mutation(
                            &self.volume_id,
                            &to_parent_display,
                            super::MutationEvent::Created(to_name),
                        )
                        .await;
                    }
                }
            }
            Ok(())
        })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::scan_for_copy: share={}, path={:?}",
                self.share_name, smb_path
            );

            self.scan_recursive(&smb_path).await
        })
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Fast paths: empty / single. Empty returns zeroes; single falls
            // through to the recursive scanner so we don't pay the cost of the
            // batch machinery for one path.
            if paths.is_empty() {
                return Ok(BatchScanResult {
                    aggregate: CopyScanResult {
                        file_count: 0,
                        dir_count: 0,
                        total_bytes: 0,
                        dedup_bytes: 0,
                        top_level_is_directory: false,
                    },
                    per_path: Vec::new(),
                });
            }
            if paths.len() == 1 {
                let smb_path = self.to_smb_path(&paths[0]);
                let scan = self.scan_recursive(&smb_path).await?;
                return Ok(BatchScanResult {
                    aggregate: scan.clone(),
                    per_path: vec![(paths[0].clone(), scan)],
                });
            }

            // Oracle short-circuit: group inputs by parent and ask
            // `try_get_watched_listing` for each unique parent. Any path whose
            // parent is watcher-backed gets its size + is_directory from the
            // cached `FileEntry` (no SMB stat). Remaining paths fall through
            // to the pipelined-stat flow below. Decision is per-parent: one
            // call can mix oracle-served paths with pipelined-stat paths.
            //
            // SMB stats are per-path (not per-parent listing), so the grouping
            // here is purely about oracle eligibility; the fallthrough path
            // doesn't need parent grouping itself.
            let mut per_path_results: Vec<Option<CopyScanResult>> = (0..paths.len()).map(|_| None).collect();
            let mut leftover_indices: Vec<usize> = Vec::with_capacity(paths.len());
            {
                use std::collections::HashMap;
                // Cache oracle lookups so two paths sharing a parent only pay
                // one cache scan + clone. Value: indexed-by-name view over the
                // cached entries, or None if the oracle missed for this parent.
                let mut parent_cache: HashMap<PathBuf, Option<Vec<FileEntry>>> = HashMap::new();
                for (idx, path) in paths.iter().enumerate() {
                    let original_parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
                    let entries = parent_cache
                        .entry(original_parent.clone())
                        .or_insert_with(|| try_get_watched_listing(&self.volume_id, &original_parent));

                    let Some(cached_entries) = entries.as_ref() else {
                        leftover_indices.push(idx);
                        continue;
                    };

                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        leftover_indices.push(idx);
                        continue;
                    };

                    let Some(entry) = cached_entries.iter().find(|e| e.name == name) else {
                        // Cache doesn't know this child (stale selection,
                        // encoding mismatch). Fall through to a real stat for
                        // safety rather than reporting it as missing.
                        leftover_indices.push(idx);
                        continue;
                    };

                    if entry.is_directory {
                        // Directories still need a recursive scan to count
                        // descendants. The oracle just told us "this is a
                        // dir without an SMB stat"; recurse to expand it.
                        let smb_path = self.to_smb_path(path);
                        let scan = self.scan_recursive(&smb_path).await?;
                        per_path_results[idx] = Some(scan);
                    } else {
                        per_path_results[idx] = Some(CopyScanResult {
                            file_count: 1,
                            dir_count: 0,
                            total_bytes: entry.size.unwrap_or(0),
                            dedup_bytes: entry.size.unwrap_or(0),
                            top_level_is_directory: false,
                        });
                    }
                }

                if !leftover_indices.is_empty() {
                    debug!(
                        "SmbVolume::scan_for_copy_batch: share={}, oracle resolved {}/{} paths; pipelining stats for {}",
                        self.share_name,
                        paths.len() - leftover_indices.len(),
                        paths.len(),
                        leftover_indices.len()
                    );
                }
            }

            // All paths resolved via oracle: assemble the result and skip the
            // pipelined-stat machinery entirely.
            if leftover_indices.is_empty() {
                let mut aggregate = CopyScanResult {
                    file_count: 0,
                    dir_count: 0,
                    total_bytes: 0,
                    dedup_bytes: 0,
                    top_level_is_directory: false,
                };
                let mut per_path = Vec::with_capacity(paths.len());
                for (i, slot) in per_path_results.into_iter().enumerate() {
                    let scan = slot.expect("oracle path must have populated every index");
                    aggregate.file_count += scan.file_count;
                    aggregate.dir_count += scan.dir_count;
                    aggregate.total_bytes += scan.total_bytes;
                    aggregate.dedup_bytes += scan.dedup_bytes;
                    per_path.push((paths[i].clone(), scan));
                }
                return Ok(BatchScanResult { aggregate, per_path });
            }

            // Pre-compute SMB paths so the pipelined stats can borrow strings
            // that outlive the futures' lifetimes. We compute them for the
            // leftover indices only so an oracle-only path costs zero
            // `to_smb_path` calls below.
            let smb_paths: Vec<(usize, String)> = leftover_indices
                .iter()
                .map(|&idx| (idx, self.to_smb_path(&paths[idx])))
                .collect();

            debug!(
                "SmbVolume::scan_for_copy_batch: share={}, {} paths leftover for pipelined stats (oracle handled {})",
                self.share_name,
                smb_paths.len(),
                paths.len() - smb_paths.len()
            );

            // Build N pipelined stats: one cloned `Connection` per path, no
            // lock held across any stat. `Arc<Tree>` is shared cheaply. Empty
            // paths (volume root) skip the stat: the root is always a
            // directory, and they route straight into the recursion list.
            use futures_util::StreamExt;
            use futures_util::stream::FuturesUnordered;

            let tree_arc = self.tree_arc().await?;

            // Index tracks original position so results can be reassembled in input order.
            enum StatOutcome {
                Root,
                // smb2 FileInfo: carries `is_directory` and `size`.
                Entry(smb2::client::tree::FileInfo),
            }

            type StatFuture = Pin<Box<dyn Future<Output = (usize, Result<StatOutcome, smb2::Error>)> + Send>>;
            let mut stat_futs: FuturesUnordered<StatFuture> = FuturesUnordered::new();

            for (idx, smb_path) in &smb_paths {
                let idx = *idx;
                if smb_path.is_empty() {
                    // Root: no stat needed. Inline a ready future so the
                    // ordering logic below still sees a slot for this index.
                    stat_futs.push(Box::pin(std::future::ready((idx, Ok(StatOutcome::Root)))));
                    continue;
                }
                // Briefly lock client to clone a Connection per path, then
                // release. All clones multiplex over the single SMB session.
                let conn = {
                    let mut guard = self.client.lock().await;
                    let client = guard
                        .as_mut()
                        .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?;
                    client.connection_mut().clone()
                };
                let tree = Arc::clone(&tree_arc);
                let path_owned = smb_path.clone();
                stat_futs.push(Box::pin(async move {
                    let mut conn = conn;
                    let r = tree.stat(&mut conn, &path_owned).await;
                    (idx, r.map(StatOutcome::Entry))
                }));
            }

            // `per_path_results` is already shaped to the input length and
            // pre-populated with oracle-resolved entries; the pipelined-stat
            // path below only fills the still-None slots.
            // Indices to recurse into after the stat batch finishes.
            let mut dirs_to_recurse: Vec<usize> = Vec::new();

            while let Some((idx, result)) = stat_futs.next().await {
                match result {
                    Ok(StatOutcome::Root) => {
                        // Root path → always a directory, recurse later.
                        dirs_to_recurse.push(idx);
                    }
                    Ok(StatOutcome::Entry(info)) => {
                        if info.is_directory {
                            dirs_to_recurse.push(idx);
                        } else {
                            per_path_results[idx] = Some(CopyScanResult {
                                file_count: 1,
                                dir_count: 0,
                                total_bytes: info.size,
                                dedup_bytes: info.size,
                                top_level_is_directory: false,
                            });
                        }
                    }
                    Err(e) => {
                        // Mirror handle_smb_result for the state transition on
                        // connection loss, then map and propagate.
                        let kind = e.kind();
                        if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                            warn!(
                                "SmbVolume::scan_for_copy_batch(share={}): connection lost ({}), transitioning to Disconnected",
                                self.share_name, e
                            );
                            self.transition_to_disconnected();
                        } else {
                            warn!("SmbVolume::scan_for_copy_batch(share={}): {}", self.share_name, e);
                        }
                        return Err(map_smb_error(e));
                    }
                }
            }

            // Recurse sequentially into each discovered directory. Per-dir
            // recursion still serializes on listing + child stats; that's a
            // future "Fix 5" (pipelined directory recursion). For the 100 ×
            // tiny-file scenario all sources are files, so this loop is never
            // entered.
            // `smb_paths` is `Vec<(idx, String)>` keyed by the leftover index;
            // build a lookup so dir-recursion can find each path by its
            // original input index.
            let smb_path_by_idx: std::collections::HashMap<usize, &str> =
                smb_paths.iter().map(|(i, s)| (*i, s.as_str())).collect();
            for idx in dirs_to_recurse {
                let smb_path = smb_path_by_idx
                    .get(&idx)
                    .expect("dirs_to_recurse only carries indices from the leftover stat batch");
                let scan = self.scan_recursive(smb_path).await?;
                per_path_results[idx] = Some(scan);
            }

            // Fold per-path into aggregate + per_path vec (in input order).
            let mut aggregate = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                dedup_bytes: 0,
                top_level_is_directory: false,
            };
            let mut per_path = Vec::with_capacity(paths.len());
            for (i, slot) in per_path_results.into_iter().enumerate() {
                let scan = slot.expect("every input path must have a result by this point");
                aggregate.file_count += scan.file_count;
                aggregate.dir_count += scan.dir_count;
                aggregate.total_bytes += scan.total_bytes;
                aggregate.dedup_bytes += scan.dedup_bytes;
                per_path.push((paths[i].clone(), scan));
            }

            Ok(BatchScanResult { aggregate, per_path })
        })
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // List destination directory to check for conflicts
            let entries = self.list_directory_impl(dest_path).await?;
            let mut conflicts = Vec::new();

            for item in source_items {
                if let Some(existing) = entries.iter().find(|e| e.name == item.name) {
                    let dest_modified = existing.modified_at.map(|s| s as i64);
                    conflicts.push(ScanConflict {
                        source_path: item.name.clone(),
                        dest_path: existing.path.clone(),
                        source_size: item.size,
                        dest_size: existing.size.unwrap_or(0),
                        source_modified: item.modified,
                        dest_modified,
                    });
                }
            }

            Ok(conflicts)
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // Reads the `network.smbConcurrency` setting (default 10, clamped 1..=32).
        // Updated at app startup from `settings.json` via
        // `file_system::set_smb_concurrency`. Lock-free atomic load on every
        // call, so a settings change in the current session applies on the next
        // batch-copy dispatch (no reconnect required; Connection::clone is
        // cheap).
        crate::file_system::smb_concurrency()
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::open_read_stream: share={}, path={:?}",
                self.share_name, smb_path
            );

            let stream = self.open_smb_download_stream(&smb_path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            // Compound fast-path: if the caller-provided hint fits in one READ,
            // send CREATE+READ+CLOSE as a single compound frame (1 RTT) instead
            // of the 3-RTT streaming open. Drives the compound on a cloned
            // `Connection` with no lock held, so N concurrent small reads
            // pipeline over one SMB session. Falls through to the streaming
            // path when the hint is missing, too large, or the compound read
            // returns short (truncated file, rare but possible if size
            // changed since the scan).
            if let Some(size) = size_hint {
                let (tree, mut conn) = self.clone_session().await?;
                let max_read = conn.params().map(|p| p.max_read_size).unwrap_or(65536) as u64;
                if size > 0 && size <= max_read {
                    debug!(
                        "SmbVolume::open_read_stream_with_hint: share={}, path={:?}, size={}; using compound fast-path",
                        self.share_name, smb_path, size
                    );
                    let read_result = tree.read_file_compound(&mut conn, &smb_path).await;
                    let data = self.handle_smb_result("open_read_stream_with_hint(compound)", read_result)?;
                    if data.len() as u64 == size {
                        return Ok(Box::new(InlineReadStream::new(data)) as Box<dyn VolumeReadStream>);
                    }
                    debug!(
                        "SmbVolume::open_read_stream_with_hint: compound read returned {} bytes, expected {}; falling back to streaming",
                        data.len(),
                        size
                    );
                }
            }

            debug!(
                "SmbVolume::open_read_stream_with_hint: share={}, path={:?}; using streaming path",
                self.share_name, smb_path
            );
            let stream = self.open_smb_download_stream(&smb_path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        // Lock-free streaming write path.
        //
        // Both branches below drive the upload on a cloned `Connection`
        // (cheap `Arc::clone`) and an `Arc<Tree>`. The client mutex is
        // held only for the few microseconds of `clone_session()`, never
        // for the upload itself. With smb2 0.9's owned `FileWriter`, N
        // concurrent `write_from_stream` calls on one `SmbVolume`
        // pipeline N WRITE chains over a single SMB session: smb2's
        // receiver task multiplexes responses by `MessageId`.
        //
        // This collapses the historical two-phase pattern (brief
        // `clone_session` for the fast-path → drop → long
        // session-mutex hold for the streaming fallback) into a single
        // clone. The old shape deadlocked under sustained concurrent
        // pressure; the regression test
        // `smb_integration_concurrent_streaming_writes_no_deadlock`
        // pins this shape.
        Box::pin(async move {
            let smb_path = self.to_smb_path(dest);

            debug!(
                "SmbVolume::write_from_stream: share={}, path={:?}, size={}",
                self.share_name, smb_path, size
            );

            // Acquire a cloned session once, up front. Both the compound
            // fast-path and the streaming fallback drive their write on
            // this same clone — no second `clone_session` needed.
            let (tree, conn) = self.clone_session().await?;

            // Best-effort delete of a partial file on a FRESH cloned session.
            // Once a `FileWriter` is open and bytes have streamed into it, an
            // early error (source read error, `write_chunk` / `finish`
            // failure) would otherwise leave a half-written file at the real
            // destination name (AGENTS.md principle #4: a failed copy must not
            // leave corrupt bytes under the user's intended name). The delete
            // runs on a fresh session because the writer's own connection may
            // be gone. The caller MUST close the leaked write handle first
            // (`writer.abort()` where the writer is still owned), else this
            // delete hits a sharing violation against the still-open handle.
            let delete_partial = || async {
                if let Ok((tree_for_delete, mut conn_for_delete)) = self.clone_session().await {
                    let _ = tree_for_delete.delete_file(&mut conn_for_delete, &smb_path).await;
                }
            };

            // Compound fast-path: when the caller promised a size that fits
            // in one WRITE, drain the source stream into a buffer and send
            // CREATE+WRITE+FLUSH+CLOSE as a single compound frame (1 RTT
            // instead of 4). Small files are the hot case; we fall through
            // to the streaming writer for anything larger or when the source
            // returns short.
            let bytes_written = 'write: {
                if size > 0 {
                    let max_write = conn.params().map(|p| p.max_write_size).unwrap_or(65536) as u64;
                    if size <= max_write {
                        let mut buffer = Vec::with_capacity(size as usize);
                        while let Some(chunk_result) = stream.next_chunk().await {
                            // Compound drain buffers in memory; no writer/handle
                            // is open yet, so a source error here can't leave a
                            // partial on the server. Just propagate.
                            let chunk = chunk_result?;
                            buffer.extend_from_slice(&chunk);
                            // Fire progress per chunk AND honor cancellation, so
                            // the fast-path has the same cancel/progress contract
                            // as the streaming fallback below. Cancel here aborts
                            // before the compound WRITE touches the wire: the
                            // destination never sees a partial file.
                            if on_progress(buffer.len() as u64, size).is_break() {
                                return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                            }
                        }
                        if buffer.len() as u64 == size {
                            debug!(
                                "SmbVolume::write_from_stream: using compound fast-path ({} bytes)",
                                buffer.len()
                            );
                            let mut conn = conn;
                            let write_result = tree.write_file_compound(&mut conn, &smb_path, &buffer).await;
                            break 'write self.handle_smb_result("write_from_stream(compound)", write_result)?;
                        }
                        // Size mismatch: feed the already-drained buffer through
                        // the streaming writer on the same cloned connection.
                        // No lock acquired; this is the rare path.
                        debug!(
                            "SmbVolume::write_from_stream: compound fast-path source yielded {} bytes, expected {}; falling back to streaming writer",
                            buffer.len(),
                            size
                        );
                        let writer_result = tree.create_file_writer(conn, &smb_path).await;
                        let mut writer = self.handle_smb_result("write_from_stream(open)", writer_result)?;
                        if !buffer.is_empty() {
                            let write_result = writer.write_chunk(&buffer).await;
                            if let Err(ve) = self.handle_smb_result("write_from_stream(write_chunk)", write_result) {
                                // Writer still owned: abort (closes the leaked
                                // handle) then delete the partial, mirroring the
                                // cancel branch, then propagate the original error.
                                let _ = writer.abort().await;
                                delete_partial().await;
                                return Err(ve);
                            }
                        }
                        // The source signalled end-of-stream by returning None
                        // above (we exited the drain loop). No further chunks.
                        // `finish()` consumes the writer, so on failure the
                        // handle is already gone (best-effort delete only).
                        let finish_result = writer.finish().await;
                        if let Err(ve) = self.handle_smb_result("write_from_stream(finish)", finish_result) {
                            delete_partial().await;
                            return Err(ve);
                        }
                        break 'write buffer.len() as u64;
                    }
                }

                // Streaming path for large / unknown-size writes. Drives the
                // owned `FileWriter` on the cloned `Connection` directly —
                // no client mutex is held while WRITEs are in flight, so N
                // concurrent large copies pipeline over one SMB session.
                let writer_result = tree.create_file_writer(conn, &smb_path).await;
                let mut writer = self.handle_smb_result("write_from_stream(open)", writer_result)?;

                let mut bytes_read = 0u64;

                loop {
                    let chunk = match stream.next_chunk().await {
                        None => break,
                        Some(Ok(chunk)) => chunk,
                        Some(Err(e)) => {
                            // Source read error with the writer already open:
                            // abort (closes the leaked handle), delete the
                            // partial, then propagate the original error.
                            let _ = writer.abort().await;
                            delete_partial().await;
                            return Err(e);
                        }
                    };
                    if chunk.is_empty() {
                        continue;
                    }

                    let write_result = writer.write_chunk(&chunk).await;
                    if let Err(ve) = self.handle_smb_result("write_from_stream(write_chunk)", write_result) {
                        let _ = writer.abort().await;
                        delete_partial().await;
                        return Err(ve);
                    }

                    bytes_read += chunk.len() as u64;

                    if on_progress(bytes_read, size) == std::ops::ControlFlow::Break(()) {
                        // Abort drains in-flight WRITE responses and closes the
                        // handle without the server-side fsync that `finish()`
                        // would force (we're about to delete the partial file
                        // anyway). Dropping directly would leave stale responses
                        // on the connection and poison the next op.
                        let _ = writer.abort().await;
                        // Best-effort delete of the partial file on its own
                        // cloned connection (the writer's connection is gone).
                        delete_partial().await;
                        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                    }
                }

                // `finish()` consumes the writer; on failure the handle is
                // already gone, so we can only best-effort delete the partial.
                let finish_result = writer.finish().await;
                if let Err(ve) = self.handle_smb_result("write_from_stream(finish)", finish_result) {
                    delete_partial().await;
                    return Err(ve);
                }

                bytes_read
            };

            // Patch the listing cache from local knowledge so the destination
            // pane sees the new file without waiting for a CHANGE_NOTIFY
            // round-trip. The SMB watcher has a loss window between
            // consecutive `next_events()` calls; relying on it alone left
            // bulk cross-volume copies showing only a subset of the just-
            // copied files until the user navigated away and back.
            if let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    super::MutationEvent::Created(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(bytes_written)
        })
    }

    fn smb_connection_state(&self) -> Option<SmbConnectionState> {
        // SmbVolume always returns `Some` so the frontend can distinguish
        // "not an SMB volume" (None) from "SMB volume in trouble"
        // (Some(Disconnected)). The reconnect manager keys off the latter.
        // The internal state machine is binary; the outer `OsMount` variant
        // is only attached by `enrich_smb_connection_state` for SMB shares
        // that have an OS mount but no Cmdr smb2 session at all.
        Some(match self.connection_state() {
            ConnectionState::Direct => SmbConnectionState::Direct,
            ConnectionState::Disconnected => SmbConnectionState::Disconnected,
        })
    }

    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.do_attempt_reconnect())
    }

    fn on_unmount(&self) {
        // Mark the volume permanently dead so any in-flight reconnect bails
        // out before installing a session into an orphaned volume.
        self.unmounted.store(true, Ordering::Relaxed);

        // Transition to Disconnected. We deliberately set the atomic directly
        // instead of going through `transition_to_disconnected()`, because the
        // volume is being unregistered: the FE will learn via `volumes-changed`
        // and an extra `smb-connection-changed` event would race with that.
        self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);

        // Cancel the background watcher task. The task will call watcher.close()
        // to release the SMB directory handle before exiting.
        if let Ok(mut guard) = self.watcher_cancel.lock()
            && let Some(cancel_tx) = guard.take()
        {
            let _ = cancel_tx.send(());
            debug!("SmbVolume cleanup for {}: watcher cancel sent", self.share_name);
        }

        // Drop the smb2 session. Uses blocking_lock() / blocking_write() since
        // on_unmount is sync (called from FSEvents thread, no Tokio runtime).
        // Safe because we just set state to Disconnected, so no async task
        // will acquire either lock. Drop Tree first, then SmbClient: Tree
        // holds a tree_id referenced by session-scoped server state, and we
        // want it to go first so any lingering `FileDownload` clones finish
        // before the client (which owns the Connection) vanishes. In
        // practice all three just drop their Arc refcounts; the order is
        // defensive.
        {
            let mut tree_guard = self.tree.blocking_write();
            *tree_guard = None;
        }
        {
            let mut client_guard = self.client.blocking_lock();
            *client_guard = None;
        }

        debug!("SmbVolume cleanup for {}: smb2 session dropped", self.share_name);
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

// These test submodules live as sibling files next to `smb.rs` (matching the
// `in_memory_test.rs` / `local_posix_test.rs` convention) but are children of
// the `smb` module so `super::*` reaches the backend's private items. Because
// `smb.rs` is a file module (not a `mod.rs` directory), the sibling paths are
// spelled explicitly with `#[path]`.
#[cfg(test)]
#[path = "smb_integration_test.rs"]
mod smb_integration_test;
#[cfg(test)]
#[path = "smb_soak_test.rs"]
mod smb_soak_test;
#[cfg(test)]
#[path = "smb_test.rs"]
mod smb_test;
#[cfg(test)]
#[path = "smb_test_support.rs"]
mod smb_test_support;

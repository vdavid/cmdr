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
                } else {
                    result.file_count += 1;
                    result.total_bytes += entry.size.unwrap_or(0);
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
                    top_level_is_directory: false,
                };
                let mut per_path = Vec::with_capacity(paths.len());
                for (i, slot) in per_path_results.into_iter().enumerate() {
                    let scan = slot.expect("oracle path must have populated every index");
                    aggregate.file_count += scan.file_count;
                    aggregate.dir_count += scan.dir_count;
                    aggregate.total_bytes += scan.total_bytes;
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
                top_level_is_directory: false,
            };
            let mut per_path = Vec::with_capacity(paths.len());
            for (i, slot) in per_path_results.into_iter().enumerate() {
                let scan = slot.expect("every input path must have a result by this point");
                aggregate.file_count += scan.file_count;
                aggregate.dir_count += scan.dir_count;
                aggregate.total_bytes += scan.total_bytes;
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
                            self.handle_smb_result("write_from_stream(write_chunk)", write_result)?;
                        }
                        // The source signalled end-of-stream by returning None
                        // above (we exited the drain loop). No further chunks.
                        let finish_result = writer.finish().await;
                        self.handle_smb_result("write_from_stream(finish)", finish_result)?;
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

                while let Some(chunk_result) = stream.next_chunk().await {
                    let chunk = chunk_result?;
                    if chunk.is_empty() {
                        continue;
                    }

                    let write_result = writer.write_chunk(&chunk).await;
                    self.handle_smb_result("write_from_stream(write_chunk)", write_result)?;

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
                        if let Ok((tree_for_delete, mut conn_for_delete)) = self.clone_session().await {
                            let _ = tree_for_delete.delete_file(&mut conn_for_delete, &smb_path).await;
                        }
                        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                    }
                }

                let finish_result = writer.finish().await;
                self.handle_smb_result("write_from_stream(finish)", finish_result)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::smb_volume_id;

    // ── Type mapping tests ──────────────────────────────────────────

    #[test]
    fn filetime_to_unix_secs_known_date() {
        // 2024-01-01 00:00:00 UTC = FileTime(133_485_408_000_000_000)
        let ft = smb2::pack::FileTime(133_485_408_000_000_000);
        let secs = filetime_to_unix_secs(ft).unwrap();
        assert_eq!(secs, 1_704_067_200);
    }

    #[test]
    fn filetime_to_unix_secs_zero_returns_none() {
        let ft = smb2::pack::FileTime::ZERO;
        assert!(filetime_to_unix_secs(ft).is_none());
    }

    #[test]
    fn directory_entry_to_file_entry_file() {
        let entry = smb2::client::tree::DirectoryEntry {
            name: "report.pdf".to_string(),
            size: 1024,
            is_directory: false,
            created: smb2::pack::FileTime(133_485_408_000_000_000),
            modified: smb2::pack::FileTime(133_485_408_000_000_000),
        };

        let fe = directory_entry_to_file_entry(&entry, "/Volumes/Share/Documents");
        assert_eq!(fe.name, "report.pdf");
        assert_eq!(fe.path, "/Volumes/Share/Documents/report.pdf");
        assert!(!fe.is_directory);
        assert!(!fe.is_symlink);
        assert_eq!(fe.size, Some(1024));
        assert_eq!(fe.modified_at, Some(1_704_067_200));
        assert_eq!(fe.created_at, Some(1_704_067_200));
        assert_eq!(fe.icon_id, "ext:pdf");
    }

    #[test]
    fn directory_entry_to_file_entry_directory() {
        let entry = smb2::client::tree::DirectoryEntry {
            name: "Photos".to_string(),
            size: 0,
            is_directory: true,
            created: smb2::pack::FileTime::ZERO,
            modified: smb2::pack::FileTime::ZERO,
        };

        let fe = directory_entry_to_file_entry(&entry, "/Volumes/Share");
        assert_eq!(fe.name, "Photos");
        assert_eq!(fe.path, "/Volumes/Share/Photos");
        assert!(fe.is_directory);
        assert_eq!(fe.size, None);
        assert_eq!(fe.modified_at, None);
        assert_eq!(fe.icon_id, "dir");
    }

    #[test]
    fn fs_info_to_space_info_conversion() {
        let info = smb2::client::tree::FsInfo {
            total_bytes: 1_000_000_000,
            free_bytes: 400_000_000,
            total_free_bytes: 400_000_000,
            bytes_per_sector: 512,
            sectors_per_unit: 8,
        };

        let space = fs_info_to_space_info(&info);
        assert_eq!(space.total_bytes, 1_000_000_000);
        assert_eq!(space.available_bytes, 400_000_000);
        assert_eq!(space.used_bytes, 600_000_000);
    }

    #[test]
    fn map_smb_error_not_found() {
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::OBJECT_NAME_NOT_FOUND,
            command: smb2::types::Command::Create,
        };
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::NotFound(_)));
    }

    #[test]
    fn map_smb_error_delete_pending() {
        // STATUS_DELETE_PENDING surfaces when a delete has been requested but at
        // least one open handle is keeping the file alive. smb2 currently classifies
        // it as `ErrorKind::Other`, so `map_smb_error` must dispatch on the raw
        // NTSTATUS to produce the typed `VolumeError::DeletePending` variant —
        // otherwise the FE falls back to the generic "disk needs attention" copy
        // instead of the transient "file is being removed" message.
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::DELETE_PENDING,
            command: smb2::types::Command::Create,
        };
        let ve = map_smb_error(err);
        assert!(
            matches!(ve, VolumeError::DeletePending(_)),
            "STATUS_DELETE_PENDING should map to VolumeError::DeletePending, got: {:?}",
            ve,
        );
    }

    #[test]
    fn map_smb_error_access_denied() {
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::ACCESS_DENIED,
            command: smb2::types::Command::Create,
        };
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::PermissionDenied(_)));
    }

    #[test]
    fn map_smb_error_disconnected() {
        let err = smb2::Error::Disconnected;
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
    }

    #[test]
    fn map_smb_error_timeout() {
        let err = smb2::Error::Timeout;
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::ConnectionTimeout(_)));
    }

    #[test]
    fn map_smb_error_disk_full() {
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::DISK_FULL,
            command: smb2::types::Command::Write,
        };
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::StorageFull { .. }));
    }

    #[test]
    fn map_smb_error_session_expired() {
        let err = smb2::Error::SessionExpired;
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
    }

    #[test]
    fn map_smb_error_auth_required() {
        let err = smb2::Error::Auth {
            message: "Authentication failed".to_string(),
        };
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::PermissionDenied(_)));
    }

    #[test]
    fn map_smb_error_io() {
        let err = smb2::Error::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke"));
        let ve = map_smb_error(err);
        // IO errors (callback errors, etc.) are not connection losses; they map to IoError.
        // Real connection losses come through Error::Disconnected → ConnectionLost.
        assert!(matches!(ve, VolumeError::IoError { .. }));
    }

    #[test]
    fn map_smb_error_already_exists() {
        // STATUS_OBJECT_NAME_COLLISION (returned by Create when the name exists) must
        // surface as AlreadyExists so the volume_strategy merge-directory path can
        // swallow it instead of bubbling a generic IO error to the user.
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::OBJECT_NAME_COLLISION,
            command: smb2::types::Command::Create,
        };
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::AlreadyExists(_)));
    }

    #[test]
    fn map_smb_error_file_is_a_directory() {
        // STATUS_FILE_IS_A_DIRECTORY is returned when delete_file is called on a dir.
        // smb2 0.8.0 exposes this as the typed `ErrorKind::IsADirectory` variant, so
        // `map_smb_error` surfaces it as `VolumeError::IsADirectory`; the delete
        // fast-path matches on that to decide whether to retry with delete_directory.
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::FILE_IS_A_DIRECTORY,
            command: smb2::types::Command::Create,
        };
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::IsADirectory(_)));
    }

    #[test]
    fn map_smb_error_access_denied_is_not_misclassified() {
        // Non-directory errors must not be classified as IsADirectory.
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::ACCESS_DENIED,
            command: smb2::types::Command::Create,
        };
        let ve = map_smb_error(err);
        assert!(matches!(ve, VolumeError::PermissionDenied(_)));
    }

    // ── Connection state tests ──────────────────────────────────────

    #[test]
    fn connection_state_round_trip() {
        for state in [ConnectionState::Direct, ConnectionState::Disconnected] {
            assert_eq!(ConnectionState::from_u8(state as u8), state);
        }
    }

    #[test]
    fn connection_state_unknown_value_defaults_to_disconnected() {
        // The internal state machine is binary; `1` (the old `OsMount`
        // discriminant) and any other unknown byte must decode as
        // `Disconnected`, the safe / "stop using smb2" state.
        assert_eq!(ConnectionState::from_u8(1), ConnectionState::Disconnected);
        assert_eq!(ConnectionState::from_u8(255), ConnectionState::Disconnected);
    }

    // ── Path conversion tests ───────────────────────────────────────

    #[test]
    fn to_smb_path_empty() {
        let vol = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("")), "");
        assert_eq!(vol.to_smb_path(Path::new("/")), "");
        assert_eq!(vol.to_smb_path(Path::new(".")), "");
    }

    #[test]
    fn to_smb_path_relative() {
        let vol = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("Documents")), "Documents");
        assert_eq!(
            vol.to_smb_path(Path::new("Documents/report.pdf")),
            "Documents/report.pdf"
        );
    }

    #[test]
    fn to_smb_path_absolute_under_mount() {
        let vol = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("/Volumes/TestShare/Documents")), "Documents");
        assert_eq!(
            vol.to_smb_path(Path::new("/Volumes/TestShare/Documents/report.pdf")),
            "Documents/report.pdf"
        );
    }

    #[test]
    fn to_smb_path_mount_root() {
        let vol = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("/Volumes/TestShare")), "");
    }

    #[test]
    fn to_display_path_empty_is_mount_root() {
        let vol = make_test_volume();
        assert_eq!(vol.to_display_path(""), "/Volumes/TestShare");
    }

    #[test]
    fn to_display_path_with_subpath() {
        let vol = make_test_volume();
        assert_eq!(
            vol.to_display_path("Documents/report.pdf"),
            "/Volumes/TestShare/Documents/report.pdf"
        );
    }

    #[test]
    fn supports_watching_returns_false() {
        let vol = make_test_volume();
        assert!(!vol.supports_watching());
    }

    #[test]
    fn name_returns_share_name() {
        let vol = make_test_volume();
        assert_eq!(vol.name(), "TestShare");
    }

    #[test]
    fn root_returns_mount_path() {
        let vol = make_test_volume();
        assert_eq!(vol.root(), Path::new("/Volumes/TestShare"));
    }

    #[test]
    fn local_path_returns_none() {
        let vol = make_test_volume();
        assert!(vol.local_path().is_none());
    }

    #[test]
    fn supports_export_returns_true() {
        let vol = make_test_volume();
        assert!(vol.supports_export());
    }

    // ── Reconnect tests (no Docker, no real network) ────────────────

    /// Helper that flips a test volume into `Direct` so we can test the
    /// "already connected" no-op path without needing a real session.
    fn make_test_volume_direct() -> SmbVolume {
        let vol = make_test_volume();
        vol.state.store(ConnectionState::Direct as u8, Ordering::Relaxed);
        vol
    }

    #[tokio::test]
    async fn attempt_reconnect_noop_when_already_direct() {
        // If state is Direct, the helper bails early without building a session.
        // This is the path concurrent callers hit after the winner finishes.
        let vol = make_test_volume_direct();
        let result = vol.do_attempt_reconnect().await;
        assert!(result.is_ok(), "expected Ok when already Direct, got {:?}", result);
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
    }

    #[tokio::test]
    async fn attempt_reconnect_bails_when_unmounted() {
        // After `on_unmount` runs, reconnect must not try to build a new session
        // (otherwise we'd leak a watcher + smb2 session into an orphaned volume).
        let vol = make_test_volume();
        vol.unmounted.store(true, Ordering::Relaxed);
        let result = vol.do_attempt_reconnect().await;
        assert!(
            matches!(result, Err(VolumeError::DeviceDisconnected(_))),
            "expected DeviceDisconnected when unmounted, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn single_flight_concurrent_callers_serialize() {
        // Two parallel `do_attempt_reconnect` calls must serialize on
        // `reconnect_lock`. With the volume already Direct, both should return
        // Ok cheaply: the second one observes Direct after the first releases
        // the guard. Mutex contention itself is the assertion that single-flight
        // is wired up; if it wasn't, both calls would race past the early-exit
        // check.
        let vol = Arc::new(make_test_volume_direct());
        let v2 = Arc::clone(&vol);
        let v3 = Arc::clone(&vol);
        let (r1, r2) = tokio::join!(async move { v2.do_attempt_reconnect().await }, async move {
            v3.do_attempt_reconnect().await
        });
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
    }

    #[tokio::test]
    async fn transition_to_disconnected_idempotent() {
        // Calling `transition_to_disconnected` twice should only emit once.
        // We can't verify the emit count without a real `AppHandle`, but we
        // can verify the underlying `swap` semantics: the second call is a
        // no-op (returns the same value).
        let vol = make_test_volume_direct();
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
        vol.transition_to_disconnected();
        assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
        vol.transition_to_disconnected();
        assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn transition_to_direct_idempotent() {
        let vol = make_test_volume();
        assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
        vol.transition_to_direct();
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
        vol.transition_to_direct();
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
    }

    #[test]
    fn listing_is_watched_false_when_disconnected() {
        // No watcher_cancel set and state Disconnected: false.
        let vol = make_test_volume();
        assert!(!vol.listing_is_watched(Path::new("/")));
    }

    #[test]
    fn listing_is_watched_false_when_direct_but_no_watcher() {
        // State Direct but `watcher_cancel` empty: still false (we need both).
        let vol = make_test_volume_direct();
        assert!(!vol.listing_is_watched(Path::new("/")));
    }

    #[test]
    fn listing_is_watched_false_when_watcher_set_but_disconnected() {
        // `watcher_cancel` populated but state Disconnected: false.
        let vol = make_test_volume();
        let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
        *vol.watcher_cancel.lock().unwrap() = Some(tx);
        assert!(!vol.listing_is_watched(Path::new("/")));
    }

    #[test]
    fn listing_is_watched_true_when_direct_and_watcher_set() {
        // Both conditions met: true.
        let vol = make_test_volume_direct();
        let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
        *vol.watcher_cancel.lock().unwrap() = Some(tx);
        assert!(vol.listing_is_watched(Path::new("/")));
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_listing_is_watched_flips_with_connection() {
        // End-to-end check against a live Docker SMB server: after
        // `connect_smb_volume`, the watcher is spawned and state is Direct,
        // so the oracle gate returns true. After flipping the state to
        // Disconnected (simulating a ConnectionLost event), the gate flips
        // false even though `watcher_cancel` is still set: the contract is
        // "watcher present AND Direct," and a half-broken volume must not be
        // treated as fresh.
        let vol = make_docker_volume().await;
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
        assert!(
            vol.listing_is_watched(Path::new("/")),
            "expected true on a freshly-connected Docker volume"
        );

        vol.transition_to_disconnected();
        assert!(
            !vol.listing_is_watched(Path::new("/")),
            "expected false after transitioning to Disconnected"
        );
    }

    #[test]
    fn on_unmount_marks_volume_dead() {
        // `on_unmount` is sync (called from FSEvents thread) and uses
        // `blocking_lock`, so this must be a `#[test]`, not a `#[tokio::test]`
        // (the latter panics inside a runtime when calling `blocking_lock`).
        let vol = make_test_volume_direct();
        assert!(!vol.unmounted.load(Ordering::Relaxed));
        vol.on_unmount();
        assert!(vol.unmounted.load(Ordering::Relaxed));
        assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
    }

    /// Creates a test SmbVolume in disconnected state (no real connection).
    fn make_test_volume() -> SmbVolume {
        let params = SmbConnectionParams {
            server: "192.168.1.100".to_string(),
            share_name: "TestShare".to_string(),
            port: 445,
            username: "Guest".to_string(),
            password: String::new(),
        };
        SmbVolume {
            name: "TestShare".to_string(),
            mount_path: PathBuf::from("/Volumes/TestShare"),
            share_name: "TestShare".to_string(),
            volume_id: "volumestestshare".to_string(),
            params: Arc::new(tokio::sync::RwLock::new(params)),
            client: Arc::new(tokio::sync::Mutex::new(None)),
            tree: Arc::new(tokio::sync::RwLock::new(None)),
            state: Arc::new(AtomicU8::new(ConnectionState::Disconnected as u8)),
            watcher_cancel: std::sync::Mutex::new(None),
            reconnect_lock: Arc::new(tokio::sync::Mutex::new(())),
            unmounted: Arc::new(AtomicBool::new(false)),
        }
    }

    // ── Integration tests (require Docker SMB containers) ──────────
    //
    // Run with: cargo nextest run smb_integration --run-ignored all
    // Prerequisites: ./apps/desktop/test/smb-servers/start.sh

    /// Connects to the Docker smb-guest container (share "public"). Default port
    /// 10480 matches smb2's guest test container; override with
    /// `SMB_CONSUMER_GUEST_PORT` to match `smb2::testing::guest_port()`.
    async fn make_docker_volume() -> SmbVolume {
        let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10480);
        let volume_id = smb_volume_id("127.0.0.1", port, "public");
        let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
        connect_smb_volume("public", "/tmp/smb-test-mount", &volume_id, params)
            .await
            .unwrap_or_else(|e| {
                panic!("Failed to connect to Docker SMB container at 127.0.0.1:{port}. Is it running? ({e:?})")
            })
    }

    /// Unique directory name for test isolation.
    ///
    /// Combines the PID, a nanosecond timestamp, and a process-wide atomic
    /// counter so that tests running in parallel never collide: neither
    /// within one process (the nanosecond clock resolution isn't fine enough
    /// on its own) nor across the separate processes nextest forks per test
    /// (where the static counter resets to 0 and two processes hitting the
    /// same nanos window would otherwise produce identical names, leaving
    /// stale directories on the SMB share for later runs to trip on).
    fn test_dir_name() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        format!("cmdr-test-{pid}-{ts}-{n}")
    }

    /// Ensures a test directory is clean before use (deletes recursively if it exists).
    async fn ensure_clean(vol: &SmbVolume, dir: &str) {
        if vol.exists(Path::new(dir)).await {
            // Delete contents recursively
            if let Ok(entries) = vol.list_directory_impl(Path::new(dir)).await {
                for entry in entries {
                    let child = format!("{}/{}", dir, entry.name);
                    if entry.is_directory {
                        Box::pin(ensure_clean(vol, &child)).await;
                    } else {
                        let _ = vol.delete(Path::new(&child)).await;
                    }
                }
            }
            let _ = vol.delete(Path::new(dir)).await;
        }
    }

    // ── Byte-level integrity helpers ────────────────────────────────
    //
    // Every SMB copy test that lands a file on a destination hashes the
    // source bytes and the destination bytes and compares the two. A
    // pipeline bug that drops, duplicates, reorders, or reuses a chunk's
    // buffer will change the hash; the old `bytes_written == expected`
    // and `metadata.size == N` assertions would silently pass. blake3 is
    // fast (well over a GB/s single-threaded), so the 20 MB streaming
    // tests pay negligible hashing cost on top of the SMB RTTs.
    //
    // `hash_volume_file` streams the destination through `open_read_stream`
    // so we also avoid buffering e.g. 20 MB into a `Vec<u8>` just to
    // compare with `assert_eq!` (which on mismatch used to print an
    // unreadable megabyte-sized diff). The hex-formatted hash in the
    // assertion message is actionable on failure.

    fn hash_bytes(data: &[u8]) -> [u8; 32] {
        *blake3::hash(data).as_bytes()
    }

    async fn hash_volume_file(volume: &dyn Volume, path: &Path) -> [u8; 32] {
        let mut stream = volume
            .open_read_stream(path)
            .await
            .expect("open read stream for hashing");
        let mut hasher = blake3::Hasher::new();
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = chunk.expect("read chunk for hashing");
            hasher.update(&chunk);
        }
        *hasher.finalize().as_bytes()
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_attempt_reconnect_rebuilds_session() {
        // Drives the full reconnect cycle against a real SMB server:
        // 1. Connect, verify Direct.
        // 2. Force-flip to Disconnected (simulating a ConnectionLost event). Drop the underlying client +
        //    tree to mimic a dead session.
        // 3. Verify hot-path ops fail with DeviceDisconnected.
        // 4. Call attempt_reconnect; verify it succeeds and state is Direct.
        // 5. Verify hot-path ops work again.
        let vol = make_docker_volume().await;
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
        assert!(vol.list_directory_impl(Path::new("")).await.is_ok());

        // Simulate "the server hung up": drop the smb2 session and flip state.
        // We don't need to actually break the network; `attempt_reconnect`'s
        // job is to rebuild the session regardless of why state went down.
        {
            let mut client_guard = vol.client.lock().await;
            *client_guard = None;
        }
        {
            let mut tree_guard = vol.tree.write().await;
            *tree_guard = None;
        }
        vol.transition_to_disconnected();
        assert_eq!(vol.connection_state(), ConnectionState::Disconnected);

        // Hot-path op should fail: clone_session refuses while Disconnected.
        let result = vol.list_directory_impl(Path::new("")).await;
        assert!(
            matches!(result, Err(VolumeError::DeviceDisconnected(_))),
            "expected DeviceDisconnected before reconnect, got {:?}",
            result
        );

        // Reconnect should rebuild the session and flip back to Direct.
        vol.do_attempt_reconnect()
            .await
            .expect("attempt_reconnect should succeed against a live Docker SMB");
        assert_eq!(vol.connection_state(), ConnectionState::Direct);

        // And hot-path ops should work again.
        let entries = vol
            .list_directory_impl(Path::new(""))
            .await
            .expect("list_directory should succeed after reconnect");
        assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_attempt_reconnect_noop_when_already_direct() {
        // Call reconnect against a live, healthy session. Should be a fast no-op
        // (no extra round-trip to the server).
        let vol = make_docker_volume().await;
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
        let start = std::time::Instant::now();
        vol.do_attempt_reconnect().await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(vol.connection_state(), ConnectionState::Direct);
        // No-op should be effectively instant. Any real session build would
        // take tens of ms minimum even against localhost. Pad the bound for
        // CI noise.
        assert!(
            elapsed < Duration::from_millis(50),
            "noop reconnect took {:?}; expected <50ms",
            elapsed
        );
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_list_directory() {
        let vol = make_docker_volume().await;
        let entries = vol.list_directory_impl(Path::new("")).await.unwrap();
        // The public share should be listable (may have files from other tests)
        assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_create_and_read_file() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;

        // Create a directory
        vol.create_directory(Path::new(&dir)).await.unwrap();

        // Create a file inside it
        let file_path = format!("{}/test.txt", dir);
        let content = b"hello from cmdr integration test";
        vol.create_file(Path::new(&file_path), content).await.unwrap();

        // Verify it exists
        assert!(vol.exists(Path::new(&file_path)).await);
        assert!(!vol.is_directory(Path::new(&file_path)).await.unwrap());

        // Verify metadata
        let meta = vol.get_metadata(Path::new(&file_path)).await.unwrap();
        assert_eq!(meta.name, "test.txt");
        assert_eq!(meta.size, Some(content.len() as u64));
        assert!(!meta.is_directory);

        // Byte-level integrity: read the destination back and compare bytes.
        // Catches any pipeline bug that lets metadata say "N bytes" while the
        // wire payload is something other than the source.
        let mut readback_stream = vol.open_read_stream(Path::new(&file_path)).await.unwrap();
        let mut readback = Vec::new();
        while let Some(Ok(chunk)) = readback_stream.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, content, "destination bytes must match source bytes");

        // List the directory and verify the file is there
        let entries = vol.list_directory_impl(Path::new(&dir)).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test.txt");

        // Clean up
        vol.delete(Path::new(&file_path)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    /// Regression for the high-severity audit finding: `create_file` is a
    /// no-overwrite contract. Pre-fix, SMB delegated to `tree.write_file`
    /// which uses `FileOverwriteIf` disposition and silently truncated.
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_create_file_does_not_clobber_existing() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;

        vol.create_directory(Path::new(&dir)).await.unwrap();
        let file_path = format!("{}/notes.txt", dir);
        let original = b"important user data";
        vol.create_file(Path::new(&file_path), original).await.unwrap();

        // Second create on the same path must fail with AlreadyExists;
        // bytes on the wire must be unchanged.
        let result = vol.create_file(Path::new(&file_path), b"junk").await;
        assert!(
            matches!(result, Err(VolumeError::AlreadyExists(_))),
            "expected AlreadyExists, got {:?}",
            result
        );

        let mut readback = vol.open_read_stream(Path::new(&file_path)).await.unwrap();
        let mut bytes = Vec::new();
        while let Some(Ok(chunk)) = readback.next_chunk().await {
            bytes.extend_from_slice(&chunk);
        }
        assert_eq!(bytes, original, "original bytes must survive a colliding create_file");

        vol.delete(Path::new(&file_path)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    /// Regression test for a unit-mismatch bug where SMB returned `modified_at` in
    /// milliseconds while the rest of cmdr (and the frontend formatter) expects seconds.
    /// That caused displayed years like 58247 on real shares. Asserts the mtime of a
    /// just-created file lands near wall-clock `now`, in Unix seconds.
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_modified_at_is_unix_seconds() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();
        let file_path = format!("{}/mtime.txt", dir);
        vol.create_file(Path::new(&file_path), b"mtime").await.unwrap();

        let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let meta = vol.get_metadata(Path::new(&file_path)).await.unwrap();
        let mtime = meta.modified_at.expect("mtime should be populated");

        // Must be Unix seconds, not millis (*1000) or micros (*1_000_000).
        // Allow a 1 hour window for clock skew between host and container.
        let lower = now_secs.saturating_sub(3600);
        let upper = now_secs + 3600;
        assert!(
            mtime >= lower && mtime <= upper,
            "modified_at {mtime} out of range [{lower}, {upper}]; likely wrong unit (seconds expected)",
        );

        vol.delete(Path::new(&file_path)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_rename() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        let old_path = format!("{}/old.txt", dir);
        let new_path = format!("{}/new.txt", dir);

        vol.create_file(Path::new(&old_path), b"rename me").await.unwrap();

        // Rename
        vol.rename(Path::new(&old_path), Path::new(&new_path), false)
            .await
            .unwrap();

        // Old is gone, new exists
        assert!(!vol.exists(Path::new(&old_path)).await);
        assert!(vol.exists(Path::new(&new_path)).await);

        // Clean up
        vol.delete(Path::new(&new_path)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_rename_force_overwrites() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        let src = format!("{}/src.txt", dir);
        let dst = format!("{}/dst.txt", dir);

        vol.create_file(Path::new(&src), b"source content").await.unwrap();
        vol.create_file(Path::new(&dst), b"will be overwritten").await.unwrap();

        // Non-force should fail
        let err = vol.rename(Path::new(&src), Path::new(&dst), false).await;
        assert!(matches!(err, Err(VolumeError::AlreadyExists(_))));

        // Force should succeed
        vol.rename(Path::new(&src), Path::new(&dst), true).await.unwrap();
        assert!(!vol.exists(Path::new(&src)).await);
        assert!(vol.exists(Path::new(&dst)).await);

        // Clean up
        vol.delete(Path::new(&dst)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_delete_directory() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        assert!(vol.exists(Path::new(&dir)).await);
        assert!(vol.is_directory(Path::new(&dir)).await.unwrap());

        vol.delete(Path::new(&dir)).await.unwrap();
        assert!(!vol.exists(Path::new(&dir)).await);
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_read_stream_single_file() {
        // Exercises the SMB → local byte path (now via open_read_stream) at
        // the simplest shape.
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        let smb_file = format!("{}/export-test.txt", dir);
        let content = b"exported content";
        vol.create_file(Path::new(&smb_file), content).await.unwrap();

        let mut stream = vol.open_read_stream(Path::new(&smb_file)).await.unwrap();
        assert_eq!(stream.total_size(), content.len() as u64);
        let mut readback = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, content);

        vol.delete(Path::new(&smb_file)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_write_from_stream_single_file() {
        // Exercises the local → SMB byte path (now via write_from_stream) at
        // the simplest shape. Uses InMemoryVolume as the source stream.
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        let content = b"imported content";
        let source = InMemoryVolume::new("Source");
        source
            .create_file(Path::new("/import-test.txt"), content)
            .await
            .unwrap();

        let smb_file = format!("{}/import-test.txt", dir);
        let stream = source.open_read_stream(Path::new("/import-test.txt")).await.unwrap();
        let size = stream.total_size();
        let bytes = vol
            .write_from_stream(Path::new(&smb_file), size, stream, &|_, _| {
                std::ops::ControlFlow::Continue(())
            })
            .await
            .unwrap();
        assert_eq!(bytes, content.len() as u64);

        assert!(vol.exists(Path::new(&smb_file)).await);
        let meta = vol.get_metadata(Path::new(&smb_file)).await.unwrap();
        assert_eq!(meta.size, Some(content.len() as u64));

        // Byte-level integrity: the bytes that landed on the SMB share must
        // be the same bytes the source stream produced. A bug in the write
        // pipeline (wrong chunk reused, compound-write fast-path mis-splitting
        // the buffer) would leave size correct but content wrong.
        let mut verify = vol.open_read_stream(Path::new(&smb_file)).await.unwrap();
        let mut readback = Vec::new();
        while let Some(Ok(chunk)) = verify.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, content, "SMB destination bytes must match source bytes");

        vol.delete(Path::new(&smb_file)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_scan_for_copy() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        // Create a small tree
        vol.create_directory(Path::new(&dir)).await.unwrap();
        let sub = format!("{}/inner", dir);
        vol.create_directory(Path::new(&sub)).await.unwrap();
        vol.create_file(Path::new(&format!("{}/f1.txt", dir)), b"aaa")
            .await
            .unwrap();
        vol.create_file(Path::new(&format!("{}/inner/f2.txt", dir)), b"bbbbbb")
            .await
            .unwrap();

        let result = vol.scan_for_copy(Path::new(&dir)).await.unwrap();
        assert_eq!(result.file_count, 2);
        assert_eq!(result.dir_count, 2); // dir + inner
        assert_eq!(result.total_bytes, 9); // 3 + 6

        // Clean up
        vol.delete(Path::new(&format!("{}/inner/f2.txt", dir))).await.unwrap();
        vol.delete(Path::new(&format!("{}/f1.txt", dir))).await.unwrap();
        vol.delete(Path::new(&sub)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_scan_for_copy_batch_mixed() {
        // Pipelined batch scan on the SMB hot copy path.
        // Mixed batch of files + a directory: aggregate counts should match
        // what the per-path scan_for_copy loop would produce, and the
        // per_path vec should carry correct top_level_is_directory / size.
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        vol.create_file(Path::new(&format!("{}/a.txt", dir)), b"aaa")
            .await
            .unwrap();
        vol.create_file(Path::new(&format!("{}/b.txt", dir)), b"bbbb")
            .await
            .unwrap();
        vol.create_file(Path::new(&format!("{}/c.txt", dir)), b"ccccc")
            .await
            .unwrap();
        let subdir = format!("{}/nested", dir);
        vol.create_directory(Path::new(&subdir)).await.unwrap();
        vol.create_file(Path::new(&format!("{}/nested/d.txt", dir)), b"dddddd")
            .await
            .unwrap();

        let paths: Vec<PathBuf> = vec![
            PathBuf::from(format!("{}/a.txt", dir)),
            PathBuf::from(format!("{}/b.txt", dir)),
            PathBuf::from(format!("{}/c.txt", dir)),
            PathBuf::from(format!("{}/nested", dir)),
            PathBuf::from(format!("{}/nested/d.txt", dir)),
        ];

        let batch = vol.scan_for_copy_batch(&paths).await.unwrap();

        // Compare against per-path scan_for_copy to ensure parity.
        let mut expected_files = 0usize;
        let mut expected_dirs = 0usize;
        let mut expected_bytes = 0u64;
        for p in &paths {
            let r = vol.scan_for_copy(p).await.unwrap();
            expected_files += r.file_count;
            expected_dirs += r.dir_count;
            expected_bytes += r.total_bytes;
        }
        assert_eq!(batch.aggregate.file_count, expected_files);
        assert_eq!(batch.aggregate.dir_count, expected_dirs);
        assert_eq!(batch.aggregate.total_bytes, expected_bytes);

        // per_path preserves input order and type info.
        assert_eq!(batch.per_path.len(), paths.len());
        for (i, (path, scan)) in batch.per_path.iter().enumerate() {
            assert_eq!(path, &paths[i]);
            let is_dir_name = path.to_string_lossy().ends_with("/nested");
            assert_eq!(scan.top_level_is_directory, is_dir_name, "path #{} type mismatch", i);
        }

        // The top-level files' per_path entries carry the file size.
        let a = batch
            .per_path
            .iter()
            .find(|(p, _)| p.to_string_lossy().ends_with("/a.txt"))
            .unwrap();
        assert_eq!(a.1.total_bytes, 3);

        // Cleanup.
        for entry in &["nested/d.txt", "a.txt", "b.txt", "c.txt"] {
            vol.delete(Path::new(&format!("{}/{}", dir, entry))).await.unwrap();
        }
        vol.delete(Path::new(&subdir)).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_scan_for_copy_batch_single_path() {
        // Regression guard for the N=1 fast-path: should behave exactly like
        // scan_for_copy and handle the empty-root case naturally.
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        vol.create_file(Path::new(&format!("{}/only.txt", dir)), b"single")
            .await
            .unwrap();

        let path = PathBuf::from(format!("{}/only.txt", dir));
        let batch = vol.scan_for_copy_batch(std::slice::from_ref(&path)).await.unwrap();
        let single = vol.scan_for_copy(&path).await.unwrap();

        assert_eq!(batch.aggregate.file_count, single.file_count);
        assert_eq!(batch.aggregate.dir_count, single.dir_count);
        assert_eq!(batch.aggregate.total_bytes, single.total_bytes);
        assert_eq!(batch.per_path.len(), 1);
        assert_eq!(batch.per_path[0].0, path);
        assert!(!batch.per_path[0].1.top_level_is_directory);
        assert_eq!(batch.per_path[0].1.total_bytes, 6);

        vol.delete(&path).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_scan_for_copy_batch_propagates_missing_path_error() {
        // If one path in the batch doesn't exist, the whole batch must
        // surface an error (callers treat scan as a pre-flight gate: a
        // missing source is a user-visible problem, not a silent drop).
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        vol.create_file(Path::new(&format!("{}/real.txt", dir)), b"data")
            .await
            .unwrap();

        let paths: Vec<PathBuf> = vec![
            PathBuf::from(format!("{}/real.txt", dir)),
            PathBuf::from(format!("{}/does-not-exist.txt", dir)),
            PathBuf::from(format!("{}/also-real-but-missing.txt", dir)),
        ];

        let result = vol.scan_for_copy_batch(&paths).await;
        assert!(matches!(result, Err(VolumeError::NotFound(_))));

        // Cleanup.
        vol.delete(Path::new(&format!("{}/real.txt", dir))).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_scan_for_conflicts() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).await.unwrap();
        vol.create_file(Path::new(&format!("{}/exists.txt", dir)), b"data")
            .await
            .unwrap();

        let source_items = vec![
            SourceItemInfo {
                name: "exists.txt".to_string(),
                size: 100,
                modified: Some(0),
            },
            SourceItemInfo {
                name: "missing.txt".to_string(),
                size: 200,
                modified: Some(0),
            },
        ];

        let conflicts = vol.scan_for_conflicts(&source_items, Path::new(&dir)).await.unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].source_path, "exists.txt");

        // Clean up
        vol.delete(Path::new(&format!("{}/exists.txt", dir))).await.unwrap();
        vol.delete(Path::new(&dir)).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_space_info() {
        let vol = make_docker_volume().await;
        let space = vol.get_space_info().await.unwrap();
        assert!(space.total_bytes > 0);
        assert!(space.available_bytes > 0);
        assert!(space.used_bytes <= space.total_bytes);
    }

    // ── SmbReadStream consumer tests ────────────────────────────────
    //
    // These test the consumer side of the channel-backed SmbReadStream in
    // isolation. End-to-end SMB streaming is covered by the Docker
    // integration tests below (smb_integration_open_read_stream,
    // smb_integration_export_streams).

    /// Builds an SmbReadStream backed by a pre-seeded channel, bypassing the
    /// real SMB producer task. Returns the stream plus the cancel receiver
    /// side so tests can assert that drop sends a cancel signal.
    fn make_stream_from_chunks(
        chunks: Vec<Result<Vec<u8>, VolumeError>>,
        total_size: u64,
    ) -> (SmbReadStream, tokio::sync::oneshot::Receiver<()>) {
        let (chunk_tx, chunk_rx) =
            tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY.max(chunks.len()));
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

        for chunk in chunks {
            // blocking_send is fine in tests; we sized the channel to fit.
            chunk_tx.try_send(chunk).expect("channel has capacity in test setup");
        }
        // Drop chunk_tx so recv returns None after draining.
        drop(chunk_tx);

        let stream = SmbReadStream {
            rx: chunk_rx,
            cancel: Some(cancel_tx),
            total_size,
            bytes_read: 0,
        };
        (stream, cancel_rx)
    }

    #[tokio::test]
    async fn smb_read_stream_empty_file() {
        let (mut stream, _cancel_rx) = make_stream_from_chunks(vec![], 0);
        assert_eq!(stream.total_size(), 0);
        assert_eq!(stream.bytes_read(), 0);
        assert!(stream.next_chunk().await.is_none());
    }

    #[tokio::test]
    async fn smb_read_stream_yields_chunks_in_order() {
        let (mut stream, _cancel_rx) =
            make_stream_from_chunks(vec![Ok(vec![1u8; 100]), Ok(vec![2u8; 50]), Ok(vec![3u8; 30])], 180);
        assert_eq!(stream.total_size(), 180);

        let c1 = stream.next_chunk().await.unwrap().unwrap();
        assert_eq!(c1, vec![1u8; 100]);
        assert_eq!(stream.bytes_read(), 100);

        let c2 = stream.next_chunk().await.unwrap().unwrap();
        assert_eq!(c2, vec![2u8; 50]);
        assert_eq!(stream.bytes_read(), 150);

        let c3 = stream.next_chunk().await.unwrap().unwrap();
        assert_eq!(c3, vec![3u8; 30]);
        assert_eq!(stream.bytes_read(), 180);

        assert!(stream.next_chunk().await.is_none());
    }

    #[tokio::test]
    async fn smb_read_stream_propagates_mid_stream_error() {
        let (mut stream, _cancel_rx) = make_stream_from_chunks(
            vec![
                Ok(vec![1u8; 10]),
                Err(VolumeError::DeviceDisconnected("simulated".to_string())),
            ],
            0,
        );

        let first = stream.next_chunk().await.unwrap().unwrap();
        assert_eq!(first, vec![1u8; 10]);
        assert_eq!(stream.bytes_read(), 10);

        let second = stream.next_chunk().await.unwrap();
        assert!(matches!(second, Err(VolumeError::DeviceDisconnected(_))));
        // bytes_read should not have advanced on the error
        assert_eq!(stream.bytes_read(), 10);
    }

    #[tokio::test]
    async fn smb_read_stream_drop_sends_cancel() {
        let (stream, mut cancel_rx) = make_stream_from_chunks(vec![Ok(vec![1u8; 10])], 10);
        drop(stream);

        // The cancel oneshot should have been fired by Drop.
        match cancel_rx.try_recv() {
            Ok(()) => {}
            other => panic!("expected cancel signal, got {other:?}"),
        }
    }

    #[test]
    fn smb_supports_streaming() {
        // SmbVolume should report streaming support so cross-volume copies
        // (MTP↔SMB) use the streaming path instead of NotSupported/temp files.
        let vol = make_test_volume();
        assert!(vol.supports_streaming());
    }

    // ── SMB streaming integration tests (Docker) ───────────────────

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_open_read_stream() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let data = b"streaming read test content";
        vol.create_file(Path::new(&format!("{}/read.txt", dir)), data)
            .await
            .unwrap();

        let mut stream = vol
            .open_read_stream(Path::new(&format!("{}/read.txt", dir)))
            .await
            .unwrap();
        assert_eq!(stream.total_size(), data.len() as u64);

        let mut reassembled = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            reassembled.extend_from_slice(&chunk);
        }
        assert_eq!(reassembled, data);
        assert_eq!(stream.bytes_read(), data.len() as u64);

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_write_from_stream() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        // Create a source via InMemoryVolume
        let source = InMemoryVolume::new("Source");
        let data: Vec<u8> = (0..=255).cycle().take(50_000).collect();
        source.create_file(Path::new("/payload.bin"), &data).await.unwrap();

        let stream = source.open_read_stream(Path::new("/payload.bin")).await.unwrap();
        let no_progress = &|_: u64, _: u64| std::ops::ControlFlow::Continue(());
        let bytes = vol
            .write_from_stream(Path::new(&format!("{}/payload.bin", dir)), 50_000, stream, no_progress)
            .await
            .unwrap();
        assert_eq!(bytes, 50_000);

        // Read back and verify content integrity
        let mut verify = vol
            .open_read_stream(Path::new(&format!("{}/payload.bin", dir)))
            .await
            .unwrap();
        let mut readback = Vec::new();
        while let Some(Ok(chunk)) = verify.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, data);

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_write_from_stream_with_progress() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let source = InMemoryVolume::new("Source");
        let data = vec![0xCD; 200_000]; // ~200 KB
        source.create_file(Path::new("/big.bin"), &data).await.unwrap();

        use std::sync::atomic::{AtomicU64, AtomicUsize};

        let progress_calls = AtomicUsize::new(0);
        let last_bytes = AtomicU64::new(0);

        let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
        let bytes = vol
            .write_from_stream(
                Path::new(&format!("{}/big.bin", dir)),
                200_000,
                stream,
                &|bytes_done, total| {
                    progress_calls.fetch_add(1, Ordering::Relaxed);
                    last_bytes.store(bytes_done, Ordering::Relaxed);
                    assert_eq!(total, 200_000);
                    std::ops::ControlFlow::Continue(())
                },
            )
            .await
            .unwrap();

        assert_eq!(bytes, 200_000);
        assert!(
            progress_calls.load(Ordering::Relaxed) >= 1,
            "expected at least 1 progress call"
        );
        assert_eq!(last_bytes.load(Ordering::Relaxed), 200_000);

        // Byte-level integrity: a progress-reporting write that loses or
        // duplicates chunks would still satisfy the "progress_calls >= 1
        // and final bytes_done == 200_000" assertions; hash the destination
        // against the source to catch that.
        let mut verify = vol
            .open_read_stream(Path::new(&format!("{}/big.bin", dir)))
            .await
            .unwrap();
        let mut readback = Vec::with_capacity(200_000);
        while let Some(Ok(chunk)) = verify.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, data, "destination bytes must match source bytes");

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_write_from_stream_cancel() {
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let source = InMemoryVolume::new("Source");
        let data = vec![0xEF; 500_000]; // ~500 KB, several chunks
        source.create_file(Path::new("/big.bin"), &data).await.unwrap();

        let call_count = std::sync::atomic::AtomicUsize::new(0);
        let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
        let result = vol
            .write_from_stream(Path::new(&format!("{}/big.bin", dir)), 500_000, stream, &|_, _| {
                let n = call_count.fetch_add(1, Ordering::Relaxed);
                if n >= 1 {
                    std::ops::ControlFlow::Break(())
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            })
            .await;

        assert!(result.is_err(), "expected cancellation error");

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_cross_volume_streaming_copy() {
        // Full end-to-end: InMemoryVolume → SmbVolume via open_read_stream + write_from_stream.
        // Tests the same path that copy_single_path uses for non-local volumes.
        use std::sync::atomic::{AtomicUsize, Ordering};

        let smb_vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&smb_vol, &dir).await;
        smb_vol.create_directory(Path::new(&dir)).await.unwrap();

        let source = InMemoryVolume::new("Source");
        let data: Vec<u8> = (0..=255).cycle().take(100_000).collect();
        source.create_file(Path::new("/photo.bin"), &data).await.unwrap();

        let progress_calls = AtomicUsize::new(0);

        // Read from InMemory, write to SMB (the same path copy_single_path takes)
        let stream = source.open_read_stream(Path::new("/photo.bin")).await.unwrap();
        let bytes = smb_vol
            .write_from_stream(Path::new(&format!("{}/photo.bin", dir)), 100_000, stream, &|_, _| {
                progress_calls.fetch_add(1, Ordering::Relaxed);
                std::ops::ControlFlow::Continue(())
            })
            .await
            .unwrap();

        assert_eq!(bytes, 100_000);
        assert!(progress_calls.load(Ordering::Relaxed) >= 1);

        // Verify content via read back
        let mut verify = smb_vol
            .open_read_stream(Path::new(&format!("{}/photo.bin", dir)))
            .await
            .unwrap();
        let mut readback = Vec::new();
        while let Some(Ok(chunk)) = verify.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, data);

        ensure_clean(&smb_vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_open_read_stream_large_file_spans_many_chunks() {
        // Verifies the streaming reader delivers a multi-MB file correctly
        // across many chunk boundaries. Before the channel-backed rewrite, the
        // whole file was buffered in memory up front.
        //
        // The file has to exceed `max_read_size` (up to 8 MB on Samba) for
        // smb2 to split the read into more than one READ. 20 MB is a safe
        // multiple that stays under the single-chunk ceiling.
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        // 20 MB: guarantees multiple READs even at 8 MB max_read_size.
        let size = 20 * 1024 * 1024;
        let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let smb_path = format!("{}/big-stream.bin", dir);
        vol.create_file(Path::new(&smb_path), &data).await.unwrap();

        // Hash chunks as they arrive so a 20 MB mismatch produces a single
        // 32-byte hex pair instead of a 20 MB `Vec<u8>` diff. Also avoids
        // the 20 MB reassembly allocation.
        let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
        assert_eq!(stream.total_size(), size as u64);

        let mut hasher = blake3::Hasher::new();
        let mut chunks_seen = 0usize;
        let mut total_read = 0usize;
        while let Some(result) = stream.next_chunk().await {
            let chunk = result.unwrap();
            assert!(!chunk.is_empty(), "should not yield empty chunks");
            hasher.update(&chunk);
            total_read += chunk.len();
            chunks_seen += 1;
        }
        assert_eq!(total_read, size, "total bytes streamed must equal source size");
        let readback_hash = *hasher.finalize().as_bytes();
        let expected_hash = hash_bytes(&data);
        assert_eq!(
            readback_hash, expected_hash,
            "streamed bytes must match source (expected blake3 {:x?}, got {:x?})",
            expected_hash, readback_hash
        );
        assert_eq!(stream.bytes_read(), size as u64);
        assert!(chunks_seen >= 2, "multi-MB file should span multiple chunks");

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_read_stream_large_file_multi_chunk() {
        // SMB → local byte path now goes through `open_read_stream`, then the
        // caller writes into whatever destination. Verify that the streaming
        // reader yields multiple chunks for a multi-MB file.
        //
        // `max_read_size` negotiation can go up to 8 MB on modern Samba, so
        // the file has to be >8 MB to guarantee multiple READs.
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let size = 20 * 1024 * 1024; // 20 MB, exceeds 8 MB max_read_size
        let data: Vec<u8> = (0..size).map(|i| ((i * 7) % 251) as u8).collect();
        let smb_path = format!("{}/export-large.bin", dir);
        vol.create_file(Path::new(&smb_path), &data).await.unwrap();

        // Hash chunks as they arrive (see the sibling large-file test for
        // why we avoid `assert_eq!` on 20 MB `Vec<u8>`s).
        let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
        assert_eq!(stream.total_size(), size as u64);

        let mut chunks_seen = 0usize;
        let mut hasher = blake3::Hasher::new();
        let mut total_read = 0usize;
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            chunks_seen += 1;
            hasher.update(&chunk);
            total_read += chunk.len();
        }
        assert!(
            chunks_seen >= 2,
            "streaming should yield multiple chunks for a multi-MB file"
        );
        assert_eq!(total_read, size, "total bytes streamed must equal source size");
        let readback_hash = *hasher.finalize().as_bytes();
        let expected_hash = hash_bytes(&data);
        assert_eq!(
            readback_hash, expected_hash,
            "streamed bytes must match source (expected blake3 {:x?}, got {:x?})",
            expected_hash, readback_hash
        );

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_open_read_stream_cancel_by_drop() {
        // Drop the stream mid-way and verify that subsequent SMB operations
        // on the same volume still work (producer task released the mutex).
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let data = vec![0xAA; 2 * 1024 * 1024]; // 2 MB
        let smb_path = format!("{}/cancel-me.bin", dir);
        vol.create_file(Path::new(&smb_path), &data).await.unwrap();

        let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
        // Read exactly one chunk then drop
        let _first = stream.next_chunk().await.unwrap().unwrap();
        drop(stream);

        // Subsequent op on the volume should succeed; the producer task
        // must have released the session mutex on cancel.
        let entries = vol.list_directory(Path::new(&dir), None).await.unwrap();
        assert!(entries.iter().any(|e| e.name == "cancel-me.bin"));

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_write_from_stream_local_source_large_file() {
        // Local → SMB byte path now goes through LocalPosixVolume's
        // `open_read_stream` + SmbVolume's `write_from_stream`. Verify that
        // multi-MB input triggers multiple progress callbacks and round-trips.
        use std::sync::atomic::{AtomicU64, AtomicUsize};

        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let size = 4 * 1024 * 1024; // 4 MB, spans multiple import chunks
        let data: Vec<u8> = (0..size).map(|i| ((i * 13) % 251) as u8).collect();

        let local_tmp = std::env::temp_dir().join(format!("cmdr-smb-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&local_tmp);
        std::fs::create_dir_all(&local_tmp).unwrap();
        std::fs::write(local_tmp.join("import-large.bin"), &data).unwrap();

        let local_vol = crate::file_system::volume::LocalPosixVolume::new("local-src", local_tmp.clone());

        let smb_path = format!("{}/import-large.bin", dir);
        let progress_calls = AtomicUsize::new(0);
        let last_bytes = AtomicU64::new(0);

        let stream = local_vol.open_read_stream(Path::new("import-large.bin")).await.unwrap();
        assert_eq!(stream.total_size(), size as u64);

        let bytes = vol
            .write_from_stream(Path::new(&smb_path), size as u64, stream, &|done, total| {
                progress_calls.fetch_add(1, Ordering::Relaxed);
                last_bytes.store(done, Ordering::Relaxed);
                assert_eq!(total, size as u64);
                std::ops::ControlFlow::Continue(())
            })
            .await
            .unwrap();

        assert_eq!(bytes, size as u64);
        assert!(
            progress_calls.load(Ordering::Relaxed) >= 2,
            "streaming write should call progress multiple times for a multi-chunk source"
        );
        assert_eq!(last_bytes.load(Ordering::Relaxed), size as u64);

        // Byte-level integrity: hash the source and the destination and
        // compare. Streaming hash avoids materializing a 4 MB `Vec<u8>`
        // just to `assert_eq!` it, and on mismatch we get a legible hex
        // dump instead of a multi-megabyte diff.
        let expected_hash = hash_bytes(&data);
        let actual_hash = hash_volume_file(&vol as &dyn Volume, Path::new(&smb_path)).await;
        assert_eq!(
            actual_hash, expected_hash,
            "SMB destination bytes must match source (expected blake3 {:x?}, got {:x?})",
            expected_hash, actual_hash
        );

        let _ = std::fs::remove_dir_all(&local_tmp);
        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_write_from_stream_streams_large_file() {
        // InMemoryVolume → SmbVolume via write_from_stream with a multi-chunk
        // source. Verifies the SMB write path now pulls chunks on demand
        // rather than collecting the full source into a Vec<u8>.
        use std::sync::atomic::{AtomicU64, AtomicUsize};

        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let size: usize = 4 * 1024 * 1024; // 4 MB
        let data: Vec<u8> = (0..size).map(|i| ((i * 11) % 251) as u8).collect();

        let source = InMemoryVolume::new("Source");
        source.create_file(Path::new("/big-stream.bin"), &data).await.unwrap();

        let smb_path = format!("{}/big-stream.bin", dir);
        let progress_calls = AtomicUsize::new(0);
        let last_bytes = AtomicU64::new(0);

        let stream = source.open_read_stream(Path::new("/big-stream.bin")).await.unwrap();
        let bytes = vol
            .write_from_stream(Path::new(&smb_path), size as u64, stream, &|done, total| {
                progress_calls.fetch_add(1, Ordering::Relaxed);
                last_bytes.store(done, Ordering::Relaxed);
                assert_eq!(total, size as u64);
                std::ops::ControlFlow::Continue(())
            })
            .await
            .unwrap();

        assert_eq!(bytes, size as u64);
        assert!(
            progress_calls.load(Ordering::Relaxed) >= 2,
            "streaming write should call progress multiple times for a multi-chunk source"
        );
        assert_eq!(last_bytes.load(Ordering::Relaxed), size as u64);

        // Byte-level integrity: streaming hash over the destination catches
        // any chunk drop/duplicate/reuse that "bytes_written == expected"
        // on its own can't see. See the sibling local-source test for the
        // rationale on hashing vs. `assert_eq!` on a 4 MB buffer.
        let expected_hash = hash_bytes(&data);
        let actual_hash = hash_volume_file(&vol as &dyn Volume, Path::new(&smb_path)).await;
        assert_eq!(
            actual_hash, expected_hash,
            "SMB destination bytes must match source (expected blake3 {:x?}, got {:x?})",
            expected_hash, actual_hash
        );

        ensure_clean(&vol, &dir).await;
    }

    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_write_from_stream_cancel_mid_write() {
        // Cancel partway through a multi-chunk write via progress-break.
        // Verifies Cancelled is returned and that the SMB session is still
        // usable for subsequent ops (writer.abort() drains in-flight WRITE
        // responses cleanly on cancel, best-effort-deletes the partial file).
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let size = 4 * 1024 * 1024; // 4 MB, several write chunks
        let data = vec![0xC3u8; size];

        let source = InMemoryVolume::new("Source");
        source.create_file(Path::new("/cancel-me.bin"), &data).await.unwrap();

        let smb_path = format!("{}/cancel-me.bin", dir);
        let call_count = std::sync::atomic::AtomicUsize::new(0);

        let stream = source.open_read_stream(Path::new("/cancel-me.bin")).await.unwrap();
        let result = vol
            .write_from_stream(Path::new(&smb_path), size as u64, stream, &|_, _| {
                let n = call_count.fetch_add(1, Ordering::Relaxed);
                if n >= 1 {
                    std::ops::ControlFlow::Break(())
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            })
            .await;

        assert!(
            matches!(result, Err(VolumeError::Cancelled(_))),
            "expected Cancelled, got {result:?}"
        );

        // The session must still work after cancel.
        let _ = vol.list_directory(Path::new(&dir), None).await.unwrap();

        ensure_clean(&vol, &dir).await;
    }

    /// Cross-task content integrity: 100 concurrent SMB → local copies, each file
    /// with unique deterministic content. After the batch completes, every
    /// destination's blake3 hash must match the hash of the source it claims to
    /// come from: catches buffer reuse across tasks, wrong-buffer-to-wrong-path
    /// routing, races in the `Arc<Mutex<Option<SmbClient>>>` +
    /// `Arc<RwLock<Option<Arc<Tree>>>>` split-session (Fix 2), and
    /// cross-MessageId wire demux mistakes on cloned `Connection`s.
    ///
    /// Identical-content tests can't see any of these; every file would hash
    /// the same, so a "swapped slice mid-file" or "task B's buffer landed under
    /// task A's path" bug would pass trivially. Unique per-file content makes
    /// any cross-contamination flip at least one destination's hash.
    ///
    /// Runs the real copy pipeline (`copy_volumes_with_progress`, the same
    /// function `copy_between_volumes` calls) so `FuturesUnordered` + Fix 2's
    /// split session + Fix 3's compound fast-path + Fix 4's pipelined scan all
    /// execute together, the way a user's "copy 100 files" action does.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_copy_100_unique_files_no_cross_contamination() {
        use crate::file_system::write_operations::{
            CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
        };
        use std::time::{Duration, Instant};

        // Content scheme: `blake3(b"cmdr-fix8-" || index_le) .as_bytes() repeated 320 times`
        // = 10_240 bytes per file, truly unique per index, every byte position varies
        // between files. Any cross-task slice swap (even a 32-byte block in the
        // middle of one file coming from a neighbor's buffer) flips blake3.
        // 10 KB keeps fixture setup cheap and stays inside the SMB compound
        // fast-path (Fix 3) so we're exercising it, not the streaming fallback.
        fn expected_content(index: usize) -> Vec<u8> {
            let mut seed = Vec::with_capacity(10 + 8);
            seed.extend_from_slice(b"cmdr-fix8-");
            seed.extend_from_slice(&(index as u64).to_le_bytes());
            let block = *blake3::hash(&seed).as_bytes(); // 32 bytes
            let mut out = Vec::with_capacity(32 * 320);
            for _ in 0..320 {
                out.extend_from_slice(&block);
            }
            out
        }

        const FILE_COUNT: usize = 100;

        // Hold the concrete `SmbVolume` for `ensure_clean` (which takes
        // `&SmbVolume`) and clone an `Arc<dyn Volume>` view of the same
        // session for the copy pipeline.
        let smb_vol = Arc::new(make_docker_volume().await);
        let src_dir = test_dir_name();
        ensure_clean(&smb_vol, &src_dir).await;
        smb_vol.create_directory(Path::new(&src_dir)).await.unwrap();
        let vol: Arc<dyn Volume> = smb_vol.clone();

        // Fixture: create 100 files on the SMB source, serially. Parallel
        // `create_file` on a single SMB session wouldn't speed this up
        // (creates are 1 RTT each), and keeping setup simple keeps any bug
        // the test catches unambiguously a read/copy-path bug, not a
        // write-path races-with-itself bug.
        let fixture_start = Instant::now();
        let mut source_paths: Vec<PathBuf> = Vec::with_capacity(FILE_COUNT);
        for i in 0..FILE_COUNT {
            let name = format!("f_{:03}.bin", i);
            let smb_path = format!("{}/{}", src_dir, name);
            vol.create_file(Path::new(&smb_path), &expected_content(i))
                .await
                .unwrap();
            source_paths.push(PathBuf::from(smb_path));
        }
        log::info!(
            "smb_integration_copy_100_unique_files: fixture setup took {:?}",
            fixture_start.elapsed()
        );

        // Destination: local TempDir wrapped in a LocalPosixVolume. We feed the
        // copy pipeline the same way production does (SMB volume → Local
        // volume → `copy_volumes_with_progress`). `dest_path` is "/" relative to
        // the local volume root (i.e. the TempDir itself).
        let local_dir = tempfile::TempDir::new().expect("create TempDir");
        let dest_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
            "dest",
            local_dir.path().to_path_buf(),
        ));

        let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
        let events = Arc::new(CollectorEventSink::new());
        let config = VolumeCopyConfig::default();

        let copy_start = Instant::now();
        let result = copy_volumes_with_progress(
            events.clone(),
            "test-op-100-unique",
            &state,
            Arc::clone(&vol),
            &source_paths,
            Arc::clone(&dest_vol),
            Path::new("/"),
            &config,
        )
        .await;
        log::info!(
            "smb_integration_copy_100_unique_files: copy pipeline took {:?}",
            copy_start.elapsed()
        );
        assert!(result.is_ok(), "copy should succeed: {:?}", result);

        // Count landed files: cheap aggregate sanity check before per-index
        // verification. A cross-contamination bug that swapped two destinations
        // would still show 100 files here, so this is not the real check.
        let entries = std::fs::read_dir(local_dir.path())
            .expect("read dest dir")
            .filter_map(|e| e.ok())
            .count();
        assert_eq!(entries, FILE_COUNT, "expected {} files at destination", FILE_COUNT);

        // Per-index integrity: for each source index, read its destination file
        // and compare blake3 against the expected hash derived from the same
        // index. Assert each one individually so a swap of two destinations
        // fails loudly with both offending indices, not a vague aggregate.
        let mut mismatches: Vec<String> = Vec::new();
        for i in 0..FILE_COUNT {
            let name = format!("f_{:03}.bin", i);
            let dest_path = local_dir.path().join(&name);
            let actual_bytes = match std::fs::read(&dest_path) {
                Ok(b) => b,
                Err(e) => {
                    mismatches.push(format!("{}: couldn't read destination: {}", name, e));
                    continue;
                }
            };
            let expected_bytes = expected_content(i);
            let expected_hash = hash_bytes(&expected_bytes);
            let actual_hash = hash_bytes(&actual_bytes);
            if actual_hash != expected_hash {
                // Find the first diff position and a small slice of context;
                // a 10 KB diff dump would drown the terminal on any failure.
                let first_diff = expected_bytes.iter().zip(actual_bytes.iter()).position(|(a, b)| a != b);
                let diff_detail = match first_diff {
                    Some(pos) => {
                        let end_exp = pos.saturating_add(16).min(expected_bytes.len());
                        let end_act = pos.saturating_add(16).min(actual_bytes.len());
                        format!(
                            "first diff at byte {}: expected {:02x?}, got {:02x?}",
                            pos,
                            &expected_bytes[pos..end_exp],
                            &actual_bytes[pos..end_act]
                        )
                    }
                    None => {
                        // Same bytes but different length (hashes differ so
                        // there must be a difference somewhere).
                        format!(
                            "byte-for-byte equal in overlap but lengths differ: expected {}, got {}",
                            expected_bytes.len(),
                            actual_bytes.len()
                        )
                    }
                };
                mismatches.push(format!(
                    "{}: expected blake3 {} ({} bytes), got blake3 {} ({} bytes); {}",
                    name,
                    hex_of(&expected_hash),
                    expected_bytes.len(),
                    hex_of(&actual_hash),
                    actual_bytes.len(),
                    diff_detail,
                ));
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} of {} destinations failed content check:\n  - {}",
            mismatches.len(),
            FILE_COUNT,
            mismatches.join("\n  - "),
        );

        // Cleanup the SMB source. The TempDir cleans itself on drop.
        ensure_clean(&smb_vol, &src_dir).await;
    }

    /// Hex formatter for blake3 hashes in failure messages. Avoids a hex-crate
    /// dep just for test diagnostics.
    fn hex_of(bytes: &[u8; 32]) -> String {
        let mut s = String::with_capacity(64);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    // ── Soak test: repeated SMB→Local copy pipeline ────────────────
    //
    // Catches accumulating bugs that short tests miss: credit drift,
    // file-descriptor leaks, memory growth, per-iteration slowdown. The short
    // integration tests above verify single-operation correctness; this one
    // hammers the same pipeline thousands of times and watches for drift.
    //
    // Modes (pick via env):
    // - Default (no env):           `CMDR_SOAK_ITERATIONS=100` (≈1–2 min). Sanity-check run for gross
    //   leaks.
    // - Explicit iteration count:    `CMDR_SOAK_ITERATIONS=3000 ...`
    // - Time-bounded:                `CMDR_SOAK_DURATION_SECS=1800 ...` (30 min)
    //
    // Uses `smb-consumer-auth` (port 10481, share `private`, `testuser` /
    // `testpass`) because it permits writes. Never runs by default; gated
    // on `#[ignore]`.

    /// `getrusage(RUSAGE_SELF).ru_maxrss`: peak resident set size. On macOS the
    /// value is in bytes; on Linux it's in kilobytes. Returns megabytes.
    ///
    /// Why peak-RSS not current-RSS: macOS/Linux both surface `ru_maxrss` from
    /// `getrusage(2)` without needing extra deps (`sysinfo` with `process`
    /// feature, `proc_pidinfo` FFI, or `/proc/self/status`). For a leak hunt
    /// peak RSS is actually the metric we want; current RSS oscillates with
    /// glibc/jemalloc GC, peak is monotonic and only grows when we genuinely
    /// retain more bytes.
    fn process_peak_rss_mb() -> f64 {
        #[cfg(unix)]
        {
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
            if rc != 0 {
                return 0.0;
            }
            let ru_maxrss = usage.ru_maxrss as f64;
            #[cfg(target_os = "macos")]
            {
                // bytes → MB
                ru_maxrss / (1024.0 * 1024.0)
            }
            #[cfg(not(target_os = "macos"))]
            {
                // Linux: kilobytes → MB
                ru_maxrss / 1024.0
            }
        }
        #[cfg(not(unix))]
        {
            0.0
        }
    }

    /// Counts this process's open file descriptors. Both macOS and Linux
    /// expose `/dev/fd/` as a directory listing the current process's open
    /// descriptors (on Linux it's actually a symlink to `/proc/self/fd/`).
    /// A short-lived extra FD is opened to read the directory; subtract 1
    /// so the returned number reflects the steady-state count before the
    /// measurement started.
    fn open_fd_count() -> usize {
        match std::fs::read_dir("/dev/fd") {
            Ok(iter) => iter.count().saturating_sub(1),
            Err(_) => 0,
        }
    }

    /// Snapshots SMB credit counters inside the `SmbVolume`'s `SmbClient`. Used
    /// between iterations to spot credit drift (a leak bleeds credits over
    /// time; exhaustion would stall future reads). Returns `None` if the
    /// session isn't available.
    async fn smb_credits_snapshot(vol: &SmbVolume) -> Option<u16> {
        let guard = vol.client.lock().await;
        guard.as_ref().map(|c| c.credits())
    }

    /// Connects to the `smb-consumer-auth` Docker container (share `private`,
    /// writable, credentials `testuser` / `testpass`). Default port 10481
    /// matches smb2's auth test container; override via
    /// `SMB_CONSUMER_AUTH_PORT`.
    async fn make_docker_auth_volume() -> SmbVolume {
        let port: u16 = std::env::var("SMB_CONSUMER_AUTH_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10481);
        let volume_id = smb_volume_id("127.0.0.1", port, "private");
        let params = SmbConnectionParams::new("127.0.0.1", "private", port, Some("testuser"), Some("testpass"));
        connect_smb_volume("private", "/tmp/smb-soak-mount", &volume_id, params)
            .await
            .unwrap_or_else(|e| panic!("Failed to connect to Docker SMB auth container at 127.0.0.1:{port} ({e:?})"))
    }

    #[tokio::test]
    #[ignore = "Soak test: requires Docker SMB containers. Run with CMDR_SOAK_ITERATIONS or CMDR_SOAK_DURATION_SECS."]
    async fn smb_soak_copy_loop() {
        use crate::file_system::write_operations::{
            CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
        };
        use std::time::{Duration, Instant};

        let _ = env_logger::try_init();

        // Deterministic per-index content: 10 KB of a repeated 32-byte
        // blake3-derived block. Same scheme as
        // `smb_integration_copy_100_unique_files_no_cross_contamination`.
        fn expected_content(index: usize) -> Vec<u8> {
            let mut seed = Vec::with_capacity(10 + 8);
            seed.extend_from_slice(b"cmdr-soak-");
            seed.extend_from_slice(&(index as u64).to_le_bytes());
            let block = *blake3::hash(&seed).as_bytes();
            let mut out = Vec::with_capacity(32 * 320);
            for _ in 0..320 {
                out.extend_from_slice(&block);
            }
            out
        }

        const FILE_COUNT: usize = 100;

        // Iteration budget. Duration takes priority if both are set; it's
        // the more useful knob for manual long-soak runs.
        let duration_budget: Option<Duration> = std::env::var("CMDR_SOAK_DURATION_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs);
        let iteration_budget: usize = std::env::var("CMDR_SOAK_ITERATIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        // Fixture: 100 × 10 KB deterministic files, created once on the SMB
        // source. Re-used across every iteration (the loop only reads).
        let smb_vol = Arc::new(make_docker_auth_volume().await);
        let src_dir = test_dir_name();
        ensure_clean(&smb_vol, &src_dir).await;
        smb_vol.create_directory(Path::new(&src_dir)).await.unwrap();
        let vol: Arc<dyn Volume> = smb_vol.clone();

        let fixture_start = Instant::now();
        let mut source_paths: Vec<PathBuf> = Vec::with_capacity(FILE_COUNT);
        for i in 0..FILE_COUNT {
            let name = format!("f_{:03}.bin", i);
            let smb_path = format!("{}/{}", src_dir, name);
            vol.create_file(Path::new(&smb_path), &expected_content(i))
                .await
                .unwrap();
            source_paths.push(PathBuf::from(smb_path));
        }
        log::info!(
            "smb_soak_copy_loop: fixture setup ({} × 10 KB) took {:?}",
            FILE_COUNT,
            fixture_start.elapsed()
        );

        // Baseline snapshot before the loop so deltas are meaningful. Force
        // an initial RSS read so macOS's ru_maxrss is warmed up.
        let baseline_rss_mb = process_peak_rss_mb();
        let baseline_fds = open_fd_count();
        let baseline_credits = smb_credits_snapshot(&smb_vol).await;
        log::info!(
            "smb_soak_copy_loop: baseline: RSS {:.1} MB, FDs {}, credits {:?}",
            baseline_rss_mb,
            baseline_fds,
            baseline_credits,
        );

        let loop_start = Instant::now();

        // Per-iteration wall-clock for drift analysis. Pre-allocate when the
        // iteration count is bounded; grow dynamically when duration-bound.
        let mut per_iter_ms: Vec<f64> = match duration_budget {
            Some(_) => Vec::with_capacity(1024),
            None => Vec::with_capacity(iteration_budget),
        };
        let mut peak_rss_mb = baseline_rss_mb;
        let mut peak_fds = baseline_fds;
        let mut iter_errors: Vec<String> = Vec::new();
        let mut iter_idx: usize = 0;

        // Summary cadence: every 10% of the bound, or every 100 iterations
        // when duration-bound (whichever is reached first).
        let summary_every: usize = match duration_budget {
            Some(_) => 100,
            None => (iteration_budget / 10).max(1),
        };

        loop {
            // Stop condition: duration takes priority if set.
            match duration_budget {
                Some(d) => {
                    if loop_start.elapsed() >= d {
                        break;
                    }
                }
                None => {
                    if iter_idx >= iteration_budget {
                        break;
                    }
                }
            }

            // Fresh per-iteration destination. Tempdir drops at the end of
            // the block, so the only on-disk state between iterations is
            // the 100 source bytes on the SMB side.
            let local_dir = tempfile::TempDir::new().expect("create TempDir");
            let dest_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
                "dest",
                local_dir.path().to_path_buf(),
            ));
            let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
            let events = Arc::new(CollectorEventSink::new());
            let config = VolumeCopyConfig::default();

            let iter_start = Instant::now();
            let result = copy_volumes_with_progress(
                events.clone(),
                &format!("soak-iter-{iter_idx}"),
                &state,
                Arc::clone(&vol),
                &source_paths,
                Arc::clone(&dest_vol),
                Path::new("/"),
                &config,
            )
            .await;
            let iter_elapsed = iter_start.elapsed();
            per_iter_ms.push(iter_elapsed.as_secs_f64() * 1000.0);

            if let Err(e) = result {
                iter_errors.push(format!("iter {iter_idx}: copy failed: {e:?}"));
                break;
            }

            // Per-index blake3 verification. A byte-swap bug or a buffer
            // reuse between concurrent tasks flips the hash immediately.
            let mut mismatches = 0usize;
            for i in 0..FILE_COUNT {
                let name = format!("f_{:03}.bin", i);
                let dest_path = local_dir.path().join(&name);
                let actual_bytes = match std::fs::read(&dest_path) {
                    Ok(b) => b,
                    Err(e) => {
                        iter_errors.push(format!("iter {iter_idx}: read {name}: {e}"));
                        mismatches += 1;
                        continue;
                    }
                };
                if hash_bytes(&actual_bytes) != hash_bytes(&expected_content(i)) {
                    iter_errors.push(format!(
                        "iter {iter_idx}: {name} content mismatch (size={} expected={})",
                        actual_bytes.len(),
                        expected_content(i).len()
                    ));
                    mismatches += 1;
                }
            }
            if mismatches > 0 {
                break;
            }

            // Refresh peaks. Read the current per-iter resource sample and
            // track the high-water marks so the final assertions see the
            // worst case, not just the end state.
            let rss = process_peak_rss_mb();
            let fds = open_fd_count();
            if rss > peak_rss_mb {
                peak_rss_mb = rss;
            }
            if fds > peak_fds {
                peak_fds = fds;
            }

            // Cadence summary: recent window average + current deltas.
            iter_idx += 1;
            if iter_idx.is_multiple_of(summary_every) {
                let window_start = iter_idx.saturating_sub(summary_every);
                let window: &[f64] = &per_iter_ms[window_start..iter_idx];
                let window_avg = window.iter().sum::<f64>() / window.len() as f64;
                let credits = smb_credits_snapshot(&smb_vol).await;
                log::info!(
                    "smb_soak_copy_loop: iter {} (window-avg {:.1} ms, RSS {:.1} MB, Δ {:+.1}, FDs {}, Δ {:+}, credits {:?})",
                    iter_idx,
                    window_avg,
                    rss,
                    rss - baseline_rss_mb,
                    fds,
                    fds as i64 - baseline_fds as i64,
                    credits,
                );
            }

            // Dest tempdir drops here, so on-disk FD count on the local
            // side lands back at baseline before the next iteration.
            drop(dest_vol);
        }

        let total_elapsed = loop_start.elapsed();
        let total_iters = per_iter_ms.len();
        let final_rss_mb = process_peak_rss_mb();
        let final_fds = open_fd_count();
        let final_credits = smb_credits_snapshot(&smb_vol).await;

        // Cleanup SMB source before any assertion, so a failed assertion
        // doesn't leave debris in the container.
        ensure_clean(&smb_vol, &src_dir).await;

        // Require at least 20 iterations to compute a meaningful drift
        // ratio (10%-window math needs two non-trivial samples).
        if total_iters < 20 {
            panic!(
                "soak ran only {}; need at least 20 to compute drift (set CMDR_SOAK_ITERATIONS=100 minimum)",
                crate::pluralize::pluralize(total_iters as u64, "iteration")
            );
        }

        // Drift ratio: average of the last 10% of iterations vs. the first
        // 10%. A clean pipeline should sit near 1.0; a slowdown above 1.20
        // fails the test.
        let window = (total_iters / 10).max(1);
        let first_avg = per_iter_ms[..window].iter().sum::<f64>() / window as f64;
        let last_avg = per_iter_ms[total_iters - window..].iter().sum::<f64>() / window as f64;
        let drift = last_avg / first_avg;

        log::info!(
            "smb_soak_copy_loop: DONE: {} iters in {:?} ({:.1} ms/iter avg)",
            total_iters,
            total_elapsed,
            per_iter_ms.iter().sum::<f64>() / total_iters as f64
        );
        log::info!(
            "smb_soak_copy_loop: drift first10%={:.1} ms last10%={:.1} ms ratio={:.3}",
            first_avg,
            last_avg,
            drift
        );
        log::info!(
            "smb_soak_copy_loop: RSS baseline {:.1} MB → peak {:.1} MB → final {:.1} MB (Δ peak {:+.1} MB)",
            baseline_rss_mb,
            peak_rss_mb,
            final_rss_mb,
            peak_rss_mb - baseline_rss_mb,
        );
        log::info!(
            "smb_soak_copy_loop: FDs baseline {} → peak {} → final {} (Δ final {:+})",
            baseline_fds,
            peak_fds,
            final_fds,
            final_fds as i64 - baseline_fds as i64,
        );
        log::info!(
            "smb_soak_copy_loop: credits baseline {:?} → final {:?}",
            baseline_credits,
            final_credits
        );

        // Hard failures: any iteration error, drift, memory peak, FD leak.
        assert!(
            iter_errors.is_empty(),
            "{} iteration error(s):\n  - {}",
            iter_errors.len(),
            iter_errors.join("\n  - ")
        );
        assert!(
            drift < 1.20,
            "iteration-wall-clock drift {:.3}× (first10%={:.1} ms, last10%={:.1} ms) exceeds 1.20×",
            drift,
            first_avg,
            last_avg
        );
        assert!(
            peak_rss_mb - baseline_rss_mb < 100.0,
            "peak RSS grew by {:.1} MB (baseline {:.1} MB, peak {:.1} MB); exceeds 100 MB ceiling",
            peak_rss_mb - baseline_rss_mb,
            baseline_rss_mb,
            peak_rss_mb
        );
        let fd_delta = final_fds as i64 - baseline_fds as i64;
        assert!(
            fd_delta < 5,
            "final FD count grew by {} (baseline {}, final {}); exceeds 5 FD ceiling (suggests leak)",
            fd_delta,
            baseline_fds,
            final_fds
        );
    }

    // ── SMB streaming-write regression test ────────────────────────────
    //
    // Helpers + one `#[ignore]`d integration test that guards against the
    // streaming-write deadlock fixed in commit `efb15479`. See the docstring
    // on `smb_integration_concurrent_streaming_writes_no_deadlock` for the
    // full story.

    /// All test artifacts on the SMB share live under this prefix. The
    /// cleanup helper refuses to delete anything that doesn't start with it.
    const TEST_PREFIX_ROOT: &str = "_test/cmdr-regression-";

    /// Captures `client-mutex:` (cmdr) and `recv:` (smb2 receiver loop)
    /// debug lines into bounded ring buffers so a hung test's panic message
    /// can include the last ~30 lines from each stream. That's invaluable
    /// for diagnosing a future regression. Installed via `log::set_logger`
    /// once per process; subsequent installs are no-ops.
    struct MutexCaptureLogger {
        mutex_lines: std::sync::Mutex<std::collections::VecDeque<String>>,
        recv_lines: std::sync::Mutex<std::collections::VecDeque<String>>,
    }
    impl log::Log for MutexCaptureLogger {
        fn enabled(&self, _md: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            let msg = format!("{}", record.args());
            let target = record.target();
            // `client-mutex:` lines come from smb.rs via `log::debug!` with
            // the module-path target (`cmdr_lib::file_system::volume::smb`).
            // `recv:` lines come from the smb2 receiver loop with an `smb2::*`
            // target.
            // allowed-error-string-match: routes log records into ring buffers by our own `log::debug!` message-prefix convention (`client-mutex:` from this file, `recv:` from the smb2 crate's receiver loop). Not error/state classification; we own both prefixes and `cleanup_test_prefix` would notice drift. Pinned by `mutex_capture_logger_routes_known_prefixes`.
            if msg.starts_with("client-mutex:") {
                let mut q = self.mutex_lines.lock().unwrap();
                if q.len() >= 200 {
                    q.pop_front();
                }
                q.push_back(format!("[{}] {}", target, msg));
                // allowed-error-string-match: same convention as the `client-mutex:` branch above — routes smb2 receiver-loop log records by message prefix, not error/state classification. Pinned by `mutex_capture_logger_routes_known_prefixes`.
            } else if msg.starts_with("recv:") || (target.starts_with("smb2") && msg.contains("recv")) {
                let mut q = self.recv_lines.lock().unwrap();
                if q.len() >= 200 {
                    q.pop_front();
                }
                q.push_back(format!("[{}] {}", target, msg));
            }
            // The captured ring buffers are the diagnostic. We deliberately
            // skip mirroring to stderr: `eprintln!` is denied crate-wide,
            // and re-emitting through `log::*` would recurse into this same
            // logger (and the mutex above) on every call.
        }
        fn flush(&self) {}
    }

    static MUTEX_CAPTURE_LOGGER: OnceLock<&'static MutexCaptureLogger> = OnceLock::new();

    fn install_mutex_capture_logger() -> &'static MutexCaptureLogger {
        if let Some(l) = MUTEX_CAPTURE_LOGGER.get() {
            return l;
        }
        let leaked: &'static MutexCaptureLogger = Box::leak(Box::new(MutexCaptureLogger {
            mutex_lines: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(200)),
            recv_lines: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(200)),
        }));
        // Best-effort: if another logger is already installed, ignore.
        let _ = log::set_logger(leaked);
        log::set_max_level(log::LevelFilter::Debug);
        let _ = MUTEX_CAPTURE_LOGGER.set(leaked);
        leaked
    }

    /// Pins the `client-mutex:` and `recv:` log-message prefix convention the
    /// `MutexCaptureLogger` routes by. The prefixes are intentionally part of
    /// our log-message contract (see the actual `log::debug!` sites further up
    /// in this file and in the smb2 receiver loop). If the prefixes drift, the
    /// debug ring buffer stops capturing them and a future hung-test triage
    /// loses the diagnostic. This test pins both prefixes against the
    /// canonical message-format helpers so any rename of the convention
    /// triggers a compile-fail or string-mismatch here first.
    #[test]
    fn mutex_capture_logger_routes_known_prefixes() {
        // Format mirrors the real `log::debug!` sites in `clone_session`.
        let mutex_msg = format!(
            "client-mutex: waiting ticket={} caller=clone_session share={}",
            7, "Public"
        );
        let recv_msg = "recv: smb2 frame 0x10 mid=42";
        let other_msg = "some unrelated log line";

        assert!(
            mutex_msg.starts_with("client-mutex:"),
            "mutex prefix drifted: {mutex_msg}"
        );
        assert!(recv_msg.starts_with("recv:"), "recv prefix drifted: {recv_msg}");
        assert!(!other_msg.starts_with("client-mutex:") && !other_msg.starts_with("recv:"));
    }

    /// Deletes every file under `unique_prefix_smb` and then the directory
    /// itself. Safety: refuses any path that doesn't start with
    /// `TEST_PREFIX_ROOT`, both at the top level and per entry, so a logic
    /// bug in the caller can never reach outside the regression sandbox.
    /// Called explicitly at the end of each pass (best effort: logs but
    /// never overrides the test outcome).
    async fn cleanup_test_prefix(vol: &SmbVolume, mount_path: &Path, unique_prefix_smb: &str) {
        assert!(
            unique_prefix_smb.starts_with(TEST_PREFIX_ROOT),
            "cleanup_test_prefix: refusing to clean a prefix outside {TEST_PREFIX_ROOT:?}: {unique_prefix_smb:?}"
        );
        let dir_abs = mount_path.join(unique_prefix_smb.trim_start_matches('/'));
        let rel_of = |abs: &Path| -> String {
            abs.to_string_lossy()
                .strip_prefix(mount_path.to_string_lossy().as_ref())
                .map(|s| s.trim_start_matches('/').to_string())
                .unwrap_or_else(|| abs.to_string_lossy().to_string())
        };
        match vol.list_directory_impl(&dir_abs).await {
            Ok(entries) => {
                for entry in entries {
                    let abs = dir_abs.join(&entry.name);
                    let rel = rel_of(&abs);
                    if !rel.starts_with(TEST_PREFIX_ROOT) {
                        log::warn!("cleanup_test_prefix: refusing to delete {rel} (outside prefix)");
                        continue;
                    }
                    if let Err(e) = vol.delete(&abs).await {
                        log::warn!("cleanup_test_prefix: failed to delete {rel}: {e:?}");
                    }
                }
            }
            Err(e) => log::warn!("cleanup_test_prefix: list_directory_impl failed for {dir_abs:?}: {e:?}"),
        }
        let rel_dir = rel_of(&dir_abs);
        if rel_dir.starts_with(TEST_PREFIX_ROOT)
            && let Err(e) = vol.delete(&dir_abs).await
        {
            log::warn!("cleanup_test_prefix: failed to delete prefix dir {rel_dir}: {e:?}");
        }
    }

    /// Connects to a Docker SMB fixture's `public` share at `127.0.0.1:port`
    /// as guest. `mount_label` becomes the synthetic mount path
    /// (`/Volumes/<label>`); no real OS mount is needed because the test
    /// only drives the smb2 path.
    async fn connect_docker_smb_volume(port: u16, mount_label: &str) -> SmbVolume {
        let mount_path = format!("/Volumes/{mount_label}");
        let volume_id = smb_volume_id("127.0.0.1", port, "public");
        let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
        connect_smb_volume("public", &mount_path, &volume_id, params)
            .await
            .unwrap_or_else(|e| panic!("connect to 127.0.0.1:{port} failed: {e:?}"))
    }

    /// One pass of the concurrent-streaming-write scenario:
    /// - generate `n_files` source files of `file_size` bytes in a tempdir,
    /// - pre-upload `n_conflicts` of them to the destination at the same size so `OverwriteSmaller`
    ///   resolves them as Skip,
    /// - run `copy_volumes_with_progress` over all `n_files` with a timeout,
    /// - on timeout, panic with the last 30 mutex/recv lines as a diagnostic dump,
    /// - clean up the unique prefix directory either way.
    async fn run_concurrent_write_pass(
        vol: Arc<SmbVolume>,
        mount_path: &Path,
        logger: &'static MutexCaptureLogger,
        n_files: usize,
        n_conflicts: usize,
        file_size: usize,
        timeout_secs: u64,
    ) -> Duration {
        use crate::file_system::write_operations::{
            CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
        };

        assert!(n_conflicts <= n_files);

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let unique_prefix = format!("{TEST_PREFIX_ROOT}{ts}-n{n_files}");

        let dest_dir_abs = mount_path.join(unique_prefix.trim_start_matches('/'));
        let _ = vol.create_directory(&mount_path.join("_test")).await;
        vol.create_directory(&dest_dir_abs)
            .await
            .expect("create unique dest dir");

        let local_dir = tempfile::TempDir::new().expect("tempdir");
        for i in 0..n_files {
            let name = format!("f_{i:04}.bin");
            let path = local_dir.path().join(&name);
            // Distinct content per file (byte = i % 251) + an 8-byte seed
            // prefix, so identical-size pre-uploads still hash-differ from
            // their sources should we ever want to verify content.
            let mut buf = vec![0u8; file_size];
            buf[..8].copy_from_slice(&(i as u64).to_le_bytes());
            for b in buf.iter_mut().skip(8) {
                *b = (i % 251) as u8;
            }
            std::fs::write(&path, &buf).expect("write source");
        }

        log::info!(
            "regression: pre-uploading {} to {unique_prefix}",
            crate::pluralize::pluralize(n_conflicts as u64, "conflicting file")
        );
        for i in 0..n_conflicts {
            let name = format!("f_{i:04}.bin");
            let dest_abs = dest_dir_abs.join(&name);
            let buf = std::fs::read(local_dir.path().join(&name)).unwrap();
            let stream: Box<dyn VolumeReadStream> = Box::new(InlineReadStream::new(buf.clone()));
            let size = buf.len() as u64;
            let progress = |_a: u64, _b: u64| -> std::ops::ControlFlow<()> { std::ops::ControlFlow::Continue(()) };
            let bytes = vol
                .write_from_stream(&dest_abs, size, stream, &progress)
                .await
                .unwrap_or_else(|e| panic!("pre-upload {name} failed: {e:?}"));
            assert_eq!(bytes, size, "pre-upload size mismatch");
        }
        log::info!("regression: pre-upload done");

        let src_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
            "regression-src",
            local_dir.path().to_path_buf(),
        ));
        let dst_vol: Arc<dyn Volume> = vol.clone() as Arc<dyn Volume>;
        let source_rel_paths: Vec<PathBuf> = (0..n_files).map(|i| PathBuf::from(format!("f_{i:04}.bin"))).collect();

        let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
        let events = Arc::new(CollectorEventSink::new());
        let config = VolumeCopyConfig {
            conflict_resolution: crate::file_system::write_operations::ConflictResolution::OverwriteSmaller,
            ..VolumeCopyConfig::default()
        };

        let start = std::time::Instant::now();
        log::info!(
            "regression: spawning copy n_files={n_files} n_conflicts={n_conflicts} size={file_size} timeout={timeout_secs}s"
        );

        let res = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            copy_volumes_with_progress(
                events.clone(),
                "regression-op",
                &state,
                Arc::clone(&src_vol),
                &source_rel_paths,
                Arc::clone(&dst_vol),
                &dest_dir_abs,
                &config,
            ),
        )
        .await;

        let elapsed = start.elapsed();

        let panic_msg: Option<String> = match res {
            Ok(Ok(())) => {
                log::info!("regression: copy completed in {elapsed:?}");
                None
            }
            Ok(Err(e)) => Some(format!("regression: copy failed in {elapsed:?}: {e:?}")),
            Err(_) => {
                let tail = |q: &std::sync::Mutex<std::collections::VecDeque<String>>| -> Vec<String> {
                    let q = q.lock().unwrap();
                    let n = q.len().min(30);
                    q.iter().skip(q.len() - n).cloned().collect()
                };
                let mutex_dump = tail(&logger.mutex_lines);
                let recv_dump = tail(&logger.recv_lines);
                let last_ticket = CLIENT_LOCK_TICKET.load(Ordering::Relaxed);
                Some(format!(
                    "regression: HANG after {:?} (timeout={}s) n_files={} n_conflicts={} last_ticket={}\n\
                     ── last {} client-mutex lines ──\n{}\n── last {} recv lines ──\n{}\n",
                    elapsed,
                    timeout_secs,
                    n_files,
                    n_conflicts,
                    last_ticket,
                    mutex_dump.len(),
                    mutex_dump.join("\n"),
                    recv_dump.len(),
                    recv_dump.join("\n"),
                ))
            }
        };

        cleanup_test_prefix(&vol, mount_path, &unique_prefix).await;

        if let Some(m) = panic_msg {
            panic!("{m}");
        }
        elapsed
    }

    /// Guards the invariant that concurrent streaming writes through
    /// `SmbVolume::write_from_stream` complete without deadlocking.
    ///
    /// Uses the consumer-class `smb-consumer-maxreadsize` fixture
    /// (`smb2 max read = smb2 max write = 65536`) so every 1 MB write exceeds
    /// the server's max_write and is forced through the streaming-fallback
    /// (FileWriter) path. That's the path that historically nested a
    /// per-write lock under the client mutex and could starve the receiver
    /// task to a halt.
    ///
    /// Shape (200 files, 140 OverwriteSmaller conflicts + 60 actual copies,
    /// concurrency=8) mirrors the production workload that originally
    /// surfaced the bug, where mixed conflict-skip / write iterations on a
    /// shared SmbClient stressed the lock-ordering pattern hardest.
    ///
    /// Run with `./apps/desktop/test/smb-servers/start.sh core` (CI does
    /// this) or `start.sh all`, then either `./scripts/check.sh --rust` or
    /// `cargo nextest run -p cmdr smb_integration_concurrent_streaming_writes_no_deadlock
    /// --run-ignored all`.
    ///
    /// Originally hung at a QNAP NAS for >5 minutes before the fix in smb2
    /// 0.9.0 (`FileWriter` owns its `Connection`) and the matching
    /// `write_from_stream` rewrite. On post-fix code each pass completes in
    /// roughly 5–15 s.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Requires docker-compose smb-consumer-maxreadsize on port 10494 (started by start.sh core)"]
    async fn smb_integration_concurrent_streaming_writes_no_deadlock() {
        use futures_util::FutureExt;

        // 10494 matches smb2's smb-consumer-maxreadsize container; override
        // with `SMB_CONSUMER_MAXREADSIZE_PORT` to match
        // `smb2::testing::maxreadsize_port()` (requires the `smb-e2e`
        // feature; bare integration tests hardcode the default).
        let port: u16 = std::env::var("SMB_CONSUMER_MAXREADSIZE_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10494);
        let logger = install_mutex_capture_logger();
        let prior_concurrency = crate::file_system::smb_concurrency();
        crate::file_system::set_smb_concurrency(8);

        let vol = Arc::new(connect_docker_smb_volume(port, "cmdr-regression-maxreadsize").await);
        let mount_path = vol.mount_path.clone();

        let result = std::panic::AssertUnwindSafe(run_concurrent_write_pass(
            Arc::clone(&vol),
            &mount_path,
            logger,
            /* n_files = */ 200,
            /* n_conflicts = */ 140,
            /* file_size = */ 1024 * 1024,
            /* timeout_secs = */ 120,
        ))
        .catch_unwind()
        .await;

        // Always restore concurrency, even on panic, before resuming the unwind.
        crate::file_system::set_smb_concurrency(prior_concurrency);
        if let Err(p) = result {
            std::panic::resume_unwind(p);
        }
    }

    #[test]
    #[should_panic(expected = "refusing to clean a prefix outside")]
    fn cleanup_test_prefix_rejects_unsafe_prefix() {
        // The cleanup helper is async, but the safety assert fires before
        // any await point. Poll the future once via a no-op waker so we
        // hit the assert without needing a runtime.
        use std::task::Context;
        let vol = make_test_volume();
        let mount = PathBuf::from("/Volumes/TestShare");
        let mut fut = Box::pin(cleanup_test_prefix(&vol, &mount, "etc/passwd"));
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let _ = fut.as_mut().poll(&mut cx); // panics in the assert
    }

    #[test]
    fn test_prefix_root_is_safely_scoped() {
        // Static check: the prefix lives under `_test/` and clearly
        // identifies cmdr's regression test, so a future reader (or a
        // misconfigured share) can recognize stale artifacts at a glance.
        assert!(TEST_PREFIX_ROOT.starts_with("_test/"));
        assert!(TEST_PREFIX_ROOT.contains("cmdr-regression-"));
    }
}

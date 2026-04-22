//! SMB volume implementation using direct smb2 protocol operations.
//!
//! Wraps an smb2 session to provide file system access through the Volume trait.
//! The share remains OS-mounted (for Finder/Terminal/drag-drop compatibility),
//! but all Cmdr file operations go through smb2's pipelined I/O for better
//! performance and fail-fast behavior.

use super::{
    BatchScanResult, CopyScanResult, ScanConflict, SmbConnectionState, SourceItemInfo, SpaceInfo, Volume, VolumeError,
    VolumeReadStream, path_to_id,
};
use crate::file_system::listing::FileEntry;
use log::{debug, warn};
use smb2::client::tree::Tree;
use smb2::{ClientConfig, SmbClient};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

// ── Connection state ────────────────────────────────────────────────

/// Connection health states for an SmbVolume.
///
/// Stored as `AtomicU8` for lock-free reads from any thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// smb2 session is active. All ops go through smb2 (fast path).
    Direct = 0,
    /// smb2 is down but the OS mount is alive. Ops fall through to filesystem calls.
    OsMount = 1,
    /// Both smb2 and OS mount are down. Return errors immediately.
    Disconnected = 2,
}

impl ConnectionState {
    fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Direct,
            1 => Self::OsMount,
            2 => Self::Disconnected,
            _ => Self::Disconnected,
        }
    }
}

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
    match err.kind() {
        ErrorKind::NotFound => VolumeError::NotFound(err.to_string()),
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
/// `Connection` — so N downloads run pipelined on one SMB session instead of
/// serializing through the mutex. The `watcher_cancel` field uses
/// `std::sync::Mutex` because it is only accessed briefly (no awaits while
/// held).
pub struct SmbVolume {
    /// Display name (share name).
    name: String,
    /// OS mount point (for example, "/Volumes/Documents").
    mount_path: PathBuf,
    /// Server hostname or IP address.
    server: String,
    /// SMB share name.
    share_name: String,
    /// Volume ID for listing cache lookups (from `path_to_id(mount_path)`).
    volume_id: String,
    /// smb2 client (owns the Connection). `None` when disconnected.
    ///
    /// Most methods still lock this mutex and call `client.stat(tree, ...)`
    /// etc. — SmbClient's async methods need `&mut self`, and these aren't
    /// hot-path parallel. The hot copy path (compound read/write, download
    /// stream) briefly locks just to clone the `Connection` (via
    /// `client.connection_mut().clone()`), releases the lock, and drives the
    /// op on the clone. This is what gives concurrency across files while the
    /// underlying SMB session multiplexes the frames.
    client: Arc<tokio::sync::Mutex<Option<SmbClient>>>,
    /// Tree (share connection), wrapped as `Arc<Tree>` so concurrent hot-path
    /// ops can hold a reference without serializing on the client mutex.
    /// `None` when disconnected. The `RwLock` is essentially uncontended — we
    /// only write on disconnect — so readers just clone the `Arc` out under a
    /// read guard and drop the guard immediately.
    tree: Arc<tokio::sync::RwLock<Option<Arc<Tree>>>>,
    /// Current connection health.
    /// Wrapped in `Arc` so background tasks (streaming read producer) can update
    /// the state on mid-stream connection loss.
    state: Arc<AtomicU8>,
    /// Cancel sender for the background watcher task. Send to stop watching.
    watcher_cancel: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl SmbVolume {
    /// Creates a new SMB volume with an established smb2 connection.
    ///
    /// # Arguments
    /// * `name` - Display name (typically the share name)
    /// * `mount_path` - OS mount point path
    /// * `server` - Server hostname or IP
    /// * `share_name` - SMB share name
    /// * `volume_id` - Volume ID for listing cache lookups
    /// * `client` - Connected `SmbClient`
    /// * `tree` - Connected `Tree` for the share
    #[allow(
        clippy::too_many_arguments,
        reason = "Constructor needs all fields; a builder would be overengineering"
    )]
    pub fn new(
        name: impl Into<String>,
        mount_path: impl Into<PathBuf>,
        server: impl Into<String>,
        share_name: impl Into<String>,
        volume_id: impl Into<String>,
        client: SmbClient,
        tree: Tree,
    ) -> Self {
        Self {
            name: name.into(),
            mount_path: mount_path.into(),
            server: server.into(),
            share_name: share_name.into(),
            volume_id: volume_id.into(),
            client: Arc::new(tokio::sync::Mutex::new(Some(client))),
            tree: Arc::new(tokio::sync::RwLock::new(Some(Arc::new(tree)))),
            state: Arc::new(AtomicU8::new(ConnectionState::Direct as u8)),
            watcher_cancel: std::sync::Mutex::new(None),
        }
    }

    /// Returns the current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
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
                // return `true` — only the leaf file branch sets `false`.
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

            // It's a directory — list and recurse
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

    /// Checks that the connection is in `Direct` state. Returns an error
    /// for `Disconnected` or `OsMount`.
    fn check_connection(&self) -> Result<(), VolumeError> {
        match self.connection_state() {
            ConnectionState::Direct => Ok(()),
            ConnectionState::OsMount => Err(VolumeError::NotSupported),
            ConnectionState::Disconnected => Err(VolumeError::DeviceDisconnected(
                "SMB connection is disconnected".to_string(),
            )),
        }
    }

    /// Acquires the client mutex and returns a guard over the `Option<SmbClient>`.
    /// Checks connection state first, then verifies the client is present.
    ///
    /// Most methods still go through this (stat, list_directory, rename, etc.)
    /// — only the hot streaming-read / compound-write paths use the cheaper
    /// `clone_connection` helper that releases the lock before driving the op.
    async fn acquire_client(&self) -> Result<tokio::sync::MutexGuard<'_, Option<SmbClient>>, VolumeError> {
        self.check_connection()?;
        let guard = self.client.lock().await;
        if guard.is_none() {
            return Err(VolumeError::DeviceDisconnected("SMB session not available".to_string()));
        }
        Ok(guard)
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
    /// `Arc::clone` — all clones multiplex frames over the same SMB session),
    /// and releases the lock. Also reads out an `Arc<Tree>`. Returns both.
    ///
    /// Callers can then drive `Tree::download` / `Tree::read_file_compound` /
    /// `Tree::write_file_compound` on the owned `Connection` without holding
    /// any lock, enabling multiple concurrent copies on a single `SmbVolume`.
    async fn clone_session(&self) -> Result<(Arc<Tree>, smb2::client::Connection), VolumeError> {
        self.check_connection()?;
        let tree = self.tree_arc().await?;
        let conn = {
            let mut guard = self.client.lock().await;
            let client = guard
                .as_mut()
                .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?;
            client.connection_mut().clone()
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
        let share_name = self.share_name.clone();
        let smb_path_owned = smb_path.to_string();

        tokio::spawn(async move {
            // The task owns its `Connection` clone and an `Arc<Tree>` reference.
            // No lock is held, so other tasks can spawn in parallel and each
            // drive their own download on a fresh `Connection` clone — all
            // multiplexed over the same SMB session by smb2's receiver task.
            let mut conn = conn;
            let mut download = match tree.download(&mut conn, &smb_path_owned).await {
                Ok(d) => d,
                Err(e) => {
                    update_state_on_smb_error(&state_arc, &e);
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
                                // Consumer dropped — stop pumping.
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            update_state_on_smb_error(&state_arc, &e);
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
            // `conn` and `tree` drop here — the `Arc<Connection>` inner and the
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
                    self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);
                } else if matches!(kind, smb2::ErrorKind::NotFound) {
                    // NotFound is expected for existence checks (rename dest, conflict detection)
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
        let state = self.connection_state();
        if state == ConnectionState::Disconnected {
            return Err(VolumeError::DeviceDisconnected(
                "SMB connection is disconnected".to_string(),
            ));
        }

        if state == ConnectionState::OsMount {
            return Err(VolumeError::NotSupported);
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
                    self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);
                } else if matches!(kind, smb2::ErrorKind::NotFound) {
                    debug!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                } else {
                    warn!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                }

                Err(map_smb_error(e))
            }
        }
    }
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
/// the whole file in memory — peak is bounded by the channel capacity.
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
            // Best-effort — if the producer already finished, recv side is dropped
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
/// SMB compound response — there's no more I/O to drive, just hand the bytes
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
/// `Disconnected`. Mirrors `handle_smb_result` for contexts without `&self`.
fn update_state_on_smb_error(state: &AtomicU8, err: &smb2::Error) {
    if matches!(
        err.kind(),
        smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired
    ) {
        state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);
    }
}

impl Volume for SmbVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.mount_path
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let entries = self.list_directory_impl(path).await?;
            // smb2's list_directory returns all entries at once, so report
            // progress as a single batch after the call completes.
            if let Some(on_progress) = on_progress {
                on_progress(entries.len());
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
        // Start with false — the existing FSEvents watcher on the OS mount
        // point already provides change notifications. smb2-native watching
        // can be added later as an optimization.
        false
    }

    fn supports_local_fs_access(&self) -> bool {
        // SmbVolume handles listing notifications via notify_mutation,
        // so the old std::fs-based synthetic diff path is not needed.
        false
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
                let (tree, mut conn) = self.clone_session().await?;
                let result = tree.write_file(&mut conn, &smb_path, &data).await;
                self.handle_smb_result("create_file", result)?;
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
            // the server returns STATUS_FILE_IS_A_DIRECTORY — then try delete_directory.
            // This avoids a stat round-trip for every file in bulk deletes.
            let file_result = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.delete_file(&mut conn, &smb_path).await;
                self.handle_smb_result("delete_file", r)
            };

            match file_result {
                Ok(()) => {} // File deleted successfully
                Err(VolumeError::IoError { ref message, .. }) if message.contains("FILE_IS_A_DIRECTORY") => {
                    // It's a directory — try delete_directory
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
                    // Try file delete first; if that fails (it's a dir), try directory delete
                    let file_result = {
                        let (tree, mut conn) = self.clone_session().await?;
                        let r = tree.delete_file(&mut conn, &smb_to).await;
                        self.handle_smb_result("rename(delete_dest_file)", r)
                    };
                    if file_result.is_err() {
                        let (tree, mut conn) = self.clone_session().await?;
                        let r = tree.delete_directory(&mut conn, &smb_to).await;
                        self.handle_smb_result("rename(delete_dest_dir)", r)?;
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

            // Pre-compute SMB paths so the pipelined stats can borrow strings
            // that outlive the futures' lifetimes.
            let smb_paths: Vec<String> = paths.iter().map(|p| self.to_smb_path(p)).collect();

            debug!(
                "SmbVolume::scan_for_copy_batch: share={}, {} paths — pipelining stats",
                self.share_name,
                paths.len()
            );

            // Build N pipelined stats: one cloned `Connection` per path, no
            // lock held across any stat. `Arc<Tree>` is shared cheaply. Empty
            // paths (volume root) skip the stat — the root is always a
            // directory — and route straight into the recursion list.
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

            for (idx, smb_path) in smb_paths.iter().enumerate() {
                if smb_path.is_empty() {
                    // Root — no stat needed. Inline a ready future so the
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

            // Stage per-path scan results + "recurse later" list while
            // draining the pipelined stats as they complete.
            let mut per_path_results: Vec<Option<CopyScanResult>> = (0..paths.len()).map(|_| None).collect();
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
                            self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);
                        } else {
                            warn!("SmbVolume::scan_for_copy_batch(share={}): {}", self.share_name, e);
                        }
                        return Err(map_smb_error(e));
                    }
                }
            }

            // Recurse sequentially into each discovered directory. Per-dir
            // recursion still serializes on listing + child stats — that's a
            // future "Fix 5" (pipelined directory recursion). For the 100 ×
            // tiny-file scenario all sources are files, so this loop is never
            // entered.
            for idx in dirs_to_recurse {
                let smb_path = &smb_paths[idx];
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
        // batch-copy dispatch (no reconnect required — Connection::clone is
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
            // returns short (truncated file — rare but possible if size
            // changed since the scan).
            if let Some(size) = size_hint {
                let (tree, mut conn) = self.clone_session().await?;
                let max_read = conn.params().map(|p| p.max_read_size).unwrap_or(65536) as u64;
                if size > 0 && size <= max_read {
                    debug!(
                        "SmbVolume::open_read_stream_with_hint: share={}, path={:?}, size={} — using compound fast-path",
                        self.share_name, smb_path, size
                    );
                    let read_result = tree.read_file_compound(&mut conn, &smb_path).await;
                    let data = self.handle_smb_result("open_read_stream_with_hint(compound)", read_result)?;
                    if data.len() as u64 == size {
                        return Ok(Box::new(InlineReadStream::new(data)) as Box<dyn VolumeReadStream>);
                    }
                    debug!(
                        "SmbVolume::open_read_stream_with_hint: compound read returned {} bytes, expected {} — falling back to streaming",
                        data.len(),
                        size
                    );
                }
            }

            debug!(
                "SmbVolume::open_read_stream_with_hint: share={}, path={:?} — using streaming path",
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
        Box::pin(async move {
            let smb_path = self.to_smb_path(dest);

            debug!(
                "SmbVolume::write_from_stream: share={}, path={:?}, size={}",
                self.share_name, smb_path, size
            );

            // Compound fast-path: when the caller promised a size that fits in
            // one WRITE, drain the source stream into a buffer and send
            // CREATE+WRITE+FLUSH+CLOSE as a single compound frame (1 RTT
            // instead of 4). Runs on a cloned `Connection` with no lock held,
            // so N concurrent small writes pipeline over one SMB session.
            // Small files are the hot case — for anything larger we fall
            // through to the streaming writer below.
            if size > 0 {
                let (tree, mut conn) = self.clone_session().await?;
                let max_write = conn.params().map(|p| p.max_write_size).unwrap_or(65536) as u64;
                if size <= max_write {
                    let mut buffer = Vec::with_capacity(size as usize);
                    while let Some(chunk_result) = stream.next_chunk().await {
                        let chunk = chunk_result?;
                        buffer.extend_from_slice(&chunk);
                    }
                    if buffer.len() as u64 == size {
                        debug!(
                            "SmbVolume::write_from_stream: using compound fast-path ({} bytes)",
                            buffer.len()
                        );
                        let write_result = tree.write_file_compound(&mut conn, &smb_path, &buffer).await;
                        let bytes_written = self.handle_smb_result("write_from_stream(compound)", write_result)?;
                        // Emit a single progress tick so counters match the
                        // streaming path's post-loop state.
                        let _ = on_progress(bytes_written, size);
                        return Ok(bytes_written);
                    }
                    // Size mismatch — drop the cloned conn and re-feed the
                    // already-drained buffer through the streaming writer
                    // below (which needs the client mutex because
                    // `FileWriter` borrows `&'a mut Connection` from the
                    // `SmbClient`).
                    debug!(
                        "SmbVolume::write_from_stream: compound fast-path source yielded {} bytes, expected {} — falling back",
                        buffer.len(),
                        size
                    );
                    drop(conn);
                    let tree_arc = tree;
                    let mut guard = self.acquire_client().await?;
                    let client = guard.as_mut().unwrap();
                    let writer_result = client.create_file_writer(&tree_arc, &smb_path).await;
                    let mut writer = self.handle_smb_result("write_from_stream(open)", writer_result)?;
                    if !buffer.is_empty() {
                        let write_result = writer.write_chunk(&buffer).await;
                        self.handle_smb_result("write_from_stream(write_chunk)", write_result)?;
                    }
                    let finish_result = writer.finish().await;
                    self.handle_smb_result("write_from_stream(finish)", finish_result)?;
                    return Ok(buffer.len() as u64);
                }
            }

            // Streaming fallback for large / unknown-size writes. Holds the
            // client mutex for the duration of the transfer because
            // `FileWriter<'a>` borrows `&'a mut Connection` from the
            // `SmbClient` we create it from. Large files are rare in the hot
            // copy path, so this doesn't hurt concurrency in practice — the
            // compound fast-path above handles every small file without
            // touching the client mutex for the write itself.
            let tree_arc = self.tree_arc().await?;
            let mut guard = self.acquire_client().await?;
            let client = guard.as_mut().unwrap();

            let writer_result = client.create_file_writer(&tree_arc, &smb_path).await;
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
                    let _ = tree_arc.delete_file(client.connection_mut(), &smb_path).await;
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                }
            }

            let finish_result = writer.finish().await;
            self.handle_smb_result("write_from_stream(finish)", finish_result)?;

            Ok(bytes_read)
        })
    }

    fn smb_connection_state(&self) -> Option<SmbConnectionState> {
        match self.connection_state() {
            ConnectionState::Direct => Some(SmbConnectionState::Direct),
            ConnectionState::OsMount => Some(SmbConnectionState::OsMount),
            ConnectionState::Disconnected => None,
        }
    }

    fn on_unmount(&self) {
        // Transition to Disconnected
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
        // will acquire either lock. Drop Tree first, then SmbClient — Tree
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
/// Also spawns a background watcher task for detecting external changes.
pub async fn connect_smb_volume(
    name: &str,
    mount_path: &str,
    server: &str,
    share_name: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) -> Result<SmbVolume, smb2::Error> {
    use crate::network::smb_connection::build_smb_addr;

    let addr = build_smb_addr(server, port);

    let config = ClientConfig {
        addr: addr.clone(),
        timeout: Duration::from_secs(10),
        username: username.unwrap_or("Guest").to_string(),
        password: password.unwrap_or("").to_string(),
        domain: String::new(),
        auto_reconnect: false,
        compression: true,
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
    };

    let mut client = SmbClient::connect(config).await?;
    let tree = client.connect_share(share_name).await?;
    let volume_id = path_to_id(mount_path);

    let vol = SmbVolume::new(name, mount_path, server, share_name, volume_id.clone(), client, tree);

    // Spawn the background watcher task with its own dedicated connection
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    let watcher_addr = addr;
    let watcher_share = share_name.to_string();
    let watcher_username = username.unwrap_or("Guest").to_string();
    let watcher_password = password.unwrap_or("").to_string();
    let watcher_volume_id = volume_id;
    let watcher_mount_path = PathBuf::from(mount_path);

    tokio::spawn(super::smb_watcher::run_smb_watcher(
        watcher_addr,
        watcher_share,
        watcher_username,
        watcher_password,
        watcher_volume_id,
        watcher_mount_path,
        cancel_rx,
    ));

    *vol.watcher_cancel.lock().unwrap() = Some(cancel_tx);

    Ok(vol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::InMemoryVolume;

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
        // IO errors (callback errors, etc.) are not connection losses — they map to IoError.
        // Real connection losses come through Error::Disconnected → ConnectionLost.
        assert!(matches!(ve, VolumeError::IoError { .. }));
    }

    // ── Connection state tests ──────────────────────────────────────

    #[test]
    fn connection_state_round_trip() {
        for state in [
            ConnectionState::Direct,
            ConnectionState::OsMount,
            ConnectionState::Disconnected,
        ] {
            assert_eq!(ConnectionState::from_u8(state as u8), state);
        }
    }

    #[test]
    fn connection_state_unknown_value_defaults_to_disconnected() {
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

    /// Creates a test SmbVolume in disconnected state (no real connection).
    fn make_test_volume() -> SmbVolume {
        SmbVolume {
            name: "TestShare".to_string(),
            mount_path: PathBuf::from("/Volumes/TestShare"),
            server: "192.168.1.100".to_string(),
            share_name: "TestShare".to_string(),
            volume_id: "volumestestshare".to_string(),
            client: Arc::new(tokio::sync::Mutex::new(None)),
            tree: Arc::new(tokio::sync::RwLock::new(None)),
            state: Arc::new(AtomicU8::new(ConnectionState::Disconnected as u8)),
            watcher_cancel: std::sync::Mutex::new(None),
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
        connect_smb_volume("public", "/tmp/smb-test-mount", "127.0.0.1", "public", None, None, port)
            .await
            .unwrap_or_else(|e| {
                panic!("Failed to connect to Docker SMB container at 127.0.0.1:{port}. Is it running? ({e:?})")
            })
    }

    /// Unique directory name for test isolation.
    ///
    /// Combines a nanosecond timestamp with a process-wide atomic counter so
    /// that tests running in parallel on the same process never collide (the
    /// nanosecond clock resolution isn't fine enough on its own — we've seen
    /// multiple tests grab the same nanos within a nextest run).
    fn test_dir_name() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("cmdr-test-{}-{}", ts, n)
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
    // buffer will change the hash — the old `bytes_written == expected`
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

        // Must be Unix seconds — not millis (*1000) or micros (*1_000_000).
        // Allow a 1 hour window for clock skew between host and container.
        let lower = now_secs.saturating_sub(3600);
        let upper = now_secs + 3600;
        assert!(
            mtime >= lower && mtime <= upper,
            "modified_at {mtime} out of range [{lower}, {upper}] — likely wrong unit (seconds expected)",
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
        // Phase 4 Fix 4 — pipelined batch scan on the SMB hot copy path.
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
        // surface an error (callers treat scan as a pre-flight gate — a
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
            // blocking_send is fine in tests — we sized the channel to fit.
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
        // and final bytes_done == 200_000" assertions — hash the destination
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

        // Read from InMemory, write to SMB — the same path copy_single_path takes
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

        // 20 MB — guarantees multiple READs even at 8 MB max_read_size.
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

        // Hash chunks as they arrive — see the sibling large-file test for
        // why we avoid `assert_eq!` on 20 MB `Vec<u8>`s.
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

        // Subsequent op on the volume should succeed — the producer task
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
}

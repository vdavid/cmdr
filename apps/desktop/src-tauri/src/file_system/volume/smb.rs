//! SMB volume implementation using direct smb2 protocol operations.
//!
//! Wraps an smb2 session to provide file system access through the Volume trait.
//! The share remains OS-mounted (for Finder/Terminal/drag-drop compatibility),
//! but all Cmdr file operations go through smb2's pipelined I/O for better
//! performance and fail-fast behavior.

use super::{
    CopyScanResult, ScanConflict, SmbConnectionState, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream,
    path_to_id,
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
/// # Thread safety
///
/// The smb2 session is protected by a `tokio::sync::Mutex` so that async
/// Volume methods can hold the lock across `.await` points without blocking
/// the runtime. The `watcher_cancel` field uses `std::sync::Mutex` because
/// it is only accessed briefly (no awaits while held).
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
    /// smb2 session + tree connection. `None` when disconnected.
    /// Wrapped in `Arc` so we can use `lock_owned()` for long-lived streaming reads
    /// — the producer task that feeds `SmbReadStream` owns an `OwnedMutexGuard`
    /// for the download's duration, then releases it when the stream is done or
    /// dropped.
    smb: Arc<tokio::sync::Mutex<Option<(SmbClient, Tree)>>>,
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
            smb: Arc::new(tokio::sync::Mutex::new(Some((client, tree)))),
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
            };

            // Stat to determine if this is a file or directory
            if smb_path.is_empty() {
                // Root is always a directory, scan its contents
            } else {
                let info = {
                    let mut guard = self.acquire_smb().await?;
                    let (client, tree) = guard.as_mut().unwrap();
                    let r = client.stat(tree, smb_path).await;
                    self.handle_smb_result("scan_for_copy(stat)", r)?
                };

                if !info.is_directory {
                    result.file_count = 1;
                    result.total_bytes = info.size;
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
            let mut guard = self.acquire_smb().await?;
            let (client, tree) = guard.as_mut().unwrap();
            let r = client.list_directory(tree, &smb_path).await;
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

    /// Acquires the smb2 mutex and returns a mutable reference to the session.
    /// Checks connection state first, then verifies the session is present.
    async fn acquire_smb(&self) -> Result<tokio::sync::MutexGuard<'_, Option<(SmbClient, Tree)>>, VolumeError> {
        self.check_connection()?;
        let guard = self.smb.lock().await;
        if guard.is_none() {
            return Err(VolumeError::DeviceDisconnected("SMB session not available".to_string()));
        }
        Ok(guard)
    }

    /// Opens a streaming download on the given SMB-relative path.
    ///
    /// Acquires the SMB session lock in a background task (via `lock_owned`),
    /// opens an `smb2::FileDownload`, and returns an `SmbReadStream` that
    /// pipes chunks through a bounded mpsc channel. The lock stays held by
    /// the task until the download completes or the stream is dropped.
    ///
    /// This is the single streaming-read primitive for `SmbVolume`. The
    /// cross-volume streaming path (`open_read_stream`) goes through here, so
    /// no path has to buffer whole files in memory.
    async fn open_smb_download_stream(&self, smb_path: &str) -> Result<SmbReadStream, VolumeError> {
        self.check_connection()?;

        let (size_tx, size_rx) = tokio::sync::oneshot::channel::<Result<u64, VolumeError>>();
        let (chunk_tx, chunk_rx) =
            tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let smb_arc = Arc::clone(&self.smb);
        let state_arc = Arc::clone(&self.state);
        let share_name = self.share_name.clone();
        let smb_path_owned = smb_path.to_string();

        tokio::spawn(async move {
            let mut owned_guard = smb_arc.lock_owned().await;

            let (client, tree) = match owned_guard.as_mut() {
                Some(session) => session,
                None => {
                    let _ = size_tx.send(Err(VolumeError::DeviceDisconnected(
                        "SMB session not available".to_string(),
                    )));
                    return;
                }
            };

            let mut download = match client.download(tree, &smb_path_owned).await {
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
            // `owned_guard` drops here (releases the SMB session mutex).
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
        F: FnOnce(&mut SmbClient, &mut Tree) -> Result<T, smb2::Error>,
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

        let mut guard = self.smb.blocking_lock();

        let (client, tree) = guard
            .as_mut()
            .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?;

        match f(client, tree) {
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
                let mut guard = self.acquire_smb().await?;
                let (client, tree) = guard.as_mut().unwrap();
                let r = client.stat(tree, &smb_path).await;
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
                let guard = self.acquire_smb().await;
                if let Ok(mut guard) = guard {
                    let (client, tree) = guard.as_mut().unwrap();
                    client.stat(tree, &smb_path).await.is_ok()
                } else {
                    false
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
                let mut guard = self.acquire_smb().await?;
                let (client, tree) = guard.as_mut().unwrap();
                let r = client.stat(tree, &smb_path).await;
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
                let mut guard = self.acquire_smb().await?;
                let (client, tree) = guard.as_mut().unwrap();
                let r = client.fs_info(tree).await;
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
                let mut guard = self.acquire_smb().await?;
                let (client, tree) = guard.as_mut().unwrap();
                let result = client.write_file(tree, &smb_path, &data).await;
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
                let mut guard = self.acquire_smb().await?;
                let (client, tree) = guard.as_mut().unwrap();
                let result = client.create_directory(tree, &smb_path).await;
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
                let mut guard = self.acquire_smb().await?;
                let (client, tree) = guard.as_mut().unwrap();
                let r = client.delete_file(tree, &smb_path).await;
                self.handle_smb_result("delete_file", r)
            };

            match file_result {
                Ok(()) => {} // File deleted successfully
                Err(VolumeError::IoError { ref message, .. }) if message.contains("FILE_IS_A_DIRECTORY") => {
                    // It's a directory — try delete_directory
                    let mut guard = self.acquire_smb().await?;
                    let (client, tree) = guard.as_mut().unwrap();
                    let r = client.delete_directory(tree, &smb_path).await;
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
                    let mut guard = self.acquire_smb().await?;
                    let (client, tree) = guard.as_mut().unwrap();
                    client.stat(tree, &smb_to).await.is_ok()
                };

                if dest_exists {
                    // Try file delete first; if that fails (it's a dir), try directory delete
                    let file_result = {
                        let mut guard = self.acquire_smb().await?;
                        let (client, tree) = guard.as_mut().unwrap();
                        let r = client.delete_file(tree, &smb_to).await;
                        self.handle_smb_result("rename(delete_dest_file)", r)
                    };
                    if file_result.is_err() {
                        let mut guard = self.acquire_smb().await?;
                        let (client, tree) = guard.as_mut().unwrap();
                        let r = client.delete_directory(tree, &smb_to).await;
                        self.handle_smb_result("rename(delete_dest_dir)", r)?;
                    }
                }
            } else {
                // Check if dest exists and return AlreadyExists if so
                let dest_exists = {
                    let mut guard = self.acquire_smb().await?;
                    let (client, tree) = guard.as_mut().unwrap();
                    client.stat(tree, &smb_to).await.is_ok()
                };
                if dest_exists {
                    return Err(VolumeError::AlreadyExists(to.display().to_string()));
                }
            }

            {
                let mut guard = self.acquire_smb().await?;
                let (client, tree) = guard.as_mut().unwrap();
                let r = client.rename(tree, &smb_from, &smb_to).await;
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
        // Phase 4.2: hardcoded at 10 — within Phase 3's smb2 `MAX_PIPELINE_WINDOW`
        // of 32 and a safe default for QNAP-class servers. Phase 4.3 wires this
        // to the `network.smbConcurrency` setting (range 1..=32).
        // TODO(P4.3): replace with settings accessor.
        10
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
                "SmbVolume::write_from_stream: share={}, path={:?}",
                self.share_name, smb_path
            );

            // Holds the SMB session mutex for the duration of the transfer.
            // Different volumes have different mutexes, so awaiting
            // `stream.next_chunk()` while holding this one is safe.
            let mut guard = self.acquire_smb().await?;
            let (client, tree) = guard.as_mut().unwrap();

            let writer_result = client.create_file_writer(tree, &smb_path).await;
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
                    let _ = client.delete_file(tree, &smb_path).await;
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

        // Drop the smb2 session. Uses blocking_lock() since on_unmount is sync
        // (called from FSEvents thread, no Tokio runtime). This is safe because
        // we just set state to Disconnected, so no async task will acquire the lock.
        let mut guard = self.smb.blocking_lock();
        *guard = None;

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
            smb: Arc::new(tokio::sync::Mutex::new(None)),
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
    fn test_dir_name() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        format!("cmdr-test-{}", ts)
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
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        // ~8 MB of content with a deterministic pattern for integrity check
        let size = 8 * 1024 * 1024;
        let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let smb_path = format!("{}/big-stream.bin", dir);
        vol.create_file(Path::new(&smb_path), &data).await.unwrap();

        let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
        assert_eq!(stream.total_size(), size as u64);

        let mut reassembled = Vec::with_capacity(size);
        let mut chunks_seen = 0usize;
        while let Some(result) = stream.next_chunk().await {
            let chunk = result.unwrap();
            assert!(!chunk.is_empty(), "should not yield empty chunks");
            reassembled.extend_from_slice(&chunk);
            chunks_seen += 1;
        }
        assert_eq!(reassembled, data);
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
        let vol = make_docker_volume().await;
        let dir = test_dir_name();
        ensure_clean(&vol, &dir).await;
        vol.create_directory(Path::new(&dir)).await.unwrap();

        let size = 4 * 1024 * 1024; // 4 MB, enough to span many chunks
        let data: Vec<u8> = (0..size).map(|i| ((i * 7) % 251) as u8).collect();
        let smb_path = format!("{}/export-large.bin", dir);
        vol.create_file(Path::new(&smb_path), &data).await.unwrap();

        let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
        assert_eq!(stream.total_size(), size as u64);

        let mut chunks_seen = 0usize;
        let mut readback: Vec<u8> = Vec::with_capacity(size);
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            chunks_seen += 1;
            readback.extend_from_slice(&chunk);
        }
        assert!(
            chunks_seen >= 2,
            "streaming should yield multiple chunks for a multi-MB file"
        );
        assert_eq!(readback, data);

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

        // Verify content integrity via the streaming reader.
        let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
        assert_eq!(stream.total_size(), size as u64);
        let mut readback = Vec::with_capacity(size);
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, data);

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

        // Verify content integrity via the streaming reader.
        let mut verify = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
        assert_eq!(verify.total_size(), size as u64);
        let mut readback = Vec::with_capacity(size);
        while let Some(Ok(chunk)) = verify.next_chunk().await {
            readback.extend_from_slice(&chunk);
        }
        assert_eq!(readback, data);

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

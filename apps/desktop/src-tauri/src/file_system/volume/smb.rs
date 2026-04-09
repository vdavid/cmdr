//! SMB volume implementation using direct smb2 protocol operations.
//!
//! Wraps an smb2 session to provide file system access through the Volume trait.
//! The share remains OS-mounted (for Finder/Terminal/drag-drop compatibility),
//! but all Cmdr file operations go through smb2's pipelined I/O for better
//! performance and fail-fast behavior.

use super::{CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError};
use crate::file_system::listing::FileEntry;
use log::{debug, warn};
use smb2::client::tree::Tree;
use smb2::{ClientConfig, SmbClient};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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

/// Converts an `smb2::FileTime` to milliseconds since the Unix epoch,
/// matching `FileEntry.modified_at` / `created_at` format.
fn filetime_to_millis(ft: smb2::pack::FileTime) -> Option<u64> {
    let st = ft.to_system_time()?;
    let dur = st.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_millis() as u64)
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
    fe.modified_at = filetime_to_millis(entry.modified);
    fe.created_at = filetime_to_millis(entry.created);
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
        ErrorKind::Cancelled => VolumeError::IoError("Operation cancelled".to_string()),
        _ => VolumeError::IoError(err.to_string()),
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
/// Methods are called from `tokio::task::spawn_blocking` contexts, making it
/// safe to use `Handle::block_on` for async smb2 calls (same pattern as MtpVolume).
pub struct SmbVolume {
    /// Display name (share name).
    name: String,
    /// OS mount point (for example, "/Volumes/Documents").
    mount_path: PathBuf,
    /// Server hostname or IP address.
    server: String,
    /// SMB share name.
    share_name: String,
    /// smb2 session + tree connection. `None` when disconnected.
    smb: Mutex<Option<(SmbClient, Tree)>>,
    /// Current connection health.
    state: AtomicU8,
    /// Tokio runtime handle for async bridging.
    runtime_handle: tokio::runtime::Handle,
}

impl SmbVolume {
    /// Creates a new SMB volume with an established smb2 connection.
    ///
    /// # Arguments
    /// * `name` - Display name (typically the share name)
    /// * `mount_path` - OS mount point path
    /// * `server` - Server hostname or IP
    /// * `share_name` - SMB share name
    /// * `client` - Connected `SmbClient`
    /// * `tree` - Connected `Tree` for the share
    /// * `runtime_handle` - Tokio runtime handle for async bridging
    pub fn new(
        name: impl Into<String>,
        mount_path: impl Into<PathBuf>,
        server: impl Into<String>,
        share_name: impl Into<String>,
        client: SmbClient,
        tree: Tree,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            name: name.into(),
            mount_path: mount_path.into(),
            server: server.into(),
            share_name: share_name.into(),
            smb: Mutex::new(Some((client, tree))),
            state: AtomicU8::new(ConnectionState::Direct as u8),
            runtime_handle,
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

    // ── Recursive helpers for export/import/scan ──────────────────────

    /// Exports a single file from SMB to a local path. Returns bytes written.
    fn export_single_file(&self, smb_path: &str, local_dest: &Path) -> Result<u64, VolumeError> {
        let handle = self.runtime_handle.clone();
        let sp = smb_path.to_string();

        let data = self.with_smb("export_to_local(read)", |client, tree| {
            handle.block_on(client.read_file_pipelined(tree, &sp))
        })?;

        // Ensure parent directory exists
        if let Some(parent) = local_dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VolumeError::IoError(e.to_string()))?;
        }

        let len = data.len() as u64;
        std::fs::write(local_dest, &data).map_err(|e| VolumeError::IoError(e.to_string()))?;
        Ok(len)
    }

    /// Recursively exports a directory from SMB to a local path. Returns total bytes.
    fn export_directory_recursive(&self, smb_path: &str, local_dest: &Path) -> Result<u64, VolumeError> {
        std::fs::create_dir_all(local_dest).map_err(|e| VolumeError::IoError(e.to_string()))?;

        let display_path = self.to_display_path(smb_path);
        let entries = self.list_directory(Path::new(&display_path))?;
        let mut total_bytes = 0u64;

        for entry in &entries {
            let child_smb = if smb_path.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", smb_path, entry.name)
            };
            let child_local = local_dest.join(&entry.name);

            if entry.is_directory {
                total_bytes += self.export_directory_recursive(&child_smb, &child_local)?;
            } else {
                total_bytes += self.export_single_file(&child_smb, &child_local)?;
            }
        }

        Ok(total_bytes)
    }

    /// Imports a single local file to SMB. Returns bytes written.
    fn import_single_file(&self, local_source: &Path, smb_path: &str) -> Result<u64, VolumeError> {
        let data = std::fs::read(local_source).map_err(|e| VolumeError::IoError(e.to_string()))?;
        let len = data.len() as u64;
        let handle = self.runtime_handle.clone();
        let sp = smb_path.to_string();

        self.with_smb("import_from_local(write)", |client, tree| {
            handle.block_on(client.write_file_pipelined(tree, &sp, &data))
        })?;

        Ok(len)
    }

    /// Recursively imports a local directory to SMB. Returns total bytes.
    fn import_directory_recursive(&self, local_source: &Path, smb_path: &str) -> Result<u64, VolumeError> {
        let handle = self.runtime_handle.clone();
        let sp = smb_path.to_string();

        self.with_smb("import_from_local(mkdir)", |client, tree| {
            handle.block_on(client.create_directory(tree, &sp))
        })?;

        let read_dir = std::fs::read_dir(local_source).map_err(|e| VolumeError::IoError(e.to_string()))?;
        let mut total_bytes = 0u64;

        for dir_entry in read_dir {
            let dir_entry = dir_entry.map_err(|e| VolumeError::IoError(e.to_string()))?;
            let child_local = dir_entry.path();
            let child_name = dir_entry.file_name().to_string_lossy().to_string();
            let child_smb = if smb_path.is_empty() {
                child_name
            } else {
                format!("{}/{}", smb_path, child_name)
            };

            if child_local.is_dir() {
                total_bytes += self.import_directory_recursive(&child_local, &child_smb)?;
            } else {
                total_bytes += self.import_single_file(&child_local, &child_smb)?;
            }
        }

        Ok(total_bytes)
    }

    /// Recursively scans an SMB path, accumulating file/dir counts and total bytes.
    fn scan_recursive(&self, smb_path: &str, result: &mut CopyScanResult) -> Result<(), VolumeError> {
        let handle = self.runtime_handle.clone();
        let sp = smb_path.to_string();

        // Stat to determine if this is a file or directory
        if smb_path.is_empty() {
            // Root is always a directory, scan its contents
        } else {
            let info = self.with_smb("scan_for_copy(stat)", |client, tree| {
                handle.block_on(client.stat(tree, &sp))
            })?;

            if !info.is_directory {
                result.file_count += 1;
                result.total_bytes += info.size;
                return Ok(());
            }
        }

        // It's a directory — list and recurse
        result.dir_count += 1;
        let display_path = self.to_display_path(smb_path);
        let entries = self.list_directory(Path::new(&display_path))?;

        for entry in &entries {
            let child_smb = if smb_path.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", smb_path, entry.name)
            };

            if entry.is_directory {
                self.scan_recursive(&child_smb, result)?;
            } else {
                result.file_count += 1;
                result.total_bytes += entry.size.unwrap_or(0);
            }
        }

        Ok(())
    }

    // ── Connection helpers ──────────────────────────────────────────────

    /// Runs an smb2 operation, handling connection state transitions.
    ///
    /// On disconnection errors, transitions state to `Disconnected` (for now;
    /// `OsMount` fallback will be added in a follow-up).
    fn with_smb<F, T>(&self, op_name: &str, f: F) -> Result<T, VolumeError>
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

        let mut guard = self
            .smb
            .lock()
            .map_err(|e| VolumeError::IoError(format!("Failed to acquire SMB lock: {}", e)))?;

        let (client, tree) = guard
            .as_mut()
            .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?;

        match f(client, tree) {
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
                } else {
                    warn!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                }

                Err(map_smb_error(e))
            }
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

    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let smb_path = self.to_smb_path(path);
        let display_path = self.to_display_path(&smb_path);
        let handle = self.runtime_handle.clone();

        debug!(
            "SmbVolume::list_directory: share={}, input={:?}, smb_path={:?}",
            self.share_name, path, smb_path
        );

        let start = std::time::Instant::now();

        let result = self.with_smb("list_directory", |client, tree| {
            handle.block_on(client.list_directory(tree, &smb_path))
        })?;

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

    fn list_directory_with_progress(
        &self,
        path: &Path,
        on_progress: &dyn Fn(usize),
    ) -> Result<Vec<FileEntry>, VolumeError> {
        // smb2's list_directory returns all entries at once, so we report
        // progress as a single batch after the call completes.
        let entries = self.list_directory(path)?;
        on_progress(entries.len());
        Ok(entries)
    }

    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let smb_path = self.to_smb_path(path);
        let handle = self.runtime_handle.clone();

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

        let info = self.with_smb("get_metadata", |client, tree| {
            handle.block_on(client.stat(tree, &smb_path))
        })?;

        let name = Path::new(&smb_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| smb_path.clone());
        let display_path = self.to_display_path(&smb_path);

        let mut fe = FileEntry::new(name, display_path, info.is_directory, false);
        fe.size = if info.is_directory { None } else { Some(info.size) };
        fe.modified_at = filetime_to_millis(info.modified);
        fe.created_at = filetime_to_millis(info.created);
        Ok(fe)
    }

    fn exists(&self, path: &Path) -> bool {
        let smb_path = self.to_smb_path(path);
        if smb_path.is_empty() {
            return true; // Root always exists if we're connected
        }
        let handle = self.runtime_handle.clone();

        self.with_smb("exists", |client, tree| handle.block_on(client.stat(tree, &smb_path)))
            .is_ok()
    }

    fn is_directory(&self, path: &Path) -> Result<bool, VolumeError> {
        let smb_path = self.to_smb_path(path);
        if smb_path.is_empty() {
            return Ok(true); // Root is always a directory
        }
        let handle = self.runtime_handle.clone();

        let info = self.with_smb("is_directory", |client, tree| {
            handle.block_on(client.stat(tree, &smb_path))
        })?;

        Ok(info.is_directory)
    }

    fn supports_watching(&self) -> bool {
        // Start with false — the existing FSEvents watcher on the OS mount
        // point already provides change notifications. smb2-native watching
        // can be added later as an optimization.
        false
    }

    fn get_space_info(&self) -> Result<SpaceInfo, VolumeError> {
        let handle = self.runtime_handle.clone();

        debug!("SmbVolume::get_space_info: share={}", self.share_name);

        let info = self.with_smb("get_space_info", |client, tree| handle.block_on(client.fs_info(tree)))?;

        Ok(fs_info_to_space_info(&info))
    }

    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path);
        let handle = self.runtime_handle.clone();
        let data = content.to_vec();

        debug!("SmbVolume::create_file: share={}, path={:?}", self.share_name, smb_path);

        self.with_smb("create_file", |client, tree| {
            handle.block_on(client.write_file(tree, &smb_path, &data))
        })?;
        Ok(())
    }

    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path);
        let handle = self.runtime_handle.clone();

        debug!(
            "SmbVolume::create_directory: share={}, path={:?}",
            self.share_name, smb_path
        );

        self.with_smb("create_directory", |client, tree| {
            handle.block_on(client.create_directory(tree, &smb_path))
        })
    }

    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path);
        let handle = self.runtime_handle.clone();

        debug!("SmbVolume::delete: share={}, path={:?}", self.share_name, smb_path);

        // Stat first to determine file vs directory
        let is_dir = {
            let h = handle.clone();
            let sp = smb_path.clone();
            self.with_smb("delete(stat)", |client, tree| h.block_on(client.stat(tree, &sp)))?
                .is_directory
        };

        if is_dir {
            self.with_smb("delete_directory", |client, tree| {
                handle.block_on(client.delete_directory(tree, &smb_path))
            })
        } else {
            self.with_smb("delete_file", |client, tree| {
                handle.block_on(client.delete_file(tree, &smb_path))
            })
        }
    }

    fn rename(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let smb_from = self.to_smb_path(from);
        let smb_to = self.to_smb_path(to);
        let handle = self.runtime_handle.clone();

        debug!(
            "SmbVolume::rename: share={}, from={:?}, to={:?}, force={}",
            self.share_name, smb_from, smb_to, force
        );

        if force {
            // Check if dest exists and delete it first
            let h = handle.clone();
            let dest = smb_to.clone();
            let dest_exists = self
                .with_smb("rename(stat_dest)", |client, tree| h.block_on(client.stat(tree, &dest)))
                .is_ok();

            if dest_exists {
                let h = handle.clone();
                let dest = smb_to.clone();
                // Try file delete first; if that fails (it's a dir), try directory delete
                let file_result = self.with_smb("rename(delete_dest_file)", |client, tree| {
                    h.block_on(client.delete_file(tree, &dest))
                });
                if file_result.is_err() {
                    let h = handle.clone();
                    let dest = smb_to.clone();
                    self.with_smb("rename(delete_dest_dir)", |client, tree| {
                        h.block_on(client.delete_directory(tree, &dest))
                    })?;
                }
            }
        } else {
            // Check if dest exists and return AlreadyExists if so
            let h = handle.clone();
            let dest = smb_to.clone();
            if self
                .with_smb("rename(check_dest)", |client, tree| {
                    h.block_on(client.stat(tree, &dest))
                })
                .is_ok()
            {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }
        }

        self.with_smb("rename", |client, tree| {
            handle.block_on(client.rename(tree, &smb_from, &smb_to))
        })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn export_to_local(&self, source: &Path, local_dest: &Path) -> Result<u64, VolumeError> {
        let smb_path = self.to_smb_path(source);
        let handle = self.runtime_handle.clone();

        debug!(
            "SmbVolume::export_to_local: share={}, source={:?}, dest={}",
            self.share_name,
            smb_path,
            local_dest.display()
        );

        // Check if source is a directory or file
        let is_dir = if smb_path.is_empty() {
            true
        } else {
            let h = handle.clone();
            let sp = smb_path.clone();
            self.with_smb("export_to_local(stat)", |client, tree| {
                h.block_on(client.stat(tree, &sp))
            })?
            .is_directory
        };

        if is_dir {
            self.export_directory_recursive(&smb_path, local_dest)
        } else {
            self.export_single_file(&smb_path, local_dest)
        }
    }

    fn import_from_local(&self, local_source: &Path, dest: &Path) -> Result<u64, VolumeError> {
        let smb_path = self.to_smb_path(dest);

        debug!(
            "SmbVolume::import_from_local: share={}, source={}, dest={:?}",
            self.share_name,
            local_source.display(),
            smb_path
        );

        if local_source.is_dir() {
            self.import_directory_recursive(local_source, &smb_path)
        } else {
            self.import_single_file(local_source, &smb_path)
        }
    }

    fn scan_for_copy(&self, path: &Path) -> Result<CopyScanResult, VolumeError> {
        let smb_path = self.to_smb_path(path);

        debug!(
            "SmbVolume::scan_for_copy: share={}, path={:?}",
            self.share_name, smb_path
        );

        let mut result = CopyScanResult {
            file_count: 0,
            dir_count: 0,
            total_bytes: 0,
        };

        self.scan_recursive(&smb_path, &mut result)?;
        Ok(result)
    }

    fn scan_for_conflicts(
        &self,
        source_items: &[SourceItemInfo],
        dest_path: &Path,
    ) -> Result<Vec<ScanConflict>, VolumeError> {
        // List destination directory to check for conflicts
        let entries = self.list_directory(dest_path)?;
        let mut conflicts = Vec::new();

        for item in source_items {
            if let Some(existing) = entries.iter().find(|e| e.name == item.name) {
                let dest_modified = existing.modified_at.map(|ms| (ms / 1000) as i64);
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
    }

    fn smb_connection_state(&self) -> Option<crate::volumes::SmbConnectionState> {
        match self.connection_state() {
            ConnectionState::Direct => Some(crate::volumes::SmbConnectionState::Direct),
            ConnectionState::OsMount => Some(crate::volumes::SmbConnectionState::OsMount),
            ConnectionState::Disconnected => None,
        }
    }

    fn on_unmount(&self) {
        debug!("SmbVolume::on_unmount: disconnecting share={}", self.share_name);

        // Transition to Disconnected
        self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);

        // Drop the smb2 session (graceful disconnect)
        if let Ok(mut guard) = self.smb.lock() {
            *guard = None;
        }
    }
}

/// Creates an `SmbVolume` by connecting to a server and share.
///
/// Used by the mount flow to establish the smb2 session alongside the OS mount.
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
        addr,
        timeout: Duration::from_secs(10),
        username: username.unwrap_or("Guest").to_string(),
        password: password.unwrap_or("").to_string(),
        domain: String::new(),
        auto_reconnect: false,
        compression: true,
    };

    let mut client = SmbClient::connect(config).await?;
    let tree = client.connect_share(share_name).await?;
    let runtime_handle = tokio::runtime::Handle::current();

    Ok(SmbVolume::new(
        name,
        mount_path,
        server,
        share_name,
        client,
        tree,
        runtime_handle,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Type mapping tests ──────────────────────────────────────────

    #[test]
    fn filetime_to_millis_known_date() {
        // 2024-01-01 00:00:00 UTC = FileTime(133_485_408_000_000_000)
        let ft = smb2::pack::FileTime(133_485_408_000_000_000);
        let millis = filetime_to_millis(ft).unwrap();
        // Unix timestamp 1_704_067_200 seconds = 1_704_067_200_000 ms
        assert_eq!(millis, 1_704_067_200_000);
    }

    #[test]
    fn filetime_to_millis_zero_returns_none() {
        let ft = smb2::pack::FileTime::ZERO;
        assert!(filetime_to_millis(ft).is_none());
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
        assert_eq!(fe.modified_at, Some(1_704_067_200_000));
        assert_eq!(fe.created_at, Some(1_704_067_200_000));
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
        // IO errors with ConnectionLost kind map to DeviceDisconnected
        assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
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
        let (vol, _rt) = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("")), "");
        assert_eq!(vol.to_smb_path(Path::new("/")), "");
        assert_eq!(vol.to_smb_path(Path::new(".")), "");
    }

    #[test]
    fn to_smb_path_relative() {
        let (vol, _rt) = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("Documents")), "Documents");
        assert_eq!(
            vol.to_smb_path(Path::new("Documents/report.pdf")),
            "Documents/report.pdf"
        );
    }

    #[test]
    fn to_smb_path_absolute_under_mount() {
        let (vol, _rt) = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("/Volumes/TestShare/Documents")), "Documents");
        assert_eq!(
            vol.to_smb_path(Path::new("/Volumes/TestShare/Documents/report.pdf")),
            "Documents/report.pdf"
        );
    }

    #[test]
    fn to_smb_path_mount_root() {
        let (vol, _rt) = make_test_volume();
        assert_eq!(vol.to_smb_path(Path::new("/Volumes/TestShare")), "");
    }

    #[test]
    fn to_display_path_empty_is_mount_root() {
        let (vol, _rt) = make_test_volume();
        assert_eq!(vol.to_display_path(""), "/Volumes/TestShare");
    }

    #[test]
    fn to_display_path_with_subpath() {
        let (vol, _rt) = make_test_volume();
        assert_eq!(
            vol.to_display_path("Documents/report.pdf"),
            "/Volumes/TestShare/Documents/report.pdf"
        );
    }

    #[test]
    fn supports_watching_returns_false() {
        let (vol, _rt) = make_test_volume();
        assert!(!vol.supports_watching());
    }

    #[test]
    fn name_returns_share_name() {
        let (vol, _rt) = make_test_volume();
        assert_eq!(vol.name(), "TestShare");
    }

    #[test]
    fn root_returns_mount_path() {
        let (vol, _rt) = make_test_volume();
        assert_eq!(vol.root(), Path::new("/Volumes/TestShare"));
    }

    #[test]
    fn local_path_returns_none() {
        let (vol, _rt) = make_test_volume();
        assert!(vol.local_path().is_none());
    }

    #[test]
    fn supports_export_returns_true() {
        let (vol, _rt) = make_test_volume();
        assert!(vol.supports_export());
    }

    /// Creates a test SmbVolume in disconnected state (no real connection).
    ///
    /// Uses a dedicated single-threaded runtime since tests don't run
    /// inside a tokio context.
    fn make_test_volume() -> (SmbVolume, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let vol = SmbVolume {
            name: "TestShare".to_string(),
            mount_path: PathBuf::from("/Volumes/TestShare"),
            server: "192.168.1.100".to_string(),
            share_name: "TestShare".to_string(),
            smb: Mutex::new(None),
            state: AtomicU8::new(ConnectionState::Disconnected as u8),
            runtime_handle: rt.handle().clone(),
        };
        (vol, rt)
    }

    // ── Integration tests (require Docker SMB containers) ──────────
    //
    // Run with: cargo nextest run smb_integration --run-ignored all
    // Prerequisites: ./apps/desktop/test/smb-servers/start.sh

    /// Connects to the Docker smb-guest container (127.0.0.1:9445, share "public").
    ///
    /// Uses a multi-threaded runtime because `SmbVolume` methods call `Handle::block_on`
    /// internally (from `with_smb`). A single-threaded runtime would deadlock since
    /// the test thread is already inside `rt.block_on`.
    fn make_docker_volume() -> (SmbVolume, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let vol = rt.block_on(async {
            connect_smb_volume("public", "/tmp/smb-test-mount", "127.0.0.1", "public", None, None, 9445)
                .await
                .expect("Failed to connect to Docker SMB container at 127.0.0.1:9445. Is it running?")
        });
        (vol, rt)
    }

    /// Unique directory name for test isolation.
    fn test_dir_name() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        format!("cmdr-test-{}", ts)
    }

    /// Ensures a test directory is clean before use (deletes recursively if it exists).
    fn ensure_clean(vol: &SmbVolume, dir: &str) {
        if vol.exists(Path::new(dir)) {
            // Delete contents recursively
            if let Ok(entries) = vol.list_directory(Path::new(dir)) {
                for entry in entries {
                    let child = format!("{}/{}", dir, entry.name);
                    if entry.is_directory {
                        ensure_clean(vol, &child);
                    } else {
                        let _ = vol.delete(Path::new(&child));
                    }
                }
            }
            let _ = vol.delete(Path::new(dir));
        }
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_list_directory() {
        let (vol, _rt) = make_docker_volume();
        let entries = vol.list_directory(Path::new("")).unwrap();
        // The public share should be listable (may have files from other tests)
        assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_create_and_read_file() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();
        ensure_clean(&vol, &dir);

        // Create a directory
        vol.create_directory(Path::new(&dir)).unwrap();

        // Create a file inside it
        let file_path = format!("{}/test.txt", dir);
        let content = b"hello from cmdr integration test";
        vol.create_file(Path::new(&file_path), content).unwrap();

        // Verify it exists
        assert!(vol.exists(Path::new(&file_path)));
        assert!(!vol.is_directory(Path::new(&file_path)).unwrap());

        // Verify metadata
        let meta = vol.get_metadata(Path::new(&file_path)).unwrap();
        assert_eq!(meta.name, "test.txt");
        assert_eq!(meta.size, Some(content.len() as u64));
        assert!(!meta.is_directory);

        // List the directory and verify the file is there
        let entries = vol.list_directory(Path::new(&dir)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test.txt");

        // Clean up
        vol.delete(Path::new(&file_path)).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_rename() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).unwrap();
        let old_path = format!("{}/old.txt", dir);
        let new_path = format!("{}/new.txt", dir);

        vol.create_file(Path::new(&old_path), b"rename me").unwrap();

        // Rename
        vol.rename(Path::new(&old_path), Path::new(&new_path), false).unwrap();

        // Old is gone, new exists
        assert!(!vol.exists(Path::new(&old_path)));
        assert!(vol.exists(Path::new(&new_path)));

        // Clean up
        vol.delete(Path::new(&new_path)).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_rename_force_overwrites() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).unwrap();
        let src = format!("{}/src.txt", dir);
        let dst = format!("{}/dst.txt", dir);

        vol.create_file(Path::new(&src), b"source content").unwrap();
        vol.create_file(Path::new(&dst), b"will be overwritten").unwrap();

        // Non-force should fail
        let err = vol.rename(Path::new(&src), Path::new(&dst), false);
        assert!(matches!(err, Err(VolumeError::AlreadyExists(_))));

        // Force should succeed
        vol.rename(Path::new(&src), Path::new(&dst), true).unwrap();
        assert!(!vol.exists(Path::new(&src)));
        assert!(vol.exists(Path::new(&dst)));

        // Clean up
        vol.delete(Path::new(&dst)).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_delete_directory() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).unwrap();
        assert!(vol.exists(Path::new(&dir)));
        assert!(vol.is_directory(Path::new(&dir)).unwrap());

        vol.delete(Path::new(&dir)).unwrap();
        assert!(!vol.exists(Path::new(&dir)));
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_export_to_local() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();
        let local_tmp = std::env::temp_dir().join(&dir);

        // Create a file on the SMB share
        vol.create_directory(Path::new(&dir)).unwrap();
        let smb_file = format!("{}/export-test.txt", dir);
        let content = b"exported content";
        vol.create_file(Path::new(&smb_file), content).unwrap();

        // Export to local
        let bytes = vol
            .export_to_local(Path::new(&smb_file), &local_tmp.join("export-test.txt"))
            .unwrap();
        assert_eq!(bytes, content.len() as u64);

        // Verify local file
        let local_content = std::fs::read(local_tmp.join("export-test.txt")).unwrap();
        assert_eq!(local_content, content);

        // Clean up
        let _ = std::fs::remove_dir_all(&local_tmp);
        vol.delete(Path::new(&smb_file)).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_import_from_local() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();
        let local_tmp = std::env::temp_dir().join(format!("{}-import", dir));

        // Create a local file
        std::fs::create_dir_all(&local_tmp).unwrap();
        let local_file = local_tmp.join("import-test.txt");
        let content = b"imported content";
        std::fs::write(&local_file, content).unwrap();

        // Create target dir on SMB
        vol.create_directory(Path::new(&dir)).unwrap();

        // Import to SMB
        let smb_file = format!("{}/import-test.txt", dir);
        let bytes = vol.import_from_local(&local_file, Path::new(&smb_file)).unwrap();
        assert_eq!(bytes, content.len() as u64);

        // Verify on SMB
        assert!(vol.exists(Path::new(&smb_file)));
        let meta = vol.get_metadata(Path::new(&smb_file)).unwrap();
        assert_eq!(meta.size, Some(content.len() as u64));

        // Clean up
        let _ = std::fs::remove_dir_all(&local_tmp);
        vol.delete(Path::new(&smb_file)).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_export_directory_recursive() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();
        let local_tmp = std::env::temp_dir().join(format!("{}-export-dir", dir));

        // Create a directory tree on SMB
        vol.create_directory(Path::new(&dir)).unwrap();
        let sub = format!("{}/subdir", dir);
        vol.create_directory(Path::new(&sub)).unwrap();
        vol.create_file(Path::new(&format!("{}/a.txt", dir)), b"file a")
            .unwrap();
        vol.create_file(Path::new(&format!("{}/subdir/b.txt", dir)), b"file b")
            .unwrap();

        // Export entire directory
        let bytes = vol.export_to_local(Path::new(&dir), &local_tmp).unwrap();
        assert_eq!(bytes, 12); // "file a" (6) + "file b" (6)

        // Verify local structure
        assert!(local_tmp.join("a.txt").exists());
        assert!(local_tmp.join("subdir").is_dir());
        assert!(local_tmp.join("subdir/b.txt").exists());
        assert_eq!(std::fs::read_to_string(local_tmp.join("a.txt")).unwrap(), "file a");
        assert_eq!(
            std::fs::read_to_string(local_tmp.join("subdir/b.txt")).unwrap(),
            "file b"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&local_tmp);
        vol.delete(Path::new(&format!("{}/subdir/b.txt", dir))).unwrap();
        vol.delete(Path::new(&format!("{}/a.txt", dir))).unwrap();
        vol.delete(Path::new(&sub)).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_import_directory_recursive() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();
        let local_tmp = std::env::temp_dir().join(format!("{}-import-dir", dir));

        // Create a local directory tree
        std::fs::create_dir_all(local_tmp.join("subdir")).unwrap();
        std::fs::write(local_tmp.join("x.txt"), "file x").unwrap();
        std::fs::write(local_tmp.join("subdir/y.txt"), "file y").unwrap();

        // Import to SMB
        let bytes = vol.import_from_local(&local_tmp, Path::new(&dir)).unwrap();
        assert_eq!(bytes, 12); // "file x" (6) + "file y" (6)

        // Verify on SMB
        assert!(vol.is_directory(Path::new(&dir)).unwrap());
        assert!(vol.exists(Path::new(&format!("{}/x.txt", dir))));
        assert!(vol.is_directory(Path::new(&format!("{}/subdir", dir))).unwrap());
        assert!(vol.exists(Path::new(&format!("{}/subdir/y.txt", dir))));

        // Clean up
        let _ = std::fs::remove_dir_all(&local_tmp);
        vol.delete(Path::new(&format!("{}/subdir/y.txt", dir))).unwrap();
        vol.delete(Path::new(&format!("{}/x.txt", dir))).unwrap();
        vol.delete(Path::new(&format!("{}/subdir", dir))).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_scan_for_copy() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();

        // Create a small tree
        vol.create_directory(Path::new(&dir)).unwrap();
        let sub = format!("{}/inner", dir);
        vol.create_directory(Path::new(&sub)).unwrap();
        vol.create_file(Path::new(&format!("{}/f1.txt", dir)), b"aaa").unwrap();
        vol.create_file(Path::new(&format!("{}/inner/f2.txt", dir)), b"bbbbbb")
            .unwrap();

        let result = vol.scan_for_copy(Path::new(&dir)).unwrap();
        assert_eq!(result.file_count, 2);
        assert_eq!(result.dir_count, 2); // dir + inner
        assert_eq!(result.total_bytes, 9); // 3 + 6

        // Clean up
        vol.delete(Path::new(&format!("{}/inner/f2.txt", dir))).unwrap();
        vol.delete(Path::new(&format!("{}/f1.txt", dir))).unwrap();
        vol.delete(Path::new(&sub)).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_scan_for_conflicts() {
        let (vol, _rt) = make_docker_volume();
        let dir = test_dir_name();

        vol.create_directory(Path::new(&dir)).unwrap();
        vol.create_file(Path::new(&format!("{}/exists.txt", dir)), b"data")
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

        let conflicts = vol.scan_for_conflicts(&source_items, Path::new(&dir)).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].source_path, "exists.txt");

        // Clean up
        vol.delete(Path::new(&format!("{}/exists.txt", dir))).unwrap();
        vol.delete(Path::new(&dir)).unwrap();
    }

    #[test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    fn smb_integration_space_info() {
        let (vol, _rt) = make_docker_volume();
        let space = vol.get_space_info().unwrap();
        assert!(space.total_bytes > 0);
        assert!(space.available_bytes > 0);
        assert!(space.used_bytes <= space.total_bytes);
    }
}

//! SMB volume implementation using direct smb2 protocol operations.
//!
//! Wraps an smb2 session to provide file system access through the Volume trait.
//! The share remains OS-mounted (for Finder/Terminal/drag-drop compatibility),
//! but all Cmdr file operations go through smb2's pipelined I/O for better
//! performance and fail-fast behavior.

use super::{SpaceInfo, Volume, VolumeError};
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
    fn to_smb_path(&self, path: &Path) -> String {
        let path_str = path.to_string_lossy();

        // Handle paths that start with the mount path (absolute paths from frontend)
        if let Some(relative) = path_str.strip_prefix(self.mount_path.to_string_lossy().as_ref()) {
            let trimmed = relative.trim_start_matches('/');
            return trimmed.to_string();
        }

        // Handle empty or root paths
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return String::new();
        }

        // Strip leading slash for absolute paths
        path_str.strip_prefix('/').unwrap_or(&path_str).to_string()
    }

    /// Returns the full absolute path for a relative SMB path (under mount point).
    fn to_display_path(&self, smb_path: &str) -> String {
        if smb_path.is_empty() {
            self.mount_path.to_string_lossy().to_string()
        } else {
            format!("{}/{}", self.mount_path.display(), smb_path)
        }
    }

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
                let volume_err = map_smb_error(e);

                // On connection loss, transition to Disconnected
                if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                    warn!("SmbVolume::{}: connection lost, transitioning to Disconnected", op_name);
                    self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);
                }

                Err(volume_err)
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
            "SmbVolume::list_directory: share={}, smb_path={}",
            self.share_name, smb_path
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
            "SmbVolume::get_metadata: share={}, smb_path={}",
            self.share_name, smb_path
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
}

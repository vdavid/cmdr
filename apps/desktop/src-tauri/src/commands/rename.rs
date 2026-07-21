//! Tauri commands for file rename operations.
//!
//! Thin pass-throughs: the rename validation + the managed rename mutation live
//! in `file_system::write_operations::rename`. These commands expand tilde,
//! resolve the `volume_id`, apply the IPC timeout tiers (2 s validity/permission,
//! 5 s rename), and map errors to `IpcError`.

use std::path::PathBuf;
use tokio::time::Duration;

use super::file_system::expand_tilde;
use super::util::{IpcError, timeout_detached};
use crate::file_system::write_operations::trash::trash_single_journaled;
use crate::file_system::write_operations::{
    RenameValidityResult, check_rename_permission_sync, check_rename_validity_impl, rename_managed,
};

// ============================================================================
// Rename operations
// ============================================================================

/// Moves a file or directory to the macOS Trash via NSFileManager.
#[tauri::command]
#[specta::specta]
pub async fn move_to_trash(path: String) -> Result<(), IpcError> {
    let expanded = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded);

    // Defensive registration with the downloads watcher's ignore set; no-ops
    // outside ~/Downloads.
    crate::downloads::note_pending_write_for_cmdr(&path_buf);

    // Journal as a one-item trash op (captures the in-trash location + the
    // subtree's search leaves), mirroring the batch trash path. Initiator
    // threading through this command lands with the provenance-completion pass.
    tokio::time::timeout(
        Duration::from_secs(15),
        tokio::task::spawn_blocking(move || {
            trash_single_journaled(&path_buf, crate::operation_log::types::Initiator::User)
        }),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(|e| IpcError::from_err(format!("Task failed: {}", e)))?
    .map_err(IpcError::from_err)
    .map(|_in_trash| ())
}

/// Checks if a file/folder can be renamed (parent writable, not immutable, not SIP-protected, not
/// locked).
#[tauri::command]
#[specta::specta]
pub async fn check_rename_permission(path: String) -> Result<(), IpcError> {
    let expanded = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded);

    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || check_rename_permission_sync(&path_buf)),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(|e| IpcError::from_err(format!("Task failed: {}", e)))?
    .map_err(IpcError::from_err)
}

/// Validates a new filename and checks for conflicts in the same directory.
/// Uses inode comparison to detect case-only renames (valid on case-insensitive APFS).
/// When `volume_id` is provided and not `"root"`, uses the Volume trait for conflict detection
/// (needed for MTP and other non-local volumes).
#[tauri::command]
#[specta::specta]
pub async fn check_rename_validity(
    dir: String,
    old_name: String,
    new_name: String,
    volume_id: Option<String>,
) -> Result<RenameValidityResult, IpcError> {
    let expanded_dir = expand_tilde(&dir);
    let volume_id_str = volume_id.unwrap_or_else(|| "root".to_string());

    // Detached: conflict detection on a non-local volume LISTS the directory, so
    // on MTP a bare timeout would drop a listing mid-`GetObjectInfo`.
    timeout_detached(
        Duration::from_secs(2),
        check_rename_validity_impl(expanded_dir, old_name, new_name, volume_id_str),
    )
    .await
}

/// Renames a file or directory. When `force` is true, proceeds even if the destination exists.
///
/// When `volume_id` is provided and not `"root"`, routes through the Volume trait
/// (needed for MTP and other non-local volumes). Otherwise uses `std::fs::rename`.
/// The mutation runs as a managed instant op (busy-marks the volume, appears
/// briefly in the queue), still inline and result-returning.
#[tauri::command]
#[specta::specta]
pub async fn rename_file(
    from: String,
    to: String,
    force: bool,
    volume_id: Option<String>,
    initiator: Option<crate::operation_log::types::Initiator>,
) -> Result<(), IpcError> {
    let volume_id_str = volume_id.unwrap_or_else(|| "root".to_string());

    let (from_path, to_path) = if volume_id_str != "root" {
        // Non-local volume paths are volume-relative; never tilde-expand them.
        (PathBuf::from(&from), PathBuf::from(&to))
    } else {
        (PathBuf::from(expand_tilde(&from)), PathBuf::from(expand_tilde(&to)))
    };

    // Detached: the 5 s deadline bounds the FE's wait, not the rename. On MTP the
    // rename is a PTP `SetObjectPropValue`, and dropping it mid-transaction
    // wedges the phone; the op finishes behind the timeout instead.
    timeout_detached(
        Duration::from_secs(5),
        rename_managed(
            from_path,
            to_path,
            force,
            volume_id_str,
            initiator.unwrap_or(crate::operation_log::types::Initiator::User),
        ),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_rename_cmd_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("Failed to create test directory");
        dir
    }

    fn cleanup_test_dir(path: &PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    /// Registers a real local-FS "root" volume so `rename_file` with
    /// `volume_id = None` (→ "root") exercises the managed path's local branch,
    /// the same one production hits. Idempotent via `register_if_absent`.
    fn ensure_root_volume() {
        use crate::file_system::get_volume_manager;
        use crate::file_system::volume::LocalPosixVolume;
        use std::sync::Arc;
        get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
    }

    // ========================================================================
    // Rename permission checks
    // ========================================================================

    #[tokio::test]
    async fn test_check_rename_permission_writable_file() {
        let tmp = create_test_dir("rename_perm_ok");
        let file = tmp.join("test.txt");
        fs::write(&file, "content").unwrap();
        let result = check_rename_permission(file.to_string_lossy().to_string()).await;
        assert!(result.is_ok());
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_check_rename_permission_nonexistent() {
        let result = check_rename_permission("/nonexistent_12345/file.txt".to_string()).await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("doesn't exist"));
    }

    // ========================================================================
    // Rename validity checks
    // ========================================================================

    #[tokio::test]
    async fn test_check_rename_validity_valid_no_conflict() {
        let tmp = create_test_dir("rename_valid_ok");
        let dir = tmp.to_string_lossy().to_string();
        fs::write(tmp.join("old.txt"), "content").unwrap();
        let result = check_rename_validity(dir, "old.txt".to_string(), "new.txt".to_string(), None).await;
        assert!(result.is_ok());
        let check = result.unwrap();
        assert!(check.valid);
        assert!(!check.has_conflict);
        assert!(!check.is_case_only_rename);
        assert!(check.conflict.is_none());
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_check_rename_validity_empty_name() {
        let tmp = create_test_dir("rename_valid_empty");
        let dir = tmp.to_string_lossy().to_string();
        let result = check_rename_validity(dir, "old.txt".to_string(), "   ".to_string(), None).await;
        assert!(result.is_ok());
        let check = result.unwrap();
        assert!(!check.valid);
        assert!(check.error.is_some());
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_check_rename_validity_slash_in_name() {
        let tmp = create_test_dir("rename_valid_slash");
        let dir = tmp.to_string_lossy().to_string();
        let result = check_rename_validity(dir, "old.txt".to_string(), "foo/bar".to_string(), None).await;
        assert!(result.is_ok());
        let check = result.unwrap();
        assert!(!check.valid);
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_check_rename_validity_conflict_detected() {
        let tmp = create_test_dir("rename_valid_conflict");
        let dir = tmp.to_string_lossy().to_string();
        fs::write(tmp.join("old.txt"), "old content").unwrap();
        fs::write(tmp.join("existing.txt"), "existing content").unwrap();
        let result = check_rename_validity(dir, "old.txt".to_string(), "existing.txt".to_string(), None).await;
        assert!(result.is_ok());
        let check = result.unwrap();
        assert!(check.valid);
        assert!(check.has_conflict);
        assert!(!check.is_case_only_rename);
        assert!(check.conflict.is_some());
        let conflict = check.conflict.unwrap();
        assert_eq!(conflict.name, "existing.txt");
        assert_eq!(conflict.size, 16); // "existing content" = 16 bytes
        cleanup_test_dir(&tmp);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_check_rename_validity_case_only_same_inode() {
        let tmp = create_test_dir("rename_valid_case");
        let dir = tmp.to_string_lossy().to_string();
        fs::write(tmp.join("MyFile.txt"), "content").unwrap();
        // On case-insensitive APFS, "myfile.txt" resolves to the same inode as "MyFile.txt"
        let result = check_rename_validity(dir, "MyFile.txt".to_string(), "myfile.txt".to_string(), None).await;
        assert!(result.is_ok());
        let check = result.unwrap();
        assert!(check.valid);
        // On case-insensitive FS (APFS default), this should detect same inode
        // On case-sensitive FS, there's no conflict at all
        // Either way, the rename is valid
        if check.has_conflict {
            assert!(check.is_case_only_rename);
        }
        cleanup_test_dir(&tmp);
    }

    // ========================================================================
    // Rename file (managed-wrapper transparency: same returns as before)
    // ========================================================================

    #[tokio::test]
    async fn test_rename_file_success() {
        ensure_root_volume();
        let tmp = create_test_dir("rename_file_ok");
        let old = tmp.join("old.txt");
        let new = tmp.join("new.txt");
        fs::write(&old, "content").unwrap();
        let result = rename_file(
            old.to_string_lossy().to_string(),
            new.to_string_lossy().to_string(),
            false,
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
        assert!(!old.exists());
        assert!(new.exists());
        assert_eq!(fs::read_to_string(&new).unwrap(), "content");
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_rename_file_conflict_no_force() {
        ensure_root_volume();
        let tmp = create_test_dir("rename_file_conflict");
        let old = tmp.join("old.txt");
        let new = tmp.join("new.txt");
        fs::write(&old, "old").unwrap();
        fs::write(&new, "new").unwrap();
        let result = rename_file(
            old.to_string_lossy().to_string(),
            new.to_string_lossy().to_string(),
            false,
            None,
            None,
        )
        .await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("already exists"));
        // Both files still intact
        assert!(old.exists());
        assert!(new.exists());
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_rename_file_force_overwrites() {
        ensure_root_volume();
        let tmp = create_test_dir("rename_file_force");
        let old = tmp.join("old.txt");
        let new = tmp.join("new.txt");
        fs::write(&old, "new content").unwrap();
        fs::write(&new, "old content").unwrap();
        let result = rename_file(
            old.to_string_lossy().to_string(),
            new.to_string_lossy().to_string(),
            true,
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(&new).unwrap(), "new content");
        cleanup_test_dir(&tmp);
    }

    // ========================================================================
    // Move to trash
    // ========================================================================

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_move_to_trash_success() {
        let tmp = create_test_dir("trash_ok");
        let file = tmp.join("trash_me.txt");
        fs::write(&file, "goodbye").unwrap();
        let result = move_to_trash(file.to_string_lossy().to_string()).await;
        assert!(result.is_ok());
        assert!(!file.exists());
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_move_to_trash_nonexistent() {
        let result = move_to_trash("/nonexistent_12345/trash_me.txt".to_string()).await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("doesn't exist"));
    }
}

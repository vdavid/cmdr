//! Tauri commands for file rename operations.

use std::path::{Path, PathBuf};

use super::file_system::expand_tilde;

// ============================================================================
// Rename operations
// ============================================================================

/// Moves a file or directory to the macOS Trash via NSFileManager.
#[tauri::command]
pub async fn move_to_trash(path: String) -> Result<(), String> {
    let expanded = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded);

    tokio::task::spawn_blocking(move || move_to_trash_sync(&path_buf))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Synchronous trash implementation using macOS NSFileManager.trashItem.
#[cfg(target_os = "macos")]
fn move_to_trash_sync(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    if !path.exists() {
        return Err(format!("'{}' doesn't exist", path.display()));
    }

    let path_str = path.to_string_lossy();
    let ns_path = NSString::from_str(&path_str);
    let url = NSURL::fileURLWithPath(&ns_path);
    let file_manager = NSFileManager::defaultManager();

    // trashItemAtURL:resultingItemURL:error: moves the item to Trash.
    // We pass None for resultingItemURL since we don't need the trash location.
    file_manager
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| format!("Failed to move to trash: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn move_to_trash_sync(path: &Path) -> Result<(), String> {
    Err(format!(
        "Moving to trash is not supported on this platform for '{}'",
        path.display()
    ))
}

/// Checks if a file/folder can be renamed (parent writable, not immutable, not SIP-protected, not locked).
#[tauri::command]
pub async fn check_rename_permission(path: String) -> Result<(), String> {
    let expanded = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded);

    tokio::task::spawn_blocking(move || check_rename_permission_sync(&path_buf))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Result of a rename validity check.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameValidityResult {
    /// Whether the new name is valid (passes filename validation).
    pub valid: bool,
    /// Validation error message, if any.
    pub error: Option<crate::file_system::validation::ValidationError>,
    /// Whether a conflict exists (a sibling with the same name).
    pub has_conflict: bool,
    /// If there's a conflict, whether it's a case-only rename of the same file (same inode).
    pub is_case_only_rename: bool,
    /// Conflicting file info, if any.
    pub conflict: Option<ConflictFileInfo>,
}

/// Metadata about a conflicting sibling file.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFileInfo {
    pub name: String,
    /// In bytes.
    pub size: u64,
    /// Unix timestamp in seconds.
    pub modified: Option<i64>,
    pub is_directory: bool,
}

/// Validates a new filename and checks for conflicts in the same directory.
/// Uses inode comparison to detect case-only renames (valid on case-insensitive APFS).
#[tauri::command]
pub async fn check_rename_validity(
    dir: String,
    old_name: String,
    new_name: String,
) -> Result<RenameValidityResult, String> {
    let expanded_dir = expand_tilde(&dir);

    tokio::task::spawn_blocking(move || check_rename_validity_sync(&expanded_dir, &old_name, &new_name))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Renames a file or directory. When `force` is true, proceeds even if the destination exists.
#[tauri::command]
pub async fn rename_file(from: String, to: String, force: bool) -> Result<(), String> {
    let from_expanded = expand_tilde(&from);
    let to_expanded = expand_tilde(&to);
    let from_path = PathBuf::from(&from_expanded);
    let to_path = PathBuf::from(&to_expanded);

    tokio::task::spawn_blocking(move || {
        if !force && from_path != to_path && std::fs::symlink_metadata(&to_path).is_ok() {
            return Err(format!("'{}' already exists", to_path.display()));
        }
        std::fs::rename(&from_path, &to_path).map_err(|e| format!("Rename failed: {}", e))
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Synchronous permission check implementation.
fn check_rename_permission_sync(path: &Path) -> Result<(), String> {
    // Check that the file itself exists
    if std::fs::symlink_metadata(path).is_err() {
        return Err(format!("'{}' doesn't exist", path.display()));
    }

    // Check parent directory is writable
    let parent = path
        .parent()
        .ok_or_else(|| "Can't rename the root directory".to_string())?;
    check_dir_writable(parent)?;

    // Check macOS-specific flags (immutable, SIP, locks)
    #[cfg(target_os = "macos")]
    check_macos_flags(path)?;

    Ok(())
}

/// Checks if a directory is writable using access(W_OK).
#[cfg(unix)]
fn check_dir_writable(dir: &Path) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(dir.as_os_str().as_bytes()).map_err(|_| "Invalid path".to_string())?;
    // SAFETY: c_path is a valid null-terminated C string
    let result = unsafe { libc::access(c_path.as_ptr(), libc::W_OK) };
    if result != 0 {
        return Err(format!(
            "The folder '{}' is not writable. Check folder permissions in Finder.",
            dir.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_dir_writable(_dir: &Path) -> Result<(), String> {
    Ok(())
}

/// Checks macOS-specific immutable/SIP/lock flags.
#[cfg(target_os = "macos")]
fn check_macos_flags(path: &Path) -> Result<(), String> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid path".to_string())?;

    let mut stat = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: c_path is valid, stat is a valid pointer
    let result = unsafe { libc::lstat(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        // Can't stat — file may have been deleted, let the rename itself fail with a clear error
        return Ok(());
    }

    // SAFETY: lstat succeeded
    let stat = unsafe { stat.assume_init() };

    // UF_IMMUTABLE (user immutable / "uchg" flag)
    const UF_IMMUTABLE: u32 = 0x00000002;
    // SF_IMMUTABLE (system immutable, set by SIP)
    const SF_IMMUTABLE: u32 = 0x00020000;

    if (stat.st_flags & UF_IMMUTABLE) != 0 {
        return Err(
            "This file is locked (immutable flag). Unlock it in Finder > Get Info before renaming.".to_string(),
        );
    }
    if (stat.st_flags & SF_IMMUTABLE) != 0 {
        return Err("This file is protected by System Integrity Protection and can't be renamed.".to_string());
    }

    Ok(())
}

/// Synchronous validity check implementation.
fn check_rename_validity_sync(dir: &str, old_name: &str, new_name: &str) -> Result<RenameValidityResult, String> {
    use crate::file_system::validation::{validate_filename, validate_path_length};

    let trimmed = new_name.trim();

    // Validate filename
    if let Err(error) = validate_filename(trimmed) {
        return Ok(RenameValidityResult {
            valid: false,
            error: Some(error),
            has_conflict: false,
            is_case_only_rename: false,
            conflict: None,
        });
    }

    // Validate resulting path length
    let new_path = PathBuf::from(dir).join(trimmed);
    if let Err(error) = validate_path_length(&new_path) {
        return Ok(RenameValidityResult {
            valid: false,
            error: Some(error),
            has_conflict: false,
            is_case_only_rename: false,
            conflict: None,
        });
    }

    // Check for conflict: does a sibling with this name already exist?
    let old_path = PathBuf::from(dir).join(old_name);
    let conflict_info = check_sibling_conflict(&old_path, &new_path);

    Ok(RenameValidityResult {
        valid: true,
        error: None,
        has_conflict: conflict_info.0,
        is_case_only_rename: conflict_info.1,
        conflict: conflict_info.2,
    })
}

/// Checks if a file with `new_path` exists and whether it's the same inode as `old_path`
/// (case-only rename on case-insensitive FS).
#[cfg(unix)]
fn check_sibling_conflict(old_path: &Path, new_path: &Path) -> (bool, bool, Option<ConflictFileInfo>) {
    use std::os::unix::fs::MetadataExt;

    let new_meta = match std::fs::symlink_metadata(new_path) {
        Ok(m) => m,
        Err(_) => return (false, false, None), // No conflict
    };

    // Check if it's the same inode (case-only rename)
    let is_same_inode = std::fs::symlink_metadata(old_path)
        .map(|old_meta| old_meta.dev() == new_meta.dev() && old_meta.ino() == new_meta.ino())
        .unwrap_or(false);

    let modified = new_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let conflict = ConflictFileInfo {
        name: new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: new_meta.len(),
        modified,
        is_directory: new_meta.is_dir(),
    };

    (true, is_same_inode, Some(conflict))
}

#[cfg(not(unix))]
fn check_sibling_conflict(_old_path: &Path, new_path: &Path) -> (bool, bool, Option<ConflictFileInfo>) {
    let new_meta = match std::fs::symlink_metadata(new_path) {
        Ok(m) => m,
        Err(_) => return (false, false, None),
    };

    let modified = new_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let conflict = ConflictFileInfo {
        name: new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: new_meta.len(),
        modified,
        is_directory: new_meta.is_dir(),
    };

    // Without inode comparison, we can't detect case-only renames
    (true, false, Some(conflict))
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
        assert!(result.unwrap_err().contains("doesn't exist"));
    }

    // ========================================================================
    // Rename validity checks
    // ========================================================================

    #[tokio::test]
    async fn test_check_rename_validity_valid_no_conflict() {
        let tmp = create_test_dir("rename_valid_ok");
        let dir = tmp.to_string_lossy().to_string();
        fs::write(tmp.join("old.txt"), "content").unwrap();
        let result = check_rename_validity(dir, "old.txt".to_string(), "new.txt".to_string()).await;
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
        let result = check_rename_validity(dir, "old.txt".to_string(), "   ".to_string()).await;
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
        let result = check_rename_validity(dir, "old.txt".to_string(), "foo/bar".to_string()).await;
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
        let result = check_rename_validity(dir, "old.txt".to_string(), "existing.txt".to_string()).await;
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
        let result = check_rename_validity(dir, "MyFile.txt".to_string(), "myfile.txt".to_string()).await;
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
    // Rename file
    // ========================================================================

    #[tokio::test]
    async fn test_rename_file_success() {
        let tmp = create_test_dir("rename_file_ok");
        let old = tmp.join("old.txt");
        let new = tmp.join("new.txt");
        fs::write(&old, "content").unwrap();
        let result = rename_file(
            old.to_string_lossy().to_string(),
            new.to_string_lossy().to_string(),
            false,
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
        let tmp = create_test_dir("rename_file_conflict");
        let old = tmp.join("old.txt");
        let new = tmp.join("new.txt");
        fs::write(&old, "old").unwrap();
        fs::write(&new, "new").unwrap();
        let result = rename_file(
            old.to_string_lossy().to_string(),
            new.to_string_lossy().to_string(),
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
        // Both files still intact
        assert!(old.exists());
        assert!(new.exists());
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_rename_file_force_overwrites() {
        let tmp = create_test_dir("rename_file_force");
        let old = tmp.join("old.txt");
        let new = tmp.join("new.txt");
        fs::write(&old, "new content").unwrap();
        fs::write(&new, "old content").unwrap();
        let result = rename_file(
            old.to_string_lossy().to_string(),
            new.to_string_lossy().to_string(),
            true,
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
        assert!(result.unwrap_err().contains("doesn't exist"));
    }
}

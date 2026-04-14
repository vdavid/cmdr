//! Integration tests for validation functions, safety checks, and filesystem detection.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::*;

// ============================================================================
// Test utilities
// ============================================================================

fn create_temp_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!("cmdr_write_integration_test_{}", name));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
    temp_dir
}

fn cleanup_temp_dir(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

// ============================================================================
// Conflict resolution tests
// ============================================================================

#[test]
fn test_find_unique_name() {
    let temp_dir = create_temp_dir("unique_name");

    // Create existing files
    fs::write(temp_dir.join("file.txt"), "").unwrap();
    fs::write(temp_dir.join("file (1).txt"), "").unwrap();
    fs::write(temp_dir.join("file (2).txt"), "").unwrap();

    // Find unique name would return "file (3).txt"
    let path = temp_dir.join("file.txt");
    let unique = find_unique_name(&path);
    assert_eq!(unique.file_name().unwrap().to_str().unwrap(), "file (3).txt");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_find_unique_name_no_extension() {
    let temp_dir = create_temp_dir("unique_name_no_ext");

    fs::write(temp_dir.join("file"), "").unwrap();
    fs::write(temp_dir.join("file (1)"), "").unwrap();

    let path = temp_dir.join("file");
    let unique = find_unique_name(&path);
    assert_eq!(unique.file_name().unwrap().to_str().unwrap(), "file (2)");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_conflict_rename_generates_unique_names() {
    let temp_dir = create_temp_dir("conflict_rename");

    // Create existing files
    fs::write(temp_dir.join("file.txt"), "original").unwrap();
    fs::write(temp_dir.join("file (1).txt"), "first copy").unwrap();
    fs::write(temp_dir.join("file (2).txt"), "second copy").unwrap();

    // Find unique name should return "file (3).txt"
    let path = temp_dir.join("file.txt");
    let unique = find_unique_name(&path);
    assert_eq!(unique.file_name().unwrap().to_str().unwrap(), "file (3).txt");

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Helper function to find unique name (exposed for testing)
// ============================================================================

fn find_unique_name(path: &std::path::Path) -> PathBuf {
    let parent = path.parent().unwrap_or(std::path::Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());

    let mut counter = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return new_path;
        }
        counter += 1;
    }
}

// ============================================================================
// Validation function tests
// ============================================================================

#[test]
fn test_validate_sources_with_existing_files() {
    use super::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_exist");
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();

    let result = validate_sources(&[file1, file2]);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_sources_with_missing_file() {
    use super::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_missing");
    let file1 = temp_dir.join("exists.txt");
    let file2 = temp_dir.join("missing.txt");
    fs::write(&file1, "content").unwrap();

    let result = validate_sources(&[file1, file2]);
    assert!(matches!(result, Err(WriteOperationError::SourceNotFound { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_sources_with_symlink() {
    use super::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_symlink");
    let target = temp_dir.join("target.txt");
    let link = temp_dir.join("link");
    fs::write(&target, "content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Should accept symlinks
    let result = validate_sources(&[link]);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_sources_with_broken_symlink() {
    use super::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_broken_symlink");
    let link = temp_dir.join("broken_link");
    std::os::unix::fs::symlink("/nonexistent/path", &link).unwrap();

    // Should accept broken symlinks (symlink_metadata succeeds)
    let result = validate_sources(&[link]);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_existing_dir() {
    use super::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_dir");
    let dest = temp_dir.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let result = validate_destination(&dest);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_missing_dir() {
    use super::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_missing");
    let dest = temp_dir.join("missing");

    let result = validate_destination(&dest);
    assert!(matches!(result, Err(WriteOperationError::SourceNotFound { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_file() {
    use super::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_file");
    let dest = temp_dir.join("file.txt");
    fs::write(&dest, "content").unwrap();

    let result = validate_destination(&dest);
    assert!(matches!(result, Err(WriteOperationError::IoError { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_not_same_location_different() {
    use super::validate_not_same_location;

    let temp_dir = create_temp_dir("validate_same_loc_diff");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let file = src_dir.join("file.txt");
    fs::write(&file, "content").unwrap();

    let result = validate_not_same_location(&[file], &dst_dir);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_not_same_location_same() {
    use super::validate_not_same_location;

    let temp_dir = create_temp_dir("validate_same_loc_same");
    let file = temp_dir.join("file.txt");
    fs::write(&file, "content").unwrap();

    // Copying file to same directory
    let result = validate_not_same_location(&[file], &temp_dir);
    assert!(matches!(result, Err(WriteOperationError::SameLocation { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_not_inside_source_ok() {
    use super::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_ok");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let result = validate_destination_not_inside_source(&[src_dir], &dst_dir);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_not_inside_source_nested() {
    use super::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_nested");
    let src_dir = temp_dir.join("src");
    let dst_dir = src_dir.join("nested/dest");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Copying src to src/nested/dest - destination is inside source
    let result = validate_destination_not_inside_source(std::slice::from_ref(&src_dir), &dst_dir);
    assert!(matches!(
        result,
        Err(WriteOperationError::DestinationInsideSource { .. })
    ));

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Filesystem detection tests
// ============================================================================

#[test]
fn test_is_same_filesystem_same_volume() {
    use super::is_same_filesystem;

    let temp_dir = create_temp_dir("same_fs");
    let dir1 = temp_dir.join("dir1");
    let dir2 = temp_dir.join("dir2");
    fs::create_dir_all(&dir1).unwrap();
    fs::create_dir_all(&dir2).unwrap();

    let result = is_same_filesystem(&dir1, &dir2);
    assert!(result.is_ok());
    assert!(result.unwrap()); // Should be same filesystem

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_filesystem_with_root() {
    use super::is_same_filesystem;

    let temp_dir = create_temp_dir("same_fs_root");

    // Compare temp dir with root (/) - should be same on most systems
    let result = is_same_filesystem(&temp_dir, std::path::Path::new("/"));
    assert!(result.is_ok());
    // Note: Result depends on whether temp is on the same volume as root

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: path canonicalization (prevents ".." bypass)
// ============================================================================

#[test]
fn test_validate_destination_not_inside_source_dotdot_bypass() {
    use super::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_dotdot");
    let src_dir = temp_dir.join("src");
    let nested = src_dir.join("nested");
    fs::create_dir_all(&nested).unwrap();

    // Attempt to bypass with ".." segments: src/nested/../nested is still inside src
    let sneaky_dest = src_dir.join("nested").join("..").join("nested");
    let result = validate_destination_not_inside_source(std::slice::from_ref(&src_dir), &sneaky_dest);
    assert!(
        matches!(result, Err(WriteOperationError::DestinationInsideSource { .. })),
        "Should detect destination inside source even with '..' segments"
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_not_inside_source_symlink_bypass() {
    use super::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_symlink");
    let src_dir = temp_dir.join("src");
    let nested = src_dir.join("nested");
    fs::create_dir_all(&nested).unwrap();

    // Create a symlink outside src that points back into src
    let alias = temp_dir.join("alias_to_nested");
    std::os::unix::fs::symlink(&nested, &alias).unwrap();

    // Destination via symlink resolves back inside source
    let result = validate_destination_not_inside_source(std::slice::from_ref(&src_dir), &alias);
    assert!(
        matches!(result, Err(WriteOperationError::DestinationInsideSource { .. })),
        "Should detect destination inside source even through symlinks"
    );

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: destination writability check
// ============================================================================

#[test]
fn test_validate_destination_writable_ok() {
    use super::validate_destination_writable;

    let temp_dir = create_temp_dir("validate_writable_ok");
    let dest = temp_dir.join("writable_dest");
    fs::create_dir_all(&dest).unwrap();

    let result = validate_destination_writable(&dest);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_writable_readonly() {
    use super::validate_destination_writable;

    // Skip if running as root (root bypasses permission checks)
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let temp_dir = create_temp_dir("validate_writable_ro");
    let dest = temp_dir.join("readonly_dest");
    fs::create_dir_all(&dest).unwrap();
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o555)).unwrap();

    let result = validate_destination_writable(&dest);
    assert!(
        matches!(result, Err(WriteOperationError::PermissionDenied { .. })),
        "Should reject read-only destination"
    );

    // Restore for cleanup
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).unwrap();
    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: inode identity check (copy-over-self via symlink)
// ============================================================================

#[test]
fn test_is_same_file_different_files() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_diff");
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();

    assert!(!is_same_file(&file1, &file2));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_file_via_symlink() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_symlink");
    let file = temp_dir.join("original.txt");
    let link = temp_dir.join("link.txt");
    fs::write(&file, "content").unwrap();
    std::os::unix::fs::symlink(&file, &link).unwrap();

    // file and link resolve to the same inode
    assert!(is_same_file(&file, &link));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_file_via_hardlink() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_hardlink");
    let file = temp_dir.join("original.txt");
    let link = temp_dir.join("hardlink.txt");
    fs::write(&file, "content").unwrap();
    fs::hard_link(&file, &link).unwrap();

    // Hard links share the same inode
    assert!(is_same_file(&file, &link));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_file_nonexistent_dest() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_noexist");
    let file = temp_dir.join("exists.txt");
    let missing = temp_dir.join("missing.txt");
    fs::write(&file, "content").unwrap();

    // Should return false when destination doesn't exist
    assert!(!is_same_file(&file, &missing));

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: path length validation
// ============================================================================

#[test]
fn test_validate_path_length_ok() {
    use super::validate_path_length;

    let path = PathBuf::from("/tmp/short/path/file.txt");
    assert!(validate_path_length(&path).is_ok());
}

#[test]
fn test_validate_path_length_name_too_long() {
    use super::validate_path_length;

    // 256-byte filename exceeds the 255-byte APFS limit
    let long_name = "a".repeat(256);
    let path = PathBuf::from("/tmp").join(&long_name);
    let result = validate_path_length(&path);
    assert!(
        matches!(result, Err(WriteOperationError::IoError { ref message, .. }) if message.contains("File name")),
        "Should reject file names exceeding 255 bytes, got: {:?}",
        result
    );
}

#[test]
fn test_validate_path_length_path_too_long() {
    use super::validate_path_length;

    // Build a path longer than 1024 bytes
    let mut path = PathBuf::from("/tmp");
    while path.as_os_str().len() < 1025 {
        path = path.join("a_long_segment_name_here");
    }

    let result = validate_path_length(&path);
    assert!(
        matches!(result, Err(WriteOperationError::IoError { ref message, .. }) if message.contains("Path exceeds")),
        "Should reject paths exceeding 1024 bytes, got: {:?}",
        result
    );
}

#[test]
fn test_validate_path_length_exact_limit() {
    use super::validate_path_length;

    // Exactly 255 bytes is acceptable
    let name = "a".repeat(255);
    let path = PathBuf::from("/tmp").join(&name);
    // Only checks name length, this should pass if total path < 1024
    if path.as_os_str().len() <= 1024 {
        assert!(validate_path_length(&path).is_ok());
    }
}

// ============================================================================
// Copy safety: special file filtering
// ============================================================================

#[cfg(unix)]
#[test]
fn test_special_file_socket_skipped() {
    use std::os::unix::net::UnixListener;

    let temp_dir = create_temp_dir("special_socket");
    let socket_path = temp_dir.join("test.sock");

    // Create a Unix domain socket
    let _listener = UnixListener::bind(&socket_path).unwrap();
    assert!(socket_path.exists());

    // Verify it's not a regular file, symlink, or directory
    let metadata = fs::symlink_metadata(&socket_path).unwrap();
    assert!(!metadata.is_file());
    assert!(!metadata.is_dir());
    assert!(!metadata.is_symlink());

    // The scan functions skip special files — we verify the metadata detection works
    // (Full scan integration requires a Tauri app handle, tested at a higher level)

    cleanup_temp_dir(&temp_dir);
}

#[cfg(unix)]
#[test]
fn test_special_file_fifo_skipped() {
    let temp_dir = create_temp_dir("special_fifo");
    let fifo_path = temp_dir.join("test.fifo");

    // Create a FIFO
    let c_path = std::ffi::CString::new(fifo_path.to_str().unwrap()).unwrap();
    let result = unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) };
    assert_eq!(result, 0, "Failed to create FIFO");
    assert!(fifo_path.exists());

    // Verify it's detected as a special file
    let metadata = fs::symlink_metadata(&fifo_path).unwrap();
    assert!(!metadata.is_file());
    assert!(!metadata.is_dir());
    assert!(!metadata.is_symlink());

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: disk space check
// ============================================================================

#[cfg(unix)]
#[test]
fn test_validate_disk_space_sufficient() {
    use super::validate_disk_space;

    let temp_dir = create_temp_dir("disk_space_ok");
    // Requesting 1 byte should always succeed on any volume with free space
    let result = validate_disk_space(&temp_dir, 1);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[cfg(unix)]
#[test]
fn test_validate_disk_space_insufficient() {
    use super::validate_disk_space;

    let temp_dir = create_temp_dir("disk_space_fail");
    // Requesting an absurdly large amount (1 exabyte) should fail
    let result = validate_disk_space(&temp_dir, u64::MAX);
    assert!(
        matches!(result, Err(WriteOperationError::InsufficientSpace { .. })),
        "Should reject when required space exceeds available, got: {:?}",
        result
    );

    cleanup_temp_dir(&temp_dir);
}

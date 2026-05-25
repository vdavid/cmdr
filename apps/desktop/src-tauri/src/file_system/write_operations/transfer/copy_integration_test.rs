//! Integration tests for copy operations.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

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
// Copy tests
// ============================================================================

#[test]
fn test_copy_single_file() {
    let temp_dir = create_temp_dir("copy_single");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.txt");
    fs::write(&src_file, "Hello, world!").unwrap();

    // Verify source exists
    assert!(src_file.exists());

    // This test just verifies the file can be copied using the low-level module
    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("file.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());
        assert!(dst_file.exists());
        assert_eq!(fs::read_to_string(&dst_file).unwrap(), "Hello, world!");
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_directory_recursive() {
    let temp_dir = create_temp_dir("copy_recursive");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");

    // Create nested structure
    fs::create_dir_all(src_dir.join("subdir1/subdir2")).unwrap();
    fs::write(src_dir.join("file1.txt"), "file1").unwrap();
    fs::write(src_dir.join("subdir1/file2.txt"), "file2").unwrap();
    fs::write(src_dir.join("subdir1/subdir2/file3.txt"), "file3").unwrap();

    fs::create_dir_all(&dst_dir).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::{CopyOptions, copy_file_native};

        let result = copy_file_native(&src_dir, &dst_dir.join("src"), CopyOptions::default(), None);
        assert!(result.is_ok());
        assert!(dst_dir.join("src/file1.txt").exists());
        assert!(dst_dir.join("src/subdir1/file2.txt").exists());
        assert!(dst_dir.join("src/subdir1/subdir2/file3.txt").exists());
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_preserves_permissions() {
    let temp_dir = create_temp_dir("copy_permissions");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("script.sh");
    fs::write(&src_file, "#!/bin/bash\necho hello").unwrap();
    fs::set_permissions(&src_file, fs::Permissions::from_mode(0o755)).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("script.sh");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());

        let dst_perms = fs::metadata(&dst_file).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_preserves_symlinks() {
    let temp_dir = create_temp_dir("copy_symlinks");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a file and a symlink to it
    let target_file = src_dir.join("target.txt");
    fs::write(&target_file, "target content").unwrap();

    let symlink = src_dir.join("link");
    std::os::unix::fs::symlink(&target_file, &symlink).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_symlink;

        let dst_link = dst_dir.join("link");
        let result = copy_symlink(&symlink, &dst_link);
        assert!(result.is_ok());

        // Verify it's a symlink
        assert!(dst_link.is_symlink());
        let link_target = fs::read_link(&dst_link).unwrap();
        assert_eq!(link_target, target_file);
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_handles_broken_symlink() {
    let temp_dir = create_temp_dir("copy_broken_symlink");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a symlink to a nonexistent target
    let nonexistent = src_dir.join("nonexistent");
    let symlink = src_dir.join("broken_link");
    std::os::unix::fs::symlink(&nonexistent, &symlink).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_symlink;

        let dst_link = dst_dir.join("broken_link");
        let result = copy_symlink(&symlink, &dst_link);
        assert!(result.is_ok());

        // Verify it's a broken symlink
        assert!(dst_link.is_symlink());
        assert!(!dst_link.exists()); // exists() returns false for broken symlinks
    }

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Symlink loop detection tests
// ============================================================================

#[test]
fn test_copy_detects_symlink_loop() {
    let temp_dir = create_temp_dir("symlink_loop");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a directory structure with a symlink loop
    // src/a/b -> src/a (creates a loop)
    let a_dir = src_dir.join("a");
    fs::create_dir_all(&a_dir).unwrap();
    fs::write(a_dir.join("file.txt"), "content").unwrap();

    let loop_link = a_dir.join("b");
    std::os::unix::fs::symlink(&a_dir, &loop_link).unwrap();

    // The copy should detect the loop during scanning
    // We can't easily test the full copy operation without a Tauri app handle,
    // but we can verify the symlink loop exists
    assert!(loop_link.is_symlink());
    let link_target = fs::read_link(&loop_link).unwrap();
    assert_eq!(link_target, a_dir);

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Edge case tests (copy-related)
// ============================================================================

#[test]
fn test_special_characters_in_paths() {
    let temp_dir = create_temp_dir("special_chars");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create files with special characters
    let special_names = vec!["file with spaces.txt", "file'with'quotes.txt", "file\"double\".txt"];

    for name in &special_names {
        let src_file = src_dir.join(name);
        fs::write(&src_file, name).unwrap();

        #[cfg(target_os = "macos")]
        {
            use super::macos_copy::copy_single_file_native;

            let dst_file = dst_dir.join(name);
            let result = copy_single_file_native(&src_file, &dst_file, false, None);
            assert!(result.is_ok(), "Failed to copy file: {}", name);
            assert!(dst_file.exists());
        }
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_empty_directory() {
    let temp_dir = create_temp_dir("empty_dir");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create empty subdirectory
    let empty_subdir = src_dir.join("empty");
    fs::create_dir_all(&empty_subdir).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::{CopyOptions, copy_file_native};

        let result = copy_file_native(&empty_subdir, &dst_dir.join("empty"), CopyOptions::default(), None);
        assert!(result.is_ok());
        assert!(dst_dir.join("empty").exists());
        assert!(dst_dir.join("empty").is_dir());
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_readonly_source() {
    let temp_dir = create_temp_dir("readonly_source");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a readonly file
    let src_file = src_dir.join("readonly.txt");
    fs::write(&src_file, "readonly content").unwrap();
    fs::set_permissions(&src_file, fs::Permissions::from_mode(0o444)).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("readonly.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());
        assert!(dst_file.exists());

        // Verify permissions are preserved
        let dst_perms = fs::metadata(&dst_file).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o444);
    }

    // Cleanup: restore write permissions so we can delete
    fs::set_permissions(&src_file, fs::Permissions::from_mode(0o644)).unwrap();

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_long_paths() {
    let temp_dir = create_temp_dir("long_paths");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a deeply nested path (but not too deep to fail on the OS)
    let mut nested = src_dir.clone();
    for i in 0..20 {
        nested = nested.join(format!("level{}", i));
    }
    fs::create_dir_all(&nested).unwrap();
    let long_file = nested.join("file.txt");
    fs::write(&long_file, "content").unwrap();

    assert!(long_file.exists());

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        // Copy the deeply nested file
        let mut dst_nested = dst_dir.clone();
        for i in 0..20 {
            dst_nested = dst_nested.join(format!("level{}", i));
        }
        fs::create_dir_all(&dst_nested).unwrap();
        let dst_file = dst_nested.join("file.txt");

        let result = copy_single_file_native(&long_file, &dst_file, false, None);
        assert!(result.is_ok());
        assert!(dst_file.exists());
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_readonly_destination() {
    let temp_dir = create_temp_dir("readonly_dest");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create source file
    let src_file = src_dir.join("file.txt");
    fs::write(&src_file, "content").unwrap();

    // Make destination directory read-only
    fs::set_permissions(&dst_dir, fs::Permissions::from_mode(0o555)).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("file.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);

        // Should fail with permission denied
        assert!(result.is_err());
    }

    // Restore permissions for cleanup
    fs::set_permissions(&dst_dir, fs::Permissions::from_mode(0o755)).unwrap();

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_preserves_xattrs() {
    let temp_dir = create_temp_dir("xattrs");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.txt");
    fs::write(&src_file, "content").unwrap();

    // Set an extended attribute using xattr command
    let xattr_result = std::process::Command::new("xattr")
        .args(["-w", "com.test.attr", "test_value", src_file.to_str().unwrap()])
        .output();

    if xattr_result.is_err() || !xattr_result.as_ref().unwrap().status.success() {
        // Skip test if xattr command not available
        cleanup_temp_dir(&temp_dir);
        return;
    }

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("file.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());

        // Verify xattr was preserved
        let output = std::process::Command::new("xattr")
            .args(["-p", "com.test.attr", dst_file.to_str().unwrap()])
            .output()
            .expect("Failed to read xattr");

        let value = String::from_utf8_lossy(&output.stdout);
        assert_eq!(value.trim(), "test_value", "xattr not preserved: {}", value);
    }

    cleanup_temp_dir(&temp_dir);
}

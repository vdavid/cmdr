//! Extracts update tarballs and syncs files into the running `.app` bundle.
//!
//! This preserves the bundle directory's inode and xattrs, which keeps macOS TCC
//! (Full Disk Access) permissions intact across updates.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const STAGING_DIR: &str = "/tmp/cmdr-update-staging";

/// Extracts the tarball at `tarball_path` and syncs its contents into the running app bundle.
///
/// The tarball is expected to contain a `Cmdr.app/` root directory (as produced by `tauri-action`).
pub fn install(tarball_path: &Path) -> Result<(), String> {
    let staging = Path::new(STAGING_DIR);
    let staged_app = staging.join("Cmdr.app");

    // Clean up any previous staging dir
    if staging.exists() {
        fs::remove_dir_all(staging).map_err(|e| format!("Couldn't clean staging dir: {e}"))?;
    }
    fs::create_dir_all(staging).map_err(|e| format!("Couldn't create staging dir: {e}"))?;

    extract_tarball(tarball_path, staging)?;

    if !staged_app.exists() {
        return Err(format!(
            "Extracted tarball doesn't contain Cmdr.app/ at {}",
            staged_app.display()
        ));
    }

    let bundle_path = find_running_bundle()?;
    log::info!("Installing update into bundle: {}", bundle_path.display());

    let staged_contents = staged_app.join("Contents");
    let bundle_contents = bundle_path.join("Contents");

    if !staged_contents.exists() {
        return Err("Staged app missing Contents/ directory".to_string());
    }

    // Try direct sync first; escalate to admin privileges if permission denied
    match sync_bundle(&staged_contents, &bundle_contents) {
        Ok(()) => {}
        Err(e) if is_permission_error(&e) => {
            log::info!("Direct write denied, escalating with admin privileges");
            sync_with_admin_privileges(&staged_contents, &bundle_contents)?;
        }
        Err(e) => return Err(e),
    }

    // Touch the .app bundle to trigger LaunchServices refresh
    touch_bundle(&bundle_path);

    // Clean up staging dir
    if let Err(e) = fs::remove_dir_all(staging) {
        log::warn!("Couldn't clean up staging dir: {e}");
    }

    log::info!("Update installed successfully");
    Ok(())
}

/// Extracts a `.tar.gz` tarball into `dest_dir`.
fn extract_tarball(tarball_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(tarball_path).map_err(|e| format!("Couldn't open tarball: {e}"))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive
        .unpack(dest_dir)
        .map_err(|e| format!("Couldn't extract tarball: {e}"))
}

/// Finds the running app's `.app` bundle path by walking up from `current_exe()`.
fn find_running_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Couldn't get current exe: {e}"))?;
    let mut path = exe.as_path();
    while let Some(parent) = path.parent() {
        if path.extension().is_some_and(|ext| ext == "app") {
            return Ok(path.to_path_buf());
        }
        path = parent;
    }
    Err(format!("Couldn't find .app bundle in path: {}", exe.display()))
}

/// Syncs files from `src` (new Contents/) into `dest` (existing Contents/).
///
/// Order: Resources first, then Info.plist, then _CodeSignature, then binary last.
/// After syncing, deletes files in `dest` that don't exist in `src`.
fn sync_bundle(src: &Path, dest: &Path) -> Result<(), String> {
    // Collect all relative paths from the source for deletion pass
    let src_paths = collect_relative_paths(src)?;

    // Phase 1: sync files in order (binary last to minimize signature inconsistency window)
    sync_subtree(src, dest, Path::new("Resources"))?;
    sync_file_if_exists(src, dest, Path::new("Info.plist"))?;
    sync_subtree(src, dest, Path::new("_CodeSignature"))?;
    sync_file_if_exists(src, dest, Path::new("CodeResources"))?;
    sync_subtree(src, dest, Path::new("MacOS"))?;

    // Sync any remaining files not covered by the ordered phases above
    sync_remaining(src, dest, &src_paths)?;

    // Phase 2: delete files in dest that aren't in src
    delete_stale_files(dest, &src_paths)?;

    Ok(())
}

/// Recursively syncs a subtree from src to dest.
fn sync_subtree(src_root: &Path, dest_root: &Path, relative: &Path) -> Result<(), String> {
    let src_path = src_root.join(relative);
    if !src_path.exists() {
        return Ok(());
    }

    if src_path.is_file() || src_path.is_symlink() {
        return copy_file_creating_dirs(&src_path, &dest_root.join(relative));
    }

    if src_path.is_dir() {
        let entries = fs::read_dir(&src_path).map_err(|e| format!("Couldn't read dir {}: {e}", src_path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Couldn't read dir entry in {}: {e}", src_path.display()))?;
            let child_relative = relative.join(entry.file_name());
            sync_subtree(src_root, dest_root, &child_relative)?;
        }
    }

    Ok(())
}

/// Syncs a single file if it exists in src.
fn sync_file_if_exists(src_root: &Path, dest_root: &Path, relative: &Path) -> Result<(), String> {
    let src_path = src_root.join(relative);
    if src_path.exists() {
        copy_file_creating_dirs(&src_path, &dest_root.join(relative))?;
    }
    Ok(())
}

/// Syncs files that weren't already handled by the ordered phases.
fn sync_remaining(src_root: &Path, dest_root: &Path, all_src_paths: &HashSet<PathBuf>) -> Result<(), String> {
    for relative in all_src_paths {
        let src_path = src_root.join(relative);
        if !src_path.is_file() && !src_path.is_symlink() {
            continue;
        }
        let dest_path = dest_root.join(relative);
        // Skip if already synced (file exists and has same length — good enough for our use case
        // since all files are freshly extracted and we just wrote them in the ordered phases)
        if dest_path.exists()
            && let (Ok(src_meta), Ok(dest_meta)) = (fs::metadata(&src_path), fs::metadata(&dest_path))
            && src_meta.len() == dest_meta.len()
        {
            continue;
        }
        copy_file_creating_dirs(&src_path, &dest_path)?;
    }
    Ok(())
}

/// Copies a file, creating parent directories as needed.
///
/// Uses atomic rename (write to temp file, then rename) instead of in-place overwrite.
/// This creates a new inode, which prevents macOS's kernel code signing cache from
/// validating the new binary's pages against the old binary's cached code directory.
fn copy_file_creating_dirs(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| format!("Couldn't create dir {}: {e}", parent.display()))?;
    }

    // Write to a temp file in the same directory, then atomically rename.
    // This ensures the destination gets a new inode, avoiding stale kernel code signing cache.
    let temp = dest.with_extension("cmdr-update-tmp");
    fs::copy(src, &temp).map_err(|e| format!("Couldn't copy {} -> {}: {e}", src.display(), temp.display()))?;
    fs::rename(&temp, dest).map_err(|e| {
        // Clean up temp file on rename failure
        let _ = fs::remove_file(&temp);
        format!("Couldn't rename {} -> {}: {e}", temp.display(), dest.display())
    })?;
    Ok(())
}

/// Collects all relative file paths under `root` into a `HashSet`.
fn collect_relative_paths(root: &Path) -> Result<HashSet<PathBuf>, String> {
    let mut paths = HashSet::new();
    collect_relative_paths_recursive(root, root, &mut paths)?;
    Ok(paths)
}

fn collect_relative_paths_recursive(base: &Path, current: &Path, paths: &mut HashSet<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(current).map_err(|e| format!("Couldn't read dir {}: {e}", current.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Couldn't read dir entry in {}: {e}", current.display()))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(base)
            .map_err(|e| format!("Couldn't strip prefix: {e}"))?
            .to_path_buf();
        if path.is_dir() && !path.is_symlink() {
            collect_relative_paths_recursive(base, &path, paths)?;
        } else {
            paths.insert(relative);
        }
    }
    Ok(())
}

/// Deletes files and empty directories in `dest` that aren't present in `src_paths`.
fn delete_stale_files(dest: &Path, src_paths: &HashSet<PathBuf>) -> Result<(), String> {
    let dest_paths = collect_relative_paths(dest)?;
    for relative in &dest_paths {
        if !src_paths.contains(relative) {
            let stale = dest.join(relative);
            log::debug!("Deleting stale file: {}", stale.display());
            if let Err(e) = fs::remove_file(&stale) {
                log::warn!("Couldn't delete stale file {}: {e}", stale.display());
            }
        }
    }
    // Clean up empty directories (bottom-up)
    remove_empty_dirs(dest);
    Ok(())
}

/// Recursively removes empty directories under `dir`.
fn remove_empty_dirs(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            remove_empty_dirs(&path);
            // Try to remove — fails silently if not empty, which is fine
            let _ = fs::remove_dir(&path);
        }
    }
}

/// Checks whether an error message indicates a permission denial.
fn is_permission_error(error: &str) -> bool {
    error.contains("Permission denied") || error.contains("Operation not permitted")
}

/// Performs the file sync using `osascript` with admin privileges.
/// Uses `rsync -a --delete` because it's the simplest way to express the full sync in a
/// single shell command that macOS's `do shell script` can execute.
fn sync_with_admin_privileges(staged_contents: &Path, bundle_contents: &Path) -> Result<(), String> {
    let src = format!("{}/", staged_contents.display()); // trailing slash for rsync
    let dest = format!("{}/", bundle_contents.display());
    let script = format!(
        "do shell script \"rsync -a --delete '{}' '{}'\" with administrator privileges",
        src, dest
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("Couldn't run osascript: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Admin sync didn't succeed: {stderr}"));
    }

    Ok(())
}

/// Touches the `.app` bundle directory to update its modification time.
/// This triggers a LaunchServices refresh so the Dock and Finder pick up any icon/metadata changes.
fn touch_bundle(bundle_path: &Path) {
    let now = filetime::FileTime::now();
    if let Err(e) = filetime::set_file_mtime(bundle_path, now) {
        log::warn!("Couldn't touch bundle: {e}");
    }
}

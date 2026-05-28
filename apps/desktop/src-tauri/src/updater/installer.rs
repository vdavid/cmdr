//! Extracts update tarballs and syncs files into the running `.app` bundle.
//!
//! This preserves the bundle directory's inode and xattrs, which keeps macOS TCC
//! (Full Disk Access) permissions intact across updates.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Error type for the sync pipeline (`sync_bundle` + its inner helpers).
///
/// Carries both a formatted user-facing `message` and the original
/// `io::ErrorKind` so the caller can decide to escalate to admin privileges
/// on `PermissionDenied` without resorting to substring-matching the
/// formatted text. Pre-fix the helpers stringified `io::Error` at every
/// boundary and the caller substring-matched English `"Permission denied"` /
/// `"Operation not permitted"`, which silently failed on localized macOS.
#[derive(Debug)]
struct SyncError {
    message: String,
    kind: io::ErrorKind,
}

impl SyncError {
    fn from_io<F>(context: F, err: io::Error) -> Self
    where
        F: FnOnce() -> String,
    {
        Self {
            kind: err.kind(),
            message: format!("{}: {err}", context()),
        }
    }

    fn other(message: impl Into<String>) -> Self {
        Self {
            kind: io::ErrorKind::Other,
            message: message.into(),
        }
    }

    fn is_permission_denied(&self) -> bool {
        matches!(self.kind, io::ErrorKind::PermissionDenied)
    }
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Returns the per-instance staging dir. Each dev/worktree instance gets its own subdir so
/// concurrent `Cmdr` processes (main repo + worktree sessions) don't race on a shared
/// `/tmp/cmdr-update-staging` and trip `ENOTEMPTY` when one cleans up while another extracts.
///
/// `CMDR_INSTANCE_ID` is set by `tauri-wrapper.js` for every dev session; production builds
/// don't go through the wrapper, so the env var is absent and the dir lands at
/// `<tmp>/cmdr-update-staging-default`.
fn staging_dir() -> PathBuf {
    let instance = std::env::var("CMDR_INSTANCE_ID").unwrap_or_else(|_| "default".to_string());
    std::env::temp_dir().join(format!("cmdr-update-staging-{instance}"))
}

/// Returns `true` when the current executable lives inside a `.app` bundle.
///
/// Used to gate the updater entirely in dev builds, where `current_exe()` points at
/// `target/<triple>/release/Cmdr` (no `.app` ancestor) and the install path can't possibly
/// succeed. Calling `check_for_update`/`install_update` from such a build only produced
/// noisy auto-error-reports without any chance of installing anything.
pub fn is_running_from_app_bundle() -> bool {
    find_running_bundle().is_ok()
}

/// Extracts the tarball at `tarball_path` and syncs its contents into the running app bundle.
///
/// The tarball is expected to contain a `Cmdr.app/` root directory (as produced by `tauri-action`).
pub fn install(tarball_path: &Path) -> Result<(), String> {
    let staging_owned = staging_dir();
    let staging = staging_owned.as_path();
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
        Err(e) if e.is_permission_denied() => {
            log::info!("Direct write denied, escalating with admin privileges");
            sync_with_admin_privileges(&staged_contents, &bundle_contents)?;
        }
        Err(e) => return Err(e.to_string()),
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
    find_app_bundle_above(&exe).ok_or_else(|| format!("Couldn't find .app bundle in path: {}", exe.display()))
}

/// Pure walk: returns the closest ancestor of `start` whose own segment ends in `.app`,
/// or `None` if no such ancestor exists. Split out from [`find_running_bundle`] so it can
/// be unit-tested without touching `current_exe()`.
fn find_app_bundle_above(start: &Path) -> Option<PathBuf> {
    let mut path = start;
    while path.parent().is_some() {
        if path.extension().is_some_and(|ext| ext == "app") {
            return Some(path.to_path_buf());
        }
        path = path.parent()?;
    }
    None
}

/// Syncs files from `src` (new Contents/) into `dest` (existing Contents/).
///
/// Order: Resources first, then Info.plist, then _CodeSignature, then binary last.
/// After syncing, deletes files in `dest` that don't exist in `src`.
fn sync_bundle(src: &Path, dest: &Path) -> Result<(), SyncError> {
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
fn sync_subtree(src_root: &Path, dest_root: &Path, relative: &Path) -> Result<(), SyncError> {
    let src_path = src_root.join(relative);
    if !src_path.exists() {
        return Ok(());
    }

    if src_path.is_file() || src_path.is_symlink() {
        return copy_file_creating_dirs(&src_path, &dest_root.join(relative));
    }

    if src_path.is_dir() {
        let entries = fs::read_dir(&src_path)
            .map_err(|e| SyncError::from_io(|| format!("Couldn't read dir {}", src_path.display()), e))?;
        for entry in entries {
            let entry = entry
                .map_err(|e| SyncError::from_io(|| format!("Couldn't read dir entry in {}", src_path.display()), e))?;
            let child_relative = relative.join(entry.file_name());
            sync_subtree(src_root, dest_root, &child_relative)?;
        }
    }

    Ok(())
}

/// Syncs a single file if it exists in src.
fn sync_file_if_exists(src_root: &Path, dest_root: &Path, relative: &Path) -> Result<(), SyncError> {
    let src_path = src_root.join(relative);
    if src_path.exists() {
        copy_file_creating_dirs(&src_path, &dest_root.join(relative))?;
    }
    Ok(())
}

/// Syncs files that weren't already handled by the ordered phases.
fn sync_remaining(src_root: &Path, dest_root: &Path, all_src_paths: &HashSet<PathBuf>) -> Result<(), SyncError> {
    for relative in all_src_paths {
        let src_path = src_root.join(relative);
        if !src_path.is_file() && !src_path.is_symlink() {
            continue;
        }
        let dest_path = dest_root.join(relative);
        // Skip if already synced (file exists and has same length, good enough for our use case
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
fn copy_file_creating_dirs(src: &Path, dest: &Path) -> Result<(), SyncError> {
    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)
            .map_err(|e| SyncError::from_io(|| format!("Couldn't create dir {}", parent.display()), e))?;
    }

    // Write to a temp file in the same directory, then atomically rename.
    // This ensures the destination gets a new inode, avoiding stale kernel code signing cache.
    let temp = dest.with_extension("cmdr-update-tmp");
    fs::copy(src, &temp)
        .map_err(|e| SyncError::from_io(|| format!("Couldn't copy {} -> {}", src.display(), temp.display()), e))?;
    fs::rename(&temp, dest).map_err(|e| {
        // Clean up temp file on rename failure
        let _ = fs::remove_file(&temp);
        SyncError::from_io(
            || format!("Couldn't rename {} -> {}", temp.display(), dest.display()),
            e,
        )
    })?;
    Ok(())
}

/// Collects all relative file paths under `root` into a `HashSet`.
fn collect_relative_paths(root: &Path) -> Result<HashSet<PathBuf>, SyncError> {
    let mut paths = HashSet::new();
    collect_relative_paths_recursive(root, root, &mut paths)?;
    Ok(paths)
}

fn collect_relative_paths_recursive(
    base: &Path,
    current: &Path,
    paths: &mut HashSet<PathBuf>,
) -> Result<(), SyncError> {
    let entries = fs::read_dir(current)
        .map_err(|e| SyncError::from_io(|| format!("Couldn't read dir {}", current.display()), e))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| SyncError::from_io(|| format!("Couldn't read dir entry in {}", current.display()), e))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(base)
            .map_err(|e| SyncError::other(format!("Couldn't strip prefix: {e}")))?
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
fn delete_stale_files(dest: &Path, src_paths: &HashSet<PathBuf>) -> Result<(), SyncError> {
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
            // Try to remove (fails silently if not empty, which is fine)
            let _ = fs::remove_dir(&path);
        }
    }
}

/// AppleScript that reads two positional arguments and feeds them through
/// `quoted form of` before splicing into the shell command. This is the only safe way
/// to forward filesystem paths into `do shell script` — interpolating the paths into
/// the script text leaves them open to shell-injection (a `'` in any ancestor folder
/// name escapes the single-quote wrapping and the rest of the path runs as root).
const ADMIN_SYNC_SCRIPT: &str = "on run argv\n\
    set src to item 1 of argv\n\
    set dst to item 2 of argv\n\
    do shell script \"rsync -a --delete \" & quoted form of src & \" \" & quoted form of dst with administrator privileges\n\
end run";

/// Builds the argv passed to `osascript`. Split out from [`sync_with_admin_privileges`]
/// so it can be unit-tested without invoking the auth dialog.
fn build_admin_sync_argv(staged_contents: &Path, bundle_contents: &Path) -> Vec<OsString> {
    let mut src: OsString = staged_contents.as_os_str().to_os_string();
    src.push("/"); // trailing slash for rsync
    let mut dst: OsString = bundle_contents.as_os_str().to_os_string();
    dst.push("/");
    vec!["-e".into(), ADMIN_SYNC_SCRIPT.into(), src, dst]
}

/// Performs the file sync using `osascript` with admin privileges.
/// Uses `rsync -a --delete` because it's the simplest way to express the full sync in a
/// single shell command that macOS's `do shell script` can execute.
fn sync_with_admin_privileges(staged_contents: &Path, bundle_contents: &Path) -> Result<(), String> {
    let argv = build_admin_sync_argv(staged_contents, bundle_contents);

    let output = Command::new("osascript")
        .args(&argv)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_dir_uses_instance_id_when_set() {
        // SAFETY: nextest runs each test in its own process, so env-var mutation is process-local.
        unsafe { std::env::set_var("CMDR_INSTANCE_ID", "wt-foo") };
        let dir = staging_dir();
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some("cmdr-update-staging-wt-foo")
        );
    }

    #[test]
    fn staging_dir_falls_back_to_default_when_unset() {
        // SAFETY: see note in the sibling test.
        unsafe { std::env::remove_var("CMDR_INSTANCE_ID") };
        let dir = staging_dir();
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some("cmdr-update-staging-default")
        );
    }

    #[test]
    fn staging_dir_sits_under_temp_dir() {
        // SAFETY: see note in the sibling test.
        unsafe { std::env::remove_var("CMDR_INSTANCE_ID") };
        let dir = staging_dir();
        assert!(dir.starts_with(std::env::temp_dir()), "{:?} should sit under tmp", dir);
    }

    #[test]
    fn find_app_bundle_above_returns_bundle_for_installed_layout() {
        let exe = Path::new("/Applications/Cmdr.app/Contents/MacOS/Cmdr");
        assert_eq!(
            find_app_bundle_above(exe),
            Some(PathBuf::from("/Applications/Cmdr.app"))
        );
    }

    #[test]
    fn find_app_bundle_above_returns_bundle_for_user_applications_layout() {
        let exe = Path::new("/Users/jane/Applications/Cmdr.app/Contents/MacOS/Cmdr");
        assert_eq!(
            find_app_bundle_above(exe),
            Some(PathBuf::from("/Users/jane/Applications/Cmdr.app"))
        );
    }

    #[test]
    fn find_app_bundle_above_returns_none_for_dev_target_layout() {
        // Regression for the noisy auto-error-reports observed in v0.21.0: dev builds run from
        // `target/<triple>/release/Cmdr` have no `.app` ancestor, so the installer can't possibly
        // succeed. The wrapping `is_running_from_app_bundle()` gate uses exactly this distinction.
        let exe = Path::new("/Users/jane/code/cmdr/target/aarch64-apple-darwin/release/Cmdr");
        assert_eq!(find_app_bundle_above(exe), None);
    }

    #[test]
    fn find_app_bundle_above_returns_none_for_worktree_target_layout() {
        let exe = Path::new("/Users/jane/code/cmdr/.claude/worktrees/feature/target/aarch64-apple-darwin/release/Cmdr");
        assert_eq!(find_app_bundle_above(exe), None);
    }

    #[test]
    fn sync_error_classifies_permission_denied_by_io_kind_not_message() {
        // Regression for the medium-severity audit finding: pre-fix the
        // installer substring-matched English `"Permission denied"` and
        // `"Operation not permitted"`, so localized macOS systems never
        // reached the admin-escalation arm and updates silently failed.
        // The typed variant must trigger regardless of message wording.
        let err = SyncError::from_io(
            || "Couldn't copy /a -> /b".to_string(),
            io::Error::from(io::ErrorKind::PermissionDenied),
        );
        assert!(err.is_permission_denied());

        // Localized French wording from a real `EACCES` on macOS — the old
        // substring check would have returned false on this exact string.
        let localized_message_err = SyncError {
            message: "Couldn't rename /a -> /b: Permission refusée".to_string(),
            kind: io::ErrorKind::PermissionDenied,
        };
        assert!(localized_message_err.is_permission_denied());

        // Other I/O failures must NOT escalate.
        let other = SyncError::from_io(|| "Couldn't copy".to_string(), io::Error::from(io::ErrorKind::NotFound));
        assert!(!other.is_permission_denied());
    }

    #[test]
    fn build_admin_sync_argv_keeps_malicious_dest_as_single_argument() {
        // Bundle path with an embedded single quote and a shell metacharacter sequence
        // that would inject a command if the path were splatted into the script text.
        let staged = Path::new("/private/tmp/cmdr-update-staging-default/Cmdr.app/Contents");
        let dest = Path::new("/Applications/Don't '; touch /tmp/pwned; '.app/Contents");

        let argv = build_admin_sync_argv(staged, dest);

        assert_eq!(argv[0], "-e", "first arg must be the -e flag");
        assert_eq!(argv[1], ADMIN_SYNC_SCRIPT, "second arg must be the constant script");
        // The malicious dest stays a single argv entry; nothing from the path leaks into
        // the script template, so `quoted form of` (inside the script) handles the quote
        // safely at runtime.
        assert_eq!(
            argv[3].to_string_lossy(),
            "/Applications/Don't '; touch /tmp/pwned; '.app/Contents/"
        );
        // Script template must NOT mention the dest path; it must reference positional args only.
        assert!(
            !ADMIN_SYNC_SCRIPT.contains("pwned") && !ADMIN_SYNC_SCRIPT.contains("Don't"),
            "script template must be a constant, not built from path strings"
        );
        // And it MUST use AppleScript's own quoter (defense against future edits).
        assert!(
            ADMIN_SYNC_SCRIPT.contains("quoted form of"),
            "script must pass paths through `quoted form of` before shelling out"
        );
    }

    #[test]
    fn build_admin_sync_argv_appends_trailing_slash_for_rsync() {
        let staged = Path::new("/tmp/staged/Contents");
        let dest = Path::new("/Applications/Cmdr.app/Contents");
        let argv = build_admin_sync_argv(staged, dest);
        assert!(argv[2].to_string_lossy().ends_with("/Contents/"));
        assert!(argv[3].to_string_lossy().ends_with("/Contents/"));
    }
}

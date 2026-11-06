//! Extracts update tarballs and syncs files into the running `.app` bundle.
//!
//! This preserves the bundle directory's inode and xattrs, which keeps macOS TCC
//! (Full Disk Access) permissions intact across updates.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cmdr_fs::log_private::LogPrivate;
use cmdr_update_trust::{CodeSignatureRefusal, extract_tarball};

use super::UpdateInstallError;
use super::advisories::{self, SecurityNotice};
use super::bundle_location::{self, BundleWriteBlocker};

/// Error type for the sync pipeline (`sync_bundle` + its inner helpers).
///
/// Carries both a formatted log `message` and the original `io::ErrorKind`
/// so the caller can tell a permission refusal (`sync_failure`) from a broken
/// install without substring-matching the formatted text. Pre-fix the helpers
/// stringified `io::Error` at every
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

    /// Kept apart from [`Self::is_permission_denied`] so the notice can name the read-only volume
    /// (move Cmdr) rather than the account (replace Cmdr).
    fn is_read_only(&self) -> bool {
        matches!(self.kind, io::ErrorKind::ReadOnlyFilesystem)
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
    running_bundle().is_ok()
}

/// Extracts the tarball at `tarball_path` and syncs its contents into the running app bundle.
///
/// The tarball is expected to contain a `Cmdr.app/` root directory (as produced by `tauri-action`).
/// `may_install` is the managed policy's answer for the version the extracted bundle names.
pub fn install(
    tarball_path: &Path,
    may_install: &dyn Fn(&semver::Version) -> bool,
) -> Result<VettedBundle, UpdateInstallError> {
    // Resolved (and vetted) before extraction: unpacking ~63 MB into staging only to find the
    // destination unwritable is work nobody gets anything for. The frontend gates on the same
    // classification one step earlier; this arm is what keeps a direct caller honest.
    let bundle_path = running_bundle()?;
    if let Some(blocker) = bundle_location::classify(&bundle_path) {
        log::info!("Not installing: {} is {blocker}", LogPrivate::dir_path(&bundle_path));
        return Err(UpdateInstallError::Unwritable { blocker });
    }

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
        return Err(UpdateInstallError::from(format!(
            "Extracted tarball doesn't contain Cmdr.app/ at {}",
            staged_app.display()
        )));
    }

    let vetted = vet_staged_bundle(
        &staged_app,
        env!("CARGO_PKG_VERSION"),
        may_install,
        &cmdr_update_trust::verify_release_bundle,
        &advisories::read_bundle_list,
    )?;

    log::info!("Installing update into bundle: {}", LogPrivate::dir_path(&bundle_path));

    let staged_contents = staged_app.join("Contents");
    let bundle_contents = bundle_path.join("Contents");

    if !staged_contents.exists() {
        return Err("Staged app missing Contents/ directory".to_string().into());
    }

    sync_bundle(&staged_contents, &bundle_contents)
        .map_err(|e| sync_failure(e, bundle_location::not_writable_for_this_user))?;

    // Touch the .app bundle to trigger LaunchServices refresh
    touch_bundle(&bundle_path);

    // Clean up staging dir
    if let Err(e) = fs::remove_dir_all(staging) {
        log::warn!("Couldn't clean up staging dir: {e}");
    }

    log::info!("Update installed successfully");
    Ok(vetted)
}

/// What a staged bundle that passed every check holds: the version its own `Info.plist` names (the
/// one the policy was asked about, and so the one that gets installed) and the security notice its
/// own advisory list names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VettedBundle {
    pub version: semver::Version,
    pub security_notice: Option<SecurityNotice>,
}

/// Refuses a staged bundle that isn't code-signed by Cmdr's Developer ID (`cmdr_update_trust`), whose
/// own version isn't newer than `current`, or that the managed policy doesn't allow (`may_install`).
///
/// The code signature goes first: until it holds, nothing inside the bundle (its `Info.plist`
/// version included) is anything more than what the updater key's holder chose to write.
///
/// The policy is asked about the version the bundle NAMES, not the one the check offered: an
/// unsigned manifest could offer a version under the ceiling and serve a signed release above it.
///
/// Answers the security notice the bundle's own advisory list names (`read_advisories`, injected
/// so tests can see it's never called early). Read only once every check has passed, for the same
/// reason as the version: before that, the list is just bytes the updater key's holder chose.
fn vet_staged_bundle(
    staged_app: &Path,
    current: &str,
    may_install: &dyn Fn(&semver::Version) -> bool,
    code_signature: &dyn Fn(&Path) -> Result<(), CodeSignatureRefusal>,
    read_advisories: &dyn Fn(&Path) -> Option<String>,
) -> Result<VettedBundle, UpdateInstallError> {
    if let Err(refusal) = code_signature(staged_app) {
        crate::log_error!("Refusing the update: its minisign signature verified, but {refusal}");
        return Err(UpdateInstallError::UntrustedCodeSignature {
            detail: refusal.to_string(),
        });
    }
    let version = refuse_unless_newer(staged_app, current)?;
    if !may_install(&version) {
        log::info!(target: "managed_policy", "Not installing update {version}: the organization's policy refuses it");
        return Err(UpdateInstallError::BlockedByPolicy);
    }
    // `refuse_unless_newer` already parsed `current`, so this can only fail if it stops doing that.
    let security_notice = semver::Version::parse(current).ok().and_then(|running| {
        read_advisories(staged_app).and_then(|json| advisories::notice_from(&json, &running, &version))
    });
    Ok(VettedBundle {
        version,
        security_notice,
    })
}

/// Refuses a staged bundle whose own version isn't newer than `current`, and answers that version.
///
/// ❗ The minisign signature proves the archive is a real release, not WHICH one:
/// its trusted comment names only `file:Cmdr.app.tar.gz`, and the manifest that
/// names the version isn't signed. So an older signed archive served behind a
/// manifest claiming a newer version would verify and roll the install back. The
/// version inside the signed bytes (`Contents/Info.plist`) is what binds it.
fn refuse_unless_newer(staged_app: &Path, current: &str) -> Result<semver::Version, String> {
    let staged = cmdr_update_trust::bundle_version(staged_app)
        .map_err(|e| format!("Couldn't read the update's version: {e}"))?;
    let staged_version =
        semver::Version::parse(&staged).map_err(|e| format!("The update's version {staged} isn't semver: {e}"))?;
    let current_version =
        semver::Version::parse(current).map_err(|e| format!("This build's version {current} isn't semver: {e}"))?;
    if staged_version <= current_version {
        return Err(format!(
            "Refusing the update: it holds version {staged_version}, which isn't newer than {current_version}"
        ));
    }
    Ok(staged_version)
}

/// Finds the running app's `.app` bundle path by walking up from `current_exe()`.
///
/// Crate-visible rather than `pub(super)`: `dock/` needs the same answer to decide whether a Dock
/// tile could point at this copy.
pub(crate) fn running_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Couldn't get current exe: {e}"))?;
    find_app_bundle_above(&exe).ok_or_else(|| format!("Couldn't find .app bundle in path: {}", exe.display()))
}

/// Pure walk: returns the closest ancestor of `start` whose own segment ends in `.app`,
/// or `None` if no such ancestor exists. Split out from [`running_bundle`] so it can
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
/// Order: Resources first, then Info.plist, then _CodeSignature, then PlugIns, then binary last.
/// After syncing, deletes files in `dest` that don't exist in `src`.
fn sync_bundle(src: &Path, dest: &Path) -> Result<(), SyncError> {
    // Collect all relative paths from the source for deletion pass
    let src_paths = collect_relative_paths(src)?;

    // Phase 1: sync files in order (binary last to minimize signature inconsistency window)
    sync_subtree(src, dest, Path::new("Resources"))?;
    sync_file_if_exists(src, dest, Path::new("Info.plist"))?;
    sync_subtree(src, dest, Path::new("_CodeSignature"))?;
    sync_file_if_exists(src, dest, Path::new("CodeResources"))?;
    // `sync_subtree` must take a missing folder quietly, since a bundle may have no extension.
    sync_subtree(src, dest, Path::new("PlugIns"))?;
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
            log::debug!("Deleting stale file: {}", LogPrivate::file_path(&stale));
            if let Err(e) = fs::remove_file(&stale) {
                log::warn!("Couldn't delete stale file {}: {e}", LogPrivate::file_path(&stale));
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

/// What a sync that didn't finish tells the frontend.
///
/// A write refused on permissions or a read-only mount is the same "can't install it here" answer
/// the pre-flight `bundle_location::classify` gives, reached when the probe missed it (a nested
/// mount, a file flag). ❌ No escalation: on macOS 13+ App Management refuses an admin-run helper
/// too, so a password prompt would only end in the same refusal (`DETAILS.md` § A bundle that
/// can't be written). `not_writable` asks the admin question only when it's needed.
fn sync_failure(e: SyncError, not_writable: impl Fn() -> BundleWriteBlocker) -> UpdateInstallError {
    let blocker = if e.is_read_only() {
        BundleWriteBlocker::ReadOnlyVolume
    } else if e.is_permission_denied() {
        not_writable()
    } else {
        return e.to_string().into();
    };
    log::info!("The sync was refused ({e}), so this install can't write its own bundle: {blocker}");
    UpdateInstallError::Unwritable { blocker }
}

/// Touches the `.app` bundle directory to update its modification time.
/// This triggers a LaunchServices refresh so the Dock and Finder pick up any icon/metadata changes,
/// and the "apps installed" notification `nehelper` rebuilds the Local Network rule from
/// (`local_network_refresh`, which touches again on the next launch for that reason).
pub(super) fn touch_bundle(bundle_path: &Path) {
    let now = filetime::FileTime::now();
    if let Err(e) = filetime::set_file_mtime(bundle_path, now) {
        log::warn!("Couldn't touch bundle: {e}");
    }
}

#[cfg(test)]
#[path = "installer_tests.rs"]
mod tests;

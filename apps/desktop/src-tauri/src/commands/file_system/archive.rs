//! Archive-password commands: store or clear the per-archive password an
//! encrypted archive needs to browse or extract.
//!
//! The password lives on the resolved [`ArchiveVolume`] (remember-for-this-archive
//! — see its docs), which `VolumeManager::resolve` mints per archive and
//! LRU-caches. The frontend calls [`set_archive_password`] after prompting, then
//! retries the navigation or copy, which now decrypts. A WRONG password isn't
//! reported here (this only stores it): it surfaces on that retry as the typed
//! `NeedsPassword` (browse) / `ArchiveNeedsPassword` (extract) signal.

use std::path::Path;

use crate::file_system::get_volume_manager;
use crate::file_system::volume::backends::archive::ArchiveVolume;

/// Stores `password` for the archive at `archive_path` on `parent_volume_id`,
/// overwriting any previous one (so a fresh attempt replaces a rejected password).
///
/// `archive_path` may be the archive file itself OR any path inside it — both
/// resolve to the same `ArchiveVolume`, so the frontend can pass whichever it has
/// in hand (the `.zip` path when prompting on a browse, an inner source path when
/// prompting on an extract).
#[tauri::command]
#[specta::specta]
pub async fn set_archive_password(
    parent_volume_id: String,
    archive_path: String,
    password: String,
) -> Result<(), String> {
    with_archive(&parent_volume_id, &archive_path, move |archive| {
        archive.set_password(password)
    })
    .await
}

/// Forgets any stored password for the archive (the user cancelled the prompt, or
/// the frontend is resetting state).
#[tauri::command]
#[specta::specta]
pub async fn clear_archive_password(parent_volume_id: String, archive_path: String) -> Result<(), String> {
    with_archive(&parent_volume_id, &archive_path, ArchiveVolume::clear_password).await
}

/// Resolves `(parent_volume_id, archive_path)` to its [`ArchiveVolume`] and runs
/// `action` on it. Errs only when the path doesn't resolve to an archive (a
/// programming error or a since-changed path), never on a wrong password.
async fn with_archive(
    parent_volume_id: &str,
    archive_path: &str,
    action: impl FnOnce(&ArchiveVolume),
) -> Result<(), String> {
    let resolved = get_volume_manager()
        .resolve(parent_volume_id, Path::new(archive_path))
        .await;
    let volume = resolved
        .volume
        .ok_or_else(|| format!("no volume registered for '{parent_volume_id}'"))?;
    let archive = volume
        .as_any()
        .downcast_ref::<ArchiveVolume>()
        .ok_or_else(|| format!("'{archive_path}' is not a password-protected archive"))?;
    action(archive);
    Ok(())
}

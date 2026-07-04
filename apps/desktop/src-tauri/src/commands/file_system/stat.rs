//! Batched per-path "is this a directory?" probe.
//!
//! Used by the drag-and-drop transfer path: dropped paths arrive from the OS
//! pasteboard as bare absolute paths with no type info, so the confirmation
//! dialog and completion toast can't split them into files vs. folders without
//! a stat. This command resolves the top-level kind of each path in ONE batched
//! IPC, never walking subtrees.
//!
//! Per-item failures (a path that doesn't resolve to the local filesystem — a
//! virtual MTP/SMB path that landed on the pasteboard, a vanished entry, a
//! permission error) map to `None` ("unknown"), never an error for the whole
//! batch. The caller treats `None` as "fall back to today's approximate
//! behavior" rather than blocking the drop.

use crate::commands::util::{TimedOut, blocking_with_timeout_flag};
use std::path::Path;
use tokio::time::Duration;

/// Reads stat for paths from the pasteboard, so use the read timeout. A batch
/// of plain `symlink_metadata` calls on local paths is microseconds each; the
/// timeout only bites if one of the paths sits on a hung mount.
const STAT_PATHS_TIMEOUT: Duration = Duration::from_secs(2);

/// For each input path, returns:
/// - `Some(true)`  — the path is a directory,
/// - `Some(false)` — the path is a non-directory (file, symlink, …),
/// - `None`        — the kind is unknown (stat failed: the path doesn't resolve
///   to the local filesystem, vanished, or we lack permission).
///
/// The result vector is index-aligned with `paths`. This is a pure,
/// Tauri-free helper so the kind logic stays unit-testable. We use
/// `symlink_metadata` (not `metadata`) so a symlink reports as a non-directory
/// rather than following into its (possibly slow / missing) target.
pub fn stat_paths_kinds_blocking(paths: &[String]) -> Vec<Option<bool>> {
    paths
        .iter()
        .map(|p| match std::fs::symlink_metadata(Path::new(p)) {
            Ok(meta) => Some(meta.is_dir()),
            Err(_) => None,
        })
        .collect()
}

/// Batched per-path directory probe for the drag-and-drop transfer path.
///
/// Returns a `Vec<Option<bool>>` index-aligned with `paths` (see
/// `stat_paths_kinds_blocking`). Runs in `spawn_blocking` under the read
/// timeout; on a batch timeout the whole vector falls back to `None`
/// (all-unknown) with `timed_out: true`, so the caller cleanly degrades to the
/// approximate count shape rather than freezing the drop on slow volume I/O.
#[tauri::command]
#[specta::specta]
pub async fn stat_paths_kinds(paths: Vec<String>) -> TimedOut<Vec<Option<bool>>> {
    let count = paths.len();
    let fallback = vec![None; count];
    let paths_for_blocking = paths.clone();
    let mut result = blocking_with_timeout_flag(STAT_PATHS_TIMEOUT, fallback, move || {
        stat_paths_kinds_blocking(&paths_for_blocking)
    })
    .await;

    // A path INSIDE an archive can't be `symlink_metadata`'d (it's not a real FS
    // path), so it comes back `None`. Route those through the archive volume to
    // recover the kind. Gate on "inside an archive" (not just "crosses"): the
    // `.zip` file ITSELF is a real file that `symlink_metadata` already classified
    // as a non-directory, so it never reaches here — and if it did, we must NOT
    // report the archive ROOT's directory-ness for the file. `path_is_inside_archive`
    // does no I/O unless a component carries a `.zip` extension.
    if !result.timed_out {
        for (kind, path) in result.data.iter_mut().zip(paths.iter()) {
            if kind.is_some() || !crate::file_system::volume::backends::archive::path_is_inside_archive(Path::new(path))
            {
                continue;
            }
            let resolved = crate::file_system::get_volume_manager()
                .resolve(crate::file_system::volume::DEFAULT_VOLUME_ID, Path::new(path));
            if let Some(volume) = resolved.volume
                && let Ok(is_dir) = volume.is_directory(&resolved.path).await
            {
                *kind = Some(is_dir);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_stat_kinds_test_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    #[test]
    fn classifies_files_and_dirs() {
        let tmp = test_dir("mixed");
        let file = tmp.join("a.txt");
        fs::write(&file, b"hi").unwrap();
        let subdir = tmp.join("sub");
        fs::create_dir(&subdir).unwrap();

        let paths = vec![
            file.to_string_lossy().into_owned(),
            subdir.to_string_lossy().into_owned(),
        ];
        let kinds = stat_paths_kinds_blocking(&paths);
        assert_eq!(kinds, vec![Some(false), Some(true)]);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unknown_for_nonexistent_and_virtual_paths() {
        // A vanished local path, an MTP-shaped virtual path, and an SMB virtual
        // path all fail `symlink_metadata` → None. They must NOT poison the
        // whole batch, and the result stays index-aligned.
        let tmp = test_dir("unknown");
        let real = tmp.join("real");
        fs::create_dir(&real).unwrap();

        let paths = vec![
            real.to_string_lossy().into_owned(),
            "/this/does/not/exist/12345".to_string(),
            "mtp-1234://Internal storage/DCIM".to_string(),
            "smb://server/share/file".to_string(),
        ];
        let kinds = stat_paths_kinds_blocking(&paths);
        assert_eq!(kinds, vec![Some(true), None, None, None]);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn empty_input_yields_empty() {
        assert_eq!(stat_paths_kinds_blocking(&[]), Vec::<Option<bool>>::new());
    }

    #[tokio::test]
    async fn command_returns_aligned_kinds_without_timeout() {
        let tmp = test_dir("cmd");
        let file = tmp.join("f");
        fs::write(&file, b"x").unwrap();
        let paths = vec![file.to_string_lossy().into_owned(), "/nope/nope".to_string()];

        let result = stat_paths_kinds(paths).await;
        assert!(!result.timed_out);
        assert_eq!(result.data, vec![Some(false), None]);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn a_zip_file_itself_reports_as_a_file_not_a_directory() {
        // The `.zip` file is a real file: `symlink_metadata` classifies it, and the
        // archive-inner recovery branch must NOT re-report the archive ROOT's
        // directory-ness for it (which would mislead drag/transfer callers).
        let tmp = test_dir("zipfile");
        let zip = tmp.join("bundle.zip");
        fs::write(&zip, b"PK\x03\x04rest").unwrap();

        let result = stat_paths_kinds(vec![zip.to_string_lossy().into_owned()]).await;
        assert!(!result.timed_out);
        assert_eq!(result.data, vec![Some(false)], "the .zip file must report as a file");

        let _ = fs::remove_dir_all(&tmp);
    }
}

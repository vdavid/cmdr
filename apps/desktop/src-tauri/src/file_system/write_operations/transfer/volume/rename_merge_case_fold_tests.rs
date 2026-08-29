//! Case-insensitive backends and the late-detected collision (the TOCTOU net).
//!
//! SMB and a case-insensitive APFS answer `rename` with `AlreadyExists` for a
//! name the exact-match preflight map never saw as a clash, so the collision
//! surfaces DURING the merge rather than before it. These pin that the late
//! path routes such a child through the conflict resolver, prompting exactly
//! once, instead of erroring or prompting twice. The wrapper below models the
//! backend; the merge semantics they assume are in `rename_merge_tests.rs`.

use super::super::conflict_responder_test_support::{
    ConflictResponderSink, file_conflict_count, folder_conflict_count_any_dir,
};
use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{make_state, read, write_file};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tempfile::TempDir;

/// A `LocalPosixVolume` wrapper that makes `rename` and `list_directory`
/// case-insensitive, modeling SMB / APFS. Renaming onto an existing
/// case-folded name returns `AlreadyExists` even when the exact-match map
/// missed it; listing reflects the real on-disk (lowercased fixture) names so
/// the late-detection path can find the case-folded sibling.
struct CaseInsensitiveVolume {
    inner: Arc<LocalPosixVolume>,
}

impl CaseInsensitiveVolume {
    /// Resolves a path to its real on-disk casing by listing the parent and
    /// matching case-insensitively. Returns the input unchanged if no sibling
    /// matches (the name is free).
    async fn fold(&self, path: &Path) -> PathBuf {
        let parent = match path.parent() {
            Some(p) => p,
            None => return path.to_path_buf(),
        };
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_lowercase(),
            None => return path.to_path_buf(),
        };
        if let Ok(entries) = self.inner.list_directory(parent, None).await {
            for e in entries {
                if e.name.to_lowercase() == name {
                    return parent.join(&e.name);
                }
            }
        }
        path.to_path_buf()
    }
}

impl Volume for CaseInsensitiveVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.get_metadata(&folded).await
        })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.exists(&folded).await
        })
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.is_directory(&folded).await
        })
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.delete(&folded).await
        })
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Case-insensitive collision: if a case-folded sibling already holds
            // the target name (different exact spelling), reject with
            // AlreadyExists like SMB / APFS would.
            let folded_to = self.fold(to).await;
            if !force && folded_to != *to && self.inner.exists(&folded_to).await {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }
            let folded_from = self.fold(from).await;
            self.inner.rename(&folded_from, to, force).await
        })
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::volume::CopyScanResult, VolumeError>> + Send + 'a>>
    {
        self.inner.scan_for_copy(path)
    }
}
/// A case-folded FILE collision (the exact-match map misses `Clash.txt` vs
/// `clash.txt`) prompts EXACTLY ONCE under Stop and resolves correctly — the
/// late-detected path routes it through the resolver instead of erroring.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn case_folded_file_collision_prompts_exactly_once() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_file(root, "src/album/Clash.txt", b"SRC");
    write_file(root, "dst/album/clash.txt", b"DEST");

    let volume: Arc<dyn Volume> = Arc::new(CaseInsensitiveVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
    });

    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    // Answer Skip once; if the net re-prompted we'd see more than one recorded
    // file conflict.
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-casefold-file",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert_eq!(
        file_conflict_count(&events.inner),
        1,
        "a case-folded file collision must prompt exactly once"
    );
    assert_eq!(folder_conflict_count_any_dir(&events.inner), 0);
}

/// A child resolved Overwrite that THEN collides on the case-folded name must
/// NOT prompt twice — the stored decision finalizes the case-folded replace.
///
/// To force the map to miss but the rename to collide, the dest holds the file
/// under a different casing than the source AND the exact source name. The
/// resolver answers Overwrite once; the late path finalizes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn case_folded_overwrite_does_not_prompt_twice() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_file(root, "src/album/Photo.JPG", b"SRC-NEW");
    write_file(root, "dst/album/photo.jpg", b"DEST-OLD");

    let volume: Arc<dyn Volume> = Arc::new(CaseInsensitiveVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
    });

    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-casefold-overwrite",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert_eq!(
        file_conflict_count(&events.inner),
        1,
        "a child resolved Overwrite must NOT re-prompt when its rename collides on case-fold"
    );
    // The overwrite landed: exactly one file under the dest album, holding the
    // new bytes (the dest's case-folded name is replaced in place).
    let entries = volume.list_directory(Path::new("dst/album"), None).await.unwrap();
    let jpgs: Vec<_> = entries
        .iter()
        .filter(|e| e.name.to_lowercase().ends_with(".jpg"))
        .collect();
    assert_eq!(jpgs.len(), 1, "case-folded overwrite must not leave a duplicate");
    let landed = read(root, &format!("dst/album/{}", jpgs[0].name));
    assert_eq!(landed, b"SRC-NEW", "the overwrite must land the source bytes");
}

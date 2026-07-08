//! Data-safety tests for the remote zip-edit orchestration (pull → apply →
//! upload → swap). The red-first anchor for the core remote-edit guarantee:
//! the remote ORIGINAL is intact until the final swap, and a cancel anywhere
//! before it leaves the original untouched with no debris.
//!
//! These drive [`pull_apply_upload_swap`] directly against a non-local
//! `InMemoryVolume` (the remote-parent stand-in — it streams the `.zip` bytes
//! from its store through `open_read_stream` / `write_from_stream` / `rename` /
//! `delete`), so they exercise the orchestration without the managed-op
//! machinery. The end-to-end SMB (Docker) and MTP (virtual-device) round-trips
//! live in the integration suites.

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::super::state::{OperationIntent, WriteOperationState};
use super::pull_apply_upload_swap;
use crate::file_system::volume::backends::archive::mutator::{self, AddEntry, AddSource, Changeset, MutationHooks};
use crate::file_system::volume::{InMemoryVolume, Volume};

/// A no-op `MutationHooks`: the mutator never pauses/cancels in these tests, so
/// every trait method keeps its default.
struct NoHooks;
impl MutationHooks for NoHooks {}

/// Builds a real zip from `(name, contents)` pairs via the `zip` crate.
fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in entries {
            writer.start_file(*name, options).expect("start file");
            writer.write_all(contents).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }
    cursor.into_inner()
}

/// Streams the remote `.zip` back out of the parent and parses it into a
/// `name -> contents` map. Panics if the archive can't be opened (a corrupt swap
/// fails loudly here).
async fn read_remote_zip(parent: &dyn Volume, path: &Path) -> HashMap<String, Vec<u8>> {
    let mut stream = parent.open_read_stream(path).await.expect("open remote archive");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("read chunk"));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("remote archive parses");
    let mut out = HashMap::new();
    for i in 0..archive.len() {
        use std::io::Read;
        let mut entry = archive.by_index(i).expect("entry");
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).expect("entry bytes");
        out.insert(name, buf);
    }
    out
}

/// Names of every entry in the archive's parent directory (to assert no leftover
/// `.cmdr-tmp-*` temp remains after a swap).
async fn sibling_names(parent: &dyn Volume, archive_path: &Path) -> Vec<String> {
    let dir = archive_path.parent().expect("archive has a parent dir");
    parent
        .list_directory(dir, None)
        .await
        .expect("list parent dir")
        .into_iter()
        .map(|e| e.name)
        .collect()
}

fn add_entry(inner: &str, bytes: &[u8]) -> AddEntry {
    AddEntry {
        inner_path: inner.to_string(),
        source: AddSource::Bytes(bytes.to_vec()),
    }
}

/// Seeds a NON-local `InMemoryVolume` (the remote-parent stand-in) holding
/// `archive_path` with `zip_bytes`. Its containing dir is created so listing it
/// works.
async fn remote_parent_with_zip(archive_path: &Path, zip_bytes: &[u8]) -> Arc<InMemoryVolume> {
    let parent = InMemoryVolume::new("Remote");
    if let Some(dir) = archive_path.parent() {
        parent.create_directory(dir).await.expect("seed parent dir");
    }
    parent
        .create_file(archive_path, zip_bytes)
        .await
        .expect("seed remote zip");
    Arc::new(parent)
}

fn running_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// Unix-seconds "now", for aging a seeded temp relative to the reaper threshold.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Seeds a leftover temp file next to (or near) the archive with a chosen mtime,
/// so a reap test can model a stale crash leftover vs a live concurrent upload.
/// The bytes don't matter — the reaper only lists names and mtimes, then deletes.
async fn seed_temp(parent: &InMemoryVolume, temp_path: &Path, modified_at_secs: u64) {
    parent
        .create_file(temp_path, b"partial upload")
        .await
        .expect("seed temp");
    parent.set_modified_at(temp_path, Some(modified_at_secs));
}

/// A plain add edit, driven against `parent` through the remote orchestration.
/// Returns whether the edit committed (the error type isn't `Debug`, so callers
/// assert on the boolean rather than `.expect()`ing).
async fn run_add_edit(parent: Arc<dyn Volume>, archive_path: PathBuf) -> bool {
    pull_apply_upload_swap(
        parent,
        archive_path,
        running_state(),
        move |working: &Path| -> Result<(), super::RemoteEditError> {
            let changeset = Changeset {
                adds: vec![add_entry("added.txt", b"fresh bytes")],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await
    .is_ok()
}

#[tokio::test]
async fn remote_edit_reaps_a_stale_temp_for_the_same_archive() {
    // A crash between a prior upload and its swap left `bundle.zip.cmdr-tmp-<uuid>`
    // on the remote, well older than the reap threshold. The next edit of the same
    // archive must delete it.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let stale_temp = PathBuf::from("/device/bundle.zip.cmdr-tmp-0000stale");
    let parent = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    seed_temp(parent.as_ref(), &stale_temp, now_secs() - 48 * 60 * 60).await;

    assert!(
        run_add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone()).await,
        "the remote edit should commit"
    );

    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "the stale leftover temp should be reaped, got: {names:?}"
    );
    // The edit itself still landed.
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(back.contains_key("added.txt"), "the edit committed");
}

#[tokio::test]
async fn remote_edit_keeps_a_fresh_temp_for_the_same_archive() {
    // A temp with a recent mtime models a live concurrent upload from ANOTHER Cmdr
    // instance (another machine on the same share). It must NOT be reaped mid-flight.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let fresh_temp = PathBuf::from("/device/bundle.zip.cmdr-tmp-1111fresh");
    let parent = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    seed_temp(parent.as_ref(), &fresh_temp, now_secs()).await;

    assert!(
        run_add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone()).await,
        "the remote edit should commit"
    );

    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        names.iter().any(|n| n == "bundle.zip.cmdr-tmp-1111fresh"),
        "a fresh temp (a live concurrent upload) must be spared, got: {names:?}"
    );
}

#[tokio::test]
async fn remote_edit_ignores_a_stale_temp_for_a_different_archive() {
    // A stale temp belonging to a DIFFERENT archive in the same dir must never be
    // touched — the reaper matches only this archive's own temp-name shape.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let other_temp = PathBuf::from("/device/other.zip.cmdr-tmp-2222other");
    let parent = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    seed_temp(parent.as_ref(), &other_temp, now_secs() - 48 * 60 * 60).await;

    assert!(
        run_add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone()).await,
        "the remote edit should commit"
    );

    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        names.iter().any(|n| n == "other.zip.cmdr-tmp-2222other"),
        "a different archive's temp must be left alone, got: {names:?}"
    );
}

#[tokio::test]
async fn remote_edit_reap_delete_failure_does_not_fail_the_edit() {
    // The reap is best-effort: even when deleting the stale temp fails, the edit
    // must still commit (it swaps via rename-overwrite, which doesn't call delete).
    let archive_path = PathBuf::from("/device/bundle.zip");
    let stale_temp = PathBuf::from("/device/bundle.zip.cmdr-tmp-3333stuck");
    let parent = InMemoryVolume::new("Remote").with_delete_failing();
    parent.create_directory(Path::new("/device")).await.expect("seed dir");
    parent
        .create_file(&archive_path, &build_zip(&[("keep.txt", b"keep me")]))
        .await
        .expect("seed zip");
    seed_temp(&parent, &stale_temp, now_secs() - 48 * 60 * 60).await;
    let parent = Arc::new(parent);

    assert!(
        run_add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone()).await,
        "the edit commits despite a reap delete failure"
    );

    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(back.contains_key("added.txt"), "the edit committed");
}

#[tokio::test]
async fn remote_edit_adds_an_entry_and_swaps_it_into_place() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let original = build_zip(&[("keep.txt", b"keep me")]);
    let parent = remote_parent_with_zip(&archive_path, &original).await;
    let state = running_state();

    // The closure is the SAME local plan+apply the local path runs: here it just
    // adds an entry via the mutator, against whatever working path it's handed
    // (the pulled-local temp).
    let result = pull_apply_upload_swap(
        parent.clone() as Arc<dyn Volume>,
        archive_path.clone(),
        state,
        move |working: &Path| -> Result<(), super::RemoteEditError> {
            let changeset = Changeset {
                adds: vec![add_entry("added.txt", b"fresh bytes")],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await;
    assert!(result.is_ok(), "the remote edit should commit");

    // The remote archive now reflects the edit...
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert_eq!(back.get("keep.txt").map(Vec::as_slice), Some(b"keep me".as_slice()));
    assert_eq!(
        back.get("added.txt").map(Vec::as_slice),
        Some(b"fresh bytes".as_slice())
    );

    // ...and no upload temp lingers next to it.
    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "no leftover upload temp, got: {names:?}"
    );
}

#[tokio::test]
async fn remote_edit_cancel_before_swap_leaves_the_original_intact() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let original = build_zip(&[("keep.txt", b"keep me")]);
    let parent = remote_parent_with_zip(&archive_path, &original).await;
    let state = running_state();
    let state_for_closure = Arc::clone(&state);

    // The closure applies the edit to the LOCAL working copy, then flips the op to
    // cancelled — so the orchestrator's pre-upload cancel check trips and it never
    // touches the remote. This models a cancel landing after the local build but
    // before the remote is changed.
    let result = pull_apply_upload_swap(
        parent.clone() as Arc<dyn Volume>,
        archive_path.clone(),
        state,
        move |working: &Path| -> Result<(), super::RemoteEditError> {
            let changeset = Changeset {
                adds: vec![add_entry("added.txt", b"fresh bytes")],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            state_for_closure
                .intent
                .store(OperationIntent::Stopped as u8, Ordering::Relaxed);
            Ok(())
        },
    )
    .await;
    assert!(
        matches!(result, Err(super::RemoteEditError::Cancelled)),
        "a cancel before the swap must report Cancelled"
    );

    // The remote original is byte-for-byte intact: no added entry, no temp.
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(back.contains_key("keep.txt"), "the original entry survives");
    assert!(!back.contains_key("added.txt"), "the edit never reached the remote");
    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "a cancelled edit leaves no upload temp, got: {names:?}"
    );
}

#[tokio::test]
async fn remote_edit_swaps_via_delete_then_rename_on_a_sibling_allowing_backend() {
    // A backend that allows same-name siblings (MTP) must take the delete-then-
    // rename swap, never an atomic rename onto the live name (which would
    // duplicate). Assert the edit lands and exactly one archive object remains.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let original = build_zip(&[("keep.txt", b"keep me")]);

    let parent = InMemoryVolume::new("Remote-MTP").with_sibling_duplicates_allowed();
    parent.create_directory(Path::new("/device")).await.expect("seed dir");
    parent.create_file(&archive_path, &original).await.expect("seed zip");
    let parent = Arc::new(parent);
    let state = running_state();

    let result = pull_apply_upload_swap(
        parent.clone() as Arc<dyn Volume>,
        archive_path.clone(),
        state,
        move |working: &Path| -> Result<(), super::RemoteEditError> {
            let changeset = Changeset {
                deletes: vec!["keep.txt".to_string()],
                adds: vec![add_entry("added.txt", b"fresh bytes")],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await;
    assert!(
        result.is_ok(),
        "the remote edit should commit on a sibling-allowing backend"
    );

    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(!back.contains_key("keep.txt"), "the deleted entry is gone");
    assert_eq!(
        back.get("added.txt").map(Vec::as_slice),
        Some(b"fresh bytes".as_slice())
    );

    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert_eq!(
        names.iter().filter(|n| n.as_str() == "bundle.zip").count(),
        1,
        "exactly one archive object remains (no duplicate), got: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "no leftover upload temp, got: {names:?}"
    );
}

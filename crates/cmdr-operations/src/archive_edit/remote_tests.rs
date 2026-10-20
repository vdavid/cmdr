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
use std::future::Future;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::super::super::state::{OperationIntent, WriteOperationState};
use super::super::super::testing::TestOperations;
use super::pull_apply_upload_swap;
use crate::in_flight_temps::{RecordHome, track_temp};
use crate::transfer::volume::{FaultyOp, FaultyVolume};
use crate::types::WriteOperationError;
use cmdr_archive::mutator::{self, AddEntry, AddSource, Changeset, MutationHooks};
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};

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
        let name = entry.name().expect("a decodable name").into_owned();
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

/// A running state on an engine of its own.
fn running_state() -> Arc<WriteOperationState> {
    Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)))
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
        "remote",
        archive_path,
        running_state(),
        None,
        move |working: &Path| -> Result<(), super::EditError> {
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
    // must still commit.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let stale_temp = PathBuf::from("/device/bundle.zip.cmdr-tmp-3333stuck");
    let parent = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    seed_temp(parent.as_ref(), &stale_temp, now_secs() - 48 * 60 * 60).await;
    let faulty = FaultyVolume::wrapping(Arc::clone(&parent))
        .failing_call_at(
            FaultyOp::Delete,
            stale_temp.clone(),
            1,
            VolumeError::IoError {
                message: "the server refused".to_owned().into(),
                raw_os_error: None,
            },
        )
        .arc();

    assert!(
        run_add_edit(faulty.clone() as Arc<dyn Volume>, archive_path.clone()).await,
        "the edit commits despite a reap delete failure"
    );

    assert!(faulty.fault_fired(FaultyOp::Delete), "the reap's delete was refused");
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(back.contains_key("added.txt"), "the edit committed");
}

#[tokio::test]
async fn remote_edit_reap_spares_a_temp_the_ledger_records() {
    // A year-old temp the in-flight ledger records for this volume is a swap a
    // crash cut off after the original's delete: the archive's ONLY copy, which
    // the volume's sweep lands. The age gate alone would reap it.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let recorded_temp = PathBuf::from("/device/bundle.zip.cmdr-tmp-4444landing");
    let parent = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    seed_temp(parent.as_ref(), &recorded_temp, now_secs() - 365 * 24 * 60 * 60).await;
    let engine = TestOperations::new();
    let earlier = engine.new_operation_state(Duration::from_millis(50));
    track_temp(&earlier, Some(RecordHome::volume_space("remote")), &recorded_temp);

    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let result = pull_apply_upload_swap(
        parent.clone() as Arc<dyn Volume>,
        "remote",
        archive_path.clone(),
        state,
        None,
        move |working: &Path| -> Result<(), super::EditError> {
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

    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        names.iter().any(|n| n == "bundle.zip.cmdr-tmp-4444landing"),
        "❗ a recorded temp is never reaped, got: {names:?}"
    );
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
        "remote",
        archive_path.clone(),
        state,
        None,
        move |working: &Path| -> Result<(), super::EditError> {
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
        "remote",
        archive_path.clone(),
        state,
        None,
        move |working: &Path| -> Result<(), super::EditError> {
            let changeset = Changeset {
                adds: vec![add_entry("added.txt", b"fresh bytes")],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            state_for_closure.intent().set_for_test(OperationIntent::Stopped);
            Ok(())
        },
    )
    .await;
    assert!(
        matches!(result, Err(super::EditError::Cancelled)),
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
        "remote",
        archive_path.clone(),
        state,
        None,
        move |working: &Path| -> Result<(), super::EditError> {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_edit_journals_as_a_zip_edit_never_as_an_overwrite() {
    // The swap's delete of the archive notes the op not rollbackable because it
    // overwrote, as a copy's file Overwrite does. An archive edit has a reason of
    // its own, and the refusal a person reads names it.
    use crate::journal;
    use crate::testing::TestOperationLog;
    use cmdr_operation_log::store::{open_read_connection, read_operation};
    use cmdr_operation_log::types::{ArchiveSubkind, ExecutionStatus, Initiator, NotRollbackableReason};

    let op_id = "op-remote-edit-journal";
    let archive_path = PathBuf::from("/device/bundle.zip");
    let parent = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    let log = TestOperationLog::new(op_id);
    let engine = TestOperations::journaling_into(&log);
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));
    journal::open_archive_op(state.scope(), op_id, Initiator::User, "remote");

    let result = pull_apply_upload_swap(
        parent.clone() as Arc<dyn Volume>,
        "remote",
        archive_path.clone(),
        Arc::clone(&state),
        None,
        move |working: &Path| -> Result<(), super::EditError> {
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
    journal::finalize_archive_op(state.scope(), ArchiveSubkind::Edit, false, ExecutionStatus::Done, None);

    let conn = open_read_connection(log.db()).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(
        row.not_rollbackable_reason,
        Some(NotRollbackableReason::ZipEditUnsupported)
    );
}

// ---- A swap that goes wrong half-way ----------------------------------------
//
// The transfer engine's stale-temp sweep deletes any `.cmdr-tmp-*` it finds a
// while later, so a swap must never leave the only copy of the archive under its
// temp name with nothing recording it. Ported from `main`'s #513 cells: each
// asserts that no step loses a whole archive.

/// The volume id these cells record the swap under.
const SWAP_VOLUME_ID: &str = "remote-swap";

/// A phone-like parent (no atomic replace, same-name siblings allowed) seeded
/// with `bundle.zip`.
async fn sibling_allowing_parent(archive_path: &Path) -> Arc<InMemoryVolume> {
    let parent = InMemoryVolume::new("Remote-MTP").with_sibling_duplicates_allowed();
    parent.create_directory(Path::new("/device")).await.expect("seed dir");
    parent
        .create_file(archive_path, &build_zip(&[("keep.txt", b"keep me")]))
        .await
        .expect("seed zip");
    Arc::new(parent)
}

/// [`run_add_edit`] on `state`, answering the orchestration's own result.
async fn add_edit_on(
    parent: Arc<dyn Volume>,
    archive_path: PathBuf,
    state: Arc<WriteOperationState>,
) -> Result<(), super::EditError> {
    pull_apply_upload_swap(
        parent,
        SWAP_VOLUME_ID,
        archive_path,
        state,
        None,
        move |working: &Path| -> Result<(), super::EditError> {
            let changeset = Changeset {
                adds: vec![add_entry("added.txt", b"fresh bytes")],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await
}

/// [`add_edit_on`] on a running state of its own.
async fn add_edit(parent: Arc<dyn Volume>, archive_path: PathBuf) -> Result<(), super::EditError> {
    add_edit_on(parent, archive_path, running_state()).await
}

/// Every name in `dir`.
async fn names_in(parent: &dyn Volume, dir: &str) -> Vec<String> {
    sibling_names(parent, &Path::new(dir).join("x")).await
}

/// The paths in `/device` whose archive holds `entry`.
async fn archives_holding(parent: &dyn Volume, entry: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for name in names_in(parent, "/device").await {
        let at = Path::new("/device").join(&name);
        if read_remote_zip(parent, &at).await.contains_key(entry) {
            found.push(at);
        }
    }
    found
}

/// What the transfer engine's stale-temp sweep would leave: every
/// `.cmdr-tmp-*` in `/device` deleted, the harshest it could be.
async fn sweep_transfer_temps(parent: &dyn Volume) {
    for name in names_in(parent, "/device").await {
        if name.contains(".cmdr-tmp-") {
            parent
                .delete(&Path::new("/device").join(name))
                .await
                .expect("sweep a temp");
        }
    }
}

/// The aside the swap left in `/device`, if any.
async fn aside_in_device(parent: &dyn Volume) -> Option<PathBuf> {
    names_in(parent, "/device")
        .await
        .into_iter()
        .find(|name| name.contains(".cmdr-temp-"))
        .map(|name| Path::new("/device").join(name))
}

/// Neither the edit nor the original can take the archive's name: the edit
/// moves to a ` (recovered)` name the error names, ❌ never left in temp space,
/// and the original stays findable under a name of its own.
#[tokio::test]
async fn a_name_that_refuses_both_the_edit_and_the_original_keeps_both_findable() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let inner = sibling_allowing_parent(&archive_path).await;
    inner.set_rename_to_failing(&archive_path);

    let result = add_edit(inner.clone() as Arc<dyn Volume>, archive_path.clone()).await;

    let Err(super::EditError::Op(WriteOperationError::NewDataKeptAt { path, kept_at, .. })) = result else {
        panic!("the error must say where the edited archive is");
    };
    assert_eq!(path, "/device/bundle.zip");
    assert!(
        !kept_at.contains(".cmdr-tmp-"),
        "the edit must not keep a name the stale-temp sweep deletes: {kept_at}"
    );
    let edit = read_remote_zip(inner.as_ref(), Path::new(&kept_at)).await;
    assert!(edit.contains_key("added.txt"), "the edited archive, whole");
    let originals: Vec<PathBuf> = {
        let mut found = Vec::new();
        for at in archives_holding(inner.as_ref(), "keep.txt").await {
            if !read_remote_zip(inner.as_ref(), &at).await.contains_key("added.txt") {
                found.push(at);
            }
        }
        found
    };
    assert_eq!(originals.len(), 1, "❗ the original stays findable: {originals:?}");
    let names = names_in(inner.as_ref(), "/device").await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "nothing left in temp space, got: {names:?}"
    );
}

/// The edit can't take the name, but the original goes back to it: the edit
/// simply didn't happen, and nothing is left beside the original.
#[tokio::test]
async fn a_landing_that_fails_puts_the_original_back() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let parent = FaultyVolume::wrapping(sibling_allowing_parent(&archive_path).await)
        .failing_call(FaultyOp::Rename, 2, VolumeError::NotConnected("a hiccup".into()))
        .arc();

    let result = add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone()).await;

    assert!(parent.fault_fired(FaultyOp::Rename), "the landing rename failed");
    assert!(
        matches!(result, Err(super::EditError::Op(ref e)) if !matches!(e, WriteOperationError::NewDataKeptAt { .. })),
        "a plain refusal: the original is back"
    );
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(
        back.contains_key("keep.txt") && !back.contains_key("added.txt"),
        "the original, at its name"
    );
    assert_eq!(
        names_in(parent.as_ref(), "/device").await,
        ["bundle.zip"],
        "no temp, no aside"
    );
}

/// The move aside is refused and the original is still there: the edit didn't
/// happen, the original keeps its bytes, and the spare temp goes.
#[tokio::test]
async fn a_refused_move_aside_leaves_the_original_whole_and_takes_the_temp_away() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let parent = FaultyVolume::wrapping(sibling_allowing_parent(&archive_path).await)
        .failing_call(FaultyOp::Rename, 1, VolumeError::NotConnected("busy".into()))
        .arc();

    let result = add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone()).await;

    assert!(parent.fault_fired(FaultyOp::Rename), "the move aside was refused");
    assert!(
        matches!(result, Err(super::EditError::Op(ref e)) if !matches!(e, WriteOperationError::NewDataKeptAt { .. })),
        "a plain refusal: nothing moved"
    );
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(
        back.contains_key("keep.txt") && !back.contains_key("added.txt"),
        "the original, untouched"
    );
    assert_eq!(
        names_in(parent.as_ref(), "/device").await,
        ["bundle.zip"],
        "no temp, no aside"
    );
}

/// The edit landed but its old version couldn't be deleted: the edit is in
/// place and the old version stays beside it under a name no stale-temp sweep
/// takes, recorded so the volume's next sweep can remove it once it sees the
/// edit whole at the name.
#[tokio::test]
async fn an_aside_that_wont_delete_stays_beside_the_landed_edit() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let parent = FaultyVolume::wrapping(sibling_allowing_parent(&archive_path).await)
        .failing_call(FaultyOp::Delete, 1, VolumeError::NotConnected("busy".into()))
        .arc();

    assert!(
        add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone())
            .await
            .is_ok()
    );

    assert!(parent.fault_fired(FaultyOp::Delete), "the aside delete was refused");
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(back.contains_key("added.txt"), "the edit is in place");
    let aside = aside_in_device(parent.as_ref()).await.expect("the old version stays");
    assert!(read_remote_zip(parent.as_ref(), &aside).await.contains_key("keep.txt"));
}

/// A crash after each step of the swap (modelled as the device going silent:
/// every call after the cut fails, so nothing more reaches it), then the next
/// launch's ledger sweep, then the harshest stale-temp sweep, then the next
/// remote zip edit in that folder. At every cut a complete archive is at the
/// archive's name, ❗ the original is never lost before the edit is whole at
/// the name, and nothing is left in temp space or aside.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crash_after_any_step_of_the_swap_loses_no_archive() {
    use crate::in_flight_temps::Ledger;

    let archive_path = PathBuf::from("/device/bundle.zip");
    let sibling = PathBuf::from("/device/other.zip");
    // Mutating calls in order: upload, move aside, landing, delete the aside.
    for cut in 1..=4 {
        let dir = cmdr_fs::testing::TestDir::new(&format!("remote-swap-crash-{cut}"));
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).expect("data dir");
        let ledger = Ledger::recording_in(&data_dir);
        let engine = TestOperations::new();
        let state = Arc::new(
            engine
                .new_operation_state(Duration::from_millis(50))
                .with_in_flight_ledger(ledger.clone()),
        );
        let inner = sibling_allowing_parent(&archive_path).await;
        inner
            .create_file(&sibling, &build_zip(&[("other.txt", b"other")]))
            .await
            .expect("seed the sibling zip");
        let dying = Arc::new(GoesSilentAfter::new(Arc::clone(&inner), cut));

        // Success once the edit took its name, whatever happens to the aside.
        let landed = add_edit_on(dying as Arc<dyn Volume>, archive_path.clone(), state)
            .await
            .is_ok();
        assert_eq!(landed, cut >= 3, "cut {cut}: success only once the edit took its name");
        if cut == 2 {
            assert!(
                !inner.exists(&archive_path).await && aside_in_device(inner.as_ref()).await.is_some(),
                "cut 2: the original is aside, its name empty"
            );
        }

        // The process dies, and the next one launches with the device back.
        ledger.die();
        let next_app = TestOperations::new();
        next_app
            .volumes()
            .register(SWAP_VOLUME_ID, Arc::clone(&inner) as Arc<dyn Volume>);
        let tally = Ledger::for_test().launch_in(&data_dir, &next_app).wait();
        sweep_transfer_temps(inner.as_ref()).await;
        assert!(
            add_edit(inner.clone() as Arc<dyn Volume>, sibling.clone())
                .await
                .is_ok(),
            "cut {cut}: the next touch"
        );

        let back = read_remote_zip(inner.as_ref(), &archive_path).await;
        assert!(
            back.contains_key("keep.txt"),
            "cut {cut}: a whole archive at the name: {tally:?}"
        );
        // Cut 2 left the original aside and the edit on its temp, recorded as
        // a landing (the edit reported it kept there): the sweep finishes that
        // landing and, with the edit whole at the name, removes the aside.
        assert_eq!(
            back.contains_key("added.txt"),
            cut >= 2,
            "cut {cut}: the right version at the name"
        );
        let names = names_in(inner.as_ref(), "/device").await;
        assert!(
            !names
                .iter()
                .any(|n| n.contains(".cmdr-tmp-") || n.contains(".cmdr-temp-")),
            "cut {cut}: nothing left in temp space or aside: {names:?}"
        );
        drop(engine);
    }
}

/// Two old versions of one archive sit aside under scratch names nothing
/// records (a crash of some earlier swap): nothing in a name says which is
/// newer, so a remote edit in that folder leaves both visible and puts neither
/// at the archive's name.
#[tokio::test]
async fn two_asides_claiming_one_empty_name_both_stay_put() {
    let sibling = PathBuf::from("/device/other.zip");
    let inner = remote_parent_with_zip(&sibling, &build_zip(&[("other.txt", b"other")])).await;
    let older = PathBuf::from("/device/bundle.zip.cmdr-temp-00000000-0000-4000-8000-000000000001");
    let newer = PathBuf::from("/device/bundle.zip.cmdr-temp-ffffffff-0000-4000-8000-000000000002");
    inner
        .create_file(&older, &build_zip(&[("v.txt", b"older")]))
        .await
        .expect("seed the older aside");
    inner
        .create_file(&newer, &build_zip(&[("v.txt", b"newer")]))
        .await
        .expect("seed the newer aside");

    assert!(add_edit(inner.clone() as Arc<dyn Volume>, sibling).await.is_ok());

    assert!(
        !inner.exists(Path::new("/device/bundle.zip")).await,
        "an ambiguous pair must not pick a version for the name"
    );
    assert!(
        inner.exists(&older).await && inner.exists(&newer).await,
        "both asides stay"
    );
}

/// A parent that answers normally for its first `mutations` writes, renames,
/// and deletes, then goes silent: every call after that fails, the way nothing
/// reaches a phone that was unplugged or a process that died.
struct GoesSilentAfter {
    inner: Arc<InMemoryVolume>,
    mutations_left: std::sync::Mutex<usize>,
}

impl GoesSilentAfter {
    fn new(inner: Arc<InMemoryVolume>, mutations: usize) -> Self {
        Self {
            inner,
            mutations_left: std::sync::Mutex::new(mutations),
        }
    }

    /// Whether the cut has passed.
    fn silent(&self) -> bool {
        *self.mutations_left.lock().expect("lock") == 0
    }

    /// Spends one mutation, or answers that the parent is silent.
    fn mutate(&self) -> Result<(), VolumeError> {
        let mut left = self.mutations_left.lock().expect("lock");
        if *left == 0 {
            return Err(gone());
        }
        *left -= 1;
        Ok(())
    }
}

fn gone() -> VolumeError {
    VolumeError::NotConnected("the device went away".into())
}

impl Volume for GoesSilentAfter {
    crate::forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        local_path,
        supports_streaming,
        supports_local_fs_access,
        operations_are_local,
        create_directory_errors_on_existing_dir,
        supports_unknown_length_writes,
        write_is_single_shot,
        publishes_writes_whole,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<cmdr_fs::entry::FileEntry>, VolumeError>> + Send + 'a>> {
        if self.silent() {
            return Box::pin(async { Err(gone()) });
        }
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::entry::FileEntry, VolumeError>> + Send + 'a>> {
        if self.silent() {
            return Box::pin(async { Err(gone()) });
        }
        self.inner.get_metadata(path)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn cmdr_fs::volume::VolumeReadStream>, VolumeError>> + Send + 'a>>
    {
        if self.silent() {
            return Box::pin(async { Err(gone()) });
        }
        self.inner.open_read_stream(path)
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: cmdr_fs::volume::WriteMode,
        length: cmdr_fs::volume::StreamLength,
        stream: Box<dyn cmdr_fs::volume::VolumeReadStream>,
        on_progress: &'a (dyn Fn(cmdr_fs::volume::StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::StreamWritten, VolumeError>> + Send + 'a>> {
        if let Err(e) = self.mutate() {
            return Box::pin(async { Err(e) });
        }
        self.inner.write_from_stream(dest, mode, length, stream, on_progress)
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        if let Err(e) = self.mutate() {
            return Box::pin(async { Err(e) });
        }
        self.inner.rename(from, to, force)
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        if let Err(e) = self.mutate() {
            return Box::pin(async { Err(e) });
        }
        self.inner.delete(path)
    }
}

/// The replacing rename lands on the server but its answer is an error. The temp
/// is gone, so the swap happened: the edit is reported as landed, and nothing
/// deletes the archive that now holds it.
#[tokio::test]
async fn a_replacing_rename_that_landed_despite_its_error_keeps_the_edit() {
    let archive_path = PathBuf::from("/share/bundle.zip");
    let inner = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    let parent = Arc::new(RenameLandsButErrs { inner });

    let result = add_edit(parent.clone() as Arc<dyn Volume>, archive_path.clone()).await;

    assert!(result.is_ok(), "the rename landed, so the edit committed");
    let back = read_remote_zip(parent.as_ref(), &archive_path).await;
    assert!(
        back.contains_key("keep.txt") && back.contains_key("added.txt"),
        "the edit is in place"
    );
    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "no leftover upload temp, got: {names:?}"
    );
}

/// A parent whose replacing rename APPLIES and still answers an error, the way a
/// lost response looks over a network share.
struct RenameLandsButErrs {
    inner: Arc<InMemoryVolume>,
}

impl Volume for RenameLandsButErrs {
    crate::forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        list_directory,
        get_metadata,
        exists,
        is_directory,
        create_file,
        create_directory,
        create_directory_all,
        delete,
        open_read_stream,
        write_from_stream,
        local_path,
        supports_streaming,
        supports_local_fs_access,
        operations_are_local,
        create_directory_errors_on_existing_dir,
        supports_unknown_length_writes,
        write_is_single_shot,
        publishes_writes_whole,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Stands in for SMB, the backend whose replace is one atomic step.
    fn supports_atomic_replace_rename(&self) -> bool {
        true
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.inner.rename(from, to, force).await?;
            if force {
                Err(VolumeError::NotConnected("the reply got lost".into()))
            } else {
                Ok(())
            }
        })
    }
}

/// A backend whose forced rename clears the destination in a step of its own
/// (SFTP without `posix-rename@openssh.com`, for one) and dies between the two
/// halves. The swap must not hand it the replace: the original would be gone
/// and the edit left under a `.cmdr-tmp-*` name the stale-temp sweep deletes.
#[tokio::test]
async fn a_backend_whose_forced_rename_clears_first_never_gets_the_replace() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let inner = remote_parent_with_zip(&archive_path, &build_zip(&[("keep.txt", b"keep me")])).await;
    let dying = Arc::new(ClearsThenDiesOnAForcedRename {
        inner: Arc::clone(&inner),
        dead: AtomicBool::new(false),
    });

    let _ = add_edit(dying as Arc<dyn Volume>, archive_path.clone()).await;
    sweep_transfer_temps(inner.as_ref()).await;

    assert!(
        !archives_holding(inner.as_ref(), "keep.txt").await.is_empty(),
        "a complete archive must survive the sweep"
    );
}

/// A parent whose forced rename deletes the destination, then dies: every call
/// after that fails, the way nothing reaches a server once the process is gone.
struct ClearsThenDiesOnAForcedRename {
    inner: Arc<InMemoryVolume>,
    dead: AtomicBool,
}

impl ClearsThenDiesOnAForcedRename {
    fn alive(&self) -> Result<(), VolumeError> {
        if self.dead.load(Ordering::SeqCst) {
            Err(gone())
        } else {
            Ok(())
        }
    }
}

impl Volume for ClearsThenDiesOnAForcedRename {
    crate::forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        local_path,
        supports_streaming,
        supports_local_fs_access,
        operations_are_local,
        create_directory_errors_on_existing_dir,
        supports_unknown_length_writes,
        write_is_single_shot,
        publishes_writes_whole,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<cmdr_fs::entry::FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.alive()?;
            self.inner.list_directory(path, on_progress).await
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::entry::FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.alive()?;
            self.inner.get_metadata(path).await
        })
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn cmdr_fs::volume::VolumeReadStream>, VolumeError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.alive()?;
            self.inner.open_read_stream(path).await
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: cmdr_fs::volume::WriteMode,
        length: cmdr_fs::volume::StreamLength,
        stream: Box<dyn cmdr_fs::volume::VolumeReadStream>,
        on_progress: &'a (dyn Fn(cmdr_fs::volume::StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::StreamWritten, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.alive()?;
            self.inner
                .write_from_stream(dest, mode, length, stream, on_progress)
                .await
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.alive()?;
            if force && self.inner.exists(to).await {
                self.inner.delete(to).await?;
                self.dead.store(true, Ordering::SeqCst);
                return Err(gone());
            }
            self.inner.rename(from, to, force).await
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.alive()?;
            self.inner.delete(path).await
        })
    }
}

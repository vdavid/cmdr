//! Rollback engine tests (M3): the hard data-safety invariants, TDD'd.
//!
//! The pure decision helpers (`verify_snapshot`, `is_self_collision`,
//! `inverse_action`, …) are unit-tested in isolation. The per-kind reversal is
//! tested end-to-end against `InMemoryVolume`s: seed the journal with the rows an
//! operation would have recorded (with realistic size/mtime snapshots — the
//! capture layer's correctness is M2's concern; here the engine is under test) and
//! the post-op filesystem state, run `execute_rollback`, and assert the invariant
//! **apply-then-rollback == original state**, plus the specific data-loss traps
//! D7 surfaced.

use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::file_system::VolumeManager;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::operation_log::store::{
    OperationRow, open_read_connection, operation_log_db_path, read_inverse_op, read_operation, read_operation_items,
};
use crate::operation_log::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState, RowRole,
    SearchCoverage,
};
use crate::operation_log::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter};

/// A fixed mtime pinned onto seeded files so the recorded snapshot and the live
/// entry agree (verify → Match).
const MT: u64 = 1_700_000_000;

// ── Harness ──────────────────────────────────────────────────────────────────

/// A test rig: a writer over a temp-DB journal + a volume registry the engine
/// resolves item volumes through. The temp dir is returned so it outlives the run.
struct Rig {
    writer: OperationLogWriter,
    vm: VolumeManager,
    _dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = operation_log_db_path(dir.path());
        let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
        Rig {
            writer,
            vm: VolumeManager::new(),
            _dir: dir,
        }
    }

    fn register(&self, id: &str, vol: Arc<InMemoryVolume>) {
        self.vm.register(id, vol as Arc<dyn Volume>);
    }

    fn read_op(&self, op_id: &str) -> OperationRow {
        let conn = open_read_connection(self.writer.db_path()).expect("read conn");
        read_operation(&conn, op_id).expect("read").expect("op present")
    }

    /// Seed an operation header + item rows + a terminal `rollback_state`, exactly
    /// as the capture layer would after the op ran.
    fn seed(
        &self,
        op_id: &str,
        kind: OpKind,
        src_vol: &str,
        dst_vol: Option<&str>,
        state: RollbackState,
        items: Vec<JournalItem>,
    ) {
        self.writer
            .open_operation(OpenOperation {
                op_id: op_id.to_string(),
                kind,
                initiator: Initiator::User,
                source_volume_id: Some(src_vol.to_string()),
                dest_volume_id: dst_vol.map(str::to_string),
                item_count: items.len() as u64,
                started_at: 100,
                rolls_back_op_id: None,
                execution_status: ExecutionStatus::Running,
            })
            .expect("open");
        let n = items.len() as u64;
        if !items.is_empty() {
            self.writer.record_items(op_id, items).expect("record");
        }
        self.writer
            .finalize_operation(FinalizeOperation {
                op_id: op_id.to_string(),
                execution_status: ExecutionStatus::Done,
                rollback_state: state,
                not_rollbackable_reason: None,
                archive_subkind: None,
                search_coverage: SearchCoverage::Full,
                search_coverage_reason: None,
                ended_at: 200,
                item_count: None,
                items_done: n,
                bytes_total: 0,
                dev_summary: None,
            })
            .expect("finalize");
        self.writer.flush_blocking().expect("flush");
    }

    async fn rollback(&self, op_id: &str) -> RollbackReport {
        let original = self.read_op(op_id);
        execute_rollback(&self.vm, &self.writer, &original, "inv-1", Initiator::User, &|| false).await
    }
}

fn split(path: &str) -> (String, String) {
    let p = Path::new(path);
    (
        p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}

/// A `rollback_unit` file row: source on `src_vol`, its landed copy/move on
/// `dst_vol`, with a size + mtime snapshot.
fn file_unit(seq: i64, src_vol: &str, src: &str, dst_vol: &str, dst: &str, size: i64) -> JournalItem {
    let (sd, sn) = split(src);
    let (dd, dn) = split(dst);
    JournalItem {
        seq,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: src_vol.to_string(),
        source_dir: sd,
        source_name: sn,
        dest_volume_id: Some(dst_vol.to_string()),
        dest_dir: Some(dd),
        dest_name: Some(dn),
        size: Some(size),
        mtime: Some(MT as i64),
        outcome: ItemOutcome::Done,
        overwrote: false,
    }
}

/// A created-directory `rollback_unit` row (source == dest == the created path).
fn dir_unit(seq: i64, vol: &str, path: &str) -> JournalItem {
    let (d, n) = split(path);
    JournalItem {
        seq,
        entry_type: EntryType::Dir,
        row_role: RowRole::RollbackUnit,
        source_volume_id: vol.to_string(),
        source_dir: d.clone(),
        source_name: n.clone(),
        dest_volume_id: Some(vol.to_string()),
        dest_dir: Some(d),
        dest_name: Some(n),
        size: None,
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
    }
}

async fn put(vol: &InMemoryVolume, path: &str, content: &[u8]) {
    vol.create_file(Path::new(path), content).await.expect("create_file");
    vol.set_modified_at(Path::new(path), Some(MT));
}

async fn mkdir(vol: &InMemoryVolume, path: &str) {
    vol.create_directory(Path::new(path)).await.expect("create_directory");
}

async fn exists(vol: &InMemoryVolume, path: &str) -> bool {
    vol.exists(Path::new(path)).await
}

async fn read(vol: &InMemoryVolume, path: &str) -> Vec<u8> {
    let mut s = vol.open_read_stream(Path::new(path)).await.expect("open stream");
    let mut out = Vec::new();
    while let Some(chunk) = s.next_chunk().await {
        out.extend_from_slice(&chunk.expect("chunk"));
    }
    out
}

fn entry(name: &str, inode: Option<u64>, size: Option<u64>, mtime: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        modified_at: mtime,
        inode,
        ..FileEntry::new(name.to_string(), format!("/{name}"), false, false)
    }
}

// ── Pure decision helpers ────────────────────────────────────────────────────

#[test]
fn verify_snapshot_match_drift_unverifiable() {
    let live = entry("f", None, Some(10), Some(MT));
    // Both fields recorded and equal ⇒ Match.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), &live),
        SnapshotVerdict::Match
    );
    // A recorded field differs ⇒ Drift.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64 + 1), &live),
        SnapshotVerdict::Drift
    );
    assert_eq!(
        verify_snapshot(Some(11), Some(MT as i64), &live),
        SnapshotVerdict::Drift
    );
    // Only size recorded (volume transfers carry no mtime), and it matches ⇒ Match.
    assert_eq!(verify_snapshot(Some(10), None, &live), SnapshotVerdict::Match);
    // A recorded field whose live counterpart is absent ⇒ Unverifiable (fail safe).
    let no_mtime = entry("f", None, Some(10), None);
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), &no_mtime),
        SnapshotVerdict::Unverifiable
    );
    // Nothing recorded ⇒ nothing to prove identity on ⇒ Unverifiable.
    assert_eq!(verify_snapshot(None, None, &live), SnapshotVerdict::Unverifiable);
}

#[test]
fn self_collision_by_inode_and_by_case_fold() {
    // Real inodes (LocalPosix): same inode ⇒ self, whatever the names.
    let a = entry("dog.JPG", Some(7), Some(1), Some(MT));
    let b = entry("dog.jpg", Some(7), Some(1), Some(MT));
    assert!(is_self_collision(
        true,
        Path::new("/d/dog.JPG"),
        Path::new("/d/dog.jpg"),
        &a,
        &b
    ));
    // Different inodes ⇒ a real collision even with the same name.
    let c = entry("dog.jpg", Some(9), Some(1), Some(MT));
    assert!(!is_self_collision(
        true,
        Path::new("/d/dog.jpg"),
        Path::new("/d/dog.jpg"),
        &a,
        &c
    ));
    // No inodes (MTP/SMB), same volume, case-only difference ⇒ self by path fold.
    let x = entry("dog.JPG", None, Some(1), Some(MT));
    let y = entry("dog.jpg", None, Some(1), Some(MT));
    assert!(is_self_collision(
        true,
        Path::new("/d/dog.JPG"),
        Path::new("/d/dog.jpg"),
        &x,
        &y
    ));
    // No inodes, DIFFERENT volume, same relative path ⇒ NOT self (the occupant is a
    // genuinely different file — a move-back must never overwrite it).
    assert!(!is_self_collision(
        false,
        Path::new("/a.txt"),
        Path::new("/a.txt"),
        &x,
        &y
    ));
    // No inodes, same volume, genuinely different sibling ⇒ not self.
    assert!(!is_self_collision(
        true,
        Path::new("/d/cat.jpg"),
        Path::new("/d/dog.jpg"),
        &x,
        &y
    ));
}

#[test]
fn pure_mapping_helpers() {
    assert_eq!(inverse_kind(OpKind::Copy), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::CreateFolder), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::ArchiveEdit), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::Move), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Trash), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Rename), OpKind::Rename);

    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::File),
        Some(InverseAction::RemoveFileIfUnchanged)
    );
    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::CreateFolder, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::Move, EntryType::File),
        Some(InverseAction::RestoreMove)
    );
    assert_eq!(inverse_action(OpKind::Delete, EntryType::File), None);

    assert_eq!(resolve_final_state(3, 0, false), RollbackState::RolledBack);
    assert_eq!(resolve_final_state(2, 1, false), RollbackState::PartiallyRolledBack);
    // Non-canceled, all skipped ⇒ partial (won't clear on retry).
    assert_eq!(resolve_final_state(0, 3, false), RollbackState::PartiallyRolledBack);
    // Canceled with nothing reversed ⇒ back to rollbackable (a clean retry).
    assert_eq!(resolve_final_state(0, 0, true), RollbackState::Rollbackable);
    // Canceled after reversing some ⇒ partial.
    assert_eq!(resolve_final_state(3, 0, true), RollbackState::PartiallyRolledBack);
}

// ── Per-kind reversal: apply-then-rollback == original ───────────────────────

#[tokio::test]
async fn copy_rollback_removes_copies_and_created_dirs() {
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    // Post-copy DST state: a whole /tree copied in (files + the dirs the copy made).
    mkdir(&dst, "/tree").await;
    mkdir(&dst, "/tree/sub").await;
    put(&dst, "/tree/a.txt", b"aaa").await;
    put(&dst, "/tree/sub/b.txt", b"bbbb").await;
    rig.register("src", src);
    rig.register("dst", dst.clone());

    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/tree/a.txt", "dst", "/tree/a.txt", 3),
            file_unit(1, "src", "/tree/sub/b.txt", "dst", "/tree/sub/b.txt", 4),
            dir_unit(2, "dst", "/tree"),
            dir_unit(3, "dst", "/tree/sub"),
        ],
    );

    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(report.reversed, 4);
    assert_eq!(report.skipped, 0);
    // DST is back to its original (empty) state — no copied files, NO empty dirs left.
    assert!(!exists(&dst, "/tree/a.txt").await);
    assert!(!exists(&dst, "/tree/sub/b.txt").await);
    assert!(!exists(&dst, "/tree/sub").await, "the deeper created dir is removed");
    assert!(
        !exists(&dst, "/tree").await,
        "the created dir is removed after its contents"
    );

    // The original op's items are all marked rolled_back, and the inverse op is
    // journaled linking back to it.
    let conn = open_read_connection(rig.writer.db_path()).expect("conn");
    let items = read_operation_items(&conn, "op", 100).expect("items");
    assert!(items.iter().all(|i| i.outcome == ItemOutcome::RolledBack));
    let inverse = read_inverse_op(&conn, "op").expect("inv").expect("present");
    assert_eq!(inverse.op_id, "inv-1");
    assert_eq!(inverse.kind, OpKind::Delete, "undoing a copy is a delete");
    assert_eq!(inverse.rolls_back_op_id.as_deref(), Some("op"));
}

#[tokio::test]
async fn move_rollback_restores_files_cross_volume() {
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    // Post-move: files live on DST, SRC is empty.
    put(&dst, "/one.txt", b"1").await;
    put(&dst, "/two.txt", b"22").await;
    rig.register("src", src.clone());
    rig.register("dst", dst.clone());

    rig.seed(
        "op",
        OpKind::Move,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/one.txt", "dst", "/one.txt", 1),
            file_unit(1, "src", "/two.txt", "dst", "/two.txt", 2),
        ],
    );

    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    // Files are back on SRC with their bytes, and gone from DST.
    assert_eq!(read(&src, "/one.txt").await, b"1");
    assert_eq!(read(&src, "/two.txt").await, b"22");
    assert!(!exists(&dst, "/one.txt").await);
    assert!(!exists(&dst, "/two.txt").await);
}

#[tokio::test]
async fn same_volume_move_and_rename_restore_by_rename() {
    // A same-volume move (top-level) and a rename both reverse via a same-volume
    // rename back.
    for (kind, src, dst) in [
        (OpKind::Move, "/src/a.txt", "/dst/a.txt"),
        (OpKind::Rename, "/photo.jpg", "/image.jpg"),
    ] {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, dst, b"data").await; // the item now sits at its post-op location
        rig.register("v", v.clone());
        rig.seed(
            "op",
            kind,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", src, "v", dst, 4)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack, "{kind:?}");
        assert!(exists(&v, src).await, "{kind:?}: restored to its original name");
        assert!(!exists(&v, dst).await, "{kind:?}: gone from its post-op name");
    }
}

#[tokio::test]
async fn trash_rollback_restores_from_recorded_in_trash_location() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    // The trashed item sits at its recorded in-trash location.
    put(&v, "/.Trash/doc.txt", b"hello").await;
    rig.register("v", v.clone());
    // Trash rows carry the original path as source, the in-trash location as dest.
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/doc.txt", "v", "/.Trash/doc.txt", 5)],
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(read(&v, "/doc.txt").await, b"hello", "restored to the original path");
    assert!(!exists(&v, "/.Trash/doc.txt").await, "removed from the trash");
}

#[tokio::test]
async fn trash_rollback_skips_when_trash_was_emptied() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    // The trash was emptied since: the in-trash location is gone.
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/doc.txt", "v", "/.Trash/doc.txt", 5)],
    );
    let report = rig.rollback("op").await;
    // Gone ⇒ the desired end state (absent from trash) already holds, an idempotent
    // no-op, so nothing to restore and the item isn't a blocking skip.
    assert!(!exists(&v, "/doc.txt").await);
    assert_eq!(report.reversed, 1, "an already-gone item is an idempotent no-op");
}

#[tokio::test]
async fn create_folder_rollback_removes_if_empty_but_skips_if_a_file_was_added() {
    // Empty ⇒ removed.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        mkdir(&v, "/newdir").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFolder,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![dir_unit(0, "v", "/newdir")],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack);
        assert!(!exists(&v, "/newdir").await);
    }
    // A file added since ⇒ the folder is NOT swept away; partial.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        mkdir(&v, "/newdir").await;
        put(&v, "/newdir/added.txt", b"mine").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFolder,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![dir_unit(0, "v", "/newdir")],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
        assert!(exists(&v, "/newdir").await, "the non-empty dir is kept");
        assert_eq!(read(&v, "/newdir/added.txt").await, b"mine", "the added file survives");
    }
}

#[tokio::test]
async fn create_file_rollback_removes_unchanged_but_skips_modified() {
    // Unchanged ⇒ removed.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/new.txt", b"orig").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFile,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/new.txt", "v", "/new.txt", 4)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack);
        assert!(!exists(&v, "/new.txt").await);
    }
    // Modified since (size changed) ⇒ drift ⇒ skipped, the file survives.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/new.txt", b"orig-and-then-edited").await; // size != recorded 4
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFile,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/new.txt", "v", "/new.txt", 4)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
        assert!(exists(&v, "/new.txt").await, "a modified created file is never deleted");
    }
}

#[tokio::test]
async fn compress_rollback_deletes_net_new_archive_but_never_a_modified_one() {
    // Unchanged net-new archive ⇒ deleted.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/out.zip", b"ZIPBYTES").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::ArchiveEdit,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/out.zip", "v", "/out.zip", 8)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack);
        assert!(!exists(&v, "/out.zip").await);
    }
    // The user zip-edited the archive afterward (size changed) ⇒ the recheck sees
    // drift ⇒ the archive is untouched (deleting it would destroy their additions).
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/out.zip", b"ZIPBYTES-plus-a-new-entry").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::ArchiveEdit,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/out.zip", "v", "/out.zip", 8)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
        assert_eq!(
            read(&v, "/out.zip").await,
            b"ZIPBYTES-plus-a-new-entry",
            "a modified archive is untouched"
        );
    }
}

// ── The data-loss traps ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_new_file_at_the_restore_path_is_never_overwritten() {
    // move / trash / rename: occupy the restore target with a NEW file and assert
    // the undo skips that item and leaves the new file byte-for-byte intact.
    for (kind, orig, landed) in [
        (OpKind::Move, "/a.txt", "/moved/a.txt"),
        (OpKind::Trash, "/a.txt", "/.Trash/a.txt"),
        (OpKind::Rename, "/a.txt", "/b.txt"),
    ] {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, landed, b"original").await;
        // The user has since created a NEW file at the original path.
        put(&v, orig, b"the-users-new-file").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            kind,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", orig, "v", landed, 8)],
        );

        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack, "{kind:?}");
        assert_eq!(report.skipped, 1, "{kind:?}: the occupied-target item is skipped");
        assert_eq!(
            read(&v, orig).await,
            b"the-users-new-file",
            "{kind:?}: the new file is intact"
        );
        assert!(
            exists(&v, landed).await,
            "{kind:?}: the moved item stays put (not lost)"
        );
    }
}

#[tokio::test]
async fn drift_on_one_item_skips_only_that_item() {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/keep.txt", b"XXXX").await; // will be modified below (drift)
    put(&dst, "/gone.txt", b"YY").await; // unchanged ⇒ reversible
    // Modify keep.txt so it no longer matches its recorded size snapshot.
    dst.set_reported_size(Path::new("/keep.txt"), 9999);
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/keep.txt", "dst", "/keep.txt", 4),
            file_unit(1, "src", "/gone.txt", "dst", "/gone.txt", 2),
        ],
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert!(exists(&dst, "/keep.txt").await, "the drifted copy is NOT deleted");
    assert!(!exists(&dst, "/gone.txt").await, "the unchanged copy IS deleted");
}

#[tokio::test]
async fn unverifiable_precondition_skips_never_proceeds() {
    // A copy leaf whose mtime was recorded but whose live entry can't report it
    // (an InMemoryVolume standing in for MTP/SMB with modified: None): the recheck
    // is Unverifiable, so the item is skipped rather than deleted.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/x.bin", b"1234").await;
    dst.set_modified_at(Path::new("/x.bin"), None); // backend can't prove the mtime
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "src", "/x.bin", "dst", "/x.bin", 4)],
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(
        report.final_state,
        RollbackState::PartiallyRolledBack,
        "an unverifiable skip lands partial (won't clear on retry)"
    );
    assert!(exists(&dst, "/x.bin").await, "an unverifiable item is never deleted");
}

#[tokio::test]
async fn cancel_stops_and_keeps_what_was_reversed() {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    for i in 0..4 {
        put(&dst, &format!("/f{i}.txt"), b"x").await;
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        (0..4)
            .map(|i| file_unit(i, "src", &format!("/f{i}.txt"), "dst", &format!("/f{i}.txt"), 1))
            .collect(),
    );
    // Cancel immediately: the predicate is already true when the run starts.
    let original = rig.read_op("op");
    let report = execute_rollback(&rig.vm, &rig.writer, &original, "inv-1", Initiator::User, &|| true).await;
    assert!(report.canceled);
    assert_eq!(report.reversed, 0, "canceled before any item ran");
    assert_eq!(
        report.final_state,
        RollbackState::Rollbackable,
        "nothing reversed ⇒ retryable"
    );
    // The copies are untouched — a canceled rollback keeps the pre-rollback state.
    assert!(exists(&dst, "/f0.txt").await);
}

#[tokio::test]
async fn a_canceled_original_op_rolls_back_exactly_its_completed_items() {
    // A copy canceled mid-way journals only the files it actually completed (M2),
    // so rolling it back reverses exactly those — a canceled `execution_status`
    // never blocks rollback (D4). Here only one of the two intended files landed.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/done.txt", b"1").await; // the completed copy
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.writer
        .open_operation(OpenOperation {
            op_id: "op".into(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("src".into()),
            dest_volume_id: Some("dst".into()),
            item_count: 2, // two were planned...
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    // ...but only one completed before the cancel, so only one row exists.
    rig.writer
        .record_items("op", vec![file_unit(0, "src", "/done.txt", "dst", "/done.txt", 1)])
        .expect("record");
    rig.writer
        .finalize_operation(FinalizeOperation {
            op_id: "op".into(),
            execution_status: ExecutionStatus::Canceled,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: 2,
            item_count: None,
            items_done: 1,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
    rig.writer.flush_blocking().expect("flush");

    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 1, "reverses exactly the completed item");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&dst, "/done.txt").await, "the one completed copy is undone");
}

#[tokio::test]
async fn streams_a_large_op_across_pages() {
    // More units than one page (ROLLBACK_PAGE = 512) proves the paged cursor
    // advances across pages without materializing the whole list.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    const N: i64 = 1_200;
    let mut units = Vec::new();
    for i in 0..N {
        let p = format!("/f{i}.bin");
        put(&dst, &p, b"x").await;
        units.push(file_unit(i, "src", &p, "dst", &p, 1));
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, N as u64, "every page's items reversed");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&dst, "/f0.bin").await);
    assert!(!exists(&dst, &format!("/f{}.bin", N - 1)).await);
}

// ── The op-level gate ────────────────────────────────────────────────────────

#[test]
fn check_rollbackable_gates_state_and_connectivity() {
    let vm = VolumeManager::new();
    vm.register("src", Arc::new(InMemoryVolume::new("Src")) as Arc<dyn Volume>);
    vm.register("dst", Arc::new(InMemoryVolume::new("Dst")) as Arc<dyn Volume>);

    let base = |state: RollbackState, reason: Option<NotRollbackableReason>, dst: Option<&str>| OperationRow {
        op_id: "op".into(),
        kind: OpKind::Copy,
        archive_subkind: None,
        initiator: Initiator::User,
        execution_status: ExecutionStatus::Done,
        rollback_state: state,
        not_rollbackable_reason: reason,
        rolls_back_op_id: None,
        source_volume_id: Some("src".into()),
        dest_volume_id: dst.map(str::to_string),
        started_at: 1,
        ended_at: Some(2),
        item_count: 1,
        items_done: 1,
        bytes_total: 0,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        dev_summary: None,
    };

    // Rollbackable + all volumes present ⇒ Ok.
    assert!(check_rollbackable(&vm, &base(RollbackState::Rollbackable, None, Some("dst"))).is_ok());
    // Already rolling back ⇒ typed refusal (double-rollback guard).
    assert_eq!(
        check_rollbackable(&vm, &base(RollbackState::RollingBack, None, Some("dst"))),
        Err(RollbackRefusal::AlreadyRollingBack)
    );
    // Already rolled back ⇒ nothing to do.
    assert_eq!(
        check_rollbackable(&vm, &base(RollbackState::RolledBack, None, Some("dst"))),
        Err(RollbackRefusal::AlreadyRolledBack)
    );
    // Not rollbackable (a delete) ⇒ carries the stored reason.
    assert_eq!(
        check_rollbackable(
            &vm,
            &base(
                RollbackState::NotRollbackable,
                Some(NotRollbackableReason::PermanentDelete),
                Some("dst")
            )
        ),
        Err(RollbackRefusal::NotRollbackable(NotRollbackableReason::PermanentDelete))
    );
    // A move that overwrote ⇒ not rollbackable with the overwrote reason.
    assert_eq!(
        check_rollbackable(
            &vm,
            &base(
                RollbackState::NotRollbackable,
                Some(NotRollbackableReason::Overwrote),
                Some("dst")
            )
        ),
        Err(RollbackRefusal::NotRollbackable(NotRollbackableReason::Overwrote))
    );
    // A required volume isn't connected ⇒ typed unavailable naming the volume.
    assert_eq!(
        check_rollbackable(&vm, &base(RollbackState::Rollbackable, None, Some("backup"))),
        Err(RollbackRefusal::VolumeUnavailable {
            volume_id: "backup".into()
        })
    );
}

// ── The entry point: gate, set rolling_back, reset on spawn failure ───────────

/// A helper that seeds a minimal rollbackable copy op (one dst file) + registers
/// its volumes, returning the rig.
async fn rig_with_rollbackable_op(op_id: &str) -> Rig {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/f.txt", b"x").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst);
    rig.seed(
        op_id,
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "src", "/f.txt", "dst", "/f.txt", 1)],
    );
    rig
}

#[tokio::test]
async fn double_rollback_is_refused_with_already_rolling_back() {
    let rig = rig_with_rollbackable_op("op").await;
    // First rollback: gate passes, the op is set rolling_back.
    let first = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| Ok(()));
    assert!(first.is_ok(), "first rollback accepted");
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::RollingBack);
    // Second rollback while still rolling_back ⇒ typed refusal, and the spawn
    // closure is never reached.
    let second = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| {
        panic!("spawn must not run for an already-rolling-back op")
    });
    assert_eq!(second.unwrap_err(), RollbackRefusal::AlreadyRollingBack);
}

#[tokio::test]
async fn synchronous_spawn_failure_resets_to_rollbackable_and_a_retry_is_accepted() {
    let rig = rig_with_rollbackable_op("op").await;
    // The spawn fails synchronously (a volume dropped between the gate and spawn).
    let failed = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| {
        Err(RollbackRefusal::VolumeUnavailable {
            volume_id: "dst".into(),
        })
    });
    assert_eq!(
        failed.unwrap_err(),
        RollbackRefusal::VolumeUnavailable {
            volume_id: "dst".into()
        }
    );
    // NOT wedged: the op was reset to rollbackable, so an immediate retry is taken.
    assert_eq!(
        rig.read_op("op").rollback_state,
        RollbackState::Rollbackable,
        "a failed spawn must not leave the op stuck rolling_back"
    );
    let retry = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| Ok(()));
    assert!(retry.is_ok(), "the retry is accepted after the reset");
}

#[tokio::test]
async fn entry_refuses_unknown_and_not_rollbackable_and_disconnected() {
    let rig = Rig::new();
    rig.register("v", Arc::new(InMemoryVolume::new("V")));
    // Unknown op.
    assert_eq!(
        rollback_operation(&rig.vm, &rig.writer, "nope", |_| Ok(())).unwrap_err(),
        RollbackRefusal::UnknownOperation
    );
    // A delete is never rollbackable — refused with the stored reason.
    rig.seed(
        "del",
        OpKind::Delete,
        "v",
        None,
        RollbackState::NotRollbackable,
        vec![file_unit(0, "v", "/gone.txt", "v", "/gone.txt", 1)],
    );
    // (Delete finalizes not_rollbackable via the pipeline; seed sets the state, but
    // the reason column is nulled by seed, so the gate reports a default reason.)
    assert!(matches!(
        rollback_operation(&rig.vm, &rig.writer, "del", |_| Ok(())).unwrap_err(),
        RollbackRefusal::NotRollbackable(_)
    ));
    // A rollbackable op whose volume isn't registered ⇒ unavailable.
    rig.seed(
        "x",
        OpKind::Copy,
        "gonevol",
        Some("gonevol"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "gonevol", "/f", "gonevol", "/f", 1)],
    );
    assert_eq!(
        rollback_operation(&rig.vm, &rig.writer, "x", |_| Ok(())).unwrap_err(),
        RollbackRefusal::VolumeUnavailable {
            volume_id: "gonevol".into()
        }
    );
}

// ── Startup reconcile ────────────────────────────────────────────────────────

/// Seed an op left `rolling_back`, plus an optional unfinalized inverse op with
/// the given per-item outcomes, and run the reconcile.
fn seed_rolling_back(rig: &Rig, op_id: &str, inverse: Option<(&str, &[ItemOutcome])>) {
    rig.writer
        .open_operation(OpenOperation {
            op_id: op_id.to_string(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("src".into()),
            dest_volume_id: Some("dst".into()),
            item_count: 1,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Done,
        })
        .expect("open orig");
    rig.writer
        .set_rollback_state(op_id, RollbackState::RollingBack, None)
        .expect("set rolling_back");
    if let Some((inv_id, outcomes)) = inverse {
        rig.writer
            .open_operation(OpenOperation {
                op_id: inv_id.to_string(),
                kind: OpKind::Delete,
                initiator: Initiator::User,
                source_volume_id: Some("dst".into()),
                dest_volume_id: None,
                item_count: outcomes.len() as u64,
                started_at: 2,
                rolls_back_op_id: Some(op_id.to_string()),
                execution_status: ExecutionStatus::Running,
            })
            .expect("open inverse");
        let items: Vec<_> = outcomes
            .iter()
            .enumerate()
            .map(|(i, &outcome)| JournalItem {
                seq: i as i64,
                entry_type: EntryType::File,
                row_role: RowRole::RollbackUnit,
                source_volume_id: "dst".into(),
                source_dir: "/".into(),
                source_name: format!("f{i}"),
                dest_volume_id: None,
                dest_dir: None,
                dest_name: None,
                size: Some(1),
                mtime: Some(MT as i64),
                outcome,
                overwrote: false,
            })
            .collect();
        rig.writer.record_items(inv_id, items).expect("record inverse items");
        // Deliberately NOT finalized — it crashed mid-stream.
    }
    rig.writer.flush_blocking().expect("flush");
}

#[tokio::test]
async fn reconcile_resolves_from_inverse_outcomes() {
    // (i) inverse reversed something ⇒ partially_rolled_back.
    {
        let rig = Rig::new();
        seed_rolling_back(&rig, "op", Some(("inv", &[ItemOutcome::Done, ItemOutcome::Skipped])));
        reconcile_rolling_back_on_open(&rig.writer);
        assert_eq!(rig.read_op("op").rollback_state, RollbackState::PartiallyRolledBack);
    }
    // (i') inverse reversed nothing (all skipped) ⇒ back to rollbackable.
    {
        let rig = Rig::new();
        seed_rolling_back(&rig, "op", Some(("inv", &[ItemOutcome::Skipped])));
        reconcile_rolling_back_on_open(&rig.writer);
        assert_eq!(rig.read_op("op").rollback_state, RollbackState::Rollbackable);
    }
}

#[tokio::test]
async fn reconcile_with_no_inverse_row_returns_to_rollbackable_and_a_reissue_resumes() {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/f.txt", b"x").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    // Crashed AFTER setting rolling_back but before the inverse op opened, and the
    // op still has real rollback_unit rows to reverse.
    rig.writer
        .open_operation(OpenOperation {
            op_id: "op".into(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("src".into()),
            dest_volume_id: Some("dst".into()),
            item_count: 1,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Done,
        })
        .expect("open");
    rig.writer
        .record_items("op", vec![file_unit(0, "src", "/f.txt", "dst", "/f.txt", 1)])
        .expect("record");
    rig.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("set");
    rig.writer.flush_blocking().expect("flush");

    // No inverse op ⇒ reconcile returns it straight to rollbackable.
    reconcile_rolling_back_on_open(&rig.writer);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::Rollbackable);

    // A re-issued rollback now resumes and finishes idempotently.
    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(
        !exists(&dst, "/f.txt").await,
        "the re-issued rollback reversed the copy"
    );
}

#[tokio::test]
async fn retention_cannot_prune_a_rollbacks_source_mid_stream() {
    use crate::operation_log::writer::PruneRequest;
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/one.txt", b"1").await;
    put(&dst, "/two.txt", b"22").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/one.txt", "dst", "/one.txt", 1),
            file_unit(1, "src", "/two.txt", "dst", "/two.txt", 2),
        ],
    );
    // The op is mid-rollback...
    rig.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("set");
    // ...and a retention pass runs that WOULD prune it by age (ended_at 200 is well
    // before the cutoff). It must skip a `rolling_back` op so the source rows a live
    // rollback is streaming can't vanish out from under it (Finding 6).
    rig.writer
        .prune(PruneRequest {
            max_age_secs: Some(0),
            max_size_bytes: None,
            now_secs: 1_000_000,
            vacuum: true,
        })
        .expect("prune");
    rig.writer.flush_blocking().expect("flush");

    // The op and every item survived the prune.
    let conn = open_read_connection(rig.writer.db_path()).expect("conn");
    assert!(
        read_operation(&conn, "op").expect("read").is_some(),
        "the rolling_back op is not pruned"
    );
    assert_eq!(
        read_operation_items(&conn, "op", 100).expect("items").len(),
        2,
        "its source rows survive"
    );
    drop(conn);

    // Reset to rollbackable (as the reconcile would) and run the rollback to
    // completion: it restores every item because the rows were never pruned.
    rig.writer
        .set_rollback_state("op", RollbackState::Rollbackable, None)
        .expect("reset");
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 2, "both source rows were still there to reverse");
    assert!(!exists(&dst, "/one.txt").await);
    assert!(!exists(&dst, "/two.txt").await);
}

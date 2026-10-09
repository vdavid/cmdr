//! Fresh compression lifecycle, output, progress, metadata, and journaling.

use super::compress::compress_start;
use super::test_support::*;
use crate::file_system::write_operations::WriteOperationPhase;

/// End-to-end: compress two local files into a new ZIP and read both entries back.
#[tokio::test]
async fn compress_start_packs_local_files_into_a_new_zip() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");

    // A local source volume holding two files at its root. Repetition makes the
    // source-byte workload deliberately asymmetric with the much smaller zip,
    // so the test catches a progress implementation that measures output bytes
    // while claiming to measure compression work.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    let one = vec![b'a'; 96 * 1024];
    let two = vec![b'b'; 32 * 1024];
    let source_bytes = (one.len() + two.len()) as u64;
    std::fs::write(src_root.join("one.txt"), &one).expect("w1");
    std::fs::write(src_root.join("two.txt"), &two).expect("w2");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root.clone()));

    // The target ZIP doesn't exist yet; it must remain absent until publication.
    let dest = tmp.path().join("bundle.zip");
    assert!(!dest.exists(), "the target must not exist before compress");

    let events = PhaseGateSink::new(WriteOperationPhase::FinishingCompression);
    let start = compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start compress");
    assert_eq!(start.operation_type, WriteOperationType::Compress);

    wait_until_async(Duration::from_secs(5), "finishing-compression phase gate", || {
        events.entered()
    })
    .await;
    assert!(
        !dest.exists(),
        "fresh compression must stay on its staged name until ZIP close and validation finish"
    );
    assert!(
        events.inner.complete.lock_ignore_poison().is_empty(),
        "completion must wait for ZIP finalization"
    );
    {
        let gated = events.inner.progress.lock_ignore_poison();
        let finishing = gated
            .last()
            .expect("finishing-compression progress before gate release");
        assert_eq!(finishing.phase, WriteOperationPhase::FinishingCompression);
        assert_eq!((finishing.files_done, finishing.files_total), (0, 0));
        assert_eq!((finishing.bytes_done, finishing.bytes_total), (0, 0));
    }
    events.release();

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.inner.complete.lock_ignore_poison().is_empty()
    })
    .await;

    // Both files landed at the archive root with their exact bytes.
    assert_eq!(read_entry(&dest, "one.txt").as_deref(), Some(one.as_slice()));
    assert_eq!(read_entry(&dest, "two.txt").as_deref(), Some(two.as_slice()));

    let zip_bytes = std::fs::metadata(&dest).expect("zip metadata").len();
    assert_ne!(source_bytes, zip_bytes, "the two progress axes must be distinguishable");
    let progress = events.inner.progress.lock_ignore_poison();
    let compressing = progress
        .iter()
        .filter(|event| event.phase == WriteOperationPhase::Compressing)
        .collect::<Vec<_>>();
    assert!(
        !compressing.is_empty(),
        "compression must have a determinate source-byte phase"
    );
    assert!(
        compressing
            .iter()
            .all(|event| event.operation_type == WriteOperationType::Compress),
        "every compression tick must carry the distinct operation identity"
    );
    assert_eq!(compressing.last().expect("compressing tick").bytes_total, source_bytes);
    assert!(
        compressing
            .iter()
            .all(|event| event.bytes_done < event.bytes_total || event.bytes_total == 0),
        "the determinate compression phase must not display a false 100% while ZIP finalization remains"
    );
    let finishing = progress
        .iter()
        .find(|event| event.phase == WriteOperationPhase::FinishingCompression)
        .expect("finishing-compression phase");
    assert_eq!((finishing.files_done, finishing.files_total), (0, 0));
    assert_eq!((finishing.bytes_done, finishing.bytes_total), (0, 0));
    assert!(
        progress.iter().all(|event| event.step.is_none()),
        "a direct compress is one step and never numbers its phases"
    );
    let first_compressing = progress
        .iter()
        .position(|event| event.phase == WriteOperationPhase::Compressing)
        .expect("a compressing tick");
    let scanning = progress[..first_compressing]
        .iter()
        .rfind(|event| event.phase == WriteOperationPhase::Scanning)
        .expect("planning reports its walk before compression starts");
    assert_eq!(
        (scanning.files_done, scanning.bytes_done),
        (2, source_bytes),
        "the last scanning tick names everything the walk found"
    );
    assert_eq!(
        (scanning.files_total, scanning.bytes_total),
        (0, 0),
        "a scan has no denominator"
    );

    let complete = events.inner.complete.lock_ignore_poison();
    assert!(complete[0].files_skipped == 0, "a clean compress skips nothing");
    assert_eq!(complete[0].operation_type, WriteOperationType::Compress);
}

#[tokio::test]
async fn a_name_taken_after_start_is_not_replaced_or_reported_complete() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("new.txt"), b"new archive bytes").expect("write source");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root));
    let dest = tmp.path().join("bundle.zip");
    let raced_bytes = b"a file created after the compression was approved";
    let events = PhaseGateSink::new(WriteOperationPhase::FinishingCompression);

    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start net-new compress");
    wait_until_async(Duration::from_secs(5), "finishing-compression phase gate", || {
        events.entered()
    })
    .await;

    std::fs::write(&dest, raced_bytes).expect("race a file into the free target name");
    events.release();
    wait_until_async(Duration::from_secs(5), "the late-conflict terminal event", || {
        !events.inner.errors.lock_ignore_poison().is_empty()
    })
    .await;

    assert_eq!(std::fs::read(&dest).expect("read raced destination"), raced_bytes);
    assert!(events.inner.complete.lock_ignore_poison().is_empty());
}

#[tokio::test]
async fn stop_policy_prompts_for_duplicate_source_names_and_honors_overwrite() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("first")).expect("mkdir first");
    std::fs::create_dir_all(src_root.join("second")).expect("mkdir second");
    std::fs::write(src_root.join("first/report.txt"), b"first").expect("write first");
    std::fs::write(src_root.join("second/report.txt"), b"second").expect("write second");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root));
    let dest = tmp.path().join("bundle.zip");
    let events = Arc::new(CollectorEventSink::new());

    let started = compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("first/report.txt"), PathBuf::from("second/report.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Stop,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start duplicate-name compress");

    wait_until_async(Duration::from_secs(5), "duplicate-name conflict", || {
        !events.conflicts.lock_ignore_poison().is_empty() || !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(events.errors.lock_ignore_poison().is_empty());
    let clash = events.conflicts.lock_ignore_poison()[0].conflict_id;
    resolve_write_conflict(&started.operation_id, clash, ConflictResolution::Overwrite, false);
    wait_until_async(Duration::from_secs(5), "duplicate-name compress completion", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;

    assert_eq!(read_entry(&dest, "report.txt").as_deref(), Some(b"second".as_slice()));
}

#[tokio::test]
async fn an_existing_local_archive_stays_byte_exact_until_publication() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("new.txt"), b"new bytes").expect("write source");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root));
    let dest = tmp.path().join("bundle.zip");
    let original = zip_bytes(&[("old.txt", b"original bytes")]);
    std::fs::write(&dest, &original).expect("write original archive");

    let events = PhaseGateSink::new(WriteOperationPhase::FinishingCompression);
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start replacement compress");

    wait_until_async(Duration::from_secs(5), "finishing-compression phase gate", || {
        events.entered()
    })
    .await;
    assert_eq!(std::fs::read(&dest).expect("read original"), original);
    events.release();
    wait_until_async(Duration::from_secs(5), "write-complete", || {
        !events.inner.complete.lock_ignore_poison().is_empty()
    })
    .await;
    assert_eq!(read_entry(&dest, "new.txt").as_deref(), Some(b"new bytes".as_slice()));
    assert!(read_entry(&dest, "old.txt").is_none());
}

#[tokio::test]
async fn local_alias_and_destination_inside_source_are_refused_before_registration() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir_all(&source_dir).expect("mkdir source");
    let source_archive = source_dir.join("source.zip");
    std::fs::write(&source_archive, b"source").expect("write source");
    let alias = tmp.path().join("alias.zip");
    std::fs::hard_link(&source_archive, &alias).expect("hard link alias");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", "/"));
    let events = Arc::new(CollectorEventSink::new());

    let alias_result = compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        Arc::clone(&source_volume),
        vec![source_archive],
        alias,
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await;
    assert!(matches!(
        alias_result,
        Err(WriteOperationError::DestinationInsideSource { .. })
    ));

    let inside_result = compress_start(
        events,
        source_volume,
        vec![source_dir.clone()],
        source_dir.join("nested.zip"),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await;
    assert!(matches!(
        inside_result,
        Err(WriteOperationError::DestinationInsideSource { .. })
    ));
}

/// ❗ DATA SAFETY: a compress onto a destination Cmdr may not write must refuse
/// BEFORE it touches that file, leaving the original byte-for-byte intact.
///
/// The assertion that matters is byte-level equality of the original file, not
/// merely that the call returned an error.
#[tokio::test]
async fn compress_refuses_a_read_only_destination_without_touching_its_bytes() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("one.txt"), b"first").expect("w1");

    // Every destination whose FORMAT Cmdr refuses to write: a document container
    // and the two read-only archive formats. A `.docx` is the one a user is most
    // likely to have sitting in the folder they're compressing into.
    for name in ["report.docx", "sheet.xlsx", "lib.jar", "old.tar", "old.7z"] {
        let dest = tmp.path().join(name);
        let original: &[u8] = b"the user's real document, which must survive a refused compress";
        std::fs::write(&dest, original).expect("pre-write the victim file");

        let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root.clone()));
        let events = Arc::new(CollectorEventSink::new());
        let result = compress_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            source_volume,
            vec![PathBuf::from("one.txt")],
            dest.clone(),
            unique_lane_id(),
            ConflictResolution::Overwrite,
            0,
            None,
            None,
            crate::operation_log::types::Initiator::User,
        )
        .await;

        // Refused, and typed — the same `ReadOnlyDevice` every other archive-edit
        // route answers with.
        let err = result
            .err()
            .unwrap_or_else(|| panic!("{name}: compress must be refused"));
        assert!(
            matches!(err, WriteOperationError::ReadOnlyDevice { .. }),
            "{name}: expected a typed read-only refusal, got {err:?}"
        );

        // ❗ The file is untouched. Byte-for-byte, not "still exists" — the bug
        // left a valid 22-byte zip behind, which would pass a weaker check.
        let after = std::fs::read(&dest).unwrap_or_else(|e| panic!("{name}: the original must still be readable: {e}"));
        assert_eq!(
            after, original,
            "{name}: a refused compress must not have altered the destination's bytes"
        );
    }
}

/// The compress driver supplies the `archive_edit` subkind + net-new flag the
/// journal can't derive from `WriteOperationType` (both compress and zip-inner
/// edit cross IPC as `ArchiveEdit`). A net-new compress finalizes with
/// `subkind = compress`, records the created archive as its single `rollback_unit`
/// item, and computes `rollbackable` eligibility — proving the subkind came from
/// the driver, not the op type.
#[tokio::test]
async fn compress_journals_subkind_and_net_new_from_the_driver() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::operation_log::TestJournalGuard;
    use crate::operation_log::capture::WriterJournal;
    use crate::operation_log::store::{
        open_read_connection, operation_log_db_path, read_operation, read_operation_items,
    };
    use crate::operation_log::types::{ArchiveSubkind, ExecutionStatus, Initiator, OpKind, RollbackState, RowRole};
    use crate::operation_log::writer::OperationLogWriter;

    let jdir = tempfile::tempdir().expect("jdir");
    let jdb = operation_log_db_path(jdir.path());
    // Serializes journal-slot tests under plain `cargo test`; clears on drop.
    let _journal = TestJournalGuard::install(Arc::new(WriterJournal::new(
        OperationLogWriter::spawn(&jdb).expect("spawn writer"),
    )));

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("one.txt"), b"first").expect("w1");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root.clone()));

    let dest = tmp.path().join("bundle.zip");
    assert!(!dest.exists(), "the target must be net-new");

    let events = Arc::new(CollectorEventSink::new());
    let start = compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        Initiator::User,
    )
    .await
    .expect("start compress");

    // Poll the journal itself so this cell asserts the persisted status and not
    // only the terminal event's in-memory observation.
    let op_id = start.operation_id.clone();
    let jdb_poll = jdb.clone();
    wait_until_async(
        Duration::from_secs(5),
        "the operation to reach Done in the journal",
        || {
            open_read_connection(&jdb_poll)
                .ok()
                .and_then(|c| read_operation(&c, &op_id).ok().flatten())
                .is_some_and(|r| r.execution_status == ExecutionStatus::Done)
        },
    )
    .await;

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, &start.operation_id).expect("read").expect("row");
    assert_eq!(row.kind, OpKind::ArchiveEdit);
    assert_eq!(
        row.archive_subkind,
        Some(ArchiveSubkind::Compress),
        "the subkind is the driver's, not derived from WriteOperationType"
    );
    assert_eq!(
        row.rollback_state,
        RollbackState::Rollbackable,
        "a net-new compress is rollbackable (delete the created archive)"
    );
    let items = read_operation_items(&conn, &start.operation_id, 100).expect("items");
    assert_eq!(
        items.len(),
        1,
        "the created archive is the single rollback_unit, got {items:?}"
    );
    assert_eq!(items[0].row_role, RowRole::RollbackUnit);
    assert_eq!(items[0].source_name, "bundle.zip");
}

/// The header says where the sources came FROM and how much was packed. Pre-fix
/// it named the archive's volume on both sides and counted zero items and bytes,
/// so a Mac-to-phone compress read as a phone-to-phone one that packed nothing.
#[tokio::test]
async fn compress_journals_the_source_volume_and_what_it_packed() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::operation_log::TestJournalGuard;
    use crate::operation_log::capture::WriterJournal;
    use crate::operation_log::store::{open_read_connection, operation_log_db_path, read_operation};
    use crate::operation_log::types::{ExecutionStatus, Initiator};
    use crate::operation_log::writer::OperationLogWriter;

    let jdir = tempfile::tempdir().expect("jdir");
    let jdb = operation_log_db_path(jdir.path());
    let _journal = TestJournalGuard::install(Arc::new(WriterJournal::new(
        OperationLogWriter::spawn(&jdb).expect("spawn writer"),
    )));

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("album")).expect("mkdir src");
    std::fs::write(src_root.join("one.txt"), b"first").expect("w1");
    std::fs::write(src_root.join("album/two.txt"), b"second!").expect("w2");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root.clone()));
    let source_id = unique_lane_id();
    get_volume_manager().register(&source_id, Arc::clone(&source_volume));
    let parent_id = unique_lane_id();

    let start = compress_start(
        Arc::new(CollectorEventSink::new()) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt"), PathBuf::from("album")],
        tmp.path().join("bundle.zip"),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        Initiator::User,
    )
    .await
    .expect("start compress");

    let op_id = start.operation_id.clone();
    let jdb_poll = jdb.clone();
    wait_until_async(
        Duration::from_secs(5),
        "the operation to reach Done in the journal",
        || {
            open_read_connection(&jdb_poll)
                .ok()
                .and_then(|c| read_operation(&c, &op_id).ok().flatten())
                .is_some_and(|r| r.execution_status == ExecutionStatus::Done)
        },
    )
    .await;
    get_volume_manager().unregister(&source_id);

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, &start.operation_id).expect("read").expect("row");
    assert_eq!(row.source_volume_id.as_deref(), Some(source_id.as_str()));
    assert_eq!(row.dest_volume_id.as_deref(), Some(parent_id.as_str()));
    // `one.txt`, the `album` folder, and `album/two.txt`.
    assert_eq!((row.item_count, row.items_done), (3, 3));
    assert_eq!(row.bytes_total, 12, "the uncompressed source bytes");
}

/// Compressing a whole folder packs its subtree under the folder name — the common
/// "compress the directory under the cursor" case.
#[tokio::test]
async fn compress_start_packs_a_directory_subtree() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("project/sub")).expect("mkdir");
    std::fs::write(src_root.join("project/readme.txt"), b"top").expect("w1");
    std::fs::write(src_root.join("project/sub/deep.txt"), b"nested").expect("w2");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root.clone()));

    let dest = tmp.path().join("project.zip");
    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("project")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start compress");

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
    assert_eq!(
        read_entry(&dest, "project/readme.txt").as_deref(),
        Some(b"top".as_slice())
    );
    assert_eq!(
        read_entry(&dest, "project/sub/deep.txt").as_deref(),
        Some(b"nested".as_slice())
    );
}

// ---- Compression level ---------------------------------------------------------

/// A genuinely compressible payload: varied enough that a higher deflate effort
/// can find better matches, so level 1 and level 9 produce different stored sizes.
fn compressible_payload() -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..4_000u32 {
        out.extend_from_slice(
            format!("line {i}: the quick brown fox jumps over the lazy dog #{}\n", i % 97).as_bytes(),
        );
    }
    out
}

/// The stored (compressed) size of one entry in a zip.
fn entry_compressed_size(path: &Path, name: &str) -> u64 {
    let file = std::fs::File::open(path).expect("open result zip");
    let mut archive = ZipArchive::new(file).expect("parse result zip");
    archive.by_name(name).expect("entry present").compressed_size()
}

/// Compresses one file holding `payload` at `level` into a fresh zip and returns
/// its path. The entry lands at the archive root as `data.txt`.
async fn compress_payload_at(dir: &Path, tag: &str, level: Option<i64>, payload: &[u8]) -> PathBuf {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let src_root = dir.join(format!("src-{tag}"));
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("data.txt"), payload).expect("write payload");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root.clone()));

    let dest = dir.join(format!("out-{tag}.zip"));
    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("data.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        level,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start compress");
    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
    // Every level must round-trip to the exact original bytes.
    assert_eq!(
        read_entry(&dest, "data.txt").as_deref(),
        Some(payload),
        "level {level:?} must round-trip the payload"
    );
    dest
}

/// Level 9 (Smaller) produces a strictly smaller entry than level 1 (Faster) on
/// this compressible payload, and both round-trip. Strict `<` (not `<=`) is the
/// real proof the level threads end-to-end: if it didn't, every level would fall
/// back to the crate default and the two sizes would match. Deterministic for a
/// fixed payload, so `<` is not flaky.
#[tokio::test]
async fn compress_level_9_is_smaller_than_level_1() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let payload = compressible_payload();

    let at_1 = compress_payload_at(tmp.path(), "l1", Some(1), &payload).await;
    let at_9 = compress_payload_at(tmp.path(), "l9", Some(9), &payload).await;

    let size_1 = entry_compressed_size(&at_1, "data.txt");
    let size_9 = entry_compressed_size(&at_9, "data.txt");
    assert!(
        size_9 < size_1,
        "level 9 ({size_9} bytes) must beat level 1 ({size_1} bytes) — else the level isn't threading"
    );
}

/// The default (`None`) is byte-stable with today's behavior: it maps to the crate
/// default level 6, so an unset level and an explicit 6 produce the same stored size.
#[tokio::test]
async fn compress_default_none_matches_explicit_level_6() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let payload = compressible_payload();

    let default = compress_payload_at(tmp.path(), "def", None, &payload).await;
    let explicit_6 = compress_payload_at(tmp.path(), "six", Some(6), &payload).await;

    assert_eq!(
        entry_compressed_size(&default, "data.txt"),
        entry_compressed_size(&explicit_6, "data.txt"),
        "the default (None) must equal explicit level 6 — no byte-level behavior change"
    );
}

/// An out-of-range level must CLAMP into 1..=9, not fail the edit: the zip crate
/// hard-errors on a raw out-of-range deflate level at the first entry write. A
/// wild value (a bad config, an MCP `set_setting` with a huge number) still
/// produces a valid archive, sized as the nearest in-range level.
#[tokio::test]
async fn compress_out_of_range_level_clamps_instead_of_failing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let payload = compressible_payload();

    let too_low = compress_payload_at(tmp.path(), "low", Some(0), &payload).await;
    let too_high = compress_payload_at(tmp.path(), "high", Some(42), &payload).await;
    let at_1 = compress_payload_at(tmp.path(), "one", Some(1), &payload).await;
    let at_9 = compress_payload_at(tmp.path(), "nine", Some(9), &payload).await;

    // Clamped to the boundary levels: 0 -> 1, 42 -> 9.
    assert_eq!(
        entry_compressed_size(&too_low, "data.txt"),
        entry_compressed_size(&at_1, "data.txt"),
        "level 0 must clamp to level 1"
    );
    assert_eq!(
        entry_compressed_size(&too_high, "data.txt"),
        entry_compressed_size(&at_9, "data.txt"),
        "level 42 must clamp to level 9"
    );
}

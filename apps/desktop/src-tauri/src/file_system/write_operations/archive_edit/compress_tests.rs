//! Compress: seed a valid empty zip at a target, then pack sources into it. The
//! seed is the net-new surface (the copy-into machinery is covered by
//! `copy_into_tests`), so these tests pin the seed's validity, that it's
//! load-bearing (a compress against a 0-byte target would fail), and one end-to-end
//! compress of local files.

use super::compress::{compress_start, seed_empty_zip};
use super::test_support::*;
use crate::file_system::volume::backends::archive::bytes_start_with_zip_signature;

/// The seed writes a valid empty archive: the reader opens it with zero entries,
/// and its first bytes pass the shared zip-signature check. Pre-fix (a 0-byte
/// stub) `ZipArchive::new` errors here, so this test is RED until the real
/// 22-byte EOCD seed lands.
#[test]
fn seed_empty_zip_writes_a_valid_zero_entry_archive() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("out.zip");

    seed_empty_zip(&target).expect("seed");

    // The reader opens it as a real, empty archive.
    let file = std::fs::File::open(&target).expect("open seeded zip");
    let archive = ZipArchive::new(file).expect("a 0-byte file would fail here; the seed must be a valid empty zip");
    assert_eq!(archive.len(), 0, "a fresh seed holds zero entries");

    // The magic check the routing/boundary layer uses accepts the seed.
    let mut header = [0u8; 4];
    let mut f = std::fs::File::open(&target).expect("reopen");
    f.read_exact(&mut header).expect("read header");
    assert!(
        bytes_start_with_zip_signature(&header),
        "the seed's first bytes must pass the shared zip-signature check"
    );
}

/// The seed replaces any existing file at the target atomically (temp+rename), so
/// an overwrite lands a clean empty archive, never a torn one, and leaves no temp.
#[test]
fn seed_empty_zip_overwrites_an_existing_file_atomically() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("out.zip");
    std::fs::write(&target, b"stale bytes that are not a zip").expect("pre-write");

    seed_empty_zip(&target).expect("seed over existing");

    let file = std::fs::File::open(&target).expect("open");
    let archive = ZipArchive::new(file).expect("the overwrite must leave a valid empty zip");
    assert_eq!(archive.len(), 0);
    // No temp sibling left behind after the atomic rename.
    let leftover = std::fs::read_dir(tmp.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().contains(".cmdr-tmp-"));
    assert!(!leftover, "the temp sibling must be gone after the rename");
}

/// End-to-end: compress two local files into a new zip at a target and read both
/// entries back. This is where the seed being load-bearing shows — with the 0-byte
/// stub, `route_archive_copy_into`'s in-closure `ZipArchive::new` fails and neither
/// entry lands. (Manually verified RED by temporarily skipping the seed.)
#[tokio::test]
async fn compress_start_packs_local_files_into_a_new_zip() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");

    // A local source volume holding two files at its root.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("one.txt"), b"first").expect("w1");
    std::fs::write(src_root.join("two.txt"), b"second").expect("w2");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

    // The target zip doesn't exist yet — compress must seed it, then add the files.
    let dest = tmp.path().join("bundle.zip");
    assert!(!dest.exists(), "the target must not exist before compress");

    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start compress");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "compress should complete"
    );

    // Both files landed at the archive root with their exact bytes.
    assert_eq!(read_entry(&dest, "one.txt").as_deref(), Some(b"first".as_slice()));
    assert_eq!(read_entry(&dest, "two.txt").as_deref(), Some(b"second".as_slice()));

    let complete = events.complete.lock_ignore_poison();
    assert!(complete[0].files_skipped == 0, "a clean compress skips nothing");
}

/// The compress driver supplies the `archive_edit` subkind + net-new flag the
/// journal can't derive from `WriteOperationType` (both compress and zip-inner
/// edit cross IPC as `ArchiveEdit`, Finding 3). A net-new compress finalizes with
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
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

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
        Initiator::User,
    )
    .await
    .expect("start compress");

    // Poll the journal itself (not just the complete event, which fires before
    // finalize) until the op is durably finalized.
    let op_id = start.operation_id.clone();
    let jdb_poll = jdb.clone();
    assert!(
        wait_until(|| {
            open_read_connection(&jdb_poll)
                .ok()
                .and_then(|c| read_operation(&c, &op_id).ok().flatten())
                .is_some_and(|r| r.execution_status == ExecutionStatus::Done)
        })
        .await,
        "the compress op should finalize in the journal"
    );

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
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

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
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start compress");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
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
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

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
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start compress");
    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "compress at level {level:?} should complete"
    );
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

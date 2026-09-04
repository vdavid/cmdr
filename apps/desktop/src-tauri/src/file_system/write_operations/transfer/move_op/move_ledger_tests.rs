//! What the local move's rollback ledger records about each rename it made.
//!
//! A `#[path]` child of `move_op`, like the other suites here, so it can reach
//! `MoveTransaction` and `merge_move_directory` directly.

use super::test_support::make_state;
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::ledger::WrittenIdentity;

/// Every child a directory-into-directory merge renames is recorded with the
/// identity it landed with.
///
/// This is the case the in-memory ledger exists for: the journal marks a merge
/// `not_rollbackable`, so a cancelled folder-into-folder move has nothing but
/// this ledger to reverse from. Recording the children as unidentifiable would
/// make a later "leave anything I can't verify" rule reverse nothing at all.
#[test]
fn a_merged_move_records_every_child_it_renamed() {
    let tmp = tempfile::tempdir().unwrap();
    let source_dir = tmp.path().join("from/album");
    let dest_dir = tmp.path().join("into/album");
    fs::create_dir_all(source_dir.join("nested")).unwrap();
    fs::create_dir_all(source_dir.join("fresh")).unwrap();
    // `nested` exists at the destination, so the merge recurses into it and
    // renames the child; `fresh` doesn't, so the whole directory is renamed.
    fs::create_dir_all(dest_dir.join("nested")).unwrap();
    fs::write(source_dir.join("one.txt"), b"first").unwrap();
    fs::write(source_dir.join("nested/two.txt"), vec![9u8; 300]).unwrap();
    fs::write(source_dir.join("fresh/three.txt"), b"third").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();
    let mut move_tx = MoveTransaction::new();
    let mut files_skipped = 0usize;

    merge_move_directory(
        &source_dir,
        &dest_dir,
        &config,
        &*events,
        "op-merge-ledger",
        &state,
        &mut ApplyToAll::default(),
        &mut move_tx,
        &mut files_skipped,
        &mut None,
    )
    .expect("the merge should land");

    assert_eq!(files_skipped, 0);
    assert_eq!(move_tx.renames.len(), 3, "one entry per child renamed");
    for item in &move_tx.renames {
        assert_eq!(
            item.landed.identity,
            WrittenIdentity::at_local_path(&item.landed.path),
            "{} has to be recorded as the entry that landed there",
            item.landed.path.display()
        );
        assert_ne!(
            item.landed.identity,
            WrittenIdentity::Unverifiable,
            "{} was recorded with nothing to recognize it by",
            item.landed.path.display()
        );
    }
    let sizes: HashSet<Option<u64>> = move_tx
        .renames
        .iter()
        .map(|item| item.landed.identity.recorded_size())
        .collect();
    assert!(
        sizes.contains(&Some(300)),
        "a child renamed inside the recursion carries its own size, got {sizes:?}"
    );
    let renamed_dir = move_tx
        .renames
        .iter()
        .find(|item| item.landed.path.ends_with("fresh"))
        .expect("the directory with no destination counterpart is renamed whole");
    assert!(
        matches!(renamed_dir.landed.identity, WrittenIdentity::LocalDir { .. }),
        "a directory records its node and no size, got {:?}",
        renamed_dir.landed.identity
    );
}

/// A top-level same-FS rename is recorded with the identity the item carried
/// across it. A rename preserves the node id, so the snapshot taken before it
/// describes the landed item exactly.
#[test]
fn a_renamed_top_level_item_is_recorded_with_the_identity_it_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    let source = src_dir.join("clip.mov");
    fs::write(&source, vec![1u8; 4096]).unwrap();
    let before = WrittenIdentity::at_local_path(&source);

    let mut move_tx = MoveTransaction::new();
    move_tx.record(
        source.clone(),
        WrittenFile::local_stat(dst_dir.join("clip.mov"), fs::symlink_metadata(&source).ok().as_ref()),
    );
    fs::rename(&source, dst_dir.join("clip.mov")).unwrap();

    let landed = &move_tx.renames[0].landed;
    assert_eq!(
        landed.identity, before,
        "the pre-rename snapshot IS the landed identity"
    );
    assert_eq!(landed.identity, WrittenIdentity::at_local_path(&landed.path));
}

/// The move ledger is a stack: reversing drains it, so it never claims a rename
/// it has already put back.
#[test]
fn reversing_a_move_drains_its_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    let source = src_dir.join("notes.txt");
    fs::write(&source, b"body").unwrap();
    let landed = dst_dir.join("notes.txt");
    fs::rename(&source, &landed).unwrap();

    let mut move_tx = MoveTransaction::new();
    move_tx.record(source.clone(), WrittenFile::local(landed.clone()));

    move_tx.rollback();

    assert!(source.exists(), "the item came back");
    assert!(!landed.exists());
    assert!(move_tx.renames.is_empty(), "a reversed rename is no longer claimed");
}

/// What a same-FS move flushes for durability: the directories whose entries it
/// changed, one fsync each, however many files crossed them.
///
/// A `rename(2)` moves a directory ENTRY. The file's data blocks and inode are
/// untouched, so syncing the file itself buys nothing — and on macOS that sync
/// is `fcntl(F_FULLFSYNC)`, a device-level barrier, per file, dirty or not.
/// Both sides count: the entry left one directory and arrived in another.
#[test]
fn a_move_flushes_one_entry_per_directory_it_touched() {
    let tmp = tempfile::tempdir().unwrap();
    let photos = tmp.path().join("photos");
    let scans = tmp.path().join("scans");
    let archive = tmp.path().join("archive");
    for dir in [&photos, &scans, &archive] {
        fs::create_dir_all(dir).unwrap();
    }

    let mut move_tx = MoveTransaction::new();
    for i in 0..50 {
        let source = if i % 2 == 0 { &photos } else { &scans }.join(format!("{i}.jpg"));
        fs::write(&source, b"pixels").unwrap();
        let landed = archive.join(format!("{i}.jpg"));
        fs::rename(&source, &landed).unwrap();
        move_tx.record(source, WrittenFile::local(landed));
    }

    let touched = move_tx.touched_directories();

    assert_eq!(
        touched.iter().collect::<HashSet<_>>(),
        [&photos, &scans, &archive].into_iter().collect(),
        "both directories the entries left and the one they arrived in"
    );
    assert_eq!(
        touched.len(),
        3,
        "one entry per DIRECTORY — 50 renames through 3 directories cost 3 fsyncs, not 50"
    );
    for dir in &touched {
        assert!(
            dir.is_dir(),
            "{} has to be a directory, not a moved file",
            dir.display()
        );
    }
}

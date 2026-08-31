//! What the in-flight ledger vocabulary records, and what it refuses to record.

use super::*;

/// A local file is snapshotted with both halves of its identity, and the size is
/// the file's own.
#[test]
fn a_local_file_records_its_size_and_node() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("photo.raw");
    std::fs::write(&file, b"0123456789").unwrap();

    let recorded = WrittenFile::local(file.clone());

    assert_eq!(recorded.path, file);
    let WrittenIdentity::LocalFile { size, node } = recorded.identity else {
        panic!("a local file must record a local-file identity, got {recorded:?}");
    };
    assert_eq!(size, 10);
    assert_eq!(node, WrittenIdentity::at_local_path(&file).node().unwrap());
}

/// The point of carrying the node id: a file swapped for a DIFFERENT file of
/// exactly the same size is a different entry, which size alone can't see. This
/// is what an editor's write-temp-then-rename does to a destination.
#[test]
fn a_same_size_replacement_is_a_different_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notes.txt");
    std::fs::write(&file, b"before").unwrap();
    let recorded = WrittenFile::local(file.clone());

    // Replace it the way an editor does: write a temp, rename it over the top.
    let temp = tmp.path().join("notes.txt.tmp");
    std::fs::write(&temp, b"after!").unwrap();
    std::fs::rename(&temp, &file).unwrap();

    let now = WrittenIdentity::at_local_path(&file);
    assert_eq!(
        now.recorded_size(),
        recorded.identity.recorded_size(),
        "the replacement is the same size, so size alone can't tell them apart"
    );
    assert_ne!(now, recorded.identity, "the node id has to tell them apart");
}

/// A directory records its node and no size: a directory's reported size shifts
/// as children come and go, so recording it would report a folder as changed for
/// nothing more than a file dropped into it.
#[test]
fn a_local_directory_records_its_node_and_no_size() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("album");
    std::fs::create_dir(&dir).unwrap();

    let recorded = WrittenFile::local(dir);

    assert!(
        matches!(recorded.identity, WrittenIdentity::LocalDir { .. }),
        "got {:?}",
        recorded.identity
    );
    assert_eq!(recorded.identity.recorded_size(), None);
}

/// A symlink describes ITSELF. `symlink_metadata` reads the link's own node, so a
/// copied link is recognizable even when it dangles.
#[test]
fn a_symlink_records_the_link_not_its_target() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target.bin");
    std::fs::write(&target, vec![0u8; 4096]).unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let recorded = WrittenFile::local(link.clone());

    assert_ne!(
        recorded.identity,
        WrittenFile::local(target).identity,
        "the link must not borrow its target's identity"
    );
    assert_eq!(recorded.identity, WrittenIdentity::at_local_path(&link));
}

/// A path that can't be stat'd is unprovable, and says so.
#[test]
fn a_missing_path_is_unverifiable() {
    let tmp = tempfile::tempdir().unwrap();

    let recorded = WrittenFile::local(tmp.path().join("never-existed"));

    assert_eq!(recorded.identity, WrittenIdentity::Unverifiable);
    assert_eq!(recorded.identity.recorded_size(), None);
}

/// A volume file carries the size the write reported, and no node: no backend
/// but the local filesystem has one.
#[test]
fn a_volume_file_records_its_size_only() {
    let recorded = WrittenFile::volume(PathBuf::from("/share/clip.mov"), 1_048_576);

    assert_eq!(recorded.identity, WrittenIdentity::VolumeFile { size: 1_048_576 });
    assert_eq!(recorded.identity.recorded_size(), Some(1_048_576));
}

/// A partial is its own case, not an unverifiable file. A reversal removes one on
/// sight, so the two must never collapse into each other.
#[test]
fn a_partial_is_not_an_unverifiable_file() {
    let recorded = WrittenFile::own_partial(PathBuf::from("/share/half.mov"));

    assert_eq!(recorded.identity, WrittenIdentity::OwnPartial);
    assert_ne!(recorded.identity, WrittenIdentity::Unverifiable);
    assert_eq!(
        recorded.identity.recorded_size(),
        None,
        "a partial has no size by construction"
    );
}

// ── The local copy's ledger (`CopyTransaction`) ──

#[test]
fn copy_transaction_rollback_deletes_files_and_dirs_in_reverse() {
    // Build a real on-disk transaction: nested dirs + a file, then roll
    // back. Both removals must happen. The rollback must walk dirs in
    // reverse-creation order so the leaf is removed before its parent.
    let tmp = tempfile::tempdir().unwrap();
    let outer = tmp.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir(&outer).unwrap();
    std::fs::create_dir(&inner).unwrap();
    let file = inner.join("data.bin");
    std::fs::write(&file, b"hello").unwrap();

    let mut tx = CopyTransaction::new();
    tx.record_dir(outer.clone());
    tx.record_dir(inner.clone());
    tx.record_file(WrittenFile::local(file.clone()));

    tx.rollback();

    assert!(!file.exists(), "file must be removed on rollback");
    assert!(!inner.exists(), "inner dir must be removed (leaf-first)");
    assert!(!outer.exists(), "outer dir must be removed");
}

#[test]
fn copy_transaction_commit_prevents_drop_rollback() {
    // Kills: replace CopyTransaction::commit with (), and the `!self.committed`
    // guard in Drop. After commit(), files must survive Drop.
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("kept.txt");
    std::fs::write(&file, b"persist").unwrap();

    {
        let mut tx = CopyTransaction::new();
        tx.record_file(WrittenFile::local(file.clone()));
        tx.commit();
    } // Drop runs here.

    assert!(file.exists(), "commit() must prevent the Drop-based rollback");
}

#[test]
fn copy_transaction_drop_rolls_back_when_not_committed() {
    // Kills: replace <impl Drop>::drop with (), and `delete !` in Drop.
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("ephemeral.txt");
    std::fs::write(&file, b"will be gone").unwrap();

    {
        let mut tx = CopyTransaction::new();
        tx.record_file(WrittenFile::local(file.clone()));
        // No commit; Drop should roll back.
    }

    assert!(!file.exists(), "Drop-on-uncommitted must remove recorded files");
}

#[test]
fn copy_transaction_record_methods_push_in_order() {
    // Kills: replace record_file/record_dir with ().
    let mut tx = CopyTransaction::new();
    tx.record_file(WrittenFile::local(PathBuf::from("/a")));
    tx.record_file(WrittenFile::local(PathBuf::from("/b")));
    tx.record_dir(PathBuf::from("/d1"));
    assert_eq!(tx.created_file_paths(), vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    assert_eq!(tx.created_dirs, vec![PathBuf::from("/d1")]);
    tx.commit(); // suppress Drop rollback (paths don't exist anyway, but be tidy)
}

#[test]
fn copy_transaction_pops_the_newest_file_first() {
    // The ledger is a stack: a reversal takes the newest entry off as it undoes
    // it, so what's left is exactly what this operation still has on disk.
    let mut tx = CopyTransaction::new();
    tx.record_file(WrittenFile::local(PathBuf::from("/a")));
    tx.record_file(WrittenFile::local(PathBuf::from("/b")));

    assert_eq!(tx.pop_file().map(|f| f.path), Some(PathBuf::from("/b")));
    assert_eq!(tx.created_file_paths(), vec![PathBuf::from("/a")]);
    assert_eq!(tx.pop_file().map(|f| f.path), Some(PathBuf::from("/a")));
    assert!(tx.pop_file().is_none(), "an emptied ledger claims nothing");
    tx.commit();
}

impl WrittenIdentity {
    /// The node id, for tests that compare two snapshots of the same entry.
    fn node(&self) -> Option<NodeId> {
        match self {
            Self::LocalFile { node, .. } | Self::LocalDir { node } => Some(*node),
            Self::VolumeFile { .. } | Self::OwnPartial | Self::Unverifiable => None,
        }
    }
}

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

impl WrittenIdentity {
    /// The node id, for tests that compare two snapshots of the same entry.
    fn node(&self) -> Option<NodeId> {
        match self {
            Self::LocalFile { node, .. } | Self::LocalDir { node } => Some(*node),
            Self::VolumeFile { .. } | Self::OwnPartial | Self::Unverifiable => None,
        }
    }
}

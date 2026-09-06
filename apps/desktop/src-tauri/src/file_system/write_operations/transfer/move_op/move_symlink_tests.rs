//! A symlink is an opaque leaf to both local move engines.
//!
//! A `#[path]` child of `move_op`, like the other suites here. Every cell here
//! defends one rule: a move renames a link, it never walks through one. The
//! danger is a link whose TARGET is a directory meeting a real directory of the
//! same name at the destination — follow it and the merge starts renaming
//! entries out of a folder the user never selected, and the destination side is
//! just as bad (a rename through a dest-side link lands the user's files
//! somewhere they'll never look for them).
//!
//! So a link meeting a directory is a TYPE MISMATCH, resolved by the normal
//! conflict machinery: prompt on Stop, skip on Skip, land aside on Rename.
//! `transfer/DETAILS.md` § "Symlinks are opaque to a move".

use super::test_support::{run_cross_fs_move, run_same_fs_move};
use super::*;
use crate::file_system::write_operations::types::ConflictResolution;
use std::os::unix::fs::symlink;

/// The fixture every cell here starts from: a target directory holding one file
/// that lives OUTSIDE the moved selection, so anything that reaches it is
/// visibly reaching past what the user asked for.
struct LinkFixture {
    tmp: tempfile::TempDir,
    /// The directory the link points at, outside `src` and outside `dst`.
    target: PathBuf,
    src_root: PathBuf,
    dst_root: PathBuf,
}

fn link_fixture() -> LinkFixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("outside").join("target");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&target).unwrap();
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    fs::write(target.join("inside.txt"), b"OUTSIDE THE SELECTION").unwrap();
    LinkFixture {
        tmp,
        target,
        src_root,
        dst_root,
    }
}

impl LinkFixture {
    /// The link's target still holds everything it started with.
    fn assert_target_untouched(&self, cell: &str) {
        assert!(
            self.target.join("inside.txt").exists(),
            "{cell}: a move must never reach through a symlink into its target"
        );
        assert_eq!(
            fs::read(self.target.join("inside.txt")).unwrap(),
            b"OUTSIDE THE SELECTION",
            "{cell}: the target's file keeps its bytes"
        );
    }
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

// ============================================================================
// A link-to-directory child meeting a real directory
// ============================================================================

/// Skip: the link stays in the source, both directories are untouched.
#[test]
fn a_dir_link_child_meeting_a_real_dir_is_skipped_not_merged() {
    let fx = link_fixture();
    let src_dir = fx.src_root.join("d");
    fs::create_dir_all(&src_dir).unwrap();
    symlink(&fx.target, src_dir.join("link")).unwrap();
    fs::write(src_dir.join("plain.txt"), b"PLAIN").unwrap();

    let dst_dir = fx.dst_root.join("d");
    fs::create_dir_all(dst_dir.join("link")).unwrap();

    let result = run_same_fs_move(
        std::slice::from_ref(&src_dir),
        &fx.dst_root,
        ConflictResolution::Skip,
        "symlink-child-skip",
    );
    assert!(result.is_ok(), "{:?}", result.err());

    fx.assert_target_untouched("skip");
    assert!(
        is_link(&src_dir.join("link")),
        "skip: the skipped link stays in the source, still a link"
    );
    assert_eq!(
        fs::read_dir(dst_dir.join("link")).unwrap().count(),
        0,
        "skip: the destination's real directory stays empty"
    );
    assert!(
        dst_dir.join("plain.txt").exists(),
        "skip: the non-clashing sibling still moves"
    );
}

/// Rename: the link lands beside the destination directory as a link, and the
/// directory it clashed with is left alone.
#[test]
fn a_dir_link_child_meeting_a_real_dir_lands_aside_on_rename() {
    let fx = link_fixture();
    let src_dir = fx.src_root.join("d");
    fs::create_dir_all(&src_dir).unwrap();
    symlink(&fx.target, src_dir.join("link")).unwrap();

    let dst_dir = fx.dst_root.join("d");
    fs::create_dir_all(dst_dir.join("link")).unwrap();

    let result = run_same_fs_move(
        std::slice::from_ref(&src_dir),
        &fx.dst_root,
        ConflictResolution::Rename,
        "symlink-child-rename",
    );
    assert!(result.is_ok(), "{:?}", result.err());

    fx.assert_target_untouched("rename");
    let aside = dst_dir.join("link (1)");
    assert!(is_link(&aside), "rename: the incoming link lands aside AS a link");
    assert_eq!(
        fs::read_link(&aside).unwrap(),
        fx.target,
        "rename: and still points where it always did"
    );
    assert_eq!(
        fs::read_dir(dst_dir.join("link")).unwrap().count(),
        0,
        "rename: the destination's real directory is kept, untouched"
    );
}

/// No clash at all: the link rides across as a link, never dereferenced.
#[test]
fn a_dir_link_child_with_no_clash_moves_as_a_link() {
    let fx = link_fixture();
    let src_dir = fx.src_root.join("d");
    fs::create_dir_all(&src_dir).unwrap();
    symlink(&fx.target, src_dir.join("link")).unwrap();
    fs::create_dir_all(fx.dst_root.join("d")).unwrap();

    let result = run_same_fs_move(
        std::slice::from_ref(&src_dir),
        &fx.dst_root,
        ConflictResolution::Skip,
        "symlink-child-no-clash",
    );
    assert!(result.is_ok(), "{:?}", result.err());

    fx.assert_target_untouched("no clash");
    let landed = fx.dst_root.join("d").join("link");
    assert!(
        is_link(&landed),
        "the link moves as a link, not as a copy of its target"
    );
    assert_eq!(fs::read_link(&landed).unwrap(), fx.target);
}

/// The other direction: a real directory in the source meeting a LINK at the
/// destination. Merging would rename the source's files through the link, into
/// a folder the user never chose as the destination.
#[test]
fn a_real_dir_child_meeting_a_dir_link_at_the_destination_is_not_merged() {
    let fx = link_fixture();
    let src_dir = fx.src_root.join("d");
    fs::create_dir_all(src_dir.join("sub")).unwrap();
    fs::write(src_dir.join("sub").join("mine.txt"), b"MINE").unwrap();

    let dst_dir = fx.dst_root.join("d");
    fs::create_dir_all(&dst_dir).unwrap();
    symlink(&fx.target, dst_dir.join("sub")).unwrap();

    let result = run_same_fs_move(
        std::slice::from_ref(&src_dir),
        &fx.dst_root,
        ConflictResolution::Skip,
        "symlink-dest-skip",
    );
    assert!(result.is_ok(), "{:?}", result.err());

    assert!(
        !fx.target.join("mine.txt").exists(),
        "dest link: nothing may land inside the link's target"
    );
    fx.assert_target_untouched("dest link");
    assert!(
        src_dir.join("sub").join("mine.txt").exists(),
        "dest link: the skipped source directory keeps its file"
    );
    assert!(is_link(&dst_dir.join("sub")), "dest link: the link itself is untouched");
}

/// A link whose target is a FILE, meeting a real directory. The link is a leaf
/// either way, so this is the same type mismatch and the same refusal.
#[test]
fn a_file_link_child_meeting_a_real_dir_is_skipped() {
    let fx = link_fixture();
    let file_target = fx.tmp.path().join("outside").join("note.txt");
    fs::write(&file_target, b"A REAL FILE").unwrap();

    let src_dir = fx.src_root.join("d");
    fs::create_dir_all(&src_dir).unwrap();
    symlink(&file_target, src_dir.join("link")).unwrap();

    let dst_dir = fx.dst_root.join("d");
    fs::create_dir_all(dst_dir.join("link")).unwrap();
    fs::write(dst_dir.join("link").join("theirs.txt"), b"THEIRS").unwrap();

    let result = run_same_fs_move(
        std::slice::from_ref(&src_dir),
        &fx.dst_root,
        ConflictResolution::Skip,
        "file-link-child-skip",
    );
    assert!(result.is_ok(), "{:?}", result.err());

    assert!(is_link(&src_dir.join("link")), "the skipped link stays in the source");
    assert_eq!(fs::read(&file_target).unwrap(), b"A REAL FILE");
    assert_eq!(
        fs::read(dst_dir.join("link").join("theirs.txt")).unwrap(),
        b"THEIRS",
        "the destination directory keeps its own content"
    );
}

// ============================================================================
// The same rule at the top level, on both engines
// ============================================================================

/// A top-level source that IS a link, meeting a real directory of that name at
/// the destination. `same_fs.rs`'s own dir-vs-dir test, not the merge helper's.
#[test]
fn a_top_level_dir_link_meeting_a_real_dir_is_skipped_not_merged() {
    let fx = link_fixture();
    let source = fx.src_root.join("link");
    symlink(&fx.target, &source).unwrap();
    fs::create_dir_all(fx.dst_root.join("link")).unwrap();

    let result = run_same_fs_move(
        std::slice::from_ref(&source),
        &fx.dst_root,
        ConflictResolution::Skip,
        "symlink-top-level-skip",
    );
    assert!(result.is_ok(), "{:?}", result.err());

    fx.assert_target_untouched("top level");
    assert!(is_link(&source), "the skipped link stays where it is");
    assert_eq!(
        fs::read_dir(fx.dst_root.join("link")).unwrap().count(),
        0,
        "the destination's real directory stays empty"
    );
}

/// The cross-filesystem engine's phase 3 makes the same dir-vs-dir decision
/// against the STAGED tree, and a staged link is still a link.
#[test]
fn a_staged_dir_link_meeting_a_real_dir_is_skipped_not_merged() {
    let fx = link_fixture();
    let source = fx.src_root.join("link");
    symlink(&fx.target, &source).unwrap();
    fs::create_dir_all(fx.dst_root.join("link")).unwrap();

    let result = run_cross_fs_move(
        std::slice::from_ref(&source),
        &fx.dst_root,
        ConflictResolution::Skip,
        "symlink-staged-skip",
    );
    assert!(result.is_ok(), "{:?}", result.err());

    fx.assert_target_untouched("staged");
    assert!(is_link(&source), "the skipped link keeps its original");
    assert_eq!(
        fs::read_dir(fx.dst_root.join("link")).unwrap().count(),
        0,
        "the destination's real directory stays empty"
    );
}

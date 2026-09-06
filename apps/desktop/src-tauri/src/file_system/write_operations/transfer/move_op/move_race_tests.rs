//! What a move does with a destination that appeared after it looked.
//!
//! A `#[path]` child of `move_op`. Each engine decides "is this name free?" with
//! a stat and lands with a rename, and those are two operations: a file another
//! process creates in between is in the way by the time the syscall runs. A
//! plain POSIX `rename` would replace it silently — no prompt, no conflict, no
//! backup — so the landing has to refuse instead.
//!
//! The window is one syscall wide and can't be produced from a test without a
//! second process, so these drive `rename_onto_free_name` directly: an entry
//! sitting at the destination is exactly the state the engine is in once its
//! check has passed and the racing writer has won. `move_reversal_tests.rs`
//! pins the reversal's own rename the same way, for the same reason.

use super::*;

/// Something took the name between the check and the rename. The move refuses,
/// and the file that got there first keeps its bytes.
#[test]
fn a_landing_on_a_free_name_refuses_a_file_that_appeared_in_the_window() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("notes.txt");
    let dest = tmp.path().join("landed.txt");
    fs::write(&source, b"the file being moved").unwrap();
    // Stands in for the file created after the engine found this name free.
    fs::write(&dest, b"somebody else got here first").unwrap();

    let result = rename_onto_free_name(&source, &dest);

    assert!(
        matches!(&result, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists),
        "an occupied destination refuses, whenever it got occupied: {result:?}"
    );
    assert_eq!(
        fs::read(&dest).unwrap(),
        b"somebody else got here first",
        "the file that got there first keeps its bytes"
    );
    assert!(source.exists(), "and the move's own item stays put, not half-gone");
}

/// The name really is free, so the item lands.
#[test]
fn a_landing_on_a_free_name_lands_when_the_name_really_is_free() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("notes.txt");
    let dest = tmp.path().join("landed.txt");
    fs::write(&source, b"the file being moved").unwrap();

    rename_onto_free_name(&source, &dest).expect("a free name accepts the item");

    assert_eq!(fs::read(&dest).unwrap(), b"the file being moved");
    assert!(!source.exists());
}

/// A dangling symlink is an entry too, and one `Path::exists()` reads as absent.
/// It's in the way of the rename all the same, so it gets the same refusal
/// rather than being silently replaced.
#[cfg(unix)]
#[test]
fn a_landing_on_a_free_name_refuses_a_dangling_symlink_too() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("notes.txt");
    let dest = tmp.path().join("landed.txt");
    fs::write(&source, b"the file being moved").unwrap();
    std::os::unix::fs::symlink(tmp.path().join("nothing-here"), &dest).unwrap();

    let result = rename_onto_free_name(&source, &dest);

    assert!(
        matches!(&result, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists),
        "a dangling link occupies the name: {result:?}"
    );
    assert!(
        fs::symlink_metadata(&dest).unwrap().file_type().is_symlink(),
        "the link is still there"
    );
}

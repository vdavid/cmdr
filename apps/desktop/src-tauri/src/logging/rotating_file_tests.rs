use std::fs;

use super::*;
use crate::test_support::TestDir;

fn read(dir: &TestDir, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok()
}

/// One record the way fern delivers it: the bytes, then exactly one flush.
fn record(log: &mut RotatingFile, line: &str) {
    log.write_all(line.as_bytes()).unwrap();
    log.flush().unwrap();
}

/// Regression anchor for CRASH-EC4QS (0.51.0): `file-rotate` lists the rotated siblings once, at
/// startup, and asserted `!new_path.exists()` when another process had created one since. The
/// panic killed whichever thread was logging (that time, the drive index writer).
#[test]
fn a_sibling_created_behind_its_back_is_rotated_over_instead_of_panicking() {
    let dir = TestDir::new("rotating-file-foreign-sibling");
    let mut log = RotatingFile::new(dir.join("cmdr.log"), 2, 10);
    record(&mut log, "first\n");
    fs::write(dir.join("cmdr.log.1"), "foreign 1\n").unwrap();
    fs::write(dir.join("cmdr.log.2"), "foreign 2\n").unwrap();

    record(&mut log, "second line\n");

    assert_eq!(read(&dir, "cmdr.log.1").as_deref(), Some("first\nsecond line\n"));
    assert_eq!(read(&dir, "cmdr.log.2").as_deref(), Some("foreign 1\n"));
    assert_eq!(read(&dir, "cmdr.log.3"), None, "only two rotated files are kept");
}

#[test]
fn below_the_limit_everything_stays_in_the_live_file() {
    let dir = TestDir::new("rotating-file-below-limit");
    let mut log = RotatingFile::new(dir.join("cmdr.log"), 2, 100);
    record(&mut log, "one\n");
    record(&mut log, "two\n");

    assert_eq!(read(&dir, "cmdr.log").as_deref(), Some("one\ntwo\n"));
    assert_eq!(read(&dir, "cmdr.log.1"), None);
}

#[test]
fn a_record_is_never_split_and_older_files_shift_down() {
    let dir = TestDir::new("rotating-file-shift");
    let mut log = RotatingFile::new(dir.join("cmdr.log"), 2, 8);
    // A record lands whole, in several writes, then rotates on its flush.
    log.write_all(b"2026 ").unwrap();
    log.write_all(b"aaaaaa\n").unwrap();
    log.flush().unwrap();
    record(&mut log, "bbbbbbbbb\n");
    record(&mut log, "ccccccccc\n");
    record(&mut log, "tail\n");

    assert_eq!(read(&dir, "cmdr.log").as_deref(), Some("tail\n"));
    assert_eq!(read(&dir, "cmdr.log.1").as_deref(), Some("ccccccccc\n"));
    assert_eq!(read(&dir, "cmdr.log.2").as_deref(), Some("bbbbbbbbb\n"));
    assert_eq!(read(&dir, "cmdr.log.3"), None, "the oldest file was dropped");
}

#[test]
fn an_existing_live_file_counts_toward_the_limit() {
    let dir = TestDir::new("rotating-file-resume");
    fs::write(dir.join("cmdr.log"), "from the last session\n").unwrap();
    let mut log = RotatingFile::new(dir.join("cmdr.log"), 1, 25);

    record(&mut log, "new\n");

    assert_eq!(
        read(&dir, "cmdr.log.1").as_deref(),
        Some("from the last session\nnew\n")
    );
}

/// `eager_prune(dir, 0)` deletes every file, the live one included, when the user turns log
/// storage off. Writes keep going to the open (now unlinked) file, and the next rotation finds no
/// `cmdr.log` to move rather than failing.
#[test]
fn a_live_file_deleted_underneath_rotates_without_an_error() {
    let dir = TestDir::new("rotating-file-deleted");
    let mut log = RotatingFile::new(dir.join("cmdr.log"), 1, 10);
    record(&mut log, "kept\n");
    fs::remove_file(dir.join("cmdr.log")).unwrap();

    record(&mut log, "over the limit\n");
    record(&mut log, "fresh\n");

    assert_eq!(read(&dir, "cmdr.log.1"), None);
    assert_eq!(read(&dir, "cmdr.log").as_deref(), Some("fresh\n"));
}

/// A directory that refuses the rename reports it once, then keeps logging, and only tries again
/// after another `max_bytes`.
#[cfg(unix)]
#[test]
fn a_refused_rotation_is_an_error_not_a_panic_and_logging_continues() {
    use std::os::unix::fs::PermissionsExt as _;
    // SAFETY: `geteuid` takes no arguments, can't fail, and only reads the process's credentials.
    if unsafe { libc::geteuid() } == 0 {
        return; // root ignores the read-only directory, so there's nothing to refuse
    }
    let dir = TestDir::new("rotating-file-refused");
    let mut log = RotatingFile::new(dir.join("cmdr.log"), 1, 20);
    record(&mut log, "before\n");
    fs::set_permissions(&*dir, fs::Permissions::from_mode(0o555)).unwrap();

    log.write_all(b"over the limit\n").unwrap();
    let refused = log.flush();
    log.write_all(b"still\n").unwrap();
    let next = log.flush();
    fs::set_permissions(&*dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(refused.is_err(), "the refused rename comes back as an error");
    assert!(next.is_ok(), "no retry until another max_bytes was written");
    assert_eq!(
        read(&dir, "cmdr.log").as_deref(),
        Some("before\nover the limit\nstill\n")
    );
}

/// Regression anchor for #812: two Cmdr processes logged into one directory, each with its own
/// byte counter. When B rotated, A kept appending to the renamed file (now `cmdr.log.1`), and when
/// A reached its own limit it renamed B's live `cmdr.log` away and shifted A's own file down. A
/// writer whose open file isn't the one at `cmdr.log` any more must reopen `cmdr.log` instead.
#[test]
fn a_writer_rotated_away_by_another_reopens_the_live_file_instead_of_renaming_it() {
    let dir = TestDir::new("rotating-file-two-writers");
    let mut a = RotatingFile::new(dir.join("cmdr.log"), 2, 20);
    let mut b = RotatingFile::new(dir.join("cmdr.log"), 2, 20);
    record(&mut a, "a1 aaaaaaaaaaaaa\n"); // 17 bytes, below the limit
    record(&mut b, "b1 bbbbbbbbbbbbb\n"); // B counts 34 bytes and rotates
    record(&mut b, "b2\n"); // B's fresh cmdr.log

    record(&mut a, "a2 aaaaaa\n"); // A counts 27 bytes, but its file is now cmdr.log.1
    record(&mut a, "a3\n");

    assert_eq!(
        read(&dir, "cmdr.log.1").as_deref(),
        Some("a1 aaaaaaaaaaaaa\nb1 bbbbbbbbbbbbb\na2 aaaaaa\n"),
        "A's file is where B rotated it"
    );
    assert_eq!(
        read(&dir, "cmdr.log").as_deref(),
        Some("b2\na3\n"),
        "A joins B's live file rather than renaming it away"
    );
    assert_eq!(read(&dir, "cmdr.log.2"), None, "nothing was shifted down a second time");
}

/// The same race, caught before B's next write: B rotated, so no `cmdr.log` exists yet. A still
/// must not shift the files B just rotated.
#[test]
fn a_writer_rotated_away_with_no_live_file_yet_renames_nothing() {
    let dir = TestDir::new("rotating-file-two-writers-gone");
    let mut a = RotatingFile::new(dir.join("cmdr.log"), 2, 20);
    let mut b = RotatingFile::new(dir.join("cmdr.log"), 2, 20);
    record(&mut a, "a1 aaaaaaaaaaaaa\n");
    record(&mut b, "b1 bbbbbbbbbbbbb\n"); // B rotates; there's no cmdr.log now

    record(&mut a, "a2 aaaaaa\n");
    record(&mut a, "a3\n");

    assert_eq!(
        read(&dir, "cmdr.log.1").as_deref(),
        Some("a1 aaaaaaaaaaaaa\nb1 bbbbbbbbbbbbb\na2 aaaaaa\n")
    );
    assert_eq!(read(&dir, "cmdr.log").as_deref(), Some("a3\n"));
    assert_eq!(read(&dir, "cmdr.log.2"), None);
}

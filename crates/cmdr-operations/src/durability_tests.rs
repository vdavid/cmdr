//! What the closing flush makes durable, and what it answers when it can't.
//!
//! Driven through `flush_created_destinations` over a real scratch tree, with
//! `test_hook` recording (and, where a test needs it, failing) every directory
//! sync. A move deletes its sources only on `Ok`, so these pin which answers keep
//! a user's originals.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::test_hook;
use super::{FlushFailure, flush_created_destinations, refuses_directory_fsync};
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::WriteOperationType;
use cmdr_fs::testing::TestDir;

/// A landed tree the way a transfer leaves it: `dest/top.txt`, 20 files in
/// `dest/tree/sub/`, and `dest/only/inner/`, an empty directory whose parent
/// `only` holds nothing but it.
struct Landed {
    _dir: TestDir,
    dest: PathBuf,
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

fn landed_tree() -> Landed {
    let dir = TestDir::new("durability");
    let dest = dir.join("dest");
    let tree = dest.join("tree");
    let sub = tree.join("sub");
    let only = dest.join("only");
    let inner = only.join("inner");
    fs::create_dir_all(&sub).expect("create dest/tree/sub");
    fs::create_dir_all(&inner).expect("create dest/only/inner");
    let top = dest.join("top.txt");
    fs::write(&top, b"top").expect("write top.txt");
    let mut files = vec![top];
    for i in 0..20 {
        let leaf = sub.join(format!("leaf-{i}.txt"));
        fs::write(&leaf, b"leaf").expect("write a leaf");
        files.push(leaf);
    }
    Landed {
        _dir: dir,
        dest,
        files,
        dirs: vec![tree, sub, only, inner],
    }
}

fn flush(files: &[PathBuf], dirs: &[PathBuf], already_synced: &HashSet<PathBuf>) -> Result<(), FlushFailure> {
    let events = CollectorEventSink::new();
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(200)));
    flush_created_destinations(
        &events,
        "op-durability",
        WriteOperationType::Copy,
        &state,
        files.len(),
        files.len(),
        0,
        0,
        files,
        dirs,
        already_synced,
    )
}

fn flush_landed(landed: &Landed) -> Result<(), FlushFailure> {
    flush(&landed.files, &landed.dirs, &landed.files.iter().cloned().collect())
}

/// The directories that gained an entry in [`landed_tree`]: the parent of every
/// file and the parent of every created directory.
fn dirs_that_gained_entries(dest: &Path) -> Vec<PathBuf> {
    sorted(vec![
        dest.to_path_buf(),
        dest.join("tree"),
        dest.join("tree").join("sub"),
        dest.join("only"),
    ])
}

fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths
}

fn fail_at(target: PathBuf, errno: i32) -> impl FnMut(&Path) -> Option<io::Result<()>> {
    move |dir| (dir == target).then(|| Err(io::Error::from_raw_os_error(errno)))
}

/// A chunked copy already `sync_data`d every file, but that says nothing about
/// the directory entries naming them. Every directory that gained an entry is
/// fsynced, each exactly once however many files it holds, and an empty leaf
/// directory (`inner`) costs nothing of its own: its parent's fsync covers it.
#[test]
fn every_directory_that_gained_an_entry_is_fsynced_once_even_when_file_data_was_already_synced() {
    let landed = landed_tree();
    let log = test_hook::record(|_| None);

    let result = flush_landed(&landed);

    assert_eq!(result, Ok(()));
    assert_eq!(
        sorted(log.dirs()),
        dirs_that_gained_entries(&landed.dest),
        "one fsync per directory that gained an entry, whatever `already_synced` says"
    );
}

/// A directory fsync that fails for a real reason means the entry may not be on
/// disk, so the flush answers the path and errno a caller needs to keep the
/// sources. It still tries every other directory first: a copy keeps going on a
/// failure and should end as durable as it can.
#[test]
fn a_directory_fsync_failing_with_eio_answers_that_directory_and_errno() {
    let landed = landed_tree();
    let failing = landed.dest.join("tree").join("sub");
    let log = test_hook::record(fail_at(failing.clone(), libc::EIO));

    let result = flush_landed(&landed);

    assert_eq!(
        result,
        Err(FlushFailure {
            path: failing,
            errno: Some(libc::EIO),
        })
    );
    assert_eq!(
        sorted(log.dirs()),
        dirs_that_gained_entries(&landed.dest),
        "the failure doesn't stop the other directories from being flushed"
    );
}

/// Some filesystems don't do directory fsync at all. Failing on that would turn
/// every move there into a failed move with duplicated files, so the whole "not
/// supported" class counts as done (with a `warn`), and the rest of the flush
/// still runs.
#[test]
fn a_directory_that_doesnt_support_fsync_still_answers_ok() {
    for errno in [
        libc::ENOTSUP,
        libc::EOPNOTSUPP,
        libc::EINVAL,
        libc::ENOSYS,
        libc::ENOTTY,
    ] {
        let landed = landed_tree();
        let log = test_hook::record(fail_at(landed.dest.join("tree"), errno));

        let result = flush_landed(&landed);

        assert_eq!(result, Ok(()), "errno {errno} is a refusal, not a failure");
        assert_eq!(
            sorted(log.dirs()),
            dirs_that_gained_entries(&landed.dest),
            "errno {errno}: every directory is still attempted"
        );
    }
}

/// One errno's classification for a failed directory fsync, with its raw value on
/// each platform. The values differ (`ENOSYS` is 78 on macOS and 38 on Linux, and
/// Linux's `ENOTSUP` IS `EOPNOTSUPP`), so a constant that resolves differently
/// can't quietly change which failures keep a move's sources.
struct ErrnoRow {
    name: &'static str,
    errno: i32,
    macos: i32,
    linux: i32,
    unsupported: bool,
}

const ERRNO_TABLE: &[ErrnoRow] = &[
    ErrnoRow {
        name: "ENOTSUP",
        errno: libc::ENOTSUP,
        macos: 45,
        linux: 95,
        unsupported: true,
    },
    ErrnoRow {
        name: "EOPNOTSUPP",
        errno: libc::EOPNOTSUPP,
        macos: 102,
        linux: 95,
        unsupported: true,
    },
    ErrnoRow {
        name: "EINVAL",
        errno: libc::EINVAL,
        macos: 22,
        linux: 22,
        unsupported: true,
    },
    ErrnoRow {
        name: "ENOSYS",
        errno: libc::ENOSYS,
        macos: 78,
        linux: 38,
        unsupported: true,
    },
    ErrnoRow {
        name: "ENOTTY",
        errno: libc::ENOTTY,
        macos: 25,
        linux: 25,
        unsupported: true,
    },
    ErrnoRow {
        name: "EIO",
        errno: libc::EIO,
        macos: 5,
        linux: 5,
        unsupported: false,
    },
    ErrnoRow {
        name: "ENOSPC",
        errno: libc::ENOSPC,
        macos: 28,
        linux: 28,
        unsupported: false,
    },
    ErrnoRow {
        name: "EDQUOT",
        errno: libc::EDQUOT,
        macos: 69,
        linux: 122,
        unsupported: false,
    },
    ErrnoRow {
        name: "EROFS",
        errno: libc::EROFS,
        macos: 30,
        linux: 30,
        unsupported: false,
    },
    ErrnoRow {
        name: "ENXIO",
        errno: libc::ENXIO,
        macos: 6,
        linux: 6,
        unsupported: false,
    },
    ErrnoRow {
        name: "ENODEV",
        errno: libc::ENODEV,
        macos: 19,
        linux: 19,
        unsupported: false,
    },
    ErrnoRow {
        name: "ENOENT",
        errno: libc::ENOENT,
        macos: 2,
        linux: 2,
        unsupported: false,
    },
    ErrnoRow {
        name: "ENOTDIR",
        errno: libc::ENOTDIR,
        macos: 20,
        linux: 20,
        unsupported: false,
    },
    ErrnoRow {
        name: "EACCES",
        errno: libc::EACCES,
        macos: 13,
        linux: 13,
        unsupported: false,
    },
    ErrnoRow {
        name: "EPERM",
        errno: libc::EPERM,
        macos: 1,
        linux: 1,
        unsupported: false,
    },
    ErrnoRow {
        name: "ETIMEDOUT",
        errno: libc::ETIMEDOUT,
        macos: 60,
        linux: 110,
        unsupported: false,
    },
];

/// Only the "this filesystem doesn't do that" class counts as done; every other
/// errno, and an error carrying none, keeps a move's sources.
#[test]
fn each_directory_fsync_errno_classifies_the_same_on_macos_and_linux() {
    let (platform, expected_value): (&str, fn(&ErrnoRow) -> i32) = if cfg!(target_os = "macos") {
        ("macOS", |row| row.macos)
    } else {
        ("Linux", |row| row.linux)
    };
    for row in ERRNO_TABLE {
        assert_eq!(row.errno, expected_value(row), "{}'s value on {platform}", row.name);
        assert_eq!(
            refuses_directory_fsync(&io::Error::from_raw_os_error(row.errno)),
            row.unsupported,
            "{} ({}) should {}",
            row.name,
            row.errno,
            if row.unsupported {
                "count as done"
            } else {
                "keep the sources"
            }
        );
    }
    assert!(
        !refuses_directory_fsync(&io::Error::other("no errno")),
        "an error with no errno keeps the sources"
    );
}

/// A filesystem that doesn't do directory fsync answers that for every directory
/// on it, so one `warn` per destination volume per operation says so; a warn per
/// directory would bury the log under a large tree. `/dev` stands in for a second
/// volume (the hook answers for it, so nothing there is opened).
#[test]
fn a_volume_that_doesnt_support_directory_fsync_warns_once_per_operation() {
    use std::os::unix::fs::MetadataExt;

    let landed = landed_tree();
    let other_volume = PathBuf::from("/dev");
    let device = |path: &Path| fs::metadata(path).expect("stat for its device").dev();
    assert_ne!(
        device(&landed.dest),
        device(&other_volume),
        "premise: `/dev` is a different volume from the scratch dir"
    );
    let mut dirs = landed.dirs.clone();
    dirs.push(other_volume.join("cmdr-placeholder-entry"));
    let synced: HashSet<PathBuf> = landed.files.iter().cloned().collect();
    let log = test_hook::record(|_| Some(Err(io::Error::from_raw_os_error(libc::ENOSYS))));

    assert_eq!(flush(&landed.files, &dirs, &synced), Ok(()));
    assert_eq!(log.dirs().len(), 5, "every directory is still attempted");
    let warned = log.unsupported_warnings();
    assert_eq!(warned.len(), 2, "one warn per volume, not per directory: {warned:?}");

    assert_eq!(flush(&landed.files, &dirs, &synced), Ok(()));
    assert_eq!(
        log.unsupported_warnings().len(),
        4,
        "a new operation warns for each volume again"
    );
}

/// A created file whose data the strategy didn't sync, and that the flush can't
/// reach, has no proof of being on disk: that's an `Err`, never a skipped warn.
#[test]
fn a_created_file_the_flush_cant_reach_answers_its_path() {
    let landed = landed_tree();
    let _log = test_hook::record(|_| None);
    let vanished = landed.dest.join("vanished.txt");
    let mut files = landed.files.clone();
    files.push(vanished.clone());

    let result = flush(&files, &landed.dirs, &HashSet::new());

    assert_eq!(
        result,
        Err(FlushFailure {
            path: vanished,
            errno: Some(libc::ENOENT),
        })
    );
}

/// A symlink has no data of its own to sync, but it's still an entry in its
/// directory, so that directory is flushed.
#[test]
fn a_created_symlink_skips_the_data_sync_but_flushes_its_directory() {
    let dir = TestDir::new("durability-symlink");
    let links = dir.join("links");
    fs::create_dir_all(&links).unwrap();
    let link = links.join("dangling");
    cmdr_fs::testing::symlink(dir.join("nowhere"), &link).unwrap();
    let log = test_hook::record(|_| None);

    let result = flush(std::slice::from_ref(&link), &[], &HashSet::new());

    assert_eq!(result, Ok(()));
    assert_eq!(log.dirs(), vec![links]);
}

/// The characterization table, every errno: a directory fsync the filesystem doesn't
/// do at all (five names, four numbers on Linux) is tolerated, and every other errno,
/// or none, stays a failure that keeps the sources.
#[cfg(unix)]
#[test]
fn every_errno_keeps_its_directory_fsync_answer() {
    let unsupported = [
        libc::ENOTSUP,
        libc::EOPNOTSUPP,
        libc::EINVAL,
        libc::ENOSYS,
        libc::ENOTTY,
    ];
    for code in 0..=4096 {
        assert_eq!(
            refuses_directory_fsync(&io::Error::from_raw_os_error(code)),
            unsupported.contains(&code),
            "errno {code}"
        );
    }
    assert!(!refuses_directory_fsync(&io::Error::other("no OS code")));
}

use super::*;
use crate::testing::TestDir;

/// Barriers a pass issues for `n` notes on one device, per platform: one on
/// macOS, none where the per-file sync already flushed the drive's cache.
fn barriers_for_one_device() -> u64 {
    u64::from(NEEDS_DEVICE_BARRIER)
}

/// Writes and hands `count` small files in `dir` to the drive, noting each.
fn write_and_note(barriers: &DeviceBarriers, dir: &Path, count: usize) {
    for i in 0..count {
        let path = dir.join(format!("f{i:04}"));
        std::fs::write(&path, b"small").unwrap();
        let file = File::open(&path).unwrap();
        sync_to_drive(&file).unwrap();
        barriers.note_synced(&file, &path);
    }
}

/// The issue's whole point (#1145): many files on one device cost ONE barrier,
/// at the close, never one each.
#[test]
fn many_files_on_one_device_cost_one_closing_barrier() {
    let dir = TestDir::new("durable-many");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    write_and_note(&barriers, &dir, 200);
    assert_eq!(
        barriers.barriers_issued(),
        0,
        "no barrier before the interval or the close"
    );

    barriers.flush().unwrap();
    assert_eq!(barriers.barriers_issued(), barriers_for_one_device());

    barriers.flush().unwrap();
    assert_eq!(
        barriers.barriers_issued(),
        barriers_for_one_device(),
        "a pass with nothing owed issues nothing"
    );
}

/// The interval pass: with it always due, every note barriers its device, and
/// the close then owes nothing.
#[test]
fn an_elapsed_interval_barriers_at_the_next_note() {
    let dir = TestDir::new("durable-interval");
    let barriers = DeviceBarriers::with_interval(Duration::ZERO);
    write_and_note(&barriers, &dir, 3);
    assert_eq!(barriers.barriers_issued(), 3 * barriers_for_one_device());

    barriers.flush().unwrap();
    assert_eq!(barriers.barriers_issued(), 3 * barriers_for_one_device());
}

/// A directory noted many times is synced once per pass, and its device owes
/// a barrier even when no file was noted there.
#[test]
fn a_noted_directory_alone_owes_its_device_a_barrier() {
    let dir = TestDir::new("durable-dirs");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    for _ in 0..50 {
        barriers.note_dir(&dir);
    }
    assert_eq!(barriers.owed.lock_ignore_poison().dirs, vec![dir.to_path_buf()]);

    barriers.flush().unwrap();
    assert_eq!(barriers.barriers_issued(), barriers_for_one_device());
    assert!(barriers.owed.lock_ignore_poison().dirs.is_empty());
}

/// A directory that can't be opened is logged and skipped: the directory sync
/// stays best effort, and the pass still answers for the devices.
#[test]
fn a_vanished_noted_directory_doesnt_fail_the_pass() {
    let dir = TestDir::new("durable-gone");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    barriers.note_dir(&dir.join("never-made"));
    assert!(barriers.flush().is_ok());
}

/// The unsupported class is "no barrier to offer"; real I/O failures aren't.
#[test]
fn only_the_unsupported_class_counts_as_no_barrier() {
    // By raw errno, since a class can have no code of its own on a host:
    // Linux's `ENOTSUP` IS `EOPNOTSUPP`, so one of the two classes has none.
    for code in [
        libc::ENOTSUP,
        libc::EOPNOTSUPP,
        libc::EINVAL,
        libc::ENOSYS,
        libc::ENOTTY,
    ] {
        assert!(is_sync_unsupported(&io::Error::from_raw_os_error(code)), "errno {code}");
    }
    for code in [libc::EPERM, libc::EACCES, libc::EIO, libc::ENOSPC] {
        assert!(
            !is_sync_unsupported(&io::Error::from_raw_os_error(code)),
            "errno {code}"
        );
    }
    assert!(!is_sync_unsupported(&io::Error::other("no errno")));
}

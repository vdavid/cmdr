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

/// The time the passes took accrues when they run, and only then: the ETA
/// leaves exactly that time out of what it learns a file costs (#1149).
#[test]
fn time_spent_counts_the_passes_only() {
    let dir = TestDir::new("durable-time-spent");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    write_and_note(&barriers, &dir, 20);
    assert_eq!(barriers.time_spent(), Duration::ZERO, "noting a file runs no pass");

    barriers.flush().unwrap();
    let after_close = barriers.time_spent();
    assert!(after_close > Duration::ZERO, "the closing pass took some time");
    assert!(barriers.time_spent() >= after_close, "monotonic");
}

/// A pass's file syncs are each file's own cost, which the ETA's cost model
/// has to learn, so they stay out of `time_spent`; only the barrier half counts.
#[cfg(target_os = "macos")]
#[test]
fn time_spent_leaves_the_passs_file_syncs_out() {
    const SYNC_TAKES: Duration = Duration::from_millis(300);
    let dir = TestDir::new("durable-time-syncs");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    write_and_note_written(&barriers, &dir, 1);
    barriers.script_syncs(|_| {
        // allowed-test-sleep: the sync's duration IS the subject; the script
        // stands in for a slow `fsync`, and nothing else could take its time.
        std::thread::sleep(SYNC_TAKES);
        None
    });

    barriers.flush().unwrap();
    assert!(barriers.time_spent() < SYNC_TAKES, "{:?}", barriers.time_spent());
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

/// Whether a written file's sync waits for the pass on this host's scratch
/// disk: on macOS (a local volume with a barrier tier), not elsewhere.
const DEFERS: bool = cfg!(target_os = "macos");

/// Writes `count` small files in `dir` and notes each as written, unsynced.
fn write_and_note_written(barriers: &DeviceBarriers, dir: &Path, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|i| {
            let path = dir.join(format!("w{i:04}"));
            std::fs::write(&path, b"small").unwrap();
            let file = File::open(&path).unwrap();
            barriers.note_written(&file, &path).unwrap();
            path
        })
        .collect()
}

/// The point of batching: a new file on a local volume costs no sync of its
/// own. The pass syncs everything written since the last one, then barriers.
#[test]
fn written_files_on_a_local_volume_wait_for_the_pass() {
    let dir = TestDir::new("durable-written");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    let written = write_and_note_written(&barriers, &dir, 50);
    let before_pass = if DEFERS { Vec::new() } else { written.clone() };
    assert_eq!(barriers.synced_paths(), before_pass, "no sync per file before the pass");

    barriers.flush().unwrap();
    let mut synced = barriers.synced_paths();
    synced.sort();
    assert_eq!(synced, written, "the pass syncs each written file once");
    assert_eq!(barriers.barriers_issued(), barriers_for_one_device());
    assert_eq!(barriers.volume_syncs(), 0);
}

/// A landing renames the file after it was noted: the pass syncs it under the
/// name it has now.
#[test]
fn a_renamed_written_file_is_synced_under_its_new_name() {
    let dir = TestDir::new("durable-renamed");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    let temp = write_and_note_written(&barriers, &dir, 1).remove(0);
    let landed = dir.join("landed");
    std::fs::rename(&temp, &landed).unwrap();
    barriers.note_renamed(&temp, &landed);

    barriers.flush().unwrap();
    let expected = if DEFERS { vec![landed] } else { vec![temp] };
    assert_eq!(barriers.synced_paths(), expected);
    assert_eq!(barriers.volume_syncs(), 0);
}

/// A written file the pass can't find by name (renamed by someone who didn't
/// say) is still covered: its volume gets a whole-volume sync instead.
#[cfg(target_os = "macos")]
#[test]
fn a_written_file_gone_from_its_name_falls_back_to_a_volume_sync() {
    let dir = TestDir::new("durable-gone-written");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    let temp = write_and_note_written(&barriers, &dir, 1).remove(0);
    std::fs::rename(&temp, dir.join("moved-silently")).unwrap();

    barriers.flush().unwrap();
    assert_eq!(barriers.volume_syncs(), 1);
    assert_eq!(barriers.barriers_issued(), 1);
}

/// A file a failed landing threw away owes nothing, not even the fallback.
#[test]
fn a_forgotten_written_file_owes_no_sync() {
    let dir = TestDir::new("durable-forgotten");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    let temp = write_and_note_written(&barriers, &dir, 1).remove(0);
    std::fs::remove_file(&temp).unwrap();
    barriers.forget(&temp);

    barriers.flush().unwrap();
    assert_eq!(barriers.volume_syncs(), 0);
}

/// A landing that replaces the user's file syncs its bytes first, at once, so
/// an OS crash can't leave neither version.
#[test]
fn hand_over_syncs_a_written_file_now() {
    let dir = TestDir::new("durable-hand-over");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    let temp = write_and_note_written(&barriers, &dir, 1).remove(0);
    barriers.hand_over(&temp).unwrap();
    let once = vec![temp.clone()];
    let expected = if DEFERS { once.clone() } else { vec![temp.clone(), temp] };
    assert_eq!(barriers.synced_paths(), expected, "synced before the landing rename");

    barriers.flush().unwrap();
    assert_eq!(barriers.synced_paths(), expected, "and not again at the pass");
    assert_eq!(barriers.barriers_issued(), barriers_for_one_device());
}

/// A write problem only the sync reveals surfaces at the pass. An interval pass
/// can't answer anyone, so the failure waits for the next flush, which answers
/// it truthfully; a retry of a failed `fsync` proves nothing, so it isn't one.
#[cfg(target_os = "macos")]
#[test]
fn a_sync_failure_in_an_interval_pass_fails_the_next_flush() {
    let dir = TestDir::new("durable-late-failure");
    let barriers = DeviceBarriers::with_interval(Duration::ZERO);
    barriers.script_syncs(|_| Some(io::Error::from_raw_os_error(libc::EIO)));
    let written = write_and_note_written(&barriers, &dir, 1);
    assert_eq!(barriers.synced_paths(), written, "the interval pass ran at the note");

    let failure = barriers.flush().unwrap_err();
    assert_eq!(failure.path, written[0]);
    assert_eq!(failure.error.raw_os_error(), Some(libc::EIO));
    assert!(barriers.flush().is_ok(), "answered once");
}

/// `hand_over_written` syncs every written file ahead of a rename the tracker
/// won't hear about (a move's staged tree going to its final place), and the
/// closing flush answers a failure there.
#[cfg(target_os = "macos")]
#[test]
fn hand_over_written_syncs_now_and_the_flush_answers_its_failure() {
    let dir = TestDir::new("durable-hand-over-written");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    let written = write_and_note_written(&barriers, &dir, 3);
    let failing = written[1].clone();
    barriers.script_syncs(move |path| (path == failing).then(|| io::Error::from_raw_os_error(libc::EIO)));
    barriers.hand_over_written();
    assert_eq!(barriers.synced_paths().len(), 3);

    let failure = barriers.flush().unwrap_err();
    assert_eq!(failure.path, written[1]);
    assert_eq!(barriers.synced_paths().len(), 3, "nothing left for the pass");
}

/// On a volume that can't defer (every platform but macOS, and a network
/// mount there), a written file's sync runs inline and answers there.
#[cfg(not(target_os = "macos"))]
#[test]
fn an_inline_sync_failure_answers_the_note() {
    let dir = TestDir::new("durable-inline-failure");
    let barriers = DeviceBarriers::with_interval(Duration::MAX);
    barriers.script_syncs(|_| Some(io::Error::from_raw_os_error(libc::EIO)));
    let path = dir.join("f");
    std::fs::write(&path, b"x").unwrap();
    let file = File::open(&path).unwrap();
    assert!(barriers.note_written(&file, &path).is_err());
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

//! Tests for the file viewer IPC commands: the destination guard, the pulling open's
//! progress and stall rules, and the save's.

use super::*;
use crate::file_viewer::RangeEnd;
use std::io::Write as _;

/// Writes a minimal real zip (one stored entry) so the boundary magic check passes.
fn write_zip(path: &std::path::Path) {
    use zip::write::SimpleFileOptions;
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file(
            "inner.txt",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .expect("start");
    writer.write_all(b"hello").expect("write");
    writer.finish().expect("finish");
}

/// Registers an in-memory phone under `id` whose 64 KiB chunks arrive `delay`
/// apart, holding `len` bytes at `big.log`, and returns that file's path.
async fn a_phone_sending_slowly(id: &str, len: usize, delay: Duration) -> String {
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::volume::{InMemoryVolume, Volume as _};

    let root = format!("mtp://{id}/1");
    let volume = InMemoryVolume::new("Phone")
        .with_root(&root)
        .with_read_chunk_delay(delay);
    let path = format!("{root}/big.log");
    volume
        .create_file(std::path::Path::new(&path), &vec![b'x'; len])
        .await
        .expect("seed the file");
    get_volume_manager().register(id, Arc::new(volume));
    path
}

/// A row fetch past its deadline answers `TimedOut`, and the fetch itself is told
/// to stop: its cancel flag flips, so a read stuck on a slow mount quits at its next
/// row instead of holding a blocking thread for nobody.
#[tokio::test]
async fn a_row_fetch_that_times_out_is_told_to_stop() {
    use std::sync::atomic::Ordering;

    let stopped = Arc::new(AtomicBool::new(false));
    let fetch_stopped = Arc::clone(&stopped);
    let outcome = get_lines_within(Duration::from_millis(20), move |cancel| {
        crate::test_support::wait_until(Duration::from_secs(5), "the deadline to flip the fetch's flag", || {
            cancel.load(Ordering::Relaxed)
        });
        fetch_stopped.store(true, Ordering::Relaxed);
        Err(ViewerError::Cancelled)
    })
    .await;

    assert!(matches!(outcome, Err(ViewerError::TimedOut)), "got {outcome:?}");
    crate::test_support::wait_until_async(Duration::from_secs(5), "the orphaned fetch to stop", || {
        stopped.load(Ordering::Relaxed)
    })
    .await;
}

/// A pull that gets no bytes for the stall limit answers the typed
/// `StoppedResponding` without waiting for the source, and the pull, detached
/// rather than dropped, still stops at its chunk boundary and removes its temp.
/// Pre-fix a pull had a flat 30 s budget and no stall rule at all.
#[tokio::test]
async fn a_pull_that_goes_quiet_answers_stopped_responding_and_leaves_no_temp() {
    let extract = crate::test_support::TestDir::new("viewer_pull_stall");
    file_viewer::init_materialize_dir(extract.to_path_buf());
    let path = a_phone_sending_slowly("viewer-stall-cell", 3 * 64 * 1024, Duration::from_millis(900)).await;

    let outcome = open_pulling(
        &Arc::new(PendingOpen::new()),
        path,
        "viewer-stall-cell".to_string(),
        String::new(),
        false,
        Duration::from_millis(250),
        |_| {},
    )
    .await;

    assert!(
        matches!(outcome, Err(ViewerError::StoppedResponding)),
        "a quiet pull answers StoppedResponding, got {outcome:?}"
    );
    crate::test_support::wait_until_async(Duration::from_secs(5), "the quiet pull to remove its temp", || {
        std::fs::read_dir(&extract).is_ok_and(|entries| entries.count() == 0)
    })
    .await;
}

/// While a pull runs, its window hears how far it got, against the size the source
/// declared, and only when there's news. That's what the viewer's bar draws.
#[tokio::test]
async fn a_pull_reports_its_progress_to_the_window_as_bytes_arrive() {
    use crate::ignore_poison::IgnorePoison;

    let extract = crate::test_support::TestDir::new("viewer_pull_emit");
    file_viewer::init_materialize_dir(extract.to_path_buf());
    let len = 6 * 64 * 1024;
    let path = a_phone_sending_slowly("viewer-emit-cell", len, Duration::from_millis(100)).await;
    let heard = Arc::new(std::sync::Mutex::new(Vec::<ViewerPullProgress>::new()));

    let outcome = {
        let heard = Arc::clone(&heard);
        open_pulling(
            &Arc::new(PendingOpen::new()),
            path,
            "viewer-emit-cell".to_string(),
            String::new(),
            false,
            Duration::from_secs(5),
            move |progress| heard.lock_ignore_poison().push(progress),
        )
        .await
    };
    let opened = outcome.expect("the file opens");
    file_viewer::close_session(&opened.session_id).expect("close");

    let heard = heard.lock_ignore_poison();
    assert!(!heard.is_empty(), "the window hears at least one progress report");
    assert!(
        heard
            .iter()
            .all(|p| p.bytes_total == Some(len as u64) && p.bytes_done <= len as u64),
        "every report counts against the declared size, got {heard:?}"
    );
    assert!(
        heard.windows(2).all(|pair| pair[0].bytes_done < pair[1].bytes_done),
        "every report is news, got {heard:?}"
    );
}

/// Rows per fetch (`FETCH_CHUNK` in `range_read.rs`) and the bytes one row costs,
/// including its delimiter.
///
/// ❗ A fetch has to come to SEVERAL flush thresholds, not exactly one. The reader owes
/// the last row's delimiter rather than writing it, so a fetch sized to exactly one
/// `STREAM_CHUNK_BYTES` lands one byte short and flushes nothing until the next fetch is
/// already under way. At 1 KiB a row, a fetch is 4 MiB and the save writes four times
/// inside it, which is what makes "the temp has bytes by now" true where these tests
/// assert it.
const FETCH_ROWS: usize = 4096;
const ROW_BYTES: usize = 1024;

/// A session over a backend that takes `per_chunk` to answer each fetch, serving
/// `chunks` fetches worth of rows, each fetch worth several of the save's writes.
///
/// Returns the session id and the bytes the whole range comes to.
fn a_session_reading_slowly(chunks: usize, per_chunk: Duration) -> (String, usize) {
    const FETCH_LINES: usize = FETCH_ROWS;
    const STRIDE: usize = ROW_BYTES;
    let line_count = FETCH_LINES * chunks;
    let backend = file_viewer::session::ScriptedBackend::new(&"s".repeat(STRIDE - 1), line_count, move |_| {
        // allowed-test-sleep: the slow answer IS the subject. These tests are about
        // what a save that takes a long time honestly is allowed to do, so the delay
        // stands in for a slow disk rather than synchronizing anything.
        std::thread::sleep(per_chunk);
    });
    let session_id = file_viewer::session::test_only_install_session(
        Box::new(backend),
        std::path::PathBuf::from("/scripted/slow.txt"),
    );
    (session_id, line_count * STRIDE - 1)
}

/// The `.cmdr-tmp.<read_id>` file a save writes before its rename, if it's there.
fn save_temp_file(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().contains("cmdr-tmp"))
}

/// Escape has to land while the stall watch is running. The watch itself only ever
/// answers silence, so a save the user stops mid-flight has to come back on its own
/// road: `Cancelled` rather than `TimedOut`, with nothing left behind. The limit here is
/// long enough that the watch can't be what ended it.
#[tokio::test]
async fn escape_during_a_watched_save_stops_it_and_leaves_no_temp() {
    use crate::ignore_poison::IgnorePoison;

    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("out.txt");
    let session_holder: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
    let holder = Arc::clone(&session_holder);
    // What the temp held at the moment of the cancel, so "it cleaned up after itself"
    // can't pass by the save never having written anything.
    let temp_at_cancel = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let observed = Arc::clone(&temp_at_cancel);
    let probe_dir = dir.path().to_path_buf();

    // Cancel on the second fetch, by which point the first fetch's 4 MiB has been
    // written: `viewer_cancel_read` is the road Escape takes, so this is that gesture.
    let backend = file_viewer::session::ScriptedBackend::new(&"s".repeat(ROW_BYTES - 1), FETCH_ROWS * 3, move |call| {
        if call == 1
            && let Some(sid) = holder.lock_ignore_poison().as_ref()
        {
            let written = save_temp_file(&probe_dir)
                .and_then(|p| std::fs::metadata(p).ok())
                .map_or(0, |m| m.len());
            observed.store(written, std::sync::atomic::Ordering::SeqCst);
            file_viewer::cancel_read(sid, 1).expect("the session is open");
        }
    });
    let session_id = file_viewer::session::test_only_install_session(
        Box::new(backend),
        std::path::PathBuf::from("/scripted/cancelled.txt"),
    );
    *session_holder.lock_ignore_poison() = Some(session_id.clone());

    let result = write_range_watched(
        session_id.clone(),
        1,
        RangeEnd::Row { row: 0, offset: 0 },
        RangeEnd::Eof,
        dest.to_string_lossy().into_owned(),
        Duration::from_secs(30),
    )
    .await;

    assert!(
        matches!(result, Err(ViewerError::Cancelled)),
        "Escape must answer Cancelled, not the watch's TimedOut, got {result:?}"
    );
    assert!(
        temp_at_cancel.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "the cancel has to land on a save that had already written, or the cleanup below proves nothing"
    );
    assert!(!dest.exists(), "a cancelled save must not create the destination");
    assert!(
        save_temp_file(dir.path()).is_none(),
        "a cancelled save must take its temp file with it"
    );
    assert_eq!(file_viewer::session::active_read_count(&session_id), 0);

    file_viewer::close_session(&session_id).expect("close");
}

/// A save that keeps writing must run to its end, however long that is. The copy
/// dialog refuses a clipboard copy past 100 MiB and offers "Save as" instead, so a
/// save is exactly the operation with no honest upper bound on its duration; a total
/// deadline would kill the saves the button exists for.
#[tokio::test]
async fn a_save_that_keeps_writing_runs_past_the_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("out.txt");
    // ~1.8 s of work against a 1 s limit, with a write every ~150 ms: three times
    // the limit in total, a fraction of it between any two signs of life.
    let (session_id, total_bytes) = a_session_reading_slowly(12, Duration::from_millis(150));

    let result = write_range_watched(
        session_id.clone(),
        1,
        RangeEnd::Row { row: 0, offset: 0 },
        RangeEnd::Eof,
        dest.to_string_lossy().into_owned(),
        Duration::from_secs(1),
    )
    .await;

    assert!(
        result.is_ok(),
        "a save making progress must not be cut off, got {result:?}"
    );
    assert_eq!(
        std::fs::metadata(&dest).expect("the destination exists").len(),
        total_bytes as u64
    );
    file_viewer::close_session(&session_id).expect("close");
}

/// A save that goes quiet for the limit gives up, and the work it detached stops
/// and takes its temp file with it.
#[tokio::test]
async fn a_save_that_goes_quiet_gives_up_and_leaves_no_temp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("out.txt");
    // One fetch lands and is written (so the temp holds bytes, which is what makes the
    // cleanup below worth asserting), then the source goes quiet for far longer than
    // the limit.
    let quiet_session = file_viewer::session::test_only_install_session(
        Box::new(file_viewer::session::ScriptedBackend::new(
            &"s".repeat(ROW_BYTES - 1),
            FETCH_ROWS * 2,
            |call| {
                if call > 0 {
                    // allowed-test-sleep: the silence IS the subject; this stands in
                    // for a read that stopped coming back.
                    std::thread::sleep(Duration::from_secs(3));
                }
            },
        )),
        std::path::PathBuf::from("/scripted/quiet.txt"),
    );

    let result = write_range_watched(
        quiet_session.clone(),
        1,
        RangeEnd::Row { row: 0, offset: 0 },
        RangeEnd::Eof,
        dest.to_string_lossy().into_owned(),
        Duration::from_millis(500),
    )
    .await;

    assert!(
        matches!(result, Err(ViewerError::TimedOut)),
        "a save that stopped writing must give up, got {result:?}"
    );
    assert!(!dest.exists(), "a save that gave up must not leave a destination");
    assert!(
        save_temp_file(dir.path()).is_some(),
        "the give-up has to land on a save that had already written, or the cleanup below proves nothing"
    );
    crate::test_support::wait_until_async(Duration::from_secs(10), "the stopped save to clean up", || {
        save_temp_file(dir.path()).is_none()
    })
    .await;
    file_viewer::close_session(&quiet_session).expect("close");
}

#[tokio::test]
async fn write_range_rejects_a_destination_inside_an_archive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("bundle.zip");
    write_zip(&zip);

    // The guard runs before any session lookup, so a bogus session id is fine: the
    // point is that an archive-inner DESTINATION is refused with the typed error.
    let dest = zip.join("inner.txt");
    let err = viewer_write_range_to_file(
        "no-such-session".to_string(),
        0,
        RangeEnd::Eof,
        RangeEnd::Eof,
        dest.to_string_lossy().into_owned(),
    )
    .await
    .expect_err("archive-inner destination must be refused");
    assert!(
        matches!(err, ViewerError::DestinationIsReadOnly),
        "expected DestinationIsReadOnly, got {err:?}"
    );

    // A plain sibling destination passes the guard (proves it's not a blanket reject);
    // the bogus session then surfaces as SessionNotFound.
    let plain = dir.path().join("out.txt");
    let err = viewer_write_range_to_file(
        "no-such-session".to_string(),
        0,
        RangeEnd::Eof,
        RangeEnd::Eof,
        plain.to_string_lossy().into_owned(),
    )
    .await
    .expect_err("bogus session should fail past the guard");
    assert!(
        matches!(err, ViewerError::SessionNotFound { .. }),
        "expected the guard to pass and the session lookup to fail, got {err:?}"
    );
}

/// The guard is about ROUTES, not about archives: a path inside a repo's
/// virtual `.git` trees has no directory on disk either, so a save there must
/// meet the same typed refusal rather than a raw `std::fs` errno. A real file
/// under `.git` is an ordinary local path and still writes.
#[tokio::test]
async fn saving_into_a_repos_history_is_refused_the_way_saving_into_a_zip_is() {
    use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

    let dir = temp_dir("viewer_save_guard", "snapshot");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    crate::file_system::git::wiring::set_virtual_portal_enabled(true);

    let snapshot = dir.join(".git/branches/main/saved.txt");
    let err = viewer_write_range_to_file(
        "no-such-session".to_string(),
        0,
        RangeEnd::Eof,
        RangeEnd::Eof,
        snapshot.to_string_lossy().into_owned(),
    )
    .await
    .expect_err("a snapshot destination must be refused");
    assert!(
        matches!(err, ViewerError::DestinationIsReadOnly),
        "expected DestinationIsReadOnly, got {err:?}"
    );

    // A REAL file under `.git` is the parent volume's and takes ordinary
    // writes, so it passes the guard and fails only on the bogus session.
    let real = dir.join(".git/config.bak");
    let err = viewer_write_range_to_file(
        "no-such-session".to_string(),
        0,
        RangeEnd::Eof,
        RangeEnd::Eof,
        real.to_string_lossy().into_owned(),
    )
    .await
    .expect_err("bogus session should fail past the guard");
    assert!(
        matches!(err, ViewerError::SessionNotFound { .. }),
        "a real path under `.git` must pass the guard, got {err:?}"
    );

    cleanup(&dir);
}

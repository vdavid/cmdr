//! Tests for a session's range reads: `read_range` (the clipboard) and
//! `write_range_to_file` (Save as file), cancellation included.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;

use super::session;
use super::session_test_support::{create_test_dir, write_test_file};
use super::{FULL_LOAD_THRESHOLD, FileEncoding, RangeEnd, ViewerError};

fn row(row: u64, offset: u32) -> RangeEnd {
    RangeEnd::Row { row, offset }
}

#[test]
fn read_range_full_load_anchor_equals_focus_returns_empty() {
    let dir = create_test_dir("range_eq");
    let file = write_test_file(&dir, "test.txt", "hello\nworld\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let out = session::read_range(&sid, 1, row(0, 3), row(0, 3)).unwrap();
    assert_eq!(out, "");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_single_line_slice() {
    let dir = create_test_dir("range_single_line");
    let file = write_test_file(&dir, "test.txt", "hello world\nsecond line\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // Slice "ello w" out of the first line.
    let out = session::read_range(&sid, 1, row(0, 1), row(0, 7)).unwrap();
    assert_eq!(out, "ello w");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_multi_line_includes_newlines_between() {
    let dir = create_test_dir("range_multi_line");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\ndelta\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // From (0, 2) "pha\n" + "beta\n" + "gamma\n" + "del" => "pha\nbeta\ngamma\ndel".
    let out = session::read_range(&sid, 1, row(0, 2), row(3, 3)).unwrap();
    assert_eq!(out, "pha\nbeta\ngamma\ndel");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_reversed_inputs_normalised() {
    let dir = create_test_dir("range_reversed");
    let file = write_test_file(&dir, "test.txt", "hello world\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let forward = session::read_range(&sid, 1, row(0, 0), row(0, 5)).unwrap();
    let reversed = session::read_range(&sid, 2, row(0, 5), row(0, 0)).unwrap();
    assert_eq!(forward, reversed);
    assert_eq!(forward, "hello");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_out_of_range_returns_typed_error() {
    let dir = create_test_dir("range_oor");
    let file = write_test_file(&dir, "test.txt", "one line only\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let err = session::read_range(&sid, 1, row(99, 0), row(99, 5)).unwrap_err();
    assert!(matches!(err, ViewerError::OutOfRange));

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_eof_selects_to_end() {
    let dir = create_test_dir("range_eof");
    let file = write_test_file(&dir, "test.txt", "first\nsecond\nthird\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let out = session::read_range(&sid, 1, row(0, 0), RangeEnd::Eof).unwrap();
    // The trailing newline is excluded (half-open semantics; the last line's content is included).
    assert_eq!(out, "first\nsecond\nthird\n");
    // Note: this file *has* a trailing newline; the split gives a 4th empty "line", whose
    // start is included. Trailing newline trimming leaves "first\nsecond\nthird\n".

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_utf16_surrogate_clamps_down() {
    let dir = create_test_dir("range_surrogate");
    // "👋hi" has UTF-16 length 4 (emoji is 2 units, then h, i).
    let file = write_test_file(&dir, "test.txt", "👋hi\nnext\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // Offset 1 lands inside the surrogate pair; clamp to 0 — output excludes the emoji entirely.
    let out = session::read_range(&sid, 1, row(0, 1), row(0, 3)).unwrap();
    // From (clamped) 0 to 3 (= 'h'): "👋h"? No — clamp pulls offset 1 down to byte 0, so we get [0..byte_for_3].
    // Byte for offset 3 = end of 'h' = byte 5. So output is the full "👋h" = 5 bytes.
    assert_eq!(out, "👋h");

    // A clearer case: offset 1 to 1 (single-line collapsed inside the emoji's surrogate) → "".
    let out_empty = session::read_range(&sid, 2, row(0, 1), row(0, 1)).unwrap();
    assert_eq!(out_empty, "");

    // Offset 0 to 2 (full emoji) → "👋".
    let out_emoji = session::read_range(&sid, 3, row(0, 0), row(0, 2)).unwrap();
    assert_eq!(out_emoji, "👋");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_only_newlines_file() {
    let dir = create_test_dir("range_only_newlines");
    // Three empty lines: "\n\n\n" gives 4 entries when split by '\n' (empty, empty, empty, empty).
    let file = write_test_file(&dir, "test.txt", "\n\n\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // Select all up to line 2 offset 0: "(empty)\n(empty)\n" = "\n\n".
    let out = session::read_range(&sid, 1, row(0, 0), row(2, 0)).unwrap();
    assert_eq!(out, "\n\n");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_byte_seek_eof_selects_whole_file() {
    let dir = create_test_dir("range_bs_eof");
    // Create a > 1 MB file so we land in ByteSeek.
    let line_str = "0123456789".repeat(10) + "\n"; // 101 bytes
    let line_count = (FULL_LOAD_THRESHOLD as usize / line_str.len()) + 100;
    let content: String = line_str.repeat(line_count);
    let file = write_test_file(&dir, "big.txt", &content);
    let open = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = open.session_id.clone();

    let out = session::read_range(&sid, 1, row(0, 0), RangeEnd::Eof).unwrap();
    // Every byte, final newline included: a file ending in a newline has a final empty
    // row, so ⌘A gives the same answer whichever backend a file's size lands it on.
    assert_eq!(out, content);

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_cancellation_returns_cancelled() {
    use std::sync::atomic::AtomicBool;

    // Direct test of the inner-loop cancel check: bypass the session and drive the
    // pure `read_range` against a `FullLoad` backend, flipping the cancel flag BEFORE
    // the call so the result is deterministic (no thread race). A file with ~512
    // lines is plenty to exceed `CANCEL_CHECK_LINES = 256`, guaranteeing the in-loop
    // check fires at least once.
    use super::full_load::FullLoadBackend;
    use super::range_read::read_range;

    let line_str = "x".repeat(64) + "\n"; // 65 bytes per line.
    let content: String = line_str.repeat(512);
    let backend = FullLoadBackend::from_content(&content, "cancel.txt");

    let cancel = AtomicBool::new(true);
    let result = read_range(&backend, row(0, 0), RangeEnd::Eof, &cancel);

    assert!(
        matches!(result, Err(ViewerError::Cancelled)),
        "expected ViewerError::Cancelled, got: {:?}",
        result
    );
}

#[test]
fn read_range_session_cancellation_returns_cancelled_and_cleans_up() {
    // End-to-end through the session: use a file big enough that the read can't
    // complete before the canceller fires. 1 MB + 1024 lines pushes us into ByteSeek
    // mode and the read takes long enough to interrupt deterministically.
    let dir = create_test_dir("range_session_cancel");
    // ~10 MB file with 4 KB lines: 4096 lines = ~16 MB → ByteSeek mode, and the read
    // needs many fetch chunks. The in-loop check fires hundreds of times.
    let line_str = "z".repeat(4096) + "\n";
    let content: String = line_str.repeat(4096);
    let file = write_test_file(&dir, "big.txt", &content);
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let sid_for_cancel = sid.clone();
    let canceller = thread::spawn(move || {
        // Cancel the instant the read registers itself, which it does before reading a
        // byte. The in-loop check (every 64 KB or 256 lines) then fires well before a
        // ~16 MB read completes. ❗ A spin, ❌ not a sleep or a polling wait: a fixed
        // 2 ms head start lost the race on a loaded macOS runner, where the canceller
        // wasn't scheduled until the whole file was read (2026-10-20), and a poll
        // interval would reopen the same window.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while session::active_read_count(&sid_for_cancel) == 0 {
            assert!(std::time::Instant::now() < deadline, "the read never registered");
            thread::yield_now();
        }
        session::cancel_read(&sid_for_cancel, 42).unwrap();
    });

    let result = session::read_range(&sid, 42, row(0, 0), RangeEnd::Eof);
    let _ = canceller.join();

    assert!(
        matches!(result, Err(ViewerError::Cancelled)),
        "expected ViewerError::Cancelled, got: {:?}",
        result.map(|s| format!("Ok({} bytes)", s.len()))
    );
    assert_eq!(session::active_read_count(&sid), 0);

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_full_load_crlf_preserves_carriage_returns() {
    // CRLF files: the backend keeps the `\r` AS PART of the line text (it only splits
    // on `\n`). So `read_range` returns lines with their `\r` intact, and the byte-
    // offset arithmetic (`line.len() + 1`) correctly accounts for both bytes. This
    // test pins both behaviours so a future "auto-strip \r" change doesn't silently
    // drift `byte_offset`.
    let dir = create_test_dir("range_crlf");
    let content = "alpha\r\nbeta\r\ngamma\r\n";
    let file = write_test_file(&dir, "crlf.txt", content);
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // ⌘A-equivalent: read everything. The backend keeps `\r` as part of each line;
    // `range_read` rejoins with `\n` between lines and trims exactly one trailing
    // newline at EOF (the same half-open behaviour the LF test asserts). Net: the
    // original CRLF bytes round-trip exactly.
    let out = session::read_range(&sid, 1, row(0, 0), RangeEnd::Eof).unwrap();
    assert_eq!(out, "alpha\r\nbeta\r\ngamma\r\n");

    // Multi-line slice: from (0, 2) to (1, 3). On line 0 the text after offset 2 is
    // "pha\r" (offset 2 in UTF-16 lands on byte 2 of "alpha\r"). Then the joining
    // `\n`. Then on line 1 from offset 0 to 3 = "bet".
    let slice = session::read_range(&sid, 2, row(0, 2), row(1, 3)).unwrap();
    assert_eq!(slice, "pha\r\nbet");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_cleans_up_active_reads_on_success() {
    let dir = create_test_dir("range_cleanup");
    let file = write_test_file(&dir, "test.txt", "small\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    session::read_range(&sid, 7, row(0, 0), row(0, 5)).unwrap();
    assert_eq!(session::active_read_count(&sid), 0);

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_session_not_found_returns_typed_error() {
    let err = session::read_range("nonexistent-session", 1, row(0, 0), row(0, 5)).unwrap_err();
    assert!(matches!(err, ViewerError::SessionNotFound { .. }));
}

#[test]
fn cancel_read_unknown_id_is_no_op() {
    let dir = create_test_dir("cancel_unknown");
    let file = write_test_file(&dir, "test.txt", "small\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // No read with id 99; cancel should succeed silently.
    session::cancel_read(&sid, 99).unwrap();

    session::close_session(&sid).unwrap();
}

#[test]
fn write_range_to_file_writes_atomically() {
    let dir = create_test_dir("write_range");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let dest = dir.join("out.txt");
    session::write_range_to_file(&sid, 1, row(0, 0), row(2, 5), &dest, &session::SaveProgress::new()).unwrap();
    let written = fs::read_to_string(&dest).unwrap();
    assert_eq!(written, "alpha\nbeta\ngamma");

    // No leftover temp file in the dir.
    let leftover = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains("cmdr-tmp"));
    assert!(!leftover, "temp file leaked after successful write");

    session::close_session(&sid).unwrap();
}

#[test]
fn write_range_to_file_propagates_out_of_range_error() {
    let dir = create_test_dir("write_range_oor");
    let file = write_test_file(&dir, "test.txt", "one line\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let dest = dir.join("out.txt");
    let err = session::write_range_to_file(&sid, 1, row(99, 0), row(99, 5), &dest, &session::SaveProgress::new())
        .unwrap_err();
    assert!(matches!(err, ViewerError::OutOfRange));
    assert!(!dest.exists());

    session::close_session(&sid).unwrap();
}

/// The `.cmdr-tmp-<uuid>` file a save writes before its rename, if one is there.
fn save_temp_file(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().contains("cmdr-tmp"))
}

/// Bytes in the save's temp file right now; 0 when the save hasn't created one.
fn save_temp_len(dir: &Path) -> u64 {
    save_temp_file(dir)
        .and_then(|p| fs::metadata(p).ok())
        .map_or(0, |m| m.len())
}

/// A line long enough that a few thousand of them dwarf the save's flush threshold,
/// sized so one line plus its `\n` is exactly 1 KiB.
fn kib_line() -> String {
    "x".repeat(1023)
}

/// A save must hand bytes to the destination as it reads them, so its peak memory is
/// one chunk rather than the whole selection (the 100 MiB clipboard refusal offers
/// "Save as" as the way out, so buffering here would allocate exactly what the refusal
/// protects the user from).
///
/// The probe is structural, not timed: the scripted backend looks at the destination
/// temp from inside the read loop, at a point the reader reaches deterministically. A
/// save that buffers the whole range is caught by the temp still being empty when the
/// reader asks for its second chunk.
#[test]
fn write_range_to_file_streams_as_it_reads() {
    let dir = create_test_dir("write_range_streams");
    let dest = dir.join("out.txt");
    let probe_dir = dir.to_path_buf();
    let temp_lens: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let lens_for_probe = temp_lens.clone();

    // 5,000 KiB lines: more than one 4,096-line fetch chunk, and several times the
    // flush threshold, so a streaming save has flushed well before chunk two.
    let backend = session::ScriptedBackend::new(&kib_line(), 5_000, move |call| {
        if call > 0 {
            lens_for_probe.lock_ignore_poison().push(save_temp_len(&probe_dir));
        }
    });
    let sid = session::test_only_install_session(Box::new(backend), dir.join("scripted.txt"));

    session::write_range_to_file(&sid, 1, row(0, 0), RangeEnd::Eof, &dest, &session::SaveProgress::new()).unwrap();

    let lens = temp_lens.lock_ignore_poison().clone();
    assert!(
        !lens.is_empty(),
        "the fixture must span more than one fetch chunk for this to prove anything"
    );
    assert!(
        lens[0] > 0,
        "nothing had reached the destination by the time the reader asked for its second chunk: \
         the save is buffering the whole range in memory"
    );

    session::close_session(&sid).unwrap();
}

/// Byte-exactness across chunk seams: a range several times the flush threshold saves
/// exactly what the source holds, with no byte dropped or doubled at a flush boundary.
#[test]
fn write_range_to_file_matches_the_source_across_chunks() {
    let dir = create_test_dir("write_range_chunks");
    // ~2.8 MB, so the save flushes a few times on the way through.
    let mut content = String::new();
    for i in 0..30_000 {
        content.push_str(&format!("line {:06} {}\n", i, "y".repeat(80)));
    }
    let file = write_test_file(&dir, "big.txt", &content);
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // Explicit end rather than `Eof`: this file opens on ByteSeek and upgrades to
    // LineIndex in the background, and the two disagree on whether a newline-terminated
    // file has a trailing empty line. Naming the last line's end keeps the expected
    // bytes the same whichever backend serves the read.
    let dest = dir.join("out.txt");
    let last_line_len = 92; // "line 029999 " + 80 padding characters
    session::write_range_to_file(
        &sid,
        1,
        row(0, 0),
        row(29_999, last_line_len),
        &dest,
        &session::SaveProgress::new(),
    )
    .unwrap();

    let written = fs::read_to_string(&dest).unwrap();
    let expected = content.strip_suffix('\n').unwrap();
    // Compared with a message rather than `assert_eq!`: a mismatch would otherwise
    // dump megabytes of fixture into the test output.
    assert!(
        written == expected,
        "saved {} bytes, expected {}",
        written.len(),
        expected.len()
    );

    session::close_session(&sid).unwrap();
}

/// The save decodes, so a UTF-16 source lands as UTF-8 text. Copying raw bytes would
/// be cheaper and would silently change the file on every non-UTF-8 source.
#[test]
fn write_range_to_file_saves_a_utf16_source_as_utf8() {
    let dir = create_test_dir("write_range_utf16");
    let file = dir.join("utf16.txt");
    let mut bytes = vec![0xFF, 0xFE]; // UTF-16LE BOM
    for unit in "alpha\nbeta\ngamma\n".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&file, &bytes).unwrap();

    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;
    let dest = dir.join("out.txt");
    let saved =
        session::write_range_to_file(&sid, 1, row(0, 0), row(2, 5), &dest, &session::SaveProgress::new()).unwrap();

    // UTF-8 text, not the source's UTF-16 code units (which would carry NUL bytes).
    assert_eq!(fs::read(&dest).unwrap(), b"alpha\nbeta\ngamma");
    assert_eq!(saved, session::SavedAs::Utf8);

    session::close_session(&sid).unwrap();
}

/// Cancelling mid-save leaves the destination untouched and no temp file behind, even
/// though the save has already written part of the range out.
#[test]
fn write_range_to_file_cancelled_mid_stream_leaves_nothing_behind() {
    let dir = create_test_dir("write_range_cancel");
    let dest = dir.join("out.txt");
    let session_for_probe: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let probe = session_for_probe.clone();

    // Cancel once the reader comes back for its second chunk: by then a streaming save
    // has bytes in its temp, which is exactly the state the cleanup has to handle.
    let backend = session::ScriptedBackend::new(&kib_line(), 5_000, move |call| {
        if call == 1
            && let Some(sid) = probe.lock_ignore_poison().as_ref()
        {
            session::cancel_read(sid, 1).unwrap();
        }
    });
    let sid = session::test_only_install_session(Box::new(backend), dir.join("scripted.txt"));
    *session_for_probe.lock_ignore_poison() = Some(sid.clone());

    let err = session::write_range_to_file(&sid, 1, row(0, 0), RangeEnd::Eof, &dest, &session::SaveProgress::new())
        .unwrap_err();
    assert!(matches!(err, ViewerError::Cancelled));
    assert!(!dest.exists(), "a cancelled save must not create the destination");
    assert!(
        save_temp_file(&dir).is_none(),
        "a cancelled save must clean up its temp file"
    );
    assert_eq!(session::active_read_count(&sid), 0);

    session::close_session(&sid).unwrap();
}

/// A save that outlived its stall watch: the watch flags the cancel and says "that took
/// too long", and the last write then finishes anyway. The rename must not land after
/// that answer, or the destination changes behind a reported failure.
#[test]
fn write_range_to_file_cancelled_after_its_last_read_leaves_the_destination_alone() {
    let dir = create_test_dir("write_range_cancel_late");
    let dest = dir.join("out.txt");
    fs::write(&dest, b"what was there").unwrap();
    let session_for_probe: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let probe = session_for_probe.clone();

    // Three short lines come back in one chunk, so the walk never looks at the flag
    // again after this read: the cancel lands just as a stalled final write would.
    let backend = session::ScriptedBackend::new("short", 3, move |_call| {
        if let Some(sid) = probe.lock_ignore_poison().as_ref() {
            session::cancel_read(sid, 1).unwrap();
        }
    });
    let sid = session::test_only_install_session(Box::new(backend), dir.join("scripted.txt"));
    *session_for_probe.lock_ignore_poison() = Some(sid.clone());

    let err = session::write_range_to_file(&sid, 1, row(0, 0), RangeEnd::Eof, &dest, &session::SaveProgress::new())
        .unwrap_err();

    assert!(matches!(err, ViewerError::Cancelled), "got {err:?}");
    assert_eq!(fs::read(&dest).unwrap(), b"what was there");
    assert!(save_temp_file(&dir).is_none(), "the temp goes instead of the rename");

    session::close_session(&sid).unwrap();
}

/// A file detected as UTF-8 from its head, with a stray Latin-1 byte past it.
fn utf8_head_then_a_latin1_byte(dir: &Path) -> (PathBuf, usize) {
    let line = "a".repeat(63);
    let clean_rows = 1_200; // 1,200 64-byte lines: past the 64 KiB detection head
    let mut bytes = Vec::new();
    for _ in 0..clean_rows {
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(b"caf\xE9 au lait\nend\n");
    let file = dir.join("mostly-utf8.log");
    fs::write(&file, &bytes).unwrap();
    (file, clean_rows)
}

/// The decode swaps a byte it can't read for U+FFFD on screen, but the save keeps the
/// file's own bytes: a quietly altered copy reported as a success is what this guards.
#[test]
fn write_range_to_file_keeps_the_source_bytes_a_utf8_decode_would_replace() {
    let dir = create_test_dir("write_range_lossy");
    let (file, bad_row) = utf8_head_then_a_latin1_byte(&dir);
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;
    let dest = dir.join("out.txt");

    session::write_range_to_file(
        &sid,
        1,
        row(bad_row as u64 - 1, 60),
        row(bad_row as u64 + 1, 2),
        &dest,
        &session::SaveProgress::new(),
    )
    .unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"aaa\ncaf\xE9 au lait\nen");

    // Within the one row, both ends measured past the replaced byte.
    session::write_range_to_file(
        &sid,
        2,
        row(bad_row as u64, 2),
        row(bad_row as u64, 7),
        &dest,
        &session::SaveProgress::new(),
    )
    .unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"f\xE9 au");

    session::close_session(&sid).unwrap();
}

/// A UTF-8 save that kept undecodable bytes is still UTF-8, so it carries no note.
#[test]
fn a_utf8_save_that_kept_source_bytes_reports_utf8() {
    let dir = create_test_dir("write_range_lossy_outcome");
    let (file, bad_row) = utf8_head_then_a_latin1_byte(&dir);
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;
    let dest = dir.join("out.txt");

    let saved = session::write_range_to_file(
        &sid,
        1,
        row(bad_row as u64, 0),
        RangeEnd::Eof,
        &dest,
        &session::SaveProgress::new(),
    )
    .unwrap();
    assert_eq!(saved, session::SavedAs::Utf8);
    assert_eq!(fs::read(&dest).unwrap(), b"caf\xE9 au lait\nend\n");

    session::close_session(&sid).unwrap();
}

/// `text` as UTF-16 LE code units, with `lone_high_surrogate_at` (a UTF-16 index into
/// `text`) getting a lone `0xD800` inserted before it: the one byte pair a UTF-16
/// decode can't read.
fn utf16le(text: &str, lone_high_surrogate_at: Option<usize>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (index, unit) in text.encode_utf16().enumerate() {
        if Some(index) == lone_high_surrogate_at {
            bytes.extend_from_slice(&0xD800u16.to_le_bytes());
        }
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn utf16le_file(dir: &Path, body: &[u8]) -> PathBuf {
    let file = dir.join("utf16.txt");
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend_from_slice(body);
    fs::write(&file, &bytes).unwrap();
    file
}

/// A UTF-16 range whose decode lost a character saves the source's own bytes, BOM
/// first, and says so: converting to UTF-8 would have swapped that character for
/// U+FFFD and reported a clean save.
#[test]
fn a_lossy_utf16_range_saves_in_its_own_encoding_and_says_so() {
    let dir = create_test_dir("write_range_utf16_lossy");
    // "alpha\n", then "beta" with a lone high surrogate before its "t", then "gamma\n".
    let file = utf16le_file(&dir, &utf16le("alpha\nbeta\ngamma\n", Some(8)));
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;
    let dest = dir.join("out.txt");

    let saved =
        session::write_range_to_file(&sid, 1, row(0, 2), row(2, 3), &dest, &session::SaveProgress::new()).unwrap();
    assert_eq!(
        saved,
        session::SavedAs::SourceEncoding {
            encoding: FileEncoding::Utf16Le
        }
    );
    // Row 1 reads "be\u{FFFD}ta" on screen; the FFFD is one UTF-16 unit, like the source.
    let mut expected = vec![0xFF, 0xFE];
    expected.extend_from_slice(&utf16le("pha\nbeta\ngam", Some(6)));
    assert_eq!(fs::read(&dest).unwrap(), expected);

    // Within the one row, ends on either side of the lost character.
    let saved =
        session::write_range_to_file(&sid, 2, row(1, 1), row(1, 4), &dest, &session::SaveProgress::new()).unwrap();
    assert!(matches!(saved, session::SavedAs::SourceEncoding { .. }));
    let mut expected = vec![0xFF, 0xFE];
    expected.extend_from_slice(&utf16le("et", Some(1)));
    assert_eq!(fs::read(&dest).unwrap(), expected);

    // To the end of the file, its last newline included (the empty row after it ends the range).
    let saved =
        session::write_range_to_file(&sid, 3, row(1, 0), RangeEnd::Eof, &dest, &session::SaveProgress::new()).unwrap();
    assert!(matches!(saved, session::SavedAs::SourceEncoding { .. }));
    let mut expected = vec![0xFF, 0xFE];
    expected.extend_from_slice(&utf16le("beta\ngamma\n", Some(2)));
    assert_eq!(fs::read(&dest).unwrap(), expected);

    session::close_session(&sid).unwrap();
}

/// Only the SELECTED text decides: a range that skips the lost character converts to
/// UTF-8 as usual, with no note.
#[test]
fn a_utf16_range_beside_a_lost_character_still_saves_as_utf8() {
    let dir = create_test_dir("write_range_utf16_beside_lossy");
    let file = utf16le_file(&dir, &utf16le("alpha\nbeta\ngamma\n", Some(8)));
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;
    let dest = dir.join("out.txt");

    let saved =
        session::write_range_to_file(&sid, 1, row(1, 3), row(2, 2), &dest, &session::SaveProgress::new()).unwrap();
    assert_eq!(saved, session::SavedAs::Utf8);
    assert_eq!(fs::read(&dest).unwrap(), b"ta\nga");

    session::close_session(&sid).unwrap();
}

#[test]
fn read_range_stitching_adjacent_ranges_equals_one_big_range() {
    let dir = create_test_dir("range_stitch");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\ndelta\nepsilon\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // Pick a split point in the middle of the file: between (1, 2) and (1, 2).
    let big = session::read_range(&sid, 1, row(0, 0), row(4, 5)).unwrap();
    let first = session::read_range(&sid, 2, row(0, 0), row(1, 2)).unwrap();
    let second = session::read_range(&sid, 3, row(1, 2), row(4, 5)).unwrap();
    assert_eq!(big, format!("{}{}", first, second));

    session::close_session(&sid).unwrap();
}

//! Tail walker: read a log file from the end backwards in fixed-size chunks, yielding
//! lines newest-first. Stop the moment a leading ISO-8601 timestamp falls older than a
//! cutoff. Used by Flow A's "last hour of log content" filter so we don't have to read
//! and redact the entire ~180 MB log dir to ship a 1 MB triage bundle.
//!
//! ## Why backwards?
//!
//! Flow A's window is "the last N minutes of content," anchored on `now`. The newest
//! lines live at the END of the file. Reading forward from the start means redacting and
//! buffering everything older too — wasted CPU when 99 %+ of the file is outside the
//! window. Reading backward from the end lets us bail the instant we cross the cutoff.
//!
//! ## Multi-line entries (panic backtraces, state YAML)
//!
//! Lines without a parseable leading timestamp are continuation lines of a multi-line
//! record (panic backtraces, state-snapshot YAML). They MUST pass through untouched, and
//! the cut boundary must land on a *timestamped* line — otherwise we'd ship a partial
//! backtrace prefix without its header. Concretely: when the walker sees a non-timestamped
//! line, it yields it without making a cutoff decision; the very next timestamped line is
//! what the cutoff is checked against.
//!
//! ## Long lines spanning multiple chunks
//!
//! Lines longer than `CHUNK_SIZE` (state YAML, big backtraces, ~10 KB stack frames) are
//! handled by accumulating chunks in a tail buffer until a `\n` shows up. No max line
//! length is assumed.
//!
//! ## CRLF
//!
//! The fern file chain writes LF only, but we strip a trailing `\r` defensively in case
//! a future change introduces CRLF anywhere (or someone hand-edits a log on Windows).

use chrono::{DateTime, Utc};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Read this many bytes per backward seek. Picked to be large enough that "typical" lines
/// (a few hundred bytes) come in batches of dozens, but small enough that a 100 ms seek on
/// a network mount doesn't show up as a stutter. 64 KB matches the file chain's write
/// buffer, so we usually pick up exactly one rotation's worth of buffered writes per pull.
const CHUNK_SIZE: usize = 64 * 1024;

/// Outcome of walking a single log file from the tail.
///
/// `lines` is in **forward order** (oldest kept first, newest last) — we reverse before
/// returning so callers can write them straight into the zip without flipping again.
/// `hit_cutoff` is true when the walker stopped because it crossed the timestamp cutoff
/// (vs. ran out of file). Callers use this to decide whether to keep walking older
/// rotations (no — we have everything we need) or to keep going (yes — the window
/// extends past this file's age).
pub struct TailWalkResult {
    /// Lines kept, oldest-first. Newlines stripped. `\r` trim-trailed.
    pub lines: Vec<String>,
    /// True if the walker stopped at the cutoff. False if it consumed the whole file.
    pub hit_cutoff: bool,
}

/// Read `path` from the end backward. Yield lines until either (a) a leading timestamp
/// is older than `cutoff`, or (b) the file is exhausted.
///
/// Lines without a parseable leading ISO-8601 timestamp are kept as continuation context
/// without triggering a cutoff check — see the module doc.
///
/// Errors are mapped to "no lines, did not hit cutoff" so the caller can keep going. We
/// log a warning at the same call site the legacy file-by-file reader did. An empty file
/// returns `(empty lines, hit_cutoff = false)`.
pub fn walk_tail(path: &Path, cutoff: DateTime<Utc>) -> std::io::Result<TailWalkResult> {
    let mut file = File::open(path)?;
    let total_len = file.seek(SeekFrom::End(0))?;
    if total_len == 0 {
        return Ok(TailWalkResult {
            lines: Vec::new(),
            hit_cutoff: false,
        });
    }

    // Newest-first scratch list. Reversed at the end.
    let mut newest_first: Vec<String> = Vec::new();
    let mut hit_cutoff = false;

    // `pending` holds bytes we've read but haven't split into a complete line yet. It's
    // the SUFFIX of the not-yet-yielded prefix of the file: if we read backward and the
    // chunk is `b"abc\ndefgh"`, "defgh" is held until the next chunk shows where its
    // line begins (or until we hit byte 0).
    let mut pending: Vec<u8> = Vec::new();
    let mut cursor = total_len;
    let mut buf = vec![0u8; CHUNK_SIZE];

    'outer: while cursor > 0 {
        let read_len = cursor.min(CHUNK_SIZE as u64) as usize;
        cursor -= read_len as u64;
        file.seek(SeekFrom::Start(cursor))?;
        let slice = &mut buf[..read_len];
        file.read_exact(slice)?;

        // Prepend chunk to pending, building combined = chunk_bytes + pending.
        let mut combined: Vec<u8> = Vec::with_capacity(read_len + pending.len());
        combined.extend_from_slice(slice);
        combined.extend_from_slice(&pending);
        pending.clear();

        // Walk newlines from RIGHT to LEFT inside `combined`. Each `\n` separates
        // a finished line on its right from earlier text on its left. When we cross a
        // newline, we yield the right-hand line. The leftmost segment (before the
        // earliest `\n` we found in `combined`) becomes the new `pending` — it might
        // continue earlier in the file.
        let mut end = combined.len();
        while end > 0 {
            // Find the previous '\n' in combined[..end].
            match combined[..end].iter().rposition(|b| *b == b'\n') {
                Some(nl_idx) => {
                    let line_bytes = &combined[nl_idx + 1..end];
                    end = nl_idx;
                    if !try_emit_line(line_bytes, cutoff, &mut newest_first, &mut hit_cutoff) {
                        break 'outer;
                    }
                }
                None => {
                    // No more newlines in this chunk. Whatever's left is the start of a
                    // line that may continue in older bytes. Park it as `pending` and
                    // pull another chunk.
                    pending.extend_from_slice(&combined[..end]);
                    end = 0;
                }
            }
        }
    }

    // Anything left in `pending` is a line that runs from byte 0 (no earlier newline
    // could exist). Emit it last — it's the OLDEST line in the file.
    if !pending.is_empty() && !hit_cutoff {
        // Doesn't matter what the function returns here — we're at file start, the loop
        // is going to exit anyway.
        let _ = try_emit_line(&pending, cutoff, &mut newest_first, &mut hit_cutoff);
    }

    newest_first.reverse();
    Ok(TailWalkResult {
        lines: newest_first,
        hit_cutoff,
    })
}

/// Try to emit one line. Returns `false` to signal the outer walker should stop
/// immediately (cutoff hit on a timestamped line). Returns `true` otherwise — the
/// walker keeps consuming older bytes.
///
/// `kept` is appended to (newest-first order); `hit_cutoff` is set to `true` once the
/// cutoff fires.
fn try_emit_line(raw: &[u8], cutoff: DateTime<Utc>, kept: &mut Vec<String>, hit_cutoff: &mut bool) -> bool {
    let mut bytes = raw;
    if bytes.last() == Some(&b'\r') {
        bytes = &bytes[..bytes.len() - 1];
    }
    // Empty lines: skip silently. The most common case is the empty slice that follows
    // the file's final `\n`. Less common: a blank separator line in the middle of the
    // log. Either way, blank lines have nothing useful for triage.
    if bytes.is_empty() {
        return true;
    }
    // Logs are UTF-8 by convention; `from_utf8_lossy` preserves bytes triagers can read
    // even when a line has a stray non-UTF-8 sequence (rare but the redactor handles it
    // downstream — we don't want to drop lines for that).
    let line = String::from_utf8_lossy(bytes).into_owned();

    if let Some(line_ts) = parse_leading_iso8601(&line)
        && line_ts < cutoff
    {
        // Cut here. Don't include this line — it's older than the window.
        *hit_cutoff = true;
        return false;
    }

    kept.push(line);
    true
}

/// Parses an ISO-8601 stamp at the start of a log line (matches the format produced by
/// `logging::dispatch::file_timestamp`: `YYYY-MM-DDTHH:MM:SS.mmm±HH:MM`).
///
/// Returns `None` for lines that don't start with one — pre-fix-3 lines that just have
/// `HH:MM:SS.mmm`, blank lines, panic-backtrace continuation lines, redacted-payload
/// lines, etc. Callers fall back to keeping the line in that case rather than risk a
/// false drop.
pub fn parse_leading_iso8601(line: &str) -> Option<DateTime<Utc>> {
    // The timestamp is always 29 chars: 23 for date+time+ms + 6 for `±HH:MM`.
    if line.len() < 29 {
        return None;
    }
    let candidate = &line[..29];
    DateTime::parse_from_str(candidate, "%Y-%m-%dT%H:%M:%S%.3f%:z")
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use std::io::Write;

    fn write_tmp(name: &str, body: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cmdr-tail-walker-{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let p = dir.join("log");
        let mut f = File::create(&p).expect("create");
        f.write_all(body).expect("write");
        p
    }

    fn iso_line(ts: DateTime<Utc>, body: &str) -> String {
        let local = ts.with_timezone(&chrono::Local);
        format!("{} {body}", local.format("%Y-%m-%dT%H:%M:%S%.3f%:z"),)
    }

    #[test]
    fn reads_short_file_in_reverse_when_no_cutoff() {
        let now = Utc::now();
        let body = format!(
            "{}\n{}\n{}\n",
            iso_line(now - ChronoDuration::seconds(3), "A"),
            iso_line(now - ChronoDuration::seconds(2), "B"),
            iso_line(now - ChronoDuration::seconds(1), "C"),
        );
        let p = write_tmp("basic", body.as_bytes());
        let result = walk_tail(&p, now - ChronoDuration::days(1)).unwrap();
        assert_eq!(result.lines.len(), 3);
        assert!(result.lines[0].ends_with(" A"));
        assert!(result.lines[1].ends_with(" B"));
        assert!(result.lines[2].ends_with(" C"));
        // Reached EOF, didn't cut.
        assert!(!result.hit_cutoff);
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn long_line_spanning_multiple_chunks() {
        let now = Utc::now();
        // 200 KB single line — three+ chunks worth of bytes for one line.
        let big_body: String = "x".repeat(200 * 1024);
        let big_line = iso_line(now - ChronoDuration::seconds(1), &big_body);
        let body = format!(
            "{}\n{}\n",
            iso_line(now - ChronoDuration::seconds(2), "short"),
            big_line,
        );
        let p = write_tmp("longline", body.as_bytes());
        let result = walk_tail(&p, now - ChronoDuration::days(1)).unwrap();
        assert_eq!(result.lines.len(), 2);
        assert!(result.lines[0].ends_with(" short"));
        assert_eq!(result.lines[1], big_line);
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn no_trailing_newline() {
        let now = Utc::now();
        let body = format!(
            "{}\n{}",
            iso_line(now - ChronoDuration::seconds(2), "first"),
            iso_line(now - ChronoDuration::seconds(1), "last-without-newline"),
        );
        let p = write_tmp("no-trailing-nl", body.as_bytes());
        let result = walk_tail(&p, now - ChronoDuration::days(1)).unwrap();
        assert_eq!(result.lines.len(), 2);
        assert!(result.lines[1].ends_with(" last-without-newline"));
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn empty_file_returns_no_lines() {
        let p = write_tmp("empty", b"");
        let result = walk_tail(&p, Utc::now()).unwrap();
        assert!(result.lines.is_empty());
        assert!(!result.hit_cutoff);
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn untimestamped_continuation_lines_pass_through() {
        // A panic-style block in the middle: header has a timestamp, frames don't.
        // The cutoff sits between "very old timestamped" and the rest.
        let now = Utc::now();
        let inside = now - ChronoDuration::minutes(10);
        let outside = now - ChronoDuration::hours(2);

        let body = format!(
            "{old} OLD line\n{header} ERROR something panicked\n   stack: frame 0\n   stack: frame 1\n{newer} INFO recovered\n",
            old = iso_line(outside, "").trim_end(),
            header = iso_line(inside, "").trim_end(),
            newer = iso_line(now - ChronoDuration::minutes(1), "").trim_end(),
        );
        let p = write_tmp("multiline", body.as_bytes());
        let result = walk_tail(&p, now - ChronoDuration::hours(1)).unwrap();
        // Should keep: the recovered line, the panic header + 2 frames. Drop the OLD
        // line. Order: oldest-first (header, frame 0, frame 1, recovered).
        assert!(result.hit_cutoff, "should have stopped at the cutoff");
        let joined = result.lines.join("\n");
        assert!(!joined.contains("OLD line"), "old line must be dropped: {joined}");
        assert!(joined.contains("ERROR something panicked"));
        assert!(joined.contains("frame 0"));
        assert!(joined.contains("frame 1"));
        assert!(joined.contains("recovered"));
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn window_boundary_drops_older_timestamped_lines() {
        let now = Utc::now();
        let cutoff = now - ChronoDuration::minutes(30);
        let body = format!(
            "{}\n{}\n{}\n{}\n",
            iso_line(now - ChronoDuration::hours(2), "way-old"),
            iso_line(now - ChronoDuration::hours(1), "old"),
            iso_line(now - ChronoDuration::minutes(20), "kept-1"),
            iso_line(now - ChronoDuration::minutes(5), "kept-2"),
        );
        let p = write_tmp("window", body.as_bytes());
        let result = walk_tail(&p, cutoff).unwrap();
        let joined = result.lines.join("\n");
        assert!(joined.contains("kept-1"));
        assert!(joined.contains("kept-2"));
        assert!(!joined.contains("way-old"));
        assert!(!joined.contains(" old"));
        assert!(result.hit_cutoff);
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn crlf_trailing_carriage_return_is_trimmed() {
        let now = Utc::now();
        let body = format!("{}\r\n", iso_line(now - ChronoDuration::seconds(1), "winline"));
        let p = write_tmp("crlf", body.as_bytes());
        let result = walk_tail(&p, now - ChronoDuration::days(1)).unwrap();
        assert_eq!(result.lines.len(), 1);
        assert!(!result.lines[0].ends_with('\r'));
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }
}

//! Bounded, duplicate-coalescing file writer for the log pipeline.
//!
//! ## Why
//!
//! The file chain writes every record synchronously (fern flushes after each one, and
//! `file-rotate` writes straight to an unbuffered `File`). That synchronous, unbuffered
//! path is exactly what makes the crash tail complete: every line a crashing run logged
//! is already on disk when the next launch bundles it. We keep that property.
//!
//! The failure mode it does NOT defend against is a runaway loop logging the SAME line
//! thousands of times a second: each identical record still costs a `write` syscall under
//! the log mutex, which once pegged a core and, via mutex contention, stalled threads that
//! only wanted to log. (An incident logged 12,700 near-identical warnings; a thread sample
//! caught ~900 samples inside `write`.) The source of that particular loop is fixed
//! elsewhere (fatal storage errors now stop instead of retrying), but this is the general
//! safety net so no future loop can burn a core through the log file.
//!
//! ## What
//!
//! [`CoalescingWriter`] wraps the real file sink. It buffers the bytes of one record (fern
//! hands us a record as one-or-more `write` calls followed by exactly one `flush`, all
//! under fern's own writer mutex), then on `flush` decides whether to emit it:
//!
//! - The first [`BURST_THRESHOLD`] occurrences of an identical `LEVEL target message` line
//!   inside a [`WINDOW`] are emitted verbatim, so normal low-rate repeats lose NOTHING.
//! - Beyond that, within the same window, identical lines are dropped and counted.
//! - When the window rolls over, the next occurrence is emitted with a
//!   `[+N identical suppressed]` tag, so triage sees the true repetition rate.
//!
//! Distinct lines are never coalesced with each other, so only a genuine flood is ever
//! trimmed. Suppressed records do no `write` syscall at all, which is what removes the
//! CPU burn and shrinks the mutex hold time to a hash-map probe.
//!
//! ## Timestamp placement
//!
//! The ISO-8601 stamp is prepended HERE, at emit time, not in the fern formatter. That's
//! deliberate: the dedup key must be timestamp-free, or two identical messages a
//! millisecond apart would hash differently and never coalesce. The on-disk line shape is
//! unchanged (`<iso-ts> LEVEL target  message`), so the error reporter's timestamp parsing
//! keeps working.

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};

/// Window over which identical lines are coalesced. Chosen so a runaway loop surfaces a
/// fresh count roughly once a second while a tight loop's thousands of repeats collapse to
/// a handful of writes per second.
const WINDOW: Duration = Duration::from_secs(1);

/// How many identical lines pass verbatim inside one window before coalescing kicks in.
/// Keeps ordinary low-rate repeats (a warning that legitimately fires a few times) fully
/// intact; only a genuine flood exceeds it.
const BURST_THRESHOLD: u64 = 3;

/// Cap on distinct keys tracked at once. A long session logs many distinct lines; without
/// a bound the map would grow unbounded. When exceeded we drop inactive keys (and, if still
/// full, clear), which at worst re-emits a line that would have been coalesced.
const MAX_KEYS: usize = 4096;

/// Per-key coalescing state within the current window.
struct KeyState {
    /// Start of the current window for this key.
    window_start: Instant,
    /// Occurrences seen in the current window (passed + suppressed).
    seen_in_window: u64,
    /// Occurrences dropped in the current window (surfaced at the next window rollover).
    suppressed: u64,
}

/// What to do with one record.
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    /// Emit the line. `suppressed_before` is the count carried over from the previous
    /// window (0 unless a flood just rolled over); when non-zero, tag the emitted line.
    Pass { suppressed_before: u64 },
    /// Drop the line (a within-window duplicate past the burst threshold).
    Suppress,
}

/// Duplicate-coalescing decision state, keyed by the timestamp-free record bytes. Not
/// thread-safe on its own; [`CoalescingWriter`] relies on fern serializing all access
/// through its writer mutex.
struct Coalescer {
    window: Duration,
    burst_threshold: u64,
    max_keys: usize,
    seen: HashMap<Box<[u8]>, KeyState>,
}

impl Coalescer {
    fn new(window: Duration, burst_threshold: u64, max_keys: usize) -> Self {
        Self {
            window,
            burst_threshold,
            max_keys,
            seen: HashMap::new(),
        }
    }

    /// Decide the fate of one occurrence of `key` (the timestamp-free `LEVEL target
    /// message` bytes) at time `now`.
    fn decide(&mut self, key: &[u8], now: Instant) -> Decision {
        if let Some(state) = self.seen.get_mut(key) {
            if now.duration_since(state.window_start) >= self.window {
                // Window rolled over: surface what the previous window suppressed.
                let carried = state.suppressed;
                state.window_start = now;
                state.seen_in_window = 1;
                state.suppressed = 0;
                return Decision::Pass {
                    suppressed_before: carried,
                };
            }
            state.seen_in_window += 1;
            if state.seen_in_window <= self.burst_threshold {
                Decision::Pass { suppressed_before: 0 }
            } else {
                state.suppressed += 1;
                Decision::Suppress
            }
        } else {
            self.maybe_evict(now);
            self.seen.insert(
                key.to_vec().into_boxed_slice(),
                KeyState {
                    window_start: now,
                    seen_in_window: 1,
                    suppressed: 0,
                },
            );
            Decision::Pass { suppressed_before: 0 }
        }
    }

    /// Keep the key set bounded. Called only on the insert path, so the common
    /// already-tracked case stays a single probe.
    fn maybe_evict(&mut self, now: Instant) {
        if self.seen.len() < self.max_keys {
            return;
        }
        let window = self.window;
        // Drop keys whose window has elapsed: they're idle, and a later occurrence simply
        // starts a fresh window (at worst one extra verbatim line).
        self.seen.retain(|_, s| now.duration_since(s.window_start) < window);
        if self.seen.len() >= self.max_keys {
            // Still full of active keys (a distinct-message storm). Deduping isn't helping
            // here anyway; reset rather than grow without bound.
            self.seen.clear();
        }
    }
}

/// A `Write` that coalesces identical record floods before handing bytes to `inner`.
///
/// One record arrives as a run of `write` calls terminated by a single `flush` (fern's
/// contract), all serialized by fern's writer mutex, so the buffered `pending` bytes hold
/// exactly one record at `flush` time and no interior locking is needed.
pub(crate) struct CoalescingWriter<W: Write> {
    inner: W,
    pending: Vec<u8>,
    coalescer: Coalescer,
    /// Emits the leading timestamp. A field (not a direct call) so tests can pin it.
    ts_fn: fn() -> String,
}

impl<W: Write> CoalescingWriter<W> {
    /// Production constructor: real window/threshold and the ISO-8601 file timestamp.
    pub(crate) fn new(inner: W) -> Self {
        Self::with_config(
            inner,
            WINDOW,
            BURST_THRESHOLD,
            MAX_KEYS,
            super::dispatch::file_timestamp,
        )
    }

    fn with_config(inner: W, window: Duration, burst_threshold: u64, max_keys: usize, ts_fn: fn() -> String) -> Self {
        Self {
            inner,
            pending: Vec::with_capacity(256),
            coalescer: Coalescer::new(window, burst_threshold, max_keys),
            ts_fn,
        }
    }

    /// Emit one already-decided record: `<ts> <line>` plus an optional suppression tag.
    fn emit(&mut self, line: &[u8], suppressed_before: u64) -> io::Result<()> {
        self.inner.write_all((self.ts_fn)().as_bytes())?;
        self.inner.write_all(b" ")?;
        self.inner.write_all(line)?;
        if suppressed_before > 0 {
            write!(self.inner, " [+{suppressed_before} identical suppressed]")?;
        }
        self.inner.write_all(b"\n")?;
        Ok(())
    }
}

impl<W: Write> Write for CoalescingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            // A bare flush (e.g. an explicit logger flush) with no record buffered.
            return self.inner.flush();
        }
        // fern appends the line separator ("\n") as the record's tail; the dedup key is the
        // record without it, so identical messages hash the same regardless of newline.
        let line: &[u8] = match self.pending.strip_suffix(b"\n") {
            Some(stripped) => stripped,
            None => &self.pending,
        };
        let decision = self.coalescer.decide(line, Instant::now());
        let result = match decision {
            Decision::Pass { suppressed_before } => {
                let line = line.to_vec();
                let r = self.emit(&line, suppressed_before);
                // Match fern's per-record flush so the crash tail stays on disk.
                r.and_then(|()| self.inner.flush())
            }
            Decision::Suppress => Ok(()),
        };
        self.pending.clear();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A `Write` sink capturing bytes, shared so a test can inspect what got emitted.
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Sink {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("sink lock")).into_owned()
        }
        fn lines(&self) -> usize {
            self.text().lines().count()
        }
    }
    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("sink lock").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn fixed_ts() -> String {
        "2026-07-18T10:00:00.000+02:00".to_string()
    }

    /// Push a whole record through the writer the way fern does: write the bytes, then a
    /// single flush.
    fn feed(w: &mut CoalescingWriter<Sink>, record: &str) {
        w.write_all(record.as_bytes()).expect("write");
        w.write_all(b"\n").expect("write newline");
        w.flush().expect("flush");
    }

    /// Distinct lines must never be coalesced with each other: normal operation loses
    /// nothing. Pre-fix a naive whole-line-with-timestamp dedup would also have passed
    /// these, but the point is that varied traffic is untouched.
    #[test]
    fn distinct_lines_all_pass() {
        let sink = Sink::new();
        let mut w = CoalescingWriter::with_config(sink.clone(), WINDOW, BURST_THRESHOLD, MAX_KEYS, fixed_ts);
        for i in 0..50 {
            feed(&mut w, &format!("DEBUG scanner  read dir {i}"));
        }
        assert_eq!(sink.lines(), 50, "every distinct line should be emitted");
    }

    /// Low-rate identical repeats up to the burst threshold pass verbatim, so a warning
    /// that legitimately fires a few times isn't swallowed.
    #[test]
    fn identical_repeats_within_threshold_pass() {
        let sink = Sink::new();
        let mut w = CoalescingWriter::with_config(sink.clone(), WINDOW, BURST_THRESHOLD, MAX_KEYS, fixed_ts);
        for _ in 0..BURST_THRESHOLD {
            feed(&mut w, "WARN net  flaky mount retry");
        }
        assert_eq!(
            sink.lines(),
            BURST_THRESHOLD as usize,
            "up to the threshold passes verbatim"
        );
    }

    /// A tight identical-line flood inside one window collapses to just the threshold's
    /// worth of writes: the rest do no `write` syscall at all. This is the CPU-burn
    /// defense — 10,000 repeats must not become 10,000 writes.
    #[test]
    fn identical_flood_is_coalesced() {
        let sink = Sink::new();
        let mut w = CoalescingWriter::with_config(sink.clone(), WINDOW, BURST_THRESHOLD, MAX_KEYS, fixed_ts);
        for _ in 0..10_000 {
            feed(&mut w, "WARN stall_probe::sqlite_busy  writer busy_handler attempt");
        }
        // The whole loop runs well inside one 1s window, so only the burst threshold is
        // written; everything past it is suppressed.
        assert_eq!(
            sink.lines(),
            BURST_THRESHOLD as usize,
            "a flood must collapse to the burst threshold, got:\n{}",
            sink.text()
        );
    }

    /// After the window rolls over, the next occurrence is emitted with the suppressed
    /// count, so triage sees how hard the loop was spinning.
    #[test]
    fn suppressed_count_surfaces_after_window() {
        let mut c = Coalescer::new(Duration::from_secs(1), 3, MAX_KEYS);
        let t0 = Instant::now();
        let key = b"WARN x  looping";
        // First 3 pass, next 7 suppressed, all in window 0.
        for _ in 0..3 {
            assert_eq!(c.decide(key, t0), Decision::Pass { suppressed_before: 0 });
        }
        for _ in 0..7 {
            assert_eq!(c.decide(key, t0), Decision::Suppress);
        }
        // Cross the window boundary: the carried count surfaces.
        let t1 = t0 + Duration::from_millis(1100);
        assert_eq!(c.decide(key, t1), Decision::Pass { suppressed_before: 7 });
        // And the counter resets for the new window.
        assert_eq!(c.decide(key, t1), Decision::Pass { suppressed_before: 0 });
    }

    /// A plain emit is `<ts> <line>` with no tag; a rollover emit appends the suppressed
    /// count. The on-disk shape keeps the leading ISO timestamp the error reporter parses.
    #[test]
    fn emit_shapes_the_on_disk_line() {
        let sink = Sink::new();
        let mut w = CoalescingWriter::with_config(sink.clone(), WINDOW, BURST_THRESHOLD, MAX_KEYS, fixed_ts);
        w.emit(b"WARN x  loop", 0).expect("emit");
        w.emit(b"WARN x  loop", 7).expect("emit");
        let text = sink.text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "2026-07-18T10:00:00.000+02:00 WARN x  loop");
        assert_eq!(
            lines[1],
            "2026-07-18T10:00:00.000+02:00 WARN x  loop [+7 identical suppressed]"
        );
    }

    /// A bare flush with nothing buffered must not emit a blank line (fern also flushes
    /// the whole dispatch on demand).
    #[test]
    fn bare_flush_emits_nothing() {
        let sink = Sink::new();
        let mut w = CoalescingWriter::with_config(sink.clone(), WINDOW, BURST_THRESHOLD, MAX_KEYS, fixed_ts);
        w.flush().expect("flush");
        assert_eq!(sink.text(), "", "a bare flush writes nothing");
    }

    /// The key set stays bounded under a distinct-message storm.
    #[test]
    fn key_set_is_bounded() {
        let mut c = Coalescer::new(Duration::from_secs(60), 3, 8);
        let now = Instant::now();
        for i in 0..100 {
            c.decide(format!("DEBUG t  msg {i}").as_bytes(), now);
        }
        assert!(
            c.seen.len() <= 8,
            "key set must stay within the cap, got {}",
            c.seen.len()
        );
    }
}

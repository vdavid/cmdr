//! Tests for the `~/Downloads` watcher.
//!
//! A sibling module rather than an inline `#[cfg(test)] mod tests`, matching the
//! repo's `*_test.rs` convention: the tests outweigh the watcher itself, and
//! keeping them out of `watcher.rs` keeps the file the harness auto-reads small.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{
    EventKind,
    event::{ModifyKind, RenameMode},
};
use notify_debouncer_full::DebouncedEvent;

use super::watcher::*;
use super::{IgnoreSet, is_eligible};

use std::fs;
use std::sync::Mutex;
use std::sync::mpsc;

use notify::Event;
use tempfile::TempDir;

/// `tempfile::TempDir::new` creates a hidden `.tmpXXX` dir on macOS, which
/// trips our hidden-component eligibility check on every contained path.
/// Use a non-dot prefix so positive-path assertions work.
fn unhidden_tempdir() -> TempDir {
    tempfile::Builder::new()
        .prefix("cmdr-downloads-test-")
        .tempdir()
        .unwrap()
}

/// Test sink that captures every emitted event.
struct ChannelSink {
    tx: Mutex<mpsc::Sender<DownloadDetectedEvent>>,
}

impl ChannelSink {
    fn new() -> (Arc<Self>, mpsc::Receiver<DownloadDetectedEvent>) {
        let (tx, rx) = mpsc::channel();
        (Arc::new(Self { tx: Mutex::new(tx) }), rx)
    }
}

impl EventSink for ChannelSink {
    fn emit(&self, event: DownloadDetectedEvent) {
        let _ = self.tx.lock().unwrap().send(event);
    }
}

fn touch(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, b"hi").unwrap();
    p
}

/// `touch`, then stamp the file's mtime at `unix_secs`.
///
/// `scan_latest` ranks by mtime, so its tests need one file to be provably newer than another.
/// Stamping beats writing them a few milliseconds apart: the ordering holds no matter how
/// coarse the filesystem's timestamp granularity is, and the test costs nothing to run.
fn touch_stamped(dir: &Path, name: &str, unix_secs: i64) -> PathBuf {
    let p = touch(dir, name);
    filetime::set_file_mtime(&p, filetime::FileTime::from_unix_time(unix_secs, 0)).unwrap();
    p
}

fn make_event(kind: EventKind, paths: Vec<PathBuf>) -> DebouncedEvent {
    let raw = Event {
        kind,
        paths,
        attrs: Default::default(),
    };
    DebouncedEvent::new(raw, Instant::now())
}

fn always_eligible(_: &Path) -> bool {
    true
}

fn never_eligible(_: &Path) -> bool {
    false
}

// ── classify_event unit tests ────────────────────────────────────

#[test]
fn eligible_create_emits() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let p = root.path().join("foo.zip");
    let result = classify_event(&EventSummary::Create(p.clone()), &set, &always_eligible);
    assert_eq!(result, Classification::Emit(p));
}

#[test]
fn ineligible_create_is_dropped() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let p = root.path().join("foo.crdownload");
    let result = classify_event(&EventSummary::Create(p), &set, &never_eligible);
    assert_eq!(result, Classification::Dropped);
}

#[test]
fn create_in_ignore_set_is_suppressed() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let p = root.path().join("foo.zip");
    set.note_pending(p.clone(), Duration::from_secs(5));
    let result = classify_event(&EventSummary::Create(p), &set, &always_eligible);
    assert_eq!(result, Classification::Suppressed);
}

#[test]
fn rename_both_emits_to_path() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let from = root.path().join("foo.zip.crdownload");
    let to = root.path().join("foo.zip");
    let summary = EventSummary::RenameBoth {
        from: from.clone(),
        to: to.clone(),
    };
    let result = classify_event(&summary, &set, &always_eligible);
    assert_eq!(result, Classification::Emit(to));
}

#[test]
fn rename_both_to_in_ignore_set_is_suppressed() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let from = root.path().join("foo.zip.crdownload");
    let to = root.path().join("foo.zip");
    set.note_pending(to.clone(), Duration::from_secs(5));
    let summary = EventSummary::RenameBoth { from, to };
    let result = classify_event(&summary, &set, &always_eligible);
    assert_eq!(result, Classification::Suppressed);
}

#[test]
fn rename_both_from_in_ignore_set_is_suppressed() {
    // Cmdr moved a file out of Downloads — register the source path and
    // both halves of the rename pair should be silenced.
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let from = root.path().join("foo.zip");
    let to = root.path().join("subdir").join("foo.zip");
    set.note_pending(from.clone(), Duration::from_secs(5));
    let summary = EventSummary::RenameBoth { from, to };
    let result = classify_event(&summary, &set, &always_eligible);
    assert_eq!(result, Classification::Suppressed);
}

#[test]
fn rename_both_ineligible_to_is_dropped() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let from = root.path().join("foo.zip");
    let to = root.path().join("foo.zip.crdownload");
    let summary = EventSummary::RenameBoth { from, to };
    let result = classify_event(&summary, &set, &never_eligible);
    assert_eq!(result, Classification::Dropped);
}

#[test]
fn rename_to_alone_emits_when_eligible() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let p = root.path().join("foo.zip");
    let result = classify_event(&EventSummary::RenameTo(p.clone()), &set, &always_eligible);
    assert_eq!(result, Classification::Emit(p));
}

#[test]
fn rename_any_emits_for_the_surviving_endpoint() {
    // Real `is_eligible`, not a stub: `fs::metadata` IS the direction
    // oracle for a `RenameAny`, so stubbing it would test nothing.
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let landed = touch(root.path(), "landed.zip");
    let result = classify_event(&EventSummary::RenameAny(landed.clone()), &set, &is_eligible);
    assert_eq!(result, Classification::Emit(landed));
}

#[test]
fn rename_any_drops_the_vanished_endpoint() {
    // The `from` half of a move-out arrives with the same direction-less
    // kind as the `to` half. It no longer exists, so eligibility drops it.
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let gone = root.path().join("moved-away.zip");
    let result = classify_event(&EventSummary::RenameAny(gone), &set, &is_eligible);
    assert_eq!(result, Classification::Dropped);
}

#[test]
fn rename_any_in_ignore_set_is_suppressed() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let p = touch(root.path(), "cmdr-own.zip");
    set.note_pending(p.clone(), Duration::from_secs(5));
    let result = classify_event(&EventSummary::RenameAny(p), &set, &is_eligible);
    assert_eq!(result, Classification::Suppressed);
}

#[test]
fn other_event_kinds_are_dropped() {
    let root = unhidden_tempdir();
    let set = IgnoreSet::new(root.path().to_path_buf());
    let result = classify_event(&EventSummary::Other, &set, &always_eligible);
    assert_eq!(result, Classification::Dropped);
}

// ── translate_debounced unit tests ───────────────────────────────

#[test]
fn translates_create_event() {
    let p = PathBuf::from("/tmp/foo.zip");
    let ev = make_event(EventKind::Create(notify::event::CreateKind::File), vec![p.clone()]);
    assert_eq!(translate_debounced(&ev), vec![EventSummary::Create(p)]);
}

#[test]
fn translates_rename_both_event() {
    let from = PathBuf::from("/tmp/foo.zip.crdownload");
    let to = PathBuf::from("/tmp/foo.zip");
    let ev = make_event(
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        vec![from.clone(), to.clone()],
    );
    assert_eq!(translate_debounced(&ev), vec![EventSummary::RenameBoth { from, to }]);
}

#[test]
fn translates_rename_to_event() {
    let p = PathBuf::from("/tmp/foo.zip");
    let ev = make_event(EventKind::Modify(ModifyKind::Name(RenameMode::To)), vec![p.clone()]);
    assert_eq!(translate_debounced(&ev), vec![EventSummary::RenameTo(p)]);
}

#[test]
fn translates_rename_any_event() {
    // macOS FSEvents cannot associate a rename's two halves, so every half
    // reaches us as `RenameMode::Any`. Dropping it loses the final path
    // whenever the debouncer fails to pair the halves.
    let p = PathBuf::from("/tmp/foo.zip");
    let ev = make_event(EventKind::Modify(ModifyKind::Name(RenameMode::Any)), vec![p.clone()]);
    assert_eq!(translate_debounced(&ev), vec![EventSummary::RenameAny(p)]);
}

#[test]
fn translates_modify_content_to_other() {
    let p = PathBuf::from("/tmp/foo.zip");
    let ev = make_event(
        EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
        vec![p],
    );
    assert_eq!(translate_debounced(&ev), vec![EventSummary::Other]);
}

#[test]
fn translates_remove_to_other() {
    let p = PathBuf::from("/tmp/foo.zip");
    let ev = make_event(EventKind::Remove(notify::event::RemoveKind::File), vec![p]);
    assert_eq!(translate_debounced(&ev), vec![EventSummary::Other]);
}

#[test]
fn translates_rename_from_alone_to_other() {
    // The matching `RenameTo` carries the final path; `RenameFrom` on
    // its own gives us no actionable info.
    let p = PathBuf::from("/tmp/foo.zip");
    let ev = make_event(EventKind::Modify(ModifyKind::Name(RenameMode::From)), vec![p]);
    assert_eq!(translate_debounced(&ev), vec![EventSummary::Other]);
}

// ── scan_latest_fallback unit tests ──────────────────────────────

#[test]
fn scan_latest_returns_none_for_empty_dir() {
    let td = unhidden_tempdir();
    assert_eq!(scan_latest(td.path()), None);
}

#[test]
fn scan_latest_picks_most_recent() {
    let td = unhidden_tempdir();
    let _a = touch_stamped(td.path(), "a.txt", 1_700_000_000);
    let b = touch_stamped(td.path(), "b.txt", 1_700_000_060);
    let latest = scan_latest(td.path()).unwrap();
    assert_eq!(latest, b);
}

#[test]
fn scan_latest_caps_at_max_depth() {
    // SCAN_MAX_DEPTH is `6` so that browser-style landings like
    // `~/Downloads/Chrome/extracted/a/b/c/file` (5 levels under root) are
    // covered. A file SEVEN levels deep is past the cap and must be
    // ignored even if it's the most recent eligible file in the tree.
    let td = unhidden_tempdir();
    // Shallow file: directly under root.
    let shallow = touch_stamped(td.path(), "shallow.bin", 1_700_000_000);

    // Deep file: 7 levels under root (root/d1/d2/d3/d4/d5/d6/d7/deep.bin).
    let mut deep_dir = td.path().to_path_buf();
    for n in 1..=7 {
        deep_dir.push(format!("d{n}"));
    }
    fs::create_dir_all(&deep_dir).unwrap();
    // Make sure the deep file is newer than the shallow one.
    let deep = touch_stamped(&deep_dir, "deep.bin", 1_700_000_060);

    let latest = scan_latest(td.path()).expect("expected at least the shallow file");
    // The cap means the deep file isn't visited, so the shallow one wins
    // despite being older.
    assert_eq!(latest, shallow, "expected shallow within cap, got {latest:?}");
    assert_ne!(latest, deep, "deep file must be skipped past the cap");
}

#[test]
fn scan_latest_finds_file_within_cap() {
    // Sanity: a file at SCAN_MAX_DEPTH minus a couple levels (browser-typical
    // `Downloads/Chrome/extracted/file.bin`, 3 levels) is found.
    let td = unhidden_tempdir();
    let nested = td.path().join("Chrome").join("extracted");
    fs::create_dir_all(&nested).unwrap();
    let file = nested.join("file.bin");
    fs::write(&file, b"hi").unwrap();
    assert_eq!(scan_latest(td.path()), Some(file));
}

#[test]
fn scan_latest_skips_partial_and_hidden() {
    let td = unhidden_tempdir();
    let real = touch_stamped(td.path(), "real.zip", 1_700_000_000);
    let _partial = touch_stamped(td.path(), "newer.crdownload", 1_700_000_060);
    let _hidden = touch_stamped(td.path(), ".secret", 1_700_000_120);
    let latest = scan_latest(td.path()).unwrap();
    assert_eq!(latest, real);
}

// ── FDA-gate helper ──────────────────────────────────────────────

#[test]
fn starvation_message_separates_a_dead_stream_from_a_dropped_event() {
    // Arming ate the budget: the stream never came up, so the redo loop never
    // really got to run. Blaming delivery here sends the reader the wrong way.
    let never_armed = starvation_message(Duration::from_millis(13_900), 1);
    assert!(never_armed.contains("arming alone ate"), "{never_armed}");
    assert!(never_armed.contains("1 redo attempt(s)"), "{never_armed}");

    // The opposite shape: a live stream that swallowed every event.
    let armed_then_dropped = starvation_message(Duration::from_millis(300), 14);
    assert!(armed_then_dropped.contains("armed in"), "{armed_then_dropped}");
    assert!(
        armed_then_dropped.contains("14 redo attempt(s)"),
        "{armed_then_dropped}"
    );

    // Both name the host-wide suspect, which is what stops the next reader
    // from bisecting Cmdr for a sick daemon.
    for diagnostic in [&never_armed, &armed_then_dropped] {
        assert!(diagnostic.contains("fseventsd"), "{diagnostic}");
    }
}

#[test]
fn desired_running_mirrors_fda_gate() {
    assert!(desired_running(false), "open gate -> should run");
    assert!(!desired_running(true), "closed gate -> should not run");
}

// ── The one test against a real `notify` watcher ─────────────────

fn wait_for(
    rx: &mpsc::Receiver<DownloadDetectedEvent>,
    timeout: Duration,
    predicate: impl Fn(&DownloadDetectedEvent) -> bool,
) -> Option<DownloadDetectedEvent> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match rx.recv_timeout(remaining) {
            Ok(ev) => {
                if predicate(&ev) {
                    return Some(ev);
                }
            }
            Err(_) => return None,
        }
    }
}

/// Longer than the 200 ms debounce + filesystem-flush slack. Real macOS
/// `notify`/FSEvents delivery lags seconds under load, so wait generously.
/// Kept below the 20 s nextest cap these integration tests get in
/// `.config/nextest.toml` so the recv fails cleanly here instead of nextest
/// SIGTERM-ing the whole process at the tight global 8 s cap.
const EVENT_TIMEOUT: Duration = Duration::from_secs(15);

/// What a starved run reports, split so the reader knows WHICH half of the
/// budget went missing.
///
/// Arming and the redo loop share one `EVENT_TIMEOUT`, so a single "starved"
/// line can't tell "the stream never came up" from "it came up and then
/// every event was dropped" — two very different causes. Naming the arming
/// cost and the attempts it left is what turns the next failure into a read
/// rather than an instrumentation session.
///
/// The fseventsd pointer is here because the daemon is host-wide and its bad
/// state looks exactly like a Cmdr bug from inside the test: on a healthy Mac
/// arming is well under a second, and a wedged fseventsd (pinned CPU,
/// multi-GB RSS) stretched it past 10 s on a fully idle machine.
fn starvation_message(arming: Duration, attempts: u32) -> String {
    let cause = if arming >= EVENT_TIMEOUT / 2 {
        format!("arming alone ate {arming:.1?} of it, leaving {attempts} redo attempt(s)")
    } else {
        format!("the watch armed in {arming:.1?}, then {attempts} redo attempt(s) all went undelivered")
    };
    format!("no download-detected within {EVENT_TIMEOUT:?} (FSEvents watch starved): {cause}. {FSEVENTSD_HINT}")
}

/// Names every file [`prime_watch`] writes.
const PRIME_SENTINEL_PREFIX: &str = "cmdr-prime-sentinel-";

/// Where to look before suspecting this code. Shared by both starvation
/// sites, because the daemon is host-wide and either one can be its victim.
const FSEVENTSD_HINT: &str = "Arming is well under a second on a healthy Mac; if it wasn't here, check fseventsd \
     (`ps aux | grep fseventsd`) before suspecting this code — a daemon pinned at 100% CPU with multi-GB RSS \
     starves every FSEvents delivery on the host, and `sudo killall fseventsd` restarts it.";

/// Drive a real FSEvents-observed mutation to a reliable emit, defeating
/// both the just-registered-watch arming window and FSEvents' habit of
/// coalescing or dropping a lone event under host saturation.
///
/// `Debouncer::watch` returns before macOS finishes arming the FSEvents
/// stream, and a mutation landing inside that arming window is dropped
/// entirely, not merely delayed. Separately, once the stream IS live, a
/// single create/rename can still be coalesced away or dropped when every
/// core is busy (a full-suite run pins all of them). Both are unrecoverable
/// by waiting — the event is gone — so we redo the real mutation on a fresh
/// name until the watch delivers a matching emit, all inside ONE
/// `EVENT_TIMEOUT` budget (kept under the 20 s nextest cap; there's no
/// second budget to stack, which is what blew the cap when a priming step
/// and a long wait were separate).
///
/// `mutate(attempt)` performs the genuine operation for a given attempt
/// (a create, or a partial→final rename). `matches` accepts ANY attempt's
/// emit, not just the latest, so a merely-slow event from an earlier attempt
/// still counts instead of being discarded — which would otherwise turn slow
/// delivery into a spurious failure. Returns the matched event.
///
/// Primes the watch (via [`prime_watch`] on `dir`) before the first real
/// mutation: throwaway creates prove the stream is armed and delivering, so
/// the redo loop below only has to defeat coalescing on a KNOWN-live watch,
/// not gamble on when arming finishes. Priming shares this one `EVENT_TIMEOUT`
/// deadline, so it never stacks a second budget past the 20 s nextest cap.
fn observe_mutation(
    dir: &Path,
    rx: &mpsc::Receiver<DownloadDetectedEvent>,
    mut mutate: impl FnMut(u32),
    matches: impl Fn(&DownloadDetectedEvent) -> bool,
) -> DownloadDetectedEvent {
    let started = Instant::now();
    let deadline = started + EVENT_TIMEOUT;
    prime_watch(dir, rx, deadline);
    let arming = started.elapsed();
    let mut attempt = 0u32;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "{}", starvation_message(arming, attempt));
        mutate(attempt);
        attempt += 1;
        // A live stream delivers within the 200 ms debounce plus slack, so a
        // short per-attempt wait re-triggers a fresh mutation quickly when an
        // event was coalesced/dropped — more redo cycles per budget is the
        // self-heal lever (a lone rename, the most coalesce-prone class,
        // needs several tries under load). Losing nothing by going short:
        // `matches` accepts ANY attempt's emit, so a merely-slow event from
        // an earlier attempt still lands in a later wait. Never exceed the
        // overall deadline.
        let per_attempt = remaining.min(Duration::from_secs(1));
        if let Some(ev) = wait_for(rx, per_attempt, &matches) {
            return ev;
        }
    }
}

/// Prove a freshly-registered FSEvents watch is delivering, then drain the
/// proof events. Called by [`observe_mutation`] before its redo loop, bounded
/// by its `deadline` so priming and the redo loop share one budget and can't
/// stack past the 20 s cap.
///
/// Repeatedly creates a throwaway eligible file (a fresh name each time, so
/// each is a distinct `Create` the sink emits) until one is observed. A
/// rewrite of the same name is a `Modify`, which never emits, so the name
/// must change each iteration. The drain clears only what has ARRIVED: a
/// sentinel can still be delivered after the caller's own file, so a caller
/// matches its events by name, never by order.
fn prime_watch(dir: &Path, rx: &mpsc::Receiver<DownloadDetectedEvent>, deadline: Instant) {
    let mut n = 0u32;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "FSEvents watch never began delivering before the deadline (arming never completed). {FSEVENTSD_HINT}"
        );
        let _ = fs::write(dir.join(format!("{PRIME_SENTINEL_PREFIX}{n}.dat")), b"x");
        n += 1;
        if rx.recv_timeout(remaining.min(Duration::from_millis(500))).is_ok() {
            break;
        }
    }
    // Drain sentinel events already in flight so they can't shadow the
    // caller's real event.
    while rx.try_recv().is_ok() {}
}

/// The one cell that proves the OS end: a real file dropped into a really
/// watched directory reaches the sink, named the way FSEvents names it
/// (canonical, `/private/var/…` for a `/var/…` tempdir). Everything else in
/// this file runs on [`DownloadsWatcher::scripted_at`], because each real watch
/// is a wait on the host-wide `fseventsd`, and under a saturated suite that
/// wait alone can outlast the test.
#[test]
fn dropping_a_file_emits_one_event() {
    let td = unhidden_tempdir();
    let canon_root = td.path().canonicalize().unwrap();
    let (sink, rx) = ChannelSink::new();
    let watcher = DownloadsWatcher::start_at(td.path().to_path_buf(), sink).unwrap();

    // A direct create of a final-form file must emit for that file. Redo the
    // create on a fresh name until the watch delivers one (see
    // `observe_mutation` for why a single create can be lost under load).
    let ev = observe_mutation(
        td.path(),
        &rx,
        |n| {
            touch(td.path(), &format!("drop-{n}.txt"));
        },
        |e| e.file_name.starts_with("drop-") && e.file_name.ends_with(".txt"),
    );
    let expected_path = canon_root.join(&ev.file_name);
    assert_eq!(ev.path, expected_path.to_string_lossy());
    assert_eq!(ev.parent_dir, canon_root.to_string_lossy());
    assert!(!ev.in_subdir);
    assert!(ev.size_bytes.is_some());

    drop(watcher);
}

// ── The pipeline, fed scripted batches ───────────────────────────

/// A `Create` naming `path`: how FSEvents reports a new file.
fn created(path: &Path) -> DebouncedEvent {
    make_event(
        EventKind::Create(notify::event::CreateKind::File),
        vec![path.to_path_buf()],
    )
}

/// One half of a macOS rename. FSEvents can't pair the halves, so each arrives
/// as a direction-less `RenameMode::Any` (see `translates_rename_any_event`).
fn rename_half(path: &Path) -> DebouncedEvent {
    make_event(
        EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
        vec![path.to_path_buf()],
    )
}

/// A scripted watcher on a fresh tempdir, plus the dir's canonical spelling:
/// the one FSEvents reports every path in.
fn scripted() -> (
    TempDir,
    PathBuf,
    DownloadsWatcher,
    mpsc::Receiver<DownloadDetectedEvent>,
) {
    let td = unhidden_tempdir();
    let canon_root = td.path().canonicalize().unwrap();
    let (sink, rx) = ChannelSink::new();
    let watcher = DownloadsWatcher::scripted_at(td.path().to_path_buf(), sink);
    (td, canon_root, watcher, rx)
}

/// File names of everything the sink got so far. `deliver` is synchronous, so
/// this is the whole answer, no waiting.
fn emitted(rx: &mpsc::Receiver<DownloadDetectedEvent>) -> Vec<String> {
    rx.try_iter().map(|e| e.file_name).collect()
}

#[test]
fn partial_rename_to_final_emits_for_final_path() {
    let (_td, root, watcher, rx) = scripted();
    // On disk as a browser leaves it: the partial is gone, the final is there.
    let partial = root.join("dl.zip.crdownload");
    let final_path = root.join("dl.zip");
    fs::write(&partial, b"data").unwrap();
    fs::rename(&partial, &final_path).unwrap();

    watcher.deliver(vec![rename_half(&partial), rename_half(&final_path)]);

    let events: Vec<_> = rx.try_iter().collect();
    assert_eq!(events.len(), 1, "only the final path surfaces: {events:?}");
    assert_eq!(events[0].path, final_path.to_string_lossy());
    assert_eq!(events[0].parent_dir, root.to_string_lossy());
    assert!(!events[0].in_subdir);
}

#[test]
fn note_pending_write_suppresses_matching_event() {
    let (td, root, watcher, rx) = scripted();
    // Registered through the spelling a write op holds (`/var/…` on macOS),
    // reported in the canonical one FSEvents uses: the ignore set has to match
    // across the two.
    watcher.note_pending_write(td.path().join("cmdr-own.txt"), DEFAULT_IGNORE_TTL);
    touch(td.path(), "cmdr-own.txt");
    touch(td.path(), "control.txt");

    watcher.deliver(vec![
        created(&root.join("cmdr-own.txt")),
        created(&root.join("control.txt")),
    ]);

    // The unregistered control surfacing proves the batch was handled, so the
    // own write's silence is suppression, not a watch that saw nothing.
    assert_eq!(emitted(&rx), vec!["control.txt"]);
}

/// Registration matches by path prefix and never asks the filesystem: every
/// write op registers its target, a share's folder included, and resolving a
/// path on a network mount blocked a New Folder on a busy NAS for 12 s
/// (ERR-AREUV). The observable trace of "no resolving": a path reached
/// through a symlink OUTSIDE the root isn't registered under the root.
#[test]
fn note_pending_write_never_resolves_the_path() {
    let td = unhidden_tempdir();
    let root = td.path().join("Downloads");
    fs::create_dir(&root).unwrap();
    let alias = td.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let (sink, _rx) = ChannelSink::new();
    let watcher = DownloadsWatcher::scripted_at(root.clone(), sink);

    watcher.note_pending_write(alias.join("new"), Duration::from_secs(60));

    assert!(
        !watcher.is_pending_write(&root.join("new")),
        "registration resolved the symlinked parent, a filesystem call per write"
    );
}

#[test]
fn latest_download_returns_ring_value_after_event() {
    let (td, root, watcher, rx) = scripted();
    touch(td.path(), "ring.txt");

    watcher.deliver(vec![created(&root.join("ring.txt"))]);

    // The emit and the ring agree, and "go to latest" reads the path exactly as
    // reported. That it's canonical is the OS's half, pinned by the real
    // `dropping_a_file_emits_one_event`.
    assert_eq!(emitted(&rx), vec!["ring.txt"]);
    assert_eq!(watcher.latest_download(), Some(root.join("ring.txt")));
}

/// Regression (ERR-CFHX6): a download renamed away after landing stayed the
/// ring's latest, so ⌃⌥⌘J revealed a name the pane listing no longer held and
/// the FE's cursor move threw. The newest entry still on disk wins instead, and
/// with none left the lookup falls through to the scan.
#[test]
fn latest_download_skips_a_download_renamed_away_since() {
    let (td, root, watcher, _rx) = scripted();
    touch(td.path(), "older.pdf");
    touch(td.path(), "transaction-history.csv");
    watcher.deliver(vec![created(&root.join("older.pdf"))]);
    watcher.deliver(vec![created(&root.join("transaction-history.csv"))]);
    assert_eq!(watcher.latest_download(), Some(root.join("transaction-history.csv")));

    fs::rename(
        root.join("transaction-history.csv"),
        root.join("transaction-history.csv.old"),
    )
    .unwrap();
    assert_eq!(watcher.latest_download(), Some(root.join("older.pdf")));

    fs::remove_file(root.join("older.pdf")).unwrap();
    assert_eq!(
        watcher.latest_download(),
        None,
        "every ring entry is gone, so the scan decides"
    );
}

#[test]
fn scan_latest_fallback_finds_file_with_no_events() {
    let td = unhidden_tempdir();
    let canon_root = td.path().canonicalize().unwrap();
    touch(td.path(), "exists.txt");
    let (sink, _rx) = ChannelSink::new();
    let watcher = DownloadsWatcher::scripted_at(td.path().to_path_buf(), sink);

    // Ring is empty; the fallback finds the file under the canonical root the
    // watcher resolved at start.
    assert_eq!(watcher.latest_download(), None);
    assert_eq!(watcher.scan_latest_fallback(), Some(canon_root.join("exists.txt")));
}

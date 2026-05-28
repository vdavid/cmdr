//! `~/Downloads` watcher.
//!
//! Recursive `notify` watch (via `notify-debouncer-full`) on the resolved
//! Downloads directory. Filters each debounced event through
//! [`super::is_eligible`] and the [`super::IgnoreSet`] (Cmdr-own writes), then
//! pushes survivors into [`super::LatestRing`] and emits a `download-detected`
//! Tauri event.
//!
//! ## Lifecycle
//!
//! Tied to the FDA gate (`crate::fda_gate::is_fda_pending_runtime`). At
//! startup and on every main-window focus transition, `lib.rs` calls
//! [`refresh_runtime`] which starts the watcher when the gate is open and
//! stops it when the gate closes. The watcher holds no FDA-protected state
//! beyond its `notify` handle; dropping the handle releases the OS watch.
//!
//! ## Event classification
//!
//! `notify_debouncer_full::DebouncedEvent` carries the raw `notify::Event`
//! plus debounce timestamps. We translate each into an [`EventSummary`] then
//! ask [`classify_event`] for the path (if any) to surface. This keeps the
//! decision logic pure and testable without constructing `DebouncedEvent`
//! fixtures.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{
    EventKind, RecommendedWatcher, RecursiveMode,
    event::{ModifyKind, RenameMode},
};
use notify_debouncer_full::{DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::{IgnoreSet, LatestRing, is_eligible};

/// How long an `IgnoreSet` entry lives by default. Browser FS events land
/// within a few hundred ms of the syscall; 5 s is plenty of headroom.
pub const DEFAULT_IGNORE_TTL: Duration = Duration::from_secs(5);

/// `notify-debouncer-full` window. Matches the listing watcher's default
/// (200 ms), small enough that the toast feels prompt but big enough that the
/// rename pair from a browser's `.crdownload` → final dance collapses into
/// one batched call.
const DEBOUNCE_MS: u64 = 200;

/// Payload of the `download-detected` Tauri event.
///
/// `specta::Type` is derived so the TS binding picks the shape up. The type is
/// referenced from `DownloadsWatcherStatus::last_detected` so it lands in
/// `bindings.ts` even though Tauri events themselves aren't on the typed-event
/// surface yet.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDetectedEvent {
    pub path: String,
    pub parent_dir: String,
    pub file_name: String,
    /// Milliseconds since the Unix epoch.
    pub observed_at_ms: u64,
    /// `true` when the file sits in a subdirectory under the Downloads root,
    /// `false` when it's a direct child.
    pub in_subdir: bool,
    /// Best-effort file size. `None` if the stat failed (file already gone,
    /// permission denied, etc.).
    pub size_bytes: Option<u64>,
}

/// Errors when starting the watcher.
#[derive(Debug)]
pub enum WatcherError {
    /// `notify-debouncer-full` couldn't build a debouncer.
    Debouncer(notify::Error),
    /// `Debouncer::watch` failed to attach to the resolved Downloads dir.
    Watch(notify::Error),
}

impl std::fmt::Display for WatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debouncer(e) => write!(f, "Failed to create downloads debouncer: {e}"),
            Self::Watch(e) => write!(f, "Failed to watch Downloads dir: {e}"),
        }
    }
}

impl std::error::Error for WatcherError {}

/// Resolve the Downloads directory: `dirs::download_dir()` with a `$HOME/Downloads` fallback.
///
/// Returns `None` if neither lookup succeeds (no `HOME`, no XDG dir, etc.).
pub fn resolved_downloads_dir() -> Option<PathBuf> {
    dirs::download_dir().or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
}

/// Sink for `download-detected` events. Production uses [`AppHandleSink`];
/// tests use an mpsc-backed sink so they don't need a running Tauri app.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: DownloadDetectedEvent);
}

/// `AppHandle`-backed sink. Forwards each event to the main window via
/// `Emitter::emit`.
pub struct AppHandleSink {
    app: AppHandle,
}

impl AppHandleSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for AppHandleSink {
    fn emit(&self, event: DownloadDetectedEvent) {
        if let Err(err) = self.app.emit("download-detected", &event) {
            log::warn!(
                target: "downloads::watcher",
                "Failed to emit download-detected event: {err}",
            );
        }
    }
}

/// Internal classifier input. One per `notify::Event` after we collapse
/// `paths` and the `kind` into the shape `classify_event` cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EventSummary {
    /// File or final-form create. `notify::EventKind::Create(_)`.
    Create(PathBuf),
    /// Rename carrying both endpoints in one event (`RenameMode::Both`).
    RenameBoth { from: PathBuf, to: PathBuf },
    /// Half a rename pair (`RenameMode::To`). We only act on the `To`
    /// variant; debouncing usually upgrades this to `RenameBoth`, but on
    /// systems where it doesn't, the `To` half still carries the final-form
    /// path.
    RenameTo(PathBuf),
    /// Anything we deliberately drop: modify-content, attribute changes,
    /// access, removes, `RenameFrom` alone, etc. Carried for tests but never
    /// emits.
    Other,
}

/// What [`classify_event`] decided to surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Classification {
    /// Surface this path (already eligible AND not ignored).
    Emit(PathBuf),
    /// Suppress: matched the ignore set, on either half of a rename or a
    /// direct create.
    Suppressed,
    /// Dropped: ineligible (hidden, partial suffix, directory) or unhandled
    /// event kind. No toast, no log.
    Dropped,
}

/// Decide what to do with one classified event.
///
/// Pure function: takes a pre-computed [`EventSummary`] and the ignore set,
/// plus an `is_eligible_fn` so tests can inject a stub that doesn't touch
/// the disk. The default caller passes [`is_eligible`].
pub(crate) fn classify_event(
    summary: &EventSummary,
    ignore_set: &IgnoreSet,
    is_eligible_fn: &dyn Fn(&Path) -> bool,
) -> Classification {
    match summary {
        EventSummary::Create(path) => {
            if ignore_set.is_pending(path) {
                return Classification::Suppressed;
            }
            if is_eligible_fn(path) {
                Classification::Emit(path.clone())
            } else {
                Classification::Dropped
            }
        }
        EventSummary::RenameBoth { from, to } => {
            // If either endpoint is in the ignore set, this rename came from
            // Cmdr (own write to a final path, or own move-out). Drop both
            // halves silently.
            if ignore_set.is_pending(from) || ignore_set.is_pending(to) {
                return Classification::Suppressed;
            }
            if is_eligible_fn(to) {
                Classification::Emit(to.clone())
            } else {
                Classification::Dropped
            }
        }
        EventSummary::RenameTo(path) => {
            if ignore_set.is_pending(path) {
                return Classification::Suppressed;
            }
            if is_eligible_fn(path) {
                Classification::Emit(path.clone())
            } else {
                Classification::Dropped
            }
        }
        EventSummary::Other => Classification::Dropped,
    }
}

/// Translate one `DebouncedEvent` into zero or more [`EventSummary`]s.
///
/// `notify` emits one event per filesystem operation. For renames the
/// debouncer usually pairs them (`RenameMode::Both` with two paths), but not
/// always: we fall back to per-half summaries. The output preserves
/// multiplicity (a single `Create` with two paths becomes two summaries).
pub(crate) fn translate_debounced(event: &DebouncedEvent) -> Vec<EventSummary> {
    match &event.kind {
        EventKind::Create(_) => event.paths.iter().cloned().map(EventSummary::Create).collect(),
        EventKind::Modify(ModifyKind::Name(mode)) => match mode {
            RenameMode::Both if event.paths.len() >= 2 => {
                vec![EventSummary::RenameBoth {
                    from: event.paths[0].clone(),
                    to: event.paths[1].clone(),
                }]
            }
            RenameMode::To => event.paths.iter().cloned().map(EventSummary::RenameTo).collect(),
            // RenameFrom alone, or RenameAny / Other — drop. We act on the
            // `To` (or the paired `Both`) for the final-form path.
            _ => vec![EventSummary::Other],
        },
        // Modify-content, attribute changes, access, removes, etc.
        _ => vec![EventSummary::Other],
    }
}

/// Handle to the running watcher. Drop to stop watching.
pub struct DownloadsWatcher {
    // Held to keep the OS watch alive; never read directly.
    #[allow(dead_code, reason = "Debouncer must outlive the watcher to keep notify alive")]
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    ignore_set: Arc<IgnoreSet>,
    latest_ring: Arc<LatestRing>,
    downloads_root: PathBuf,
}

impl DownloadsWatcher {
    /// Start a watcher on the user's resolved Downloads directory and emit
    /// events through `app`. Returns `Err(WatcherError)` if `notify` couldn't
    /// attach (missing dir, permission denied, etc.).
    pub fn start(app: &AppHandle) -> Result<Self, WatcherError> {
        let root = resolved_downloads_dir().unwrap_or_else(|| PathBuf::from("/tmp/cmdr-downloads-missing"));
        let sink: Arc<dyn EventSink> = Arc::new(AppHandleSink::new(app.clone()));
        Self::start_at(root, sink)
    }

    /// Test-friendly constructor: watch `downloads_root` and route events to
    /// `sink`. Production code uses [`Self::start`].
    ///
    /// `downloads_root` is canonicalized so it matches the paths `notify`
    /// reports (macOS FSEvents resolves firmlinks: `/var/...` →
    /// `/private/var/...`). Without canonicalization, ignore-set lookups
    /// and `parent_dir == downloads_root` checks would compare a
    /// user-facing path against a canonical one and silently miss.
    pub fn start_at(downloads_root: PathBuf, sink: Arc<dyn EventSink>) -> Result<Self, WatcherError> {
        let downloads_root = std::fs::canonicalize(&downloads_root).unwrap_or(downloads_root);
        let ignore_set = Arc::new(IgnoreSet::new(downloads_root.clone()));
        let latest_ring = Arc::new(LatestRing::new());

        let ignore_for_cb = Arc::clone(&ignore_set);
        let ring_for_cb = Arc::clone(&latest_ring);
        let root_for_cb = downloads_root.clone();
        let sink_for_cb = Arc::clone(&sink);

        let mut debouncer = new_debouncer(
            Duration::from_millis(DEBOUNCE_MS),
            None,
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    handle_events(
                        &events,
                        &ignore_for_cb,
                        &ring_for_cb,
                        &root_for_cb,
                        sink_for_cb.as_ref(),
                    );
                }
                Err(errors) => {
                    for err in errors {
                        log::warn!(target: "downloads::watcher", "Watch error: {err}");
                    }
                }
            },
        )
        .map_err(WatcherError::Debouncer)?;

        debouncer
            .watch(&downloads_root, RecursiveMode::Recursive)
            .map_err(WatcherError::Watch)?;

        log::info!(
            target: "downloads::watcher",
            "Started watching Downloads at {}",
            downloads_root.display(),
        );

        Ok(Self {
            debouncer,
            ignore_set,
            latest_ring,
            downloads_root,
        })
    }

    /// Stop watching. Equivalent to dropping the handle; explicit version
    /// exists so call sites can be obvious about lifecycle.
    pub fn stop(self) {
        log::info!(
            target: "downloads::watcher",
            "Stopped watching Downloads at {}",
            self.downloads_root.display(),
        );
        // `self` drops here; debouncer drop releases the OS watch.
    }

    /// Register a Cmdr-own pending write so its FS event gets suppressed.
    /// Silently no-ops for paths outside the watched Downloads root.
    ///
    /// The path is canonicalized via its parent directory so it matches the
    /// shape `notify` reports (macOS resolves firmlinks like
    /// `/var/folders/...` → `/private/var/folders/...`). The file leaf may
    /// not exist yet — that's the whole point of the pre-write hook — so
    /// canonicalization happens at parent-dir granularity.
    pub fn note_pending_write(&self, path: PathBuf, ttl: Duration) {
        self.ignore_set.note_pending(canonicalize_for_match(&path), ttl);
    }

    /// Bulk version of [`Self::note_pending_write`]. Reserved for future
    /// call sites with a known full destination list (transfer driver,
    /// etc.); the per-file [`Self::note_pending_write`] is what M3 wires.
    #[allow(
        dead_code,
        reason = "Hook contract surface; per-file note_pending_write is what's wired today"
    )]
    pub fn note_pending_writes(&self, paths: Vec<PathBuf>, ttl: Duration) {
        let canonical: Vec<PathBuf> = paths.iter().map(|p| canonicalize_for_match(p)).collect();
        self.ignore_set.note_pending_batch(canonical, ttl);
    }

    /// Most-recently observed eligible download, or `None` if the ring is
    /// empty. The "reveal latest download" action reads this first; if `None`
    /// it falls back to [`Self::scan_latest_fallback`].
    pub fn latest_download(&self) -> Option<PathBuf> {
        self.latest_ring.latest()
    }

    /// Scan the Downloads dir recursively for the most-recently modified
    /// eligible file. O(N) over the dir contents; called only when the ring
    /// is empty (cold start before any event has arrived).
    pub fn scan_latest_fallback(&self) -> Option<PathBuf> {
        scan_latest(&self.downloads_root)
    }

    /// Resolved Downloads root the watcher was started against.
    #[allow(
        dead_code,
        reason = "Exposed for the Settings pane / debug surfaces; M2b leaves it unconsumed"
    )]
    pub fn downloads_root(&self) -> &Path {
        &self.downloads_root
    }
}

/// Process a batch of debounced events. Pulled out so the callback closure
/// stays small.
fn handle_events(
    events: &[DebouncedEvent],
    ignore_set: &IgnoreSet,
    latest_ring: &LatestRing,
    downloads_root: &Path,
    sink: &dyn EventSink,
) {
    for raw in events {
        for summary in translate_debounced(raw) {
            match classify_event(&summary, ignore_set, &is_eligible) {
                Classification::Emit(path) => {
                    let observed = Instant::now();
                    latest_ring.push(path.clone(), observed);
                    let payload = build_payload(&path, downloads_root);
                    log::debug!(
                        target: "downloads::watcher",
                        "Emitting download-detected for {} (in_subdir={})",
                        payload.path,
                        payload.in_subdir,
                    );
                    sink.emit(payload);
                }
                Classification::Suppressed => {
                    log::debug!(
                        target: "downloads::watcher",
                        "Suppressed event for {:?} (Cmdr-own write or move)",
                        summary,
                    );
                }
                Classification::Dropped => {}
            }
        }
    }
}

fn build_payload(path: &Path, downloads_root: &Path) -> DownloadDetectedEvent {
    let parent_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let in_subdir = path.parent().is_some_and(|p| p != downloads_root);
    let size_bytes = std::fs::metadata(path).ok().map(|m| m.len());
    let observed_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    DownloadDetectedEvent {
        path: path.to_string_lossy().to_string(),
        parent_dir,
        file_name,
        observed_at_ms,
        in_subdir,
        size_bytes,
    }
}

/// Walk `root` recursively and return the path with the greatest mtime among
/// eligible files. `None` if no eligible file is found or `root` is missing.
fn scan_latest(root: &Path) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, SystemTime)> = None;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !is_eligible(path) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        match &best {
            None => best = Some((path.to_path_buf(), mtime)),
            Some((_, best_mt)) if mtime > *best_mt => best = Some((path.to_path_buf(), mtime)),
            _ => {}
        }
    }
    best.map(|(p, _)| p)
}

/// Canonicalize `path` so its prefix matches the canonicalized
/// `downloads_root` used internally. `notify` reports the canonical form
/// on macOS (firmlinks `/var/folders/...` → `/private/var/folders/...`),
/// so a hook caller's un-canonicalized path would silently drop on the
/// ignore set's prefix check.
///
/// The file leaf may not exist yet (the hook fires before the syscall), so
/// we canonicalize the parent and rejoin the leaf. If canonicalization of
/// the parent fails — missing dir, broken symlink, permission denied — we
/// return the original path unchanged; the worst case is a one-off
/// false-positive toast for a Cmdr-own write.
fn canonicalize_for_match(path: &Path) -> PathBuf {
    let Some(parent) = path.parent() else {
        return path.to_path_buf();
    };
    let Some(name) = path.file_name() else {
        return path.to_path_buf();
    };
    match std::fs::canonicalize(parent) {
        Ok(canon_parent) => canon_parent.join(name),
        Err(_) => path.to_path_buf(),
    }
}

/// Pure helper: decide whether the watcher should be running given the FDA
/// gate's state. Extracted for unit testing without a Tauri runtime.
///
/// Returns `true` when the gate is open (`pending == false`); `false`
/// otherwise. Callers compare this against whether the watcher is currently
/// alive and start/stop accordingly.
pub fn desired_running(fda_pending: bool) -> bool {
    !fda_pending
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::sync::Mutex;
    use std::sync::mpsc;
    use std::thread;

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
        let _a = touch(td.path(), "a.txt");
        thread::sleep(Duration::from_millis(20));
        let b = touch(td.path(), "b.txt");
        let latest = scan_latest(td.path()).unwrap();
        assert_eq!(latest, b);
    }

    #[test]
    fn scan_latest_skips_partial_and_hidden() {
        let td = unhidden_tempdir();
        let real = touch(td.path(), "real.zip");
        thread::sleep(Duration::from_millis(20));
        let _partial = touch(td.path(), "newer.crdownload");
        thread::sleep(Duration::from_millis(20));
        let _hidden = touch(td.path(), ".secret");
        let latest = scan_latest(td.path()).unwrap();
        assert_eq!(latest, real);
    }

    // ── FDA-gate helper ──────────────────────────────────────────────

    #[test]
    fn desired_running_mirrors_fda_gate() {
        assert!(desired_running(false), "open gate -> should run");
        assert!(!desired_running(true), "closed gate -> should not run");
    }

    // ── Integration tests against a real `notify` watcher ────────────

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

    fn expect_silence(
        rx: &mpsc::Receiver<DownloadDetectedEvent>,
        timeout: Duration,
    ) -> Result<(), DownloadDetectedEvent> {
        match rx.recv_timeout(timeout) {
            Ok(ev) => Err(ev),
            Err(_) => Ok(()),
        }
    }

    /// Longer than the 200 ms debounce + filesystem-flush slack. CI macOS
    /// FSEvents can lag a few seconds; choose a generous default.
    const EVENT_TIMEOUT: Duration = Duration::from_secs(8);

    #[test]
    fn dropping_a_file_emits_one_event() {
        let td = unhidden_tempdir();
        let canon_root = td.path().canonicalize().unwrap();
        let (sink, rx) = ChannelSink::new();
        let watcher = DownloadsWatcher::start_at(td.path().to_path_buf(), sink).unwrap();

        touch(td.path(), "foo.txt");
        let ev =
            wait_for(&rx, EVENT_TIMEOUT, |e| e.file_name == "foo.txt").expect("expected one download-detected event");
        let expected_path = canon_root.join("foo.txt");
        assert_eq!(ev.path, expected_path.to_string_lossy());
        assert_eq!(ev.parent_dir, canon_root.to_string_lossy());
        assert!(!ev.in_subdir);
        assert!(ev.size_bytes.is_some());

        drop(watcher);
    }

    #[test]
    fn partial_rename_to_final_emits_for_final_path() {
        let td = unhidden_tempdir();
        let canon_root = td.path().canonicalize().unwrap();
        let (sink, rx) = ChannelSink::new();
        let watcher = DownloadsWatcher::start_at(td.path().to_path_buf(), sink).unwrap();

        let partial = td.path().join("foo.zip.crdownload");
        let final_path = td.path().join("foo.zip");
        fs::write(&partial, b"data").unwrap();
        // Let the create event flush before the rename. Without this, some
        // platforms coalesce both into a single ambiguous event.
        thread::sleep(Duration::from_millis(400));
        fs::rename(&partial, &final_path).unwrap();

        let ev = wait_for(&rx, EVENT_TIMEOUT, |e| e.file_name == "foo.zip")
            .expect("expected download-detected for the renamed final-path");
        let expected_final = canon_root.join("foo.zip");
        assert_eq!(ev.path, expected_final.to_string_lossy());

        drop(watcher);
    }

    #[test]
    fn note_pending_write_suppresses_matching_event() {
        let td = unhidden_tempdir();
        let (sink, rx) = ChannelSink::new();
        let watcher = DownloadsWatcher::start_at(td.path().to_path_buf(), sink).unwrap();

        let p = td.path().join("cmdr-own.txt");
        watcher.note_pending_write(p.clone(), Duration::from_secs(5));
        fs::write(&p, b"hi").unwrap();

        // Wait long enough that any normal event would have arrived.
        if let Err(unexpected) = expect_silence(&rx, Duration::from_secs(2)) {
            panic!("expected silence, got event: {unexpected:?}");
        }

        drop(watcher);
    }

    #[test]
    fn latest_download_returns_ring_value_after_event() {
        let td = unhidden_tempdir();
        let canon_root = td.path().canonicalize().unwrap();
        let (sink, rx) = ChannelSink::new();
        let watcher = DownloadsWatcher::start_at(td.path().to_path_buf(), sink).unwrap();

        touch(td.path(), "ring.txt");
        let _ =
            wait_for(&rx, EVENT_TIMEOUT, |e| e.file_name == "ring.txt").expect("expected event before checking ring");

        assert_eq!(watcher.latest_download(), Some(canon_root.join("ring.txt")));
        drop(watcher);
    }

    #[test]
    fn scan_latest_fallback_finds_file_with_no_events() {
        let td = unhidden_tempdir();
        let canon_root = td.path().canonicalize().unwrap();
        // Drop a file BEFORE starting the watcher so its mtime is set.
        touch(td.path(), "exists.txt");

        let (sink, _rx) = ChannelSink::new();
        let watcher = DownloadsWatcher::start_at(td.path().to_path_buf(), sink).unwrap();

        // Ring is empty; fallback should find the file (under the canonical
        // root the watcher resolved during start).
        assert_eq!(watcher.latest_download(), None);
        assert_eq!(watcher.scan_latest_fallback(), Some(canon_root.join("exists.txt")));

        drop(watcher);
    }
}

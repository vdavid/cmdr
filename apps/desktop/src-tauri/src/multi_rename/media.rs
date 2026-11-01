//! Media fields for a session: what each file's EXIF and tags say, read once in
//! the background and cached for the session's life, and what the sheet asks of
//! that cache (progress, the picker's overview, a token's samples).
//!
//! `session.rs` owns one [`SessionMedia`] per session and hands every preview a
//! copy of it as the [`MediaLookup`]. ❌ Nothing here takes `SESSIONS`, no file is
//! read under any lock, and the cache's own lock is held only for short steps:
//! storing an answer, copying the cache out, or resolving a few sample rows.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use tauri_specta::Event;
use unicode_normalization::UnicodeNormalization;

use cmdr_fs::deadline::Deadline;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_private::LogPrivate;
use cmdr_media_meta::{MediaFacts, MediaField, ReadError, VolumeByteSource, is_media_name, read_media_from};

use super::mask::{FieldGap, FieldText, MediaToken, RowFacts, RowMedia};
use super::plan::{MediaLookup, folder_names};
use crate::ignore_poison::IgnorePoison;

const LOG_TARGET: &str = "multi_rename::media";

/// How many files the loader reads at once. A NAS answers several requests in
/// parallel; MTP serializes inside its own backend, so no per-volume tuning.
pub(crate) const MAX_PARALLEL_READS: usize = 8;

/// The least time between two progress events; a run's last one always goes.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// How many reads in a row may time out or fail on I/O before the loader takes
/// the volume for dead and marks every file left `Unreadable`.
const GIVE_UP_AFTER_FAILURES: usize = 3;

/// One file's read budget on a local disk; a volume with a live session gets
/// more (`io_budget_for_volume`).
const READ_BUDGET: Duration = Duration::from_secs(10);

/// How far a session's media read got. `total` counts media files only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaProgress {
    pub read: usize,
    pub total: usize,
    pub done: bool,
}

/// The `multi-rename-media-progress` event: a session's read moved on. At most
/// every 250 ms, plus a final one with `done`. ❗ The struct name kebab-cases to
/// the wire event name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameMediaProgress {
    pub session_id: String,
    pub read: usize,
    pub total: usize,
    pub done: bool,
}

/// What the field picker shows: the read's progress, and each field's example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaOverview {
    pub progress: MediaProgress,
    /// Every field, in `MediaField::ALL` order.
    pub fields: Vec<MediaFieldOverview>,
}

/// One field in the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaFieldOverview {
    pub field: MediaField,
    /// The field in its default format, from the first file in rename order that
    /// has it; `None` when no file read so far does.
    pub example: Option<String>,
    /// How many files read so far have it.
    pub files_with_it: usize,
}

/// What one `[=source.field…]` placeholder renders alone on the token editor's
/// sample rows, and how it fares across the whole session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldSamples {
    pub samples: Vec<FieldSample>,
    pub usage: FieldUsage,
}

/// The placeholder on one row: its text, or why it has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldSample {
    /// The file's place in the session (what `PreviewRow::row` names).
    pub row: usize,
    pub text: Option<String>,
    pub gap: Option<FieldGap>,
}

/// How the session's files fare with one placeholder, for "38 files use the
/// modified date".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldUsage {
    /// Files with the field's own value.
    pub with_value: usize,
    /// Files without it that take the `|modified` fallback.
    pub fallback: usize,
    /// Files without it and with no fallback: problem rows.
    pub missing: usize,
    /// Files still being read.
    pub reading: usize,
}

/// One file the loader reads.
pub(crate) struct FileJob {
    /// Where it is on the session's volume.
    pub path: PathBuf,
    /// Its name, which picks the parser.
    pub name: String,
    /// Its size from the listing (`0` when the listing has none).
    pub size: u64,
    /// Set when the session goes: a read stops at its next fetch.
    pub cancel: Arc<AtomicBool>,
    /// The listing's facts the read was for, so a file that changed meanwhile is
    /// read again rather than cached with the old answer.
    stamp: Stamp,
}

/// Reads one file's facts. Runs on a blocking thread.
pub(crate) type FileReader = Arc<dyn Fn(&FileJob) -> Result<MediaFacts, ReadError> + Send + Sync>;

/// Hands a progress event to the sheet.
pub(crate) type ProgressEmit = Arc<dyn Fn(MultiRenameMediaProgress) + Send + Sync>;

/// What a session's loader reads with and reports to.
pub(crate) struct Loader {
    pub reader: FileReader,
    pub emit: ProgressEmit,
    /// How many files at once.
    pub parallel: usize,
    /// Whether the volume went away (unregistered or not connected): the loader
    /// asks before each read and gives up on every file left when it has.
    pub volume_gone: VolumeGone,
}

/// Whether a session's volume can no longer answer.
pub(crate) type VolumeGone = Arc<dyn Fn() -> bool + Send + Sync>;

impl Loader {
    /// Reads files on the volume `volume_id` (resolved per file, so a reconnect's
    /// new volume is the one read) and reports through `emit`.
    pub(crate) fn for_volume(volume_id: String, emit: ProgressEmit) -> Self {
        Self {
            reader: volume_reader(volume_id.clone()),
            emit,
            parallel: MAX_PARALLEL_READS,
            volume_gone: Arc::new(move || {
                crate::file_system::volume::manager::get_volume_manager()
                    .get(&volume_id)
                    .is_none_or(|volume| volume.connection_state().is_some_and(|state| !state.is_live()))
            }),
        }
    }
}

/// The real [`FileReader`]: `read_media_from` over the file's bytes through
/// `Volume::read_range` (64 KiB blocks, cover art never fetched), within one
/// file's budget. ❗ Builds on the runtime that
/// calls it, so call it on the async executor.
fn volume_reader(volume_id: String) -> FileReader {
    let handle = tokio::runtime::Handle::current();
    Arc::new(move |job: &FileJob| {
        let volume = crate::file_system::volume::manager::get_volume_manager()
            .get(&volume_id)
            .ok_or_else(|| ReadError::Io("the session's volume isn't registered".to_string()))?;
        let budget = crate::file_system::volume::io_budget_for_volume(&volume_id, READ_BUDGET);
        let source = VolumeByteSource::new(
            volume,
            job.path.clone(),
            job.size,
            handle.clone(),
            Deadline::new(budget),
            Arc::clone(&job.cancel),
        );
        read_media_from(source, &job.name)
    })
}

/// What the listing said about a file when it was read: a different size or
/// date means it changed, and its facts are stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: Option<u64>,
    modified: Option<u64>,
}

impl Stamp {
    fn of(entry: &FileEntry) -> Self {
        Self {
            size: entry.size,
            modified: entry.modified_at,
        }
    }
}

/// Where one file's read stands.
#[derive(Debug, Clone)]
pub(crate) enum MediaRead {
    Reading,
    Read(Arc<MediaFacts>),
    /// Why is in the log; the row only says it couldn't be read.
    Unreadable,
    /// A folder, or a file whose type isn't one we read.
    NotMedia,
}

impl MediaRead {
    fn row_media(&self) -> RowMedia<'_> {
        match self {
            Self::Reading => RowMedia::Reading,
            Self::Read(facts) => RowMedia::Read(facts),
            Self::Unreadable => RowMedia::Unreadable,
            Self::NotMedia => RowMedia::NotMedia,
        }
    }

    fn is_media(&self) -> bool {
        !matches!(self, Self::NotMedia)
    }

    fn is_reading(&self) -> bool {
        matches!(self, Self::Reading)
    }
}

/// One file in the cache.
#[derive(Clone)]
struct Cached {
    is_directory: bool,
    stamp: Stamp,
    state: MediaRead,
}

/// The cache and the loader's bookkeeping, behind one lock. ❗ Held only for
/// short steps: a preview works on a copy ([`SessionMedia::lookup`]), and the
/// overview and a token's usage copy out what they need first.
struct Inner {
    files: HashMap<Arc<str>, Cached>,
    /// Files waiting to be read, the next one first.
    queue: VecDeque<Arc<str>>,
    /// Files a reader holds now.
    in_flight: HashSet<Arc<str>>,
    /// Media files, and how many of them are no longer `Reading`.
    total: usize,
    read: usize,
    /// Bumped on every change to `files`, so a remembered usage knows it's stale.
    generation: u64,
    /// Set by `start`; a loader can only run once it's there.
    loader: Option<(Arc<Loader>, tokio::runtime::Handle)>,
    running: bool,
}

impl Inner {
    /// Sets `name`'s state, keeping the counts in step.
    fn set_state(&mut self, name: &str, state: MediaRead) {
        let Some(cached) = self.files.get_mut(name) else {
            return;
        };
        let was = std::mem::replace(&mut cached.state, state);
        let now = &cached.state;
        self.total = self.total + usize::from(now.is_media()) - usize::from(was.is_media());
        let done = |state: &MediaRead| state.is_media() && !state.is_reading();
        self.read = self.read + usize::from(done(now)) - usize::from(done(&was));
        self.generation += 1;
    }

    fn progress(&self) -> MediaProgress {
        MediaProgress {
            read: self.read,
            total: self.total,
            done: self.read == self.total,
        }
    }

    fn is_waiting(&self, name: &str) -> bool {
        self.files.get(name).is_some_and(|c| c.state.is_reading()) && !self.in_flight.contains(name)
    }

    /// The next file to read, marked in flight; `None` when there's none or the
    /// session is going.
    fn next_job(&mut self, dir: &Path, cancel: &Arc<AtomicBool>) -> Option<FileJob> {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        while let Some(name) = self.queue.pop_front() {
            if !self.is_waiting(&name) {
                continue;
            }
            let stamp = self.files[&name].stamp;
            self.in_flight.insert(Arc::clone(&name));
            return Some(FileJob {
                path: dir.join(&*name),
                size: stamp.size.unwrap_or(0),
                name: name.to_string(),
                cancel: Arc::clone(cancel),
                stamp,
            });
        }
        None
    }

    /// Marks every file still waiting `Unreadable` and empties the queue; the
    /// reads in flight finish on their own. Answers how many it marked.
    fn give_up_waiting(&mut self) -> usize {
        let queued: Vec<Arc<str>> = self.queue.drain(..).collect();
        let mut marked = 0;
        for name in queued {
            if self.is_waiting(&name) {
                self.set_state(&name, MediaRead::Unreadable);
                marked += 1;
            }
        }
        marked
    }
}

/// The last usage `field_samples` counted: for which token, at which cache
/// generation. A keystroke in the token editor that doesn't change the token,
/// with nothing read since, answers from here instead of walking every file.
struct RememberedUsage {
    token: MediaToken,
    generation: u64,
    usage: FieldUsage,
}

/// One session's media: every file's read, the queue, and the loader's state.
pub(crate) struct SessionMedia {
    session_id: String,
    dir: PathBuf,
    cancel: Arc<AtomicBool>,
    inner: Mutex<Inner>,
    usage: Mutex<Option<RememberedUsage>>,
}

impl SessionMedia {
    /// The cache for the files `names` (in rename order) of the folder at `dir`,
    /// whose listing entries are `entries`: a media file is `Reading` and queued in
    /// rename order, a folder or any other file `NotMedia`. Nothing reads until
    /// [`start`](Self::start).
    pub(crate) fn seed(session_id: &str, dir: PathBuf, names: &[String], entries: &[FileEntry]) -> Self {
        let by_name: HashMap<&str, &FileEntry> = entries.iter().map(|e| (e.name.as_str(), e)).collect();
        let mut files = HashMap::with_capacity(names.len());
        let mut queue = VecDeque::new();
        for name in names {
            let Some(entry) = by_name.get(name.as_str()) else {
                // Gone already: the preview shows it `Missing` and never asks.
                continue;
            };
            let name: Arc<str> = Arc::from(name.as_str());
            let state = if !entry.is_directory && is_media_name(&name) {
                queue.push_back(Arc::clone(&name));
                MediaRead::Reading
            } else {
                MediaRead::NotMedia
            };
            files.insert(
                name,
                Cached {
                    is_directory: entry.is_directory,
                    stamp: Stamp::of(entry),
                    state,
                },
            );
        }
        let total = queue.len();
        Self {
            session_id: session_id.to_string(),
            dir,
            cancel: Arc::default(),
            inner: Mutex::new(Inner {
                files,
                queue,
                in_flight: HashSet::new(),
                total,
                read: 0,
                generation: 0,
                loader: None,
                running: false,
            }),
            usage: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock_ignore_poison()
    }

    /// How far the read got.
    pub(crate) fn progress(&self) -> MediaProgress {
        self.lock().progress()
    }

    /// Starts reading with `loader`, on the runtime that calls this. A second
    /// call keeps the first loader.
    pub(crate) fn start(self: &Arc<Self>, loader: Loader) {
        let mut inner = self.lock();
        if inner.loader.is_none() {
            inner.loader = Some((Arc::new(loader), tokio::runtime::Handle::current()));
        }
        self.kick(&mut inner);
    }

    /// Runs the loader if it's started, idle, and has work.
    fn kick(self: &Arc<Self>, inner: &mut Inner) {
        if inner.running || inner.queue.is_empty() || self.is_cancelled() {
            return;
        }
        let Some((loader, handle)) = inner.loader.clone() else {
            return;
        };
        inner.running = true;
        handle.spawn(Arc::clone(self).run(loader));
    }

    /// Stops the loader: no new read starts, and a read in flight stops at its
    /// next fetch. For a session that's going.
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Reads the files `wanted` next (the rows the sheet shows), after `first`
    /// (the first file in rename order) while it's still waiting.
    pub(crate) fn prioritize<'a>(self: &Arc<Self>, first: Option<&'a str>, wanted: impl IntoIterator<Item = &'a str>) {
        let mut inner = self.lock();
        let mut front: Vec<Arc<str>> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for name in first.into_iter().chain(wanted) {
            if inner.is_waiting(name) && seen.insert(name) {
                front.push(Arc::from(name));
            }
        }
        inner.queue.retain(|name| !seen.contains(&**name));
        for name in front.into_iter().rev() {
            inner.queue.push_front(name);
        }
        self.kick(&mut inner);
    }

    /// Takes what `other` (a session over the same folder) read: each file waiting here
    /// that `other` read for the same listing facts, and `allowed` lets through, gets its
    /// answer. The rest stay queued for this session's own loader.
    pub(crate) fn adopt_where(&self, other: &SessionMedia, allowed: impl Fn(&str) -> bool) {
        // One lock at a time: `other`'s answers are copied out before ours is taken.
        let answers: Vec<(Arc<str>, Stamp, MediaRead)> = other
            .lock()
            .files
            .iter()
            .filter(|(name, _)| allowed(name))
            .filter(|(_, cached)| matches!(cached.state, MediaRead::Read(_) | MediaRead::Unreadable))
            .map(|(name, cached)| (Arc::clone(name), cached.stamp, cached.state.clone()))
            .collect();
        let mut inner = self.lock();
        for (name, stamp, state) in answers {
            if inner.is_waiting(&name) && inner.files[&name].stamp == stamp {
                inner.set_state(&name, state);
            }
        }
        let Inner { queue, files, .. } = &mut *inner;
        queue.retain(|name| files.get(name).is_some_and(|c| c.state.is_reading()));
    }

    /// Reads every waiting file with `loader`, inline, and returns once each is read,
    /// unreadable, or cancelled. For a preview with no sheet (the MCP `multi_rename` tool).
    pub(crate) async fn read_all(self: &Arc<Self>, loader: Loader) {
        {
            let mut inner = self.lock();
            if inner.running || inner.queue.is_empty() || self.is_cancelled() {
                return;
            }
            inner.running = true;
        }
        Arc::clone(self).run(Arc::new(loader)).await;
    }

    /// A copy of the cache as one preview's [`MediaLookup`], so the preview runs
    /// without the lock. [`Lookup::finish`] re-queues the files it found changed.
    pub(crate) fn lookup(self: &Arc<Self>) -> Lookup {
        let files = self.lock().files.clone();
        Lookup {
            media: Arc::clone(self),
            files,
            changed: RefCell::new(Vec::new()),
        }
    }

    /// The loader: reads queued files, `parallel` at a time, until none is left,
    /// the session goes, or the volume does, then says how far it got.
    ///
    /// A dead volume costs a bounded few reads, never one budget per file: after
    /// [`GIVE_UP_AFTER_FAILURES`] timeouts or I/O errors in a row, or once the
    /// volume reports it's gone, every file still waiting is `Unreadable` at once.
    /// So at most `parallel + GIVE_UP_AFTER_FAILURES - 1` reads time out per run,
    /// each leaving one detached `read_range` behind (`VolumeByteSource`).
    async fn run(self: Arc<Self>, loader: Arc<Loader>) {
        let mut reads = tokio::task::JoinSet::new();
        let mut last_emit: Option<Instant> = None;
        let mut failures_in_a_row = 0;
        loop {
            while reads.len() < loader.parallel.max(1) {
                if (loader.volume_gone)() {
                    self.give_up("its volume went away");
                    break;
                }
                let Some(job) = self.lock().next_job(&self.dir, &self.cancel) else {
                    break;
                };
                let reader = Arc::clone(&loader.reader);
                reads.spawn_blocking(move || {
                    // A parser that panics on a hostile file makes it unreadable, not the read stuck.
                    match catch_unwind(AssertUnwindSafe(|| reader(&job))) {
                        Ok(result) => (job, result, false),
                        Err(_) => (job, Err(ReadError::Io("the reader panicked".to_string())), true),
                    }
                });
            }
            let Some(joined) = reads.join_next().await else {
                if self.stop_if_idle() {
                    break;
                }
                continue;
            };
            match joined {
                Ok((job, result, panicked)) => {
                    // A hostile file is that file's problem; a timeout or an I/O error is the volume's.
                    let failed = !panicked && matches!(result, Err(ReadError::TimedOut | ReadError::Io(_)));
                    self.store(job, result);
                    failures_in_a_row = if failed { failures_in_a_row + 1 } else { 0 };
                    if failures_in_a_row >= GIVE_UP_AFTER_FAILURES {
                        self.give_up("its volume keeps failing");
                    }
                }
                // `catch_unwind` above leaves only a runtime shutdown here.
                Err(error) => log::debug!(target: LOG_TARGET, "a media read didn't come back: {error}"),
            }
            if !self.is_cancelled() && last_emit.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
                last_emit = Some(Instant::now());
                self.emit(&loader);
            }
        }
        if !self.is_cancelled() {
            self.emit(&loader);
        }
    }

    /// Marks every file still waiting `Unreadable` (logged once, with `why`).
    fn give_up(&self, why: &str) {
        let marked = self.lock().give_up_waiting();
        if marked > 0 {
            log::warn!(
                target: LOG_TARGET,
                "Stopped reading media fields: {why}; {} left unread",
                crate::pluralize::pluralize(marked as u64, "file")
            );
        }
    }

    /// Ends the loader's run when nothing's queued (or the session went); `false`
    /// when work arrived since the queue last ran dry.
    fn stop_if_idle(&self) -> bool {
        let mut inner = self.lock();
        let has_work = !self.is_cancelled() && inner.queue.iter().any(|name| inner.is_waiting(name));
        if !has_work {
            inner.running = false;
            inner.queue.clear();
        }
        !has_work
    }

    /// Sends the progress as it stands: `done` only when every media file is
    /// read, so a run ending while another starts never claims it.
    fn emit(&self, loader: &Loader) {
        let progress = self.progress();
        (loader.emit)(MultiRenameMediaProgress {
            session_id: self.session_id.clone(),
            read: progress.read,
            total: progress.total,
            done: progress.done,
        });
    }

    /// Stores a read's answer, unless the file changed meanwhile (then it's read
    /// again) or the session is going.
    fn store(&self, job: FileJob, result: Result<MediaFacts, ReadError>) {
        let mut inner = self.lock();
        let Some((name, cached)) = inner.files.get_key_value(job.name.as_str()) else {
            return;
        };
        let name = Arc::clone(name);
        let (reading, stamp) = (cached.state.is_reading(), cached.stamp);
        inner.in_flight.remove(&name);
        if !reading {
            return;
        }
        if stamp != job.stamp {
            inner.queue.push_front(name);
            return;
        }
        let state = match result {
            Ok(facts) => MediaRead::Read(Arc::new(facts)),
            Err(ReadError::Cancelled) => return,
            Err(error) => {
                log::debug!(
                    target: LOG_TARGET,
                    "Couldn't read the media fields of {}: {error:?}",
                    LogPrivate::file_name(&job.name)
                );
                MediaRead::Unreadable
            }
        };
        inner.set_state(&name, state);
    }

    /// The picker's overview over the files `names` (in rename order). Copies the
    /// read facts out under the lock, then formats without it.
    pub(crate) fn overview(&self, names: &[String]) -> MediaOverview {
        let (facts, progress) = {
            let inner = self.lock();
            let facts: Vec<Arc<MediaFacts>> = names
                .iter()
                .filter_map(|name| match &inner.files.get(name.as_str())?.state {
                    MediaRead::Read(facts) => Some(Arc::clone(facts)),
                    _ => None,
                })
                .collect();
            (facts, inner.progress())
        };
        let fields = MediaField::ALL
            .into_iter()
            .map(|field| {
                let token = MediaToken::of(field);
                MediaFieldOverview {
                    field,
                    example: facts.iter().find_map(|facts| token.value_text(facts)),
                    files_with_it: facts.iter().filter(|facts| facts.get(field).is_some()).count(),
                }
            })
            .collect();
        MediaOverview { progress, fields }
    }

    /// `token` rendered alone on the files at `rows` of `names` (in rename order;
    /// a row past the end is left out), and its usage across all of them. The
    /// usage is remembered per token until the cache changes.
    pub(crate) fn field_samples(&self, token: &MediaToken, names: &[String], rows: &[usize]) -> FieldSamples {
        let (parent, grandparent) = folder_names(&self.dir);
        let resolve = |files: &HashMap<Arc<str>, Cached>, position: usize, name: &str| {
            let cached = files.get(name);
            // Composed, as the preview renders it (`plan.rs`).
            let file_name: String = name.nfc().collect();
            token.resolve(&RowFacts {
                file_name: &file_name,
                is_directory: cached.is_some_and(|c| c.is_directory),
                parent: &parent,
                grandparent: &grandparent,
                modified: cached.and_then(|c| c.stamp.modified).and_then(local_time),
                position,
                media: cached.map_or(RowMedia::NotMedia, |c| c.state.row_media()),
            })
        };
        // The sample rows are a handful: resolved under the lock, which also says
        // whether the remembered usage still holds.
        let (samples, generation) = {
            let inner = self.lock();
            let samples = rows
                .iter()
                .filter_map(|&row| Some((row, names.get(row)?)))
                .map(|(row, name)| match resolve(&inner.files, row, name) {
                    Ok(text) => FieldSample {
                        row,
                        text: Some(text.into_text()),
                        gap: None,
                    },
                    Err(gap) => FieldSample {
                        row,
                        text: None,
                        gap: Some(gap),
                    },
                })
                .collect();
            (samples, inner.generation)
        };
        let remembered = self
            .usage
            .lock_ignore_poison()
            .as_ref()
            .filter(|r| r.generation == generation && r.token == *token)
            .map(|r| r.usage);
        let usage = remembered.unwrap_or_else(|| {
            let (files, generation) = {
                let inner = self.lock();
                (inner.files.clone(), inner.generation)
            };
            let mut usage = FieldUsage::default();
            for (position, name) in names.iter().enumerate() {
                match resolve(&files, position, name) {
                    Ok(FieldText::Value(_)) => usage.with_value += 1,
                    Ok(FieldText::Fallback(_)) => usage.fallback += 1,
                    Err(FieldGap::Reading) => usage.reading += 1,
                    Err(FieldGap::NoValue { .. } | FieldGap::Unreadable { .. }) => usage.missing += 1,
                }
            }
            *self.usage.lock_ignore_poison() = Some(RememberedUsage {
                token: token.clone(),
                generation,
                usage,
            });
            usage
        });
        FieldSamples { samples, usage }
    }
}

/// A Unix-seconds date as local wall-clock time, as the preview reads `modified`.
fn local_time(unix_seconds: u64) -> Option<NaiveDateTime> {
    let seconds = i64::try_from(unix_seconds).ok()?;
    Local.timestamp_opt(seconds, 0).single().map(|t| t.naive_local())
}

/// The cache, copied for one preview. A file whose listing entry no longer
/// matches what was read answers `Reading`, and [`finish`](Self::finish) queues
/// it again.
pub(crate) struct Lookup {
    media: Arc<SessionMedia>,
    files: HashMap<Arc<str>, Cached>,
    changed: RefCell<Vec<(String, bool, Stamp)>>,
}

impl MediaLookup for Lookup {
    fn media(&self, entry: &FileEntry) -> RowMedia<'_> {
        let stamp = Stamp::of(entry);
        match self.files.get(entry.name.as_str()) {
            Some(cached) if !cached.state.is_media() => RowMedia::NotMedia,
            Some(cached) if cached.stamp == stamp => cached.state.row_media(),
            // Changed since it was read, or new to the cache: read it (again).
            _ if !entry.is_directory && is_media_name(&entry.name) => {
                self.changed
                    .borrow_mut()
                    .push((entry.name.clone(), entry.is_directory, stamp));
                RowMedia::Reading
            }
            _ => RowMedia::NotMedia,
        }
    }
}

impl Lookup {
    /// Ends the preview: the files it found changed are `Reading` again, first in
    /// the queue, and the loader runs if it was idle. A file another preview
    /// already re-queued for the same listing facts is left as it is.
    pub(crate) fn finish(self) {
        let Self { media, changed, .. } = self;
        let mut inner = media.lock();
        for (name, is_directory, stamp) in changed.into_inner() {
            if inner
                .files
                .get(name.as_str())
                .is_some_and(|c| c.stamp == stamp && c.state.is_media())
            {
                continue;
            }
            let name: Arc<str> = Arc::from(name);
            let cached = inner.files.entry(Arc::clone(&name)).or_insert(Cached {
                is_directory,
                stamp,
                state: MediaRead::NotMedia,
            });
            cached.stamp = stamp;
            inner.set_state(&name, MediaRead::Reading);
            inner.queue.push_front(name);
        }
        media.kick(&mut inner);
    }
}

//! A Multi-Rename session: the files one open sheet renames, resolved ONCE.
//!
//! Opening the sheet turns the pane's selection (row numbers, checked against the
//! listing's sequence) into file names in that folder, and every preview and the
//! apply work from those names. A file appearing in the folder while the sheet
//! is open shifts the pane's rows but never the session's files; a session file
//! that vanished previews as `Missing`, never as another file.
//!
//! The names and the latest preview stay here: the sheet gets counts and the
//! page of rows it draws, and sends back only the session and preview ids. The
//! files' order is the rename order, which the sheet can change a row at a time
//! (`move_row`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::rt::{self, Instant, Runtime};
use cmdr_listing::{ListingId, ListingLookupError, Listings};

use crate::error::MultiRenameError;
use crate::host::MultiRenameHost;
use crate::mask::{MaskError, MediaToken, RangeToken, RowFacts, RowMedia};
use crate::media::{FieldSamples, MediaOverview, MediaProgress, ProgressEmit, SessionMedia};
use crate::names_file::{self, Written};
use crate::plan::{
    Compiled, MultiRenameSpec, NameEdits, PreviewRow, RowStatus, SpecError, folder_names, preview_names,
};

/// How many sessions live at once unless the host says otherwise
/// ([`MultiRenameBuilder::max_sessions`]). A sheet is modal, so more than one or
/// two is a window that closed without saying so.
pub const MAX_SESSIONS: usize = 4;

/// A session nobody touched for this long is a forgotten sheet (a reloaded
/// window, a crash): opening another one drops it.
const IDLE_LIMIT: Duration = Duration::from_secs(30 * 60);

/// How many rows a preview hands back with its counts: the table's first screen.
pub const FIRST_PAGE: usize = 200;

/// The most rows one page request returns.
pub(crate) const MAX_PAGE: usize = 1000;

/// An open sheet's session.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameOpened {
    pub session_id: String,
    /// How many files the session renames.
    pub count: usize,
}

/// How a preview's rows add up, for the footer and the Rename button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCounts {
    pub ready: usize,
    pub unchanged: usize,
    /// Every other row: a bad or taken name, a duplicate, a file that's gone, a
    /// media field the file doesn't have.
    pub problems: usize,
    /// Rows whose media fields are still being read: neither ready nor a problem yet.
    pub reading: usize,
}

impl PreviewCounts {
    fn of(rows: &[PreviewRow]) -> Self {
        let mut counts = Self::default();
        for row in rows {
            match row.status {
                RowStatus::Ready => counts.ready += 1,
                RowStatus::Unchanged => counts.unchanged += 1,
                RowStatus::Reading => counts.reading += 1,
                _ => counts.problems += 1,
            }
        }
        counts
    }
}

/// Which rows a page counts through: all of them, or only the problems
/// (`RowStatus::is_problem`), so the sheet can list those alone without holding
/// every row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PreviewFilter {
    All,
    Problems,
}

/// A preview: its id (what apply and paging name), its counts, and its first rows.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenamePreview {
    pub preview_id: u64,
    pub counts: PreviewCounts,
    /// Rows `0..FIRST_PAGE`; the rest come from `get_multi_rename_preview_rows`.
    pub rows: Vec<PreviewRow>,
}

/// The latest preview a session answered, kept so paging reads it and apply can
/// prove the user saw what it would rename.
struct StoredPreview {
    id: u64,
    spec: MultiRenameSpec,
    /// The Results names it showed, so apply renames with exactly those.
    edits: Arc<NameEdits>,
    rows: Vec<PreviewRow>,
}

struct Session {
    listing_id: String,
    volume_id: String,
    dir: PathBuf,
    /// The files, in rename order. A row's `row` is its index here.
    names: Arc<[String]>,
    /// Names the user typed in Results, which every preview from now on takes.
    edits: Arc<NameEdits>,
    /// The Results file this session wrote last, the only one it reads back.
    written: Option<Written>,
    next_preview: u64,
    latest: Option<Arc<StoredPreview>>,
    last_used: Instant,
    /// What the files' EXIF and tags say, read in the background.
    media: Arc<SessionMedia>,
}

/// A session that goes (closed, evicted) stops its media read.
impl Drop for Session {
    fn drop(&mut self) {
        self.media.cancel();
    }
}

/// The Multi-Rename Tool's engine: every open sheet's session, over the host's
/// listing engine, runtime, and answers ([`MultiRenameHost`]). Cheap to clone;
/// every clone is the same engine. Two engines share nothing.
#[derive(Clone)]
pub struct MultiRename {
    inner: Arc<Inner>,
}

struct Inner {
    listings: Listings,
    runtime: Runtime,
    host: Arc<dyn MultiRenameHost>,
    max_sessions: usize,
    sessions: Mutex<HashMap<String, Session>>,
    next_session: AtomicU64,
}

/// Builds a [`MultiRename`]: [`MultiRename::builder`], then the knobs, then [`build`](Self::build).
pub struct MultiRenameBuilder {
    listings: Listings,
    runtime: Runtime,
    host: Arc<dyn MultiRenameHost>,
    max_sessions: usize,
}

impl MultiRenameBuilder {
    /// How many sessions live at once ([`MAX_SESSIONS`] by default); opening one
    /// more drops the least recently used. A host whose tests run in parallel over
    /// one engine raises it out of reach.
    #[must_use]
    pub fn max_sessions(mut self, max_sessions: usize) -> Self {
        self.max_sessions = max_sessions.max(1);
        self
    }

    /// The engine, with no session open.
    #[must_use]
    pub fn build(self) -> MultiRename {
        MultiRename {
            inner: Arc::new(Inner {
                listings: self.listings,
                runtime: self.runtime,
                host: self.host,
                max_sessions: self.max_sessions,
                sessions: Mutex::new(HashMap::new()),
                next_session: AtomicU64::new(1),
            }),
        }
    }
}

impl MultiRename {
    /// An engine over `listings` (where a session's files come from), running its
    /// background work (media reads, apply's blocking steps) on `runtime`, and asking
    /// `host` the rest.
    pub fn builder(listings: Listings, runtime: Runtime, host: Arc<dyn MultiRenameHost>) -> MultiRenameBuilder {
        MultiRenameBuilder {
            listings,
            runtime,
            host,
            max_sessions: MAX_SESSIONS,
        }
    }

    pub(crate) fn host(&self) -> &dyn MultiRenameHost {
        &*self.inner.host
    }

    pub(crate) fn runtime(&self) -> &Runtime {
        &self.inner.runtime
    }

    /// Opens a session over the pane's `selected` rows (backend row numbers in
    /// rename order; `None` for every row the pane shows), read at
    /// `expected_sequence`. Refuses with `SelectionChanged` when the pane's rows
    /// aren't the listing's state any more, rather than reading them in a newer one.
    pub fn open(
        &self,
        listing_id: &str,
        include_hidden: bool,
        selected: Option<&[usize]>,
        expected_sequence: u64,
    ) -> Result<MultiRenameOpened, MultiRenameError> {
        let listings = &self.inner.listings;
        let id = ListingId::new(listing_id);
        let gone = || MultiRenameError::Gone {
            listing_id: listing_id.to_string(),
        };
        let names = listings
            .selection_names(&id, include_hidden, selected, expected_sequence)
            .map_err(|error| match error {
                ListingLookupError::Gone { .. } => gone(),
                ListingLookupError::Changed { .. } => MultiRenameError::SelectionChanged {
                    listing_id: listing_id.to_string(),
                },
            })?;
        let (volume_id, dir) = listings.location(&id).ok_or_else(gone)?;
        let entries = listings.entries(&id).map_err(|_| gone())?;
        let count = names.len();
        let session_id = format!(
            "multi-rename-{}",
            self.inner.next_session.fetch_add(1, Ordering::Relaxed)
        );
        let media = Arc::new(SessionMedia::seed(&session_id, dir.clone(), &names, &entries));
        let mut sessions = self.inner.sessions.lock_ignore_poison();
        evict_to(&mut sessions, rt::now(), self.inner.max_sessions);
        sessions.insert(
            session_id.clone(),
            Session {
                listing_id: listing_id.to_string(),
                volume_id,
                dir,
                names: names.into(),
                edits: Arc::default(),
                written: None,
                next_preview: 1,
                latest: None,
                last_used: rt::now(),
                media,
            },
        );
        Ok(MultiRenameOpened { session_id, count })
    }

    /// Starts reading the session's media files in the background, on the engine's
    /// runtime, through the host's loader, reporting through `emit`.
    pub fn start_media(&self, session_id: &str, emit: ProgressEmit) -> Result<(), MultiRenameError> {
        let (media, volume_id) = self.with_session(session_id, |s| (Arc::clone(&s.media), s.volume_id.clone()))?;
        let loader = self.host().media_loader(&volume_id, emit, self.runtime());
        media.start(loader, self.runtime().clone());
        Ok(())
    }

    /// [`start_media`](Self::start_media) with any loader: a test's reader and emitter.
    #[cfg(any(test, feature = "testing"))]
    pub fn start_media_with(&self, session_id: &str, loader: crate::media::Loader) -> Result<(), MultiRenameError> {
        self.with_session(session_id, |s| Arc::clone(&s.media))?
            .start(loader, self.runtime().clone());
        Ok(())
    }

    /// How far the session's media read got.
    pub fn media_progress(&self, session_id: &str) -> Result<MediaProgress, MultiRenameError> {
        Ok(self.with_session(session_id, |s| Arc::clone(&s.media))?.progress())
    }

    /// The field picker's overview: each field's example from the first file in
    /// rename order that has it.
    pub fn media_overview(&self, session_id: &str) -> Result<MediaOverview, MultiRenameError> {
        let (media, names) = self.with_session(session_id, |s| (Arc::clone(&s.media), Arc::clone(&s.names)))?;
        Ok(media.overview(&names))
    }

    /// The media placeholder `token` (its text between the brackets,
    /// `=exif.taken:d`) rendered alone on the session's files at `rows` (at most
    /// `MAX_RANGE_SAMPLES`; a row past the end is left out), and how all the
    /// session's files fare with it. Reads the media cache, so it answers before any
    /// preview uses the token. A token that isn't a media field is a spec error,
    /// worded like the mask's own.
    pub fn field_samples(
        &self,
        session_id: &str,
        token: &str,
        rows: &[usize],
    ) -> Result<FieldSamples, MultiRenameError> {
        let parsed = MediaToken::parse(token).ok_or_else(|| unknown_placeholder(token))?;
        let (media, names) = self.with_session(session_id, |s| (Arc::clone(&s.media), Arc::clone(&s.names)))?;
        let rows: Vec<usize> = rows
            .iter()
            .copied()
            .filter(|&row| row < names.len())
            .take(MAX_RANGE_SAMPLES)
            .collect();
        Ok(media.field_samples(&parsed, &names, &rows))
    }

    /// Reads the session's files at `rows` (the ones the sheet shows) next, after
    /// the first file in rename order. Rows past the end are ignored.
    pub fn prioritize_media(&self, session_id: &str, rows: &[usize]) -> Result<(), MultiRenameError> {
        let (media, names) = self.with_session(session_id, |s| (Arc::clone(&s.media), Arc::clone(&s.names)))?;
        let wanted = rows.iter().filter_map(|&row| names.get(row)).map(String::as_str);
        media.prioritize(names.first().map(String::as_str), wanted);
        Ok(())
    }

    /// The media caches of the open sessions over the folder `dir` on `volume_id`, so a
    /// preview without a sheet (the MCP `multi_rename` tool) can take what they read.
    pub fn media_over(&self, volume_id: &str, dir: &Path) -> Vec<Arc<SessionMedia>> {
        self.inner
            .sessions
            .lock_ignore_poison()
            .values()
            .filter(|s| s.volume_id == volume_id && s.dir == dir)
            .map(|s| Arc::clone(&s.media))
            .collect()
    }

    /// Ends a session, and with it its Results file. No-op when it's already gone.
    pub fn close(&self, session_id: &str) {
        self.inner.sessions.lock_ignore_poison().remove(session_id);
    }

    fn with_session<R>(&self, session_id: &str, f: impl FnOnce(&mut Session) -> R) -> Result<R, MultiRenameError> {
        let mut sessions = self.inner.sessions.lock_ignore_poison();
        let session = sessions.get_mut(session_id).ok_or(MultiRenameError::SessionClosed)?;
        session.last_used = rt::now();
        Ok(f(session))
    }

    /// Every row of `names`' preview against the folder as the listing holds it now,
    /// with the Results names in `edits` and the media facts read so far. A file
    /// that changed since it was read previews as `Reading` and is read again.
    fn compute(
        &self,
        inputs: &Inputs,
        compiled: &Compiled,
        edits: &NameEdits,
    ) -> Result<Vec<PreviewRow>, MultiRenameError> {
        let siblings = self
            .inner
            .listings
            .entries(&ListingId::new(inputs.listing_id.clone()))
            .map_err(|_| MultiRenameError::Gone {
                listing_id: inputs.listing_id.clone(),
            })?;
        let media = inputs.media.lookup();
        let rows = preview_names(compiled, &inputs.dir, &inputs.names, &siblings, edits, &media);
        media.finish();
        Ok(rows)
    }

    /// The sheet's live preview of `spec` over the session's files. Stores it as the
    /// session's latest unless a newer one already landed.
    pub fn preview(&self, session_id: &str, spec: &MultiRenameSpec) -> Result<MultiRenamePreview, MultiRenameError> {
        let compiled = Compiled::new(spec).map_err(|error| MultiRenameError::Spec { error })?;
        let (preview_id, inputs) = self.with_session(session_id, |s| {
            let id = s.next_preview;
            s.next_preview += 1;
            (id, inputs_of(s))
        })?;
        let rows = self.compute(&inputs, &compiled, &inputs.edits)?;
        let answer = MultiRenamePreview {
            preview_id,
            counts: PreviewCounts::of(&rows),
            rows: rows.iter().take(FIRST_PAGE).cloned().collect(),
        };
        self.with_session(session_id, |s| {
            // Ids are handed out in request order, so an older preview finishing
            // late never replaces the one the sheet shows.
            if s.latest.as_ref().is_none_or(|latest| latest.id < preview_id) {
                s.latest = Some(Arc::new(StoredPreview {
                    id: preview_id,
                    spec: spec.clone(),
                    edits: Arc::clone(&inputs.edits),
                    rows,
                }));
            }
        })?;
        Ok(answer)
    }

    /// The stored preview `preview_id`, or `PreviewOutOfDate` when a newer one replaced it.
    fn stored(&self, session_id: &str, preview_id: u64) -> Result<(Arc<StoredPreview>, Inputs), MultiRenameError> {
        self.with_session(session_id, |s| {
            s.latest
                .as_ref()
                .filter(|latest| latest.id == preview_id)
                .map(|latest| (Arc::clone(latest), inputs_of(s)))
                .ok_or(MultiRenameError::PreviewOutOfDate)
        })?
    }

    /// Rows `offset..offset + limit` (at most `MAX_PAGE`) of preview `preview_id`,
    /// counted among the rows `filter` keeps. A problems page walks the rows before
    /// it, which is a scan of plain structs: a few ms at 200k rows.
    pub fn page(
        &self,
        session_id: &str,
        preview_id: u64,
        offset: usize,
        limit: usize,
        filter: PreviewFilter,
    ) -> Result<Vec<PreviewRow>, MultiRenameError> {
        let (latest, _) = self.stored(session_id, preview_id)?;
        let kept = latest
            .rows
            .iter()
            .filter(|row| filter == PreviewFilter::All || row.status.is_problem());
        Ok(kept.skip(offset).take(limit.min(MAX_PAGE)).cloned().collect())
    }

    /// Moves the session's file at `row` one place up or down in the rename order (the
    /// order `[C]` counts) and answers its new place; at that end it stays put. A move
    /// retires the latest preview: its rows no longer say what the session renames, so
    /// paging and apply refuse it with `PreviewOutOfDate` until the sheet's next preview.
    pub fn move_row(&self, session_id: &str, row: usize, direction: RowMove) -> Result<usize, MultiRenameError> {
        self.with_session(session_id, |s| {
            if row >= s.names.len() {
                return Err(MultiRenameError::Internal {
                    detail: format!("row {row} is past the session's {} files", s.names.len()),
                });
            }
            let to = match direction {
                RowMove::Up => row.checked_sub(1),
                RowMove::Down => Some(row + 1).filter(|&to| to < s.names.len()),
            };
            let Some(to) = to else {
                return Ok(row);
            };
            let mut names = s.names.to_vec();
            names.swap(row, to);
            s.names = names.into();
            s.latest = None;
            Ok(to)
        })?
    }

    /// What the range placeholder `token` (its text between the brackets, `N2-5`)
    /// takes from each of the session's files at `rows`, computed by `mask.rs`'s own
    /// slicing. Reads the files as the latest preview lists them (its old names, and
    /// whether each is a folder), so it answers for exactly the rows the sheet shows;
    /// `PreviewOutOfDate` while a move has retired it. A row past the end is left out;
    /// at most `MAX_RANGE_SAMPLES` rows answer. A token that isn't a range is a spec
    /// error, worded like the mask's own.
    pub fn range_samples(
        &self,
        session_id: &str,
        token: &str,
        rows: &[usize],
    ) -> Result<Vec<RangeSample>, MultiRenameError> {
        let range = RangeToken::parse(token).ok_or_else(|| unknown_placeholder(token))?;
        let (latest, dir) = self
            .with_session(session_id, |s| {
                s.latest.as_ref().map(|l| (Arc::clone(l), s.dir.clone()))
            })?
            .ok_or(MultiRenameError::PreviewOutOfDate)?;
        let (parent, grandparent) = folder_names(&dir);
        Ok(rows
            .iter()
            .filter_map(|&row| latest.rows.get(row))
            .take(MAX_RANGE_SAMPLES)
            .map(|shown| {
                // Composed, as the preview renders it (`plan.rs`).
                let file_name: String = shown.old_name.nfc().collect();
                let facts = RowFacts {
                    file_name: &file_name,
                    is_directory: shown.is_directory,
                    parent: &parent,
                    grandparent: &grandparent,
                    modified: None,
                    position: shown.row,
                    media: RowMedia::NotMedia,
                };
                let text = range.source(&facts);
                let taken = range
                    .bounds(text.chars().count())
                    .map(|(first, last)| TakenChars { first, last });
                RangeSample {
                    row: shown.row,
                    text: text.to_string(),
                    taken,
                }
            })
            .collect())
    }

    /// Recomputes preview `preview_id`'s spec against the folder as it is now and
    /// requires its ready rows to be EXACTLY the ones that preview showed. While a
    /// row's media fields are being read it refuses with `StillReading`, so the
    /// sheet's queued Rename waits for the preview after the read, but only when
    /// that preview was itself still reading. A fully read preview whose files
    /// changed since is `PreviewOutOfDate`: ❌ never let it queue, or the rename
    /// would run names the user never saw.
    pub(crate) fn prepare(&self, session_id: &str, preview_id: u64) -> Result<Prepared, MultiRenameError> {
        let (seen, inputs) = self.stored(session_id, preview_id)?;
        let compiled = Compiled::new(&seen.spec).map_err(|error| MultiRenameError::Spec { error })?;
        let rows = self.compute(&inputs, &compiled, &seen.edits)?;
        let reading = |rows: &[PreviewRow]| rows.iter().any(|row| matches!(row.status, RowStatus::Reading));
        if reading(&rows) {
            return Err(if reading(&seen.rows) {
                MultiRenameError::StillReading
            } else {
                MultiRenameError::PreviewOutOfDate
            });
        }
        let ready: Vec<PreviewRow> = rows.into_iter().filter(|row| row.status.is_ready()).collect();
        let shown = seen.rows.iter().filter(|row| row.status.is_ready());
        if !ready.iter().eq(shown) {
            return Err(MultiRenameError::PreviewOutOfDate);
        }
        if ready.is_empty() {
            return Err(MultiRenameError::NothingToRename);
        }
        Ok(Prepared {
            volume_id: inputs.volume_id,
            dir: inputs.dir,
            ready,
        })
    }

    /// The spec preview `preview_id` ran, while it's the session's latest.
    pub fn spec_of(&self, session_id: &str, preview_id: u64) -> Result<MultiRenameSpec, MultiRenameError> {
        self.stored(session_id, preview_id)
            .map(|(latest, _)| latest.spec.clone())
    }

    /// Results (⌥⏎): writes preview `preview_id`'s rows as `old<TAB>new` lines for
    /// the user's editor, at the host's path, and returns it. The session reads back
    /// only this file (`read_names`).
    pub fn write_names(&self, session_id: &str, preview_id: u64) -> Result<PathBuf, MultiRenameError> {
        let (latest, _) = self.stored(session_id, preview_id)?;
        // The old file first: it has the same path, and dropping it deletes it.
        self.with_session(session_id, |s| s.written = None)?;
        let couldnt = |detail: String| MultiRenameError::CouldntWriteNames { detail };
        let path = self
            .host()
            .results_path(session_id)
            .ok_or_else(|| couldnt("the OS has no cache folder".to_string()))?;
        let written = names_file::write(&path, &latest.rows).map_err(|e| couldnt(e.to_string()))?;
        self.with_session(session_id, |s| s.written = Some(written))?;
        Ok(path)
    }

    /// Reads the session's Results file back: a line the user changed names its row
    /// from now on. Returns how many rows carry a typed name; the next preview shows them.
    pub fn read_names(&self, session_id: &str) -> Result<usize, MultiRenameError> {
        let gone = |detail: String| MultiRenameError::NamesFileGone { detail };
        let path = self
            .with_session(session_id, |s| s.written.as_ref().map(|w| w.path.clone()))?
            .ok_or_else(|| gone("no Results file was written".to_string()))?;
        // Read outside the lock; the merge below checks the file is still this session's.
        let text = names_file::read(&path).map_err(|e| gone(e.to_string()))?;
        self.with_session(session_id, |s| {
            let written = s
                .written
                .as_ref()
                .filter(|w| w.path == path)
                .ok_or_else(|| gone("Results was cleared meanwhile".to_string()))?;
            let edits = written.merge(&text, &s.edits);
            let count = edits.len();
            s.edits = Arc::new(edits);
            Ok(count)
        })?
    }

    /// Drops the typed names and the Results file: every row follows the settings again.
    pub fn clear_names(&self, session_id: &str) -> Result<(), MultiRenameError> {
        self.with_session(session_id, |s| {
            s.edits = Arc::default();
            s.written = None;
        })
    }

    /// Whether session `session_id` is still open.
    #[cfg(any(test, feature = "testing"))]
    pub fn is_open(&self, session_id: &str) -> bool {
        self.inner.sessions.lock_ignore_poison().contains_key(session_id)
    }
}

/// A placeholder token the sheet's editor asked about that isn't the kind it edits:
/// a spec error, worded like the mask's own.
fn unknown_placeholder(token: &str) -> MultiRenameError {
    MultiRenameError::Spec {
        error: SpecError::NameMask {
            error: MaskError::Unknown {
                placeholder: token.to_string(),
            },
        },
    }
}

/// Drops idle sessions, then the least recently used until one more fits under `cap`.
fn evict_to(sessions: &mut HashMap<String, Session>, now: Instant, cap: usize) {
    sessions.retain(|_, s| now.duration_since(s.last_used) < IDLE_LIMIT);
    while sessions.len() >= cap {
        let Some(oldest) = sessions
            .iter()
            .min_by_key(|(_, s)| s.last_used)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        sessions.remove(&oldest);
    }
}

/// What a computation needs from a session, copied out so no lock is held while
/// it runs.
struct Inputs {
    listing_id: String,
    volume_id: String,
    dir: PathBuf,
    names: Arc<[String]>,
    edits: Arc<NameEdits>,
    media: Arc<SessionMedia>,
}

fn inputs_of(s: &Session) -> Inputs {
    Inputs {
        listing_id: s.listing_id.clone(),
        volume_id: s.volume_id.clone(),
        dir: s.dir.clone(),
        names: Arc::clone(&s.names),
        edits: Arc::clone(&s.edits),
        media: Arc::clone(&s.media),
    }
}

/// What apply renames: the rows ready now, proven to be the ones preview
/// `preview_id` showed.
pub(crate) struct Prepared {
    pub volume_id: String,
    pub dir: PathBuf,
    pub ready: Vec<PreviewRow>,
}

/// Which way ⇧↑ / ⇧↓ moves a row in the rename order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RowMove {
    Up,
    Down,
}

/// The most rows one range-samples request answers: the editor shows five.
pub(crate) const MAX_RANGE_SAMPLES: usize = 16;

/// What a range placeholder takes from one session file, for the sheet's range
/// editor: the text its field reads and the characters it takes from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RangeSample {
    /// The file's place in the session (what `PreviewRow::row` names).
    pub row: usize,
    /// The field's text: the name, extension, folder, folder above, or full name.
    pub text: String,
    /// The characters taken, as indices into `text`'s Unicode scalar values (what
    /// `Array.from(text)` splits), or `None` when it takes nothing.
    pub taken: Option<TakenChars>,
}

/// The first and last character a range takes, from 0, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TakenChars {
    pub first: usize,
    pub last: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(last_used: Instant) -> Session {
        Session {
            listing_id: String::new(),
            volume_id: "root".to_string(),
            dir: PathBuf::new(),
            names: Vec::new().into(),
            edits: Arc::default(),
            written: None,
            next_preview: 1,
            latest: None,
            last_used,
            media: Arc::new(SessionMedia::seed("", PathBuf::new(), &[], &[])),
        }
    }

    fn row(status: RowStatus) -> PreviewRow {
        PreviewRow {
            row: 0,
            old_name: "a.jpg".to_string(),
            new_name: "a.jpg".to_string(),
            status,
            icon_id: None,
            is_directory: false,
            edited: false,
        }
    }

    #[test]
    fn rows_still_reading_count_as_reading_and_never_as_problems() {
        let field = cmdr_media_meta::MediaField::ExifTaken;
        let rows = [
            row(RowStatus::Ready),
            row(RowStatus::Unchanged),
            row(RowStatus::Reading),
            row(RowStatus::Reading),
            row(RowStatus::NoValue { field }),
            row(RowStatus::Unreadable { field }),
            row(RowStatus::Missing),
        ];

        assert_eq!(
            PreviewCounts::of(&rows),
            PreviewCounts {
                ready: 1,
                unchanged: 1,
                problems: 3,
                reading: 2,
            }
        );
    }

    #[test]
    fn an_evicted_session_stops_its_media_read() {
        let now = rt::now();
        let evicted = session(now - Duration::from_secs(60));
        let kept = session(now);
        let (evicted_media, kept_media) = (Arc::clone(&evicted.media), Arc::clone(&kept.media));
        let mut sessions = HashMap::from([("evicted".to_string(), evicted), ("kept".to_string(), kept)]);

        evict_to(&mut sessions, now, 2);

        assert!(evicted_media.is_cancelled());
        assert!(!kept_media.is_cancelled());
    }

    #[test]
    fn a_forgotten_sheet_goes_and_the_cap_drops_the_least_recently_used() {
        let now = rt::now();
        let mut sessions = HashMap::from([
            ("idle".to_string(), session(now - IDLE_LIMIT - Duration::from_secs(1))),
            ("old".to_string(), session(now - Duration::from_secs(60))),
            ("recent".to_string(), session(now - Duration::from_secs(5))),
            ("newest".to_string(), session(now)),
        ]);

        evict_to(&mut sessions, now, 3);

        let mut left: Vec<&str> = sessions.keys().map(String::as_str).collect();
        left.sort_unstable();
        assert_eq!(left, vec!["newest", "recent"]);
    }
}

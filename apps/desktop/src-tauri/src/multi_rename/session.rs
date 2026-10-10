//! A Multi-Rename session: the files one open sheet renames, resolved ONCE.
//!
//! Opening the sheet turns the pane's selection (row numbers, checked against the
//! listing's sequence) into file names in that folder, and every preview and the
//! apply work from those names. A file appearing in the folder while the sheet
//! is open shifts the pane's rows but never the session's files; a session file
//! that vanished previews as `Missing`, never as another file.
//!
//! The names and the latest preview stay here: the sheet gets counts and the
//! page of rows it draws, and sends back only the session and preview ids.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::file_system::listing::cached_listing::{LISTING_CACHE, reconciled_cache};
use crate::file_system::listing::metadata::FileEntry;
use crate::ignore_poison::{IgnorePoison, RwLockIgnorePoison};

use super::error::MultiRenameError;
use super::names_file::{self, Written};
use super::plan::{Compiled, MultiRenameSpec, NameEdits, PreviewRow, RowStatus, preview};

/// How many sessions live at once. A sheet is modal, so more than one or two is
/// a window that closed without saying so. Tests share the store and run in
/// parallel, so there the cap is out of reach (`evict` has its own tests).
const MAX_SESSIONS: usize = if cfg!(test) { 1024 } else { 4 };

/// A session nobody touched for this long is a forgotten sheet (a reloaded
/// window, a crash): opening another one drops it.
const IDLE_LIMIT: Duration = Duration::from_secs(30 * 60);

/// How many rows a preview hands back with its counts: the table's first screen.
pub(crate) const FIRST_PAGE: usize = 200;

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
    /// Every other row: a bad or taken name, a duplicate, a file that's gone.
    pub problems: usize,
}

impl PreviewCounts {
    fn of(rows: &[PreviewRow]) -> Self {
        let mut counts = Self::default();
        for row in rows {
            match row.status {
                RowStatus::Ready => counts.ready += 1,
                RowStatus::Unchanged => counts.unchanged += 1,
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
}

static SESSIONS: LazyLock<Mutex<HashMap<String, Session>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// Opens a session over the pane's `selected` rows (backend row numbers in
/// rename order; `None` for every row the pane shows), read at
/// `expected_sequence`. Refuses with `SelectionChanged` when the pane's rows
/// aren't the listing's state any more, rather than reading them in a newer one.
pub(crate) fn open(
    listing_id: &str,
    include_hidden: bool,
    selected: Option<&[usize]>,
    expected_sequence: u64,
) -> Result<MultiRenameOpened, MultiRenameError> {
    let (volume_id, dir, names) = {
        let cache = reconciled_cache(&[listing_id]);
        let listing = cache.get(listing_id).ok_or_else(|| MultiRenameError::Gone {
            listing_id: listing_id.to_string(),
        })?;
        let changed = || MultiRenameError::SelectionChanged {
            listing_id: listing_id.to_string(),
        };
        if listing.sequence.load(Ordering::Acquire) != expected_sequence || listing.include_hidden() != include_hidden {
            return Err(changed());
        }
        let rows = listing.pane_rows();
        let names: Vec<String> = match selected {
            Some(selected) => selected
                .iter()
                .map(|&row| rows.get(row).map(|entry| entry.name.clone()).ok_or_else(changed))
                .collect::<Result<_, _>>()?,
            None => rows.iter().map(|entry| entry.name.clone()).collect(),
        };
        listing.touch();
        (listing.volume_id.clone(), listing.path.as_path().to_path_buf(), names)
    };
    let count = names.len();
    let session_id = format!("multi-rename-{}", NEXT_SESSION.fetch_add(1, Ordering::Relaxed));
    let mut sessions = SESSIONS.lock_ignore_poison();
    evict_to(&mut sessions, Instant::now(), MAX_SESSIONS);
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
            last_used: Instant::now(),
        },
    );
    Ok(MultiRenameOpened { session_id, count })
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

/// Ends a session, and with it its Results file. No-op when it's already gone.
pub(crate) fn close(session_id: &str) {
    SESSIONS.lock_ignore_poison().remove(session_id);
}

/// What a computation needs from a session, copied out so no lock is held while
/// it runs.
struct Inputs {
    listing_id: String,
    volume_id: String,
    dir: PathBuf,
    names: Arc<[String]>,
    edits: Arc<NameEdits>,
}

fn with_session<R>(session_id: &str, f: impl FnOnce(&mut Session) -> R) -> Result<R, MultiRenameError> {
    let mut sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get_mut(session_id).ok_or(MultiRenameError::SessionClosed)?;
    session.last_used = Instant::now();
    Ok(f(session))
}

/// Every row of `names`' preview against the folder as the listing holds it now,
/// with the Results names in `edits`.
fn compute(inputs: &Inputs, compiled: &Compiled, edits: &NameEdits) -> Result<Vec<PreviewRow>, MultiRenameError> {
    let (present, missing, siblings) = {
        let cache = LISTING_CACHE.read_ignore_poison();
        let listing = cache.get(&inputs.listing_id).ok_or_else(|| MultiRenameError::Gone {
            listing_id: inputs.listing_id.clone(),
        })?;
        listing.touch();
        let by_name: HashMap<&str, &FileEntry> = listing.entries().iter().map(|e| (e.name.as_str(), e)).collect();
        let mut present: Vec<(usize, FileEntry)> = Vec::with_capacity(inputs.names.len());
        let mut missing: Vec<usize> = Vec::new();
        for (row, name) in inputs.names.iter().enumerate() {
            match by_name.get(name.as_str()) {
                Some(entry) => present.push((row, (*entry).clone())),
                None => missing.push(row),
            }
        }
        (present, missing, listing.entries().to_vec())
    };
    let rows: Vec<(usize, &FileEntry)> = present.iter().map(|(row, entry)| (*row, entry)).collect();
    let mut previewed = preview(compiled, &inputs.dir, &rows, &siblings, edits);
    previewed.extend(missing.into_iter().map(|row| PreviewRow {
        row,
        old_name: inputs.names[row].clone(),
        new_name: inputs.names[row].clone(),
        status: RowStatus::Missing,
        icon_id: None,
        is_directory: false,
        edited: false,
    }));
    previewed.sort_by_key(|r| r.row);
    Ok(previewed)
}

/// The sheet's live preview of `spec` over the session's files. Stores it as the
/// session's latest unless a newer one already landed.
pub(crate) fn preview_session(
    session_id: &str,
    spec: &MultiRenameSpec,
) -> Result<MultiRenamePreview, MultiRenameError> {
    let compiled = Compiled::new(spec).map_err(|error| MultiRenameError::Spec { error })?;
    let (preview_id, inputs) = with_session(session_id, |s| {
        let id = s.next_preview;
        s.next_preview += 1;
        (id, inputs_of(s))
    })?;
    let rows = compute(&inputs, &compiled, &inputs.edits)?;
    let answer = MultiRenamePreview {
        preview_id,
        counts: PreviewCounts::of(&rows),
        rows: rows.iter().take(FIRST_PAGE).cloned().collect(),
    };
    with_session(session_id, |s| {
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

fn inputs_of(s: &Session) -> Inputs {
    Inputs {
        listing_id: s.listing_id.clone(),
        volume_id: s.volume_id.clone(),
        dir: s.dir.clone(),
        names: Arc::clone(&s.names),
        edits: Arc::clone(&s.edits),
    }
}

/// The stored preview `preview_id`, or `PreviewOutOfDate` when a newer one replaced it.
fn stored(session_id: &str, preview_id: u64) -> Result<(Arc<StoredPreview>, Inputs), MultiRenameError> {
    with_session(session_id, |s| {
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
pub(crate) fn page(
    session_id: &str,
    preview_id: u64,
    offset: usize,
    limit: usize,
    filter: PreviewFilter,
) -> Result<Vec<PreviewRow>, MultiRenameError> {
    let (latest, _) = stored(session_id, preview_id)?;
    let kept = latest
        .rows
        .iter()
        .filter(|row| filter == PreviewFilter::All || row.status.is_problem());
    Ok(kept.skip(offset).take(limit.min(MAX_PAGE)).cloned().collect())
}

/// What apply renames: the rows ready now, proven to be the ones preview
/// `preview_id` showed.
pub(crate) struct Prepared {
    pub volume_id: String,
    pub dir: PathBuf,
    pub ready: Vec<PreviewRow>,
}

/// Recomputes preview `preview_id`'s spec against the folder as it is now and
/// requires its ready rows to be EXACTLY the ones that preview showed.
pub(crate) fn prepare(session_id: &str, preview_id: u64) -> Result<Prepared, MultiRenameError> {
    let (seen, inputs) = stored(session_id, preview_id)?;
    let compiled = Compiled::new(&seen.spec).map_err(|error| MultiRenameError::Spec { error })?;
    let ready: Vec<PreviewRow> = compute(&inputs, &compiled, &seen.edits)?
        .into_iter()
        .filter(|row| row.status.is_ready())
        .collect();
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

/// Results (⌥⏎): writes preview `preview_id`'s rows as `old<TAB>new` lines for
/// the user's editor, and returns the file's path. The session reads back only
/// this file (`read_names`).
pub(crate) fn write_names(session_id: &str, preview_id: u64) -> Result<PathBuf, MultiRenameError> {
    let (latest, _) = stored(session_id, preview_id)?;
    // The old file first: it has the same path, and dropping it deletes it.
    with_session(session_id, |s| s.written = None)?;
    let path = names_file::path_for(session_id);
    let written = names_file::write(&path, &latest.rows)
        .map_err(|e| MultiRenameError::CouldntWriteNames { detail: e.to_string() })?;
    with_session(session_id, |s| s.written = Some(written))?;
    Ok(path)
}

/// Reads the session's Results file back: a line the user changed names its row
/// from now on. Returns how many rows carry a typed name; the next preview shows them.
pub(crate) fn read_names(session_id: &str) -> Result<usize, MultiRenameError> {
    let gone = |detail: String| MultiRenameError::NamesFileGone { detail };
    let path = with_session(session_id, |s| s.written.as_ref().map(|w| w.path.clone()))?
        .ok_or_else(|| gone("no Results file was written".to_string()))?;
    // Read outside the lock; the merge below checks the file is still this session's.
    let text = std::fs::read_to_string(&path).map_err(|e| gone(e.to_string()))?;
    with_session(session_id, |s| {
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
pub(crate) fn clear_names(session_id: &str) -> Result<(), MultiRenameError> {
    with_session(session_id, |s| {
        s.edits = Arc::default();
        s.written = None;
    })
}

#[cfg(test)]
pub(super) fn is_open(session_id: &str) -> bool {
    SESSIONS.lock_ignore_poison().contains_key(session_id)
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
        }
    }

    #[test]
    fn a_forgotten_sheet_goes_and_the_cap_drops_the_least_recently_used() {
        let now = Instant::now();
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

//! The `multi_rename` tool: the Multi-Rename Tool's (⌃M) settings over a pane's files, as a
//! dry-run preview or as a rename proposal the user reviews. ❌ It never renames anything.
//!
//! - **Every sheet knob is a param, and every one is optional**: the wire names are
//!   `MultiRenameSpec`'s own, so a field the sheet gains is a schema line here, not a mapping.
//!   Unset, a knob takes the sheet's blank value (`[N]` / `[E]`, nothing replaced, case as is).
//! - **The files are the pane's, resolved the way every pane tool does it**
//!   (`resolve_pane_target_paths`): `names`, else the selection, else the cursor item.
//! - **The preview is the sheet's own** (`multi_rename::plan::preview_names`), against the
//!   folder's entries as the pane's listing holds them, so a row's status here is the status
//!   the sheet would show. A dry run pages it with `start` / `limit`.
//! - **Media fields (`[=exif.taken]`, `[=tag.artist]`) read through the sheet's own cache and
//!   loader** (`read_media`), inline: an open sheet's reads over the same folder are taken, the
//!   rest read before the preview, capped at `MAX_MEDIA_READS`, stopped if the call goes away.
//! - **`dryRun: false` stages the ready rows as ONE rename group** on the suggested-ops spine
//!   (`agent::suggested_ops::propose`), the same review surface Ask Cmdr's proposals use. The
//!   user approves or rejects it there; the bridge renames on approval, with its live drift
//!   check. Rows that aren't ready are left out and counted.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::{ToolError, ToolResult, fit_to_result_budget, resolve_pane_target_paths, target_pane_state};
use crate::agent::AgentDb;
use crate::agent::store::proposals::{GroupIntent, NewGroup, NewRename, NewSweep, OpSnapshot};
use crate::agent::tools::read::sensitive::{GateStop, Protected, classify, note_refusal, path_gate_within};
use crate::ignore_poison::IgnorePoison;
use crate::multi_rename::media::{FileJob, Loader, SessionMedia};
use crate::multi_rename::plan::{Compiled, MultiRenameSpec, NameEdits, NoMedia, PreviewRow, RowStatus, preview_names};
use crate::security_log::ContentTool;
use cmdr_fs::entry::FileEntry;
use cmdr_media_meta::ReadError;

/// Rows a dry run returns when the caller doesn't say.
const DEFAULT_LIMIT: usize = 50;
/// The most rows one dry run returns, before the size cut.
const MAX_LIMIT: usize = 200;
/// The most renames one proposal carries. A pattern rename is reviewable by its rule and a
/// sample, unlike 10,000 hand-picked names, but past this the review list stops being one the
/// user can scroll with any sense of what's in it.
const MAX_PROPOSED_RENAMES: usize = 10_000;

/// The sheet's blank settings, the base every call's knobs are laid over.
fn blank_spec() -> Value {
    json!({
        "nameMask": "[N]",
        "extensionMask": "[E]",
        "search": "",
        "replace": "",
        "caseSensitive": false,
        "firstOnly": false,
        "includeExtension": false,
        "regex": false,
        "substitute": false,
        "case": "unchanged",
        "removeDiacritics": false,
        "greekToLatin": false,
        "normalizeUnicode": false,
    })
}

/// The spec knobs, which are exactly `MultiRenameSpec`'s camelCase fields.
fn spec_keys() -> Vec<String> {
    match blank_spec() {
        Value::Object(map) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// Which rows a dry run pages through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowFilter {
    All,
    Problems,
}

#[derive(Debug, Clone, PartialEq)]
struct Request {
    spec: MultiRenameSpec,
    names: Option<Vec<String>>,
    dry_run: bool,
    start: usize,
    limit: usize,
    filter: RowFilter,
    rationale: Option<String>,
}

fn parse_request(params: &Value) -> Result<Request, ToolError> {
    let mut spec = blank_spec();
    if let (Value::Object(base), Value::Object(given)) = (&mut spec, params) {
        for key in spec_keys() {
            if let Some(value) = given.get(&key).filter(|v| !v.is_null()) {
                base.insert(key, value.clone());
            }
        }
    }
    let spec: MultiRenameSpec = serde_json::from_value(spec)
        .map_err(|e| ToolError::invalid_params(format!("A rename setting has the wrong type: {e}")))?;

    let names = match params.get("names") {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| ToolError::invalid_params("'names' must be an array of file names"))?,
        ),
        Some(_) => return Err(ToolError::invalid_params("'names' must be an array of file names")),
    };
    let dry_run = match params.get("dryRun") {
        None | Some(Value::Null) => true,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| ToolError::invalid_params("'dryRun' must be true or false"))?,
    };
    let start = optional_whole(params, "start")?.unwrap_or(0);
    let limit = optional_whole(params, "limit")?.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(ToolError::invalid_params(format!(
            "'limit' must be between 1 and {MAX_LIMIT}"
        )));
    }
    let filter = match params.get("filter").and_then(Value::as_str) {
        None | Some("all") => RowFilter::All,
        Some("problems") => RowFilter::Problems,
        Some(other) => {
            return Err(ToolError::invalid_params(format!(
                "Unknown filter '{other}'. Use 'all' or 'problems'."
            )));
        }
    };
    if !dry_run && (params.get("start").is_some() || params.get("limit").is_some() || params.get("filter").is_some()) {
        return Err(ToolError::invalid_params(
            "'start', 'limit', and 'filter' page a dry run. A proposal always carries every ready row.",
        ));
    }
    let rationale = params
        .get("rationale")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    Ok(Request {
        spec,
        names,
        dry_run,
        start,
        limit,
        filter,
        rationale,
    })
}

fn optional_whole(params: &Value, key: &str) -> Result<Option<usize>, ToolError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| ToolError::invalid_params(format!("'{key}' must be a whole number of 0 or more"))),
    }
}

/// How a preview's rows add up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    ready: usize,
    unchanged: usize,
    /// Every other row: a bad or taken name, a duplicate, a file that's gone.
    problems: usize,
}

impl Counts {
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

/// One previewed row, without the sheet's icon key.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RowOut {
    /// The row's place in the rename order: what a counter numbers by.
    row: usize,
    old_name: String,
    new_name: String,
    status: RowStatus,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_directory: bool,
}

/// A dry run's answer: the counts over every row, and the page asked for.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DryRun {
    dry_run: bool,
    folder: String,
    counts: Counts,
    /// Rows the filter keeps, across the whole batch.
    total: usize,
    start: usize,
    returned: usize,
    /// More rows the filter keeps sit past this page (by the limit or the size cut).
    truncated: bool,
    rows: Vec<RowOut>,
    /// Files the sensitive-content gate kept this call from reading: their rows read
    /// `unreadable`, and this is why.
    #[serde(skip_serializing_if = "is_zero")]
    protected_files: usize,
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

fn dry_run_page(folder: &Path, rows: &[PreviewRow], start: usize, limit: usize, filter: RowFilter) -> DryRun {
    let kept: Vec<&PreviewRow> = rows
        .iter()
        .filter(|row| filter == RowFilter::All || row.status.is_problem())
        .collect();
    let total = kept.len();
    let page: Vec<RowOut> = kept
        .into_iter()
        .skip(start)
        .take(limit)
        .map(|row| RowOut {
            row: row.row,
            old_name: row.old_name.clone(),
            new_name: row.new_name.clone(),
            status: row.status.clone(),
            is_directory: row.is_directory,
        })
        .collect();
    let fitted = fit_to_result_budget(page);
    let returned = fitted.items.len();
    DryRun {
        dry_run: true,
        folder: folder.to_string_lossy().into_owned(),
        counts: Counts::of(rows),
        total,
        start,
        returned,
        truncated: start.saturating_add(returned) < total,
        rows: fitted.items,
        protected_files: 0,
    }
}

/// Why the ready rows can't become a proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProposalRefusal {
    NothingReady,
    TooMany { ready: usize },
}

/// The rename group for the batch's ready rows: names under `folder`, each with what its file
/// looked like in the listing, for the bridge's drift check at approval.
fn rename_group(
    volume_id: &str,
    folder: &Path,
    rows: &[PreviewRow],
    siblings: &[FileEntry],
    rationale: Option<String>,
) -> Result<NewGroup, ProposalRefusal> {
    let by_name: HashMap<&str, &FileEntry> = siblings.iter().map(|e| (e.name.as_str(), e)).collect();
    let renames: Vec<NewRename> = rows
        .iter()
        .filter(|row| row.status.is_ready())
        .map(|row| NewRename {
            source_path: folder.join(&row.old_name).to_string_lossy().into_owned(),
            new_name: row.new_name.clone(),
            snapshot: by_name.get(row.old_name.as_str()).map(|entry| OpSnapshot {
                size: entry.size,
                mtime: entry.modified_at.and_then(|t| i64::try_from(t).ok()),
                inode: entry.inode,
            }),
        })
        .collect();
    match renames.len() {
        0 => return Err(ProposalRefusal::NothingReady),
        ready if ready > MAX_PROPOSED_RENAMES => return Err(ProposalRefusal::TooMany { ready }),
        _ => {}
    }
    let parent = folder.to_string_lossy().into_owned();
    let display_name = crate::agent::tools::propose::rename::rename_display_name(&parent, renames.len());
    Ok(NewGroup {
        intent: GroupIntent::Rename { parent, renames },
        source_volume_id: volume_id.to_string(),
        display_name,
        rationale,
        selector: None,
    })
}

fn spec_error(error: crate::multi_rename::plan::SpecError) -> ToolError {
    let data = serde_json::to_value(&error).unwrap_or(Value::Null);
    ToolError::invalid_params("The rename settings don't run: see data.specError.")
        .with_data(json!({ "specError": data }))
}

/// Why the tool refuses a spec the sheet would take, in `data.specError` beside the
/// sheet's own `SpecError`s.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ToolSpecRefusal {
    /// A mask uses `[=source.field]` and more than `max` of the files would have to be
    /// read for it: too slow for one call on a slow volume (S3, MTP), and the sheet
    /// reads in the background with progress.
    TooManyMediaFiles { files: usize, max: usize },
}

fn compile(spec: &MultiRenameSpec) -> Result<Compiled, ToolError> {
    Compiled::new(spec).map_err(spec_error)
}

/// The most media files one call reads for a mask with media fields: at a few per
/// second on a slow volume, still an answer in minutes. Files an open sheet over the
/// same folder already read don't count.
const MAX_MEDIA_READS: usize = 2_000;

/// Stops the read when the call goes away (the client hung up and the request's
/// future is dropped): no new read starts, and one in flight stops at its next fetch.
struct CancelOnDrop(Arc<SessionMedia>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// How long one file's gate may wait on its path resolving: a hung mount, past which the file
/// reads as timed out, which counts toward the loader giving up on the volume.
const GATE_BUDGET: Duration = Duration::from_secs(5);

/// The reasons the gate refused a call's media reads, for the answer and the security log.
#[derive(Clone, Default)]
struct RefusedReads(Arc<Mutex<Vec<Protected>>>);

impl RefusedReads {
    fn take(&self) -> Vec<Protected> {
        std::mem::take(&mut *self.0.lock_ignore_poison())
    }
}

/// `loader` with the sensitive-content gate in front of every read, since a media field is
/// file-derived text (`tag.comment`, `tag.title`, EXIF) and `Propose` runs on wakes too.
/// It runs here, on the read's own blocking thread, never on the dispatch task. A refusal is
/// `NotSupported` so that a folder of refused files isn't read as a dying volume.
fn gated(loader: Loader, refused: &RefusedReads) -> Loader {
    let inner = loader.reader;
    let refused = refused.clone();
    Loader {
        reader: Arc::new(
            move |job: &FileJob| match path_gate_within(&job.path.to_string_lossy(), GATE_BUDGET) {
                Ok(()) => inner(job),
                Err(GateStop::Protected(reason)) => {
                    refused.0.lock_ignore_poison().push(reason);
                    Err(ReadError::NotSupported)
                }
                Err(GateStop::TimedOut) => Err(ReadError::TimedOut),
            },
        ),
        ..loader
    }
}

/// Whether the folder itself passes the gate, off the dispatch task. Anything but a clear yes
/// is a no: this only decides whether another session's answers may be taken.
async fn folder_passes_gate(folder: &Path) -> bool {
    let folder = folder.to_string_lossy().into_owned();
    matches!(
        tokio::task::spawn_blocking(move || path_gate_within(&folder, GATE_BUDGET)).await,
        Ok(Ok(()))
    )
}

/// The EXIF and tags behind a media mask's preview, through the sheet's own cache and
/// loader (`multi_rename::media`), so every value, `NoValue`, `Unreadable`, and
/// `|modified` fallback is the sheet's. Seeded from the same `siblings` the preview
/// runs on, it takes what the `open` sheets over this folder already read, refuses past
/// `max_reads` files left to read, and reads the rest before returning.
async fn read_media(
    folder: &Path,
    names: &[String],
    siblings: &[FileEntry],
    open: &[Arc<SessionMedia>],
    loader: Loader,
    max_reads: usize,
) -> Result<Arc<SessionMedia>, ToolError> {
    let media = Arc::new(SessionMedia::seed(
        "mcp-multi-rename",
        folder.to_path_buf(),
        names,
        siblings,
    ));
    // A sheet read for a person, past the gate. So an answer is taken only where nothing needs
    // resolving, and whatever isn't taken reads through the gated loader, which refuses it.
    if !open.is_empty() && folder_passes_gate(folder).await {
        let links: HashSet<&str> = siblings
            .iter()
            .filter(|entry| entry.is_symlink)
            .map(|entry| entry.name.as_str())
            .collect();
        let may_take = |name: &str| !links.contains(name) && classify(&folder.join(name).to_string_lossy()).is_none();
        for sheet in open {
            media.adopt_where(sheet, may_take);
        }
    }
    let progress = media.progress();
    let to_read = progress.total - progress.read;
    if to_read > max_reads {
        let refusal = ToolSpecRefusal::TooManyMediaFiles {
            files: to_read,
            max: max_reads,
        };
        let data = serde_json::to_value(refusal).unwrap_or(Value::Null);
        return Err(ToolError::invalid_params(format!(
            "The mask's media fields need {} read for their EXIF and tags, and one call reads at most {}. Select \
             fewer files, or ask the user to open the Multi-rename sheet (⌃M), which reads them in the background. \
             See data.specError.",
            crate::pluralize::pluralize(to_read as u64, "file"),
            crate::pluralize::pluralize(max_reads as u64, "file"),
        ))
        .with_data(json!({ "specError": data })));
    }
    // Dropped at the end too, which is harmless: nothing reads after `read_all`.
    let _stop = CancelOnDrop(Arc::clone(&media));
    media.read_all(loader).await;
    Ok(media)
}

/// The sheet's preview of `names`, with the files' media facts when the mask has
/// media fields (`media`), else none.
fn preview(
    compiled: &Compiled,
    folder: &Path,
    names: &[String],
    siblings: &[FileEntry],
    media: Option<&Arc<SessionMedia>>,
) -> Vec<PreviewRow> {
    let edits = NameEdits::default();
    match media {
        // No `Lookup::finish`: the media was read for exactly these `siblings`, so no row
        // found its file changed, and no loader waits for a re-queue.
        Some(media) => preview_names(compiled, folder, names, siblings, &edits, &media.lookup()),
        None => preview_names(compiled, folder, names, siblings, &edits, &NoMedia),
    }
}

pub async fn execute_multi_rename<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let request = parse_request(params)?;
    let (_pane, state) = target_pane_state(app, params)?;
    let volume_id = state
        .volume_id
        .clone()
        .ok_or_else(|| ToolError::internal("The pane's volume isn't available yet"))?;
    let paths = resolve_pane_target_paths(&state, request.names.as_deref())?;
    let folder = PathBuf::from(&state.path);
    let names: Vec<String> = paths
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| ToolError::internal(format!("'{p}' has no file name")))
        })
        .collect::<Result<_, _>>()?;
    let siblings = crate::listing_host::listings()
        .cached_entries(&volume_id, &folder)
        .ok_or_else(|| ToolError::internal("The pane's listing isn't available yet"))?;
    let compiled = compile(&request.spec)?;
    let refused = RefusedReads::default();
    let media = if compiled.uses_media() {
        let open = crate::multi_rename::session::media_over(&volume_id, &folder);
        // No one watches this read's progress: the call's answer is its end.
        let loader = gated(Loader::for_volume(volume_id.clone(), Arc::new(|_| {})), &refused);
        Some(read_media(&folder, &names, &siblings, &open, loader, MAX_MEDIA_READS).await?)
    } else {
        None
    };
    let refused = refused.take();
    for reason in &refused {
        note_refusal(ContentTool::MultiRename, *reason);
    }

    // A mask and a regex per row: off the async worker for a big folder.
    let (rows, siblings) = tokio::task::spawn_blocking({
        let folder = folder.clone();
        move || {
            let rows = preview(&compiled, &folder, &names, &siblings, media.as_ref());
            (rows, siblings)
        }
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?;

    if request.dry_run {
        let page = DryRun {
            protected_files: refused.len(),
            ..dry_run_page(&folder, &rows, request.start, request.limit, request.filter)
        };
        return serde_json::to_value(page).map_err(|e| ToolError::internal(e.to_string()));
    }

    let counts = Counts::of(&rows);
    let group = match rename_group(&volume_id, &folder, &rows, &siblings, request.rationale.clone()) {
        Ok(group) => group,
        Err(ProposalRefusal::NothingReady) => {
            return Err(ToolError::invalid_params(
                "No row is ready to rename, so there's nothing to propose. Run a dry run to see why.",
            )
            .with_data(json!({ "reason": "nothingReady", "counts": counts })));
        }
        Err(ProposalRefusal::TooMany { ready }) => {
            return Err(ToolError::invalid_params(format!(
                "Too many renames for one proposal: this batch has {}, and the cap is {MAX_PROPOSED_RENAMES}. Select fewer files.",
                crate::pluralize::pluralize(ready as u64, "ready row")
            ))
            .with_data(json!({ "reason": "tooMany", "ready": ready, "max": MAX_PROPOSED_RENAMES })));
        }
    };
    let op_count = group.intent.op_count();
    let db_path = app
        .try_state::<AgentDb>()
        .ok_or_else(|| ToolError::internal("Cmdr's suggestion store isn't open yet."))?
        .db_path()
        .to_path_buf();
    let sweep = NewSweep {
        conversation_id: None,
        created_by_model: None,
        rationale: request.rationale,
    };
    let proposed = tokio::task::spawn_blocking(move || {
        let conn = crate::agent::store::open_write_connection(&db_path)?;
        crate::agent::suggested_ops::propose(&conn, &sweep, std::slice::from_ref(&group), now_secs())
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?
    .map_err(|e| ToolError::internal(e.to_string()))?;

    Ok(json!({
        "dryRun": false,
        "proposed": true,
        "sweepId": proposed.set_id,
        "groupId": proposed.group_ids.first(),
        "renames": op_count,
        "counts": counts,
        "protectedFiles": refused.len(),
        "note": "Nothing was renamed. The user reviews the proposal in Cmdr and approves or rejects it.",
    }))
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Unused outside the schema's own test: the knob names, for the schema to stay in step.
#[cfg(test)]
pub(crate) fn spec_knob_names() -> Vec<String> {
    spec_keys()
}

#[cfg(test)]
#[path = "multi_rename_test.rs"]
mod tests;

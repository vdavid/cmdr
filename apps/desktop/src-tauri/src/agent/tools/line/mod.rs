//! What the rail's tool line says about one call: what it looked at ([`ToolSubject`]), what
//! came back ([`ToolOutcome`]), what the model was shown ([`ToolPreview`]), and whether it
//! repeats an earlier call of the same turn ([`repeat_marks`]).
//!
//! Every one is a PURE function of what `main.db` already stores (the call's arguments and its
//! result content), so the live stream and a reloaded thread compute the same facts from the
//! same inputs, and an old thread gets them with no migration. The facts are typed: the
//! frontend picks the words. Depth: `DETAILS.md`.

mod outcome;
mod preview;
mod repeat;
mod subject;

#[cfg(test)]
mod tests;

use std::path::Path;

use serde::Serialize;

pub use outcome::outcome_of;
pub use preview::{PREVIEW_ROWS, preview_of};
pub use repeat::repeat_marks;
pub use subject::subject_of;

use crate::agent::memory::MemoryRefusalKind;
use crate::location::miss::MissToken;
use crate::tools::connect::ConnectOutcome;

/// A line's identity: the assistant row that made the call, plus the call's id. ❌ Never the
/// call id alone: ids collide across rounds (the fake mints `fake-call-{index}` per round, and
/// Gemini's reset per response).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LineRef {
    pub message_id: i64,
    pub call_id: String,
}

/// A path a line names: its text, and whether it's a place the panes can go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LinePath {
    /// `~` expanded, bounded.
    pub text: String,
    /// Only an absolute path the bound left whole: a relative one (the model's own words) resolves
    /// against nothing the panes know, and a cut one names no real place.
    pub linkable: bool,
}

impl LinePath {
    /// The model's (or a result's) `path`, `~` already expanded.
    pub fn of(path: &str) -> Self {
        let text = bounded(path);
        let linkable = text == path && Path::new(path).is_absolute();
        Self { text, linkable }
    }

    /// The path only when it's a place to go: a preview row has its name for the text.
    pub fn linkable_text(path: &str) -> Option<String> {
        let line_path = Self::of(path);
        line_path.linkable.then_some(line_path.text)
    }
}

/// What one call looked at. Every string is bounded; every path is a [`LinePath`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ToolSubject {
    /// `app_state`: both panes.
    Panes,
    /// `list_pane_files`, from its RESULT: the folder and whether only the selection was
    /// listed. Both `None` until the call returns (or when it was refused).
    FocusedPane {
        folder: Option<LinePath>,
        selection_only: Option<bool>,
    },
    /// `list_dir`, `folder_importance`.
    Folder {
        path: LinePath,
    },
    /// `inspect_file`, `image_facts`: the first path asked about and how many were.
    Files {
        first_path: LinePath,
        count: u32,
    },
    /// `search`: the name pattern, plus the scope's INCLUDE paths (excludes dropped).
    Search {
        query: Option<String>,
        scope_paths: Vec<LinePath>,
    },
    /// `search_photos`.
    PhotoSearch {
        query: String,
    },
    ImportantFolders,
    /// `list_volumes`.
    Drives,
    /// `connect_volume`: the volume id the model asked to connect.
    Volume {
        id: String,
    },
    /// `operations_list`.
    OperationLog,
    /// `operations_get`.
    Operation {
        id: String,
    },
    /// `propose_rename_plan`: how many renames the plan carries.
    RenamePlan {
        count: u32,
    },
    /// `list_suggestions`.
    Suggestions,
    /// `get_suggestion_group`.
    SuggestionGroup,
    /// `propose_suggestions`: how many groups the sweep carries.
    SuggestionSweep {
        count: u32,
    },
    /// `propose_server`. No name: the server's name is the model's text, and the card shows it.
    ServerSuggestion,
    NothingToSuggest,
    /// `memory_write`, `memory_edit`: `path` is the model's own, RELATIVE to the memory folder.
    /// `absolute` is set only when it lands inside the folder (and the bound leaves it whole), and
    /// only then is it a place to go.
    MemoryFile {
        path: String,
        absolute: Option<String>,
    },
    /// A name outside the agent's view (hallucinated, a typo). An empty `name` is a legacy
    /// nameless call.
    UnknownTool {
        name: String,
    },
    /// A known tool whose arguments don't carry what it needs.
    Unparsed,
}

/// What came back. Read off OUR OWN result keys, never wording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ToolOutcome {
    /// A paged list: `returned` rows of `total` (absent when the tool doesn't know it).
    Items {
        returned: u32,
        total: Option<u64>,
        lower_bound: bool,
    },
    /// `search` / `search_photos`. `match_count` counts PAST the row cap, so it is never a
    /// `total`; `lower_bound` when ground is still uncovered.
    Matches {
        returned: u32,
        match_count: u64,
        lower_bound: bool,
    },
    /// A search that matched nothing. With `lower_bound` that's "none found YET", ❌ never "no
    /// matches".
    NothingFound {
        lower_bound: bool,
    },
    /// A single-object answer.
    Answered,
    /// The index a photo tool reads isn't there to answer.
    IndexUnavailable {
        reason: IndexUnavailableReason,
    },
    /// A propose tool staged `count` operations for the user to review.
    Staged {
        count: u32,
    },
    /// A propose tool staged nothing (`readyForReview: false`).
    NotStaged,
    /// A memory note landed.
    Saved,
    Refused {
        reason: RefusalReason,
    },
    /// The place couldn't be read or resolved: the location contract's typed miss
    /// (`location/miss.rs`).
    LocationMiss {
        miss: MissToken,
    },
    /// How `connect_volume` went.
    Connection {
        outcome: ConnectOutcome,
    },
}

impl ToolOutcome {
    /// Whether the call got what it asked for. A refusal, a plan that staged nothing, a location
    /// miss, and a volume that didn't end up connected are not ok; everything else is. Agrees with
    /// `dispatch_ok` except where that one reads a typed answer without a `problem` key as ok: a
    /// memory refusal, a rename plan whose evidence was rejected, a location miss, and a connect
    /// that didn't connect.
    pub fn ok(&self) -> bool {
        match self {
            ToolOutcome::Refused { .. } | ToolOutcome::NotStaged | ToolOutcome::LocationMiss { .. } => false,
            ToolOutcome::Connection { outcome } => {
                matches!(outcome, ConnectOutcome::Connected | ConnectOutcome::AlreadyConnected)
            }
            ToolOutcome::Items { .. }
            | ToolOutcome::Matches { .. }
            | ToolOutcome::NothingFound { .. }
            | ToolOutcome::Answered
            | ToolOutcome::IndexUnavailable { .. }
            | ToolOutcome::Staged { .. }
            | ToolOutcome::Saved => true,
        }
    }
}

/// Why the call was turned down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RefusalReason {
    /// `available: false`: not a tool the agent has.
    ToolUnavailable,
    /// The repeat breaker: an identical call already failed this turn.
    RepeatedCall,
    /// The schema gate or the handler said no (the JSON can't tell those two apart).
    Problem,
    Memory {
        refusal: MemoryRefusalKind,
    },
}

/// Why a photo tool's index couldn't answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IndexUnavailableReason {
    ImageIndexingOff,
    SemanticModelNotInstalled,
}

/// The expanded line: a curated look at what the model received. ❌ It never claims the model
/// saw rows it didn't: `rows` is a prefix of what the result carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolPreview {
    /// The first [`PREVIEW_ROWS`] rows. Empty for a tool with no list.
    pub rows: Vec<ToolPreviewRow>,
    /// How many rows the result handed the model.
    pub rows_given_to_model: Option<u32>,
    /// What those rows were cut from (a search's `matchCount`).
    pub total: Option<u64>,
    pub lower_bound: bool,
    /// The verbatim sentence the model was told, when the call was refused. Model-facing English,
    /// shown labeled as such.
    pub problem: Option<String>,
    /// The stored result is a set-aside stub, so there's nothing to show of it.
    pub set_aside: bool,
}

/// One row of a preview. Names, paths, and snippets are file data, so they render as plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolPreviewRow {
    pub name: String,
    /// Where the row is, only when it's a place to go ([`LinePath::linkable_text`]); `name` is
    /// the row's text either way.
    pub path: Option<String>,
    pub kind: ToolPreviewRowKind,
    pub size_bytes: Option<u64>,
    /// A `find` hit's line number.
    pub line: Option<u32>,
    /// A `find` hit's text, or a photo's recognized text.
    pub snippet: Option<String>,
    /// Why this path came back without an answer.
    pub status: Option<ToolPreviewRowStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ToolPreviewRowKind {
    File,
    Folder,
    Other,
}

/// A per-path answer that isn't one: `inspect_file`'s statuses and `image_facts`' not-indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ToolPreviewRowStatus {
    Missing,
    Unreadable,
    Unreachable,
    UnsupportedVolume,
    /// Under a mount point with nothing mounted there: the volume isn't connected.
    NoVolumeAtPath,
    NotIndexed,
    /// The sensitive-content gate kept the file's contents from the assistant.
    Protected,
}

/// The longest any untrusted string on a line gets: a path, a name, a query, a snippet.
pub const MAX_TEXT_CHARS: usize = 512;

/// `text` cut to [`MAX_TEXT_CHARS`], with `…` marking the cut.
fn bounded(text: &str) -> String {
    bounded_to(text, MAX_TEXT_CHARS)
}

fn bounded_to(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// A result's place: `location.path` (the location contract's output), else the bare `path` a
/// thread stored before results carried a `location`.
fn location_path(object: &serde_json::Value) -> Option<&str> {
    object
        .get("location")
        .and_then(|location| location.get("path"))
        .or_else(|| object.get("path"))
        .and_then(serde_json::Value::as_str)
}

/// A count off the wire as a `u32`, saturating: a hostile or huge number never panics.
fn count_u32(value: Option<&serde_json::Value>) -> Option<u32> {
    value
        .and_then(serde_json::Value::as_u64)
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
}

/// A length as a `u32`, saturating.
fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

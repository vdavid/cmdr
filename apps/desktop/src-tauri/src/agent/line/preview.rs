//! The expanded line: the first rows of what the model received, and how much it got.

use serde_json::Value;

use super::{
    LinePath, ToolOutcome, ToolPreview, ToolPreviewRow, ToolPreviewRowKind, ToolPreviewRowStatus, bounded, bounded_to,
    count_u32, location_path, outcome_of,
};
use crate::agent::llm::types::{AgentToolResult, ToolId};
use crate::tools::read::join_child_path;

/// How many rows a preview carries. A taste of what the model saw, not a second listing: the
/// whole result is one Copy away.
pub const PREVIEW_ROWS: usize = 20;

/// A `problem` sentence is the model's whole instruction, so it gets more room than a name.
const MAX_PROBLEM_CHARS: usize = 2_000;

/// What the model received from this call, curated. The counts come from [`outcome_of`], so
/// the line's outcome and its preview can't disagree. Total: an unknown shape gives no rows.
pub fn preview_of(tool: &ToolId, result: &AgentToolResult) -> ToolPreview {
    if result.elided {
        return ToolPreview {
            rows: Vec::new(),
            rows_given_to_model: None,
            total: None,
            lower_bound: false,
            problem: None,
            set_aside: true,
        };
    }
    let content = &result.content;
    let (rows_given_to_model, total, lower_bound) = match outcome_of(tool, result) {
        ToolOutcome::Items {
            returned,
            total,
            lower_bound,
        } => (Some(returned), total, lower_bound),
        ToolOutcome::Matches {
            returned,
            match_count,
            lower_bound,
        } => (Some(returned), Some(match_count), lower_bound),
        ToolOutcome::NothingFound { lower_bound } => (Some(0), Some(0), lower_bound),
        _ => (None, None, false),
    };
    let mut rows = rows_of(tool, content);
    rows.truncate(PREVIEW_ROWS);
    ToolPreview {
        rows,
        rows_given_to_model,
        total,
        lower_bound,
        problem: content
            .get("problem")
            .and_then(Value::as_str)
            .map(|problem| bounded_to(problem, MAX_PROBLEM_CHARS)),
        set_aside: false,
    }
}

/// The rows of a list-carrying result, in the order the model read them. Builds at most
/// [`PREVIEW_ROWS`] from each list, so a 200-row answer costs 20.
fn rows_of(tool: &ToolId, content: &Value) -> Vec<ToolPreviewRow> {
    match tool {
        ToolId::ListDir | ToolId::ListPaneFiles => {
            let parent = location_path(content);
            let list = if matches!(tool, ToolId::ListDir) {
                "children"
            } else {
                "entries"
            };
            items(content, list)
                .filter_map(|entry| {
                    let name = entry.get("name")?.as_str()?;
                    Some(ToolPreviewRow {
                        path: parent.and_then(|parent| LinePath::linkable_text(&join_child_path(parent, name))),
                        kind: file_or_folder(entry, "isDirectory"),
                        size_bytes: entry.get("size").and_then(Value::as_u64),
                        ..row(name)
                    })
                })
                .collect()
        }
        ToolId::ImportantFolders => items(content, "folders")
            .filter_map(|folder| Some(place_row(location_path(folder)?, ToolPreviewRowKind::Folder)))
            .collect(),
        ToolId::ListVolumes => items(content, "volumes")
            .filter_map(|volume| {
                let name = volume.get("name")?.as_str()?;
                Some(ToolPreviewRow {
                    path: volume
                        .get("mountPath")
                        .and_then(Value::as_str)
                        .and_then(LinePath::linkable_text),
                    kind: ToolPreviewRowKind::Folder,
                    ..row(name)
                })
            })
            .collect(),
        ToolId::InspectFile => inspect_rows(content),
        ToolId::Search => items(content, "entries")
            .filter_map(|entry| {
                let name = entry.get("name")?.as_str()?;
                Some(ToolPreviewRow {
                    path: entry
                        .get("path")
                        .and_then(Value::as_str)
                        .and_then(LinePath::linkable_text),
                    kind: file_or_folder(entry, "isDirectory"),
                    size_bytes: entry.get("sizeBytes").and_then(Value::as_u64),
                    ..row(name)
                })
            })
            .collect(),
        ToolId::SearchPhotos => items(content, "hits")
            .filter_map(|hit| {
                Some(ToolPreviewRow {
                    snippet: hit.get("matchText").and_then(Value::as_str).map(bounded),
                    ..path_row(hit, ToolPreviewRowKind::File)?
                })
            })
            .collect(),
        ToolId::ImageFacts => items(content, "facts")
            .filter_map(|facts| {
                let status = match facts.get("state").and_then(Value::as_str) {
                    Some("notIndexed") => Some(ToolPreviewRowStatus::NotIndexed),
                    Some("protected") => Some(ToolPreviewRowStatus::Protected),
                    _ => None,
                };
                Some(ToolPreviewRow {
                    snippet: facts.get("text").and_then(Value::as_str).map(bounded),
                    status,
                    ..path_row(facts, ToolPreviewRowKind::File)?
                })
            })
            .collect(),
        ToolId::GetSuggestionGroup => items(content, "ops")
            .filter_map(|op| {
                Some(ToolPreviewRow {
                    snippet: op.get("newName").and_then(Value::as_str).map(bounded),
                    ..path_row(op, ToolPreviewRowKind::Other)?
                })
            })
            .collect(),
        ToolId::ListSuggestions => items(content, "sweeps")
            .flat_map(|sweep| sweep.get("groups").and_then(Value::as_array).into_iter().flatten())
            .take(PREVIEW_ROWS)
            .filter_map(|group| Some(row(group.get("displayName")?.as_str()?)))
            .collect(),
        // No list to show: the counts, the problem, and Copy say the rest.
        ToolId::AppState
        | ToolId::FolderImportance
        | ToolId::OperationsList
        | ToolId::OperationsGet
        | ToolId::ProposeRenamePlan
        | ToolId::ProposeSuggestions
        | ToolId::ProposeServer
        | ToolId::NothingToSuggest
        | ToolId::ConnectVolume
        | ToolId::MemoryWrite
        | ToolId::MemoryEdit
        | ToolId::Unrecognized(_) => Vec::new(),
    }
}

/// `inspect_file`: one row per file, or one per `find` hit, carrying the per-path status.
fn inspect_rows(content: &Value) -> Vec<ToolPreviewRow> {
    let mut rows = Vec::new();
    for file in items(content, "files") {
        if rows.len() >= PREVIEW_ROWS {
            break;
        }
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        let name = file
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_else(|| base_name(path));
        let status = match file.get("status").and_then(Value::as_str) {
            Some("missing") => Some(ToolPreviewRowStatus::Missing),
            Some("unreadable") => Some(ToolPreviewRowStatus::Unreadable),
            Some("unreachable") => Some(ToolPreviewRowStatus::Unreachable),
            Some("unsupportedVolume") => Some(ToolPreviewRowStatus::UnsupportedVolume),
            Some("noVolumeAtPath") => Some(ToolPreviewRowStatus::NoVolumeAtPath),
            Some("protected") => Some(ToolPreviewRowStatus::Protected),
            _ => None,
        };
        let kind = if file.get("status").and_then(Value::as_str) == Some("folder") {
            ToolPreviewRowKind::Folder
        } else {
            ToolPreviewRowKind::File
        };
        let base = ToolPreviewRow {
            path: LinePath::linkable_text(path),
            kind,
            size_bytes: file.get("sizeBytes").and_then(Value::as_u64),
            status,
            ..row(name)
        };
        let hits = file
            .get("content")
            .and_then(|content| content.get("find"))
            .and_then(|find| find.get("lines"))
            .and_then(Value::as_array);
        match hits {
            Some(hits) if !hits.is_empty() => {
                for hit in hits.iter().take(PREVIEW_ROWS - rows.len()) {
                    rows.push(ToolPreviewRow {
                        // A PDF hit's `line` counts within its page, so it isn't a line number.
                        line: if hit.get("page").is_some() {
                            None
                        } else {
                            count_u32(hit.get("line"))
                        },
                        snippet: hit.get("text").and_then(Value::as_str).map(bounded),
                        ..base.clone()
                    });
                }
            }
            _ => rows.push(base),
        }
    }
    rows
}

/// The first [`PREVIEW_ROWS`] items of the array at `key`.
fn items<'a>(content: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    content
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(PREVIEW_ROWS)
}

/// A row with only a name.
fn row(name: &str) -> ToolPreviewRow {
    ToolPreviewRow {
        name: bounded(name),
        path: None,
        kind: ToolPreviewRowKind::Other,
        size_bytes: None,
        line: None,
        snippet: None,
        status: None,
    }
}

/// A row named after the last component of the object's `path`.
fn path_row(object: &Value, kind: ToolPreviewRowKind) -> Option<ToolPreviewRow> {
    Some(place_row(object.get("path")?.as_str()?, kind))
}

/// A row named after the last component of `path`.
fn place_row(path: &str, kind: ToolPreviewRowKind) -> ToolPreviewRow {
    ToolPreviewRow {
        path: LinePath::linkable_text(path),
        kind,
        ..row(base_name(path))
    }
}

fn file_or_folder(entry: &Value, key: &str) -> ToolPreviewRowKind {
    if entry.get(key).and_then(Value::as_bool) == Some(true) {
        ToolPreviewRowKind::Folder
    } else {
        ToolPreviewRowKind::File
    }
}

fn base_name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

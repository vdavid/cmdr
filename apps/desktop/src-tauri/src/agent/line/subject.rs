//! What a call looked at, from its arguments (and, for `list_pane_files`, its result).

use std::path::Path;

use serde_json::Value;

use super::{LinePath, ToolSubject, bounded, len_u32};
use crate::agent::llm::types::{AgentToolCall, ToolId};
use crate::tools::read::expand_tilde;

/// What `call` looked at. `result` refines it where the arguments can't say (`list_pane_files`
/// takes none: its folder lives in its answer). `memory_root` is the memory folder, so a memory
/// path the model named can become a place to go; `None` leaves it plain text.
///
/// Total: arguments of any shape give a subject, falling back to [`ToolSubject::Unparsed`].
pub fn subject_of(call: &AgentToolCall, result: Option<&Value>, memory_root: Option<&Path>) -> ToolSubject {
    let args = &call.arguments;
    let parsed = match &call.tool {
        ToolId::AppState => Some(ToolSubject::Panes),
        ToolId::ListPaneFiles => Some(focused_pane(result)),
        ToolId::ListDir | ToolId::FolderImportance => path_arg(args, "path").map(|path| ToolSubject::Folder { path }),
        ToolId::InspectFile | ToolId::ImageFacts => files(args),
        ToolId::Search => Some(search(args)),
        ToolId::SearchPhotos => str_arg(args, "query").map(|query| ToolSubject::PhotoSearch { query: bounded(query) }),
        ToolId::ImportantFolders => Some(ToolSubject::ImportantFolders),
        ToolId::ListVolumes => Some(ToolSubject::Drives),
        ToolId::ConnectVolume => str_arg(args, "volumeId").map(|id| ToolSubject::Volume { id: bounded(id) }),
        ToolId::OperationsList => Some(ToolSubject::OperationLog),
        ToolId::OperationsGet => str_arg(args, "operationId").map(|id| ToolSubject::Operation { id: bounded(id) }),
        ToolId::ProposeRenamePlan => array_len(args, "renames").map(|count| ToolSubject::RenamePlan { count }),
        ToolId::ListSuggestions => Some(ToolSubject::Suggestions),
        ToolId::GetSuggestionGroup => Some(ToolSubject::SuggestionGroup),
        ToolId::ProposeSuggestions => array_len(args, "groups").map(|count| ToolSubject::SuggestionSweep { count }),
        ToolId::ProposeServer => Some(ToolSubject::ServerSuggestion),
        ToolId::NothingToSuggest => Some(ToolSubject::NothingToSuggest),
        ToolId::MemoryWrite | ToolId::MemoryEdit => str_arg(args, "path").map(|path| memory_file(path, memory_root)),
        ToolId::Unrecognized(name) => Some(ToolSubject::UnknownTool { name: bounded(name) }),
    };
    parsed.unwrap_or(ToolSubject::Unparsed)
}

/// `list_pane_files`' folder and scope, read off its success shape. A refusal or a still-running
/// call has neither.
fn focused_pane(result: Option<&Value>) -> ToolSubject {
    let answer = result.filter(|content| content.get("entries").is_some());
    ToolSubject::FocusedPane {
        folder: answer.and_then(super::location_path).map(LinePath::of),
        selection_only: answer
            .and_then(|content| content.get("scope"))
            .and_then(Value::as_str)
            .and_then(|scope| match scope {
                "selection" => Some(true),
                "folder" => Some(false),
                _ => None,
            }),
    }
}

fn files(args: &Value) -> Option<ToolSubject> {
    let paths = args.get("paths")?.as_array()?;
    let first_path = paths
        .iter()
        .find_map(Value::as_str)
        .filter(|path| !path.trim().is_empty())?;
    Some(ToolSubject::Files {
        first_path: LinePath::of(&expand_tilde(first_path)),
        count: len_u32(paths.len()),
    })
}

/// The pattern plus the scope's include paths, through the executor's own scope grammar
/// (quotes, escaped commas, `~`), so a path the tool searched is the path the line names.
fn search(args: &Value) -> ToolSubject {
    let scope_paths = str_arg(args, "scope")
        .map(|scope| {
            crate::search::parse_scope(scope)
                .include_paths
                .iter()
                .map(|path| LinePath::of(path))
                .collect()
        })
        .unwrap_or_default();
    ToolSubject::Search {
        query: str_arg(args, "pattern").map(bounded),
        scope_paths,
    }
}

fn memory_file(path: &str, memory_root: Option<&Path>) -> ToolSubject {
    ToolSubject::MemoryFile {
        path: bounded(path),
        absolute: memory_root
            .and_then(|root| crate::agent::memory::shown_location(root, path))
            .and_then(|absolute| LinePath::linkable_text(&absolute.to_string_lossy())),
    }
}

/// A non-blank string argument.
fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// A non-blank path argument, `~` expanded.
fn path_arg(args: &Value, key: &str) -> Option<LinePath> {
    str_arg(args, key).map(|path| LinePath::of(&expand_tilde(path)))
}

fn array_len(args: &Value, key: &str) -> Option<u32> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| len_u32(items.len()))
}

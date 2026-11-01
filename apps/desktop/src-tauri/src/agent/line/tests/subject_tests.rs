//! `subject_of`: what each tool's call looked at.

use std::path::Path;

use serde_json::{Value, json};

use super::call;
use super::fixtures;
use crate::agent::line::{LinePath, MAX_TEXT_CHARS, ToolSubject, subject_of};
use crate::agent::llm::types::ToolId;
use crate::tools::read::pane_listing::PaneListingScope;

fn subject(tool: ToolId, args: Value) -> ToolSubject {
    subject_of(&call(tool, args), None, None)
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

#[test]
fn argument_only_subjects_per_tool() {
    let table = [
        (ToolId::AppState, json!({}), ToolSubject::Panes),
        (
            ToolId::ListDir,
            json!({ "path": "/Users/u/Downloads", "sortBy": "size" }),
            ToolSubject::Folder {
                path: LinePath::of("/Users/u/Downloads"),
            },
        ),
        (
            ToolId::FolderImportance,
            json!({ "path": "/Volumes/NAS" }),
            ToolSubject::Folder {
                path: LinePath::of("/Volumes/NAS"),
            },
        ),
        (
            ToolId::InspectFile,
            json!({ "paths": ["/a.txt", "/b.txt", "/c.txt"], "find": { "query": "x" } }),
            ToolSubject::Files {
                first_path: LinePath::of("/a.txt"),
                count: 3,
            },
        ),
        (
            ToolId::ImageFacts,
            json!({ "paths": ["/p/1.jpg"] }),
            ToolSubject::Files {
                first_path: LinePath::of("/p/1.jpg"),
                count: 1,
            },
        ),
        (
            ToolId::SearchPhotos,
            json!({ "query": "receipts", "mode": "ocr" }),
            ToolSubject::PhotoSearch {
                query: "receipts".into(),
            },
        ),
        (
            ToolId::ImportantFolders,
            json!({ "top": 5 }),
            ToolSubject::ImportantFolders,
        ),
        (ToolId::ListVolumes, json!({}), ToolSubject::Drives),
        (
            ToolId::ConnectVolume,
            json!({ "volumeId": "smb-naspi" }),
            ToolSubject::Volume { id: "smb-naspi".into() },
        ),
        (
            ToolId::ListDir,
            json!({ "path": "/Volumes/naspi/papers", "volumeId": "smb-naspi" }),
            ToolSubject::Folder {
                path: LinePath::of("/Volumes/naspi/papers"),
            },
        ),
        (ToolId::OperationsList, json!({}), ToolSubject::OperationLog),
        (
            ToolId::OperationsGet,
            json!({ "operationId": "op-9" }),
            ToolSubject::Operation { id: "op-9".into() },
        ),
        (
            ToolId::ProposeRenamePlan,
            json!({ "renames": [{}, {}] }),
            ToolSubject::RenamePlan { count: 2 },
        ),
        (ToolId::ListSuggestions, json!({}), ToolSubject::Suggestions),
        (
            ToolId::GetSuggestionGroup,
            json!({ "groupId": 7 }),
            ToolSubject::SuggestionGroup,
        ),
        (
            ToolId::ProposeSuggestions,
            json!({ "groups": [{}] }),
            ToolSubject::SuggestionSweep { count: 1 },
        ),
        (
            ToolId::ProposeServer,
            json!({ "protocol": "sftp", "host": "build.lan" }),
            ToolSubject::ServerSuggestion,
        ),
        (
            ToolId::NothingToSuggest,
            json!({ "reason": "quiet" }),
            ToolSubject::NothingToSuggest,
        ),
        (
            ToolId::Unrecognized("delete".into()),
            json!({ "path": "/" }),
            ToolSubject::UnknownTool { name: "delete".into() },
        ),
        (
            ToolId::Unrecognized(String::new()),
            json!({}),
            ToolSubject::UnknownTool { name: String::new() },
        ),
    ];
    for (tool, args, expected) in table {
        assert_eq!(subject(tool.clone(), args), expected, "{tool:?}");
    }
}

#[test]
fn a_tilde_path_reaches_the_frontend_absolute() {
    assert_eq!(
        subject(ToolId::ListDir, json!({ "path": "~/Downloads" })),
        ToolSubject::Folder {
            path: LinePath::of(&format!("{}/Downloads", home())),
        }
    );
    assert_eq!(
        subject(ToolId::InspectFile, json!({ "paths": ["~/a.txt"] })),
        ToolSubject::Files {
            first_path: LinePath::of(&format!("{}/a.txt", home())),
            count: 1,
        }
    );
}

/// ⚠️ A link hands its path to the panes, so it must be a real place. A relative path (the
/// model's own words) resolves against nothing the panes know, and a cut one names no real place:
/// both keep their text and lose the link.
#[test]
fn only_an_absolute_whole_path_is_a_place_to_go() {
    let long = format!("/{}", "x".repeat(MAX_TEXT_CHARS * 2));
    let folder = |path: &str| match subject(ToolId::ListDir, json!({ "path": path })) {
        ToolSubject::Folder { path } => path,
        other => panic!("a folder subject, got {other:?}"),
    };

    assert!(folder("/Users/u/Downloads").linkable);
    let relative = folder("Downloads");
    assert_eq!(relative.text, "Downloads");
    assert!(!relative.linkable, "relative");
    assert!(!folder(&long).linkable, "cut");

    let ToolSubject::Search { scope_paths, .. } = subject(ToolId::Search, json!({ "scope": "Docs, /Volumes/NAS" }))
    else {
        panic!("a search subject");
    };
    assert_eq!(
        scope_paths.iter().map(|path| path.linkable).collect::<Vec<_>>(),
        [false, true]
    );

    assert_eq!(LinePath::linkable_text("/a/b.txt").as_deref(), Some("/a/b.txt"));
    assert_eq!(LinePath::linkable_text("b.txt"), None);
    assert_eq!(LinePath::linkable_text(&long), None);
}

#[test]
fn search_names_its_pattern_and_only_the_scopes_include_paths() {
    assert_eq!(
        subject(
            ToolId::Search,
            json!({ "pattern": "*.pdf", "scope": "~/Docs, \"/Volumes/My, Drive\", !node_modules" })
        ),
        ToolSubject::Search {
            query: Some("*.pdf".into()),
            scope_paths: vec![
                LinePath::of(&format!("{}/Docs", home())),
                LinePath::of("/Volumes/My, Drive")
            ],
        },
        "the executor's own grammar: quotes keep a comma, `!` excludes drop"
    );
    assert_eq!(
        subject(ToolId::Search, json!({ "sizeMin": "1GB" })),
        ToolSubject::Search {
            query: None,
            scope_paths: Vec::new(),
        },
        "a size-only search names no pattern and no scope"
    );
}

#[test]
fn list_pane_files_takes_its_folder_and_scope_from_the_result() {
    let pane_call = call(ToolId::ListPaneFiles, json!({}));
    let unknown = ToolSubject::FocusedPane {
        folder: None,
        selection_only: None,
    };
    assert_eq!(subject_of(&pane_call, None, None), unknown, "still running");
    assert_eq!(
        subject_of(&pane_call, Some(&fixtures::problem()), None),
        unknown,
        "refused: no folder to name"
    );
    assert_eq!(
        subject_of(
            &pane_call,
            Some(&fixtures::pane_listing(PaneListingScope::Selection, &["a.jpg"], 1)),
            None
        ),
        ToolSubject::FocusedPane {
            folder: Some(LinePath::of("/Users/u/Photos")),
            selection_only: Some(true),
        }
    );
    assert_eq!(
        subject_of(
            &pane_call,
            Some(&fixtures::pane_listing(PaneListingScope::Folder, &["a.jpg"], 1)),
            None
        ),
        ToolSubject::FocusedPane {
            folder: Some(LinePath::of("/Users/u/Photos")),
            selection_only: Some(false),
        }
    );
    // A thread stored before results carried `location` named its folder as a bare `path`.
    let legacy = json!({ "pane": "left", "path": "/Users/u/Old", "scope": "folder", "entries": [] });
    assert_eq!(
        subject_of(&pane_call, Some(&legacy), None),
        ToolSubject::FocusedPane {
            folder: Some(LinePath::of("/Users/u/Old")),
            selection_only: Some(false),
        }
    );
    assert_eq!(
        subject_of(&pane_call, Some(&fixtures::location_miss()), None),
        unknown,
        "a miss lists no folder"
    );
}

#[test]
fn a_memory_path_is_a_place_only_when_it_lands_inside_the_folder() {
    let root = Path::new("/data/ai/memory");
    let memory = |path: &str| {
        subject_of(
            &call(ToolId::MemoryWrite, json!({ "path": path, "content": "x" })),
            None,
            Some(root),
        )
    };

    assert_eq!(
        memory("notes/home.md"),
        ToolSubject::MemoryFile {
            path: "notes/home.md".into(),
            absolute: Some("/data/ai/memory/notes/home.md".into()),
        }
    );
    for escape in ["../../etc/passwd.md", "/tmp/x.md", "notes.sh"] {
        assert_eq!(
            memory(escape),
            ToolSubject::MemoryFile {
                path: escape.into(),
                absolute: None,
            },
            "an escape stays plain text: {escape}"
        );
    }
    assert_eq!(
        subject_of(&call(ToolId::MemoryEdit, json!({ "path": "AGENTS.md" })), None, None),
        ToolSubject::MemoryFile {
            path: "AGENTS.md".into(),
            absolute: None,
        },
        "no memory root, no place to go"
    );
}

#[test]
fn malformed_arguments_fall_back_to_unparsed_never_a_panic() {
    let malformed = [
        (ToolId::ListDir, json!({})),
        (ToolId::ListDir, json!({ "path": 7 })),
        (ToolId::ListDir, json!({ "path": "   " })),
        (ToolId::FolderImportance, json!("not an object")),
        (ToolId::InspectFile, json!({ "paths": [] })),
        (ToolId::InspectFile, json!({ "paths": "/a.txt" })),
        (ToolId::ImageFacts, json!({ "paths": [1, 2] })),
        (ToolId::SearchPhotos, json!({})),
        (ToolId::OperationsGet, json!({ "operationId": 5 })),
        (ToolId::ProposeRenamePlan, json!({ "renames": "all of them" })),
        (ToolId::ProposeSuggestions, json!(null)),
        (ToolId::MemoryWrite, json!({ "content": "x" })),
    ];
    for (tool, args) in malformed {
        assert_eq!(
            subject(tool.clone(), args.clone()),
            ToolSubject::Unparsed,
            "{tool:?} {args}"
        );
    }
}

#[test]
fn every_untrusted_string_is_bounded() {
    let long = "x".repeat(MAX_TEXT_CHARS * 4);
    let ToolSubject::Folder { path } = subject(ToolId::ListDir, json!({ "path": format!("/{long}") })) else {
        panic!("a folder subject");
    };
    assert_eq!(
        path.text.chars().count(),
        MAX_TEXT_CHARS + 1,
        "cut, plus the `…` marking it"
    );
    assert!(path.text.ends_with('…'));
    let ToolSubject::UnknownTool { name } = subject(ToolId::Unrecognized(long.clone()), json!({})) else {
        panic!("an unknown-tool subject");
    };
    assert!(name.chars().count() <= MAX_TEXT_CHARS + 1);
}

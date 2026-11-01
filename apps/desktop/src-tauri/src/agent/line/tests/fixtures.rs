//! Real result shapes, one per tool. Built from the handler's own result struct where it's
//! reachable from here (pane listing, inspect, important folders); the rest copy their serde
//! field names (`tools/actions/` keeps its structs private, and the memory and propose results
//! are `json!` in their handlers).

use serde_json::{Value, json};

use super::to_json;
use crate::location::Location;
use crate::tools::connect::{ConnectAnswer, ConnectOutcome};
use crate::tools::read::importance::{ImportantFoldersResult, RankedFolderOut};
use crate::tools::read::inspect::{
    Content, FileRow, FindHits, FindLine, InspectResult, InspectRow, InspectedFile, TextContent,
};
use crate::tools::read::pane_listing::{PaneListingEntry, PaneListingResult, PaneListingScope};

fn on_root(path: &str) -> Location {
    Location {
        volume_id: "root".into(),
        path: path.into(),
    }
}

fn on_root_row(row: FileRow) -> InspectRow {
    InspectRow {
        volume_id: Some("root".into()),
        row,
    }
}

pub(super) fn pane_listing(scope: PaneListingScope, names: &[&str], total: usize) -> Value {
    to_json(&PaneListingResult {
        pane: "left".into(),
        location: on_root("/Users/u/Photos"),
        scope,
        total,
        returned: names.len(),
        truncated: total > names.len(),
        entries: names
            .iter()
            .map(|name| PaneListingEntry {
                name: (*name).into(),
                is_directory: name.ends_with('/'),
                is_symlink: false,
                size: Some(10),
                modified: None,
            })
            .collect(),
    })
}

fn inspected(path: &str, content: Content) -> InspectRow {
    on_root_row(FileRow::Ok(Box::new(InspectedFile {
        path: path.into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        extension: Some("txt".into()),
        size_bytes: Some(1_234),
        size_human: Some("1.2 KiB".into()),
        modified: None,
        modified_human: None,
        mime: None,
        content,
    })))
}

/// Five paths asked: the first read with two `find` hits, the second missing, the third
/// unreadable, the fourth under a mount point with nothing mounted, and the fifth unanswered.
pub(super) fn inspect_with_find() -> Value {
    let find = |lines: Vec<(usize, &str)>| {
        Content::Text(TextContent {
            encoding: "UTF-8".into(),
            total_lines: Some(900),
            line_numbers_approximate: false,
            window: None,
            find: Some(FindHits {
                total_matches: lines.len(),
                matches_capped: false,
                returned_lines: lines.len(),
                lines: lines
                    .into_iter()
                    .map(|(line, text)| FindLine {
                        page: None,
                        line,
                        matches: 1,
                        text: text.into(),
                    })
                    .collect(),
                truncated: false,
                scan_incomplete: false,
                pages_scanned: None,
                bytes_scanned: None,
                bytes_scanned_human: None,
                total_bytes: None,
                total_bytes_human: None,
            }),
        })
    };
    to_json(&InspectResult::Ok {
        files: vec![
            inspected(
                "/Users/u/notes.txt",
                find(vec![(12, "invoice 2026"), (812, "invoice paid")]),
            ),
            on_root_row(FileRow::Missing {
                path: "/Users/u/gone.txt".into(),
            }),
            on_root_row(FileRow::Unreadable {
                path: "/Users/u/locked.txt".into(),
                reason: crate::tools::read::inspect::UnreadableReason::Permission,
            }),
            on_root_row(FileRow::Protected {
                path: "/Users/u/.ssh/id_rsa".into(),
                reason: crate::tools::sensitive::Protected::Credentials,
                note: crate::tools::sensitive::PROTECTED_NOTE,
            }),
            InspectRow {
                volume_id: None,
                row: FileRow::NoVolumeAtPath {
                    path: "/Volumes/naspi/papers/tax.pdf".into(),
                },
            },
        ],
        total: 6,
        returned: 5,
        truncated: true,
        unanswered: vec!["/Users/u/slow.txt".into()],
        nearby: Some(Vec::new()),
    })
}

pub(super) fn important_folders(paths: &[&str], truncated: bool) -> Value {
    to_json(&ImportantFoldersResult {
        under: None,
        folders: paths
            .iter()
            .map(|path| RankedFolderOut {
                location: on_root(path),
                score: 0.9,
                why: String::new(),
                folded: 0,
                via: None,
            })
            .collect(),
        truncated,
        volumes: Vec::new(),
        empty: None,
    })
}

/// `list_dir` (`read/listing.rs::ListDirResult`): a page of 2 children of 5.
pub(super) fn list_dir() -> Value {
    json!({
        "location": { "volumeId": "root", "path": "/Users/u/Downloads" },
        "coverage": { "indexStatus": "fresh", "authoritative": true },
        "volume": { "id": "root" },
        "children": [
            { "name": "big.iso", "size": 9_000_000_000_u64, "sizeHuman": "8.4 GiB" },
            { "name": "Old", "isDirectory": true, "size": 4_000, "sizeHuman": "3.9 KiB" }
        ],
        "total": 5,
        "returned": 2,
        "truncated": true,
        "sortedBy": "size"
    })
}

/// A location miss (`location/miss.rs::LocationMissResult`), the shape every tool that takes a
/// place answers when it can't place or read it.
pub(super) fn location_miss() -> Value {
    to_json(&crate::location::miss::LocationMissResult {
        miss: crate::location::miss::MissToken::VolumeNotConnected,
        location: crate::location::resolve::LocationInput {
            volume_id: Some("smb-naspi".into()),
            path: "/Volumes/naspi/papers".into(),
        },
        detail: "naspi isn't connected. connect_volume can connect it.".into(),
        root: None,
        nearby: Some(Vec::new()),
        same_share_as: Some(Vec::new()),
    })
}

/// `connect_volume` (`tools/connect.rs::ConnectAnswer`).
pub(super) fn connect(outcome: ConnectOutcome) -> Value {
    let connected = matches!(outcome, ConnectOutcome::Connected | ConnectOutcome::AlreadyConnected);
    to_json(&ConnectAnswer {
        outcome,
        location: connected.then(|| Location {
            volume_id: "smb-naspi".into(),
            path: "/Volumes/naspi".into(),
        }),
        via: None,
        reason: None,
        detail: "What to do next.",
    })
}

/// `search` (`tools/actions/search/result.rs::SearchResult`).
pub(super) fn search(match_count: u64, returned: usize, complete: bool, capped: bool) -> Value {
    let entries: Vec<Value> = (0..returned)
        .map(|i| {
            json!({
                "name": format!("report-{i}.pdf"),
                "path": format!("/Users/u/Docs/report-{i}.pdf"),
                "parentPath": "/Users/u/Docs",
                "isDirectory": false,
                "sizeBytes": 2_048,
                "sizeHuman": "2 KiB"
            })
        })
        .collect();
    json!({
        "targetVolumeId": "root",
        "matchCount": match_count,
        "matchCountHuman": format!("{match_count} matches"),
        "returned": returned,
        "truncated": false,
        "entries": entries,
        "coverage": {
            "complete": complete, "stillWalking": !complete, "foldersFound": 10, "capped": capped,
            "hiddenByExcludes": 0, "permissionDenied": [], "declined": [], "stillCovering": [],
            "unresolvedScopes": [], "uncoveredScopes": [], "abandonedGround": false, "abandonedLocations": 0
        },
        "notes": []
    })
}

/// `search_photos` (`tools/actions/photos.rs::SearchPhotosResult::Ok`).
pub(super) fn search_photos(total: usize, indexing: bool) -> Value {
    let hits: Vec<Value> = (0..total.min(3))
        .map(|i| {
            json!({ "path": format!("/Users/u/Pictures/IMG_{i}.jpg"), "volume": "root", "matchKind": "ocr", "matchText": "RECEIPT" })
        })
        .collect();
    json!({
        "status": "ok",
        "mode": "semantic+ocr",
        "returned": hits.len(),
        "hits": hits,
        "total": total,
        "truncated": false,
        "coverage": [{ "volume": "root", "indexing": indexing, "enrichedCount": 100, "incomplete": indexing }]
    })
}

/// `image_facts` (`tools/actions/image_facts.rs::ImageFactsResult::Ok`).
pub(super) fn image_facts() -> Value {
    json!({
        "status": "ok",
        "facts": [
            { "path": "/Users/u/Pictures/a.jpg", "state": "indexed", "volume": "root", "text": "TOTAL 42.00" },
            { "path": "/Users/u/Pictures/b.jpg", "state": "notIndexed" },
            { "path": "/Users/u/.ssh/c.jpg", "state": "protected", "reason": "credentials" }
        ],
        "total": 3,
        "returned": 3,
        "truncated": false,
        "coverage": []
    })
}

/// `operations_list` (`tools/actions/operation_log.rs`).
pub(super) fn operations_list() -> Value {
    json!({ "operations": [{ "opId": "op-1" }], "count": 1, "total": 3, "returned": 1, "truncated": true })
}

/// `list_volumes` (`read/volumes.rs`): `{ volumes: [VolumeSnapshot] }`.
pub(super) fn list_volumes() -> Value {
    json!({ "volumes": [
        { "name": "Macintosh HD", "id": "root", "kind": "local", "mountPath": "/" },
        { "name": "Pixel", "id": "mtp-1", "kind": "device" }
    ] })
}

/// `list_suggestions` (`suggestions/list.rs`).
pub(super) fn list_suggestions() -> Value {
    json!({
        "status": "pending",
        "sweeps": [{ "sweepId": 1, "createdAt": 0, "createdAtHuman": "today", "groups": [
            { "groupId": 7, "sweepId": 1, "verb": "trash", "status": "pending", "displayName": "Old installers",
              "sourceVolumeId": "root", "reversible": "yes", "opCount": 4 }
        ] }],
        "total": 1,
        "returned": 1,
        "truncated": false
    })
}

/// `get_suggestion_group` (`suggestions/group.rs`).
pub(super) fn suggestion_group() -> Value {
    json!({
        "found": true,
        "group": { "groupId": 7 },
        "ops": [{ "opId": 1, "path": "/Users/u/Downloads/setup.dmg", "status": "pending" }],
        "offset": 0,
        "total": 4,
        "returned": 1,
        "truncated": true
    })
}

/// `propose_suggestions`' staged report (`suggestions/propose.rs::ProposeReport`).
pub(super) fn sweep_staged() -> Value {
    json!({
        "readyForReview": true,
        "sweepId": 3,
        "groups": [
            { "groupId": 1, "verb": "trash", "displayName": "A", "opCount": 4, "outcome": "created" },
            { "groupId": 2, "verb": "move", "displayName": "B", "opCount": 2, "outcome": "amended" }
        ]
    })
}

/// `propose_server`'s staged report: `propose_suggestions`' shape, one zero-op group long.
pub(super) fn server_staged() -> Value {
    json!({
        "readyForReview": true,
        "sweepId": 3,
        "groups": [{ "groupId": 9, "verb": "addServer", "displayName": "Build box", "opCount": 0, "outcome": "created" }]
    })
}

/// `memory_write` / `memory_edit` (`tools/memory.rs::answer`).
pub(super) fn memory_saved() -> Value {
    json!({ "saved": true, "path": "AGENTS.md", "bytes": 120, "remainingBytes": 50_000 })
}

pub(super) fn memory_refused(token: &str) -> Value {
    json!({ "saved": false, "refused": token, "detail": "Memory is full: prune it." })
}

/// The view gate's refusal (`tool_gate.rs::refuse_unavailable`).
pub(super) fn unavailable() -> Value {
    json!({ "available": false, "requested": "delete", "reason": "Ask Cmdr can't touch your files." })
}

/// The repeat breaker's annotation (`chat/runtime/repeats.rs::annotate`): the original problem
/// kept beside the two keys.
pub(super) fn repeated() -> Value {
    json!({ "problem": "list_dir needs path.", "repeatedCall": true, "guidance": "Change the arguments." })
}

pub(super) fn problem() -> Value {
    json!({ "problem": "list_dir needs path. It takes limit and path." })
}

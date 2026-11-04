//! ⌘F end to end on the demo's host: a real `Demo`, its in-memory volume, the
//! folder-size fold, and the Tauri-shaped JSON in and out.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::commands::Demo;
use crate::events::EventSink;
use crate::events_test::{ts_keys, ts_type_of_event};
use crate::search::{SEARCH_CANCELLED, SEARCH_COMPLETE, SEARCH_ERROR, SEARCH_PROGRESS};
use crate::tree::DemoTree;

#[derive(Default)]
struct Collector(Mutex<Vec<(String, Value)>>);

impl EventSink for Collector {
    fn emit(&self, event: &str, payload: Value) {
        self.0
            .lock()
            .expect("collector lock")
            .push((event.to_string(), payload));
    }
}

impl Collector {
    fn for_run(&self, run_id: &str) -> Vec<(String, Value)> {
        let events = self.0.lock().expect("collector lock");
        events.iter().filter(|(_, p)| p["runId"] == run_id).cloned().collect()
    }
}

fn demo() -> (Demo, Arc<Collector>) {
    let tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "home": "/Users/you",
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir" },
            { "path": "/Users/you/Documents", "kind": "dir" },
            { "path": "/Users/you/Documents/Report.pdf", "kind": "file", "size": 50_000, "modifiedMs": 1_700_000_000_000u64 },
            { "path": "/Users/you/Documents/Taxes", "kind": "dir" },
            { "path": "/Users/you/Documents/Taxes/report-2025.pdf", "kind": "file", "size": 2_000_000 },
            { "path": "/Users/you/Pictures", "kind": "dir" },
            { "path": "/Users/you/Pictures/beach.jpg", "kind": "file", "size": 3_000_000 },
            { "path": "/Users/you/Pictures/Café", "kind": "dir" },
            { "path": "/Users/you/Pictures/Café/crème.jpg", "kind": "file", "size": 10 },
            { "path": "/Users/you/node_modules", "kind": "dir" },
            { "path": "/Users/you/node_modules/report.js", "kind": "file", "size": 1 },
            { "path": "/Users/you/Shortcut", "kind": "symlink", "symlinkTarget": "/Users/you/Documents" },
        ],
    }))
    .expect("tree parses");
    let events = Arc::new(Collector::default());
    let demo = Demo::new(tree, Arc::clone(&events) as _).expect("valid tree");
    (demo, events)
}

fn query(over: Value) -> Value {
    let mut q = json!({
        "namePattern": null, "patternType": "glob", "minSize": null, "maxSize": null,
        "modifiedAfter": null, "modifiedBefore": null, "isDirectory": null, "includePaths": null,
        "excludeDirNames": null, "includePathIds": null, "countOnly": false, "limit": 30,
        "caseSensitive": null, "excludeSystemDirs": null, "sortBy": null,
    });
    for (k, v) in over.as_object().expect("an object").clone() {
        q[k] = v;
    }
    q
}

async fn call(demo: &Demo, command: &str, args: Value) -> Value {
    demo.invoke(command, args)
        .await
        .unwrap_or_else(|e| panic!("{command} refused: {e}"))
}

async fn search(demo: &Demo, over: Value) -> Value {
    call(demo, "search_files", json!({ "query": query(over) })).await
}

fn paths(result: &Value) -> Vec<String> {
    let mut paths: Vec<String> = result["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["path"].as_str().expect("a path").to_string())
        .collect();
    paths.sort();
    paths
}

#[tokio::test(flavor = "multi_thread")]
async fn the_index_is_ready_on_first_ask() {
    let (demo, _) = demo();
    let prepared = call(&demo, "prepare_search_index", json!({ "volumeId": null })).await;
    assert_eq!(prepared["ready"], true);
    assert_eq!(prepared["loading"], false);
    // The root plus every entry the tree lists.
    assert_eq!(prepared["entryCount"], 14);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_glob_finds_files_case_insensitively_and_leaves_out_system_folders() {
    let (demo, _) = demo();
    let found = search(&demo, json!({ "namePattern": "REPORT*" })).await;
    assert_eq!(
        paths(&found),
        [
            "/Users/you/Documents/Report.pdf",
            "/Users/you/Documents/Taxes/report-2025.pdf"
        ]
    );
    assert_eq!(
        found["hiddenByExcludes"], 1,
        "`node_modules/report.js` is a system folder's"
    );
    assert_eq!(found["targetVolumeId"], "root");
    let parent = found["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["name"] == "Report.pdf")
        .expect("found")["parentPath"]
        .clone();
    assert_eq!(parent, "~/Documents", "rows read from home, as on a Mac");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_accented_name_matches_the_pattern_as_typed() {
    let (demo, _) = demo();
    let found = search(&demo, json!({ "namePattern": "crème*" })).await;
    assert_eq!(paths(&found), ["/Users/you/Pictures/Café/crème.jpg"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scope_resolves_through_the_fold_and_a_stranger_comes_back_unresolved() {
    let (demo, _) = demo();
    let scoped = search(
        &demo,
        json!({ "namePattern": "*.pdf", "includePaths": ["~/documents/TAXES"] }),
    )
    .await;
    assert_eq!(paths(&scoped), ["/Users/you/Documents/Taxes/report-2025.pdf"]);

    let elsewhere = search(&demo, json!({ "namePattern": "*", "includePaths": ["/Volumes/naspi"] })).await;
    assert_eq!(elsewhere["entries"], json!([]));
    assert_eq!(elsewhere["unresolvedScopes"], json!(["/Volumes/naspi"]));

    // A scope naming a symlinked folder resolves through the link, as the desktop's
    // `realpath` does (`InMemoryVolume::canonicalize`): it searches the folder the link
    // points at, and the rows carry their real paths (#959).
    let through_link = search(&demo, json!({ "namePattern": "*", "includePaths": ["~/Shortcut"] })).await;
    assert_eq!(
        paths(&through_link),
        [
            "/Users/you/Documents",
            "/Users/you/Documents/Report.pdf",
            "/Users/you/Documents/Taxes",
            "/Users/you/Documents/Taxes/report-2025.pdf",
        ]
    );
    assert_eq!(through_link["unresolvedScopes"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_size_filter_and_fill_read_the_fold_the_panes_show() {
    let (demo, _) = demo();
    let big = search(
        &demo,
        json!({ "isDirectory": true, "minSize": 2_500_000, "namePattern": "*" }),
    )
    .await;
    assert_eq!(paths(&big), ["/Users", "/Users/you", "/Users/you/Pictures"]);
    let pictures = big["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["name"] == "Pictures")
        .expect("found")
        .clone();
    assert_eq!(pictures["size"], 3_000_010, "the fold's recursive logical size");
    let stats = call(&demo, "get_dir_stats", json!({ "path": "/Users/you/Pictures" })).await;
    assert_eq!(
        stats["recursiveSize"], pictures["size"],
        "one folder-size truth: {stats}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_write_reaches_the_next_search() {
    let (demo, _) = demo();
    assert_eq!(
        paths(&search(&demo, json!({ "namePattern": "Fresh" })).await),
        Vec::<String>::new()
    );
    let made = call(
        &demo,
        "create_directory",
        json!({ "volumeId": "root", "parentPath": "/Users/you", "name": "Fresh", "initiator": "user" }),
    )
    .await;
    assert_eq!(made["type"], "done", "{made}");
    assert_eq!(
        paths(&search(&demo, json!({ "namePattern": "Fresh" })).await),
        ["/Users/you/Fresh"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_streaming_run_reports_the_whole_answer_and_the_terminal_coverage() {
    let (demo, events) = demo();
    let started = call(
        &demo,
        "search_files_streaming",
        json!({ "query": query(json!({ "namePattern": "*.pdf" })), "runId": "r1", "order": 1 }),
    )
    .await;
    assert_eq!(started, json!({ "runId": "r1", "targetVolumeId": "root" }));
    let run = events.for_run("r1");
    let names: Vec<&str> = run.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names.first(), Some(&SEARCH_PROGRESS));
    assert_eq!(names.last(), Some(&SEARCH_COMPLETE));
    let rows: usize = run
        .iter()
        .filter(|(n, _)| n == SEARCH_PROGRESS)
        .map(|(_, p)| p["entries"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(rows, 2);
    let (_, complete) = run.last().expect("a terminal event");
    assert_eq!(complete["matchCount"], 2);
    assert_eq!(complete["coverage"]["walk"], "nothingToWalk");
    assert_eq!(complete["coverage"]["kind"], "covered");
    assert_eq!(complete["coverage"]["unresolvedScopes"], json!([]), "{complete}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_pattern_fails_the_run_as_a_query_error() {
    let (demo, events) = demo();
    call(
        &demo,
        "search_files_streaming",
        json!({ "query": query(json!({ "namePattern": "(", "patternType": "regex" })), "runId": "bad", "order": 1 }),
    )
    .await;
    let run = events.for_run("bad");
    let (name, error) = run.last().expect("a terminal event");
    assert_eq!(name, SEARCH_ERROR);
    assert_eq!(error["error"], "query");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_run_events_carry_exactly_the_keys_bindings_declare() {
    let (demo, events) = demo();
    call(
        &demo,
        "search_files_streaming",
        json!({ "query": query(json!({ "namePattern": "*" })), "runId": "k", "order": 1 }),
    )
    .await;
    call(
        &demo,
        "search_files_streaming",
        json!({ "query": query(json!({ "namePattern": "[" , "patternType": "regex" })), "runId": "e", "order": 2 }),
    )
    .await;
    let mut seen = BTreeSet::new();
    for (name, payload) in events.0.lock().expect("collector lock").iter() {
        let keys: BTreeSet<String> = payload.as_object().expect("an object").keys().cloned().collect();
        assert_eq!(keys, ts_keys(ts_type_of_event(name)), "{name}");
        seen.insert(name.clone());
    }
    assert!(seen.contains(SEARCH_PROGRESS) && seen.contains(SEARCH_COMPLETE) && seen.contains(SEARCH_ERROR));
    // A cancelled run's event names its type in `bindings.ts` too.
    assert_eq!(ts_type_of_event(SEARCH_CANCELLED), "SearchCancelledEvent");
}

#[tokio::test(flavor = "multi_thread")]
async fn recent_searches_dedupe_cap_and_clear_for_the_session() {
    let (demo, _) = demo();
    let entry = |id: &str, query: &str| {
        json!({
            "id": id, "timestamp": 1, "mode": "filename", "query": query, "filters": {},
            "scope": "", "caseSensitive": false, "excludeSystemDirs": true, "resultCount": 2,
        })
    };
    call(
        &demo,
        "add_recent_search",
        json!({ "entry": entry("a", "*.pdf"), "maxCount": null }),
    )
    .await;
    call(
        &demo,
        "add_recent_search",
        json!({ "entry": entry("b", "*.jpg"), "maxCount": null }),
    )
    .await;
    call(
        &demo,
        "add_recent_search",
        json!({ "entry": entry("c", "  *.PDF "), "maxCount": null }),
    )
    .await;
    let ids = |list: Value| -> Vec<String> {
        list.as_array()
            .expect("a list")
            .iter()
            .map(|e| e["id"].as_str().expect("an id").to_string())
            .collect()
    };
    assert_eq!(
        ids(call(&demo, "get_recent_searches", json!({ "limit": null })).await),
        ["c", "b"]
    );
    assert_eq!(
        ids(call(&demo, "get_recent_searches", json!({ "limit": 1 })).await),
        ["c"]
    );
    call(&demo, "apply_recent_searches_max_count", json!({ "maxCount": 1 })).await;
    assert_eq!(
        ids(call(&demo, "get_recent_searches", json!({ "limit": null })).await),
        ["c"]
    );
    call(&demo, "remove_recent_search", json!({ "id": "c" })).await;
    assert_eq!(
        call(&demo, "get_recent_searches", json!({ "limit": null })).await,
        json!([])
    );
    call(
        &demo,
        "add_recent_search",
        json!({ "entry": entry("d", "x"), "maxCount": 5 }),
    )
    .await;
    call(&demo, "clear_recent_searches", json!({})).await;
    assert_eq!(
        call(&demo, "get_recent_searches", json!({ "limit": null })).await,
        json!([])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_small_commands_answer_as_the_desktop_does() {
    let (demo, _) = demo();
    let parsed = call(
        &demo,
        "parse_search_scope",
        json!({ "scope": "~/Documents, !node_modules" }),
    )
    .await;
    assert_eq!(parsed["includePaths"], json!(["/Users/you/Documents"]), "{parsed}");
    let excludes = call(&demo, "get_system_dir_excludes", json!({})).await;
    assert!(
        excludes
            .as_array()
            .is_some_and(|e| e.iter().any(|n| n == "node_modules"))
    );
    let sorted = call(
        &demo,
        "sort_search_results",
        json!({
            "rows": [
                { "name": "img_10.png", "isDirectory": false, "size": 300, "modifiedAt": 30 },
                { "name": "img_2.png", "isDirectory": false, "size": 100, "modifiedAt": 20 },
            ],
            "sortBy": "name", "sortOrder": "ascending", "dirSortMode": "likeFiles",
        }),
    )
    .await;
    assert_eq!(sorted, json!({ "order": [1, 0], "effectiveSortBy": "name" }));
    assert_eq!(call(&demo, "cancel_search", json!({ "runId": "nobody" })).await, false);
    assert_eq!(
        call(&demo, "release_search_index", json!({ "keepRunId": null })).await,
        Value::Null
    );
}

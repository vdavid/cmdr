//! The commands end to end: a real engine, a real in-memory volume, and the
//! Tauri-shaped JSON in and out.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::commands::{COMMANDS, Demo};
use crate::events::EventSink;
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
    fn names_for(&self, listing_id: &str) -> Vec<String> {
        let events = self.0.lock().expect("collector lock");
        events
            .iter()
            .filter(|(_, p)| p["listingId"] == listing_id)
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn payload(&self, listing_id: &str, event: &str) -> Option<Value> {
        let events = self.0.lock().expect("collector lock");
        events
            .iter()
            .find(|(name, p)| name == event && p["listingId"] == listing_id)
            .map(|(_, p)| p.clone())
    }
}

fn demo() -> (Demo, Arc<Collector>) {
    let tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir" },
            { "path": "/Users/you/Documents", "kind": "dir" },
            { "path": "/Users/you/Music", "kind": "dir" },
            { "path": "/Users/you/report.pdf", "kind": "file", "size": 4096, "modifiedMs": 1_700_000_000_000u64 },
            { "path": "/Users/you/notes 10.txt", "kind": "file", "size": 30 },
            { "path": "/Users/you/notes 9.txt", "kind": "file", "size": 20 },
            { "path": "/Users/you/.zshrc", "kind": "file", "size": 12 },
        ],
    }))
    .expect("tree parses");
    let events = Arc::new(Collector::default());
    let demo = Demo::new(tree, Arc::clone(&events) as _).expect("valid tree");
    (demo, events)
}

async fn call(demo: &Demo, command: &str, args: Value) -> Value {
    demo.invoke(command, args)
        .await
        .unwrap_or_else(|e| panic!("{command} refused: {e}"))
}

/// Starts a listing of `path` and waits for its `listing-complete`.
async fn open(demo: &Demo, events: &Collector, id: &str, path: &str) -> Value {
    let started = call(
        demo,
        "list_directory_start_streaming",
        json!({
            "volumeId": "root", "path": path, "includeHidden": false,
            "sortBy": "name", "sortOrder": "ascending", "listingId": id,
        }),
    )
    .await;
    assert_eq!(started, json!({ "listingId": id, "status": { "status": "loading" } }));
    for _ in 0..500 {
        if let Some(done) = events.payload(id, "listing-complete") {
            return done;
        }
        if let Some(error) = events.payload(id, "listing-error") {
            panic!("listing {id} failed: {error}");
        }
        // allowed-test-sleep: polls for the engine's own background task, with a hard cap.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("listing {id} never completed: {:?}", events.names_for(id));
}

fn names(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .expect("an array of entries")
        .iter()
        .map(|e| e["name"].as_str().expect("a name"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_listing_streams_its_events_and_serves_rows_in_the_apps_order() {
    let (demo, events) = demo();
    let done = open(&demo, &events, "pane-1", "/Users/you").await;
    assert_eq!(done["totalCount"], 5, "the dotfile is hidden");
    assert_eq!(done["volumeRoot"], "/");
    assert_eq!(done["effectiveSortBy"], "name");
    let seen = events.names_for("pane-1");
    assert_eq!(seen.first().map(String::as_str), Some("listing-opening"));
    assert!(seen.contains(&"listing-read-complete".to_string()), "{seen:?}");

    let rows = call(
        &demo,
        "get_file_range",
        json!({ "listingId": "pane-1", "start": 0, "count": 10, "includeHidden": false }),
    )
    .await;
    // Folders first, then names in natural order: `9` before `10`.
    assert_eq!(
        names(&rows),
        ["Documents", "Music", "notes 9.txt", "notes 10.txt", "report.pdf"]
    );
    assert_eq!(rows[4]["size"], 4096);
    assert_eq!(rows[4]["modifiedAt"], 1_700_000_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn lookups_resorts_and_filters_answer_like_the_desktop() {
    let (demo, events) = demo();
    open(&demo, &events, "pane-2", "/Users/you").await;
    let id = "pane-2";

    let index = call(
        &demo,
        "find_file_index",
        json!({ "listingId": id, "name": "report.pdf", "includeHidden": false }),
    )
    .await;
    assert_eq!(index, 4);
    let fuzzy = call(
        &demo,
        "find_first_fuzzy_match",
        json!({ "listingId": id, "query": "mus", "includeHidden": false }),
    )
    .await;
    assert_eq!(fuzzy, 1);
    let at = call(
        &demo,
        "get_file_at",
        json!({ "listingId": id, "index": 0, "includeHidden": true }),
    )
    .await;
    assert_eq!(at["name"], "Documents");

    let resorted = call(
        &demo,
        "resort_listing",
        json!({
            "listingId": id, "sortBy": "size", "sortOrder": "descending",
            "cursorFilename": "report.pdf", "includeHidden": false,
        }),
    )
    .await;
    assert_eq!(resorted["totalCount"], 5);
    assert_eq!(resorted["newCursorIndex"], 2, "folders first, then the biggest file");

    let shown = call(
        &demo,
        "set_listing_include_hidden",
        json!({ "listingId": id, "includeHidden": true }),
    )
    .await;
    assert_eq!(shown["totalCount"], 6);

    let filtered = call(
        &demo,
        "set_listing_name_filter",
        json!({
            "listingId": id, "pattern": "notes", "includeHidden": true,
            "selectedIndices": [], "refuseEmpty": false,
        }),
    )
    .await;
    assert_eq!(filtered["totalCount"], 2, "{filtered}");

    let stats = call(
        &demo,
        "get_listing_stats",
        json!({ "listingId": id, "includeHidden": true }),
    )
    .await;
    assert!(stats["totalFiles"].is_number(), "{stats}");

    let refreshed = call(&demo, "refresh_listing", json!({ "listingId": id, "force": true })).await;
    assert_eq!(refreshed, json!({ "data": null, "timedOut": false }));

    let alive = call(
        &demo,
        "keep_listings_alive",
        json!({ "listingIds": [id, "never-opened"] }),
    )
    .await;
    assert_eq!(alive, json!(["never-opened"]));
    call(&demo, "list_directory_end", json!({ "listingId": id })).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_listing_rejects_with_the_typed_error() {
    let (demo, _) = demo();
    let refused = demo
        .invoke(
            "get_file_range",
            json!({ "listingId": "gone", "start": 0, "count": 1, "includeHidden": false }),
        )
        .await;
    assert_eq!(refused, Err(json!({ "type": "gone", "listingId": "gone" })));
}

/// A folder the tree doesn't hold ends in the same typed `notFound` a disk's
/// missing folder does.
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_folder_ends_in_not_found() {
    let (demo, events) = demo();
    let error = open_failing(&demo, &events, "pane-3", "root", "/nope").await;
    assert_eq!(error["error"]["reason"]["reason"], "notFound", "{error}");
}

/// A volume id nothing serves ends in a typed `listing-error` too.
#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_volume_ends_in_a_typed_listing_error() {
    let (demo, events) = demo();
    let error = open_failing(&demo, &events, "pane-4", "usb", "/").await;
    assert_eq!(error["error"]["reason"]["reason"], "notFound", "{error}");
}

/// Starts a listing that should fail and waits for its `listing-error`.
async fn open_failing(demo: &Demo, events: &Collector, id: &str, volume_id: &str, path: &str) -> Value {
    call(
        demo,
        "list_directory_start_streaming",
        json!({
            "volumeId": volume_id, "path": path, "includeHidden": false,
            "sortBy": "name", "sortOrder": "ascending", "listingId": id,
        }),
    )
    .await;
    for _ in 0..500 {
        if let Some(error) = events.payload(id, "listing-error") {
            return error;
        }
        // allowed-test-sleep: polls for the engine's own background task, with a hard cap.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no listing-error: {:?}", events.names_for(id));
}

#[tokio::test]
async fn unknown_commands_and_bad_arguments_reject_with_a_string() {
    let (demo, _) = demo();
    assert!(matches!(
        demo.invoke("copy_files", json!({})).await,
        Err(Value::String(_))
    ));
    assert!(matches!(
        demo.invoke("get_file_range", json!({ "listingId": 7 })).await,
        Err(Value::String(_))
    ));
    assert!(!Demo::handles("copy_files"));
    assert!(COMMANDS.iter().all(|c| Demo::handles(c)));
}

//! The write commands end to end, natively: the real write engine on the page's
//! one in-memory volume, Tauri-shaped JSON in and out, and the listing engine's
//! panes hearing every change as `directory-diff`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::testing::wait_until_async;
use serde_json::{Value, json};

use crate::commands::Demo;
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
    fn find(&self, event: &str, matches: impl Fn(&Value) -> bool) -> Option<Value> {
        let events = self.0.lock().expect("collector lock");
        events
            .iter()
            .find(|(name, p)| name == event && matches(p))
            .map(|(_, p)| p.clone())
    }

    fn count(&self, event: &str) -> usize {
        let events = self.0.lock().expect("collector lock");
        events.iter().filter(|(name, _)| name == event).count()
    }

    async fn wait_for(&self, event: &str, matches: impl Fn(&Value) -> bool) -> Value {
        let description = format!("a `{event}`");
        wait_until_async(Duration::from_secs(10), &description, || {
            self.find(event, &matches).is_some()
        })
        .await;
        self.find(event, &matches).expect("it arrived")
    }

    /// Waits for `operation_id`'s `write-settled`, and answers its terminal event.
    async fn settled(&self, operation_id: &str) -> String {
        let of = |p: &Value| p["operationId"] == operation_id;
        self.wait_for("write-settled", of).await;
        ["write-complete", "write-cancelled", "write-error"]
            .into_iter()
            .find(|name| self.find(name, of).is_some())
            .expect("a terminal event before write-settled")
            .to_string()
    }
}

const BIG: u64 = 64 * 1024 * 1024;

fn demo() -> (Demo, Arc<Collector>) {
    let tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir" },
            { "path": "/Users/you/Documents", "kind": "dir" },
            { "path": "/Users/you/Documents/b.txt", "kind": "file", "size": 7 },
            { "path": "/Users/you/a.txt", "kind": "file", "size": 1_000 },
            { "path": "/Users/you/b.txt", "kind": "file", "size": 2_000 },
            { "path": "/Users/you/c.txt", "kind": "file", "size": 3_000 },
            { "path": "/Users/you/big.bin", "kind": "file", "size": BIG },
            { "path": "/Users/you/dated.txt", "kind": "file", "size": 10, "modifiedMs": 1_700_000_000_000u64 },
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

/// Opens a listing of `path` as `id` and waits until it's complete.
async fn open(demo: &Demo, events: &Collector, id: &str, path: &str) {
    call(
        demo,
        "list_directory_start_streaming",
        json!({
            "volumeId": "root", "path": path, "includeHidden": false,
            "sortBy": "name", "sortOrder": "ascending", "listingId": id,
        }),
    )
    .await;
    events.wait_for("listing-complete", |p| p["listingId"] == id).await;
}

/// The names `id`'s listing shows now.
async fn names(demo: &Demo, id: &str) -> Vec<String> {
    let rows = call(
        demo,
        "get_file_range",
        json!({ "listingId": id, "start": 0, "count": 100, "includeHidden": false }),
    )
    .await;
    rows.as_array()
        .expect("rows")
        .iter()
        .map(|r| r["name"].as_str().expect("name").to_string())
        .collect()
}

fn config(conflict_resolution: &str) -> Value {
    json!({ "progressIntervalMs": 50, "conflictResolution": conflict_resolution, "maxConflictsToShow": 100 })
}

async fn copy(demo: &Demo, sources: &[&str], dest: &str, conflict_resolution: &str) -> String {
    let started = call(
        demo,
        "copy_between_volumes",
        json!({
            "sourceVolumeId": "root", "sourcePaths": sources, "destVolumeId": "root", "destPath": dest,
            "config": config(conflict_resolution), "initiator": "user",
        }),
    )
    .await;
    started["operationId"].as_str().expect("an operation id").to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_lands_every_file_and_the_open_pane_sees_it() {
    let (demo, events) = demo();
    open(&demo, &events, "docs", "/Users/you/Documents").await;
    let op = copy(
        &demo,
        &["/Users/you/a.txt", "/Users/you/c.txt"],
        "/Users/you/Documents",
        "stop",
    )
    .await;
    assert_eq!(events.settled(&op).await, "write-complete");
    events.wait_for("directory-diff", |p| p["listingId"] == "docs").await;
    assert_eq!(names(&demo, "docs").await, ["a.txt", "b.txt", "c.txt"]);
    let rows = call(
        &demo,
        "get_recent_operation_log_entries",
        json!({ "limit": 10, "offset": 0 }),
    )
    .await;
    assert_eq!(rows.as_array().expect("rows").len(), 1, "the copy journaled: {rows}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_asks_and_skip_keeps_the_destination() {
    let (demo, events) = demo();
    let op = copy(&demo, &["/Users/you/b.txt"], "/Users/you/Documents", "stop").await;
    let asked = events.wait_for("write-conflict", |p| p["operationId"] == op).await;
    let resolved = call(
        &demo,
        "resolve_write_conflict",
        json!({ "operationId": op, "conflictId": asked["conflictId"], "resolution": "skip", "applyToAll": false }),
    )
    .await;
    assert_eq!(resolved, json!("resolved"), "{resolved}");
    assert_eq!(events.settled(&op).await, "write-complete");
    let space = call(&demo, "get_volume_space", json!({ "path": "/" })).await;
    assert_eq!(space["data"]["kind"], "bounded", "{space}");
    let detail = call(
        &demo,
        "get_operation_log_detail",
        json!({ "operationId": op, "itemLimit": 10, "itemOffset": 0 }),
    )
    .await;
    assert!(detail.is_object(), "{detail}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paced_copy_cancels() {
    let (demo, events) = demo();
    let op = copy(&demo, &["/Users/you/big.bin"], "/Users/you/Documents", "stop").await;
    events.wait_for("write-progress", |p| p["operationId"] == op).await;
    let queue = call(&demo, "list_operations", json!({})).await;
    assert_eq!(queue.as_array().expect("ops").len(), 1, "{queue}");
    call(
        &demo,
        "cancel_write_operation",
        json!({ "operationId": op, "rollback": false }),
    )
    .await;
    let ended = events.settled(&op).await;
    assert_eq!(ended, "write-cancelled", "{:?}", events.find("write-error", |_| true));
}

#[tokio::test(flavor = "multi_thread")]
async fn move_delete_rename_and_new_folder_reach_the_pane() {
    let (demo, events) = demo();
    open(&demo, &events, "home", "/Users/you").await;

    let made = call(
        &demo,
        "create_directory",
        json!({ "volumeId": "root", "parentPath": "/Users/you", "name": "New", "initiator": "user" }),
    )
    .await;
    assert_eq!(made["type"], "done", "{made}");

    let renamed = call(
        &demo,
        "rename_file",
        json!({ "from": "/Users/you/a.txt", "to": "/Users/you/z.txt", "force": false, "volumeId": "root", "initiator": "user" }),
    )
    .await;
    assert_eq!(renamed["type"], "done", "{renamed}");

    // The desktop's same-`root` move: with no local disk it's a volume move.
    let moved = call(
        &demo,
        "move_files",
        json!({ "sources": ["/Users/you/c.txt"], "destination": "/Users/you/New", "config": config("stop"), "initiator": "user" }),
    )
    .await;
    let moved = moved["operationId"].as_str().expect("id").to_string();
    assert_eq!(events.settled(&moved).await, "write-complete");

    let deleted = call(
        &demo,
        "delete_files",
        json!({ "sources": ["/Users/you/b.txt"], "volumeId": "root", "initiator": "user" }),
    )
    .await;
    let deleted = deleted["operationId"].as_str().expect("id").to_string();
    assert_eq!(events.settled(&deleted).await, "write-complete");

    // The volume patches the pane as it writes, so it shows every change by the settle.
    assert_eq!(
        names(&demo, "home").await,
        ["Documents", "New", "big.bin", "dated.txt", "z.txt"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn undo_reverses_a_copy() {
    let (demo, events) = demo();
    open(&demo, &events, "docs", "/Users/you/Documents").await;
    let op = copy(&demo, &["/Users/you/a.txt"], "/Users/you/Documents", "stop").await;
    assert_eq!(events.settled(&op).await, "write-complete");
    let report = call(&demo, "undo_operations", json!({ "operationIds": [op] })).await;
    assert_eq!(report["restored"], 1, "{report}");
    assert_eq!(names(&demo, "docs").await, ["b.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_trash_refuses_typed_as_on_a_volume_without_one() {
    let (demo, events) = demo();
    let refused = demo
        .invoke(
            "trash_files",
            json!({ "sources": ["/Users/you/a.txt"], "initiator": "user" }),
        )
        .await
        .expect_err("no trash here");
    assert_eq!(refused["type"], "trash_not_supported", "{refused}");
    let refused = demo
        .invoke(
            "move_to_trash",
            json!({ "path": "/Users/you/a.txt", "initiator": "user" }),
        )
        .await
        .expect_err("no trash here");
    assert!(refused.is_object(), "typed, not a sentence: {refused}");
    assert_eq!(events.count("write-complete"), 0);
}

/// The dialogs' path questions see what the engine wrote, not the page's first tree.
#[tokio::test(flavor = "multi_thread")]
async fn the_path_questions_see_every_write() {
    let (demo, _events) = demo();
    let made = call(
        &demo,
        "create_directory",
        json!({ "volumeId": "root", "parentPath": "/Users/you", "name": "New", "initiator": "user" }),
    )
    .await;
    assert_eq!(made["type"], "done", "{made}");
    for command in ["path_exists", "destination_exists"] {
        let answer = call(&demo, command, json!({ "volumeId": "root", "path": "/Users/you/New" })).await;
        assert_eq!(answer, json!({ "data": true, "timedOut": false }), "{command}");
    }
    let kinds = call(
        &demo,
        "stat_paths_kinds",
        json!({ "paths": ["/Users/you/New", "/Users/you/a.txt", "/Users/you/gone"] }),
    )
    .await;
    assert_eq!(kinds, json!({ "data": [true, false, null], "timedOut": false }));
    let echo = call(
        &demo,
        "destination_root_echo",
        json!({ "destVolumeId": "root", "destPath": "/Users/you" }),
    )
    .await;
    assert_eq!(echo, Value::Null, "a volume rooted at `/` has no root to echo");
}

#[test]
fn every_write_command_is_routed_to_the_write_engine() {
    for command in Demo::commands() {
        assert!(Demo::handles(command), "{command}");
    }
    assert!(Demo::handles("copy_between_volumes"));
    assert!(Demo::handles("list_operations"));
}

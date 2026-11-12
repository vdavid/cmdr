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

/// Waits until the queue no longer holds `operation_id`, for an operation its
/// command registered before answering (a rollback's reversal), so that "not
/// held" can only mean it settled. It asks the queue itself: a reversal this
/// small can settle before its registration broadcasts, and then no
/// `operations-changed` ever lists it.
async fn left_the_queue(demo: &Demo, operation_id: &str) {
    let description = format!("`{operation_id}` to leave the queue");
    wait_until_async(Duration::from_secs(5), &description, || {
        let queue = demo.operations().engine().list_operations();
        queue.iter().all(|row| row.operation_id != operation_id)
    })
    .await;
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

/// The Operation log's item tree answers from the real journal, and a request field
/// the demo hasn't learned is refused rather than dropped.
#[tokio::test(flavor = "multi_thread")]
async fn the_log_tree_answers_and_refuses_a_field_it_doesnt_know() {
    let (demo, events) = demo();
    let op = copy(&demo, &["/Users/you/a.txt"], "/Users/you/Documents", "stop").await;
    assert_eq!(events.settled(&op).await, "write-complete");
    let request = json!({ "expanded": [], "selection": null, "cursor": null, "offset": 0, "limit": 50 });
    let view = call(
        &demo,
        "get_operation_log_tree",
        json!({ "operationId": op, "request": request }),
    )
    .await;
    assert_eq!(view["summary"]["kind"], "built", "{view}");
    assert_eq!(view["rows"][0]["label"], "a.txt", "{view}");
    assert_eq!(view["tally"]["selectable"], 1, "{view}");

    let mut unknown = request.clone();
    unknown["selected"] = json!([]);
    let refused = demo
        .invoke(
            "get_operation_log_tree",
            json!({ "operationId": op, "request": unknown }),
        )
        .await;
    assert!(refused.is_err(), "an unknown field must not be dropped: {refused:?}");
}

/// The demo's Roll back takes the tree's selection: unchecking one copied file
/// reverses only the other, the op stays partial, and a field the demo hasn't
/// learned is refused rather than dropped (a dropped `selection` would reverse
/// everything while the dialog said "selected").
#[tokio::test(flavor = "multi_thread")]
async fn a_selective_rollback_reverses_only_what_was_selected() {
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
    let request = json!({ "expanded": [], "selection": null, "cursor": null, "offset": 0, "limit": 50 });
    let view = call(
        &demo,
        "get_operation_log_tree",
        json!({ "operationId": op, "request": request }),
    )
    .await;
    let a = view["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["label"] == "a.txt")
        .unwrap_or_else(|| panic!("a.txt has a row: {view}"));
    let selection =
        json!({ "base": "everything", "toggles": [{ "kind": "item", "seq": a["key"]["seq"], "on": false }] });

    let mut unknown = json!({ "operationId": op, "initiator": "user", "selection": selection });
    unknown["selected"] = json!([]);
    let refused = demo.invoke("rollback_operation", unknown).await;
    assert!(refused.is_err(), "an unknown field must not be dropped: {refused:?}");

    let dispatched = call(
        &demo,
        "rollback_operation",
        json!({ "operationId": op, "initiator": "user", "selection": selection }),
    )
    .await;
    let inverse = dispatched["inverseOpId"].as_str().expect("an inverse id").to_string();
    // A reversal ends with no `write-complete` of its own: it has settled once the
    // queue it joined no longer lists it, and it resolves the op before it leaves.
    left_the_queue(&demo, &inverse).await;
    assert_eq!(
        names(&demo, "docs").await,
        ["a.txt", "b.txt"],
        "only the selected copy went"
    );
    let rows = call(
        &demo,
        "get_recent_operation_log_entries",
        json!({ "limit": 10, "offset": 0 }),
    )
    .await;
    let original = rows
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["opId"] == op)
        .unwrap_or_else(|| panic!("the copy is listed: {rows}"));
    assert_eq!(original["rollbackState"], "partiallyRolledBack", "{original}");
}

/// A rename on the demo volume undoes through the planner, dated or not: the
/// rename journals the size and mtime its one `get_metadata` probe read, so the
/// planner has something to verify.
#[tokio::test(flavor = "multi_thread")]
async fn a_rename_undoes() {
    let (demo, events) = demo();
    open(&demo, &events, "home", "/Users/you").await;
    for (from, to) in [("dated.txt", "dated2.txt"), ("a.txt", "a2.txt"), ("Documents", "Docs")] {
        let renamed = call(
            &demo,
            "rename_file",
            json!({
                "from": format!("/Users/you/{from}"), "to": format!("/Users/you/{to}"),
                "force": false, "volumeId": "root", "initiator": "user",
            }),
        )
        .await;
        assert_eq!(renamed["type"], "done", "{renamed}");
        let log = call(
            &demo,
            "get_recent_operation_log_entries",
            json!({ "limit": 1, "offset": 0 }),
        )
        .await;
        let report = call(&demo, "undo_operations", json!({ "operationIds": [log[0]["opId"]] })).await;
        assert_eq!(report["restored"], 1, "{from}: {report}");
    }
    assert_eq!(
        names(&demo, "home").await,
        ["Documents", "a.txt", "b.txt", "big.bin", "c.txt", "dated.txt"]
    );
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
    // The deleted-folder poll asks this every 2 s with no volume id; a "no" walked a pane on `/` home.
    let root = call(&demo, "path_exists", json!({ "volumeId": null, "path": "/" })).await;
    assert_eq!(
        root,
        json!({ "data": true, "timedOut": false }),
        "the volume root is there"
    );
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

/// The page's tree with only `free_bytes` of room left on the disk.
fn demo_with_free_bytes(free_bytes: u64) -> (Demo, Arc<Collector>) {
    let tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "freeBytes": free_bytes,
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir" },
            { "path": "/Users/you/Documents", "kind": "dir" },
            { "path": "/Users/you/a.txt", "kind": "file", "size": 1_000 },
            { "path": "/Users/you/c.txt", "kind": "file", "size": 3_000 },
        ],
    }))
    .expect("tree parses");
    let events = Arc::new(Collector::default());
    let demo = Demo::new(tree, Arc::clone(&events) as _).expect("valid tree");
    (demo, events)
}

/// A copy onto a full disk fails, the queue keeps it with a Retry, and the Retry
/// lands it once a delete has made room.
#[tokio::test(flavor = "multi_thread")]
async fn a_copy_onto_a_full_disk_is_kept_for_retry_and_lands_once_there_is_room() {
    let (demo, events) = demo_with_free_bytes(2_500);
    let space = call(&demo, "get_volume_space", json!({ "path": "/" })).await;
    assert_eq!(space["data"]["availableBytes"], 2_500, "{space}");

    let op = copy(&demo, &["/Users/you/c.txt"], "/Users/you/Documents", "stop").await;
    assert_eq!(events.settled(&op).await, "write-error");
    let queue = call(&demo, "list_operations", json!({})).await;
    let failed = queue
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["operationId"] == op)
        .unwrap_or_else(|| panic!("the failure stays in the queue: {queue}"));
    assert_eq!(failed["canRetry"], true, "{failed}");

    let deleted = call(
        &demo,
        "delete_files",
        json!({ "sources": ["/Users/you/a.txt"], "volumeId": "root", "initiator": "user" }),
    )
    .await;
    let deleted = deleted["operationId"].as_str().expect("id").to_string();
    assert_eq!(events.settled(&deleted).await, "write-complete");

    let retried = call(&demo, "retry_failed_operation", json!({ "operationId": op })).await;
    let again = retried["operationId"]
        .as_str()
        .unwrap_or_else(|| panic!("a Retry starts the copy again: {retried}"))
        .to_string();
    assert_eq!(events.settled(&again).await, "write-complete");
    open(&demo, &events, "docs", "/Users/you/Documents").await;
    assert_eq!(names(&demo, "docs").await, ["c.txt"]);
}

#[test]
fn every_write_command_is_routed_to_the_write_engine() {
    for command in Demo::commands() {
        assert!(Demo::handles(command), "{command}");
    }
    assert!(Demo::handles("copy_between_volumes"));
    assert!(Demo::handles("list_operations"));
}

/// The visitor's home folder, and every folder holding it, refuse delete and the
/// delete dialog's preview, typed, as on the desktop. Regression anchor: deleting
/// `/Users` used to go through and land the pane on "Path not found".
#[tokio::test(flavor = "multi_thread")]
async fn the_home_folder_and_its_holders_refuse_delete() {
    let tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "home": "/Users/you",
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir" },
            { "path": "/Users/you/a.txt", "kind": "file", "size": 10 },
            { "path": "/Users/guest", "kind": "dir" },
        ],
    }))
    .expect("tree parses");
    let events = Arc::new(Collector::default());
    let demo = Demo::new(tree, Arc::clone(&events) as _).expect("valid tree");

    for (source, relation) in [
        ("~", "isHome"),
        ("/Users/you/", "isHome"),
        ("/Users", "holdsHome"),
        ("/", "holdsHome"),
    ] {
        let refused = demo
            .invoke(
                "delete_files",
                json!({ "sources": [source], "volumeId": "root", "initiator": "user" }),
            )
            .await
            .expect_err("a delete of the home folder refuses");
        assert_eq!(refused["type"], "home_folder", "{source}: {refused}");
        assert_eq!(refused["relation"], relation, "{source}: {refused}");

        let preview = demo
            .invoke(
                "start_scan_preview",
                json!({ "sources": [source], "sourceVolumeId": "root", "purpose": "delete" }),
            )
            .await
            .expect_err("the delete dialog's preview refuses");
        assert_eq!(preview["type"], "home_folder", "{source}: {preview}");
    }

    // The dialogs ask first, so no confirm UI shows for a refused one.
    for action in ["delete", "move", "rename"] {
        let answer = call(
            &demo,
            "check_home_folder_guard",
            json!({ "sources": ["/Users"], "volumeId": "root", "action": action }),
        )
        .await;
        assert_eq!(answer["type"], "home_folder", "{action}: {answer}");
        assert_eq!(answer["action"], action, "{answer}");
        let clear = call(
            &demo,
            "check_home_folder_guard",
            json!({ "sources": ["/Users/guest"], "volumeId": "root", "action": action }),
        )
        .await;
        assert!(clear.is_null(), "{action}: {clear}");
    }
    let moved = demo
        .invoke(
            "move_files",
            json!({ "sources": ["/Users/you"], "destination": "/Users/guest", "initiator": "user" }),
        )
        .await
        .expect_err("a move of the home folder refuses");
    assert_eq!(moved["action"], "move", "{moved}");

    // Another user's home folder deletes as before.
    let deleted = call(
        &demo,
        "delete_files",
        json!({ "sources": ["/Users/guest"], "volumeId": "root", "initiator": "user" }),
    )
    .await;
    let deleted = deleted["operationId"].as_str().expect("id").to_string();
    assert_eq!(events.settled(&deleted).await, "write-complete");
}

/// The demo says when the journal's folder-index build settled, as the app's host does, so
/// a tree that answered `indexesBuilding` reads again on the event rather than a timer.
#[tokio::test(flavor = "multi_thread")]
async fn the_demo_says_when_the_trees_folder_indexes_settled() {
    let (_demo, events) = demo();
    let ready = events.wait_for("operation-log-tree-indexes-settled", |_| true).await;
    assert_eq!(ready, json!({}));
}

/// The tree backfill answers as the desktop's does: a fresh op is built, an unknown one
/// is unknown, and an op journaled before the summary existed backfills on the page's
/// event loop with its steps out as `operation-log-tree-backfill`.
#[tokio::test(flavor = "multi_thread")]
async fn an_old_op_backfills_its_tree_summary_and_says_so() {
    let (demo, events) = demo();
    let op = copy(&demo, &["/Users/you/a.txt"], "/Users/you/Documents", "stop").await;
    assert_eq!(events.settled(&op).await, "write-complete");
    let start = |id: &str| call(&demo, "start_operation_log_tree_backfill", json!({ "operationId": id }));
    assert_eq!(start(&op).await, json!({ "kind": "built" }));
    assert_eq!(start("nope").await, json!({ "kind": "unknownOperation" }));

    let writer = demo.operations().engine().operation_log().writer().expect("open");
    writer.flush_blocking().expect("flush");
    let conn = cmdr_fs::sqlite_util::open(demo.operations().log_db()).expect("journal");
    conn.execute_batch(&format!(
        "PRAGMA busy_timeout = 5000;
         DELETE FROM operation_tree_dirs WHERE op_id = '{op}';
         UPDATE operations SET tree_summary = NULL WHERE op_id = '{op}';"
    ))
    .expect("an old op");
    assert_eq!(start(&op).await, json!({ "kind": "started" }));
    let finished = events
        .wait_for("operation-log-tree-backfill", |p| p["step"]["kind"] == "finished")
        .await;
    assert_eq!(
        finished,
        json!({ "opId": op, "step": { "kind": "finished", "detail": { "kind": "built" } } })
    );
    assert_eq!(start(&op).await, json!({ "kind": "built" }));

    let strict = demo
        .invoke(
            "cancel_operation_log_tree_backfill",
            json!({ "operationId": op, "extra": 1 }),
        )
        .await;
    assert!(strict.is_err(), "an unknown field refuses loudly");
}

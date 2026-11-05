//! ⌃M end to end on the demo's host: a real `Demo`, its in-memory volume, the panes,
//! the write engine and its journal, and the Tauri-shaped JSON in and out.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::testing::wait_until_async;
use serde_json::{Value, json};

use crate::commands::Demo;
use crate::events::EventSink;
use crate::events_test::{BINDINGS, ts_keys, ts_type_of_event};
use crate::multi_rename::{MULTI_RENAME_COMMANDS, MULTI_RENAME_MEDIA_PROGRESS};
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
    fn all(&self, event: &str) -> Vec<Value> {
        let events = self.0.lock().expect("collector lock");
        events
            .iter()
            .filter(|(name, _)| name == event)
            .map(|(_, p)| p.clone())
            .collect()
    }

    async fn wait_for(&self, event: &str, matches: impl Fn(&Value) -> bool) -> Value {
        let description = format!("a `{event}`");
        wait_until_async(Duration::from_secs(10), &description, || {
            self.all(event).iter().any(&matches)
        })
        .await;
        self.all(event).into_iter().find(matches).expect("it arrived")
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
            { "path": "/Users/you/Pictures", "kind": "dir" },
            { "path": "/Users/you/Pictures/beach.jpg", "kind": "file", "size": 3_000, "modifiedMs": 1_700_000_000_000u64 },
            { "path": "/Users/you/Pictures/dunes.jpg", "kind": "file", "size": 2_000, "modifiedMs": 1_700_000_000_000u64 },
            { "path": "/Users/you/Pictures/notes.txt", "kind": "file", "size": 10 },
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

async fn refused(demo: &Demo, command: &str, args: Value) -> Value {
    match demo.invoke(command, args).await {
        Ok(answer) => panic!("{command} answered {answer}"),
        Err(error) => error,
    }
}

/// Lists the pictures as `pics`, and opens a sheet over every row.
async fn open_sheet(demo: &Demo, events: &Collector) -> Value {
    call(
        demo,
        "list_directory_start_streaming",
        json!({
            "volumeId": "root", "path": "/Users/you/Pictures", "includeHidden": false,
            "sortBy": "name", "sortOrder": "ascending", "listingId": "pics",
        }),
    )
    .await;
    events.wait_for("listing-complete", |p| p["listingId"] == "pics").await;
    call(
        demo,
        "open_multi_rename",
        json!({ "listingId": "pics", "includeHidden": false, "selectedIndices": null, "expectedSequence": 0 }),
    )
    .await
}

fn spec(name_mask: &str) -> Value {
    json!({
        "nameMask": name_mask, "extensionMask": "[E]", "search": "", "replace": "",
        "caseSensitive": false, "firstOnly": false, "includeExtension": false, "regex": false,
        "substitute": false, "case": "unchanged", "removeDiacritics": false,
    })
}

fn new_names(preview: &Value) -> Vec<String> {
    preview["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| r["newName"].as_str().expect("a new name").to_string())
        .collect()
}

async fn names(demo: &Demo) -> Vec<String> {
    let rows = call(
        demo,
        "get_file_range",
        json!({ "listingId": "pics", "start": 0, "count": 100, "includeHidden": false }),
    )
    .await;
    rows.as_array()
        .expect("rows")
        .iter()
        .map(|r| r["name"].as_str().expect("name").to_string())
        .collect()
}

#[test]
fn the_engine_answers_every_multi_rename_command_the_app_has() {
    let in_bindings: BTreeSet<&str> = BINDINGS
        // `__TAURI_INVOKE<T>('name', …)` or `__TAURI_INVOKE(\n 'name', …)`: the first quoted word.
        .split("__TAURI_INVOKE")
        .skip(1)
        .filter_map(|rest| rest.split('\'').nth(1))
        .filter(|name| name.contains("multi_rename"))
        .collect();
    let ours: BTreeSet<&str> = MULTI_RENAME_COMMANDS.iter().copied().collect();
    assert_eq!(ours, in_bindings);
    for command in MULTI_RENAME_COMMANDS {
        assert!(Demo::handles(command), "{command}");
        assert!(Demo::commands().any(|c| c == *command), "{command}");
    }
    assert_eq!(
        ts_type_of_event(MULTI_RENAME_MEDIA_PROGRESS),
        "MultiRenameMediaProgress"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sheet_opens_previews_moves_renames_and_rolls_back() {
    let (demo, events) = demo();
    let opened = open_sheet(&demo, &events).await;
    assert_eq!(opened["count"], 3);
    assert_eq!(opened["resultsAvailable"], false, "a page has no Results");
    let session = opened["sessionId"].as_str().expect("a session").to_string();
    assert_eq!(
        call(&demo, "get_multi_rename_last_settings", json!({})).await,
        Value::Null
    );

    let preview = call(
        &demo,
        "preview_multi_rename",
        json!({ "sessionId": session, "spec": spec("[N]_[C:2]") }),
    )
    .await;
    assert_eq!(new_names(&preview), ["beach_01.jpg", "dunes_02.jpg", "notes_03.txt"]);
    assert_eq!(preview["counts"]["ready"], 3);

    // A move retires the preview on screen: paging it and applying it wait for the next.
    assert_eq!(
        call(
            &demo,
            "move_multi_rename_row",
            json!({ "sessionId": session, "row": 2, "direction": "up" }),
        )
        .await,
        1
    );
    let stale = json!({ "sessionId": session, "previewId": preview["previewId"] });
    assert_eq!(
        refused(&demo, "apply_multi_rename", stale.clone()).await["type"],
        "previewOutOfDate"
    );
    let preview = call(
        &demo,
        "preview_multi_rename",
        json!({ "sessionId": session, "spec": spec("[N]_[C:2]") }),
    )
    .await;
    assert_eq!(new_names(&preview), ["beach_01.jpg", "notes_02.txt", "dunes_03.jpg"]);
    let page = call(
        &demo,
        "get_multi_rename_preview_rows",
        json!({ "sessionId": session, "previewId": preview["previewId"], "offset": 1, "limit": 1, "filter": "all" }),
    )
    .await;
    assert_eq!(page[0]["newName"], "notes_02.txt");

    let started = call(
        &demo,
        "apply_multi_rename",
        json!({ "sessionId": session, "previewId": preview["previewId"] }),
    )
    .await;
    assert_eq!(started["renaming"], 3);
    let op = started["operationId"].as_str().expect("an operation").to_string();
    events.wait_for("write-settled", |p| p["operationId"] == op).await;
    events.wait_for("directory-diff", |p| p["listingId"] == "pics").await;
    assert_eq!(names(&demo).await, ["beach_01.jpg", "dunes_03.jpg", "notes_02.txt"]);

    // The fields' history took the mask that renamed, on top of a new install's seeds.
    let history = call(&demo, "get_multi_rename_history", json!({})).await;
    let values: Vec<&str> = history
        .as_array()
        .expect("a list")
        .iter()
        .map(|e| e["value"].as_str().expect("a value"))
        .collect();
    assert_eq!(values, ["[N]_[C:2]", "[Y-M-D h.m.s]", "[N]"]);

    // Roll back: the journal's undo puts every name back.
    let diffs = events.all("directory-diff").len();
    let report = call(&demo, "undo_operations", json!({ "operationIds": [op] })).await;
    assert_eq!(report["restored"], 3, "{report}");
    wait_until_async(Duration::from_secs(10), "the undo's diff", || {
        events.all("directory-diff").len() > diffs
    })
    .await;
    assert_eq!(names(&demo).await, ["beach.jpg", "dunes.jpg", "notes.txt"]);
    call(&demo, "close_multi_rename", json!({ "sessionId": session })).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_media_mask_shows_unreadable_rows_and_the_read_ends_done() {
    let (demo, events) = demo();
    let session = open_sheet(&demo, &events).await["sessionId"]
        .as_str()
        .expect("a session")
        .to_string();
    let last = events
        .wait_for(MULTI_RENAME_MEDIA_PROGRESS, |p| p["done"] == true)
        .await;
    assert_eq!(
        last,
        json!({ "sessionId": session, "read": 2, "total": 2, "done": true })
    );
    // Throttled: the first and the final, never one per file.
    assert!(events.all(MULTI_RENAME_MEDIA_PROGRESS).len() <= 2);
    for payload in events.all(MULTI_RENAME_MEDIA_PROGRESS) {
        let keys: BTreeSet<String> = payload.as_object().expect("an object").keys().cloned().collect();
        assert_eq!(keys, ts_keys("MultiRenameMediaProgress"));
    }
    // A sheet that subscribes late asks, and hears the same.
    assert_eq!(
        call(
            &demo,
            "get_multi_rename_media_progress",
            json!({ "sessionId": session })
        )
        .await,
        json!({ "read": 2, "total": 2, "done": true })
    );

    let preview = call(
        &demo,
        "preview_multi_rename",
        json!({ "sessionId": session, "spec": spec("[=exif.taken]") }),
    )
    .await;
    let statuses: Vec<&Value> = preview["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| &r["status"])
        .collect();
    assert_eq!(statuses[0], &json!({ "type": "unreadable", "field": "exifTaken" }));
    assert_eq!(statuses[1], &json!({ "type": "unreadable", "field": "exifTaken" }));
    // A file that isn't media has no value, which isn't the read's fault.
    assert_eq!(statuses[2]["type"], "noValue");
    assert_eq!(preview["counts"]["ready"], 0);

    // With the modified-date fallback, the photos rename (2023, in any time zone).
    let fallback = call(
        &demo,
        "preview_multi_rename",
        json!({ "sessionId": session, "spec": spec("[=exif.taken|modified:Y] [N]") }),
    )
    .await;
    assert_eq!(
        new_names(&fallback)[..2],
        ["2023 beach.jpg", "2023 dunes.jpg"],
        "{fallback}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn results_refuse_typed_with_nowhere_to_write() {
    let (demo, events) = demo();
    let session = open_sheet(&demo, &events).await["sessionId"]
        .as_str()
        .expect("a session")
        .to_string();
    let preview = call(
        &demo,
        "preview_multi_rename",
        json!({ "sessionId": session, "spec": spec("[N]") }),
    )
    .await;
    let error = refused(
        &demo,
        "write_multi_rename_names",
        json!({ "sessionId": session, "previewId": preview["previewId"] }),
    )
    .await;
    assert_eq!(error["type"], "couldntWriteNames");
    // The session stays usable.
    call(
        &demo,
        "preview_multi_rename",
        json!({ "sessionId": session, "spec": spec("[N]") }),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn presets_seed_once_and_edit_in_memory() {
    let (demo, _) = demo();
    let seeds = json!({ "seeds": [{ "name": "Date and time", "spec": spec("[Y-M-D]") }] });
    let first = call(&demo, "get_multi_rename_presets", seeds.clone()).await;
    assert_eq!(first.as_array().expect("a list").len(), 1);
    let seeded = first[0]["id"].clone();
    call(&demo, "delete_multi_rename_preset", json!({ "id": seeded })).await;
    // Seeds go in once: a deleted seed stays deleted.
    assert_eq!(call(&demo, "get_multi_rename_presets", seeds).await, json!([]));

    let preset = |id: &str, name: &str| json!({ "preset": { "id": id, "name": name, "spec": spec("[N]") } });
    call(&demo, "save_multi_rename_preset", preset("a", "Alpha")).await;
    let list = call(&demo, "save_multi_rename_preset", preset("b", "Beta")).await;
    assert_eq!(list.as_array().expect("a list").len(), 2);
    let list = call(
        &demo,
        "rename_multi_rename_preset",
        json!({ "id": "a", "name": "Aleph" }),
    )
    .await;
    assert_eq!(list[0]["name"], "Aleph");
    let list = call(
        &demo,
        "move_multi_rename_preset",
        json!({ "id": "b", "direction": "up" }),
    )
    .await;
    assert_eq!(list[0]["id"], "b");
    let list = call(
        &demo,
        "update_multi_rename_preset",
        json!({ "id": "a", "spec": spec("[N]-x") }),
    )
    .await;
    assert_eq!(list[1]["spec"]["nameMask"], "[N]-x");

    for n in 2..200 {
        call(
            &demo,
            "save_multi_rename_preset",
            preset(&format!("p{n}"), &format!("P{n}")),
        )
        .await;
    }
    let full = refused(&demo, "save_multi_rename_preset", preset("z", "Zed")).await;
    assert_eq!(full, json!({ "type": "listFull", "max": 200 }));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_last_settings_round_trip() {
    let (demo, _) = demo();
    call(
        &demo,
        "save_multi_rename_last_settings",
        json!({ "spec": spec("[N]_x"), "presetId": "p1" }),
    )
    .await;
    let last = call(&demo, "get_multi_rename_last_settings", json!({})).await;
    assert_eq!(last["spec"]["nameMask"], "[N]_x");
    assert_eq!(last["presetId"], "p1");
}

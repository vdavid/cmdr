//! Folder sizes end to end: the fold reaches the panes' rows, the status bar's
//! `get_dir_stats*`, and the write engine's dialogs, and every write recounts
//! the folders above it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_listing::host::ListingIndex;
use cmdr_operations::KnownSize;
use cmdr_operations::host::{ExpectedTotals, OperationIndex};
use serde_json::{Value, json};

use crate::commands::Demo;
use crate::events::EventSink;
use crate::events_test::ts_keys;
use crate::folder_sizes::{DirStats, FolderSizes};
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
    fn seen(&self, event: &str, matches: impl Fn(&Value) -> bool) -> bool {
        let events = self.0.lock().expect("collector lock");
        events.iter().any(|(name, p)| name == event && matches(p))
    }

    async fn wait_for(&self, event: &str, matches: impl Fn(&Value) -> bool) {
        let description = format!("a `{event}`");
        wait_until_async(Duration::from_secs(10), &description, || self.seen(event, &matches)).await;
    }
}

fn tree() -> DemoTree {
    serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir" },
            { "path": "/Users/you/Documents", "kind": "dir" },
            { "path": "/Users/you/Documents/Deep", "kind": "dir" },
            { "path": "/Users/you/Documents/Deep/b.txt", "kind": "file", "size": 7 },
            { "path": "/Users/you/a.txt", "kind": "file", "size": 1_000 },
        ],
    }))
    .expect("tree parses")
}

fn demo() -> (Demo, Arc<Collector>) {
    let events = Arc::new(Collector::default());
    let demo = Demo::new(tree(), Arc::clone(&events) as _).expect("valid tree");
    (demo, events)
}

async fn call(demo: &Demo, command: &str, args: Value) -> Value {
    demo.invoke(command, args)
        .await
        .unwrap_or_else(|e| panic!("{command} refused: {e}"))
}

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

/// The row named `name` in `id`'s listing.
async fn row(demo: &Demo, id: &str, name: &str) -> Value {
    let rows = call(
        demo,
        "get_file_range",
        json!({ "listingId": id, "start": 0, "count": 100, "includeHidden": false }),
    )
    .await;
    rows.as_array()
        .expect("rows")
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no `{name}` row in {rows}"))
        .clone()
}

async fn size_of(demo: &Demo, id: &str, name: &str) -> Value {
    row(demo, id, name).await["recursiveSize"].clone()
}

async fn settle(demo: &Demo, events: &Collector, started: Value) {
    let op = started["operationId"].as_str().expect("an operation id").to_string();
    events.wait_for("write-settled", |p| p["operationId"] == op).await;
    let _ = demo;
}

/// Waits for `id` to hear that its row at `path` now holds `size` bytes: the
/// size news goes out gathered, a moment after the change (`backend_listings.rs`).
async fn heard_size(events: &Collector, id: &str, path: &str, size: u64) {
    events
        .wait_for("listing-index-sizes-changed", |p| {
            p["listingId"] == id
                && p["folders"].as_array().is_some_and(|rows| {
                    rows.iter()
                        .any(|r| r["path"] == path && r["stats"]["recursiveSize"] == size)
                })
        })
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pane_shows_complete_folder_sizes() {
    let (demo, events) = demo();
    open(&demo, &events, "home", "/Users/you").await;
    let documents = row(&demo, "home", "Documents").await;
    assert_eq!(documents["recursiveSize"], 7);
    assert_eq!(documents["recursiveFileCount"], 1);
    assert_eq!(documents["recursiveDirCount"], 1);
    assert_eq!(documents["recursiveSizeComplete"], true);
    assert_eq!(documents["recursiveSizeStale"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_and_a_delete_recount_every_pane_above_them() {
    let (demo, events) = demo();
    open(&demo, &events, "home", "/Users/you").await;
    open(&demo, &events, "users", "/Users").await;
    assert_eq!(size_of(&demo, "users", "you").await, 1_007);

    let copied = call(
        &demo,
        "copy_between_volumes",
        json!({
            "sourceVolumeId": "root", "sourcePaths": ["/Users/you/a.txt"], "destVolumeId": "root",
            "destPath": "/Users/you/Documents/Deep",
            "config": { "progressIntervalMs": 50, "conflictResolution": "stop", "maxConflictsToShow": 100 },
            "initiator": "user",
        }),
    )
    .await;
    settle(&demo, &events, copied).await;
    // Two levels above the change, and three, once the gathered size news went out.
    heard_size(&events, "home", "/Users/you/Documents", 1_007).await;
    heard_size(&events, "users", "/Users/you", 2_007).await;
    assert_eq!(size_of(&demo, "home", "Documents").await, 1_007);
    assert_eq!(size_of(&demo, "users", "you").await, 2_007);

    let deleted = call(
        &demo,
        "delete_files",
        json!({ "sources": ["/Users/you/Documents/Deep/b.txt"], "volumeId": "root", "initiator": "user" }),
    )
    .await;
    settle(&demo, &events, deleted).await;
    heard_size(&events, "home", "/Users/you/Documents", 1_000).await;
    heard_size(&events, "users", "/Users/you", 2_000).await;
    assert_eq!(size_of(&demo, "home", "Documents").await, 1_000);
    assert_eq!(size_of(&demo, "users", "you").await, 2_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn dir_stats_answer_folders_and_null_for_the_rest() {
    let (demo, _events) = demo();
    let stats = call(
        &demo,
        "get_dir_stats_batch",
        json!({ "paths": ["/Users/you", "/Users/you/a.txt", "/nowhere"] }),
    )
    .await;
    assert_eq!(stats[0]["recursiveSize"], 1_007);
    assert_eq!(stats[0]["recursiveFileCount"], 2);
    assert_eq!(stats[0]["recursiveDirCount"], 2);
    assert_eq!(stats[0]["path"], "/Users/you");
    assert_eq!(stats[1], Value::Null);
    assert_eq!(stats[2], Value::Null);
    let one = call(&demo, "get_dir_stats", json!({ "path": "/Users" })).await;
    assert_eq!(one["recursiveSize"], 1_007);
}

#[test]
fn dir_stats_carry_exactly_the_bindings_keys() {
    let sizes = FolderSizes::new(Arc::new(InMemoryVolume::new("Mem")));
    let stats: Vec<Option<DirStats>> = sizes.dir_stats(&["/".to_string()]);
    let value = serde_json::to_value(stats[0].as_ref().expect("the root is a folder")).expect("serializes");
    let keys: std::collections::BTreeSet<String> = value.as_object().expect("an object").keys().cloned().collect();
    assert_eq!(keys, ts_keys("DirStats"));
}

fn sizes_over_tree() -> FolderSizes {
    FolderSizes::new(Arc::new(tree().into_in_memory().expect("valid tree")))
}

#[test]
fn the_write_engine_reads_exact_sizes_and_totals() {
    let sizes = sizes_over_tree();
    assert_eq!(
        sizes.recursive_size(Path::new("/Users/you")),
        Some(KnownSize::Exact { bytes: 1_007 })
    );
    assert_eq!(
        sizes.recursive_size(Path::new("/Users/you/a.txt")),
        None,
        "a file is no folder"
    );
    assert_eq!(
        sizes.expected_totals(&[PathBuf::from("/Users/you/a.txt"), PathBuf::from("/Users/you/Documents")]),
        Some(ExpectedTotals { files: 2, bytes: 1_007 })
    );
    assert_eq!(sizes.expected_totals(&[PathBuf::from("/nowhere")]), None);
}

#[test]
fn enrich_writes_only_folders_that_are_not_symlinks() {
    let sizes = sizes_over_tree();
    let mut entries = vec![
        FileEntry::new("Documents".into(), "/Users/you/Documents".into(), true, false),
        FileEntry::new("a.txt".into(), "/Users/you/a.txt".into(), false, false),
        FileEntry::new("link".into(), "/Users/you/Documents".into(), true, true),
    ];
    sizes.enrich("root", &mut entries);
    assert_eq!(entries[0].recursive_size, Some(7));
    assert_eq!(entries[1].recursive_size, None);
    assert_eq!(entries[2].recursive_size, None);
}

impl Collector {
    fn last(&self, event: &str, matches: impl Fn(&Value) -> bool) -> Option<Value> {
        let events = self.0.lock().expect("collector lock");
        events
            .iter()
            .rev()
            .find(|(name, p)| name == event && matches(p))
            .map(|(_, p)| p.clone())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_write_tells_each_pane_above_it_its_moved_row_and_its_own_total() {
    let (demo, events) = demo();
    open(&demo, &events, "home", "/Users/you").await;
    open(&demo, &events, "deep", "/Users/you/Documents/Deep").await;
    let made = call(
        &demo,
        "create_directory",
        json!({ "volumeId": "root", "parentPath": "/Users/you/Documents/Deep", "name": "New", "initiator": "user" }),
    )
    .await;
    assert_eq!(made["type"], "done", "{made}");
    let of = |id: &'static str| move |p: &Value| p["listingId"] == id;
    events.wait_for("listing-index-sizes-changed", of("home")).await;
    let home = events
        .last("listing-index-sizes-changed", of("home"))
        .expect("home heard it");
    assert_eq!(home["full"], false);
    assert_eq!(home["folders"][0]["path"], "/Users/you/Documents");
    assert_eq!(home["folders"][0]["stats"]["recursiveDirCount"], 2);
    assert_eq!(home["currentDirChanged"], true);
    assert_eq!(home["currentDir"]["recursiveDirCount"], 3);
    // The pane AT the change heard the new row itself: only its own total moved.
    let deep = events
        .last("listing-index-sizes-changed", of("deep"))
        .expect("deep heard it");
    assert_eq!(deep["folders"], json!([]));
    assert_eq!(deep["currentDir"]["recursiveDirCount"], 1);
    // Written into the cache before the event, so the status bar's re-read agrees.
    assert_eq!(row(&demo, "home", "Documents").await["recursiveDirCount"], 2);
}

/// A long delete tells a pane above it what moved a few times, not once per file: each
/// event sent the page through the frontend's handler, and 1,200 of them at the end of a
/// delete held it for about 45 ms (#956).
#[tokio::test(flavor = "multi_thread")]
async fn a_long_delete_tells_a_pane_above_it_in_a_few_coalesced_events() {
    const FILES: usize = 300;
    let mut entries = vec![
        json!({ "path": "/Users", "kind": "dir" }),
        json!({ "path": "/Users/you", "kind": "dir" }),
        json!({ "path": "/Users/you/many", "kind": "dir" }),
        json!({ "path": "/Users/you/keep.txt", "kind": "file", "size": 5 }),
    ];
    let sources: Vec<String> = (0..FILES).map(|i| format!("/Users/you/many/f{i}.txt")).collect();
    entries.extend(
        sources
            .iter()
            .map(|p| json!({ "path": p, "kind": "file", "size": 100 })),
    );
    let tree: DemoTree =
        serde_json::from_value(json!({ "volumeId": "root", "root": "/", "entries": entries })).expect("tree parses");
    let events = Arc::new(Collector::default());
    let demo = Demo::new(tree, Arc::clone(&events) as _).expect("valid tree");
    open(&demo, &events, "home", "/Users/you").await;

    let deleted = call(
        &demo,
        "delete_files",
        json!({ "sources": sources, "volumeId": "root", "initiator": "user" }),
    )
    .await;
    settle(&demo, &events, deleted).await;
    let of_home = |p: &Value| p["listingId"] == "home";
    let emptied = |p: &Value| of_home(p) && p["folders"][0]["stats"]["recursiveFileCount"] == 0;
    events.wait_for("listing-index-sizes-changed", emptied).await;

    let heard = events
        .0
        .lock()
        .expect("collector lock")
        .iter()
        .filter(|(name, p)| name == "listing-index-sizes-changed" && of_home(p))
        .count();
    assert!(heard <= 20, "{heard} size events for {FILES} deleted files");
    let last = events
        .last("listing-index-sizes-changed", of_home)
        .expect("home heard it");
    assert_eq!(last["folders"][0]["path"], "/Users/you/many");
    assert_eq!(last["currentDir"]["recursiveSize"], 5);
    assert_eq!(size_of(&demo, "home", "many").await, 0);
}

#[test]
fn the_sizes_event_carries_exactly_the_bindings_keys() {
    use crate::events_test::ts_type_of_event;
    use crate::folder_sizes::{FolderReading, LISTING_INDEX_SIZES_CHANGED, ListingIndexSizesChanged};
    assert_eq!(
        ts_type_of_event(LISTING_INDEX_SIZES_CHANGED),
        "ListingIndexSizesChanged"
    );
    let folders = [FolderReading {
        path: "/a".into(),
        stats: None,
    }];
    let event = ListingIndexSizesChanged {
        listing_id: "l",
        full: false,
        folders: &folders,
        current_dir_changed: false,
        current_dir: None,
    };
    let keys = |v: &Value| -> std::collections::BTreeSet<String> {
        v.as_object().expect("an object").keys().cloned().collect()
    };
    let value = serde_json::to_value(&event).expect("serializes");
    assert_eq!(keys(&value["folders"][0]), ts_keys("FolderSizes"));
    assert_eq!(keys(&value), ts_keys("ListingIndexSizesChanged"));
}

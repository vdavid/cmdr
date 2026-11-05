//! Files dropped in from the visitor's computer: the plan, the budget, and the
//! landed bytes working like any other file's.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::testing::wait_until_async;
use serde_json::{Value, json};

use crate::commands::Demo;
use crate::events::EventSink;
use crate::imports::{IMPORT_BUDGET_BYTES, ImportError, ImportItem, ImportRefusal};
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

fn demo() -> (Demo, Arc<Collector>) {
    let tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir" },
            { "path": "/Users/you/Downloads", "kind": "dir" },
        ],
    }))
    .expect("tree parses");
    let events = Arc::new(Collector::default());
    let demo = Demo::new(tree, Arc::clone(&events) as _).expect("valid tree");
    (demo, events)
}

fn items(value: Value) -> Vec<ImportItem> {
    serde_json::from_value(value).expect("items parse")
}

async fn call(demo: &Demo, command: &str, args: Value) -> Value {
    demo.invoke(command, args)
        .await
        .unwrap_or_else(|e| panic!("{command} refused: {e}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_folder_lands_staged_with_its_structure_and_bytes() {
    let (demo, _) = demo();
    let imports = demo.imports();
    let plan = imports
        .plan(&items(json!([
            { "path": "trip", "kind": "folder" },
            { "path": "trip/day 1", "kind": "folder", "modifiedMs": 1_700_000_000_000u64 },
            { "path": "trip/day 1/a.txt", "kind": "file", "size": 5, "modifiedMs": 1_700_000_000_000u64 },
            { "path": "notes.md", "kind": "file", "size": 3 },
        ])))
        .await
        .expect("plans");
    assert_eq!(plan.accepted, 4);
    assert_eq!(plan.refusal, None);
    let staging = format!("/.drops/{}", plan.batch);
    assert_eq!(
        plan.roots,
        vec![format!("{staging}/trip"), format!("{staging}/notes.md")]
    );

    imports.file(plan.batch, 2, b"hello".to_vec()).await.expect("lands");
    imports.file(plan.batch, 3, b"hi!".to_vec()).await.expect("lands");
    assert_eq!(imports.used_bytes(), 8);

    // Folder sizes and kinds come off the volume like any other file's.
    let stats = call(&demo, "get_dir_stats", json!({ "path": format!("{staging}/trip") })).await;
    assert_eq!(stats["recursiveSize"], 5);
    assert_eq!(stats["recursiveFileCount"], 1);
    assert_eq!(stats["recursiveDirCount"], 1);
    let kinds = call(
        &demo,
        "stat_paths_kinds",
        json!({ "paths": [format!("{staging}/trip"), format!("{staging}/notes.md")] }),
    )
    .await;
    assert_eq!(kinds["data"], json!([true, false]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_staged_file_copies_and_reads_like_any_other() {
    let (demo, events) = demo();
    let imports = demo.imports();
    let plan = imports
        .plan(&items(json!([{ "path": "hello.txt", "kind": "file", "size": 5 }])))
        .await
        .expect("plans");
    imports.file(plan.batch, 0, b"hello".to_vec()).await.expect("lands");

    let started = call(
        &demo,
        "copy_between_volumes",
        json!({
            "sourceVolumeId": "root", "sourcePaths": plan.roots, "destVolumeId": "root",
            "destPath": "/Users/you/Downloads",
            "config": { "progressIntervalMs": 50, "conflictResolution": "stop", "maxConflictsToShow": 100 },
            "initiator": "user",
        }),
    )
    .await;
    let op = started["operationId"].as_str().expect("an operation id").to_string();
    let settled = || {
        let seen = events.0.lock().expect("collector lock");
        seen.iter()
            .any(|(name, p)| name == "write-settled" && p["operationId"] == op)
    };
    wait_until_async(Duration::from_secs(10), "the copy to settle", settled).await;

    let stats = call(&demo, "get_dir_stats", json!({ "path": "/Users/you/Downloads" })).await;
    assert_eq!(stats["recursiveSize"], 5);
    let kinds = call(
        &demo,
        "stat_paths_kinds",
        json!({ "paths": ["/Users/you/Downloads/hello.txt"] }),
    )
    .await;
    assert_eq!(kinds["data"], json!([false]));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_budget_takes_the_first_items_that_fit_and_refuses_the_rest() {
    let (demo, _) = demo();
    let imports = demo.imports();
    let half = IMPORT_BUDGET_BYTES / 2;
    let plan = imports
        .plan(&items(json!([
            { "path": "a.bin", "kind": "file", "size": half },
            { "path": "b.bin", "kind": "file", "size": half + 1 },
            // Small enough to fit, but after the first refusal: the rest stays out.
            { "path": "c.txt", "kind": "file", "size": 1 },
            { "path": "folder", "kind": "folder" },
        ])))
        .await
        .expect("plans");
    assert_eq!(plan.accepted, 1);
    assert_eq!(plan.roots.len(), 1);
    assert_eq!(
        plan.refusal,
        Some(ImportRefusal::OverBudget {
            budget_bytes: IMPORT_BUDGET_BYTES,
            left_out: 3,
        })
    );
    assert_eq!(
        serde_json::to_value(plan.refusal).expect("serializes"),
        json!({ "kind": "overBudget", "budgetBytes": IMPORT_BUDGET_BYTES, "leftOut": 3 })
    );
    imports
        .file(plan.batch, 0, vec![0; half as usize])
        .await
        .expect("lands");

    // The budget is the visit's: a later drop has only what's left.
    let later = imports
        .plan(&items(json!([
            { "path": "d.bin", "kind": "file", "size": half },
            { "path": "e.bin", "kind": "file", "size": 1 },
        ])))
        .await
        .expect("plans");
    assert_eq!(later.accepted, 1);
    assert_eq!(imports.used_bytes(), IMPORT_BUDGET_BYTES);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_changed_size_lands_nothing_and_gives_its_room_back() {
    let (demo, _) = demo();
    let imports = demo.imports();
    let plan = imports
        .plan(&items(json!([{ "path": "a.txt", "kind": "file", "size": 5 }])))
        .await
        .expect("plans");
    let refused = imports.file(plan.batch, 0, b"longer!".to_vec()).await;
    assert_eq!(refused, Err(ImportError::SizeChanged));
    assert_eq!(imports.used_bytes(), 0);
    // Its slot is spent: a second try names nothing.
    let again = imports.file(plan.batch, 0, b"hello".to_vec()).await;
    assert_eq!(again, Err(ImportError::UnknownItem));
    let kinds = call(
        &demo,
        "stat_paths_kinds",
        json!({ "paths": [plan.roots[0].to_string()] }),
    )
    .await;
    assert_eq!(kinds["data"], json!([null]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_that_climbs_out_of_the_drop_is_refused() {
    let (demo, _) = demo();
    for bad in ["../escape.txt", "/abs.txt", "", "a/./b"] {
        let refused = demo
            .imports()
            .plan(&items(json!([{ "path": bad, "kind": "file", "size": 1 }])))
            .await;
        assert!(
            matches!(refused, Err(ImportError::BadPath { .. })),
            "{bad:?}: {refused:?}"
        );
    }
    assert_eq!(demo.imports().used_bytes(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_drop_stages_in_its_own_folder() {
    let (demo, _) = demo();
    let first = demo
        .imports()
        .plan(&items(json!([{ "path": "same.txt", "kind": "file", "size": 0 }])))
        .await
        .expect("plans");
    demo.imports().file(first.batch, 0, Vec::new()).await.expect("lands");
    let second = demo
        .imports()
        .plan(&items(json!([{ "path": "same.txt", "kind": "file", "size": 0 }])))
        .await
        .expect("plans");
    demo.imports().file(second.batch, 0, Vec::new()).await.expect("lands");
    assert_ne!(first.roots, second.roots);
}

// ── The staging folder's life: swept at the transfer's end, or once nothing can take it ──

/// Plans and lands one file of `text` named `name`, then hands it off as a drop.
async fn dropped(demo: &Demo, name: &str, text: &str) -> crate::imports::ImportPlan {
    let imports = demo.imports();
    let plan = imports
        .plan(&items(json!([{ "path": name, "kind": "file", "size": text.len() }])))
        .await
        .expect("plans");
    imports
        .file(plan.batch, 0, text.as_bytes().to_vec())
        .await
        .expect("lands");
    imports.hand_off(plan.batch, true).await;
    plan
}

async fn swept(demo: &Demo, batch: u64) {
    let imports = demo.imports();
    wait_until_async(Duration::from_secs(10), "the staging folder to go", || {
        !imports.is_staged(batch)
    })
    .await;
    let kinds = call(
        demo,
        "stat_paths_kinds",
        json!({ "paths": [format!("/.drops/{batch}")] }),
    )
    .await;
    assert_eq!(
        kinds["data"],
        json!([null]),
        "the staging folder is gone from the volume"
    );
}

fn config(conflicts: &str) -> Value {
    json!({ "progressIntervalMs": 50, "conflictResolution": conflicts, "maxConflictsToShow": 100 })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dismissed_drop_is_swept_and_its_room_given_back() {
    let (demo, _) = demo();
    let imports = demo.imports();
    let whole = json!([{ "path": "big.bin", "kind": "file", "size": IMPORT_BUDGET_BYTES }]);
    let plan = imports.plan(&items(whole.clone())).await.expect("plans");
    imports
        .file(plan.batch, 0, vec![0; IMPORT_BUDGET_BYTES as usize])
        .await
        .expect("lands");
    imports.hand_off(plan.batch, true).await;
    assert_eq!(imports.used_bytes(), IMPORT_BUDGET_BYTES);

    // The transfer dialog closed without a transfer: the app unblocks file operations.
    imports.release_offered().await;
    swept(&demo, plan.batch).await;
    assert_eq!(imports.used_bytes(), 0);
    let again = imports.plan(&items(whole)).await.expect("plans");
    assert_eq!(again.accepted, 1, "the full budget is free again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_drop_nothing_came_of_is_swept_at_once_and_one_nobody_took_at_the_next_drop() {
    let (demo, _) = demo();
    let imports = demo.imports();
    let empty = imports
        .plan(&items(json!([{ "path": "a.txt", "kind": "file", "size": 3 }])))
        .await
        .expect("plans");
    // The page couldn't read the file, so it offered nothing.
    imports.hand_off(empty.batch, false).await;
    swept(&demo, empty.batch).await;
    assert_eq!(imports.used_bytes(), 0);

    // A drop that landed nowhere: the next drop sweeps it first.
    let nowhere = dropped(&demo, "b.txt", "bee").await;
    let next = dropped(&demo, "c.txt", "sea").await;
    swept(&demo, nowhere.batch).await;
    assert!(imports.is_staged(next.batch));
    assert_eq!(imports.used_bytes(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_sweeps_its_drop_at_its_end_and_keeps_what_landed_counted() {
    let (demo, _) = demo();
    let plan = dropped(&demo, "hello.txt", "hello").await;
    let moved = call(
        &demo,
        "move_files",
        json!({ "sources": plan.roots, "destination": "/Users/you/Downloads", "config": config("stop"), "initiator": "user" }),
    )
    .await;
    assert!(moved["operationId"].is_string());
    // Nothing but the transfer's end sweeps a claimed drop.
    swept(&demo, plan.batch).await;
    let kinds = call(
        &demo,
        "stat_paths_kinds",
        json!({ "paths": ["/Users/you/Downloads/hello.txt"] }),
    )
    .await;
    assert_eq!(kinds["data"], json!([false]));
    assert_eq!(demo.imports().used_bytes(), 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_that_left_a_file_behind_gives_its_room_back() {
    let (demo, _) = demo();
    let first = dropped(&demo, "same.txt", "first").await;
    call(
        &demo,
        "move_files",
        json!({ "sources": first.roots, "destination": "/Users/you/Downloads", "config": config("stop"), "initiator": "user" }),
    )
    .await;
    swept(&demo, first.batch).await;

    // A file of that name is there now, and this transfer skips the clash.
    let second = dropped(&demo, "same.txt", "second").await;
    call(
        &demo,
        "move_files",
        json!({ "sources": second.roots, "destination": "/Users/you/Downloads", "config": config("skip"), "initiator": "user" }),
    )
    .await;
    swept(&demo, second.batch).await;
    assert_eq!(demo.imports().used_bytes(), 5, "only the first drop's bytes landed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_sweeps_its_drop_and_keeps_what_it_wrote_counted() {
    let (demo, _) = demo();
    let plan = dropped(&demo, "hello.txt", "hello").await;
    call(
        &demo,
        "copy_between_volumes",
        json!({
            "sourceVolumeId": "root", "sourcePaths": plan.roots, "destVolumeId": "root",
            "destPath": "/Users/you/Downloads", "config": config("stop"), "initiator": "user",
        }),
    )
    .await;
    swept(&demo, plan.batch).await;
    let kinds = call(
        &demo,
        "stat_paths_kinds",
        json!({ "paths": ["/Users/you/Downloads/hello.txt"] }),
    )
    .await;
    assert_eq!(kinds["data"], json!([false]));
    assert_eq!(demo.imports().used_bytes(), 5);
}

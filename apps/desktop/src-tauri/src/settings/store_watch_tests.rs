//! The watcher runs the real (patched) store plugin on a mock app. A change set in the shared
//! store stands in for another window's write: the Settings window's `setSetting` reaches the
//! plugin through the same `Store::set` the `set` command calls.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use tauri::test::MockRuntime;
use tauri_plugin_store::{Store, StoreExt};

use super::{FreshKeyMove, fresh_key_move, watch};
use crate::test_support::TestDir;

/// Generous: the worker is a thread hop and one small file write.
const REACTS_WITHIN: Duration = Duration::from_secs(5);

fn app() -> tauri::App<MockRuntime> {
    let app = tauri::test::mock_app();
    app.handle()
        .plugin(tauri_plugin_store::Builder::default().build())
        .expect("the store plugin registers on a mock app");
    app
}

/// A store with no auto-save, as the frontend loads it, so only an explicit save writes the file.
fn store_at(app: &tauri::App<MockRuntime>, path: &Path) -> Arc<Store<MockRuntime>> {
    app.store_builder(path)
        .disable_auto_save()
        .build()
        .expect("a store at an absolute path")
}

fn on_disk(path: &Path, key: &str) -> Option<serde_json::Value> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str::<serde_json::Value>(&contents)
        .ok()?
        .get(key)
        .cloned()
}

/// Watches `path`, reporting each reaction with what the FILE said about `askCmdr.enabled` at
/// that moment: the value the backend's gates would read.
fn watch_reporting(
    app: &tauri::App<MockRuntime>,
    path: &Path,
) -> mpsc::Receiver<(FreshKeyMove, Option<serde_json::Value>)> {
    let (tx, rx) = mpsc::channel();
    let read_from = path.to_path_buf();
    watch(app.handle(), path.to_path_buf(), move |_, moved| {
        let _ = tx.send((moved, on_disk(&read_from, "askCmdr.enabled")));
    });
    rx
}

/// Regression for #1194: the Settings window turned Ask Cmdr off, the main window pushed, and the
/// backend re-read a `settings.json` that still said on, because the change waited out the
/// Settings window's save debounce.
#[test]
fn a_switch_turned_off_in_another_window_is_on_disk_before_the_gates_re_read_it() {
    let dir = TestDir::new("store_watch_switch_off");
    let path = dir.join("settings.json");
    let app = app();
    let store = store_at(&app, &path);
    store.set("askCmdr.enabled", true);
    store.save().expect("the starting file");
    let reactions = watch_reporting(&app, &path);

    store.set("askCmdr.enabled", false);

    let (moved, seen) = reactions
        .recv_timeout(REACTS_WITHIN)
        .expect("the switch's owner hears the move");
    assert_eq!(moved, FreshKeyMove::AskCmdrSwitch);
    assert_eq!(
        seen,
        Some(serde_json::json!(false)),
        "the file holds the off when the gates re-read"
    );
}

#[test]
fn a_reset_switch_reaches_the_file_too() {
    let dir = TestDir::new("store_watch_switch_reset");
    let path = dir.join("settings.json");
    let app = app();
    let store = store_at(&app, &path);
    store.set("askCmdr.enabled", true);
    store.save().expect("the starting file");
    let reactions = watch_reporting(&app, &path);

    store.delete("askCmdr.enabled");

    let (moved, seen) = reactions.recv_timeout(REACTS_WITHIN).expect("a reset is a move too");
    assert_eq!(moved, FreshKeyMove::AskCmdrSwitch);
    assert_eq!(seen, None, "the key is gone from the file, which reads as off");
}

#[test]
fn only_fresh_read_keys_in_the_settings_file_save_and_react() {
    let dir = TestDir::new("store_watch_only_fresh");
    let path = dir.join("settings.json");
    let app = app();
    let store = store_at(&app, &path);
    let other = store_at(&app, &dir.join("shortcuts.json"));
    let reactions = watch_reporting(&app, &path);

    store.set("appearance.uiDensity", "compact");
    other.set("askCmdr.enabled", true);
    // A marker the watcher does follow. Moves reach the worker in emit order, and a burst runs in
    // `FreshKeyMove` order, where the switch sorts first: had either change above been followed,
    // its move would arrive before (or with, and ahead of) this one.
    store.set("askCmdr.proactive", true);

    let (moved, _) = reactions.recv_timeout(REACTS_WITHIN).expect("the marker is followed");
    assert_eq!(
        moved,
        FreshKeyMove::WakeCadence,
        "neither earlier change is the backend's to follow"
    );
}

#[test]
fn every_key_the_backend_reads_fresh_is_watched() {
    assert_eq!(fresh_key_move("askCmdr.enabled"), Some(FreshKeyMove::AskCmdrSwitch));
    for key in ["askCmdr.proactive", "askCmdr.wakeDelay"] {
        assert_eq!(fresh_key_move(key), Some(FreshKeyMove::WakeCadence), "{key}");
    }
    for key in [
        "askCmdr.interactiveModel",
        "askCmdr.chatMemorySize",
        "askCmdr.spendPause",
        "ai.reasoningEfforts",
        "aboutThis.aiVerdict",
    ] {
        assert_eq!(fresh_key_move(key), Some(FreshKeyMove::ReadAtUse), "{key}");
    }
    // A frontend-only gate: the wake loop never reads it.
    assert_eq!(fresh_key_move("askCmdr.wakeToast"), None);
    assert_eq!(fresh_key_move("appearance.uiDensity"), None);
}

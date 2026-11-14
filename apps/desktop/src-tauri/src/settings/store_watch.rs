//! Keeps `settings.json` on disk in step with the settings store for the keys the backend reads
//! fresh from the file, and tells their owner once the file holds the new value.
//!
//! Every window shares one in-memory `tauri-plugin-store` per file, and a window's change reaches
//! it at once, but each window's disk save waits out a 500 ms debounce (`settings-writer.ts`). The
//! backend's gates read the FILE (`ai_gates.rs`, `ask_cmdr.rs`), so without this a gate read the
//! old value for up to that long, and a window closed inside its debounce left the file stale
//! until quit (#1194). Hooked on the plugin's own `store://change` event, so no window has to
//! remember to save or push, whichever window (or MCP) made the change.
//!
//! Rust authors no value here: it writes the plugin's own in-memory store, the same write the
//! plugin makes at `RunEvent::Exit`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use serde::Deserialize;
use tauri::{AppHandle, Listener, Runtime};
use tauri_plugin_store::StoreExt;

const LOG_TARGET: &str = "settings::store_watch";

/// What a fresh-read key's move asks of the backend once the file holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FreshKeyMove {
    /// Read fresh at the moment of use (a send, a card): the file holding it is the whole job.
    ReadAtUse,
    /// `askCmdr.enabled`: the wake loop's readiness is a cached answer, and a running turn has to
    /// stop when it goes off.
    AskCmdrSwitch,
    /// The proactive loop's cadence (`askCmdr.proactive`, `askCmdr.wakeDelay`): it drives a timer
    /// parked on the loop's own thread, which has to re-read and re-arm.
    WakeCadence,
}

/// The keys the backend reads fresh from `settings.json` after startup, and what each move asks.
/// A key missing here keeps the frontend's debounced save. ❌ Don't add one the backend doesn't
/// read from the file: every entry costs a disk write per change. `askCmdr.wakeToast` is
/// deliberately absent: the frontend gates the toast, and the loop never reads it.
pub fn fresh_key_move(key: &str) -> Option<FreshKeyMove> {
    match key {
        "askCmdr.enabled" => Some(FreshKeyMove::AskCmdrSwitch),
        "askCmdr.proactive" | "askCmdr.wakeDelay" => Some(FreshKeyMove::WakeCadence),
        "askCmdr.interactiveModel"
        | "askCmdr.chatMemorySize"
        | "askCmdr.spendPause"
        | "ai.reasoningEfforts"
        | "aboutThis.aiVerdict" => Some(FreshKeyMove::ReadAtUse),
        _ => None,
    }
}

/// The plugin's `store://change` payload, the two fields this needs.
#[derive(Deserialize)]
struct StoreChange {
    path: PathBuf,
    key: String,
}

/// Starts watching the store at `settings_path`. For each change to a fresh-read key, a worker
/// thread saves the store to disk and then calls `react` with the move, once per distinct move in
/// a burst of changes.
///
/// ⚠️ The plugin emits `store://change` while it holds the store's lock, and Tauri runs a Rust
/// listener inline, so the listener only hands the move to the worker: saving from inside it
/// would deadlock on that lock.
pub fn watch<R: Runtime>(
    app: &AppHandle<R>,
    settings_path: PathBuf,
    react: impl Fn(&AppHandle<R>, FreshKeyMove) + Send + 'static,
) {
    let (moves, inbox) = mpsc::channel::<FreshKeyMove>();
    let watched = settings_path.clone();
    app.listen_any("store://change", move |event| {
        let Ok(change) = serde_json::from_str::<StoreChange>(event.payload()) else {
            return;
        };
        if change.path != watched {
            return;
        }
        if let Some(moved) = fresh_key_move(&change.key)
            && moves.send(moved).is_err()
        {
            log::warn!(target: LOG_TARGET, "the settings store watcher is gone; {moved:?} waits for the debounced save");
        }
    });

    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("settings-store-watch".into())
        .spawn(move || {
            while let Ok(first) = inbox.recv() {
                let mut burst = BTreeSet::from([first]);
                burst.extend(inbox.try_iter());
                save(&app, &settings_path);
                for moved in burst {
                    react(&app, moved);
                }
            }
        });
    if let Err(e) = spawned {
        log::warn!(target: LOG_TARGET, "the settings store watcher didn't start: {e}");
    }
}

/// Writes the shared store to disk. A store no window loaded has nothing to write, and one whose
/// file didn't load refuses (the vendored patch); either way the gates keep reading the old file.
fn save<R: Runtime>(app: &AppHandle<R>, settings_path: &Path) {
    let Some(store) = app.get_store(settings_path) else {
        log::debug!(target: LOG_TARGET, "a fresh-read key moved in a settings store that isn't loaded");
        return;
    };
    if let Err(e) = store.save() {
        log::warn!(target: LOG_TARGET, "couldn't save settings.json after a fresh-read key moved: {e}");
    }
}

#[cfg(test)]
#[path = "store_watch_tests.rs"]
mod tests;

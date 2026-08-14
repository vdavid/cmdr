//! Main-window position and size persistence across launches.
//!
//! Replaces `tauri-plugin-window-state`, which deadlocked the whole UI: its
//! `save_window_state` IPC command held the state mutex across main-thread
//! round-trips (`inner_size`, `outer_position`, …) while the `Moved`/`Resized`
//! handlers took that same mutex *on* the main thread. A resize during an
//! in-flight save froze the app hard. See `DETAILS.md` for the full story.
//!
//! The shape here makes that class of bug impossible rather than unlikely:
//!
//! - **Geometry comes from the event payload, never from a getter.**
//!   `WindowEvent::Resized(size)` and `Moved(position)` already carry exactly
//!   what the plugin was calling back into the window to ask for.
//! - **The lock is never held across a window call.** Every handler queries
//!   what it needs first, then takes the lock for a few field assignments.
//!   [`WindowStateStore`] documents this as an invariant; keep it.
//! - **The disk writer only ever sees a snapshot.** It clones under the lock,
//!   releases, then serializes and writes. It never touches a window.
//!
//! Ported and modified from `tauri-plugin-window-state` v2.4.1, used under MIT.
//! Copyright 2019-2023 Tauri Programme within The Commons Conservancy.

mod geometry;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, WindowEvent};
use tokio::sync::Notify;

use crate::ignore_poison::IgnorePoison;
use geometry::{MonitorRect, WindowGeometry, apply_move, apply_resize, is_on_any_monitor, restore_position};

/// Kept identical to the plugin's default so an upgrading user's saved
/// position is picked up rather than silently reset.
const STATE_FILE_NAME: &str = ".window-state.json";

/// The only window we persist. Settings, Debug, and viewer windows
/// deliberately start fresh each launch; within a session they remember
/// position via `crate::child_window_state` (in-memory).
const MAIN_WINDOW_LABEL: &str = "main";

/// How long a burst of move/resize events is coalesced before hitting disk.
/// Long enough that a drag doesn't thrash the file, short enough that a crash
/// mid-session loses at most a moment of repositioning.
const WRITE_DEBOUNCE: Duration = Duration::from_millis(750);

const LOG_TARGET: &str = "window_state";

/// In-memory window geometry plus its disk backing.
///
/// **Invariant: never call into a `Window`/`WebviewWindow` while holding
/// `geometries`.** Window getters and setters round-trip to the main thread,
/// and the main thread takes this lock in the event handlers; holding across
/// one is precisely the deadlock this module exists to remove. Every method
/// here keeps its critical section to plain field access.
///
/// ❌ Don't add a "currently restoring" flag. Upstream has one and it can't
/// work: tao dispatches `set_position` / `set_size` / `maximize` to the main
/// queue, so their events arrive after any synchronous flag would already have
/// been cleared. [`geometry::apply_move`] makes the guard unnecessary instead,
/// by being a no-op for the values restore just applied.
pub struct WindowStateStore {
    /// Keyed by window label. A map (rather than a bare struct) purely for
    /// on-disk compatibility with the plugin's format.
    geometries: Mutex<HashMap<String, WindowGeometry>>,
    /// Poked on every change; the flusher task waits on it.
    flush_requested: Arc<Notify>,
    path: PathBuf,
}

impl WindowStateStore {
    fn load(path: PathBuf) -> Self {
        let geometries = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
                log::warn!(target: LOG_TARGET, "Ignoring unreadable {STATE_FILE_NAME}: {e}");
                HashMap::new()
            }),
            // Absent on first launch, which is not worth logging.
            Err(_) => HashMap::new(),
        };

        Self {
            geometries: Mutex::new(geometries),
            flush_requested: Arc::new(Notify::new()),
            path,
        }
    }

    fn get(&self, label: &str) -> Option<WindowGeometry> {
        self.geometries.lock_ignore_poison().get(label).copied()
    }

    /// Mutates one window's geometry and schedules a debounced disk write.
    ///
    /// `edit` must not touch a window: see the type-level invariant.
    fn edit(&self, label: &str, edit: impl FnOnce(&mut WindowGeometry)) {
        {
            let mut geometries = self.geometries.lock_ignore_poison();
            edit(geometries.entry(label.to_string()).or_default());
        }
        self.flush_requested.notify_one();
    }

    /// Clones the state out from under the lock so serialization and I/O
    /// happen with nothing held.
    fn snapshot(&self) -> HashMap<String, WindowGeometry> {
        self.geometries.lock_ignore_poison().clone()
    }

    /// Writes the current state to disk, atomically. Safe to call from any
    /// thread: it never touches a window.
    fn write_to_disk(&self) {
        let snapshot = self.snapshot();
        let json = match serde_json::to_string_pretty(&snapshot) {
            Ok(json) => json,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Couldn't serialize window state: {e}");
                return;
            }
        };

        let tmp = self.path.with_extension("json.tmp");
        if let Err(e) = crate::config::durable_write_json(&self.path, &tmp, &json) {
            log::warn!(target: LOG_TARGET, "Couldn't write {STATE_FILE_NAME}: {e}");
        }
    }
}

/// Loads saved state, registers it, and starts the debounced disk writer.
/// Call once during app setup, before any window is shown.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let dir = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "No data dir, window state won't persist: {e}");
            return;
        }
    };

    let store = Arc::new(WindowStateStore::load(dir.join(STATE_FILE_NAME)));
    spawn_flusher(Arc::clone(&store));
    app.manage(store);
}

/// Waits for changes and writes them out, coalescing bursts.
///
/// Subscribes rather than polls: an idle app does no work at all. The sleep
/// after each wake is what collapses a whole drag into one write.
fn spawn_flusher(store: Arc<WindowStateStore>) {
    let notify = Arc::clone(&store.flush_requested);
    tauri::async_runtime::spawn(async move {
        loop {
            notify.notified().await;
            tokio::time::sleep(WRITE_DEBOUNCE).await;
            store.write_to_disk();
        }
    });
}

fn store_of<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<WindowStateStore>> {
    app.try_state::<Arc<WindowStateStore>>().map(|s| Arc::clone(&s))
}

/// Applies saved geometry to the main window. Placement only.
///
/// **Deliberately does not show the window**, though the plugin's
/// `restore_state` did. The main window is created `"visible": false` and the
/// frontend owns showing it, from `onMount`
/// (`routes/(main)/show-main-on-mount.ts`, via the `show_main_window`
/// command). That has to stay the only path: `show_main_window` orders the
/// window to the *back* in E2E mode so test runs don't steal the developer's
/// focus, and a bare `show()` here would defeat that.
///
/// So the saved `visible` flag is recorded but never acted on. It stays in the
/// schema so plugin-written files round-trip unchanged.
pub fn restore<R: Runtime>(window: &WebviewWindow<R>) {
    let Some(store) = store_of(window.app_handle()) else {
        return;
    };

    let label = window.label().to_string();
    apply_saved_geometry(window, &store, &label);
}

fn apply_saved_geometry<R: Runtime>(window: &WebviewWindow<R>, store: &WindowStateStore, label: &str) {
    let Some(saved) = store.get(label).filter(|s| *s != WindowGeometry::default()) else {
        // Nothing usable saved: seed from wherever the OS put the window so
        // the first move or resize has a sane base to edit.
        seed_from_live_window(window, store, label);
        return;
    };

    // Position first: sizing a window that's about to move can bounce it
    // between monitors with different scale factors.
    let monitors = monitor_rects(window);
    if monitors.is_empty() || is_on_any_monitor(&saved, &monitors) {
        let (x, y) = restore_position(&saved);
        if let Err(e) = window.set_position(PhysicalPosition { x, y }) {
            log::warn!(target: LOG_TARGET, "Couldn't restore position: {e}");
        }
    } else {
        log::info!(
            target: LOG_TARGET,
            "Saved position ({}, {}) is off every connected monitor; letting the OS place the window",
            saved.x,
            saved.y
        );
    }

    if saved.width > 0
        && saved.height > 0
        && let Err(e) = window.set_size(PhysicalSize {
            width: saved.width,
            height: saved.height,
        })
    {
        log::warn!(target: LOG_TARGET, "Couldn't restore size: {e}");
    }

    if saved.maximized
        && let Err(e) = window.maximize()
    {
        log::warn!(target: LOG_TARGET, "Couldn't maximize: {e}");
    }

    if saved.fullscreen
        && let Err(e) = window.set_fullscreen(true)
    {
        log::warn!(target: LOG_TARGET, "Couldn't restore fullscreen: {e}");
    }
}

fn seed_from_live_window<R: Runtime>(window: &WebviewWindow<R>, store: &WindowStateStore, label: &str) {
    // Every getter runs before the lock is taken. See the store's invariant.
    let size = window.inner_size().ok();
    let position = window.outer_position().ok();
    let fullscreen = window.is_fullscreen().unwrap_or(false);

    store.edit(label, |geometry| {
        if let Some(size) = size {
            geometry.width = size.width;
            geometry.height = size.height;
        }
        if let Some(position) = position {
            geometry.x = position.x;
            geometry.y = position.y;
            geometry.prev_x = position.x;
            geometry.prev_y = position.y;
        }
        geometry.fullscreen = fullscreen;
        geometry.visible = true;
    });
}

fn monitor_rects<R: Runtime>(window: &WebviewWindow<R>) -> Vec<MonitorRect> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            MonitorRect {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            }
        })
        .collect()
}

/// Wires the main window's move/resize/close events to the store.
pub fn track<R: Runtime>(window: &WebviewWindow<R>) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    let Some(store) = store_of(window.app_handle()) else {
        return;
    };

    let label = window.label().to_string();
    let tracked = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(position) => on_moved(&tracked, &store, &label, position),
        WindowEvent::Resized(size) => on_resized(&tracked, &store, &label, size),
        WindowEvent::CloseRequested { .. } => {
            capture_window_flags(&tracked, &store, &label);
            store.write_to_disk();
        }
        _ => {}
    });
}

fn on_moved<R: Runtime>(
    window: &WebviewWindow<R>,
    store: &WindowStateStore,
    label: &str,
    position: &PhysicalPosition<i32>,
) {
    // Query before locking, always.
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let (x, y) = (position.x, position.y);
    store.edit(label, |geometry| apply_move(geometry, x, y));
}

fn on_resized<R: Runtime>(window: &WebviewWindow<R>, store: &WindowStateStore, label: &str, size: &PhysicalSize<u32>) {
    // All window queries happen up front, before the lock.
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let fullscreen = window.is_fullscreen().unwrap_or(false);

    // A maximized or fullscreen window's size is the monitor's, not the
    // user's. Recording it would lose the size to restore on un-maximize.
    if maximized || fullscreen {
        store.edit(label, |geometry| {
            geometry.maximized = maximized;
            geometry.fullscreen = fullscreen;
        });
        return;
    }

    let (width, height) = (size.width, size.height);
    store.edit(label, |geometry| {
        geometry.maximized = false;
        geometry.fullscreen = false;
        // allowed-discarded-outcome: `false` means a zero-sized event, which it already declines to record. There is nothing for the resize handler to do differently.
        apply_resize(geometry, width, height);
    });
}

/// Refreshes the flags that no event payload carries, ahead of a final write.
///
/// ❌ Don't add `visible` here. `NSWindow.isVisible` reads `NO` while the window
/// is miniaturized and while the app is hidden (⌘H), so sampling it on close
/// would persist `visible: false` for a window the user never hid. Nothing acts
/// on the flag today, but recording a wrong value is a trap for whoever does.
fn capture_window_flags<R: Runtime>(window: &WebviewWindow<R>, store: &WindowStateStore, label: &str) {
    let maximized = window.is_maximized().unwrap_or(false);
    let fullscreen = window.is_fullscreen().unwrap_or(false);

    store.edit(label, |geometry| {
        geometry.maximized = maximized;
        geometry.fullscreen = fullscreen;
    });
}

/// Final synchronous write on app exit, so the last move or resize isn't lost
/// to the debounce window.
pub fn save_on_exit<R: Runtime>(app: &AppHandle<R>) {
    let Some(store) = store_of(app) else { return };
    store.write_to_disk();
}

#[cfg(test)]
mod tests {
    use super::*;

    const LABEL: &str = MAIN_WINDOW_LABEL;

    fn store_in(dir: &std::path::Path) -> WindowStateStore {
        WindowStateStore::load(dir.join(STATE_FILE_NAME))
    }

    #[test]
    fn a_missing_file_loads_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(store_in(dir.path()).snapshot().is_empty());
    }

    #[test]
    fn a_corrupt_file_loads_as_empty_instead_of_failing() {
        // A truncated or hand-mangled file must cost the user their window
        // position, never a startup crash.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(STATE_FILE_NAME), "{not valid json").expect("write garbage");
        assert!(store_in(dir.path()).snapshot().is_empty());
    }

    #[test]
    fn edits_round_trip_through_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_in(dir.path());
        store.edit(LABEL, |geometry| {
            apply_move(geometry, 300, 400);
            apply_resize(geometry, 1024, 768);
        });
        store.write_to_disk();

        let reloaded = store_in(dir.path());
        let geometry = reloaded.get(LABEL).expect("the entry must survive a reload");
        assert_eq!((geometry.x, geometry.y), (300, 400));
        assert_eq!((geometry.width, geometry.height), (1024, 768));
    }

    #[test]
    fn writing_leaves_no_temp_file_behind() {
        // The write is temp + rename; a leftover .tmp means the rename didn't happen.
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_in(dir.path());
        store.edit(LABEL, |geometry| apply_move(geometry, 1, 2));
        store.write_to_disk();

        assert!(dir.path().join(STATE_FILE_NAME).exists());
        assert!(!dir.path().join(".window-state.json.tmp").exists());
    }

    #[test]
    fn a_plugin_written_file_keeps_its_geometry() {
        // The upgrade path: a file the old plugin wrote must load, and the
        // dropped `decorated` field must not make it unreadable.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(STATE_FILE_NAME),
            r#"{"main":{"width":2474,"height":1218,"x":2590,"y":372,"prev_x":2126,"prev_y":372,
                "maximized":false,"visible":true,"decorated":true,"fullscreen":false}}"#,
        )
        .expect("write plugin file");

        let geometry = store_in(dir.path()).get(LABEL).expect("plugin entry must load");
        assert_eq!((geometry.width, geometry.height), (2474, 1218));
        assert_eq!((geometry.x, geometry.y), (2590, 372));
    }

    #[test]
    fn only_the_edited_label_is_touched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_in(dir.path());
        store.edit(LABEL, |geometry| apply_move(geometry, 10, 20));
        store.edit("settings", |geometry| apply_move(geometry, 30, 40));

        assert_eq!(store.get(LABEL).expect("main").x, 10);
        assert_eq!(store.get("settings").expect("settings").x, 30);
    }
}

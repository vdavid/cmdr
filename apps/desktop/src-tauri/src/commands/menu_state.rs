//! Menu-state sync commands: the pushes that keep the native menu bar saying what
//! the frontend is showing (view mode, hidden files, pin tab, reopen tab, undo trash, the
//! Select menu's live "same kind" label, the UI language) and the greying that
//! follows the focused pane, a dialog, or a window swap.
//!
//! Its sibling `menu.rs` holds the context-menu popups. Both are a thin IPC layer
//! over the `crate::menu` builders and `MenuState`.

use crate::ignore_poison::IgnorePoison;
use crate::menu::{
    MenuState, SELECT_SAME_KIND_ID, SameKindTarget, SettingsChanged, TrashUndoTarget, UNDO_TRASH_ID, ViewMode,
    apply_menu_item_states, cloned_handle, rebuild_view_mode_items, same_kind_menu_label, set_menu_context,
    sync_view_mode_check_states, undo_trash_menu_label,
};
#[cfg(target_os = "macos")]
use crate::menu::{swap_to_main_menu, swap_to_viewer_menu};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::Event as _;

/// Toggle hidden files visibility - updates menu checkbox and emits event.
///
/// This is the "external trigger" path: MCP tool calls and any other Rust-side
/// caller that needs to flip the setting from outside the explorer. It updates
/// the macOS `CheckMenuItem` and emits `settings-changed` so the explorer
/// listener picks up the change.
///
/// **The keyboard-shortcut / command-palette path does NOT use this.** That
/// path mutates the explorer's FE state directly (synchronous, no Rust round-
/// trip) and uses [`sync_menu_show_hidden`] to push the new check state to the
/// native menu. Routing the FE-driven toggle through here would create an
/// IPC → event → effect → DOM-update chain that the e2e test against `⌘⇧.`
/// flaked on (~1/25) when the slow lane was under load.
#[tauri::command]
#[specta::specta]
pub fn toggle_hidden_files<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let menu_state = app.state::<MenuState<R>>();
    // A handle, ❌ not the guard: the MCP tool calls this off the main thread (`cloned_handle`).
    let Some(check_item) = cloned_handle(&menu_state.show_hidden_files) else {
        return Err("Menu not initialized".to_string());
    };

    // Get current state and toggle it
    let current = check_item.is_checked().unwrap_or(false);
    let new_state = !current;
    check_item.set_checked(new_state).map_err(|e| e.to_string())?;

    // Emit event to frontend with the new state
    SettingsChanged {
        show_hidden_files: new_state,
    }
    .emit(&app)
    .map_err(|e| e.to_string())?;

    Ok(new_state)
}

/// One-way sync of the native "Show hidden files" `CheckMenuItem` checked
/// state from the frontend. Does NOT emit `settings-changed`: the FE is the
/// caller, it already knows the new state and has already updated its own
/// view. Idempotent — safe to call with the current state.
#[tauri::command]
#[specta::specta]
pub fn sync_menu_show_hidden<R: Runtime>(app: AppHandle<R>, checked: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let Some(check_item) = cloned_handle(&menu_state.show_hidden_files) else {
        // Menu not yet initialized (very early in startup). The next menu
        // build will pick up the persisted setting, so a no-op here is fine.
        return Ok(());
    };
    check_item.set_checked(checked).map_err(|e| e.to_string())?;
    Ok(())
}

/// Pushes the full View menu state from the frontend: which pane is active and
/// the per-pane view modes. The menu's check states are updated for both pane
/// pairs, and if the active pane changed since the last call the keyboard
/// accelerators are migrated to the newly-active pair via
/// `rebuild_view_mode_items`. Called on initial mount, focus change, swap, and
/// after any view-mode change (palette, MCP, menu click round-trip).
#[tauri::command]
#[specta::specta]
pub fn update_view_mode_menu<R: Runtime>(
    app: AppHandle<R>,
    active_pane: String,
    left_mode: String,
    right_mode: String,
) -> Result<(), String> {
    if active_pane != "left" && active_pane != "right" {
        return Err(format!("Invalid active_pane: {active_pane}"));
    }
    let parse_mode = |s: &str| match s {
        "full" => Ok(ViewMode::Full),
        "brief" => Ok(ViewMode::Brief),
        other => Err(format!("Invalid view mode: {other}")),
    };
    let left = parse_mode(&left_mode)?;
    let right = parse_mode(&right_mode)?;

    let menu_state = app.state::<MenuState<R>>();

    // Stash new state, then decide whether a full rebuild is needed.
    let active_changed = {
        let mut guard = menu_state.view_mode_active_pane.lock_ignore_poison();
        let changed = *guard != active_pane;
        *guard = active_pane;
        changed
    };
    *menu_state.view_mode_left.lock_ignore_poison() = left;
    *menu_state.view_mode_right.lock_ignore_poison() = right;

    if active_changed {
        rebuild_view_mode_items(&app, &menu_state).map_err(|e| e.to_string())?;
    } else {
        sync_view_mode_check_states(&menu_state).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Updates the File menu "Pin tab" / "Unpin tab" label based on the active tab's pin state.
#[tauri::command]
#[specta::specta]
pub fn update_pin_tab_menu<R: Runtime>(app: AppHandle<R>, is_pinned: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let Some(item) = cloned_handle(&menu_state.pin_tab) else {
        return Err("Menu not initialized".to_string());
    };
    item.set_text(crate::menu::pin_tab_label(is_pinned))
        .map_err(|e| e.to_string())
}

/// Rewrites the Select menu's "Select all of the same kind" item to say what it would select right
/// now: "Select all folders", "Select all with extension *.pdf", or the neutral fallback.
///
/// `target` is the focused pane's cursor row, as `pane/same-kind-target.svelte.ts` publishes it —
/// the same value the command palette labels its row from, so the two can't disagree. `None` is a
/// row with no kind (`..`, an empty listing), and restores the neutral label.
///
/// The frontend debounces this by 200 ms and skips a push that would render the same words, so
/// holding an arrow key down costs one call. ❗ The COMMAND never reads any of this: it re-reads
/// the cursor row when it runs, so a label a frame behind can't change what gets selected.
#[tauri::command]
#[specta::specta]
pub fn update_select_same_kind_menu<R: Runtime>(
    app: AppHandle<R>,
    target: Option<SameKindTarget>,
) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    menu_state.set_item_label(SELECT_SAME_KIND_ID, same_kind_menu_label(target.as_ref()))?;
    // `set_text` leaves a plain NSMenuItem title behind, so the dimmed `⌥⇧=` has to be redrawn or
    // it vanishes on the first cursor move.
    #[cfg(target_os = "macos")]
    crate::menu::set_display_accelerators_from_command(&app);
    Ok(())
}

/// Greys out (or restores) Edit > "Undo move to trash" and makes it name what it would put back:
/// "Undo move to trash of “report.pdf”", "… of 3 items", or the plain wording, greyed out, for
/// `None` (nothing to undo).
///
/// `target` is the newest trash this session nobody has undone yet, as `trash-undo.ts` remembers
/// it; that module is the only caller, and pushes on every change. ⚠️ CHROME only: the command
/// re-reads that memory when it runs, so a label a beat behind can't undo a different trash.
#[tauri::command]
#[specta::specta]
pub fn update_undo_trash_menu<R: Runtime>(app: AppHandle<R>, target: Option<TrashUndoTarget>) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    menu_state
        .undo_trash_available
        .store(target.is_some(), Ordering::Relaxed);
    menu_state.set_item_label(UNDO_TRASH_ID, undo_trash_menu_label(target.as_ref()))?;
    apply_menu_item_states(&menu_state);
    // `set_text` leaves a plain NSMenuItem title behind, so a rebound display-only shortcut has
    // to be redrawn (see `update_select_same_kind_menu`).
    #[cfg(target_os = "macos")]
    crate::menu::set_display_accelerators_from_command(&app);
    Ok(())
}

/// Tells Rust which language the UI speaks, and rebuilds the native menu bar if
/// that moved it.
///
/// `language` is the raw `appearance.language` setting: a catalog tag the user
/// pinned, or `None` / `"system"` for "follow the OS". The frontend pushes it on
/// startup and on every change, because the native surfaces (menu bar, window
/// title, the already-running alert) resolve their own copy and can't read the
/// webview's.
///
/// Rebuilding is skipped when the resolved catalog didn't actually move: going
/// from `'system'` to the language the OS already reported changes nothing
/// visible, and a rebuild is a flicker plus a round of frontend re-pushes.
#[tauri::command]
#[specta::specta]
pub fn set_ui_language<R: Runtime>(app: AppHandle<R>, language: Option<String>) -> Result<(), String> {
    if !crate::intl::set_language_preference(language) {
        return Ok(());
    }
    // Installing a menu is AppKit work, and a command handler runs on a worker
    // thread. Fire-and-forget past the hop: a failed dispatch leaves the old
    // language on the bar, never a half-built one.
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = crate::menu::rebuild_menu_bar(&handle) {
            log::warn!(target: "menu", "Couldn't rebuild the menu bar in the new language: {e}");
        }
    })
    .map_err(|e| e.to_string())
}

/// Enables or disables the Tab menu "Reopen closed tab" item based on whether the
/// focused pane's closed-tab stack has entries.
#[tauri::command]
#[specta::specta]
pub fn set_reopen_closed_tab_enabled<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    menu_state.reopen_closed_tab_enabled.store(enabled, Ordering::Relaxed);
    apply_menu_item_states(&menu_state);
    Ok(())
}

/// Activates the right app menu for the window that just gained focus.
///
/// `kind` is one of:
/// - `"main"`: the main file explorer gained focus. On macOS, swap the app-level menu bar back to
///   the main menu (if a different menu is installed), then enable all explorer items.
/// - `"viewer"`: a viewer window gained focus. On macOS, swap to the shared viewer menu. No-op on
///   Linux (viewer windows carry their own per-window menu).
/// - `"other"`: Settings or Debug gained focus. On macOS, swap to the main menu, then disable
///   explorer items (Settings / Debug reuse the main menu with items greyed out).
///
/// On macOS the menu bar is app-level (one bar, tauri-apps/tauri#5768), so we swap it via
/// `app.set_menu()` on focus-gain. `active_menu_kind` tracks the installed menu so we skip redundant
/// swaps. After every swap we re-run `cleanup_macos_menus` (macOS re-injects Edit items) and, when
/// swapping back to the main menu, re-apply SF Symbol icons (they don't reliably survive a swap).
#[tauri::command]
#[specta::specta]
pub fn activate_window_menu<R: Runtime>(app: AppHandle<R>, kind: String) -> Result<(), String> {
    match kind.as_str() {
        "main" => {
            #[cfg(target_os = "macos")]
            {
                swap_to_main_menu(&app);
                // Coming back from a terminal is when the `cmdr` command may have changed under
                // us (`rm`, `brew install`), so its menu label re-reads the filesystem here.
                crate::cli::commands::refresh_menu_item(&app);
            }
            set_menu_context(app, "explorer".to_string())
        }
        "viewer" => {
            #[cfg(target_os = "macos")]
            swap_to_viewer_menu(&app);
            #[cfg(not(target_os = "macos"))]
            let _ = &app;
            Ok(())
        }
        "other" => {
            #[cfg(target_os = "macos")]
            swap_to_main_menu(&app);
            set_menu_context(app, "other".to_string())
        }
        other => Err(format!("Unknown window menu kind: {other}")),
    }
}

/// Greys out (or restores) the File menu's "Open terminal here", following the
/// focused pane's volume.
///
/// Called by the main window whenever the focused pane, its tab, or that tab's
/// volume changes. ⚠️ CHROME only: a disabled item's accelerator still fires, so
/// the real refusals stay in the frontend handler (which words the hint) and in
/// `open_terminal_here` itself (which answers `not_a_local_path`).
#[tauri::command]
#[specta::specta]
pub fn set_open_terminal_here_enabled<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    menu_state.open_terminal_here_enabled.store(enabled, Ordering::Relaxed);
    apply_menu_item_states(&menu_state);
    Ok(())
}

/// Greys out (or restores) the menu items that would start a file operation.
///
/// Called by the main window whenever a dialog opens or closes, or the Ask Cmdr
/// composer takes or gives up focus. ⚠️ CHROME only: a disabled item's accelerator
/// still fires, so this stops the app OFFERING what it would refuse; the refusals
/// themselves live in `mcp/executor/mod.rs` and the two frontend gates.
#[tauri::command]
#[specta::specta]
pub fn set_file_operations_blocked<R: Runtime>(app: AppHandle<R>, blocked: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    menu_state.file_operations_blocked.store(blocked, Ordering::Relaxed);
    apply_menu_item_states(&menu_state);
    Ok(())
}

/// Greys out the main-menu items whose commands the main window's dialog gate refuses right now:
/// every `BLOCKED_BY_DIALOGS` command while a dialog, an explorer overlay, or the command palette is
/// up, and none once it's gone. The only writer is `routes/(main)/menu-dialog-gate.svelte.ts`.
///
/// ⚠️ CHROME for the regular items: a disabled item's accelerator still fires, and the dispatch core
/// refuses those commands itself. The two check items are the exception, because they toggle
/// themselves before the frontend hears of the click: `handle_menu_event` reverts a refused one
/// from this same set.
#[tauri::command]
#[specta::specta]
pub fn set_commands_refused_over_dialog<R: Runtime>(app: AppHandle<R>, command_ids: Vec<String>) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    *menu_state.commands_refused_over_dialog.lock_ignore_poison() = command_ids.into_iter().collect();
    apply_menu_item_states(&menu_state);
    Ok(())
}

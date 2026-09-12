//! Native menu commands: context menus (file / breadcrumb / volume row / parent
//! row / tab / network host / function key bar), the macOS app-menu-bar swap on focus change, and
//! the menu-state sync commands (view mode, hidden files, pin tab, reopen tab).
//!
//! Thin IPC layer over the `crate::menu` builders and `MenuState`.

use crate::ignore_poison::IgnorePoison;
use crate::menu::{
    ContextMenuPaneFacts, DetachWord, FileContextInfo, MenuState, ServerRowMenu, SettingsChanged, ViewMode,
    apply_menu_item_states, build_breadcrumb_context_menu, build_context_menu, build_function_key_bar_context_menu,
    build_network_host_context_menu, build_parent_row_context_menu, build_tab_context_menu,
    build_volume_row_context_menu, frontend_shortcut_to_accelerator, rebuild_view_mode_items, set_menu_context,
    sync_view_mode_check_states,
};
#[cfg(target_os = "macos")]
use crate::menu::{swap_to_main_menu, swap_to_viewer_menu};
use std::sync::atomic::Ordering;
use tauri::menu::ContextMenu;
use tauri::{AppHandle, Manager, Runtime, Window};
use tauri_specta::Event as _;

#[tauri::command]
#[specta::specta]
pub fn update_menu_context<R: Runtime>(app: AppHandle<R>, path: String, filename: String) {
    let state = app.state::<MenuState<R>>();
    let mut context = state.context.lock_ignore_poison();
    context.path = path;
    context.filename = filename;
}

/// Makes `window` the focused one, on the way to popping a context menu up over it.
///
/// ❗ Load-bearing, not politeness. `handle_menu_event` refuses every
/// `CommandScope::FileScoped` command unless the MAIN window has focus, because an
/// accelerator reaches that handler whatever is in front. A context menu is the one case
/// where the test asks the wrong question: this menu was popped from a named window over
/// a named row, so that window IS the target however focus stood a moment ago. Without
/// this call, right-clicking a pane row while Settings or the viewer is in front draws a
/// menu whose file items (`Copy path`, `Show in Finder`, `Copy`, `Move`, `Delete`) are
/// all enabled and all silently do nothing — a right-click doesn't make a window key.
///
/// ❌ Don't replace it with a "a context menu is up" flag: `popup()` returns BEFORE the
/// event loop processes muda's `MenuEvent` (see [`show_tab_context_menu`]), so a flag
/// scoped to the popup is already cleared when the click arrives, and one that outlives
/// the popup would switch the guard off for the next accelerator.
///
/// A failure only costs the focus, so it's logged rather than propagated: the menu still
/// opens, and the guard still logs its own refusal if the click then goes nowhere.
fn focus_for_context_menu<R: Runtime>(window: &Window<R>) {
    if let Err(e) = window.set_focus() {
        log::warn!(target: "menu", "couldn't focus `{}` before its context menu: {e}", window.label());
    }
}

/// What the PANE contributes to a file context menu, as opposed to the file that
/// was right-clicked. Grouped because they answer one question each about the
/// surface the click landed in, and because the frontend fills them all from the
/// same pane read.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneContextMenuFacts {
    /// Omits Rename and New folder. `true` from the search-results virtual pane,
    /// whose rows aren't a real directory; see `apps/desktop/src/lib/search/capabilities.ts`.
    pub restrict_destination_actions: bool,
    /// The pane's listing id, so a Finder-tag click can refresh that listing's
    /// cache after writing. Empty for a virtual pane with no normal listing.
    pub listing_id: String,
    /// Whether "Open terminal here" is clickable. It opens the PANE's folder, not
    /// the right-clicked file, so only a pane on OS-visible paths offers it.
    pub can_open_terminal_here: bool,
    /// Whether this pane's ROWS are real OS paths, which is what a share service
    /// needs. Not the same question as `can_open_terminal_here`: the search-results
    /// snapshot has no folder of its own yet lists real files.
    pub can_share: bool,
}

/// Shows the file context menu.
#[tauri::command]
#[specta::specta]
pub fn show_file_context_menu<R: Runtime>(
    window: Window<R>,
    path: String,
    filename: String,
    is_directory: bool,
    paths: Vec<String>,
    pane: PaneContextMenuFacts,
    target: crate::menu::ContextMenuTarget,
) -> Result<(), String> {
    let app = window.app_handle();

    // The "primary" path drives single-file actions like "Copy 'filename'", Get info,
    // Quick look. `paths` carries the full selection that "Open with" and cloud actions
    // should apply to: it equals `[path]` when the right-clicked file isn't part of a
    // multi-selection, or the entire selection otherwise.
    let context_paths = if paths.is_empty() { vec![path.clone()] } else { paths };
    // How many rows the header names, taken before `context_paths` moves into `MenuState`.
    let target_count = context_paths.len();

    // Compute per-file context (sync status, FP-domain membership, candidate "Open with"
    // apps). The LaunchServices query for candidates can take 50-200 ms on a cold cache,
    // which delays the popup; the cache (in `file_system::open_with`) keeps later
    // right-clicks fast.
    #[cfg(target_os = "macos")]
    let mut info = build_file_context_info(&path, &context_paths, is_directory);
    #[cfg(not(target_os = "macos"))]
    let info = FileContextInfo;

    // What a macOS service, or a share service, would act on: the same rows, as paths,
    // taken before `context_paths` moves into `MenuState`.
    #[cfg(target_os = "macos")]
    let services_paths: Vec<std::path::PathBuf> = context_paths.iter().map(std::path::PathBuf::from).collect();

    // The services macOS offers for those rows, one `Share` submenu item each. Left
    // empty when the pane's rows aren't OS paths, and empty is also macOS's own answer
    // for a row it can't share — either way the item is left out entirely.
    //
    // ⚠️ The marker has to come from THIS thread, not a hop: `services_for` arms the
    // click side by index, and `on_menu_event` reads it back on the main thread. A sync
    // `#[tauri::command]` runs there, which is also why `popup()` works below; without a
    // marker the submenu is simply absent, like a pane that can't share.
    #[cfg(target_os = "macos")]
    if pane.can_share {
        match objc2::MainThreadMarker::new() {
            Some(mtm) => info.share_services = crate::file_system::share::services_for(mtm, &services_paths),
            None => log::warn!(target: "menu", "Not on the main thread; the context menu offers no Share submenu"),
        }
    }

    // Update menu context so on_menu_event has paths + bundle map for the new items.
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.path = path.clone();
        context.filename = filename.clone();
        context.paths = context_paths;
        context.tags_listing_id = pane.listing_id;
        #[cfg(target_os = "macos")]
        {
            // Filled in from build_context_menu's return value below.
            context.open_with_apps.clear();
        }
    }

    // Media-index group: shown only while image indexing is enabled, keyed on this
    // folder's live membership (OS-path checks against the live config, no I/O).
    let image_index_enabled = cmdr_index::media_index::gate::is_enabled();
    let image_index = crate::menu::ImageIndexMenuState {
        enabled: image_index_enabled,
        excluded: image_index_enabled && cmdr_index::media_index::network::config::is_excluded(&path),
        chosen: image_index_enabled && cmdr_index::media_index::network::config::is_chosen_folder(&path),
        covered_by_parent: image_index_enabled
            && cmdr_index::media_index::network::config::is_covered_by_parent_folder(&path),
    };

    let result = build_context_menu(
        app,
        &filename,
        is_directory,
        &info,
        ContextMenuPaneFacts {
            restrict_destination_actions: pane.restrict_destination_actions,
            can_open_terminal_here: pane.can_open_terminal_here,
            can_share: pane.can_share,
        },
        image_index,
        crate::menu::ContextMenuTargetFacts {
            count: target_count,
            count_text: target.count_text.as_deref(),
            size_text: target.size_text.as_deref(),
        },
    )
    .map_err(|e| e.to_string())?;

    // Stash the bundle_id → app_path map so on_menu_event can resolve clicks on
    // `open-with:<bundle-id>` items back to a real app URL.
    #[cfg(target_os = "macos")]
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.open_with_apps = result.open_with_apps;
    }

    // AppKit's Services menu, borrowed for as long as this menu is up, pointed at the
    // right-clicked rows rather than the pane selection. Answers `None` when the menu
    // carries no Services item. ❗ The binding has to outlive `popup()` (which runs the
    // tracking loop), so ❌ never `let _ =`: that would hand the menu back before it
    // was ever shown.
    #[cfg(target_os = "macos")]
    let _services_loan = crate::menu::lend_services_menu(&result.menu, services_paths);

    // SF Symbols on the items worth spotting at a glance. Same story as the loan above:
    // Tauri hands out no `NSMenu` for a context menu, so the icons land when the menu
    // starts tracking, which happens inside `popup()`. ❌ Never `let _ =`.
    #[cfg(target_os = "macos")]
    let _icon_loan = crate::menu::lend_context_menu_icons(&result.menu);

    // The header line, restyled from "greyed-out command" to "header" at the same
    // moment and for the same reason as the icons above. ❌ Never `let _ =`; if this
    // never lands, the plain disabled item it replaces is still correct.
    #[cfg(target_os = "macos")]
    let _header_loan = crate::menu::lend_context_menu_header(&result.menu);

    focus_for_context_menu(&window);
    result.menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

/// A context menu has to appear now. Half a second is already more than the user
/// should wait for one label.
#[cfg(target_os = "macos")]
const MENU_SYNC_STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

#[cfg(target_os = "macos")]
fn build_file_context_info(primary_path: &str, all_paths: &[String], is_directory: bool) -> FileContextInfo {
    use crate::file_system::cloud_actions::is_in_icloud_drive;
    use crate::file_system::open_with::compute_open_with_choices;
    use crate::file_system::sync_status::status_within_blocking;
    use std::path::PathBuf;

    let path_buf = PathBuf::from(primary_path);
    let is_icloud_drive = is_in_icloud_drive(&path_buf);

    // Sync status of the primary path only (drives the eviction pair's label).
    // Bounded, because this runs before a context menu pops: a provider that stops
    // answering must cost the menu a plain label, not a delay the user can feel.
    //
    // The probe itself is provider-agnostic and answers correctly for third-party
    // providers (a streamed Google Drive file carries `SF_DATALESS` like any other
    // stub, verified 2026-09-07 with `stat -f %Sf`). We skip it off iCloud anyway,
    // because the only thing this value picks between is the eviction pair, and
    // that pair is iCloud-only. Widening the menu, not this call, is what a
    // third-party provider would need.
    let sync_status = if is_icloud_drive {
        status_within_blocking(primary_path, MENU_SYNC_STATUS_TIMEOUT)
    } else {
        Default::default()
    };

    // Google Drive links for the primary path: ONE resolution, every URL shape
    // built from it. Cheap enough to stay on the menu-build
    // path without a timeout of its own: a `getxattr`, or a couple of hundred bytes of
    // JSON for a native-doc stub, or — for a MIRRORED file, where neither exists — two
    // indexed reads of Drive's own local databases. Measured 2026-09-09 on a real
    // 3,268-item mirror: 12 µs for a path outside Drive, 0.5 ms for one inside it, and
    // 9.8 ms on the first call of a 30-second window (`google_drive/mirror_db.rs`
    // caches the account scan for exactly that reason).
    let google_drive_links = crate::file_system::google_drive::item_links(&path_buf, is_directory);

    let open_with = compute_open_with_choices(all_paths.iter().map(PathBuf::from).collect());

    // Which color tags the WHOLE selection already carries (drives the checked circle).
    // Read each path's tags once; `applied_colors` marks a color only when every path
    // has it.
    let per_path_tags: Vec<Vec<crate::file_system::listing::metadata::TagRef>> = all_paths
        .iter()
        .map(|p| crate::file_system::tags::read_tags(&PathBuf::from(p)))
        .collect();
    let applied_tag_colors = crate::file_system::tags::applied_colors(&per_path_tags);

    FileContextInfo {
        sync_status,
        is_icloud_drive,
        google_drive_links,
        open_with,
        // Filled in by the caller, which holds the main-thread marker the enumeration
        // has to share with the click handler.
        share_services: Vec::new(),
        applied_tag_colors,
    }
}

/// Shows a native context menu for the breadcrumb path bar.
///
/// `shortcut` is the user's configured shortcut for "Copy path" in frontend format
/// (e.g. "⌘⌥C"), or empty string if no shortcut is configured.
/// `eject_volume_id` + `eject_volume_name` are set when the breadcrumb represents an
/// ejectable volume; both must be present (or both absent) — the command stashes the
/// id in `MenuState.volume_row_context` so `on_menu_event` can dispatch the click.
#[tauri::command]
#[specta::specta]
pub fn show_breadcrumb_context_menu<R: Runtime>(
    window: Window<R>,
    shortcut: String,
    eject_volume_id: Option<String>,
    eject_volume_name: Option<String>,
) -> Result<(), String> {
    let app = window.app_handle();
    let accelerator = frontend_shortcut_to_accelerator(&shortcut).unwrap_or_default();
    // Disable the eject item while a write op touches this volume (the picker's
    // inline eject button is disabled the same way).
    let eject_busy = eject_volume_id
        .as_ref()
        .is_some_and(|id| crate::file_system::busy_volume_ids().contains(id));
    let detach_word = eject_volume_id
        .as_deref()
        .map_or(DetachWord::Eject, DetachWord::for_volume_id);
    let menu = build_breadcrumb_context_menu(app, &accelerator, eject_volume_name.as_deref(), eject_busy, detach_word)
        .map_err(|e| e.to_string())?;

    // Stash eject target so on_menu_event can read it back when the user clicks
    // the "Eject (name)" item. If only one of the two args is present, treat as no
    // eject target — the builder also won't render the item.
    {
        let state = app.state::<MenuState<R>>();
        let mut ctx = state.volume_row_context.lock_ignore_poison();
        if let (Some(id), Some(name)) = (eject_volume_id, eject_volume_name) {
            ctx.volume_id = id;
            ctx.volume_name = name;
        } else {
            ctx.volume_id.clear();
            ctx.volume_name.clear();
        }
    }

    focus_for_context_menu(&window);
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

/// Shows a native context menu for a row in the volume-selector dropdown (fire-and-forget).
///
/// A favorite row (`is_favorite`) gets `Rename` + `Remove`; an ejectable volume row gets
/// `Eject ({name})`; a SERVER row (`server` present) gets Disconnect / Forget saved password /
/// Forget server instead, because a server has nothing to unplug. The picked action is delivered
/// asynchronously via the `volume-context-action` Tauri event from `on_menu_event` (the same path
/// as the breadcrumb eject item). The target id + name are stashed in
/// `MenuState.volume_row_context` so the handler can read them back.
///
/// ❗ `server` is the CALLER's reading of the row (which items apply), because this command is
/// synchronous and deciding "is a secret stored?" here would put a secret-store read on the popup
/// path. `busy` is filled in here, though: `busy_volume_ids()` is the backend's own answer, and the
/// destructive items are disabled by it exactly like Eject.
#[tauri::command]
#[specta::specta]
pub fn show_volume_row_context_menu<R: Runtime>(
    window: Window<R>,
    volume_id: String,
    volume_name: String,
    is_favorite: bool,
    is_ejectable: bool,
    server: Option<ServerRowMenu>,
) -> Result<(), String> {
    let app = window.app_handle();

    // Disable the eject item while a write op touches this volume (matches the inline
    // eject button and the breadcrumb menu). Favorites are never ejectable.
    let busy = crate::file_system::busy_volume_ids().contains(&volume_id);
    let eject_busy = is_ejectable && busy;
    let eject_name = (is_ejectable && !is_favorite).then_some(volume_name.as_str());
    let server = server.map(|s| ServerRowMenu { busy, ..s });
    let menu = build_volume_row_context_menu(
        app,
        is_favorite,
        eject_name,
        eject_busy,
        DetachWord::for_volume_id(&volume_id),
        server.as_ref(),
    )
    .map_err(|e| e.to_string())?;

    {
        let state = app.state::<MenuState<R>>();
        let mut ctx = state.volume_row_context.lock_ignore_poison();
        ctx.volume_id = volume_id;
        ctx.volume_name = volume_name;
    }

    focus_for_context_menu(&window);
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

/// Shows the minimal `..` parent-row context menu (just "Add to favorites").
///
/// `parent_path` is the directory the `..` row points at; we stash it in `MenuState.context.path`
/// so `on_menu_event` favorites it when the user clicks the item. The full file context menu makes
/// no sense on `..`, hence this dedicated one-item menu.
#[tauri::command]
#[specta::specta]
pub fn show_parent_row_context_menu<R: Runtime>(window: Window<R>, parent_path: String) -> Result<(), String> {
    let app = window.app_handle();
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.path = parent_path;
        context.filename = "..".to_string();
    }
    let menu = build_parent_row_context_menu(app).map_err(|e| e.to_string())?;
    focus_for_context_menu(&window);
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

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
    let guard = menu_state.show_hidden_files.lock_ignore_poison();
    let Some(check_item) = guard.as_ref() else {
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
    let guard = menu_state.show_hidden_files.lock_ignore_poison();
    let Some(check_item) = guard.as_ref() else {
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

/// Shows a native context menu for a tab (fire-and-forget).
/// The selected action is delivered asynchronously via a `tab-context-action` Tauri event
/// from `on_menu_event`, because `popup()` returns before the event loop processes the
/// `MenuEvent` from muda. A synchronous channel approach doesn't work here: the wakeup
/// signal posted during the popup's NSEvent tracking loop gets consumed, so `recv` always
/// times out.
#[tauri::command]
#[specta::specta]
pub fn show_tab_context_menu(
    window: Window<tauri::Wry>,
    is_pinned: bool,
    can_close: bool,
    has_other_unpinned_tabs: bool,
) -> Result<(), String> {
    let app = window.app_handle().clone();

    let menu =
        build_tab_context_menu(&app, is_pinned, can_close, has_other_unpinned_tabs).map_err(|e| e.to_string())?;
    focus_for_context_menu(&window);
    menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

/// Shows the function key bar's context menu (fire-and-forget): a single "Hide
/// function key bar" item. The click is delivered asynchronously via the
/// `function-key-bar-hide-requested` event from `on_menu_event`, same shape as
/// [`show_tab_context_menu`].
#[tauri::command]
#[specta::specta]
pub fn show_function_key_bar_context_menu(window: Window<tauri::Wry>) -> Result<(), String> {
    let app = window.app_handle().clone();
    let menu = build_function_key_bar_context_menu(&app).map_err(|e| e.to_string())?;
    focus_for_context_menu(&window);
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

/// Shows a native context menu for a network host (fire-and-forget).
/// The selected action is delivered asynchronously via a `network-host-context-action` Tauri event
/// from `on_menu_event`.
#[tauri::command]
#[specta::specta]
pub fn show_network_host_context_menu(
    window: Window<tauri::Wry>,
    host_id: String,
    host_name: String,
    is_manual: bool,
    has_credentials: bool,
) -> Result<(), String> {
    let app = window.app_handle().clone();

    let menu = build_network_host_context_menu(&app, is_manual, has_credentials).map_err(|e| e.to_string())?;

    // Store context so on_menu_event can include host info in the emitted event
    {
        let state = app.state::<MenuState<tauri::Wry>>();
        let mut ctx = state.network_host_context.lock_ignore_poison();
        ctx.host_id = host_id;
        ctx.host_name = host_name;
    }

    focus_for_context_menu(&window);
    menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

/// Updates the File menu "Pin tab" / "Unpin tab" label based on the active tab's pin state.
#[tauri::command]
#[specta::specta]
pub fn update_pin_tab_menu<R: Runtime>(app: AppHandle<R>, is_pinned: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.pin_tab.lock_ignore_poison();
    let Some(item) = guard.as_ref() else {
        return Err("Menu not initialized".to_string());
    };
    item.set_text(crate::menu::pin_tab_label(is_pinned))
        .map_err(|e| e.to_string())
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
            swap_to_main_menu(&app);
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

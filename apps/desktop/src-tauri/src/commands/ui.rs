use crate::ignore_poison::IgnorePoison;
use crate::menu::{
    CLOSE_TAB_ID, CommandScope, FileContextInfo, MenuState, REOPEN_CLOSED_TAB_ID, SettingsChanged, ViewMode,
    build_breadcrumb_context_menu, build_context_menu, build_network_host_context_menu, build_parent_row_context_menu,
    build_tab_context_menu, frontend_shortcut_to_accelerator, menu_id_to_command, rebuild_view_mode_items,
    sync_view_mode_check_states,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use tauri::menu::ContextMenu;
use tauri::{AppHandle, Manager, Runtime, Window};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_specta::Event as _;

#[tauri::command]
#[specta::specta]
pub fn update_menu_context<R: Runtime>(app: AppHandle<R>, path: String, filename: String) {
    let state = app.state::<MenuState<R>>();
    let mut context = state.context.lock_ignore_poison();
    context.path = path;
    context.filename = filename;
}

/// Shows the file context menu.
///
/// `restrict_destination_actions` is a frontend opt-in: when true, the Rust
/// menu builder omits Rename and New folder. The flag is `false` by default
/// for existing local-pane callers; the search-results virtual pane passes
/// `true`. See `apps/desktop/src/lib/search/capabilities.ts` for the flag set.
#[tauri::command]
#[specta::specta]
pub fn show_file_context_menu<R: Runtime>(
    window: Window<R>,
    path: String,
    filename: String,
    is_directory: bool,
    paths: Vec<String>,
    restrict_destination_actions: bool,
) -> Result<(), String> {
    let app = window.app_handle();

    // The "primary" path drives single-file actions like "Copy 'filename'", Get info,
    // Quick look. `paths` carries the full selection that "Open with" and cloud actions
    // should apply to: it equals `[path]` when the right-clicked file isn't part of a
    // multi-selection, or the entire selection otherwise.
    let context_paths = if paths.is_empty() { vec![path.clone()] } else { paths };

    // Compute per-file context (sync status, FP-domain membership, candidate "Open with"
    // apps). The LaunchServices query for candidates can take 50-200 ms on a cold cache,
    // which delays the popup; the cache (in `file_system::open_with`) keeps later
    // right-clicks fast.
    #[cfg(target_os = "macos")]
    let info = build_file_context_info(&path, &context_paths);
    #[cfg(not(target_os = "macos"))]
    let info = FileContextInfo;

    // Update menu context so on_menu_event has paths + bundle map for the new items.
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.path = path.clone();
        context.filename = filename.clone();
        context.paths = context_paths;
        #[cfg(target_os = "macos")]
        {
            // Filled in from build_context_menu's return value below.
            context.open_with_apps.clear();
        }
    }

    let result = build_context_menu(app, &filename, is_directory, &info, restrict_destination_actions)
        .map_err(|e| e.to_string())?;

    // Stash the bundle_id → app_path map so on_menu_event can resolve clicks on
    // `open-with:<bundle-id>` items back to a real app URL.
    #[cfg(target_os = "macos")]
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.open_with_apps = result.open_with_apps;
    }

    result.menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn build_file_context_info(primary_path: &str, all_paths: &[String]) -> FileContextInfo {
    use crate::file_system::cloud_actions::is_in_icloud_drive;
    use crate::file_system::open_with::compute_open_with_choices;
    use crate::file_system::sync_status::get_sync_statuses;
    use std::path::PathBuf;

    let path_buf = PathBuf::from(primary_path);
    let is_icloud_drive = is_in_icloud_drive(&path_buf);

    // Sync status of the primary path only (drives the cloud-action label).
    let sync_status = if is_icloud_drive {
        let mut statuses = get_sync_statuses(vec![primary_path.to_string()]);
        statuses.remove(primary_path).unwrap_or_default()
    } else {
        Default::default()
    };

    let open_with = compute_open_with_choices(all_paths.iter().map(PathBuf::from).collect());

    FileContextInfo {
        sync_status,
        is_icloud_drive,
        open_with,
    }
}

/// Shows a native context menu for the breadcrumb path bar.
///
/// `shortcut` is the user's configured shortcut for "Copy path" in frontend format
/// (e.g. "⌃⌘C"), or empty string if no shortcut is configured.
/// `eject_volume_id` + `eject_volume_name` are set when the breadcrumb represents an
/// ejectable volume; both must be present (or both absent) — the command stashes the
/// id in `MenuState.volume_eject_context` so `on_menu_event` can dispatch the click.
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
    let menu = build_breadcrumb_context_menu(app, &accelerator, eject_volume_name.as_deref(), eject_busy)
        .map_err(|e| e.to_string())?;

    // Stash eject target so on_menu_event can read it back when the user clicks
    // the "Eject (name)" item. If only one of the two args is present, treat as no
    // eject target — the builder also won't render the item.
    {
        let state = app.state::<MenuState<R>>();
        let mut ctx = state.volume_eject_context.lock_ignore_poison();
        if let (Some(id), Some(name)) = (eject_volume_id, eject_volume_name) {
            ctx.volume_id = id;
            ctx.volume_name = name;
        } else {
            ctx.volume_id.clear();
            ctx.volume_name.clear();
        }
    }

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
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

/// macOS: send the given NSWindow to the back of the window list without focusing
/// it. `orderBack:` still makes the window visible (just behind everything), so
/// the webview keeps rendering and the E2E tests can drive it over the socket.
#[cfg(target_os = "macos")]
fn order_ns_window_back(ns_window: *mut objc2::runtime::AnyObject) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    if ns_window.is_null() {
        return Err("NSWindow pointer is null".into());
    }
    // SAFETY: `ns_window` is the live, non-null `NSWindow` Tauri owns for this webview (null-checked
    // above). `-orderBack:` takes an `id` sender; we pass nil. Returns void, so there's no ownership
    // to manage. This is E2E-only window plumbing (gated by `is_e2e_mode`), never on a user path.
    unsafe {
        let _: () = msg_send![ns_window, orderBack: std::ptr::null_mut::<AnyObject>()];
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn show_main_window<R: Runtime>(window: Window<R>) -> Result<(), String> {
    // E2E: on macOS, order the window to the back without focusing it instead of
    // `window.show()` (which calls `makeKeyAndOrderFront:`, always grabbing OS
    // focus AND raising the window to the front). This keeps a test run's windows
    // out of the developer's way. Linux/Windows test runs happen in headless
    // containers, so the standard show is fine there.
    #[cfg(target_os = "macos")]
    if crate::test_mode::is_e2e_mode() {
        use objc2::runtime::AnyObject;
        let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
        return order_ns_window_back(ns_window);
    }
    window.show().map_err(|e| e.to_string())
}

/// E2E-only: order a freshly created child window (Settings, file viewer,
/// shortcuts) to the back without focusing it, so a test run's windows don't pop
/// in front of the developer's work. Invoked by the opener (the main window) right
/// after the child window is created, resolving it by label. No-op off macOS or
/// outside E2E; pairs with the `Prohibited` activation policy (`crate::run`) and
/// the child windows' own `focus: false`. See `crate::test_mode::is_e2e_mode`.
#[tauri::command]
#[specta::specta]
pub fn order_window_to_back<R: Runtime>(app: AppHandle<R>, label: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if crate::test_mode::is_e2e_mode() {
        use objc2::runtime::AnyObject;
        let window = app
            .get_webview_window(&label)
            .ok_or_else(|| format!("no window with label {label}"))?;
        let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
        return order_ns_window_back(ns_window);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, label);
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

// ============================================================================
// Direct file action commands (for command palette and other invocations)
// ============================================================================

/// Show a file in Finder (reveal in parent folder)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn show_in_finder(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Show a file in the default file manager (open parent folder via xdg-open)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "linux")]
pub fn show_in_finder(path: String) -> Result<(), String> {
    let parent = std::path::Path::new(&path)
        .parent()
        .unwrap_or(std::path::Path::new("/"));
    Command::new("xdg-open")
        .arg(parent)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn show_in_finder(_path: String) -> Result<(), String> {
    Err("Show in file manager is not available on this platform".to_string())
}

/// Copy text to clipboard
#[tauri::command]
#[specta::specta]
pub fn copy_to_clipboard<R: Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

// ============================================================================
// Quick Look (native QLPreviewPanel on macOS, stubs elsewhere)
// ============================================================================
//
// Three commands rather than one because the panel is a process-wide singleton
// owned by AppKit — we can `open` it, re-target it via `set_path`, or `close`
// it, but we don't get to construct fresh instances. The frontend tracks
// `isOpen` and picks the right call. See `crate::quick_look` for the full
// design (and the why-singleton, why-main-thread, why-events arguments).
//
// All three commands wrap their main-thread hop in `blocking_with_timeout` (2 s)
// so a wedged AppKit pump never freezes the IPC blocking pool.

/// Open (or re-open) Quick Look on the given path.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn quick_look_open(app: AppHandle, path: String, volume_id: String) -> Result<(), String> {
    use crate::commands::util::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    if !volume_supports_local_fs(&volume_id) {
        log::debug!(
            target: "quick_look",
            "skipping open: volume {volume_id} doesn't support local fs access (path={path})"
        );
        return Ok(());
    }

    let app_inner = app.clone();
    let path_inner = path;
    blocking_with_timeout(Duration::from_secs(2), Err("timed out".to_string()), move || {
        let (tx, rx) = channel();
        let app_for_closure = app_inner.clone();
        let path_main = std::path::PathBuf::from(path_inner);
        app_inner
            .run_on_main_thread(move || {
                let state = app_for_closure.state::<crate::quick_look::QuickLookState>();
                if let Ok(mut ctrl) = state.lock() {
                    ctrl.open_on_main(&app_for_closure, path_main);
                }
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn quick_look_set_path(app: AppHandle, path: String, volume_id: String) -> Result<(), String> {
    use crate::commands::util::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    if !volume_supports_local_fs(&volume_id) {
        log::debug!(
            target: "quick_look",
            "skipping set_path: volume {volume_id} doesn't support local fs access (path={path})"
        );
        return Ok(());
    }

    let app_inner = app.clone();
    let path_inner = path;
    blocking_with_timeout(Duration::from_secs(2), Err("timed out".to_string()), move || {
        let (tx, rx) = channel();
        let app_for_closure = app_inner.clone();
        let path_main = std::path::PathBuf::from(path_inner);
        app_inner
            .run_on_main_thread(move || {
                let state = app_for_closure.state::<crate::quick_look::QuickLookState>();
                if let Ok(mut ctrl) = state.lock() {
                    ctrl.set_path_on_main(path_main);
                }
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn quick_look_close(app: AppHandle) -> Result<(), String> {
    use crate::commands::util::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    let app_inner = app.clone();
    blocking_with_timeout(Duration::from_secs(2), Err("timed out".to_string()), move || {
        let (tx, rx) = channel();
        let app_for_closure = app_inner.clone();
        app_inner
            .run_on_main_thread(move || {
                let state = app_for_closure.state::<crate::quick_look::QuickLookState>();
                if let Ok(mut ctrl) = state.lock() {
                    ctrl.close_on_main();
                }
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub async fn quick_look_open(_app: AppHandle, _path: String, _volume_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub async fn quick_look_set_path(_app: AppHandle, _path: String, _volume_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub async fn quick_look_close(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

/// Helper: returns true if the named volume supports `std::fs`-style access
/// (local POSIX, OS-mounted SMB). False for MTP and other protocol-only
/// volumes — those have no NSURL the Quick Look panel can preview.
#[cfg(target_os = "macos")]
fn volume_supports_local_fs(volume_id: &str) -> bool {
    let manager = crate::file_system::get_volume_manager();
    match manager.get(volume_id) {
        Some(volume) => volume.supports_local_fs_access(),
        None => {
            // Unknown volume id — assume yes so we don't accidentally silence
            // working previews. The frontend always sends a real id for entries
            // it just listed.
            log::debug!(target: "quick_look", "volume {volume_id} not found; assuming local fs access");
            true
        }
    }
}

/// Open the Get Info window for a file (macOS only, no-op on other platforms)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn get_info(path: String) -> Result<(), String> {
    // Pass the path as a positional argument via `on run argv` to avoid AppleScript injection.
    let script = r#"on run argv
        tell application "Finder"
            activate
            open information window of (POSIX file (item 1 of argv) as alias)
        end tell
    end run"#;

    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub fn get_info(_path: String) -> Result<(), String> {
    Ok(())
}

/// Open file in the system's default text editor (macOS only)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Make a cloud-managed file available offline (download it). On macOS, talks to the
/// File Provider extension responsible for the file (iCloud Drive, Dropbox, GDrive,
/// OneDrive, Box, etc.).
#[tauri::command]
#[specta::specta]
pub async fn cloud_make_available_offline(path: String) -> Result<(), String> {
    // 30s timeout like every other fs-touching command: a wedged File Provider
    // extension can hang the blocking call indefinitely. The download request is
    // fire-and-forget server-side, so releasing the IPC on timeout is correct.
    let work = tokio::task::spawn_blocking(move || {
        crate::file_system::cloud_actions::request_download(std::path::Path::new(&path))
    });
    match tokio::time::timeout(tokio::time::Duration::from_secs(30), work).await {
        Ok(joined) => joined.map_err(|e| e.to_string())?,
        Err(_elapsed) => Err("Timed out reaching iCloud — give it another try".to_string()),
    }
}

/// Evict a cloud-managed file's local copy, leaving a placeholder. Counterpart to
/// `cloud_make_available_offline`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_remove_download(path: String) -> Result<(), String> {
    // 30s timeout: same hung-File-Provider risk as `cloud_make_available_offline`.
    // Eviction is fire-and-forget server-side, so releasing the IPC on timeout is fine.
    let work =
        tokio::task::spawn_blocking(move || crate::file_system::cloud_actions::evict_item(std::path::Path::new(&path)));
    match tokio::time::timeout(tokio::time::Duration::from_secs(30), work).await {
        Ok(joined) => joined.map_err(|e| e.to_string())?,
        Err(_elapsed) => Err("Timed out reaching iCloud — give it another try".to_string()),
    }
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "linux")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn open_in_editor(_path: String) -> Result<(), String> {
    Err("Open in editor is not available on this platform".to_string())
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
    let label = if is_pinned { "Unpin tab" } else { "Pin tab" };
    item.set_text(label).map_err(|e| e.to_string())
}

/// Enables or disables the Tab menu "Reopen closed tab" item based on whether the
/// focused pane's closed-tab stack has entries. Mirrors the dynamic-label pattern
/// used by `update_pin_tab_menu`.
#[tauri::command]
#[specta::specta]
pub fn set_reopen_closed_tab_enabled<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.reopen_closed_tab.lock_ignore_poison();
    let Some(item) = guard.as_ref() else {
        return Err("Menu not initialized".to_string());
    };
    item.set_enabled(enabled).map_err(|e| e.to_string())
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

/// Swaps the app-level menu bar to the main menu, if a different menu is installed.
///
/// After the swap, re-runs the macOS Edit-item cleanup and re-applies SF Symbol icons (neither
/// reliably survives `app.set_menu()`). Skips all of this when the main menu is already active.
#[cfg(target_os = "macos")]
fn swap_to_main_menu<R: Runtime>(app: &AppHandle<R>) {
    use crate::menu::ActiveMenuKind;
    let menu_state = app.state::<MenuState<R>>();

    {
        let mut active = menu_state.active_menu_kind.lock_ignore_poison();
        if *active == ActiveMenuKind::Main {
            return;
        }
        let main_menu = menu_state.main_menu.lock_ignore_poison();
        let Some(main_menu) = main_menu.as_ref() else {
            log::warn!(target: "menu", "main menu not stored; cannot swap app menu back to main");
            return;
        };
        if let Err(e) = app.set_menu(main_menu.clone()) {
            log::warn!(target: "menu", "Failed to swap app menu to main: {e}");
            return;
        }
        *active = ActiveMenuKind::Main;
    }

    // macOS re-injects Edit items on every `set_menu`, and SF Symbol icons don't survive the swap,
    // so re-run both on the main thread (mirrors the startup ordering in `lib.rs`).
    crate::menu::cleanup_macos_menus_from_command(app);
    if let Err(e) = app.run_on_main_thread(crate::menu::set_macos_menu_icons) {
        log::warn!(target: "menu", "Failed to re-apply macOS menu icons after swap: {e}");
    }
}

/// Swaps the app-level menu bar to the shared viewer menu, if a different menu is installed.
///
/// After the swap, re-runs the macOS Edit-item cleanup. Skips all of this when the viewer menu is
/// already active.
#[cfg(target_os = "macos")]
fn swap_to_viewer_menu<R: Runtime>(app: &AppHandle<R>) {
    use crate::menu::ActiveMenuKind;
    let menu_state = app.state::<MenuState<R>>();

    {
        let mut active = menu_state.active_menu_kind.lock_ignore_poison();
        if *active == ActiveMenuKind::Viewer {
            return;
        }
        let viewer_menu = menu_state.viewer_menu.lock_ignore_poison();
        let Some(viewer_menu) = viewer_menu.as_ref() else {
            log::warn!(target: "menu", "viewer menu not stored; cannot swap app menu to viewer");
            return;
        };
        if let Err(e) = app.set_menu(viewer_menu.clone()) {
            log::warn!(target: "menu", "Failed to swap app menu to viewer: {e}");
            return;
        }
        *active = ActiveMenuKind::Viewer;
    }

    crate::menu::cleanup_macos_menus_from_command(app);
}

/// Enables or disables explorer-scoped menu items based on the current context.
/// - `"explorer"`: all menu items enabled (main file explorer has focus)
/// - `"other"`: all non-App items disabled except Close tab (⌘W), which doubles as "close the
///   focused window" (standard macOS behavior)
///
/// Private helper behind `activate_window_menu`: the focus-gain command owns the menu swap (macOS)
/// and then calls this to set the per-item enabled state.
fn set_menu_context<R: Runtime>(app: AppHandle<R>, context: String) -> Result<(), String> {
    let enabled = context == "explorer";
    let menu_state = app.state::<MenuState<R>>();

    for (id, entry) in menu_state.items.lock_ignore_poison().iter() {
        // Close tab stays enabled: on_menu_event has special logic to close the focused
        // non-main window when main isn't focused (standard ⌘W behavior on macOS).
        if id == CLOSE_TAB_ID {
            continue;
        }
        // Reopen closed tab is managed exclusively by `set_reopen_closed_tab_enabled`:
        // skip it here so an "explorer" context switch doesn't enable it while the
        // focused pane's closed-tab stack is empty.
        if id == REOPEN_CLOSED_TAB_ID {
            continue;
        }
        let is_app = matches!(menu_id_to_command(id), Some((_, CommandScope::App)));
        if !is_app {
            let _ = entry.item.set_enabled(enabled);
        }
    }

    // Items stored in separate MenuState fields (not in the HashMap)
    if let Some(ref item) = *menu_state.pin_tab.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.show_hidden_files.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_full_left.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_brief_left.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_full_right.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_brief_right.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    // Disable the parent "Left pane" / "Right pane" submenus too, so they appear
    // greyed out instead of opening to reveal disabled items.
    if let Some(ref submenu) = *menu_state.view_left_pane_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(enabled);
    }
    if let Some(ref submenu) = *menu_state.view_right_pane_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(enabled);
    }
    if let Some(ref submenu) = *menu_state.sort_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(enabled);
    }

    Ok(())
}

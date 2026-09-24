//! The context-menu popups: file, breadcrumb, parent row, tab, network host, and the function key
//! bar. (A volume switcher row's and a favorite's actions are the in-app `Menu`'s, not a popup
//! here: `apps/desktop/src/lib/file-explorer/navigation/row-menu.ts`.)
//!
//! The pushes that keep the menu BAR in step with the frontend (view mode, hidden files, pin tab,
//! the language, the greying) are the sibling `menu_state.rs`. Both are a thin IPC layer over the
//! `crate::menu` builders and `MenuState`.

use crate::ignore_poison::IgnorePoison;
#[cfg(not(target_os = "macos"))]
use crate::menu::FileContextInfo;
#[cfg(target_os = "macos")]
use crate::menu::context_menu_facts;
use crate::menu::{
    ContextMenuPaneFacts, ContextMenuShortcuts, DetachWord, MenuState, SameKindTarget, build_breadcrumb_context_menu,
    build_context_menu, build_function_key_bar_context_menu, build_network_host_context_menu,
    build_parent_row_context_menu, build_tab_context_menu,
};
use tauri::menu::ContextMenu;
use tauri::{AppHandle, Manager, Runtime, Window};

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

/// Where a context menu opens, when the caller names a point instead of letting the
/// OS use the pointer. In the WEBVIEW's own coordinates, in CSS pixels: the same
/// numbers `getBoundingClientRect()` gave the frontend.
///
/// Only the keyboard paths send one. A right-click passes `None` and macOS uses the
/// mouse, which is why the pointer path is untouched by all of this.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuAnchor {
    pub x: f64,
    pub y: f64,
}

/// Pops `menu` up over `window`, at `anchor` when there is one and at the pointer
/// when there isn't.
///
/// ❗ The anchor goes on the wire as a LOGICAL position, never a physical one.
/// muda converts whatever it gets with `Position::to_logical(backingScaleFactor)`
/// and then positions in the target `NSView`'s own coordinates, so logical CSS
/// pixels pass straight through and a Retina display needs no `devicePixelRatio`
/// arithmetic on either side. Sending `Physical` would halve every coordinate on a
/// 2× display. Measurements and the rest of the coordinate story:
/// `apps/desktop/src/lib/file-explorer/pane/DETAILS.md` § Keyboard context menu.
fn popup_context_menu<R: Runtime>(
    menu: &tauri::menu::Menu<R>,
    window: Window<R>,
    anchor: Option<MenuAnchor>,
) -> Result<(), String> {
    focus_for_context_menu(&window);
    match anchor {
        Some(MenuAnchor { x, y }) => menu.popup_at(window, tauri::LogicalPosition::new(x, y)),
        None => menu.popup(window),
    }
    .map_err(|e| e.to_string())
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
    /// Whether the right-clicked folder is somewhere a favorite could point back to, so the
    /// "Add to favorites" item is offered. The affordance half of [`crate::commands::favorites`]'s
    /// own gate; ❗ enforcement stays there, since a context menu is not the only add surface.
    pub can_favorite: bool,
}

/// Shows the file context menu.
#[allow(
    clippy::too_many_arguments,
    reason = "IPC payload fields, each named on the wire; `path` / `filename` / `is_directory` / `paths` all describe the right-clicked ROW and belong in one struct, which is a change to the frontend contract rather than to this signature"
)]
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
    shortcuts: ContextMenuShortcuts,
    anchor: Option<MenuAnchor>,
    // What the `Selection >` submenu's "Select all of the same kind" row would select, for the
    // row this menu is opening over. Computed live by the caller, ❗ never the menu bar's
    // debounced value: a right-click is the one moment a stale label would be read as truth.
    same_kind: Option<SameKindTarget>,
) -> Result<(), String> {
    let app = window.app_handle();

    // The "primary" path drives single-file actions like "Copy 'filename'", Get info,
    // Quick look. `paths` carries the full selection that "Open with" and cloud actions
    // should apply to: it equals `[path]` when the right-clicked file isn't part of a
    // multi-selection, or the entire selection otherwise.
    let context_paths = if paths.is_empty() { vec![path.clone()] } else { paths };
    // How many rows the header names, taken before `context_paths` moves into `MenuState`.
    let target_count = context_paths.len();

    // What a macOS service, or a share service, would act on: the same rows, as paths,
    // taken before `context_paths` moves into `MenuState`.
    #[cfg(target_os = "macos")]
    let services_paths: Vec<std::path::PathBuf> = context_paths.iter().map(std::path::PathBuf::from).collect();

    // ❗ Nothing on this thread asks the disk, a provider, or LaunchServices: this is a sync
    // command, so it runs on the MAIN thread, and on a network share every such question is
    // a round trip the whole app waits behind. The slow facts start first, on their own pool,
    // and the cheap work below runs while they answer. `menu/context_menu_facts.rs`.
    #[cfg(target_os = "macos")]
    let started = std::time::Instant::now();
    #[cfg(target_os = "macos")]
    let is_icloud_drive = crate::file_system::cloud_actions::is_in_icloud_drive(std::path::Path::new(&path));
    #[cfg(target_os = "macos")]
    let gathering = context_menu_facts::start(context_menu_facts::FactsRequest {
        primary: std::path::PathBuf::from(&path),
        paths: services_paths.clone(),
        is_directory,
        is_icloud_drive,
        can_share: pane.can_share,
    });

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
            // Both filled in below, from the facts that answered in time, and later by the
            // live menu from the ones that didn't. Cleared first, so a click can never reach
            // the last menu's apps or provider actions.
            context.open_with_apps.clear();
            context.file_provider_offer = None;
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

    // Waits out the grace period at most, then builds with what answered. Every later
    // answer goes to the live menu, tagged with this menu's generation.
    #[cfg(target_os = "macos")]
    let generation = crate::menu::next_generation();
    #[cfg(target_os = "macos")]
    let collected = gathering.collect(started + context_menu_facts::GRACE, crate::menu::late_sink(generation));
    #[cfg(target_os = "macos")]
    log::debug!(
        target: "menu",
        "Right-click menu built after {} ms, {} facts ready, still out: {:?}",
        started.elapsed().as_millis(),
        collected.ready.len(),
        collected.pending
    );
    #[cfg(target_os = "macos")]
    let info = {
        let (info, share_offer) =
            context_menu_facts::file_context_info(is_icloud_drive, collected.ready, &collected.pending);
        // What a `Share` click performs from: this menu's offer, or nothing until the late
        // one lands, so a click can never reach the last menu's. On the main thread, which a
        // sync command always is.
        match objc2::MainThreadMarker::new() {
            Some(mtm) => crate::file_system::share::arm_offer(mtm, share_offer),
            None => log::warn!(target: "menu", "Not on the main thread; the context menu's Share items do nothing"),
        }
        app.state::<MenuState<R>>()
            .context
            .lock_ignore_poison()
            .file_provider_offer = info.file_provider_offer.ready().cloned().flatten();
        info
    };
    #[cfg(not(target_os = "macos"))]
    let info = FileContextInfo;

    let result = build_context_menu(
        app,
        &filename,
        is_directory,
        &info,
        ContextMenuPaneFacts {
            restrict_destination_actions: pane.restrict_destination_actions,
            can_open_terminal_here: pane.can_open_terminal_here,
            can_share: pane.can_share,
            can_favorite: pane.can_favorite,
        },
        image_index,
        crate::menu::ContextMenuTargetFacts {
            count: target_count,
            count_text: target.count_text.as_deref(),
            size_text: target.size_text.as_deref(),
        },
        &shortcuts,
        same_kind.as_ref(),
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

    // Every image on the menu: SF Symbols on Cmdr's own items, the provider's logo on its
    // actions, app icons in "Open with", service icons in "Share", and the tag circles.
    // Same story as the loan above: Tauri hands out no `NSMenu` for a context menu, so the
    // images land when the menu starts tracking, which happens inside `popup()`. ❌ Never
    // `let _ =`.
    #[cfg(target_os = "macos")]
    let _icon_loan = crate::menu::lend_context_menu_icons(&result.menu, &info);

    // The header line, restyled from "greyed-out command" to "header" at the same
    // moment and for the same reason as the icons above. ❌ Never `let _ =`; if this
    // never lands, the plain disabled item it replaces is still correct.
    #[cfg(target_os = "macos")]
    let _header_loan = crate::menu::lend_context_menu_header(&result.menu);

    // The tag colors as Finder's one row of circles instead of seven stacked items, swapped
    // in at the same moment and for the same reason. ❌ Never `let _ =`: its `Drop` also lets
    // the row go of the items, which must not happen before `popup()` returns.
    #[cfg(target_os = "macos")]
    let _tag_row_loan = crate::menu::lend_tag_row(&result.menu, info.applied_tag_colors.ready().unwrap_or(&[false; 8]));

    // The facts that missed the grace period, landing on this menu while it's up. ❌ Never
    // `let _ =`: its `Drop` is what makes a late answer for a closed menu go nowhere.
    #[cfg(target_os = "macos")]
    let _live_loan = if collected.pending.is_empty() {
        None
    } else {
        crate::menu::lend_live_menu(result.late, generation, collected.cancel)
    };

    popup_context_menu(&result.menu, window, anchor)?;

    Ok(())
}

/// Shows a native context menu for the breadcrumb path bar.
///
/// `shortcuts` is the live shortcut registry, the same map the file context menu reads
/// its accelerator labels from (`crate::menu::ContextMenuShortcuts`).
/// `eject_volume_id` + `eject_volume_name` are set when the breadcrumb represents an
/// ejectable volume; both must be present (or both absent) — the command stashes the
/// id in `MenuState.volume_row_context` so `on_menu_event` can dispatch the click.
#[tauri::command]
#[specta::specta]
pub fn show_breadcrumb_context_menu<R: Runtime>(
    window: Window<R>,
    shortcuts: ContextMenuShortcuts,
    eject_volume_id: Option<String>,
    eject_volume_name: Option<String>,
) -> Result<(), String> {
    let app = window.app_handle();
    // Disable the eject item while a write op touches this volume or its eject is
    // still running (the picker's inline eject button is disabled the same way).
    let eject_busy = eject_volume_id
        .as_ref()
        .is_some_and(|id| crate::file_system::busy_volume_ids().contains(id) || is_ejecting(id));
    let detach_word = eject_volume_id
        .as_deref()
        .map_or(DetachWord::Eject, DetachWord::for_volume_id);
    let menu = build_breadcrumb_context_menu(app, &shortcuts, eject_volume_name.as_deref(), eject_busy, detach_word)
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

/// Whether `volume_id`'s eject is still running, so a native Eject item renders disabled
/// the way it does for a busy volume. A second pick would only join that eject anyway.
fn is_ejecting(volume_id: &str) -> bool {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        crate::file_system::volume::eject::ejecting_volume_ids()
            .iter()
            .any(|id| id == volume_id)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = volume_id;
        false
    }
}

/// Shows the minimal `..` parent-row context menu (just "Add to favorites").
///
/// `parent_path` is the directory the `..` row points at; we stash it in `MenuState.context.path`
/// so `on_menu_event` favorites it when the user clicks the item. The full file context menu makes
/// no sense on `..`, hence this dedicated one-item menu.
#[tauri::command]
#[specta::specta]
pub fn show_parent_row_context_menu<R: Runtime>(
    window: Window<R>,
    parent_path: String,
    anchor: Option<MenuAnchor>,
) -> Result<(), String> {
    let app = window.app_handle();
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.path = parent_path;
        context.filename = "..".to_string();
    }
    let menu = build_parent_row_context_menu(app).map_err(|e| e.to_string())?;
    popup_context_menu(&menu, window, anchor)?;
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

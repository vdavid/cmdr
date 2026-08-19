//! Building the menu bar again in a new language.
//!
//! A menu label can't be translated in place: muda has no `set_text` on a
//! `Submenu` title, and the SF Symbol and AppKit-cleanup passes resolve items
//! through the live bar anyway. So a language change throws the whole bar away
//! and builds a new one. That's the rare event (a person changing their
//! language), so correctness beats cleverness here: nothing tries to work out
//! which labels actually moved.
//!
//! What survives the rebuild is what Rust knows: the checked states, the
//! per-pane view modes, and which of the two macOS bars is installed. What
//! doesn't is what only the frontend knows (custom accelerators, the pin/unpin
//! label, whether "Reopen closed tab" is live) — those come back through the
//! [`MenuBarRebuilt`] event.

use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::Event as _;

use crate::ignore_poison::IgnorePoison as _;
use crate::system_events::MenuBarRebuilt;

use super::MenuState;

/// Rebuilds the whole native menu bar in the currently active UI language and
/// re-stores every item reference in [`MenuState`].
///
/// Call it after `intl::native_strings::refresh_active_locale` reports that the
/// active catalog moved; calling it when nothing moved costs a visible flicker
/// for nothing.
///
/// ❗ Must run on the main thread (it installs a menu, which is AppKit work).
/// The two callers that aren't already there hop via `run_on_main_thread`.
pub fn rebuild_menu_bar<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu_state = app.state::<MenuState<R>>();

    // Carry the state the new bar has to come up in. The show-hidden tick is read
    // off the live item (the menu itself is the authority for it, see
    // `menu/CLAUDE.md`); the view modes and the licence label come from the
    // cached copies `MenuState` already keeps for `rebuild_view_mode_items`.
    let show_hidden = menu_state
        .show_hidden_files
        .lock_ignore_poison()
        .as_ref()
        .and_then(|item| item.is_checked().ok())
        .unwrap_or(false);
    let view_mode = *menu_state.view_mode_left.lock_ignore_poison();
    let has_existing_license = menu_state
        .has_existing_license
        .load(std::sync::atomic::Ordering::Relaxed);

    let menu_items = super::build_menu(app, show_hidden, view_mode, has_existing_license)?;

    #[cfg(target_os = "macos")]
    let main_menu_clone = menu_items.menu.clone();
    let new_menu = menu_items.menu;

    *menu_state.show_hidden_files.lock_ignore_poison() = Some(menu_items.show_hidden_files);
    *menu_state.view_mode_full_left.lock_ignore_poison() = Some(menu_items.view_mode_full_left);
    *menu_state.view_mode_brief_left.lock_ignore_poison() = Some(menu_items.view_mode_brief_left);
    *menu_state.view_mode_full_right.lock_ignore_poison() = Some(menu_items.view_mode_full_right);
    *menu_state.view_mode_brief_right.lock_ignore_poison() = Some(menu_items.view_mode_brief_right);
    *menu_state.view_left_pane_submenu.lock_ignore_poison() = Some(menu_items.view_left_pane_submenu);
    *menu_state.view_right_pane_submenu.lock_ignore_poison() = Some(menu_items.view_right_pane_submenu);
    *menu_state.pin_tab.lock_ignore_poison() = Some(menu_items.pin_tab);
    *menu_state.reopen_closed_tab.lock_ignore_poison() = Some(menu_items.reopen_closed_tab);
    *menu_state.items.lock_ignore_poison() = menu_items.items;
    *menu_state.sort_submenu.lock_ignore_poison() = Some(menu_items.sort_submenu);

    install(app, &menu_state, new_menu)?;

    // The right-hand pane's mode isn't a `build_menu` argument (that call only
    // takes one), so put it back afterwards. This also re-attaches the keyboard
    // accelerator to whichever pane is active.
    super::rebuild_view_mode_items(app, &menu_state)?;

    #[cfg(target_os = "macos")]
    {
        *menu_state.main_menu.lock_ignore_poison() = Some(main_menu_clone);
    }

    if let Err(e) = (MenuBarRebuilt {}).emit(app) {
        log::warn!(target: "menu", "Menu bar rebuilt, but the frontend wasn't told, so custom accelerators stay stale: {e}");
    }
    Ok(())
}

/// Installs the freshly built bar, and on macOS the freshly built viewer bar
/// alongside it.
///
/// macOS has one app-level bar, so which of the two is installed depends on
/// which window has focus right now; `active_menu_kind` already tracks that, so
/// the rebuild honors it rather than yanking a focused viewer back to the main
/// bar. Both post-construction passes run afterwards, exactly as the focus-swap
/// path does (`menu/DETAILS.md` § Per-window menu activation).
#[cfg(target_os = "macos")]
fn install<R: Runtime>(
    app: &AppHandle<R>,
    menu_state: &MenuState<R>,
    main_menu: tauri::menu::Menu<R>,
) -> tauri::Result<()> {
    use super::ActiveMenuKind;

    let viewer_menu_items = super::build_viewer_menu(app)?;
    let viewer_menu = viewer_menu_items.menu.clone();
    *menu_state.viewer_word_wrap.lock_ignore_poison() = Some(viewer_menu_items.word_wrap);
    *menu_state.viewer_menu.lock_ignore_poison() = Some(viewer_menu_items.menu);

    let active = *menu_state.active_menu_kind.lock_ignore_poison();
    app.set_menu(match active {
        ActiveMenuKind::Main => main_menu,
        ActiveMenuKind::Viewer => viewer_menu,
    })?;

    // AppKit re-injects its Edit items on every `set_menu`, and SF Symbols never
    // survive one.
    super::cleanup_macos_menus(app);
    if active == ActiveMenuKind::Main {
        super::set_macos_menu_icons(app);
    }
    Ok(())
}

/// Installs the freshly built bar on the main window.
///
/// Linux menus are per-window, so viewer windows keep whatever
/// `viewer_setup_menu` gave them; a viewer open across a language change keeps
/// its old labels until it's reopened. Acceptable: Linux has no live OS-language
/// signal at all (`intl/live_locale.rs`), so the only way to get here is the
/// user changing the setting in Cmdr's own Settings window.
#[cfg(not(target_os = "macos"))]
fn install<R: Runtime>(
    app: &AppHandle<R>,
    _menu_state: &MenuState<R>,
    main_menu: tauri::menu::Menu<R>,
) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_menu(main_menu)?;
    }
    Ok(())
}

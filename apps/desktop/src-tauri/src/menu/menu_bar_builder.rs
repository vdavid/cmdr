//! Builds `menu_bar::MENU_BAR` into the native menu bar for the platform this binary runs on, and
//! collects the handles `MenuState` keeps.

use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu},
};

use crate::intl::menu_t;

use super::menu_bar::MENU_BAR;
use super::menu_items::APP_MENU_TITLE;
use super::menu_spec::{CheckRole, EntryKind, Label, Pane, Platform, Predefined, SubmenuRole, SubmenuSpec, Tracking};
use super::mnemonics::Mnemonics;
use super::{MenuItemEntry, MenuItems, ViewMode};

/// Builds the application menu bar for the current platform, in the active UI language.
///
/// `view_mode` is the left pane's. The right pane's items start on Brief, and
/// `rebuild_view_mode_items` sets both panes from the frontend's state afterwards.
pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
) -> tauri::Result<MenuItems<R>> {
    let platform = Platform::current();
    let mut builder = Builder {
        app,
        platform,
        show_hidden_files,
        view_mode,
        has_existing_license,
        items: HashMap::new(),
        show_hidden: None,
        left: PaneHandles::new(),
        right: PaneHandles::new(),
        pin_tab: None,
        sort_submenu: None,
    };
    let menu = Menu::new(app)?;
    // The bar's own titles share one mnemonic allocator, and each submenu gets its own.
    let mut titles = Mnemonics::new();
    for bar_menu in MENU_BAR.iter().filter(|bar_menu| bar_menu.is_on(platform)) {
        let title = builder.label(bar_menu.submenu.title, &mut titles);
        let submenu = builder.submenu(&bar_menu.submenu, &title)?;
        menu.append(&submenu)?;
    }
    Ok(builder.into_menu_items(menu))
}

/// The build's inputs, plus the handles it collects on the way for [`MenuItems`].
struct Builder<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    platform: Platform,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
    items: HashMap<String, MenuItemEntry<R>>,
    show_hidden: Option<CheckMenuItem<R>>,
    left: PaneHandles<R>,
    right: PaneHandles<R>,
    pin_tab: Option<MenuItem<R>>,
    sort_submenu: Option<Submenu<R>>,
}

/// One pane's view-mode submenu and the Full and Brief items in it.
struct PaneHandles<R: Runtime> {
    full: Option<CheckMenuItem<R>>,
    brief: Option<CheckMenuItem<R>>,
    submenu: Option<Submenu<R>>,
}

impl<R: Runtime> PaneHandles<R> {
    fn new() -> Self {
        Self {
            full: None,
            brief: None,
            submenu: None,
        }
    }
}

impl<R: Runtime> Builder<'_, R> {
    /// Builds `spec` titled `title`, registering each tracked item at its position in it.
    ///
    /// Labels get their mnemonics in display order, which is what makes a given translation always
    /// underline the same letters.
    fn submenu(&mut self, spec: &SubmenuSpec, title: &str) -> tauri::Result<Submenu<R>> {
        let app = self.app;
        let platform = self.platform;
        let mut mnemonics = Mnemonics::new();
        let mut children = Vec::new();
        let mut tracked = Vec::new();
        for (position, entry) in spec.entries_on(platform).enumerate() {
            let child = match entry {
                EntryKind::Item(item) => {
                    let label = self.label(item.label, &mut mnemonics);
                    let built = MenuItem::with_id(app, item.id, label, item.enabled, item.accelerator.on(platform))?;
                    match item.tracking {
                        Tracking::Tracked => tracked.push((item.id, position, built.clone())),
                        Tracking::Untracked => {}
                        Tracking::PinTab => self.pin_tab = Some(built.clone()),
                    }
                    MenuItemKind::MenuItem(built)
                }
                EntryKind::Check(check) => {
                    let label = self.label(check.label, &mut mnemonics);
                    let checked = match check.role {
                        CheckRole::ShowHiddenFiles => self.show_hidden_files,
                        CheckRole::ViewMode(pane, mode) => self.initial_view_mode(pane) == mode,
                    };
                    let accelerator = check.accelerator.on(platform);
                    let built = CheckMenuItem::with_id(app, check.id, label, true, checked, accelerator)?;
                    match check.role {
                        CheckRole::ShowHiddenFiles => self.show_hidden = Some(built.clone()),
                        CheckRole::ViewMode(pane, ViewMode::Full) => self.pane(pane).full = Some(built.clone()),
                        CheckRole::ViewMode(pane, ViewMode::Brief) => self.pane(pane).brief = Some(built.clone()),
                    }
                    MenuItemKind::Check(built)
                }
                EntryKind::Separator => MenuItemKind::Predefined(PredefinedMenuItem::separator(app)?),
                EntryKind::Predefined(kind, label) => {
                    let label = self.label(*label, &mut mnemonics);
                    MenuItemKind::Predefined(predefined(app, *kind, &label)?)
                }
                EntryKind::Submenu(nested) => {
                    let nested_title = self.label(nested.title, &mut mnemonics);
                    let built = self.submenu(nested, &nested_title)?;
                    match nested.role {
                        SubmenuRole::Plain => {}
                        SubmenuRole::SortBy => self.sort_submenu = Some(built.clone()),
                        SubmenuRole::Pane(pane) => self.pane(pane).submenu = Some(built.clone()),
                    }
                    MenuItemKind::Submenu(built)
                }
            };
            children.push(child);
        }

        let refs: Vec<&dyn IsMenuItem<R>> = children.iter().map(|child| child as &dyn IsMenuItem<R>).collect();
        let submenu = match spec.id.on(platform) {
            Some(id) => Submenu::with_id_and_items(app, id, title, true, &refs)?,
            None => Submenu::with_items(app, title, true, &refs)?,
        };
        for (id, position, item) in tracked {
            self.items.insert(
                id.to_string(),
                MenuItemEntry {
                    item,
                    submenu: submenu.clone(),
                    position,
                },
            );
        }
        Ok(submenu)
    }

    /// `label` in the active language, with a mnemonic from `mnemonics` where the platform uses them.
    fn label(&self, label: Label, mnemonics: &mut Mnemonics) -> String {
        let text = label
            .catalog_key(self.platform, self.has_existing_license)
            .map_or_else(|| APP_MENU_TITLE.to_string(), menu_t);
        mnemonics.assign(&text)
    }

    /// The mode a pane's items start checked on: the caller's for the left pane, the default for the
    /// right one until `rebuild_view_mode_items` runs.
    fn initial_view_mode(&self, pane: Pane) -> ViewMode {
        match pane {
            Pane::Left => self.view_mode,
            Pane::Right => ViewMode::default(),
        }
    }

    fn pane(&mut self, pane: Pane) -> &mut PaneHandles<R> {
        match pane {
            Pane::Left => &mut self.left,
            Pane::Right => &mut self.right,
        }
    }

    fn into_menu_items(self, menu: Menu<R>) -> MenuItems<R> {
        MenuItems {
            menu,
            show_hidden_files: built(self.show_hidden, "Show hidden files item"),
            view_mode_full_left: built(self.left.full, "left pane's Full view item"),
            view_mode_brief_left: built(self.left.brief, "left pane's Brief view item"),
            view_mode_full_right: built(self.right.full, "right pane's Full view item"),
            view_mode_brief_right: built(self.right.brief, "right pane's Brief view item"),
            view_left_pane_submenu: built(self.left.submenu, "Left pane submenu"),
            view_right_pane_submenu: built(self.right.submenu, "Right pane submenu"),
            pin_tab: built(self.pin_tab, "Pin tab item"),
            items: self.items,
            sort_submenu: built(self.sort_submenu, "Sort by submenu"),
        }
    }
}

/// A handle every platform's bar builds, which `menu_bar_test.rs` pins, so a `None` here is a broken
/// `MENU_BAR` rather than something to recover from.
fn built<T>(handle: Option<T>, what: &str) -> T {
    handle.unwrap_or_else(|| panic!("`MENU_BAR` builds no {what} on this platform"))
}

fn predefined<R: Runtime>(app: &AppHandle<R>, kind: Predefined, label: &str) -> tauri::Result<PredefinedMenuItem<R>> {
    let text = Some(label);
    match kind {
        Predefined::Services => PredefinedMenuItem::services(app, text),
        Predefined::Hide => PredefinedMenuItem::hide(app, text),
        Predefined::HideOthers => PredefinedMenuItem::hide_others(app, text),
        Predefined::ShowAll => PredefinedMenuItem::show_all(app, text),
        Predefined::Quit => PredefinedMenuItem::quit(app, text),
        Predefined::Undo => PredefinedMenuItem::undo(app, text),
        Predefined::Redo => PredefinedMenuItem::redo(app, text),
        Predefined::Minimize => PredefinedMenuItem::minimize(app, text),
        Predefined::Maximize => PredefinedMenuItem::maximize(app, text),
    }
}

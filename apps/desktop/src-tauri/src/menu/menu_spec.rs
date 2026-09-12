//! The words `menu_bar.rs` writes the native menu bar in, and where a platform difference resolves.
//!
//! Plain data with no `cfg`, so both platforms' bars exist on every host: `menu_bar_test.rs` pins
//! the Linux bar on a Mac too. `menu_bar_builder.rs` builds the one for [`Platform::current`].

use super::ViewMode;

/// The two menu bars Cmdr builds. Every platform that isn't macOS gets the Linux one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    MacOs,
    Linux,
}

impl Platform {
    /// The platform this binary was built for.
    pub(crate) const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// A value each platform sets on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PerPlatform<T> {
    pub(crate) macos: T,
    pub(crate) linux: T,
}

impl<T: Copy> PerPlatform<T> {
    pub(crate) fn on(self, platform: Platform) -> T {
        match platform {
            Platform::MacOs => self.macos,
            Platform::Linux => self.linux,
        }
    }

    /// The value for the platform this binary was built for.
    pub(crate) fn current(self) -> T {
        self.on(Platform::current())
    }
}

/// A menu item's keyboard accelerator on each platform, in Tauri's accelerator syntax.
pub(crate) type Accelerator = PerPlatform<Option<&'static str>>;

/// No accelerator on either platform.
pub(crate) const NONE: Accelerator = PerPlatform {
    macos: None,
    linux: None,
};

/// The same accelerator on both platforms.
pub(crate) const fn both(accelerator: &'static str) -> Accelerator {
    PerPlatform {
        macos: Some(accelerator),
        linux: Some(accelerator),
    }
}

/// An accelerator only the macOS bar registers.
pub(crate) const fn macos(accelerator: &'static str) -> Accelerator {
    PerPlatform {
        macos: Some(accelerator),
        linux: None,
    }
}

/// One accelerator per platform.
pub(crate) const fn split(on_macos: &'static str, on_linux: &'static str) -> Accelerator {
    PerPlatform {
        macos: Some(on_macos),
        linux: Some(on_linux),
    }
}

/// What a menu or an item says, before translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Label {
    /// A `menu.*` catalog key.
    Key(&'static str),
    /// One catalog key per platform.
    PerPlatformKey(PerPlatform<&'static str>),
    /// `menu.app.licenseDetails` once the user has a license key, `menu.app.licenseEnter` before.
    /// Only the second takes input, so only it gets the ellipsis.
    License,
    /// `menu_items::APP_MENU_TITLE`, which is never translated.
    AppName,
}

impl Label {
    /// The catalog key this label reads, or `None` for [`Label::AppName`], which has none.
    pub(crate) fn catalog_key(self, platform: Platform, has_existing_license: bool) -> Option<&'static str> {
        match self {
            Self::Key(key) => Some(key),
            Self::PerPlatformKey(keys) => Some(keys.on(platform)),
            Self::License if has_existing_license => Some("menu.app.licenseDetails"),
            Self::License => Some("menu.app.licenseEnter"),
            Self::AppName => None,
        }
    }
}

/// An item muda builds wired to the OS's own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Predefined {
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    Undo,
    Redo,
    Minimize,
    Maximize,
}

/// Where the built menu item is kept once the bar is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tracking {
    /// In `MenuState.items`, with its submenu and its position there: `apply_menu_item_states` sets
    /// whether it's enabled, and `update_menu_item_accelerator` swaps a custom shortcut onto it by
    /// removing it and reinserting a new item at that position.
    Tracked,
    /// Nowhere. Nothing changes it after the build.
    Untracked,
    /// In `MenuState.pin_tab`, whose label the frontend flips between Pin tab and Unpin tab.
    PinTab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pane {
    Left,
    Right,
}

/// What a check item toggles, which decides how it starts and which `MenuState` field keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckRole {
    /// `MenuState.show_hidden_files`, checked while hidden files show.
    ShowHiddenFiles,
    /// One pane's Full or Brief item (`MenuState.view_mode_full_left` and its three siblings),
    /// checked while that pane is in that mode.
    ViewMode(Pane, ViewMode),
}

/// Which `MenuState` field keeps a submenu, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubmenuRole {
    /// None: nothing changes it after the build.
    Plain,
    /// `MenuState.sort_submenu`, greyed out as a whole while the explorer doesn't own the menu.
    SortBy,
    /// `MenuState.view_left_pane_submenu` or `view_right_pane_submenu`, which
    /// `rebuild_view_mode_items` reinserts that pane's Full and Brief items into.
    Pane(Pane),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ItemSpec {
    pub(crate) id: &'static str,
    pub(crate) label: Label,
    pub(crate) accelerator: Accelerator,
    pub(crate) enabled: bool,
    pub(crate) tracking: Tracking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CheckSpec {
    pub(crate) id: &'static str,
    pub(crate) label: Label,
    pub(crate) accelerator: Accelerator,
    pub(crate) role: CheckRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SubmenuSpec {
    /// The ID the submenu is built with, or `None` to build it without one. macOS's AppKit passes
    /// find a menu by ID (`menu/DETAILS.md` § "Finding a menu from AppKit").
    pub(crate) id: PerPlatform<Option<&'static str>>,
    pub(crate) title: Label,
    pub(crate) role: SubmenuRole,
    pub(crate) entries: &'static [Entry],
}

impl SubmenuSpec {
    /// This submenu's entries on `platform`, in display order, so each one's index is its position.
    pub(crate) fn entries_on(&self, platform: Platform) -> impl Iterator<Item = &'static EntryKind> {
        self.entries
            .iter()
            .filter(move |entry| entry.only.is_none_or(|only| only == platform))
            .map(|entry| &entry.kind)
    }
}

/// One row of a submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The one platform this row exists on, or `None` for both.
    pub(crate) only: Option<Platform>,
    pub(crate) kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Item(ItemSpec),
    Check(CheckSpec),
    Separator,
    Predefined(Predefined, Label),
    Submenu(SubmenuSpec),
}

/// One top-level menu in the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BarMenu {
    /// The one platform this menu exists on, or `None` for both.
    pub(crate) only: Option<Platform>,
    pub(crate) submenu: SubmenuSpec,
}

impl BarMenu {
    pub(crate) fn is_on(&self, platform: Platform) -> bool {
        self.only.is_none_or(|only| only == platform)
    }
}

/// A top-level menu on both platforms. Its ID reaches macOS alone: nothing on Linux looks a menu
/// up, so Linux builds it without one.
pub(crate) const fn menu(id: &'static str, title: &'static str, entries: &'static [Entry]) -> BarMenu {
    BarMenu {
        only: None,
        submenu: top_level(id, Label::Key(title), entries),
    }
}

/// A top-level menu only macOS has.
pub(crate) const fn macos_menu(id: &'static str, title: Label, entries: &'static [Entry]) -> BarMenu {
    BarMenu {
        only: Some(Platform::MacOs),
        submenu: top_level(id, title, entries),
    }
}

const fn top_level(id: &'static str, title: Label, entries: &'static [Entry]) -> SubmenuSpec {
    SubmenuSpec {
        id: PerPlatform {
            macos: Some(id),
            linux: None,
        },
        title,
        role: SubmenuRole::Plain,
        entries,
    }
}

/// A command item, [`Tracking::Tracked`].
pub(crate) const fn item(id: &'static str, key: &'static str, accelerator: Accelerator) -> Entry {
    labeled_item(id, Label::Key(key), accelerator)
}

/// A command item whose label isn't a single catalog key, [`Tracking::Tracked`].
pub(crate) const fn labeled_item(id: &'static str, label: Label, accelerator: Accelerator) -> Entry {
    both_platforms(EntryKind::Item(ItemSpec {
        id,
        label,
        accelerator,
        enabled: true,
        tracking: Tracking::Tracked,
    }))
}

pub(crate) const fn check(id: &'static str, key: &'static str, accelerator: Accelerator, role: CheckRole) -> Entry {
    both_platforms(EntryKind::Check(CheckSpec {
        id,
        label: Label::Key(key),
        accelerator,
        role,
    }))
}

pub(crate) const SEPARATOR: Entry = both_platforms(EntryKind::Separator);

pub(crate) const fn predefined(kind: Predefined, key: &'static str) -> Entry {
    both_platforms(EntryKind::Predefined(kind, Label::Key(key)))
}

/// A submenu nested in another, built with the same `id` on both platforms.
pub(crate) const fn submenu(
    id: Option<&'static str>,
    key: &'static str,
    role: SubmenuRole,
    entries: &'static [Entry],
) -> Entry {
    both_platforms(EntryKind::Submenu(SubmenuSpec {
        id: PerPlatform { macos: id, linux: id },
        title: Label::Key(key),
        role,
        entries,
    }))
}

pub(crate) const fn macos_only(entry: Entry) -> Entry {
    Entry {
        only: Some(Platform::MacOs),
        kind: entry.kind,
    }
}

pub(crate) const fn linux_only(entry: Entry) -> Entry {
    Entry {
        only: Some(Platform::Linux),
        kind: entry.kind,
    }
}

const fn both_platforms(kind: EntryKind) -> Entry {
    Entry { only: None, kind }
}

impl Entry {
    /// This item, [`Tracking::Untracked`].
    pub(crate) const fn untracked(self) -> Self {
        self.tracked_as(Tracking::Untracked)
    }

    /// This item, kept as [`Tracking::PinTab`].
    pub(crate) const fn pin_tab(self) -> Self {
        self.tracked_as(Tracking::PinTab)
    }

    /// This item, built disabled.
    pub(crate) const fn disabled(self) -> Self {
        match self.kind {
            EntryKind::Item(mut item) => {
                item.enabled = false;
                Self {
                    only: self.only,
                    kind: EntryKind::Item(item),
                }
            }
            _ => panic!("only a command item can start disabled"),
        }
    }

    const fn tracked_as(self, tracking: Tracking) -> Self {
        match self.kind {
            EntryKind::Item(mut item) => {
                item.tracking = tracking;
                Self {
                    only: self.only,
                    kind: EntryKind::Item(item),
                }
            }
            _ => panic!("only a command item is tracked"),
        }
    }
}

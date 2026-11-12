//! The words `bar.rs` writes the menu bar in, and where a platform difference resolves.
//!
//! Plain data with no `cfg`, so both platforms' bars exist on every host: `bar_test.rs` pins the
//! Linux bar on a Mac too. The desktop's `menu_bar_builder.rs` builds the one for
//! [`Platform::current`]; the browser demo renders the macOS one.

use cmdr_sort::{SortColumn, SortOrder};

/// A pane's view mode, as the frontend names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    Full,
    #[default]
    Brief,
}

impl ViewMode {
    /// The frontend's word for it, which `view-mode-changed` and `update_view_mode_menu` carry.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Brief => "brief",
        }
    }

    /// The mode the frontend's word names, or `None` for a word it doesn't send.
    pub fn from_wire(word: &str) -> Option<Self> {
        match word {
            "full" => Some(Self::Full),
            "brief" => Some(Self::Brief),
            _ => None,
        }
    }
}

/// The two menu bars Cmdr builds. Every platform that isn't macOS gets the Linux one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Linux,
}

impl Platform {
    /// The platform this binary was built for.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// A value each platform sets on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerPlatform<T> {
    pub macos: T,
    pub linux: T,
}

impl<T: Copy> PerPlatform<T> {
    pub fn on(self, platform: Platform) -> T {
        match platform {
            Platform::MacOs => self.macos,
            Platform::Linux => self.linux,
        }
    }

    /// The value for the platform this binary was built for.
    pub fn current(self) -> T {
        self.on(Platform::current())
    }
}

/// A menu item's keyboard accelerator on each platform, in Tauri's accelerator syntax.
pub type Accelerator = PerPlatform<Option<&'static str>>;

/// No accelerator on either platform.
pub const NONE: Accelerator = PerPlatform {
    macos: None,
    linux: None,
};

/// The same accelerator on both platforms. ❗ Refuses one that holds ⌘ at compile time:
/// [`cmd_or_ctrl!`] is the spelling for that.
pub const fn both(accelerator: &'static str) -> Accelerator {
    assert!(
        !holds_super(accelerator),
        "a ⌘ chord is Ctrl on Linux: write it `cmd_or_ctrl!`, not `both`"
    );
    PerPlatform {
        macos: Some(accelerator),
        linux: Some(accelerator),
    }
}

/// A ⌘ chord on macOS and the same chord on Ctrl on Linux: `cmd_or_ctrl!("Shift+P")` is
/// `Cmd+Shift+P` / `Ctrl+Shift+P`.
///
/// ❗ Not one shared `Cmd+…` string, because muda reads `Cmd` as `Modifiers::META`, which GTK
/// binds and prints as the Super key, so the Linux menu offered Super chords the window manager
/// usually owns (#284). ❌ Not `CmdOrCtrl` either: it resolves by the HOST muda runs on, so
/// `bar_test.rs` couldn't pin the Linux bar from a Mac.
macro_rules! cmd_or_ctrl {
    ($chord:literal) => {
        $crate::spec::split(concat!("Cmd+", $chord), concat!("Ctrl+", $chord))
    };
}
pub(crate) use cmd_or_ctrl;

/// An accelerator only the macOS bar registers.
pub const fn macos(accelerator: &'static str) -> Accelerator {
    PerPlatform {
        macos: Some(accelerator),
        linux: None,
    }
}

/// One accelerator per platform. ❗ The Linux one can't hold Super, at compile time (#284).
pub const fn split(on_macos: &'static str, on_linux: &'static str) -> Accelerator {
    assert!(
        !holds_super(on_linux),
        "`Cmd` / `Super` binds the Super key on Linux: spell the Linux chord with Ctrl"
    );
    PerPlatform {
        macos: Some(on_macos),
        linux: Some(on_linux),
    }
}

/// Whether one of `accelerator`'s `+`-separated tokens is a modifier muda maps to
/// `Modifiers::META` (`Cmd`, `Command`, `Super`, case-insensitive), the Super key on Linux.
/// `const`, so [`both`] and [`split`] can refuse one while `MENU_BAR` compiles.
const fn holds_super(accelerator: &str) -> bool {
    let bytes = accelerator.as_bytes();
    let mut start = 0;
    let mut end = 0;
    while end <= bytes.len() {
        if end == bytes.len() || bytes[end] == b'+' {
            let (_, tail) = bytes.split_at(start);
            let (token, _) = tail.split_at(end - start);
            if token.eq_ignore_ascii_case(b"cmd")
                || token.eq_ignore_ascii_case(b"command")
                || token.eq_ignore_ascii_case(b"super")
            {
                return true;
            }
            start = end + 1;
        }
        end += 1;
    }
    false
}

/// How a display-only shortcut reaches the user, which is the one place the two platforms
/// disagree about [`ItemSpec::display_accelerator`].
///
/// macOS gets it from `macos_appkit::set_display_accelerators`, as a right-aligned, dimmed
/// run on the item's attributed title, so it reads exactly like a real key equivalent. GTK
/// offers no such hook, so Linux spells it into the label itself. Both callers (the builder
/// and `bar_test.rs`) go through here, so the snapshot pins what the bar really says.
pub fn display_accelerator_label(label: &str, shortcut: &str, platform: Platform) -> String {
    match platform {
        Platform::MacOs => label.to_string(),
        Platform::Linux => format!("{label} ({shortcut})"),
    }
}

/// What a menu or an item says, before translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// A `menu.*` catalog key.
    Key(&'static str),
    /// `menu.app.licenseDetails` once the user has a license key, `menu.app.licenseEnter` before.
    /// Only the second takes input, so only it gets the ellipsis.
    License,
    /// `menu_items::APP_MENU_TITLE`, which is never translated.
    AppName,
}

impl Label {
    /// The catalog key this label reads, or `None` for [`Label::AppName`], which has none.
    ///
    /// One key on every platform: a label that reads differently per OS is one catalog value with a
    /// branch per OS, picked where the text is looked up.
    pub fn catalog_key(self, has_existing_license: bool) -> Option<&'static str> {
        match self {
            Self::Key(key) => Some(key),
            Self::License if has_existing_license => Some("menu.app.licenseDetails"),
            Self::License => Some("menu.app.licenseEnter"),
            Self::AppName => None,
        }
    }
}

/// An item muda builds wired to the OS's own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Predefined {
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
pub enum Tracking {
    /// In `MenuState.items`, with its submenu and its position there: `apply_menu_item_states` sets
    /// whether it's enabled, and `update_menu_item_accelerator` swaps a custom shortcut onto it by
    /// removing it and reinserting a new item at that position.
    Tracked,
    /// In `MenuState.untracked_items` only: `apply_menu_item_states` sets whether it's enabled, but
    /// no custom shortcut is swapped onto it (macOS's app-menu rows, the Zoom presets).
    Untracked,
    /// In `MenuState.pin_tab`, whose label the frontend flips between Pin tab and Unpin tab.
    PinTab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Left,
    Right,
}

impl Pane {
    /// The frontend's word for it, which `view-mode-changed` and `update_view_mode_menu` carry.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// What a check item toggles, which decides how it starts and which `MenuState` field keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckRole {
    /// `MenuState.show_hidden_files`, checked while hidden files show.
    ShowHiddenFiles,
    /// One pane's Full or Brief item (`MenuState.view_mode_full_left` and its three siblings),
    /// checked while that pane is in that mode.
    ViewMode(Pane, ViewMode),
    /// A View > Sort by column or order, checked while the focused pane asks for it. Kept in no
    /// field: `sort_checks::sync_sort_check_states` finds each one by id in `MenuState.sort_submenu`.
    Sort(SortCheck),
}

/// What one View > Sort by check item stands for. Its predicate, [`SortCheck::is_on`], lives with
/// the state it reads, in `sort.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortCheck {
    Column(SortColumn),
    Order(SortOrder),
}

/// Which `MenuState` field keeps a submenu, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmenuRole {
    /// None: nothing changes it after the build.
    Plain,
    /// `MenuState.sort_submenu`, greyed out as a whole while the explorer doesn't own the menu.
    SortBy,
    /// `MenuState.view_left_pane_submenu` or `view_right_pane_submenu`, which
    /// `rebuild_view_mode_items` reinserts that pane's Full and Brief items into.
    Pane(Pane),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemSpec {
    pub id: &'static str,
    pub label: Label,
    pub accelerator: Accelerator,
    /// A shortcut the menu only SHOWS, in the frontend's display spelling (`⇧8`, `+`).
    ///
    /// The key carries no ⌘ / ⌃ / ⌥, so registering it would swallow the character
    /// app-wide (`accelerators.rs`'s modifier floor); the file pane's keydown handler
    /// owns it instead, and this is how the menu still tells the user about it. One
    /// glyph for both platforms: they differ in how it's DRAWN, not in what it says
    /// ([`display_accelerator_label`]). Set only by [`displayed_item`], which pairs it
    /// with [`NONE`] so an item can't claim both.
    pub display_accelerator: Option<&'static str>,
    pub enabled: bool,
    pub tracking: Tracking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckSpec {
    pub id: &'static str,
    pub label: Label,
    pub accelerator: Accelerator,
    pub role: CheckRole,
    /// Whether it's also in `MenuState.items`, like a [`Tracking::Tracked`] command item, so a
    /// custom shortcut can be swapped onto it. `update_menu_item_accelerator` recreates a check
    /// item as a check item, its checked state included.
    pub tracked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmenuSpec {
    /// The ID the submenu is built with, or `None` to build it without one. macOS's AppKit passes
    /// find a menu by ID (`menu/DETAILS.md` § "Finding a menu from AppKit").
    pub id: PerPlatform<Option<&'static str>>,
    pub title: Label,
    pub role: SubmenuRole,
    pub entries: &'static [Entry],
}

impl SubmenuSpec {
    /// This submenu's entries on `platform`, in display order, so each one's index is its position.
    pub fn entries_on(&self, platform: Platform) -> impl Iterator<Item = &'static EntryKind> {
        self.entries
            .iter()
            .filter(move |entry| entry.only.is_none_or(|only| only == platform))
            .map(|entry| &entry.kind)
    }
}

/// One row of a submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The one platform this row exists on, or `None` for both.
    pub only: Option<Platform>,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Item(ItemSpec),
    Check(CheckSpec),
    Separator,
    Predefined(Predefined, Label),
    Submenu(SubmenuSpec),
}

/// One top-level menu in the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarMenu {
    /// The one platform this menu exists on, or `None` for both.
    pub only: Option<Platform>,
    pub submenu: SubmenuSpec,
}

impl BarMenu {
    pub fn is_on(&self, platform: Platform) -> bool {
        self.only.is_none_or(|only| only == platform)
    }
}

/// A top-level menu on both platforms. Its ID reaches macOS alone: nothing on Linux looks a menu
/// up, so Linux builds it without one.
pub const fn menu(id: &'static str, title: &'static str, entries: &'static [Entry]) -> BarMenu {
    BarMenu {
        only: None,
        submenu: top_level(id, Label::Key(title), entries),
    }
}

/// A top-level menu only macOS has.
pub const fn macos_menu(id: &'static str, title: Label, entries: &'static [Entry]) -> BarMenu {
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
pub const fn item(id: &'static str, key: &'static str, accelerator: Accelerator) -> Entry {
    labeled_item(id, Label::Key(key), accelerator)
}

/// A command item whose label isn't a single catalog key, [`Tracking::Tracked`].
pub const fn labeled_item(id: &'static str, label: Label, accelerator: Accelerator) -> Entry {
    both_platforms(EntryKind::Item(ItemSpec {
        id,
        label,
        accelerator,
        display_accelerator: None,
        enabled: true,
        tracking: Tracking::Tracked,
    }))
}

/// A command item whose shortcut the menu can only SHOW, [`Tracking::Tracked`].
///
/// `shortcut` is the frontend's display spelling (`⇧8`, `+`, `⌥⇧=`). ❗ It is never
/// registered: the accelerator is [`NONE`] on both platforms, by construction, because a
/// combo with no ⌘ / ⌃ / ⌥ would fire app-wide and eat that character in every text
/// field. The file pane's keydown handler is what actually runs the command.
pub const fn displayed_item(id: &'static str, key: &'static str, shortcut: &'static str) -> Entry {
    both_platforms(EntryKind::Item(ItemSpec {
        id,
        label: Label::Key(key),
        accelerator: NONE,
        display_accelerator: Some(shortcut),
        enabled: true,
        tracking: Tracking::Tracked,
    }))
}

pub const fn check(id: &'static str, key: &'static str, accelerator: Accelerator, role: CheckRole) -> Entry {
    both_platforms(EntryKind::Check(CheckSpec {
        id,
        label: Label::Key(key),
        accelerator,
        role,
        tracked: false,
    }))
}

pub const SEPARATOR: Entry = both_platforms(EntryKind::Separator);

pub const fn predefined(kind: Predefined, key: &'static str) -> Entry {
    both_platforms(EntryKind::Predefined(kind, Label::Key(key)))
}

/// A submenu nested in another, built with the same `id` on both platforms.
pub const fn submenu(
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

pub const fn macos_only(entry: Entry) -> Entry {
    Entry {
        only: Some(Platform::MacOs),
        kind: entry.kind,
    }
}

pub const fn linux_only(entry: Entry) -> Entry {
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
    pub const fn untracked(self) -> Self {
        self.tracked_as(Tracking::Untracked)
    }

    /// This check item, also kept in `MenuState.items` ([`CheckSpec::tracked`]).
    pub const fn tracked(self) -> Self {
        match self.kind {
            EntryKind::Check(mut check) => {
                check.tracked = true;
                Self {
                    only: self.only,
                    kind: EntryKind::Check(check),
                }
            }
            _ => panic!("a command item is tracked already; only a check item opts in"),
        }
    }

    /// This item, kept as [`Tracking::PinTab`].
    pub const fn pin_tab(self) -> Self {
        self.tracked_as(Tracking::PinTab)
    }

    /// This item, built disabled.
    pub const fn disabled(self) -> Self {
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

//! The words `bar.rs` writes the menu bar in, and where a platform difference resolves.
//!
//! Plain data with no `cfg`, so all three platforms' bars exist on every host: `bar_test.rs` pins
//! the Windows and Linux bars on a Mac too. The desktop's `menu_bar_builder.rs` builds the one for
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

/// The three platforms Cmdr builds a menu bar for.
///
/// Windows has the Linux bar's rows and accelerators until the Windows menu model is decided
/// (`DETAILS.md` § "Three platforms"), but it's its own value, so a row or a key can differ there
/// without touching Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    /// Every platform, in the order [`PerPlatform`] writes its arms.
    pub const ALL: [Self; 3] = [Self::MacOs, Self::Windows, Self::Linux];

    /// The platform this binary was built for, from the target's own OS name, so the data stays
    /// free of `cfg`. A wasm build answers Linux, so the browser demo names its platform
    /// explicitly.
    pub const fn current() -> Self {
        match std::env::consts::OS.as_bytes() {
            b"macos" => Self::MacOs,
            b"windows" => Self::Windows,
            _ => Self::Linux,
        }
    }

    /// The frontend's word for it (`Platform` in `lib/platform/`), which
    /// `registry_default_keys.tsv` is keyed by.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

/// A value each platform sets on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerPlatform<T> {
    pub macos: T,
    pub windows: T,
    pub linux: T,
}

impl<T: Copy> PerPlatform<T> {
    pub fn on(self, platform: Platform) -> T {
        match platform {
            Platform::MacOs => self.macos,
            Platform::Windows => self.windows,
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

/// A shortcut a menu item only SHOWS on each platform, in that platform's frontend display
/// spelling (`⌥⇧=` on macOS, `Alt+Shift+=` on Windows and Linux, `*`, `F5`).
pub type DisplayShortcut = PerPlatform<Option<&'static str>>;

/// The same shown shortcut on every platform: a key with no modifier glyph (`*`, `+`, `F11`).
pub const fn shown_on_all(shortcut: &'static str) -> DisplayShortcut {
    PerPlatform {
        macos: Some(shortcut),
        windows: Some(shortcut),
        linux: Some(shortcut),
    }
}

/// One shown shortcut per platform, each in its own spelling.
pub const fn shown_split(on_macos: &'static str, on_windows: &'static str, on_linux: &'static str) -> DisplayShortcut {
    PerPlatform {
        macos: Some(on_macos),
        windows: Some(on_windows),
        linux: Some(on_linux),
    }
}

/// A shortcut shown on Windows and Linux only, where the bar leaves the key to keydown dispatch
/// and macOS's bar registers it ([`Entry::shown`]).
pub const fn shown_off_macos(shortcut: &'static str) -> DisplayShortcut {
    PerPlatform {
        macos: None,
        windows: Some(shortcut),
        linux: Some(shortcut),
    }
}

/// No accelerator on any platform.
pub const NONE: Accelerator = PerPlatform {
    macos: None,
    windows: None,
    linux: None,
};

/// The same accelerator on every platform. ❗ Refuses one that holds ⌘ at compile time:
/// [`cmd_or_ctrl!`] is the spelling for that.
pub const fn same_on_all(accelerator: &'static str) -> Accelerator {
    assert!(
        !holds_super(accelerator),
        "a ⌘ chord is Ctrl on Windows and Linux: write it `cmd_or_ctrl!`, not `same_on_all`"
    );
    PerPlatform {
        macos: Some(accelerator),
        windows: Some(accelerator),
        linux: Some(accelerator),
    }
}

/// A ⌘ chord on macOS and the same chord on Ctrl on Windows and Linux: `cmd_or_ctrl!("Shift+P")`
/// is `Cmd+Shift+P` on macOS and `Ctrl+Shift+P` on the other two.
///
/// ❗ Not one shared `Cmd+…` string, because muda reads `Cmd` as `Modifiers::META`, which GTK
/// binds and prints as the Super key (and Win32 as the Windows key), so the Linux menu offered
/// Super chords the window manager usually owns (#284). ❌ Not `CmdOrCtrl` either: it resolves by
/// the HOST muda runs on, so `bar_test.rs` couldn't pin the Linux bar from a Mac.
macro_rules! cmd_or_ctrl {
    ($chord:literal) => {
        $crate::spec::split(
            concat!("Cmd+", $chord),
            concat!("Ctrl+", $chord),
            concat!("Ctrl+", $chord),
        )
    };
}
pub(crate) use cmd_or_ctrl;

/// An accelerator only the macOS bar registers.
pub const fn macos(accelerator: &'static str) -> Accelerator {
    PerPlatform {
        macos: Some(accelerator),
        windows: None,
        linux: None,
    }
}

/// One accelerator per platform. ❗ The Windows and Linux ones can't hold Super, at compile time
/// (#284).
pub const fn split(on_macos: &'static str, on_windows: &'static str, on_linux: &'static str) -> Accelerator {
    assert!(
        !holds_super(on_windows),
        "`Cmd` / `Super` binds the Windows key on Windows: spell the Windows chord with Ctrl"
    );
    assert!(
        !holds_super(on_linux),
        "`Cmd` / `Super` binds the Super key on Linux: spell the Linux chord with Ctrl"
    );
    PerPlatform {
        macos: Some(on_macos),
        windows: Some(on_windows),
        linux: Some(on_linux),
    }
}

/// Whether one of `accelerator`'s `+`-separated tokens is a modifier muda maps to
/// `Modifiers::META` (`Cmd`, `Command`, `Super`, case-insensitive): the Super key on Linux, the
/// Windows key on Windows. `const`, so [`same_on_all`] and [`split`] can refuse one while
/// `MENU_BAR` compiles.
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

/// How a display-only shortcut reaches the user, which is the one place the platforms disagree
/// about [`ItemSpec::display_accelerator`].
///
/// macOS gets it from `macos_appkit::set_display_accelerators`, as a right-aligned, dimmed
/// run on the item's attributed title, so it reads exactly like a real key equivalent. GTK
/// offers no such hook, so Linux spells it into the label itself, and Windows does the same
/// until its menu model is decided. Both callers (the builder and `bar_test.rs`) go through
/// here, so the snapshot pins what the bar really says.
pub fn display_accelerator_label(label: &str, shortcut: &str, platform: Platform) -> String {
    match platform {
        Platform::MacOs => label.to_string(),
        Platform::Windows | Platform::Linux => format!("{label} ({shortcut})"),
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
    /// A shortcut the menu only SHOWS, per platform, in that platform's frontend display
    /// spelling (`⌥⇧=` on macOS, `Alt+Shift+=` elsewhere, `+`, `F5`).
    ///
    /// The key carries no ⌘ / ⌃ / ⌥ (or, for Select same kind, stays free on purpose: `menu/DETAILS.md`
    /// § "Display-only accelerators"), or the bar must leave it to keydown dispatch on that
    /// platform (GTK takes the F-keys first; a window wearing the bar uses `Alt+↑`), so
    /// registering it would swallow the key before the page; the page's keydown handler owns it
    /// instead, and this is how the menu still tells the user about it. The platforms also
    /// differ in how it's DRAWN ([`display_accelerator_label`]). Set by [`displayed_item`] (no
    /// accelerator anywhere) and [`Entry::shown`], which refuses a platform that both registers
    /// and shows, so a row never shows one key while firing on another.
    pub display_accelerator: DisplayShortcut,
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
            .filter(move |entry| entry.on.on(platform))
            .map(|entry| &entry.kind)
    }
}

/// Which platforms a row or a menu exists on.
pub type Platforms = PerPlatform<bool>;

/// On every platform.
pub const EVERY_PLATFORM: Platforms = PerPlatform {
    macos: true,
    windows: true,
    linux: true,
};

/// On macOS alone.
pub const MACOS_ONLY: Platforms = PerPlatform {
    macos: true,
    windows: false,
    linux: false,
};

/// On Windows and Linux, which have no app menu.
pub const WINDOWS_AND_LINUX: Platforms = PerPlatform {
    macos: false,
    windows: true,
    linux: true,
};

/// One row of a submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The platforms this row exists on.
    pub on: Platforms,
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
    /// The platforms this menu exists on.
    pub on: Platforms,
    pub submenu: SubmenuSpec,
}

impl BarMenu {
    pub fn is_on(&self, platform: Platform) -> bool {
        self.on.on(platform)
    }
}

/// A top-level menu on every platform. Its ID reaches macOS alone: nothing on Windows or Linux
/// looks a menu up, so they build it without one.
pub const fn menu(id: &'static str, title: &'static str, entries: &'static [Entry]) -> BarMenu {
    BarMenu {
        on: EVERY_PLATFORM,
        submenu: top_level(id, Label::Key(title), entries),
    }
}

/// A top-level menu only macOS has.
pub const fn macos_menu(id: &'static str, title: Label, entries: &'static [Entry]) -> BarMenu {
    BarMenu {
        on: MACOS_ONLY,
        submenu: top_level(id, title, entries),
    }
}

const fn top_level(id: &'static str, title: Label, entries: &'static [Entry]) -> SubmenuSpec {
    SubmenuSpec {
        id: PerPlatform {
            macos: Some(id),
            windows: None,
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
    on_every_platform(EntryKind::Item(ItemSpec {
        id,
        label,
        accelerator,
        display_accelerator: NO_DISPLAY,
        enabled: true,
        tracking: Tracking::Tracked,
    }))
}

/// No shown shortcut on any platform.
const NO_DISPLAY: DisplayShortcut = NONE;

/// A command item whose shortcut the menu can only SHOW, [`Tracking::Tracked`].
///
/// `shortcut` is each platform's frontend display spelling (`+`, `⌥⇧=` / `Alt+Shift+=`). ❗ It
/// is never registered: the accelerator is [`NONE`] on every platform, by construction, because
/// a combo with no ⌘ / ⌃ / ⌥ would fire app-wide and eat that character in every text field.
/// The file pane's keydown handler is what actually runs the command.
pub const fn displayed_item(id: &'static str, key: &'static str, shortcut: DisplayShortcut) -> Entry {
    on_every_platform(EntryKind::Item(ItemSpec {
        id,
        label: Label::Key(key),
        accelerator: NONE,
        display_accelerator: shortcut,
        enabled: true,
        tracking: Tracking::Tracked,
    }))
}

pub const fn check(id: &'static str, key: &'static str, accelerator: Accelerator, role: CheckRole) -> Entry {
    on_every_platform(EntryKind::Check(CheckSpec {
        id,
        label: Label::Key(key),
        accelerator,
        role,
        tracked: false,
    }))
}

pub const SEPARATOR: Entry = on_every_platform(EntryKind::Separator);

pub const fn predefined(kind: Predefined, key: &'static str) -> Entry {
    on_every_platform(EntryKind::Predefined(kind, Label::Key(key)))
}

/// A submenu nested in another, built with the same `id` on every platform.
pub const fn submenu(
    id: Option<&'static str>,
    key: &'static str,
    role: SubmenuRole,
    entries: &'static [Entry],
) -> Entry {
    on_every_platform(EntryKind::Submenu(SubmenuSpec {
        id: PerPlatform {
            macos: id,
            windows: id,
            linux: id,
        },
        title: Label::Key(key),
        role,
        entries,
    }))
}

/// This row on macOS alone.
pub const fn macos_only(entry: Entry) -> Entry {
    Entry {
        on: MACOS_ONLY,
        kind: entry.kind,
    }
}

/// This row on Windows and Linux alone: where macOS keeps it in the app menu, they keep it under
/// Edit or Help.
pub const fn windows_and_linux(entry: Entry) -> Entry {
    Entry {
        on: WINDOWS_AND_LINUX,
        kind: entry.kind,
    }
}

const fn on_every_platform(kind: EntryKind) -> Entry {
    Entry {
        on: EVERY_PLATFORM,
        kind,
    }
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
                    on: self.on,
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

    /// This item, also SHOWING `shortcut` where it registers nothing: the platforms whose bar
    /// leaves the key to keydown dispatch ([`ItemSpec::display_accelerator`]). ❗ Refuses, at
    /// compile time, a platform that both registers an accelerator and shows a shortcut.
    pub const fn shown(self, shortcut: DisplayShortcut) -> Self {
        match self.kind {
            EntryKind::Item(mut item) => {
                assert!(
                    !(item.accelerator.macos.is_some() && shortcut.macos.is_some())
                        && !(item.accelerator.windows.is_some() && shortcut.windows.is_some())
                        && !(item.accelerator.linux.is_some() && shortcut.linux.is_some()),
                    "a row either registers its key or shows it, per platform, never both"
                );
                item.display_accelerator = shortcut;
                Self {
                    on: self.on,
                    kind: EntryKind::Item(item),
                }
            }
            _ => panic!("only a command item shows a shortcut"),
        }
    }

    /// This item, built disabled.
    pub const fn disabled(self) -> Self {
        match self.kind {
            EntryKind::Item(mut item) => {
                item.enabled = false;
                Self {
                    on: self.on,
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
                    on: self.on,
                    kind: EntryKind::Item(item),
                }
            }
            _ => panic!("only a command item is tracked"),
        }
    }
}

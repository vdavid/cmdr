//! The menu bar's host in the browser: the desktop's menu-state commands, answered by storing what
//! they push, and the macOS bar drawn from `cmdr-menu` over that state.
//!
//! The desktop's `commands/menu_state.rs` (and `settings.rs`'s `update_menu_accelerator`) store
//! each push in `MenuState` and recompute the native items; [`DemoMenu`] stores the same pushes,
//! by the same command names and camelCase args, and answers [`DemoMenu::bar`] (what the page
//! draws) and [`DemoMenu::click`] (what a click sends) from the crate's own data and fold. So the
//! page's menu bar says what the native one would, with no menu logic in TS.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_menu::accelerators::{glyphs, to_accelerator};
use cmdr_menu::bar::{APP_MENU_TITLE, menus_on, spec_accelerator};
use cmdr_menu::click::{BarClick, bar_click};
use cmdr_menu::enabled::{MenuItemInputs, entry_enabled};
use cmdr_menu::ids::{
    PIN_TAB_MENU_ID, SELECT_SAME_KIND_ID, UNDO_TRASH_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_FULL_LEFT_ID,
    command_id_to_menu_id,
};
use cmdr_menu::labels::{LabelText, SameKindTarget, TrashUndoTarget, pin_tab_label, same_kind_label, undo_trash_label};
use cmdr_menu::sort::RequestedSort;
use cmdr_menu::spec::{CheckRole, EntryKind, Label, Pane, Platform, SubmenuSpec, ViewMode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The bar the demo draws: macOS's, since the demo plays a Mac.
const PLATFORM: Platform = Platform::MacOs;

/// Every command [`DemoMenu::invoke`] answers, by its Tauri name.
pub const MENU_COMMANDS: &[&str] = &[
    "activate_window_menu",
    "set_file_operations_blocked",
    "set_open_terminal_here_enabled",
    "set_reopen_closed_tab_enabled",
    "set_commands_refused_over_dialog",
    "update_menu_accelerator",
    "update_sort_menu",
    "update_view_mode_menu",
    "sync_menu_show_hidden",
    "update_pin_tab_menu",
    "update_select_same_kind_menu",
    "update_undo_trash_menu",
];

/// What the desktop's `MenuState` holds, minus the muda handles: the pushes, as they arrived.
struct MenuModel {
    explorer_menu_active: bool,
    file_operations_blocked: bool,
    open_terminal_here_enabled: bool,
    reopen_closed_tab_enabled: bool,
    refused: HashSet<String>,
    requested_sort: Option<RequestedSort>,
    show_hidden_files: bool,
    active_pane: Pane,
    view_mode_left: ViewMode,
    view_mode_right: ViewMode,
    /// The view-mode pair's accelerators, which only the active pane's two items carry.
    view_mode_full_accel: Option<String>,
    view_mode_brief_accel: Option<String>,
    /// A rebind, by menu id: what the bar registers, or what it only shows (one or the other).
    rebinds: HashMap<String, Rebind>,
    is_pinned: bool,
    same_kind: Option<SameKindTarget>,
    undo_trash: Option<TrashUndoTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Rebind {
    Registered(String),
    Displayed(String),
    Unbound,
}

impl MenuModel {
    /// The desktop's `MenuState::new`: the explorer owns the menu, nothing is refused, no trash to
    /// undo, no closed tab to reopen, both panes on Brief.
    fn new() -> Self {
        Self {
            explorer_menu_active: true,
            file_operations_blocked: false,
            open_terminal_here_enabled: true,
            reopen_closed_tab_enabled: false,
            refused: HashSet::new(),
            requested_sort: None,
            show_hidden_files: false,
            active_pane: Pane::Left,
            view_mode_left: ViewMode::Brief,
            view_mode_right: ViewMode::Brief,
            view_mode_full_accel: spec_accelerator(VIEW_MODE_FULL_LEFT_ID, PLATFORM).map(String::from),
            view_mode_brief_accel: spec_accelerator(VIEW_MODE_BRIEF_LEFT_ID, PLATFORM).map(String::from),
            rebinds: HashMap::new(),
            is_pinned: false,
            same_kind: None,
            undo_trash: None,
        }
    }

    fn inputs(&self) -> MenuItemInputs<'_> {
        MenuItemInputs {
            explorer_menu_active: self.explorer_menu_active,
            file_operations_blocked: self.file_operations_blocked,
            open_terminal_here_enabled: self.open_terminal_here_enabled,
            reopen_closed_tab_enabled: self.reopen_closed_tab_enabled,
            undo_trash_available: self.undo_trash.is_some(),
            refused: &self.refused,
        }
    }

    fn view_mode(&self, pane: Pane) -> ViewMode {
        match pane {
            Pane::Left => self.view_mode_left,
            Pane::Right => self.view_mode_right,
        }
    }
}

/// One row of the drawn bar.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Row {
    /// A command item.
    #[serde(rename_all = "camelCase")]
    Item {
        /// The menu id [`DemoMenu::click`] takes.
        id: String,
        /// What it says.
        label: RowLabel,
        /// The shortcut a Mac draws beside it, in glyphs, if any.
        shortcut: Option<String>,
        /// Whether a click does something.
        enabled: bool,
    },
    /// A check item.
    #[serde(rename_all = "camelCase")]
    Check {
        /// The menu id [`DemoMenu::click`] takes.
        id: String,
        /// What it says.
        label: RowLabel,
        /// The shortcut a Mac draws beside it, in glyphs, if any.
        shortcut: Option<String>,
        /// Whether a click does something.
        enabled: bool,
        /// Whether it carries the checkmark.
        checked: bool,
    },
    /// A nested submenu.
    Submenu {
        /// What it says.
        label: RowLabel,
        /// Whether it opens.
        enabled: bool,
        /// Its rows.
        rows: Vec<Row>,
    },
    /// An item the OS wires (Hide, Quit, Services, Undo, Minimize): a browser has no OS to ask.
    System {
        /// What it says.
        label: RowLabel,
    },
    /// A separator.
    Separator,
}

/// A row's words: a catalog key and its tokens, or a literal that is never translated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum RowLabel {
    /// A `menu.*` key in the frontend's catalog, with its `{token}` values.
    Key {
        /// The catalog key.
        key: String,
        /// Token name → value.
        args: HashMap<String, String>,
    },
    /// Words that stay as they are (the app menu's `Cmdr`).
    Text {
        /// The words.
        text: String,
    },
}

impl From<LabelText> for RowLabel {
    fn from(label: LabelText) -> Self {
        Self::Key {
            key: label.key.to_string(),
            args: label
                .args
                .into_iter()
                .map(|(token, value)| (token.to_string(), value))
                .collect(),
        }
    }
}

fn key(key: &str) -> RowLabel {
    RowLabel::Key {
        key: key.to_string(),
        args: HashMap::new(),
    }
}

/// One top-level menu of the drawn bar.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BarMenuView {
    /// Its menu id (`menu_file`, …).
    pub id: String,
    /// What the bar says.
    pub title: RowLabel,
    /// Its rows.
    pub rows: Vec<Row>,
}

/// What a click asks the page to do.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Click {
    /// Send `event` with `payload` to the main window, as `handle_menu_event`'s `emit_to("main")`.
    Emit {
        /// The Tauri event name.
        event: String,
        /// Its payload, as `bindings.ts` types it.
        payload: Value,
    },
    /// Hand a native edit selector to whatever has focus outside the main window.
    NativeEdit {
        /// `cut`, `copy`, `paste`, or `selectAll`.
        action: String,
    },
    /// Close the focused window (Close tab while another window is in front).
    CloseFocusedWindow,
    /// Nothing: a disabled row, an OS item, or an id the bar doesn't build.
    Nothing,
}

/// The page's menu bar state.
pub struct DemoMenu {
    model: Mutex<MenuModel>,
}

impl Default for DemoMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoMenu {
    /// A bar as a fresh app builds it.
    pub fn new() -> Self {
        Self {
            model: Mutex::new(MenuModel::new()),
        }
    }

    /// Whether [`DemoMenu::invoke`] answers `command`.
    pub fn handles(command: &str) -> bool {
        MENU_COMMANDS.contains(&command)
    }

    /// Stores one push, Tauri-style: `Ok` is the command's answer (all `null`), `Err` what the
    /// promise rejects with.
    pub fn invoke(&self, command: &str, args: Value) -> Result<Value, Value> {
        let mut m = self.model.lock_ignore_poison();
        match command {
            "activate_window_menu" => {
                let a: KindArgs = parse(command, args)?;
                match a.kind.as_str() {
                    "main" => m.explorer_menu_active = true,
                    "other" => m.explorer_menu_active = false,
                    // The viewer swaps in a bar of its own on desktop; the demo has no viewer yet.
                    "viewer" => {}
                    other => return Err(Value::String(format!("Unknown window menu kind: {other}"))),
                }
            }
            "set_file_operations_blocked" => m.file_operations_blocked = parse::<BlockedArgs>(command, args)?.blocked,
            "set_open_terminal_here_enabled" => {
                m.open_terminal_here_enabled = parse::<EnabledArgs>(command, args)?.enabled;
            }
            "set_reopen_closed_tab_enabled" => {
                m.reopen_closed_tab_enabled = parse::<EnabledArgs>(command, args)?.enabled;
            }
            "set_commands_refused_over_dialog" => {
                m.refused = parse::<RefusedArgs>(command, args)?.command_ids.into_iter().collect();
            }
            "update_menu_accelerator" => {
                let a: AcceleratorArgs = parse(command, args)?;
                rebind(&mut m, &a.command_id, &a.shortcut);
            }
            "update_sort_menu" => m.requested_sort = parse::<SortArgs>(command, args)?.sort,
            "update_view_mode_menu" => {
                let a: ViewModeArgs = parse(command, args)?;
                m.active_pane = match a.active_pane.as_str() {
                    "left" => Pane::Left,
                    "right" => Pane::Right,
                    other => return Err(Value::String(format!("Invalid active_pane: {other}"))),
                };
                let mode = |word: &str| {
                    ViewMode::from_wire(word).ok_or_else(|| Value::String(format!("Invalid view mode: {word}")))
                };
                m.view_mode_left = mode(&a.left_mode)?;
                m.view_mode_right = mode(&a.right_mode)?;
            }
            "sync_menu_show_hidden" => m.show_hidden_files = parse::<CheckedArgs>(command, args)?.checked,
            "update_pin_tab_menu" => m.is_pinned = parse::<PinnedArgs>(command, args)?.is_pinned,
            "update_select_same_kind_menu" => m.same_kind = parse::<TargetArgs<SameKindTarget>>(command, args)?.target,
            "update_undo_trash_menu" => m.undo_trash = parse::<TargetArgs<TrashUndoTarget>>(command, args)?.target,
            other => return Err(Value::String(format!("the demo's menu doesn't answer `{other}`"))),
        }
        Ok(Value::Null)
    }

    /// The macOS bar as it stands: every menu and row, with its words, shortcut, and state.
    pub fn bar(&self) -> Vec<BarMenuView> {
        let m = self.model.lock_ignore_poison();
        menus_on(PLATFORM)
            .map(|bar_menu| {
                let spec = &bar_menu.submenu;
                BarMenuView {
                    id: spec.id.on(PLATFORM).unwrap_or_default().to_string(),
                    title: label(spec.title),
                    rows: rows(spec, &m),
                }
            })
            .collect()
    }

    /// What a click on `menu_id` asks for, applying what the click itself changes (a check item's
    /// own state), as `handle_menu_event` does.
    pub fn click(&self, menu_id: &str) -> Click {
        let mut m = self.model.lock_ignore_poison();
        let enabled = cmdr_menu::bar::spec_entry(menu_id, PLATFORM)
            .and_then(|entry| entry_enabled(entry, &m.inputs()))
            .unwrap_or(false);
        let Some(click) = bar_click(menu_id, PLATFORM).filter(|_| enabled) else {
            return Click::Nothing;
        };
        let emit = |event: &str, payload: Value| Click::Emit {
            event: event.to_string(),
            payload,
        };
        match click {
            BarClick::Command { command_id, .. } => emit("execute-command", json!({ "commandId": command_id })),
            BarClick::ShowHiddenFiles => {
                m.show_hidden_files = !m.show_hidden_files;
                emit("settings-changed", json!({ "showHiddenFiles": m.show_hidden_files }))
            }
            BarClick::ViewMode { pane, mode } => {
                match pane {
                    Pane::Left => m.view_mode_left = mode,
                    Pane::Right => m.view_mode_right = mode,
                }
                emit("view-mode-changed", json!({ "mode": mode.wire(), "pane": pane.wire() }))
            }
            // The checks stay on the request until the frontend's sort lands and re-pushes.
            BarClick::Sort(check) => {
                let (action, value) = check.wire();
                emit("menu-sort", json!({ "action": action, "value": value }))
            }
            BarClick::Edit { command_id } if m.explorer_menu_active => {
                emit("execute-command", json!({ "commandId": command_id }))
            }
            BarClick::Edit { command_id } => Click::NativeEdit {
                action: native_action(command_id).to_string(),
            },
            BarClick::CloseTab if m.explorer_menu_active => {
                emit("execute-command", json!({ "commandId": "tab.close" }))
            }
            BarClick::CloseTab => Click::CloseFocusedWindow,
        }
    }
}

/// `update_menu_accelerator`'s rule (`commands/settings.rs`): a combo the bar may register
/// becomes its accelerator, anything else (no ⌘ / ⌃ / ⌥, a key muda can't name) is shown only,
/// and the view-mode pair's goes to whichever pane is active.
fn rebind(m: &mut MenuModel, command_id: &str, shortcut: &str) {
    let registered = to_accelerator(shortcut, PLATFORM);
    match command_id {
        "view.fullMode" => m.view_mode_full_accel = registered,
        "view.briefMode" => m.view_mode_brief_accel = registered,
        _ => {
            let Some(menu_id) = command_id_to_menu_id(command_id) else {
                return;
            };
            let rebind = match (registered, shortcut.is_empty()) {
                (Some(accelerator), _) => Rebind::Registered(accelerator),
                (None, false) => Rebind::Displayed(shortcut.to_string()),
                (None, true) => Rebind::Unbound,
            };
            m.rebinds.insert(menu_id.to_string(), rebind);
        }
    }
}

fn native_action(command_id: &str) -> &'static str {
    match command_id {
        "edit.cut" => "cut",
        "edit.copy" => "copy",
        "edit.paste" => "paste",
        _ => "selectAll",
    }
}

fn rows(spec: &'static SubmenuSpec, m: &MenuModel) -> Vec<Row> {
    let inputs = m.inputs();
    spec.entries_on(PLATFORM)
        .map(|entry| {
            let enabled = entry_enabled(entry, &inputs).unwrap_or(false);
            match entry {
                EntryKind::Item(item) => Row::Item {
                    id: item.id.to_string(),
                    label: item_label(item.id, item.label, m),
                    shortcut: shortcut(item.id, item.accelerator.on(PLATFORM), item.display_accelerator, m),
                    enabled,
                },
                EntryKind::Check(check) => {
                    let (checked, accelerator) = match check.role {
                        CheckRole::ShowHiddenFiles => (m.show_hidden_files, check.accelerator.on(PLATFORM)),
                        CheckRole::ViewMode(pane, mode) => {
                            // The pair's accelerator follows the active pane (`rebuild_view_mode_items`).
                            let accelerator = (pane == m.active_pane)
                                .then_some(match mode {
                                    ViewMode::Full => m.view_mode_full_accel.as_deref(),
                                    ViewMode::Brief => m.view_mode_brief_accel.as_deref(),
                                })
                                .flatten();
                            (m.view_mode(pane) == mode, accelerator)
                        }
                        CheckRole::Sort(role) => (role.is_on(m.requested_sort), check.accelerator.on(PLATFORM)),
                    };
                    let shortcut = match check.role {
                        CheckRole::ViewMode(..) => accelerator.map(glyphs),
                        _ => shortcut(check.id, accelerator, None, m),
                    };
                    Row::Check {
                        id: check.id.to_string(),
                        label: label(check.label),
                        shortcut,
                        enabled,
                        checked,
                    }
                }
                EntryKind::Submenu(nested) => Row::Submenu {
                    label: label(nested.title),
                    enabled,
                    rows: rows(nested, m),
                },
                EntryKind::Predefined(_, text) => Row::System { label: label(*text) },
                EntryKind::Separator => Row::Separator,
            }
        })
        .collect()
}

/// A row's shortcut: a rebind first (as `MenuState.display_accelerators` overrides the spec),
/// then what `MENU_BAR` registers, then what it only shows.
fn shortcut(id: &str, registered: Option<&str>, displayed: Option<&str>, m: &MenuModel) -> Option<String> {
    match m.rebinds.get(id) {
        Some(Rebind::Registered(accelerator)) => Some(glyphs(accelerator)),
        Some(Rebind::Displayed(glyph)) => Some(glyph.clone()),
        Some(Rebind::Unbound) => None,
        None => registered.map(glyphs).or_else(|| displayed.map(str::to_string)),
    }
}

/// An item's words: the three live labels from their pushes, every other one from the spec.
fn item_label(id: &str, spec: Label, m: &MenuModel) -> RowLabel {
    match id {
        PIN_TAB_MENU_ID => pin_tab_label(m.is_pinned).into(),
        SELECT_SAME_KIND_ID => same_kind_label(m.same_kind.as_ref()).into(),
        UNDO_TRASH_ID => undo_trash_label(m.undo_trash.as_ref()).into(),
        _ => label(spec),
    }
}

fn label(spec: Label) -> RowLabel {
    // The demo plays a fresh personal-use install: no license key yet.
    match spec.catalog_key(PLATFORM, false) {
        Some(catalog_key) => key(catalog_key),
        None => RowLabel::Text {
            text: APP_MENU_TITLE.to_string(),
        },
    }
}

fn parse<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, Value> {
    serde_json::from_value(args).map_err(|e| Value::String(format!("invalid args for command {command}: {e}")))
}

#[derive(Deserialize)]
struct KindArgs {
    kind: String,
}

#[derive(Deserialize)]
struct BlockedArgs {
    blocked: bool,
}

#[derive(Deserialize)]
struct EnabledArgs {
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefusedArgs {
    command_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceleratorArgs {
    command_id: String,
    shortcut: String,
}

#[derive(Deserialize)]
struct SortArgs {
    sort: Option<RequestedSort>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewModeArgs {
    active_pane: String,
    left_mode: String,
    right_mode: String,
}

#[derive(Deserialize)]
struct CheckedArgs {
    checked: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PinnedArgs {
    is_pinned: bool,
}

#[derive(Deserialize)]
struct TargetArgs<T> {
    target: Option<T>,
}

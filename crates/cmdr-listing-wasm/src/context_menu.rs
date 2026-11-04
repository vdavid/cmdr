//! The right-click menus' host in the browser: the desktop's `show_*_context_menu` commands,
//! answered with the rows the page draws, and a pick answered with what the native item sends.
//!
//! The desktop's `commands/menu.rs` stashes what a popup was opened over in `MenuState` and builds
//! the native menu from `cmdr_menu::context`'s rows; [`DemoMenu::popup`] stashes the same facts,
//! from the same command names and camelCase args, and answers the same rows for the page to draw.
//! [`DemoMenu::context_click`] then reads a pick through the crate's `context_click`, as
//! `handle_menu_event` does, so a pick sends the event the native item would, to the window it
//! would. No menu logic in TS.
//!
//! What the demo has no OS for: a [`cmdr_menu::context::Slot`] (Open with, Share, the provider's
//! actions, the tag colors, Services) is left out, the image-search group stays hidden (no media
//! index), a breadcrumb volume is never busy and always says Eject (the demo's volumes are local
//! disks), and a servers hub host has nothing mounted.

use std::collections::HashSet;

use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_listing::SortColumn;
use cmdr_menu::accelerators::glyphs;
use cmdr_menu::context::click::{ContextClick, context_click};
use cmdr_menu::context::file::{FileMenuFacts, PaneFacts, TargetFacts, file_menu};
use cmdr_menu::context::image_index::ImageIndexMenuState;
use cmdr_menu::context::popups::{
    BreadcrumbVolume, DetachWord, NetworkHostFacts, PaneSide, QueueRowMenuEntry, breadcrumb_menu, date_sort_menu,
    function_key_bar_menu, network_host_menu, parent_row_menu, queue_row_menu, tab_menu, viewer_text_menu,
};
use cmdr_menu::context::{ContextLabel, ContextMenuShortcuts, ContextRow};
use cmdr_menu::labels::SameKindTarget;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::menu::{Click, DemoMenu, PLATFORM, Row, RowLabel, parse};

/// Every popup command [`DemoMenu::popup`] answers, by its Tauri name.
pub const POPUP_COMMANDS: &[&str] = &[
    "show_file_context_menu",
    "show_breadcrumb_context_menu",
    "show_parent_row_context_menu",
    "show_tab_context_menu",
    "show_date_sort_menu",
    "show_function_key_bar_context_menu",
    "show_viewer_context_menu",
    "show_queue_row_context_menu",
    "show_network_host_context_menu",
];

/// What the desktop's `MenuState` stashes for a pick: the right-clicked row's path, the eject
/// target, the host row, plus the one fact the demo needs and a native menu doesn't (the window
/// the popup went up over, where a viewer's pick goes back to).
#[derive(Default)]
pub(crate) struct ContextModel {
    /// The primary right-clicked path (`MenuContext.path`), or `..`'s parent.
    path: String,
    volume_id: String,
    volume_name: String,
    host_row_id: String,
    host_id: String,
    host_name: String,
    /// The window label the last popup went up over.
    window: String,
    /// The ids the last popup offered enabled: a pick of anything else does nothing, as a greyed
    /// native row can't be picked.
    live: HashSet<String>,
}

/// A popup as the page draws it: its rows, and where it opens when the caller named a point.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Popup {
    /// Its rows, top to bottom.
    pub rows: Vec<Row>,
    /// In the calling frame's CSS pixels; `None` opens it at the pointer.
    pub anchor: Option<Anchor>,
}

/// `MenuAnchor`'s wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// From the frame's left edge.
    pub x: f64,
    /// From the frame's top edge.
    pub y: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MenuContextArgs {
    path: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TargetArgs {
    count_text: Option<String>,
    size_text: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileArgs {
    path: String,
    filename: String,
    is_directory: bool,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    pane: PaneFacts,
    #[serde(default)]
    target: TargetArgs,
    #[serde(default)]
    shortcuts: ContextMenuShortcuts,
    anchor: Option<Anchor>,
    same_kind: Option<SameKindTarget>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BreadcrumbArgs {
    #[serde(default)]
    shortcuts: ContextMenuShortcuts,
    eject_volume_id: Option<String>,
    eject_volume_name: Option<String>,
    #[serde(default)]
    offer_eject_all: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParentRowArgs {
    parent_path: String,
    anchor: Option<Anchor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabArgs {
    is_pinned: bool,
    can_close: bool,
    has_other_unpinned_tabs: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DateSortArgs {
    pane: PaneSide,
    requested: Option<SortColumn>,
    anchor: Option<Anchor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewerArgs {
    has_selection: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueRowArgs {
    entries: Vec<QueueRowMenuEntry>,
    #[serde(default)]
    shortcuts: ContextMenuShortcuts,
}

#[derive(Deserialize)]
struct HostArgs {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkHostArgs {
    row_id: String,
    host: HostArgs,
    is_manual: bool,
    is_saved: bool,
    has_credentials: bool,
    anchor: Option<Anchor>,
}

impl DemoMenu {
    /// Whether [`DemoMenu::popup`] answers `command`.
    pub fn opens_popup(command: &str) -> bool {
        POPUP_COMMANDS.contains(&command)
    }

    /// Stores `update_menu_context`'s push, as `MenuState.context` does.
    pub(crate) fn note_menu_context(&self, args: Value) -> Result<Value, Value> {
        let a: MenuContextArgs = parse("update_menu_context", args)?;
        self.context.lock_ignore_poison().path = a.path;
        Ok(Value::Null)
    }

    /// Answers one `show_*` command, opened over the window labeled `window`: stashes what a pick
    /// needs and answers the rows to draw. `Err` is what the promise rejects with.
    pub fn popup(&self, command: &str, args: Value, window: &str) -> Result<Popup, Value> {
        let mut c = self.context.lock_ignore_poison();
        let shortcutless = ContextMenuShortcuts::default();
        let (rows, shortcuts, anchor) = match command {
            "show_file_context_menu" => {
                let a: FileArgs = parse(command, args)?;
                let count = if a.paths.is_empty() { 1 } else { a.paths.len() };
                let facts = FileMenuFacts {
                    filename: &a.filename,
                    is_directory: a.is_directory,
                    pane: a.pane,
                    image_index: ImageIndexMenuState::default(),
                    target: TargetFacts {
                        count,
                        count_text: a.target.count_text.as_deref(),
                        size_text: a.target.size_text.as_deref(),
                    },
                    same_kind: a.same_kind.as_ref(),
                };
                let rows = file_menu(&facts, PLATFORM);
                c.path = a.path;
                (rows, a.shortcuts, a.anchor)
            }
            "show_breadcrumb_context_menu" => {
                let a: BreadcrumbArgs = parse(command, args)?;
                let volume = match (&a.eject_volume_id, &a.eject_volume_name) {
                    (Some(_), Some(name)) => Some(BreadcrumbVolume {
                        name,
                        busy: false,
                        word: DetachWord::Eject,
                        offer_eject_all: a.offer_eject_all,
                    }),
                    _ => None,
                };
                let rows = breadcrumb_menu(volume);
                (c.volume_id, c.volume_name) = match (a.eject_volume_id, a.eject_volume_name) {
                    (Some(id), Some(name)) => (id, name),
                    _ => (String::new(), String::new()),
                };
                (rows, a.shortcuts, None)
            }
            "show_parent_row_context_menu" => {
                let a: ParentRowArgs = parse(command, args)?;
                c.path = a.parent_path;
                (parent_row_menu(), shortcutless, a.anchor)
            }
            "show_tab_context_menu" => {
                let a: TabArgs = parse(command, args)?;
                (
                    tab_menu(a.is_pinned, a.can_close, a.has_other_unpinned_tabs),
                    shortcutless,
                    None,
                )
            }
            "show_date_sort_menu" => {
                let a: DateSortArgs = parse(command, args)?;
                (date_sort_menu(a.pane, a.requested), shortcutless, a.anchor)
            }
            "show_function_key_bar_context_menu" => (function_key_bar_menu(), shortcutless, None),
            "show_viewer_context_menu" => {
                let a: ViewerArgs = parse(command, args)?;
                (viewer_text_menu(a.has_selection, PLATFORM), shortcutless, None)
            }
            "show_queue_row_context_menu" => {
                let a: QueueRowArgs = parse(command, args)?;
                (queue_row_menu(&a.entries), a.shortcuts, None)
            }
            "show_network_host_context_menu" => {
                let a: NetworkHostArgs = parse(command, args)?;
                let rows = network_host_menu(NetworkHostFacts {
                    is_manual: a.is_manual,
                    is_saved: a.is_saved,
                    has_credentials: a.has_credentials,
                    has_mounts: false,
                });
                c.host_row_id = a.row_id;
                c.host_id = a.host.id;
                c.host_name = a.host.name;
                (rows, shortcutless, a.anchor)
            }
            other => return Err(Value::String(format!("the demo's menus don't open `{other}`"))),
        };
        c.window = window.to_string();
        c.live.clear();
        let rows = drawn(&rows, &shortcuts, &mut c.live);
        Ok(Popup { rows, anchor })
    }

    /// What a pick of `menu_id` from the last popup asks the page to do, as `handle_menu_event`'s
    /// context lanes. A row the popup didn't offer enabled does nothing.
    pub fn context_click(&self, menu_id: &str) -> Click {
        let c = self.context.lock_ignore_poison();
        if !c.live.contains(menu_id) {
            return Click::Nothing;
        }
        let Some(click) = context_click(menu_id) else {
            return Click::Nothing;
        };
        let to_main = |event: &str, payload: Value| Click::Emit {
            event: event.to_string(),
            payload,
            to: "main".to_string(),
        };
        match click {
            ContextClick::Command { command_id, .. } => to_main("execute-command", json!({ "commandId": command_id })),
            ContextClick::ShowSearchResultInFolder if c.path.is_empty() => Click::Nothing,
            ContextClick::ShowSearchResultInFolder => {
                to_main("show-search-result-in-folder", json!({ "path": c.path }))
            }
            ContextClick::AddFavorite if c.path.is_empty() => Click::Nothing,
            ContextClick::AddFavorite => Click::Invoke {
                command: "add_favorite".to_string(),
                args: json!({ "path": c.path, "name": null }),
            },
            ContextClick::MediaIndexExclusion { excluded } => to_main(
                "media-index-folder-exclusion",
                json!({ "folder": c.path, "excluded": excluded }),
            ),
            ContextClick::MediaIndexChoice { chosen } => to_main(
                "media-index-folder-choice",
                json!({ "folder": c.path, "chosen": chosen }),
            ),
            ContextClick::Tab(action) => to_main("tab-context-action", json!({ "action": action })),
            ContextClick::HideFunctionKeyBar => to_main("function-key-bar-hide-requested", Value::Null),
            ContextClick::Volume(_) if c.volume_id.is_empty() => Click::Nothing,
            ContextClick::Volume(action) => to_main(
                "volume-context-action",
                json!({ "action": action.wire(), "volumeId": c.volume_id, "volumeName": c.volume_name }),
            ),
            ContextClick::NetworkHost(action) => to_main(
                "network-host-context-action",
                json!({
                    "action": action.wire(),
                    "rowId": c.host_row_id,
                    "hostId": c.host_id,
                    "hostName": c.host_name,
                }),
            ),
            ContextClick::ViewerText(action) => Click::Emit {
                event: "viewer-context-menu-action".to_string(),
                payload: json!({ "action": action }),
                to: c.window.clone(),
            },
            ContextClick::QueueRow(action) => Click::Emit {
                event: "queue-row-context-action".to_string(),
                payload: json!({ "action": action }),
                to: "queue".to_string(),
            },
            ContextClick::DateSort { pane, column } => {
                to_main("date-sort-menu-pick", json!({ "pane": pane, "column": column }))
            }
        }
    }
}

/// The crate's rows as the page draws them: words as catalog keys, shortcuts in a Mac's glyphs,
/// slots left out. Every enabled id goes into `live`.
fn drawn(rows: &[ContextRow], shortcuts: &ContextMenuShortcuts, live: &mut HashSet<String>) -> Vec<Row> {
    rows.iter()
        .filter_map(|row| {
            Some(match row {
                ContextRow::Item(item) => {
                    if item.enabled {
                        live.insert(item.id.clone());
                    }
                    Row::Item {
                        id: item.id.clone(),
                        label: label(&item.label),
                        shortcut: item.shortcut.resolve(shortcuts, PLATFORM).as_deref().map(glyphs),
                        enabled: item.enabled,
                    }
                }
                ContextRow::Check { item, checked } => {
                    if item.enabled {
                        live.insert(item.id.clone());
                    }
                    Row::Check {
                        id: item.id.clone(),
                        label: label(&item.label),
                        shortcut: item.shortcut.resolve(shortcuts, PLATFORM).as_deref().map(glyphs),
                        enabled: item.enabled,
                        checked: *checked,
                    }
                }
                ContextRow::Submenu { label: text, rows, .. } => Row::Submenu {
                    label: label(text),
                    enabled: true,
                    rows: drawn(rows, shortcuts, live),
                },
                ContextRow::Header(text) => Row::Header {
                    label: RowLabel::Text { text: text.clone() },
                },
                ContextRow::Separator => Row::Separator,
                // The OS fills these on desktop; a browser has no OS to ask.
                ContextRow::Slot(_) => return None,
            })
        })
        .collect()
}

fn label(label: &ContextLabel) -> RowLabel {
    match label {
        ContextLabel::Key(text) => text.clone().into(),
        ContextLabel::Text(text) => RowLabel::Text { text: text.clone() },
    }
}

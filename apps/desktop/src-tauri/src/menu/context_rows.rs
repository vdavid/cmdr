//! Every native context menu, built from `cmdr_menu::context`'s rows: the one door from the
//! crate's data to muda for a popup.
//!
//! [`build_popup`] serves every menu with no OS-filled group; the file context menu calls
//! [`append_rows`] with a slot filler, since its [`Slot`]s need the macOS machinery
//! (`file_context_menu.rs`). Every label resolves through `menu_t`, and every accelerator through
//! the user's live shortcut map (`ContextMenuShortcuts`), for `Platform::current()`.

use cmdr_menu::context::{ContextItem, ContextLabel, ContextMenuShortcuts, ContextRow, Slot};
use cmdr_menu::spec::Platform;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

use super::CONTEXT_MENU_TARGET_ID;
use super::menu_items::resolve;

/// Where a row lands: the popup itself, or one of its submenus. muda's two containers share
/// `append` but no trait for it.
pub(super) enum MenuParent<'a, R: Runtime> {
    Menu(&'a Menu<R>),
    Submenu(&'a Submenu<R>),
}

impl<R: Runtime> MenuParent<'_, R> {
    pub(super) fn append(&self, item: &dyn IsMenuItem<R>) -> tauri::Result<()> {
        match self {
            Self::Menu(menu) => menu.append(item),
            Self::Submenu(submenu) => submenu.append(item),
        }
    }
}

/// Whatever fills a [`Slot`] at its position. The file menu's is the macOS facts' builder; every
/// other menu has no slot.
pub(super) type SlotFiller<'f, R> = dyn FnMut(Slot, &MenuParent<'_, R>) -> tauri::Result<()> + 'f;

/// A row's words in the UI language.
fn label_text(label: &ContextLabel) -> String {
    match label {
        ContextLabel::Key(text) => resolve(text),
        ContextLabel::Text(text) => text.clone(),
    }
}

fn menu_item<R: Runtime>(
    app: &AppHandle<R>,
    item: &ContextItem,
    shortcuts: &ContextMenuShortcuts,
) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(
        app,
        item.id.as_str(),
        label_text(&item.label),
        item.enabled,
        item.shortcut.resolve(shortcuts, Platform::current()).as_deref(),
    )
}

/// Appends `rows` to `parent`, in order, handing each [`Slot`] to `fill_slot`.
pub(super) fn append_rows<R: Runtime>(
    app: &AppHandle<R>,
    parent: &MenuParent<'_, R>,
    rows: &[ContextRow],
    shortcuts: &ContextMenuShortcuts,
    fill_slot: &mut SlotFiller<'_, R>,
) -> tauri::Result<()> {
    for row in rows {
        match row {
            ContextRow::Item(item) => parent.append(&menu_item(app, item, shortcuts)?)?,
            ContextRow::Check { item, checked } => parent.append(&CheckMenuItem::with_id(
                app,
                item.id.as_str(),
                label_text(&item.label),
                item.enabled,
                *checked,
                item.shortcut.resolve(shortcuts, Platform::current()).as_deref(),
            )?)?,
            ContextRow::Submenu { id, label, rows } => {
                let submenu = match id {
                    Some(id) => Submenu::with_id(app, *id, label_text(label), true)?,
                    None => Submenu::new(app, label_text(label), true)?,
                };
                append_rows(app, &MenuParent::Submenu(&submenu), rows, shortcuts, fill_slot)?;
                parent.append(&submenu)?;
            }
            // Disabled, which is what makes it read as a label rather than a command; on macOS
            // `lend_context_menu_header` also restyles it as a header once the menu tracks.
            ContextRow::Header(text) => parent.append(&MenuItem::with_id(
                app,
                CONTEXT_MENU_TARGET_ID,
                text,
                false,
                None::<&str>,
            )?)?,
            ContextRow::Separator => parent.append(&PredefinedMenuItem::separator(app)?)?,
            ContextRow::Slot(slot) => fill_slot(*slot, parent)?,
        }
    }
    Ok(())
}

/// A popup with no OS-filled group, from its rows.
pub fn build_popup<R: Runtime>(
    app: &AppHandle<R>,
    rows: &[ContextRow],
    shortcuts: &ContextMenuShortcuts,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    append_rows(app, &MenuParent::Menu(&menu), rows, shortcuts, &mut |slot, _| {
        // Only the file menu lays out slots, and it fills them itself.
        log::warn!(target: "menu", "a {slot:?} slot in a popup with no filler; left out");
        Ok(())
    })?;
    Ok(menu)
}

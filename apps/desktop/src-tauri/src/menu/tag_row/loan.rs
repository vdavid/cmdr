//! Arms the tag row before `popup()`, and installs it when the menu starts tracking.
//!
//! Same boundary as `context_menu_icons.rs` and `context_menu_header.rs`: Tauri hands out no
//! `NSMenu` for a context menu, so the row lands on `NSMenuDidBeginTrackingNotification`,
//! which AppKit posts before it lays the menu out. And the same bargain: if the row never
//! lands, the seven plain items stay, and they're a correct menu on their own.

use std::cell::{Cell, RefCell};

use objc2::MainThreadMarker;
use objc2::rc::{Retained, Weak};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::NSNotification;
use tauri::Runtime;
use tauri::menu::Menu;

use super::super::TAG_COLOR_ID_PREFIX;
use super::super::macos_appkit::{clear_menu_item_image, menu_item_text, observe_menu_tracking, tracking_menu};
use super::model::{ROW_LABEL_KEY, SWATCH_COUNT, SWATCHES, find_tag_run, hover_label_key};
use super::view::{RowContent, SwatchContent, TagRowView};
use crate::intl::{menu_t, menu_t_with};

/// The tag row armed for the next context menu to open.
///
/// ❗ Hold it until `popup()` returns, exactly as `IconLoan` and `HeaderLoan` are held:
/// `popup()` runs AppKit's tracking loop, so the menu only starts tracking inside it. ❌ Never
/// `let _ =`. Its `Drop` disarms, and lets the installed row go of its menu items, so a late
/// accessibility press can't fire an item whose muda state is already gone.
pub struct TagRowLoan(MainThreadMarker);

impl Drop for TagRowLoan {
    fn drop(&mut self) {
        ARMED.with(|slot| slot.borrow_mut().take());
        if let Some(row) = INSTALLED
            .with(|slot| slot.borrow_mut().take())
            .and_then(|row| row.load())
        {
            row.disarm();
        }
    }
}

/// What the tracking observer needs: the seven titles AppKit will show, in row order, and
/// everything the row draws.
#[derive(Clone)]
struct ArmedRow {
    titles: Vec<String>,
    content: RowContent,
}

thread_local! {
    /// The row for the menu currently going up. Main-thread-only by construction: every
    /// reader runs from the menu thread.
    static ARMED: RefCell<Option<ArmedRow>> = const { RefCell::new(None) };
    /// The row installed on that menu, weakly: its menu item owns it.
    static INSTALLED: RefCell<Option<Weak<TagRowView>>> = const { RefCell::new(None) };
    /// Whether the tracking observer is registered. Lazily, so the cost lands with the first
    /// right-click rather than on every launch.
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}

/// Arms `menu`'s tag row. The menu must be about to `popup()`.
///
/// `applied_tag_colors` is `FileContextInfo.applied_tag_colors`, indexed by Finder color.
/// Answers `None` when there's nothing to do: off the main thread, or the menu carries no tag
/// items.
pub fn lend_tag_row<R: Runtime>(menu: &Menu<R>, applied_tag_colors: &[bool; 8]) -> Option<TagRowLoan> {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "menu", "Not on the main thread; the tag colors stay seven plain items");
        return None;
    };
    let armed = arm(menu, applied_tag_colors)?;
    ensure_observing(mtm);
    ARMED.with(|slot| *slot.borrow_mut() = Some(armed));
    Some(TagRowLoan(mtm))
}

/// Checks the installed row's circles once the tag reads answer after the menu went up
/// (`context_menu_live.rs`). `applied_tag_colors` is indexed by Finder color, like
/// [`lend_tag_row`]'s. Nothing when no row is installed.
pub fn set_applied(applied_tag_colors: &[bool; 8]) {
    let Some(row) = INSTALLED.with(|slot| slot.borrow().as_ref().and_then(Weak::load)) else {
        return;
    };
    let in_row_order: Vec<bool> = SWATCHES
        .iter()
        .map(|swatch| applied_tag_colors[usize::from(swatch.color)])
        .collect();
    row.set_applied(&in_row_order);
}

/// Reads each tag item's live title and resolves the row's words for it.
///
/// The title is the translated color name, and it's also what the observer matches on,
/// since AppKit has never heard of a Tauri menu ID.
fn arm<R: Runtime>(menu: &Menu<R>, applied_tag_colors: &[bool; 8]) -> Option<ArmedRow> {
    let mut titles = Vec::with_capacity(SWATCH_COUNT);
    let mut swatches = Vec::with_capacity(SWATCH_COUNT);
    for swatch in &SWATCHES {
        let id = format!("{TAG_COLOR_ID_PREFIX}{}", swatch.color);
        let title = menu_item_text(&menu.get(id.as_str())?)?;
        let applied = applied_tag_colors[usize::from(swatch.color)];
        swatches.push(SwatchContent {
            name: title.clone(),
            add_label: menu_t_with(hover_label_key(false), &[("color", &title)]),
            remove_label: menu_t_with(hover_label_key(true), &[("color", &title)]),
            applied,
            light: swatch.light,
            dark: swatch.dark,
        });
        titles.push(title);
    }
    Some(ArmedRow {
        titles,
        content: RowContent {
            swatches,
            idle_label: menu_t(ROW_LABEL_KEY),
        },
    })
}

/// Registers the tracking observer, once per process.
fn ensure_observing(mtm: MainThreadMarker) {
    if OBSERVING.replace(true) {
        return;
    }
    observe_menu_tracking(mtm, "put the tag colors on one row", |mtm, note| {
        on_menu_did_begin_tracking(mtm, note);
    });
}

/// Installs the armed row on the tracking menu, if this menu carries the seven items.
///
/// Fires for every menu the app tracks, the menu bar included, so it does nothing unless a
/// row is armed and the menu holds the whole run.
fn on_menu_did_begin_tracking(mtm: MainThreadMarker, notification: &NSNotification) {
    let Some(armed) = ARMED.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let Some(menu) = tracking_menu(mtm, notification) else {
        return;
    };
    install(mtm, &menu, armed);
}

/// Puts the row on the run's first item and hides the other six.
///
/// ❌ Not `find_ns_item` per title: the header line carries the bare filename, so a folder
/// named `Red` would put the row on the header. `find_tag_run` takes the whole run or nothing.
fn install(mtm: MainThreadMarker, menu: &NSMenu, armed: ArmedRow) {
    let items: Vec<Retained<NSMenuItem>> = (0..menu.numberOfItems())
        .filter_map(|index| menu.itemAtIndex(index))
        .collect();
    let titles: Vec<String> = items.iter().map(|item| item.title().to_string()).collect();
    let Some(start) = find_tag_run(&titles, &armed.titles) else {
        return;
    };
    let run = start..start + SWATCH_COUNT;
    if items[start].view().is_some() {
        return;
    }
    // The row reads the title column from the menu whenever it draws or hit-tests, not here:
    // another observer of this same notification may not have put its images on yet.
    let row = TagRowView::new(mtm, armed.content, &items[run.clone()]);
    // ❗ Drop the fallback bitmap from the item that carries the row: AppKit still reserves the
    // image column for an item's image when a view draws it, which would push every title in
    // the menu 24 pt right (measured on macOS 27.0, `NSMenu.size` offscreen, 2026-09-16).
    clear_menu_item_image(&items[start]);
    items[start].setView(Some(&row));
    for item in &items[start + 1..run.end] {
        item.setHidden(true);
    }
    INSTALLED.with(|slot| *slot.borrow_mut() = Some(Weak::from_retained(&row)));
}

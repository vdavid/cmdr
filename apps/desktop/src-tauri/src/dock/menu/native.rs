//! Turning the decided rows into a real `NSMenu`.
//!
//! Hand-built with objc2, because Tauri exposes no `NSMenu`: `Submenu::inner()` is
//! `pub(crate)` and the `mainMenu()`-walking trick in `menu/macos_appkit.rs` doesn't
//! transfer, since a Dock menu never enters the menu bar. ❌ Not muda as a direct
//! dependency either: it works today only while cargo unifies our copy with Tauri's,
//! and a Tauri bump to a new muda would silently give the process two sets of statics
//! and a Dock menu whose clicks go nowhere.
//!
//! Every item carries its row's INDEX as its `tag`, and `mod.rs` looks the row back up
//! by that index. A tag is just an integer AppKit hands back, so the lookup is a
//! bounds-checked `get`, ❌ never an index.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{NSInteger, NSString};

use crate::intl::{menu_t, menu_t_with};
use crate::menu::macos_appkit::set_sf_symbol;

use super::rows::{DockLabel, DockRow};

/// The `menu.*` key wording a row qualified by the folder holding it.
const IN_PARENT_KEY: &str = "menu.dock.locationInParent";

/// Builds the Dock menu for `rows`, with every clickable item aimed at `target`.
///
/// `action` is the selector `mod.rs` added to the target's class. AppKit's own
/// `Options ▸` / `Show All Windows` / `Quit` get appended below whatever this returns.
pub fn build(mtm: MainThreadMarker, rows: &[DockRow], target: &AnyObject, action: Sel) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    // Nothing here validates: with autoenabling on, AppKit would ask the target for
    // `validateMenuItem:`, which the app delegate doesn't implement, and every row
    // would come up grey.
    menu.setAutoenablesItems(false);

    for (index, row) in rows.iter().enumerate() {
        let item = match row {
            DockRow::Separator => NSMenuItem::separatorItem(mtm),
            DockRow::Command(command) => {
                let item = clickable(mtm, &menu_t(command.label_key()), target, action, index);
                set_sf_symbol(&item, command.symbol());
                item
            }
            DockRow::Location(location) => {
                let item = clickable(mtm, &render(&location.label), target, action, index);
                set_sf_symbol(&item, location.kind.symbol());
                item
            }
        };
        menu.addItem(&item);
    }

    menu
}

/// One item that does something when clicked, tagged with its row index.
fn clickable(
    mtm: MainThreadMarker,
    title: &str,
    target: &AnyObject,
    action: Sel,
    index: usize,
) -> Retained<NSMenuItem> {
    let title = NSString::from_str(title);
    let key_equivalent = NSString::from_str("");
    // SAFETY: the designated initializer for `NSMenuItem`, called on a fresh `alloc`
    // from this thread's `MainThreadMarker`. `action` is a selector the caller has
    // already added to `target`'s class, and an empty key equivalent is the documented
    // way to ask for no shortcut.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &title, Some(action), &key_equivalent)
    };
    // SAFETY: `target` is the live `NSApplication` delegate, which outlives every menu
    // AppKit builds from it (it is retained by `NSApp` for the process's lifetime).
    // `setTarget:` holds it unretained, which is exactly right for that relationship.
    unsafe { item.setTarget(Some(target)) };
    item.setTag(clamp_to_tag(index));
    item
}

/// A row index as an AppKit tag, saturating rather than wrapping.
///
/// `NSInteger` is 64-bit on every platform Cmdr ships to, so this never actually
/// saturates; it exists so the conversion has no arm that can panic.
fn clamp_to_tag(index: usize) -> NSInteger {
    NSInteger::try_from(index).unwrap_or(NSInteger::MAX)
}

/// The text a location row shows.
///
/// The qualified form goes through `menu_t_with` rather than a `format!`, so a
/// translator decides how a name and the folder holding it sit together; several
/// languages don't put the qualifier in trailing parentheses.
fn render(label: &DockLabel) -> String {
    match label {
        DockLabel::Plain(name) | DockLabel::Path(name) => name.clone(),
        DockLabel::InParent { name, parent } => menu_t_with(IN_PARENT_KEY, &[("name", name), ("parent", parent)]),
    }
}

/// The app delegate, which is both the class we hang our two selectors on and the
/// target every clickable item points at.
///
/// `None` before tao installs it, which can't happen after `RunEvent::Ready` but is
/// answered rather than asserted: this runs inside an AppKit callback.
pub fn app_delegate(mtm: MainThreadMarker) -> Option<Retained<AnyObject>> {
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    let delegate = app.delegate()?;
    // SAFETY: every Objective-C object answers `-self` with itself; the cast to
    // `AnyObject` erases the protocol type without changing the pointer. Reached only
    // from the main thread, which `mtm` witnesses.
    Some(unsafe { msg_send![&*delegate, self] })
}

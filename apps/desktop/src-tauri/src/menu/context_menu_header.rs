//! The first line of the file context menu: what the menu is about to act on.
//!
//! Cmdr follows Finder's rule — a right-click inside the current selection acts on
//! the whole selection, a right-click outside it acts on that one row — and nothing
//! on screen used to say which. Select three photos, right-click a fourth, and every
//! item below applied to the fourth alone with no way to tell. This line says it.
//!
//! The line itself (its words, and its place first in the menu) is the crate's
//! (`cmdr_menu::context::file::header_label`), built as a DISABLED item, which is what makes it
//! read as a label rather than a command (`context_rows.rs`). This file holds what the IPC sends
//! for it, and the macOS half: an attributed title set when the menu begins tracking, so it also
//! LOOKS like a header instead of a greyed-out command. If that half doesn't happen
//! the plain disabled label stays, which is why nothing here can panic.
//!
//! ❗ The header composes no user-facing WORDS and formats no numbers. See [`ContextMenuTarget`].
//!
//! ❌ The line carries no file KIND either ("PNG image", which Finder shows): `UTType`
//! would pull `UniformTypeIdentifiers.framework` past the app's 10.15 floor, and the
//! `NSURL` alternative reads the disk on every right-click and can hang on a dead
//! mount. Name and size answer the question the header exists for. `DETAILS.md`.

/// What `show_file_context_menu` accepts over IPC about the right-clicked ROW(S).
///
/// Its own struct rather than a trailing argument, so there's a home for the next such
/// fact. It lives HERE rather than beside `PaneContextMenuFacts` in `commands/menu.rs`
/// (which is where the pane pair sits) because the whole rationale for the one field is
/// the header's, and one copy of that rationale is better than two.
/// ❗ Both fields are TEXT the frontend already rendered, and that is the whole design:
/// a locale's thousands separator, decimal separator, plural category, and size unit are
/// all things `crate::intl` deliberately can't do (`menu_t` is a table lookup with no ICU
/// and no number formatting), while the webview owns all four. Composing either half here
/// would mean a second, worse implementation drifting from the pane's own status bar and
/// size column, with both on screen at once. Rust decides the SHAPE of the label; the
/// frontend supplies the words and digits inside it.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextMenuTarget {
    /// How many rows the menu acts on, as the frontend words it (`3 items`,
    /// `12,345 items`) — the count with its grouping separator and the plural form its
    /// language needs, from `fileExplorer.contextMenu.itemCount`.
    ///
    /// ❗ Read ONLY when the backend's own `context_paths.len()` is 2 or more: the NUMBER
    /// of targets decides filename-vs-count, and that number is the backend's, derived
    /// from the paths the menu will actually act on. This field is the text for the
    /// count branch, never the reason to take it.
    ///
    /// `None` falls back to the bare number (`cmdr_menu::context::file::header_label`).
    pub count_text: Option<String>,
    /// The size the header shows, as the frontend rendered it (`2.1 MB`).
    ///
    /// Two settings decide what a size looks like (`appearance.fileSizeFormat` and
    /// `listing.sizeUnit`), and the formatter that honours them is
    /// `src/lib/units/byte-size.ts`.
    ///
    /// `None` means there's no size to show: a folder, a row whose size isn't known
    /// yet, or a selection the frontend can't total honestly. ❌ Never a fabricated zero.
    pub size_text: Option<String>,
}

/// The macOS half: the header line drawn as a header rather than as a greyed-out
/// command.
///
/// Same boundary and same reason as `context_menu_icons.rs`: Tauri hands out no
/// `NSMenu` for a context menu, so the only way in is
/// `NSMenuDidBeginTrackingNotification`, which AppKit posts before the menu is laid
/// out. And the same bargain: a title that isn't there costs the styling and nothing
/// else, because the plain disabled item is already correct on its own.
///
/// ❌ Not `+[NSMenuItem sectionHeaderWithTitle:]`: it's macOS 14, and it CREATES an
/// item, so it can't restyle the one muda already made. Swapping items inside a menu
/// muda owns would desync muda's own child bookkeeping.
#[cfg(target_os = "macos")]
mod macos {
    use std::cell::{Cell, RefCell};

    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSMenuItem};
    use objc2_foundation::{NSAttributedString, NSDictionary, NSNotification, NSString};
    use tauri::Runtime;
    use tauri::menu::Menu;

    use super::super::CONTEXT_MENU_TARGET_ID;
    use super::super::macos_appkit::{find_ns_item, menu_item_text, observe_menu_tracking, tracking_menu};

    /// The header armed for the next context menu to open.
    ///
    /// ❗ Hold it until `popup()` returns, exactly as `IconLoan` and `ServicesLoan` are
    /// held: `popup()` runs AppKit's tracking loop, so the menu only starts tracking
    /// inside it, and dropping this early would disarm before the title ever landed. ❌
    /// Never `let _ =`.
    pub struct HeaderLoan(MainThreadMarker);

    impl Drop for HeaderLoan {
        fn drop(&mut self) {
            ARMED.with(|slot| slot.borrow_mut().take());
        }
    }

    thread_local! {
        /// The header's live TITLE, because AppKit has never heard of a Tauri menu ID.
        /// Main-thread-only by construction: every reader runs from the menu thread.
        static ARMED: RefCell<Option<String>> = const { RefCell::new(None) };
        /// Whether the tracking observer is registered. Lazily, so the cost lands with
        /// the first right-click rather than on every launch.
        static OBSERVING: Cell<bool> = const { Cell::new(false) };
    }

    /// Arms `menu`'s header for styling. The menu must be about to `popup()`.
    ///
    /// Answers `None` when there's nothing to do: off the main thread, or the menu has
    /// no header item (every context menu but the file one).
    pub fn lend_context_menu_header<R: Runtime>(menu: &Menu<R>) -> Option<HeaderLoan> {
        let Some(mtm) = MainThreadMarker::new() else {
            log::warn!(target: "menu", "Not on the main thread; the context menu's first line stays a plain disabled item");
            return None;
        };
        let title = menu_item_text(&menu.get(CONTEXT_MENU_TARGET_ID)?)?;
        ensure_observing(mtm);
        ARMED.with(|slot| *slot.borrow_mut() = Some(title));
        Some(HeaderLoan(mtm))
    }

    /// Registers the tracking observer, once per process.
    fn ensure_observing(mtm: MainThreadMarker) {
        if OBSERVING.replace(true) {
            return;
        }
        observe_menu_tracking(
            mtm,
            "style the right-click menu's first line as a header",
            |mtm, note| {
                on_menu_did_begin_tracking(mtm, note);
            },
        );
    }

    /// Restyles the armed title on the tracking menu, if this menu carries it.
    ///
    /// Fires for every menu the app tracks, the menu bar included, so it does nothing
    /// unless a header is armed.
    fn on_menu_did_begin_tracking(mtm: MainThreadMarker, notification: &NSNotification) {
        let Some(title) = ARMED.with(|slot| slot.borrow().clone()) else {
            return;
        };
        let Some(menu) = tracking_menu(mtm, notification) else {
            return;
        };
        let Some(item) = find_ns_item(&menu, &title) else {
            return;
        };
        style_as_header(&item, &title);
    }

    /// Sets a small-system-font, secondary-colour attributed title on `item`.
    fn style_as_header(item: &NSMenuItem, title: &str) {
        let font = NSFont::menuFontOfSize(NSFont::smallSystemFontSize());
        let color = NSColor::secondaryLabelColor();
        // SAFETY: both are AppKit's own attribute-name constants, immortal statics read
        // through the bindings' declared type; nothing is mutated and nothing escapes
        // this call. The `unsafe` is only because a Rust `extern static` can't be proven
        // initialized at compile time.
        let keys: [&NSString; 2] = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
        let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
        let attributes: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(&keys, &values);
        let string = NSString::from_str(title);
        // SAFETY: `initWithString_attributes:` is unsafe only because it takes an
        // uninitialized allocation and an untyped attribute dictionary. The allocation is
        // this call's own, and the dictionary maps two real `NSAttributedStringKey`s to
        // objects of the classes those keys require (`NSFont`, `NSColor`).
        let attributed = unsafe {
            NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &string, Some(&attributes))
        };
        item.setAttributedTitle(Some(&attributed));
    }
}

#[cfg(target_os = "macos")]
pub use macos::lend_context_menu_header;

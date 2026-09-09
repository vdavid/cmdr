//! The first line of the file context menu: what the menu is about to act on.
//!
//! Cmdr follows Finder's rule — a right-click inside the current selection acts on
//! the whole selection, a right-click outside it acts on that one row — and nothing
//! on screen used to say which. Select three photos, right-click a fourth, and every
//! item below applied to the fourth alone with no way to tell. This line says it.
//!
//! Portable half: a DISABLED `MenuItem`, which is what makes it read as a label
//! rather than a command, plus a separator. It needs no AppKit and works on Linux.
//! macOS half: an attributed title set when the menu begins tracking, so it also
//! LOOKS like a header instead of a greyed-out command. If that half doesn't happen
//! the plain disabled label stays, which is why nothing here can panic.
//!
//! ❗ This module composes no user-facing WORDS and formats no numbers. It decides the
//! label's shape from the one fact it owns — how many rows the menu will act on — and
//! fills each slot with text the frontend already rendered. See [`ContextMenuTarget`].

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

use super::CONTEXT_MENU_TARGET_ID;
use super::menu_items::truncate_for_menu_label;

/// Max chars of filename in the header before the middle ellipsis kicks in. The same
/// budget `Copy "<name>"` uses, so the two longest lines of the menu agree on width.
const TARGET_NAME_MAX_CHARS: usize = 50;

/// What joins the name and the size. A middle dot (U+00B7) between spaces: it reads
/// the same in every language we ship, so it needs no catalog entry of its own.
const PART_SEPARATOR: &str = " \u{00B7} ";

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
    /// `None` falls back to the bare number (see `context_menu_target_label`).
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

/// What the right-clicked ROW(S) contribute to the header, as opposed to the pane
/// they sit in (`super::ContextMenuPaneFacts`). The builder-facing half of
/// [`ContextMenuTarget`], with the count the backend adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct ContextMenuTargetFacts<'a> {
    /// How many rows the menu will act on: the whole selection when the click landed
    /// inside it, one row when it didn't. Always ≥ 1 (`show_file_context_menu`
    /// derives it from `context_paths`, which it never lets be empty). ❗ The one field
    /// the backend owns, and the only thing that decides which branch the label takes.
    pub count: usize,
    /// [`ContextMenuTarget::count_text`], borrowed.
    pub count_text: Option<&'a str>,
    /// [`ContextMenuTarget::size_text`], borrowed.
    pub size_text: Option<&'a str>,
}

/// Appends the header line and the separator under it. Call it FIRST, before any
/// group: it's the top line of the menu.
pub(super) fn append_context_menu_header<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    filename: &str,
    target: ContextMenuTargetFacts<'_>,
) -> tauri::Result<()> {
    // Disabled, and that's the point: a label can't be clicked, and `menu_id_to_command`
    // maps this ID to nothing, so even a click that somehow arrived would do nothing.
    let item = MenuItem::with_id(
        app,
        CONTEXT_MENU_TARGET_ID,
        context_menu_target_label(filename, target),
        false,
        None::<&str>,
    )?;
    menu.append(&item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    Ok(())
}

/// The header's text: what the menu acts on, then its size when there is one.
///
/// One row reads `photo.jpg · 2.1 MB`, several read `3 items · 3.2 MB`, and either
/// drops the size half when there's nothing to show (`photo folder`, `3 items`).
///
/// `target.count` picks the branch; the words inside each branch come from elsewhere
/// (the filesystem for a name, the frontend for a count or a size).
fn context_menu_target_label(filename: &str, target: ContextMenuTargetFacts<'_>) -> String {
    let subject = if target.count > 1 {
        // The bare number when the frontend sent no wording. It loses the noun, but it
        // keeps the one fact this line exists for — that the menu acts on more than the
        // row under the pointer — and can't be mistaken for a filename. ❌ Not the
        // primary filename, which would restate the exact ambiguity the header removes,
        // and ❌ not the size alone, which would drop the count silently.
        non_empty(target.count_text).map_or_else(|| target.count.to_string(), str::to_string)
    } else {
        truncate_for_menu_label(filename, TARGET_NAME_MAX_CHARS)
    };
    match non_empty(target.size_text) {
        Some(size) => format!("{subject}{PART_SEPARATOR}{size}"),
        None => subject,
    }
}

/// `text` unless it's absent or blank. An empty string is the same answer as `None`, so
/// a caller that formatted one and got nothing back can't leave a separator dangling.
fn non_empty(text: Option<&str>) -> Option<&str> {
    text.filter(|value| !value.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// No locale pinning here, and none needed: nothing in this module reads a catalog.
    /// The words arrive over IPC, already in the user's language.
    fn target<'a>(count: usize, count_text: Option<&'a str>, size_text: Option<&'a str>) -> ContextMenuTargetFacts<'a> {
        ContextMenuTargetFacts {
            count,
            count_text,
            size_text,
        }
    }

    #[test]
    fn one_row_reads_its_name_then_its_size() {
        assert_eq!(
            context_menu_target_label("photo.jpg", target(1, None, Some("2.1 MB"))),
            "photo.jpg · 2.1 MB"
        );
    }

    #[test]
    fn one_row_with_no_size_is_just_its_name() {
        // The folder case: no size to show, so no separator dangling after the name.
        assert_eq!(
            context_menu_target_label("photo folder", target(1, None, None)),
            "photo folder"
        );
    }

    #[test]
    fn several_rows_read_as_the_count_the_frontend_worded_then_the_total() {
        assert_eq!(
            context_menu_target_label("photo.jpg", target(3, Some("3 items"), Some("3.2 MB"))),
            "3 items · 3.2 MB"
        );
    }

    #[test]
    fn several_rows_with_no_total_are_just_the_count() {
        // A selection holding a folder: the frontend can't total it honestly, so it
        // sends `None` rather than a number that would understate the pile.
        assert_eq!(
            context_menu_target_label("photo.jpg", target(3, Some("3 items"), None)),
            "3 items"
        );
    }

    /// The count's WORDING is the frontend's; the count itself is ours.
    ///
    /// A locale's grouping separator and plural form both live in the webview, so the
    /// text arrives pre-rendered — but the NUMBER of targets is what picks this branch,
    /// and that comes from the paths the menu will act on.
    #[test]
    fn the_wording_is_used_verbatim_however_large_the_count() {
        assert_eq!(
            context_menu_target_label("photo.jpg", target(12_345, Some("12,345 items"), Some("9.9 GB"))),
            "12,345 items · 9.9 GB"
        );
    }

    /// One row shows its filename even if a count text arrives: `count` alone decides
    /// the branch, so a frontend that over-sends can't turn a name into a tally.
    #[test]
    fn one_row_shows_its_name_even_when_a_count_text_arrives() {
        assert_eq!(
            context_menu_target_label("photo.jpg", target(1, Some("1 item"), None)),
            "photo.jpg"
        );
    }

    /// The defensive branch: no wording for several rows still says the menu acts on
    /// more than the row under the pointer.
    #[test]
    fn several_rows_with_no_wording_still_say_how_many() {
        assert_eq!(
            context_menu_target_label("photo.jpg", target(3, None, Some("3.2 MB"))),
            "3 · 3.2 MB"
        );
        // ❗ Never the primary filename, which would restate the exact ambiguity this
        // line exists to remove, and never the size alone, which drops the count.
        assert_eq!(context_menu_target_label("photo.jpg", target(3, None, None)), "3");
    }

    /// An empty string is the same answer as `None` on either half, so a caller that
    /// formats one and gets nothing back can't leave a separator hanging off the end.
    #[test]
    fn an_empty_string_leaves_no_dangling_separator() {
        assert_eq!(
            context_menu_target_label("photo.jpg", target(1, None, Some(""))),
            "photo.jpg"
        );
        assert_eq!(context_menu_target_label("photo.jpg", target(3, Some(""), None)), "3");
    }

    /// A click on the header does nothing, on top of the item being disabled.
    ///
    /// Disabled is the first guard and AppKit won't deliver the click at all, but the ID
    /// travels through the same flat space as every other menu ID, so the second guard is
    /// that it maps to no command. A stray mapping here would fire a real command from a
    /// line that is only a label.
    #[test]
    fn the_header_maps_to_no_command() {
        assert!(super::super::menu_id_to_command(CONTEXT_MENU_TARGET_ID).is_none());
    }

    /// A pathological filename can't blow the menu's width; it middle-ellipsizes the
    /// way `Copy "<name>"` does, keeping the extension.
    #[test]
    fn a_long_filename_is_truncated_in_the_middle() {
        let long = format!("{}.jpg", "a".repeat(80));
        let label = context_menu_target_label(&long, target(1, None, Some("2.1 MB")));
        assert!(label.starts_with("aaa"), "{label}");
        assert!(label.contains('\u{2026}'), "{label}");
        assert!(label.ends_with(".jpg · 2.1 MB"), "{label}");
        // The name half stays inside the budget; the size rides outside it.
        let name = label.strip_suffix(" · 2.1 MB").expect("the size half is appended last");
        assert_eq!(name.chars().count(), TARGET_NAME_MAX_CHARS);
    }
}

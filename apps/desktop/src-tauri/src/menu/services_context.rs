//! `Services` inside Cmdr's own right-click menu (macOS).
//!
//! The same list `Cmdr > Services` shows, on the row the user right-clicked. Two
//! things make that work, and both exist because AppKit owns exactly ONE Services
//! menu and it is already spoken for:
//!
//! 1. **The menu is borrowed, not copied.** `NSApplication.servicesMenu` is the only
//!    `NSMenu` AppKit fills, and an `NSMenu` can have one supermenu at a time. So the
//!    context menu's Services item borrows it for as long as the menu is up and
//!    [`ServicesLoan`] hands it back to the menu bar afterwards.
//! 2. **The selection is overridden while the loan is out.** A context menu acts on
//!    the RIGHT-CLICKED row, which is not what the pane pushed (`services_menu`'s
//!    selection is the pane's answer to Finder's rule). `services_menu::selection`'s
//!    context target replaces it for the loan's lifetime.
//!
//! ⚠️ Tauri exposes no `NSMenu` for a context menu (muda's `ns_menu()` sits behind
//! Tauri's sealed `ContextMenuBase`), so the borrow can't happen before `popup()`.
//! `NSMenuDidBeginTrackingNotification` is the handle: AppKit posts it with the menu
//! it is about to track, which is where [`on_menu_did_begin_tracking`] does the swap.
//! Details and what else was tried: `DETAILS.md` § "Services in the right-click menu".

use std::cell::{Cell, RefCell};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::{ClassType, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuDidBeginTrackingNotification, NSMenuItem};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

use crate::intl::menu_t;
use crate::services_menu::selection;

use super::SERVICES_CONTEXT_ID;
use super::macos_appkit::{detach_from_supermenu, find_ns_item};

/// The app menu's own catalog key: the same word for the same system feature, and
/// macOS spells Finder's context-menu item the same way.
fn services_label() -> String {
    menu_t("menu.app.services")
}

/// Appends `Services` (with a submenu arrow, filled by AppKit when the menu opens) to
/// a file context menu.
///
/// Last, in its own group, which is where Finder puts it.
///
/// The submenu starts EMPTY: AppKit's menu can only be borrowed once the menu is
/// actually tracking (see the module docs). If the borrow never happens the user sees
/// an empty submenu rather than an item that does nothing when clicked.
pub fn append_services_submenu<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<()> {
    let services = Submenu::with_id(app, SERVICES_CONTEXT_ID, services_label(), true)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&services)?;
    Ok(())
}

/// AppKit's Services menu, out on loan to a context menu.
///
/// Dropping it hands the menu back to the menu bar and gives the Services menu back
/// to the pane selection. ❗ Hold it until `popup()` returns: `popup()` runs AppKit's
/// tracking loop, so the whole life of the context menu, the user's pick, and the
/// pasteboard write that follows all happen inside it.
///
/// Not `Send`: it carries a `MainThreadMarker`, and everything it touches is
/// main-thread-only AppKit.
pub struct ServicesLoan(MainThreadMarker);

impl Drop for ServicesLoan {
    fn drop(&mut self) {
        let mtm = self.0;
        // Re-parenting an `NSMenu` is the one step here that can raise, and this runs
        // on the way out of an `extern "C"` frame that aborts on panic.
        if let Err(e) = objc2::exception::catch(AssertUnwindSafe(|| hand_back(mtm))) {
            log::warn!(target: "menu", "Couldn't hand AppKit's Services menu back to the menu bar: {e:?}");
        }
        selection::clear_context_target();
    }
}

/// Points the Services menu at the right-clicked row(s) and arms the borrow for the
/// next context menu that opens.
///
/// `paths` is `MenuState.context.paths`: the right-clicked row alone, or the whole
/// selection when that row is part of it, which is Finder's rule and the one the rest
/// of the context menu already follows.
///
/// Answers `None` when there is nothing to lend to — `menu` carries no Services item
/// (the pane's rows aren't OS paths), or there are no paths — which leaves any item
/// alone rather than pointing it at the wrong files.
pub fn lend_services_menu<R: Runtime>(menu: &Menu<R>, paths: Vec<PathBuf>) -> Option<ServicesLoan> {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "menu", "Not on the main thread; the context menu's Services submenu stays empty");
        return None;
    };
    if paths.is_empty() {
        return None;
    }
    // The house rule for reaching AppKit: key by ID, and ask the live item what title
    // it carries, so the match survives translation and anything muda does to a label.
    let label = menu.get(SERVICES_CONTEXT_ID)?.as_submenu()?.text().ok()?;
    ensure_observing(mtm);
    selection::set_context_target(paths);
    LOAN.with(|slot| {
        *slot.borrow_mut() = Some(Loan { label, attached: None });
    });
    Some(ServicesLoan(mtm))
}

/// What a live loan knows: the title to find the borrowing item by, and — once a menu
/// starts tracking — who borrowed what from whom.
struct Loan {
    label: String,
    attached: Option<Attachment>,
}

struct Attachment {
    /// AppKit's own Services menu, the thing being borrowed.
    managed: Retained<NSMenu>,
    /// The context menu's Services item, which is showing it right now.
    borrower: Retained<NSMenuItem>,
    /// The menu bar's Services item, to hand it back to. `None` means AppKit's menu
    /// was parentless when we took it, which only happens if a menu-bar swap raced
    /// the right-click; the next `cleanup_macos_menus` re-adopts it.
    lender: Option<Retained<NSMenuItem>>,
}

thread_local! {
    /// The loan in flight, if any. Main-thread-only by construction: it holds
    /// `Retained` AppKit objects, and every reader runs from the menu thread.
    static LOAN: RefCell<Option<Loan>> = const { RefCell::new(None) };
    /// Whether the tracking observer is registered. Registering it lazily keeps the
    /// cost with the first right-click instead of every launch.
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}

/// Registers the tracking observer, once per process.
///
/// Takes the marker to pin registration to the menu thread, which is where
/// `OBSERVING` and `LOAN` live and where the callback expects to run.
fn ensure_observing(_mtm: MainThreadMarker) {
    if OBSERVING.replace(true) {
        return;
    }
    let block = block2::RcBlock::new(move |notification: NonNull<NSNotification>| {
        // SAFETY: `NSNotificationCenter` hands the callback a live notification for the
        // duration of the call, and this observer asked for no queue, so it is
        // delivered synchronously on the thread that posted — the menu thread.
        let notification = unsafe { notification.as_ref() };
        let Some(mtm) = MainThreadMarker::new() else {
            log::warn!(target: "menu", "Menu tracking began off the main thread; Services stays empty");
            return;
        };
        // The same `catch` reasoning as `ServicesLoan::drop`: this frame is called
        // from ObjC and an escaping raise would abort.
        if let Err(e) = objc2::exception::catch(AssertUnwindSafe(|| {
            on_menu_did_begin_tracking(mtm, notification);
        })) {
            log::warn!(target: "menu", "Couldn't lend AppKit's Services menu to the context menu: {e:?}");
        }
    });
    // SAFETY: `NSMenuDidBeginTrackingNotification` is AppKit's own name constant, and
    // the observer is deliberately never removed: it lives for the process, like the
    // accent-color one in `accent_color.rs`.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSMenuDidBeginTrackingNotification),
            None,
            None,
            &block,
        );
    }
}

/// Hangs AppKit's Services menu off the context menu's Services item, the moment that
/// menu starts tracking.
///
/// Fires for every menu the app tracks (the menu bar included), so it does nothing
/// unless a loan is armed and the tracking menu is the one carrying our item.
fn on_menu_did_begin_tracking(mtm: MainThreadMarker, notification: &NSNotification) {
    let Some(label) = LOAN.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|loan| loan.attached.is_none())
            .map(|loan| loan.label.clone())
    }) else {
        return;
    };
    let Some(menu) = tracking_menu(mtm, notification) else {
        return;
    };
    let Some(borrower) = find_ns_item(&menu, &label) else {
        // Every other menu in the app: the menu bar, a breadcrumb menu, a tab menu.
        return;
    };
    let Some(managed) = NSApplication::sharedApplication(mtm).servicesMenu() else {
        log::warn!(target: "menu", "AppKit owns no Services menu to lend; the submenu stays empty");
        return;
    };
    let lender = detach_from_supermenu(&managed);
    borrower.setSubmenu(Some(&managed));
    log::debug!(
        target: "menu",
        "Lent AppKit's Services menu ({} items) to the right-click menu",
        managed.numberOfItems()
    );
    LOAN.with(|slot| {
        if let Some(loan) = slot.borrow_mut().as_mut() {
            loan.attached = Some(Attachment {
                managed,
                borrower,
                lender,
            });
        }
    });
}

/// The `NSMenu` a tracking notification is about. Takes the marker because the
/// `Retained<NSMenu>` it mints is a main-thread-only object.
fn tracking_menu(_mtm: MainThreadMarker, notification: &NSNotification) -> Option<Retained<NSMenu>> {
    let object = notification.object()?;
    // Walking the chain rather than comparing the class outright: AppKit tracks
    // private `NSMenu` subclasses (the menu bar's own, for one), and `isKindOfClass:`
    // isn't reachable on an `AnyObject` without dropping into `msg_send!`.
    let is_menu =
        std::iter::successors(Some(object.class()), |class| class.superclass()).any(|class| class == NSMenu::class());
    if !is_menu {
        return None;
    }
    // SAFETY: the walk above proves the object's class descends from `NSMenu`, and an
    // `NSMenu` can only exist on the main thread, which the caller holds a marker for.
    Some(unsafe { Retained::cast_unchecked::<NSMenu>(object) })
}

/// Puts AppKit's Services menu back on the menu bar's Services item.
fn hand_back(_mtm: MainThreadMarker) {
    let Some(Loan {
        attached: Some(attachment),
        ..
    }) = LOAN.take()
    else {
        return;
    };
    attachment.borrower.setSubmenu(None);
    let Some(lender) = attachment.lender else {
        log::warn!(target: "menu", "AppKit's Services menu had no menu-bar item to go back to");
        return;
    };
    lender.setSubmenu(Some(&attachment.managed));
    log::debug!(target: "menu", "Handed AppKit's Services menu back to the menu bar");
}

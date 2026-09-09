//! The menu that appears when someone right-clicks Cmdr's Dock tile.
//!
//! Live only while the app runs (no `NSDockTilePlugIn`), and built from scratch on
//! every right-click, so it always shows the bookmarks and tabs of the moment.
//!
//! ## The two guarantees this module exists to hold
//!
//! AppKit calls `applicationDockMenu:` **on the main thread, synchronously, while the
//! Dock waits for the answer**. So:
//!
//! 1. ❌ **A Rust panic must never unwind into Objective-C**: that is undefined
//!    behavior, not a clean crash. Both callbacks wrap their ENTIRE body in
//!    `catch_unwind` and answer nil / do nothing on the panic path, and the AppKit
//!    half additionally sits inside `objc2::exception::catch` (an ObjC exception is a
//!    foreign exception `catch_unwind` can't see).
//! 2. ❌ **Nothing here may block.** No I/O, no syscall against a user path, no lock
//!    another thread can hold across I/O. Every source is read with `try_lock` /
//!    `try_read` and contributes nothing when it isn't free; `rows.rs` judges paths by
//!    shape alone. What each source is and why it's safe: `DETAILS.md`.
//!
//! ## How a click gets out
//!
//! The menu is ours, not muda's, so a click never reaches
//! `menu::menu_handlers::handle_menu_event` and never meets its `CommandScope::FileScoped`
//! focus guard, which would have dropped every one of these, since a Dock right-click
//! happens with Cmdr in the background by definition. Each item carries its row index
//! as its AppKit `tag`; [`item_clicked`] looks the row up, raises the main window, and
//! emits the same `execute-command` / `reveal-path` events the rest of the app already
//! listens for.

mod native;
mod rows;
mod sources;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::{NSMenu, NSMenuItem};
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;

use crate::drag_image_detection::warn_once;
use crate::window_events::{ExecuteCommand, RevealPath};
use rows::{DockCommand, DockRow};

const LOG_TARGET: &str = "dock::menu";

/// The handle every click needs, stashed at install time. `applicationDockMenu:` has
/// no way to be handed one.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// The home directory, resolved ONCE at install time and off the main thread.
///
/// ❗ Resolved early on purpose: `dirs::home_dir` falls back to `getpwuid_r` when
/// `$HOME` is unset, and on a directory-service-managed Mac that call can reach the
/// network. Doing it here means the menu build only ever reads a `PathBuf`.
static HOME: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Warn-once flags, so a recurring failure can't fill the log with one line per
/// right-click.
static WARNED_MENU_PANIC: AtomicBool = AtomicBool::new(false);
static WARNED_CLICK_PANIC: AtomicBool = AtomicBool::new(false);
static WARNED_MENU_EXCEPTION: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The rows behind the menu AppKit is currently showing, indexed by item tag.
    ///
    /// A thread-local rather than a lock: both the build and the click run on the main
    /// thread, so there is nothing to contend with and nothing that could block. Each
    /// borrow is taken, read, and dropped within one statement; ❌ never held across a
    /// call that could re-enter. Same shape as `menu/context_menu_icons.rs`'s `ARMED`.
    static SHOWING: std::cell::RefCell<Vec<DockRow>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Teaches the live `NSApplication` delegate to answer `applicationDockMenu:`.
///
/// Called from `app_lifecycle.rs`'s `RunEvent::Ready`, beside `drag_image_detection::install`.
/// tao's `TaoAppDelegateParent` implements neither selector we add, so this is a plain
/// `class_addMethod` with no swizzle and no original to forward to.
pub fn install(app: AppHandle) {
    APP.set(app).ok();

    // Off the main thread, so neither answer is ever computed while the Dock waits:
    // the home directory, and the favorites cache the menu reads through `try_lock`.
    tauri::async_runtime::spawn_blocking(|| {
        HOME.set(dirs::home_dir()).ok();
        // Called for the load-and-seed side effect alone. The menu itself reads this
        // cache through `list_cached`, which never touches disk and never waits, so
        // filling it here is what keeps the first right-click of a session from
        // showing an empty bookmarks group.
        drop(crate::favorites::store::list());
    });

    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: LOG_TARGET, "install ran off the main thread; the Dock tile menu stays macOS's default");
        return;
    };
    let Some(delegate) = native::app_delegate(mtm) else {
        log::warn!(target: LOG_TARGET, "NSApplication has no delegate yet; the Dock tile menu stays macOS's default");
        return;
    };
    // SAFETY: `delegate` is the live app delegate object. `-class` is a universal
    // `NSObject` selector answering the object's class, so `*const AnyClass` matches
    // the real signature. Main thread, witnessed by `mtm`.
    let cls: *const AnyClass = unsafe { msg_send![&*delegate, class] };
    // SAFETY: the class pointer `-class` just returned; `as_ref` dereferences it only
    // when non-null.
    let Some(cls) = (unsafe { cls.as_ref() }) else {
        log::warn!(target: LOG_TARGET, "couldn't read the app delegate's class; the Dock tile menu stays macOS's default");
        return;
    };

    // SAFETY: `Imp` is the runtime's type-erased function pointer, so the
    // implementation has to be transmuted into it, the same shape
    // `drag_image_detection.rs` uses. `dock_menu` is an `extern "C-unwind"` function
    // whose Rust signature matches `"@@:@"`, the encoding of
    // `- (NSMenu *)applicationDockMenu:(NSApplication *)sender`. `add_method` refuses
    // to replace an existing implementation, so this can't clobber tao's.
    unsafe {
        add_method(
            cls,
            sel!(applicationDockMenu:),
            std::mem::transmute::<*const (), Imp>(dock_menu as *const ()),
            c"@@:@",
            "Dock tile menu",
        );
    }
    // SAFETY: same as above, for `- (void)cmdrDockMenuItemClicked:(NSMenuItem *)sender`,
    // whose encoding is `"v@:@"`. The `cmdr` prefix keeps the selector out of every
    // namespace AppKit and tao draw from.
    unsafe {
        add_method(
            cls,
            sel!(cmdrDockMenuItemClicked:),
            std::mem::transmute::<*const (), Imp>(item_clicked as *const ()),
            c"v@:@",
            "Dock tile menu clicks",
        );
    }
}

/// Adds one method to a class we don't own, refusing to replace one that's there.
///
/// A selector that has appeared since (a tao upgrade implementing `applicationDockMenu:`)
/// means the answer is a swizzle, not an add, and silently overwriting theirs would
/// break whatever they added it for. So: log loudly and leave the feature off.
///
/// # Safety
/// `imp` must be an `extern "C-unwind"` function whose signature matches `types` and
/// the ObjC calling convention for `selector`.
unsafe fn add_method(cls: &AnyClass, selector: Sel, imp: Imp, types: &std::ffi::CStr, what: &str) {
    if cls.instance_method(selector).is_some() {
        log::warn!(
            target: LOG_TARGET,
            "{:?} already implements `{selector:?}`, which leaves {what} switched off. \
             Something upstream (most likely a tao upgrade) grew this method; \
             adopting it now needs a swizzle that forwards to theirs, rather than an add.",
            cls.name()
        );
        return;
    }
    // SAFETY: forwarded from the caller's contract (`imp`'s signature matches `types`
    // and the selector's calling convention), plus the guard above, which means this
    // adds a method rather than replacing one.
    let added =
        unsafe { objc2::ffi::class_addMethod(cls as *const AnyClass as *mut AnyClass, selector, imp, types.as_ptr()) };
    if !added.as_bool() {
        log::warn!(target: LOG_TARGET, "the runtime refused `{selector:?}`, which leaves {what} switched off");
    }
}

// ---------------------------------------------------------------------------
// The two AppKit callbacks
// ---------------------------------------------------------------------------

/// `- (NSMenu *)applicationDockMenu:(NSApplication *)sender`
///
/// ❗ The whole body is inside `catch_unwind`. AppKit calls this synchronously while
/// the Dock waits, from Objective-C, and a Rust panic unwinding across that boundary is
/// undefined behavior. A Dock menu that fails to appear is a nuisance; a panic there
/// corrupts the process.
unsafe extern "C-unwind" fn dock_menu(_this: &AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> *mut NSMenu {
    match catch_unwind(AssertUnwindSafe(build)) {
        Ok(Some(menu)) => Retained::autorelease_return(menu),
        Ok(None) => std::ptr::null_mut(),
        Err(_) => {
            warn_once(
                &WARNED_MENU_PANIC,
                "dock::menu: panic while building the Dock tile menu. macOS shows its own \
                 default menu instead for the rest of this session. Check `dock/menu/rows.rs`.",
            );
            std::ptr::null_mut()
        }
    }
}

/// `- (void)cmdrDockMenuItemClicked:(NSMenuItem *)sender`
///
/// Same unwind rule as [`dock_menu`]: this is an Objective-C callback too.
unsafe extern "C-unwind" fn item_clicked(_this: &AnyObject, _cmd: Sel, sender: *mut AnyObject) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes the clicked `NSMenuItem` as the sender of the action it
        // was given, valid for the duration of this call. `as_ref` dereferences it only
        // when non-null.
        let Some(item) = (unsafe { sender.cast::<NSMenuItem>().as_ref() }) else {
            return;
        };
        // The tag is an integer AppKit hands back, so it is answered, never trusted:
        // a negative or out-of-range one simply does nothing.
        let Ok(index) = usize::try_from(item.tag()) else {
            return;
        };
        let Some(row) = SHOWING.with(|showing| showing.borrow().get(index).cloned()) else {
            return;
        };
        perform(&row);
    }));
    if result.is_err() {
        warn_once(
            &WARNED_CLICK_PANIC,
            "dock::menu: panic while acting on a Dock tile menu click. Those clicks do \
             nothing for the rest of this session. Check `dock/menu/mod.rs::perform`.",
        );
    }
}

// ---------------------------------------------------------------------------
// Building, and acting
// ---------------------------------------------------------------------------

/// The menu for this right-click, or `None` when we'd rather show nothing.
///
/// `None` leaves macOS to draw its own default menu, which is a fine outcome and the
/// answer to every "this isn't available right now": no delegate, no app handle, an
/// ObjC exception out of `NSMenu`.
fn build() -> Option<Retained<NSMenu>> {
    let mtm = MainThreadMarker::new()?;
    let app = APP.get()?;
    let target = native::app_delegate(mtm)?;

    let home = HOME.get().and_then(|home| home.as_deref());
    let built = rows::menu_rows(&sources::bookmarks(), &sources::tabs(app), home);

    // In its own `catch`: `NSMenu` work can raise an ObjC exception, which is a foreign
    // exception `catch_unwind` cannot see and which would otherwise leave the process.
    let menu = objc2::exception::catch(AssertUnwindSafe(|| {
        native::build(mtm, &built, &target, sel!(cmdrDockMenuItemClicked:))
    }));
    let menu = match menu {
        Ok(menu) => menu,
        Err(e) => {
            warn_once(
                &WARNED_MENU_EXCEPTION,
                "dock::menu: AppKit raised while building the Dock tile menu, so macOS shows \
                 its own default instead for the rest of this session.",
            );
            log::warn!(target: LOG_TARGET, "NSMenu raised: {e:?}");
            return None;
        }
    };

    // Only once the menu exists: the rows are the lookup table its tags point into, and
    // replacing them for a menu that never went up would strand the previous menu's clicks.
    SHOWING.with(|showing| *showing.borrow_mut() = built);
    Some(menu)
}

/// What a clicked row does.
///
/// Every row raises the main window first. A Dock right-click happens with Cmdr in the
/// background by definition, so a dialog opened without raising would land behind
/// whatever the user is actually looking at. `unminimize` → `show` → `set_focus` is the
/// same sequence the global go-to-latest hotkey uses, for the same reason.
fn perform(row: &DockRow) {
    let Some(app) = APP.get() else {
        return;
    };
    raise_main_window(app);

    match row {
        // Raising the window WAS the command.
        DockRow::Command(DockCommand::OpenCmdr) | DockRow::Separator => {}
        DockRow::Command(command) => {
            let Some(command_id) = command.command_id() else {
                return;
            };
            if let Err(e) = (ExecuteCommand {
                command_id: command_id.to_string(),
            })
            .emit_to(app, "main")
            {
                log::warn!(target: LOG_TARGET, "couldn't send `{command_id}` to the main window: {e}");
            }
        }
        DockRow::Location(location) => {
            if let Err(e) = (RevealPath {
                path: location.path.clone(),
            })
            .emit_to(app, "main")
            {
                log::warn!(target: LOG_TARGET, "couldn't ask the main window to show a folder: {e}");
            }
        }
    }
}

/// Brings the main window forward, unminimizing and showing it on the way.
///
/// Missing entirely is logged rather than repaired: closing the main window quits the
/// whole app (`app_lifecycle::on_window_event`), so a running process without one is a
/// state nothing produces, and a builder here would be untested code for it.
fn raise_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!(target: LOG_TARGET, "no main window to bring forward");
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    if let Err(e) = window.set_focus() {
        log::warn!(target: LOG_TARGET, "couldn't bring the main window forward: {e}");
    }
}

#[cfg(test)]
mod tests;

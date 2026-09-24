//! Fills the file context menu in while it's open (macOS): every slow fact that missed the
//! grace period (`context_menu_facts.rs`) lands here, on the main thread, while the user is
//! already looking at the menu.
//!
//! What a late fact can change, and how, is set by three hard limits:
//!
//! - **muda holds the context menu itself borrowed for the whole popup** (`Menu::append`,
//!   `insert`, even `ns_menu()` panic with "RefCell already borrowed" until `popup()`
//!   returns; muda 0.19.3, verified with a standalone probe on macOS 27.0, 2026-09-24). So
//!   ❌ nothing here may call a method on the top-level `Menu`. Submenus and single items
//!   carry their own cells and change freely, and so does the menu's `NSMenu` through AppKit.
//! - **AppKit keeps the highlight at the same ROW, not on the same item**, so a row inserted
//!   above the highlight moves it onto a different command (same probe). So a late fact never
//!   inserts into the context menu itself. It fills a submenu ("Open with", `Share`), changes a
//!   row that's already there (the tag checks), or reveals rows that were built hidden
//!   ([`SlotGroup`]: the Drive, iCloud, and File Provider groups, which sit near the bottom).
//! - **A late answer reaches the main thread through the main dispatch queue**, which AppKit
//!   drains while a menu tracks. Tauri's own `run_on_main_thread` from another thread goes
//!   through the event-loop proxy, and every Tauri menu call blocks its caller on the answer.
//!   From the main thread Tauri runs a menu call inline, which is what makes the calls here
//!   safe.
//!
//! Every late answer is tagged with its menu's generation, and one for a menu that already
//! closed is dropped.

use std::cell::{Cell, RefCell};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};

use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::NSNotification;
use tauri::menu::{MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};

use super::MenuState;
use super::context_menu_facts::{CancelGathering, Fact};
use super::context_menu_icons::{
    find_title_run, land_open_with_icons, land_provider_logos, land_share_icons, land_tag_circles,
};
use super::macos_appkit::{observe_menu_tracking, tracking_menu};
use crate::file_system::sync_status::SyncStatus;
use crate::ignore_poison::IgnorePoison as _;

/// A submenu that opened before its contents were known: the disabled line standing in for
/// them, and the separator between that line and the submenu's fixed last item.
pub struct Placeheld<R: Runtime> {
    pub submenu: Submenu<R>,
    pub placeholder: MenuItem<R>,
    pub separator: PredefinedMenuItem<R>,
}

/// A group of context-menu rows built before their fact answered, hidden when the menu
/// starts tracking and revealed when it does. The group's leading separator is the row just
/// above its first item.
///
/// Built with their final IDs, so a click routes exactly as it would on a group built with
/// the answer in hand, and with titles that tell them apart ([`find_title_run`] finds the
/// group by them, as one run).
pub struct SlotGroup<R: Runtime> {
    pub items: Vec<MenuItem<R>>,
    /// The live rows, separator first, found when the menu starts tracking. Empty until
    /// then, and for good if the group wasn't found, in which case nothing is revealed.
    rows: Vec<Retained<NSMenuItem>>,
}

impl<R: Runtime> SlotGroup<R> {
    pub fn new(items: Vec<MenuItem<R>>) -> Self {
        Self {
            items,
            rows: Vec::new(),
        }
    }

    /// Finds the group's rows in `root` and hides them, before AppKit lays the menu out.
    fn hide(&mut self, root: &NSMenu) {
        let titles: Option<Vec<String>> = self.items.iter().map(|item| item.text().ok()).collect();
        let all: Vec<Retained<NSMenuItem>> = (0..root.numberOfItems())
            .filter_map(|index| root.itemAtIndex(index))
            .collect();
        let live_titles: Vec<String> = all.iter().map(|item| item.title().to_string()).collect();
        let Some(start) = titles.and_then(|titles| find_title_run(&live_titles, &titles)) else {
            log::warn!(target: "menu", "A right-click group that was still loading isn't in the menu, so it stays as built");
            return;
        };
        let Some(separator) = start
            .checked_sub(1)
            .and_then(|index| all.get(index))
            .filter(|row| row.isSeparatorItem())
        else {
            log::warn!(target: "menu", "A right-click group that was still loading has no separator above it, so it stays as built");
            return;
        };
        self.rows = std::iter::once(separator.clone())
            .chain(all[start..start + self.items.len()].iter().cloned())
            .collect();
        for row in &self.rows {
            row.setHidden(true);
        }
    }

    /// Shows the separator and the items at `which`.
    fn reveal(&self, which: impl IntoIterator<Item = usize>) {
        let Some((separator, items)) = self.rows.split_first() else {
            return;
        };
        let mut any = false;
        for index in which {
            if let Some(row) = items.get(index) {
                row.setHidden(false);
                any = true;
            }
        }
        if any {
            separator.setHidden(false);
        }
    }
}

/// Everything in one context menu that a late fact fills in. Only the facts still pending
/// when the menu went up have a target.
pub struct LateTargets<R: Runtime> {
    pub app: AppHandle<R>,
    pub open_with: Option<Placeheld<R>>,
    pub share: Option<Placeheld<R>>,
    /// Open in Google Drive, Copy Google Drive link, Ask Gemini.
    pub drive: Option<SlotGroup<R>>,
    /// Make available offline, Remove download.
    pub icloud: Option<SlotGroup<R>>,
    /// The provider's own actions, in slots whose titles the offer replaces.
    pub provider: Option<SlotGroup<R>>,
    /// The seven tag items, when their applied colors are still pending.
    pub tag_items: Vec<MenuItem<R>>,
}

impl<R: Runtime> LateTargets<R> {
    pub fn new(app: &AppHandle<R>) -> Self {
        Self {
            app: app.clone(),
            open_with: None,
            share: None,
            drive: None,
            icloud: None,
            provider: None,
            tag_items: Vec::new(),
        }
    }
}

/// What the tracking observer and a late answer need from the live menu, without the
/// Tauri runtime type a thread-local can't carry.
trait LiveTargets {
    fn on_tracking(&mut self, root: &NSMenu);
    fn apply(&mut self, mtm: MainThreadMarker, root: &NSMenu, fact: Fact);
}

impl<R: Runtime> LiveTargets for LateTargets<R> {
    fn on_tracking(&mut self, root: &NSMenu) {
        for group in [&mut self.drive, &mut self.icloud, &mut self.provider]
            .into_iter()
            .flatten()
        {
            group.hide(root);
        }
    }

    fn apply(&mut self, mtm: MainThreadMarker, root: &NSMenu, fact: Fact) {
        let kind = fact.kind();
        if let Err(e) = self.apply_fact(mtm, root, fact) {
            log::warn!(target: "menu", "Couldn't fill the right-click menu in with {kind:?}: {e}");
        }
    }
}

impl<R: Runtime> LateTargets<R> {
    fn apply_fact(&mut self, mtm: MainThreadMarker, root: &NSMenu, fact: Fact) -> tauri::Result<()> {
        let state = self.app.state::<MenuState<R>>();
        match fact {
            Fact::OpenWith(choices) => {
                if let Some(pending) = &self.open_with {
                    let map = super::open_with::fill_open_with_submenu(&self.app, pending, &choices.candidates)?;
                    state.context.lock_ignore_poison().open_with_apps = map;
                    land_open_with_icons(mtm, root, &pending.submenu, &choices);
                }
            }
            Fact::Share(offer) => {
                if let Some(pending) = &self.share {
                    let services = offer
                        .as_ref()
                        .map(|offer| offer.services().to_vec())
                        .unwrap_or_default();
                    // Armed before the items exist, so neither a click nor an icon can find an
                    // index the offer doesn't have.
                    crate::file_system::share::arm_offer(mtm, offer);
                    super::share_submenu::fill_share_submenu(&self.app, pending, &services)?;
                    land_share_icons(mtm, root, &pending.submenu, services.len());
                }
            }
            Fact::Tags(applied) => {
                if !self.tag_items.is_empty() {
                    super::tag_row::set_applied(&applied);
                    land_tag_circles(mtm, root, &self.tag_items, &applied);
                }
            }
            Fact::DriveLinks(links) => {
                if let (Some(group), Some(links)) = (&self.drive, links) {
                    // Ask Gemini is the third slot, and a folder resolves no Gemini URL.
                    let shown = if links.gemini_url.is_some() { 3 } else { 2 };
                    group.reveal(0..shown);
                }
            }
            Fact::SyncStatus(status) => {
                if let Some(group) = &self.icloud {
                    // Same rule as a menu built with the answer: uploading, downloading, or
                    // unknown offers neither.
                    match status {
                        SyncStatus::OnlineOnly => group.reveal([0]),
                        SyncStatus::Synced => group.reveal([1]),
                        _ => {}
                    }
                }
            }
            Fact::ProviderOffer(offer) => {
                if let (Some(group), Some(offer)) = (&self.provider, offer) {
                    let entries = super::file_provider_items::group_entries(&offer);
                    let shown = entries.len().min(group.items.len());
                    if entries.len() > shown {
                        log::warn!(target: "menu", "{} offered {} actions and the menu had room for {shown}", offer.provider_id, entries.len());
                    }
                    for (item, (_, label)) in group.items.iter().zip(&entries) {
                        item.set_text(label)?;
                    }
                    state.context.lock_ignore_poison().file_provider_offer = Some(offer.clone());
                    group.reveal(0..shown);
                    land_provider_logos(mtm, root, &group.items[..shown], &offer);
                }
            }
        }
        Ok(())
    }
}

/// The menu currently up, while it has facts still out.
struct Live {
    generation: u64,
    targets: Box<dyn LiveTargets>,
    /// The menu's `NSMenu`, from the tracking notification. `None` until then.
    root: Option<Retained<NSMenu>>,
    /// Answers that came before the menu started tracking, applied the moment it does.
    queued: Vec<Fact>,
    cancel: CancelGathering,
}

thread_local! {
    /// Main-thread-only: it holds AppKit objects and Tauri menu handles, and every reader runs
    /// on the menu thread.
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}

/// Tells one right-click's late answers from the next one's.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// A fresh generation for the menu about to open.
pub fn next_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed) + 1
}

/// Where the gathering sends each fact that missed the grace period: onto the main queue,
/// tagged with its menu's generation.
pub fn late_sink(generation: u64) -> impl Fn(Fact) + Send + Sync + 'static {
    move |fact| DispatchQueue::main().exec_async(move || deliver(generation, fact))
}

/// The live menu, armed for the next context menu to open.
///
/// ❗ Hold it until `popup()` returns, like the other loans (`IconLoan`, `TagRowLoan`): the
/// late answers land while `popup()` runs AppKit's tracking loop. ❌ Never `let _ =`. Its
/// `Drop` forgets the menu, so a later answer goes nowhere, and cancels the facts still out.
pub struct LiveMenuLoan(MainThreadMarker);

impl Drop for LiveMenuLoan {
    fn drop(&mut self) {
        if let Some(live) = LIVE.with(|slot| slot.borrow_mut().take()) {
            live.cancel.cancel();
        }
    }
}

/// Arms `targets` for the menu about to `popup()`, so the late answers tagged `generation`
/// land on it. `None` off the main thread, where nothing could land anyway.
pub fn lend_live_menu<R: Runtime>(
    targets: LateTargets<R>,
    generation: u64,
    cancel: CancelGathering,
) -> Option<LiveMenuLoan> {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "menu", "Not on the main thread; the right-click menu can't fill in what arrives late");
        cancel.cancel();
        return None;
    };
    ensure_observing(mtm);
    LIVE.with(|slot| {
        *slot.borrow_mut() = Some(Live {
            generation,
            targets: Box::new(targets),
            root: None,
            queued: Vec::new(),
            cancel,
        });
    });
    Some(LiveMenuLoan(mtm))
}

fn ensure_observing(mtm: MainThreadMarker) {
    if OBSERVING.replace(true) {
        return;
    }
    observe_menu_tracking(
        mtm,
        "get the right-click menu ready for what arrives late",
        |mtm, note| {
            on_menu_did_begin_tracking(mtm, note);
        },
    );
}

/// Takes the live menu's `NSMenu`, hides the groups still waiting on their answers, and
/// applies whatever answered in between.
///
/// Acts on a ROOT menu only: a submenu opening posts its own notification.
fn on_menu_did_begin_tracking(mtm: MainThreadMarker, notification: &NSNotification) {
    let Some(menu) = tracking_menu(mtm, notification) else {
        return;
    };
    // SAFETY: `supermenu` is unsafe only because the reference is unretained; it is only
    // tested for presence here, inside this synchronous main-thread call.
    if unsafe { menu.supermenu() }.is_some() {
        return;
    }
    LIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(live) = slot.as_mut().filter(|live| live.root.is_none()) else {
            return;
        };
        live.targets.on_tracking(&menu);
        for fact in std::mem::take(&mut live.queued) {
            live.targets.apply(mtm, &menu, fact);
        }
        live.root = Some(menu);
    });
}

/// Lands one late answer on the main thread, if its menu is still the one up.
fn deliver(generation: u64, fact: Fact) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "menu", "A late right-click answer arrived off the main thread and was dropped");
        return;
    };
    // Called from GCD, where an escaping Rust panic or an ObjC raise would abort the app.
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
        objc2::exception::catch(AssertUnwindSafe(|| {
            LIVE.with(|slot| {
                let mut slot = slot.borrow_mut();
                let Some(live) = slot.as_mut().filter(|live| live.generation == generation) else {
                    log::debug!(target: "menu", "A late {:?} arrived after its right-click menu closed; dropped", fact.kind());
                    return;
                };
                log::debug!(target: "menu", "Late {:?} landed on the open right-click menu", fact.kind());
                match live.root.clone() {
                    Some(root) => live.targets.apply(mtm, &root, fact),
                    None => live.queued.push(fact),
                }
            });
        }))
    }));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::warn!(target: "menu", "Filling the right-click menu in raised: {e:?}"),
        Err(_) => log::warn!(target: "menu", "Filling the right-click menu in panicked"),
    }
}

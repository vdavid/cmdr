//! The macOS `Share` submenu: what macOS offers for a selection, and how to run one.
//!
//! Three steps, all reached from the file context menu. [`enumerate_offer`] asks macOS
//! which services it would offer for the right-clicked rows, on ANY thread: it reads each
//! file's attributes, over the network on a share, so it runs on the menu's fact pool
//! rather than the main thread (`menu/context_menu_facts.rs`). [`arm_offer`] then hands the
//! answer to the main thread for the menu that's up, and [`perform_offered`] runs the one
//! the user picked. AirDrop, Mail, Messages, Notes, and every installed share extension
//! come from the system; Cmdr only hands over the file URLs.
//!
//! ⚠️ Arming and performing are paired by INDEX through a main-thread thread-local, so
//! both take a `MainThreadMarker`: the offer is armed where the click will read it, and
//! `performWithItems:` puts UI on screen.
//!
//! Why hand-build rather than let AppKit draw it: `NSSharingServicePicker`'s
//! `standardShareMenuItem` is a plain action item, not a submenu — `hasSubmenu=false`,
//! `submenu=nil`, action `_performStandardShareMenuItem:` (measured on macOS 26.6.2,
//! 2026-09-09) — so it only opens the same popover it always did. `DETAILS.md` §
//! "The Share submenu" has the rest.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSImage, NSSharingService};
use objc2_foundation::{NSArray, NSString, NSURL};

/// Why a share didn't happen: a state the caller can log, never a message. One
/// variant, because the main-thread question is answered by the `MainThreadMarker`
/// both entry points take rather than by a refusal.
#[derive(Debug, PartialEq, Eq)]
pub enum ShareError {
    /// The click named a service the live offer doesn't have.
    NoSuchService,
}

/// One service, as a menu item needs it: no AppKit types, so it crosses into the
/// menu builder freely. Its icon stays with the live offer ([`offered_image`]).
#[derive(Clone)]
pub struct ShareService {
    /// What the item says. `menuItemTitle` is the service's own answer for exactly
    /// this use, and it's macOS copy in the system language, ❌ never ours to
    /// translate.
    pub title: String,
}

/// What macOS offers for one selection: the exact items it was asked about, the services it
/// answered with, and each one's title. Enumerated on a worker, armed on the main thread.
pub struct ShareOffer {
    items: Retained<NSArray>,
    services: Vec<Retained<NSSharingService>>,
    titles: Vec<ShareService>,
}

// SAFETY: a `ShareOffer` crosses threads exactly once, from the framework-pool worker that
// enumerated it to the main thread that arms it, and nothing touches it on the worker after
// the hand-off. What it holds is an `NSArray` of immutable `NSURL`s and `NSSharingService`s,
// whose retain and release are atomic; `sharingServicesForItems:` itself runs clean under
// Apple's Main Thread Checker off the main thread (Xcode's `libMainThreadChecker.dylib` over a
// Swift probe, macOS 27.0, 2026-09-24, with an `NSView` control run proving the checker live),
// and its answer matches the main thread's service for service.
unsafe impl Send for ShareOffer {}

impl ShareOffer {
    /// The services, one menu item each, in macOS's order. EMPTY means macOS offers nothing
    /// for this selection (a path that vanished, a broken symlink).
    pub fn services(&self) -> &[ShareService] {
        &self.titles
    }
}

/// Asks macOS which services it offers for `paths`, in its own order. `None` when no path
/// makes a file URL, so there is nothing to offer and nothing a click may find.
///
/// Runs on any thread, and belongs off the main one: the enumeration reads every file's
/// attributes (`getattrlist`), which is a network round trip on a share, and it waits on
/// ShareKit's own attribute store besides. A sample of the running app on an SMB share
/// (2026-09-23, five right-clicks) spent 2.1 s of main-thread time here. About 11 ms warm
/// and ~190 ms on the first call of the process on a local disk (macOS 26.6.2, 2026-09-09).
pub fn enumerate_offer(paths: &[PathBuf]) -> Option<ShareOffer> {
    let urls = file_urls(paths);
    if urls.is_empty() {
        return None;
    }
    let items = build_items(&urls);
    let services = enumerate(&items);
    let titles = services
        .iter()
        .map(|service| ShareService {
            title: service.menuItemTitle().to_string(),
        })
        .collect();
    Some(ShareOffer {
        items,
        services,
        titles,
    })
}

/// Makes `offer` the one a `Share` click performs from, replacing the last menu's. `None`
/// clears it, so a stale offer can't aim `Share` at a file the user has moved on from.
pub fn arm_offer(_mtm: MainThreadMarker, offer: Option<ShareOffer>) {
    OFFERED.with(|slot| {
        slot.replace(offer.map(|offer| Offered {
            items: offer.items,
            services: offer.services,
        }))
    });
}

/// The icon of the service at `index` in the live offer: macOS's own `NSImage`, which the
/// `Share` submenu puts beside its name (`menu/context_menu_icons.rs`). `None` when no offer
/// is armed or the index is past its end.
///
/// ⚠️ The image is shared with the system; copy it before resizing it.
pub fn offered_image(_mtm: MainThreadMarker, index: usize) -> Option<Retained<NSImage>> {
    offered(index).map(|(_, service)| service.image())
}

/// Runs the service at `index` in the live offer, on the very items it was
/// enumerated for.
///
/// ⚠️ This usually puts a window or sheet on screen, so it belongs one main-thread
/// turn AFTER the menu click rather than inside it (`menu_handlers.rs` hops for that
/// reason).
pub fn perform_offered(_mtm: MainThreadMarker, index: usize) -> Result<(), ShareError> {
    let (items, service) = offered(index).ok_or(ShareError::NoSuchService)?;
    // SAFETY: `performWithItems:` requires each element to conform to
    // `NSPasteboardWriting`, be an `NSItemProvider`, or be an `NSDocument`. Every
    // element of `items` is an `NSURL` (`build_items` builds it from nothing else),
    // which conforms to `NSPasteboardWriting`.
    unsafe { service.performWithItems(&items) };
    Ok(())
}

/// The live offer's service at `index`, together with the items it was enumerated
/// for. `None` when no offer is armed or the index is past its end.
fn offered(index: usize) -> Option<(Retained<NSArray>, Retained<NSSharingService>)> {
    OFFERED.with(|slot| {
        let borrowed = slot.borrow();
        let offer = borrowed.as_ref()?;
        Some((offer.items.clone(), offer.services.get(index)?.clone()))
    })
}

/// What the live offer holds: the exact `NSArray` the enumeration ran against, and
/// the services it answered with, so a click performs on the items macOS vetted
/// rather than on a fresh reading of the selection.
struct Offered {
    items: Retained<NSArray>,
    services: Vec<Retained<NSSharingService>>,
}

thread_local! {
    /// The offer behind the menu that's up, if any. Main-thread-only by
    /// construction: it holds `Retained` AppKit objects, and both the fill and the
    /// read take a `MainThreadMarker`.
    static OFFERED: RefCell<Option<Offered>> = const { RefCell::new(None) };
}

/// Asks macOS which services can take all of `items` together.
///
/// `sharingServicesForItems:` is the only API that hands over the list itself. Its
/// replacement (`standardShareMenuItem`) draws the system popover and nothing else,
/// so a submenu has no modern equivalent to reach for.
#[expect(
    deprecated,
    reason = "the only API that enumerates the services; its replacement draws the popover instead of listing them"
)]
fn enumerate(items: &NSArray) -> Vec<Retained<NSSharingService>> {
    // SAFETY: the generic precondition is that every element conforms to
    // `NSPasteboardWriting`; `build_items` puts only `NSURL`s in, which do.
    unsafe { NSSharingService::sharingServicesForItems(items) }.to_vec()
}

/// The selection as file URLs, dropping anything whose path isn't valid UTF-8.
///
/// A dropped path costs one item in the offer; the alternative (refusing the whole
/// share) costs the user the gesture.
fn file_urls(paths: &[PathBuf]) -> Vec<Retained<NSURL>> {
    paths.iter().filter_map(|path| file_url(path)).collect()
}

fn file_url(path: &Path) -> Option<Retained<NSURL>> {
    let path_str = path.to_str()?;
    Some(NSURL::fileURLWithPath(&NSString::from_str(path_str)))
}

/// Wraps the URLs in the type-erased `NSArray` both AppKit calls here want.
///
/// Their parameter is `NSArray *` with no element type, because they take anything
/// conforming to `NSPasteboardWriting`. `NSURL` is the right item for files and
/// folders, and it's what gives every service (AirDrop, Mail, Messages) the file
/// itself rather than a rendering of it.
fn build_items(urls: &[Retained<NSURL>]) -> Retained<NSArray> {
    let items: Vec<&AnyObject> = urls.iter().map(|url| AsRef::<AnyObject>::as_ref(&**url)).collect();
    NSArray::from_slice(&items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_isnt_utf8_is_skipped_rather_than_sinking_the_whole_share() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let broken = PathBuf::from(OsString::from_vec(vec![b'/', 0xff, 0xfe]));
        let urls = file_urls(&[PathBuf::from("/tmp/ok.txt"), broken]);
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn an_index_no_offer_backs_answers_nothing_instead_of_a_different_service() {
        // The menu ids ARE indices into the live offer, so a lookup that fell through
        // to a neighbour would share a file the user didn't pick. Two ways to miss:
        // no offer at all (a menu that outlived the one that filled it),
        assert!(offered(0).is_none(), "an unarmed offer has no service to perform");
        // and an index past a real offer's end.
        OFFERED.with(|slot| {
            slot.replace(Some(Offered {
                items: build_items(&[]),
                services: Vec::new(),
            }))
        });
        assert!(offered(0).is_none(), "an offer with no services has none at index 0");
        assert!(offered(7).is_none());
        OFFERED.with(|slot| slot.replace(None));
    }
}

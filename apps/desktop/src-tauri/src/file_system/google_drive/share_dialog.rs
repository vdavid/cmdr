//! "Share on Google Drive": opens Drive for desktop's own share dialog for one item.
//!
//! Drive declares Share as a File Provider custom action (`ACTION_SHARE`) in its
//! extension's `Info.plist`. Finder runs such an action through FileProvider.framework's
//! private host side, and so does this module:
//!
//! 1. `FPItemManager.defaultManager` resolves the path to an `FPItem`
//!    (`fetchItemForURL:completionHandler:`).
//! 2. [`offers_share`] applies Drive's own activation rule to the item's `userInfo`.
//! 3. A click builds an `FPVendorDefinedActionOperation` for `ACTION_SHARE` and hands it
//!    to `scheduleAction:`. `fileproviderd` forwards it to Drive's extension, and Drive
//!    opens the dialog in its own window.
//!
//! Verified on Drive for desktop 130.0, Darwin 25.6.0, with a private-API probe from an
//! unsigned, unentitled process, 2026-09-11. The two nearer-looking doors are shut:
//! `FPItemManager.operationForAction:items:` builds operations for the system actions
//! only (it throws "build your own ACTION_SHARE operation!"), and FileProviderUI's
//! `FPUIActionController` wants a UI extension Drive doesn't ship.
//!
//! **Stream mode only.** A mirrored file isn't a File Provider item (the fetch answers
//! nil), and Drive's Finder menu there comes from its Finder Sync extension, which only
//! Finder can host. No item, no menu entry.
//!
//! **Private API, so everything fails closed.** The framework, both classes, and every
//! selector are looked up at runtime, every call runs inside `objc2::exception::catch`,
//! and every wait is bounded. A macOS or Drive update that moves any of it takes the
//! menu item away; it never crashes.

use std::ffi::CStr;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSString, NSURL};

const LOG_TARGET: &str = "google_drive";

/// Drive for desktop's File Provider extension, as the item's `providerID` names it.
const DRIVE_PROVIDER_ID: &str = "com.google.drivefs.fpext";

/// The custom action Drive's extension declares for its Finder "Share" entry.
const SHARE_ACTION_ID: &str = "ACTION_SHARE";

const FILE_PROVIDER_FRAMEWORK: &CStr = c"/System/Library/Frameworks/FileProvider.framework/FileProvider";

/// How long a click waits for File Provider to resolve the item. The user already
/// asked, so this can be generous; the menu's own wait is the caller's.
const CLICK_TIMEOUT: Duration = Duration::from_secs(5);

/// What the share gate reads off a File Provider item.
///
/// The flags stay `Option` because Drive's rule compares against `YES` / `NO`, and a
/// missing key matches neither.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ItemFacts {
    provider_id: String,
    action_share: Option<bool>,
    shared_drive_root: Option<bool>,
}

/// File Provider's own names for an item, which is what an action is addressed to.
#[derive(Debug)]
struct ItemAddress {
    provider_domain_id: String,
    item_identifier: String,
}

/// One fetched item: what the gate reads, plus the address a click needs.
#[derive(Debug)]
struct FetchedItem {
    facts: ItemFacts,
    address: Option<ItemAddress>,
}

/// Why a click didn't open the dialog. Logged, since the user sees only nothing happening.
#[derive(Debug)]
enum ShareError {
    /// FileProvider.framework, `FPItemManager`, or `FPVendorDefinedActionOperation`
    /// is missing, or no longer has the selectors this module calls.
    ApiUnavailable,
    /// File Provider didn't resolve the path to an item in time.
    NoItem,
    /// Drive's rule doesn't offer Share for this item (any more).
    NotOffered,
    /// An Objective-C exception came out of a private call.
    Exception,
}

/// Whether Drive's own `SHARE_WITH_DRIVE` action applies to this item.
///
/// Mirrors the activation rule in Drive's extension `Info.plist`:
/// `$item.userInfo.ACTION_SHARE == YES AND $item.userInfo.SHARED_DRIVE_ROOT == NO`.
/// A shared drive's root gets Drive's member-management entries instead, which this
/// menu doesn't offer. The rule's `fileproviderItems.@count == 1` is the caller's.
fn offers_share(facts: &ItemFacts) -> bool {
    facts.provider_id == DRIVE_PROVIDER_ID && facts.action_share == Some(true) && facts.shared_drive_root == Some(false)
}

/// Whether the context menu offers "Share on Google Drive" for `path`.
///
/// Waits at most `timeout` for File Provider; no answer in time means no.
pub fn can_share(path: &Path, timeout: Duration) -> bool {
    fetch_item(path, timeout).is_some_and(|item| offers_share(&item.facts))
}

/// Opens Drive's share dialog for `path`, off the calling thread.
///
/// A plain named thread rather than `sync_status/pool.rs`: every File Provider call
/// here is asynchronous, so the thread only waits on a channel with a deadline and
/// always exits. Nothing in it can wedge.
pub fn open_share_dialog(path: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("drive-share".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || match share(&path) {
            Ok(()) => log::debug!(target: LOG_TARGET, "Share on Google Drive scheduled for {path:?}"),
            Err(reason) => {
                log::warn!(target: LOG_TARGET, "Share on Google Drive didn't start for {path:?}: {reason:?}")
            }
        });
    if let Err(e) = spawned {
        log::warn!(target: LOG_TARGET, "Share on Google Drive: couldn't start its thread: {e}");
    }
}

fn share(path: &Path) -> Result<(), ShareError> {
    let item = fetch_item(path, CLICK_TIMEOUT).ok_or(ShareError::NoItem)?;
    if !offers_share(&item.facts) {
        return Err(ShareError::NotOffered);
    }
    let address = item.address.ok_or(ShareError::NoItem)?;
    schedule_share(&address)
}

/// Loads FileProvider.framework once. Cmdr doesn't link it, and its private classes
/// only resolve by name once it's loaded.
fn file_provider_loaded() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| {
        // SAFETY: `dlopen` is thread-safe and the `c""` literal is NUL-terminated. The handle
        // is never closed on purpose: the classes it registers have to outlive every call here.
        let handle = unsafe { libc::dlopen(FILE_PROVIDER_FRAMEWORK.as_ptr(), libc::RTLD_NOW) };
        !handle.is_null()
    })
}

/// `FPItemManager.defaultManager`, when the class still has every selector used here.
fn item_manager() -> Option<Retained<AnyObject>> {
    if !file_provider_loaded() {
        return None;
    }
    let class = AnyClass::get(c"FPItemManager")?;
    let shaped = class.metaclass().responds_to(sel!(defaultManager))
        && class.responds_to(sel!(fetchItemForURL:completionHandler:))
        && class.responds_to(sel!(scheduleAction:));
    if !shaped {
        return None;
    }
    // SAFETY: `+defaultManager` exists on the class (checked above), takes no arguments, and
    // returns an object; a nil answer becomes `None`.
    unsafe { msg_send![class, defaultManager] }
}

/// Resolves `path` to a File Provider item, waiting at most `timeout`.
fn fetch_item(path: &Path, timeout: Duration) -> Option<FetchedItem> {
    let (tx, rx) = mpsc::channel::<Option<FetchedItem>>();
    let started = objc2::exception::catch(AssertUnwindSafe(|| {
        let manager = item_manager()?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
        let completion = RcBlock::new(move |item: *mut AnyObject, _error: *mut NSError| {
            // SAFETY: File Provider passes nil or an `FPItem` it keeps alive for the block's call.
            let fetched = unsafe { item.as_ref() }.and_then(|item| {
                objc2::exception::catch(AssertUnwindSafe(|| read_item(item)))
                    .ok()
                    .flatten()
            });
            // A failed send means the caller stopped waiting (its timeout), which it already handles.
            let _ = tx.send(fetched);
        });
        // SAFETY: `fetchItemForURL:completionHandler:` exists on the manager's class (checked in
        // `item_manager`). `url` is a live `NSURL`, and the block matches the handler's
        // `(FPItem *, NSError *)` shape. File Provider copies the block, so ours can drop after.
        let () = unsafe { msg_send![&*manager, fetchItemForURL: &*url, completionHandler: &*completion] };
        Some(())
    }));
    match started {
        Ok(Some(())) => rx.recv_timeout(timeout).ok().flatten(),
        Ok(None) => None,
        Err(exception) => {
            log::debug!(target: LOG_TARGET, "File Provider item fetch threw: {exception:?}");
            None
        }
    }
}

/// Reads the facts and address off a live `FPItem`. Called inside `exception::catch`.
fn read_item(item: &AnyObject) -> Option<FetchedItem> {
    let class = item.class();
    if !class.responds_to(sel!(providerID)) || !class.responds_to(sel!(userInfo)) {
        return None;
    }
    // SAFETY: `providerID` exists on the item's class (checked above) and returns an object or nil.
    let provider_id: Option<Retained<AnyObject>> = unsafe { msg_send![item, providerID] };
    // SAFETY: `userInfo` exists on the item's class (checked above) and returns an object or nil.
    let user_info: Option<Retained<AnyObject>> = unsafe { msg_send![item, userInfo] };
    let user_info = user_info
        .as_deref()
        .and_then(|info| info.downcast_ref::<NSDictionary>());
    let facts = ItemFacts {
        provider_id: string_of(provider_id.as_deref()).unwrap_or_default(),
        action_share: user_info.and_then(|info| flag(info, "ACTION_SHARE")),
        shared_drive_root: user_info.and_then(|info| flag(info, "SHARED_DRIVE_ROOT")),
    };

    let address = if class.responds_to(sel!(providerDomainID)) && class.responds_to(sel!(itemIdentifier)) {
        // SAFETY: `providerDomainID` exists on the item's class (checked above) and returns an object or nil.
        let domain: Option<Retained<AnyObject>> = unsafe { msg_send![item, providerDomainID] };
        // SAFETY: `itemIdentifier` exists on the item's class (checked above) and returns an object or nil.
        let identifier: Option<Retained<AnyObject>> = unsafe { msg_send![item, itemIdentifier] };
        string_of(domain.as_deref()).zip(string_of(identifier.as_deref())).map(
            |(provider_domain_id, item_identifier)| ItemAddress {
                provider_domain_id,
                item_identifier,
            },
        )
    } else {
        None
    };
    Some(FetchedItem { facts, address })
}

fn string_of(object: Option<&AnyObject>) -> Option<String> {
    object?.downcast_ref::<NSString>().map(NSString::to_string)
}

/// A `userInfo` flag as Drive writes it: an `NSNumber` 0 or 1. Anything else is missing.
fn flag(user_info: &NSDictionary, key: &str) -> Option<bool> {
    let value = user_info.objectForKey(&NSString::from_str(key))?;
    value.downcast_ref::<NSNumber>().map(NSNumber::boolValue)
}

/// Hands `ACTION_SHARE` for `address` to File Provider.
///
/// Returns once the operation is scheduled. The operation's completion block isn't
/// used: its signature is private, and guessing an argument we'd dereference could
/// crash the app over a dialog that shows for itself anyway.
fn schedule_share(address: &ItemAddress) -> Result<(), ShareError> {
    let scheduled = objc2::exception::catch(AssertUnwindSafe(|| -> Result<(), ShareError> {
        let manager = item_manager().ok_or(ShareError::ApiUnavailable)?;
        let class = AnyClass::get(c"FPVendorDefinedActionOperation").ok_or(ShareError::ApiUnavailable)?;
        if !class.responds_to(sel!(initWithActionIdentifier:providerDomainID:itemIdentifiers:)) {
            return Err(ShareError::ApiUnavailable);
        }
        let action = NSString::from_str(SHARE_ACTION_ID);
        let domain = NSString::from_str(&address.provider_domain_id);
        let identifiers = NSArray::from_retained_slice(&[NSString::from_str(&address.item_identifier)]);
        // SAFETY: `+alloc` is NSObject's, sent to a class that exists (looked up above).
        let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        // SAFETY: the initializer exists on the class (checked above) and takes three objects,
        // all live for the call: the action id, the provider domain id, and the item ids. A nil
        // answer becomes `None`.
        let operation: Option<Retained<AnyObject>> = unsafe {
            msg_send![allocated, initWithActionIdentifier: &*action, providerDomainID: &*domain, itemIdentifiers: &*identifiers]
        };
        let operation = operation.ok_or(ShareError::ApiUnavailable)?;
        // SAFETY: `scheduleAction:` exists on the manager's class (checked in `item_manager`) and
        // takes the operation, which is live for the call.
        let () = unsafe { msg_send![&*manager, scheduleAction: &*operation] };
        Ok(())
    }));
    scheduled.unwrap_or(Err(ShareError::Exception))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shareable() -> ItemFacts {
        ItemFacts {
            provider_id: DRIVE_PROVIDER_ID.to_string(),
            action_share: Some(true),
            shared_drive_root: Some(false),
        }
    }

    #[test]
    fn a_plain_drive_item_offers_share() {
        assert!(offers_share(&shareable()));
    }

    #[test]
    fn a_shared_drive_root_offers_no_share() {
        let facts = ItemFacts {
            shared_drive_root: Some(true),
            ..shareable()
        };
        assert!(!offers_share(&facts));
    }

    #[test]
    fn an_item_drive_says_cannot_be_shared_offers_no_share() {
        let facts = ItemFacts {
            action_share: Some(false),
            ..shareable()
        };
        assert!(!offers_share(&facts));
    }

    #[test]
    fn another_providers_item_offers_no_share() {
        let facts = ItemFacts {
            provider_id: "com.getdropbox.dropbox.fileprovider".to_string(),
            ..shareable()
        };
        assert!(!offers_share(&facts));
    }

    /// Drive's predicate compares the flags against `YES` and `NO`, and a missing key
    /// matches neither, so either key missing means Drive's own menu wouldn't show it.
    #[test]
    fn a_missing_flag_offers_no_share() {
        let no_action = ItemFacts {
            action_share: None,
            ..shareable()
        };
        let no_root_flag = ItemFacts {
            shared_drive_root: None,
            ..shareable()
        };
        assert!(!offers_share(&no_action));
        assert!(!offers_share(&no_root_flag));
    }
}

//! Spotlight's record of an item's use: `kMDItemLastUsedDate` and
//! `kMDItemUseCount`, from `MDItemCopyAttribute`.
//!
//! A Firefox download has no date, and a file an app just wrote has one equal to
//! its last change (both verified on macOS 27.0.1 with `mdls`, 2026-11-13).
//! `cmdr_about::payload::usage_fact` reads both as `NoRecord`, ❌ never "never
//! opened" and ❌ never "opened when it was saved".

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::date::CFDateGetAbsoluteTime;
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::payload::{Usage, usage_fact};

/// Seconds between the CF epoch (2001-01-01) and the Unix epoch.
const CF_TO_UNIX_EPOCH: f64 = 978_307_200.0;

#[repr(C)]
struct OpaqueMdItem(std::ffi::c_void);
type MdItemRef = *mut OpaqueMdItem;

// SAFETY: the CoreServices MDItem C signatures. `MDItemCreate` and
// `MDItemCopyAttribute` both return +1 references the caller releases; both take a
// null allocator.
#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn MDItemCreate(allocator: CFTypeRef, path: CFStringRef) -> MdItemRef;
    fn MDItemCopyAttribute(item: MdItemRef, name: CFStringRef) -> CFTypeRef;
}

pub(crate) fn last_opened(path: &str) -> Fact<Usage> {
    objc2::rc::autoreleasepool(|_| {
        let cf_path = CFString::new(path);
        // SAFETY: `cf_path` is live for the call; a null allocator is the default.
        let item = unsafe { MDItemCreate(std::ptr::null(), cf_path.as_concrete_TypeRef()) };
        if item.is_null() {
            return Fact::unknown(UnknownWhy::NoRecord);
        }
        let last_used = copy_attribute(item, "kMDItemLastUsedDate").map(|value| {
            // SAFETY: `kMDItemLastUsedDate` is documented as a CFDate, and `value` is the
            // live +1 reference `copy_attribute` returned, released right after.
            let absolute = unsafe { CFDateGetAbsoluteTime(value as _) };
            // SAFETY: balances `MDItemCopyAttribute`'s +1.
            unsafe { CFRelease(value) };
            absolute + CF_TO_UNIX_EPOCH
        });
        let use_count = copy_attribute(item, "kMDItemUseCount").and_then(|value| {
            // SAFETY: `kMDItemUseCount` is a CFNumber; the get rule wraps it without
            // taking ownership, and the release below balances the +1.
            let count = unsafe { CFNumber::wrap_under_get_rule(value as CFNumberRef) }.to_i64();
            // SAFETY: balances `MDItemCopyAttribute`'s +1.
            unsafe { CFRelease(value) };
            count.and_then(|count| u64::try_from(count).ok())
        });
        // SAFETY: `item` is the non-null +1 reference from `MDItemCreate`, not used after.
        unsafe { CFRelease(item as CFTypeRef) };
        let last_used = last_used
            .filter(|seconds| seconds.is_finite())
            .map(|seconds| seconds as i64);
        usage_fact(last_used, use_count, modified(path))
    })
}

/// The item's own modification date, which tells a save's stamp from an open.
fn modified(path: &str) -> Option<i64> {
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    let seconds = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(seconds).ok()
}

/// One attribute's +1 reference, or `None` when Spotlight keeps none.
fn copy_attribute(item: MdItemRef, name: &'static str) -> Option<CFTypeRef> {
    let name = CFString::from_static_string(name);
    // SAFETY: `item` is a live MDItemRef the caller holds; `name` is live for the call.
    let value = unsafe { MDItemCopyAttribute(item, name.as_concrete_TypeRef()) };
    (!value.is_null()).then_some(value)
}

//! The CFPreferences boundary: one array-valued key in another app's preferences domain, read and
//! written as `plist::Value` so everything above this file is plain data.
//!
//! ❌ **Never read or write `~/Library/Preferences/com.apple.dock.plist` directly.** `cfprefsd`
//! owns that file and keeps an in-memory cache in front of it, so a direct read can be stale and a
//! direct write gets clobbered the next time the daemon flushes. Every access here goes through
//! CFPreferences, which is the same door `defaults` uses.
//!
//! The domain is addressed fully-qualified (current user, any host) rather than through the
//! `…AppValue…` shorthand, so the read describes exactly the array the write will replace.
//! `CFPreferencesCopyAppValue` would also fold in managed and any-user layers, which is the right
//! answer to "what does the Dock see" and the wrong one to "what am I about to overwrite";
//! [`is_forced`] asks the managed question separately.

use objc2_core_foundation::{
    CFPreferencesAppSynchronize, CFPreferencesAppValueIsForced, CFPreferencesCopyValue, CFPreferencesSetValue,
    CFPropertyList, CFRetained, CFString, kCFPreferencesAnyHost, kCFPreferencesCurrentUser,
};

use crate::cf_plist;

/// What can go wrong reaching a preferences domain. Each variant is a distinct thing the caller
/// can say to a person; ❌ never a message to match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrefsError {
    /// The key isn't in the domain at all.
    Missing,
    /// The key is there but doesn't hold an array.
    NotAnArray,
    /// Core Foundation and `plist` disagreed about the value. Shouldn't happen: both speak the
    /// same binary plist.
    Unreadable,
    /// The new value couldn't be turned into a property list to hand to CFPreferences.
    Unencodable,
    /// The write went in but `cfprefsd` didn't confirm the flush, so nothing can be assumed to
    /// have landed.
    NotFlushed,
}

/// Reads the array at `key` in `domain`.
pub(super) fn read_array(domain: &str, key: &str) -> Result<Vec<plist::Value>, PrefsError> {
    let key = CFString::from_str(key);
    let domain = CFString::from_str(domain);
    // SAFETY: the four CFPreferences domain constants are `&'static CFString` globals exported by
    // CoreFoundation; reading them is unsafe only because they live in an `extern "C"` block.
    let (user, host) = unsafe { (kCFPreferencesCurrentUser, kCFPreferencesAnyHost) };

    let value = CFPreferencesCopyValue(&key, &domain, user, host).ok_or(PrefsError::Missing)?;

    match to_plist(&value)? {
        plist::Value::Array(entries) => Ok(entries),
        _ => Err(PrefsError::NotAnArray),
    }
}

/// Replaces the array at `key` in `domain` and flushes it to `cfprefsd`.
///
/// The flush is what makes the value visible to another process, so a failed synchronize is a
/// failed write however well the set went.
pub(super) fn write_array(domain: &str, key: &str, entries: &[plist::Value]) -> Result<(), PrefsError> {
    let value = from_plist(&plist::Value::Array(entries.to_vec()))?;
    let key = CFString::from_str(key);
    let domain_string = CFString::from_str(domain);
    // SAFETY: as in `read_array`, plus: `CFPreferencesSetValue` is unsafe because the value has to
    // be a real property list, and `from_plist` only ever returns one CoreFoundation itself built.
    unsafe {
        let (user, host) = (kCFPreferencesCurrentUser, kCFPreferencesAnyHost);
        CFPreferencesSetValue(&key, Some(&value), &domain_string, user, host);
    }

    if CFPreferencesAppSynchronize(&domain_string) {
        Ok(())
    } else {
        Err(PrefsError::NotFlushed)
    }
}

/// Whether `key` in `domain` is managed by a configuration profile, which makes it read-only.
///
/// An MDM-managed Dock silently swallows writes, so this turns "nothing happened" into something
/// the caller can say out loud.
pub(super) fn is_forced(domain: &str, key: &str) -> bool {
    CFPreferencesAppValueIsForced(&CFString::from_str(key), &CFString::from_str(domain))
}

/// A Core Foundation property list as a `plist::Value`.
fn to_plist(value: &CFPropertyList) -> Result<plist::Value, PrefsError> {
    cf_plist::to_plist(value).ok_or(PrefsError::Unreadable)
}

/// A `plist::Value` as a Core Foundation property list, the same way round.
fn from_plist(value: &plist::Value) -> Result<CFRetained<CFPropertyList>, PrefsError> {
    cf_plist::from_plist(value).ok_or(PrefsError::Unencodable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cf_plist::ScratchDomain;
    use crate::dock::entries::{PERSISTENT_APPS_KEY, app_tile};
    use std::path::Path;

    /// The Dock's real domain, read-only. Reading is safe; writing here from a test is not.
    const DOCK_DOMAIN: &str = "com.apple.dock";

    /// A per-test scratch domain. ❌ Never point a test at `com.apple.dock`: a test run would
    /// rearrange the machine's Dock.
    fn scratch(tag: &str) -> ScratchDomain {
        ScratchDomain::new(&format!("com.getcmdr.docktest.{tag}"))
    }

    #[test]
    fn an_array_survives_the_round_trip_through_cfprefsd() {
        let scratch = scratch("roundtrip");
        let key = "entries";

        let written = vec![
            app_tile(Path::new("/Applications/Cmdr.app"), Some("com.x.cmdr"), 7),
            app_tile(Path::new("/Applications/Google Chrome.app"), None, 8),
        ];
        write_array(scratch.domain(), key, &written).expect("write the scratch array");

        let read_back = read_array(scratch.domain(), key).expect("read the scratch array");
        assert_eq!(read_back, written, "cfprefsd must hand back what we gave it");

        scratch.set(key, None);
    }

    #[test]
    fn a_key_that_was_never_written_reads_as_missing() {
        let scratch = scratch("missing");

        assert_eq!(
            read_array(scratch.domain(), "aKeyNobodyEverWrote"),
            Err(PrefsError::Missing)
        );
    }

    #[test]
    fn a_key_holding_something_other_than_an_array_is_typed_as_such() {
        let scratch = scratch("notanarray");
        let key = "entries";
        let value = from_plist(&plist::Value::String("nope".to_string())).expect("encode");

        scratch.set(key, Some(&value));

        assert_eq!(read_array(scratch.domain(), key), Err(PrefsError::NotAnArray));

        scratch.set(key, None);
    }

    #[test]
    fn the_real_dock_domain_is_readable_without_a_permission_prompt() {
        // Read-only, and the load-bearing half of the TCC question: if reading another app's
        // domain needed consent, this would hang or come back empty. ❌ Never write here.
        let entries =
            read_array(DOCK_DOMAIN, PERSISTENT_APPS_KEY).expect("the Dock always has a persistent-apps array");

        assert!(
            entries.iter().all(|e| e.as_dictionary().is_some()),
            "every persistent-apps entry is a dictionary"
        );
    }

    #[test]
    fn the_dock_is_not_managed_by_a_configuration_profile_here() {
        // Documents the shape of the managed check as much as it asserts anything: on a machine
        // with an MDM Dock payload this would be `true` and the pin would be refused.
        assert!(!is_forced(DOCK_DOMAIN, PERSISTENT_APPS_KEY));
    }
}

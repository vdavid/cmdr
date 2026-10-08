//! Core Foundation property lists as `plist::Value`, so every CFPreferences reader above this file
//! (`dock/prefs.rs`, `managed_policy/source.rs`) handles plain data.
//!
//! Both directions go through the binary plist both sides speak, which keeps the conversion total:
//! anything CFPreferences can hold, `plist` can represent, and the other way round.

use objc2_core_foundation::{
    CFData, CFPropertyList, CFPropertyListCreateData, CFPropertyListCreateWithData, CFPropertyListFormat, CFRetained,
};

/// A Core Foundation property list as a `plist::Value`. `None` when Core Foundation and `plist`
/// disagree about the value, which shouldn't happen.
pub(crate) fn to_plist(value: &CFPropertyList) -> Option<plist::Value> {
    // SAFETY: `value` is a live property list (callers get it from CFPreferences or from
    // `from_plist`); the null error pointer is allowed and means "don't hand me a CFError".
    let data = unsafe {
        CFPropertyListCreateData(
            None,
            Some(value),
            CFPropertyListFormat::BinaryFormat_v1_0,
            0,
            std::ptr::null_mut(),
        )
    }?;

    plist::Value::from_reader(std::io::Cursor::new(data.to_vec())).ok()
}

/// A `plist::Value` as a Core Foundation property list. `None` when it can't be encoded.
pub(crate) fn from_plist(value: &plist::Value) -> Option<CFRetained<CFPropertyList>> {
    let mut encoded = Vec::new();
    value.to_writer_binary(&mut encoded).ok()?;
    let data = CFData::from_bytes(&encoded);

    // SAFETY: `data` is a live CFData holding a well-formed binary plist; both out-parameters are
    // null, which CFPropertyListCreateWithData documents as "don't report the format or the error".
    unsafe { CFPropertyListCreateWithData(None, Some(&data), 0, std::ptr::null_mut(), std::ptr::null_mut()) }
}

/// A preferences domain that belongs to nobody, so a test can write to it freely.
///
/// One domain per test, so the teardown can delete the whole thing without racing a sibling test
/// writing to it (nextest runs each test in its own process, concurrently). ❌ Never point one at a
/// real app's domain: a test run would rewrite that app's preferences.
#[cfg(test)]
pub(crate) struct ScratchDomain(String);

#[cfg(test)]
impl ScratchDomain {
    /// `domain` must be unique to the test, like `com.getcmdr.docktest.roundtrip`.
    pub(crate) fn new(domain: &str) -> Self {
        Self(domain.to_string())
    }

    pub(crate) fn domain(&self) -> &str {
        &self.0
    }

    /// Writes a raw value under `key` in the CURRENT USER's layer (what `defaults write` does) and
    /// flushes it; `None` removes the key.
    pub(crate) fn set(&self, key: &str, value: Option<&CFPropertyList>) {
        use objc2_core_foundation::{
            CFPreferencesAppSynchronize, CFPreferencesSetValue, CFString, kCFPreferencesAnyHost,
            kCFPreferencesCurrentUser,
        };

        let key = CFString::from_str(key);
        let domain = CFString::from_str(&self.0);
        // SAFETY: the CFPreferences domain constants are `&'static CFString` globals exported by
        // CoreFoundation, and `value` is a real property list (or `None`, which removes the key).
        unsafe {
            let (user, host) = (kCFPreferencesCurrentUser, kCFPreferencesAnyHost);
            CFPreferencesSetValue(&key, value, &domain, user, host);
        }
        assert!(CFPreferencesAppSynchronize(&domain), "flush the scratch domain");
    }
}

#[cfg(test)]
impl Drop for ScratchDomain {
    /// Takes the whole domain with it, so a test run leaves nothing in `~/Library/Preferences`.
    ///
    /// Both steps are needed, in this order (measured on macOS 26, 2026-09-09): `defaults delete`
    /// drops the domain from `cfprefsd`'s memory but leaves an empty plist on disk, and unlinking on
    /// its own loses a race with the daemon, which still holds the domain and writes the file
    /// straight back out.
    fn drop(&mut self) {
        let _ = std::process::Command::new("/usr/bin/defaults")
            .args(["delete", &self.0])
            .output();
        if let Some(home) = dirs::home_dir() {
            let _ = std::fs::remove_file(home.join("Library/Preferences").join(format!("{}.plist", self.0)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_survives_the_round_trip_through_core_foundation() {
        let mut dict = plist::Dictionary::new();
        dict.insert("flag".into(), plist::Value::Boolean(true));
        dict.insert("count".into(), plist::Value::Integer(7.into()));
        dict.insert(
            "hosts".into(),
            plist::Value::Array(vec![plist::Value::String("api.openai.com".into())]),
        );
        let value = plist::Value::Dictionary(dict);

        let cf = from_plist(&value).expect("encode");
        assert_eq!(to_plist(&cf), Some(value));
    }
}

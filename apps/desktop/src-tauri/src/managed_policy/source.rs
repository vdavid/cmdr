//! Where forced values come from. Everything above this file sees `plist::Value`s keyed by name and
//! never touches CFPreferences.

/// A source of FORCED preference values: what a configuration profile put in place, never the
/// user's own `defaults write`.
pub trait ManagedPrefsSource {
    /// The forced value under `key`, or `None` when nothing forces it.
    fn forced_value(&self, key: &str) -> Option<plist::Value>;

    /// Every key the source can name, for the "unknown key" debug line. Sources that can't list
    /// their managed layer (CFPreferences) return nothing, which only loses that log line.
    fn listed_keys(&self) -> Vec<String> {
        Vec::new()
    }
}

/// A value that CFPreferences reports as forced but that won't convert to a `plist::Value`. It's
/// the wrong type for every key, so each key reads it as its most restrictive value (rule 4: a
/// value we can't read never turns a restriction off).
#[cfg(target_os = "macos")]
fn unreadable_forced_value() -> plist::Value {
    plist::Value::Data(Vec::new())
}

/// The real thing on macOS: the app's domain as `cfprefsd` merges it, filtered to forced keys.
#[cfg(target_os = "macos")]
pub struct CfPrefsSource {
    domain: objc2_core_foundation::CFRetained<objc2_core_foundation::CFString>,
}

#[cfg(target_os = "macos")]
impl CfPrefsSource {
    /// Opens `domain` for one read pass. ❗ Synchronizes first, or a profile installed while Cmdr
    /// runs stays invisible to this process (the same gotcha as `glass_tint.rs`).
    pub fn synchronized(domain: &str) -> Self {
        use objc2_core_foundation::{CFPreferencesAppSynchronize, CFString};

        let domain = CFString::from_str(domain);
        if !CFPreferencesAppSynchronize(&domain) {
            log::debug!(target: "managed_policy", "CFPreferencesAppSynchronize returned false; reading the cached domain");
        }
        Self { domain }
    }
}

#[cfg(target_os = "macos")]
impl ManagedPrefsSource for CfPrefsSource {
    fn forced_value(&self, key: &str) -> Option<plist::Value> {
        use objc2_core_foundation::{CFPreferencesAppValueIsForced, CFPreferencesCopyAppValue, CFString};

        let key = CFString::from_str(key);
        // ❗ `IsForced` FIRST: `CopyAppValue` merges the user's own layer too, and a user's
        // `defaults write` must never read as policy.
        if !CFPreferencesAppValueIsForced(&key, &self.domain) {
            return None;
        }
        let value = CFPreferencesCopyAppValue(&key, &self.domain)?;
        Some(crate::cf_plist::to_plist(&value).unwrap_or_else(unreadable_forced_value))
    }
}

/// Test builds only: a plist file standing in for the whole managed layer, named by
/// `CMDR_MANAGED_PREFS_FILE`. Every key in it counts as forced. ❌ Never compiled into a plain
/// release build: it would let anyone replace IT's policy.
#[cfg(any(debug_assertions, feature = "playwright-e2e", test))]
pub struct PlistFileSource {
    values: plist::Dictionary,
}

#[cfg(any(debug_assertions, feature = "playwright-e2e", test))]
impl PlistFileSource {
    /// Reads `path` now. A missing file is an empty policy, quietly: it's how an E2E run starts,
    /// unmanaged until a spec writes one. A malformed one reads as empty too, with a warning.
    pub fn read(path: &std::path::Path) -> Self {
        if !path.exists() {
            log::debug!(target: "managed_policy", "{} doesn't exist; reading no policy", path.display());
            return Self {
                values: plist::Dictionary::new(),
            };
        }
        let values = match plist::Value::from_file(path) {
            Ok(plist::Value::Dictionary(values)) => values,
            Ok(_) => {
                log::warn!(target: "managed_policy", "{} isn't a plist dictionary; reading no policy", path.display());
                plist::Dictionary::new()
            }
            Err(e) => {
                log::warn!(target: "managed_policy", "Couldn't read {}: {e}; reading no policy", path.display());
                plist::Dictionary::new()
            }
        };
        Self { values }
    }
}

#[cfg(any(debug_assertions, feature = "playwright-e2e", test))]
impl ManagedPrefsSource for PlistFileSource {
    fn forced_value(&self, key: &str) -> Option<plist::Value> {
        self.values.get(key).cloned()
    }

    fn listed_keys(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }
}

/// Platforms without a managed-preferences mechanism (yet): no restriction.
#[cfg(not(target_os = "macos"))]
pub struct NoManagedPrefs;

#[cfg(not(target_os = "macos"))]
impl ManagedPrefsSource for NoManagedPrefs {
    fn forced_value(&self, _key: &str) -> Option<plist::Value> {
        None
    }
}

/// An in-memory source for unit tests: every value counts as forced.
#[cfg(test)]
#[derive(Default)]
pub struct FakeSource(plist::Dictionary);

#[cfg(test)]
impl FakeSource {
    pub fn with(entries: &[(&str, plist::Value)]) -> Self {
        Self(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
    }
}

#[cfg(test)]
impl ManagedPrefsSource for FakeSource {
    fn forced_value(&self, key: &str) -> Option<plist::Value> {
        self.0.get(key).cloned()
    }

    fn listed_keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::cf_plist::{ScratchDomain, from_plist};
    use crate::managed_policy::keys::{self, DISABLE_USAGE_STATS};

    #[test]
    fn a_value_in_the_users_own_layer_is_not_policy() {
        let scratch = ScratchDomain::new("com.getcmdr.policytest.userlayer");
        let value = from_plist(&plist::Value::Boolean(true)).expect("encode");
        scratch.set(DISABLE_USAGE_STATS, Some(&value));

        // The user layer really holds it: `CopyAppValue` (which merges every layer) sees it.
        let key = objc2_core_foundation::CFString::from_str(DISABLE_USAGE_STATS);
        let domain = objc2_core_foundation::CFString::from_str(scratch.domain());
        assert!(
            objc2_core_foundation::CFPreferencesCopyAppValue(&key, &domain).is_some(),
            "the scratch write must be visible, or this test proves nothing"
        );

        let source = CfPrefsSource::synchronized(scratch.domain());
        assert_eq!(source.forced_value(DISABLE_USAGE_STATS), None);
        assert!(!keys::parse(&source).policy.is_managed());
    }

    #[test]
    fn a_key_nobody_wrote_reads_as_unforced() {
        let scratch = ScratchDomain::new("com.getcmdr.policytest.empty");
        let source = CfPrefsSource::synchronized(scratch.domain());
        assert_eq!(source.forced_value(DISABLE_USAGE_STATS), None);
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;
    use crate::test_support::TestDir;

    #[test]
    fn a_plist_file_round_trips_into_forced_values() {
        let dir = TestDir::new("managed-prefs-file");
        let path = dir.join("policy.plist");
        let mut dict = plist::Dictionary::new();
        dict.insert("DisableUpdates".into(), plist::Value::Boolean(true));
        dict.insert("MaxUpdateVersion".into(), plist::Value::String("0.52".into()));
        plist::Value::Dictionary(dict)
            .to_file_xml(&path)
            .expect("write the test plist");

        let source = PlistFileSource::read(&path);
        assert_eq!(source.forced_value("DisableUpdates"), Some(plist::Value::Boolean(true)));
        assert_eq!(
            source.forced_value("MaxUpdateVersion"),
            Some(plist::Value::String("0.52".into()))
        );
        assert_eq!(source.forced_value("DisableAI"), None);
    }

    #[test]
    fn a_missing_or_malformed_file_reads_as_no_policy() {
        let dir = TestDir::new("managed-prefs-bad");
        let missing = PlistFileSource::read(&dir.join("nope.plist"));
        assert!(missing.listed_keys().is_empty());

        let garbage = dir.join("garbage.plist");
        std::fs::write(&garbage, "not a plist").expect("write");
        assert!(PlistFileSource::read(&garbage).listed_keys().is_empty());
    }
}

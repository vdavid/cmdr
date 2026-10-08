//! The drift guard: the sample profile, the bare preference file, and the `/trust` key list are the
//! public mirror of `keys.rs`, so a key added here can't be missing there (or the other way round),
//! and every example value an admin starts from parses without a warning.

use std::collections::BTreeSet;

use super::keys::{self, ALL_KEYS};
use super::source::FakeSource;

const PREFERENCE_FILE: &str = include_str!("../../../../website/public/mdm/com.veszelovszki.cmdr.plist");
const PROFILE: &str = include_str!("../../../../website/public/mdm/cmdr-managed-preferences.mobileconfig");
const TRUST_TS: &str = include_str!("../../../../website/src/lib/trust.ts");

fn expected_keys() -> BTreeSet<String> {
    ALL_KEYS.iter().map(ToString::to_string).collect()
}

pub(super) fn read_dictionary(text: &str, what: &str) -> plist::Dictionary {
    plist::Value::from_reader_xml(text.as_bytes())
        .unwrap_or_else(|e| panic!("{what} isn't a valid plist: {e}"))
        .into_dictionary()
        .unwrap_or_else(|| panic!("{what} isn't a dictionary at the top"))
}

/// The keys of the profile's one Cmdr payload, minus the `Payload*` bookkeeping every payload has.
fn profile_payload() -> plist::Dictionary {
    let profile = read_dictionary(PROFILE, "the sample profile");
    let payloads = profile
        .get("PayloadContent")
        .and_then(plist::Value::as_array)
        .expect("the profile has a PayloadContent array");
    let mut cmdr: Vec<&plist::Dictionary> = payloads
        .iter()
        .filter_map(plist::Value::as_dictionary)
        .filter(|p| p.get("PayloadType").and_then(plist::Value::as_string) == Some(crate::config::BUNDLE_ID))
        .collect();
    assert_eq!(
        cmdr.len(),
        1,
        "the profile needs exactly one payload of type {}",
        crate::config::BUNDLE_ID
    );
    cmdr.remove(0)
        .iter()
        .filter(|(key, _)| !key.starts_with("Payload"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// The `key: '…'` entries of `managedPreferenceKeys` in `trust.ts`.
fn trust_page_keys() -> BTreeSet<String> {
    let start = TRUST_TS
        .find("export const managedPreferenceKeys")
        .expect("trust.ts exports managedPreferenceKeys");
    let rest = &TRUST_TS[start..];
    let block = &rest[..rest.find("\n]").expect("managedPreferenceKeys ends with a `]` line")];
    block
        .lines()
        .filter_map(|line| line.trim().strip_prefix("key: '"))
        .map(|tail| tail.split('\'').next().unwrap_or_default().to_string())
        .collect()
}

fn assert_parses_cleanly(dictionary: &plist::Dictionary, what: &str) {
    let entries: Vec<(&str, plist::Value)> = dictionary
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    let parsed = keys::parse(&FakeSource::with(&entries));
    assert!(
        parsed.warnings.is_empty(),
        "{what} has an example value Cmdr can't read: {:?}",
        parsed.warnings
    );
}

#[test]
fn the_preference_file_carries_every_key_and_nothing_else() {
    let dictionary = read_dictionary(PREFERENCE_FILE, "the preference file");
    assert_eq!(dictionary.keys().cloned().collect::<BTreeSet<_>>(), expected_keys());
    assert_parses_cleanly(&dictionary, "the preference file");
}

#[test]
fn the_sample_profile_carries_every_key_and_nothing_else() {
    let payload = profile_payload();
    assert_eq!(payload.keys().cloned().collect::<BTreeSet<_>>(), expected_keys());
    assert_parses_cleanly(&payload, "the sample profile");
}

#[test]
fn the_profile_and_the_preference_file_agree_on_every_value() {
    assert_eq!(
        profile_payload(),
        read_dictionary(PREFERENCE_FILE, "the preference file")
    );
}

#[test]
fn the_trust_page_lists_every_key_and_nothing_else() {
    assert_eq!(trust_page_keys(), expected_keys());
}

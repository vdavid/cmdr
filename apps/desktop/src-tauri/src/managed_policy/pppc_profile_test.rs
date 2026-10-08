//! The Full Disk Access (PPPC) profile names Cmdr by bundle id and code requirement, and `/trust`
//! publishes that requirement. Both have to match what the release build signs with, or a profile
//! an admin already deployed stops matching after a signing change and Cmdr silently loses Full
//! Disk Access on every managed Mac. So the expected requirement is built here from the release
//! config (`tauri.conf.json`'s bundle id and the Team ID in its signing identity), and both copies
//! must equal it. The requirement's shape is Apple's for a Developer ID app, read from a shipped
//! build with `codesign -d -r-`.

use super::public_docs_test::read_dictionary;

const PROFILE: &str = include_str!("../../../../website/public/mdm/cmdr-full-disk-access.mobileconfig");
const PREFERENCES_PROFILE: &str = include_str!("../../../../website/public/mdm/cmdr-managed-preferences.mobileconfig");
const TRUST_TS: &str = include_str!("../../../../website/src/lib/trust.ts");
const TAURI_CONF: &str = include_str!("../../tauri.conf.json");

const TCC_PAYLOAD_TYPE: &str = "com.apple.TCC.configuration-profile-policy";

/// The designated requirement a Developer ID build of this bundle id and team gets.
fn expected_requirement() -> String {
    let conf: serde_json::Value = serde_json::from_str(TAURI_CONF).expect("tauri.conf.json is JSON");
    let bundle_id = conf["identifier"].as_str().expect("tauri.conf.json has an identifier");
    assert_eq!(bundle_id, crate::config::BUNDLE_ID);
    let identity = conf["bundle"]["macOS"]["signingIdentity"]
        .as_str()
        .expect("tauri.conf.json has bundle.macOS.signingIdentity");
    assert!(
        identity.starts_with("Developer ID Application: "),
        "the release signs with a Developer ID Application identity, not {identity}"
    );
    let team_id = identity
        .rsplit_once('(')
        .and_then(|(_, tail)| tail.strip_suffix(')'))
        .expect("the signing identity ends with (TEAMID)");
    format!(
        "identifier \"{bundle_id}\" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = \"{team_id}\""
    )
}

/// The `codeRequirement` string `trust.ts` exports (single-quoted, on the line after its name).
fn trust_page_requirement() -> String {
    let start = TRUST_TS
        .find("export const codeRequirement =")
        .expect("trust.ts exports codeRequirement");
    let rest = &TRUST_TS[start..];
    let open = rest.find('\'').expect("codeRequirement is a single-quoted string") + 1;
    let len = rest[open..].find('\'').expect("codeRequirement's string ends");
    rest[open..open + len].to_string()
}

/// Every entry of every service in the profile's one TCC payload, with the service name.
fn profile_entries() -> Vec<(String, plist::Dictionary)> {
    let profile = read_dictionary(PROFILE, "the Full Disk Access profile");
    let payloads = profile
        .get("PayloadContent")
        .and_then(plist::Value::as_array)
        .expect("the profile has a PayloadContent array");
    assert_eq!(payloads.len(), 1, "the profile has exactly one payload");
    let payload = payloads[0].as_dictionary().expect("the payload is a dictionary");
    assert_eq!(
        payload.get("PayloadType").and_then(plist::Value::as_string),
        Some(TCC_PAYLOAD_TYPE)
    );
    let services = payload
        .get("Services")
        .and_then(plist::Value::as_dictionary)
        .expect("the TCC payload has a Services dictionary");
    services
        .iter()
        .flat_map(|(service, entries)| {
            entries
                .as_array()
                .unwrap_or_else(|| panic!("{service} isn't an array"))
                .iter()
                .map(move |entry| {
                    let entry = entry
                        .as_dictionary()
                        .unwrap_or_else(|| panic!("an entry of {service} isn't a dictionary"));
                    (service.clone(), entry.clone())
                })
        })
        .collect()
}

fn string<'a>(entry: &'a plist::Dictionary, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(plist::Value::as_string)
}

#[test]
fn the_trust_page_publishes_the_release_requirement() {
    assert_eq!(trust_page_requirement(), expected_requirement());
}

#[test]
fn every_grant_in_the_profile_names_the_release_build() {
    let entries = profile_entries();
    assert!(
        entries.iter().any(|(service, _)| service == "SystemPolicyAllFiles"),
        "the profile grants Full Disk Access"
    );
    let expected = expected_requirement();
    for (service, entry) in &entries {
        assert_eq!(string(entry, "Identifier"), Some(crate::config::BUNDLE_ID), "{service}");
        assert_eq!(string(entry, "IdentifierType"), Some("bundleID"), "{service}");
        assert_eq!(string(entry, "CodeRequirement"), Some(expected.as_str()), "{service}");
        // `Allowed`, not `Authorization`: the latter needs macOS 11, and Cmdr runs on 10.15.
        assert_eq!(
            entry.get("Allowed").and_then(plist::Value::as_boolean),
            Some(true),
            "{service}"
        );
        assert!(!entry.contains_key("Authorization"), "{service}");
    }
}

/// An MDM tells profiles apart by these, so the two published profiles must not share any.
#[test]
fn the_two_profiles_have_distinct_identifiers() {
    fn ids(text: &str, what: &str) -> Vec<String> {
        let profile = read_dictionary(text, what);
        let mut ids = vec![profile.clone()];
        ids.extend(
            profile
                .get("PayloadContent")
                .and_then(plist::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(plist::Value::as_dictionary)
                .cloned(),
        );
        ids.iter()
            .flat_map(|d| [string(d, "PayloadIdentifier"), string(d, "PayloadUUID")])
            .map(|id| {
                id.unwrap_or_else(|| panic!("{what} has a payload without an identifier or UUID"))
                    .to_string()
            })
            .collect()
    }
    let fda = ids(PROFILE, "the Full Disk Access profile");
    let prefs = ids(PREFERENCES_PROFILE, "the managed preferences profile");
    for id in &fda {
        assert!(!prefs.contains(id), "both profiles use {id}");
    }
    let unique: std::collections::BTreeSet<_> = fda.iter().collect();
    assert_eq!(
        unique.len(),
        fda.len(),
        "the Full Disk Access profile reuses an identifier"
    );
}

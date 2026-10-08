//! The key names and the one pure parse from a source to a [`ManagedPolicy`]. The ONLY place a key
//! name is spelled in Rust. Parse rules: `DETAILS.md` § Value parsing.

use super::source::ManagedPrefsSource;
use super::{HostPattern, ManagedPolicy, UpdateCeiling};

pub const DISABLE_USAGE_STATS: &str = "DisableUsageStats";
pub const DISABLE_CRASH_AND_ERROR_REPORTS: &str = "DisableCrashAndErrorReports";
pub const DISABLE_AUTOMATIC_UPDATE_CHECKS: &str = "DisableAutomaticUpdateChecks";
pub const DISABLE_UPDATES: &str = "DisableUpdates";
pub const MAX_UPDATE_VERSION: &str = "MaxUpdateVersion";
pub const DISABLE_AI: &str = "DisableAI";
pub const DISABLE_CLOUD_AI: &str = "DisableCloudAI";
pub const ALLOWED_CLOUD_AI_HOSTS: &str = "AllowedCloudAIHosts";

/// Every key Cmdr reads. The drift guard compares the sample profile and `/trust` against it.
pub const ALL_KEYS: [&str; 8] = [
    DISABLE_USAGE_STATS,
    DISABLE_CRASH_AND_ERROR_REPORTS,
    DISABLE_AUTOMATIC_UPDATE_CHECKS,
    DISABLE_UPDATES,
    MAX_UPDATE_VERSION,
    DISABLE_AI,
    DISABLE_CLOUD_AI,
    ALLOWED_CLOUD_AI_HOSTS,
];

/// A parse result: the policy, plus one line per value we couldn't read as written. The cache logs
/// the warnings when they change, so a bad profile warns once, not on every read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub policy: ManagedPolicy,
    pub warnings: Vec<String>,
}

/// Reads every key from `source`. Never fails: a value we can't read takes that key's most
/// restrictive reading (rule 4).
pub fn parse(source: &dyn ManagedPrefsSource) -> Parsed {
    let mut warnings = Vec::new();
    let mut flag = |key: &str| {
        source
            .forced_value(key)
            .is_some_and(|value| read_bool(key, &value, &mut warnings))
    };

    let usage_stats_disabled = flag(DISABLE_USAGE_STATS);
    let reports_disabled = flag(DISABLE_CRASH_AND_ERROR_REPORTS);
    let automatic_update_checks_disabled = flag(DISABLE_AUTOMATIC_UPDATE_CHECKS);
    let mut updates_disabled = flag(DISABLE_UPDATES);
    let ai_disabled = flag(DISABLE_AI);
    let cloud_ai_disabled = flag(DISABLE_CLOUD_AI);

    let update_ceiling = match source.forced_value(MAX_UPDATE_VERSION) {
        None => None,
        Some(value) => {
            let ceiling = UpdateCeiling::from_plist(&value);
            if ceiling.is_none() {
                warnings.push(format!(
                    "{MAX_UPDATE_VERSION} is {value:?}, which isn't a version like \"0.52\"; turning updates off"
                ));
                updates_disabled = true;
            }
            ceiling
        }
    };

    let allowed_cloud_ai_hosts = source
        .forced_value(ALLOWED_CLOUD_AI_HOSTS)
        .map(|value| read_hosts(&value, &mut warnings));

    for key in source.listed_keys() {
        if !ALL_KEYS.contains(&key.as_str()) {
            log::debug!(target: "managed_policy", "Ignoring the unknown managed key {key:?}");
        }
    }

    Parsed {
        policy: ManagedPolicy {
            usage_stats_disabled,
            reports_disabled,
            automatic_update_checks_disabled,
            updates_disabled,
            update_ceiling,
            ai_disabled,
            cloud_ai_disabled,
            allowed_cloud_ai_hosts,
        },
        warnings,
    }
}

/// A bool the way admins write one: `<true/>`, an integer (`0` is false), or one of the strings
/// `true`/`false`/`yes`/`no`/`1`/`0` in any case (`defaults write … Key true` without `-bool`
/// stores a string). Anything else restricts (`true`) and warns.
fn read_bool(key: &str, value: &plist::Value, warnings: &mut Vec<String>) -> bool {
    let read = match value {
        plist::Value::Boolean(b) => Some(*b),
        plist::Value::Integer(n) => Some(n.as_signed().is_none_or(|n| n != 0)),
        plist::Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    };
    read.unwrap_or_else(|| {
        warnings.push(format!(
            "{key} is {value:?}, which isn't a yes or no; reading it as yes"
        ));
        true
    })
}

/// The host list: valid entries kept, malformed ones dropped with a warning. A value that isn't an
/// array reads as an empty list, which allows no host.
fn read_hosts(value: &plist::Value, warnings: &mut Vec<String>) -> Vec<HostPattern> {
    let Some(entries) = value.as_array() else {
        warnings.push(format!(
            "{ALLOWED_CLOUD_AI_HOSTS} is {value:?}, which isn't a list; allowing no cloud AI host"
        ));
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let pattern = entry.as_string().and_then(HostPattern::parse);
            if pattern.is_none() {
                warnings.push(format!(
                    "{ALLOWED_CLOUD_AI_HOSTS} entry {entry:?} isn't a host, `host:port`, `*.domain`, or URL; dropping it"
                ));
            }
            pattern
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::source::FakeSource;
    use crate::managed_policy::{AiPolicy, UpdatePolicy};
    use plist::Value;

    fn parse_one(key: &str, value: Value) -> Parsed {
        parse(&FakeSource::with(&[(key, value)]))
    }

    fn string(text: &str) -> Value {
        Value::String(text.into())
    }

    fn int(n: i64) -> Value {
        Value::Integer(n.into())
    }

    fn strings(entries: &[&str]) -> Value {
        Value::Array(entries.iter().map(|e| string(e)).collect())
    }

    #[test]
    fn no_keys_is_no_restriction() {
        let parsed = parse(&FakeSource::default());
        assert_eq!(parsed.policy, ManagedPolicy::default());
        assert!(!parsed.policy.is_managed());
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn a_forced_true_restricts_and_a_forced_false_doesnt() {
        let on = parse_one(DISABLE_USAGE_STATS, Value::Boolean(true));
        assert!(on.policy.usage_stats_disabled());
        assert!(on.policy.is_managed());

        let off = parse_one(DISABLE_USAGE_STATS, Value::Boolean(false));
        assert!(!off.policy.usage_stats_disabled());
        assert!(!off.policy.is_managed(), "a forced false is no restriction");
        assert!(off.warnings.is_empty());
    }

    #[test]
    fn bools_accept_strings_and_integers() {
        for (value, expected) in [
            (string("true"), true),
            (string("YES"), true),
            (string("1"), true),
            (string(" Yes "), true),
            (string("false"), false),
            (string("No"), false),
            (string("0"), false),
            (int(0), false),
            (int(1), true),
            (int(2), true),
        ] {
            let parsed = parse_one(DISABLE_CRASH_AND_ERROR_REPORTS, value.clone());
            assert_eq!(parsed.policy.reports_disabled(), expected, "{value:?}");
            assert!(parsed.warnings.is_empty(), "{value:?} is a valid bool");
        }
    }

    #[test]
    fn an_unreadable_bool_restricts_and_warns() {
        for value in [
            string("maybe"),
            string(""),
            Value::Dictionary(plist::Dictionary::new()),
            Value::Real(0.0),
            Value::Data(Vec::new()),
        ] {
            let parsed = parse_one(DISABLE_AI, value.clone());
            assert_eq!(parsed.policy.ai(), AiPolicy::Off, "{value:?} must read as restrictive");
            assert_eq!(parsed.warnings.len(), 1, "{value:?} must warn");
        }
    }

    #[test]
    fn every_bool_key_maps_to_its_answer() {
        assert!(
            parse_one(DISABLE_USAGE_STATS, Value::Boolean(true))
                .policy
                .usage_stats_disabled()
        );
        assert!(
            parse_one(DISABLE_CRASH_AND_ERROR_REPORTS, Value::Boolean(true))
                .policy
                .reports_disabled()
        );
        assert_eq!(
            parse_one(DISABLE_AUTOMATIC_UPDATE_CHECKS, Value::Boolean(true))
                .policy
                .updates(),
            UpdatePolicy::Enabled {
                automatic_checks: false,
                ceiling: None
            }
        );
        assert_eq!(
            parse_one(DISABLE_UPDATES, Value::Boolean(true)).policy.updates(),
            UpdatePolicy::Disabled
        );
        assert_eq!(parse_one(DISABLE_AI, Value::Boolean(true)).policy.ai(), AiPolicy::Off);
        assert_eq!(
            parse_one(DISABLE_CLOUD_AI, Value::Boolean(true)).policy.ai(),
            AiPolicy::LocalOnly
        );
    }

    #[test]
    fn a_ceiling_parses_from_a_string_or_an_integer() {
        let cases = [
            (string("0.52"), UpdateCeiling::Minor(0, 52)),
            (string("0.52.3"), UpdateCeiling::Patch(0, 52, 3)),
            (string("1"), UpdateCeiling::Major(1)),
            (string("v0.52"), UpdateCeiling::Minor(0, 52)),
            (int(1), UpdateCeiling::Major(1)),
        ];
        for (value, ceiling) in cases {
            let parsed = parse_one(MAX_UPDATE_VERSION, value.clone());
            assert_eq!(
                parsed.policy.updates(),
                UpdatePolicy::Enabled {
                    automatic_checks: true,
                    ceiling: Some(ceiling)
                },
                "{value:?}"
            );
            assert!(parsed.warnings.is_empty());
        }
    }

    #[test]
    fn an_unreadable_ceiling_disables_updates() {
        for value in [
            string(""),
            string("latest"),
            string("0.52.3.1"),
            Value::Real(0.52),
            int(-1),
            Value::Boolean(true),
        ] {
            let parsed = parse_one(MAX_UPDATE_VERSION, value.clone());
            assert_eq!(parsed.policy.updates(), UpdatePolicy::Disabled, "{value:?}");
            assert_eq!(parsed.warnings.len(), 1, "{value:?} must warn");
        }
    }

    #[test]
    fn a_ceiling_and_no_automatic_checks_combine() {
        let parsed = parse(&FakeSource::with(&[
            (MAX_UPDATE_VERSION, string("0.52")),
            (DISABLE_AUTOMATIC_UPDATE_CHECKS, Value::Boolean(true)),
        ]));
        assert_eq!(
            parsed.policy.updates(),
            UpdatePolicy::Enabled {
                automatic_checks: false,
                ceiling: Some(UpdateCeiling::Minor(0, 52))
            }
        );
    }

    #[test]
    fn disable_updates_overrides_the_other_update_keys() {
        let parsed = parse(&FakeSource::with(&[
            (MAX_UPDATE_VERSION, string("0.52")),
            (DISABLE_AUTOMATIC_UPDATE_CHECKS, Value::Boolean(false)),
            (DISABLE_UPDATES, Value::Boolean(true)),
        ]));
        assert_eq!(parsed.policy.updates(), UpdatePolicy::Disabled);
    }

    #[test]
    fn a_host_list_keeps_valid_entries_and_drops_malformed_ones_with_a_warning() {
        let parsed = parse_one(
            ALLOWED_CLOUD_AI_HOSTS,
            Value::Array(vec![
                string("api.openai.com"),
                string("*"),
                int(7),
                string("*.openai.azure.com"),
            ]),
        );
        assert_eq!(parsed.policy.ai(), AiPolicy::Allowed);
        assert_eq!(parsed.warnings.len(), 2, "{:?}", parsed.warnings);
        let hosts = parsed.policy.allowed_cloud_ai_hosts.as_deref().expect("a host list");
        assert_eq!(
            hosts.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["api.openai.com", "*.openai.azure.com"]
        );
    }

    #[test]
    fn an_empty_or_non_array_host_list_means_on_device_only() {
        assert_eq!(
            parse_one(ALLOWED_CLOUD_AI_HOSTS, strings(&[])).policy.ai(),
            AiPolicy::LocalOnly
        );
        let not_an_array = parse_one(ALLOWED_CLOUD_AI_HOSTS, string("api.openai.com"));
        assert_eq!(not_an_array.policy.ai(), AiPolicy::LocalOnly);
        assert_eq!(not_an_array.warnings.len(), 1);
        assert_eq!(
            parse_one(ALLOWED_CLOUD_AI_HOSTS, strings(&["*", ""])).policy.ai(),
            AiPolicy::LocalOnly,
            "a list whose every entry is malformed allows nothing"
        );
    }

    #[test]
    fn ai_keys_follow_their_precedence() {
        let all = parse(&FakeSource::with(&[
            (DISABLE_AI, Value::Boolean(true)),
            (DISABLE_CLOUD_AI, Value::Boolean(true)),
            (ALLOWED_CLOUD_AI_HOSTS, strings(&["api.openai.com"])),
        ]));
        assert_eq!(all.policy.ai(), AiPolicy::Off);

        let cloud_and_hosts = parse(&FakeSource::with(&[
            (DISABLE_CLOUD_AI, Value::Boolean(true)),
            (ALLOWED_CLOUD_AI_HOSTS, strings(&["api.openai.com"])),
        ]));
        assert_eq!(cloud_and_hosts.policy.ai(), AiPolicy::LocalOnly);
    }

    #[test]
    fn an_unknown_key_is_ignored() {
        let parsed = parse(&FakeSource::with(&[
            ("DisableTimeTravel", Value::Boolean(true)),
            (DISABLE_UPDATES, Value::Boolean(true)),
        ]));
        assert_eq!(parsed.policy.updates(), UpdatePolicy::Disabled);
        assert!(
            parsed.warnings.is_empty(),
            "an unknown key is a debug line, not a warning"
        );
    }
}

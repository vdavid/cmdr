//! The ONE mapping from policy to settings-registry ids. The frontend overlay reads
//! [`locked_settings`] and MCP `set_setting` asks [`refuses_write`]; backend readers of a raw
//! `settings.json` map go through [`overlay`], its Rust twin.

use serde::{Deserialize, Serialize};

use super::{AiPolicy, ManagedPolicy, UpdatePolicy};

const ANALYTICS_ENABLED: &str = "analytics.enabled";
const CRASH_REPORTS: &str = "updates.crashReports";
const ERROR_REPORTS: &str = "updates.errorReports";
const AUTO_CHECK: &str = "updates.autoCheck";
const AI_PROVIDER: &str = "ai.provider";
const ASK_CMDR_ENABLED: &str = "askCmdr.enabled";

/// A setting value the policy pins. Typed rather than `serde_json::Value`, which can't cross IPC
/// (`src/lib/ipc/CLAUDE.md`); crosses as a plain `boolean | string`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub enum LockedValue {
    Bool(bool),
    Text(String),
}

impl From<&LockedValue> for serde_json::Value {
    fn from(value: &LockedValue) -> Self {
        match value {
            LockedValue::Bool(b) => Self::Bool(*b),
            LockedValue::Text(s) => Self::String(s.clone()),
        }
    }
}

/// How the policy constrains one setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SettingLock {
    /// The setting reads as `value`, whatever is stored.
    Fixed { value: LockedValue },
    /// The setting can't hold any of `values`; a stored one reads as `fallback`.
    DisallowedValues {
        values: Vec<LockedValue>,
        fallback: LockedValue,
    },
}

/// One setting the organization manages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LockedSetting {
    /// The settings-registry id, like `analytics.enabled`.
    pub id: String,
    pub lock: SettingLock,
}

/// Every setting `policy` locks, in a stable order. A key forced to its permissive value locks
/// nothing.
pub fn locked_settings(policy: &ManagedPolicy) -> Vec<LockedSetting> {
    let off = |id: &str| LockedSetting {
        id: id.to_string(),
        lock: SettingLock::Fixed {
            value: LockedValue::Bool(false),
        },
    };
    let provider = |value: &str| LockedValue::Text(value.to_string());

    let mut locked = Vec::new();
    if policy.usage_stats_disabled() {
        locked.push(off(ANALYTICS_ENABLED));
    }
    if policy.reports_disabled() {
        locked.extend([off(CRASH_REPORTS), off(ERROR_REPORTS)]);
    }
    let automatic_checks = matches!(
        policy.updates(),
        UpdatePolicy::Enabled {
            automatic_checks: true,
            ..
        }
    );
    if !automatic_checks {
        locked.push(off(AUTO_CHECK));
    }
    match policy.ai() {
        AiPolicy::Allowed => {}
        // Ask Cmdr runs on the local model too, so only `Off` touches it.
        AiPolicy::LocalOnly => locked.push(LockedSetting {
            id: AI_PROVIDER.to_string(),
            lock: SettingLock::DisallowedValues {
                values: vec![provider("cloud")],
                // ❌ Never `local`: that would start a multi-GB model download nobody asked for.
                fallback: provider("off"),
            },
        }),
        AiPolicy::Off => locked.extend([
            LockedSetting {
                id: AI_PROVIDER.to_string(),
                lock: SettingLock::Fixed { value: provider("off") },
            },
            off(ASK_CMDR_ENABLED),
        ]),
    }
    locked
}

/// Whether `policy` refuses writing `value` to setting `id`: a `Fixed` lock refuses every write, a
/// `DisallowedValues` lock only its own values. MCP `set_setting` asks this before the round trip.
pub fn refuses_write(policy: &ManagedPolicy, id: &str, value: &serde_json::Value) -> bool {
    locked_settings(policy)
        .into_iter()
        .filter(|locked| locked.id == id)
        .any(|locked| match locked.lock {
            SettingLock::Fixed { .. } => true,
            SettingLock::DisallowedValues { values, .. } => values.iter().any(|v| serde_json::Value::from(v) == *value),
        })
}

/// Applies `policy`'s locks to a raw `settings.json` map in memory, so a backend reader sees the
/// EFFECTIVE values. Pure: ❌ never write the result back (the policy overlays, it never rewrites).
///
/// A missing or unparseable file (anything but a JSON object) becomes an object holding just the
/// pinned values when something is locked: its reader would otherwise fall back to the setting's
/// default, which for `analytics.enabled` is on.
pub fn overlay(policy: &ManagedPolicy, settings: &mut serde_json::Value) {
    let locked = locked_settings(policy);
    if locked.is_empty() {
        return;
    }
    if !settings.is_object() {
        *settings = serde_json::Value::Object(serde_json::Map::new());
    }
    let Some(map) = settings.as_object_mut() else {
        return;
    };
    for LockedSetting { id, lock } in locked {
        match lock {
            SettingLock::Fixed { value } => {
                map.insert(id, (&value).into());
            }
            SettingLock::DisallowedValues { values, fallback } => {
                let disallowed = map
                    .get(&id)
                    .is_some_and(|stored| values.iter().any(|v| serde_json::Value::from(v) == *stored));
                if disallowed {
                    map.insert(id, (&fallback).into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::UpdateCeiling;
    use serde_json::json;

    fn fixed_bool(id: &str, value: bool) -> LockedSetting {
        LockedSetting {
            id: id.into(),
            lock: SettingLock::Fixed {
                value: LockedValue::Bool(value),
            },
        }
    }

    #[test]
    fn no_policy_locks_nothing() {
        assert_eq!(locked_settings(&ManagedPolicy::default()), vec![]);
    }

    #[test]
    fn telemetry_keys_lock_their_switches_off() {
        let policy = ManagedPolicy {
            usage_stats_disabled: true,
            reports_disabled: true,
            ..Default::default()
        };
        assert_eq!(
            locked_settings(&policy),
            vec![
                fixed_bool(ANALYTICS_ENABLED, false),
                fixed_bool(CRASH_REPORTS, false),
                fixed_bool(ERROR_REPORTS, false),
            ]
        );
    }

    #[test]
    fn both_update_keys_lock_background_checks_off_and_a_ceiling_alone_locks_nothing() {
        for policy in [
            ManagedPolicy {
                automatic_update_checks_disabled: true,
                ..Default::default()
            },
            ManagedPolicy {
                updates_disabled: true,
                ..Default::default()
            },
        ] {
            assert_eq!(locked_settings(&policy), vec![fixed_bool(AUTO_CHECK, false)]);
        }
        let ceiling_only = ManagedPolicy {
            update_ceiling: Some(UpdateCeiling::Minor(0, 52)),
            ..Default::default()
        };
        assert_eq!(locked_settings(&ceiling_only), vec![]);
    }

    #[test]
    fn ai_off_pins_the_provider_and_ask_cmdr() {
        let policy = ManagedPolicy {
            ai_disabled: true,
            cloud_ai_disabled: true,
            ..Default::default()
        };
        assert_eq!(
            locked_settings(&policy),
            vec![
                LockedSetting {
                    id: AI_PROVIDER.into(),
                    lock: SettingLock::Fixed {
                        value: LockedValue::Text("off".into())
                    },
                },
                fixed_bool(ASK_CMDR_ENABLED, false),
            ]
        );
    }

    #[test]
    fn local_only_disallows_the_cloud_provider_and_leaves_ask_cmdr_alone() {
        let policy = ManagedPolicy {
            cloud_ai_disabled: true,
            ..Default::default()
        };
        assert_eq!(
            locked_settings(&policy),
            vec![LockedSetting {
                id: AI_PROVIDER.into(),
                lock: SettingLock::DisallowedValues {
                    values: vec![LockedValue::Text("cloud".into())],
                    fallback: LockedValue::Text("off".into()),
                },
            }]
        );
    }

    #[test]
    fn a_host_list_alone_locks_no_setting() {
        let policy = ManagedPolicy {
            allowed_cloud_ai_hosts: Some(vec![super::super::HostPattern::parse("api.openai.com").expect("parse")]),
            ..Default::default()
        };
        assert_eq!(locked_settings(&policy), vec![]);
    }

    #[test]
    fn the_wire_shape_is_camel_case_with_plain_values() {
        let json = serde_json::to_value(LockedSetting {
            id: AI_PROVIDER.into(),
            lock: SettingLock::DisallowedValues {
                values: vec![LockedValue::Text("cloud".into())],
                fallback: LockedValue::Text("off".into()),
            },
        })
        .expect("serialize");
        assert_eq!(
            json,
            json!({ "id": "ai.provider", "lock": { "kind": "disallowedValues", "values": ["cloud"], "fallback": "off" } })
        );
    }

    #[test]
    fn a_fixed_lock_refuses_every_write_and_a_disallow_lock_only_its_values() {
        let off = ManagedPolicy {
            usage_stats_disabled: true,
            ..Default::default()
        };
        assert!(refuses_write(&off, "analytics.enabled", &json!(false)));
        assert!(refuses_write(&off, "analytics.enabled", &json!(true)));
        assert!(!refuses_write(&off, "updates.crashReports", &json!(true)));

        let local_only = ManagedPolicy {
            cloud_ai_disabled: true,
            ..Default::default()
        };
        assert!(refuses_write(&local_only, "ai.provider", &json!("cloud")));
        assert!(!refuses_write(&local_only, "ai.provider", &json!("local")));
        assert!(!refuses_write(
            &ManagedPolicy::default(),
            "ai.provider",
            &json!("cloud")
        ));
    }

    #[test]
    fn overlay_pins_locked_values_and_leaves_the_rest_alone() {
        let policy = ManagedPolicy {
            usage_stats_disabled: true,
            cloud_ai_disabled: true,
            ..Default::default()
        };
        let mut settings = json!({
            "analytics.enabled": true,
            "ai.provider": "cloud",
            "updates.crashReports": true,
            "appearance.theme": "dark",
        });
        overlay(&policy, &mut settings);
        assert_eq!(
            settings,
            json!({
                "analytics.enabled": false,
                "ai.provider": "off",
                "updates.crashReports": true,
                "appearance.theme": "dark",
            })
        );
    }

    #[test]
    fn overlay_keeps_an_allowed_value_under_a_disallow_lock() {
        let policy = ManagedPolicy {
            cloud_ai_disabled: true,
            ..Default::default()
        };
        let mut settings = json!({ "ai.provider": "local" });
        overlay(&policy, &mut settings);
        assert_eq!(settings, json!({ "ai.provider": "local" }));
    }

    #[test]
    fn overlay_without_a_policy_changes_nothing_even_for_a_missing_file() {
        let mut missing = serde_json::Value::Null;
        overlay(&ManagedPolicy::default(), &mut missing);
        assert_eq!(missing, serde_json::Value::Null);

        let mut settings = json!({ "analytics.enabled": true });
        overlay(&ManagedPolicy::default(), &mut settings);
        assert_eq!(settings, json!({ "analytics.enabled": true }));
    }

    #[test]
    fn overlay_on_a_missing_file_still_reports_the_pinned_values() {
        // A reader that falls back to a default on a missing key would otherwise see the
        // default (`analytics.enabled` defaults to on), not the policy.
        let policy = ManagedPolicy {
            usage_stats_disabled: true,
            ..Default::default()
        };
        let mut missing = serde_json::Value::Null;
        overlay(&policy, &mut missing);
        assert_eq!(missing, json!({ "analytics.enabled": false }));
    }
}

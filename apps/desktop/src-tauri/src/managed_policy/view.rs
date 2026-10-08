//! The one payload the frontend gets: `get_managed_policy` returns it and `managed-policy-changed`
//! carries it, so the frontend never derives the lock list or the summary itself.

use serde::{Deserialize, Serialize};

use super::{AiPolicy, LockedSetting, ManagedPolicy, UpdatePolicy, locked_settings};

/// Everything the UI shows about the policy: per-row locks, feature-level states, and the "what
/// your organization manages" summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManagedPolicyView {
    /// Whether any key restricts anything.
    pub managed: bool,
    pub usage_stats_disabled: bool,
    pub reports_disabled: bool,
    pub updates: UpdatePolicyView,
    pub ai: AiPolicyView,
    pub locked_settings: Vec<LockedSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpdatePolicyView {
    Disabled,
    Enabled {
        automatic_checks: bool,
        /// The canonical spelling, like `0.52`. `null` when there's no ceiling.
        ceiling: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiPolicyView {
    pub mode: AiPolicy,
    /// The hosts cloud AI may reach, normalized (`api.openai.com`, `*.openai.azure.com`,
    /// `localhost:11434`). `null` when any host goes, or when cloud AI is off altogether.
    pub allowed_cloud_hosts: Option<Vec<String>>,
}

/// The policy changed while Cmdr runs. Same payload as `get_managed_policy`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ManagedPolicyChanged {
    pub policy: ManagedPolicyView,
}

impl From<&ManagedPolicy> for ManagedPolicyView {
    fn from(policy: &ManagedPolicy) -> Self {
        let updates = match policy.updates() {
            UpdatePolicy::Disabled => UpdatePolicyView::Disabled,
            UpdatePolicy::Enabled {
                automatic_checks,
                ceiling,
            } => UpdatePolicyView::Enabled {
                automatic_checks,
                ceiling: ceiling.map(|c| c.to_string()),
            },
        };
        Self {
            managed: policy.is_managed(),
            usage_stats_disabled: policy.usage_stats_disabled(),
            reports_disabled: policy.reports_disabled(),
            updates,
            ai: AiPolicyView {
                mode: policy.ai(),
                allowed_cloud_hosts: policy
                    .allowed_cloud_hosts()
                    .map(|hosts| hosts.iter().map(ToString::to_string).collect()),
            },
            locked_settings: locked_settings(policy),
        }
    }
}

/// The organization's policy as the UI shows it. Reads the cache, never CFPreferences.
#[tauri::command]
#[specta::specta]
pub fn get_managed_policy() -> ManagedPolicyView {
    ManagedPolicyView::from(&*super::current())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::locked::{LockedValue, SettingLock};
    use crate::test_support::TestDir;
    use serde_json::json;

    #[test]
    fn no_policy_reads_as_unmanaged() {
        assert_eq!(
            ManagedPolicyView::from(&ManagedPolicy::default()),
            ManagedPolicyView {
                managed: false,
                usage_stats_disabled: false,
                reports_disabled: false,
                updates: UpdatePolicyView::Enabled {
                    automatic_checks: true,
                    ceiling: None
                },
                ai: AiPolicyView {
                    mode: AiPolicy::Allowed,
                    allowed_cloud_hosts: None
                },
                locked_settings: vec![],
            }
        );
    }

    #[test]
    fn the_wire_shape_is_camel_case() {
        let view = ManagedPolicyView::from(&ManagedPolicy::default());
        assert_eq!(
            serde_json::to_value(&view).expect("serialize"),
            json!({
                "managed": false,
                "usageStatsDisabled": false,
                "reportsDisabled": false,
                "updates": { "kind": "enabled", "automaticChecks": true, "ceiling": null },
                "ai": { "mode": "allowed", "allowedCloudHosts": null },
                "lockedSettings": [],
            })
        );
    }

    /// The M1 "done" line: the command, through the real loader and the file override, returns
    /// the policy the file describes. nextest runs each test in its own process, so this test's
    /// env var and the process-wide cache are its own.
    #[test]
    fn get_managed_policy_reads_the_file_override() {
        let dir = TestDir::new("managed-policy-override");
        let path = dir.join("policy.plist");
        std::fs::write(
            &path,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>DisableCrashAndErrorReports</key><true/>
  <key>DisableAutomaticUpdateChecks</key><string>yes</string>
  <key>MaxUpdateVersion</key><string>0.52</string>
  <key>AllowedCloudAIHosts</key><array><string>LOCALHOST</string><string>*.openai.azure.com</string></array>
</dict></plist>"#,
        )
        .expect("write the test policy");

        // SAFETY: this test is the only code in its (nextest) process that reads or writes the
        // environment, and it sets the variable before anything reads the policy.
        unsafe { std::env::set_var("CMDR_MANAGED_PREFS_FILE", &path) };

        assert_eq!(
            get_managed_policy(),
            ManagedPolicyView {
                managed: true,
                usage_stats_disabled: false,
                reports_disabled: true,
                updates: UpdatePolicyView::Enabled {
                    automatic_checks: false,
                    ceiling: Some("0.52".into())
                },
                ai: AiPolicyView {
                    mode: AiPolicy::Allowed,
                    allowed_cloud_hosts: Some(vec!["localhost".into(), "*.openai.azure.com".into()]),
                },
                locked_settings: vec![
                    LockedSetting {
                        id: "updates.crashReports".into(),
                        lock: SettingLock::Fixed {
                            value: LockedValue::Bool(false)
                        },
                    },
                    LockedSetting {
                        id: "updates.errorReports".into(),
                        lock: SettingLock::Fixed {
                            value: LockedValue::Bool(false)
                        },
                    },
                    LockedSetting {
                        id: "updates.autoCheck".into(),
                        lock: SettingLock::Fixed {
                            value: LockedValue::Bool(false)
                        },
                    },
                ],
            }
        );
    }
}

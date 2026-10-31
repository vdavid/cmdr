//! The typed vocabulary of the security event log. Every `kind` and every enum value below is a
//! token in the file format (`security-events.jsonl`): renaming a TOKEN is a format change,
//! renaming a Rust variant is free.
//!
//! ❌ No free-text field, and no `String` built from an error's `Display`. The only strings are
//! versions, volume ids, and managed-policy key names, so a secret can't slip in by accident;
//! `tests::no_variant_grows_a_field_unnoticed` pins each variant's field list.

use serde::{Deserialize, Serialize};

/// The format version written as `v` on every line.
pub const FORMAT_VERSION: u32 = 1;

/// One security-relevant thing that happened. The one enum both the log and any other consumer
/// (see `subscribe` in `mod.rs`) match on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecurityEvent {
    /// The person allowed cloud AI.
    CloudAiConsentGiven,
    /// The person took the cloud AI consent back.
    CloudAiConsentWithdrawn,
    McpServerTurnedOn {
        by: Initiator,
    },
    McpServerTurnedOff {
        by: Initiator,
    },
    /// The first MCP request turned away for `reason` since the last quiet spell.
    McpRejectionsStarted {
        reason: McpRejection,
    },
    /// Rejections for `reason` went quiet (or Cmdr quit): `count` of them in this episode,
    /// the first one included.
    McpRejectionsEnded {
        reason: McpRejection,
        count: u64,
    },
    /// An organization's policy took effect at launch or changed later. Lists which managed keys
    /// restrict anything, never their values (an allowlist value can name an internal host).
    ManagedPolicyApplied {
        trigger: PolicyTrigger,
        keys_before: Vec<String>,
        keys_after: Vec<String>,
    },
    UpdateInstalled {
        from_version: String,
        to_version: String,
    },
    /// The organization's policy refused installing `version`.
    UpdateRefusedByPolicy {
        version: String,
        reason: PolicyRefusal,
    },
    /// An offered update didn't carry a valid signature, so Cmdr refused it.
    UpdateSignatureInvalid {
        version: String,
        check: SignatureCheck,
    },
    /// The first SMB reply refused for a bad or missing signature on `volume_id` since its last
    /// quiet spell.
    SmbSignatureRejectionsStarted {
        volume_id: String,
    },
    SmbSignatureRejectionsEnded {
        volume_id: String,
        count: u64,
    },
    RecordingTurnedOn {
        by: RecordingSwitch,
    },
    /// The last line written before recording stops.
    RecordingTurnedOff {
        by: RecordingSwitch,
    },
    /// Cmdr couldn't read one of its own state files, so it set the file aside (or recreated a
    /// database) and started that state fresh. Names which file, never its path.
    StateFileRecovered {
        file: StateFile,
    },
}

/// Who started or stopped something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Initiator {
    /// The person, through Settings.
    User,
    /// Cmdr itself, at launch, from the saved setting.
    Launch,
}

/// Why the MCP server turned a request away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRejection {
    /// No bearer token: usually a client still being set up.
    MissingToken,
    /// A token that isn't this run's: usually a client holding the previous run's token.
    WrongToken,
    /// The server had no token to compare against (it shouldn't happen while serving).
    NoTokenConfigured,
    /// A non-loopback `Origin`: a web page tried to reach the server.
    ForeignOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyTrigger {
    /// The policy Cmdr found at launch.
    Launch,
    /// A change while Cmdr was running.
    Change,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyRefusal {
    /// `DisableUpdates`: no version at all.
    UpdatesDisabled,
    /// The version is past `MaxUpdateVersion`.
    AboveCeiling,
}

/// Which signature didn't hold. Also crosses IPC in the update notice
/// (`crate::security_notices::AccessNotice`), where it rides the same tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SignatureCheck {
    /// The archive's minisign signature against the updater keys.
    Minisign,
    /// The bundle's Apple code signature against Cmdr's Developer ID.
    CodeSignature,
}

/// Who switched recording on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingSwitch {
    /// The person's switch in Settings.
    User,
    /// The organization's policy started or stopped requiring the log.
    Organization,
}

/// One of Cmdr's own state files in the app data dir. Also crosses IPC in the recovery notice
/// (`crate::state_recovery::StateRecovery`), where it rides the same tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StateFile {
    /// `settings.json`.
    Settings,
    /// `shortcuts.json`.
    Shortcuts,
    /// `license.json`. The key itself lives in the secret store.
    License,
    /// `app-status.json`: each pane's folder, tabs, and layout.
    AppStatus,
    /// `operation-log.db`, which undo reads.
    OperationLog,
    /// `main.db`, Ask Cmdr's chats.
    ChatHistory,
    /// `favorites.json`.
    Favorites,
    /// The Go to path dialog's recent paths.
    GoToPathHistory,
    /// The search dialog's recent queries.
    SearchHistory,
    /// The Selection dialog's recent selections.
    SelectionHistory,
    /// Saved multi-rename presets.
    MultiRenamePresets,
    /// The multi-rename dialog's last settings.
    MultiRenameLastSettings,
    /// The multi-rename dialog's per-field history.
    MultiRenameFieldHistory,
    /// `usage.json`, the days Cmdr was opened on.
    LaunchDays,
    /// `s3_known_places.json`: saved S3 accounts and bucket places.
    S3Places,
    /// `nfs_known_places.json`: saved NFS servers and export places.
    NfsPlaces,
    /// `known-sftp-servers.json`.
    SftpServers,
    /// `webdav_known_servers.json`.
    WebdavServers,
    /// `known-shares.json`: saved SMB shares and each server's last sign-in.
    SmbShares,
    /// `manual-servers.json`: SMB servers added by address.
    SmbServers,
    /// `known-sftp-hosts.json`: the SSH host keys the person approved.
    SftpHostKeys,
    /// `smb-link-consents.json`: the other servers the person let SMB links sign in to.
    SmbLinkConsents,
    /// `known-webdav-certificates.json`: the server certificates the person pinned.
    WebdavCertificates,
    /// `ai/ai-state.json`: whether local AI is installed, its server, and a dismissed offer.
    LocalAi,
    /// `credentials.enc`, the saved passwords and keys on a Linux without a keyring.
    Credentials,
}

/// One line of the file: `{"v":1,"at":"<UTC ISO 8601>","event":{"kind":…}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    pub v: u32,
    pub at: String,
    pub event: SecurityEvent,
}

impl LogLine {
    /// A line stamped with the current UTC time, to the millisecond.
    pub fn now(event: SecurityEvent) -> Self {
        Self {
            v: FORMAT_VERSION,
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of each variant, and the fields each may carry. The `match` has no wildcard, so a new
    /// variant doesn't compile until it's listed here with its fields.
    fn fields_of(event: &SecurityEvent) -> &'static [&'static str] {
        match event {
            SecurityEvent::CloudAiConsentGiven | SecurityEvent::CloudAiConsentWithdrawn => &[],
            SecurityEvent::McpServerTurnedOn { .. } | SecurityEvent::McpServerTurnedOff { .. } => &["by"],
            SecurityEvent::McpRejectionsStarted { .. } => &["reason"],
            SecurityEvent::McpRejectionsEnded { .. } => &["reason", "count"],
            SecurityEvent::ManagedPolicyApplied { .. } => &["trigger", "keys_before", "keys_after"],
            SecurityEvent::UpdateInstalled { .. } => &["from_version", "to_version"],
            SecurityEvent::UpdateRefusedByPolicy { .. } => &["version", "reason"],
            SecurityEvent::UpdateSignatureInvalid { .. } => &["version", "check"],
            SecurityEvent::SmbSignatureRejectionsStarted { .. } => &["volume_id"],
            SecurityEvent::SmbSignatureRejectionsEnded { .. } => &["volume_id", "count"],
            SecurityEvent::RecordingTurnedOn { .. } | SecurityEvent::RecordingTurnedOff { .. } => &["by"],
            SecurityEvent::StateFileRecovered { .. } => &["file"],
        }
    }

    /// Every variant once, beside the exact line it writes. The line is the file format: when
    /// this fails because a token moved, that's a format change, not a test to update casually.
    fn every_variant() -> Vec<(SecurityEvent, &'static str)> {
        vec![
            (
                SecurityEvent::CloudAiConsentGiven,
                r#"{"kind":"cloud_ai_consent_given"}"#,
            ),
            (
                SecurityEvent::CloudAiConsentWithdrawn,
                r#"{"kind":"cloud_ai_consent_withdrawn"}"#,
            ),
            (
                SecurityEvent::McpServerTurnedOn { by: Initiator::Launch },
                r#"{"kind":"mcp_server_turned_on","by":"launch"}"#,
            ),
            (
                SecurityEvent::McpServerTurnedOff { by: Initiator::User },
                r#"{"kind":"mcp_server_turned_off","by":"user"}"#,
            ),
            (
                SecurityEvent::McpRejectionsStarted {
                    reason: McpRejection::WrongToken,
                },
                r#"{"kind":"mcp_rejections_started","reason":"wrong_token"}"#,
            ),
            (
                SecurityEvent::McpRejectionsEnded {
                    reason: McpRejection::ForeignOrigin,
                    count: 12,
                },
                r#"{"kind":"mcp_rejections_ended","reason":"foreign_origin","count":12}"#,
            ),
            (
                SecurityEvent::ManagedPolicyApplied {
                    trigger: PolicyTrigger::Change,
                    keys_before: vec![],
                    keys_after: vec!["DisableUpdates".into()],
                },
                r#"{"kind":"managed_policy_applied","trigger":"change","keys_before":[],"keys_after":["DisableUpdates"]}"#,
            ),
            (
                SecurityEvent::UpdateInstalled {
                    from_version: "0.52.0".into(),
                    to_version: "0.53.0".into(),
                },
                r#"{"kind":"update_installed","from_version":"0.52.0","to_version":"0.53.0"}"#,
            ),
            (
                SecurityEvent::UpdateRefusedByPolicy {
                    version: "0.53.0".into(),
                    reason: PolicyRefusal::AboveCeiling,
                },
                r#"{"kind":"update_refused_by_policy","version":"0.53.0","reason":"above_ceiling"}"#,
            ),
            (
                SecurityEvent::UpdateSignatureInvalid {
                    version: "0.53.0".into(),
                    check: SignatureCheck::Minisign,
                },
                r#"{"kind":"update_signature_invalid","version":"0.53.0","check":"minisign"}"#,
            ),
            (
                SecurityEvent::SmbSignatureRejectionsStarted {
                    volume_id: "smb-nas".into(),
                },
                r#"{"kind":"smb_signature_rejections_started","volume_id":"smb-nas"}"#,
            ),
            (
                SecurityEvent::SmbSignatureRejectionsEnded {
                    volume_id: "smb-nas".into(),
                    count: 3,
                },
                r#"{"kind":"smb_signature_rejections_ended","volume_id":"smb-nas","count":3}"#,
            ),
            (
                SecurityEvent::RecordingTurnedOn {
                    by: RecordingSwitch::Organization,
                },
                r#"{"kind":"recording_turned_on","by":"organization"}"#,
            ),
            (
                SecurityEvent::RecordingTurnedOff {
                    by: RecordingSwitch::User,
                },
                r#"{"kind":"recording_turned_off","by":"user"}"#,
            ),
            (
                SecurityEvent::StateFileRecovered {
                    file: StateFile::OperationLog,
                },
                r#"{"kind":"state_file_recovered","file":"operation_log"}"#,
            ),
        ]
    }

    #[test]
    fn every_variant_writes_its_stable_line_and_reads_back() {
        for (event, line) in every_variant() {
            assert_eq!(serde_json::to_string(&event).expect("serializes"), line);
            let back: SecurityEvent = serde_json::from_str(line).expect("reads back");
            assert_eq!(back, event);
        }
    }

    #[test]
    fn no_variant_grows_a_field_unnoticed() {
        for (event, _) in every_variant() {
            let value = serde_json::to_value(&event).expect("serializes");
            let object = value.as_object().expect("an event is a JSON object");
            let mut keys: Vec<&str> = object.keys().map(String::as_str).filter(|k| *k != "kind").collect();
            let mut expected = fields_of(&event).to_vec();
            keys.sort_unstable();
            expected.sort_unstable();
            assert_eq!(keys, expected, "{event:?} carries a field nobody reviewed");
        }
    }

    #[test]
    fn a_line_carries_the_format_version_and_a_utc_timestamp() {
        let line = LogLine::now(SecurityEvent::CloudAiConsentGiven);
        let json = serde_json::to_string(&line).expect("serializes");
        assert!(json.starts_with(r#"{"v":1,"at":""#), "{json}");
        assert!(
            json.ends_with(r#"","event":{"kind":"cloud_ai_consent_given"}}"#),
            "{json}"
        );
        assert!(line.at.ends_with('Z'), "UTC, not local time: {}", line.at);
        chrono::DateTime::parse_from_rfc3339(&line.at).expect("ISO 8601");
    }
}

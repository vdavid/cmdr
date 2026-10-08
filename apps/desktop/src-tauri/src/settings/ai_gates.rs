//! The switches the AI gates read fresh from `settings.json` on every check: Ask Cmdr's on/off
//! and the two held-"no" markers. Each is a plain boolean where only a real JSON `true` counts,
//! so an absent key, a hand-edited string, or an unreadable file all read as the closed answer.
//!
//! Read fresh (not from the startup `Settings`) because each one closes a gate the moment it's
//! written: the send path reads it per send, and the wake readiness is refreshed by an explicit
//! push (`ask_cmdr_enabled_changed`, `cloud_ai_consent_revoke_pending_changed`).

use std::fs;

/// Ask Cmdr's switch (`askCmdr.enabled`) as the gates need it: on, off by the person, or on by
/// the person and pinned off by the organization's policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskCmdrSwitch {
    On,
    /// The person's own answer (or no answer yet). Withdraws the purpose of the wake backlog.
    Off,
    /// The person switched it on and the organization's policy (`DisableAI`) pins it off. Nothing
    /// runs, but nothing the person stored is taken away: the policy overlays, it never rewrites.
    ManagedOff,
}

/// Ask Cmdr's switch, read by the send gate and the wake readiness. An absent key reads as OFF
/// (fail quiet): the registry default is `false`, and every user who should have it on gets it
/// written explicitly (onboarding, or the one-time mapping from the legacy Ask Cmdr opt-in). Read
/// through the organization's locks (`managed_policy::overlay`).
pub fn load_ask_cmdr_switch<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AskCmdrSwitch {
    let Ok(data_dir) = crate::config::resolved_app_data_dir(app) else {
        return AskCmdrSwitch::Off;
    };
    let Ok(contents) = fs::read_to_string(data_dir.join("settings.json")) else {
        return AskCmdrSwitch::Off;
    };
    ask_cmdr_switch_under(&crate::managed_policy::current(), &contents)
}

/// The policy's lock over what `settings.json` holds, keeping the person's own off apart.
fn ask_cmdr_switch_under(policy: &crate::managed_policy::ManagedPolicy, contents: &str) -> AskCmdrSwitch {
    let stored = serde_json::from_str::<serde_json::Value>(contents).unwrap_or(serde_json::Value::Null);
    let enabled = |settings: &serde_json::Value| {
        settings.get("askCmdr.enabled").and_then(serde_json::Value::as_bool) == Some(true)
    };
    if !enabled(&stored) {
        return AskCmdrSwitch::Off;
    }
    let mut effective = stored;
    crate::managed_policy::overlay(policy, &mut effective);
    if enabled(&effective) {
        AskCmdrSwitch::On
    } else {
        AskCmdrSwitch::ManagedOff
    }
}

#[cfg(test)]
fn parse_ask_cmdr_enabled(contents: &str) -> bool {
    ask_cmdr_switch_under(&crate::managed_policy::ManagedPolicy::default(), contents) == AskCmdrSwitch::On
}

/// Whether a "no" to Ask Cmdr is held for a `main.db` that refused to record it
/// (`askCmdr.consentRevokePending`), from before Ask Cmdr's opt-in became a plain switch. Its one
/// reader is the one-time mapping onto `askCmdr.enabled` (`ask_cmdr_legacy_opt_in`): a held "no"
/// maps to off.
pub fn load_ask_cmdr_consent_revoke_pending<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    load_true_flag(app, parse_ask_cmdr_consent_revoke_pending)
}

fn parse_ask_cmdr_consent_revoke_pending(contents: &str) -> bool {
    parse_true_flag(contents, "askCmdr.consentRevokePending")
}

/// Whether a "no" to cloud AI is held for a `main.db` that refused to record it
/// (`ai.cloudConsentRevokePending`). `ai::cloud_consent::RevokePending::load` is its one reader,
/// which is how every cloud gate sees it. The marker only exists on the path where a revoke was
/// refused; the store record stays the answer everywhere else.
pub fn load_cloud_consent_revoke_pending<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    load_true_flag(app, parse_cloud_consent_revoke_pending)
}

fn parse_cloud_consent_revoke_pending(contents: &str) -> bool {
    parse_true_flag(contents, "ai.cloudConsentRevokePending")
}

/// Read `settings.json` fresh and apply `parse`. An unreadable data dir or file reads as `false`.
fn load_true_flag<R: tauri::Runtime>(app: &tauri::AppHandle<R>, parse: fn(&str) -> bool) -> bool {
    let Ok(data_dir) = crate::config::resolved_app_data_dir(app) else {
        return false;
    };
    let Ok(contents) = fs::read_to_string(data_dir.join("settings.json")) else {
        return false;
    };
    parse(&contents)
}

/// Whether `key` holds a real JSON `true`.
fn parse_true_flag(contents: &str, key: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(contents)
        .ok()
        .and_then(|json| json.get(key).and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The legacy held "no" maps Ask Cmdr off, so only the value the frontend wrote (a JSON `true`)
    /// counts. Anything else, including a stringly `"true"` from a hand edit, leaves the store record
    /// as the answer.
    #[test]
    fn a_held_consent_revoke_reads_only_from_a_real_true() {
        assert!(parse_ask_cmdr_consent_revoke_pending(
            r#"{ "askCmdr.consentRevokePending": true }"#
        ));
        for contents in [
            "{}",
            r#"{ "askCmdr.consentRevokePending": false }"#,
            r#"{ "askCmdr.consentRevokePending": "true" }"#,
            "not json at all",
        ] {
            assert!(
                !parse_ask_cmdr_consent_revoke_pending(contents),
                "{contents} must not read as a held revoke"
            );
        }
    }

    /// Ask Cmdr's switch opens the send gate and the proactive loop, so only a real JSON `true`
    /// turns it on. An absent key reads as off (fail quiet): the one-time mapping and onboarding
    /// write it explicitly for everyone who should have it on.
    #[test]
    fn ask_cmdr_is_on_only_for_a_real_true() {
        assert!(parse_ask_cmdr_enabled(r#"{ "askCmdr.enabled": true }"#));
        for contents in [
            "{}",
            r#"{ "askCmdr.enabled": false }"#,
            r#"{ "askCmdr.enabled": "true" }"#,
            r#"{ "askCmdr.enabled": 1 }"#,
            "not json at all",
        ] {
            assert!(!parse_ask_cmdr_enabled(contents), "{contents} must read as off");
        }
    }

    /// `DisableAI` pins Ask Cmdr off whatever the person stored, so the send gate and the wake
    /// loop read the same answer as the switch in Settings. But a pinned off is the policy's, not
    /// the person's: it reads as `ManagedOff`, which takes nothing away. Only a stored off is `Off`.
    #[test]
    fn a_managed_off_is_told_apart_from_the_persons_own_off() {
        use crate::managed_policy::testing::{self, DISABLE_AI, DISABLE_CLOUD_AI};
        let on = r#"{ "askCmdr.enabled": true }"#;
        let off = r#"{ "askCmdr.enabled": false }"#;
        let ai_off = testing::forcing(&[DISABLE_AI]);
        assert_eq!(ask_cmdr_switch_under(&ai_off, on), AskCmdrSwitch::ManagedOff);
        assert_eq!(ask_cmdr_switch_under(&ai_off, off), AskCmdrSwitch::Off);
        assert_eq!(ask_cmdr_switch_under(&ai_off, "not json at all"), AskCmdrSwitch::Off);
        assert_eq!(
            ask_cmdr_switch_under(&testing::forcing(&[DISABLE_CLOUD_AI]), on),
            AskCmdrSwitch::On
        );
        assert_eq!(
            ask_cmdr_switch_under(&crate::managed_policy::ManagedPolicy::default(), on),
            AskCmdrSwitch::On
        );
    }

    /// A held "no" to cloud AI closes every cloud gate, so only the value the frontend writes (a
    /// JSON `true`) may hold one. Anything else leaves the store record as the answer.
    #[test]
    fn a_held_cloud_consent_revoke_reads_only_from_a_real_true() {
        assert!(parse_cloud_consent_revoke_pending(
            r#"{ "ai.cloudConsentRevokePending": true }"#
        ));
        for contents in [
            "{}",
            r#"{ "ai.cloudConsentRevokePending": false }"#,
            r#"{ "ai.cloudConsentRevokePending": "true" }"#,
            // The legacy Ask Cmdr marker is a different answer to a different question.
            r#"{ "askCmdr.consentRevokePending": true }"#,
            "not json at all",
        ] {
            assert!(
                !parse_cloud_consent_revoke_pending(contents),
                "{contents} must not read as a held cloud revoke"
            );
        }
    }
}

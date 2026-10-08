//! Anonymous beta usage analytics: the consent gate, the event spool, and the heartbeat.
//!
//! See `analytics/CLAUDE.md` for the full model. In short: feature events land in an on-disk spool
//! ([`events::capture`]), and a background loop ([`heartbeat`]) posts a `/heartbeat` at most once
//! per three hours, carrying the random `anal_` install id, app/OS/arch identity, a PII-free
//! config-shape snapshot, the uptime since the last beat, and the spooled events. Everything is
//! gated on consent (tri-state, default-on) and on [`suppression_reason`], which keeps every dev,
//! CI, E2E, and capture instance out of production analytics unless explicitly forced for
//! integration tests.

mod config_shape;
pub mod events;
pub(crate) mod first_index;
mod heartbeat;
pub mod session;
mod spool;
pub mod volume_sink;

use spool::Spool;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::AppHandle;

/// Override env var that forces analytics from an otherwise-suppressed instance, so an integration
/// test can drive the loop against a localhost Worker. Without it, no dev, CI, E2E, or capture
/// instance ever spools or beats, so a test run can't pollute production analytics.
const FORCE_ENV: &str = "CMDR_ANALYTICS_FORCE";

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// The event spool, opened at [`init`]. `None` before then, or when the data dir can't be resolved.
static SPOOL: OnceLock<Spool> = OnceLock::new();

/// Stores the app handle and opens the event spool. Call once during setup, before [`start`].
pub fn init(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
    if let Some(dir) = data_dir() {
        let _ = SPOOL.set(Spool::new(dir.join(spool::SPOOL_FILE_NAME)));
    }
}

/// Starts the background heartbeat loop. Call once from setup, after [`init`].
pub fn start() {
    heartbeat::start();
}

fn data_dir() -> Option<PathBuf> {
    let app = APP_HANDLE.get()?;
    crate::config::resolved_app_data_dir(app)
        .inspect_err(|e| log::warn!(target: "analytics", "No app data dir for analytics: {e}"))
        .ok()
}

fn spool() -> Option<&'static Spool> {
    SPOOL.get()
}

/// Whether this process may collect and send analytics right now.
enum SendPermission {
    /// Not a real user's install (see [`suppression_reason`]).
    Suppressed(SuppressionReason),
    /// The user turned analytics off.
    OptedOut,
    Granted,
}

/// Asks both gates: the suppression gate first (it's free), then consent from `settings.json`.
fn send_permission() -> SendPermission {
    if let Some(reason) = suppression_reason() {
        return SendPermission::Suppressed(reason);
    }
    if APP_HANDLE.get().is_none() {
        return SendPermission::Suppressed(SuppressionReason::NotInitialized);
    }
    if usage_consent_granted(&crate::managed_policy::current(), read_raw_settings()) {
        SendPermission::Granted
    } else {
        SendPermission::OptedOut
    }
}

/// Buckets an item count into a coarse, PII-free range string for analytics. A raw count is fine to
/// ship (it's not PII), but a bucket keeps the dashboard's cardinality low and the signal readable.
///
/// Shared by every event that reports "how many things", so two dashboards never end up with two
/// different ideas of what "a lot" is.
pub fn item_count_bucket(count: usize) -> &'static str {
    match count {
        0 => "0",
        1 => "1",
        2..=10 => "2-10",
        11..=100 => "11-100",
        101..=1000 => "101-1000",
        _ => "1000+",
    }
}

/// Consent from the stored settings as the EFFECTIVE `analytics.enabled`: the organization's locks
/// over the person's own choice, so a managed off is an ordinary opt-out (capture drops, the
/// heartbeat forgets the spool and the unreported uptime).
fn usage_consent_granted(policy: &crate::managed_policy::ManagedPolicy, mut settings: serde_json::Value) -> bool {
    crate::managed_policy::overlay(policy, &mut settings);
    analytics_consent_granted(settings.get("analytics.enabled").and_then(serde_json::Value::as_bool))
}

/// Whether analytics may send, per the tri-state consent rule. `None` (no key persisted, the
/// opted-in default) and `Some(true)` mean granted; only `Some(false)` is an opt-out.
pub fn analytics_consent_granted(analytics_enabled: Option<bool>) -> bool {
    analytics_enabled != Some(false)
}

/// Why this process must not send analytics. Carried (rather than collapsed to a bool) so the
/// debug log names the exact condition that fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuppressionReason {
    /// Not a release build.
    DebugBuild,
    /// One of [`crate::prod_instance::NON_PROD_ENV_VARS`] is set in this process's environment.
    NonProdEnv(&'static str),
    /// [`init`] hasn't run, so there's no settings to read consent from.
    NotInitialized,
}

impl std::fmt::Display for SuppressionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DebugBuild => f.write_str("debug build"),
            Self::NonProdEnv(name) => write!(f, "{name} is set"),
            Self::NotInitialized => f.write_str("analytics not initialized"),
        }
    }
}

/// Pure core of the gate: the whole matrix is decided here, with `is_debug_build`, `forced`, and
/// `env_is_set` injected so it's unit-testable without mutating the process environment (which a
/// parallel test runner can't do safely).
fn suppression_reason_for(
    is_debug_build: bool,
    forced: bool,
    env_is_set: &dyn Fn(&str) -> bool,
) -> Option<SuppressionReason> {
    if forced {
        return None;
    }
    if is_debug_build {
        return Some(SuppressionReason::DebugBuild);
    }
    crate::prod_instance::non_prod_env_var_in(env_is_set).map(SuppressionReason::NonProdEnv)
}

/// The ONE analytics gate: `Some(reason)` when this process must not send, `None` when it may.
/// Both the heartbeat loop and `events::capture` call it (through [`send_permission`]), so the
/// heartbeat and the event stream can never disagree about whether an install is real.
///
/// `CMDR_ANALYTICS_FORCE=1` overrides every condition, which is what lets an integration test
/// drive the loop against a localhost Worker.
fn suppression_reason() -> Option<SuppressionReason> {
    suppression_reason_for(cfg!(debug_assertions), std::env::var_os(FORCE_ENV).is_some(), &|name| {
        std::env::var_os(name).is_some()
    })
}

/// Reads the stored `settings.json` as a raw JSON value, before any managed lock: callers overlay
/// the policy themselves (consent, the config shape). Resolves the data dir
/// without an `AppHandle` (mirroring the install-id and early-load helpers). A missing or corrupt
/// file yields `Value::Null`, which the builder treats as "no settings."
fn read_raw_settings() -> serde_json::Value {
    let Some(data_dir) = crate::config::standalone_app_data_dir() else {
        return serde_json::Value::Null;
    };
    let settings_path = data_dir.join("settings.json");
    std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod consent_tests {
    use super::*;
    use crate::managed_policy::testing;
    use serde_json::json;

    #[test]
    fn the_persons_own_choice_decides_without_a_policy() {
        let none = crate::managed_policy::ManagedPolicy::default();
        assert!(usage_consent_granted(&none, json!({})));
        assert!(usage_consent_granted(&none, serde_json::Value::Null));
        assert!(usage_consent_granted(&none, json!({ "analytics.enabled": true })));
        assert!(!usage_consent_granted(&none, json!({ "analytics.enabled": false })));
    }

    #[test]
    fn a_managed_off_wins_over_every_stored_choice() {
        let off = testing::forcing(&[testing::DISABLE_USAGE_STATS]);
        for stored in [
            json!({}),
            serde_json::Value::Null,
            json!({ "analytics.enabled": true }),
            json!({ "analytics.enabled": false }),
        ] {
            assert!(!usage_consent_granted(&off, stored.clone()), "{stored}");
        }
    }

    #[test]
    fn other_keys_leave_usage_consent_alone() {
        let reports_off = testing::forcing(&[testing::DISABLE_CRASH_AND_ERROR_REPORTS, testing::DISABLE_AI]);
        assert!(usage_consent_granted(
            &reports_off,
            json!({ "analytics.enabled": true })
        ));
        assert!(!usage_consent_granted(
            &reports_off,
            json!({ "analytics.enabled": false })
        ));
    }
}

#[cfg(test)]
mod suppression_tests {
    use super::*;
    use std::collections::HashSet;

    /// Asks the gate about a process whose environment holds exactly `vars` and nothing else.
    fn reason(is_debug: bool, forced: bool, vars: &[&str]) -> Option<SuppressionReason> {
        let set: HashSet<&str> = vars.iter().copied().collect();
        suppression_reason_for(is_debug, forced, &|name| set.contains(name))
    }

    #[test]
    fn release_build_with_clean_env_may_send() {
        assert_eq!(reason(false, false, &[]), None);
    }

    #[test]
    fn debug_build_is_suppressed() {
        assert_eq!(reason(true, false, &[]), Some(SuppressionReason::DebugBuild));
    }

    /// Every var in the list suppresses on its own, in a release build with nothing else set.
    #[test]
    fn each_non_prod_env_var_suppresses_alone() {
        for name in crate::prod_instance::NON_PROD_ENV_VARS {
            assert_eq!(
                reason(false, false, &[name]),
                Some(SuppressionReason::NonProdEnv(name)),
                "{name} alone must suppress analytics"
            );
        }
    }

    // The list's own contract (which vars it holds, and that every tooling launcher's env trips
    // it) is pinned beside the list in `crate::prod_instance`, so it holds for the updater gate
    // too. What's left here is what's specific to the analytics gate.

    /// The force override still wins over every condition, so the localhost-Worker integration
    /// test can drive the loop.
    #[test]
    fn force_override_beats_every_condition() {
        assert_eq!(reason(true, true, crate::prod_instance::NON_PROD_ENV_VARS), None);
    }

    /// The debug log has to name the condition, or the next pollution incident is undiagnosable.
    #[test]
    fn reasons_name_the_condition() {
        assert_eq!(SuppressionReason::DebugBuild.to_string(), "debug build");
        assert_eq!(
            SuppressionReason::NonProdEnv("CMDR_DATA_DIR").to_string(),
            "CMDR_DATA_DIR is set"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_none_is_granted() {
        // The opted-in default: no persisted key → analytics on.
        assert!(analytics_consent_granted(None));
    }

    #[test]
    fn consent_some_true_is_granted() {
        assert!(analytics_consent_granted(Some(true)));
    }

    #[test]
    fn consent_some_false_is_opted_out() {
        assert!(!analytics_consent_granted(Some(false)));
    }
}

#[cfg(test)]
mod bucket_tests {
    use super::item_count_bucket;

    #[test]
    fn item_count_buckets_map_to_coarse_ranges() {
        assert_eq!(item_count_bucket(0), "0");
        assert_eq!(item_count_bucket(1), "1");
        assert_eq!(item_count_bucket(2), "2-10");
        assert_eq!(item_count_bucket(10), "2-10");
        assert_eq!(item_count_bucket(11), "11-100");
        assert_eq!(item_count_bucket(100), "11-100");
        assert_eq!(item_count_bucket(101), "101-1000");
        assert_eq!(item_count_bucket(1000), "101-1000");
        assert_eq!(item_count_bucket(1001), "1000+");
        assert_eq!(item_count_bucket(50_000), "1000+");
    }
}

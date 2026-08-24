//! Anonymous beta usage analytics: the heartbeat sender + consent gate.
//!
//! See `analytics/CLAUDE.md` for the full model. In short: a background loop posts a `/heartbeat`
//! on launch and then hourly, carrying the random `anal_` install id, app/OS/arch identity, and a
//! PII-free config-shape snapshot. Everything is gated on consent (tri-state, default-on) and on
//! [`suppression_reason`], which keeps every dev, CI, E2E, and capture instance out of production
//! analytics unless explicitly forced for integration tests.

mod config_shape;
pub(crate) mod first_index;
pub mod posthog;
pub mod session;
pub mod volume_sink;

use serde::Serialize;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::AppHandle;

/// Heartbeat ingestion endpoint. Debug builds hit the local Worker; release hits production.
#[cfg(debug_assertions)]
const HEARTBEAT_URL: &str = "http://localhost:8787/heartbeat";
#[cfg(not(debug_assertions))]
const HEARTBEAT_URL: &str = "https://api.getcmdr.com/heartbeat";

/// How often to beat after the launch beat.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Network timeout for one fire-and-forget beat. Mirrors the crash/error reporters.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);

/// Override env var that forces beats from an otherwise-suppressed instance, so an integration
/// test can drive the loop against a localhost Worker. Without it, no dev, CI, E2E, or capture
/// instance ever beats, so a test run can't pollute production analytics.
const FORCE_ENV: &str = "CMDR_ANALYTICS_FORCE";

/// Bundle id from `tauri.conf.json`, mirrored so the raw-settings read works without an
/// `AppHandle`. Matches `settings/loader.rs`'s early-load helpers. Keep in sync if it changes.
const BUNDLE_ID: &str = "com.veszelovszki.cmdr";

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// The `/heartbeat` request body. Field names are camelCase on the wire (matching the M2 Worker
/// contract); `Option::None` serializes to `null`. M4 (PostHog) and M7 (diag id) must keep this
/// shape in sync with the server's validator.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatPayload {
    /// `anal_` + a lowercase hyphenated v4 UUID. Required; matches `^anal_[0-9a-f-]{36}$`.
    anal_id: String,
    /// Semver `x.y.z` from `CARGO_PKG_VERSION`.
    app_version: String,
    /// Human-readable OS version, always non-empty.
    os_version: String,
    /// `aarch64` / `x86_64`.
    arch: String,
    /// `"release"` / `"debug"`.
    build_mode: Option<String>,
    /// The PII-free config-shape snapshot. An arbitrary JSON object, stored verbatim by the server.
    config: serde_json::Value,
}

/// Stores the app handle. Call once during setup, before [`start`].
pub fn init(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
}

/// Starts the background heartbeat loop: one beat on launch, then one every hour. Call once from
/// setup, after [`init`].
pub fn start() {
    tauri::async_runtime::spawn(async {
        loop {
            send_beat_if_allowed().await;
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
        }
    });
}

/// Whether analytics may send right now, per the tri-state consent rule. `None` (no key persisted,
/// the opted-in default) and `Some(true)` mean granted; only `Some(false)` is an opt-out. Both the
/// heartbeat loop and (later) `track_event` gate through this one helper.
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

pub fn analytics_consent_granted(analytics_enabled: Option<bool>) -> bool {
    analytics_enabled != Some(false)
}

/// Env vars whose mere PRESENCE proves this process is not a real user's production install, and
/// so must never reach production analytics. A production launch (Finder, Dock, Spotlight, the
/// updater's relaunch) sets NONE of them; every dev, E2E, and capture launcher sets at least one.
///
/// Presence, not value: `CMDR_E2E_MODE=0` still means a harness composed this environment, and
/// failing closed costs nothing. Sources for each, all per `docs/tooling/instance-isolation.md`:
///
/// - `CI`: any CI runner.
/// - `CMDR_INSTANCE_ID`: dev, per-worktree dev, and every E2E shard. Prod leaves it unset by
///   definition, which makes it the single strongest signal.
/// - `CMDR_DATA_DIR`: an isolated data dir. Prod resolves `app_data_dir()` instead, and an
///   isolated dir is exactly what mints a fresh install id. It also covers
///   `scripts/marketing-shots.ts`, which deliberately sets no other hook.
/// - `CMDR_E2E_MODE`: the Playwright and Linux Docker E2E lanes, plus `scripts/i18n-capture.ts`.
/// - `CMDR_MOCK_FDA`: the FDA mock. Only a harness ever sets it, and it's what made 1,550 phantom
///   installs report `fdaGranted: true` on their first-ever launch.
const NON_PROD_ENV_VARS: &[&str] = &[
    "CI",
    "CMDR_INSTANCE_ID",
    "CMDR_DATA_DIR",
    "CMDR_E2E_MODE",
    "CMDR_MOCK_FDA",
];

/// Why this process must not send analytics. Carried (rather than collapsed to a bool) so the
/// debug log names the exact condition that fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuppressionReason {
    /// Not a release build.
    DebugBuild,
    /// One of [`NON_PROD_ENV_VARS`] is set in this process's environment.
    NonProdEnv(&'static str),
}

impl std::fmt::Display for SuppressionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DebugBuild => f.write_str("debug build"),
            Self::NonProdEnv(name) => write!(f, "{name} is set"),
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
    NON_PROD_ENV_VARS
        .iter()
        .copied()
        .find(|name| env_is_set(name))
        .map(SuppressionReason::NonProdEnv)
}

/// The ONE analytics gate: `Some(reason)` when this process must not send, `None` when it may.
/// Both the heartbeat loop and `posthog::capture` call it, so the heartbeat and the event stream
/// can never disagree about whether an install is real.
///
/// `CMDR_ANALYTICS_FORCE=1` overrides every condition, which is what lets an integration test
/// drive the loop against a localhost Worker.
fn suppression_reason() -> Option<SuppressionReason> {
    suppression_reason_for(cfg!(debug_assertions), std::env::var_os(FORCE_ENV).is_some(), &|name| {
        std::env::var_os(name).is_some()
    })
}

async fn send_beat_if_allowed() {
    if let Some(reason) = suppression_reason() {
        log::debug!(target: "analytics", "Heartbeat suppressed ({reason}, no force override)");
        return;
    }

    // Read consent through the shared settings loader the rest of the backend uses (the same path
    // M4's `track_event` gate will reuse), so consent resolution stays consistent app-wide.
    let Some(app) = APP_HANDLE.get() else {
        log::warn!(target: "analytics", "Heartbeat skipped: app handle not initialized");
        return;
    };
    let settings = crate::settings::load_settings(app);
    if !analytics_consent_granted(settings.analytics_enabled) {
        // Fully silent: an opted-out install sends nothing at all, not even an "I opted out" bit.
        return;
    }

    let payload = build_payload();
    send_payload(payload).await;
}

fn build_payload() -> HeartbeatPayload {
    let fda_granted = !crate::fda_gate::is_fda_pending_runtime();
    let config = config_shape::build_config_shape(&read_raw_settings(), fda_granted);

    HeartbeatPayload {
        anal_id: crate::install_id::analytics_id(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: crate::platform::os_version(),
        arch: std::env::consts::ARCH.to_string(),
        build_mode: Some(current_build_mode().to_string()),
        config,
    }
}

fn current_build_mode() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "release" }
}

/// Reads `settings.json` as a raw JSON value for the config-shape builder. Resolves the data dir
/// without an `AppHandle` (mirroring the install-id and early-load helpers). A missing or corrupt
/// file yields `Value::Null`, which the builder treats as "no settings."
fn read_raw_settings() -> serde_json::Value {
    let data_dir: PathBuf = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        PathBuf::from(custom)
    } else {
        match dirs::data_dir() {
            Some(base) => base.join(BUNDLE_ID),
            None => return serde_json::Value::Null,
        }
    };
    let settings_path = data_dir.join("settings.json");
    std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or(serde_json::Value::Null)
}

async fn send_payload(payload: HeartbeatPayload) {
    let client = match reqwest::Client::builder().timeout(HEARTBEAT_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "analytics", "Couldn't build heartbeat HTTP client: {e}");
            return;
        }
    };

    match client.post(HEARTBEAT_URL).json(&payload).send().await {
        Ok(response) if response.status().is_success() => {
            log::debug!(target: "analytics", "Heartbeat sent ({})", response.status());
        }
        Ok(response) => {
            log::warn!(target: "analytics", "Heartbeat server returned {}", response.status());
        }
        Err(e) => {
            // Fire-and-forget: a failed beat is fine, the next hourly tick retries.
            log::debug!(target: "analytics", "Heartbeat send failed: {e}");
        }
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
        for name in NON_PROD_ENV_VARS {
            assert_eq!(
                reason(false, false, &[name]),
                Some(SuppressionReason::NonProdEnv(name)),
                "{name} alone must suppress analytics"
            );
        }
    }

    /// The vars a real production install can never have set. Pinned by name so shrinking the
    /// list is a deliberate, visible act: every one of them was a live pollution source.
    #[test]
    fn the_non_prod_env_var_list_covers_every_isolation_signal() {
        for name in [
            "CI",
            "CMDR_INSTANCE_ID",
            "CMDR_DATA_DIR",
            "CMDR_E2E_MODE",
            "CMDR_MOCK_FDA",
        ] {
            assert!(
                NON_PROD_ENV_VARS.contains(&name),
                "{name} must stay in the suppression list"
            );
        }
    }

    /// The env each tooling launcher actually stamps. If a launcher's env stops tripping the gate,
    /// that harness starts minting phantom production installs again, so pin all of them here.
    #[test]
    fn every_tooling_launcher_is_suppressed() {
        // `scripts/check/checks/desktop-svelte-e2e-playwright.go` and `e2e-playwright-app.go`.
        let e2e_checker = ["CMDR_INSTANCE_ID", "CMDR_DATA_DIR", "CMDR_E2E_MODE", "CMDR_MOCK_FDA"];
        // `apps/desktop/scripts/i18n-capture.ts`.
        let i18n_capture = ["CMDR_E2E_MODE", "CMDR_DATA_DIR", "CMDR_MOCK_FDA"];
        // `apps/desktop/scripts/marketing-shots.ts` deliberately leaves `CMDR_E2E_MODE` unset.
        let marketing_shots = ["CMDR_DATA_DIR"];
        // `apps/desktop/scripts/tauri-wrapper.ts` (dev and per-worktree dev).
        let dev_wrapper = ["CMDR_INSTANCE_ID", "CMDR_DATA_DIR"];

        for (label, vars) in [
            ("e2e checker", &e2e_checker[..]),
            ("i18n capture", &i18n_capture[..]),
            ("marketing shots", &marketing_shots[..]),
            ("dev wrapper", &dev_wrapper[..]),
        ] {
            assert!(
                reason(false, false, vars).is_some(),
                "{label} must not reach production analytics"
            );
        }
    }

    /// The force override still wins over every condition, so the localhost-Worker integration
    /// test can drive the loop.
    #[test]
    fn force_override_beats_every_condition() {
        assert_eq!(reason(true, true, NON_PROD_ENV_VARS), None);
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
    use serde_json::json;

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

    #[test]
    fn payload_serializes_with_camelcase_and_nested_config() {
        let payload = HeartbeatPayload {
            anal_id: "anal_178c8e27-511f-4f0e-a1fc-6a44f2ab7341".to_string(),
            app_version: "1.2.3".to_string(),
            os_version: "macOS 26.0".to_string(),
            arch: "aarch64".to_string(),
            build_mode: Some("release".to_string()),
            config: json!({ "theme.mode": "dark", "fdaGranted": true }),
        };
        let value = serde_json::to_value(&payload).expect("serialize");

        // camelCase field names on the wire, matching the M2 Worker contract.
        assert_eq!(value["analId"], json!("anal_178c8e27-511f-4f0e-a1fc-6a44f2ab7341"));
        assert_eq!(value["appVersion"], json!("1.2.3"));
        assert_eq!(value["osVersion"], json!("macOS 26.0"));
        assert_eq!(value["arch"], json!("aarch64"));
        assert_eq!(value["buildMode"], json!("release"));
        // config is a nested object, stored verbatim.
        assert_eq!(value["config"]["theme.mode"], json!("dark"));
        assert_eq!(value["config"]["fdaGranted"], json!(true));

        // The anal id matches the heartbeat contract regex shape.
        let anal = value["analId"].as_str().expect("string");
        assert!(anal.starts_with("anal_"));
        assert_eq!(anal.strip_prefix("anal_").expect("prefix").len(), 36);
    }

    #[test]
    fn payload_none_build_mode_serializes_to_null() {
        let payload = HeartbeatPayload {
            anal_id: "anal_x".to_string(),
            app_version: "1.0.0".to_string(),
            os_version: "macOS 26.0".to_string(),
            arch: "aarch64".to_string(),
            build_mode: None,
            config: json!({}),
        };
        let value = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(value["buildMode"], json!(null));
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

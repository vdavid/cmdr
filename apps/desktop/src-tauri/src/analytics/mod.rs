//! Anonymous beta usage analytics: the heartbeat sender + consent gate.
//!
//! See `analytics/CLAUDE.md` for the full model. In short: a background loop posts a `/heartbeat`
//! on launch and then hourly, carrying the random `anal_` install id, app/OS/arch identity, and a
//! PII-free config-shape snapshot. Everything is gated on consent (tri-state, default-on) and
//! suppressed in dev/CI builds unless explicitly forced for integration tests.

mod config_shape;
pub mod posthog;

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

/// Override env var that forces beats even in a debug/CI build, so an integration test can drive
/// the loop against a localhost Worker. Without it, dev and CI never beat (so test runs and local
/// dev don't pollute production analytics).
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
pub fn analytics_consent_granted(analytics_enabled: Option<bool>) -> bool {
    analytics_enabled != Some(false)
}

/// `true` when this build must not send (dev or CI), unless the force override is set.
fn suppressed() -> bool {
    if std::env::var(FORCE_ENV).is_ok() {
        return false;
    }
    cfg!(debug_assertions) || std::env::var("CI").is_ok()
}

async fn send_beat_if_allowed() {
    if suppressed() {
        log::debug!(target: "analytics", "Heartbeat suppressed (dev or CI, no force override)");
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

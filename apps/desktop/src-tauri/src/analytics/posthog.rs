//! PostHog feature events: the one backend path for all product analytics events.
//!
//! See `analytics/CLAUDE.md` § "PostHog feature events" for the full model. In short: backend code
//! calls [`capture`] directly, frontend code calls the `track_event` IPC command (which calls
//! [`capture`]). Both ride the SAME consent gate and dev/CI suppression as the heartbeat, attach the
//! `anal_` install id as the PostHog `distinct_id`, and mirror the PII-free config-shape as `$set`
//! person properties.
//!
//! Events are an OPEN set: [`capture`] takes an arbitrary event name plus an arbitrary PII-free prop
//! map, so adding an event later is a one-line call with whatever categorical props that event
//! needs. The PII-free convention (enums, counts, bools only; never paths, names, queries, prompts)
//! is enforced socially by review and backstopped in debug builds by [`sanitize_props`].

use super::config_shape;
use serde_json::{Map, Value};

/// PostHog capture endpoint (EU cloud, project `136072`). Same host the website uses.
const CAPTURE_URL: &str = "https://eu.i.posthog.com/capture/";

/// Network timeout for one fire-and-forget capture. Mirrors the heartbeat sender.
const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The public PostHog project key (`phc_...`), baked at build time via the `CMDR_POSTHOG_KEY` env
/// var (a GitHub secret for release builds). `None` for local dev builds, where `capture` is a
/// no-op. The key is public by design (PostHog ingest keys are safe in client code).
const POSTHOG_KEY: Option<&str> = option_env!("CMDR_POSTHOG_KEY");

/// Captures one PostHog feature event. Fire-and-forget: builds the payload and spawns the POST, then
/// returns immediately so no call site ever blocks on the network.
///
/// Gated identically to the heartbeat: a no-op in dev/CI builds (unless `CMDR_ANALYTICS_FORCE=1`),
/// a no-op when the user opted out (`analytics.enabled == Some(false)`), and a no-op when no
/// `CMDR_POSTHOG_KEY` was baked in (local dev). `props` is an arbitrary PII-free object; pass
/// `serde_json::json!({})` for an event with no properties.
pub fn capture(event: &str, props: Value) {
    if super::suppressed() {
        log::debug!(target: "analytics", "PostHog event '{event}' suppressed (dev or CI, no force override)");
        return;
    }

    // Consent reuses the SAME tri-state gate the heartbeat uses, read through the shared settings
    // loader so consent resolution stays consistent app-wide.
    let Some(app) = super::APP_HANDLE.get() else {
        log::warn!(target: "analytics", "PostHog event '{event}' skipped: app handle not initialized");
        return;
    };
    let settings = crate::settings::load_settings(app);
    if !super::analytics_consent_granted(settings.analytics_enabled) {
        // Fully silent: an opted-out install sends nothing at all.
        return;
    }

    let Some(api_key) = POSTHOG_KEY else {
        // Local dev: no key baked in. Log once at debug so a dev sees why nothing ships.
        log_missing_key_once();
        return;
    };

    let fda_granted = !crate::fda_gate::is_fda_pending_runtime();
    let config = config_shape::build_config_shape(&super::read_raw_settings(), fda_granted);
    let body = build_capture_body(api_key, event, props, &crate::install_id::analytics_id(), config);

    send_capture(body);
}

/// Builds the PostHog `/capture/` request body. Pure (no I/O, no gating), so it's directly
/// unit-testable. Shape:
///
/// ```json
/// {
///   "api_key": "phc_...",
///   "event": "<name>",
///   "distinct_id": "anal_<uuid>",
///   "properties": { "source": "desktop", ...props },
///   "$set": <config-shape>
/// }
/// ```
///
/// `source: "desktop"` is injected first so a stray `source` in `props` can't shadow it (and so the
/// dashboard can always split desktop events from the website's). The config-shape is the SAME
/// allowlisted object the heartbeat ships, so there's exactly one source of truth for person
/// properties.
fn build_capture_body(api_key: &str, event: &str, props: Value, distinct_id: &str, config: Value) -> Value {
    let mut properties = Map::new();
    properties.insert("source".to_string(), Value::String("desktop".to_string()));
    if let Value::Object(prop_map) = sanitize_props(event, props) {
        for (key, value) in prop_map {
            // `source` injected above wins: skip any caller-supplied `source`.
            properties.entry(key).or_insert(value);
        }
    }

    Value::Object(Map::from_iter([
        ("api_key".to_string(), Value::String(api_key.to_string())),
        ("event".to_string(), Value::String(event.to_string())),
        ("distinct_id".to_string(), Value::String(distinct_id.to_string())),
        ("properties".to_string(), Value::Object(properties)),
        ("$set".to_string(), config),
    ]))
}

/// Dev-build PII backstop: scans string prop VALUES for PII shapes and logs a scoped warning if one
/// slips through. This is a safety net for the open prop map, NOT a substitute for the PII-free
/// convention (every event must pass only enums / counts / bools by design). Numbers, bools, and
/// short enum strings pass freely; a string containing `/`, `\`, `@`, or a `~/` home prefix trips
/// the guard. Returns `props` unchanged either way (it never strips, only warns) so production
/// behavior is identical with the guard compiled out.
fn sanitize_props(event: &str, props: Value) -> Value {
    #[cfg(debug_assertions)]
    if let Value::Object(map) = &props {
        for (key, value) in map {
            if let Value::String(s) = value
                && looks_pii_shaped(s)
            {
                log::warn!(
                    target: "analytics",
                    "PostHog event '{event}' prop '{key}' looks PII-shaped (contains a path / email / home-prefix). \
                     Analytics props must be PII-free enums/counts/bools only; never paths, names, queries, or prompts."
                );
            }
        }
    }
    // Reference `event` on the release path so the param isn't flagged unused with the guard off.
    let _ = event;
    props
}

/// Whether a string value looks like PII (a path, email, or home-prefixed path). Heuristic, used
/// only by the debug-build [`sanitize_props`] net.
#[cfg(debug_assertions)]
fn looks_pii_shaped(s: &str) -> bool {
    s.starts_with("~/") || s.contains('/') || s.contains('\\') || s.contains('@')
}

/// Logs the "no PostHog key baked in" notice once per process so a local dev sees why feature events
/// don't ship, without spamming the log on every event.
fn log_missing_key_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::debug!(
            target: "analytics",
            "No CMDR_POSTHOG_KEY baked in (local dev build): PostHog feature events are a no-op"
        );
    });
}

/// Spawns the fire-and-forget POST. A failed capture is fine (the next event retries the channel);
/// we never block or surface the error.
fn send_capture(body: Value) {
    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder().timeout(CAPTURE_TIMEOUT).build() {
            Ok(c) => c,
            Err(e) => {
                log::warn!(target: "analytics", "Couldn't build PostHog HTTP client: {e}");
                return;
            }
        };
        match client.post(CAPTURE_URL).json(&body).send().await {
            Ok(response) if response.status().is_success() => {
                log::debug!(target: "analytics", "PostHog event sent ({})", response.status());
            }
            Ok(response) => {
                log::warn!(target: "analytics", "PostHog server returned {}", response.status());
            }
            Err(e) => {
                log::debug!(target: "analytics", "PostHog send failed: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capture_body_has_expected_shape() {
        let config = json!({ "theme.mode": "dark", "fdaGranted": true });
        let body = build_capture_body(
            "phc_test",
            "pane_navigated",
            json!({ "volume_kind": "local" }),
            "anal_178c8e27-511f-4f0e-a1fc-6a44f2ab7341",
            config.clone(),
        );

        assert_eq!(body["api_key"], json!("phc_test"));
        assert_eq!(body["event"], json!("pane_navigated"));
        assert_eq!(body["distinct_id"], json!("anal_178c8e27-511f-4f0e-a1fc-6a44f2ab7341"));
        // `source: "desktop"` is always injected.
        assert_eq!(body["properties"]["source"], json!("desktop"));
        // Arbitrary props pass through.
        assert_eq!(body["properties"]["volume_kind"], json!("local"));
        // `$set` is the config-shape verbatim (one source of truth for person properties).
        assert_eq!(body["$set"], config);
    }

    #[test]
    fn distinct_id_is_the_anal_id() {
        let body = build_capture_body("phc_test", "app_launched", json!({}), "anal_abc", json!({}));
        let distinct = body["distinct_id"].as_str().expect("string");
        assert!(
            distinct.starts_with("anal_"),
            "distinct_id must be the analytics id: {distinct}"
        );
    }

    #[test]
    fn injected_source_cannot_be_shadowed_by_props() {
        // A caller passing `source: "sneaky"` must not override the injected `desktop` value.
        let body = build_capture_body("phc_test", "e", json!({ "source": "sneaky" }), "anal_x", json!({}));
        assert_eq!(body["properties"]["source"], json!("desktop"));
    }

    #[test]
    fn arbitrary_props_are_open_ended() {
        // The event API is open: any PII-free prop map passes through, no fixed schema.
        let body = build_capture_body(
            "phc_test",
            "file_transfer_completed",
            json!({ "op": "copy", "item_count": "11-100", "had_conflicts": false }),
            "anal_x",
            json!({}),
        );
        assert_eq!(body["properties"]["op"], json!("copy"));
        assert_eq!(body["properties"]["item_count"], json!("11-100"));
        assert_eq!(body["properties"]["had_conflicts"], json!(false));
    }

    // The PII backstop only runs in debug builds (where these tests run under `cargo nextest`).
    #[cfg(debug_assertions)]
    #[test]
    fn pii_guard_trips_on_pii_shaped_strings() {
        assert!(looks_pii_shaped("/Users/dave/secret"), "absolute path");
        assert!(looks_pii_shaped("~/Documents"), "home prefix");
        assert!(looks_pii_shaped("person@example.com"), "email");
        assert!(looks_pii_shaped("C:\\Users\\dave"), "windows path");
        assert!(looks_pii_shaped("photos/sunset.jpg"), "relative path");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn pii_guard_passes_plain_enums_and_values() {
        // Categorical enums, buckets, and plain words are not PII-shaped.
        assert!(!looks_pii_shaped("local"));
        assert!(!looks_pii_shaped("copy"));
        assert!(!looks_pii_shaped("11-100"));
        assert!(!looks_pii_shaped("disconnected"));
        assert!(!looks_pii_shaped("filename"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn sanitize_props_returns_props_unchanged() {
        // The guard only warns; it never strips. A PII-shaped value still passes through (so the
        // dev sees the warning AND the bug isn't silently masked).
        let props = json!({ "volume_kind": "local", "leaked": "/Users/dave" });
        let out = sanitize_props("test_event", props.clone());
        assert_eq!(out, props);
    }
}

//! Feature events: the one backend path for all product analytics events.
//!
//! See `analytics/CLAUDE.md` for the full model. In short: backend code calls [`capture`] directly,
//! frontend code calls the `track_event` IPC command (which calls [`capture`]). Both ride the SAME
//! consent gate and non-prod suppression as the heartbeat. An event makes no request of its own: it
//! lands in the on-disk spool, and the next heartbeat carries it to our server, which forwards it.
//!
//! Events are an OPEN set: [`capture`] takes an arbitrary event name plus an arbitrary PII-free prop
//! map, so adding an event later is a one-line call with whatever categorical props that event
//! needs. The PII-free convention (enums, counts, bools only; never paths, names, queries, prompts)
//! is enforced socially by review and backstopped in debug builds by [`sanitize_props`].

use super::SendPermission;
use super::spool::SpooledEvent;
use serde_json::{Map, Value};

/// The longest event name the server accepts.
const MAX_EVENT_NAME_LEN: usize = 100;

/// Records one feature event. Never blocks on disk or network: the spool append runs on a blocking
/// worker, and the network send is the heartbeat's.
///
/// A no-op in any non-production instance (debug build, CI, an E2E shard, any isolated data dir;
/// unless `CMDR_ANALYTICS_FORCE=1`) and when the user opted out (`analytics.enabled ==
/// Some(false)`). `props` is an arbitrary PII-free object; pass `serde_json::json!({})` for an event
/// with no properties.
pub fn capture(event: &str, props: Value) {
    match super::send_permission() {
        SendPermission::Granted => {}
        SendPermission::Suppressed(reason) => {
            log::trace!(target: "analytics", "Event '{event}' suppressed ({reason}, no force override)");
            return;
        }
        // Fully silent: an opted-out install records nothing at all.
        SendPermission::OptedOut => return,
    }
    let Some(spooled) = spooled_event(event, props, EventStamp::now()) else {
        log::warn!(target: "analytics", "Event '{event}' dropped: names are 1-100 chars of a-z, 0-9, _, and $");
        return;
    };
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(spool) = super::spool() {
            spool.append(&spooled);
        }
    });
}

/// What makes one firing of an event distinct: when, which id, and which build.
struct EventStamp {
    timestamp: String,
    id: String,
    app_version: &'static str,
}

impl EventStamp {
    /// A stamp for an event firing now: RFC 3339 UTC with milliseconds (for example
    /// `2026-09-24T10:00:00.123Z`), a fresh v4 id, and this build's version.
    fn now() -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            id: uuid::Uuid::new_v4().to_string(),
            app_version: env!("CARGO_PKG_VERSION"),
        }
    }
}

/// What `event` fired with `props` looks like in the spool, or `None` when the name isn't one the
/// server accepts. Props that aren't an object become no props. Pure, so the shape is
/// unit-testable.
fn spooled_event(event: &str, props: Value, stamp: EventStamp) -> Option<SpooledEvent> {
    if !is_valid_event_name(event) {
        return None;
    }
    let properties = match sanitize_props(event, props) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    Some(SpooledEvent {
        event: event.to_string(),
        timestamp: stamp.timestamp,
        id: stamp.id,
        app_version: stamp.app_version.to_string(),
        properties,
    })
}

/// 1–100 chars of `[a-z0-9_$]`, what the heartbeat's validator accepts (`$` for PostHog's own
/// reserved names).
fn is_valid_event_name(event: &str) -> bool {
    (1..=MAX_EVENT_NAME_LEN).contains(&event.len())
        && event
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'$')
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
                    // allowed-log-private: an analytics prop key is a name in our own code
                    "Event '{event}' prop '{key}' looks PII-shaped (contains a path / email / home-prefix). \
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const AT: &str = "2026-09-24T10:00:00.123Z";
    const ID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

    fn stamp() -> EventStamp {
        EventStamp {
            timestamp: AT.to_string(),
            id: ID.to_string(),
            app_version: "1.2.3",
        }
    }

    #[test]
    fn an_event_spools_its_own_props_and_its_stamp() {
        let e = spooled_event("pane_navigated", json!({ "volume_kind": "local" }), stamp()).expect("valid");
        let value = serde_json::to_value(&e).expect("serialize");
        assert_eq!(
            value,
            json!({
                "event": "pane_navigated",
                "timestamp": AT,
                "id": ID,
                "appVersion": "1.2.3",
                "properties": { "volume_kind": "local" },
            })
        );
    }

    /// The id is what lets the server store a retried beat's events once, so every event gets its
    /// own, and the version is the build that fired it, not the one that happens to send it.
    #[test]
    fn a_fresh_stamp_carries_a_unique_v4_id_and_this_build() {
        let (a, b) = (EventStamp::now(), EventStamp::now());
        assert_ne!(a.id, b.id);
        let parsed = uuid::Uuid::parse_str(&a.id).expect("a uuid");
        assert_eq!(parsed.get_version_num(), 4);
        assert_eq!(a.id, parsed.hyphenated().to_string(), "lowercase hyphenated");
        assert_eq!(a.app_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn nothing_is_injected_into_the_props() {
        let e = spooled_event("app_launched", json!({}), stamp()).expect("valid");
        assert!(e.properties.is_empty());
    }

    #[test]
    fn arbitrary_props_are_open_ended() {
        let e = spooled_event(
            "file_transfer_completed",
            json!({ "op": "copy", "item_count": "11-100", "had_conflicts": false, "open_tabs": 3 }),
            stamp(),
        )
        .expect("valid");
        assert_eq!(e.properties["op"], json!("copy"));
        assert_eq!(e.properties["item_count"], json!("11-100"));
        assert_eq!(e.properties["had_conflicts"], json!(false));
        assert_eq!(e.properties["open_tabs"], json!(3));
    }

    #[test]
    fn props_that_arent_an_object_become_no_props() {
        let e = spooled_event("app_launched", json!(["x"]), stamp()).expect("valid");
        assert!(e.properties.is_empty());
    }

    #[test]
    fn names_the_server_would_refuse_are_dropped() {
        assert!(spooled_event("", json!({}), stamp()).is_none());
        assert!(spooled_event("Pane_Navigated", json!({}), stamp()).is_none());
        assert!(spooled_event("pane-navigated", json!({}), stamp()).is_none());
        assert!(spooled_event("pane navigated", json!({}), stamp()).is_none());
        assert!(spooled_event(&"a".repeat(101), json!({}), stamp()).is_none());
        assert!(spooled_event(&"a".repeat(100), json!({}), stamp()).is_some());
        assert!(spooled_event("$pageview", json!({}), stamp()).is_some());
        assert!(spooled_event("tab_opened_2", json!({}), stamp()).is_some());
    }

    #[test]
    fn timestamps_are_rfc3339_utc_with_milliseconds() {
        let now = EventStamp::now().timestamp;
        assert!(now.ends_with('Z'), "{now}");
        assert!(chrono::DateTime::parse_from_rfc3339(&now).is_ok(), "{now}");
        assert_eq!(now.len(), AT.len(), "{now}");
    }

    // The PII backstop only runs in debug builds (where these tests run under `cargo nextest`).
    #[cfg(debug_assertions)]
    #[test]
    fn pii_guard_trips_on_pii_shaped_strings() {
        assert!(looks_pii_shaped("/Users/test_user_10/secret"), "absolute path");
        assert!(looks_pii_shaped("~/Documents"), "home prefix");
        assert!(looks_pii_shaped("person@example.com"), "email");
        assert!(looks_pii_shaped("C:\\Users\\test_user_10"), "windows path");
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
        let props = json!({ "volume_kind": "local", "leaked": "/Users/test_user_10" });
        let out = sanitize_props("test_event", props.clone());
        assert_eq!(out, props);
    }
}

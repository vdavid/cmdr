//! The heartbeat: the one request analytics makes, at most once per three hours.
//!
//! A loop wakes every [`WAKE_TICK`], adds the time since the last tick to the unreported uptime,
//! and sends `POST /heartbeat` when [`CADENCE`] says one is due. A beat carries the install's
//! identity, the PII-free config shape, `uptimeSeconds` (app runtime no earlier beat reported), and
//! up to [`MAX_EVENTS_PER_BEAT`] events from the front of the spool. Wire contract:
//! `docs/specs/network-chatter-plan.md`.
//!
//! What a beat accounts for leaves local state only when the server acknowledged it with a 2xx:
//! the spool drops exactly that batch, and the unreported uptime drops by exactly what was sent. The
//! schedule and the uptime live in `analytics-heartbeat.json`, so a relaunch neither resets the
//! three hours nor loses a short session's time.

use super::spool::{Spool, SpooledEvent};
use super::{SendPermission, config_shape};
use crate::managed_policy::{Egress, ManagedPolicy};
use crate::send_schedule::{SendCadence, SendRecord, now_unix_ms};
use crate::server_request::ServerRequestError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Heartbeat ingestion endpoint. Debug builds hit the local Worker; release hits production.
#[cfg(debug_assertions)]
const HEARTBEAT_URL: &str = "http://localhost:8787/heartbeat";
#[cfg(not(debug_assertions))]
const HEARTBEAT_URL: &str = "https://api.getcmdr.com/heartbeat";

/// At most one acknowledged beat per three hours; a beat that didn't land is retried no sooner than
/// 15 minutes later.
pub(super) const CADENCE: SendCadence = SendCadence {
    interval: Duration::from_secs(3 * 60 * 60),
    retry_floor: Duration::from_secs(15 * 60),
};

/// How often the loop wakes to count uptime and ask whether a beat is due.
const WAKE_TICK: Duration = Duration::from_secs(5 * 60);

/// Network timeout for one beat. A beat can carry a couple of hundred KB of events.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(20);

/// The server keeps at most this many events per beat, so the client never sends more.
pub(super) const MAX_EVENTS_PER_BEAT: usize = 500;

/// Serialized event bytes per beat. The server caps the body at 256 KB and the config at 16 KB, so
/// this leaves room for both plus the JSON around them.
const EVENT_BYTES_PER_BEAT: usize = 192 * 1024;

/// The loop's persisted state, in the app data dir.
const STATE_FILE_NAME: &str = "analytics-heartbeat.json";

/// The `/heartbeat` request body. camelCase on the wire, matching the Worker's validator.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatPayload {
    /// `anal_` + a lowercase hyphenated v4 UUID. Matches `^anal_[0-9a-f-]{36}$`.
    anal_id: String,
    /// Semver `x.y.z` from `CARGO_PKG_VERSION`.
    app_version: String,
    /// Human-readable OS version, always non-empty.
    os_version: String,
    /// `aarch64` / `x86_64`.
    arch: String,
    /// `"release"` / `"debug"`.
    build_mode: Option<String>,
    /// The PII-free config-shape snapshot, stored verbatim by the server.
    config: serde_json::Value,
    /// App runtime this beat accounts for that no earlier acknowledged beat reported.
    uptime_seconds: u64,
    /// Spooled feature events, oldest first, at most [`MAX_EVENTS_PER_BEAT`].
    events: Vec<SpooledEvent>,
}

/// What `analytics-heartbeat.json` holds.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatState {
    #[serde(flatten)]
    schedule: SendRecord,
    /// Whole seconds of runtime no acknowledged beat has reported yet.
    #[serde(default)]
    unreported_uptime_seconds: u64,
}

/// How a beat ended, as far as local state cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BeatOutcome {
    /// 2xx: the server stored the beat and its events.
    Acknowledged,
    /// The server refused these exact bytes (400, 413, 422), so sending them again can't work.
    Refused,
    /// Anything else: no answer, a timeout, a 5xx, a 429. Worth retrying unchanged.
    Failed,
    /// The organization's policy turned usage stats off since the permission check, so nothing
    /// went out. Cleans up like an opt-out.
    BlockedByPolicy,
}

impl BeatOutcome {
    fn from_status(status: u16) -> Self {
        match status {
            200..=299 => Self::Acknowledged,
            400 | 413 | 422 => Self::Refused,
            _ => Self::Failed,
        }
    }
}

/// Applies a beat's outcome to the persisted state. Returns whether the spool should drop the batch
/// the beat carried.
///
/// A refused batch is dropped rather than retried: the same bytes would be refused every 15 minutes
/// forever, and the heartbeat under them (the daily-active signal) with them. Its uptime is kept,
/// since uptime alone can't be what the server objected to.
fn settle(state: &mut HeartbeatState, outcome: BeatOutcome, sent_uptime: u64, now_ms: i64) -> bool {
    match outcome {
        BeatOutcome::Acknowledged => {
            state.schedule.record_success(now_ms);
            state.unreported_uptime_seconds = state.unreported_uptime_seconds.saturating_sub(sent_uptime);
            true
        }
        BeatOutcome::Refused => {
            state.schedule.record_failure(now_ms);
            true
        }
        BeatOutcome::Failed => {
            state.schedule.record_failure(now_ms);
            false
        }
        BeatOutcome::BlockedByPolicy => false,
    }
}

/// Drops everything collected but not yet sent: the spooled events and the unreported uptime. An
/// opt-out (the user's or the organization's) is fully silent, and nothing collected before it is
/// sent later either. Returns whether `state` changed and needs saving.
fn forget_unsent(state: &mut HeartbeatState, spool: Option<&Spool>) -> bool {
    if let Some(spool) = spool {
        spool.clear();
    }
    let had_uptime = state.unreported_uptime_seconds > 0;
    state.unreported_uptime_seconds = 0;
    had_uptime
}

/// Adds `elapsed` to the unreported uptime in whole seconds, keeping the sub-second remainder in
/// `carry` so five-minute ticks don't lose a second each.
fn add_uptime(state: &mut HeartbeatState, carry: &mut Duration, elapsed: Duration) {
    let total = *carry + elapsed;
    state.unreported_uptime_seconds = state.unreported_uptime_seconds.saturating_add(total.as_secs());
    *carry = Duration::from_nanos(u64::from(total.subsec_nanos()));
}

/// Starts the loop. Call once from setup, after `analytics::init`.
pub(super) fn start() {
    let Some(data_dir) = super::data_dir() else {
        log::warn!(target: "analytics", "Heartbeat not started: no app data dir");
        return;
    };
    tauri::async_runtime::spawn(run(data_dir.join(STATE_FILE_NAME)));
}

async fn run(state_path: PathBuf) {
    let mut state = load_state(&state_path);
    let mut last_tick = Instant::now();
    let mut carry = Duration::ZERO;
    let mut logged_suppression = false;
    loop {
        // `Instant` doesn't advance while the machine sleeps (macOS and Linux both), so a closed
        // lid isn't counted as runtime.
        let elapsed = last_tick.elapsed();
        last_tick = Instant::now();

        match super::send_permission() {
            SendPermission::Suppressed(reason) => {
                if !logged_suppression {
                    log::debug!(target: "analytics", "Heartbeat suppressed ({reason}, no force override)");
                    logged_suppression = true;
                }
            }
            SendPermission::OptedOut => {
                if forget_unsent(&mut state, super::spool()) {
                    save_state(&state_path, &state);
                }
                carry = Duration::ZERO;
            }
            SendPermission::Granted => {
                add_uptime(&mut state, &mut carry, elapsed);
                if state.schedule.due_in(now_unix_ms(), CADENCE).is_zero()
                    && let Some(spool) = super::spool()
                {
                    let _ = beat(&mut state, spool).await;
                }
                save_state(&state_path, &state);
            }
        }

        tokio::time::sleep(WAKE_TICK).await;
    }
}

async fn beat(state: &mut HeartbeatState, spool: &Spool) -> BeatOutcome {
    let mut batch = spool.take_batch(MAX_EVENTS_PER_BEAT, EVENT_BYTES_PER_BEAT);
    let sent_uptime = state.unreported_uptime_seconds;
    let events = crate::pluralize::pluralize(batch.events.len() as u64, "event");
    let payload = build_payload(sent_uptime, std::mem::take(&mut batch.events));

    let outcome = send_payload(&payload).await;
    if settle(state, outcome, sent_uptime, now_unix_ms()) {
        spool.acknowledge(&batch);
    }
    if outcome == BeatOutcome::BlockedByPolicy {
        forget_unsent(state, Some(spool));
    }
    match outcome {
        BeatOutcome::Acknowledged => {
            log::debug!(target: "analytics", "Heartbeat sent ({sent_uptime} s uptime, {events})");
        }
        BeatOutcome::Refused => {
            log::warn!(
                target: "analytics",
                "Heartbeat refused; dropped the {events} it carried. Check the Worker's heartbeat validator"
            );
        }
        BeatOutcome::Failed | BeatOutcome::BlockedByPolicy => {}
    }
    outcome
}

fn build_payload(uptime_seconds: u64, events: Vec<SpooledEvent>) -> HeartbeatPayload {
    let fda_granted = !crate::fda_gate::is_fda_pending_runtime();
    let config = config_for(
        &crate::managed_policy::current(),
        super::read_raw_settings(),
        fda_granted,
    );

    HeartbeatPayload {
        anal_id: crate::install_id::analytics_id(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: crate::platform::os_version(),
        arch: std::env::consts::ARCH.to_string(),
        build_mode: Some(current_build_mode().to_string()),
        config,
        uptime_seconds,
        events,
    }
}

/// The config shape of the EFFECTIVE settings: the organization's locks over the stored ones, plus
/// whether any policy is in force (one coarse bool, never which keys).
fn config_for(policy: &ManagedPolicy, mut settings: serde_json::Value, fda_granted: bool) -> serde_json::Value {
    crate::managed_policy::overlay(policy, &mut settings);
    config_shape::build_config_shape(
        &settings,
        config_shape::RuntimeState {
            fda_granted,
            managed_by_organization: policy.is_managed(),
        },
    )
}

fn current_build_mode() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "release" }
}

async fn send_payload(payload: &HeartbeatPayload) -> BeatOutcome {
    send_payload_to(HEARTBEAT_URL, payload).await
}

async fn send_payload_to(url: &str, payload: &HeartbeatPayload) -> BeatOutcome {
    let client = match cmdr_http::client_builder().timeout(HEARTBEAT_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "analytics", "Couldn't build heartbeat HTTP client: {e}");
            return BeatOutcome::Failed;
        }
    };

    match crate::server_request::send(Egress::Heartbeat, client.post(url).json(payload)).await {
        Ok(_) => BeatOutcome::Acknowledged,
        Err(ServerRequestError::Refused { status, .. }) => {
            let outcome = BeatOutcome::from_status(status);
            if outcome == BeatOutcome::Failed {
                log::debug!(target: "analytics", "Heartbeat server returned {status}");
            }
            outcome
        }
        Err(ServerRequestError::BlockedByPolicy) => BeatOutcome::BlockedByPolicy,
        Err(e) => {
            // A beat that didn't land is fine: the next one after the retry floor carries it all.
            log::debug!(target: "analytics", "Heartbeat send failed: {e}");
            BeatOutcome::Failed
        }
    }
}

fn load_state(path: &Path) -> HeartbeatState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_state(path: &Path, state: &HeartbeatState) {
    let Ok(content) = serde_json::to_string_pretty(state) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = crate::config::durable_write_json(path, &tmp, &content) {
        log::debug!(target: "analytics", "Couldn't persist heartbeat state: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::testing;
    use serde_json::json;

    fn payload(events: Vec<SpooledEvent>) -> HeartbeatPayload {
        HeartbeatPayload {
            anal_id: "anal_178c8e27-511f-4f0e-a1fc-6a44f2ab7341".to_string(),
            app_version: "1.2.3".to_string(),
            os_version: "macOS 26.0".to_string(),
            arch: "aarch64".to_string(),
            build_mode: Some("release".to_string()),
            config: json!({ "theme.mode": "dark", "fdaGranted": true }),
            uptime_seconds: 5400,
            events,
        }
    }

    fn event() -> SpooledEvent {
        SpooledEvent {
            event: "pane_navigated".to_string(),
            timestamp: "2026-09-24T10:00:00.123Z".to_string(),
            id: "0f8fad5b-d9cb-469f-a165-70867728950e".to_string(),
            app_version: "1.2.2".to_string(),
            properties: json!({ "volume_kind": "local" })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// The field names are the wire contract with the Worker (`docs/specs/network-chatter-plan.md`
    /// § Wire contract). A rename here silently loses the field on the server.
    #[test]
    fn payload_field_names_match_the_wire_contract() {
        let value = serde_json::to_value(payload(vec![event()])).expect("serialize");
        let mut keys: Vec<&str> = value.as_object().expect("object").keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "analId",
                "appVersion",
                "arch",
                "buildMode",
                "config",
                "events",
                "osVersion",
                "uptimeSeconds"
            ]
        );
        let mut event_keys: Vec<&str> = value["events"][0]
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        event_keys.sort_unstable();
        assert_eq!(event_keys, ["appVersion", "event", "id", "properties", "timestamp"]);
    }

    #[test]
    fn payload_carries_values_verbatim() {
        let value = serde_json::to_value(payload(vec![event()])).expect("serialize");
        assert_eq!(value["analId"], json!("anal_178c8e27-511f-4f0e-a1fc-6a44f2ab7341"));
        assert_eq!(value["appVersion"], json!("1.2.3"));
        assert_eq!(value["osVersion"], json!("macOS 26.0"));
        assert_eq!(value["arch"], json!("aarch64"));
        assert_eq!(value["buildMode"], json!("release"));
        assert_eq!(value["config"]["theme.mode"], json!("dark"));
        assert_eq!(value["uptimeSeconds"], json!(5400));
        assert_eq!(value["events"][0]["event"], json!("pane_navigated"));
        assert_eq!(value["events"][0]["timestamp"], json!("2026-09-24T10:00:00.123Z"));
        assert_eq!(value["events"][0]["id"], json!("0f8fad5b-d9cb-469f-a165-70867728950e"));
        // The event keeps the version that produced it, whatever the beat's own version is.
        assert_eq!(value["events"][0]["appVersion"], json!("1.2.2"));
        assert_eq!(value["events"][0]["properties"], json!({ "volume_kind": "local" }));
    }

    #[test]
    fn a_beat_with_nothing_spooled_still_sends_an_empty_events_array() {
        let mut p = payload(vec![]);
        p.build_mode = None;
        let value = serde_json::to_value(p).expect("serialize");
        assert_eq!(value["events"], json!([]));
        assert_eq!(value["buildMode"], json!(null));
    }

    #[test]
    fn statuses_map_to_outcomes() {
        assert_eq!(BeatOutcome::from_status(200), BeatOutcome::Acknowledged);
        assert_eq!(BeatOutcome::from_status(204), BeatOutcome::Acknowledged);
        assert_eq!(BeatOutcome::from_status(400), BeatOutcome::Refused);
        assert_eq!(BeatOutcome::from_status(413), BeatOutcome::Refused);
        assert_eq!(BeatOutcome::from_status(422), BeatOutcome::Refused);
        assert_eq!(BeatOutcome::from_status(429), BeatOutcome::Failed);
        assert_eq!(BeatOutcome::from_status(500), BeatOutcome::Failed);
        assert_eq!(BeatOutcome::from_status(503), BeatOutcome::Failed);
    }

    async fn server_answering(status: u16) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(status))
            .mount(&server)
            .await;
        server
    }

    /// The beat rides `server_request::send`; its statuses still land on the same outcomes.
    #[tokio::test]
    async fn a_sent_beat_maps_the_servers_answer_as_before() {
        for (status, expected) in [
            (200, BeatOutcome::Acknowledged),
            (422, BeatOutcome::Refused),
            (413, BeatOutcome::Refused),
            (429, BeatOutcome::Failed),
            (503, BeatOutcome::Failed),
        ] {
            let server = server_answering(status).await;
            assert_eq!(
                send_payload_to(&server.uri(), &payload(vec![])).await,
                expected,
                "{status}"
            );
        }
    }

    #[tokio::test]
    async fn a_beat_that_gets_no_answer_failed() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("bind loopback")
            .local_addr()
            .expect("a bound socket has an address")
            .port();
        let outcome = send_payload_to(&format!("http://127.0.0.1:{port}/heartbeat"), &payload(vec![])).await;
        assert_eq!(outcome, BeatOutcome::Failed);
    }

    #[tokio::test]
    async fn a_managed_off_beat_sends_nothing() {
        let server = server_answering(200).await;
        let _policy = testing::override_for_test(testing::forcing(&[testing::DISABLE_USAGE_STATS]));

        let outcome = send_payload_to(&server.uri(), &payload(vec![event()])).await;

        assert_eq!(outcome, BeatOutcome::BlockedByPolicy);
        assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 0);
    }

    /// A policy that arrives between the permission check and the send cleans up exactly like an
    /// opt-out: nothing collected while it was on goes out later.
    #[tokio::test]
    async fn a_beat_the_policy_blocks_forgets_the_spool_and_the_uptime() {
        let dir = crate::test_support::TestDir::new("heartbeat-blocked-by-policy");
        let spool = Spool::new(dir.join("events.jsonl"));
        spool.append(&event());
        let mut s = state(600);
        let _policy = testing::override_for_test(testing::forcing(&[testing::DISABLE_USAGE_STATS]));

        let outcome = beat(&mut s, &spool).await;

        assert_eq!(outcome, BeatOutcome::BlockedByPolicy);
        assert_eq!(s.unreported_uptime_seconds, 0);
        assert!(
            spool
                .take_batch(MAX_EVENTS_PER_BEAT, EVENT_BYTES_PER_BEAT)
                .events
                .is_empty()
        );
        assert_eq!(s.schedule, SendRecord::default(), "a blocked beat isn't a failed send");
    }

    /// The heartbeat reports what's in force, so a stored `true` under a managed off reads `false`.
    #[test]
    fn the_config_shape_reports_effective_values_and_the_managed_flag() {
        let stored = json!({
            "analytics.enabled": true,
            "updates.crashReports": true,
            "updates.errorReports": true,
            "theme.mode": "dark",
        });
        let policy = testing::forcing(&[testing::DISABLE_USAGE_STATS, testing::DISABLE_CRASH_AND_ERROR_REPORTS]);

        let config = config_for(&policy, stored.clone(), true);

        assert_eq!(config["analytics.enabled"], json!(false));
        assert_eq!(config["updates.crashReports"], json!(false));
        assert_eq!(config["updates.errorReports"], json!(false));
        assert_eq!(config["theme.mode"], json!("dark"));
        assert_eq!(config["managedByOrganization"], json!(true));

        let unmanaged = config_for(&ManagedPolicy::default(), stored, true);
        assert_eq!(unmanaged["analytics.enabled"], json!(true));
        assert_eq!(unmanaged["managedByOrganization"], json!(false));
    }

    fn state(unreported: u64) -> HeartbeatState {
        HeartbeatState {
            schedule: SendRecord::default(),
            unreported_uptime_seconds: unreported,
        }
    }

    /// Uptime counted while a beat was on the wire isn't what the beat reported, so only the sent
    /// amount comes off.
    #[test]
    fn an_acknowledged_beat_subtracts_only_what_it_sent() {
        let mut s = state(700);
        assert!(settle(&mut s, BeatOutcome::Acknowledged, 600, 1_000));
        assert_eq!(s.unreported_uptime_seconds, 100);
        assert_eq!(s.schedule.last_success_ms, Some(1_000));
    }

    #[test]
    fn a_failed_beat_keeps_everything_for_the_retry() {
        let mut s = state(600);
        assert!(!settle(&mut s, BeatOutcome::Failed, 600, 1_000));
        assert_eq!(s.unreported_uptime_seconds, 600);
        assert_eq!(s.schedule.last_failure_ms, Some(1_000));
        assert_eq!(s.schedule.last_success_ms, None);
    }

    #[test]
    fn a_refused_beat_drops_its_events_and_keeps_its_uptime() {
        let mut s = state(600);
        assert!(settle(&mut s, BeatOutcome::Refused, 600, 1_000));
        assert_eq!(s.unreported_uptime_seconds, 600);
        assert_eq!(s.schedule.last_failure_ms, Some(1_000));
    }

    #[test]
    fn uptime_accumulates_whole_seconds_and_carries_the_rest() {
        let mut s = state(0);
        let mut carry = Duration::ZERO;
        add_uptime(&mut s, &mut carry, Duration::from_millis(1_600));
        add_uptime(&mut s, &mut carry, Duration::from_millis(1_600));
        assert_eq!(s.unreported_uptime_seconds, 3);
        assert_eq!(carry, Duration::from_millis(200));
    }

    #[test]
    fn state_round_trips_and_a_missing_file_is_a_fresh_start() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(STATE_FILE_NAME);
        assert_eq!(load_state(&path), HeartbeatState::default());

        let mut s = state(42);
        s.schedule.record_success(7);
        save_state(&path, &s);
        assert_eq!(load_state(&path), s);
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            on_disk,
            json!({ "lastSuccessMs": 7, "lastFailureMs": null, "unreportedUptimeSeconds": 42 })
        );
    }

    #[test]
    fn the_cadence_is_three_hours_with_a_fifteen_minute_retry_floor() {
        assert_eq!(CADENCE.interval, Duration::from_secs(10_800));
        assert_eq!(CADENCE.retry_floor, Duration::from_secs(900));
    }
}

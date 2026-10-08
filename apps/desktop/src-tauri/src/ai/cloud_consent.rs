//! "Allow cloud AI": the one consent every cloud LLM call needs, made STRUCTURAL.
//!
//! Nothing reaches a cloud AI service until the user turns this on. The Settings switch is the
//! UX layer; this is the enforcement. ⚠️ **The predicate has ONE caller path into the send
//! decision: [`super::manager::resolve_backend`]**, which every LLM call in the app resolves its
//! backend through (folder suggestions, search and selection translation, MCP `ai_search`, both
//! Ask Cmdr slots). Beside it, only the wake readiness snapshot (through the same resolution),
//! [`super::connection_check::check_ai_connection`], and the status command read it. Local AI
//! needs no consent: nothing leaves the Mac.
//!
//! The record lives in `main.db`'s `meta` table (`agent::store::ConsentRecord::CloudAi`), and
//! every check fails CLOSED: an absent record, a stale version, an unreadable store, or a "no"
//! still held for the store all read as "not allowed".

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::Event as _;

use crate::agent::AgentDb;
use crate::agent::store::{self, AgentStoreError, ConsentRecord};
use crate::managed_policy::{ManagedAiRefusal, ManagedPolicy};

const LOG_TARGET: &str = "ai::cloud_consent";

/// The consent-copy version the user must have accepted for cloud AI to be allowed. **Bump this
/// whenever the `ai.cloudConsent.*` copy changes materially**, so a stale acceptance no longer
/// counts and users re-consent to the new wording. The copy lives in the frontend catalog; this
/// integer is its machine-checkable version, recorded in `main.db`.
pub const CLOUD_AI_CONSENT_VERSION: u32 = 1;

/// Whether a "no" to cloud AI is still waiting to reach `main.db`.
///
/// Turning the switch off (or onboarding's "no AI" pick) clears the record in `main.db`. When
/// the store refuses that write twice, the frontend holds the answer in `settings.json`
/// (`ai.cloudConsentRevokePending`) and retries until the store takes it. It lives there
/// because `main.db` is the store that just refused. ⚠️ **Every consent check takes it**, so
/// the "no" holds on the very next check and the retry only makes the store catch up: that's
/// why it's an argument of [`has_current_cloud_consent`] rather than something a caller may
/// remember to check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevokePending {
    No,
    Yes,
}

impl RevokePending {
    /// Read fresh from `settings.json`. An absent or unreadable marker reads as `No`, since the
    /// store record is then the only answer there is; the marker only ever exists on the path
    /// where a revoke was refused.
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        if crate::settings::load_cloud_consent_revoke_pending(app) {
            Self::Yes
        } else {
            Self::No
        }
    }
}

/// Whether the user has allowed cloud AI under the CURRENT consent copy. Fails CLOSED: an absent
/// record, a stale version, an unreadable store, or a "no" still held for the store all read as
/// "not allowed".
///
/// This is the single predicate every gate calls. The organization's managed policy is one more
/// input, the way [`RevokePending`] is: when it rules out every cloud host, the answer is "not
/// allowed" whatever the record says, and the record stays untouched for when the profile goes.
/// A host list doesn't count here; it narrows where cloud AI goes, not whether the user agreed.
pub fn has_current_cloud_consent(conn: &Connection, revoke: RevokePending, policy: &ManagedPolicy) -> bool {
    policy.any_cloud_refusal().is_none()
        && revoke == RevokePending::No
        && matches!(
            store::get_consent(conn, ConsentRecord::CloudAi),
            Ok(Some(consent)) if consent.version == CLOUD_AI_CONSENT_VERSION
        )
}

/// [`has_current_cloud_consent`] against the live app: no agent store, a connection that won't
/// open, or anything else unreadable answers `false` with a warning.
pub(crate) fn cloud_consent_from_app<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(db) = app.try_state::<AgentDb>() else {
        log::warn!(target: LOG_TARGET, "no agent store to read cloud AI consent from, refusing");
        return false;
    };
    match db.open_read_connection() {
        Ok(conn) => has_current_cloud_consent(&conn, RevokePending::load(app), &crate::managed_policy::current()),
        Err(e) => {
            log::warn!(target: LOG_TARGET, "reading cloud AI consent failed, refusing: {e}");
            false
        }
    }
}

// ── IPC ──────────────────────────────────────────────────────────────────────────

/// Whether the user allowed cloud AI, and the audit of what they accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudAiConsentStatus {
    /// True only when the user accepted the CURRENT `current_version` and no "no" is held for
    /// the store: exactly what every cloud gate answers. The one flag the switch reads.
    pub accepted: bool,
    /// The copy version the user must have accepted to be `accepted`.
    pub current_version: u32,
    /// The version the user last accepted, or `None` if never.
    pub accepted_version: Option<u32>,
    /// When the user last accepted (unix secs), or `None` if never.
    pub accepted_at: Option<i64>,
    /// Set when the organization's policy rules out every cloud host: the switch is locked off
    /// for this reason, whatever the record says.
    pub managed: Option<ManagedAiRefusal>,
}

/// Why a consent write didn't land. The frontend re-reads the status either way; this tells it
/// whether to retry or hold a "no".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CloudAiConsentWriteError {
    /// The agent store never opened this launch, so there's nowhere to record anything.
    StoreUnavailable,
    /// `main.db` refused the write. `detail` is for logs only.
    StoreRefused { detail: String },
}

impl From<AgentStoreError> for CloudAiConsentWriteError {
    fn from(e: AgentStoreError) -> Self {
        CloudAiConsentWriteError::StoreRefused { detail: e.to_string() }
    }
}

/// Emitted after every consent write and every held-revoke change, so each window refreshes the
/// switch, the locked cloud setup, and the feature gates. No payload: listeners re-read the
/// status.
#[derive(Debug, Clone, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct CloudAiConsentChanged;

/// The cloud AI consent status. A missing or unreadable store reads as not accepted, so the gate
/// stays closed rather than failing open.
#[tauri::command]
#[specta::specta]
pub async fn cloud_ai_consent_status(app: AppHandle) -> CloudAiConsentStatus {
    let not_accepted = CloudAiConsentStatus {
        accepted: false,
        current_version: CLOUD_AI_CONSENT_VERSION,
        accepted_version: None,
        accepted_at: None,
        managed: crate::managed_policy::current().any_cloud_refusal(),
    };
    let revoke = RevokePending::load(&app);
    let policy = crate::managed_policy::current();
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return not_accepted;
    };
    let read = tauri::async_runtime::spawn_blocking(move || -> Result<CloudAiConsentStatus, AgentStoreError> {
        let conn = store::open_read_connection(&db_path)?;
        status_from(&conn, revoke, &policy)
    })
    .await;
    match read {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            log::warn!(target: LOG_TARGET, "reading the cloud AI consent status failed: {e}");
            not_accepted
        }
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the cloud AI consent status read didn't finish: {e}");
            not_accepted
        }
    }
}

/// The status as the store and the held "no" see it. Pure over a connection, so it's testable.
fn status_from(
    conn: &Connection,
    revoke: RevokePending,
    policy: &ManagedPolicy,
) -> Result<CloudAiConsentStatus, AgentStoreError> {
    let stored = store::get_consent(conn, ConsentRecord::CloudAi)?;
    Ok(CloudAiConsentStatus {
        accepted: has_current_cloud_consent(conn, revoke, policy),
        current_version: CLOUD_AI_CONSENT_VERSION,
        accepted_version: stored.map(|c| c.version),
        accepted_at: stored.map(|c| c.at),
        managed: policy.any_cloud_refusal(),
    })
}

/// Record the user's "Allow cloud AI" (timestamp + copy version). Idempotent. ❌ Only the
/// switch's own click may call this: nothing else grants consent.
#[tauri::command]
#[specta::specta]
pub async fn accept_cloud_ai_consent(app: AppHandle) -> Result<(), CloudAiConsentWriteError> {
    let now = now_secs();
    let recorded = write(&app, move |conn| {
        store::set_cloud_ai_consent(conn, CLOUD_AI_CONSENT_VERSION, now)
    })
    .await;
    announce_change(&app);
    recorded
}

/// Turn cloud AI off: clear the record, then stop every in-flight cloud call (running Ask Cmdr
/// turns, rail and wake alike, and folder-suggestion streams). The cancel runs whether or not
/// the store took the clear: the user said "stop", and a refused write is held as a "no" by the
/// frontend, which closes every gate on the next check anyway. `ai.provider` stays as it is.
#[tauri::command]
#[specta::specta]
pub async fn revoke_cloud_ai_consent(app: AppHandle) -> Result<(), CloudAiConsentWriteError> {
    let cleared = write(&app, store::clear_cloud_ai_consent).await;
    stop_in_flight_calls("cloud AI turned off");
    announce_change(&app);
    cleared
}

/// Tell the cloud gates that a held "no" (`ai.cloudConsentRevokePending`) was just set or let go
/// of. No value crosses: the gates read `settings.json` themselves, so the frontend calls this
/// right after saving it. A newly held "no" stops in-flight calls exactly like a revoke.
#[tauri::command]
#[specta::specta]
pub async fn cloud_ai_consent_revoke_pending_changed(app: AppHandle) {
    if RevokePending::load(&app) == RevokePending::Yes {
        stop_in_flight_calls("cloud AI turned off");
    }
    announce_change(&app);
}

/// Cancel every in-flight AI call: running Ask Cmdr turns and suggestion streams. `reason` labels
/// the log line. Translate calls are one-shot requests of a few seconds and aren't tracked: they
/// finish, and the next one refuses. Also what a narrowing managed policy runs (`super::managed`).
pub(super) fn stop_in_flight_calls(reason: &str) {
    let turns = crate::agent::chat::cancel::cancel_all();
    let streams = super::stream_registry::cancel_all();
    if turns + streams > 0 {
        log::info!(
            target: LOG_TARGET,
            "{reason}: stopped {turns} Ask Cmdr turn(s) and {streams} suggestion stream(s)"
        );
    }
}

/// The wake loop's readiness is a cached answer, and every window shows the switch: both hear
/// about a change here.
fn announce_change(app: &AppHandle) {
    crate::agent::wake::refresh_readiness(app);
    if let Err(e) = CloudAiConsentChanged.emit(app) {
        log::warn!(target: LOG_TARGET, "the cloud AI consent change didn't reach the windows: {e}");
    }
}

/// Run one consent write on a short-lived `main.db` connection off the IPC thread.
async fn write<F>(app: &AppHandle, op: F) -> Result<(), CloudAiConsentWriteError>
where
    F: FnOnce(&Connection) -> Result<(), AgentStoreError> + Send + 'static,
{
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return Err(CloudAiConsentWriteError::StoreUnavailable);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open_write_connection(&db_path)?;
        op(&conn).map_err(CloudAiConsentWriteError::from)
    })
    .await
    .map_err(|e| CloudAiConsentWriteError::StoreRefused { detail: e.to_string() })?
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::testing::{self, ALLOWED_CLOUD_AI_HOSTS, DISABLE_AI, DISABLE_CLOUD_AI};
    use crate::managed_policy::{ManagedAiRefusal, ManagedPolicy};

    fn no_policy() -> ManagedPolicy {
        ManagedPolicy::default()
    }

    /// A policy that rules out every cloud host closes the gate over a consent the store still
    /// records, without touching the record: remove the profile and the user's own answer is back.
    #[test]
    fn a_policy_without_cloud_ai_closes_the_gate_over_a_recorded_consent() {
        let conn = migrated_conn();
        store::set_cloud_ai_consent(&conn, CLOUD_AI_CONSENT_VERSION, 1_780_000_000).expect("set");
        for (policy, refusal) in [
            (testing::forcing(&[DISABLE_CLOUD_AI]), ManagedAiRefusal::CloudAiOff),
            (testing::forcing(&[DISABLE_AI]), ManagedAiRefusal::AiOff),
        ] {
            assert!(!has_current_cloud_consent(&conn, RevokePending::No, &policy));
            let status = status_from(&conn, RevokePending::No, &policy).expect("status");
            assert!(!status.accepted);
            assert_eq!(status.managed, Some(refusal));
            assert_eq!(
                status.accepted_version,
                Some(CLOUD_AI_CONSENT_VERSION),
                "the record stays"
            );
        }
        assert!(has_current_cloud_consent(&conn, RevokePending::No, &no_policy()));
    }

    /// A host list narrows WHERE cloud AI goes, not whether the user agreed to it.
    #[test]
    fn a_host_list_leaves_consent_to_the_user() {
        let conn = migrated_conn();
        store::set_cloud_ai_consent(&conn, CLOUD_AI_CONSENT_VERSION, 1_780_000_000).expect("set");
        let listed = testing::from_values(&[(
            ALLOWED_CLOUD_AI_HOSTS,
            plist::Value::Array(vec![plist::Value::String("api.openai.com".into())]),
        )]);
        assert!(has_current_cloud_consent(&conn, RevokePending::No, &listed));
        assert_eq!(
            status_from(&conn, RevokePending::No, &listed).expect("status").managed,
            None
        );
    }

    fn migrated_conn() -> Connection {
        let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
        store::run_migrations(&conn, store::MIGRATIONS).expect("migrate");
        conn
    }

    #[test]
    fn no_record_is_not_allowed() {
        let conn = migrated_conn();
        assert!(
            !has_current_cloud_consent(&conn, RevokePending::No, &no_policy()),
            "a fresh DB with no consent record ⇒ gate closed"
        );
    }

    #[test]
    fn a_stale_copy_version_is_not_allowed() {
        let conn = migrated_conn();
        // An older accepted version no longer counts once the copy (and the constant) moved on.
        store::set_cloud_ai_consent(&conn, CLOUD_AI_CONSENT_VERSION.wrapping_sub(1), 1_780_000_000).expect("set");
        assert!(
            !has_current_cloud_consent(&conn, RevokePending::No, &no_policy()),
            "a stale copy version ⇒ gate closed"
        );
    }

    #[test]
    fn the_current_copy_version_is_allowed() {
        let conn = migrated_conn();
        store::set_cloud_ai_consent(&conn, CLOUD_AI_CONSENT_VERSION, 1_780_000_000).expect("set");
        assert!(
            has_current_cloud_consent(&conn, RevokePending::No, &no_policy()),
            "accepting the current copy ⇒ gate open"
        );
    }

    #[test]
    fn a_held_revoke_closes_the_gate_over_a_consent_the_store_still_records() {
        // The store refused the revoke, so the record is still there; the held "no" wins.
        let conn = migrated_conn();
        store::set_cloud_ai_consent(&conn, CLOUD_AI_CONSENT_VERSION, 1_780_000_000).expect("set");
        assert!(
            !has_current_cloud_consent(&conn, RevokePending::Yes, &no_policy()),
            "a revoke held for the store ⇒ gate closed, whatever the store still says"
        );
    }

    /// Ask Cmdr's old opt-in, at any version, is a different answer to a different question.
    #[test]
    fn a_legacy_ask_cmdr_opt_in_allows_no_cloud_ai() {
        let conn = migrated_conn();
        store::set_legacy_ask_cmdr_consent_for_tests(&conn, CLOUD_AI_CONSENT_VERSION, 1_780_000_000);
        store::set_legacy_ask_cmdr_consent_for_tests(&conn, 4, 1_780_000_000);
        assert!(!has_current_cloud_consent(&conn, RevokePending::No, &no_policy()));
    }

    #[test]
    fn the_status_reports_the_audit_and_the_same_answer_as_the_gate() {
        let conn = migrated_conn();
        store::set_cloud_ai_consent(&conn, CLOUD_AI_CONSENT_VERSION, 1_780_000_000).expect("set");
        let open = status_from(&conn, RevokePending::No, &no_policy()).expect("status");
        assert_eq!(
            open,
            CloudAiConsentStatus {
                accepted: true,
                current_version: CLOUD_AI_CONSENT_VERSION,
                accepted_version: Some(CLOUD_AI_CONSENT_VERSION),
                accepted_at: Some(1_780_000_000),
                managed: None,
            }
        );
        // A held "no" closes it while the audit still says what the store holds.
        let held = status_from(&conn, RevokePending::Yes, &no_policy()).expect("status");
        assert!(!held.accepted);
        assert_eq!(held.accepted_version, Some(CLOUD_AI_CONSENT_VERSION));
    }
}

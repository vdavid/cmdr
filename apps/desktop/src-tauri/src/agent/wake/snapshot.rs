//! The readiness snapshot: what the three gates said, the last time anything changed them.
//!
//! ⚠️ **A cached atomic, ❌ not a per-batch query.** `Inbox::admit_if_permitted` needs a
//! [`WakeReadiness`], and the consent bit behind it lives in `main.db`. Asking per batch would
//! put a second SQLite round trip on the live loop's path, which is the one thing that path may
//! not do. So it is computed on the events that can move it (consent, disk access, the key) and
//! read as one relaxed atomic load everywhere else.
//!
//! A stale snapshot can only be stale between a gate changing and [`refresh_readiness`] being
//! called for it, and every caller of that is an explicit user action. It fails CLOSED: the
//! initial value is `NeedsConsent`, so nothing is stored before the store has even been opened.

use std::sync::atomic::{AtomicU8, Ordering};

use tauri::{AppHandle, Manager, Runtime};

use super::channel::{WakeControl, send_control};
use super::{AgentGates, WakeReadiness, readiness};
use crate::agent::AgentDb;
use crate::agent::consent::has_current_consent;

const LOG_TARGET: &str = "agent::wake";

const NEEDS_CONSENT: u8 = 0;
const NEEDS_FULL_DISK_ACCESS: u8 = 1;
const NEEDS_API_KEY: u8 = 2;
const READY: u8 = 3;

/// Starts closed. Before `agent::start` has run there is no store to read consent from, and
/// "we haven't looked yet" must not read as "the user said yes".
static READINESS: AtomicU8 = AtomicU8::new(NEEDS_CONSENT);

fn as_code(readiness: WakeReadiness) -> u8 {
    match readiness {
        WakeReadiness::NeedsConsent => NEEDS_CONSENT,
        WakeReadiness::NeedsFullDiskAccess => NEEDS_FULL_DISK_ACCESS,
        WakeReadiness::NeedsApiKey => NEEDS_API_KEY,
        WakeReadiness::Ready => READY,
    }
}

fn from_code(code: u8) -> WakeReadiness {
    match code {
        NEEDS_FULL_DISK_ACCESS => WakeReadiness::NeedsFullDiskAccess,
        NEEDS_API_KEY => WakeReadiness::NeedsApiKey,
        READY => WakeReadiness::Ready,
        // Anything unrecognized is the closed answer, which is also the initial one.
        _ => WakeReadiness::NeedsConsent,
    }
}

/// What the gates said last. One relaxed load: safe to call per live batch.
pub fn readiness_snapshot() -> WakeReadiness {
    from_code(READINESS.load(Ordering::Relaxed))
}

/// Re-evaluate the three gates and cache the answer.
///
/// ⚠️ **Never on the live-loop thread**: this reads `main.db`. Call it from `agent::start` and
/// from each place a gate can move — the consent screen, the AI settings, the Full Disk Access
/// decision.
///
/// A change is announced to the wake loop, which may have parked its timer against the old
/// answer. Announcing unconditionally would wake the loop on every settings save. The new value
/// is not returned: [`readiness_snapshot`] is the one way to read it, so no caller can end up
/// acting on a copy that a later refresh has already moved past.
pub fn refresh_readiness<R: Runtime>(app: &AppHandle<R>) {
    let gates = AgentGates {
        consented: consented(app),
        fda_pending: crate::fda_gate::is_fda_pending_runtime(),
        has_api_key: has_api_key(app),
    };
    let next = readiness(gates);
    let previous = READINESS.swap(as_code(next), Ordering::Relaxed);
    if previous != as_code(next) {
        log::debug!(target: LOG_TARGET, "readiness moved to {next:?}");
        send_control(WakeControl::ReadinessChanged);
        // The status corner renders the gap for a user who opted in, so it has to hear about a
        // gate closing or opening: the API key is set in another window and Full Disk Access is
        // granted outside the app entirely.
        super::indicator::emit_status();
    }
}

/// Whether the user has accepted the current consent copy. Fails closed: no store, no
/// connection, or an unreadable record all read as "no".
fn consented<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(db) = app.try_state::<AgentDb>() else {
        return false;
    };
    match db.open_read_connection() {
        Ok(conn) => has_current_consent(&conn),
        Err(e) => {
            log::warn!(target: LOG_TARGET, "reading consent for the wake gates failed, refusing: {e}");
            false
        }
    }
}

/// Whether a usable provider is configured for the interactive slot — the same resolution a
/// send performs, so the indicator can never say "ready" for a slot that would refuse.
///
/// ⚠️ That includes the E2E fake's short-circuit. `resolve_agent_llm` answers `Ok` under
/// `CMDR_E2E_ASK_CMDR_FAKE` with `ai.provider` still off, so without the same branch here the
/// gate would report `NeedsApiKey` for a slot that resolves fine, and no wake could ever run
/// under the harness.
fn has_api_key<R: Runtime>(app: &AppHandle<R>) -> bool {
    if crate::test_mode::ask_cmdr_fake_active() {
        return true;
    }
    use crate::ai::manager::BackendResolution;
    let model_override = crate::settings::load_ask_cmdr_interactive_model(app);
    matches!(
        crate::ai::manager::resolve_backend_with_model(model_override.as_deref()),
        BackendResolution::Ready(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every state has to survive the trip through the atomic. A silent collapse here would
    /// make the indicator report the wrong gap, which is worse than reporting none.
    #[test]
    fn every_readiness_state_round_trips_through_the_cache() {
        for state in [
            WakeReadiness::Ready,
            WakeReadiness::NeedsConsent,
            WakeReadiness::NeedsFullDiskAccess,
            WakeReadiness::NeedsApiKey,
        ] {
            assert_eq!(from_code(as_code(state)), state);
        }
    }

    /// ❌ "We haven't looked yet" must never read as "the user said yes": before the store is
    /// open there is nowhere to read consent from, and the pipeline would be storing a record
    /// of what somebody does with their files for a purpose they never agreed to.
    #[test]
    fn an_unrecognized_or_unset_code_reads_as_no_consent() {
        assert_eq!(from_code(200), WakeReadiness::NeedsConsent);
        assert!(!from_code(200).admits_to_inbox());
    }
}

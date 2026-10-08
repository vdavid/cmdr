//! The readiness snapshot: what the three gates said, the last time anything changed them.
//!
//! ⚠️ **A cached atomic, ❌ not a per-batch query.** `Inbox::admit_if_permitted` needs a
//! [`WakeReadiness`], and the gates behind it live in `settings.json` and `main.db` (the cloud
//! consent). Asking per batch would put file and SQLite round trips on the live loop's path, which
//! is the one thing that path may not do. So it is computed on the events that can move it (the
//! Ask Cmdr switch, cloud consent, AI settings, disk access) and read as one relaxed atomic load
//! everywhere else.
//!
//! A stale snapshot can only be stale between a gate changing and [`refresh_readiness`] being
//! called for it, and every caller of that is an explicit user action. It fails CLOSED: the
//! initial value is `AskCmdrOff`, so nothing is stored before anything has been read.

use std::sync::atomic::{AtomicU8, Ordering};

use tauri::{AppHandle, Runtime};

use super::channel::{WakeControl, send_control};
use super::{AgentGates, ProviderGate, WakeReadiness, readiness};

const LOG_TARGET: &str = "agent::wake";

const ASK_CMDR_OFF: u8 = 0;
const NEEDS_FULL_DISK_ACCESS: u8 = 1;
const NEEDS_API_KEY: u8 = 2;
const READY: u8 = 3;
const OFF: u8 = 4;
const NEEDS_CLOUD_CONSENT: u8 = 5;

/// Starts closed. Before `agent::start` has run nothing has been read, and "we haven't looked
/// yet" must not read as "the user switched it on".
static READINESS: AtomicU8 = AtomicU8::new(ASK_CMDR_OFF);

fn as_code(readiness: WakeReadiness) -> u8 {
    match readiness {
        WakeReadiness::AskCmdrOff => ASK_CMDR_OFF,
        WakeReadiness::NeedsFullDiskAccess => NEEDS_FULL_DISK_ACCESS,
        WakeReadiness::NeedsApiKey => NEEDS_API_KEY,
        WakeReadiness::Ready => READY,
        WakeReadiness::Off => OFF,
        WakeReadiness::NeedsCloudConsent => NEEDS_CLOUD_CONSENT,
    }
}

fn from_code(code: u8) -> WakeReadiness {
    match code {
        NEEDS_FULL_DISK_ACCESS => WakeReadiness::NeedsFullDiskAccess,
        NEEDS_API_KEY => WakeReadiness::NeedsApiKey,
        READY => WakeReadiness::Ready,
        OFF => WakeReadiness::Off,
        NEEDS_CLOUD_CONSENT => WakeReadiness::NeedsCloudConsent,
        // Anything unrecognized is the closed answer, which is also the initial one.
        _ => WakeReadiness::AskCmdrOff,
    }
}

/// What the gates said last. One relaxed load: safe to call per live batch.
pub fn readiness_snapshot() -> WakeReadiness {
    from_code(READINESS.load(Ordering::Relaxed))
}

/// Re-evaluate the gates and cache the answer.
///
/// ⚠️ **Never on the live-loop thread**: this reads `settings.json` and `main.db`. Call it from
/// `agent::start` and from each place a gate can move — the Ask Cmdr switch, the cloud AI
/// switch, the AI settings, the Full Disk Access decision.
///
/// A change is announced to the wake loop, which may have parked its timer against the old
/// answer. Announcing unconditionally would wake the loop on every settings save. The new value
/// is not returned: [`readiness_snapshot`] is the one way to read it, so no caller can end up
/// acting on a copy that a later refresh has already moved past.
pub fn refresh_readiness<R: Runtime>(app: &AppHandle<R>) {
    let gates = AgentGates {
        ask_cmdr: crate::settings::load_ask_cmdr_switch(app),
        fda_pending: crate::fda_gate::is_fda_pending_runtime(),
        provider: provider_gate(app),
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

/// What the interactive slot's provider would do with a send — the same resolution a send
/// performs (cloud consent included), so the indicator can never say "ready" for a slot that
/// would refuse, nor name a gap to somebody who turned AI off.
///
/// ⚠️ That includes the E2E fake's short-circuit, which resolves as [`ProviderGate::Ready`].
/// `resolve_agent_llm` answers `Ok` under `CMDR_E2E_ASK_CMDR_FAKE` with `ai.provider` still off,
/// so without the same branch here the gate would report `Off` for a slot that resolves fine, and
/// no wake could ever run under the harness.
fn provider_gate<R: Runtime>(app: &AppHandle<R>) -> ProviderGate {
    if crate::test_mode::ask_cmdr_fake_active() {
        return ProviderGate::Ready;
    }
    use crate::ai::manager::BackendResolution;
    let model_override = crate::settings::load_ask_cmdr_interactive_model(app);
    match crate::ai::manager::resolve_backend_with_model(app, model_override.as_deref()) {
        BackendResolution::Ready(_) => ProviderGate::Ready,
        BackendResolution::Off => ProviderGate::Off,
        BackendResolution::NoCloudConsent => ProviderGate::NeedsCloudConsent,
        // The organization's answer, already given: silent like the user's own "off", and the
        // stored backlog stays (the policy can lift again). `DisableAI` reaches readiness through
        // the switch first (`AskCmdrSwitch::ManagedOff`), with the same effect.
        BackendResolution::Managed(_) => ProviderGate::Off,
        BackendResolution::NotConfigured(_) | BackendResolution::UnknownProvider(_) => ProviderGate::NotConfigured,
    }
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
            WakeReadiness::AskCmdrOff,
            WakeReadiness::Off,
            WakeReadiness::NeedsCloudConsent,
            WakeReadiness::NeedsFullDiskAccess,
            WakeReadiness::NeedsApiKey,
        ] {
            assert_eq!(from_code(as_code(state)), state);
        }
    }

    /// ❌ "We haven't looked yet" must never read as "the user switched it on": the pipeline would
    /// be storing a record of what somebody does with their files for a feature they never turned
    /// on.
    #[test]
    fn an_unrecognized_or_unset_code_reads_as_ask_cmdr_off() {
        assert_eq!(from_code(200), WakeReadiness::AskCmdrOff);
        assert!(!from_code(200).admits_to_inbox());
    }
}

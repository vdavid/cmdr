//! What the agent may do when consent, disk access, or a key is missing.

use super::super::*;

/// Everything in place.
fn ready() -> AgentGates {
    AgentGates {
        consented: true,
        fda_pending: false,
        has_api_key: true,
    }
}

#[test]
fn everything_in_place_is_ready() {
    assert_eq!(readiness(ready()), WakeReadiness::Ready);
}

/// **Consent outranks everything, and this is the one a later reader would flatten into an
/// arbitrary order.** Asking somebody to grant Full Disk Access, or to paste an API key, for a
/// feature they have not opted into is asking them to widen access for something they may not
/// want at all. Whatever else is missing, the answer is the consent screen first.
#[test]
fn missing_consent_outranks_every_other_gap() {
    let nothing_configured = AgentGates {
        consented: false,
        fda_pending: true,
        has_api_key: false,
    };

    assert_eq!(readiness(nothing_configured), WakeReadiness::NeedsConsent);
}

/// Disk access outranks the key, because it decides whether the agent can SEE anything. A user
/// told to configure a key, who then finds the agent has nothing to say because it cannot read
/// the flagship folder, has been sent round the houses.
#[test]
fn disk_access_outranks_the_key() {
    let consented_but_blind = AgentGates {
        consented: true,
        fda_pending: true,
        has_api_key: false,
    };

    assert_eq!(readiness(consented_but_blind), WakeReadiness::NeedsFullDiskAccess);
}

#[test]
fn a_missing_key_is_the_last_gap() {
    let no_key = AgentGates {
        has_api_key: false,
        ..ready()
    };

    assert_eq!(readiness(no_key), WakeReadiness::NeedsApiKey);
}

/// **Without consent the pipeline stores nothing.** Admitting rows means keeping a record of
/// what the user has been doing with their files, for a purpose they have not agreed to. It
/// also avoids the surprise where somebody consents on Tuesday and is handed a backlog of
/// everything they did since installing.
#[test]
fn nothing_is_stored_without_consent() {
    let unconsented = readiness(AgentGates {
        consented: false,
        ..ready()
    });

    assert!(!unconsented.admits_to_inbox());
}

/// **With consent but no key, signal DOES accumulate.** The user opted in; a missing key is a
/// gap they can close, and the backlog waiting for them belongs to them. The staleness horizon
/// bounds it, so a key added a month later yields a week of signal rather than a year.
#[test]
fn signal_accumulates_while_only_the_key_is_missing() {
    let no_key = readiness(AgentGates {
        has_api_key: false,
        ..ready()
    });

    assert_eq!(no_key, WakeReadiness::NeedsApiKey);
    assert!(
        no_key.admits_to_inbox(),
        "the backlog belongs to the user, to come back to"
    );
    assert!(!no_key.may_wake(), "but nothing may reach a provider without a key");
}

/// Only a fully ready agent may run a turn. Every gap stops the wake, including the two that
/// still allow signal to accumulate.
#[test]
fn only_a_ready_agent_may_wake() {
    assert!(readiness(ready()).may_wake());
    for gap in [
        WakeReadiness::NeedsConsent,
        WakeReadiness::NeedsFullDiskAccess,
        WakeReadiness::NeedsApiKey,
    ] {
        assert!(!gap.may_wake(), "{gap:?} must not run a turn");
    }
}

/// Disk access gates the WAKE, not the storing: whatever reaches the pipeline at all is ground
/// the indexer could already see, so there is nothing privacy-sensitive about holding it. What
/// is missing is the ground the flagship scenario reads, so a digest built now would describe
/// a fraction of the truth.
#[test]
fn a_pending_disk_decision_stops_the_wake_but_not_the_inbox() {
    let blind = readiness(AgentGates {
        fda_pending: true,
        ..ready()
    });

    assert!(!blind.may_wake());
    assert!(blind.admits_to_inbox());
}

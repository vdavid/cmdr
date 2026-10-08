//! What the agent may do when Ask Cmdr, AI itself, cloud consent, disk access, or a key is missing.

use super::super::*;
use crate::settings::AskCmdrSwitch;

/// **The organization pinning Ask Cmdr off is not the person switching it off.** The policy
/// overlays and never rewrites: removing the profile must bring the backlog back, so a managed off
/// stores nothing new and runs nothing, like `Off`, but takes nothing away.
#[test]
fn a_managed_off_keeps_the_backlog_a_switched_off_ask_cmdr_would_take() {
    let managed_off = readiness(AgentGates {
        ask_cmdr: AskCmdrSwitch::ManagedOff,
        ..ready()
    });

    assert_eq!(managed_off, WakeReadiness::Off);
    assert!(managed_off.permits_stored_signal(), "the policy withdraws no purpose");
    assert!(!managed_off.admits_to_inbox());
    assert!(!managed_off.may_wake());
}

/// Everything in place.
fn ready() -> AgentGates {
    AgentGates {
        ask_cmdr: AskCmdrSwitch::On,
        fda_pending: false,
        provider: ProviderGate::Ready,
    }
}

#[test]
fn everything_in_place_is_ready() {
    assert_eq!(readiness(ready()), WakeReadiness::Ready);
}

/// **Ask Cmdr's switch outranks everything, and this is the one a later reader would flatten into
/// an arbitrary order.** Asking somebody to grant Full Disk Access, or to paste an API key, for a
/// feature they switched off is asking them to widen access for something they may not want at
/// all. Whatever else is missing, the answer is the switch first.
#[test]
fn ask_cmdr_off_outranks_every_other_gap() {
    let nothing_configured = AgentGates {
        ask_cmdr: AskCmdrSwitch::Off,
        fda_pending: true,
        provider: ProviderGate::NotConfigured,
    };

    assert_eq!(readiness(nothing_configured), WakeReadiness::AskCmdrOff);
}

/// **Ask Cmdr off outranks `Off` too, even though both render as silence.** They differ in what
/// they DO: only `AskCmdrOff` takes the stored backlog away, so letting the AI toggle mask a
/// switched-off Ask Cmdr would leave that record on disk.
#[test]
fn ask_cmdr_off_outranks_ai_being_off() {
    let switched_off = AgentGates {
        ask_cmdr: AskCmdrSwitch::Off,
        provider: ProviderGate::Off,
        ..ready()
    };

    assert_eq!(readiness(switched_off), WakeReadiness::AskCmdrOff);
    assert!(!readiness(switched_off).permits_stored_signal());
}

/// **Turning AI off is an answer, not a gap.** ❌ It must never report `NeedsApiKey`: the status
/// corner would show a warning telling somebody to finish setting up a provider they deliberately
/// switched off.
#[test]
fn ai_turned_off_is_its_own_state_not_a_missing_key() {
    let switched_off = AgentGates {
        provider: ProviderGate::Off,
        ..ready()
    };

    assert_eq!(readiness(switched_off), WakeReadiness::Off);
}

/// `Off` outranks the two gaps that ask the user for something. There is nothing to ask somebody
/// who turned the feature off, so a Full Disk Access prompt or a key nag would be noise about a
/// feature they are not using.
#[test]
fn ai_being_off_outranks_the_gaps_it_would_make_pointless() {
    let off_and_blind = AgentGates {
        fda_pending: true,
        provider: ProviderGate::Off,
        ..ready()
    };

    assert_eq!(readiness(off_and_blind), WakeReadiness::Off);
}

/// **Cloud AI not allowed is an answer too, and it comes right after `Off`.** The user picked a
/// cloud service and hasn't allowed sending to it, so asking them for disk access or a key for a
/// service they may never allow is noise. `Off` still outranks it: a user who turned AI off has
/// nothing to allow.
#[test]
fn a_missing_cloud_consent_outranks_disk_access_and_the_key() {
    let not_allowed = AgentGates {
        fda_pending: true,
        provider: ProviderGate::NeedsCloudConsent,
        ..ready()
    };

    assert_eq!(readiness(not_allowed), WakeReadiness::NeedsCloudConsent);
}

/// `Off` and `NeedsCloudConsent` come from the same provider gate, so they can't both hold; the
/// order that matters is against Ask Cmdr's own switch, which outranks the cloud one.
#[test]
fn ask_cmdr_being_off_outranks_a_missing_cloud_consent() {
    let both = AgentGates {
        ask_cmdr: AskCmdrSwitch::Off,
        provider: ProviderGate::NeedsCloudConsent,
        ..ready()
    };

    assert_eq!(readiness(both), WakeReadiness::AskCmdrOff);
}

/// **Without cloud consent nothing new is stored, and nothing runs, but the backlog stays.** Like
/// `Off`: the rows were gathered while cloud AI was allowed (or on a local provider), and turning
/// the switch off is something the user can flip straight back.
#[test]
fn a_missing_cloud_consent_stores_nothing_new_and_keeps_the_backlog() {
    let not_allowed = readiness(AgentGates {
        provider: ProviderGate::NeedsCloudConsent,
        ..ready()
    });

    assert!(!not_allowed.admits_to_inbox());
    assert!(not_allowed.permits_stored_signal());
    assert!(!not_allowed.may_wake());
}

/// Disk access outranks the key, because it decides whether the agent can SEE anything. A user
/// told to configure a key, who then finds the agent has nothing to say because it cannot read
/// the flagship folder, has been sent round the houses.
#[test]
fn disk_access_outranks_the_key() {
    let on_but_blind = AgentGates {
        fda_pending: true,
        provider: ProviderGate::NotConfigured,
        ..ready()
    };

    assert_eq!(readiness(on_but_blind), WakeReadiness::NeedsFullDiskAccess);
}

#[test]
fn a_missing_key_is_the_last_gap() {
    let no_key = AgentGates {
        provider: ProviderGate::NotConfigured,
        ..ready()
    };

    assert_eq!(readiness(no_key), WakeReadiness::NeedsApiKey);
}

/// **With Ask Cmdr off the pipeline stores nothing.** Admitting rows means keeping a record of
/// what the user has been doing with their files, for a feature they switched off. It also
/// avoids the surprise where somebody switches it on on Tuesday and is handed a backlog of
/// everything they did since installing.
#[test]
fn nothing_is_stored_with_ask_cmdr_off() {
    let ask_cmdr_off = readiness(AgentGates {
        ask_cmdr: AskCmdrSwitch::Off,
        ..ready()
    });

    assert!(!ask_cmdr_off.admits_to_inbox());
}

/// **With AI off the pipeline stores nothing new either**, because the pile could only grow for a
/// feature that is switched off.
#[test]
fn nothing_new_is_stored_while_ai_is_off() {
    let switched_off = readiness(AgentGates {
        provider: ProviderGate::Off,
        ..ready()
    });

    assert_eq!(switched_off, WakeReadiness::Off);
    assert!(!switched_off.admits_to_inbox());
    assert!(!switched_off.may_wake());
}

/// **But what was already stored stays.** ❌ The purge path must never key on `admits_to_inbox`:
/// `Off` refuses new rows the way `AskCmdrOff` does, and sharing the predicate would delete
/// somebody's own signal the moment they flipped a toggle they can flip straight back. Ask Cmdr
/// is the purpose those rows were kept for, so its switch is the only thing that takes them away.
#[test]
fn turning_ai_off_keeps_the_backlog_that_ask_cmdr_off_would_take() {
    let switched_off = readiness(AgentGates {
        provider: ProviderGate::Off,
        ..ready()
    });
    let ask_cmdr_off = readiness(AgentGates {
        ask_cmdr: AskCmdrSwitch::Off,
        ..ready()
    });

    assert!(switched_off.permits_stored_signal(), "a toggle withdraws no purpose");
    assert!(
        !ask_cmdr_off.permits_stored_signal(),
        "switching Ask Cmdr off withdraws the purpose"
    );
}

/// **With everything on but no key, signal DOES accumulate.** The user switched it on; a missing key is a
/// gap they can close, and the backlog waiting for them belongs to them. The staleness horizon
/// bounds it, so a key added a month later yields a week of signal rather than a year.
#[test]
fn signal_accumulates_while_only_the_key_is_missing() {
    let no_key = readiness(AgentGates {
        provider: ProviderGate::NotConfigured,
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
        WakeReadiness::AskCmdrOff,
        WakeReadiness::Off,
        WakeReadiness::NeedsCloudConsent,
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

//! What the `eject` tool tells an agent, as values rather than sentences.

use super::*;
use crate::file_system::volume::eject::holders::HolderScan;
use crate::file_system::volume::eject::{EjectError, EjectOutcome, EjectStep};

/// Every refusal an agent can meet, one per `EjectError` variant.
fn every_refusal() -> Vec<EjectError> {
    vec![
        EjectError::Busy,
        EjectError::VolumeNotFound {
            volume_id: "vol-1".to_string(),
        },
        EjectError::NotEjectable {
            volume_id: "vol-1".to_string(),
        },
        EjectError::NotAnSmbVolume {
            volume_id: "vol-1".to_string(),
        },
        EjectError::RemoteNotConnected {
            volume_id: "sftp-1".to_string(),
        },
        EjectError::DeviceDisconnectRefused {
            provider: "mtp".to_string(),
            detail: "the phone wouldn't let go".to_string(),
        },
        EjectError::UnmountRefused {
            holders: HolderScan::Incomplete { named: Vec::new() },
            detail: "in use by process 1234".to_string(),
        },
        EjectError::HeldBeforeEject {
            holders: HolderScan::Incomplete { named: Vec::new() },
        },
        EjectError::TimedOut,
        EjectError::NotResponding {
            step: EjectStep::IndexStop,
        },
        EjectError::Unexpected {
            detail: "a task panicked".to_string(),
        },
    ]
}

#[test]
fn a_refusals_tag_is_the_wire_types_own_name() {
    // One vocabulary, two readers: an agent branching on `data.outcome` and the
    // frontend rendering `EjectError`'s serde tag must be talking about the same
    // refusal. Drifting them apart is silent, which is what this catches.
    for error in every_refusal() {
        let serialized = serde_json::to_value(&error).expect("an EjectError serializes");
        let wire_tag = serialized
            .get("type")
            .and_then(Value::as_str)
            .expect("every EjectError carries its type tag");
        assert_eq!(
            refusal_tag(&error),
            wire_tag,
            "the tool's tag and the wire type disagree about {error:?}"
        );
    }
}

#[test]
fn every_outcome_has_its_own_tag_and_sentence() {
    let outcomes = [
        EjectOutcome::DeviceDisconnected,
        EjectOutcome::RemoteDisconnected,
        EjectOutcome::Unmounted,
        EjectOutcome::DiskEjected,
        EjectOutcome::AlreadyGone,
    ];

    let mut tags: Vec<&str> = outcomes.iter().map(|outcome| outcome_tag(*outcome)).collect();
    tags.sort_unstable();
    let distinct = tags.len();
    tags.dedup();
    assert_eq!(
        tags.len(),
        distinct,
        "two outcomes share a tag, so an agent can't tell them apart"
    );

    for outcome in outcomes {
        let message = success_message(outcome, "vol-1");
        assert!(
            message.contains("vol-1"),
            "an outcome's sentence names the volume it's about, got {message:?}"
        );
    }
}

#[test]
fn an_outcome_tag_is_the_wire_types_own_name() {
    // The same one-vocabulary rule as the refusals: the IPC command hands the
    // frontend this enum, and the tool hands an agent these tags.
    for outcome in [
        EjectOutcome::DeviceDisconnected,
        EjectOutcome::RemoteDisconnected,
        EjectOutcome::Unmounted,
        EjectOutcome::DiskEjected,
        EjectOutcome::AlreadyGone,
    ] {
        let serialized = serde_json::to_value(outcome).expect("an EjectOutcome serializes");
        assert_eq!(
            Some(outcome_tag(outcome)),
            serialized.as_str(),
            "the tool's tag and the wire type disagree about {outcome:?}"
        );
    }
}

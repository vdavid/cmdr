//! The mapping's completeness contract: nothing the index can say may fall on
//! the floor between the subsystems and the host.

use crate::error_reporter::auto_dispatcher::{TEST_LOCK, reset_for_test, set_enabled, snapshot_for_test};
use cmdr_index::ActivityPhase;
use cmdr_index::testing::events::one_of_every_kind;
use cmdr_index::{Diagnostic, IndexErrorReport, IndexEvent, IndexEventKind};

use super::{Destination, route};

#[test]
fn every_event_kind_has_a_sample_to_map() {
    let kinds: Vec<IndexEventKind> = one_of_every_kind().iter().map(IndexEvent::kind).collect();
    for kind in IndexEventKind::ALL {
        assert!(
            kinds.contains(&kind),
            "{kind:?} has no sample in `one_of_every_kind`, so nothing checks that it maps anywhere"
        );
    }
    assert_eq!(
        kinds.len(),
        IndexEventKind::ALL.len(),
        "`one_of_every_kind` has a duplicate or a stray"
    );
}

#[test]
fn every_event_maps_to_a_destination_with_a_non_empty_name() {
    // The sample set includes an `Error`, and routing it for real is the point —
    // which means this touches the auto-dispatcher's global state. Without the
    // lock it lands a walker failure in the window the test below is asserting on.
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut wire_names: Vec<&str> = Vec::new();
    for event in one_of_every_kind() {
        let kind = event.kind();
        // `None` suppresses the Tauri emit; every other arm runs for real.
        match route(event, None) {
            Destination::Frontend(name) => {
                assert!(!name.is_empty(), "{kind:?} maps to an empty Tauri event name");
                wire_names.push(name);
            }
            // The two that reach the host's own machinery instead of the frontend.
            Destination::ErrorReport | Destination::RestrictedPaths => {
                assert!(
                    matches!(kind, IndexEventKind::Error | IndexEventKind::PathAccessDenied),
                    "{kind:?} is a frontend event, so it needs a wire name"
                );
            }
        }
    }
    let mut deduped = wire_names.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        wire_names.len(),
        "two events share one Tauri event name, so the frontend can't tell them apart"
    );
}

/// The subsystems can't invoke `log_error!` (a crate-root macro), so an
/// `IndexEvent::Error` is the only way a failure inside indexing reaches the
/// auto-dispatcher and, from there, shipped error reports. Silently dropping it
/// would compile, ship, and cost us the feedback loop, so pin it.
#[test]
fn an_error_event_reaches_the_auto_dispatcher() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_test();
    set_enabled(true);

    route(
        IndexEvent::Error {
            report: IndexErrorReport::StorageFailed {
                failure: cmdr_index::IndexFailure {
                    code: 11,
                    extended_code: 267,
                },
                context: Diagnostic("insert entries".into()),
                detail: Diagnostic("database disk image is malformed".into()),
            },
        },
        None,
    );

    let snapshot = snapshot_for_test().expect("an index storage failure must open a debounce window");
    assert_eq!(
        snapshot.0, "cmdr::indexing::store",
        "the report needs a stable category so triage can group index-storage failures"
    );
    assert!(
        snapshot.1.contains("267"),
        "the extended SQLite code is the discriminating fact; it must survive into the report: {}",
        snapshot.1
    );

    reset_for_test();
    set_enabled(false);
}

#[test]
fn the_phase_payload_serializes_volume_id_as_camel_case() {
    // The payload crosses IPC as `{ volumeId, phase }`; the frontend binding and
    // `index-state` read exactly those keys.
    use serde_json::json;
    let ev = super::IndexPhaseChangedEvent {
        volume_id: "smb-nas".to_string(),
        phase: ActivityPhase::Reconciling,
    };
    assert_eq!(
        serde_json::to_value(&ev).unwrap(),
        json!({ "volumeId": "smb-nas", "phase": "reconciling" })
    );
}

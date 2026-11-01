//! Ring, throttle, and report-transform tests for the state history.
use super::*;
use crate::redact::RedactionContext;
use chrono::{TimeZone, Utc};
use cmdr_operations::{LifecycleStatus, WriteOperationPhase, WriteOperationType};
use std::time::{Duration, Instant};

const RAW_NAME: &str = "PRIVATE-NAME-ASYMMETRIC";
const RAW_PATH: &str = "/Users/private/Projects/PRIVATE-NAME-ASYMMETRIC/report.pdf";
const OPERATION_ID: &str = "0199a2e7-47d8-7c31-a897-c58f415f4f91";
const VOLUME_ID: &str = "smb-PRIVATE-NAME-ASYMMETRIC-0123456789abcdef";

fn raw(sequence: u64) -> RawStateSnapshot {
    RawStateSnapshot {
        sequence,
        captured_at: Utc.timestamp_opt(1_800_000_000 + sequence as i64, 0).unwrap(),
        generation: 40 + sequence,
        focused: Some(PaneSide::Right),
        show_hidden: true,
        panes: vec![RawPaneSnapshot {
            side: PaneSide::Right,
            path: RAW_PATH.to_string(),
            volume_id: Some(format!("smb-{RAW_NAME}-0123456789abcdef")),
            volume_name: Some(RAW_NAME.to_string()),
            backend: Some(ReportBackend::Smb),
            connection: Some(ReportConnectionState::NeedsSignIn),
            view: Some(PaneView::Full),
            sort_field: Some(PaneSortField::Size),
            sort_order: Some(PaneSortOrder::Descending),
            total_files: 91,
            loaded_count: 17,
            cursor_index: 13,
            cursor: Some(RawEntryIdentity {
                name: RAW_NAME.to_string(),
                path: RAW_PATH.to_string(),
                role: EntryRole::File,
            }),
            selected_count: 5,
            selected_files: 2,
            selected_folders: 3,
            tab_count: 4,
        }],
        operations: vec![RawOperationSnapshot {
            operation_id: Some(OPERATION_ID.to_string()),
            operation_type: WriteOperationType::Copy,
            lifecycle: LifecycleStatus::Paused,
            phase: Some(WriteOperationPhase::Copying),
            source: Some(RAW_PATH.to_string()),
            destination: Some(format!("smb://{RAW_NAME}/share/out")),
            current_file: Some(RAW_NAME.to_string()),
            files_done: 7,
            files_total: 19,
            bytes_done: 11,
            bytes_total: 23,
        }],
        volumes: vec![
            RawVolumeSnapshot {
                id: VOLUME_ID.to_string(),
                name: RAW_NAME.to_string(),
                kind: Some(ReportVolumeKind::Smb),
                connection: Some(ReportConnectionState::NeedsSignIn),
                readiness: None,
            },
            RawVolumeSnapshot {
                id: "adb-PRIVATE-SERIAL".to_string(),
                name: "PRIVATE-OWNER's Pixel 8".to_string(),
                kind: Some(ReportVolumeKind::Adb),
                connection: None,
                readiness: Some(ReportDeviceReadiness::WaitingForAuthorization),
            },
        ],
        recent_listing_error_count: 5,
        recent_listing_failures: vec![RawListingFailure {
            at: Utc.timestamp_opt(1_800_000_000, 0).single(),
            volume_id: VOLUME_ID.to_string(),
            backend: Some(ReportBackend::Smb),
            reason: Some("permissionDenied".to_string()),
            category: Some(ErrorCategory::NeedsAction),
            path: RAW_PATH.to_string(),
        }],
    }
}

/// A report takes its own capture even inside the error throttle, so a failure that logged
/// below error level still ships the state at report time. It leaves the throttle alone.
#[test]
fn a_report_capture_bypasses_the_throttle_and_keeps_order() {
    let mut history = StateHistory::default();
    let start = Instant::now();
    assert!(history.record(start, raw(0)));

    let report_sequence = history
        .reserve_for_report("ERR-AB23X")
        .expect("the first build of a report captures");
    let mut snapshot = raw(50);
    snapshot.sequence = report_sequence;
    history.store(snapshot);

    assert!(
        !history.record(start + Duration::from_secs(29), raw(1)),
        "the error throttle still holds"
    );
    assert!(history.record(start + Duration::from_secs(30), raw(2)));
    assert_eq!(
        history.snapshot().iter().map(|s| s.generation).collect::<Vec<_>>(),
        vec![40, 90, 42],
        "oldest first, the report capture in its reserved place"
    );
}

/// Preview and send rebuild one report under one id; the send must carry the capture the
/// preview showed, and one report takes one ring slot.
#[test]
fn a_report_captures_once_per_id() {
    let mut history = StateHistory::default();
    assert!(
        history.reserve_for_report("ERR-AB23X").is_some(),
        "the preview captures"
    );
    assert!(history.reserve_for_report("ERR-AB23X").is_none(), "the send reuses it");
    assert!(
        history.reserve_for_report("ERR-CD45Y").is_some(),
        "another report captures"
    );
}

#[test]
fn history_throttles_at_thirty_seconds_caps_at_eight_and_stays_oldest_first() {
    let mut history = StateHistory::default();
    let start = Instant::now();
    for i in 0..10 {
        let at = start + Duration::from_secs(i * 30);
        assert!(history.record(at, raw(i)), "the exact 30-second boundary is due");
        assert!(!history.record(at + Duration::from_secs(29), raw(100 + i)));
    }

    let snapshots = history.snapshot();
    assert_eq!(snapshots.len(), 8);
    assert_eq!(
        snapshots.iter().map(|s| s.sequence).collect::<Vec<_>>(),
        (2..10).collect::<Vec<_>>()
    );
}

#[test]
fn report_transform_correlates_within_one_report_separates_reports_and_keeps_typed_facts() {
    let first = RedactionContext::for_test([0x11; 32], "ERR-FIRST");
    let second = RedactionContext::for_test([0x11; 32], "ERR-SECOND");
    let first_report = redact_snapshots(&[raw(1), raw(2)], &first);
    let second_report = redact_snapshots(&[raw(1)], &second);

    let first_json = serde_json::to_string(&first_report).unwrap();
    let second_json = serde_json::to_string(&second_report).unwrap();
    assert!(!first_json.contains(RAW_NAME));
    assert!(!first_json.contains(RAW_PATH));
    assert!(!second_json.contains(RAW_NAME));
    assert!(!second_json.contains(RAW_PATH));
    assert_eq!(first_report[0].panes[0].path, first_report[1].panes[0].path);
    assert_ne!(first_report[0].panes[0].path, second_report[0].panes[0].path);

    let pane = &first_report[0].panes[0];
    assert_eq!(pane.backend, Some(ReportBackend::Smb));
    assert_eq!(pane.connection, Some(ReportConnectionState::NeedsSignIn));
    assert_eq!(pane.selected_files, 2);
    assert_eq!(pane.selected_folders, 3);
    let operation = &first_report[0].operations[0];
    assert_eq!(operation.operation_id.as_deref(), Some(OPERATION_ID));
    assert_eq!(operation.lifecycle, LifecycleStatus::Paused);
    assert_eq!(operation.files_done, 7);
    assert_eq!(operation.bytes_total, 23);
}

#[test]
fn unknown_strings_and_non_random_operation_ids_fail_closed() {
    assert_eq!(PaneView::parse("PRIVATE VIEW"), None);
    assert_eq!(PaneSortField::parse("PRIVATE SORT"), None);
    assert_eq!(PaneSortOrder::parse("PRIVATE ORDER"), None);
    assert_eq!(PaneSide::parse("PRIVATE SIDE"), None);
    assert_eq!(keep_operation_id("copy-PRIVATE-NAME-ASYMMETRIC"), None);
    assert_eq!(keep_operation_id(OPERATION_ID).as_deref(), Some(OPERATION_ID));
}

#[test]
fn a_new_process_history_starts_empty_instead_of_loading_a_previous_session() {
    let mut old_process = StateHistory::default();
    assert!(old_process.record(Instant::now(), raw(1)));
    assert_eq!(old_process.snapshot().len(), 1);

    let new_process = StateHistory::default();
    assert!(new_process.snapshot().is_empty());
}

#[test]
fn report_transform_tokenizes_volumes_and_listing_failures_and_keeps_typed_facts() {
    let redaction = RedactionContext::for_test([0x11; 32], "ERR-VOLUMES");
    let report = redact_snapshots(&[raw(1)], &redaction);
    let json = serde_json::to_string(&report).unwrap();
    for private in [RAW_NAME, RAW_PATH, "PRIVATE-SERIAL", "PRIVATE-OWNER"] {
        assert!(!json.contains(private), "{private:?} survived: {json}");
    }

    let snapshot = &report[0];
    let smb = &snapshot.volumes[0];
    assert_eq!(smb.kind, Some(ReportVolumeKind::Smb));
    assert_eq!(smb.connection, Some(ReportConnectionState::NeedsSignIn));
    assert_eq!(smb.readiness, None);
    let adb = &snapshot.volumes[1];
    assert_eq!(adb.kind, Some(ReportVolumeKind::Adb));
    assert_eq!(adb.readiness, Some(ReportDeviceReadiness::WaitingForAuthorization));

    let failure = &snapshot.recent_listing_failures[0];
    assert_eq!(failure.backend, Some(ReportBackend::Smb));
    assert_eq!(failure.reason.as_deref(), Some("permissionDenied"));
    assert_eq!(failure.category, Some(ErrorCategory::NeedsAction));
    assert!(failure.at.as_deref().is_some_and(|at| at.starts_with("2027-01-15T")));
    assert_eq!(
        failure.volume_id, smb.volume_id,
        "one volume, one token across the snapshot"
    );
    assert_eq!(
        failure.volume_id,
        snapshot.panes[0].volume_id.clone().unwrap_or_default()
    );
    assert_eq!(
        failure.path, snapshot.panes[0].path,
        "one path, one token across the snapshot"
    );
}

#[test]
fn a_captured_listing_failure_keeps_the_variant_and_drops_the_message_and_path_fields() {
    let failure = RecentListingError {
        at_unix_ms: 1_800_000_000_000,
        listing_id: "listing-1".to_string(),
        volume_id: "not-a-registered-volume".to_string(),
        path: RAW_PATH.to_string(),
        message: "PRIVATE-OS-PROSE".to_string(),
        reason: Some(ListingErrorReason::NotFound {
            path: "/Users/private/PRIVATE-REASON-PATH".to_string(),
        }),
        category: Some(ErrorCategory::NeedsAction),
    };
    let raw = capture_listing_failure(&failure);
    assert_eq!(raw.reason.as_deref(), Some("notFound"));
    assert_eq!(raw.category, Some(ErrorCategory::NeedsAction));
    assert_eq!(raw.backend, None, "an unregistered volume has no backend to report");
    assert!(!format!("{raw:?}").contains("PRIVATE-OS-PROSE"));
    assert!(!format!("{raw:?}").contains("PRIVATE-REASON-PATH"));
}

#[test]
fn volume_wire_words_read_back_to_report_enums_and_unknown_ones_are_absent() {
    use crate::file_system::volume::{ConnectionState as S, DeviceReadiness as R, DeviceUnavailableReason as Why};
    use crate::tools::resources::volumes::{connection_state_token, device_readiness_token};
    for state in [
        S::Direct,
        S::OsMount,
        S::Disconnected,
        S::NeedsSignIn,
        S::NeedsHostKeyApproval,
        S::Saved,
    ] {
        assert_eq!(
            ReportConnectionState::from_token(connection_state_token(state)),
            Some(state.into())
        );
    }
    for (readiness, expected) in [
        (R::Ready, ReportDeviceReadiness::Ready),
        (
            R::WaitingForAuthorization,
            ReportDeviceReadiness::WaitingForAuthorization,
        ),
        (R::Connecting, ReportDeviceReadiness::Connecting),
        (
            R::Unavailable { reason: Why::Offline },
            ReportDeviceReadiness::UnavailableOffline,
        ),
        (
            R::Unavailable {
                reason: Why::NoPermissions,
            },
            ReportDeviceReadiness::UnavailableNoPermissions,
        ),
    ] {
        assert_eq!(
            ReportDeviceReadiness::from_token(device_readiness_token(readiness)),
            Some(expected)
        );
    }
    assert_eq!(ReportConnectionState::from_token("PRIVATE STATE"), None);
    assert_eq!(ReportDeviceReadiness::from_token("PRIVATE READINESS"), None);
}

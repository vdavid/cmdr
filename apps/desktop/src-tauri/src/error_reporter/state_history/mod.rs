//! Bounded, process-local state history for error-report bundles.
//!
//! Capture clones typed in-memory stores only. Raw names and paths live in this module's
//! private input structs and are transformed with the bundle's [`RedactionContext`] when the
//! manifest is assembled. Nothing here writes to logs or disk.

use crate::file_system::volume::friendly_error::{ErrorCategory, ListingErrorReason};
use crate::ignore_poison::IgnorePoison;
use crate::mcp::PaneStateStore;
use crate::mcp::listing_errors::RecentListingError;
use crate::mcp::pane_state::PaneState;
use crate::mcp::resources::volumes::VolumeSummary;
use crate::redact::RedactionContext;
use chrono::{DateTime, Utc};
use cmdr_operation_log::types::OperationId;
use cmdr_operations::{LifecycleStatus, WriteOperationPhase, WriteOperationType};
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::Manager;

mod report_types;
pub use report_types::*;

const CAPACITY: usize = 8;
const THROTTLE: Duration = Duration::from_secs(30);
/// The newest listing failures each capture keeps (the MCP ring holds 20).
const LISTING_FAILURES_PER_CAPTURE: usize = 5;

static HISTORY: LazyLock<Mutex<StateHistory>> = LazyLock::new(|| Mutex::new(StateHistory::default()));

#[derive(Default)]
struct StateHistory {
    last_capture_at: Option<Instant>,
    next_sequence: u64,
    snapshots: VecDeque<RawStateSnapshot>,
    /// Report ids that already took their capture, newest last, bounded like the ring.
    captured_reports: VecDeque<String>,
}

impl StateHistory {
    fn reserve(&mut self, now: Instant) -> Option<u64> {
        if self
            .last_capture_at
            .and_then(|last| now.checked_duration_since(last))
            .is_some_and(|elapsed| elapsed < THROTTLE)
        {
            return None;
        }
        self.last_capture_at = Some(now);
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        Some(sequence)
    }

    /// A sequence for a report's own capture, once per report id: preview and send rebuild one
    /// report, and the send must ship the capture the preview showed, in one ring slot. No
    /// throttle, and the error throttle's clock is left alone.
    fn reserve_for_report(&mut self, report_id: &str) -> Option<u64> {
        if self.captured_reports.iter().any(|id| id == report_id) {
            return None;
        }
        self.captured_reports.push_back(report_id.to_string());
        while self.captured_reports.len() > CAPACITY {
            self.captured_reports.pop_front();
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        Some(sequence)
    }

    fn store(&mut self, snapshot: RawStateSnapshot) {
        let insertion = self
            .snapshots
            .iter()
            .position(|existing| existing.sequence > snapshot.sequence)
            .unwrap_or(self.snapshots.len());
        self.snapshots.insert(insertion, snapshot);
        while self.snapshots.len() > CAPACITY {
            self.snapshots.pop_front();
        }
    }

    #[cfg(test)]
    fn record(&mut self, now: Instant, mut snapshot: RawStateSnapshot) -> bool {
        let Some(sequence) = self.reserve(now) else {
            return false;
        };
        snapshot.sequence = sequence;
        self.store(snapshot);
        true
    }

    fn snapshot(&self) -> Vec<RawStateSnapshot> {
        self.snapshots.iter().cloned().collect()
    }
}

/// Reserve this error's throttled slot and capture typed state away from the logging caller.
pub(super) fn capture_if_due(app: tauri::AppHandle<tauri::Wry>) {
    let sequence = {
        let mut history = HISTORY.lock_ignore_poison();
        let Some(sequence) = history.reserve(Instant::now()) else {
            return;
        };
        sequence
    };
    tauri::async_runtime::spawn(async move {
        // The same volume pipeline `cmdr://state` reads, timeout-guarded inside, so a hung
        // mount can't wedge this task.
        let volumes = crate::mcp::resources::volumes::snapshot_volumes().await;
        let Some(snapshot) = capture(&app, sequence, Utc::now(), &volumes) else {
            return;
        };
        HISTORY.lock_ignore_poison().store(snapshot);
    });
}

/// Capture the state now, for the report being built. The ring otherwise only fills on
/// `log_error!`, and plenty of reported failures log below error level, which left their
/// reports without any state at all.
pub(super) async fn capture_for_report<R: tauri::Runtime>(app: &tauri::AppHandle<R>, report_id: &str) {
    let Some(sequence) = HISTORY.lock_ignore_poison().reserve_for_report(report_id) else {
        return;
    };
    let volumes = crate::mcp::resources::volumes::snapshot_volumes().await;
    if let Some(snapshot) = capture(app, sequence, Utc::now(), &volumes) {
        HISTORY.lock_ignore_poison().store(snapshot);
    }
}

/// Transform the current process's history with one report's correlation context.
pub(super) fn for_report(redaction: &RedactionContext) -> Vec<DiagnosticStateSnapshot> {
    let snapshots = HISTORY.lock_ignore_poison().snapshot();
    redact_snapshots(&snapshots, redaction)
}

fn capture<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    sequence: u64,
    captured_at: DateTime<Utc>,
    volumes: &[VolumeSummary],
) -> Option<RawStateSnapshot> {
    let store = app.try_state::<PaneStateStore>()?;
    let left = store.get_left();
    let right = store.get_right();
    let listing_errors = crate::mcp::listing_errors::snapshot();
    Some(RawStateSnapshot {
        sequence,
        captured_at,
        generation: store.get_generation(),
        focused: PaneSide::parse(&store.get_focused_pane()),
        show_hidden: left.show_hidden,
        panes: vec![
            capture_pane(PaneSide::Left, &left),
            capture_pane(PaneSide::Right, &right),
        ],
        operations: capture_operations(),
        volumes: volumes.iter().map(capture_volume).collect(),
        recent_listing_error_count: listing_errors.len(),
        recent_listing_failures: listing_errors
            .iter()
            .rev()
            .take(LISTING_FAILURES_PER_CAPTURE)
            .rev()
            .map(capture_listing_failure)
            .collect(),
    })
}

fn capture_volume(volume: &VolumeSummary) -> RawVolumeSnapshot {
    RawVolumeSnapshot {
        id: volume.id.clone(),
        name: volume.name.clone(),
        kind: Some(volume.kind.into()),
        connection: volume.connection_state.and_then(ReportConnectionState::from_token),
        readiness: volume.device_readiness.and_then(ReportDeviceReadiness::from_token),
    }
}

/// One listing failure, minus its raw message: the typed reason and category say what the
/// user saw, and the message can quote OS prose the typed fields already classify.
fn capture_listing_failure(failure: &RecentListingError) -> RawListingFailure {
    let backend = crate::file_system::volume::manager::get_volume_manager()
        .get(&failure.volume_id)
        .map(|volume| volume.backend_kind().into());
    RawListingFailure {
        at: DateTime::from_timestamp_millis(i64::try_from(failure.at_unix_ms).unwrap_or(i64::MAX)),
        volume_id: failure.volume_id.clone(),
        backend,
        reason: failure.reason.as_ref().and_then(reason_token),
        category: failure.category,
        path: failure.path.clone(),
    }
}

/// The serde tag of a listing reason (`permissionDenied`, `connectionTimedOutErrno`, …): the
/// variant alone, never its fields, which can carry the path.
fn reason_token(reason: &ListingErrorReason) -> Option<String> {
    serde_json::to_value(reason)
        .ok()?
        .get("reason")?
        .as_str()
        .map(str::to_string)
}

fn capture_pane(side: PaneSide, pane: &PaneState) -> RawPaneSnapshot {
    let volume = pane
        .volume_id
        .as_deref()
        .and_then(|id| crate::file_system::volume::manager::get_volume_manager().get(id));
    let cursor = pane
        .cursor_index
        .checked_sub(pane.loaded_start)
        .and_then(|index| pane.files.get(index))
        .map(|entry| RawEntryIdentity {
            name: entry.name.clone(),
            path: entry.path.clone(),
            role: if entry.is_directory {
                EntryRole::Folder
            } else {
                EntryRole::File
            },
        });
    let (selected_files, selected_folders) = pane
        .selected_indices
        .iter()
        .filter_map(|index| index.checked_sub(pane.loaded_start))
        .filter_map(|index| pane.files.get(index))
        .fold((0usize, 0usize), |(files, folders), entry| {
            if entry.is_directory {
                (files, folders + 1)
            } else {
                (files + 1, folders)
            }
        });
    RawPaneSnapshot {
        side,
        path: pane.path.clone(),
        volume_id: pane.volume_id.clone(),
        volume_name: pane.volume_name.clone(),
        backend: volume.as_ref().map(|volume| volume.backend_kind().into()),
        connection: volume.and_then(|volume| volume.connection_state()).map(Into::into),
        view: PaneView::parse(&pane.view_mode),
        sort_field: PaneSortField::parse(&pane.sort_field),
        sort_order: PaneSortOrder::parse(&pane.sort_order),
        total_files: pane.total_files,
        loaded_count: pane.files.len(),
        cursor_index: pane.cursor_index,
        cursor,
        selected_count: pane.selected_indices.len(),
        selected_files,
        selected_folders,
        tab_count: pane.tabs.len(),
    }
}

fn capture_operations() -> Vec<RawOperationSnapshot> {
    let Ok(engine) = crate::operations_host::operations() else {
        return Vec::new();
    };
    engine
        .list_operations()
        .into_iter()
        .map(|operation| {
            let progress = engine.get_operation_status(&OperationId::from(operation.operation_id.as_str()));
            RawOperationSnapshot {
                operation_id: keep_operation_id(&operation.operation_id),
                operation_type: operation.operation_type,
                lifecycle: operation.status,
                phase: progress.as_ref().map(|status| status.phase),
                source: operation.source,
                destination: operation.destination,
                current_file: progress.as_ref().and_then(|status| status.current_file.clone()),
                files_done: progress.as_ref().map_or(0, |status| status.files_done),
                files_total: progress.as_ref().map_or(0, |status| status.files_total),
                bytes_done: progress.as_ref().map_or(0, |status| status.bytes_done),
                bytes_total: progress.as_ref().map_or(0, |status| status.bytes_total),
            }
        })
        .collect()
}

fn keep_operation_id(id: &str) -> Option<String> {
    uuid::Uuid::parse_str(id)
        .ok()
        .filter(|id| id.get_version() == Some(uuid::Version::SortRand))
        .map(|_| id.to_string())
}

fn redact_snapshots(snapshots: &[RawStateSnapshot], redaction: &RedactionContext) -> Vec<DiagnosticStateSnapshot> {
    snapshots
        .iter()
        .map(|snapshot| DiagnosticStateSnapshot {
            captured_at: snapshot.captured_at.to_rfc3339(),
            generation: snapshot.generation,
            focused: snapshot.focused,
            show_hidden: snapshot.show_hidden,
            panes: snapshot
                .panes
                .iter()
                .map(|pane| DiagnosticPaneSnapshot {
                    side: pane.side,
                    path: redaction.redact_path(&pane.path),
                    volume_id: pane.volume_id.as_deref().map(|id| redaction.redact_volume_id(id)),
                    volume_name: pane
                        .volume_name
                        .as_deref()
                        .map(|name| redaction.redact_volume_name(name)),
                    backend: pane.backend,
                    connection: pane.connection,
                    view: pane.view,
                    sort_field: pane.sort_field,
                    sort_order: pane.sort_order,
                    total_files: pane.total_files,
                    loaded_count: pane.loaded_count,
                    cursor_index: pane.cursor_index,
                    cursor: pane.cursor.as_ref().map(|cursor| DiagnosticEntryIdentity {
                        name: redaction.redact_name(&cursor.name, cursor.role == EntryRole::Folder),
                        path: redaction.redact_path(&cursor.path),
                        role: cursor.role,
                    }),
                    selected_count: pane.selected_count,
                    selected_files: pane.selected_files,
                    selected_folders: pane.selected_folders,
                    tab_count: pane.tab_count,
                })
                .collect(),
            operations: snapshot
                .operations
                .iter()
                .map(|operation| DiagnosticOperationSnapshot {
                    operation_id: operation.operation_id.clone(),
                    operation_type: operation.operation_type,
                    lifecycle: operation.lifecycle,
                    phase: operation.phase,
                    source: operation.source.as_deref().map(|path| redaction.redact_path(path)),
                    destination: operation.destination.as_deref().map(|path| redaction.redact_path(path)),
                    current_file: operation
                        .current_file
                        .as_deref()
                        .map(|name| redaction.redact_name(name, false)),
                    files_done: operation.files_done,
                    files_total: operation.files_total,
                    bytes_done: operation.bytes_done,
                    bytes_total: operation.bytes_total,
                })
                .collect(),
            volumes: snapshot
                .volumes
                .iter()
                .map(|volume| DiagnosticVolumeSnapshot {
                    volume_id: redaction.redact_volume_id(&volume.id),
                    name: redaction.redact_volume_name(&volume.name),
                    kind: volume.kind,
                    connection: volume.connection,
                    readiness: volume.readiness,
                })
                .collect(),
            recent_listing_error_count: snapshot.recent_listing_error_count,
            recent_listing_failures: snapshot
                .recent_listing_failures
                .iter()
                .map(|failure| DiagnosticListingFailure {
                    at: failure.at.map(|at| at.to_rfc3339()),
                    volume_id: redaction.redact_volume_id(&failure.volume_id),
                    backend: failure.backend,
                    reason: failure.reason.clone(),
                    category: failure.category,
                    path: redaction.redact_path(&failure.path),
                })
                .collect(),
        })
        .collect()
}

#[derive(Debug, Clone)]
struct RawStateSnapshot {
    sequence: u64,
    captured_at: DateTime<Utc>,
    generation: u64,
    focused: Option<PaneSide>,
    show_hidden: bool,
    panes: Vec<RawPaneSnapshot>,
    operations: Vec<RawOperationSnapshot>,
    volumes: Vec<RawVolumeSnapshot>,
    recent_listing_error_count: usize,
    recent_listing_failures: Vec<RawListingFailure>,
}

#[derive(Debug, Clone)]
struct RawVolumeSnapshot {
    id: String,
    name: String,
    kind: Option<ReportVolumeKind>,
    connection: Option<ReportConnectionState>,
    readiness: Option<ReportDeviceReadiness>,
}

#[derive(Debug, Clone)]
struct RawListingFailure {
    at: Option<DateTime<Utc>>,
    volume_id: String,
    backend: Option<ReportBackend>,
    reason: Option<String>,
    category: Option<ErrorCategory>,
    path: String,
}

#[derive(Debug, Clone)]
struct RawPaneSnapshot {
    side: PaneSide,
    path: String,
    volume_id: Option<String>,
    volume_name: Option<String>,
    backend: Option<ReportBackend>,
    connection: Option<ReportConnectionState>,
    view: Option<PaneView>,
    sort_field: Option<PaneSortField>,
    sort_order: Option<PaneSortOrder>,
    total_files: usize,
    loaded_count: usize,
    cursor_index: usize,
    cursor: Option<RawEntryIdentity>,
    selected_count: usize,
    selected_files: usize,
    selected_folders: usize,
    tab_count: usize,
}

#[derive(Debug, Clone)]
struct RawEntryIdentity {
    name: String,
    path: String,
    role: EntryRole,
}

#[derive(Debug, Clone)]
struct RawOperationSnapshot {
    operation_id: Option<String>,
    operation_type: WriteOperationType,
    lifecycle: LifecycleStatus,
    phase: Option<WriteOperationPhase>,
    source: Option<String>,
    destination: Option<String>,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
}

#[cfg(test)]
pub(super) const PRIVACY_TEST_RAW_NAME: &str = "PRIVATE-NAME-SENTINEL-ASYMMETRIC.pdf";
#[cfg(test)]
pub(super) const PRIVACY_TEST_RAW_PATH: &str = "/mnt/<alice-smith>/client secret";
#[cfg(test)]
pub(super) const PRIVACY_TEST_EXTERNAL_PROSE: &str = "PRIVATE-EXTERNAL-PROSE-SENTINEL-ASYMMETRIC";

/// Build adversarial raw state through the same pane projection and report transform as
/// production. The omitted MCP-only fields deliberately carry prose and names so archive-level
/// tests prove that functional MCP state did not become diagnostic payload by accident.
#[cfg(test)]
pub(super) fn privacy_fixture_for_test(redaction: &RedactionContext) -> Vec<DiagnosticStateSnapshot> {
    use crate::mcp::pane_state::{MountErrorInfo, PaneFileEntry, TabInfo, TypeToJumpInfo};

    let pane = PaneState {
        path: PRIVACY_TEST_RAW_PATH.to_string(),
        volume_id: Some("smb-private-host-private-share-0123456789abcdef".to_string()),
        volume_name: Some("PRIVATE-VOLUME-NAME-SENTINEL".to_string()),
        files: vec![
            PaneFileEntry {
                name: PRIVACY_TEST_RAW_NAME.to_string(),
                path: PRIVACY_TEST_RAW_PATH.to_string(),
                ..Default::default()
            },
            PaneFileEntry {
                name: PRIVACY_TEST_EXTERNAL_PROSE.to_string(),
                path: format!("/Users/private-account/{PRIVACY_TEST_EXTERNAL_PROSE}"),
                is_directory: true,
                ..Default::default()
            },
        ],
        cursor_index: 0,
        view_mode: "full".to_string(),
        selected_indices: vec![0, 1],
        sort_field: "size".to_string(),
        sort_order: "desc".to_string(),
        total_files: 91,
        loaded_end: 2,
        show_hidden: true,
        tabs: vec![TabInfo {
            id: PRIVACY_TEST_EXTERNAL_PROSE.to_string(),
            path: format!("/Users/private-account/{PRIVACY_TEST_EXTERNAL_PROSE}"),
            pinned: true,
            active: true,
        }],
        type_to_jump: Some(TypeToJumpInfo {
            buffer: PRIVACY_TEST_EXTERNAL_PROSE.to_string(),
            indicator_visible: true,
            indicator_stale: false,
            last_matched_name: Some(PRIVACY_TEST_EXTERNAL_PROSE.to_string()),
        }),
        mount_error: Some(MountErrorInfo {
            share: "PRIVATE-REMOTE-SHARE-SENTINEL".to_string(),
            reason: "timeout".to_string(),
            message: PRIVACY_TEST_EXTERNAL_PROSE.to_string(),
        }),
        ..Default::default()
    };
    let mut raw_pane = capture_pane(PaneSide::Right, &pane);
    raw_pane.backend = Some(ReportBackend::Smb);
    raw_pane.connection = Some(ReportConnectionState::NeedsSignIn);

    let raw = RawStateSnapshot {
        sequence: 0,
        captured_at: DateTime::parse_from_rfc3339("2027-01-15T08:00:00+00:00")
            .expect("test timestamp is valid")
            .with_timezone(&Utc),
        generation: 73,
        focused: Some(PaneSide::Right),
        show_hidden: true,
        panes: vec![raw_pane],
        operations: vec![RawOperationSnapshot {
            operation_id: Some("0199a2e7-47d8-7c31-a897-c58f415f4f91".to_string()),
            operation_type: WriteOperationType::Copy,
            lifecycle: LifecycleStatus::Paused,
            phase: Some(WriteOperationPhase::Copying),
            source: Some(PRIVACY_TEST_RAW_PATH.to_string()),
            destination: Some(
                "smb://private-user:private-password@private-host.local/private-share/private-folder".to_string(),
            ),
            current_file: Some(PRIVACY_TEST_RAW_NAME.to_string()),
            files_done: 7,
            files_total: 19,
            bytes_done: 11,
            bytes_total: 23,
        }],
        volumes: vec![RawVolumeSnapshot {
            id: "smb-private-host-private-share-0123456789abcdef".to_string(),
            name: "PRIVATE-VOLUME-NAME-SENTINEL".to_string(),
            kind: Some(ReportVolumeKind::Smb),
            connection: Some(ReportConnectionState::NeedsSignIn),
            readiness: None,
        }],
        recent_listing_error_count: 5,
        recent_listing_failures: vec![capture_listing_failure(&RecentListingError {
            at_unix_ms: 1_800_000_000_000,
            listing_id: "listing-1".to_string(),
            volume_id: "smb-private-host-private-share-0123456789abcdef".to_string(),
            path: PRIVACY_TEST_RAW_PATH.to_string(),
            message: PRIVACY_TEST_EXTERNAL_PROSE.to_string(),
            reason: Some(ListingErrorReason::PermissionDenied {
                path: PRIVACY_TEST_RAW_PATH.to_string(),
            }),
            category: Some(ErrorCategory::NeedsAction),
        })],
    };
    redact_snapshots(&[raw], redaction)
}

#[cfg(test)]
mod tests;

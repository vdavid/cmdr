//! The report-side shape of the state history: what a bundle's manifest carries. Identities
//! arrive here already transformed with the report's context; everything else is a closed enum
//! or a count, and an unknown value maps to absent.

use crate::file_system::volume::friendly_error::ErrorCategory;
use crate::file_system::volume::{BackendKind, ConnectionState};
use crate::file_system::write_operations::{LifecycleStatus, WriteOperationPhase, WriteOperationType};
use crate::mcp::resources::volumes::VolumeKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticStateSnapshot {
    pub captured_at: String,
    pub generation: u64,
    pub focused: Option<PaneSide>,
    pub show_hidden: bool,
    pub panes: Vec<DiagnosticPaneSnapshot>,
    pub operations: Vec<DiagnosticOperationSnapshot>,
    /// Every volume the app's own volume list shows, as `cmdr://state` sees it.
    pub volumes: Vec<DiagnosticVolumeSnapshot>,
    /// How many listing failures the process-wide ring holds (at most 20).
    pub recent_listing_error_count: usize,
    /// The newest few of those, typed.
    pub recent_listing_failures: Vec<DiagnosticListingFailure>,
}

/// One volume: identities as report tokens, state as closed enums.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticVolumeSnapshot {
    pub volume_id: String,
    pub name: String,
    pub kind: Option<ReportVolumeKind>,
    /// How live its session is. `None` for anything with no session (a local disk).
    pub connection: Option<ReportConnectionState>,
    /// Whether the device behind it can be opened. `None` for anything that isn't a device.
    pub readiness: Option<ReportDeviceReadiness>,
}

/// One directory-listing failure, without its raw message.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticListingFailure {
    pub at: Option<String>,
    pub volume_id: String,
    /// The backend serving the volume at capture time; `None` once the volume is gone.
    pub backend: Option<ReportBackend>,
    /// The `ListingErrorReason` variant (`permissionDenied`, `connectionTimedOutErrno`, …).
    pub reason: Option<String>,
    pub category: Option<ErrorCategory>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticPaneSnapshot {
    pub side: PaneSide,
    pub path: String,
    pub volume_id: Option<String>,
    pub volume_name: Option<String>,
    pub backend: Option<ReportBackend>,
    pub connection: Option<ReportConnectionState>,
    pub view: Option<PaneView>,
    pub sort_field: Option<PaneSortField>,
    pub sort_order: Option<PaneSortOrder>,
    pub total_files: usize,
    pub loaded_count: usize,
    pub cursor_index: usize,
    pub cursor: Option<DiagnosticEntryIdentity>,
    pub selected_count: usize,
    pub selected_files: usize,
    pub selected_folders: usize,
    pub tab_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntryIdentity {
    pub name: String,
    pub path: String,
    pub role: EntryRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticOperationSnapshot {
    pub operation_id: Option<String>,
    pub operation_type: WriteOperationType,
    pub lifecycle: LifecycleStatus,
    pub phase: Option<WriteOperationPhase>,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub current_file: Option<String>,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PaneSide {
    Left,
    Right,
}

impl PaneSide {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EntryRole {
    File,
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PaneView {
    Brief,
    Full,
}

impl PaneView {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "brief" => Some(Self::Brief),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PaneSortField {
    Name,
    Extension,
    Size,
    Modified,
    Relevance,
}

impl PaneSortField {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "name" => Some(Self::Name),
            "extension" => Some(Self::Extension),
            "size" => Some(Self::Size),
            "modified" => Some(Self::Modified),
            "relevance" => Some(Self::Relevance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PaneSortOrder {
    Ascending,
    Descending,
}

impl PaneSortOrder {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "asc" => Some(Self::Ascending),
            "desc" => Some(Self::Descending),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ReportBackend {
    Local,
    Smb,
    Sftp,
    Webdav,
    S3,
    Mtp,
    Adb,
    Archive,
    GitPortal,
}

impl From<BackendKind> for ReportBackend {
    fn from(value: BackendKind) -> Self {
        match value {
            BackendKind::Local => Self::Local,
            BackendKind::Smb => Self::Smb,
            BackendKind::Sftp => Self::Sftp,
            BackendKind::Webdav => Self::Webdav,
            BackendKind::S3 => Self::S3,
            BackendKind::Mtp => Self::Mtp,
            BackendKind::Adb => Self::Adb,
            BackendKind::Archive => Self::Archive,
            BackendKind::GitPortal => Self::GitPortal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ReportConnectionState {
    Direct,
    OsMount,
    Disconnected,
    NeedsSignIn,
    NeedsHostKeyApproval,
    Saved,
}

impl From<ConnectionState> for ReportConnectionState {
    fn from(value: ConnectionState) -> Self {
        match value {
            ConnectionState::Direct => Self::Direct,
            ConnectionState::OsMount => Self::OsMount,
            ConnectionState::Disconnected => Self::Disconnected,
            ConnectionState::NeedsSignIn => Self::NeedsSignIn,
            ConnectionState::NeedsHostKeyApproval => Self::NeedsHostKeyApproval,
            ConnectionState::Saved => Self::Saved,
        }
    }
}

impl ReportConnectionState {
    /// Read back the `cmdr://state` wire word (`mcp::resources::volumes::connection_state_token`).
    /// An unknown word maps to absent.
    pub(super) fn from_token(token: &str) -> Option<Self> {
        match token {
            "direct" => Some(Self::Direct),
            "os_mount" => Some(Self::OsMount),
            "disconnected" => Some(Self::Disconnected),
            "needs_sign_in" => Some(Self::NeedsSignIn),
            "needs_host_key_approval" => Some(Self::NeedsHostKeyApproval),
            "saved" => Some(Self::Saved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ReportVolumeKind {
    Local,
    Smb,
    Sftp,
    Webdav,
    S3,
    Mtp,
    Adb,
    Network,
    Virtual,
}

impl From<VolumeKind> for ReportVolumeKind {
    fn from(value: VolumeKind) -> Self {
        match value {
            VolumeKind::Local => Self::Local,
            VolumeKind::Smb => Self::Smb,
            VolumeKind::Sftp => Self::Sftp,
            VolumeKind::Webdav => Self::Webdav,
            VolumeKind::S3 => Self::S3,
            VolumeKind::Mtp => Self::Mtp,
            VolumeKind::Adb => Self::Adb,
            VolumeKind::Network => Self::Network,
            VolumeKind::Virtual => Self::Virtual,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ReportDeviceReadiness {
    Ready,
    WaitingForAuthorization,
    UnavailableOffline,
    UnavailableNoPermissions,
}

impl ReportDeviceReadiness {
    /// Read back the `cmdr://state` wire word (`mcp::resources::volumes::device_readiness_token`).
    /// An unknown word maps to absent.
    pub(super) fn from_token(token: &str) -> Option<Self> {
        match token {
            "ready" => Some(Self::Ready),
            "waiting_for_authorization" => Some(Self::WaitingForAuthorization),
            "unavailable_offline" => Some(Self::UnavailableOffline),
            "unavailable_no_permissions" => Some(Self::UnavailableNoPermissions),
            _ => None,
        }
    }
}

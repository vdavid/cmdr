//! Copy, move, delete, and trash, their scan preview, and the questions a running
//! transfer asks (`commands/file_system/{write_ops,volume_copy}.rs` and
//! `commands/volumes.rs`'s `get_volume_space` in the app).

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::deadline::{Deadline, TimedOut};
use cmdr_fs::volume::{SpaceInfo, WriteAccess};
use cmdr_operation_log::types::{Initiator, OperationId};
use cmdr_operations::{
    CONFLICT_CHECK_BUDGET, ConflictId, ConflictResolution, ConflictResolutionOutcome, HomeFolderAction,
    LockedSourcesAnswer, LockedSourcesAnswerOutcome, ScanPreviewPurpose, SourceItemInput, SourcesGoneAnswerOutcome,
    TrashRoutingAnswer, VolumeCopyConfig, WriteOperationConfig, resolve_dest_path,
};
use serde::Deserialize;
use serde_json::Value;

use super::super::DemoOperations;
use super::super::sinks::DemoScanPreviewSink;
use super::OperationIdArgs;
use crate::invoke::{answer, ok, parse};

pub(super) const NAMES: &[&str] = &[
    "copy_between_volumes",
    "move_between_volumes",
    "move_files",
    "delete_files",
    "trash_files",
    "trash_routing_for_paths",
    "check_home_folder_guard",
    "cancel_write_operation",
    "start_scan_preview",
    "cancel_scan_preview",
    "check_scan_preview_status",
    "scan_volume_for_conflicts",
    "destination_write_access",
    "resolve_write_conflict",
    "continue_without_gone_sources",
    "answer_locked_sources",
    "get_volume_space",
];

impl DemoOperations {
    pub(super) async fn transfers(&self, command: &str, args: Value) -> Result<Value, Value> {
        let e = &self.engine;
        match command {
            "copy_between_volumes" => {
                let a: TransferArgs = parse(command, args)?;
                answer(
                    e.start_volume_copy(
                        self.op_sink(),
                        a.source_volume_id,
                        a.source_paths.iter().map(PathBuf::from).collect(),
                        a.dest_volume_id,
                        a.dest_path,
                        a.config.unwrap_or_default(),
                        a.initiator,
                        None,
                    )
                    .await,
                )
            }
            "move_between_volumes" => {
                let a: TransferArgs = parse(command, args)?;
                answer(
                    e.start_volume_move(
                        self.op_sink(),
                        a.source_volume_id,
                        a.source_paths.iter().map(PathBuf::from).collect(),
                        a.dest_volume_id,
                        a.dest_path,
                        a.config.unwrap_or_default(),
                        a.initiator,
                        None,
                    )
                    .await,
                )
            }
            // The desktop's same-`root` move. With no local disk the engine runs it
            // as a volume move on `root`, carrying the dialog's `previewId` along.
            "move_files" => {
                let a: MoveFilesArgs = parse(command, args)?;
                answer(
                    e.move_files_start(
                        self.op_sink(),
                        a.sources.iter().map(PathBuf::from).collect(),
                        PathBuf::from(a.destination),
                        a.config.unwrap_or_default(),
                        vec![],
                        None,
                        a.initiator,
                        None,
                        None,
                    )
                    .await,
                )
            }
            "delete_files" => {
                let a: DeleteArgs = parse(command, args)?;
                answer(
                    e.delete_files_start(
                        self.op_sink(),
                        a.sources.iter().map(PathBuf::from).collect(),
                        a.config.unwrap_or_default(),
                        a.volume_id,
                        a.initiator,
                        None,
                    )
                    .await,
                )
            }
            // The engine refuses it typed: the OS trash needs the local disk.
            "trash_files" => {
                let a: TrashArgs = parse(command, args)?;
                answer(
                    e.trash_files_start(
                        self.op_sink(),
                        a.sources.iter().map(PathBuf::from).collect(),
                        a.item_sizes,
                        a.config.unwrap_or_default(),
                        a.initiator,
                        None,
                    )
                    .await,
                )
            }
            // No cloud folder holds anything online-only here: the trash answer, as
            // the desktop's timeout default is.
            "trash_routing_for_paths" => ok(TrashRoutingAnswer::TRASH),
            "check_home_folder_guard" => {
                let a: HomeFolderGuardArgs = parse(command, args)?;
                let volume_id = a.volume_id.unwrap_or_else(|| self.volume_id.clone());
                let sources: Vec<PathBuf> = a.sources.iter().map(PathBuf::from).collect();
                ok(e.home_folder_guard(&volume_id, &sources, a.action))
            }
            "cancel_write_operation" => {
                let a: CancelArgs = parse(command, args)?;
                e.cancel_write_operation(&OperationId::from(a.operation_id), a.rollback);
                Ok(Value::Null)
            }
            "start_scan_preview" => {
                let a: ScanPreviewArgs = parse(command, args)?;
                let volume_id = a.source_volume_id.unwrap_or_else(|| self.volume_id.clone());
                // `None` asks the engine for the volume its id names.
                answer(e.start_scan_preview(
                    Arc::new(DemoScanPreviewSink {
                        sink: Arc::clone(&self.sink),
                    }),
                    a.sources.iter().map(PathBuf::from).collect(),
                    None,
                    volume_id,
                    a.progress_interval_ms.unwrap_or(500),
                    a.purpose,
                    a.mac_only_files,
                ))
            }
            "cancel_scan_preview" => {
                let a: PreviewIdArgs = parse(command, args)?;
                e.cancel_scan_preview(&a.preview_id);
                Ok(Value::Null)
            }
            "check_scan_preview_status" => {
                let a: PreviewIdArgs = parse(command, args)?;
                ok(e.get_scan_preview_totals(&a.preview_id))
            }
            "scan_volume_for_conflicts" => {
                let a: ConflictScanArgs = parse(command, args)?;
                answer(
                    e.scan_volume_for_conflicts_within(
                        Deadline::new(CONFLICT_CHECK_BUDGET),
                        a.volume_id,
                        a.source_items,
                        a.dest_path,
                        a.source_volume_id,
                        a.source_paths,
                    )
                    .await,
                )
            }
            "destination_write_access" => {
                let a: DestArgs = parse(command, args)?;
                if a.dest_volume_id != self.volume_id {
                    return ok(WriteAccess::Unknown);
                }
                let dest = resolve_dest_path(&self.volume, a.dest_path);
                ok(self.volume.write_access_at(&dest).await)
            }
            "resolve_write_conflict" => {
                let a: ResolveConflictArgs = parse(command, args)?;
                let outcome: ConflictResolutionOutcome = e.resolve_write_conflict(
                    &OperationId::from(a.operation_id),
                    a.conflict_id,
                    a.resolution,
                    a.apply_to_all,
                );
                ok(outcome)
            }
            "continue_without_gone_sources" => {
                let a: OperationIdArgs = parse(command, args)?;
                let outcome: SourcesGoneAnswerOutcome =
                    e.continue_without_gone_sources(&OperationId::from(a.operation_id));
                ok(outcome)
            }
            "answer_locked_sources" => {
                let a: LockedArgs = parse(command, args)?;
                let outcome: LockedSourcesAnswerOutcome =
                    e.answer_locked_sources(&OperationId::from(a.operation_id), a.answer);
                ok(outcome)
            }
            // The page has one volume, so every path's space is its space.
            "get_volume_space" => {
                let _: PathArgs = parse(command, args)?;
                let space: Option<SpaceInfo> = self.volume.get_space_info().await.ok();
                ok(TimedOut {
                    data: space,
                    timed_out: false,
                })
            }
            other => Err(Value::String(format!("the demo doesn't answer `{other}`"))),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferArgs {
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Initiator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveFilesArgs {
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
    initiator: Initiator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteArgs {
    sources: Vec<String>,
    volume_id: Option<String>,
    config: Option<WriteOperationConfig>,
    initiator: Initiator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashArgs {
    sources: Vec<String>,
    item_sizes: Option<Vec<u64>>,
    config: Option<WriteOperationConfig>,
    initiator: Initiator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelArgs {
    operation_id: String,
    rollback: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanPreviewArgs {
    sources: Vec<String>,
    source_volume_id: Option<String>,
    progress_interval_ms: Option<u64>,
    purpose: ScanPreviewPurpose,
    #[serde(default)]
    mac_only_files: cmdr_fs::mac_only_files::MacOnlyFiles,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HomeFolderGuardArgs {
    sources: Vec<String>,
    volume_id: Option<String>,
    action: HomeFolderAction,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewIdArgs {
    preview_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConflictScanArgs {
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
    source_volume_id: Option<String>,
    source_paths: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DestArgs {
    dest_volume_id: String,
    dest_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveConflictArgs {
    operation_id: String,
    conflict_id: ConflictId,
    resolution: ConflictResolution,
    apply_to_all: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockedArgs {
    operation_id: String,
    answer: LockedSourcesAnswer,
}

#[derive(Deserialize)]
struct PathArgs {
    #[serde(rename = "path")]
    _path: String,
}

//! New folder, new file, rename, and their pre-flights
//! (`commands/file_system/write_ops.rs` and `commands/rename.rs` in the app):
//! each answers within the same deadline the desktop's does, and one that
//! outlives it answers `StillRunning` and settles on `mutation-settled`.

use std::path::PathBuf;

use cmdr_operation_log::types::Initiator;
use cmdr_operations::{MUTATION_REPLY_DEADLINE, MutationError, MutationSettled, reply_within};
use serde::Deserialize;
use serde_json::Value;

use super::super::DemoOperations;
use super::super::wire::{self, emit};
use crate::commands::{answer, ok, parse};

pub(super) const NAMES: &[&str] = &[
    "create_directory",
    "create_file",
    "rename_file",
    "check_rename_permission",
    "check_rename_validity",
    "move_to_trash",
];

impl DemoOperations {
    /// Where an instant mutation that outlived its deadline reports how it ended.
    fn on_settled(&self) -> impl FnOnce(MutationSettled) + Send + 'static {
        let sink = std::sync::Arc::clone(&self.sink);
        move |settled| emit(&*sink, wire::MUTATION_SETTLED, &settled)
    }

    pub(super) async fn instant(&self, command: &str, args: Value) -> Result<Value, Value> {
        let e = &self.engine;
        match command {
            "create_directory" | "create_file" => {
                let a: CreateArgs = parse(command, args)?;
                let engine = e.clone();
                let make_dir = command == "create_directory";
                answer(
                    reply_within(
                        MUTATION_REPLY_DEADLINE,
                        async move {
                            if make_dir {
                                engine
                                    .create_directory_managed(a.volume_id, a.parent_path, a.name, a.initiator)
                                    .await
                            } else {
                                engine
                                    .create_file_managed(a.volume_id, a.parent_path, a.name, a.initiator)
                                    .await
                            }
                        },
                        self.on_settled(),
                    )
                    .await,
                )
            }
            "rename_file" => {
                let a: RenameArgs = parse(command, args)?;
                let engine = e.clone();
                let volume_id = a.volume_id.unwrap_or_else(|| self.volume_id.clone());
                answer(
                    reply_within(
                        MUTATION_REPLY_DEADLINE,
                        async move {
                            engine
                                .rename_managed(
                                    PathBuf::from(a.from),
                                    PathBuf::from(a.to),
                                    a.force,
                                    volume_id,
                                    a.initiator,
                                )
                                .await
                        },
                        self.on_settled(),
                    )
                    .await,
                )
            }
            "check_rename_permission" => {
                let a: RenamePermissionArgs = parse(command, args)?;
                let volume_id = a.volume_id.unwrap_or_else(|| self.volume_id.clone());
                answer(
                    e.check_rename_permission_for_volume(PathBuf::from(a.path), volume_id)
                        .await,
                )
            }
            "check_rename_validity" => {
                let a: RenameValidityArgs = parse(command, args)?;
                let volume_id = a.volume_id.unwrap_or_else(|| self.volume_id.clone());
                let check = e
                    .check_rename_validity_impl(a.dir, a.old_name, a.new_name, volume_id)
                    .await;
                // No storage bill to warn about in a page (the desktop prices S3 moves).
                ok(check.priced(|_| false))
            }
            // The OS trash belongs to the local disk, which a page has none of.
            "move_to_trash" => {
                let _: TrashOneArgs = parse(command, args)?;
                answer(Err::<(), _>(MutationError::TrashNotSupported))
            }
            other => Err(Value::String(format!("the demo doesn't answer `{other}`"))),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateArgs {
    volume_id: Option<String>,
    parent_path: String,
    name: String,
    initiator: Initiator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameArgs {
    from: String,
    to: String,
    force: bool,
    volume_id: Option<String>,
    initiator: Initiator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenamePermissionArgs {
    path: String,
    volume_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameValidityArgs {
    dir: String,
    old_name: String,
    new_name: String,
    volume_id: Option<String>,
}

#[derive(Deserialize)]
struct TrashOneArgs {
    #[serde(rename = "path")]
    _path: String,
    #[serde(rename = "initiator")]
    _initiator: Initiator,
}

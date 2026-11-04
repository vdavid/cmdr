//! The path questions a write's dialogs ask (`commands/file_system/{path_checks,stat}.rs` in the
//! app), answered by the volume itself, so they see every write the engine made.

use std::path::Path;

use cmdr_fs::deadline::TimedOut;
use serde::Deserialize;
use serde_json::Value;

use super::super::DemoOperations;
use crate::invoke::{ok, parse};

pub(super) const NAMES: &[&str] = &[
    "path_exists",
    "destination_exists",
    "destination_root_echo",
    "stat_paths_kinds",
];

impl DemoOperations {
    pub(super) async fn paths(&self, command: &str, args: Value) -> Result<Value, Value> {
        match command {
            // `destination_exists` also counts the name in another Unicode form on the
            // desktop; the demo's names are the page's own, so the exact check is the answer.
            "path_exists" | "destination_exists" => {
                let a: PathExistsArgs = parse(command, args)?;
                let here = a.volume_id.as_deref().is_none_or(|id| id == self.volume_id);
                let exists = here && self.volume.exists(Path::new(&a.path)).await;
                ok(TimedOut {
                    data: exists,
                    timed_out: false,
                })
            }
            "destination_root_echo" => {
                let a: RootEchoArgs = parse(command, args)?;
                let echo = (a.dest_volume_id == self.volume_id)
                    .then(|| cmdr_fs::volume::root_echo(self.volume.root(), Path::new(&a.dest_path)))
                    .flatten()
                    .map(|echo| RootEchoAnswer {
                        root_folder: echo.root_folder.to_string_lossy().into_owned(),
                        resolved: echo.resolved.to_string_lossy().into_owned(),
                        stripped: echo.stripped.to_string_lossy().into_owned(),
                    });
                ok(echo)
            }
            // `true` for a folder, `false` for anything else, `null` for a path the volume
            // doesn't hold, as the desktop's `stat` answers.
            "stat_paths_kinds" => {
                let a: StatArgs = parse(command, args)?;
                let mut kinds = Vec::with_capacity(a.paths.len());
                for path in &a.paths {
                    kinds.push(
                        self.volume
                            .get_metadata(Path::new(path))
                            .await
                            .ok()
                            .map(|e| e.is_directory),
                    );
                }
                ok(TimedOut {
                    data: kinds,
                    timed_out: false,
                })
            }
            other => Err(Value::String(format!("the demo doesn't answer `{other}`"))),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathExistsArgs {
    volume_id: Option<String>,
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootEchoArgs {
    dest_volume_id: String,
    dest_path: String,
}

/// `DestinationRootEcho` in the app (which lives in the app crate).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RootEchoAnswer {
    root_folder: String,
    resolved: String,
    stripped: String,
}

#[derive(Deserialize)]
struct StatArgs {
    paths: Vec<String>,
}

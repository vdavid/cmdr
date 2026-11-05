//! The desktop app's Multi-Rename engine ([`app_multi_rename`]) and its answers to the
//! crate's host seam (`MultiRenameHost`): media reads through the volume manager,
//! the Results file in the app's cache dir, the rename's place by the volume
//! manager's routes, `io_budget_for_volume`, and the app's write engine. Plus the
//! typed `multi-rename-media-progress` event, which only a Tauri app can send.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use cmdr_fs::deadline::Deadline;
use cmdr_fs::rt::Runtime;
use cmdr_media_meta::{ReadError, VolumeByteSource, read_media_from};
use cmdr_operations::{Operations, RenameStartError};

use super::app_files::results_path;
use super::error::{MultiRenameError, ReadOnlyPlace};
use super::media::{FileJob, FileReader, Loader, MAX_PARALLEL_READS, MediaProgressEvent, ProgressEmit};
use super::plan::PreviewRow;
use super::{MultiRename, MultiRenameHost, RenamePlace};
use crate::file_system::git::wiring::portal_serves;
use crate::file_system::volume::io_budget_for_volume;
use crate::file_system::volume::manager::{get_volume_manager, path_routes_over_its_parent};

/// The app's one Multi-Rename engine, over the app's listing engine and host, on
/// Tauri's runtime (the pool the IPC commands' blocking work ran on before the
/// engine held one). A `LazyLock`, not a slot `setup()` fills: the app's unit tests
/// install nothing and still need an engine, and a test's own runtime would die
/// with its test while this outlives it. First touched by an IPC command or the
/// MCP tool, so after `setup()` installed the listing engine.
pub fn app_multi_rename() -> &'static MultiRename {
    static ENGINE: LazyLock<MultiRename> = LazyLock::new(|| {
        MultiRename::builder(
            crate::listing_host::listings().clone(),
            tauri::async_runtime::handle().inner().clone(),
            Arc::new(AppHost),
        )
        // The app's tests share this engine and run in parallel, so there the cap
        // is out of reach (eviction has its own tests in the crate).
        .max_sessions(if cfg!(test) {
            1024
        } else {
            cmdr_multi_rename::session::MAX_SESSIONS
        })
        .build()
    });
    &ENGINE
}

/// The `multi-rename-media-progress` event: a session's read moved on. At most
/// every 250 ms, plus a final one with `done`. ❗ The struct name kebab-cases to
/// the wire event name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameMediaProgress {
    pub session_id: String,
    pub read: usize,
    pub total: usize,
    pub done: bool,
}

impl From<MediaProgressEvent> for MultiRenameMediaProgress {
    fn from(event: MediaProgressEvent) -> Self {
        let MediaProgressEvent {
            session_id,
            read,
            total,
            done,
        } = event;
        Self {
            session_id,
            read,
            total,
            done,
        }
    }
}

/// One file's read budget on a local disk; a volume with a live session gets
/// more (`io_budget_for_volume`).
const READ_BUDGET: Duration = Duration::from_secs(10);

/// Reads files on the volume `volume_id` (resolved per file, so a reconnect's
/// new volume is the one read) and reports through `emit`, its byte reads on
/// `runtime`. The MCP tool builds its own over its ambient runtime.
pub(crate) fn volume_loader(volume_id: String, emit: ProgressEmit, runtime: &Runtime) -> Loader {
    Loader {
        reader: volume_reader(volume_id.clone(), runtime.clone()),
        emit,
        parallel: MAX_PARALLEL_READS,
        volume_gone: Arc::new(move || {
            get_volume_manager()
                .get(&volume_id)
                .is_none_or(|volume| volume.connection_state().is_some_and(|state| !state.is_live()))
        }),
    }
}

/// The real [`FileReader`]: `read_media_from` over the file's bytes through
/// `Volume::read_range` (64 KiB blocks, cover art never fetched), within one
/// file's budget, on `runtime`.
fn volume_reader(volume_id: String, runtime: Runtime) -> FileReader {
    Arc::new(move |job: &FileJob| {
        let volume = get_volume_manager()
            .get(&volume_id)
            .ok_or_else(|| ReadError::Io("the session's volume isn't registered".to_string()))?;
        let budget = io_budget_for_volume(&volume_id, READ_BUDGET);
        let source = VolumeByteSource::new(
            volume,
            job.path.clone(),
            job.size,
            runtime.clone(),
            Deadline::new(budget),
            Arc::clone(&job.cancel),
        );
        read_media_from(source, &job.name)
    })
}

/// Which route serves a row's path, lexically and in `resolve`'s order (the git
/// portal first, so a `.zip` inside a snapshot is the portal's).
fn row_route(path: &Path) -> Option<ReadOnlyPlace> {
    if portal_serves(path) {
        Some(ReadOnlyPlace::GitPortal)
    } else if path_routes_over_its_parent(path) {
        Some(ReadOnlyPlace::Archive)
    } else {
        None
    }
}

struct AppHost;

impl MultiRenameHost for AppHost {
    fn media_loader(&self, volume_id: &str, emit: ProgressEmit, runtime: &Runtime) -> Loader {
        volume_loader(volume_id.to_string(), emit, runtime)
    }

    fn results_path(&self, session_id: &str) -> Option<PathBuf> {
        results_path(session_id)
    }

    fn results_supported(&self) -> bool {
        true
    }

    fn rename_place<'a>(
        &'a self,
        volume_id: &'a str,
        dir: &'a Path,
        ready: &'a [PreviewRow],
    ) -> BoxFuture<'a, RenamePlace> {
        Box::pin(async move {
            let resolved = get_volume_manager().resolve(volume_id, dir).await;
            let routed = resolved.routed.map(ReadOnlyPlace::from).or_else(|| {
                ready.iter().find_map(|row| {
                    [&row.old_name, &row.new_name]
                        .into_iter()
                        .find_map(|name| row_route(&dir.join(name)))
                })
            });
            if let Some(place) = routed {
                RenamePlace::ReadOnly(place)
            } else if volume_id == "root" {
                RenamePlace::LocalDisk
            } else {
                resolved.volume.map_or(RenamePlace::NotConnected, RenamePlace::Volume)
            }
        })
    }

    fn io_budget(&self, volume_id: &str, base: Duration) -> Duration {
        io_budget_for_volume(volume_id, base)
    }

    fn operations(&self) -> Result<Operations, MultiRenameError> {
        crate::operations_host::operations()
            .cloned()
            .map_err(|not_installed| MultiRenameError::CouldntStart {
                reason: RenameStartError::Engine {
                    error: crate::operations_host::refuse_not_installed(not_installed),
                },
            })
    }
}

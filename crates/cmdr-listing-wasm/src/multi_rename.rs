//! The Multi-Rename Tool (⌃M) in the browser: `cmdr-multi-rename`'s engine over the
//! demo's one volume, `Listings`, and write engine, answering the sheet's commands
//! the way the desktop's `commands/multi_rename.rs` does. The demo's fourth engine
//! host, beside the listing, the viewer, and the search.
//!
//! What a page can't do fails typed, through paths the sheet already words: every
//! media read answers `NotSupported` (rows using a media field say they couldn't be
//! read, and the read ends `done` at once), and there's no Results (no text editor
//! to hand the names list to; the sheet disables the button on
//! `resultsAvailable: false`). Presets, the last settings, and the field history
//! live in memory, gone on reload like the volume. Every method runs inline: the
//! desktop's IPC deadlines guard hung mounts, which an in-memory volume can't be.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::rt::Runtime;
use cmdr_fs::volume::Volume;
use cmdr_listing::Listings;
use cmdr_media_meta::ReadError;
use cmdr_multi_rename::error::MultiRenameError;
use cmdr_multi_rename::history::{FieldHistoryEntry, MAX_FIELD_HISTORY, history_entries, history_seed};
use cmdr_multi_rename::media::{Loader, MAX_PARALLEL_READS, MediaProgressEvent, ProgressEmit};
use cmdr_multi_rename::plan::{MultiRenameSpec, PreviewRow, RenameExample, render_examples};
use cmdr_multi_rename::presets::{
    MAX_PRESETS, MultiRenameLastSettings, MultiRenamePreset, PresetList, PresetMove, PresetSeed, delete_in, move_in,
    rename_in, save_in, seed_in, update_spec_in,
};
use cmdr_multi_rename::run::MultiRenameStarted;
use cmdr_multi_rename::session::{MultiRenameOpened, PreviewFilter, RowMove};
use cmdr_multi_rename::{MultiRename, MultiRenameHost, RenamePlace};
use cmdr_operations::Operations;
use futures_util::future::BoxFuture;
use serde::Deserialize;
use serde_json::Value;

use crate::events::EventSink;
use crate::invoke::{answer, ok, parse};
use crate::operations_host::DemoOperations;

/// Every command [`DemoMultiRename::invoke`] answers, by its Tauri name: all of the
/// desktop's `commands/multi_rename.rs`.
pub const MULTI_RENAME_COMMANDS: &[&str] = &[
    "open_multi_rename",
    "preview_multi_rename",
    "get_multi_rename_preview_rows",
    "move_multi_rename_row",
    "get_multi_rename_range_samples",
    "get_multi_rename_media_progress",
    "get_multi_rename_media_overview",
    "get_multi_rename_field_samples",
    "prioritize_multi_rename_media",
    "render_multi_rename_examples",
    "apply_multi_rename",
    "get_multi_rename_history",
    "write_multi_rename_names",
    "read_multi_rename_names",
    "clear_multi_rename_names",
    "close_multi_rename",
    "get_multi_rename_last_settings",
    "save_multi_rename_last_settings",
    "get_multi_rename_presets",
    "save_multi_rename_preset",
    "delete_multi_rename_preset",
    "rename_multi_rename_preset",
    "update_multi_rename_preset",
    "move_multi_rename_preset",
];

/// A session's media read moved on (`MultiRenameMediaProgress` in the app; the
/// payload is the crate's `MediaProgressEvent`, which serializes the same).
pub const MULTI_RENAME_MEDIA_PROGRESS: &str = "multi-rename-media-progress";

/// The sheet's engine over the demo's volume, plus what the desktop keeps in files.
pub struct DemoMultiRename {
    engine: MultiRename,
    /// Where the media read's progress goes: the page, as is.
    sink: Arc<dyn EventSink>,
    /// The write engine renames run on, and a fresh sink per rename from it, so a
    /// rename's events reach the page (and the drop staging's watch) as every write's do.
    operations: DemoOperations,
    kept: Mutex<Kept>,
    minted_ids: AtomicU64,
}

/// What the desktop keeps in files, here for the page's life.
struct Kept {
    presets: PresetList,
    last: Option<MultiRenameLastSettings>,
    history: Vec<FieldHistoryEntry>,
}

impl DemoMultiRename {
    /// The engine over `listings` (the panes' own) on `runtime`, renaming on
    /// `volume` (filed under `volume_id`) through `operations` (the page's one write
    /// engine), and sending the media read's progress to `sink`.
    pub(crate) fn new(
        listings: Listings,
        runtime: Runtime,
        volume_id: String,
        volume: Arc<dyn Volume>,
        operations: DemoOperations,
        sink: Arc<dyn EventSink>,
    ) -> Self {
        let host = Arc::new(DemoHost {
            volume_id,
            volume,
            operations: operations.engine().clone(),
        });
        Self {
            engine: MultiRename::builder(listings, runtime, host).build(),
            sink,
            operations,
            kept: Mutex::new(Kept {
                presets: PresetList::default(),
                last: None,
                // A fresh page is a new install.
                history: history_seed(),
            }),
            minted_ids: AtomicU64::new(0),
        }
    }

    /// Whether [`Self::invoke`] answers `command`.
    pub fn handles(command: &str) -> bool {
        MULTI_RENAME_COMMANDS.contains(&command)
    }

    /// Runs `command` with Tauri-style `args` (camelCase keys).
    pub async fn invoke(&self, command: &str, args: Value) -> Result<Value, Value> {
        let e = &self.engine;
        match command {
            "open_multi_rename" => {
                let a: OpenArgs = parse(command, args)?;
                answer(self.open(a))
            }
            "preview_multi_rename" => {
                let a: PreviewArgs = parse(command, args)?;
                answer(e.preview(&a.session_id, &a.spec))
            }
            "get_multi_rename_preview_rows" => {
                let a: RowsArgs = parse(command, args)?;
                answer(e.page(&a.session_id, a.preview_id, a.offset, a.limit, a.filter))
            }
            "move_multi_rename_row" => {
                let a: MoveRowArgs = parse(command, args)?;
                answer(e.move_row(&a.session_id, a.row, a.direction))
            }
            "get_multi_rename_range_samples" => {
                let a: SamplesArgs = parse(command, args)?;
                answer(e.range_samples(&a.session_id, &a.token, &a.rows))
            }
            "get_multi_rename_media_progress" => {
                let a: SessionArgs = parse(command, args)?;
                answer(e.media_progress(&a.session_id))
            }
            "get_multi_rename_media_overview" => {
                let a: SessionArgs = parse(command, args)?;
                answer(e.media_overview(&a.session_id))
            }
            "get_multi_rename_field_samples" => {
                let a: SamplesArgs = parse(command, args)?;
                answer(e.field_samples(&a.session_id, &a.token, &a.rows))
            }
            "prioritize_multi_rename_media" => {
                let a: PrioritizeArgs = parse(command, args)?;
                // The only refusal is a closed session, whose read already stopped.
                let _ = e.prioritize_media(&a.session_id, &a.rows);
                Ok(Value::Null)
            }
            "render_multi_rename_examples" => {
                let a: ExamplesArgs = parse(command, args)?;
                ok(render_examples(&a.examples))
            }
            "apply_multi_rename" => {
                let a: ApplyArgs = parse(command, args)?;
                answer(self.apply(a.session_id, a.preview_id).await)
            }
            "get_multi_rename_history" => ok(&self.kept.lock_ignore_poison().history),
            "write_multi_rename_names" => {
                // No Results in a page: the host has nowhere to write, so this
                // refuses `couldntWriteNames` (the sheet never asks: `resultsAvailable`).
                let a: ApplyArgs = parse(command, args)?;
                answer(
                    e.write_names(&a.session_id, a.preview_id)
                        .map(|path| path.to_string_lossy().into_owned()),
                )
            }
            "read_multi_rename_names" => {
                let a: SessionArgs = parse(command, args)?;
                answer(e.read_names(&a.session_id))
            }
            "clear_multi_rename_names" => {
                let a: SessionArgs = parse(command, args)?;
                answer(e.clear_names(&a.session_id))
            }
            "close_multi_rename" => {
                let a: SessionArgs = parse(command, args)?;
                e.close(&a.session_id);
                Ok(Value::Null)
            }
            "get_multi_rename_last_settings" => ok(&self.kept.lock_ignore_poison().last),
            "save_multi_rename_last_settings" => {
                let a: LastSettingsArgs = parse(command, args)?;
                self.kept.lock_ignore_poison().last = Some(MultiRenameLastSettings {
                    spec: a.spec,
                    preset_id: a.preset_id,
                });
                Ok(Value::Null)
            }
            "get_multi_rename_presets" => {
                let a: SeedsArgs = parse(command, args)?;
                let mut kept = self.kept.lock_ignore_poison();
                seed_in(&mut kept.presets, a.seeds, MAX_PRESETS);
                ok(&kept.presets.entries)
            }
            "save_multi_rename_preset" => {
                let a: PresetArgs = parse(command, args)?;
                let mut kept = self.kept.lock_ignore_poison();
                answer(save_in(&mut kept.presets.entries, a.preset, MAX_PRESETS).map(|()| &kept.presets.entries))
            }
            "delete_multi_rename_preset" => {
                let a: IdArgs = parse(command, args)?;
                self.edit_presets(|presets| delete_in(presets, &a.id))
            }
            "rename_multi_rename_preset" => {
                let a: RenamePresetArgs = parse(command, args)?;
                self.edit_presets(|presets| rename_in(presets, &a.id, &a.name))
            }
            "update_multi_rename_preset" => {
                let a: UpdatePresetArgs = parse(command, args)?;
                self.edit_presets(|presets| update_spec_in(presets, &a.id, &a.spec))
            }
            "move_multi_rename_preset" => {
                let a: MovePresetArgs = parse(command, args)?;
                self.edit_presets(|presets| move_in(presets, &a.id, a.direction))
            }
            _ => Err(Value::String(format!("the browser demo doesn't answer `{command}`"))),
        }
    }

    /// `open_multi_rename`: the session, then its media read, reporting to the page.
    fn open(&self, a: OpenArgs) -> Result<MultiRenameOpened, MultiRenameError> {
        let opened = self.engine.open(
            &a.listing_id,
            a.include_hidden,
            a.selected_indices.as_deref(),
            a.expected_sequence,
        )?;
        let sink = Arc::clone(&self.sink);
        let emit: ProgressEmit = Arc::new(
            move |progress: MediaProgressEvent| match serde_json::to_value(progress) {
                Ok(payload) => sink.emit(MULTI_RENAME_MEDIA_PROGRESS, payload),
                Err(error) => {
                    log::error!(target: "multi_rename::media", "demo: the progress didn't serialize: {error}")
                }
            },
        );
        self.engine.start_media(&opened.session_id, emit)?;
        Ok(opened)
    }

    /// `apply_multi_rename`: the rename, then the fields' history, as the desktop's command.
    async fn apply(&self, session_id: String, preview_id: u64) -> Result<MultiRenameStarted, MultiRenameError> {
        // Read before apply: the spec of the preview it starts from, for the history.
        let spec = self.engine.spec_of(&session_id, preview_id).ok();
        let started = self
            .engine
            .apply(self.operations.op_sink(), session_id, preview_id)
            .await?;
        let mut kept = self.kept.lock_ignore_poison();
        for entry in spec.as_ref().map(history_entries).unwrap_or_default() {
            cmdr_recents::add_to(&mut kept.history, entry, MAX_FIELD_HISTORY, || {
                format!("demo-{}", self.minted_ids.fetch_add(1, Ordering::Relaxed))
            });
        }
        Ok(started)
    }

    /// A preset list edit that answers the list after, changed or not.
    fn edit_presets(&self, change: impl FnOnce(&mut Vec<MultiRenamePreset>) -> bool) -> Result<Value, Value> {
        let mut kept = self.kept.lock_ignore_poison();
        // Changed or not, the list is the answer (the desktop's `changed` only decides a file write).
        let _ = change(&mut kept.presets.entries);
        ok(&kept.presets.entries)
    }
}

/// The demo's answers to the engine: one volume, no media reader, no Results.
struct DemoHost {
    volume_id: String,
    volume: Arc<dyn Volume>,
    operations: Operations,
}

impl MultiRenameHost for DemoHost {
    fn media_loader(&self, _volume_id: &str, emit: ProgressEmit, _runtime: &Runtime) -> Loader {
        Loader {
            // A page reads no EXIF or tags: every file settles `Unreadable` at once,
            // without counting toward the loader's give-up.
            reader: Arc::new(|_| Err(ReadError::NotSupported)),
            emit,
            parallel: MAX_PARALLEL_READS,
            volume_gone: Arc::new(|| false),
        }
    }

    fn results_path(&self, _session_id: &str) -> Option<PathBuf> {
        None
    }

    fn results_supported(&self) -> bool {
        false
    }

    fn rename_place<'a>(
        &'a self,
        volume_id: &'a str,
        _dir: &'a Path,
        _ready: &'a [PreviewRow],
    ) -> BoxFuture<'a, RenamePlace> {
        // No routes and no local disk: the page volume, through the volume engine.
        let place = if volume_id == self.volume_id {
            RenamePlace::Volume(Arc::clone(&self.volume))
        } else {
            RenamePlace::NotConnected
        };
        Box::pin(std::future::ready(place))
    }

    fn io_budget(&self, _volume_id: &str, base: Duration) -> Duration {
        base
    }

    fn operations(&self) -> Result<Operations, MultiRenameError> {
        Ok(self.operations.clone())
    }
}

// The argument shapes, one per command signature in the desktop's `commands/multi_rename.rs`.

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenArgs {
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    expected_sequence: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionArgs {
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewArgs {
    session_id: String,
    spec: MultiRenameSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RowsArgs {
    session_id: String,
    preview_id: u64,
    offset: usize,
    limit: usize,
    filter: PreviewFilter,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveRowArgs {
    session_id: String,
    row: usize,
    direction: RowMove,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SamplesArgs {
    session_id: String,
    token: String,
    rows: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrioritizeArgs {
    session_id: String,
    rows: Vec<usize>,
}

#[derive(Deserialize)]
struct ExamplesArgs {
    examples: Vec<RenameExample>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyArgs {
    session_id: String,
    preview_id: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LastSettingsArgs {
    spec: MultiRenameSpec,
    preset_id: Option<String>,
}

#[derive(Deserialize)]
struct SeedsArgs {
    seeds: Vec<PresetSeed>,
}

#[derive(Deserialize)]
struct PresetArgs {
    preset: MultiRenamePreset,
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct RenamePresetArgs {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct UpdatePresetArgs {
    id: String,
    spec: MultiRenameSpec,
}

#[derive(Deserialize)]
struct MovePresetArgs {
    id: String,
    direction: PresetMove,
}

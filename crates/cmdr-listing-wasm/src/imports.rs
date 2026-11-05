//! Files a visitor drops in from their own computer: their bytes land on the
//! demo's in-memory disk, and the app's own drop flow takes it from there
//! (`DETAILS.md` § Files dropped in from the computer).
//!
//! Two steps, so the page reads only bytes that will land. [`DemoImports::plan`]
//! takes the drop's shape (relative paths, kinds, sizes; no bytes), accepts the
//! longest prefix the visit's budget still covers, creates its folders in a
//! fresh staging folder (`/.drops/<n>`), and reserves its files' bytes. Then
//! [`DemoImports::file`] lands each accepted file's bytes. The page hands the
//! staged top-level paths to the app as an OS drop, so the transfer dialog,
//! its conflict handling, and the write engine do the rest.
//!
//! A staging folder lives only as long as something can still read it: the
//! transfer that claimed it sweeps it at its `write-settled` (done, cancelled,
//! or failed), and a drop no transfer took is swept once no dialog holds file
//! operations, or at the next drop. A sweep gives back the room of whatever
//! didn't land.
//!
//! Nothing here leaves the page: the bytes go from the browser's `File` into
//! the volume's memory, gone on reload.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cmdr_fs::rt::Runtime;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::EventSink;

/// How many bytes one visit may bring in, in all: 10 MB.
pub const IMPORT_BUDGET_BYTES: u64 = 10_000_000;

/// The folder every drop is staged under, hidden like a dotfile.
pub const DROPS_ROOT: &str = "/.drops";

/// What one dropped item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportKind {
    /// A folder; its contents are items of their own.
    Folder,
    /// A file of `size` bytes.
    File,
}

/// One dropped item, as the page reads it off the drop.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    /// Relative to the drop, `/`-separated (`trip/day 1/a.jpg`). A folder comes
    /// before everything in it.
    pub path: String,
    /// A file or a folder.
    pub kind: ImportKind,
    /// The file's size in bytes; ignored for a folder.
    #[serde(default)]
    pub size: u64,
    /// The modified time, in ms since the epoch, when the browser knows it.
    #[serde(default)]
    pub modified_ms: Option<u64>,
}

/// Why some of a drop didn't come in. The page words it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ImportRefusal {
    /// The visit's budget ran out: the first item that didn't fit, and every
    /// item after it, stayed out.
    #[serde(rename_all = "camelCase")]
    OverBudget {
        /// The whole visit's budget, in bytes.
        budget_bytes: u64,
        /// How many of the drop's items stayed out.
        left_out: usize,
    },
}

/// What [`DemoImports::plan`] decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    /// The staging batch, for [`DemoImports::file`].
    pub batch: u64,
    /// How many items, from the first, came in.
    pub accepted: usize,
    /// The staged top-level items, in drop order: the paths the drop hands the app.
    pub roots: Vec<String>,
    /// Why the rest stayed out, if any did.
    pub refusal: Option<ImportRefusal>,
}

/// Why a plan or a file didn't land.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ImportError {
    /// An item's path isn't a plain relative path (empty, absolute, or with `.` / `..`).
    #[serde(rename_all = "camelCase")]
    BadPath {
        /// The path as given.
        path: String,
    },
    /// The batch or index names no accepted file.
    UnknownItem,
    /// The bytes aren't the size the plan reserved (the file changed on disk).
    SizeChanged,
    /// The volume refused it.
    #[serde(rename_all = "camelCase")]
    Volume {
        /// The volume's refusal, for the console.
        detail: String,
    },
}

impl From<VolumeError> for ImportError {
    fn from(error: VolumeError) -> Self {
        Self::Volume {
            detail: format!("{error:?}"),
        }
    }
}

/// A planned file still waiting for its bytes.
struct Pending {
    path: PathBuf,
    size: u64,
    modified_secs: Option<u64>,
}

/// Where a staged batch is in its life.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// Planned; its files are still landing. Nothing sweeps it.
    Landing,
    /// Handed to the app as a drop, and no transfer has taken it yet: the
    /// transfer dialog is up, or the drop landed nowhere.
    Offered,
    /// A transfer reads from it; its end sweeps the batch.
    Claimed(String),
}

struct Batch {
    phase: Phase,
    /// The bytes this batch holds against the budget: its planned files', less
    /// any that didn't land in staging.
    reserved: u64,
}

/// What a claiming transfer has shown of itself, for the sweep at its end.
#[derive(Default)]
struct Transfer {
    /// A copy leaves the staged originals behind, so what it landed is what it
    /// reported writing; a move's is whatever left the staging folder.
    copy: bool,
    landed: u64,
    /// Its `write-settled` came before its command answered.
    settled: bool,
}

#[derive(Default)]
struct State {
    /// Bytes imported or reserved this visit.
    used: u64,
    /// Batches handed out so far.
    batches: u64,
    /// `(batch, index)` → the file still to land.
    pending: HashMap<(u64, usize), Pending>,
    /// Every batch with a staging folder.
    staged: HashMap<u64, Batch>,
    /// Transfers that claimed a batch (or settled before their command answered), by operation id.
    transfers: HashMap<String, Transfer>,
}

/// The visit's imports, over the page's one volume.
pub struct DemoImports {
    volume: Arc<InMemoryVolume>,
    budget: u64,
    state: Mutex<State>,
}

impl DemoImports {
    /// Imports onto `volume`, at most `budget` bytes in all.
    pub fn new(volume: Arc<InMemoryVolume>, budget: u64) -> Self {
        Self {
            volume,
            budget,
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Accepts the longest prefix of `items` the budget covers, creates its
    /// folders in a fresh staging folder, and reserves its files' bytes. An
    /// earlier drop nobody took is swept first, so its room is free again.
    pub async fn plan(&self, items: &[ImportItem]) -> Result<ImportPlan, ImportError> {
        let relative: Vec<PathBuf> = items
            .iter()
            .map(|item| relative_path(&item.path))
            .collect::<Result<_, _>>()?;
        self.release_offered().await;
        let (batch, accepted) = {
            let mut state = self.state();
            state.batches += 1;
            let mut room = self.budget.saturating_sub(state.used);
            let mut accepted = 0;
            let mut reserved = 0;
            for item in items {
                let bytes = if item.kind == ImportKind::File { item.size } else { 0 };
                if bytes > room {
                    break;
                }
                room -= bytes;
                reserved += bytes;
                accepted += 1;
            }
            state.used += reserved;
            let batch = state.batches;
            state.staged.insert(
                batch,
                Batch {
                    phase: Phase::Landing,
                    reserved,
                },
            );
            (batch, accepted)
        };

        let staging = staging_of(batch);
        match self.volume.import_folder(Path::new(DROPS_ROOT), None).await {
            Ok(()) | Err(VolumeError::AlreadyExists(_)) => {}
            Err(error) => return Err(self.abandon(batch, error).await),
        }
        if let Err(error) = self.volume.import_folder(&staging, None).await {
            return Err(self.abandon(batch, error).await);
        }

        let mut roots = Vec::new();
        for (index, (item, path)) in items.iter().zip(&relative).take(accepted).enumerate() {
            let staged = staging.join(path);
            if path.components().count() == 1 {
                roots.push(staged.display().to_string());
            }
            let modified_secs = item.modified_ms.map(|ms| ms / 1000);
            match item.kind {
                ImportKind::Folder => {
                    if let Err(error) = self.volume.import_folder(&staged, modified_secs).await {
                        return Err(self.abandon(batch, error).await);
                    }
                }
                ImportKind::File => {
                    self.state().pending.insert(
                        (batch, index),
                        Pending {
                            path: staged,
                            size: item.size,
                            modified_secs,
                        },
                    );
                }
            }
        }

        let left_out = items.len() - accepted;
        Ok(ImportPlan {
            batch,
            accepted,
            roots,
            refusal: (left_out > 0).then_some(ImportRefusal::OverBudget {
                budget_bytes: self.budget,
                left_out,
            }),
        })
    }

    /// Lands the bytes of the planned file `index` of `batch`. A file that
    /// doesn't land gives its reserved bytes back.
    pub async fn file(&self, batch: u64, index: usize, bytes: Vec<u8>) -> Result<(), ImportError> {
        let pending = self
            .state()
            .pending
            .remove(&(batch, index))
            .ok_or(ImportError::UnknownItem)?;
        let landed = if bytes.len() as u64 == pending.size {
            self.volume
                .import_file(&pending.path, bytes, pending.modified_secs)
                .await
                .map_err(ImportError::from)
        } else {
            Err(ImportError::SizeChanged)
        };
        if landed.is_err() {
            let mut state = self.state();
            state.used = state.used.saturating_sub(pending.size);
            if let Some(staged) = state.staged.get_mut(&batch) {
                staged.reserved = staged.reserved.saturating_sub(pending.size);
            }
        }
        landed
    }

    /// The page is done landing `batch`. With `offered`, it handed the staged
    /// paths to the app as a drop, which a transfer may now claim; without,
    /// nothing came of it, and the batch is swept at once.
    pub async fn hand_off(&self, batch: u64, offered: bool) {
        {
            let mut state = self.state();
            drop_pending(&mut state, batch);
            match state.staged.get_mut(&batch) {
                Some(staged) if offered => {
                    staged.phase = Phase::Offered;
                    return;
                }
                Some(_) => {}
                None => return,
            }
        }
        self.sweep(batch, None).await;
    }

    /// No dialog holds file operations any more (the app's
    /// `set_file_operations_blocked { blocked: false }`): a drop no transfer has
    /// claimed by now never will be, because its dialog was dismissed or it
    /// landed nowhere. Sweeps every such batch.
    pub async fn release_offered(&self) {
        let offered: Vec<u64> = self
            .state()
            .staged
            .iter()
            .filter(|(_, staged)| staged.phase == Phase::Offered)
            .map(|(batch, _)| *batch)
            .collect();
        for batch in offered {
            self.sweep(batch, None).await;
        }
    }

    /// A transfer command answered with `operation_id`, reading `sources`: every
    /// offered batch a source lies in is now that transfer's, swept at its end.
    pub async fn claim(&self, operation_id: &str, copy: bool, sources: &[PathBuf]) {
        let settled_already = {
            let mut state = self.state();
            let mut claimed = false;
            for (batch, staged) in state.staged.iter_mut() {
                let staging = staging_of(*batch);
                if staged.phase == Phase::Offered && sources.iter().any(|source| source.starts_with(&staging)) {
                    staged.phase = Phase::Claimed(operation_id.to_string());
                    claimed = true;
                }
            }
            if !claimed {
                state.transfers.remove(operation_id);
                return;
            }
            let transfer = state.transfers.entry(operation_id.to_string()).or_default();
            transfer.copy = copy;
            transfer.settled
        };
        if settled_already {
            self.settle(operation_id).await;
        }
    }

    /// One write-engine event, as the page hears it. Notes a transfer's progress,
    /// and answers the operation id of a `write-settled` that ended a transfer
    /// holding a batch, for the caller to [`settle`](Self::settle).
    pub fn heard(&self, event: &str, payload: &Value) -> Option<String> {
        let operation_id = payload.get("operationId")?.as_str()?;
        let mut state = self.state();
        match event {
            "write-progress" => {
                let bytes = payload.get("bytesDone").and_then(Value::as_u64)?;
                let transfer = state.transfers.entry(operation_id.to_string()).or_default();
                transfer.landed = transfer.landed.max(bytes);
                None
            }
            "write-complete" => {
                let bytes = payload.get("bytesProcessed").and_then(Value::as_u64)?;
                state.transfers.entry(operation_id.to_string()).or_default().landed = bytes;
                None
            }
            "write-settled" => {
                let held = Phase::Claimed(operation_id.to_string());
                if state.staged.values().any(|staged| staged.phase == held) {
                    Some(operation_id.to_string())
                } else {
                    // Its command may not have answered yet; `claim` finishes it then.
                    state.transfers.entry(operation_id.to_string()).or_default().settled = true;
                    None
                }
            }
            _ => None,
        }
    }

    /// The transfer `operation_id` ended (done, cancelled, or failed): sweeps the
    /// batches it held, giving back the room of whatever didn't land.
    pub async fn settle(&self, operation_id: &str) {
        let (batches, transfer) = {
            let mut state = self.state();
            let held = Phase::Claimed(operation_id.to_string());
            let batches: Vec<u64> = state
                .staged
                .iter()
                .filter(|(_, staged)| staged.phase == held)
                .map(|(batch, _)| *batch)
                .collect();
            (batches, state.transfers.remove(operation_id).unwrap_or_default())
        };
        let mut copied = if transfer.copy { transfer.landed } else { 0 };
        for batch in batches {
            copied = self.sweep(batch, Some(copied)).await;
        }
    }

    /// Bytes imported or reserved this visit.
    pub fn used_bytes(&self) -> u64 {
        self.state().used
    }

    /// Whether `batch` still has a staging folder.
    pub fn is_staged(&self, batch: u64) -> bool {
        self.state().staged.contains_key(&batch)
    }

    /// Removes `batch`'s staging folder and gives back the room of what didn't
    /// land: what's still staged, less what a copy wrote from it (`copied`).
    /// Answers the part of `copied` this batch didn't use up.
    async fn sweep(&self, batch: u64, copied: Option<u64>) -> u64 {
        drop_pending(&mut self.state(), batch);
        let staging = staging_of(batch);
        let mut held: Vec<(PathBuf, bool, u64)> = self
            .volume
            .held_entries()
            .into_iter()
            .map(|entry| (PathBuf::from(&entry.path), entry.is_directory, entry.size.unwrap_or(0)))
            .filter(|(path, _, _)| path.starts_with(&staging))
            .collect();
        let still_staged: u64 = held.iter().filter(|(_, dir, _)| !dir).map(|(_, _, size)| size).sum();
        // Deepest first: the store refuses to delete a folder that holds something.
        held.sort_by_key(|(path, _, _)| std::cmp::Reverse(path.components().count()));
        for (path, _, _) in &held {
            if let Err(error) = self.volume.delete(path).await {
                log::warn!(target: "imports", "couldn't clear a staged drop: {}", cmdr_fs::log_error::LogError(&error));
            }
        }
        let copied = copied.unwrap_or(0);
        let mut state = self.state();
        if let Some(staged) = state.staged.remove(&batch) {
            let refund = still_staged.saturating_sub(copied).min(staged.reserved);
            state.used = state.used.saturating_sub(refund);
        }
        copied.saturating_sub(still_staged)
    }

    /// Drops a plan the volume cut short: its staging is swept and its room
    /// given back.
    async fn abandon(&self, batch: u64, error: VolumeError) -> ImportError {
        drop_pending(&mut self.state(), batch);
        self.sweep(batch, None).await;
        error.into()
    }
}

/// Forgets `batch`'s files that never got their bytes, giving their room back:
/// after this, a batch's reservation is exactly what it stored.
fn drop_pending(state: &mut State, batch: u64) {
    let mut unsent = 0;
    state.pending.retain(|(b, _), pending| {
        if *b == batch {
            unsent += pending.size;
        }
        *b != batch
    });
    state.used = state.used.saturating_sub(unsent);
    if let Some(staged) = state.staged.get_mut(&batch) {
        staged.reserved = staged.reserved.saturating_sub(unsent);
    }
}

/// The write engine's events on their way to the page, watched for a claiming
/// transfer's end: its `write-settled` sweeps the batches it held.
pub struct ImportWatch {
    /// Where the events go.
    pub inner: Arc<dyn EventSink>,
    /// The imports a transfer's end settles.
    pub imports: Arc<DemoImports>,
    /// Where the sweep runs: the event arrives inside the engine's own call.
    pub runtime: Runtime,
}

impl EventSink for ImportWatch {
    fn emit(&self, event: &str, payload: Value) {
        let ended = self.imports.heard(event, &payload);
        self.inner.emit(event, payload);
        if let Some(operation_id) = ended {
            let imports = Arc::clone(&self.imports);
            drop(self.runtime.spawn(async move { imports.settle(&operation_id).await }));
        }
    }

    fn emit_to(&self, window_label: &str, event: &str, payload: Value) {
        self.inner.emit_to(window_label, event, payload);
    }
}

/// The staging folder of `batch`.
fn staging_of(batch: u64) -> PathBuf {
    PathBuf::from(DROPS_ROOT).join(batch.to_string())
}

/// `path` as a plain relative path, or [`ImportError::BadPath`].
fn relative_path(path: &str) -> Result<PathBuf, ImportError> {
    // Split by hand: `Path::components` drops a `.` silently, and `Path::is_absolute`
    // is always `false` in the browser build.
    let plain = path
        .split('/')
        .all(|name| !name.is_empty() && name != "." && name != ".." && !name.contains('\0'));
    if plain {
        Ok(PathBuf::from(path))
    } else {
        Err(ImportError::BadPath { path: path.to_string() })
    }
}

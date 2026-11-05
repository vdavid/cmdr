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
//! Nothing here leaves the page: the bytes go from the browser's `File` into
//! the volume's memory, gone on reload.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cmdr_fs::volume::{InMemoryVolume, VolumeError};
use serde::{Deserialize, Serialize};

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

#[derive(Default)]
struct State {
    /// Bytes imported or reserved this visit.
    used: u64,
    /// Batches handed out so far.
    batches: u64,
    /// `(batch, index)` → the file still to land.
    pending: std::collections::HashMap<(u64, usize), Pending>,
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

    /// Accepts the longest prefix of `items` the budget covers, creates its
    /// folders in a fresh staging folder, and reserves its files' bytes.
    pub async fn plan(&self, items: &[ImportItem]) -> Result<ImportPlan, ImportError> {
        let relative: Vec<PathBuf> = items
            .iter()
            .map(|item| relative_path(&item.path))
            .collect::<Result<_, _>>()?;
        let (batch, accepted) = {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.batches += 1;
            let mut room = self.budget.saturating_sub(state.used);
            let mut accepted = 0;
            for item in items {
                let bytes = if item.kind == ImportKind::File { item.size } else { 0 };
                if bytes > room {
                    break;
                }
                room -= bytes;
                accepted += 1;
            }
            state.used = self.budget - room;
            (state.batches, accepted)
        };

        let staging = PathBuf::from(DROPS_ROOT).join(batch.to_string());
        match self.volume.import_folder(Path::new(DROPS_ROOT), None).await {
            Ok(()) | Err(VolumeError::AlreadyExists(_)) => {}
            Err(error) => return Err(self.unreserve(items, accepted, error)),
        }
        if let Err(error) = self.volume.import_folder(&staging, None).await {
            return Err(self.unreserve(items, accepted, error));
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
                        return Err(self.unreserve(items, accepted, error));
                    }
                }
                ImportKind::File => {
                    let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    state.pending.insert(
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
        let pending = {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.pending.remove(&(batch, index)).ok_or(ImportError::UnknownItem)?
        };
        if bytes.len() as u64 != pending.size {
            self.give_back(pending.size);
            return Err(ImportError::SizeChanged);
        }
        if let Err(error) = self
            .volume
            .import_file(&pending.path, bytes, pending.modified_secs)
            .await
        {
            self.give_back(pending.size);
            return Err(error.into());
        }
        Ok(())
    }

    /// Bytes imported or reserved this visit.
    pub fn used_bytes(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .used
    }

    fn give_back(&self, bytes: u64) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.used = state.used.saturating_sub(bytes);
    }

    /// Gives back the reservations of the first `count` of `items` (and drops
    /// any of them already pending), for a plan the volume cut short.
    fn unreserve(&self, items: &[ImportItem], count: usize, error: VolumeError) -> ImportError {
        let bytes: u64 = items
            .iter()
            .take(count)
            .filter(|item| item.kind == ImportKind::File)
            .map(|item| item.size)
            .sum();
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let batch = state.batches;
        state.pending.retain(|(b, _), _| *b != batch);
        state.used = state.used.saturating_sub(bytes);
        error.into()
    }
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

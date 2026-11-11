//! What a [`MultiRename`](crate::MultiRename) asks its host: only what differs
//! between the desktop app and the browser demo. How a file's media fields are
//! read, where the Results file goes, which place a rename lands in, how long a
//! volume gets, and which write engine runs it.
//!
//! The listing engine and the runtime aren't here: the host hands them to
//! [`MultiRename::builder`](crate::MultiRename::builder) once.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::rt::Runtime;
use cmdr_fs::volume::Volume;
use cmdr_operations::Operations;
use futures_util::future::BoxFuture;

use crate::error::{MultiRenameError, ReadOnlyPlace};
use crate::media::{Loader, ProgressEmit};
use crate::plan::PreviewRow;
use crate::started::MultiRenameStarted;

/// The host's answers to a Multi-Rename engine.
pub trait MultiRenameHost: Send + Sync {
    /// The loader that reads session files' media fields on the volume `volume_id`,
    /// reporting through `emit`. Its blocking reads run on `runtime` (the engine's).
    /// `emit` is the caller's, never the host's: a read with nobody watching passes
    /// a no-op.
    fn media_loader(&self, volume_id: &str, emit: ProgressEmit, runtime: &Runtime) -> Loader;

    /// Where session `session_id` writes its Results file; `None` when there's
    /// nowhere to, which `write_names` refuses with `CouldntWriteNames`.
    fn results_path(&self, session_id: &str) -> Option<PathBuf>;

    /// Whether this host can open Results at all (the sheet's `resultsAvailable`):
    /// a capability, never derived from [`results_path`](Self::results_path), so a
    /// desktop that momentarily has nowhere to write keeps its `CouldntWriteNames`
    /// wording instead of reading as a host without Results.
    fn results_supported(&self) -> bool;

    /// Where a rename of `ready` (rows of the folder `dir` on `volume_id`) lands,
    /// decided in ONE resolve: a read-only route (the folder's, else the first row
    /// whose old or new name a route serves), the local disk, a connected volume,
    /// or none. ❌ Don't split it into a check and a lookup: two resolves can
    /// disagree.
    fn rename_place<'a>(
        &'a self,
        volume_id: &'a str,
        dir: &'a Path,
        ready: &'a [PreviewRow],
    ) -> BoxFuture<'a, RenamePlace>;

    /// How long I/O on `volume_id` may take, given `base` for a local disk.
    fn io_budget(&self, volume_id: &str, base: Duration) -> Duration;

    /// The write engine apply starts renames on.
    fn operations(&self) -> Result<Operations, MultiRenameError>;

    /// One apply ended with `result`, from a spec using the rule kinds `rules`
    /// (fixed tokens like `name_mask` or `counter`, in the sheet's order; empty when
    /// the session was gone before apply could read it). Categories only: what the
    /// desktop counts for analytics. The browser demo counts nothing, hence the no-op.
    fn rename_applied(&self, rules: &[&'static str], result: &Result<MultiRenameStarted, MultiRenameError>) {
        let _ = (rules, result);
    }
}

/// Where a rename lands, as [`MultiRenameHost::rename_place`] answers.
pub enum RenamePlace {
    /// A route serves the folder or a row read-only.
    ReadOnly(ReadOnlyPlace),
    /// The local disk: fingerprints come straight from the file system.
    LocalDisk,
    /// A connected volume: fingerprints come through it.
    Volume(Arc<dyn Volume>),
    /// No volume answers for the folder.
    NotConnected,
}

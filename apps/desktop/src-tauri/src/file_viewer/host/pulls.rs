//! A bounded temp copy of a file the OS can't open (a routed path, or one on a phone or a
//! server), which the viewer then opens like a local file.
//!
//! The host owns the temp: where it lives, how it's pulled, and how it's removed. The
//! viewer only holds the [`PreviewTemp`] for as long as a session needs it.

use std::fmt;
use std::path::{Path, PathBuf};

use super::super::ViewerError;
use super::super::pending_open::PendingOpen;

/// What removes a temp copy when its last holder lets go. The host's, so the viewer never
/// deletes a file itself.
pub trait TempRemoval: Send + Sync + fmt::Debug {}

/// A viewer's temp copy of one file, shared by every session that previews it.
///
/// A view switch ("View as text", or back to the image) opens a second session for the
/// same window before the first one closes, so both hold this through an `Arc`, and the
/// copy goes when the LAST of them drops it. It remembers which file it copies, so a
/// switch reuses it only for the same path on the same volume.
#[derive(Debug)]
pub struct PreviewTemp {
    /// The copy on local disk, named with the source's basename so the viewer shows the
    /// right title and classifies media by the right extension.
    pub temp_file: PathBuf,
    source_path: PathBuf,
    volume_id: String,
    /// Removes the copy on drop.
    _removal: Box<dyn TempRemoval>,
}

impl PreviewTemp {
    /// The copy `temp_file` of `source_path` on `volume_id`, removed by `removal` once the
    /// last holder drops it.
    pub fn new(temp_file: PathBuf, source_path: PathBuf, volume_id: &str, removal: Box<dyn TempRemoval>) -> Self {
        Self {
            temp_file,
            source_path,
            volume_id: volume_id.to_string(),
            _removal: removal,
        }
    }

    /// Whether this is a copy of `source_path` on `volume_id`, still on disk.
    pub fn is_copy_of(&self, source_path: &Path, volume_id: &str) -> bool {
        self.source_path == source_path && self.volume_id == volume_id && self.temp_file.exists()
    }
}

/// Pulls a file the viewer can't open where it is.
pub trait ViewerPulls: Send + Sync {
    /// What the viewer opens for `requested`: a bounded temp copy when the OS can't open
    /// the path (a route serves it, or its volume's paths aren't OS-visible), else
    /// `Ok(None)` and the viewer opens `requested` directly.
    ///
    /// `open` watches the pull: the host records progress there and stops once `open` is
    /// abandoned. Blocking.
    fn pull_for_viewer(
        &self,
        requested: &Path,
        volume_id: &str,
        open: &PendingOpen,
    ) -> Result<Option<PreviewTemp>, ViewerError>;
}

/// Pulls nothing: every path opens where it is.
pub struct NoPulls;

impl ViewerPulls for NoPulls {
    fn pull_for_viewer(
        &self,
        _requested: &Path,
        _volume_id: &str,
        _open: &PendingOpen,
    ) -> Result<Option<PreviewTemp>, ViewerError> {
        Ok(None)
    }
}

//! A no-op `FileViewerBackend` for media sessions (Image / PDF).
//!
//! A media open still creates a `ViewerSession` so close/teardown and the token map
//! stay uniform with text sessions, but it must NOT build a text backend (no line
//! reading, no search). The session's `backend` field is non-optional
//! (`Arc<ArcSwap<Box<dyn FileViewerBackend>>>`), so rather than make it `Option` and
//! touch every `load_backend()` caller, a media session installs this no-op backend:
//! every text-shaped call returns an empty/zero result. The bytes are served out of
//! band through the `cmdr-media://` scheme, never through this backend.

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use super::{BackendCapabilities, FileViewerBackend, LineChunk, Matcher, SearchMatch, SeekTarget, ViewerError};

/// No-op backend for media (Image/PDF) sessions. Holds only the file name and size so
/// `ViewerOpenResult` can report them; it never reads file content.
pub struct MediaBackend {
    file_name: String,
    total_bytes: u64,
}

impl MediaBackend {
    pub fn new(file_name: String, total_bytes: u64) -> Self {
        Self { file_name, total_bytes }
    }
}

impl FileViewerBackend for MediaBackend {
    fn get_lines(&self, _target: &SeekTarget, _count: usize) -> Result<LineChunk, ViewerError> {
        // Media sessions have no text lines; the FE never calls `viewer_get_lines` in
        // media mode, but return an empty, internally-consistent chunk defensively.
        Ok(LineChunk {
            lines: Vec::new(),
            first_line_number: 0,
            byte_offset: 0,
            total_lines: Some(0),
            total_bytes: self.total_bytes,
        })
    }

    fn search(
        &self,
        _matcher: &Matcher,
        _cancel: &AtomicBool,
        _matches: &Mutex<Vec<SearchMatch>>,
        _progress: &Mutex<u64>,
    ) -> Result<u64, ViewerError> {
        Ok(0)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_line_seek: false,
            supports_byte_seek: false,
            supports_fraction_seek: false,
            knows_total_lines: true,
        }
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    fn total_lines(&self) -> Option<usize> {
        Some(0)
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

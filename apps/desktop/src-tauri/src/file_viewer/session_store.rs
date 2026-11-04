//! The viewer-session vocabulary and registry: the [`ViewerSession`] a window reads
//! through and what an open returns ([`ViewerOpenResult`]). The session table itself
//! is the `Viewer`'s (`viewer.rs`).
//!
//! Both open paths build on this: text opens in [`super::session`] and media opens in
//! [`super::media_session`]. Neither imports the other's vocabulary, so the open
//! dispatcher in `session.rs` can call the media path without a module cycle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use serde::Serialize;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::content_kind::ViewerContentKind;
use super::encoding::FileEncoding;
use super::host::pulls::PreviewTemp;
use super::log_view::ViewedBackend;
use super::markdown_document::MarkdownDocument;
use super::origin::SourceOrigin;
use super::row_text::RowView;
use super::source::OpenedFile;
use super::{BackendCapabilities, FileViewerBackend, LineChunk, SearchMatch, ViewerError};

/// Which backend strategy is active for a session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BackendType {
    FullLoad,
    ByteSeek,
    LineIndex,
}

/// Result returned when opening a viewer session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewerOpenResult {
    pub session_id: String,
    pub file_name: String,
    pub total_bytes: u64,
    pub total_lines: Option<usize>,
    /// The file's ROW count, exact or (on ByteSeek) from its bytes-per-row sample. The
    /// first chunk's `total_rows` carries the same number plus which of the two it is.
    pub estimated_total_rows: usize,
    pub backend_type: BackendType,
    pub capabilities: BackendCapabilities,
    pub initial_lines: LineChunk,
    /// ByteSeek -> LineIndex upgrade in progress.
    pub is_indexing: bool,
    /// Auto-detected encoding (also the initial selection of the picker).
    pub encoding: FileEncoding,
    /// Detected content kind. `Text` flows through the line pipeline (the fields
    /// above are populated); `Image` / `Pdf` render inline from `media_token` and
    /// `Markdown` from `viewer_render_markdown`, both leaving the text fields empty.
    pub kind: ViewerContentKind,
    /// Present only for media kinds (`Image` / `Pdf`): the unguessable token the FE
    /// puts in the `cmdr-media://localhost/<token>` URL. `None` for text.
    pub media_token: Option<String>,
    /// Image pixel dimensions, header-only and best-effort. `Some` only for some
    /// `Image` files (raster formats the `image` crate can parse; `None` for HEIC,
    /// SVG, PDFs, text, or on any read error).
    pub media_dimensions: Option<MediaDimensions>,
    /// The `cmdr-media://` request reads from a volume, so it can take a while and fail
    /// for reasons the file has nothing to do with: the window follows it through
    /// `viewer-media-load`. `false` for anything served from a local path.
    pub media_from_volume: bool,
}

/// Image pixel dimensions, read header-only at open time.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaDimensions {
    pub width: u32,
    pub height: u32,
}

/// Status of an ongoing search.
///
/// `InvalidQuery` carries the user-facing reason (invalid regex syntax, multiline
/// pattern, regex exceeds size limits). Surfaced via `search_poll`; the FE renders
/// the message as plain text without inspecting its contents (per the
/// no-error-string-match rule).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SearchStatus {
    Running,
    Done,
    Cancelled,
    Idle,
    InvalidQuery { message: String },
}

/// Internal state for an active search.
pub(super) struct SearchState {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) matches: Arc<Mutex<Vec<SearchMatch>>>,
    pub(super) bytes_scanned: Arc<Mutex<u64>>,
    pub(super) status: Arc<Mutex<SearchStatus>>,
}

/// A viewer session wraps a backend and tracks search state.
///
/// Constructed via [`ViewerSession::new`] from a [`ViewerSessionInit`]; the per-session
/// runtime fields (`search`, `rebuilding`, `pending_grew`, `tail_mode`, `active_reads`)
/// always start at their idle defaults, so callers only supply what actually varies
/// between a text open and a media open. The text-open path lives in `session.rs` and the
/// media-open path in `media_session.rs`; both build through this constructor.
pub(super) struct ViewerSession {
    /// `ArcSwap` so background threads (ByteSeek → LineIndex upgrade, encoding rebuild,
    /// tail-mode `extend_to`) can replace the backend without a write lock on the
    /// `get_lines` read path. Each backend is immutable; readers pick up either the
    /// old or new backend atomically. Document rationale: `file_viewer/CLAUDE.md`
    /// § "ArcSwap rather than RwLock."
    pub(super) backend: Arc<ArcSwap<Box<dyn FileViewerBackend>>>,
    pub(super) backend_type: Mutex<BackendType>,
    pub(super) search: Option<SearchState>,
    /// Set when upgrading from ByteSeek to LineIndex in the background.
    pub(super) upgrading: Mutex<Option<Arc<AtomicBool>>>,
    /// Set when an encoding-switch rebuild is in flight. Same `AtomicBool` pattern as
    /// `upgrading`: the rebuild thread reads it to see if it's been superseded by a
    /// rapid follow-up `set_encoding`.
    pub(super) rebuilding: Mutex<Option<Arc<AtomicBool>>>,
    /// Latest pending `Grew(eof)` from the (future) watcher manager. Both the upgrade
    /// thread and the encoding-rebuild thread drain this inside their swap critical
    /// section so a tail append arriving mid-rebuild isn't silently dropped. Documented
    /// protocol: drain-read → optional `extend_to` → `ArcSwap::store` → clear flag,
    /// all under one mutex lock.
    pub(super) pending_grew: Mutex<Option<u64>>,
    /// Current encoding. Updated atomically with the backend swap on `set_encoding`.
    pub(super) encoding: Mutex<FileEncoding>,
    /// Detected encoding at open time (sticky; never changes after `open_session`).
    pub(super) detected_encoding: FileEncoding,
    /// Tail mode flag: when true, `Grew` watcher events trigger a backend
    /// `extend_to` so the open viewport auto-follows newly appended bytes.
    /// When false, the FE still hears `viewer:file-changed:<sid>` events and
    /// renders the persistent reload toast.
    pub(super) tail_mode: AtomicBool,
    /// Cancel flag the manager thread reads on session close. Dropping the
    /// session sets this flag; the manager thread observes it on its next
    /// receive cycle, drops its owned `ViewerSubscription`, and exits. The
    /// subscription's `Drop` then unwatches the path via the shared singleton.
    pub(super) watcher_stop: Arc<AtomicBool>,
    /// Per-read cancel flags. Each `read_range` call inserts an entry keyed by the FE's
    /// `read_id`, removes it on completion (cancelled or not). The FE generates fresh
    /// ids per call (a monotonic counter is fine; uniqueness within a session is all
    /// that's needed). Per-read (not session-wide) so a cancel for one read doesn't
    /// affect a follow-up read that started in the same gesture.
    pub(super) active_reads: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    pub(super) path: PathBuf,
    /// Where the file lives, for a fresh handle on reload. See `file_viewer::source`.
    pub(super) origin: SourceOrigin,
    /// The handle every backend of this session reads through, opened once. `None`
    /// until first needed on a media session, which serves its bytes elsewhere.
    pub(super) file: Mutex<Option<OpenedFile>>,
    /// The `cmdr-media://` token for a media (Image/PDF) session, dropped from the
    /// global token map at `close_session`. `None` for text sessions. Tying the drop
    /// to the single close choke point keeps the token's lifetime exactly the
    /// session's, so a closed-window viewer can't leave a live token mapping a path.
    pub(super) media_token: Option<String>,
    /// For a session over a file the OS can't open (inside a zip, in a repo's virtual
    /// `.git` trees, on a phone or server), the temp copy it reads. Shared with a view
    /// switch's session for the same window, and removed when the last session holding
    /// it closes. `None` for a normal on-disk open. See `file_viewer::materialize`.
    pub(super) temp: Option<Arc<PreviewTemp>>,
    /// The rendered document of a Markdown session (its image tokens, the remote-images
    /// opt-in), closed at `close_session`. `None` for every other session.
    pub(super) markdown: Option<Arc<MarkdownDocument>>,
    /// How rows leave this session: as decoded, or as the debug log shows them.
    pub(super) rows: RowView,
}

/// The fields that vary between a text open and a media open. Everything else on a
/// [`ViewerSession`] starts at an idle default (no search, no rebuild, no pending
/// grew, tail off, no active reads), filled in by [`ViewerSession::new`].
pub(super) struct ViewerSessionInit {
    pub(super) backend: Box<dyn FileViewerBackend>,
    pub(super) backend_type: BackendType,
    pub(super) upgrading: Option<Arc<AtomicBool>>,
    pub(super) encoding: FileEncoding,
    pub(super) detected_encoding: FileEncoding,
    pub(super) watcher_stop: Arc<AtomicBool>,
    pub(super) path: PathBuf,
    pub(super) origin: SourceOrigin,
    pub(super) file: Option<OpenedFile>,
    pub(super) media_token: Option<String>,
    pub(super) temp: Option<Arc<PreviewTemp>>,
    pub(super) markdown: Option<Arc<MarkdownDocument>>,
    pub(super) rows: RowView,
}

impl ViewerSession {
    /// Builds a session from its varying fields, defaulting the idle runtime state.
    pub(super) fn new(init: ViewerSessionInit) -> Self {
        Self {
            backend: Arc::new(ArcSwap::from_pointee(init.backend)),
            backend_type: Mutex::new(init.backend_type),
            search: None,
            upgrading: Mutex::new(init.upgrading),
            rebuilding: Mutex::new(None),
            pending_grew: Mutex::new(None),
            encoding: Mutex::new(init.encoding),
            detected_encoding: init.detected_encoding,
            tail_mode: AtomicBool::new(false),
            watcher_stop: init.watcher_stop,
            active_reads: Mutex::new(HashMap::new()),
            path: init.path,
            origin: init.origin,
            file: Mutex::new(init.file),
            media_token: init.media_token,
            temp: init.temp,
            markdown: init.markdown,
            rows: init.rows,
        }
    }

    /// Loads a freshly cloned `Arc` to the current backend, seen through the session's row view.
    /// Holds no lock so calls to `get_lines` can run in parallel. The swap sites use `backend`
    /// itself: the view wraps on read, so a swapped-in backend keeps it.
    pub(super) fn load_backend(&self) -> Arc<Box<dyn FileViewerBackend>> {
        let current = self.backend.load_full();
        if self.rows.is_raw() {
            return current;
        }
        Arc::new(Box::new(ViewedBackend {
            inner: current,
            rows: self.rows,
        }))
    }

    /// The session's open file, opening it on first use.
    pub(super) fn open_file(&self) -> Result<OpenedFile, ViewerError> {
        let mut file = self.file.lock_ignore_poison();
        if let Some(opened) = file.as_ref() {
            return Ok(opened.clone());
        }
        let opened = self.origin.open()?;
        *file = Some(opened.clone());
        Ok(opened)
    }

    /// The open file at the size the live backend knows (tail mode moves it past the
    /// size seen at open), for building a sibling backend over the same bytes.
    pub(super) fn file_at_backend_size(&self) -> Result<OpenedFile, ViewerError> {
        let mut opened = self.open_file()?;
        opened.total_bytes = self.load_backend().total_bytes();
        Ok(opened)
    }
}

/// Generates a unique session ID.
pub(super) fn generate_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

//! The `VisionBackend` seam: the inference boundary the scheduler, store, and GC
//! sit behind, so all of that logic is testable with NO GPU/ANE/FFI.
//!
//! Two implementations sit behind the trait:
//! - [`vision::VisionOcrBackend`] (macOS): real OCR via `VNRecognizeTextRequest` over
//!   a downscaled in-memory ImageIO decode, on a dedicated 8 MB-stack OS thread inside
//!   `objc2::rc::autoreleasepool` — production selects it in `scheduler::start`.
//! - [`fake::FakeVisionBackend`]: deterministic, zero-FFI, injected by every test (and
//!   the production fallback off-macOS, where Vision doesn't exist).
//!
//! Nothing above the seam knows which backend it holds.
//!
//! ## What the seam computes
//!
//! [`analyze`](VisionBackend::analyze) is the enrichment entry point: it runs OCR,
//! scene/object classification (tags), and an image feature-print embedding over ONE
//! decode of the image (plan Decision 5 — decode once, reuse). OCR alone stays
//! available via [`ocr`](VisionBackend::ocr) for the focused macOS OCR tests. CLIP
//! embeddings and face detect/embed become sibling methods as those
//! milestones land.

pub mod fake;

/// The real macOS Vision OCR backend. Only compiled on macOS (Vision/ImageIO are
/// Apple frameworks); other platforms fall back to [`fake::FakeVisionBackend`] in the
/// scheduler.
#[cfg(target_os = "macos")]
pub mod vision;

use crate::media_index::predicate::MediaKind;

/// One image handed to the backend. `kind` rides along so a backend can
/// special-case a Live Photo still later.
///
/// `bytes` is the byte-source seam that lets the SAME backend serve local and
/// network volumes (plan Decision 6, network enrichment):
/// - `None` — the backend reads `path` itself via `std::fs::read` (the local case;
///   `path` is a real on-disk filesystem path).
/// - `Some(bytes)` — the caller ALREADY fetched the compressed image bytes (the
///   network case: the enrich layer reads them off the SMB mount under a timeout,
///   OFF the serialized OCR thread, so a hung mount can't wedge OCR). The backend
///   decodes from memory and never touches `path` for I/O. `path` then carries only
///   the image's index identity (for logging + the scripted-fake lookup).
#[derive(Debug, Clone)]
pub struct ImageInput {
    /// The image's path identity (a real on-disk path for a local volume; the
    /// index-relative identity for a network volume whose bytes are in `bytes`).
    pub path: String,
    /// The typed media kind the predicate assigned.
    pub kind: MediaKind,
    /// Pre-fetched compressed image bytes, or `None` to have the backend read
    /// `path` itself. See the type docs.
    pub bytes: Option<Vec<u8>>,
}

/// The OCR result for one image. Deliberately minimal for now (the recognized text);
/// bounding boxes / per-line confidence can join later without touching callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrResult {
    /// The recognized text, newline-joined across recognized regions.
    pub text: String,
}

/// One scene/object tag Vision's `VNClassifyImageRequest` assigned to an image: a
/// taxonomy label (`"beach"`, `"dog"`) and its confidence in `0.0..=1.0`. Crosses
/// the IPC boundary (tag surfacing is a later tags UI), so it derives `Serialize`
/// + `specta::Type`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    /// The taxonomy label (a Vision classification identifier).
    pub label: String,
    /// The classifier's confidence in `[0.0, 1.0]`.
    pub score: f32,
}

/// The full enrichment analysis of ONE image, computed from a single decode (plan
/// Decision 5). OCR text (possibly empty — an image with no text), the scene/object
/// tags (possibly empty), and the image feature-print embedding (`None` if the
/// feature-print request produced nothing).
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// The recognized OCR text.
    pub ocr: OcrResult,
    /// The scene/object tags, already thresholded/capped by the backend.
    pub tags: Vec<Tag>,
    /// The image feature-print embedding (image↔image similarity), or `None` when
    /// the feature-print request yielded no observation.
    pub embedding: Option<Vec<f32>>,
}

/// A typed backend failure. Never string-matched for classification
/// (`no-string-matching`): a caller branches on the variant, not the message.
#[derive(Debug, Clone)]
pub enum VisionError {
    /// The image couldn't be decoded (a broken or unsupported file).
    Decode(String),
    /// The OCR request itself failed.
    Ocr(String),
}

impl std::fmt::Display for VisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VisionError::Decode(m) => write!(f, "image decode failed: {m}"),
            VisionError::Ocr(m) => write!(f, "vision OCR failed: {m}"),
        }
    }
}

impl std::error::Error for VisionError {}

/// The inference boundary. `Send + Sync` so an `Arc<dyn VisionBackend>` can be held
/// by the scheduler and used across its dedicated OS threads.
pub trait VisionBackend: Send + Sync {
    /// A stable stamp for the OS/Vision OCR engine this backend runs. A component of
    /// [`analysis_stamp`](VisionBackend::analysis_stamp); also the shape the focused
    /// OCR tests assert.
    fn engine_version(&self) -> String;

    /// A stable stamp for the Vision scene/object TAG taxonomy (the classifier
    /// revision). When macOS ships a new tag taxonomy this bumps, so stored tags go
    /// stale and re-tag via [`analysis_stamp`](VisionBackend::analysis_stamp) — the
    /// tag-taxonomy-version component of the provenance key (plan Decision 4).
    fn taxonomy_version(&self) -> String;

    /// The COMBINED provenance/staleness stamp the scheduler persists on each
    /// `media_status` row (in the `engine_version` column): the OCR engine, the tag
    /// taxonomy, and the feature-print revision folded together. When ANY component
    /// changes (an OS upgrade bumps the OCR engine, the tag taxonomy, or the
    /// feature-print model) a stored row goes stale and re-runs [`analyze`], even
    /// though `(path, mtime, size)` is unchanged (data-COVERAGE; the derived data is
    /// disposable). One decode produces all three outputs, so re-running the whole
    /// analysis on any component change costs nothing extra. Default folds the two
    /// stamps above; a backend with a distinct feature-print revision overrides it.
    fn analysis_stamp(&self) -> String {
        format!("{};tax={}", self.engine_version(), self.taxonomy_version())
    }

    /// Run OCR over one image. The real backend decodes (from `input.bytes` when
    /// present, else by reading `input.path`) and runs `VNRecognizeTextRequest`; the
    /// fake returns scripted text keyed on `input.path`.
    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError>;

    /// Run the full enrichment analysis over one image from a SINGLE decode: OCR,
    /// scene/object classification (tags), and the image feature-print embedding. The
    /// real backend decodes once and runs all three Vision requests on the one
    /// `CGImage`; the fake returns deterministic scripted results keyed on
    /// `input.path`. A hostile/undecodable image fails closed to a typed
    /// [`VisionError`], never a panic (as [`ocr`](VisionBackend::ocr) does).
    fn analyze(&self, input: &ImageInput) -> Result<Analysis, VisionError>;
}

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
//! ## Room to grow
//!
//! M1 needs OCR only. Tags (`VNClassifyImageRequest`, M2), image feature prints
//! (M2), CLIP embeddings (M3), and face detect/embed (M4) become sibling methods on
//! this trait as those milestones land — each returning its own typed result, each
//! fakeable the same way. Keeping [`ocr`](VisionBackend::ocr) the sole method now
//! keeps the seam honest to what M1 actually exercises.

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
/// network volumes (plan Decision 6, M1.5):
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

/// The OCR result for one image. Deliberately minimal in M1 (the recognized text);
/// bounding boxes / per-line confidence can join later without touching callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrResult {
    /// The recognized text, newline-joined across recognized regions.
    pub text: String,
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
    /// A stable stamp for the OS/Vision OCR engine this backend runs. The
    /// scheduler persists it on each `media_status` row: when it changes (an OS
    /// upgrade bumps the Vision OCR engine), stored OCR goes stale and re-runs even
    /// though `(path, mtime, size)` is unchanged (data-COVERAGE, not data-safety —
    /// OCR text is disposable). See [`crate::media_index`] DETAILS.
    fn engine_version(&self) -> String;

    /// Run OCR over one image. The real backend decodes (from `input.bytes` when
    /// present, else by reading `input.path`) and runs `VNRecognizeTextRequest`; the
    /// fake returns scripted text keyed on `input.path`.
    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError>;
}

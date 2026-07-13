//! The `VisionBackend` seam: the inference boundary the scheduler, store, and GC
//! sit behind, so all of that logic is testable with NO GPU/ANE/FFI.
//!
//! M1 defines the trait and ships ONLY a deterministic [`fake::FakeVisionBackend`].
//! The real `objc2-vision` OCR implementation is the NEXT slice; it implements this
//! same trait (decoding `ImageInput::path` on a dedicated OS thread inside an
//! `objc2::rc::autoreleasepool`, with a per-block `// SAFETY:` — `src-tauri/CLAUDE.md`),
//! and nothing above the seam changes.
//!
//! ## Room to grow
//!
//! M1 needs OCR only. Tags (`VNClassifyImageRequest`, M2), image feature prints
//! (M2), CLIP embeddings (M3), and face detect/embed (M4) become sibling methods on
//! this trait as those milestones land — each returning its own typed result, each
//! fakeable the same way. Keeping [`ocr`](VisionBackend::ocr) the sole method now
//! keeps the seam honest to what M1 actually exercises.

pub mod fake;

use crate::media_index::predicate::MediaKind;

/// One image handed to the backend. In M1 the fake reads only `path`; the real
/// backend decodes the file at `path` (downscaled, in-memory — Decision 5) and
/// never a thumbnail file. `kind` rides along so a backend can special-case a Live
/// Photo still later.
#[derive(Debug, Clone)]
pub struct ImageInput {
    /// The image's absolute path (the index's real identity).
    pub path: String,
    /// The typed media kind the predicate assigned.
    pub kind: MediaKind,
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

    /// Run OCR over one image. The real backend decodes and runs
    /// `VNRecognizeTextRequest`; the fake returns scripted text.
    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError>;
}

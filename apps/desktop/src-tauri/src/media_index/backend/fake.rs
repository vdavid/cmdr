//! A deterministic, zero-FFI [`VisionBackend`] for tests and for the M1 pipeline
//! before the real `objc2-vision` backend lands.
//!
//! It never touches the ANE, the filesystem, or `objc2`: OCR text is either
//! scripted per path or derived deterministically from the file's stem, so a test
//! can assert exactly what lands in `media_ocr`. This is the fake seam the plan
//! calls for so the scheduler, store, and GC are fully testable without hardware.

use std::collections::{HashMap, HashSet};

use super::{ImageInput, OcrResult, VisionBackend, VisionError};

/// A scriptable fake OCR backend.
///
/// - By default, `ocr` returns text derived from the image's file stem (stable and
///   deterministic), so a test that doesn't care about exact text still gets
///   predictable, searchable words.
/// - [`with_text`](FakeVisionBackend::with_text) scripts an exact result for a path.
/// - [`failing_for`](FakeVisionBackend::failing_for) scripts a decode failure for a
///   path (to exercise the scheduler's failure branch).
#[derive(Debug, Clone, Default)]
pub struct FakeVisionBackend {
    scripted: HashMap<String, String>,
    failing: HashSet<String>,
    engine_version: Option<String>,
}

impl FakeVisionBackend {
    /// A fake with a fixed engine stamp and no scripting.
    pub fn new() -> Self {
        Self::default()
    }

    /// Script the exact OCR text for a path.
    pub fn with_text(mut self, path: impl Into<String>, text: impl Into<String>) -> Self {
        self.scripted.insert(path.into(), text.into());
        self
    }

    /// Script a decode failure for a path.
    pub fn failing_for(mut self, path: impl Into<String>) -> Self {
        self.failing.insert(path.into());
        self
    }

    /// Override the engine-version stamp (to simulate an OS/Vision engine change).
    pub fn with_engine_version(mut self, version: impl Into<String>) -> Self {
        self.engine_version = Some(version.into());
        self
    }
}

impl VisionBackend for FakeVisionBackend {
    fn engine_version(&self) -> String {
        self.engine_version
            .clone()
            .unwrap_or_else(|| "fake-vision-1".to_string())
    }

    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError> {
        if self.failing.contains(&input.path) {
            return Err(VisionError::Decode(input.path.clone()));
        }
        if let Some(text) = self.scripted.get(&input.path) {
            return Ok(OcrResult { text: text.clone() });
        }
        // Deterministic default: the file stem, so unscripted images still produce
        // stable, searchable text.
        let stem = std::path::Path::new(&input.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| input.path.clone());
        Ok(OcrResult {
            text: format!("ocr text for {stem}"),
        })
    }
}

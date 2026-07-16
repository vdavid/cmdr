//! A deterministic, zero-FFI [`VisionBackend`] for tests and for the pipeline on
//! platforms without Vision.
//!
//! It never touches the ANE, the filesystem, or `objc2`: every output (OCR text,
//! tags, feature-print embedding) is either scripted per path or derived
//! deterministically from the file's stem, so a test can assert exactly what lands in
//! `media.db`. This is the fake seam the plan calls for so the scheduler, store,
//! vector search, and GC are fully testable without hardware.

use std::collections::{HashMap, HashSet};

use super::{Analysis, ImageInput, MediaAnalysis, OcrResult, Tag, VisionBackend, VisionError};
use crate::media_index::clip::backend::fake_clip_embedding;

/// The dimensionality of the fake feature-print embedding. Small and fixed so cosine/
/// top-k tests stay legible; the real backend's is far larger (a Vision feature print
/// is ~2048 floats), but nothing above the seam depends on the length.
pub const FAKE_EMBEDDING_DIMS: usize = 8;

/// A scriptable fake vision backend.
///
/// - By default, `ocr`/`analyze` derive stable text, one tag, and a deterministic
///   embedding from the image's file stem, so an unscripted image still gets
///   predictable, searchable words and a reproducible vector.
/// - [`with_text`](FakeVisionBackend::with_text) / [`with_tags`] /
///   [`with_embedding`] script an exact result for a path (precise cosine/tag tests).
/// - [`failing_for`](FakeVisionBackend::failing_for) scripts a decode failure for a
///   path (to exercise the scheduler's failure branch).
#[derive(Debug, Clone, Default)]
pub struct FakeVisionBackend {
    scripted: HashMap<String, String>,
    scripted_tags: HashMap<String, Vec<Tag>>,
    scripted_embedding: HashMap<String, Vec<f32>>,
    scripted_clip: HashMap<String, Vec<f32>>,
    failing: HashSet<String>,
    missing: HashSet<String>,
    engine_version: Option<String>,
    taxonomy_version: Option<String>,
}

impl FakeVisionBackend {
    /// A fake with fixed stamps and no scripting.
    pub fn new() -> Self {
        Self::default()
    }

    /// Script the exact OCR text for a path.
    pub fn with_text(mut self, path: impl Into<String>, text: impl Into<String>) -> Self {
        self.scripted.insert(path.into(), text.into());
        self
    }

    /// Script the exact tags for a path.
    pub fn with_tags(mut self, path: impl Into<String>, tags: Vec<Tag>) -> Self {
        self.scripted_tags.insert(path.into(), tags);
        self
    }

    /// Script the exact feature-print embedding for a path (precise cosine tests).
    pub fn with_embedding(mut self, path: impl Into<String>, embedding: Vec<f32>) -> Self {
        self.scripted_embedding.insert(path.into(), embedding);
        self
    }

    /// Script the exact CLIP image embedding for a path (precise semantic-search tests).
    /// Unscripted paths get [`fake_clip_embedding`] over the path (the shared bag-of-words
    /// space the fake text encoder also projects onto, so text→image alignment holds).
    pub fn with_clip_embedding(mut self, path: impl Into<String>, embedding: Vec<f32>) -> Self {
        self.scripted_clip.insert(path.into(), embedding);
        self
    }

    /// Script a decode failure for a path.
    pub fn failing_for(mut self, path: impl Into<String>) -> Self {
        self.failing.insert(path.into());
        self
    }

    /// Script a vanished-source failure for a path (an ENOENT-class read failure): the
    /// backend returns [`VisionError::Missing`], as the real backend does when a file
    /// disappears between the index walk and its analyze.
    pub fn missing_for(mut self, path: impl Into<String>) -> Self {
        self.missing.insert(path.into());
        self
    }

    /// Override the OCR engine-version stamp (to simulate an OS/Vision engine change).
    pub fn with_engine_version(mut self, version: impl Into<String>) -> Self {
        self.engine_version = Some(version.into());
        self
    }

    /// Override the tag-taxonomy-version stamp (to simulate an OS taxonomy change,
    /// which must re-tag stale rows — plan Decision 4).
    pub fn with_taxonomy_version(mut self, version: impl Into<String>) -> Self {
        self.taxonomy_version = Some(version.into());
        self
    }

    /// The stem-derived default OCR text for an unscripted path.
    fn default_text(path: &str) -> String {
        format!("ocr text for {}", stem(path))
    }

    /// The stem-derived default tag for an unscripted path (a single mid-confidence
    /// tag so unscripted images are still tag-searchable and deterministic).
    fn default_tags(path: &str) -> Vec<Tag> {
        vec![Tag {
            label: stem(path),
            score: 0.5,
        }]
    }

    /// A deterministic unit-length embedding derived from the path stem, so two
    /// images with the same stem get identical vectors (a predictable near-duplicate
    /// for dedup tests) and different stems get different directions.
    fn default_embedding(path: &str) -> Vec<f32> {
        let stem = stem(path);
        let mut v = vec![0f32; FAKE_EMBEDDING_DIMS];
        for (i, byte) in stem.bytes().enumerate() {
            v[i % FAKE_EMBEDDING_DIMS] += f32::from(byte);
        }
        // A stem of only NULs (never happens for a real path) would be all-zero;
        // nudge the first component so the vector is always non-degenerate.
        if v.iter().all(|c| *c == 0.0) {
            v[0] = 1.0;
        }
        v
    }
}

impl VisionBackend for FakeVisionBackend {
    fn engine_version(&self) -> String {
        self.engine_version
            .clone()
            .unwrap_or_else(|| "fake-vision-1".to_string())
    }

    fn taxonomy_version(&self) -> String {
        self.taxonomy_version
            .clone()
            .unwrap_or_else(|| "fake-tax-1".to_string())
    }

    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError> {
        if self.missing.contains(&input.path) {
            return Err(VisionError::Missing(input.path.clone()));
        }
        if self.failing.contains(&input.path) {
            return Err(VisionError::Decode(input.path.clone()));
        }
        Ok(OcrResult {
            text: self
                .scripted
                .get(&input.path)
                .cloned()
                .unwrap_or_else(|| Self::default_text(&input.path)),
        })
    }

    fn analyze(&self, input: &ImageInput) -> Result<Analysis, VisionError> {
        if self.missing.contains(&input.path) {
            return Err(VisionError::Missing(input.path.clone()));
        }
        if self.failing.contains(&input.path) {
            return Err(VisionError::Decode(input.path.clone()));
        }
        let ocr = OcrResult {
            text: self
                .scripted
                .get(&input.path)
                .cloned()
                .unwrap_or_else(|| Self::default_text(&input.path)),
        };
        let tags = self
            .scripted_tags
            .get(&input.path)
            .cloned()
            .unwrap_or_else(|| Self::default_tags(&input.path));
        let embedding = Some(
            self.scripted_embedding
                .get(&input.path)
                .cloned()
                .unwrap_or_else(|| Self::default_embedding(&input.path)),
        );
        Ok(Analysis { ocr, tags, embedding })
    }

    fn analyze_media(
        &self,
        input: &ImageInput,
        want_vision: bool,
        want_clip: bool,
    ) -> Result<MediaAnalysis, VisionError> {
        // One decode fails both sides, matching the real backend (a bad file can't be
        // decoded for Vision OR CLIP).
        if self.missing.contains(&input.path) {
            return Err(VisionError::Missing(input.path.clone()));
        }
        if self.failing.contains(&input.path) {
            return Err(VisionError::Decode(input.path.clone()));
        }
        let vision = if want_vision { Some(self.analyze(input)?) } else { None };
        let clip = want_clip.then(|| {
            self.scripted_clip
                .get(&input.path)
                .cloned()
                .unwrap_or_else(|| fake_clip_embedding(&input.path))
        });
        Ok(MediaAnalysis { vision, clip })
    }
}

/// The file stem of a path (no directory, no extension), lossily. Falls back to the
/// whole path when there's no stem.
fn stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::MediaKind;
    use super::*;

    fn input(path: &str) -> ImageInput {
        ImageInput {
            path: path.to_string(),
            kind: MediaKind::Image,
            bytes: None,
        }
    }

    #[test]
    fn analyze_is_deterministic_and_returns_all_three_outputs() {
        let backend = FakeVisionBackend::new();
        let a = backend.analyze(&input("/photos/beach.jpg")).expect("analyze");
        let b = backend.analyze(&input("/photos/beach.jpg")).expect("analyze");
        assert_eq!(a, b, "same path ⇒ identical analysis");
        assert!(!a.ocr.text.is_empty());
        assert!(!a.tags.is_empty());
        let emb = a.embedding.expect("embedding present");
        assert_eq!(emb.len(), FAKE_EMBEDDING_DIMS);
    }

    #[test]
    fn same_stem_yields_the_same_embedding_a_predictable_near_duplicate() {
        let backend = FakeVisionBackend::new();
        let a = backend.analyze(&input("/one/beach.jpg")).expect("a").embedding.unwrap();
        let b = backend.analyze(&input("/two/beach.jpg")).expect("b").embedding.unwrap();
        let c = backend
            .analyze(&input("/two/mountain.jpg"))
            .expect("c")
            .embedding
            .unwrap();
        assert_eq!(a, b, "same stem ⇒ same vector (a deterministic duplicate)");
        assert_ne!(a, c, "different stem ⇒ different vector");
    }

    #[test]
    fn scripted_tags_and_embedding_win_over_the_defaults() {
        let backend = FakeVisionBackend::new()
            .with_tags(
                "/x.jpg",
                vec![Tag {
                    label: "sunset".to_string(),
                    score: 0.7,
                }],
            )
            .with_embedding("/x.jpg", vec![9.0, 8.0]);
        let a = backend.analyze(&input("/x.jpg")).expect("analyze");
        assert_eq!(
            a.tags,
            vec![Tag {
                label: "sunset".to_string(),
                score: 0.7
            }]
        );
        assert_eq!(a.embedding, Some(vec![9.0, 8.0]));
    }

    #[test]
    fn a_failing_path_errors_from_analyze_too() {
        let backend = FakeVisionBackend::new().failing_for("/bad.jpg");
        assert!(backend.analyze(&input("/bad.jpg")).is_err());
    }
}

//! The CLIP encoder seams (query-time text encoding + the deterministic fakes).
//!
//! Image encoding is folded into the Vision backend's
//! [`analyze_media`](crate::media_index::backend::VisionBackend::analyze_media) — one
//! decode feeds both Vision and CLIP on the same worker thread. Text encoding is a
//! query-time concern behind [`ClipTextEncoder`], kept warm by the command layer.
//!
//! Both fakes ([`FakeClipTextEncoder`] and the fake image path via
//! [`fake_clip_embedding`]) project into ONE shared bag-of-words space, so a fake
//! text→image search actually aligns (a query and an image that mention the same word
//! get a high cosine) — that's what makes the query pipeline testable end-to-end without
//! a model or the ANE. The real macOS towers live in the `vision`-gated modules.

use super::ClipError;

/// Encode a text query into a CLIP text embedding (query time). The real impl keeps the
/// text tower warm (a cold Core ML load is 1–2 s; a warm encode ~2 ms); the fake returns
/// a deterministic vector in the shared fake space.
pub trait ClipTextEncoder: Send + Sync {
    /// Encode `query` to a CLIP text embedding, or a typed error when the model isn't
    /// available / a prediction fails.
    fn encode_text(&self, query: &str) -> Result<Vec<f32>, ClipError>;
}

/// The small fixed vocabulary the fake shared space projects onto. A `fake_clip_embedding`
/// is a bag-of-words indicator over these terms, so a query "a cat on the beach" and an
/// image path "/trip/beach/cat.jpg" land close in cosine.
const FAKE_VOCAB: &[&str] = &[
    "cat", "dog", "beach", "sunset", "car", "street", "game", "screenshot", "person", "food",
    "mountain", "flower", "water", "building", "tree", "sky",
];

/// A deterministic CLIP-space embedding for the fake backends: a bag-of-words indicator
/// over [`FAKE_VOCAB`]. Used by BOTH the fake text encoder (over a query) and the fake
/// image backend (over an image path), so text→image alignment holds in tests. A string
/// mentioning no vocab word yields the zero vector (cosine treats it as unranked), so
/// tests use vocab words.
pub fn fake_clip_embedding(text: &str) -> Vec<f32> {
    let lower = text.to_lowercase();
    FAKE_VOCAB
        .iter()
        .map(|w| if lower.contains(w) { 1.0 } else { 0.0 })
        .collect()
}

/// A deterministic fake text encoder over the shared [`fake_clip_embedding`] space.
#[derive(Debug, Clone, Default)]
pub struct FakeClipTextEncoder;

impl ClipTextEncoder for FakeClipTextEncoder {
    fn encode_text(&self, query: &str) -> Result<Vec<f32>, ClipError> {
        Ok(fake_clip_embedding(query))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_index::vector::cosine;

    #[test]
    fn fake_text_and_image_align_on_shared_words() {
        let query = fake_clip_embedding("a photo of a cat on the beach");
        let matching_image = fake_clip_embedding("/trips/beach/cat.jpg");
        let unrelated_image = fake_clip_embedding("/office/screenshot.png");
        assert!(
            cosine(&query, &matching_image) > cosine(&query, &unrelated_image),
            "the beach+cat image outranks an unrelated one for a beach+cat query"
        );
    }

    #[test]
    fn the_fake_encoder_is_deterministic() {
        let e = FakeClipTextEncoder;
        assert_eq!(e.encode_text("beach").unwrap(), e.encode_text("beach").unwrap());
    }
}

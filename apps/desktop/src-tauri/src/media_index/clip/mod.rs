//! CLIP natural-language semantic image search (plan M3).
//!
//! CLIP maps images and text into ONE shared 512-d vector space, so a typed query
//! ("beach sunset") is encoded to a vector and cosine-matched against the stored image
//! embeddings. Two on-device Core ML towers (downloaded on demand, checksum-verified):
//!
//! - the **image tower** embeds every enriched photo (on the same dedicated Vision
//!   worker thread, from the same decode — see [`backend`]);
//! - the **text tower** encodes a search query at query time (kept warm — a cold load is
//!   1–2 s, a warm encode ~2 ms).
//!
//! CLIP's vector space is DIFFERENT from the Vision feature print, so its embeddings live
//! in a separate `media_clip_embedding` table with independent `clip_stamp` staleness
//! ([`store::needs_clip`](super::store::needs_clip)): installing/upgrading CLIP re-embeds
//! CLIP without re-running OCR/tags, and vice versa.
//!
//! ## Module map
//!
//! - [`tokenizer`] — the CLIP byte-pair tokenizer (ctx 77), query text → int32 token ids.
//! - [`backend`] — the encoder seams: [`backend::ClipTextEncoder`] (query time) and the
//!   image encoding folded into the Vision backend's combined `analyze_media`, each with a
//!   deterministic fake so the pipeline is testable with no model/FFI.
//! - [`install`] — on-demand model download + SHA-256 verify + zip unpack + the
//!   install/loaded gate (distinct from the GGUF two-flag gate — plan Decision 9).
//!
//! The conversion that produces the shipped `.mlpackage` towers is an out-of-tree dev
//! script (`apps/desktop/scripts/convert-clip-model/`), never run by CI/pnpm.

pub mod backend;
pub mod install;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod tokenizer;

use std::path::Path;

/// Record where the CLIP towers install (the app data dir), so the query-time text tower
/// and the enrichment image tower can load them. Called once at scheduler start; a no-op
/// off macOS (CLIP runs only on macOS).
pub fn set_data_dir(data_dir: &Path) {
    #[cfg(target_os = "macos")]
    macos::set_data_dir(data_dir);
    #[cfg(not(target_os = "macos"))]
    let _ = data_dir;
}

/// The currently-installed CLIP model's provenance stamp, or `None` when no model is
/// installed OR semantic search is turned off. The scheduler reads this once per pass and
/// threads it into the enrich core's [`needs_clip`](super::store::needs_clip) decision
/// (plan M3 two-part staleness); `None` makes `needs_clip` always false, so this is the
/// ONE seam that stops every pass type (full / network / live) from embedding CLIP when
/// the user turns semantic search off.
pub fn current_stamp(data_dir: &Path) -> Option<String> {
    if !super::gate::semantic_search_enabled() {
        return None;
    }
    install::installed_stamp(data_dir)
}

/// Tokenize `query` and encode it to a CLIP text embedding via the warm text tower — the
/// query-time entry point (plan M3 Q6). `Err(ClipError::NotAvailable)` when no model is
/// installed / off macOS, so the command returns no hits rather than erroring.
pub fn encode_text_query(query: &str) -> Result<Vec<f32>, ClipError> {
    let ids = tokenizer::tokenize(query);
    #[cfg(target_os = "macos")]
    {
        macos::encode_text(ids)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ids;
        Err(ClipError::NotAvailable)
    }
}

/// Encode a CHW `[0,1]` `[1,3,224,224]` pixel buffer to a CLIP image embedding via the
/// enrichment image tower (called from the Vision worker's `analyze_media`).
pub fn encode_image_pixels(pixels: Vec<f32>) -> Result<Vec<f32>, ClipError> {
    #[cfg(target_os = "macos")]
    {
        macos::encode_image(pixels)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pixels;
        Err(ClipError::NotAvailable)
    }
}

/// A typed CLIP failure. Never string-matched for classification (`no-string-matching`):
/// a caller branches on the variant, not the message.
#[derive(Debug, Clone)]
pub enum ClipError {
    /// The CLIP model isn't installed (or failed to load), so no encoding is possible.
    NotAvailable,
    /// The Core ML model load/compile failed.
    Load(String),
    /// A prediction (text or image encode) failed.
    Predict(String),
    /// The image couldn't be decoded for image encoding (a broken/unsupported file).
    Decode(String),
}

impl std::fmt::Display for ClipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipError::NotAvailable => write!(f, "CLIP model is not available"),
            ClipError::Load(m) => write!(f, "CLIP model load failed: {m}"),
            ClipError::Predict(m) => write!(f, "CLIP prediction failed: {m}"),
            ClipError::Decode(m) => write!(f, "CLIP image decode failed: {m}"),
        }
    }
}

impl std::error::Error for ClipError {}

//! The image-qualification predicate: decide, per index entry, whether it is an
//! image `media_index` should enrich — and if not, WHY it's skipped.
//!
//! Pure over a directory's file names (no I/O, no index), so it's directly
//! unit-testable. The scheduler groups the walked index by parent directory and
//! runs [`qualify_dir`] per group, because two of the decisions are sibling-aware:
//!
//! - **Live Photos** are a still image plus a paired motion `.mov` sharing the
//!   stem. The still is enriched (tagged [`MediaKind::LivePhotoStill`] so the kind
//!   is recorded); the motion component is a video and skipped.
//! - **RAW+JPEG pairs** (a `.cr2`/`.dng`/… beside a same-stem `.jpg`) enrich the
//!   JPEG (the cheaper, Vision-friendly decode) and skip the redundant RAW. A LONE
//!   RAW (no JPEG sibling) is enriched as an image.
//! - **`.aae` edit sidecars** are skipped outright (they hold no pixels).
//! - **Videos are out of scope (images only)** and skipped (this also covers a Live Photo's
//!   motion `.mov`, whether or not it pairs a still).
//!
//! Classification is TYPED, never a message/substring branch (`no-string-matching`):
//! the decision is a [`Qualification`] enum carrying a [`MediaKind`] or a typed
//! [`SkipReason`].

use std::collections::HashMap;

/// What kind of media an ENRICHED entry is. Recorded on the `media_status` row so a
/// later milestone can treat the two differently; both are OCR-enriched today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// A plain still image.
    Image,
    /// The still half of a Live Photo (a same-stem `.mov` sits beside it).
    LivePhotoStill,
}

impl MediaKind {
    /// The stable token persisted in `media_status.media_kind`.
    pub fn as_token(self) -> &'static str {
        match self {
            MediaKind::Image => "image",
            MediaKind::LivePhotoStill => "livePhotoStill",
        }
    }

    /// Parse a persisted token back to the typed kind. An unknown token (a row
    /// written by a newer build) reads as a plain [`MediaKind::Image`] rather than
    /// erroring — the cache is disposable and the distinction isn't load-bearing.
    pub fn from_token(token: &str) -> MediaKind {
        match token {
            "livePhotoStill" => MediaKind::LivePhotoStill,
            _ => MediaKind::Image,
        }
    }
}

/// Why a file is NOT enriched. Typed so a caller (or a test) branches on the reason
/// without inspecting a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Not a media file at all (a document, an archive, an extension-less file).
    NotMedia,
    /// A video (out of scope, images only; also a Live Photo's motion component).
    Video,
    /// An `.aae` edit sidecar (no pixels to enrich).
    Sidecar,
    /// A RAW file with a same-stem JPEG sibling — the JPEG is enriched instead.
    RawWithJpegSibling,
}

/// The per-file decision: enrich it (as a typed kind) or skip it (for a typed
/// reason).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qualification {
    /// Enrich this file as the given kind.
    Enrich(MediaKind),
    /// Skip this file for the given reason.
    Skip(SkipReason),
}

/// Still-image extensions Vision OCR can decode (lowercased, no dot).
const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "gif", "tiff", "tif", "webp", "bmp",
];

/// JPEG extensions specifically — the "primary" a RAW+JPEG pair enriches.
const JPEG_EXTS: &[&str] = &["jpg", "jpeg"];

/// Camera RAW extensions. A lone RAW is enriched; a RAW beside a same-stem JPEG is
/// skipped in favor of the JPEG.
const RAW_EXTS: &[&str] = &["cr2", "cr3", "nef", "arw", "dng", "raf", "orf", "rw2", "srw", "pef"];

/// Video extensions — out of scope, images only (this also skips a Live Photo's motion
/// `.mov`).
const VIDEO_EXTS: &[&str] = &["mov", "mp4", "m4v", "avi", "mkv", "webm", "hevc"];

/// The lowercased extension of a file name, or `None` when it has none.
fn ext_of(name: &str) -> Option<String> {
    std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
}

/// The lowercased stem (name without its final extension) used to pair siblings.
/// A dotfile with no real extension keeps its whole name as the stem.
fn stem_of(name: &str) -> String {
    match std::path::Path::new(name).file_stem() {
        Some(s) => s.to_string_lossy().to_lowercase(),
        None => name.to_lowercase(),
    }
}

/// Qualify every file in one directory, sibling-aware. Returns one [`Qualification`]
/// per input name, in the same order.
///
/// The sibling logic keys on the lowercased stem: for each stem we note whether it
/// has a JPEG member (so a RAW peer skips) and whether it has a video member (so an
/// image peer is a Live Photo still). Pure string math — no I/O, no index.
pub fn qualify_dir(names: &[&str]) -> Vec<Qualification> {
    // Per-stem sibling summary, built in one pass.
    #[derive(Default)]
    struct StemInfo {
        has_jpeg: bool,
        has_video: bool,
    }
    let mut by_stem: HashMap<String, StemInfo> = HashMap::new();
    for name in names {
        let Some(ext) = ext_of(name) else { continue };
        let info = by_stem.entry(stem_of(name)).or_default();
        if JPEG_EXTS.contains(&ext.as_str()) {
            info.has_jpeg = true;
        }
        if VIDEO_EXTS.contains(&ext.as_str()) {
            info.has_video = true;
        }
    }

    names
        .iter()
        .map(|name| {
            let Some(ext) = ext_of(name) else {
                return Qualification::Skip(SkipReason::NotMedia);
            };
            let ext = ext.as_str();
            if ext == "aae" {
                return Qualification::Skip(SkipReason::Sidecar);
            }
            if VIDEO_EXTS.contains(&ext) {
                return Qualification::Skip(SkipReason::Video);
            }
            let siblings = by_stem.get(&stem_of(name));
            if IMAGE_EXTS.contains(&ext) {
                let is_live = siblings.is_some_and(|s| s.has_video);
                let kind = if is_live {
                    MediaKind::LivePhotoStill
                } else {
                    MediaKind::Image
                };
                return Qualification::Enrich(kind);
            }
            if RAW_EXTS.contains(&ext) {
                // A RAW beside a same-stem JPEG defers to the JPEG (cheaper decode);
                // a lone RAW is enriched as a plain image.
                if siblings.is_some_and(|s| s.has_jpeg) {
                    return Qualification::Skip(SkipReason::RawWithJpegSibling);
                }
                return Qualification::Enrich(MediaKind::Image);
            }
            Qualification::Skip(SkipReason::NotMedia)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_image_enriches_non_media_skips() {
        let quals = qualify_dir(&["photo.jpg", "notes.txt", "archive.zip", "no_ext"]);
        assert_eq!(quals[0], Qualification::Enrich(MediaKind::Image));
        assert_eq!(quals[1], Qualification::Skip(SkipReason::NotMedia));
        assert_eq!(quals[2], Qualification::Skip(SkipReason::NotMedia));
        assert_eq!(
            quals[3],
            Qualification::Skip(SkipReason::NotMedia),
            "no extension ⇒ not media"
        );
    }

    #[test]
    fn all_supported_image_extensions_enrich() {
        for name in ["a.png", "b.heic", "c.HEIF", "d.tiff", "e.webp", "f.JPEG"] {
            assert_eq!(
                qualify_dir(&[name])[0],
                Qualification::Enrich(MediaKind::Image),
                "{name} should enrich (extension match is case-insensitive)"
            );
        }
    }

    #[test]
    fn videos_are_skipped_out_of_scope() {
        let quals = qualify_dir(&["clip.mov", "movie.mp4", "screen.m4v"]);
        for q in quals {
            assert_eq!(q, Qualification::Skip(SkipReason::Video));
        }
    }

    #[test]
    fn live_photo_still_is_tagged_and_its_motion_skipped() {
        // A HEIC still with a same-stem MOV motion component: the still enriches as
        // a Live Photo still; the MOV is a video and skips.
        let quals = qualify_dir(&["IMG_0007.heic", "IMG_0007.mov"]);
        assert_eq!(quals[0], Qualification::Enrich(MediaKind::LivePhotoStill));
        assert_eq!(quals[1], Qualification::Skip(SkipReason::Video));
    }

    #[test]
    fn aae_sidecar_is_skipped() {
        let quals = qualify_dir(&["IMG_0008.heic", "IMG_0008.aae"]);
        assert_eq!(quals[0], Qualification::Enrich(MediaKind::Image));
        assert_eq!(quals[1], Qualification::Skip(SkipReason::Sidecar));
    }

    #[test]
    fn raw_with_jpeg_sibling_enriches_jpeg_skips_raw() {
        // The JPEG is the primary (enriched); the same-stem RAW is redundant.
        let quals = qualify_dir(&["DSC_1.cr2", "DSC_1.jpg"]);
        assert_eq!(quals[0], Qualification::Skip(SkipReason::RawWithJpegSibling));
        assert_eq!(quals[1], Qualification::Enrich(MediaKind::Image));
    }

    #[test]
    fn lone_raw_is_enriched() {
        let quals = qualify_dir(&["DSC_2.dng"]);
        assert_eq!(quals[0], Qualification::Enrich(MediaKind::Image));
    }
}

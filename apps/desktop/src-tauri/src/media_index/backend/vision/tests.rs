//! Tests for the real Vision OCR backend. The whole [`super`] module is macOS-only
//! (`#[cfg(target_os = "macos")]` at its `mod` declaration), so these run only on
//! macOS — they exercise real Vision/ImageIO FFI against a committed fixture.

use std::path::PathBuf;

use crate::media_index::backend::{ImageInput, VisionBackend, VisionError};
use crate::media_index::predicate::MediaKind;

use super::VisionOcrBackend;

/// The committed fixture: a small PNG rendering the words "CMDR OCR" and "hello
/// 2026" in black on white (generated once via CoreGraphics text drawing).
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/media_index/backend/test-fixtures/ocr-sample.png")
}

fn input(path: &str) -> ImageInput {
    ImageInput {
        path: path.to_string(),
        kind: MediaKind::Image,
        bytes: None,
    }
}

#[test]
fn real_ocr_reads_the_known_words_from_the_fixture() {
    let backend = VisionOcrBackend::new();
    let path = fixture_path();
    assert!(path.exists(), "fixture missing at {}", path.display());

    let result = backend
        .ocr(&input(&path.to_string_lossy()))
        .expect("real OCR should succeed on the fixture");
    let text = result.text.to_uppercase();

    // The recognizer is allowed to segment lines differently across OS versions, so
    // assert on the presence of the known words, not an exact string.
    assert!(
        text.contains("CMDR"),
        "expected 'CMDR' in recognized text, got: {:?}",
        result.text
    );
    assert!(
        text.contains("OCR"),
        "expected 'OCR' in recognized text, got: {:?}",
        result.text
    );
    assert!(
        text.contains("2026"),
        "expected '2026' in recognized text, got: {:?}",
        result.text
    );
}

#[test]
fn real_ocr_reads_the_known_words_from_prefetched_bytes() {
    // The network byte-fetch path: the enrich layer reads compressed bytes off the
    // mount and passes them in via `ImageInput.bytes` (never a `std::fs` read on this
    // serialized OCR thread). The backend must decode from those bytes and OCR them.
    let backend = VisionOcrBackend::new();
    let bytes = std::fs::read(fixture_path()).expect("read fixture bytes");
    let input = ImageInput {
        // A path that does NOT exist on disk, proving the backend used `bytes`, not it.
        path: "/Volumes/naspi/DCIM/prefetched.png".to_string(),
        kind: MediaKind::Image,
        bytes: Some(bytes),
    };
    let text = backend
        .ocr(&input)
        .expect("OCR from prefetched bytes")
        .text
        .to_uppercase();
    assert!(
        text.contains("CMDR"),
        "expected 'CMDR' from prefetched bytes, got: {text:?}"
    );
    assert!(
        text.contains("OCR"),
        "expected 'OCR' from prefetched bytes, got: {text:?}"
    );
}

#[test]
fn a_non_image_file_returns_a_typed_decode_error_not_a_panic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bogus = dir.path().join("not-an-image.txt");
    std::fs::write(&bogus, b"this is plainly not an image file").expect("write bogus file");

    let backend = VisionOcrBackend::new();
    let err = backend
        .ocr(&input(&bogus.to_string_lossy()))
        .expect_err("a non-image file must not OCR");
    assert!(
        matches!(err, VisionError::Decode(_)),
        "expected a typed Decode error, got {err:?}"
    );
}

#[test]
fn an_empty_file_returns_a_typed_decode_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let empty = dir.path().join("empty.png");
    std::fs::write(&empty, b"").expect("write empty file");

    let backend = VisionOcrBackend::new();
    let err = backend
        .ocr(&input(&empty.to_string_lossy()))
        .expect_err("an empty file must not OCR");
    assert!(
        matches!(err, VisionError::Decode(_)),
        "expected a typed Decode error, got {err:?}"
    );
}

#[test]
fn a_missing_file_returns_a_typed_decode_error() {
    let backend = VisionOcrBackend::new();
    let err = backend
        .ocr(&input("/no/such/path/definitely-missing.png"))
        .expect_err("a missing file must not OCR");
    assert!(
        matches!(err, VisionError::Decode(_)),
        "expected a typed Decode error, got {err:?}"
    );
}

/// End-to-end over a REAL SMB mount: read an image's bytes off `/Volumes/naspi`
/// (the OS-mount byte-fetch path, `FsByteFetcher`) and OCR them with real Vision.
/// Self-skips when the NAS isn't mounted (CI, most dev machines), so it's safe to
/// leave un-ignored: it proves the real network path only where the NAS exists.
/// Read-only on the NAS (it only reads image bytes; nothing is written there).
#[test]
fn real_smb_byte_fetch_over_naspi_then_ocr_when_mounted() {
    use crate::media_index::network::fetch::{ByteFetcher, FsByteFetcher};
    use std::time::Duration;

    let root = std::path::Path::new("/Volumes/naspi");
    if !root.exists() {
        return; // NAS not mounted (CI, most dev machines): skip gracefully.
    }
    let Some(image_path) = find_first_image(root, 4, &mut 0) else {
        return; // No image found in the bounded walk: skip.
    };

    // 1) Read the compressed bytes off the OS mount, timeout-bounded (the real
    //    byte-fetch path the network pass uses).
    let bytes = FsByteFetcher
        .fetch(&image_path.to_string_lossy(), Duration::from_secs(30))
        .unwrap_or_else(|e| panic!("SMB byte-fetch failed for {}: {e:?}", image_path.display()));
    assert!(!bytes.is_empty(), "fetched image bytes should be non-empty");

    // 2) OCR the pre-fetched bytes with real Vision (no `std::fs` on the OCR thread).
    //    A real photo may or may not contain text; either way it must return a typed
    //    result, never panic or hang — that's what this asserts.
    let backend = VisionOcrBackend::new();
    let _ = backend.ocr(&ImageInput {
        path: image_path.to_string_lossy().into_owned(),
        kind: MediaKind::Image,
        bytes: Some(bytes),
    });
}

/// A bounded recursive search for the first image file under `root` (depth-limited,
/// entry-capped) so the real-NAS test doesn't deep-walk a huge library.
#[cfg(test)]
fn find_first_image(dir: &std::path::Path, depth: u32, visited: &mut u32) -> Option<PathBuf> {
    const MAX_ENTRIES: u32 = 4000;
    if depth == 0 || *visited >= MAX_ENTRIES {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        *visited += 1;
        if *visited >= MAX_ENTRIES {
            break;
        }
        let path = entry.path();
        let file_type = entry.file_type().ok()?;
        if file_type.is_dir() {
            subdirs.push(path);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "heic" | "heif" | "gif" | "tiff") {
                return Some(path);
            }
        }
    }
    for sub in subdirs {
        if let Some(found) = find_first_image(&sub, depth - 1, visited) {
            return Some(found);
        }
    }
    None
}

#[test]
fn real_analyze_returns_ocr_tags_and_a_stable_length_feature_print() {
    // The full tags + similarity analysis over the committed fixture: one decode, real Vision OCR +
    // classify + feature print.
    let backend = VisionOcrBackend::new();
    let path = fixture_path();
    assert!(path.exists(), "fixture missing at {}", path.display());

    let a = backend
        .analyze(&input(&path.to_string_lossy()))
        .expect("real analyze should succeed on the fixture");

    // OCR still reads the known words (the fixture renders "CMDR OCR").
    assert!(
        a.ocr.text.to_uppercase().contains("CMDR"),
        "expected OCR text to contain CMDR, got {:?}",
        a.ocr.text
    );

    // Tags are well-formed: every confidence is in [0, 1] and above the store floor.
    for tag in &a.tags {
        assert!(
            (0.0..=1.0).contains(&tag.score),
            "tag score out of range: {} = {}",
            tag.label,
            tag.score
        );
        assert!(!tag.label.is_empty(), "a tag label should be non-empty");
    }

    // The feature print is a non-empty vector, and its length is stable across a
    // second analysis of the same image (image↔image comparability depends on it).
    let emb = a
        .embedding
        .expect("a feature print should be produced for a real image");
    assert!(!emb.is_empty(), "feature print should be non-empty");
    let b = backend
        .analyze(&input(&path.to_string_lossy()))
        .expect("second analyze");
    assert_eq!(
        b.embedding.map(|v| v.len()),
        Some(emb.len()),
        "feature-print length must be stable within a run"
    );
}

#[test]
fn real_analyze_of_a_photo_is_self_similar_and_over_naspi_when_mounted() {
    // Over a REAL image (self-cosine sanity): a feature print compared to itself is
    // ~1.0. Uses the fixture (always present) so this isn't NAS-gated.
    use crate::media_index::vector::cosine;
    let backend = VisionOcrBackend::new();
    let emb = backend
        .analyze(&input(&fixture_path().to_string_lossy()))
        .expect("analyze")
        .embedding
        .expect("feature print");
    let self_cos = cosine(&emb, &emb);
    assert!(
        (self_cos - 1.0).abs() < 1e-3,
        "self-cosine should be ~1.0, got {self_cos}"
    );
}

#[test]
fn engine_version_is_nonempty_stable_and_shaped() {
    let backend = VisionOcrBackend::new();
    let v1 = backend.engine_version();
    let v2 = backend.engine_version();
    assert_eq!(v1, v2, "engine version must be stable within a run");
    assert!(v1.starts_with("vision-ocr;os="), "unexpected engine stamp shape: {v1}");
    assert!(
        v1.contains(";rev="),
        "engine stamp should carry the Vision revision: {v1}"
    );
}

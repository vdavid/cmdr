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

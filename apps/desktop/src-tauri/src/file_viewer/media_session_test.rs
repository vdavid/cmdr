//! Tests for the media-open path (`try_open_media` via the public `open_session`).
//!
//! These exercise the orchestration this module owns end to end: classify by magic
//! bytes, mint a token, read header-only dimensions, and return a media `ViewerOpenResult`
//! with empty text fields. A non-media file falls through to the text pipeline, and
//! `open_session_as_text` forces text even for a media file (the "View as text" override).

use std::fs;
use std::path::{Path, PathBuf};

use super::content_kind::ViewerContentKind;
use super::media;
use super::session;

fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_viewer_media_session_{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create test directory");
    dir
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

/// A valid 1x1 transparent PNG, byte-for-byte (so `image::image_dimensions` reads
/// `1 x 1`). Magic `89 50 4E 47` classifies it as `Image` before any decode.
const ONE_BY_ONE_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
    0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', // IHDR length + type
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // width=1, height=1
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, // bit depth/color/etc + CRC
    0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', // IDAT length + type
    0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, // data
    0x18, 0xDD, 0x8D, 0xB0, // IDAT CRC
    0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82, // IEND
];

/// Minimal `%PDF-` header. Magic classifies it as `Pdf`; the viewer never decodes it
/// (WKWebView's `<embed>` does), so a header is enough for the open path.
const PDF_HEADER: &[u8] = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n";

fn write_bytes(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let file = dir.join(name);
    fs::write(&file, bytes).unwrap();
    file
}

#[test]
fn open_png_yields_image_session_with_token_and_dimensions() {
    let dir = create_test_dir("png");
    let file = write_bytes(&dir, "pixel.png", ONE_BY_ONE_PNG);

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();

    assert_eq!(result.kind, ViewerContentKind::Image);
    assert_eq!(result.file_name, "pixel.png");
    // Text fields are empty for a media session.
    assert_eq!(result.total_lines, Some(0));
    assert!(result.initial_lines.lines.is_empty());
    assert!(!result.is_indexing);
    // A token was minted and resolves to this file with the PNG MIME.
    let token = result.media_token.clone().expect("image session mints a token");
    let entry = media::resolve_token(&token).expect("minted token resolves");
    assert_eq!(entry.kind, ViewerContentKind::Image);
    assert_eq!(entry.mime, "image/png");
    // Header-only dimensions read 1x1.
    let dims = result.media_dimensions.expect("PNG dimensions read header-only");
    assert_eq!((dims.width, dims.height), (1, 1));

    // Closing drops the token (the single choke point).
    session::close_session(&result.session_id).unwrap();
    assert!(
        media::resolve_token(&token).is_none(),
        "close_session drops the media token"
    );
    cleanup(&dir);
}

#[test]
fn open_pdf_yields_pdf_session_with_token_and_no_dimensions() {
    let dir = create_test_dir("pdf");
    let file = write_bytes(&dir, "doc.pdf", PDF_HEADER);

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();

    assert_eq!(result.kind, ViewerContentKind::Pdf);
    let token = result.media_token.clone().expect("pdf session mints a token");
    let entry = media::resolve_token(&token).expect("minted token resolves");
    assert_eq!(entry.mime, "application/pdf");
    // PDFs carry no image dimensions.
    assert!(result.media_dimensions.is_none());

    session::close_session(&result.session_id).unwrap();
    assert!(media::resolve_token(&token).is_none());
    cleanup(&dir);
}

#[test]
fn open_text_file_falls_through_to_text_pipeline() {
    let dir = create_test_dir("text");
    let file = write_bytes(&dir, "notes.txt", b"hello\nworld\n");

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();

    assert_eq!(result.kind, ViewerContentKind::Text);
    assert!(result.media_token.is_none());
    assert!(result.media_dimensions.is_none());
    // The text pipeline populated real lines.
    assert!(!result.initial_lines.lines.is_empty());

    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn open_as_text_forces_text_for_a_media_file() {
    let dir = create_test_dir("astext");
    let file = write_bytes(&dir, "pixel.png", ONE_BY_ONE_PNG);

    // The "View as text" override: a real PNG, opened as text, mints no token and
    // flows through the text pipeline (it shows the raw bytes).
    let result = session::open_session_as_text(file.to_str().unwrap(), "root").unwrap();

    assert_eq!(result.kind, ViewerContentKind::Text);
    assert!(result.media_token.is_none());
    assert!(result.media_dimensions.is_none());

    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

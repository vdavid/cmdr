//! Unit tests for the content classifier and its MIME mapping.

use super::content_kind::{ViewerContentKind, classify_viewer_content, media_mime};

/// Builds a head buffer from a magic prefix padded out so length-sensitive checks
/// (WebP, HEIC) have room.
fn head(prefix: &[u8]) -> Vec<u8> {
    let mut v = prefix.to_vec();
    v.resize(prefix.len().max(64), 0);
    v
}

fn classify(bytes: &[u8], ext: Option<&str>) -> ViewerContentKind {
    classify_viewer_content(bytes, ext, true)
}

#[test]
fn jpeg_magic_classifies_as_image() {
    assert_eq!(
        classify(&head(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg")),
        ViewerContentKind::Image
    );
}

#[test]
fn png_magic_classifies_as_image() {
    let png = head(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    assert_eq!(classify(&png, Some("png")), ViewerContentKind::Image);
}

#[test]
fn gif_magic_classifies_as_image() {
    assert_eq!(classify(b"GIF89a...........", Some("gif")), ViewerContentKind::Image);
    assert_eq!(classify(b"GIF87a...........", Some("gif")), ViewerContentKind::Image);
}

#[test]
fn webp_magic_classifies_as_image() {
    let mut buf = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
    buf.resize(64, 0);
    assert_eq!(classify(&buf, Some("webp")), ViewerContentKind::Image);
}

#[test]
fn riff_without_webp_is_not_an_image() {
    // A RIFF/WAVE audio file shares the RIFF header but is not a WebP image.
    let mut buf = b"RIFF\x00\x00\x00\x00WAVEfmt ".to_vec();
    buf.resize(64, 0);
    assert_eq!(classify(&buf, Some("wav")), ViewerContentKind::Text);
}

#[test]
fn bmp_magic_classifies_as_image() {
    assert_eq!(classify(&head(b"BM"), Some("bmp")), ViewerContentKind::Image);
}

#[test]
fn tiff_both_byte_orders_classify_as_image() {
    assert_eq!(
        classify(&head(&[0x49, 0x49, 0x2A, 0x00]), Some("tif")),
        ViewerContentKind::Image
    );
    assert_eq!(
        classify(&head(&[0x4D, 0x4D, 0x00, 0x2A]), Some("tiff")),
        ViewerContentKind::Image
    );
}

#[test]
fn heic_ftyp_brands_classify_as_image() {
    for brand in [b"heic", b"heix", b"mif1", b"msf1"] {
        let mut buf = vec![0x00, 0x00, 0x00, 0x18];
        buf.extend_from_slice(b"ftyp");
        buf.extend_from_slice(brand);
        buf.resize(64, 0);
        assert_eq!(
            classify(&buf, Some("heic")),
            ViewerContentKind::Image,
            "brand {brand:?}"
        );
    }
}

#[test]
fn ftyp_with_non_heic_brand_is_not_an_image() {
    // An MP4 video (`ftyp` + `isom`/`mp42`) shares the box shape but is not a still image.
    let mut buf = vec![0x00, 0x00, 0x00, 0x18];
    buf.extend_from_slice(b"ftyp");
    buf.extend_from_slice(b"isom");
    buf.resize(64, 0);
    assert_eq!(classify(&buf, Some("mp4")), ViewerContentKind::Text);
}

#[test]
fn pdf_magic_classifies_as_pdf() {
    assert_eq!(classify(b"%PDF-1.7\n...", Some("pdf")), ViewerContentKind::Pdf);
}

#[test]
fn magic_beats_extension() {
    // A PDF wearing a `.jpg` extension is a PDF (magic decides, never the extension).
    assert_eq!(classify(b"%PDF-1.4\n...", Some("jpg")), ViewerContentKind::Pdf);
    // Plain text wearing a `.png` extension stays text.
    assert_eq!(
        classify(b"hello world, not a png", Some("png")),
        ViewerContentKind::Text
    );
}

#[test]
fn plain_text_classifies_as_text() {
    assert_eq!(classify(b"the quick brown fox\n", Some("txt")), ViewerContentKind::Text);
    assert_eq!(classify(b"fn main() {}\n", Some("rs")), ViewerContentKind::Text);
}

#[test]
fn empty_and_short_heads_classify_as_text() {
    assert_eq!(classify(b"", Some("png")), ViewerContentKind::Text);
    assert_eq!(classify(&[0xFF], Some("jpg")), ViewerContentKind::Text); // too short for FF D8 FF
    assert_eq!(classify(&[0xFF, 0xD8], Some("jpg")), ViewerContentKind::Text);
}

// --- SVG: conservative, extension-gated ---

#[test]
fn svg_root_with_svg_ext_classifies_as_image() {
    assert_eq!(
        classify(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>", Some("svg")),
        ViewerContentKind::Image
    );
}

#[test]
fn svg_root_after_xml_prolog_and_doctype_classifies_as_image() {
    let s = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\">\n<svg width=\"10\">";
    assert_eq!(classify(s, Some("svg")), ViewerContentKind::Image);
}

#[test]
fn svg_root_after_bom_and_comment_classifies_as_image() {
    let mut s = vec![0xEF, 0xBB, 0xBF];
    s.extend_from_slice(b"  <!-- a comment --> <svg>");
    assert_eq!(classify(&s, Some("svg")), ViewerContentKind::Image);
}

#[test]
fn svg_content_without_svg_ext_stays_text() {
    // Extension gate: SVG markup in a `.txt`/unknown file is not rendered as an image.
    assert_eq!(classify(b"<svg></svg>", Some("txt")), ViewerContentKind::Text);
    assert_eq!(classify(b"<svg></svg>", None), ViewerContentKind::Text);
}

#[test]
fn html_with_inline_svg_and_svg_ext_stays_text() {
    // The root token is `<html>`, not `<svg`, so even a `.svg` extension can't promote it.
    let s = b"<!DOCTYPE html>\n<html><body><svg></svg></body></html>";
    assert_eq!(classify(s, Some("svg")), ViewerContentKind::Text);
}

#[test]
fn svg_ext_but_no_svg_root_stays_text() {
    assert_eq!(classify(b"just some text", Some("svg")), ViewerContentKind::Text);
}

// --- locality gate ---

#[test]
fn non_local_always_classifies_as_text() {
    // Even a perfect PNG header is Text when the file isn't on a local volume.
    let png = head(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    assert_eq!(
        classify_viewer_content(&png, Some("png"), false),
        ViewerContentKind::Text
    );
    assert_eq!(
        classify_viewer_content(b"%PDF-1.7", Some("pdf"), false),
        ViewerContentKind::Text
    );
    assert_eq!(
        classify_viewer_content(b"<svg></svg>", Some("svg"), false),
        ViewerContentKind::Text
    );
}

// --- media_mime ---

#[test]
fn media_mime_maps_each_kind() {
    assert_eq!(media_mime(b"%PDF-1.7", ViewerContentKind::Pdf), Some("application/pdf"));
    assert_eq!(
        media_mime(&[0xFF, 0xD8, 0xFF, 0xE0], ViewerContentKind::Image),
        Some("image/jpeg")
    );
    let png = head(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    assert_eq!(media_mime(&png, ViewerContentKind::Image), Some("image/png"));
    assert_eq!(media_mime(b"GIF89a", ViewerContentKind::Image), Some("image/gif"));
    assert_eq!(media_mime(b"BM\x00\x00", ViewerContentKind::Image), Some("image/bmp"));
    let mut webp = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
    webp.resize(16, 0);
    assert_eq!(media_mime(&webp, ViewerContentKind::Image), Some("image/webp"));
    assert_eq!(
        media_mime(&[0x49, 0x49, 0x2A, 0x00], ViewerContentKind::Image),
        Some("image/tiff")
    );
    let mut heic = vec![0x00, 0x00, 0x00, 0x18];
    heic.extend_from_slice(b"ftypheic");
    assert_eq!(media_mime(&heic, ViewerContentKind::Image), Some("image/heic"));
    // No raster magic + Image kind == the confirmed-SVG case.
    assert_eq!(media_mime(b"<svg>", ViewerContentKind::Image), Some("image/svg+xml"));
    assert_eq!(media_mime(b"anything", ViewerContentKind::Text), None);
}

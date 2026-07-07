//! Tests for the pure paste-clipboard-as-file core in `payload.rs`: the
//! conservative markdown sniffer, the flavor-precedence picker (incl. TIFF→PNG
//! conversion and the fall-through on a bad TIFF), and the payload→content
//! mapping. Plus a guard that the injected `ClipboardData` static stays separate
//! from the file-URL `ClipboardEntry` static.
//!
//! Child of the `payload` module (macOS-gated), so `super::*` reaches the
//! functions under test — including ones re-exported only within the crate — and
//! `crate::clipboard::store::*` reaches the injection surface (a descendant of
//! the private `store` module).

use super::{ClipboardPayload, PastedContent, looks_like_markdown, payload_to_content, pick_clipboard_payload};
use crate::clipboard::PastedKind;
use crate::clipboard::store::ClipboardData;

// The 8-byte PNG signature. `tiff_to_png` must emit real PNG bytes.
const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// A valid TIFF that `NSBitmapImageRep` can decode, synthesized via the `image`
/// crate so the conversion has honest input (a hand-rolled blob risks the OS
/// decoder rejecting it, which would silently turn a conversion test into a
/// fall-through test).
fn valid_tiff_bytes() -> Vec<u8> {
    use image::{ImageFormat, RgbImage};
    let img = RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]));
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Tiff)
        .expect("encode a 2x2 TIFF");
    bytes
}

// ============================================================================
// Markdown sniffer — conservative: strong signal OR >=2 DISTINCT weak kinds.
// ============================================================================

#[test]
fn looks_like_markdown_promotes_on_strong_signals() {
    // Each is a single STRONG signal and must promote on its own.
    let md_cases: &[(&str, &str)] = &[
        ("```\ncode\n```", "fenced code block"),
        ("# Heading", "ATX h1 at line start"),
        ("## Sub", "ATX h2 at line start"),
        ("intro line\n# Heading", "ATX heading at the start of a LATER line"),
        ("text before\n```\ncode\n```", "fenced code block mid-text"),
    ];
    for (input, label) in md_cases {
        assert!(looks_like_markdown(input), "expected .md for {label}: {input:?}");
    }
}

#[test]
fn looks_like_markdown_promotes_on_two_distinct_weak_kinds() {
    let md_cases: &[(&str, &str)] = &[
        ("See [a](b) and *emph* here", "link + emphasis"),
        ("- item\n> quoted", "list + blockquote"),
        ("> quote with a [a](b) link", "blockquote + link"),
        ("1. first\n2. second, with **bold**", "list + emphasis"),
    ];
    for (input, label) in md_cases {
        assert!(looks_like_markdown(input), "expected .md for {label}: {input:?}");
    }
}

#[test]
fn looks_like_markdown_stays_txt_when_in_doubt() {
    // The conservative bar: anything short of a strong signal or >=2 DISTINCT
    // weak kinds stays .txt. Several of these are deliberate spec pins (marked)
    // — a wrong `.md` guess is worse than a plain `.txt`.
    let txt_cases: &[(&str, &str)] = &[
        ("", "empty"),
        ("   \n  \t", "whitespace only"),
        ("https://example.com/some/path", "URL-only text is NOT a markdown link"),
        ("Buy 2 * 3 eggs at the store", "a single stray asterisk is not emphasis"),
        ("See [a](b) for the details", "a single weak kind (link only)"),
        ("*just emphasis* and nothing else", "a single weak kind (emphasis only)"),
        (
            "- one lonely bullet",
            "a single weak kind (list only); lists are WEAK per spec",
        ),
        ("> just a quote, alone", "a single weak kind (blockquote only)"),
        // SPEC PIN: "distinct" means distinct KINDS. Two links = one kind → txt.
        ("[a](b) and also [c](d)", "two links are ONE distinct kind"),
        ("the value foo # bar is fine", "a hash mid-line is not an ATX heading"),
        // SPEC PIN (judgment call): CommonMark ATX needs a space after `#`.
        ("#hashtag no space after hash", "no space after # is not an ATX heading"),
        ("Just a plain sentence, nothing special about it.", "no signals at all"),
    ];
    for (input, label) in txt_cases {
        assert!(!looks_like_markdown(input), "expected .txt for {label}: {input:?}");
    }
}

#[test]
fn looks_like_markdown_does_not_treat_intraword_or_spaced_delimiters_as_emphasis() {
    // Lead ruling (2026-07-07): CommonMark does NOT treat intraword `_`
    // (snake_case) or space-flanked `*` (arithmetic) as emphasis. So even
    // alongside a REAL second signal (a link), these must stay `.txt` — a wrong
    // `.md` is worse than a plain `.txt`. These pin the sniffer FIX: the only
    // genuine signal in each is the link (one kind), so the result is txt.
    let txt_cases: &[(&str, &str)] = &[
        (
            "see my_module_name in [docs](url)",
            "intraword _ (snake_case) is not emphasis; only the link is a real signal",
        ),
        (
            "the result is 2 * 3 * 4, see [ref](url)",
            "space-flanked * (arithmetic) is not emphasis; only the link is real",
        ),
        (
            "area is 2*3*4, see [ref](url)",
            "intraword * (unspaced arithmetic) is not emphasis either; only the link is real",
        ),
        (
            "call foo_bar_baz, then read [a](b) and [c](d)",
            "snake_case is not emphasis and two links are one kind → still one real signal",
        ),
    ];
    for (input, label) in txt_cases {
        assert!(!looks_like_markdown(input), "expected .txt for {label}: {input:?}");
    }
}

#[test]
fn looks_like_markdown_is_bounded_on_hostile_input() {
    // Hostile-case guard (lead's diff review): a multi-megabyte pathological
    // paste must not hang the sniffer. The sniff is bounded (linear + a prefix
    // cap), so this returns quickly; the test COMPLETING (rather than the runner
    // timing out) is the real assertion for the bounded-work property, and the
    // result must be a sane `.txt`.
    let hostile = "*a".repeat(1_000_000); // ~2 MB, ~1M asterisks — quadratic-death bait
    assert!(
        !looks_like_markdown(&hostile),
        "a pathological asterisk run must sniff as .txt, fast"
    );

    // The sniff only considers a bounded prefix, so a real signal buried far past
    // that prefix is NOT seen. Pin that deliberate limitation — with a CONTROL so
    // the "deep heading ignored" result can't pass for the wrong reason (e.g. if
    // the heading weren't a valid signal at all).
    let deep_heading = format!("{}\n# Heading", "x".repeat(200_000)); // heading well past the sniff prefix
    assert!(
        !looks_like_markdown(&deep_heading),
        "a heading buried past the bounded sniff prefix is ignored (bounded work)"
    );
    // CONTROL: the exact same heading at the START is a valid strong signal → .md.
    let early_heading = format!("# Heading\n{}", "x".repeat(200_000));
    assert!(
        looks_like_markdown(&early_heading),
        "the same heading within the sniff prefix DOES promote — proves the deep one was ignored by the bound, not because headings don't count"
    );
}

// ============================================================================
// Flavor precedence: image(png>tiff>jpeg) > pdf > text.
// ============================================================================

#[test]
fn pick_prefers_png_over_every_other_flavor() {
    let data = ClipboardData {
        png: Some(b"PNGDATA".to_vec()),
        tiff: Some(valid_tiff_bytes()),
        jpeg: Some(b"JPEGDATA".to_vec()),
        pdf: Some(b"%PDF-1.4".to_vec()),
        text: Some("hello".to_string()),
    };
    match pick_clipboard_payload(data) {
        ClipboardPayload::Png(bytes) => assert_eq!(bytes, b"PNGDATA", "PNG is written verbatim, not re-encoded"),
        other => panic!("expected Png (highest precedence), got {other:?}"),
    }
}

#[test]
fn pick_converts_tiff_to_png_when_no_png_present() {
    let data = ClipboardData {
        tiff: Some(valid_tiff_bytes()),
        jpeg: Some(b"JPEGDATA".to_vec()),
        pdf: Some(b"%PDF".to_vec()),
        text: Some("hello".to_string()),
        ..Default::default()
    };
    match pick_clipboard_payload(data) {
        ClipboardPayload::Png(bytes) => assert!(
            bytes.starts_with(PNG_MAGIC),
            "a converted TIFF must be real PNG bytes (got {:02x?}…)",
            &bytes[..bytes.len().min(8)]
        ),
        other => panic!("expected TIFF converted to Png, got {other:?}"),
    }
}

#[test]
fn pick_falls_through_to_jpeg_when_tiff_conversion_fails() {
    // Spec: "a failed TIFF conversion falls through to the next flavor." Garbage
    // TIFF bytes can't decode, so JPEG (next in precedence) must win verbatim.
    let data = ClipboardData {
        tiff: Some(b"this is not a tiff".to_vec()),
        jpeg: Some(b"JPEGDATA".to_vec()),
        text: Some("hello".to_string()),
        ..Default::default()
    };
    match pick_clipboard_payload(data) {
        ClipboardPayload::Jpeg(bytes) => assert_eq!(bytes, b"JPEGDATA"),
        other => panic!("expected fall-through to Jpeg on a bad TIFF, got {other:?}"),
    }
}

#[test]
fn pick_prefers_jpeg_over_pdf_and_text() {
    let data = ClipboardData {
        jpeg: Some(b"JPEGDATA".to_vec()),
        pdf: Some(b"%PDF".to_vec()),
        text: Some("hello".to_string()),
        ..Default::default()
    };
    match pick_clipboard_payload(data) {
        ClipboardPayload::Jpeg(bytes) => assert_eq!(bytes, b"JPEGDATA"),
        other => panic!("expected Jpeg over pdf/text, got {other:?}"),
    }
}

#[test]
fn pick_prefers_pdf_over_text() {
    let data = ClipboardData {
        pdf: Some(b"%PDF-1.7".to_vec()),
        text: Some("hello".to_string()),
        ..Default::default()
    };
    match pick_clipboard_payload(data) {
        ClipboardPayload::Pdf(bytes) => assert_eq!(bytes, b"%PDF-1.7"),
        other => panic!("expected Pdf over text, got {other:?}"),
    }
}

#[test]
fn pick_returns_text_when_only_text_present() {
    let data = ClipboardData {
        text: Some("plain content".to_string()),
        ..Default::default()
    };
    match pick_clipboard_payload(data) {
        ClipboardPayload::Text(s) => assert_eq!(s, "plain content"),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn pick_returns_nothing_for_empty_clipboard() {
    match pick_clipboard_payload(ClipboardData::default()) {
        ClipboardPayload::Nothing => {}
        other => panic!("expected Nothing for an empty clipboard, got {other:?}"),
    }
}

// ============================================================================
// payload_to_content — ext / kind / bytes mapping (pure, no fs).
// ============================================================================

fn content_of(payload: ClipboardPayload) -> PastedContent {
    payload_to_content(payload).expect("expected Some content")
}

#[test]
fn payload_to_content_maps_png() {
    let c = content_of(ClipboardPayload::Png(b"PNGDATA".to_vec()));
    assert_eq!(c.ext, "png");
    assert_eq!(c.kind, PastedKind::Image);
    assert_eq!(c.bytes, b"PNGDATA");
}

#[test]
fn payload_to_content_maps_jpeg_to_jpg_extension() {
    let c = content_of(ClipboardPayload::Jpeg(b"JPEGDATA".to_vec()));
    assert_eq!(c.ext, "jpg", "JPEG is written as .jpg, verbatim");
    assert_eq!(c.kind, PastedKind::Image);
    assert_eq!(c.bytes, b"JPEGDATA");
}

#[test]
fn payload_to_content_maps_pdf() {
    let c = content_of(ClipboardPayload::Pdf(b"%PDF".to_vec()));
    assert_eq!(c.ext, "pdf");
    assert_eq!(c.kind, PastedKind::Pdf);
    assert_eq!(c.bytes, b"%PDF");
}

#[test]
fn payload_to_content_maps_plain_text_to_txt() {
    let c = content_of(ClipboardPayload::Text("just some plain words".to_string()));
    assert_eq!(c.ext, "txt");
    assert_eq!(c.kind, PastedKind::Text);
    assert_eq!(c.bytes, b"just some plain words");
}

#[test]
fn payload_to_content_maps_markdown_text_to_md() {
    let c = content_of(ClipboardPayload::Text("# A heading".to_string()));
    assert_eq!(c.ext, "md", "text the sniffer flags as markdown gets .md");
    assert_eq!(
        c.kind,
        PastedKind::Text,
        "kind is still Text; only the extension differs"
    );
    assert_eq!(c.bytes, b"# A heading");
}

#[test]
fn payload_to_content_maps_nothing_to_none() {
    assert!(
        payload_to_content(ClipboardPayload::Nothing).is_none(),
        "Nothing is the typed no-op; the caller treats None as 'nothing pasteable'"
    );
}

// ============================================================================
// Injection surface: the ClipboardData static is separate from the file-URL one.
// ============================================================================

// Serialize the two tests that touch the shared global stores.
static STORE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn injected_clipboard_data_round_trips() {
    use crate::clipboard::store::{clear_clipboard_data, read_clipboard_data, write_clipboard_data};
    let _g = STORE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_clipboard_data();
    write_clipboard_data(ClipboardData {
        text: Some("injected".to_string()),
        png: Some(b"p".to_vec()),
        ..Default::default()
    });
    let read = read_clipboard_data();
    assert_eq!(read.text.as_deref(), Some("injected"));
    assert_eq!(read.png.as_deref(), Some(&b"p"[..]));
    clear_clipboard_data();
}

#[test]
fn injecting_clipboard_data_does_not_clobber_the_file_url_entry() {
    // The two flows (copy/cut/paste FILES vs. paste CONTENT) share the store
    // module but MUST use separate statics, or a content paste would wipe a
    // pending file-copy clipboard.
    use crate::clipboard::store::{
        clear, clear_clipboard_data, read_clipboard_data, read_paths, write_clipboard_data, write_paths,
    };
    use std::path::PathBuf;
    let _g = STORE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear();
    clear_clipboard_data();

    write_paths(&[PathBuf::from("/tmp/copied.txt")]);
    write_clipboard_data(ClipboardData {
        text: Some("some pasted text".to_string()),
        ..Default::default()
    });

    // Neither write disturbed the other static.
    assert_eq!(
        read_paths(),
        vec![PathBuf::from("/tmp/copied.txt")],
        "file-URL entry survived"
    );
    assert_eq!(read_clipboard_data().text.as_deref(), Some("some pasted text"));

    clear();
    clear_clipboard_data();
}

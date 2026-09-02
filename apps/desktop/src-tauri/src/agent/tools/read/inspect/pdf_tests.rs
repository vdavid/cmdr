//! Tests for the PDF kind: the authored two-page PDF end to end (structure, page window,
//! `find` with pages), the three `textUnavailable` reasons, the pure page-loop shapers
//! under their caps and the cancel flag, a PDF inside a zip, params, and the text-only walk.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use pdf_extract::content::{Content as PdfOps, Operation};
use pdf_extract::{Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream, dictionary};
use serde_json::json;

use super::find::MAX_FIND_LINES;
use super::pdf::{
    DEFAULT_MAX_PAGES, MAX_MAX_PAGES, PageText, find_in_pages, header_version, read_pdf_with_cap, window_from_pages,
};
use super::tests::assert_text_only;
use super::*;
use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_viewer::MAX_SEARCH_MATCHES;
use crate::test_support::TestDir;
use cmdr_archive::test_fixtures::{build_zip, stored};

// ── Authoring ─────────────────────────────────────────────────────────────────

/// A page that draws `text` in Helvetica at the top left.
fn text_page(text: &str) -> PdfOps {
    PdfOps {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    }
}

/// A page with a filled rectangle and no text at all: a drawing, or a scan's shape.
fn rectangle_page() -> PdfOps {
    PdfOps {
        operations: vec![
            Operation::new("re", vec![100.into(), 100.into(), 200.into(), 200.into()]),
            Operation::new("f", vec![]),
        ],
    }
}

/// A PDF 1.5 with one page per entry of `pages`, and an Info dictionary when asked.
fn author_pdf(pages: &[PdfOps], info: Option<(&str, &str)>) -> Document {
    let mut doc = Document::new();
    doc.version = "1.5".to_string();
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let kids: Vec<Object> = pages
        .iter()
        .map(|ops| {
            let content_id = doc.add_object(Stream::new(dictionary! {}, ops.encode().unwrap()));
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            })
            .into()
        })
        .collect();
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    // A file ID pair, which the standard security handler's key derivation needs.
    doc.trailer.set(
        "ID",
        vec![
            Object::string_literal(vec![7u8; 16]),
            Object::string_literal(vec![9u8; 16]),
        ],
    );
    if let Some((title, author)) = info {
        let info_id = doc.add_object(dictionary! {
            "Title" => Object::string_literal(title),
            "Author" => Object::string_literal(author),
        });
        doc.trailer.set("Info", info_id);
    }
    doc
}

fn pdf_bytes(mut doc: Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// Page one greets, page two names the tenant. Title and author set.
fn two_page_pdf() -> Vec<u8> {
    pdf_bytes(author_pdf(
        &[
            text_page("Hello from page one"),
            text_page("Second page mentions the tenant Acme"),
        ],
        Some(("Quarterly report", "Acme Finance")),
    ))
}

fn write(dir: &TestDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn window(page_start: usize, max_pages: usize) -> TextAsk {
    TextAsk::Window(WindowOpts {
        page_start,
        max_pages,
        ..WindowOpts::default()
    })
}

fn literal(query: &str) -> TextAsk {
    TextAsk::Find(Arc::new(
        Matcher::build(
            query,
            SearchMode {
                use_regex: false,
                case_sensitive: false,
            },
        )
        .unwrap(),
    ))
}

fn inspect(path: &Path, ask: &TextAsk) -> FileRow {
    inspect_path(path.to_str().unwrap(), ask, &AtomicBool::new(false))
}

fn pdf_of(row: &FileRow) -> &PdfContent {
    match row {
        FileRow::Ok(file) => match &file.content {
            Content::Pdf(pdf) => pdf,
            other => panic!("expected a pdf row, got {other:?}"),
        },
        other => panic!("expected an ok row, got {other:?}"),
    }
}

fn pages_of(pdf: &PdfContent) -> &PdfWindow {
    pdf.pages.as_ref().expect("a window read carries its pages")
}

// ── The authored PDF, end to end ─────────────────────────────────────────────

#[test]
fn a_two_page_pdf_answers_version_page_count_title_author_and_the_text_of_its_pages() {
    let dir = TestDir::new("inspect_pdf_two_pages");
    let path = write(&dir, "report.pdf", &two_page_pdf());

    let row = inspect(&path, &TextAsk::Window(WindowOpts::default()));
    let file = match &row {
        FileRow::Ok(file) => file,
        other => panic!("{other:?}"),
    };
    assert_eq!(file.mime.as_deref(), Some("application/pdf"));
    let pdf = pdf_of(&row);
    assert_eq!(pdf.version.as_deref(), Some("1.5"));
    assert_eq!(pdf.page_count, Some(2), "exact, from the page tree");
    assert_eq!(pdf.title.as_deref(), Some("Quarterly report"));
    assert_eq!(pdf.author.as_deref(), Some("Acme Finance"));
    assert_eq!(pdf.text_unavailable, None);
    assert_eq!(pdf.has_text_layer, Some(true));
    assert_eq!(pdf.find, None);

    let pages = pages_of(pdf);
    assert_eq!((pages.page_start, pages.returned_pages, pages.truncated), (1, 2, false));
    assert_eq!(pages.pages.len(), 2);
    assert_eq!(pages.pages[0].page, 1);
    assert!(
        pages.pages[0].text.contains("Hello from page one"),
        "{:?}",
        pages.pages[0].text
    );
    assert_eq!(pages.pages[1].page, 2);
    assert!(pages.pages[1].text.contains("tenant Acme"), "{:?}", pages.pages[1].text);
    assert!(pages.pages.iter().all(|p| !p.truncated && !p.unparseable));
}

#[test]
fn page_start_and_max_pages_pick_the_second_page_alone() {
    let dir = TestDir::new("inspect_pdf_page_range");
    let path = write(&dir, "report.pdf", &two_page_pdf());

    let row = inspect(&path, &window(2, 1));
    let pages = pages_of(pdf_of(&row));
    assert_eq!((pages.page_start, pages.returned_pages, pages.truncated), (2, 1, false));
    assert_eq!(pages.pages[0].page, 2);
    assert!(pages.pages[0].text.contains("Second page"));

    // The first page alone: more pages exist, and the window says so.
    let row = inspect(&path, &window(1, 1));
    let pages = pages_of(pdf_of(&row));
    assert_eq!((pages.returned_pages, pages.truncated), (1, true));

    // Past the end: an empty, un-truncated window, and nothing to say about a text layer.
    let row = inspect(&path, &window(5, 3));
    let pdf = pdf_of(&row);
    let pages = pages_of(pdf);
    assert_eq!((pages.page_start, pages.returned_pages, pages.truncated), (5, 0, false));
    assert_eq!(pdf.has_text_layer, None);
}

#[test]
fn a_page_with_only_a_drawn_rectangle_has_no_text_layer() {
    let dir = TestDir::new("inspect_pdf_scan");
    let path = write(&dir, "scan.pdf", &pdf_bytes(author_pdf(&[rectangle_page()], None)));

    let row = inspect(&path, &TextAsk::Window(WindowOpts::default()));
    let pdf = pdf_of(&row);
    assert_eq!(pdf.page_count, Some(1));
    assert_eq!(
        pdf.has_text_layer,
        Some(false),
        "whitespace only: a scan, not an empty document"
    );
    assert_eq!((pdf.title.as_deref(), pdf.author.as_deref()), (None, None));
    let pages = pages_of(pdf);
    assert_eq!(pages.returned_pages, 1);
    assert_eq!(pages.pages[0].text, "");
    assert!(!pages.pages[0].unparseable);
}

#[test]
fn find_hits_carry_their_page_and_the_window_is_omitted() {
    let dir = TestDir::new("inspect_pdf_find");
    let path = write(&dir, "report.pdf", &two_page_pdf());

    let row = inspect(&path, &literal("TENANT"));
    let pdf = pdf_of(&row);
    assert_eq!(pdf.pages, None, "with find, the hits take the window's place");
    assert_eq!(pdf.has_text_layer, Some(true));
    let hits = pdf.find.as_ref().expect("a find row carries hits");
    assert_eq!(hits.total_matches, 1);
    assert_eq!(hits.returned_lines, 1);
    assert!(!hits.truncated && !hits.scan_incomplete && !hits.matches_capped);
    assert_eq!(hits.pages_scanned, None, "the scan reached the last page");
    let hit = &hits.lines[0];
    assert_eq!((hit.page, hit.line, hit.matches), (Some(2), 1, 1));
    assert!(hit.text.contains("tenant Acme"), "{:?}", hit.text);

    // No hits: an honest zero, still a find row.
    let row = inspect(&path, &literal("unicorn"));
    let hits = pdf_of(&row).find.as_ref().unwrap();
    assert_eq!((hits.total_matches, hits.returned_lines), (0, 0));
}

// ── Text unavailable ─────────────────────────────────────────────────────────

#[test]
fn an_encrypted_pdf_is_text_unavailable_encrypted_and_reads_no_strings() {
    let dir = TestDir::new("inspect_pdf_encrypted");
    let mut doc = author_pdf(&[text_page("secret")], Some(("Hidden title", "Hidden author")));
    let state = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: "user",
        key_length: 128,
        permissions: Permissions::default(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let path = write(&dir, "locked.pdf", &pdf_bytes(doc));

    let row = inspect(&path, &TextAsk::Window(WindowOpts::default()));
    let pdf = pdf_of(&row);
    assert_eq!(pdf.text_unavailable, Some(PdfTextUnavailable::Encrypted));
    assert_eq!(pdf.version.as_deref(), Some("1.5"));
    assert_eq!(pdf.pages, None);
    assert_eq!(pdf.find, None);
    assert_eq!(pdf.has_text_layer, None);
    assert_eq!(
        (pdf.title.as_deref(), pdf.author.as_deref()),
        (None, None),
        "encrypted strings are not decoded, so they are not guessed at either"
    );

    // `find` over it is the same answer: nothing to search.
    let row = inspect(&path, &literal("secret"));
    let pdf = pdf_of(&row);
    assert_eq!(pdf.text_unavailable, Some(PdfTextUnavailable::Encrypted));
    assert_eq!(pdf.find, None);
}

#[test]
fn a_truncated_pdf_is_unparseable_with_its_header_version_still_answered() {
    let dir = TestDir::new("inspect_pdf_truncated");
    let whole = two_page_pdf();
    let path = write(&dir, "cut.pdf", &whole[..48]);

    let row = inspect(&path, &TextAsk::Window(WindowOpts::default()));
    let pdf = pdf_of(&row);
    assert_eq!(pdf.text_unavailable, Some(PdfTextUnavailable::Unparseable));
    assert_eq!(pdf.version.as_deref(), Some("1.5"), "the header is still there");
    assert_eq!(pdf.page_count, None);
    assert_eq!(pdf.pages, None);
    assert_eq!(pdf.has_text_layer, None);
}

#[test]
fn a_pdf_over_the_size_cap_is_too_large_and_is_never_parsed() {
    let dir = TestDir::new("inspect_pdf_too_large");
    let bytes = two_page_pdf();
    let path = write(&dir, "big.pdf", &bytes);

    let pdf = read_pdf_with_cap(
        &path,
        bytes.len() as u64,
        &bytes[..64],
        &TextAsk::Window(WindowOpts::default()),
        &AtomicBool::new(false),
        bytes.len() as u64 - 1,
    )
    .unwrap();
    assert_eq!(pdf.text_unavailable, Some(PdfTextUnavailable::TooLarge));
    assert_eq!(pdf.version.as_deref(), Some("1.5"));
    assert_eq!(pdf.page_count, None, "not parsed, so no page tree");
    assert_eq!(pdf.pages, None);

    // At the cap exactly, it parses.
    let pdf = read_pdf_with_cap(
        &path,
        bytes.len() as u64,
        &bytes[..64],
        &TextAsk::Window(WindowOpts::default()),
        &AtomicBool::new(false),
        bytes.len() as u64,
    )
    .unwrap();
    assert_eq!(pdf.text_unavailable, None);
    assert_eq!(pdf.page_count, Some(2));
}

#[test]
fn header_version_reads_the_first_line_and_nothing_else() {
    assert_eq!(
        header_version(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n"),
        Some("1.7".to_string())
    );
    assert_eq!(header_version(b"%PDF-2.0\r\n1 0 obj"), Some("2.0".to_string()));
    assert_eq!(header_version(b"%PDF-"), None);
    assert_eq!(header_version(b"%PDF-\n"), None);
    assert_eq!(header_version(b"PK\x03\x04"), None);
    assert_eq!(header_version(b""), None);
}

// ── The pure page loops ──────────────────────────────────────────────────────

fn opts(page_start: usize, max_pages: usize) -> WindowOpts {
    WindowOpts {
        page_start,
        max_pages,
        ..WindowOpts::default()
    }
}

/// An extractor over fixed page texts (1-based pages), asserting the loop never asks for a
/// page the document doesn't have.
fn pages(texts: &[PageText]) -> impl FnMut(usize) -> PageText + '_ {
    move |page| {
        texts
            .get(page - 1)
            .cloned()
            .expect("the loop stays inside the page count")
    }
}

fn text(s: &str) -> PageText {
    PageText::Text(s.to_string())
}

#[test]
fn the_window_carries_whole_pages_and_stops_before_one_that_would_break_the_row_cap() {
    let texts = [text("aaaaaaaaaa"), text("bbbbbbbbbb"), text("cccccccccc")];
    let cancel = AtomicBool::new(false);

    // Two pages of ten fill a cap of 20 exactly; the third would break it.
    let (window, has_text) = window_from_pages(3, opts(1, 3), &cancel, 20, 100, pages(&texts));
    assert_eq!((window.returned_pages, window.truncated), (2, true));
    assert_eq!(window.pages[1].text, "bbbbbbbbbb");
    assert_eq!(has_text, Some(true));

    // The first page always fits, even when it alone is over the row cap.
    let (window, _) = window_from_pages(3, opts(1, 3), &cancel, 4, 100, pages(&texts));
    assert_eq!((window.returned_pages, window.truncated), (1, true));
    assert_eq!(window.pages[0].text, "aaaaaaaaaa");
}

#[test]
fn a_page_over_the_page_cap_is_cut_and_says_so() {
    let texts = [text("abcdefgh"), text("ij")];
    let cancel = AtomicBool::new(false);
    let (window, _) = window_from_pages(2, opts(1, 3), &cancel, 1_000, 5, pages(&texts));
    assert_eq!(window.pages[0].text, "abcde");
    assert!(window.pages[0].truncated);
    assert_eq!(window.pages[1].text, "ij");
    assert!(!window.pages[1].truncated);
    assert!(!window.truncated, "every page of the document is here");
}

#[test]
fn a_pre_set_cancel_flag_stops_the_loop_before_the_first_page() {
    let texts = [text("a"), text("b")];
    let (window, has_text) = window_from_pages(2, opts(1, 3), &AtomicBool::new(true), 1_000, 100, pages(&texts));
    assert_eq!((window.returned_pages, window.truncated), (0, true));
    assert_eq!(
        has_text, None,
        "no page was decoded, so nothing is claimed about a text layer"
    );
}

#[test]
fn an_unparseable_page_is_flagged_and_does_not_decide_the_text_layer() {
    let cancel = AtomicBool::new(false);
    let texts = [PageText::Unparseable, text("   \n  ")];
    let (window, has_text) = window_from_pages(2, opts(1, 3), &cancel, 1_000, 100, pages(&texts));
    assert!(window.pages[0].unparseable);
    assert_eq!(window.pages[0].text, "");
    assert_eq!(window.pages[1].text, "", "whitespace is trimmed to nothing");
    assert_eq!(has_text, Some(false), "the one page that decoded held no text");

    let texts = [PageText::Unparseable];
    let (_, has_text) = window_from_pages(1, opts(1, 3), &cancel, 1_000, 100, pages(&texts));
    assert_eq!(has_text, None);
}

fn matcher(query: &str) -> Matcher {
    Matcher::build(
        query,
        SearchMode {
            use_regex: false,
            case_sensitive: false,
        },
    )
    .unwrap()
}

#[test]
fn find_groups_hits_by_page_and_line_with_a_snippet_around_the_first_match() {
    let texts = [
        text("no hit here\nthe needle, then another needle\nlast"),
        text("needle on page two"),
    ];
    let (hits, has_text) = find_in_pages(2, &matcher("needle"), &AtomicBool::new(false), pages(&texts));
    assert_eq!(has_text, Some(true));
    assert_eq!(hits.total_matches, 3);
    assert_eq!(hits.returned_lines, 2);
    assert!(!hits.truncated && !hits.scan_incomplete);
    assert_eq!(hits.pages_scanned, None);
    assert_eq!(
        (hits.lines[0].page, hits.lines[0].line, hits.lines[0].matches),
        (Some(1), 2, 2)
    );
    assert_eq!(hits.lines[0].text, "the needle, then another needle");
    assert_eq!(
        (hits.lines[1].page, hits.lines[1].line, hits.lines[1].matches),
        (Some(2), 1, 1)
    );
}

#[test]
fn find_stops_decoding_pages_once_the_line_cap_is_full_and_says_the_scan_is_incomplete() {
    let texts: Vec<PageText> = (0..100).map(|n| text(&format!("hit {n}"))).collect();
    let (hits, _) = find_in_pages(100, &matcher("hit"), &AtomicBool::new(false), pages(&texts));
    assert_eq!(hits.returned_lines, MAX_FIND_LINES);
    assert_eq!(
        hits.total_matches, MAX_FIND_LINES,
        "counted up to where the scan stopped"
    );
    assert!(hits.scan_incomplete, "pages past the cap were not decoded");
    assert_eq!(hits.pages_scanned, Some(MAX_FIND_LINES));
    assert!(!hits.truncated, "every matching line the scan saw is carried");

    // The page that fills the cap is finished (its extra lines are counted and reported as
    // truncated); the page after it is not decoded.
    let mut texts: Vec<PageText> = (0..MAX_FIND_LINES - 1).map(|_| text("hit")).collect();
    texts.push(text("hit\nhit\nhit"));
    texts.push(text("hit"));
    let (hits, _) = find_in_pages(texts.len(), &matcher("hit"), &AtomicBool::new(false), pages(&texts));
    assert_eq!(hits.total_matches, MAX_FIND_LINES + 2);
    assert_eq!(hits.returned_lines, MAX_FIND_LINES);
    assert!(hits.truncated);
    assert!(hits.scan_incomplete);
    assert_eq!(hits.pages_scanned, Some(MAX_FIND_LINES));

    // Every page decoded: nothing incomplete.
    let texts = [text("hit\nhit\nhit"), text("hit")];
    let (hits, _) = find_in_pages(2, &matcher("hit"), &AtomicBool::new(false), pages(&texts));
    assert_eq!((hits.total_matches, hits.returned_lines), (4, 4));
    assert!(!hits.scan_incomplete && !hits.truncated);
}

#[test]
fn find_caps_the_match_count_at_the_viewers_cap_and_says_so() {
    let page = text(&"hit\n".repeat(MAX_SEARCH_MATCHES + 5));
    let texts = [page, text("hit")];
    let (hits, _) = find_in_pages(2, &matcher("hit"), &AtomicBool::new(false), pages(&texts));
    assert_eq!(hits.total_matches, MAX_SEARCH_MATCHES);
    assert!(hits.matches_capped);
    assert_eq!(hits.returned_lines, MAX_FIND_LINES);
    assert!(hits.truncated);
}

#[test]
fn find_under_a_pre_set_cancel_flag_scans_nothing_and_says_so() {
    let texts = [text("hit")];
    let (hits, has_text) = find_in_pages(1, &matcher("hit"), &AtomicBool::new(true), pages(&texts));
    assert_eq!(hits.total_matches, 0);
    assert!(hits.scan_incomplete);
    assert_eq!(hits.pages_scanned, Some(0));
    assert_eq!(has_text, None);
}

#[test]
fn find_skips_an_unparseable_page_and_keeps_going() {
    let texts = [PageText::Unparseable, text("hit")];
    let (hits, has_text) = find_in_pages(2, &matcher("hit"), &AtomicBool::new(false), pages(&texts));
    assert_eq!(hits.total_matches, 1);
    assert_eq!(hits.lines[0].page, Some(2));
    assert_eq!(has_text, Some(true));
}

// ── Inside an archive, params, and the shape ─────────────────────────────────

#[test]
fn a_pdf_inside_a_zip_reads_as_a_pdf() {
    get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
    let dir = TestDir::new("inspect_pdf_in_zip");
    let zip = write(
        &dir,
        "bundle.zip",
        &build_zip(&[stored("docs/report.pdf", two_page_pdf())]),
    );
    let inner = format!("{}/docs/report.pdf", zip.display());

    let row = inspect_path(&inner, &TextAsk::Window(WindowOpts::default()), &AtomicBool::new(false));
    let pdf = pdf_of(&row);
    assert_eq!(pdf.page_count, Some(2));
    assert_eq!(pdf.title.as_deref(), Some("Quarterly report"));
    assert!(pages_of(pdf).pages[0].text.contains("Hello from page one"));
}

#[test]
fn params_default_and_clamp_the_page_range_and_reject_junk() {
    let p = parse_params(&json!({ "paths": ["/a.pdf"] })).unwrap();
    assert!(matches!(
        p.ask,
        TextAsk::Window(WindowOpts {
            page_start: 1,
            max_pages: DEFAULT_MAX_PAGES,
            ..
        })
    ));

    let p = parse_params(&json!({ "paths": ["/a.pdf"], "pageStart": 7, "maxPages": 500 })).unwrap();
    assert!(matches!(
        p.ask,
        TextAsk::Window(WindowOpts {
            page_start: 7,
            max_pages: MAX_MAX_PAGES,
            ..
        })
    ));

    let code = ToolError::invalid_params("").code;
    for bad in [
        json!({ "paths": ["/a.pdf"], "pageStart": 0 }),
        json!({ "paths": ["/a.pdf"], "pageStart": "two" }),
        json!({ "paths": ["/a.pdf"], "maxPages": 0 }),
        json!({ "paths": ["/a.pdf"], "maxPages": -1 }),
    ] {
        assert_eq!(parse_params(&bad).unwrap_err().code, code, "{bad}");
    }

    let schema = inspect_file_schema();
    assert_eq!(schema["properties"]["pageStart"]["minimum"], 1);
    assert_eq!(schema["properties"]["maxPages"]["maximum"], MAX_MAX_PAGES);
    assert!(
        schema["properties"]["find"]["description"]
            .as_str()
            .unwrap()
            .contains("PDF")
    );
}

#[test]
fn a_pdf_row_is_text_only() {
    let dir = TestDir::new("inspect_pdf_text_only");
    let path = write(&dir, "report.pdf", &two_page_pdf());
    for ask in [TextAsk::Window(WindowOpts::default()), literal("page")] {
        let row = inspect(&path, &ask);
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["content"]["kind"], "pdf");
        assert_text_only(&json, "pdf row");
    }
}

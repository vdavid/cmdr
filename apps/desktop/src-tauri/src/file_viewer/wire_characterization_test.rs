//! The viewer's wire, pinned as JSON: every answer a command sends, every error, and every
//! event payload, exactly as the frontend receives them today.
//!
//! These pins are what makes moving the viewer into its own crate, and giving it a second
//! host in the browser, provably byte-identical: the shapes below must not change. Ids and
//! tokens are random, so they're masked; everything else is literal. A pin that has to change
//! is a wire change, which the frontend (`bindings.ts`) has to follow.
//!
//! Pinned elsewhere, not repeated here: the emitted sequence of `viewer-media-load`
//! (`media_volume_test.rs`), the `cmdr-media://` answers (`media_protocol_test.rs`), the
//! Markdown image markers and their fetches (`markdown_session_test.rs`), the debug log's
//! rows (`log_view_test.rs`), and row boundaries (`row_characterization_test.rs`).

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use super::encoding::FileEncoding;
use super::media_volume::{MediaLoadStage, ViewerMediaLoad};
use super::pending_open::ViewerPullProgress;
use super::session::{self, SavedAs};
use super::session_test_support::{create_test_dir, write_test_file};
use super::{
    ArchiveFailureKind, PendingOpen, RowText, SearchMode, SeekTarget, ViewerError, ViewerOpenResult,
    render_markdown_session, resolve_markdown_link,
};
use crate::test_support::wait_until;

/// Masks what's random per run: session ids, media tokens, and the tokens inside a
/// rendered document's image markers.
fn mask(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                if matches!(key.as_str(), "sessionId" | "mediaToken" | "token") && inner.is_string() {
                    *inner = Value::String("<masked>".into());
                } else {
                    mask(inner);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask),
        Value::String(text) => {
            let masked = regex::Regex::new("(local|remote):[0-9a-f]{32}")
                .expect("a valid pattern")
                .replace_all(text, "$1:<masked>")
                .into_owned();
            *text = masked;
        }
        _ => {}
    }
}

#[track_caller]
fn pin(actual: impl Serialize, expected: &str) {
    let mut value = serde_json::to_value(actual).expect("serializes");
    mask(&mut value);
    assert!(!expected.is_empty(), "no pin yet; the answer today is {value}");
    assert_eq!(value.to_string(), expected);
}

fn open(path: &Path) -> ViewerOpenResult {
    session::open_session(path.to_str().expect("utf-8 path"), "root").expect("opens")
}

fn open_as_text(path: &Path) -> ViewerOpenResult {
    session::open_session_as_text(path.to_str().expect("utf-8 path"), "root").expect("opens")
}

fn write_bytes(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, bytes).expect("write the fixture");
    file
}

/// A valid 1x1 PNG, so the dimensions read back.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I',
    b'D', b'A', b'T', 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D,
    0xB0, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
];

/// A CRLF line, a line with a character outside the BMP (two UTF-16 units), and a last
/// line with no newline.
const TEXT: &str = "one\r\ntwo 😀 two\nthree";

#[test]
fn a_small_text_file_opens_with_this_answer() {
    let dir = create_test_dir("wire_text");
    let result = open(&write_test_file(&dir, "notes.txt", TEXT));
    pin(
        &result,
        r##"{"sessionId":"<masked>","fileName":"notes.txt","totalBytes":23,"totalLines":3,"estimatedTotalRows":3,"backendType":"fullLoad","capabilities":{"supportsLineSeek":true,"supportsByteSeek":true,"supportsFractionSeek":true,"knowsTotalLines":true},"initialLines":{"rows":[{"text":"one\r","byteOffset":0,"continues":false,"lineNumber":0},{"text":"two 😀 two","byteOffset":5,"continues":false,"lineNumber":1},{"text":"three","byteOffset":18,"continues":false,"lineNumber":2}],"firstRowNumber":0,"byteOffset":0,"endByteOffset":23,"end":"endOfFile","totalRows":{"kind":"exact","rows":3},"totalBytes":23},"isIndexing":false,"encoding":"utf8","kind":"text","mediaToken":null,"mediaDimensions":null,"mediaFromVolume":false}"##,
    );
    session::close_session(&result.session_id).expect("close");
}

#[test]
fn a_large_text_file_opens_on_byte_seek_with_this_answer() {
    let dir = create_test_dir("wire_large");
    let line = "0123456789abcdef\n";
    let body = line.repeat(1024 * 1024 / line.len() + 10);
    let result = open(&write_test_file(&dir, "big.log", &body));
    let mut head = serde_json::to_value(&result).expect("serializes");
    // The first chunk is hundreds of rows; its first two and its framing are the shape.
    if let Some(rows) = head.pointer_mut("/initialLines/rows").and_then(Value::as_array_mut) {
        rows.truncate(2);
    }
    pin(
        head,
        r##"{"sessionId":"<masked>","fileName":"big.log","totalBytes":1048730,"totalLines":null,"estimatedTotalRows":61690,"backendType":"byteSeek","capabilities":{"supportsLineSeek":false,"supportsByteSeek":true,"supportsFractionSeek":true,"knowsTotalLines":false},"initialLines":{"rows":[{"text":"0123456789abcdef","byteOffset":0,"continues":false,"lineNumber":0},{"text":"0123456789abcdef","byteOffset":17,"continues":false,"lineNumber":1}],"firstRowNumber":0,"byteOffset":0,"endByteOffset":3400,"end":"countReached","totalRows":{"kind":"estimated","rows":61690},"totalBytes":1048730},"isIndexing":true,"encoding":"utf8","kind":"text","mediaToken":null,"mediaDimensions":null,"mediaFromVolume":false}"##,
    );
    session::close_session(&result.session_id).expect("close");
}

#[test]
fn media_and_markdown_and_forced_text_open_with_these_answers() {
    let dir = create_test_dir("wire_kinds");
    let image = open(&write_bytes(&dir, "pixel.png", PNG));
    pin(
        &image,
        r##"{"sessionId":"<masked>","fileName":"pixel.png","totalBytes":70,"totalLines":0,"estimatedTotalRows":0,"backendType":"fullLoad","capabilities":{"supportsLineSeek":false,"supportsByteSeek":false,"supportsFractionSeek":false,"knowsTotalLines":true},"initialLines":{"rows":[],"firstRowNumber":0,"byteOffset":0,"endByteOffset":0,"end":"endOfFile","totalRows":{"kind":"exact","rows":0},"totalBytes":70},"isIndexing":false,"encoding":"utf8","kind":"image","mediaToken":"<masked>","mediaDimensions":{"width":1,"height":1},"mediaFromVolume":false}"##,
    );
    let pdf = open(&write_bytes(&dir, "doc.pdf", b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n"));
    pin(
        &pdf,
        r##"{"sessionId":"<masked>","fileName":"doc.pdf","totalBytes":29,"totalLines":0,"estimatedTotalRows":0,"backendType":"fullLoad","capabilities":{"supportsLineSeek":false,"supportsByteSeek":false,"supportsFractionSeek":false,"knowsTotalLines":true},"initialLines":{"rows":[],"firstRowNumber":0,"byteOffset":0,"endByteOffset":0,"end":"endOfFile","totalRows":{"kind":"exact","rows":0},"totalBytes":29},"isIndexing":false,"encoding":"utf8","kind":"pdf","mediaToken":"<masked>","mediaDimensions":null,"mediaFromVolume":false}"##,
    );
    let markdown_file = write_test_file(&dir, "README.md", "# Title\n\nSee [the notes](notes.md#part).\n");
    let markdown = open(&markdown_file);
    pin(
        &markdown,
        r##"{"sessionId":"<masked>","fileName":"README.md","totalBytes":41,"totalLines":0,"estimatedTotalRows":0,"backendType":"fullLoad","capabilities":{"supportsLineSeek":false,"supportsByteSeek":false,"supportsFractionSeek":false,"knowsTotalLines":true},"initialLines":{"rows":[],"firstRowNumber":0,"byteOffset":0,"endByteOffset":0,"end":"endOfFile","totalRows":{"kind":"exact","rows":0},"totalBytes":41},"isIndexing":false,"encoding":"utf8","kind":"markdown","mediaToken":null,"mediaDimensions":null,"mediaFromVolume":false}"##,
    );
    let forced = open_as_text(&markdown_file);
    pin(
        &forced,
        r##"{"sessionId":"<masked>","fileName":"README.md","totalBytes":41,"totalLines":4,"estimatedTotalRows":4,"backendType":"fullLoad","capabilities":{"supportsLineSeek":true,"supportsByteSeek":true,"supportsFractionSeek":true,"knowsTotalLines":true},"initialLines":{"rows":[{"text":"# Title","byteOffset":0,"continues":false,"lineNumber":0},{"text":"","byteOffset":8,"continues":false,"lineNumber":1},{"text":"See [the notes](notes.md#part).","byteOffset":9,"continues":false,"lineNumber":2},{"text":"","byteOffset":41,"continues":false,"lineNumber":3}],"firstRowNumber":0,"byteOffset":0,"endByteOffset":41,"end":"endOfFile","totalRows":{"kind":"exact","rows":4},"totalBytes":41},"isIndexing":false,"encoding":"utf8","kind":"text","mediaToken":null,"mediaDimensions":null,"mediaFromVolume":false}"##,
    );
    for result in [image, pdf, markdown, forced] {
        session::close_session(&result.session_id).expect("close");
    }
}

#[test]
fn the_log_view_opens_with_this_answer() {
    let dir = create_test_dir("wire_log");
    let file = write_test_file(&dir, "cmdr.log", "INFO plain line\n");
    let result = session::open_for_window(
        file.to_str().expect("utf-8 path"),
        "root",
        "",
        false,
        RowText::LogDisplay,
        &PendingOpen::new(),
    )
    .expect("opens");
    pin(
        &result,
        r##"{"sessionId":"<masked>","fileName":"cmdr.log","totalBytes":16,"totalLines":2,"estimatedTotalRows":2,"backendType":"fullLoad","capabilities":{"supportsLineSeek":true,"supportsByteSeek":true,"supportsFractionSeek":true,"knowsTotalLines":true},"initialLines":{"rows":[{"text":"INFO plain line","byteOffset":0,"continues":false,"lineNumber":0},{"text":"","byteOffset":16,"continues":false,"lineNumber":1}],"firstRowNumber":0,"byteOffset":0,"endByteOffset":16,"end":"endOfFile","totalRows":{"kind":"exact","rows":2},"totalBytes":16},"isIndexing":false,"encoding":"utf8","kind":"text","mediaToken":null,"mediaDimensions":null,"mediaFromVolume":false}"##,
    );
    session::close_session(&result.session_id).expect("close");
}

#[test]
fn rows_bytes_status_and_encodings_answer_these_shapes() {
    let dir = create_test_dir("wire_rows");
    let result = open(&write_test_file(&dir, "notes.txt", TEXT));
    let sid = result.session_id.clone();
    let chunk = session::get_lines(&sid, SeekTarget::Row(1), 5, &AtomicBool::new(false)).expect("rows");
    pin(
        &chunk,
        r##"{"rows":[{"text":"two 😀 two","byteOffset":5,"continues":false,"lineNumber":1},{"text":"three","byteOffset":18,"continues":false,"lineNumber":2}],"firstRowNumber":1,"byteOffset":5,"endByteOffset":23,"end":"endOfFile","totalRows":{"kind":"exact","rows":3},"totalBytes":23}"##,
    );
    pin(session::get_bytes(&sid, 3, 4).expect("bytes"), r##"[13,10,116,119]"##);
    pin(
        session::get_session_status(&sid).expect("status"),
        r##"{"backendType":"fullLoad","isIndexing":false,"totalRows":{"kind":"exact","rows":3},"totalLines":3}"##,
    );
    pin(
        session::get_encoding_options(&sid).expect("options"),
        r##"{"current":"utf8","detected":"utf8","all":[{"encoding":"utf8","label":"UTF-8","group":"unicode"},{"encoding":"utf8WithBom","label":"UTF-8 with BOM","group":"unicode"},{"encoding":"utf16Le","label":"UTF-16 LE","group":"unicode"},{"encoding":"utf16Be","label":"UTF-16 BE","group":"unicode"},{"encoding":"windows1252","label":"Western (Windows-1252)","group":"western"},{"encoding":"iso8859_1","label":"Western (ISO-8859-1)","group":"western"},{"encoding":"macRoman","label":"Western (Mac Roman)","group":"western"},{"encoding":"usAscii","label":"US-ASCII","group":"western"}]}"##,
    );
    session::close_session(&sid).expect("close");
}

fn search_to_the_end(sid: &str, query: &str, mode: SearchMode) -> session::SearchPollResult {
    session::search_start(sid, query.to_string(), mode).expect("search starts");
    let mut last = None;
    wait_until(Duration::from_secs(5), "the search finishes", || {
        let poll = session::search_poll(sid, 0).expect("poll");
        let done = !matches!(poll.status, super::session_store::SearchStatus::Running);
        last = Some(poll);
        done
    });
    last.expect("polled at least once")
}

#[test]
fn search_answers_these_shapes_with_utf16_columns() {
    let dir = create_test_dir("wire_search");
    let result = open(&write_test_file(&dir, "notes.txt", TEXT));
    let sid = result.session_id.clone();
    let literal = SearchMode {
        use_regex: false,
        case_sensitive: false,
    };
    // "two" after the emoji sits at UTF-16 column 7 (the emoji is two units).
    pin(
        search_to_the_end(&sid, "two", literal),
        r##"{"status":{"status":"done"},"newMatches":[{"row":1,"column":0,"length":3,"byteOffset":5},{"row":1,"column":7,"length":3,"byteOffset":5}],"totalMatchCount":2,"totalBytes":23,"bytesScanned":23,"matchLimitReached":false}"##,
    );
    let regex = SearchMode {
        use_regex: true,
        case_sensitive: true,
    };
    pin(
        search_to_the_end(&sid, "e$", regex),
        r##"{"status":{"status":"done"},"newMatches":[{"row":2,"column":4,"length":1,"byteOffset":18}],"totalMatchCount":1,"totalBytes":23,"bytesScanned":23,"matchLimitReached":false}"##,
    );
    // The message is the regex crate's own wording, which a dependency bump may reword;
    // the shape around it is the wire.
    let without_wording = |poll: session::SearchPollResult| {
        let mut value = serde_json::to_value(poll).expect("serializes");
        assert!(value.pointer("/status/message").is_some_and(Value::is_string));
        value["status"]["message"] = Value::String("<the regex's message>".into());
        value
    };
    pin(
        without_wording(search_to_the_end(&sid, "(", regex)),
        r##"{"status":{"status":"invalidQuery","message":"<the regex's message>"},"newMatches":[],"totalMatchCount":0,"totalBytes":23,"bytesScanned":0,"matchLimitReached":false}"##,
    );
    session::search_cancel(&sid).expect("cancel");
    pin(
        without_wording(session::search_poll(&sid, 0).expect("poll after cancel")),
        r##"{"status":{"status":"invalidQuery","message":"<the regex's message>"},"newMatches":[],"totalMatchCount":0,"totalBytes":23,"bytesScanned":0,"matchLimitReached":false}"##,
    );
    session::close_session(&sid).expect("close");
}

#[test]
fn a_markdown_session_renders_and_resolves_links_in_these_shapes() {
    let dir = create_test_dir("wire_markdown");
    std::fs::write(dir.join("pic.png"), PNG).expect("write the picture");
    let file = write_test_file(
        &dir,
        "README.md",
        "# Title\n\n![pic](pic.png) ![web](https://example.com/x.png)\n\n[notes](notes.md#part) [site](https://getcmdr.com) [top](#title)\n\n<script>alert(1)</script>\n",
    );
    let result = open(&file);
    let sid = result.session_id.clone();
    pin(
        render_markdown_session(&sid).expect("renders"),
        r##"{"html":"<h1 id=\"md-title\">Title</h1>\n<p><img data-md-image=\"local:<masked>\" alt=\"pic\"> <img data-md-image=\"remote:<masked>\" alt=\"web\"></p>\n<p><a href=\"notes.md#part\" rel=\"noopener noreferrer\">notes</a> <a href=\"https://getcmdr.com\" rel=\"noopener noreferrer\">site</a> <a href=\"#md-title\" rel=\"noopener noreferrer\">top</a></p>\n\n","hasRemoteImages":true}"##,
    );
    let mut file_link =
        serde_json::to_value(resolve_markdown_link(&sid, "notes.md#part").expect("file link")).expect("serializes");
    let resolved = file_link["path"].as_str().expect("a path").to_string();
    let beside = dir.join("notes.md");
    assert_eq!(Path::new(&resolved), beside, "the link resolves beside the document");
    file_link["path"] = Value::String("<dir>/notes.md".into());
    pin(
        file_link,
        r##"{"kind":"file","path":"<dir>/notes.md","elementId":"md-part"}"##,
    );
    pin(
        resolve_markdown_link(&sid, "https://getcmdr.com").expect("external link"),
        r##"{"kind":"external","url":"https://getcmdr.com/"}"##,
    );
    pin(
        resolve_markdown_link(&sid, "#title").expect("anchor link"),
        r##"{"kind":"anchor","elementId":"title"}"##,
    );
    pin(
        resolve_markdown_link(&sid, "/etc/passwd").expect("blocked link"),
        r##"{"kind":"blocked"}"##,
    );
    session::close_session(&sid).expect("close");
}

#[test]
fn every_viewer_error_crosses_in_this_shape() {
    let errors = [
        ViewerError::Io {
            message: "disk said no".to_string().into(),
        },
        ViewerError::NotFound {
            path: "/a/b.txt".into(),
        },
        ViewerError::IsDirectory,
        ViewerError::SessionNotFound {
            session_id: "abc".into(),
        },
        ViewerError::Cancelled,
        ViewerError::OutOfRange,
        ViewerError::TimedOut,
        ViewerError::StoppedResponding,
        ViewerError::TooLargeToPreview { size: 10, cap: 5 },
        ViewerError::DestinationIsReadOnly,
        ViewerError::Archive {
            failure: ArchiveFailureKind::NeedsPassword,
            message: "locked".to_string().into(),
        },
        ViewerError::ColdStorage,
        ViewerError::LinkTargetUnreachable,
        ViewerError::LinkNeedsConsent,
        ViewerError::NotEnoughSpace {
            needed: 7,
            available: 3,
        },
    ];
    // `sessionId` is masked by the helper, so the one id here is checked by hand.
    let raw = serde_json::to_string(&errors[3]).expect("serializes");
    assert_eq!(raw, r##"{"kind":"sessionNotFound","sessionId":"abc"}"##);
    pin(
        &errors,
        r##"[{"kind":"io","message":"disk said no"},{"kind":"notFound","path":"/a/b.txt"},{"kind":"isDirectory"},{"kind":"sessionNotFound","sessionId":"<masked>"},{"kind":"cancelled"},{"kind":"outOfRange"},{"kind":"timedOut"},{"kind":"stoppedResponding"},{"kind":"tooLargeToPreview","size":10,"cap":5},{"kind":"destinationIsReadOnly"},{"kind":"archive","failure":"needsPassword","message":"locked"},{"kind":"coldStorage"},{"kind":"linkTargetUnreachable"},{"kind":"linkNeedsConsent"},{"kind":"notEnoughSpace","needed":7,"available":3}]"##,
    );
}

#[test]
fn saves_and_events_cross_in_these_shapes() {
    pin(
        [
            SavedAs::Utf8,
            SavedAs::SourceEncoding {
                encoding: FileEncoding::Utf16Le,
            },
        ],
        r##"[{"kind":"utf8"},{"kind":"sourceEncoding","encoding":"utf16Le"}]"##,
    );
    let loads: Vec<ViewerMediaLoad> = [MediaLoadStage::Reading, MediaLoadStage::Done, MediaLoadStage::Failed]
        .into_iter()
        .map(|stage| ViewerMediaLoad {
            token: "t".into(),
            stage,
            bytes_done: 4,
            bytes_total: 9,
        })
        .collect();
    pin(
        &loads,
        r##"[{"token":"<masked>","stage":"reading","bytesDone":4,"bytesTotal":9},{"token":"<masked>","stage":"done","bytesDone":4,"bytesTotal":9},{"token":"<masked>","stage":"failed","bytesDone":4,"bytesTotal":9}]"##,
    );
    pin(
        [
            ViewerPullProgress {
                bytes_done: 1,
                bytes_total: Some(2),
            },
            ViewerPullProgress {
                bytes_done: 3,
                bytes_total: None,
            },
        ],
        r##"[{"bytesDone":1,"bytesTotal":2},{"bytesDone":3,"bytesTotal":null}]"##,
    );
    let (grew_name, grew) = session::file_changed_event("sid-1", "grew", Some(42));
    let (rotated_name, rotated) = session::file_changed_event("sid-1", "rotated", None);
    assert_eq!(grew_name, "viewer:file-changed:sid-1");
    assert_eq!(rotated_name, "viewer:file-changed:sid-1");
    pin(
        [grew, rotated],
        r##"[{"kind":"grew","newSize":42},{"kind":"rotated","newSize":null}]"##,
    );
}

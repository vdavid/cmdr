//! `inspect_file` against the sensitive-content gate, through the real pipeline: the path rules,
//! symlinks, archives, and every way a private key's text could otherwise slip through a window
//! or a `find`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::find::{find_hits_within, prescan_for_private_key};
use super::*;
use crate::file_viewer::encoding::FileEncoding;
use crate::file_viewer::headless::open_scan_backend;
use crate::file_viewer::{Matcher, SearchMode};
use crate::security_log::testing::capture_events;
use crate::security_log::{ProtectedContent, SecurityEvent};
use crate::test_support::TestDir;
use crate::tools::sensitive::Protected;
use cmdr_archive::test_fixtures::{build_zip, stored};

const BODY_FRAGMENT: &str = "b3BlbnNzaC1rZXktdjEAAAAA";
/// Past the viewer's full-load threshold (1 MiB).
const STREAMING_FILE_BYTES: usize = 2 * 1024 * 1024;

/// A key with `body_lines` lines between its armor, each holding [`BODY_FRAGMENT`].
fn key(body_lines: usize) -> String {
    let mut text = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
    for n in 0..body_lines {
        text.push_str(&format!("{BODY_FRAGMENT}{n:040}\n"));
    }
    text.push_str("-----END OPENSSH PRIVATE KEY-----\n");
    text
}

fn write(dir: &TestDir, name: &str, body: impl AsRef<[u8]>) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

fn window(start_line: usize, max_lines: usize) -> TextAsk {
    TextAsk::Window(WindowOpts {
        start_line,
        max_lines,
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

#[track_caller]
fn assert_refused(row: &FileRow, reason: Protected) {
    match row {
        FileRow::Protected { reason: got, note, .. } => {
            assert_eq!(*got, reason);
            assert!(!note.is_empty());
        }
        other => panic!("expected a protected row, got {other:?}"),
    }
}

/// What the plan calls done: a typed refusal for `~/.ssh/id_rsa` and `mcp.token`, in the shape
/// the model reads, with the security log told the tool and the reason and never the path.
#[tokio::test]
async fn an_ssh_key_and_the_mcp_token_are_refused_through_the_call() {
    let capture = capture_events();
    let token = crate::config::standalone_app_data_dir().unwrap().join("mcp.token");
    let result = run(&json!({ "paths": ["~/.ssh/id_rsa", token.to_str().unwrap()] }))
        .await
        .unwrap();
    let json: Value = serde_json::to_value(&result).unwrap();

    assert_eq!(json["files"][0]["status"], "protected");
    assert_eq!(json["files"][0]["reason"], "credentials");
    assert!(json["files"][0]["note"].is_string());
    assert!(json["files"][0]["path"].as_str().unwrap().ends_with("/.ssh/id_rsa"));
    assert_eq!(json["files"][1]["status"], "protected");
    assert_eq!(json["files"][1]["reason"], "cmdrData");
    assert_eq!((&json["total"], &json["returned"]), (&json!(2), &json!(2)));
    assert_eq!(json["truncated"], false);
    assert!(json.get("unanswered").is_none(), "a refusal IS the answer");

    assert_eq!(
        capture.events(),
        vec![
            SecurityEvent::ContentRefusalsStarted {
                tool: ContentTool::InspectFile,
                reason: ProtectedContent::Credentials,
            },
            SecurityEvent::ContentRefusalsStarted {
                tool: ContentTool::InspectFile,
                reason: ProtectedContent::CmdrData,
            },
        ]
    );
}

#[tokio::test]
async fn a_private_key_in_a_plain_folder_is_logged_with_its_reason() {
    let capture = capture_events();
    let dir = TestDir::new("inspect-key-logged");
    let copy = write(&dir, "backup.txt", key(3));

    run(&json!({ "paths": [copy.to_str().unwrap()] })).await.unwrap();

    assert_eq!(
        capture.events(),
        vec![SecurityEvent::ContentRefusalsStarted {
            tool: ContentTool::InspectFile,
            reason: ProtectedContent::PrivateKey,
        }]
    );
}

#[test]
fn a_symlink_into_a_protected_folder_is_refused_with_no_read() {
    let dir = TestDir::new("inspect-symlink");
    std::fs::create_dir(dir.join(".ssh")).unwrap();
    write(&dir, ".ssh/id_rsa", "ordinary words");
    let link = dir.join("notes.txt");
    std::os::unix::fs::symlink(dir.join(".ssh/id_rsa"), &link).unwrap();

    assert_refused(&inspect(&link, &window(1, 200)), Protected::Credentials);
}

/// Caught on the virtual path, before anything is extracted.
#[test]
fn an_archive_path_is_refused_before_it_materializes() {
    let dir = TestDir::new("inspect-archive-path");
    let zip = write(
        &dir,
        "foo.zip",
        build_zip(&[stored(".ssh/id_rsa", b"ordinary words".to_vec())]),
    );
    let inner = zip.join(".ssh/id_rsa");
    let extract = |_: &Path, _: &str, _: &AtomicBool| -> Result<Option<_>, ViewerError> {
        panic!("a protected archive path must not be extracted")
    };

    let row = inspect_path_with(
        inner.to_str().unwrap(),
        &window(1, 200),
        &AtomicBool::new(false),
        &extract,
    );

    assert_refused(&row, Protected::Credentials);
}

#[test]
fn a_key_inside_an_archive_entry_refuses_the_entry() {
    let dir = TestDir::new("inspect-archive-key");
    let zip = write(&dir, "foo.zip", build_zip(&[stored("backup.txt", key(3).into_bytes())]));
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );

    assert_refused(
        &inspect(&zip.join("backup.txt"), &window(1, 200)),
        Protected::PrivateKey,
    );
}

#[test]
fn a_window_over_a_copied_key_is_refused() {
    let dir = TestDir::new("inspect-key-window");
    let copy = write(&dir, "backup.txt", key(50));
    assert_refused(&inspect(&copy, &window(1, 200)), Protected::PrivateKey);
}

/// `text.rs` seeks straight to `startLine`, so line 2 is a key body with no `BEGIN`.
#[test]
fn starting_a_window_inside_the_key_does_not_skip_the_check() {
    let dir = TestDir::new("inspect-key-start-line");
    let copy = write(&dir, "backup.txt", key(50));
    assert_refused(&inspect(&copy, &window(2, 200)), Protected::PrivateKey);
}

/// Deep in the body with a small `maxLines`: the window holds neither armor line.
#[test]
fn a_small_window_deep_in_the_body_sees_neither_armor_line_and_is_still_refused() {
    let dir = TestDir::new("inspect-key-deep");
    let copy = write(&dir, "backup.txt", key(150));
    assert_refused(&inspect(&copy, &window(120, 1)), Protected::PrivateKey);
}

/// A head scan would stop at 64 KB.
#[test]
fn a_key_past_the_head_is_refused() {
    let dir = TestDir::new("inspect-key-past-head");
    let filler = "just some words on a line\n".repeat(4_000);
    let copy = write(&dir, "log.txt", format!("{filler}{}", key(10)));
    let key_body_line = 4_000 + 3;
    assert_refused(&inspect(&copy, &window(key_body_line, 1)), Protected::PrivateKey);
}

/// On a file big enough for the streaming backends, line index and scan alike.
#[test]
fn a_key_in_a_large_file_is_refused_on_the_streaming_backends() {
    let dir = TestDir::new("inspect-key-large");
    let filler = "just some words on a line\n".repeat(STREAMING_FILE_BYTES / 26);
    let lines_before = filler.lines().count();
    let copy = write(&dir, "log.txt", format!("{filler}{}", key(10)));

    assert_refused(&inspect(&copy, &window(lines_before + 3, 1)), Protected::PrivateKey);
    assert_refused(&inspect(&copy, &literal(BODY_FRAGMENT)), Protected::PrivateKey);
}

/// The raw head holds NUL-separated bytes, so only the decoded text shows the armor.
#[test]
fn a_utf16_key_is_refused() {
    let dir = TestDir::new("inspect-key-utf16");
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(key(50).encode_utf16().flat_map(u16::to_le_bytes));
    let copy = write(&dir, "backup.txt", bytes);

    assert_refused(&inspect(&copy, &window(1, 200)), Protected::PrivateKey);
    assert_refused(&inspect(&copy, &window(10, 1)), Protected::PrivateKey);
    assert_refused(&inspect(&copy, &literal(BODY_FRAGMENT)), Protected::PrivateKey);
}

/// `find` on a base64 fragment matches only body lines, so its hits show no armor.
#[test]
fn find_on_a_fragment_of_the_body_is_refused() {
    let dir = TestDir::new("inspect-key-find");
    let copy = write(&dir, "backup.txt", format!("some words\n{}", key(50)));

    assert_refused(&inspect(&copy, &literal(BODY_FRAGMENT)), Protected::PrivateKey);
    assert_refused(&inspect(&copy, &literal("some words")), Protected::PrivateKey);
    assert_refused(&inspect(&copy, &literal("no such needle")), Protected::PrivateKey);
}

/// Out of time before the pre-scan saw the file: nothing unscanned may come back.
#[test]
fn a_find_whose_prescan_was_stopped_returns_no_hit_past_it() {
    let dir = TestDir::new("inspect-key-stopped");
    let copy = write(&dir, "backup.txt", format!("some words\n{}", key(50)));
    let out_of_time = AtomicBool::new(true);

    let row = inspect_path(copy.to_str().unwrap(), &literal(BODY_FRAGMENT), &out_of_time);

    let FileRow::Ok(file) = row else {
        panic!("a stopped scan isn't a refusal: it found nothing yet, got {row:?}")
    };
    let Content::Text(text) = file.content else {
        panic!("expected a text row")
    };
    let hits = text.find.expect("a find row");
    assert_eq!(hits.returned_lines, 0);
    assert!(hits.scan_incomplete, "the model must know the file wasn't searched");
}

/// A hit past where the pre-scan stopped is dropped even if the user's scan reached it.
#[test]
fn hits_past_a_stopped_prescan_are_dropped() {
    let dir = TestDir::new("inspect-key-limit");
    let first_line = "needle one\n";
    let copy = write(&dir, "a.txt", format!("{first_line}needle two\nneedle three\n"));
    let backend = open_scan_backend(&copy, FileEncoding::Utf8).unwrap();
    let TextAsk::Find(matcher) = literal("needle") else {
        unreachable!()
    };

    let hits = find_hits_within(
        backend.as_ref(),
        &matcher,
        &AtomicBool::new(false),
        Some(first_line.len() as u64),
    )
    .unwrap();

    assert_eq!(hits.total_matches, 1);
    assert_eq!(hits.lines[0].text, "needle one");
    assert!(hits.scan_incomplete);
    assert_eq!(hits.bytes_scanned, Some(first_line.len() as u64));
}

#[test]
fn the_prescan_reports_what_it_saw() {
    let dir = TestDir::new("inspect-prescan");
    let clean = write(&dir, "clean.txt", "plain words\n");
    let keyed = write(&dir, "keyed.txt", key(1));
    let scan = |path: &Path, cancel: bool| {
        let backend = open_scan_backend(path, FileEncoding::Utf8).unwrap();
        prescan_for_private_key(backend.as_ref(), &AtomicBool::new(cancel)).unwrap()
    };

    assert_eq!(scan(&clean, false), find::KeyScan::Clean);
    assert_eq!(scan(&keyed, false), find::KeyScan::Found);
    assert_eq!(scan(&clean, true), find::KeyScan::StoppedAt(0));
}

/// The check costs an honest file nothing: same window, same hits.
#[test]
fn a_file_with_no_key_reads_as_before() {
    let dir = TestDir::new("inspect-no-key");
    let plain = write(&dir, "a.txt", "one\ntwo private key talk\nthree\nfour\n");

    let row = inspect(&plain, &window(2, 2));
    let FileRow::Ok(file) = &row else {
        panic!("expected an ok row, got {row:?}")
    };
    let Content::Text(text) = &file.content else {
        panic!("expected a text row")
    };
    let shown = text.window.as_ref().unwrap();
    assert_eq!(shown.content, "two private key talk\nthree");
    assert_eq!(shown.start_line, 2);

    let FileRow::Ok(file) = inspect(&plain, &literal("three")) else {
        panic!("expected an ok row")
    };
    let Content::Text(text) = file.content else {
        panic!("expected a text row")
    };
    assert_eq!(text.find.unwrap().total_matches, 1);
}

/// A protected row counts as answered, and `path()` joins it back to the request.
#[test]
fn a_protected_row_counts_toward_the_answer() {
    let paths = vec!["/a/.ssh/id_rsa".to_string(), "/a/b.txt".to_string()];
    let placed = PlacedRows {
        rows: vec![
            Some(InspectRow {
                volume_id: Some("root".into()),
                row: FileRow::protected(paths[0].clone(), Protected::Credentials),
            }),
            None,
        ],
        nearby: None,
    };

    let InspectResult::Ok {
        total,
        returned,
        unanswered,
        ..
    } = shape_ok(&paths, placed);

    assert_eq!((total, returned), (2, 1));
    assert_eq!(unanswered, vec!["/a/b.txt".to_string()]);
}

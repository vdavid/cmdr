//! The look inside, through the real `inspect_file` core over files the tests write.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

use ::exif::experimental::Writer;
use ::exif::{Field, In, Rational, Tag, Value};

use super::*;
use crate::test_support::TestDir;

const BUDGET: Duration = Duration::from_secs(5);

fn ascii(tag: Tag, text: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![text.as_bytes().to_vec()]),
    }
}

fn rational(tag: Tag, pairs: &[(u32, u32)]) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Rational(pairs.iter().map(|&(num, denom)| Rational { num, denom }).collect()),
    }
}

/// A JPEG with a camera's EXIF, shot at 59° 20' 45.31" N, 18° 3' 55.2" E.
fn photo_with_gps() -> Vec<u8> {
    let fields = [
        ascii(Tag::Make, "Canon"),
        ascii(Tag::Model, "Canon EOS R6"),
        ascii(Tag::DateTimeOriginal, "2026:08:29 18:42:07"),
        ascii(Tag::GPSLatitudeRef, "N"),
        rational(Tag::GPSLatitude, &[(59, 1), (20, 1), (4531, 100)]),
        ascii(Tag::GPSLongitudeRef, "E"),
        rational(Tag::GPSLongitude, &[(18, 1), (3, 1), (552, 10)]),
    ];
    let mut writer = Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut blob = std::io::Cursor::new(Vec::new());
    writer.write(&mut blob, false).expect("write the EXIF block");
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(blob.into_inner());
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE1];
    let len = u16::try_from(payload.len() + 2).expect("a small block");
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&[0xFF, 0xD9]);
    bytes
}

fn path_of(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 test path")
}

/// ❗ The core hands the GPS back (the agent's `inspect_file` gets it); the verdict's look
/// drops it before anything is built for a model.
#[tokio::test]
async fn a_photos_location_never_reaches_the_prompt() {
    let dir = TestDir::new("about-peek-gps");
    let photo = dir.as_ref().join("IMG_0001.jpg");
    std::fs::write(&photo, photo_with_gps()).expect("write the photo");

    let owned = path_of(&photo).to_string();
    let core = tokio::task::spawn_blocking(move || {
        crate::tools::read::inspect::inspect_path(&owned, &ask(), &AtomicBool::new(false))
    })
    .await
    .expect("the read ran");
    let core_json = serde_json::to_string(&core).expect("serializes");
    assert!(core_json.contains("gps"), "the core itself carries it: {core_json}");

    let (contents, peek) = look(path_of(&photo), inspect_reader(), BUDGET).await;
    assert_eq!(contents, VerdictContents::CameraDetails);
    let json = serde_json::to_string(&peek).expect("serializes");
    assert!(json.contains("Canon EOS R6"), "{json}");
    for leak in ["gps", "latitude", "longitude", "59.3", "18.06"] {
        assert!(!json.contains(leak), "{leak} leaked: {json}");
    }
}

/// A text file gives its first lines only: at most 40, at most 2,000 characters, and
/// says more follows.
#[tokio::test]
async fn a_text_file_gives_a_small_first_window() {
    let dir = TestDir::new("about-peek-text");
    let notes = dir.as_ref().join("notes.txt");
    let lines: Vec<String> = (1..=500).map(|n| format!("line {n} {}", "x".repeat(60))).collect();
    std::fs::write(&notes, lines.join("\n")).expect("write the notes");

    let (contents, peek) = look(path_of(&notes), inspect_reader(), BUDGET).await;
    assert_eq!(contents, VerdictContents::FirstLines);
    let Some(Peek::FirstLines { text, more }) = peek else {
        panic!("first lines expected");
    };
    assert!(more);
    assert!(text.chars().count() <= PEEK_CHARS);
    assert!(text.lines().count() <= PEEK_LINES);
    assert!(text.starts_with("line 1 "));
    assert!(!text.contains("line 41 "));
}

/// A file the sensitive gate protects is never read: the look says so, and the
/// security log hears it by tool and reason.
#[tokio::test]
async fn a_protected_file_answers_protected_with_nothing_inside() {
    let dir = TestDir::new("about-peek-key");
    let ssh = dir.as_ref().join(".ssh");
    std::fs::create_dir(&ssh).expect("make the folder");
    let key = ssh.join("id_ed25519");
    std::fs::write(&key, "-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n").expect("write the key");

    let (contents, peek) = look(path_of(&key), inspect_reader(), BUDGET).await;
    assert_eq!(contents, VerdictContents::Protected);
    assert_eq!(peek, None);
}

/// A read past its budget leaves the verdict with the facts alone, and its cancel flag
/// flips so the core stops where it can.
#[tokio::test]
async fn a_read_past_its_budget_gives_facts_only() {
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reads);
    let slow: ReadFn = Arc::new(move |path, _ask, cancel| {
        counted.fetch_add(1, Ordering::SeqCst);
        crate::test_support::wait_until(Duration::from_secs(5), "the look is cancelled", || {
            cancel.load(Ordering::SeqCst)
        });
        FileRow::Missing { path: path.to_string() }
    });
    let (contents, peek) = look("/tmp/whatever.txt", slow, Duration::from_millis(20)).await;
    assert_eq!((contents, peek), (VerdictContents::None, None));
    assert_eq!(reads.load(Ordering::SeqCst), 1, "one read, never a retry");
}

/// Control and direction-override characters never reach the model from a file's
/// text; newlines and tabs stay.
#[test]
fn the_window_keeps_lines_and_drops_marks_that_disguise_text() {
    let (text, cut) = cap("a\tb\nc\u{202E}d\u{0}e");
    assert_eq!(text, "a\tb\ncde");
    assert!(!cut);
    let (long, cut) = cap(&"y".repeat(PEEK_CHARS + 5));
    assert!(cut);
    assert_eq!(long.chars().count(), PEEK_CHARS);
}

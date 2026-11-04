//! The debug log's view (the app's log-marker reader under the viewer): rows, search, copy, and save all work on the text a person reads, while
//! a plain open of the same file shows it as stored.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use cmdr_fs::log_private::{CLOSER, LogKind, is_reserved};

use super::SearchStatus;
use super::app_viewer as viewer;
use super::{PendingOpen, RangeEnd, RowText, SaveProgress, SearchMode, SeekTarget};
use crate::test_support::{TestDir, wait_until};

/// Two lines: one with a file path marked the way a `{}` site writes it (the value's `{:?}`
/// body between raw markers, so the NFD accent reads `\u{301}` in the file), one plain.
fn marked_log() -> String {
    format!(
        "INFO  ·listing  opened {}/Users/me/Cafe\\u{{301}}.pdf{CLOSER} in 3ms\nplain line\n",
        LogKind::FilePath.opener()
    )
}

const SHOWN: &str = "INFO  ·listing  opened /Users/me/Cafe\u{301}.pdf in 3ms";

fn open(dir: &TestDir, rows: RowText) -> String {
    let file = dir.join("cmdr.log");
    std::fs::write(&file, marked_log()).expect("write the log");
    let path = file.to_str().expect("utf-8 path");
    viewer()
        .open_for_window(path, "root", "", false, rows, &PendingOpen::new())
        .expect("open")
        .session_id
}

fn first_row(sid: &str) -> String {
    let chunk = viewer()
        .get_lines(sid, SeekTarget::Row(0), 2, &AtomicBool::new(false))
        .expect("rows");
    chunk.texts()[0].clone()
}

fn row(row: u64, offset: u32) -> RangeEnd {
    RangeEnd::Row { row, offset }
}

#[test]
fn the_debug_log_view_shows_values_and_a_plain_open_shows_the_file() {
    let dir = TestDir::new("viewer_log_view_rows");
    let plain = open(&dir, RowText::Raw);
    assert!(
        first_row(&plain).chars().any(is_reserved),
        "a plain open shows the file as stored"
    );
    viewer().close_session(&plain).expect("close");

    let file = dir.join("cmdr.log");
    let opened = viewer()
        .open_for_window(
            file.to_str().expect("utf-8 path"),
            "root",
            "",
            false,
            RowText::LogDisplay,
            &PendingOpen::new(),
        )
        .expect("open");
    assert_eq!(opened.initial_lines.texts()[0], SHOWN, "the open's first rows too");
    assert_eq!(first_row(&opened.session_id), SHOWN);
    viewer().close_session(&opened.session_id).expect("close");
}

/// A match's column is counted on the shown row, so the highlight lands on what the user sees.
#[test]
fn search_counts_columns_on_the_shown_text() {
    let dir = TestDir::new("viewer_log_view_search");
    let sid = open(&dir, RowText::LogDisplay);
    let mode = SearchMode {
        use_regex: false,
        case_sensitive: true,
    };
    viewer().search_start(&sid, "in 3ms".to_string(), mode).expect("search");
    wait_until(Duration::from_secs(5), "the search to finish", || {
        matches!(viewer().search_poll(&sid, 0).expect("poll").status, SearchStatus::Done)
    });
    let found = viewer().search_poll(&sid, 0).expect("poll").new_matches;
    let expected: usize = SHOWN[..SHOWN.find("in 3ms").expect("in the row")]
        .encode_utf16()
        .count();
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].row, found[0].column), (0, expected));
    viewer().close_session(&sid).expect("close");
}

/// Copy and "save range" hand over the shown text: what the user selected is what they get.
#[test]
fn copy_and_save_take_the_shown_text() {
    let dir = TestDir::new("viewer_log_view_copy");
    let sid = open(&dir, RowText::LogDisplay);
    // To the end of the last row is to the end of the file, its final newline included.
    let whole = format!("{SHOWN}\nplain line\n");
    assert_eq!(
        viewer().read_range(&sid, 1, row(0, 0), row(1, 10)).expect("copy"),
        whole
    );

    let dest = dir.join("saved.log");
    viewer()
        .write_range_to_file(&sid, 2, row(0, 0), row(1, 10), &dest, &SaveProgress::new())
        .expect("save");
    assert_eq!(std::fs::read_to_string(&dest).expect("read the save"), whole);
    viewer().close_session(&sid).expect("close");
}

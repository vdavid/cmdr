//! Results (⌥⏎) over a real session: the names go out to a file, the user's
//! edits come back by old name, and a rename takes them only from a preview
//! that showed them.

use std::sync::Arc;

use super::error::MultiRenameError;
use super::plan::MultiRenameSpec;
use super::run::apply;
use super::run_test::{folder, names_on_disk, settled, strip_diacritics};
use super::session::{clear_names, close, open, preview_session, read_names, write_names};
use crate::file_system::write_operations::CollectorEventSink;

fn keep_names() -> MultiRenameSpec {
    MultiRenameSpec {
        remove_diacritics: false,
        ..strip_diacritics()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_typed_in_the_results_file_is_the_one_the_rename_uses() {
    let (dir, listing) = folder("multi-rename-results", &["a.txt", "b.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let preview = preview_session(&session.session_id, &keep_names()).expect("a preview");

    let path = write_names(&session.session_id, preview.preview_id).expect("the file writes");
    assert_eq!(
        std::fs::read_to_string(&path).expect("reads"),
        "a.txt\ta.txt\nb.txt\tb.txt\n"
    );
    std::fs::write(&path, "a.txt\ta.txt\nb.txt\tbee.txt\n").expect("the editor saves");

    assert_eq!(read_names(&session.session_id).expect("reads back"), 1);
    let edited = preview_session(&session.session_id, &keep_names()).expect("a preview");
    assert_eq!(edited.counts.ready, 1);
    assert_eq!(edited.rows[1].new_name, "bee.txt");
    assert!(edited.rows[1].edited);
    assert!(!edited.rows[0].edited);

    let events = Arc::new(CollectorEventSink::new());
    apply(events.clone(), session.session_id.clone(), edited.preview_id)
        .await
        .expect("the rename starts");
    settled(&events).await;

    assert_eq!(names_on_disk(&dir), vec!["a.txt", "bee.txt"]);
    close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_edit_read_back_after_the_preview_shown_never_sneaks_into_its_rename() {
    let (dir, listing) = folder("multi-rename-results-unseen", &["a.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let shown = preview_session(&session.session_id, &keep_names()).expect("a preview");
    let path = write_names(&session.session_id, shown.preview_id).expect("the file writes");
    std::fs::write(&path, "a.txt\tay.txt\n").expect("the editor saves");
    read_names(&session.session_id).expect("reads back");

    // The preview on screen still shows `a.txt` unchanged.
    let events = Arc::new(CollectorEventSink::new());
    let outcome = apply(events, session.session_id.clone(), shown.preview_id).await;

    assert!(matches!(outcome, Err(MultiRenameError::NothingToRename)));
    assert_eq!(names_on_disk(&dir), vec!["a.txt"]);
    close(&session.session_id);
}

#[test]
fn reading_back_before_results_wrote_anything_is_refused() {
    let (_dir, listing) = folder("multi-rename-results-none", &["a.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");

    assert!(matches!(
        read_names(&session.session_id),
        Err(MultiRenameError::NamesFileGone { .. })
    ));
    close(&session.session_id);
}

#[test]
fn a_results_file_the_user_deleted_reads_back_as_gone() {
    let (_dir, listing) = folder("multi-rename-results-deleted", &["a.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let preview = preview_session(&session.session_id, &keep_names()).expect("a preview");
    let path = write_names(&session.session_id, preview.preview_id).expect("the file writes");
    std::fs::remove_file(&path).expect("removes");

    assert!(matches!(
        read_names(&session.session_id),
        Err(MultiRenameError::NamesFileGone { .. })
    ));
    close(&session.session_id);
}

#[test]
fn using_the_settings_again_drops_the_edits_and_the_file() {
    let (_dir, listing) = folder("multi-rename-results-clear", &["a.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let preview = preview_session(&session.session_id, &keep_names()).expect("a preview");
    let path = write_names(&session.session_id, preview.preview_id).expect("the file writes");
    std::fs::write(&path, "a.txt\tay.txt\n").expect("the editor saves");
    read_names(&session.session_id).expect("reads back");

    clear_names(&session.session_id).expect("clears");

    let again = preview_session(&session.session_id, &keep_names()).expect("a preview");
    assert_eq!(again.rows[0].new_name, "a.txt");
    assert!(!again.rows[0].edited);
    assert!(!path.exists(), "the file goes with the edits");
    assert!(matches!(
        read_names(&session.session_id),
        Err(MultiRenameError::NamesFileGone { .. })
    ));
    close(&session.session_id);
}

#[test]
fn closing_the_sheet_removes_its_results_file() {
    let (_dir, listing) = folder("multi-rename-results-close", &["a.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let preview = preview_session(&session.session_id, &keep_names()).expect("a preview");
    let path = write_names(&session.session_id, preview.preview_id).expect("the file writes");

    close(&session.session_id);

    assert!(!path.exists());
}

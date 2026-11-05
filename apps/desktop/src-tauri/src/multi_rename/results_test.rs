//! Results (⌥⏎) over a real session: the names go out to a file, the user's
//! edits come back by old name, and a rename takes them only from a preview
//! that showed them.

use std::sync::Arc;

use super::app_files::results_path;
use super::error::MultiRenameError;
use super::plan::MultiRenameSpec;
use super::run::apply;
use super::run_test::{folder, names_on_disk, settled, strip_diacritics};
use super::session::{clear_names, close, open, preview_session, read_names, write_names};
use cmdr_operations::testing::CollectorEventSink;

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

/// Results with nowhere to write (here a folder squats on the file's name; a host
/// with no cache folder answers the same) is a typed refusal the sheet words, and
/// the sheet goes on: it previews, reads back nothing, and renames.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn results_with_nowhere_to_write_is_refused_and_the_sheet_goes_on() {
    let (dir, listing) = folder("multi-rename-results-nowhere", &["a.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let path = results_path(&session.session_id).expect("tests have a cache folder");
    let results_dir = path.parent().expect("the file sits in a folder");
    std::fs::create_dir_all(results_dir.parent().expect("the cache folder")).expect("the cache folder");
    // Private, as Results makes it, so the squatter below is what refuses.
    let _ = std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(), 0o700).create(results_dir);
    std::fs::create_dir(&path).expect("a folder at the Results file's name");
    let preview = preview_session(&session.session_id, &keep_names()).expect("a preview");

    let refused = write_names(&session.session_id, preview.preview_id);

    assert!(
        matches!(refused, Err(MultiRenameError::CouldntWriteNames { .. })),
        "{refused:?}"
    );
    assert!(matches!(
        read_names(&session.session_id),
        Err(MultiRenameError::NamesFileGone { .. })
    ));
    let renamed = MultiRenameSpec {
        name_mask: "[N]-done".to_string(),
        ..keep_names()
    };
    let preview = preview_session(&session.session_id, &renamed).expect("still previews");
    assert_eq!(preview.counts.ready, 1);
    let events = Arc::new(CollectorEventSink::new());
    apply(events.clone(), session.session_id.clone(), preview.preview_id)
        .await
        .expect("still renames");
    settled(&events).await;
    assert_eq!(names_on_disk(&dir), vec!["a-done.txt"]);
    close(&session.session_id);
    std::fs::remove_dir(&path).expect("the squatting folder goes");
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

//! Moving a row in a real session's rename order (⇧↑ / ⇧↓ in the sheet's list): the
//! counter follows the new order, and the preview on screen stops being one apply can
//! start from, since it no longer says what the session renames.

use std::sync::Arc;

use super::app_host::app_multi_rename;
use super::error::MultiRenameError;
use super::plan::MultiRenameSpec;
use super::run_test::{folder, names_on_disk, settled, strip_diacritics};
use super::session::{PreviewFilter, RowMove, TakenChars};
use cmdr_operations::testing::CollectorEventSink;

/// `1 a.txt`, `2 b.txt`, …: the counter shows the rename order.
fn numbered() -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: "[C] [N]".to_string(),
        remove_diacritics: false,
        ..strip_diacritics()
    }
}

fn new_names(rows: &[super::plan::PreviewRow]) -> Vec<&str> {
    rows.iter().map(|row| row.new_name.as_str()).collect()
}

#[test]
fn a_moved_row_takes_its_neighbors_place_and_the_counter_follows() {
    let (_dir, listing) = folder("multi-rename-move", &["a.txt", "b.txt", "c.txt"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let before = app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");
    assert_eq!(new_names(&before.rows), vec!["1 a.txt", "2 b.txt", "3 c.txt"]);

    assert_eq!(
        app_multi_rename()
            .move_row(&session.session_id, 2, RowMove::Up)
            .expect("it moves"),
        1
    );
    let after = app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");
    assert_eq!(new_names(&after.rows), vec!["1 a.txt", "2 c.txt", "3 b.txt"]);
    assert_eq!(after.rows[1].row, 1);

    assert_eq!(
        app_multi_rename()
            .move_row(&session.session_id, 0, RowMove::Down)
            .expect("it moves"),
        1
    );
    let again = app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");
    assert_eq!(new_names(&again.rows), vec!["1 c.txt", "2 a.txt", "3 b.txt"]);
    app_multi_rename().close(&session.session_id);
}

#[test]
fn at_either_end_the_row_stays_and_the_preview_still_holds() {
    let (_dir, listing) = folder("multi-rename-move-ends", &["a.txt", "b.txt"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let shown = app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");

    assert_eq!(
        app_multi_rename()
            .move_row(&session.session_id, 0, RowMove::Up)
            .expect("a no-op"),
        0
    );
    assert_eq!(
        app_multi_rename()
            .move_row(&session.session_id, 1, RowMove::Down)
            .expect("a no-op"),
        1
    );

    let rows = app_multi_rename()
        .page(&session.session_id, shown.preview_id, 0, 10, PreviewFilter::All)
        .expect("still pages");
    assert_eq!(new_names(&rows), vec!["1 a.txt", "2 b.txt"]);
    app_multi_rename().close(&session.session_id);
}

#[test]
fn a_row_past_the_end_is_refused() {
    let (_dir, listing) = folder("multi-rename-move-past", &["a.txt"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");

    assert!(matches!(
        app_multi_rename().move_row(&session.session_id, 1, RowMove::Up),
        Err(MultiRenameError::Internal { .. })
    ));
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_retires_the_preview_on_screen_so_apply_waits_for_the_next() {
    let (dir, listing) = folder("multi-rename-move-apply", &["a.txt", "b.txt"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let shown = app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");

    app_multi_rename()
        .move_row(&session.session_id, 1, RowMove::Up)
        .expect("it moves");

    assert!(matches!(
        app_multi_rename().page(&session.session_id, shown.preview_id, 0, 10, PreviewFilter::All),
        Err(MultiRenameError::PreviewOutOfDate)
    ));
    let events = Arc::new(CollectorEventSink::new());
    let refused = app_multi_rename()
        .apply(events.clone(), session.session_id.clone(), shown.preview_id)
        .await;
    assert!(matches!(refused, Err(MultiRenameError::PreviewOutOfDate)));
    assert_eq!(names_on_disk(&dir), vec!["a.txt", "b.txt"]);

    let next = app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");
    app_multi_rename()
        .apply(events.clone(), session.session_id.clone(), next.preview_id)
        .await
        .expect("the rename starts");
    settled(&events).await;
    assert_eq!(names_on_disk(&dir), vec!["1 b.txt", "2 a.txt"]);
    app_multi_rename().close(&session.session_id);
}

#[test]
fn moving_in_a_closed_session_says_so() {
    assert!(matches!(
        app_multi_rename().move_row("multi-rename-never-opened", 0, RowMove::Down),
        Err(MultiRenameError::SessionClosed)
    ));
}

#[test]
fn range_samples_say_what_a_range_takes_from_each_file_the_preview_lists() {
    let (dir, listing) = folder("multi-rename-range-samples", &["Beach day.jpg", "IMG_2041.jpg", "x.md"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");

    // The rows the preview lists, in its order: the listing sorts them by name.
    let samples = app_multi_rename()
        .range_samples(&session.session_id, "N2-5", &[0, 1, 2])
        .expect("samples");
    let taken: Vec<(&str, Option<(usize, usize)>)> = samples
        .iter()
        .map(|s| (s.text.as_str(), s.taken.map(|t| (t.first, t.last))))
        .collect();
    assert_eq!(
        taken,
        vec![("Beach day", Some((1, 4))), ("IMG_2041", Some((1, 4))), ("x", None)]
    );
    assert_eq!(samples.iter().map(|s| s.row).collect::<Vec<_>>(), vec![0, 1, 2]);

    // The other fields, and a row past the end left out.
    let extension = app_multi_rename()
        .range_samples(&session.session_id, "E-2-", &[2, 7])
        .expect("samples");
    assert_eq!(extension.len(), 1);
    assert_eq!(extension[0].text, "md");
    assert_eq!(extension[0].taken, Some(TakenChars { first: 0, last: 1 }));
    let folder_name = dir.file_name().expect("a name").to_string_lossy().into_owned();
    let parent = app_multi_rename()
        .range_samples(&session.session_id, "P", &[0])
        .expect("samples");
    assert_eq!(parent[0].text, folder_name);
    app_multi_rename().close(&session.session_id);
}

#[test]
fn range_samples_refuse_a_token_that_isnt_a_range_and_wait_for_a_moved_rows_preview() {
    let (_dir, listing) = folder("multi-rename-range-refusals", &["a.txt", "b.txt"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    app_multi_rename()
        .preview(&session.session_id, &numbered())
        .expect("a preview");

    assert!(matches!(
        app_multi_rename().range_samples(&session.session_id, "C10", &[0]),
        Err(MultiRenameError::Spec { .. })
    ));
    app_multi_rename()
        .move_row(&session.session_id, 1, RowMove::Up)
        .expect("it moves");
    assert!(matches!(
        app_multi_rename().range_samples(&session.session_id, "N", &[0]),
        Err(MultiRenameError::PreviewOutOfDate)
    ));
    app_multi_rename().close(&session.session_id);
}

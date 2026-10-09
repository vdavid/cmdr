//! A session against a real folder: the pane's selection goes in, renamed files
//! come out on disk, through the bulk-rename executor.

use std::path::Path;
use std::sync::Arc;

use super::error::MultiRenameError;
use super::plan::{MultiRenameSpec, RowStatus};
use super::run::apply;
use super::session::{FIRST_PAGE, PreviewFilter, close, is_open, open, page, preview_session};
use super::transform::CaseChange;
use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::listing::caching_test_support::{TestListing, TestListingGuard};
use crate::file_system::listing::list_directory_core;
use crate::file_system::write_operations::CollectorEventSink;
use crate::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use crate::test_support::{TestDir, wait_until_async};

fn strip_diacritics() -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: "[N]".to_string(),
        extension_mask: "[E]".to_string(),
        search: String::new(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        include_extension: false,
        regex: false,
        substitute: false,
        case: CaseChange::Unchanged,
        remove_diacritics: true,
    }
}

/// A scratch folder holding `names`, and a cached listing of it that shows every row.
fn folder(tag: &str, names: &[&str]) -> (TestDir, TestListingGuard) {
    let dir = TestDir::new(tag);
    for name in names {
        std::fs::write(dir.join(name), b"x").expect("scratch dir is writable");
    }
    let listing = TestListing::new()
        .volume("root")
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir).expect("the dir lists"))
        .insert(tag);
    (dir, listing)
}

fn names_on_disk(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("the dir reads")
        .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Puts `name` on disk and into the cached listing at row 0, the way a watcher
/// would land a file that sorts before everything else.
fn land_file_on_top(listing: &TestListingGuard, dir: &Path, name: &str) {
    std::fs::write(dir.join(name), b"x").expect("scratch dir is writable");
    let entry = list_directory_core(dir)
        .expect("the dir lists")
        .into_iter()
        .find(|e| e.name == name)
        .expect("the new file lists");
    let mut cache = LISTING_CACHE.write_ignore_poison();
    let cached = cache.get_mut(listing.id()).expect("the listing is cached");
    cached.entries_mut().insert(0, entry);
    cached.advance_sequence();
}

/// Takes `name` off disk and out of the cached listing.
fn remove_file(listing: &TestListingGuard, dir: &Path, name: &str) {
    std::fs::remove_file(dir.join(name)).expect("the file removes");
    let mut cache = LISTING_CACHE.write_ignore_poison();
    let cached = cache.get_mut(listing.id()).expect("the listing is cached");
    cached.entries_mut().retain(|e| e.name != name);
    cached.advance_sequence();
}

async fn settled(events: &CollectorEventSink) {
    wait_until_async(std::time::Duration::from_secs(10), "the rename to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_folder_comes_out_without_diacritics_and_the_rest_is_left_alone() {
    let (dir, listing) = folder("multi-rename-apply", &["Žádost o přezkum.pdf", "plán.txt", "plain.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    assert_eq!(session.count, 3);

    let preview = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    assert_eq!(preview.counts.ready, 2, "{preview:?}");
    assert_eq!(preview.counts.unchanged, 1);

    let events = Arc::new(CollectorEventSink::new());
    let started = apply(events.clone(), session.session_id.clone(), preview.preview_id)
        .await
        .expect("the rename starts");
    assert_eq!(started.renaming, 2);
    assert_eq!(started.swaps_left_out, 0);
    settled(&events).await;

    assert_eq!(
        names_on_disk(&dir),
        vec!["Zadost o prezkum.pdf", "plain.txt", "plan.txt"]
    );
    close(&session.session_id);
}

/// The data-safety one: the selection is three files, and a file that lands
/// above them while the sheet is open never takes the place of the third.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_appearing_above_the_selection_never_joins_it() {
    let (dir, listing) = folder("multi-rename-shift", &["b-ä.txt", "c-ö.txt", "d-ü.txt", "e-é.txt"]);
    let session = open(listing.id(), true, Some(&[0, 1, 2]), 0).expect("the session opens");

    land_file_on_top(&listing, &dir, "a-å.txt");

    let preview = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    let old: Vec<&str> = preview.rows.iter().map(|r| r.old_name.as_str()).collect();
    assert_eq!(old, vec!["b-ä.txt", "c-ö.txt", "d-ü.txt"]);

    let events = Arc::new(CollectorEventSink::new());
    let started = apply(events.clone(), session.session_id.clone(), preview.preview_id)
        .await
        .expect("the rename starts");
    assert_eq!(started.renaming, 3);
    settled(&events).await;

    assert_eq!(
        names_on_disk(&dir),
        vec!["a-å.txt", "b-a.txt", "c-o.txt", "d-u.txt", "e-é.txt"]
    );
    close(&session.session_id);
}

#[test]
fn a_selection_read_at_an_older_sequence_is_refused() {
    let (dir, listing) = folder("multi-rename-stale-selection", &["b.txt", "c.txt"]);
    land_file_on_top(&listing, &dir, "a.txt");

    assert!(matches!(
        open(listing.id(), true, Some(&[0]), 0),
        Err(MultiRenameError::SelectionChanged { .. })
    ));
}

#[test]
fn a_session_file_that_vanished_previews_as_missing() {
    let (dir, listing) = folder("multi-rename-vanished", &["a-ä.txt", "b-ö.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");

    remove_file(&listing, &dir, "a-ä.txt");

    let preview = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    assert_eq!(preview.rows.len(), 2);
    assert_eq!(preview.rows[0].old_name, "a-ä.txt");
    assert_eq!(preview.rows[0].status, RowStatus::Missing);
    assert_eq!(preview.rows[0].icon_id, None, "a gone file has no icon to show");
    assert_eq!(preview.rows[1].old_name, "b-ö.txt");
    assert_eq!(preview.rows[1].status, RowStatus::Ready);
    assert_eq!((preview.counts.ready, preview.counts.problems), (1, 1));
    close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_ready_starts_nothing() {
    let (_dir, listing) = folder("multi-rename-nothing", &["plain.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let preview = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");

    let events = Arc::new(CollectorEventSink::new());
    let outcome = apply(events, session.session_id.clone(), preview.preview_id).await;

    assert!(matches!(outcome, Err(MultiRenameError::NothingToRename)));
    close(&session.session_id);
}

#[test]
fn a_gone_listing_is_refused() {
    assert!(matches!(
        open("no-such-listing", true, None, 0),
        Err(MultiRenameError::Gone { .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_that_changed_since_the_preview_is_refused() {
    let (dir, listing) = folder("multi-rename-stale", &["plán.txt", "zoo-ü.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let preview = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    assert_eq!(preview.counts.ready, 2);

    // A file takes one of the new names after the user saw the preview.
    land_file_on_top(&listing, &dir, "plan.txt");

    let events = Arc::new(CollectorEventSink::new());
    let outcome = apply(events, session.session_id.clone(), preview.preview_id).await;

    assert!(matches!(outcome, Err(MultiRenameError::PreviewOutOfDate)));
    assert!(dir.join("plán.txt").exists(), "nothing was renamed");
    assert!(dir.join("zoo-ü.txt").exists(), "nothing was renamed");
    close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_a_newer_one_replaced_is_refused() {
    let (dir, listing) = folder("multi-rename-replaced", &["plán.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let seen = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    let newer = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    assert!(newer.preview_id > seen.preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let outcome = apply(events, session.session_id.clone(), seen.preview_id).await;

    assert!(matches!(outcome, Err(MultiRenameError::PreviewOutOfDate)));
    assert!(matches!(
        page(&session.session_id, seen.preview_id, 0, 10, PreviewFilter::All),
        Err(MultiRenameError::PreviewOutOfDate)
    ));
    assert!(dir.join("plán.txt").exists(), "nothing was renamed");
    close(&session.session_id);
}

#[test]
fn the_preview_answers_one_page_and_the_rest_pages_in() {
    let names: Vec<String> = (0..FIRST_PAGE + 50).map(|i| format!("f{i:04}.txt")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (_dir, listing) = folder("multi-rename-pages", &refs);
    let session = open(listing.id(), true, None, 0).expect("the session opens");

    let preview = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    assert_eq!(preview.rows.len(), FIRST_PAGE);
    assert_eq!(preview.counts.unchanged, FIRST_PAGE + 50);

    let rest = page(
        &session.session_id,
        preview.preview_id,
        FIRST_PAGE,
        1000,
        PreviewFilter::All,
    )
    .expect("a page");
    assert_eq!(rest.len(), 50);
    assert_eq!(rest[0].row, FIRST_PAGE);
    assert_eq!(rest[0].old_name, names[FIRST_PAGE]);
    close(&session.session_id);
}

#[test]
fn a_closed_session_answers_nothing() {
    let (_dir, listing) = folder("multi-rename-closed", &["a.txt"]);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    assert!(is_open(&session.session_id));

    close(&session.session_id);

    assert!(!is_open(&session.session_id));
    assert!(matches!(
        preview_session(&session.session_id, &strip_diacritics()),
        Err(MultiRenameError::SessionClosed)
    ));
}

#[test]
fn problems_only_pages_through_the_problem_rows_alone() {
    // Each `éN` strips to `eN`, which stays: five taken names among eleven rows.
    let mut names: Vec<String> = (0..5)
        .flat_map(|i| [format!("é{i}.txt"), format!("e{i}.txt")])
        .collect();
    names.push("zz.txt".to_string());
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (_dir, listing) = folder("multi-rename-problems-only", &refs);
    let session = open(listing.id(), true, None, 0).expect("the session opens");
    let preview = preview_session(&session.session_id, &strip_diacritics()).expect("a preview");
    assert_eq!(preview.counts.problems, 5);

    let all = page(&session.session_id, preview.preview_id, 0, 100, PreviewFilter::Problems).expect("a page");
    assert_eq!(all.len(), 5);
    assert!(all.iter().all(|r| r.status == RowStatus::TargetExists));
    assert!(all.windows(2).all(|w| w[0].row < w[1].row), "in rename order");

    let tail = page(&session.session_id, preview.preview_id, 2, 2, PreviewFilter::Problems).expect("a page");
    assert_eq!(tail, all[2..4].to_vec());
    close(&session.session_id);
}

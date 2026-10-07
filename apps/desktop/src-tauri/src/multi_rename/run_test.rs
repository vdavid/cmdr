//! Preview and apply against a real folder: the rows a pane shows go in, renamed
//! files come out on disk, through the bulk-rename executor.

use std::sync::Arc;

use super::plan::{MultiRenameSpec, RowStatus};
use super::run::{ExpectedRename, MultiRenameError, apply, preview_rows};
use super::transform::CaseChange;
use crate::file_system::listing::caching_test_support::TestListing;
use crate::file_system::listing::list_directory_core;
use crate::file_system::write_operations::CollectorEventSink;
use crate::ignore_poison::IgnorePoison;
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
        counter_start: 1,
        counter_step: 1,
        counter_digits: 1,
    }
}

fn expected(preview: &[super::plan::PreviewRow]) -> Vec<ExpectedRename> {
    preview
        .iter()
        .filter(|r| r.status == RowStatus::Ready)
        .map(|r| ExpectedRename {
            row: r.row,
            old_name: r.old_name.clone(),
            new_name: r.new_name.clone(),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_folder_comes_out_without_diacritics_and_the_rest_is_left_alone() {
    let dir = TestDir::new("multi-rename-apply");
    for name in ["Žádost o přezkum.pdf", "plán.txt", "plain.txt"] {
        std::fs::write(dir.join(name), b"x").expect("scratch dir is writable");
    }
    let listing = TestListing::new()
        .volume("root")
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir).expect("the dir lists"))
        .insert("multi-rename-apply");

    let preview = preview_rows(listing.id(), false, None, &strip_diacritics()).expect("a preview");
    let ready = preview.iter().filter(|r| r.status == RowStatus::Ready).count();
    assert_eq!(ready, 2, "{preview:?}");

    let events = Arc::new(CollectorEventSink::new());
    let started = apply(
        events.clone(),
        listing.id().to_string(),
        false,
        None,
        strip_diacritics(),
        expected(&preview),
    )
    .await
    .expect("the rename starts");
    assert_eq!(started.renaming, 2);
    wait_until_async(std::time::Duration::from_secs(10), "the rename to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;

    let mut names: Vec<String> = std::fs::read_dir(&*dir)
        .expect("the dir reads")
        .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["Zadost o prezkum.pdf", "plain.txt", "plan.txt"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_ready_starts_nothing() {
    let dir = TestDir::new("multi-rename-nothing");
    std::fs::write(dir.join("plain.txt"), b"x").expect("scratch dir is writable");
    let listing = TestListing::new()
        .volume("root")
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir).expect("the dir lists"))
        .insert("multi-rename-nothing");

    let events = Arc::new(CollectorEventSink::new());
    let outcome = apply(
        events,
        listing.id().to_string(),
        false,
        None,
        strip_diacritics(),
        Vec::new(),
    )
    .await;

    assert!(matches!(outcome, Err(MultiRenameError::NothingToRename)));
}

#[test]
fn a_gone_listing_is_refused() {
    assert!(matches!(
        preview_rows("no-such-listing", false, None, &strip_diacritics()),
        Err(MultiRenameError::Gone { .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_that_changed_since_the_preview_is_refused() {
    let dir = TestDir::new("multi-rename-stale");
    std::fs::write(dir.join("plán.txt"), b"x").expect("scratch dir is writable");
    let listing = TestListing::new()
        .volume("root")
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir).expect("the dir lists"))
        .insert("multi-rename-stale");
    let preview = preview_rows(listing.id(), false, None, &strip_diacritics()).expect("a preview");
    let mut seen = expected(&preview);
    // What the user saw differs from what the folder holds now.
    seen[0].old_name = "plán-old.txt".to_string();

    let events = Arc::new(CollectorEventSink::new());
    let outcome = apply(events, listing.id().to_string(), false, None, strip_diacritics(), seen).await;

    assert!(matches!(outcome, Err(MultiRenameError::PreviewOutOfDate)));
    assert!(dir.join("plán.txt").exists(), "nothing was renamed");
}

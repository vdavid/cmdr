//! Data-safety cells on S3: a failed merge that must not sweep the user's
//! folder, a failed move that must lose no byte, a delete bound to its preview,
//! a recursive delete that takes exactly the selection, an unanswerable source
//! probe, and a download stopped mid-file.
//!
//! The scenarios are backend-blind and live in `network_safety_test_support.rs`;
//! the cells stay here, named for the `s3_integration_` lane prefix, and
//! `s3_live_engine_test.rs` runs the same scenarios against real accounts. Not
//! here: `a_name_taken_mid_upload_is_never_replaced`, whose guard is a staged
//! landing's atomic rename. S3 writes the final key, and a writer landing
//! between the last no-overwrite check and the write's own completion is the
//! blind window `crates/cmdr-s3/DETAILS.md` § "No-overwrite writes" accepts.

use cmdr_s3::volume::testing::{GARAGE, S3Target, VERSITYGW};

use super::network_safety_test_support::{
    a_cancelled_download_leaves_nothing_behind, a_delete_of_a_non_empty_folder_takes_exactly_the_selection,
    a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree,
    a_failed_folder_copy_onto_a_users_folder_keeps_their_files, a_failed_folder_move_onto_a_users_folder_loses_no_byte,
    an_unknown_source_type_never_clears_a_server_folder,
};
use super::s3_transfer_integration_test::scenario;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_failed_folder_copy_onto_a_users_folder_keeps_their_files_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "failed-merge-copy",
        a_failed_folder_copy_onto_a_users_folder_keeps_their_files,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_failed_folder_copy_onto_a_users_folder_keeps_their_files_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "failed-merge-copy",
        a_failed_folder_copy_onto_a_users_folder_keeps_their_files,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_failed_folder_move_onto_a_users_folder_loses_no_byte_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "failed-merge-move",
        a_failed_folder_move_onto_a_users_folder_loses_no_byte,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_failed_folder_move_onto_a_users_folder_loses_no_byte_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "failed-merge-move",
        a_failed_folder_move_onto_a_users_folder_loses_no_byte,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "delete-preview",
        a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "delete-preview",
        a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_delete_of_a_non_empty_folder_takes_exactly_the_selection_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "delete-tree",
        a_delete_of_a_non_empty_folder_takes_exactly_the_selection,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_delete_of_a_non_empty_folder_takes_exactly_the_selection_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "delete-tree",
        a_delete_of_a_non_empty_folder_takes_exactly_the_selection,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_an_unknown_source_type_never_clears_a_server_folder_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "unknown-source-type",
        an_unknown_source_type_never_clears_a_server_folder,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_an_unknown_source_type_never_clears_a_server_folder_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "unknown-source-type",
        an_unknown_source_type_never_clears_a_server_folder,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_cancelled_download_leaves_nothing_behind_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "cancelled-download",
        a_cancelled_download_leaves_nothing_behind,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_cancelled_download_leaves_nothing_behind_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "cancelled-download",
        a_cancelled_download_leaves_nothing_behind,
    )
    .await;
}

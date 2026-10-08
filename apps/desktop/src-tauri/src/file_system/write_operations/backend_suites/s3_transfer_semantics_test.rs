//! What a transfer MEANS on S3: merges under every policy, moves in both
//! directions and within one bucket, same-bucket copies, and a move off the
//! bucket while its files change, through the app's own engine.
//!
//! The backend-blind scenarios live in `network_semantics_test_support.rs` and
//! `network_move_drift_test_support.rs`, shared with SFTP, SMB, and WebDAV. On
//! S3 they meet a backend that publishes every write whole, renames by copy,
//! and copies within an account on the server, so the same words prove
//! different machinery. The cells stay here, named for the `s3_integration_`
//! lane prefix; `s3_live_engine_test.rs` runs the same scenarios against real
//! accounts.

use cmdr_s3::volume::testing::{GARAGE, S3Target, VERSITYGW};

use super::network_move_drift_test_support::{
    a_file_added_mid_move_off_the_server_stays, a_file_saved_over_mid_move_off_the_server_stays,
};
use super::network_semantics_test_support::{
    a_copy_into_a_missing_nested_destination_makes_every_level,
    a_deep_clash_merge_under_overwrite_replaces_only_the_clash,
    a_deep_clash_merge_under_skip_keeps_every_dest_only_file, a_folder_moved_off_the_server_leaves_no_source,
    a_folder_moved_onto_the_server_leaves_no_source, a_move_merge_onto_the_server_spares_what_it_skipped,
    a_multi_megabyte_file_round_trips_byte_exact, a_rename_policy_lands_the_clash_beside_the_users_file,
    a_same_server_copy_duplicates_a_tree, a_same_server_move_merges_without_a_folder_prompt,
    a_same_server_move_without_a_clash_moves_the_folder_whole, many_files_at_full_concurrency_land_intact,
    overwrite_smaller_replaces_only_the_smaller_destination,
};
use super::s3_transfer_integration_test::scenario;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_deep_clash_merge_under_skip_keeps_every_dest_only_file_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "merge-skip",
        a_deep_clash_merge_under_skip_keeps_every_dest_only_file,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_deep_clash_merge_under_skip_keeps_every_dest_only_file_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "merge-skip",
        a_deep_clash_merge_under_skip_keeps_every_dest_only_file,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_deep_clash_merge_under_overwrite_replaces_only_the_clash_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "merge-overwrite",
        a_deep_clash_merge_under_overwrite_replaces_only_the_clash,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_deep_clash_merge_under_overwrite_replaces_only_the_clash_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "merge-overwrite",
        a_deep_clash_merge_under_overwrite_replaces_only_the_clash,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_rename_policy_lands_the_clash_beside_the_users_file_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "merge-rename",
        a_rename_policy_lands_the_clash_beside_the_users_file,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_rename_policy_lands_the_clash_beside_the_users_file_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "merge-rename",
        a_rename_policy_lands_the_clash_beside_the_users_file,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_overwrite_smaller_replaces_only_the_smaller_destination_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "overwrite-smaller",
        overwrite_smaller_replaces_only_the_smaller_destination,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_overwrite_smaller_replaces_only_the_smaller_destination_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "overwrite-smaller",
        overwrite_smaller_replaces_only_the_smaller_destination,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_move_merge_onto_the_server_spares_what_it_skipped_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "move-merge-onto",
        a_move_merge_onto_the_server_spares_what_it_skipped,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_move_merge_onto_the_server_spares_what_it_skipped_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "move-merge-onto",
        a_move_merge_onto_the_server_spares_what_it_skipped,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_folder_moved_onto_the_server_leaves_no_source_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "move-onto",
        a_folder_moved_onto_the_server_leaves_no_source,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_folder_moved_onto_the_server_leaves_no_source_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "move-onto",
        a_folder_moved_onto_the_server_leaves_no_source,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_folder_moved_off_the_server_leaves_no_source_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "move-off",
        a_folder_moved_off_the_server_leaves_no_source,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_folder_moved_off_the_server_leaves_no_source_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "move-off",
        a_folder_moved_off_the_server_leaves_no_source,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_file_saved_over_mid_move_off_the_server_stays_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "move-drift-saved",
        a_file_saved_over_mid_move_off_the_server_stays,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_file_saved_over_mid_move_off_the_server_stays_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "move-drift-saved",
        a_file_saved_over_mid_move_off_the_server_stays,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_file_added_mid_move_off_the_server_stays_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "move-drift-added",
        a_file_added_mid_move_off_the_server_stays,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_file_added_mid_move_off_the_server_stays_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "move-drift-added",
        a_file_added_mid_move_off_the_server_stays,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_same_server_move_merges_without_a_folder_prompt_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "same-server-move-merge",
        a_same_server_move_merges_without_a_folder_prompt,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_same_server_move_merges_without_a_folder_prompt_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "same-server-move-merge",
        a_same_server_move_merges_without_a_folder_prompt,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_same_server_move_without_a_clash_moves_the_folder_whole_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "same-server-move",
        a_same_server_move_without_a_clash_moves_the_folder_whole,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_same_server_move_without_a_clash_moves_the_folder_whole_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "same-server-move",
        a_same_server_move_without_a_clash_moves_the_folder_whole,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_same_server_copy_duplicates_a_tree_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "same-server-copy",
        a_same_server_copy_duplicates_a_tree,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_same_server_copy_duplicates_a_tree_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "same-server-copy",
        a_same_server_copy_duplicates_a_tree,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_copy_into_a_missing_nested_destination_makes_every_level_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "missing-nested",
        a_copy_into_a_missing_nested_destination_makes_every_level,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_copy_into_a_missing_nested_destination_makes_every_level_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "missing-nested",
        a_copy_into_a_missing_nested_destination_makes_every_level,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_multi_megabyte_file_round_trips_byte_exact_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "multi-megabyte",
        a_multi_megabyte_file_round_trips_byte_exact,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_multi_megabyte_file_round_trips_byte_exact_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "multi-megabyte",
        a_multi_megabyte_file_round_trips_byte_exact,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_many_files_at_full_concurrency_land_intact_on_versitygw() {
    scenario(
        &S3Target::Fixture(VERSITYGW),
        "many-files",
        many_files_at_full_concurrency_land_intact,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_many_files_at_full_concurrency_land_intact_on_garage() {
    scenario(
        &S3Target::Fixture(GARAGE),
        "many-files",
        many_files_at_full_concurrency_land_intact,
    )
    .await;
}

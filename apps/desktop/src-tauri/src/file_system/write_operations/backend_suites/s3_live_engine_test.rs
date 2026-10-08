//! The engine's S3 flows against real accounts: every scenario the Docker
//! cells run (`s3_transfer_*`, `s3_rename_*`, `s3_engine_*`), plus what only a
//! real provider has: a copy between two providers, an archived object, and
//! each provider's own request counts against the cost estimate.
//!
//! ❗ Every cell here skips unless `CMDR_S3_LIVE=1` and an account's variables
//! are set (`cmdr_s3::volume::testing::live`), so the unit lane runs them as
//! no-ops and never reaches an account. The runner:
//! `apps/desktop/test/s3-servers/live-engine.sh`, which goes through
//! `cargo test` one cell at a time (the nextest cap would cut a live upload).
//!
//! Each cell runs its flows on every account in turn, prints one
//! `LIVE [<account>] <flow>: ok in 3.2 s` (or `FAILED: …`) line per pair, removes
//! everything the run wrote on that account after each flow
//! (`S3Target::clean_run`), and fails at the end if any pair failed. Not named
//! for a lane prefix: these are by hand only.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cmdr_fs::volume::{Volume, VolumeError};
use cmdr_s3::volume::testing::{S3Target, Seed, object, self_describing_bytes};
use futures_util::FutureExt as _;

use super::super::types::VolumeCopyConfig;
use super::super::types::{ConflictResolution, WriteOperationError};
use super::network_move_drift_test_support::{
    a_file_added_mid_move_off_the_server_stays, a_file_saved_over_mid_move_off_the_server_stays,
};
use super::network_safety_test_support::{
    a_cancelled_download_leaves_nothing_behind, a_delete_of_a_non_empty_folder_takes_exactly_the_selection,
    a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree,
    a_failed_folder_copy_onto_a_users_folder_keeps_their_files, a_failed_folder_move_onto_a_users_folder_loses_no_byte,
    a_name_taken_mid_upload_is_never_replaced, an_unknown_source_type_never_clears_a_server_folder,
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
use super::network_transfer_test_support::{
    a_cancelled_upload_leaves_nothing_behind, a_directory_tree_lands_intact_off_the_server,
    a_directory_tree_lands_intact_on_the_server, a_pre_existing_destination_still_probes_each_name,
    an_overwrite_answer_replaces_the_destination_bytes, awkward_names_survive_a_round_trip, run_copy, start_copy,
    tree_fingerprint,
};
use super::s3_engine_integration_test::{
    a_cancel_mid_multipart_leaves_no_object_and_no_upload, a_cancel_with_rollback_takes_back_what_landed,
    a_cancelled_overwrite_keeps_the_original, a_finished_copy_onto_a_bucket_rolls_back,
    a_folder_of_1005_objects_deletes, a_paused_upload_resumes_and_lands, requests_sent_against_the_estimate,
};
use super::s3_rename_integration_test::{
    a_batch_with_a_folder_renames_as_one_move, a_big_file_renames_by_multipart_copy_keeping_its_date,
    a_bucket_bound_provider_streams_a_cross_bucket_copy, a_copy_between_two_buckets_runs_on_the_server,
    a_folder_of_1005_objects_renames_through_the_engine, a_paused_rename_resumes_and_lands,
    a_paused_then_cancelled_rename_keeps_the_source_whole,
};
use super::s3_transfer_integration_test::{
    a_directory_tree_lands_intact_off_a_bucket, a_large_object_copies_off_a_bucket_byte_for_byte,
    copying_between_buckets_lands_every_byte, copying_off_a_bucket_lands_every_byte,
    copying_onto_a_bucket_lands_every_byte_and_the_mtime, scenario,
};
use crate::file_system::volume::LocalPosixVolume;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;

const MIB: usize = 1024 * 1024;

/// What a flow needs from an account beyond the account itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Needs {
    Nothing,
    /// A second bucket the same key reaches.
    SecondBucket,
}

/// Prints one finding, the line the report is written from.
#[allow(
    clippy::print_stdout,
    reason = "the findings ARE this harness's output, read with `--nocapture`"
)]
fn report(account: &str, flow: &str, finding: impl std::fmt::Display) {
    println!("LIVE [{account}] {flow}: {finding}");
}

/// What a panic said, for the finding line.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "a panic without a message".to_string())
}

/// Whether `CMDR_S3_LIVE_FLOWS` (comma-separated pieces of flow names) lets
/// `name` run: every flow when it's unset. A provider with a daily request
/// cap (B2) reruns one flow without paying for the rest of its cell.
fn flow_wanted(name: &str) -> bool {
    match std::env::var("CMDR_S3_LIVE_FLOWS") {
        Ok(pieces) if !pieces.trim().is_empty() => pieces
            .split(',')
            .map(str::trim)
            .any(|piece| !piece.is_empty() && name.contains(piece)),
        _ => true,
    }
}

/// The flows of one cell, run on every live account; fails once at the end
/// with every pair that failed.
struct Matrix {
    failures: Vec<String>,
}

impl Matrix {
    /// Also routes the app's and the backend's `log` lines to stderr under
    /// `RUST_LOG` (`RUST_LOG=copy=debug,volume=debug ./live-engine.sh …`), the
    /// way a live failure gets explained.
    fn new() -> Self {
        let _ = env_logger::builder().is_test(true).try_init();
        Self { failures: Vec::new() }
    }

    /// Runs `flow` on `target`, reports it, and cleans the run's prefix.
    async fn run<F, Fut>(&mut self, target: &S3Target, name: &str, needs: Needs, flow: F)
    where
        F: FnOnce(S3Target) -> Fut,
        Fut: Future<Output = ()>,
    {
        if !flow_wanted(name) {
            return;
        }
        if needs == Needs::SecondBucket && target.bucket_2().is_none() {
            report(target.name(), name, "skipped: no second bucket");
            return;
        }
        let started = Instant::now();
        let outcome = AssertUnwindSafe(flow(target.clone())).catch_unwind().await;
        let took = format!("{:.1} s", started.elapsed().as_secs_f64());
        // A cleanup that can't reach the account is a finding too, never the
        // end of the other pairs.
        if let Err(payload) = AssertUnwindSafe(target.clean_run()).catch_unwind().await {
            let message = panic_message(payload.as_ref());
            report(target.name(), name, format!("cleanup FAILED: {message}"));
            self.failures
                .push(format!("[{}] {name} cleanup: {message}", target.name()));
        }
        match outcome {
            Ok(()) => report(target.name(), name, format!("ok in {took}")),
            Err(payload) => {
                let message = panic_message(payload.as_ref());
                report(target.name(), name, format!("FAILED in {took}: {message}"));
                self.failures.push(format!("[{}] {name}: {message}", target.name()));
            }
        }
    }

    fn finish(self) {
        assert!(
            self.failures.is_empty(),
            "{} live flow(s) failed:\n{}",
            self.failures.len(),
            self.failures.join("\n")
        );
    }
}

/// Runs `flows` on every live account, account by account.
macro_rules! live_flows {
    ($($name:literal, $needs:expr, $flow:expr;)+) => {{
        let mut matrix = Matrix::new();
        for target in S3Target::live_all() {
            $( matrix.run(&target, $name, $needs, $flow).await; )+
        }
        matrix.finish();
    }};
}

// ── The byte path ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_copies_onto_and_off_a_bucket() {
    live_flows! {
        "copy off a bucket", Needs::Nothing, |t| async move { copying_off_a_bucket_lands_every_byte(&t).await };
        "copy a 65 MiB object off", Needs::Nothing, |t| async move { a_large_object_copies_off_a_bucket_byte_for_byte(&t).await };
        "copy a seeded tree off", Needs::Nothing, |t| async move { a_directory_tree_lands_intact_off_a_bucket(&t).await };
        "copy onto, one PUT and 140 MiB multipart", Needs::Nothing, |t| async move { copying_onto_a_bucket_lands_every_byte_and_the_mtime(&t).await };
        "copy a tree onto", Needs::Nothing, |t| async move { scenario(&t, "tree-onto", a_directory_tree_lands_intact_on_the_server).await };
        "copy a written tree off", Needs::Nothing, |t| async move { scenario(&t, "tree-off-written", a_directory_tree_lands_intact_off_the_server).await };
        "cancel mid-upload", Needs::Nothing, |t| async move { scenario(&t, "cancelled-upload", a_cancelled_upload_leaves_nothing_behind).await };
        "an Overwrite answer", Needs::Nothing, |t| async move { scenario(&t, "overwrite-answer", an_overwrite_answer_replaces_the_destination_bytes).await };
        "a Skip answer in a pre-existing folder", Needs::Nothing, |t| async move { scenario(&t, "pre-existing", a_pre_existing_destination_still_probes_each_name).await };
        "awkward names round trip", Needs::Nothing, |t| async move { scenario(&t, "awkward-names", awkward_names_survive_a_round_trip).await };
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_copies_between_buckets() {
    live_flows! {
        "copy between buckets keeps the date", Needs::SecondBucket, |t| async move { copying_between_buckets_lands_every_byte(&t).await };
        "a cross-bucket copy runs on the server", Needs::SecondBucket, |t| async move { a_copy_between_two_buckets_runs_on_the_server(&t).await };
        "a bucket-bound copy streams", Needs::SecondBucket, |t| async move { a_bucket_bound_provider_streams_a_cross_bucket_copy(&t).await };
    }
}

// ── Renames, moves, merges ───────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_renames_through_the_engine() {
    live_flows! {
        "rename a folder of 1,005 objects", Needs::Nothing, |t| async move { a_folder_of_1005_objects_renames_through_the_engine(&t).await };
        "rename a 17 MiB file by multipart copy", Needs::Nothing, |t| async move { a_big_file_renames_by_multipart_copy_keeping_its_date(&t).await };
        "pause then cancel a rename", Needs::Nothing, |t| async move { a_paused_then_cancelled_rename_keeps_the_source_whole(&t).await };
        "pause then resume a rename", Needs::Nothing, |t| async move { a_paused_rename_resumes_and_lands(&t).await };
        "a reviewed batch with a folder", Needs::Nothing, |t| async move { a_batch_with_a_folder_renames_as_one_move(&t).await };
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_merges_and_moves() {
    live_flows! {
        "merge under Skip", Needs::Nothing, |t| async move { scenario(&t, "merge-skip", a_deep_clash_merge_under_skip_keeps_every_dest_only_file).await };
        "merge under Overwrite", Needs::Nothing, |t| async move { scenario(&t, "merge-overwrite", a_deep_clash_merge_under_overwrite_replaces_only_the_clash).await };
        "merge under Rename (keep both)", Needs::Nothing, |t| async move { scenario(&t, "merge-rename", a_rename_policy_lands_the_clash_beside_the_users_file).await };
        "OverwriteSmaller", Needs::Nothing, |t| async move { scenario(&t, "overwrite-smaller", overwrite_smaller_replaces_only_the_smaller_destination).await };
        "a move-merge onto spares what it skipped", Needs::Nothing, |t| async move { scenario(&t, "move-merge-onto", a_move_merge_onto_the_server_spares_what_it_skipped).await };
        "move a folder onto", Needs::Nothing, |t| async move { scenario(&t, "move-onto", a_folder_moved_onto_the_server_leaves_no_source).await };
        "move a folder off", Needs::Nothing, |t| async move { scenario(&t, "move-off", a_folder_moved_off_the_server_leaves_no_source).await };
        "a file saved over mid-move off stays", Needs::Nothing, |t| async move { scenario(&t, "move-drift-saved", a_file_saved_over_mid_move_off_the_server_stays).await };
        "a file added mid-move off stays", Needs::Nothing, |t| async move { scenario(&t, "move-drift-added", a_file_added_mid_move_off_the_server_stays).await };
        "same-bucket move-merge", Needs::Nothing, |t| async move { scenario(&t, "same-move-merge", a_same_server_move_merges_without_a_folder_prompt).await };
        "same-bucket folder move", Needs::Nothing, |t| async move { scenario(&t, "same-move", a_same_server_move_without_a_clash_moves_the_folder_whole).await };
        "same-bucket tree copy", Needs::Nothing, |t| async move { scenario(&t, "same-copy", a_same_server_copy_duplicates_a_tree).await };
        "copy into a missing nested folder", Needs::Nothing, |t| async move { scenario(&t, "missing-nested", a_copy_into_a_missing_nested_destination_makes_every_level).await };
        "a 6 MiB odd-length round trip", Needs::Nothing, |t| async move { scenario(&t, "multi-megabyte", a_multi_megabyte_file_round_trips_byte_exact).await };
        "40 files at full concurrency", Needs::Nothing, |t| async move { scenario(&t, "many-files", many_files_at_full_concurrency_land_intact).await };
    }
}

// ── Safety, cancel, rollback, delete ─────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_keeps_the_users_data_safe() {
    live_flows! {
        "a failed merge copy keeps the user's files", Needs::Nothing, |t| async move { scenario(&t, "failed-merge-copy", a_failed_folder_copy_onto_a_users_folder_keeps_their_files).await };
        "a failed merge move loses no byte", Needs::Nothing, |t| async move { scenario(&t, "failed-merge-move", a_failed_folder_move_onto_a_users_folder_loses_no_byte).await };
        "a delete bound to a local-shaped preview", Needs::Nothing, |t| async move { scenario(&t, "delete-preview", a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree).await };
        "a recursive delete takes exactly the selection", Needs::Nothing, |t| async move { scenario(&t, "delete-tree", a_delete_of_a_non_empty_folder_takes_exactly_the_selection).await };
        "an unknown source type clears nothing", Needs::Nothing, |t| async move { scenario(&t, "unknown-type", an_unknown_source_type_never_clears_a_server_folder).await };
        "cancel mid-download", Needs::Nothing, |t| async move { scenario(&t, "cancelled-download", a_cancelled_download_leaves_nothing_behind).await };
        "cancel between multipart parts", Needs::Nothing, |t| async move { a_cancel_mid_multipart_leaves_no_object_and_no_upload(&t).await };
        "a cut-off overwrite keeps the original", Needs::Nothing, |t| async move { a_cancelled_overwrite_keeps_the_original(&t).await };
        "pause and resume an upload between parts", Needs::Nothing, |t| async move { a_paused_upload_resumes_and_lands(&t).await };
        "roll back a finished copy", Needs::Nothing, |t| async move { a_finished_copy_onto_a_bucket_rolls_back(&t).await };
        "cancel with rollback mid-tree", Needs::Nothing, |t| async move { a_cancel_with_rollback_takes_back_what_landed(&t).await };
        "delete a folder of 1,005 objects", Needs::Nothing, |t| async move { a_folder_of_1005_objects_deletes(&t).await };
    }
}

/// Informational: S3 writes the final key, so a writer landing between the
/// last no-overwrite check and the write's own completion is overwritten (the
/// blind window `crates/cmdr-s3/DETAILS.md` § "No-overwrite writes" accepts).
/// A failure here measures that window on a real provider; it isn't a bug.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_measures_the_name_taken_mid_upload_window() {
    live_flows! {
        "a name taken mid-upload (blind window, informational)", Needs::Nothing, |t| async move {
            scenario(&t, "name-taken", |remote, dir| a_name_taken_mid_upload_is_never_replaced(remote, dir, ConflictResolution::Skip, 2 * MIB)).await
        };
    }
}

// ── Between providers ────────────────────────────────────────────────

/// A 50 MiB file and a small tree streamed from one provider's bucket to
/// another's: no server-side copy across accounts, so every byte goes through
/// the Mac, and the destination's own write path (one PUT or parts) takes it.
async fn a_copy_between_providers_streams_every_byte(from: &S3Target, to: &S3Target) {
    let source = Arc::new(from.connect(Some(from.bucket())).await);
    let destination = Arc::new(to.connect(Some(to.bucket())).await);
    let prefix = from.prefix("cross-provider");
    let big = self_describing_bytes(50 * MIB, "cross-provider");
    let small: Vec<(String, Vec<u8>)> = ["a.txt", "nested/b.txt", "nested/deeper/c.txt"]
        .into_iter()
        .map(|name| (format!("{prefix}tree/{name}"), self_describing_bytes(10_000, name)))
        .collect();
    let big_key = format!("{prefix}tree/big.bin");
    let mut seeds = vec![object(&big_key, &big)];
    seeds.extend(small.iter().map(|(key, bytes)| object(key, bytes)));
    from.seed(from.bucket(), &seeds).await;

    let source_dir = source.root().join(prefix.trim_end_matches('/'));
    let dest_prefix = to.prefix("cross-provider");
    let dest_dir = destination.root().join(dest_prefix.trim_end_matches('/'));
    let expected = tree_fingerprint(source.as_ref(), &source_dir.join("tree")).await;
    run_copy(
        "cross-provider",
        Arc::clone(&source) as Arc<dyn Volume>,
        vec![source_dir.join("tree")],
        Arc::clone(&destination) as Arc<dyn Volume>,
        dest_dir.clone(),
    )
    .await;
    assert_eq!(
        tree_fingerprint(destination.as_ref(), &dest_dir.join("tree")).await,
        expected,
        "the tree must land on the other provider name for name and byte for byte"
    );
    assert!(to.unfinished_uploads(to.bucket(), &dest_prefix).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_copies_between_providers() {
    let targets = S3Target::live_all();
    let mut matrix = Matrix::new();
    // Each account sends to the next, so every provider is a source once and
    // a destination once.
    for (index, from) in targets.iter().enumerate() {
        if targets.len() < 2 {
            break;
        }
        let to = targets[(index + 1) % targets.len()].clone();
        let flow = format!("stream a 50 MiB tree to {}", to.name());
        let destination = to.clone();
        matrix
            .run(from, &flow, Needs::Nothing, |from| async move {
                let outcome = AssertUnwindSafe(a_copy_between_providers_streams_every_byte(&from, &destination))
                    .catch_unwind()
                    .await;
                // The destination's half of the cleanup; `run` cleans the source.
                destination.clean_run().await;
                if let Err(payload) = outcome {
                    std::panic::resume_unwind(payload);
                }
            })
            .await;
    }
    matrix.finish();
}

// ── Archived objects (AWS) ───────────────────────────────────────────

/// ❗ An object in Glacier Flexible Retrieval or Deep Archive reads as
/// `VolumeError::ColdStorage`, lists with the archived flag, and a copy of it
/// stops with `SourceInColdStorage`: an answer, not a hang or a generic
/// failure. AWS only; uploads the archived objects directly (tiny ones).
async fn an_archived_object_refuses_reads_by_name(target: &S3Target) {
    let volume = Arc::new(target.connect(Some(target.bucket())).await);
    let prefix = target.prefix("cold");
    let dir = volume.root().join(prefix.trim_end_matches('/'));
    for class in ["GLACIER", "DEEP_ARCHIVE"] {
        let key = format!("{prefix}{class}.txt");
        target
            .seed_with(
                target.bucket(),
                &[Seed {
                    key: &key,
                    bytes: b"frozen",
                    mtime: None,
                }],
                &[("x-amz-storage-class", class)],
            )
            .await;
        let path = dir.join(format!("{class}.txt"));

        let listed = volume.list_directory(&dir, None).await.expect("the folder lists");
        let row = listed
            .iter()
            .find(|entry| entry.name == format!("{class}.txt"))
            .unwrap_or_else(|| panic!("{class}: the archived object lists"));
        assert!(row.in_cold_storage, "{class}: the listing marks it archived");

        match volume.open_read_stream(&path).await {
            Err(VolumeError::ColdStorage(_)) => {}
            Err(other) => panic!("{class}: a read answered {other:?}, not ColdStorage"),
            Ok(mut stream) => match stream.next_chunk().await {
                Some(Err(VolumeError::ColdStorage(_))) => {}
                other => panic!(
                    "{class}: a read must refuse as ColdStorage, got {:?}",
                    other.map(|chunk| chunk.map(|bytes| bytes.len()))
                ),
            },
        }

        let local_dir = TestDir::new("s3_cold_copy");
        let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));
        let running = start_copy(
            "cold-copy",
            Arc::clone(&volume) as Arc<dyn Volume>,
            vec![path],
            local,
            PathBuf::from(""),
            VolumeCopyConfig::default(),
        )
        .await;
        running.settle().await;
        let errors = running.events.errors.lock_ignore_poison();
        assert!(
            matches!(
                errors.first().map(|e| &e.error),
                Some(WriteOperationError::SourceInColdStorage { .. })
            ),
            "{class}: a copy must stop as SourceInColdStorage, got {:?}",
            errors.iter().map(|e| &e.error).collect::<Vec<_>>()
        );
        assert!(
            !local_dir.join(format!("{class}.txt")).exists(),
            "{class}: nothing lands locally"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_refuses_archived_objects_by_name() {
    let mut matrix = Matrix::new();
    for target in S3Target::live_all() {
        if !matches!(target.provider(), cmdr_s3::S3Provider::Aws { .. }) {
            continue;
        }
        matrix
            .run(&target, "an archived object", Needs::Nothing, |t| async move {
                an_archived_object_refuses_reads_by_name(&t).await;
            })
            .await;
    }
    matrix.finish();
}

// ── Requests against the estimate ────────────────────────────────────

/// Reports, per provider and operation, every request kind the engine sent
/// beside what the dialog estimates, and fails where any disagrees: the same
/// bar as the fixture cell (`the_engine_sends_what_the_estimate_counts`), on
/// each provider's own no-overwrite and pin rules.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_live_engine_sends_what_the_estimate_counts() {
    live_flows! {
        "requests sent against the estimate", Needs::Nothing, |t| async move {
            let comparisons = requests_sent_against_the_estimate(&t).await;
            let mut mismatches = Vec::new();
            for comparison in &comparisons {
                report(t.name(), comparison.operation, format!("sent {:?}, estimated {:?}", comparison.sent, comparison.estimated));
                mismatches.extend(comparison.mismatches().into_iter().map(|m| format!("{}: {m}", comparison.operation)));
            }
            assert!(mismatches.is_empty(), "{}", mismatches.join("; "));
        };
    }
}

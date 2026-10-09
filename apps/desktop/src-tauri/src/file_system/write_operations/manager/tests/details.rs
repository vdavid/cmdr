//! On-demand row details (`get_operation_details`): the full paths and the
//! timing an expanded queue row shows, kept OFF the thin `operations-changed`
//! snapshot on purpose.
//!
//! Reuses the admission suite's fixtures (`unique`, `descriptor`,
//! `gated_deferred`(`_on`), `fresh_state`, `WAIT`) through `use super::*`.

use super::*;
use std::path::{Path, PathBuf};

use super::super::details::{DETAILS_SOURCE_CAP, OperationDetailsError, OperationPaths};

fn with_paths(mut desc: OperationDescriptor, sources: &[&str], destination: Option<&str>) -> OperationDescriptor {
    let sources: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    desc.summary.paths = OperationPaths::from_paths(&sources, destination.map(Path::new));
    desc
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_op_answers_with_its_full_paths_and_when_it_started() {
    let mgr = Box::leak(Box::new(OperationManager::new()));
    let op = unique("details-running");
    let lane = unique("lane");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();

    let desc = with_paths(
        descriptor(&op, vec![&lane]),
        &["/Users/me/photos/a.raw", "/Users/me/photos/b.raw"],
        Some("/Volumes/Naspolya/backup"),
    );
    mgr.spawn_managed(
        desc,
        fresh_state(),
        gated_deferred_on(mgr, op.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("started");

    let details = mgr.details(&op).expect("a live op has details");
    assert_eq!(details.operation_id, op);
    assert_eq!(
        details.source_paths,
        vec![
            "/Users/me/photos/a.raw".to_string(),
            "/Users/me/photos/b.raw".to_string()
        ],
        "the FULL paths, not the basenames the snapshot carries"
    );
    assert_eq!(details.source_count, 2);
    assert_eq!(details.destination_path.as_deref(), Some("/Volumes/Naspolya/backup"));
    assert!(details.queued_at > 0, "registration time is stamped");
    let started_at = details.started_at.expect("an admitted op has a start time");
    assert!(
        started_at >= details.queued_at,
        "it can't start before it was registered"
    );

    let _ = rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_op_has_its_paths_but_no_start_time_yet() {
    let mgr = Box::leak(Box::new(OperationManager::new()));
    let lane = unique("lane");
    let blocker = unique("details-blocker");
    let waiting = unique("details-waiting");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();
    mgr.spawn_managed(
        descriptor(&blocker, vec![&lane]),
        fresh_state(),
        gated_deferred_on(mgr, blocker.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("blocker started");

    let (w_started_tx, _w_started_rx) = oneshot::channel();
    let (_w_rel_tx, w_rel_rx) = oneshot::channel();
    let desc = with_paths(descriptor(&waiting, vec![&lane]), &["/tmp/one"], Some("/tmp/dest"));
    mgr.spawn_managed(
        desc,
        fresh_state(),
        gated_deferred_on(mgr, waiting.clone(), w_started_tx, w_rel_rx),
    );
    assert_eq!(mgr.lifecycle_status(&waiting), Some(LifecycleStatus::Queued));

    let details = mgr.details(&waiting).expect("a queued op has details");
    assert_eq!(details.source_paths, vec!["/tmp/one".to_string()]);
    assert_eq!(details.destination_path.as_deref(), Some("/tmp/dest"));
    assert_eq!(
        details.started_at, None,
        "nothing has started while it waits for a lane"
    );

    let _ = rel_tx.send(());
}

#[test]
fn an_unknown_id_is_a_typed_not_found() {
    let mgr = OperationManager::new();
    let op = unique("details-unknown");
    match mgr.details(&op) {
        Err(OperationDetailsError::NotFound { operation_id }) => assert_eq!(operation_id, op),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn a_huge_selection_is_capped_but_still_counted() {
    let sources: Vec<PathBuf> = (0..DETAILS_SOURCE_CAP + 37)
        .map(|i| PathBuf::from(format!("/src/file-{i}")))
        .collect();
    let paths = OperationPaths::from_paths(&sources, Some(Path::new("/dest")));

    let details = paths.details_for("op", 1, None);
    assert_eq!(details.source_paths.len(), DETAILS_SOURCE_CAP, "the list is bounded");
    assert_eq!(
        details.source_count,
        DETAILS_SOURCE_CAP + 37,
        "the count is the real one"
    );
    assert_eq!(details.source_paths[0], "/src/file-0", "kept in selection order");
}

#[test]
fn path_summary_carries_the_full_paths_behind_its_names() {
    let sources = vec![PathBuf::from("/a/one.txt"), PathBuf::from("/a/two.txt")];
    let summary = super::super::super::path_summary(&sources, Some(Path::new("/b/dest")));
    assert_eq!(
        summary.source.as_deref(),
        Some("one.txt (2 items)"),
        "the row's name is unchanged"
    );
    let details = summary.paths.details_for("op", 1, None);
    assert_eq!(
        details.source_paths,
        vec!["/a/one.txt".to_string(), "/a/two.txt".to_string()]
    );
    assert_eq!(details.destination_path.as_deref(), Some("/b/dest"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retained_failure_keeps_its_details_until_dismissed() {
    let mgr = Box::leak(Box::new(OperationManager::new()));
    let op = unique("details-failed");
    let lane = unique("lane");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();
    let desc = with_paths(
        descriptor(&op, vec![&lane]),
        &["/Users/me/big.iso"],
        Some("/Volumes/USB"),
    );
    mgr.spawn_managed(
        desc,
        fresh_state(),
        gated_deferred_on(mgr, op.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("started");

    mgr.record_failure(
        &op,
        WriteOperationType::Copy,
        &WriteOperationError::IoError {
            path: "/Volumes/USB/big.iso".to_string(),
            message: "disk went away".to_string(),
        },
    );
    let _ = rel_tx.send(());
    wait_until_async(WAIT, "the failed op to settle", || mgr.lifecycle_status(&op).is_none()).await;

    let details = mgr
        .details(&op)
        .expect("a retained failure still explains where it was going");
    assert_eq!(details.source_paths, vec!["/Users/me/big.iso".to_string()]);
    assert_eq!(details.destination_path.as_deref(), Some("/Volumes/USB"));
    assert!(details.started_at.is_some(), "it had started before it failed");

    mgr.dismiss_failure(&op);
    assert!(
        matches!(mgr.details(&op), Err(OperationDetailsError::NotFound { .. })),
        "a dismissed failure has nothing left to explain"
    );
}

#[test]
fn paths_on_a_volume_the_os_cant_resolve_carry_its_name() {
    let phone = crate::file_system::volume::InMemoryVolume::new("Pixel 8");
    assert!(
        !crate::file_system::volume::Volume::paths_are_os_visible(&phone),
        "fixture: an in-memory volume's paths name nothing on the Mac"
    );
    let paths = OperationPaths::from_paths(&[PathBuf::from("/DCIM/a.jpg")], Some(Path::new("/Users/me/in")))
        .on_volumes(OperationPaths::volume_label(&phone), None);

    let details = paths.details_for("op", 1, None);
    assert_eq!(details.source_volume_name.as_deref(), Some("Pixel 8"));
    assert_eq!(details.destination_volume_name, None, "a plain local path names itself");
    assert_eq!(
        details.source_paths,
        vec!["/DCIM/a.jpg".to_string()],
        "the path itself is untouched"
    );
}

#[test]
fn a_local_path_carries_no_volume_name() {
    let details = OperationPaths::from_paths(&[PathBuf::from("/a")], None).details_for("op", 1, None);
    assert_eq!(details.source_volume_name, None);
    assert_eq!(details.destination_volume_name, None);
}

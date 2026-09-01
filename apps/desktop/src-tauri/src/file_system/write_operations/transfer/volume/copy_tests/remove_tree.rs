//! `cleanup::remove_tree`, the recursive delete cross-volume moves use to
//! clear the source tree.

use super::*;

// ── remove_tree ───────────────────────────────────────────────────
//
// Regression coverage for the move-between-volumes recursive-delete fix.
// `Volume::delete` is contractually for files or *empty* directories
// (LocalPosix uses `std::fs::remove_dir`); cross-volume moves rely on
// this helper to clear out the source tree depth-first, which is why it
// carries a `TreeRemoval` naming who authorized the recursion.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_removes_nonempty_directory() {
    let vol = Arc::new(InMemoryVolume::new("V"));
    vol.create_directory(Path::new("/photos")).await.unwrap();
    vol.create_file(Path::new("/photos/a.jpg"), b"a").await.unwrap();
    vol.create_file(Path::new("/photos/b.jpg"), b"b").await.unwrap();
    vol.create_directory(Path::new("/photos/sub")).await.unwrap();
    vol.create_file(Path::new("/photos/sub/c.jpg"), b"c").await.unwrap();

    let result: Arc<dyn Volume> = vol.clone();
    remove_tree(
        &result,
        Path::new("/photos"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await
    .unwrap();

    assert!(!vol.exists(Path::new("/photos")).await);
    assert!(!vol.exists(Path::new("/photos/a.jpg")).await);
    assert!(!vol.exists(Path::new("/photos/sub/c.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_removes_single_file() {
    let vol = Arc::new(InMemoryVolume::new("V"));
    vol.create_file(Path::new("/file.txt"), b"hi").await.unwrap();

    let result: Arc<dyn Volume> = vol.clone();
    remove_tree(
        &result,
        Path::new("/file.txt"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await
    .unwrap();

    assert!(!vol.exists(Path::new("/file.txt")).await);
}

/// The whole tree can't come down because ONE leaf refuses. What comes back is
/// that leaf, not the root's own "directory not empty" — which names the folder
/// the user selected and tells them nothing they can act on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_reports_the_leaf_that_refused() {
    use super::super::super::strategy::test_support::UndeletableSource;

    let vol = UndeletableSource::new(
        "doomed.txt",
        VolumeError::IoError {
            message: "Resource busy".to_string(),
            raw_os_error: None,
        },
    );
    let volume: Arc<dyn Volume> = Arc::clone(&vol) as Arc<dyn Volume>;
    volume.create_directory(Path::new("/tree")).await.unwrap();
    volume.create_directory(Path::new("/tree/nested")).await.unwrap();
    volume.create_file(Path::new("/tree/fine.txt"), b"fine").await.unwrap();
    volume
        .create_file(Path::new("/tree/nested/doomed.txt"), b"doomed")
        .await
        .unwrap();

    let failure = remove_tree(
        &volume,
        Path::new("/tree"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await
    .expect_err("the leaf never deletes, so the sweep can't finish");
    assert_eq!(
        failure.path,
        Path::new("/tree/nested/doomed.txt"),
        "the failure must carry the leaf that refused, not the tree root"
    );

    // Best-effort still applies: everything that COULD go, went.
    assert!(!volume.exists(Path::new("/tree/fine.txt")).await);
    assert!(volume.exists(Path::new("/tree/nested/doomed.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_missing_path_is_ok() {
    // Used during move cleanup where the path may already be gone (cancelled mid-op,
    // partial state). No error.
    let vol = Arc::new(InMemoryVolume::new("V"));
    let result: Arc<dyn Volume> = vol.clone();
    let r = remove_tree(
        &result,
        Path::new("/never-existed"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await;
    assert!(r.is_ok(), "expected Ok, got {r:?}");
}

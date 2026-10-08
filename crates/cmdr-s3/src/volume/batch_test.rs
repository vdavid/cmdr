//! Work over many keys against both fixtures: the batch delete past one
//! request's thousand keys, the capped subtree tally, and what a rename costs.

use std::path::PathBuf;

use cmdr_fs::volume::{RenameWork, Volume, VolumeError};

use super::testing::*;

const MIB: usize = 1024 * 1024;

/// ❗ Past a thousand keys the delete pages into a second `DeleteObjects`, and
/// every path, an already-gone one included, answers on its own.
async fn a_batch_delete_of_1005_files_clears_them_all(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("batch-delete");
    let keys: Vec<String> = (0..1_005).map(|n| format!("{prefix}f{n:04}.txt")).collect();
    let seeds: Vec<Seed<'_>> = keys.iter().map(|key| object(key, b"x")).collect();
    seed(service, FIXTURE_BUCKET, &seeds).await;

    let mut paths: Vec<PathBuf> = keys.iter().map(|key| volume.root().join(key)).collect();
    paths.push(volume.root().join(format!("{prefix}never-there.txt")));
    let results = volume.delete_files(&paths).await;
    assert_eq!(results.len(), paths.len());
    assert!(
        results.iter().all(Result::is_ok),
        "{:?}",
        results.iter().find(|result| result.is_err())
    );
    let folder = volume.root().join(prefix.trim_end_matches('/'));
    assert!(
        matches!(
            volume.list_directory(&folder, None).await,
            Err(VolumeError::NotFound(_))
        ),
        "every key under the folder is gone"
    );
}

async fn a_tally_counts_objects_but_not_markers_and_stops_past_its_cap(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("batch-tally");
    let (a, b, c, marker) = (
        format!("{prefix}a.txt"),
        format!("{prefix}sub/b.txt"),
        format!("{prefix}sub/deeper/c.txt"),
        format!("{prefix}empty/"),
    );
    seed(
        service,
        FIXTURE_BUCKET,
        &[
            object(&a, b"aaaa"),
            object(&b, b"bb"),
            object(&c, b"c"),
            object(&marker, b""),
        ],
    )
    .await;
    let folder = volume.root().join(prefix.trim_end_matches('/'));

    let all = volume.tally_subtree(&folder, 100).await.expect("a tally");
    assert_eq!((all.files, all.bytes, all.complete), (3, 7, true));
    assert_eq!(all.folders, 4, "the folder itself, `sub`, `sub/deeper`, and `empty`");
    let mut sizes: Vec<u64> = all.per_file.iter().map(|file| file.size).collect();
    sizes.sort_unstable();
    assert_eq!(sizes, [1, 2, 4]);
    assert!(
        all.per_file.iter().all(|file| file.modified_at.is_some()),
        "each object's upload time, which early deletion bills by"
    );
    let capped = volume.tally_subtree(&folder, 2).await.expect("a tally");
    assert_eq!((capped.files, capped.complete), (2, false));
    let one = volume
        .tally_subtree(&volume.root().join(&a), 100)
        .await
        .expect("a tally");
    assert_eq!((one.files, one.bytes, one.folders, one.complete), (1, 4, 0, true));
    assert!(one.per_file[0].modified_at.is_some(), "an object's upload time");
    assert!(matches!(
        volume
            .tally_subtree(&volume.root().join(format!("{prefix}nope")), 10)
            .await,
        Err(VolumeError::NotFound(_))
    ));
}

/// A folder, and a file past the part floor, rename by copying; a small file
/// renames in one call.
async fn rename_work_tells_a_folder_and_a_big_file_from_a_small_file(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("batch-rename-work");
    let small = format!("{prefix}small.bin");
    let big = format!("{prefix}big.bin");
    let inner = format!("{prefix}folder/inner.txt");
    let big_bytes = self_describing_bytes(6 * MIB, "big");
    seed(
        service,
        FIXTURE_BUCKET,
        &[
            object(&small, b"small"),
            object(&big, &big_bytes),
            object(&inner, b"in"),
        ],
    )
    .await;

    let work = |key: String| {
        let path = volume.root().join(key);
        let volume = &volume;
        async move { volume.rename_work(&path).await }
    };
    assert_eq!(work(small).await.expect("an answer"), RenameWork::OneCall);
    assert_eq!(work(big).await.expect("an answer"), RenameWork::CopyThenDelete);
    assert_eq!(
        work(format!("{prefix}folder")).await.expect("an answer"),
        RenameWork::CopyThenDelete
    );
    assert!(matches!(
        work(format!("{prefix}nope")).await,
        Err(VolumeError::NotFound(_))
    ));
}

/// One `#[tokio::test]` per fixture for each cell, `#[ignore]`d for the shared
/// fixture lane.
macro_rules! on_both_fixtures {
    ($($cell:ident => $versitygw:ident, $garage:ident;)*) => {$(
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        #[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
        async fn $versitygw() {
            $cell(VERSITYGW).await;
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        #[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
        async fn $garage() {
            $cell(GARAGE).await;
        }
    )*};
}

on_both_fixtures! {
    a_batch_delete_of_1005_files_clears_them_all
        => a_batch_delete_of_1005_files_clears_them_all_on_versitygw,
           a_batch_delete_of_1005_files_clears_them_all_on_garage;
    a_tally_counts_objects_but_not_markers_and_stops_past_its_cap
        => a_tally_counts_objects_but_not_markers_and_stops_past_its_cap_on_versitygw,
           a_tally_counts_objects_but_not_markers_and_stops_past_its_cap_on_garage;
    rename_work_tells_a_folder_and_a_big_file_from_a_small_file
        => rename_work_tells_a_folder_and_a_big_file_from_a_small_file_on_versitygw,
           rename_work_tells_a_folder_and_a_big_file_from_a_small_file_on_garage;
}

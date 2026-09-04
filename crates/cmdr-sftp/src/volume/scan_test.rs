//! What a copy is about to cost, and what is already in its way.
//!
//! Both answers are read off real listings, so both need a real server. What the
//! cells pin is the SHAPE of the walk as much as its arithmetic: the counts climb
//! while it runs, a nested tree is counted whole, and a destination is read once
//! rather than once per source item.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{ListingProgress, ScanBoundary, SourceItemInfo, Volume};

use super::testing::*;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// A scratch tree: two files at the top, a subdirectory holding one more.
async fn seed_tree(volume: &super::SftpVolume, dir: &str) {
    volume.create_directory(Path::new(dir)).await.expect(FIXTURE);
    volume
        .create_file(Path::new(&format!("{dir}/one.txt")), b"1234567890")
        .await
        .expect(FIXTURE);
    volume
        .create_file(Path::new(&format!("{dir}/two.txt")), b"12345")
        .await
        .expect(FIXTURE);
    volume
        .create_directory(Path::new(&format!("{dir}/nested")))
        .await
        .expect(FIXTURE);
    volume
        .create_file(Path::new(&format!("{dir}/nested/three.txt")), b"123")
        .await
        .expect(FIXTURE);
}

/// Everything under a directory, counted whole.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_copy_scan_counts_the_whole_subtree() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir("scan-subtree");
    clean_scratch_deep(&volume, &dir).await;
    seed_tree(&volume, &dir).await;

    let scan = volume.scan_for_copy(Path::new(&dir)).await.expect(FIXTURE);

    assert_eq!(scan.file_count, 3, "two at the top and one a level down");
    assert_eq!(scan.dir_count, 2, "the directory itself and the nested one");
    assert_eq!(scan.total_bytes, 18);
    assert_eq!(
        scan.dedup_bytes, scan.total_bytes,
        "SFTP v3 has no link count, so the source footprint IS the write footprint"
    );
    assert!(scan.top_level_is_directory);

    clean_scratch_deep(&volume, &dir).await;
}

/// A single file scans as one file, and says so.
///
/// ❗ `top_level_is_directory` is what the copy planner reads to decide whether
/// it is merging into a folder or landing a file, so a directory-shaped answer
/// for a file is a merge that never happens.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_copy_scan_of_one_file_is_one_file() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    let scan = volume.scan_for_copy(Path::new("hello.txt")).await.expect(FIXTURE);

    assert_eq!((scan.file_count, scan.dir_count), (1, 0));
    assert!(!scan.top_level_is_directory);
}

/// ⚠️ The counts climb WHILE the walk runs.
///
/// A scan that reports only at the end leaves the transfer dialog on
/// "0 files" for the length of the walk, and leaves the scan watchdog — which
/// bounds a preview by INACTIVITY — unable to tell a slow tree from a server that
/// stopped answering.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_batch_scan_reports_while_it_walks() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir("scan-progress");
    clean_scratch_deep(&volume, &dir).await;
    seed_tree(&volume, &dir).await;

    let seen: Mutex<Vec<ListingProgress>> = Mutex::new(Vec::new());
    let record = |progress: ListingProgress| seen.lock_ignore_poison().push(progress);
    let boundary = ScanBoundary::new(Some(&record));
    let batch = volume
        .scan_for_copy_batch_with_boundary(&[PathBuf::from(&dir)], &boundary)
        .await
        .expect(FIXTURE);

    assert_eq!(batch.aggregate.file_count, 3);
    assert_eq!(batch.per_path.len(), 1);
    assert!(
        batch.aggregate.top_level_is_directory,
        "a single-path batch still carries the one type it does know"
    );

    {
        let seen = seen.lock_ignore_poison();
        assert!(
            seen.len() >= 5,
            "one report per directory entered and per file counted, got {}",
            seen.len()
        );
        assert!(
            seen.windows(2).all(|pair| pair[1].files >= pair[0].files),
            "the counts are cumulative for the call: a counter that goes backwards is worse than none"
        );
        assert_eq!(seen.last().map(|last| last.files), Some(3));
    }

    clean_scratch_deep(&volume, &dir).await;
}

/// One listing answers for every source item, ❗ never one probe each.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_conflict_scan_finds_the_names_already_taken() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir("scan-conflicts");
    clean_scratch_deep(&volume, &dir).await;
    seed_tree(&volume, &dir).await;

    let sources = [
        SourceItemInfo {
            name: "one.txt".to_string(),
            size: 99,
            modified: Some(1_700_000_000),
            is_directory: false,
        },
        SourceItemInfo {
            name: "nested".to_string(),
            size: 0,
            modified: None,
            is_directory: true,
        },
        SourceItemInfo {
            name: "not-there.txt".to_string(),
            size: 1,
            modified: None,
            is_directory: false,
        },
    ];

    let conflicts = volume
        .scan_for_conflicts(&sources, Path::new(&dir))
        .await
        .expect(FIXTURE);

    assert_eq!(conflicts.len(), 2, "two of the three names are taken");
    let file = conflicts.iter().find(|c| c.source_path == "one.txt").expect("the file");
    assert_eq!(file.source_size, 99, "the source's own numbers pass through");
    assert_eq!(file.dest_size, 10, "and the destination's come off the listing");
    assert!(!file.dest_is_directory);
    let folder = conflicts
        .iter()
        .find(|c| c.source_path == "nested")
        .expect("the folder");
    assert!(
        folder.source_is_directory && folder.dest_is_directory,
        "a folder onto a folder is a merge, and the frontend can only say so if both flags arrive"
    );

    clean_scratch_deep(&volume, &dir).await;
}

/// A destination that isn't there yet holds nothing, so nothing clashes.
///
/// ❗ Reporting the missing directory as an error instead would turn "paste into
/// a folder I'm about to create" into a failure.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_conflict_scan_of_a_destination_that_is_not_there_finds_nothing() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    let conflicts = volume
        .scan_for_conflicts(
            &[SourceItemInfo {
                name: "anything.txt".to_string(),
                size: 1,
                modified: None,
                is_directory: false,
            }],
            Path::new(&scratch_dir("scan-absent")),
        )
        .await
        .expect(FIXTURE);

    assert!(conflicts.is_empty());
}

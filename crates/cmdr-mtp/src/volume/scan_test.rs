//! Selected-source progress against an asymmetric virtual phone tree.

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::scan_stop::TestScanStop;
use cmdr_fs::volume::{ListingProgress, ScanBoundary, ScanStop, Volume, VolumeError};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::testing::{connect_fixture, device_lock, test_connection_manager, volume_for};
use crate::virtual_device::setup_virtual_mtp_device;

#[tokio::test]
async fn selected_subtrees_report_cumulative_recursive_progress() {
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    let root = fixture.root().join("internal/Progress");
    std::fs::create_dir_all(root.join("Selected/Nested/Empty")).unwrap();
    std::fs::create_dir_all(root.join("OtherSelected")).unwrap();
    for (path, size) in [
        ("unrelated.bin", 100_000),
        ("Selected/a.bin", 7),
        ("Selected/Nested/b.bin", 13),
        ("OtherSelected/c.bin", 29),
        ("loose.bin", 31),
    ] {
        std::fs::write(root.join(path), vec![0; size]).unwrap();
    }
    let manager = test_connection_manager();
    let device = connect_fixture(manager, fixture).await;
    let volume = volume_for(manager, &device, None).await;
    for (paths, expected) in [
        (vec!["/Progress/Selected"], (2, 3, 20)),
        (
            vec!["/Progress/Selected", "/Progress/OtherSelected", "/Progress/loose.bin"],
            (4, 4, 80),
        ),
    ] {
        let ticks = Mutex::new(Vec::new());
        let report = |progress: ListingProgress| ticks.lock_ignore_poison().push(progress);
        let boundary = ScanBoundary::new(Some(&report));
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let result = volume
            .scan_for_copy_batch_with_boundary(&paths, &boundary)
            .await
            .unwrap();
        let ticks = ticks.lock_ignore_poison();
        assert!(
            ticks.len() >= expected.0 + expected.1,
            "each selected entry reports progress"
        );
        for tick in ticks.iter() {
            assert!(tick.files <= expected.0 && tick.dirs <= expected.1 && tick.bytes <= expected.2);
        }
        for pair in ticks.windows(2) {
            assert!(pair[0].files <= pair[1].files && pair[0].dirs <= pair[1].dirs && pair[0].bytes <= pair[1].bytes);
        }
        let last = ticks.last().unwrap();
        assert_eq!((last.files, last.dirs, last.bytes), expected);
        assert_eq!(
            (
                result.aggregate.file_count,
                result.aggregate.dir_count,
                result.aggregate.total_bytes
            ),
            expected
        );
        assert_eq!(result.aggregate.dedup_bytes, expected.2);
        assert_eq!(result.per_path.len(), paths.len());
        assert_eq!(
            (boundary.counts().files, boundary.counts().dirs, boundary.counts().bytes),
            expected
        );
    }

    // Cancel from recursive progress, not the parent resolution listing. A
    // subsequent successful scan proves the cooperative stop leaves USB usable.
    let stop = TestScanStop::new();
    let report = |progress: ListingProgress| {
        if progress.dirs == 2 {
            stop.stop();
        }
    };
    let boundary = ScanBoundary::new(Some(&report)).stopping_at(ScanStop::new(stop.clone()));
    let paths = [PathBuf::from("/Progress/Selected")];
    assert!(matches!(
        volume.scan_for_copy_batch_with_boundary(&paths, &boundary).await,
        Err(VolumeError::Cancelled(_))
    ));
    let result = volume
        .scan_for_copy_batch_with_boundary(&paths, &ScanBoundary::silent())
        .await
        .unwrap();
    assert_eq!(
        (
            result.aggregate.file_count,
            result.aggregate.dir_count,
            result.aggregate.total_bytes
        ),
        (2, 3, 20)
    );
    device.teardown(manager).await;
}

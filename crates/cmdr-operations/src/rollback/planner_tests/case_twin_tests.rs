//! Restoring onto a name that differs from the landed one only by case or Unicode
//! normalization: the same entry on a case-insensitive volume, a DIFFERENT one on a
//! case-sensitive volume (`crates/cmdr-operation-log/DETAILS.md` § "The two
//! data-safety guards").
//!
//! `InMemoryVolume` is case- and normalization-sensitive, so it stands in for SFTP to
//! Linux, S3, or case-sensitive APFS. The case-insensitive side runs on the real
//! macOS temp dir, the only place that folds case for real.

use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_operation_log::types::{OpKind, RollbackState, SkipReason};

/// The batch-lowercase case from #511: `IMG_0001.JPG` was renamed to `img_0001.jpg`,
/// then a different file landed at `IMG_0001.JPG` (a camera re-import). Undoing the
/// rename must leave the newcomer alone: folding the two names together called it
/// "the same file" and renamed over it.
#[tokio::test]
async fn undoing_a_case_only_rename_skips_when_both_spellings_exist() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("Linux box"));
    put(&v, "/img_0001.jpg", b"OLD!").await;
    put(&v, "/IMG_0001.JPG", b"the camera's new photo").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/IMG_0001.JPG", "v", "/img_0001.jpg", 4)],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 1), "got {report:?}");
    assert_eq!(
        rig.read_items("op")[0].rollback_skip_reason,
        Some(SkipReason::RestoreTargetOccupied)
    );
    assert_eq!(read(&v, "/IMG_0001.JPG").await, b"the camera's new photo");
    assert_eq!(read(&v, "/img_0001.jpg").await, b"OLD!", "the renamed file stays put");
}

/// The same trap with a normalization-only difference: `Café.txt` (composed) was
/// renamed to its decomposed spelling, and a different file holds the composed one
/// now. On a normalization-sensitive filesystem those are two files.
#[tokio::test]
async fn undoing_a_normalization_only_rename_skips_when_both_spellings_exist() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("Linux box"));
    put(&v, "/Cafe\u{0301}.txt", b"OLD!").await;
    put(&v, "/Café.txt", b"someone else's").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/Café.txt", "v", "/Cafe\u{0301}.txt", 4)],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 1), "got {report:?}");
    assert_eq!(read(&v, "/Café.txt").await, b"someone else's");
}

/// A move between two folders whose names differ only by case: on a case-sensitive
/// volume `/srv/Media` and `/srv/media` are two folders, and a file that has since
/// landed at the old path is a different file.
#[tokio::test]
async fn undoing_a_move_between_case_twin_folders_skips_when_the_old_path_is_taken() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("Linux box"));
    for dir in ["/srv", "/srv/Media", "/srv/media"] {
        mkdir(&v, dir).await;
    }
    put(&v, "/srv/media/report.pdf", b"OLD!").await;
    put(&v, "/srv/Media/report.pdf", b"a newer report").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Move,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(
            0,
            "v",
            "/srv/Media/report.pdf",
            "v",
            "/srv/media/report.pdf",
            4,
        )],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 1), "got {report:?}");
    assert_eq!(read(&v, "/srv/Media/report.pdf").await, b"a newer report");
    assert_eq!(read(&v, "/srv/media/report.pdf").await, b"OLD!");
}

/// The other side of the guard: on a case-insensitive volume the "occupant" of the
/// old spelling IS the renamed entry, so undoing a case-only rename of a file and of
/// a folder must land. On the real macOS temp dir, because it's the one place that
/// folds case for real; skipped when the temp dir sits on a case-sensitive volume.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn undoing_a_case_only_rename_on_a_case_insensitive_volume_restores_it() {
    use cmdr_fs::volume::Volume;
    use cmdr_operation_log::types::EntryType;
    use cmdr_operation_log::writer::JournalItem;

    let work = tempfile::tempdir().expect("work");
    let root = work.path();
    std::fs::write(root.join("probe"), b"").expect("probe");
    if !root.join("PROBE").exists() {
        return;
    }
    let photo = root.join("img_0001.jpg");
    std::fs::write(&photo, b"DATA").expect("write");
    filetime::set_file_mtime(&photo, filetime::FileTime::from_unix_time(MT as i64, 0)).expect("pin mtime");
    std::fs::create_dir(root.join("photos")).expect("mkdir");

    let rig = Rig::new();
    rig.vm.register(
        "root",
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::host::testing::pane_order_host(),
        )) as Arc<dyn Volume>,
    );
    let at = |name: &str| root.join(name).to_string_lossy().into_owned();
    let folder_rename = JournalItem {
        entry_type: EntryType::Dir,
        size: None,
        mtime: None,
        ..file_unit(1, "root", &at("Photos"), "root", &at("photos"), 0)
    };
    rig.seed(
        "op",
        OpKind::Rename,
        "root",
        Some("root"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "root", &at("IMG_0001.JPG"), "root", &at("img_0001.jpg"), 4),
            folder_rename,
        ],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (2, 0), "got {report:?}");
    let names: Vec<String> = std::fs::read_dir(root)
        .expect("list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"IMG_0001.JPG".to_string()), "got {names:?}");
    assert!(names.contains(&"Photos".to_string()), "got {names:?}");
}

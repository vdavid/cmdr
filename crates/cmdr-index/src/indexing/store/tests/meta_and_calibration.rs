//! The `meta` table: the markers that gate auto-resume on connect, the volume
//! path, and the per-walk-kind scan calibration buckets.

use super::*;

/// `persisted_scan_completed` is the on-connect auto-resume gate: it reports
/// `true` ONLY for a DB that recorded a completed scan (the "the user enabled
/// indexing for this volume and it finished at least once" signal). A missing
/// file, a fresh DB with no completed scan, and an unreadable path all read
/// `false`, so a never-enabled SMB share is never auto-indexed on connect.
#[test]
fn persisted_scan_completed_reflects_the_marker() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-test.db");

    // No file yet ⇒ never enabled.
    assert!(
        !IndexStore::persisted_scan_completed(&db_path),
        "a missing DB must read as not-yet-enabled"
    );

    // A fresh DB with no completed scan ⇒ still not the resume signal (the user
    // may have started an enable that never finished; don't auto-resume it).
    let store = IndexStore::open(&db_path).expect("open store");
    drop(store);
    assert!(
        !IndexStore::persisted_scan_completed(&db_path),
        "a DB with no scan_completed_at must read as not-enabled"
    );

    // Stamp a completed scan ⇒ the resume signal.
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan_completed_at");
    drop(conn);
    assert!(
        IndexStore::persisted_scan_completed(&db_path),
        "a completed scan must read as enabled (auto-resume on connect)"
    );
}

/// The sticky `user_disabled` marker round-trips and, combined with a completed
/// scan, gates auto-resume: set ⇒ suppress resume even with a completed scan;
/// cleared ⇒ resume again. This is what makes "turn off indexing for this drive"
/// survive a reconnect (the DB stays on disk for a fast re-enable, but the marker
/// records intent).
#[test]
fn user_disabled_marker_gates_auto_resume() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-test.db");

    // Absent DB / marker ⇒ not disabled.
    assert!(!IndexStore::user_disabled(&db_path), "no DB ⇒ not disabled");

    // A completed scan with no marker is eligible to auto-resume.
    let store = IndexStore::open(&db_path).expect("open store");
    drop(store);
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan");
    drop(conn);
    assert!(IndexStore::persisted_scan_completed(&db_path));
    assert!(!IndexStore::user_disabled(&db_path), "fresh index isn't user-disabled");

    // Turn indexing off ⇒ marker set ⇒ no auto-resume (even though a scan completed).
    IndexStore::set_user_disabled(&db_path, true).expect("set marker");
    assert!(IndexStore::user_disabled(&db_path), "marker must persist");
    assert!(
        IndexStore::persisted_scan_completed(&db_path),
        "the completed-scan fact is untouched by the disable marker (DB preserved for fast resume)"
    );

    // Re-enable ⇒ marker cleared ⇒ eligible again.
    IndexStore::set_user_disabled(&db_path, false).expect("clear marker");
    assert!(!IndexStore::user_disabled(&db_path), "re-enable clears the marker");
}

#[test]
fn meta_roundtrip() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    IndexStore::update_meta(&write_conn, "volume_path", "/").unwrap();
    IndexStore::update_meta(&write_conn, "scan_duration_ms", "1234").unwrap();

    let val = IndexStore::get_meta(&write_conn, "volume_path").unwrap();
    assert_eq!(val.as_deref(), Some("/"));

    let status = store.get_index_status().unwrap();
    assert_eq!(status.volume_path.as_deref(), Some("/"));
    assert_eq!(status.scan_duration_ms.as_deref(), Some("1234"));
}

/// `set_volume_path` heals an index DB that has no `volume_path` meta (the shape a
/// real SMB index has — only the local scan-completion path ever wrote it), so
/// search can strip the mount root off scope paths without a rescan.
#[test]
fn set_volume_path_heals_a_db_missing_it() {
    let (store, _dir) = open_temp_store();
    let db_path = store.db_path().to_path_buf();

    // A fresh DB has no volume_path meta.
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    assert_eq!(IndexStore::get_meta(&conn, "volume_path").unwrap(), None);
    drop(conn);

    IndexStore::set_volume_path(&db_path, "/Volumes/naspi").unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    assert_eq!(
        IndexStore::get_meta(&conn, "volume_path").unwrap().as_deref(),
        Some("/Volumes/naspi")
    );
}

#[test]
fn read_scan_calibration_reads_seeded_keys() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    IndexStore::update_meta(&write_conn, "total_entries", "5000000").unwrap();
    IndexStore::update_meta(&write_conn, "total_physical_bytes", "905000000000").unwrap();
    IndexStore::update_meta(&write_conn, "scan_duration_ms", "149000").unwrap();

    let calibration = IndexStore::read_scan_calibration_set(&write_conn).unwrap().any;
    assert_eq!(calibration.total_entries, Some(5_000_000));
    assert_eq!(calibration.total_physical_bytes, Some(905_000_000_000));
    assert_eq!(calibration.scan_duration_ms, Some(149_000));
}

#[test]
fn read_scan_calibration_missing_keys_are_none() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // Fresh DB: none of the calibration keys exist yet.
    let set = IndexStore::read_scan_calibration_set(&conn).unwrap();
    assert_eq!(set, ScanCalibrationSet::default());
    assert_eq!(set.any.total_entries, None);
    assert_eq!(set.any.total_physical_bytes, None);
    assert_eq!(set.any.scan_duration_ms, None);

    // A non-numeric value also maps to None (parse failure), not an error.
    IndexStore::update_meta(&conn, "total_entries", "not-a-number").unwrap();
    let set = IndexStore::read_scan_calibration_set(&conn).unwrap();
    assert_eq!(set.any.total_entries, None);
}

/// Every completed scan writes BOTH its own walk-kind bucket and the unsuffixed
/// last-scan one, and the reader hands each back separately.
#[test]
fn read_scan_calibration_set_reads_each_bucket() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    IndexStore::update_meta(&conn, "scan_duration_ms_full_walk", "180000").unwrap();
    IndexStore::update_meta(&conn, "total_entries_full_walk", "5000000").unwrap();
    IndexStore::update_meta(&conn, "scan_duration_ms_change_check", "1180696").unwrap();
    IndexStore::update_meta(&conn, "total_entries_change_check", "5100000").unwrap();
    IndexStore::update_meta(&conn, "scan_duration_ms", "1180696").unwrap();
    IndexStore::update_meta(&conn, "total_entries", "5100000").unwrap();

    let set = IndexStore::read_scan_calibration_set(&conn).unwrap();
    assert_eq!(set.full_walk.scan_duration_ms, Some(180_000));
    assert_eq!(set.full_walk.total_entries, Some(5_000_000));
    assert_eq!(set.change_check.scan_duration_ms, Some(1_180_696));
    assert_eq!(set.any.scan_duration_ms, Some(1_180_696));
}

/// The whole point of the split: a full walk must be timed off the last FULL
/// WALK, never off the change check that ran more recently and takes ~5x longer.
#[test]
fn calibration_for_kind_prefers_the_same_kind() {
    let set = ScanCalibrationSet {
        full_walk: ScanCalibration {
            total_entries: Some(5_000_000),
            total_physical_bytes: Some(905_000_000_000),
            scan_duration_ms: Some(180_000),
        },
        change_check: ScanCalibration {
            total_entries: Some(5_100_000),
            total_physical_bytes: Some(910_000_000_000),
            scan_duration_ms: Some(1_180_696),
        },
        // The change check ran last, so it also owns the unsuffixed keys.
        any: ScanCalibration {
            total_entries: Some(5_100_000),
            total_physical_bytes: Some(910_000_000_000),
            scan_duration_ms: Some(1_180_696),
        },
    };

    assert_eq!(
        set.for_kind(ScanCalibrationKind::FullWalk).scan_duration_ms,
        Some(180_000),
        "a full walk must be timed off the last full walk, not off the slower change check"
    );
    assert_eq!(
        set.for_kind(ScanCalibrationKind::ChangeCheck).scan_duration_ms,
        Some(1_180_696)
    );
}

/// No same-kind sample yet (the first-ever change check): the other walk's
/// timing is wrong-ish but honest company, and beats showing no estimate.
#[test]
fn calibration_for_kind_falls_back_to_the_last_scan_of_any_kind() {
    let last_full_walk = ScanCalibration {
        total_entries: Some(5_000_000),
        total_physical_bytes: Some(905_000_000_000),
        scan_duration_ms: Some(180_000),
    };
    let set = ScanCalibrationSet {
        full_walk: last_full_walk,
        change_check: ScanCalibration::default(),
        any: last_full_walk,
    };

    assert_eq!(set.for_kind(ScanCalibrationKind::ChangeCheck), last_full_walk);
}

/// A brand-new index has nothing to calibrate from, and says so (the caller then
/// falls back to the rough, untimed tier) rather than inventing a number.
#[test]
fn calibration_for_kind_is_empty_when_nothing_is_recorded() {
    let set = ScanCalibrationSet::default();
    assert!(set.for_kind(ScanCalibrationKind::FullWalk).is_empty());
    assert!(set.for_kind(ScanCalibrationKind::ChangeCheck).is_empty());
}

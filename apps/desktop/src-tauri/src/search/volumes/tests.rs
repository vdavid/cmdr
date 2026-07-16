//! Per-volume registry tests: DB-filename parsing, indexed-volume enumeration,
//! loading a non-root volume straight from its persisted DB (mount root from meta),
//! the missing-DB honesty signal, and per-volume importance weight loading.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::indexing::store::{IndexStore, ROOT_ID};

use super::*;

// ── Fixtures ─────────────────────────────────────────────────────────

/// Write a small index DB at `data_dir/index-{volume_id}.db` with a couple of
/// entries under a mount root, stamping the `volume_path` meta the loader reads.
/// Returns nothing; the file is what the loader consumes.
fn make_index_db(data_dir: &Path, volume_id: &str, volume_path: &str) {
    let db_path = data_dir.join(format!("index-{volume_id}.db"));
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, "volume_path", volume_path).expect("meta");
    let sub = IndexStore::insert_entry_v2(&conn, ROOT_ID, "sub", true, false, None, None, None, None).unwrap();
    IndexStore::insert_entry_v2(
        &conn,
        sub,
        "report.pdf",
        false,
        false,
        Some(10),
        Some(10),
        Some(1000),
        None,
    )
    .unwrap();
}

/// Write a populated `importance-{volume_id}.db` via the real writer.
fn make_importance_db(data_dir: &Path, volume_id: &str, rows: &[(&str, f64)]) {
    use crate::importance::store::importance_db_path;
    use crate::importance::writer::{ImportanceWriter, WeightRow};
    let db_path = importance_db_path(data_dir, volume_id);
    let writer = ImportanceWriter::spawn(&db_path).expect("spawn writer");
    let weight_rows: Vec<WeightRow> = rows
        .iter()
        .map(|(path, score)| WeightRow {
            path: path.to_string(),
            score: *score,
            signals_json: "{}".to_string(),
        })
        .collect();
    writer.write_weights(1, weight_rows).expect("write");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

// ── Filename parsing ─────────────────────────────────────────────────

#[test]
fn parses_volume_id_from_index_db_filename() {
    assert_eq!(volume_id_from_index_db("index-root.db"), Some("root"));
    assert_eq!(volume_id_from_index_db("index-smb-nas.db"), Some("smb-nas"));
    // A volume id containing '-' (an MTP serial) survives the prefix/suffix strip.
    assert_eq!(volume_id_from_index_db("index-mtp-AABB-1.db"), Some("mtp-AABB-1"));
    // Sidecars and unrelated files aren't index DBs.
    assert_eq!(volume_id_from_index_db("index-root.db-wal"), None);
    assert_eq!(volume_id_from_index_db("history.db"), None);
}

// ── Indexed-volume enumeration ───────────────────────────────────────

#[test]
fn enumerates_indexed_volumes_root_first() {
    let dir = tempfile::tempdir().expect("temp dir");
    // Two external index DBs plus root, a sidecar, and a non-index file.
    make_index_db(dir.path(), ROOT_VOLUME_ID, "/");
    make_index_db(dir.path(), "smb-nas", "/Volumes/nas");
    make_index_db(dir.path(), "mtp-phone-1", "mtp://phone/1");
    std::fs::write(dir.path().join("index-root.db-wal"), b"x").ok();
    std::fs::write(dir.path().join("notes.txt"), b"x").ok();

    let ids = indexed_volume_ids_in(dir.path());
    assert_eq!(ids[0], ROOT_VOLUME_ID, "root is always first");
    assert!(ids.contains(&"smb-nas".to_string()));
    assert!(ids.contains(&"mtp-phone-1".to_string()));
    assert_eq!(
        ids.iter().filter(|id| *id == ROOT_VOLUME_ID).count(),
        1,
        "root listed once"
    );
    assert_eq!(ids.len(), 3, "root + two externals, sidecar/non-index ignored");
}

#[test]
fn enumeration_of_empty_dir_is_just_root() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert_eq!(indexed_volume_ids_in(dir.path()), vec![ROOT_VOLUME_ID.to_string()]);
}

// ── Loading a non-root volume from its persisted DB ──────────────────

#[test]
fn loads_non_root_volume_with_mount_root_from_meta() {
    let dir = tempfile::tempdir().expect("temp dir");
    make_index_db(dir.path(), "smb-nas", "/Volumes/nas");

    let cancel = AtomicBool::new(false);
    let loaded = match load_volume_blocking("smb-nas", dir.path(), &cancel) {
        VolumeLoad::Loaded(v) => v,
        other => panic!("expected Loaded, got {}", describe(&other)),
    };
    // The mount root comes from the DB's `volume_path` meta — known without the
    // volume being mounted or registered.
    assert_eq!(loaded.mount_root.as_deref(), Some("/Volumes/nas"));
    // Root sentinel + `sub` + `report.pdf`.
    assert_eq!(loaded.index.entries.len(), 3);
}

#[test]
fn missing_index_db_is_not_indexed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cancel = AtomicBool::new(false);
    // No index-smb-ghost.db on disk ⇒ the honest "not covered" signal, not a
    // silent empty success.
    assert!(matches!(
        load_volume_blocking("smb-ghost", dir.path(), &cancel),
        VolumeLoad::NotIndexed
    ));
}

// ── Per-volume importance weights ────────────────────────────────────

#[test]
fn loads_per_volume_importance_weights() {
    let dir = tempfile::tempdir().expect("temp dir");
    make_index_db(dir.path(), "smb-weighted", "/Volumes/w");
    make_importance_db(dir.path(), "smb-weighted", &[("/proj", 0.9), ("/node_modules", 0.0)]);

    let cancel = AtomicBool::new(false);
    assert!(matches!(
        load_volume_blocking("smb-weighted", dir.path(), &cancel),
        VolumeLoad::Loaded(_)
    ));
    let weights = weights_for("smb-weighted");
    assert_eq!(weights.weight_for("/proj"), 0.9);
    assert_eq!(weights.weight_for("/node_modules"), 0.0, "floored folder unscored");
    assert_eq!(weights.weight_for("/unknown"), 0.0, "unknown path ⇒ neutral");
}

#[test]
fn volume_without_importance_db_degrades_to_empty_weights() {
    let dir = tempfile::tempdir().expect("temp dir");
    make_index_db(dir.path(), "smb-noweights", "/Volumes/nw");

    let cancel = AtomicBool::new(false);
    assert!(matches!(
        load_volume_blocking("smb-noweights", dir.path(), &cancel),
        VolumeLoad::Loaded(_)
    ));
    assert!(
        weights_for("smb-noweights").is_empty(),
        "no importance.db ⇒ empty weights"
    );
}

// ── Recompute notification refreshes root weights ────────────────────

/// A recompute completing fires the volume's `watch`, and the next weight reload
/// picks up the freshly-written weights — the subscribe-don't-poll contract the
/// root importance subscriber relies on. Uses `has_changed()` (no await) so it
/// stays a plain sync test; the `watch` sender flips the flag on notify.
#[test]
fn recompute_notification_lets_the_next_reload_see_new_weights() {
    let dir = tempfile::tempdir().expect("temp dir");
    let vid = "smb-recompute";

    // First pass: an early weight, loaded into the snapshot.
    make_importance_db(dir.path(), vid, &[("/proj", 0.4)]);
    store_weights(vid, load_weights(dir.path(), vid));
    assert_eq!(weights_for(vid).weight_for("/proj"), 0.4);

    // A subscriber observes the recompute notification, then reloads and sees the
    // second pass's higher weight.
    let mut rx = crate::importance::read::subscribe(vid);
    rx.borrow_and_update();
    make_importance_db(dir.path(), vid, &[("/proj", 0.95)]);
    crate::importance::read::notify_recompute_completed_for_test(vid, 2);
    assert!(rx.has_changed().expect("sender alive"), "the notification fired");
    rx.borrow_and_update();
    store_weights(vid, load_weights(dir.path(), vid));
    assert_eq!(
        weights_for(vid).weight_for("/proj"),
        0.95,
        "the next reload after a recompute sees the new weights"
    );
}

// ── Mount-root fallback to the live volume registry ──────────────────

/// Write an index DB with entries but WITHOUT the `volume_path` meta — the shape a
/// real SMB index has (older DBs never wrote it). Mount root must then come from the
/// live volume registry.
fn make_index_db_without_volume_path(data_dir: &Path, volume_id: &str) {
    let db_path = data_dir.join(format!("index-{volume_id}.db"));
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::insert_entry_v2(&conn, ROOT_ID, "sub", true, false, None, None, None, None).unwrap();
}

/// A non-root index whose DB has no `volume_path` meta still recovers its mount root
/// from the live `VolumeManager` while the volume is mounted (the live-QA bug: real
/// SMB DBs have no `volume_path`, so the loader returned `None` and scope stripping
/// failed → 0 results). Regression: mount root resolves via the registry fallback.
#[test]
fn mount_root_falls_back_to_the_volume_registry() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;
    use std::sync::Arc;

    let dir = tempfile::tempdir().expect("temp dir");
    let vid = "smb-fallback-test";
    let root = "/Volumes/cmdr-fallback-nas";
    make_index_db_without_volume_path(dir.path(), vid);

    let manager = get_volume_manager();
    manager.register(vid, Arc::new(LocalPosixVolume::new("Fallback", root)));

    let cancel = AtomicBool::new(false);
    let loaded = match load_volume_blocking(vid, dir.path(), &cancel) {
        VolumeLoad::Loaded(v) => v,
        other => {
            manager.unregister(vid);
            panic!("expected Loaded, got {}", describe(&other));
        }
    };
    assert_eq!(
        loaded.mount_root.as_deref(),
        Some(root),
        "mount root recovered from the live volume registry when the meta is absent"
    );
    manager.unregister(vid);
}

fn describe(load: &VolumeLoad) -> String {
    match load {
        VolumeLoad::Loaded(_) => "Loaded".to_string(),
        VolumeLoad::NotIndexed => "NotIndexed".to_string(),
        VolumeLoad::Failed(e) => format!("Failed({e})"),
    }
}

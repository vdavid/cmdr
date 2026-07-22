//! Integration test: scan a live SMB fixture share over the `Volume` trait and
//! assert the index reflects its contents.
//!
//! Gated `#[ignore]` like the other Docker SMB tests, so a default
//! `cargo nextest run` skips it. The `desktop-rust-integration-tests` check
//! lane (and `./apps/desktop/test/smb-servers/start.sh` locally) brings up the
//! `core` containers; then:
//!
//! ```sh
//! cargo nextest run smb_integration_volume_scan --run-ignored all
//! ```
//!
//! This is the live half of the `Volume`-trait scanner's coverage; the
//! backend-agnostic half (writer/aggregator reuse, cancellation) is the
//! in-memory `network_scanner::tests`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::file_system::volume::smb::{SmbConnectionParams, connect_smb_volume};
use crate::file_system::volume::{Volume, smb_volume_id};
use crate::indexing::network_scanner::scan_volume_via_trait;
use crate::indexing::scanner::ScanProgress;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::writer::IndexWriter;

fn guest_port() -> u16 {
    std::env::var("SMB_CONSUMER_GUEST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10480)
}

/// Connect to the Docker `public` guest share (mirrors `smb_test_support`,
/// which is private to the `smb` module).
async fn connect_public() -> Arc<dyn Volume> {
    let port = guest_port();
    let volume_id = smb_volume_id("127.0.0.1", port, "public");
    let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
    let vol = connect_smb_volume("public", "/tmp/smb-test-mount", &volume_id, params)
        .await
        .unwrap_or_else(|e| panic!("Failed to connect to Docker SMB container at 127.0.0.1:{port}: {e:?}"));
    Arc::new(vol)
}

fn unique_dir() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static N: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after UNIX_EPOCH")
        .as_nanos();
    format!(
        "cmdr-index-test-{}-{}-{}",
        std::process::id(),
        ts,
        N.fetch_add(1, Ordering::Relaxed)
    )
}

async fn rm_rf(vol: &dyn Volume, dir: &str) {
    if vol.exists(Path::new(dir)).await
        && let Ok(entries) = vol.list_directory(Path::new(dir), None).await
    {
        for e in entries {
            let child = format!("{dir}/{}", e.name);
            if e.is_directory {
                Box::pin(rm_rf(vol, &child)).await;
            } else {
                let _ = vol.delete(Path::new(&child)).await;
            }
        }
        let _ = vol.delete(Path::new(dir)).await;
    }
}

fn progress() -> Arc<ScanProgress> {
    Arc::new(ScanProgress::new())
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_volume_scan_indexes_share() {
    let vol = connect_public().await;

    // Seed a known subtree on the share:
    //   <base>/sub/leaf.txt   (11 bytes)
    //   <base>/top.txt        (5 bytes)
    let base = format!("/{}", unique_dir());
    rm_rf(vol.as_ref(), &base).await;
    vol.create_directory(Path::new(&base))
        .await
        .expect("create base dir on share");
    let sub = format!("{base}/sub");
    vol.create_directory(Path::new(&sub)).await.expect("create sub dir");
    vol.create_file(Path::new(&format!("{sub}/leaf.txt")), b"hello world")
        .await
        .expect("create leaf.txt");
    vol.create_file(Path::new(&format!("{base}/top.txt")), b"hello")
        .await
        .expect("create top.txt");

    // Scan ONLY the seeded subtree (the share is shared across tests, so we
    // scope to our base dir as the scan root — `scan_volume_via_trait` maps the
    // scan root to ROOT_ID exactly like a volume-root scan).
    let dir = tempfile::tempdir().expect("temp db dir");
    let db_path = dir.path().join("smb-scan.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(
        Arc::clone(&vol),
        PathBuf::from(&base),
        writer.clone(),
        progress(),
        cancelled,
        crate::indexing::network_scanner::scan_pace::ScanPacer::unpaced(),
    )
    .await
    .expect("SMB volume scan should complete");

    assert!(!summary.was_cancelled);
    assert_eq!(summary.total_entries, 3, "sub/ + leaf.txt + top.txt");
    assert_eq!(summary.total_dirs, 1, "just sub/");

    writer.flush().await.expect("flush");
    writer.shutdown();

    // The index must reflect the share's contents.
    let store = IndexStore::open(&db_path).expect("reopen store");
    let children = store.list_children(ROOT_ID).expect("list root");
    assert_eq!(children.len(), 2, "scan root has sub/ and top.txt");
    let sub_entry = children.iter().find(|e| e.name == "sub").expect("sub dir indexed");
    assert!(sub_entry.is_directory);
    let top = children.iter().find(|e| e.name == "top.txt").expect("top.txt indexed");
    assert_eq!(top.logical_size, Some(5), "size comes from SMB stat");

    let sub_children = store.list_children(sub_entry.id).expect("list sub");
    assert_eq!(sub_children.len(), 1);
    assert_eq!(sub_children[0].name, "leaf.txt");
    assert_eq!(sub_children[0].logical_size, Some(11));

    // Clean up the share so reruns start fresh.
    rm_rf(vol.as_ref(), &base).await;
}

/// The live watch→index path: scan a fixture share, then MUTATE it and feed the
/// change through the real translator (`transports/smb/watch`), asserting the index reflects
/// the mutation — the "scan a share, mutate it, assert the index reflects the
/// change while Fresh" requirement. No pane/listing is involved, so it
/// also pins the volume-index-scoped (not pane-scoped) behavior: the index
/// updates from a watch event with zero open listings.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_watch_event_updates_index() {
    use super::watch::resolve_and_send_for_test;
    use crate::file_system::listing::caching::DirectoryChange;

    let vol = connect_public().await;

    // Seed and scan a known subtree (same shape as the scan test).
    let base = format!("/{}", unique_dir());
    rm_rf(vol.as_ref(), &base).await;
    vol.create_directory(Path::new(&base))
        .await
        .expect("create base dir on share");
    vol.create_file(Path::new(&format!("{base}/top.txt")), b"hello")
        .await
        .expect("create top.txt");

    let dir = tempfile::tempdir().expect("temp db dir");
    let db_path = dir.path().join("smb-watch.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let cancelled = Arc::new(AtomicBool::new(false));
    scan_volume_via_trait(
        Arc::clone(&vol),
        PathBuf::from(&base),
        writer.clone(),
        progress(),
        cancelled,
        crate::indexing::network_scanner::scan_pace::ScanPacer::unpaced(),
    )
    .await
    .expect("scan should complete");
    writer.flush().await.expect("flush after scan");

    // ── Mutation 1: add a file. The watcher would stat it and emit Added. ──
    let added_path = format!("{base}/added.txt");
    vol.create_file(Path::new(&added_path), b"twelve bytes")
        .await
        .expect("create added.txt on share");
    let added_meta = vol.get_metadata(Path::new(&added_path)).await.expect("stat added.txt");

    // The watcher delivers a mount-absolute parent path; the index ROOT_ID is the
    // scan root (`base`), so `resolve_and_send_for_test` strips `base` to `/`.
    {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let sent = resolve_and_send_for_test(
            &conn,
            &writer,
            &base, // mount root == scan root
            &base, // parent dir of added.txt is the root
            &DirectoryChange::Added(added_meta),
        );
        assert!(sent, "Added must translate to an index write");
    }
    writer.flush().await.expect("flush after add");

    {
        let store = IndexStore::open(&db_path).expect("reopen after add");
        let children = store.list_children(ROOT_ID).expect("list root");
        let added = children
            .iter()
            .find(|e| e.name == "added.txt")
            .expect("added.txt now in the index");
        assert_eq!(added.logical_size, Some(12), "size came from the SMB stat");
        // The writer auto-propagated the new file's size into the root dir_stats.
        let root_stats = IndexStore::get_dir_stats_by_id(store.read_conn(), ROOT_ID)
            .expect("root stats")
            .expect("root has stats");
        assert!(
            root_stats.recursive_logical_size >= 12,
            "root recursive size must include the added file (got {})",
            root_stats.recursive_logical_size,
        );
    }

    // ── Mutation 2: delete top.txt. The watcher emits Removed by name. ──
    vol.delete(Path::new(&format!("{base}/top.txt")))
        .await
        .expect("delete top.txt on share");
    {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let sent = resolve_and_send_for_test(
            &conn,
            &writer,
            &base,
            &base,
            &DirectoryChange::Removed("top.txt".to_string()),
        );
        assert!(sent, "Removed of an indexed file must translate to a delete");
    }
    writer.flush().await.expect("flush after delete");

    {
        let store = IndexStore::open(&db_path).expect("reopen after delete");
        let children = store.list_children(ROOT_ID).expect("list root");
        assert!(
            children.iter().all(|e| e.name != "top.txt"),
            "top.txt must be gone from the index after the Removed event",
        );
    }

    writer.shutdown();
    rm_rf(vol.as_ref(), &base).await;
}

/// The READ-side mirror of `smb_integration_volume_scan_indexes_share`: scan a
/// fixture share, then enrich a listing of it and assert directory sizes appear.
///
/// This is the regression test for the SMB read-side gap: an SMB
/// index's `ROOT_ID` is the mount root, so enrichment must strip the mount root
/// to a mount-relative path (`index_read_path` / `index_relative_path`) before
/// `resolve_path`. Without that transform, a mount-absolute parent resolves to
/// nothing and dir sizes never appear. We drive `enrich_via_parent_id_on` (the
/// fast path) directly against the freshly-scanned index with the mount-relative
/// parent the read path now computes, so the test needs no `VolumeManager` /
/// registry — the same isolation `resolve_and_send_for_test` buys the write side.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_enrich_listing_shows_sizes() {
    use super::watch::index_relative_path;
    use crate::file_system::listing::FileEntry;
    use crate::indexing::enrichment::enrich_via_parent_id_on;

    let vol = connect_public().await;

    // Seed a known subtree:  <base>/sub/ (a dir, with a 11-byte leaf inside)
    //                        <base>/top.txt (5 bytes)
    let base = format!("/{}", unique_dir());
    rm_rf(vol.as_ref(), &base).await;
    vol.create_directory(Path::new(&base))
        .await
        .expect("create base dir on share");
    let sub = format!("{base}/sub");
    vol.create_directory(Path::new(&sub)).await.expect("create sub dir");
    vol.create_file(Path::new(&format!("{sub}/leaf.txt")), b"hello world")
        .await
        .expect("create leaf.txt");
    vol.create_file(Path::new(&format!("{base}/top.txt")), b"hello")
        .await
        .expect("create top.txt");

    // Scan the subtree (scan root → ROOT_ID, like a real volume-root scan).
    let dir = tempfile::tempdir().expect("temp db dir");
    let db_path = dir.path().join("smb-enrich.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    let cancelled = Arc::new(AtomicBool::new(false));
    scan_volume_via_trait(
        Arc::clone(&vol),
        PathBuf::from(&base),
        writer.clone(),
        progress(),
        cancelled,
        crate::indexing::network_scanner::scan_pace::ScanPacer::unpaced(),
    )
    .await
    .expect("SMB volume scan should complete");
    writer.flush().await.expect("flush after scan");
    writer.shutdown();

    // Build a listing of the share's mount-absolute children, as the live pane
    // would: `sub/` (a directory, the one that should get a recursive size) and
    // `top.txt` (a file, which never gets a recursive size).
    let mut entries = vec![
        {
            let p = format!("{base}/sub");
            FileEntry::new("sub".to_string(), p, true, false)
        },
        {
            let p = format!("{base}/top.txt");
            FileEntry::new("top.txt".to_string(), p, false, false)
        },
    ];

    // The mount-relative parent the read path computes: `base` IS the mount root
    // here, so the listing parent (`base`) maps to `/` in the index path space.
    let index_parent = index_relative_path(&base, &base).expect("base maps to the index root");
    assert_eq!(index_parent, "/", "the mount root maps to the index ROOT_ID path");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    enrich_via_parent_id_on(&mut entries, &conn, &index_parent, 1).expect("enrichment must succeed");

    // The directory `sub/` must now carry a recursive size from its index: it
    // contains leaf.txt (11 bytes). Pre-fix this assertion failed — the
    // mount-absolute parent resolved to nothing and no size was applied.
    let sub_entry = entries.iter().find(|e| e.name == "sub").expect("sub in listing");
    assert_eq!(
        sub_entry.recursive_size,
        Some(11),
        "the indexed SMB directory must enrich to its recursive size",
    );

    // Clean up the share.
    let vol2 = connect_public().await;
    rm_rf(vol2.as_ref(), &base).await;
}

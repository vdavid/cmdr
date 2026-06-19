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
//! in-memory `volume_scanner::tests`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::file_system::volume::smb::{SmbConnectionParams, connect_smb_volume};
use crate::file_system::volume::{Volume, smb_volume_id};
use crate::indexing::scanner::ScanProgress;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::volume_scanner::scan_volume_via_trait;
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
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
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

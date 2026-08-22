//! Integration test for the fresh-listing oracle layered on top of
//! `SmbVolume::scan_for_copy_batch`.
//!
//! Pinned scenario: when the parent listing is watcher-backed, the SMB batch
//! scan resolves child sizes from the cache and skips the pipelined-stat
//! path entirely. We prove this by **dropping the smb2 session before the
//! scan**: if the oracle handled every path, no `tree.stat` call happens and
//! the scan still succeeds. If the oracle misses, the pipelined-stat block
//! tries to acquire the client mutex, finds `None`, and returns
//! `VolumeError::DeviceDisconnected`. That's a strong signal that doesn't
//! require plumbing a call counter through the smb2 client.
//!
//! Gated like the other Docker SMB tests with `#[ignore]` so a regular
//! `cargo nextest run` won't fail when no containers are running. Run via
//! `cargo nextest run smb_integration_scan --run-ignored all` after starting
//! `apps/desktop/test/smb-servers/start.sh`. The `smb_integration_` prefix is
//! load-bearing on this side of the seam: `desktop-rust-integration-tests`
//! selects the app crate's Docker cells by name, so a cell without it never
//! runs in CI. `fixture-lane-coverage` fails on one that lacks it.

use std::path::PathBuf;
use std::sync::Arc;

use crate::file_system::listing::caching_test_support::{TestListing, TestListingGuard};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::volume::Volume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::smb::{SmbConnectionParams, SmbVolume, connect_smb_volume};
use crate::file_system::volume::smb_volume_id;

fn make_file_entry(name: &str, parent: &str, size: u64) -> FileEntry {
    FileEntry {
        size: Some(size),
        permissions: 0o644,
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            name.to_string(),
            format!("{}/{}", parent.trim_end_matches('/'), name),
            false,
            false,
        )
    }
}

/// Seeds a cached listing owned by this test; the guard tears it down on drop,
/// unwind included.
fn insert_listing(tag: &str, volume_id: &str, path: &str, entries: Vec<FileEntry>) -> TestListingGuard {
    TestListing::new()
        .volume(volume_id)
        .path(path)
        .entries(entries)
        .sequence(1)
        .insert(tag)
}

async fn make_docker_volume() -> SmbVolume {
    let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10480);
    let volume_id = smb_volume_id("127.0.0.1", port, "public");
    let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
    connect_smb_volume(
        "public",
        "/tmp/smb-test-mount",
        &volume_id,
        params,
        crate::volume_host::host(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to connect to Docker SMB container at 127.0.0.1:{port}. Is it running? ({e:?})"))
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_scan_uses_oracle_on_hit_skips_stat_pipeline() {
    let vol = Arc::new(make_docker_volume().await);
    let vid = vol.volume_id().to_string();

    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Cache a listing for a synthetic parent path. The actual share doesn't
    // need to host these files: the oracle short-circuit reads from cache,
    // and we'll drop the SMB session so any fallthrough would fail.
    let parent = "/Volumes/TestShare/oracle-test";
    let cached = vec![
        make_file_entry("a.bin", parent, 4096),
        make_file_entry("b.bin", parent, 8192),
    ];
    let _listing = insert_listing("smb-oracle-hit", &vid, parent, cached);

    // Now break the session. With no client, any pipelined stat fails with
    // DeviceDisconnected. The oracle path doesn't acquire the client lock at
    // all for served paths, so it survives.
    vol.detach_session_for_test().await;

    let paths = vec![
        PathBuf::from(format!("{}/a.bin", parent)),
        PathBuf::from(format!("{}/b.bin", parent)),
    ];

    let result = vol
        .scan_for_copy_batch(&paths)
        .await
        .expect("oracle-served batch scan should succeed with no SMB session");

    assert_eq!(result.aggregate.file_count, 2);
    assert_eq!(
        result.aggregate.total_bytes, 12288,
        "size should come from cached entries, not real SMB stat"
    );
    assert_eq!(result.per_path.len(), 2);

    get_volume_manager().unregister(&vid);
    // No need to clean up the share: nothing was written.
    // Note: we don't transition the volume back to Direct here. The volume is
    // about to be dropped (Arc), and `Drop` doesn't restart the session.
}

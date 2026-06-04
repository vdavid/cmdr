//! Shared test helpers for the SMB backend test suites.
//!
//! Lives as a `#[cfg(test)]` submodule of `smb` so the helpers can reach
//! `SmbVolume`'s private fields and `connect_smb_volume` via `super::*`, and
//! so the sibling test modules (`smb_test`, `smb_integration_test`,
//! `smb_soak_test`) can share them through `super::smb_test_support::*`
//! without duplicating Docker-connection and byte-integrity plumbing.

use super::*;
use crate::file_system::volume::smb_volume_id;

/// Connects to the Docker smb-guest container (share "public"). Default port
/// 10480 matches smb2's guest test container; override with
/// `SMB_CONSUMER_GUEST_PORT` to match `smb2::testing::guest_port()`.
pub(super) async fn make_docker_volume() -> SmbVolume {
    let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10480);
    let volume_id = smb_volume_id("127.0.0.1", port, "public");
    let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
    connect_smb_volume("public", "/tmp/smb-test-mount", &volume_id, params)
        .await
        .unwrap_or_else(|e| {
            panic!("Failed to connect to Docker SMB container at 127.0.0.1:{port}. Is it running? ({e:?})")
        })
}

/// Unique directory name for test isolation.
///
/// Combines the PID, a nanosecond timestamp, and a process-wide atomic
/// counter so that tests running in parallel never collide: neither
/// within one process (the nanosecond clock resolution isn't fine enough
/// on its own) nor across the separate processes nextest forks per test
/// (where the static counter resets to 0 and two processes hitting the
/// same nanos window would otherwise produce identical names, leaving
/// stale directories on the SMB share for later runs to trip on).
pub(super) fn test_dir_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!("cmdr-test-{pid}-{ts}-{n}")
}

/// Ensures a test directory is clean before use (deletes recursively if it exists).
pub(super) async fn ensure_clean(vol: &SmbVolume, dir: &str) {
    if vol.exists(Path::new(dir)).await {
        // Delete contents recursively
        if let Ok(entries) = vol.list_directory_impl(Path::new(dir)).await {
            for entry in entries {
                let child = format!("{}/{}", dir, entry.name);
                if entry.is_directory {
                    Box::pin(ensure_clean(vol, &child)).await;
                } else {
                    let _ = vol.delete(Path::new(&child)).await;
                }
            }
        }
        let _ = vol.delete(Path::new(dir)).await;
    }
}

// ── Byte-level integrity helpers ────────────────────────────────
//
// Every SMB copy test that lands a file on a destination hashes the
// source bytes and the destination bytes and compares the two. A
// pipeline bug that drops, duplicates, reorders, or reuses a chunk's
// buffer will change the hash; the old `bytes_written == expected`
// and `metadata.size == N` assertions would silently pass. blake3 is
// fast (well over a GB/s single-threaded), so the 20 MB streaming
// tests pay negligible hashing cost on top of the SMB RTTs.
//
// `hash_volume_file` streams the destination through `open_read_stream`
// so we also avoid buffering e.g. 20 MB into a `Vec<u8>` just to
// compare with `assert_eq!` (which on mismatch used to print an
// unreadable megabyte-sized diff). The hex-formatted hash in the
// assertion message is actionable on failure.

pub(super) fn hash_bytes(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

pub(super) async fn hash_volume_file(volume: &dyn Volume, path: &Path) -> [u8; 32] {
    let mut stream = volume
        .open_read_stream(path)
        .await
        .expect("open read stream for hashing");
    let mut hasher = blake3::Hasher::new();
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.expect("read chunk for hashing");
        hasher.update(&chunk);
    }
    *hasher.finalize().as_bytes()
}

// ── SMB streaming-write regression test ────────────────────────────
//
// Helpers + one `#[ignore]`d integration test that guards against the
// streaming-write deadlock fixed in commit `efb15479`. See the docstring
// on `smb_integration_concurrent_streaming_writes_no_deadlock` for the
// full story.

/// All test artifacts on the SMB share live under this prefix. The
/// cleanup helper refuses to delete anything that doesn't start with it.
pub(super) const TEST_PREFIX_ROOT: &str = "_test/cmdr-regression-";

/// Deletes every file under `unique_prefix_smb` and then the directory
/// itself. Safety: refuses any path that doesn't start with
/// `TEST_PREFIX_ROOT`, both at the top level and per entry, so a logic
/// bug in the caller can never reach outside the regression sandbox.
/// Called explicitly at the end of each pass (best effort: logs but
/// never overrides the test outcome).
pub(super) async fn cleanup_test_prefix(vol: &SmbVolume, mount_path: &Path, unique_prefix_smb: &str) {
    assert!(
        unique_prefix_smb.starts_with(TEST_PREFIX_ROOT),
        "cleanup_test_prefix: refusing to clean a prefix outside {TEST_PREFIX_ROOT:?}: {unique_prefix_smb:?}"
    );
    let dir_abs = mount_path.join(unique_prefix_smb.trim_start_matches('/'));
    let rel_of = |abs: &Path| -> String {
        abs.to_string_lossy()
            .strip_prefix(mount_path.to_string_lossy().as_ref())
            .map(|s| s.trim_start_matches('/').to_string())
            .unwrap_or_else(|| abs.to_string_lossy().to_string())
    };
    match vol.list_directory_impl(&dir_abs).await {
        Ok(entries) => {
            for entry in entries {
                let abs = dir_abs.join(&entry.name);
                let rel = rel_of(&abs);
                if !rel.starts_with(TEST_PREFIX_ROOT) {
                    log::warn!("cleanup_test_prefix: refusing to delete {rel} (outside prefix)");
                    continue;
                }
                if let Err(e) = vol.delete(&abs).await {
                    log::warn!("cleanup_test_prefix: failed to delete {rel}: {e:?}");
                }
            }
        }
        Err(e) => log::warn!("cleanup_test_prefix: list_directory_impl failed for {dir_abs:?}: {e:?}"),
    }
    let rel_dir = rel_of(&dir_abs);
    if rel_dir.starts_with(TEST_PREFIX_ROOT)
        && let Err(e) = vol.delete(&dir_abs).await
    {
        log::warn!("cleanup_test_prefix: failed to delete prefix dir {rel_dir}: {e:?}");
    }
}

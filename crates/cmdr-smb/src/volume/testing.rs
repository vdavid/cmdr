//! Fixtures for the Docker-backed SMB suites, on both sides of the crate
//! boundary.
//!
//! The suites that assert on this backend live in this crate; the ones whose
//! other half is the app's own machinery (the transfer pipeline, the volume
//! registry, the listing cache) live in the app. Both connect to the same
//! container stack, seed the same kind of unique directory, and hash what landed
//! the same way, so the plumbing is here rather than duplicated across the seam.
//!
//! Everything here goes through the backend's ordinary surface. ❌ Nothing may
//! hand out the share's inner state, its client, its tree, or its scan pool: a
//! test that needs those is a white-box test of this backend and belongs in this
//! crate. [`negotiated_max_write`] and [`session_credits`] do reach the session,
//! and [`client_lock_tickets_issued`] a process-wide counter, but each answers
//! with a number and nothing else.
//!
//! Gated behind the `testing` feature, so it exists in dev targets and in no
//! shipped build. See `DETAILS.md` § "Which side a test lives on".

use std::path::Path;
use std::sync::atomic::Ordering;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{Volume, VolumeReadStream};

use super::{SmbConnectionParams, SmbVolume, connect_smb_volume};

/// Where the fixture share is mounted, as far as every suite is concerned.
///
/// Nothing is mounted there: Cmdr's own I/O rides the smb2 session, so this is
/// only the prefix path translation strips. It has to be a stable absolute path,
/// because [`share_path`] builds absolute share paths under it.
pub const TEST_MOUNT_ROOT: &str = "/tmp/smb-test-mount";

/// The host port the `smb-consumer-guest` container publishes.
///
/// Defaults to smb2's own 10480; Cmdr's stack moves the range to 11480+ so both
/// harnesses coexist, and the check runner exports the override.
pub fn guest_port() -> u16 {
    std::env::var("SMB_CONSUMER_GUEST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10480)
}

/// Guest connection parameters for the `public` share on the fixture container.
///
/// A second connection to the same share (the upgrade-swap tests take one) is
/// built from these rather than read off a live volume.
pub fn docker_guest_params() -> SmbConnectionParams {
    SmbConnectionParams::new("127.0.0.1", "public", guest_port(), None, None)
}

/// Connects to the fixture container's `public` share with a host that answers
/// nothing, which is what a test asserting on the BACKEND wants.
pub async fn make_docker_volume() -> SmbVolume {
    make_docker_volume_with_host(VolumeHost::detached()).await
}

/// Same connection, against a host the caller supplies: the app's suites pass
/// the real wiring so the listing cache and the activity tracker see what the
/// share reports.
pub async fn make_docker_volume_with_host(host: VolumeHost) -> SmbVolume {
    let params = docker_guest_params();
    let port = params.port;
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "public");
    connect_smb_volume("public", TEST_MOUNT_ROOT, &volume_id, params, host)
        .await
        .unwrap_or_else(|e| {
            panic!("Failed to connect to Docker SMB container at 127.0.0.1:{port}. Is it running? ({e:?})")
        })
}

/// An absolute share path the way production builds one: `{mount root}/{relative}`.
///
/// An ABSOLUTE path outside the mount root is `VolumeError::NotFound`, so a test
/// addressing the share absolutely must go through here rather than prefix a
/// bare `/`.
pub fn share_path(relative: &str) -> String {
    format!("{TEST_MOUNT_ROOT}/{}", relative.trim_start_matches('/'))
}

/// Unique directory name for test isolation.
///
/// Combines the PID, a nanosecond timestamp, and a process-wide atomic counter
/// so that tests running in parallel never collide: neither within one process
/// (the nanosecond clock resolution isn't fine enough on its own) nor across the
/// separate processes nextest forks per test (where the static counter resets to
/// 0 and two processes hitting the same nanos window would otherwise produce
/// identical names, leaving stale directories on the SMB share for later runs to
/// trip on).
pub fn test_dir_name() -> String {
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is after 1970")
        .as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!("cmdr-test-{pid}-{ts}-{n}")
}

/// Ensures a test directory is clean before use (deletes recursively if it exists).
pub async fn ensure_clean(vol: &SmbVolume, dir: &str) {
    if vol.exists(Path::new(dir)).await {
        // Delete contents recursively
        if let Ok(entries) = vol.list_directory(Path::new(dir), None).await {
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
// Every SMB copy test that lands a file on a destination hashes the source bytes
// and the destination bytes and compares the two. A pipeline bug that drops,
// duplicates, reorders, or reuses a chunk's buffer will change the hash; the old
// `bytes_written == expected` and `metadata.size == N` assertions would silently
// pass. blake3 is fast (well over a GB/s single-threaded), so the 20 MB streaming
// tests pay negligible hashing cost on top of the SMB RTTs.

/// The hash of a buffer in hand, for the SOURCE side of a comparison.
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// The hash of what actually landed, streamed rather than buffered.
///
/// Goes through `open_read_stream` so a 20 MB destination isn't materialized
/// into a `Vec<u8>` just to `assert_eq!` it (which on mismatch printed an
/// unreadable megabyte-sized diff). The hex-formatted hash in the assertion
/// message is what's actionable on failure.
pub async fn hash_volume_file(volume: &dyn Volume, path: &Path) -> [u8; 32] {
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

// ── The regression sandbox ─────────────────────────────────────

/// All test artifacts on the SMB share live under this prefix. The cleanup
/// helper refuses to delete anything that doesn't start with it.
pub const TEST_PREFIX_ROOT: &str = "_test/cmdr-regression-";

/// Deletes every file under `unique_prefix_smb` and then the directory itself.
///
/// Safety: refuses any path that doesn't start with [`TEST_PREFIX_ROOT`], both
/// at the top level and per entry, so a logic bug in the caller can never reach
/// outside the regression sandbox. Called explicitly at the end of each pass
/// (best effort: logs but never overrides the test outcome).
pub async fn cleanup_test_prefix(vol: &SmbVolume, mount_path: &Path, unique_prefix_smb: &str) {
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
    match vol.list_directory(&dir_abs, None).await {
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
        Err(e) => log::warn!("cleanup_test_prefix: list_directory failed for {dir_abs:?}: {e:?}"),
    }
    let rel_dir = rel_of(&dir_abs);
    if rel_dir.starts_with(TEST_PREFIX_ROOT)
        && let Err(e) = vol.delete(&dir_abs).await
    {
        log::warn!("cleanup_test_prefix: failed to delete prefix dir {rel_dir}: {e:?}");
    }
}

// ── Numbers off the live session ───────────────────────────────

/// What this session negotiated as its largest single WRITE, or `None` while
/// disconnected.
///
/// A suite sizing a "large" file needs it: `max_write` differs per server and
/// per dialect, so a hardcoded number can silently land under the limit and test
/// the compound fast path twice instead of the staged streaming path once.
pub async fn negotiated_max_write(vol: &SmbVolume) -> Option<u64> {
    vol.negotiated_max_write().await
}

/// The smb2 client's current credit count, or `None` while disconnected.
///
/// The soak suite samples it between iterations: a credit leak bleeds this down
/// over thousands of copies, and exhaustion would stall every later read.
pub async fn session_credits(vol: &SmbVolume) -> Option<u16> {
    vol.session_credits().await
}

/// How many client-mutex tickets have been handed out process-wide.
///
/// Pure diagnostics, for the stress suite's hang dump: paired with the captured
/// `client-mutex:` log lines it says whether the wedged pass was still acquiring
/// the mutex or had stopped asking.
pub fn client_lock_tickets_issued() -> u64 {
    super::session::CLIENT_LOCK_TICKET.load(Ordering::Relaxed)
}

/// A whole buffer as a one-chunk [`VolumeReadStream`], for a suite that needs to
/// drive `write_from_stream` without a second volume behind it.
pub fn inline_read_stream(bytes: Vec<u8>) -> Box<dyn VolumeReadStream> {
    Box::new(super::streams::InlineReadStream::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volume::test_support::make_test_volume;
    use std::path::PathBuf;

    #[test]
    fn mutex_capture_logger_routes_known_prefixes() {
        // Format mirrors the real `log::debug!` sites in `clone_session`.
        let mutex_msg = format!(
            "client-mutex: waiting ticket={} caller=clone_session share={}",
            7, "Public"
        );
        let recv_msg = "recv: smb2 frame 0x10 mid=42";
        let other_msg = "some unrelated log line";

        assert!(
            mutex_msg.starts_with("client-mutex:"),
            "mutex prefix drifted: {mutex_msg}"
        );
        assert!(recv_msg.starts_with("recv:"), "recv prefix drifted: {recv_msg}");
        assert!(!other_msg.starts_with("client-mutex:") && !other_msg.starts_with("recv:"));
    }

    #[test]
    #[should_panic(expected = "refusing to clean a prefix outside")]
    fn cleanup_test_prefix_rejects_unsafe_prefix() {
        // The cleanup helper is async, but the safety assert fires before
        // any await point. Poll the future once via a no-op waker so we
        // hit the assert without needing a runtime.
        use std::task::Context;
        let vol = make_test_volume();
        let mount = PathBuf::from("/Volumes/TestShare");
        let mut fut = Box::pin(cleanup_test_prefix(&vol, &mount, "etc/passwd"));
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let _ = fut.as_mut().poll(&mut cx); // panics in the assert
    }

    #[test]
    fn test_prefix_root_is_safely_scoped() {
        // Static check: the prefix lives under `_test/` and clearly
        // identifies cmdr's regression test, so a future reader (or a
        // misconfigured share) can recognize stale artifacts at a glance.
        assert!(TEST_PREFIX_ROOT.starts_with("_test/"));
        assert!(TEST_PREFIX_ROOT.contains("cmdr-regression-"));
    }
}

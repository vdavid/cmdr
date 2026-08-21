//! Session-free volume builders, for this crate's own suites.
//!
//! These build an [`SmbVolume`] by struct literal, so they reach every field of
//! [`SmbVolumeInner`] and can't leave this crate. Everything a suite on the
//! APP's side of the seam needs is in [`super::testing`], which this module
//! re-exports so one `use super::test_support::*` covers both.
//!
//! A hand-built volume has no client and no tree, which is what the unit tests
//! exercising state transitions, retirement, and the reconnect early-exits want:
//! no server, no Docker, no wire. [`make_docker_volume`] is the connected path.

use super::*;

pub(super) use super::testing::*;

/// A disconnected test volume for the share `TestShare` at `/Volumes/TestShare`.
pub(super) fn make_test_volume() -> SmbVolume {
    make_test_volume_with_id("volumestestshare")
}

/// A disconnected test volume under an explicit volume id, for the tests that
/// need the id to be unique or to match what they registered.
pub(super) fn make_test_volume_with_id(volume_id: &str) -> SmbVolume {
    make_test_volume_with(volume_id, VolumeHost::detached())
}

/// A disconnected test volume over a host the caller wired, for the tests that
/// assert on what the backend told a seam.
pub(super) fn make_test_volume_with(volume_id: &str, host: VolumeHost) -> SmbVolume {
    let params = SmbConnectionParams {
        server: "192.168.1.100".to_string(),
        share_name: "TestShare".to_string(),
        port: 445,
        username: "Guest".to_string(),
        password: String::new(),
    };
    let mount_path = PathBuf::from("/Volumes/TestShare");
    let volume_id = volume_id.to_string();
    SmbVolume {
        name: "TestShare".to_string(),
        mount_path: mount_path.clone(),
        mount_root_gone: AtomicBool::new(false),
        inner: Arc::new_cyclic(|me| SmbVolumeInner {
            share_name: "TestShare".to_string(),
            volume_id,
            params: Arc::new(tokio::sync::RwLock::new(params)),
            client: Arc::new(tokio::sync::Mutex::new(None)),
            tree: Arc::new(tokio::sync::RwLock::new(None)),
            state: Arc::new(AtomicU8::new(ConnectionState::Disconnected as u8)),
            watcher_cancel: std::sync::Mutex::new(None),
            reconnect_lock: Arc::new(tokio::sync::Mutex::new(())),
            unmounted: Arc::new(AtomicBool::new(false)),
            retirement: Arc::new(Retirement::new()),
            me: me.clone(),
            scan_pool: tokio::sync::RwLock::new(None),
            scan_session_refs: AtomicUsize::new(0),
            active_mount_path: Arc::new(StdRwLock::new(mount_path)),
            host,
        }),
    }
}

/// A test volume already flipped to `Direct`, for the paths that only run on a
/// connected volume (the reconnect no-op, the watch-coverage claim).
pub(super) fn make_test_volume_direct() -> SmbVolume {
    let vol = make_test_volume();
    vol.inner.state.store(ConnectionState::Direct as u8, Ordering::Relaxed);
    vol
}

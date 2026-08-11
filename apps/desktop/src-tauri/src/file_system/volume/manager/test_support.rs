//! Test isolation for the process-global `VolumeManager`.

use std::sync::Arc;

use super::{Volume, get_volume_manager};

/// A registration in the process-global `VolumeManager`, reverted on drop.
///
/// **Why this exists.** Under plain `cargo test` a crate's tests share one
/// process, so `get_volume_manager()` is shared by every test at once. A test
/// that registers a WELL-KNOWN id (`"root"` above all) replaces what the tests
/// beside it depend on: `create_*_core(None, …)` and `write_payload_to_dir(None, …)`
/// resolve `None` to `"root"` and expect a real local-FS volume there, so a
/// leftover `InMemoryVolume` under that id turns their real-FS assertions red
/// with no hint of who swapped it. A bare `unregister` in the teardown can't
/// put back what was there before, and it never runs at all when an assertion
/// fails first. Restoring the previous value from `Drop` (which runs on unwind
/// too) keeps the swap scoped to the test that made it.
///
/// A test that can pick a UNIQUE volume id should do that instead; this is for
/// the few that have to exercise a hardcoded one. Mirrors
/// `listing::caching_test_support::TestListingGuard` (over `LISTING_CACHE`) and
/// `write_operations::test_support::TestOperationGuard` (over
/// `WRITE_OPERATION_STATE`).
///
/// Keep the guard on the stack (`let _root = …`, never `let _ = …`): a `_`
/// binding drops immediately and the registration is gone before the test runs.
pub(crate) struct TestVolumeRegistration {
    volume_id: String,
    previous: Option<Arc<dyn Volume>>,
}

impl TestVolumeRegistration {
    /// Registers `volume` under `volume_id`, remembering whatever was there.
    ///
    /// `force_register`, because swapping in a volume with a DIFFERENT root is
    /// the whole point of the guard, and `register` keeps the incumbent there.
    pub(crate) fn install(volume_id: &str, volume: Arc<dyn Volume>) -> Self {
        let manager = get_volume_manager();
        let previous = manager.get(volume_id);
        manager.force_register(volume_id, volume);
        Self {
            volume_id: volume_id.to_string(),
            previous,
        }
    }
}

impl Drop for TestVolumeRegistration {
    fn drop(&mut self) {
        let manager = get_volume_manager();
        match self.previous.take() {
            // `force_register`, not `register`: putting the previous value back
            // has to be unconditional, and `register` keeps the incumbent when
            // the two volumes have different roots.
            Some(previous) => manager.force_register(&self.volume_id, previous),
            None => manager.unregister(&self.volume_id),
        }
    }
}

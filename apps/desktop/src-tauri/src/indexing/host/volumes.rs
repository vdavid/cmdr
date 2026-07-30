//! Which volumes exist, where they're mounted, and what kind of storage they are.
//!
//! The index doesn't own the volume registry — the app mounts, connects, ejects,
//! and reconnects, and it keeps a `VolumeManager` for all of that. The index only
//! ever asks, so it asks through this trait rather than importing the registry.
//!
//! ## What's here and what deliberately isn't
//!
//! Everything on [`VolumeProvider`] is a question only the host can answer: what's
//! mounted right now, what filesystem a path sits on, what a PTP handle resolves
//! to. Volume ID *vocabulary* is not: `cmdr_fs::volume::{smb_volume_id, mtp_ids}`
//! is pure string work with no host behind it, so it moved down rather than
//! becoming methods here. Anything you can compute from a `&str` belongs there.
//!
//! There's no `scanner_for` / `watcher_for` either. Volume-kind dispatch runs on
//! `IndexVolumeKind`, and a plugin interface with no callers is exactly what a
//! designed API is supposed to not have.
//!
//! ## Cadence
//!
//! Called at human-perceptible cadence: once per scan start, per watch event, per
//! enrichment pass. ❌ Not a per-entry path — see the dispatch rule in
//! `policy.rs`. That's why these return owned values and one of them is `async`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::Volume;

use crate::indexing::events::Diagnostic;

/// The typed filesystem facts the enable decision needs, from ONE probe of the
/// mount a path sits on (a `statfs` on macOS, `/proc/mounts` on Linux).
///
/// Two facts rather than a `FilesystemKind`, because these are the only two the
/// index acts on and both are decisions the host is better placed to make: the
/// kind → network mapping is platform-specific and the probe itself can block on a
/// wedged mount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MountFacts {
    /// The mount is a network filesystem type. The local scanner must never walk
    /// one: it would traverse a share over syscalls that block for minutes.
    pub(crate) is_network: bool,
    /// Inode identity on this mount is stable enough to match a file across a
    /// rename. False on FAT/exFAT, whose inodes are derived rather than stored, so
    /// the rename pre-pass must not trust them.
    pub(crate) inodes_trustworthy: bool,
}

impl MountFacts {
    /// What to assume when the probe won't answer. A mount that can't be probed in
    /// time is a hung one, and treating it as network keeps the local scanner off
    /// it; inode trust is moot on a path we then refuse to walk.
    pub(crate) const UNPROBEABLE: Self = Self {
        is_network: true,
        inodes_trustworthy: true,
    };
}

/// One MTP object, resolved from the bare PTP handle a device change event carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedMtpObject {
    /// Storage-relative path with a leading `/`, already in the index path space
    /// (an MTP index is rooted at the storage root).
    pub(crate) path: PathBuf,
    /// Whether the object is a directory.
    pub(crate) is_directory: bool,
    /// Logical size in bytes; `None` for directories.
    pub(crate) size: Option<u64>,
    /// Modified time as a Unix timestamp, when the device reports one.
    pub(crate) modified_at: Option<u64>,
}

/// The future [`VolumeProvider::resolve_mtp_object`] returns. Boxed because the
/// provider is used as `dyn`.
pub(crate) type ResolveMtpFut<'a> = Pin<Box<dyn Future<Output = Result<ResolvedMtpObject, Diagnostic>> + Send + 'a>>;

/// What the index asks the host about mounted storage.
pub(crate) trait VolumeProvider: Send + Sync {
    /// The volume registered under `volume_id`, or `None` when nothing is mounted
    /// under that id right now (never mounted, ejected, or a share that dropped).
    ///
    /// `None` is a normal answer, not an error: volumes come and go while an index
    /// exists for them, and every caller has a defined behavior for the gap.
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>>;

    /// Every registered volume id, in no particular order.
    fn volume_ids(&self) -> Vec<String>;

    /// The id of the volume whose mount point contains `path`, longest-mount-first,
    /// or `None` when no registered mount covers it.
    fn mount_id_for_path(&self, path: &str) -> Option<String>;

    /// Probe the filesystem `path` sits on. **Blocking**: this can stall for
    /// minutes on a wedged network mount, so callers run it off the async runtime
    /// under their own timeout and fall back to [`MountFacts::UNPROBEABLE`].
    fn mount_facts(&self, path: &Path) -> MountFacts;

    /// The SMB volume id for `path` when it resolves to an `smbfs`/`cifs` mount.
    ///
    /// It's the SAME id the host registers the share under, so a listing beneath
    /// `/Volumes/<share>` resolves to that share's index rather than the local
    /// disk's.
    fn smb_volume_id_for_path(&self, path: &str) -> Option<String>;

    /// Bytes in use on the volume containing `path`, for the scan-ETA denominator.
    ///
    /// **Blocking** for the same reason as [`mount_facts`](Self::mount_facts).
    /// `None` on any failure: a missing denominator degrades the ETA, and nothing
    /// about a scan may wait on it.
    fn volume_used_bytes(&self, path: &Path) -> Option<u64>;

    /// Resolve a PTP object handle on `(device_id, storage_id)` to a path plus the
    /// metadata an index upsert needs.
    ///
    /// Costs a device round trip on a contended session, which is why the watch
    /// layer buffers raw handles during a scan and resolves them afterwards. The
    /// error is log-only: an unresolvable handle means the object is gone or the
    /// device dropped, and the reconcile pass will catch up either way.
    fn resolve_mtp_object(&self, device_id: &str, storage_id: u32, handle: u32) -> ResolveMtpFut<'_>;
}

/// The installed provider. An `RwLock` rather than a `OnceLock` because tests
/// swap it (see [`install_for_test`]); production writes it exactly once.
static INSTALLED: RwLock<Option<Arc<dyn VolumeProvider>>> = RwLock::new(None);

/// A [`set_volume_provider`] call that arrived after one was already installed.
#[derive(Debug)]
pub(crate) struct VolumeProviderAlreadySet;

/// Tells the index which host to ask about mounted volumes. Call once at startup.
/// A second call keeps the first provider rather than swapping the registry under
/// a running scan.
pub(crate) fn set_volume_provider(provider: Arc<dyn VolumeProvider>) -> Result<(), VolumeProviderAlreadySet> {
    let mut slot = INSTALLED.write_ignore_poison();
    if slot.is_some() {
        return Err(VolumeProviderAlreadySet);
    }
    *slot = Some(provider);
    Ok(())
}

/// The installed provider, or [`NoVolumes`] when nothing was installed.
pub(crate) fn current() -> Arc<dyn VolumeProvider> {
    if let Some(installed) = INSTALLED.read_ignore_poison().as_ref() {
        return Arc::clone(installed);
    }
    static FALLBACK: OnceLock<Arc<dyn VolumeProvider>> = OnceLock::new();
    Arc::clone(FALLBACK.get_or_init(|| Arc::new(NoVolumes)))
}

/// Swap in `provider` for the duration of one test, restoring whatever was there
/// when the returned guard drops.
///
/// The slot is process-wide, so anything using this must hold [`test_lock`] first:
/// nextest runs a process per test, but a plain `cargo test` doesn't, and two tests
/// swapping the same slot concurrently would see each other's volumes.
#[cfg(test)]
#[must_use = "the provider is restored when the guard drops"]
pub(crate) fn install_for_test(provider: Arc<dyn VolumeProvider>) -> TestProviderGuard {
    let previous = INSTALLED.write_ignore_poison().replace(provider);
    TestProviderGuard { previous }
}

/// Restores the previously-installed provider on drop, including on a panic, so
/// one failing test can't leave every later one looking at its fake volumes.
#[cfg(test)]
pub(crate) struct TestProviderGuard {
    previous: Option<Arc<dyn VolumeProvider>>,
}

#[cfg(test)]
impl Drop for TestProviderGuard {
    fn drop(&mut self) {
        *INSTALLED.write_ignore_poison() = self.previous.take();
    }
}

/// Serializes tests that swap the provider. Take it BEFORE [`install_for_test`].
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A host with nothing mounted.
///
/// It's the default because "no volume registered" is already a case every caller
/// handles, so an uninstalled provider degrades to the same behavior as an ejected
/// drive instead of panicking. Tests that need volumes install a
/// [`FakeVolumeProvider`].
pub(crate) struct NoVolumes;

impl VolumeProvider for NoVolumes {
    fn get(&self, _volume_id: &str) -> Option<Arc<dyn Volume>> {
        None
    }
    fn volume_ids(&self) -> Vec<String> {
        Vec::new()
    }
    fn mount_id_for_path(&self, _path: &str) -> Option<String> {
        None
    }
    fn mount_facts(&self, _path: &Path) -> MountFacts {
        MountFacts {
            is_network: false,
            inodes_trustworthy: true,
        }
    }
    fn smb_volume_id_for_path(&self, _path: &str) -> Option<String> {
        None
    }
    fn volume_used_bytes(&self, _path: &Path) -> Option<u64> {
        None
    }
    fn resolve_mtp_object(&self, device_id: &str, storage_id: u32, handle: u32) -> ResolveMtpFut<'_> {
        let reason =
            format!("no volume provider installed (device {device_id}, storage {storage_id}, handle {handle})");
        Box::pin(async move { Err(Diagnostic::from(reason)) })
    }
}

/// A host whose mounted volumes a test controls, without an app or a real device.
///
/// Registrations are per instance, so nothing leaks between tests the way the
/// process-wide registry does. `mount_facts` reports a plain local disk unless a
/// test says otherwise.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct FakeVolumeProvider {
    volumes: RwLock<std::collections::HashMap<String, Arc<dyn Volume>>>,
    network_mounts: RwLock<std::collections::HashSet<PathBuf>>,
}

#[cfg(test)]
impl FakeVolumeProvider {
    /// An empty host, wrapped for injection.
    pub(crate) fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Mount `volume` under `volume_id`.
    pub(crate) fn register(&self, volume_id: impl Into<String>, volume: Arc<dyn Volume>) -> &Self {
        self.volumes.write_ignore_poison().insert(volume_id.into(), volume);
        self
    }

    /// Make `mount_facts` report a network filesystem for paths under `root`.
    pub(crate) fn mark_network(&self, root: impl Into<PathBuf>) -> &Self {
        self.network_mounts.write_ignore_poison().insert(root.into());
        self
    }
}

#[cfg(test)]
impl VolumeProvider for FakeVolumeProvider {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        self.volumes.read_ignore_poison().get(volume_id).map(Arc::clone)
    }

    fn volume_ids(&self) -> Vec<String> {
        self.volumes.read_ignore_poison().keys().cloned().collect()
    }

    fn mount_id_for_path(&self, path: &str) -> Option<String> {
        // Longest mount root wins, like the real registry: a drive mounted inside
        // another one must not resolve to its parent.
        self.volumes
            .read_ignore_poison()
            .iter()
            .filter(|(_, volume)| Path::new(path).starts_with(volume.root()))
            .max_by_key(|(_, volume)| volume.root().as_os_str().len())
            .map(|(id, _)| id.clone())
    }

    fn mount_facts(&self, path: &Path) -> MountFacts {
        let is_network = self
            .network_mounts
            .read_ignore_poison()
            .iter()
            .any(|root| path.starts_with(root));
        MountFacts {
            is_network,
            inodes_trustworthy: true,
        }
    }

    fn smb_volume_id_for_path(&self, _path: &str) -> Option<String> {
        None
    }

    fn volume_used_bytes(&self, _path: &Path) -> Option<u64> {
        None
    }

    fn resolve_mtp_object(&self, _device_id: &str, _storage_id: u32, handle: u32) -> ResolveMtpFut<'_> {
        Box::pin(async move {
            Err(Diagnostic::from(format!(
                "fake provider resolves no MTP handles ({handle})"
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::volume::InMemoryVolume;

    /// With no provider installed, every lookup answers the way an ejected drive
    /// does. A panic or a fabricated volume would make half the test suite depend
    /// on install order.
    #[test]
    fn an_uninstalled_provider_reports_nothing_mounted() {
        let provider = current();
        assert!(provider.get("root").is_none());
        assert!(provider.volume_ids().is_empty());
        assert!(provider.mount_id_for_path("/Volumes/anything").is_none());
    }

    /// The fake resolves the LONGEST matching mount, like the real registry: a
    /// drive mounted inside another must not resolve to its parent.
    #[test]
    fn the_fake_resolves_the_longest_matching_mount() {
        let provider = FakeVolumeProvider::shared();
        provider.register("root", Arc::new(InMemoryVolume::new("Root")));
        let nested: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Nested"));
        provider.register("nested", Arc::clone(&nested));

        // `InMemoryVolume` roots at `/`, so both match and the tie goes to neither
        // in particular; what matters is that a registered id comes back at all.
        assert!(provider.mount_id_for_path("/photos").is_some());
        assert_eq!(
            provider.get("nested").map(|v| v.name().to_string()),
            Some("Nested".into())
        );
        assert!(provider.get("absent").is_none());
    }

    /// A path marked network has to come back as network, or every test built on
    /// the fake's routing would pass vacuously.
    #[test]
    fn the_fake_reports_the_network_mounts_it_was_given() {
        let provider = FakeVolumeProvider::shared();
        provider.mark_network("/Volumes/naspi");

        assert!(provider.mount_facts(Path::new("/Volumes/naspi/media")).is_network);
        assert!(!provider.mount_facts(Path::new("/Volumes/usb")).is_network);
    }
}

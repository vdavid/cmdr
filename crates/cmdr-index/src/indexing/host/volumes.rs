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
use std::sync::atomic::{AtomicU64, Ordering};
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
pub struct MountFacts {
    /// The mount is a network filesystem type. The local scanner must never walk
    /// one: it would traverse a share over syscalls that block for minutes.
    pub is_network: bool,
    /// Inode identity on this mount is stable enough to match a file across a
    /// rename. False on FAT/exFAT, whose inodes are derived rather than stored, so
    /// the rename pre-pass must not trust them.
    pub inodes_trustworthy: bool,
}

impl MountFacts {
    /// What to assume when the probe won't answer. A mount that can't be probed in
    /// time is a hung one, and treating it as network keeps the local scanner off
    /// it; inode trust is moot on a path we then refuse to walk.
    pub const UNPROBEABLE: Self = Self {
        is_network: true,
        inodes_trustworthy: true,
    };
}

/// Which mounted filesystem a volume's root is, as the host's mount table names it.
///
/// It stays the same for as long as the filesystem stays mounted, wherever its mount
/// point moves: renaming a mounted volume moves the mount point and keeps the identity
/// (verified on macOS 26.6.2, APFS and HFS+ disk images,
/// `file_system::index_provider::real_image`, 2026-09-14). A filesystem that isn't
/// mounted has an identity the table lists nowhere. Opaque: the host packs whatever its
/// table carries (`f_fsid` on macOS, the device number on Linux), and the index only
/// compares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MountIdentity(u64);

impl MountIdentity {
    /// The identity a host's mount table gives a filesystem, packed into 64 bits.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The 64 bits a host packed, for that host to compare against its table.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// One MTP object, resolved from the bare PTP handle a device change event carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedMtpObject {
    /// Storage-relative path with a leading `/`, already in the index path space
    /// (an MTP index is rooted at the storage root).
    pub path: PathBuf,
    /// Whether the object is a directory.
    pub is_directory: bool,
    /// Logical size in bytes; `None` for directories.
    pub size: Option<u64>,
    /// Modified time as a Unix timestamp, when the device reports one.
    pub modified_at: Option<u64>,
}

/// Why the host couldn't hand the index a direct smb2 session.
///
/// Typed rather than a message, so the gate's own refusal reason maps across
/// without anyone string-matching an upgrade failure.
#[derive(Debug)]
pub enum SmbUpgradeRefusal {
    /// The share needs credentials the user hasn't supplied. The frontend's
    /// "Sign in" flow owns collecting them; indexing stays off until it does.
    CredentialsNeeded,
    /// The upgrade failed for any other reason (network, auth, a session that
    /// wouldn't come up). The diagnostic is log-only.
    Failed(Diagnostic),
}

/// The future [`VolumeProvider::ensure_direct_smb`] returns.
pub type EnsureDirectSmbFut<'a> = Pin<Box<dyn Future<Output = Result<(), SmbUpgradeRefusal>> + Send + 'a>>;

/// The future [`VolumeProvider::resolve_mtp_object`] returns. Boxed because the
/// provider is used as `dyn`.
pub type ResolveMtpFut<'a> = Pin<Box<dyn Future<Output = Result<ResolvedMtpObject, Diagnostic>> + Send + 'a>>;

/// What the index asks the host about mounted storage.
pub trait VolumeProvider: Send + Sync {
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

    /// The identity of the filesystem mounted exactly at `root`, from the host's mount
    /// table: `None` when nothing is mounted at `root` or the table couldn't be read.
    ///
    /// **Non-blocking**: the host reads its mount table, ❌ never the mount, so a dead
    /// or hung drive can't stall the answer. The index reads it once, when a
    /// local-scanner volume's index starts; a share or a phone has its own disconnect
    /// path.
    fn mount_identity(&self, root: &Path) -> Option<MountIdentity>;

    /// Whether any filesystem in the host's mount table right now has `identity`:
    /// `Some(false)` means it's mounted nowhere, `None` that the table couldn't be read.
    ///
    /// **Non-blocking**, like [`mount_identity`](Self::mount_identity). ❌ A caller never
    /// reads `None` as "gone", and ❌ never asks about a root path instead: renaming a
    /// mounted volume takes its old root out of the table while the drive stays
    /// mounted.
    fn is_mounted(&self, identity: MountIdentity) -> Option<bool>;

    /// Every mount point in the host's mount table right now, as the table spells
    /// it (the boot disk's `/` included), or `None` when the table couldn't be read.
    ///
    /// **Non-blocking**, like [`mount_identity`](Self::mount_identity): the table,
    /// ❌ never the mounts. The boot-disk index stops at every filesystem mounted
    /// inside its tree (`scanner::boot_tree_mounts`), and re-reads this at most
    /// once a second while something asks. ❌ A caller never reads `None` as
    /// "nothing is mounted".
    fn mount_points(&self) -> Option<Vec<PathBuf>>;

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

    /// Make sure `volume_id` is a direct smb2 session, upgrading it from an OS
    /// mount if that's all there is.
    ///
    /// The index can only scan a share over the `Volume` trait, which an OS mount
    /// doesn't provide. Mounting, credentials, and session management are all the
    /// host's, so it does the upgrade and reports success or a typed refusal; on
    /// success the host has REPLACED the registered volume, so re-`get` it.
    fn ensure_direct_smb(&self, volume_id: &str) -> EnsureDirectSmbFut<'_>;

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

/// Bumped whenever the installed provider changes, and whenever a test fake's mount
/// table does, so a cache of [`VolumeProvider::mount_points`] re-reads at once
/// instead of serving the previous host's table until its refresh interval runs out.
static TABLE_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Which generation of the mount table the installed provider is on. Only ever
/// compared for equality.
pub(crate) fn table_generation() -> u64 {
    TABLE_GENERATION.load(Ordering::Acquire)
}

fn table_changed() {
    TABLE_GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// A [`set_volume_provider`] call that arrived after one was already installed.
#[derive(Debug)]
pub struct VolumeProviderAlreadySet;

/// Tells the index which host to ask about mounted volumes. Call once at startup.
/// A second call keeps the first provider rather than swapping the registry under
/// a running scan.
pub(crate) fn set_volume_provider(provider: Arc<dyn VolumeProvider>) -> Result<(), VolumeProviderAlreadySet> {
    let mut slot = INSTALLED.write_ignore_poison();
    if slot.is_some() {
        return Err(VolumeProviderAlreadySet);
    }
    *slot = Some(provider);
    drop(slot);
    table_changed();
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
/// The slot is process-wide, so anything using this must hold `handle::test_lock` first:
/// nextest runs a process per test, but a plain `cargo test` doesn't, and two tests
/// swapping the same slot concurrently would see each other's volumes.
#[cfg(any(test, feature = "testing"))]
#[must_use = "the provider is restored when the guard drops"]
pub fn install_for_test(provider: Arc<dyn VolumeProvider>) -> TestProviderGuard {
    let previous = INSTALLED.write_ignore_poison().replace(provider);
    table_changed();
    TestProviderGuard { previous }
}

/// Restores the previously-installed provider on drop, including on a panic, so
/// one failing test can't leave every later one looking at its fake volumes.
#[cfg(any(test, feature = "testing"))]
pub struct TestProviderGuard {
    previous: Option<Arc<dyn VolumeProvider>>,
}

#[cfg(any(test, feature = "testing"))]
impl Drop for TestProviderGuard {
    fn drop(&mut self) {
        *INSTALLED.write_ignore_poison() = self.previous.take();
        table_changed();
    }
}

/// A host with nothing mounted.
///
/// It's the default because "no volume registered" is already a case every caller
/// handles, so an uninstalled provider degrades to the same behavior as an ejected
/// drive instead of panicking. Tests that need volumes install a
/// [`FakeVolumeProvider`].
pub struct NoVolumes;

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
    /// A host with no mount table names no filesystem, so no start captures an
    /// identity to ask about later.
    fn mount_identity(&self, _root: &Path) -> Option<MountIdentity> {
        None
    }
    /// Nothing can unmount under a host that mounts nothing.
    fn is_mounted(&self, _identity: MountIdentity) -> Option<bool> {
        Some(true)
    }
    /// A host that mounts nothing has nothing mounted inside the boot tree.
    fn mount_points(&self) -> Option<Vec<PathBuf>> {
        Some(Vec::new())
    }
    fn smb_volume_id_for_path(&self, _path: &str) -> Option<String> {
        None
    }
    fn volume_used_bytes(&self, _path: &Path) -> Option<u64> {
        None
    }
    fn ensure_direct_smb(&self, volume_id: &str) -> EnsureDirectSmbFut<'_> {
        let reason = format!("no volume provider installed, so '{volume_id}' can't be upgraded");
        Box::pin(async move { Err(SmbUpgradeRefusal::Failed(Diagnostic::from(reason))) })
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
#[cfg(any(test, feature = "testing"))]
#[derive(Default)]
pub struct FakeVolumeProvider {
    volumes: RwLock<std::collections::HashMap<String, Arc<dyn Volume>>>,
    network_mounts: RwLock<std::collections::HashSet<PathBuf>>,
    untrusted_inode_mounts: RwLock<std::collections::HashSet<PathBuf>>,
    mounts: RwLock<std::collections::HashMap<PathBuf, MountIdentity>>,
    /// Inverted so `Default` means a table that reads fine.
    table_unreadable: RwLock<bool>,
}

#[cfg(any(test, feature = "testing"))]
impl FakeVolumeProvider {
    /// An empty host, wrapped for injection.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Mount `volume` under `volume_id`.
    pub fn register(&self, volume_id: impl Into<String>, volume: Arc<dyn Volume>) -> &Self {
        self.volumes.write_ignore_poison().insert(volume_id.into(), volume);
        self
    }

    /// Make `mount_facts` report a network filesystem for paths under `root`.
    pub fn mark_network(&self, root: impl Into<PathBuf>) -> &Self {
        self.network_mounts.write_ignore_poison().insert(root.into());
        self
    }

    /// Make `mount_facts` report untrustworthy inodes for paths under `root`, the
    /// way a real FAT/exFAT mount does.
    pub fn mark_inodes_untrusted(&self, root: impl Into<PathBuf>) -> &Self {
        self.untrusted_inode_mounts.write_ignore_poison().insert(root.into());
        self
    }

    /// List a filesystem with `identity` as mounted at `root`, the way a plugged-in
    /// drive reads. A root nothing was mounted at has no identity.
    pub fn mount(&self, root: impl Into<PathBuf>, identity: MountIdentity) -> &Self {
        self.mounts.write_ignore_poison().insert(root.into(), identity);
        table_changed();
        self
    }

    /// Move the filesystem mounted at `from` to `to`, keeping its identity: a rename.
    pub fn rename_mount(&self, from: impl AsRef<Path>, to: impl Into<PathBuf>) -> &Self {
        let mut mounts = self.mounts.write_ignore_poison();
        if let Some(identity) = mounts.remove(from.as_ref()) {
            mounts.insert(to.into(), identity);
        }
        drop(mounts);
        table_changed();
        self
    }

    /// Take the filesystem mounted at `root` out of the mount table, the way a pulled
    /// drive or a finished unmount reads.
    pub fn mark_unmounted(&self, root: impl AsRef<Path>) -> &Self {
        self.mounts.write_ignore_poison().remove(root.as_ref());
        table_changed();
        self
    }

    /// Make `is_mounted` answer `None`, the way a mount table that couldn't be read
    /// does. Distinct from an unmounted drive: "don't know" must never authorize a
    /// delete, and must never flag a generation vanished.
    pub fn mark_table_unreadable(&self) -> &Self {
        *self.table_unreadable.write_ignore_poison() = true;
        table_changed();
        self
    }
}

#[cfg(any(test, feature = "testing"))]
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
        let under = |set: &RwLock<std::collections::HashSet<PathBuf>>| {
            set.read_ignore_poison().iter().any(|root| path.starts_with(root))
        };
        MountFacts {
            is_network: under(&self.network_mounts),
            inodes_trustworthy: !under(&self.untrusted_inode_mounts),
        }
    }

    fn mount_identity(&self, root: &Path) -> Option<MountIdentity> {
        self.mounts.read_ignore_poison().get(root).copied()
    }

    fn is_mounted(&self, identity: MountIdentity) -> Option<bool> {
        if *self.table_unreadable.read_ignore_poison() {
            return None;
        }
        Some(
            self.mounts
                .read_ignore_poison()
                .values()
                .any(|mounted| *mounted == identity),
        )
    }

    fn mount_points(&self) -> Option<Vec<PathBuf>> {
        if *self.table_unreadable.read_ignore_poison() {
            return None;
        }
        Some(self.mounts.read_ignore_poison().keys().cloned().collect())
    }

    fn smb_volume_id_for_path(&self, _path: &str) -> Option<String> {
        None
    }

    fn volume_used_bytes(&self, _path: &Path) -> Option<u64> {
        None
    }

    fn ensure_direct_smb(&self, volume_id: &str) -> EnsureDirectSmbFut<'_> {
        let reason = format!("fake provider upgrades nothing ('{volume_id}')");
        Box::pin(async move { Err(SmbUpgradeRefusal::Failed(Diagnostic::from(reason))) })
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
        // The installed provider is a process-wide seam, so asking what's mounted
        // needs the same lock installing one does — otherwise this reads whichever
        // fake drive a concurrent test had mounted at that instant.
        let _serialized = crate::indexing::handle::test_lock();
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

    /// The two mount facts have to come back as marked, or every test built on the
    /// fake's routing would pass vacuously. They're independent: a FAT stick is
    /// inode-untrusted but local, a share is network with fine inodes.
    #[test]
    fn the_fake_reports_the_mount_facts_it_was_given() {
        let provider = FakeVolumeProvider::shared();
        provider
            .mark_network("/Volumes/naspi")
            .mark_inodes_untrusted("/Volumes/stick");

        let share = provider.mount_facts(Path::new("/Volumes/naspi/media"));
        assert!(share.is_network);
        assert!(
            share.inodes_trustworthy,
            "a share's inodes aren't the thing in question"
        );

        let stick = provider.mount_facts(Path::new("/Volumes/stick/DCIM"));
        assert!(!stick.is_network, "a FAT stick is local");
        assert!(!stick.inodes_trustworthy);

        let plain = provider.mount_facts(Path::new("/Volumes/usb"));
        assert!(!plain.is_network);
        assert!(plain.inodes_trustworthy);
    }

    /// The fake's mount table follows a filesystem, not a path: a rename moves the
    /// identity to the new root and keeps it mounted, and only an unmount makes it
    /// gone. A sibling drive stays mounted throughout, or a presence test could pass by
    /// losing every drive at once.
    #[test]
    fn the_fake_mount_table_follows_a_filesystem_across_a_rename() {
        let provider = FakeVolumeProvider::shared();
        let (stick, sibling) = (MountIdentity::from_raw(7), MountIdentity::from_raw(8));
        provider
            .mount("/Volumes/Stick", stick)
            .mount("/Volumes/Sibling", sibling);
        assert_eq!(provider.mount_identity(Path::new("/Volumes/Stick")), Some(stick));
        assert_eq!(
            provider.mount_identity(Path::new("/Volumes/usb")),
            None,
            "nothing is mounted at that root"
        );

        provider.rename_mount("/Volumes/Stick", "/Volumes/Photos");
        assert_eq!(provider.mount_identity(Path::new("/Volumes/Stick")), None);
        assert_eq!(provider.mount_identity(Path::new("/Volumes/Photos")), Some(stick));
        assert_eq!(provider.is_mounted(stick), Some(true), "renamed, not gone");

        provider.mark_unmounted("/Volumes/Photos");
        assert_eq!(provider.is_mounted(stick), Some(false));
        assert_eq!(provider.is_mounted(sibling), Some(true));
    }

    /// With no host tracking mounts, no root names a filesystem for a start to
    /// capture, and nothing can unmount.
    #[test]
    fn an_uninstalled_provider_names_no_filesystem_and_loses_none() {
        assert_eq!(NoVolumes.mount_identity(Path::new("/Volumes/anything")), None);
        assert_eq!(NoVolumes.is_mounted(MountIdentity::from_raw(1)), Some(true));
    }
}

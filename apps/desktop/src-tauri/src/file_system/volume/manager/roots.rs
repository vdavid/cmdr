//! The set of mount roots one volume ID owns, and the rules for picking which
//! of them is active.
//!
//! One filesystem can be reached through several mount points and they all
//! derive one volume ID (an SMB share keys on `(server, port, share)`, a local
//! disk on its filesystem UUID). So a registry entry holds the SET of roots
//! carrying its ID with exactly one ACTIVE — `volume.root()` — and promotes a
//! survivor when the active one dies. Rationale and the flows that drive it:
//! `../DETAILS.md` § "A volume ID owns a set of mount roots".

use super::{Volume, VolumeManager};
use crate::ignore_poison::RwLockIgnorePoison;
use cmdr_archive::ArchiveVolume;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One mount root carrying a volume's ID.
pub(super) struct MountRoot {
    pub(super) path: PathBuf,
    /// Set once an operation on this root came back with an errno that PROVES
    /// the mount is gone rather than the file. Never cleared: a root that has
    /// answered `ENOTCONN` is only trustworthy again after a fresh mount event,
    /// which re-records it from scratch.
    pub(super) proven_stale: bool,
}

impl MountRoot {
    fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            proven_stale: false,
        }
    }

    /// Ranking key for "which root should be active": liveness first, then the
    /// canonical path shape. Lower sorts better.
    ///
    /// The path half is the original rule (shortest, ties lexicographic) and it
    /// still decides between equally-live roots: macOS suffixes the LATER mount
    /// (`/Volumes/naspi-1`), so the shortest is the original, which is what every
    /// saved path, favorite, and index row already refers to. What changed is its
    /// RANK — a proven-dead shortest root loses to a live longer one, because
    /// path shape is a guess about identity and an errno is evidence about health.
    fn rank(&self) -> (bool, usize, &Path) {
        let text = self.path.as_os_str();
        (self.proven_stale, text.len(), &self.path)
    }
}

/// One registry entry: the volume plus every mount root known to carry its ID.
///
/// Invariant: `roots` always contains `volume.root()`, and that entry is the
/// ACTIVE root. Everything else is a fallback the unmount and stale-mount paths
/// can promote to.
pub(super) struct Registration {
    pub(super) volume: Arc<dyn Volume>,
    pub(super) roots: Vec<MountRoot>,
}

impl Registration {
    /// A fresh registration whose only known root is the volume's own.
    pub(super) fn new(volume: Arc<dyn Volume>) -> Self {
        let root = MountRoot::new(volume.root());
        Self {
            volume,
            roots: vec![root],
        }
    }

    /// Whether any known root of this entry is `root` (active or fallback).
    pub(super) fn knows_root(&self, root: &Path) -> bool {
        self.roots.iter().any(|r| r.path == root)
    }

    /// Record `root` as another mount reaching this volume. Idempotent.
    pub(super) fn record_root(&mut self, root: &Path) {
        if !self.knows_root(root) {
            self.roots.push(MountRoot::new(root));
        }
    }

    /// Put back a root the entry is still anchored to even though its mount is
    /// gone, marked stale so it can never win the rank again.
    ///
    /// Keeps the `roots`-contains-`volume.root()` invariant true for a backend
    /// that declined to re-root: without this the entry would be anchored to a
    /// path it no longer claims, so `find_by_root` (and a later unmount or
    /// re-mount event for that same path) would stop recognizing it.
    pub(super) fn readd_stale_root(&mut self, root: &Path) {
        match self.roots.iter_mut().find(|r| r.path == root) {
            Some(existing) => existing.proven_stale = true,
            None => self.roots.push(MountRoot {
                path: root.to_path_buf(),
                proven_stale: true,
            }),
        }
    }

    /// Swap the volume in place (a same-root replacement, like the SMB upgrade),
    /// keeping the fallback roots the entry has collected.
    pub(super) fn replace_volume(&mut self, volume: Arc<dyn Volume>) {
        let root = volume.root().to_path_buf();
        self.volume = volume;
        self.record_root(&root);
    }

    /// The root that SHOULD be active: the best-ranked one, or `None` when the
    /// entry has no roots left at all.
    fn best_root(&self) -> Option<&MountRoot> {
        self.roots.iter().min_by(|a, b| a.rank().cmp(&b.rank()))
    }

    /// Whether the ACTIVE root is one this entry has proven dead.
    ///
    /// A root missing from the set counts as gone: the invariant says the set
    /// contains `volume.root()`, so the only way it isn't there is that the mount
    /// went away and nothing put it back.
    fn active_root_is_dead(&self) -> bool {
        let active = self.volume.root();
        self.roots
            .iter()
            .find(|r| r.path == active)
            .is_none_or(|r| r.proven_stale)
    }

    /// Tell the volume when it has ended up on a mount root that's gone.
    ///
    /// Runs after every move of the active seat. The volume keeps serving (that's
    /// why it's still registered), but the paths it publishes stop being openable
    /// by anything outside Cmdr, and only the registry has that evidence — nothing
    /// may probe a mount. Cheap and idempotent by contract, and it runs under the
    /// registry write lock, so an implementation must not reach back in here.
    fn tell_volume_if_its_root_is_dead(&self) {
        if self.active_root_is_dead() {
            self.volume.note_root_mount_gone();
        }
    }

    /// Move the ID to the best surviving root, if that isn't where it already is
    /// and the backend can be re-rooted. Returns the new active root on success.
    ///
    /// No I/O: a promotion is a pure registry swap plus whatever the backend's
    /// `rerooted` costs (for a path-addressed backend, one allocation). ❌ Never
    /// add a liveness probe here — an NSURL/`statfs` round trip on a dead network
    /// mount blocks 30–120 s, which is the whole point of `volumes/DETAILS.md`
    /// § "Hung mounts". A root that is still dead simply proves it again on the
    /// next failure and gets marked in turn.
    pub(super) fn promote_to_best_root(&mut self) -> Promotion {
        let Some(best) = self.best_root() else {
            return Promotion::NoRootsLeft;
        };
        if best.path == self.volume.root() {
            return Promotion::AlreadyBest;
        }
        let target = best.path.clone();
        match self.volume.rerooted(&target) {
            Some(rerooted) => {
                self.volume = rerooted;
                Promotion::Promoted(target)
            }
            None => Promotion::BackendCantReroot,
        }
    }
}

/// The mount-root half of the registry API: the two ways a root leaves the
/// active seat, and the read that shows the whole set.
impl VolumeManager {
    /// Every mount root known to reach `id`, active one first.
    pub fn known_roots(&self, id: &str) -> Vec<PathBuf> {
        let volumes = self.volumes.read_ignore_poison();
        let Some(entry) = volumes.get(id) else {
            return Vec::new();
        };
        let active = entry.volume.root();
        let mut roots: Vec<PathBuf> = vec![active.to_path_buf()];
        roots.extend(entry.roots.iter().filter(|r| r.path != active).map(|r| r.path.clone()));
        roots
    }

    /// Drop a mount root that has gone away, promoting a survivor when it was
    /// the active one and unregistering only when it was the LAST one.
    ///
    /// This is the unmount path's entry point, and the reason a share mounted
    /// twice survives an eject of either mount. Pure registry work under the
    /// write lock: teardown (`on_unmount`, stopping an index) belongs to the
    /// caller, which is why [`RootRemoval::Unregistered`] hands the volume back.
    pub fn remove_root(&self, root: &Path) -> RootRemoval {
        let mut volumes = self.volumes.write_ignore_poison();
        let Some((id, entry)) = volumes.iter_mut().find(|(_, entry)| entry.knows_root(root)) else {
            return RootRemoval::Unknown;
        };
        let id = id.clone();

        let was_active = entry.volume.root() == root;
        entry.roots.retain(|r| r.path != root);

        if !was_active {
            return RootRemoval::SiblingDropped { id };
        }
        match entry.promote_to_best_root() {
            Promotion::Promoted(new_root) => {
                // A survivor took over, but "survivor" is only about ranking: with
                // every sibling already proven stale, the promotion lands on a
                // corpse too.
                entry.tell_volume_if_its_root_is_dead();
                RootRemoval::Promoted { id, new_root }
            }
            Promotion::BackendCantReroot => {
                // The backend stays anchored to a mount that's gone, so the root
                // goes back into the set marked stale. Dropping it would leave the
                // entry claiming no root while `volume.root()` still returns one.
                entry.readd_stale_root(root);
                entry.tell_volume_if_its_root_is_dead();
                RootRemoval::ActiveRootStranded { id }
            }
            // `AlreadyBest` can't follow removing the active root (it's gone from
            // the set), so both remaining arms mean the entry has no roots left.
            Promotion::AlreadyBest | Promotion::NoRootsLeft => {
                let entry = volumes.remove(&id).expect("found under this key a moment ago");
                self.clear_default_if(&id);
                super::retire(&entry.volume);
                RootRemoval::Unregistered {
                    id,
                    volume: entry.volume,
                }
            }
        }
    }

    /// Record that `root` answered with an errno proving its mount is gone, and
    /// move the ID to a live sibling if there is one.
    ///
    /// The lazy half of "liveness outranks path shape": nothing probes a mount to
    /// find out whether it's alive (that blocks for 30–120 s on a wedged one),
    /// so the evidence arrives as a failed operation and the promotion rides on
    /// it. A root already known stale re-reports cheaply and changes nothing.
    pub fn mark_root_stale(&self, id: &str, root: &Path) -> StaleRootOutcome {
        let mut volumes = self.volumes.write_ignore_poison();
        let Some(entry) = volumes.get_mut(id) else {
            return StaleRootOutcome::Unchanged;
        };
        let Some(marked) = entry.roots.iter_mut().find(|r| r.path == root) else {
            return StaleRootOutcome::Unchanged;
        };
        marked.proven_stale = true;

        let promotion = entry.promote_to_best_root();
        // Either nothing better existed and the volume is sitting on the root that
        // just proved itself dead, or the best one was dead too.
        entry.tell_volume_if_its_root_is_dead();
        match promotion {
            Promotion::Promoted(new_root) => StaleRootOutcome::Promoted { new_root },
            _ => StaleRootOutcome::Unchanged,
        }
    }

    /// Find the registered non-root volume whose mount root is the longest
    /// ancestor (or equal) of `path`, returning its registry id.
    ///
    /// Used by index read routing to map a `/Volumes/X/…` path to the per-mount
    /// index it belongs to. `root` (`/`) is skipped: it prefixes every path and is
    /// the fallback the router uses when nothing more specific matches. Component-
    /// wise `starts_with` avoids a `/Volumes/XY`-matches-`/Volumes/X` false hit, and
    /// the longest-root wins so a nested mount (`/Volumes/X/Y`) beats its parent.
    ///
    /// An on-demand `ArchiveVolume` (its root is the `.zip` file, so it would win
    /// the longest-root race for every path inside the archive) is not a mount and
    /// is skipped: a path inside an archive belongs to the volume that holds the
    /// archive, which is what its callers route by.
    ///
    /// In-memory (one `RwLock<HashMap>` read, no syscall), so it's safe on the
    /// enrichment / dir-stats hot path.
    pub fn mount_id_for_path(&self, path: &str) -> Option<String> {
        let target = Path::new(path);
        self.volumes
            .read_ignore_poison()
            .iter()
            .map(|(id, entry)| (id, &entry.volume))
            // By type, not by LRU membership: an archive is registered a moment before
            // it enters the LRU, and a concurrent caller must not see it in that gap.
            .filter(|(_, v)| v.as_any().downcast_ref::<ArchiveVolume>().is_none())
            .filter(|(_, v)| v.root() != Path::new("/"))
            .filter(|(_, v)| target.starts_with(v.root()))
            .max_by_key(|(_, v)| v.root().as_os_str().len())
            .map(|(id, _)| id.clone())
    }
}

/// What [`Registration::promote_to_best_root`] did.
pub(super) enum Promotion {
    Promoted(PathBuf),
    /// The active root is already the best-ranked one.
    AlreadyBest,
    /// A better root exists but the backend won't leave its own.
    BackendCantReroot,
    NoRootsLeft,
}

/// What removing a mount root did to the registry. Every arm names an ID so the
/// caller can log, emit, and (for the last arm) tear the volume down.
pub enum RootRemoval {
    /// No registration claims this root.
    Unknown,
    /// A FALLBACK root went away; the active root and the volume are untouched.
    SiblingDropped { id: String },
    /// The active root went away and a surviving sibling took over.
    Promoted { id: String, new_root: PathBuf },
    /// The active root went away, a sibling survives, but the backend can't be
    /// re-rooted, so the registration stays where it is. The volume keeps serving
    /// whoever holds it, which beats unregistering a filesystem that's still
    /// reachable. No shipping backend declines today (`LocalPosixVolume` and
    /// `SmbVolume` both re-root), so this is the safety net for the next one.
    ActiveRootStranded { id: String },
    /// The LAST root went away, so the registration is gone. The caller owns the
    /// teardown (`on_unmount`, index stop).
    Unregistered { id: String, volume: Arc<dyn Volume> },
}

/// What marking a root stale did.
pub enum StaleRootOutcome {
    /// Nothing to do: no such volume, no such root, or nothing better to move to.
    Unchanged,
    /// A live sibling took over the ID.
    Promoted { new_root: PathBuf },
}

/// Whether `errno` proves the MOUNT behind a path is gone or wedged, rather than
/// saying something about the file.
///
/// Typed errno matching, never message text:
/// these reach us through `VolumeError::IoError { raw_os_error }`. The set matches
/// what the transfer layer already treats as a lost connection
/// (`write_operations/error_classification.rs`), plus `ESTALE`, which is precisely
/// "this handle's filesystem moved out from under you".
#[cfg(unix)]
pub fn is_stale_mount_errno(errno: i32) -> bool {
    matches!(
        errno,
        libc::ENOTCONN
            | libc::ETIMEDOUT
            | libc::EHOSTDOWN
            | libc::EHOSTUNREACH
            | libc::ENETDOWN
            | libc::ENETUNREACH
            | libc::ESTALE
    )
}

#[cfg(not(unix))]
pub fn is_stale_mount_errno(_errno: i32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::super::get_volume_manager;
    use super::*;
    use crate::file_system::listing::FileEntry;
    use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume, VolumeError};
    use crate::ignore_poison::IgnorePoison;
    use std::pin::Pin;

    fn registration_over(roots: &[&str]) -> Registration {
        let mut reg = Registration::new(Arc::new(LocalPosixVolume::new("share", roots[0])));
        for root in &roots[1..] {
            reg.record_root(Path::new(root));
        }
        reg
    }

    #[test]
    fn among_equally_live_roots_the_shortest_path_wins() {
        // The original fix, now a tie-break: macOS suffixes the later mount, so
        // the shortest root is the one saved paths and favorites refer to.
        let reg = registration_over(&["/Volumes/naspi-1", "/Volumes/naspi"]);
        assert_eq!(reg.best_root().expect("two roots").path, Path::new("/Volumes/naspi"));
    }

    #[test]
    fn equal_length_roots_break_ties_lexicographically() {
        // Pure and order-independent, so discovery order can't decide identity.
        let reg = registration_over(&["/Volumes/bbb", "/Volumes/aaa"]);
        assert_eq!(reg.best_root().expect("two roots").path, Path::new("/Volumes/aaa"));
    }

    #[test]
    fn a_proven_stale_root_loses_to_a_longer_live_one() {
        // The "worse variant": the NAS drops, macOS leaves the original mount
        // wedged, and the reconnect lands at the suffixed path. Path shape alone
        // picks the dead one every time, including across restarts.
        let mut reg = registration_over(&["/Volumes/naspi", "/Volumes/naspi-1"]);
        reg.roots[0].proven_stale = true;
        assert_eq!(reg.best_root().expect("two roots").path, Path::new("/Volumes/naspi-1"));
    }

    #[test]
    fn promotion_rebuilds_the_volume_at_the_new_root() {
        let mut reg = registration_over(&["/Volumes/naspi", "/Volumes/naspi-1"]);
        reg.roots[0].proven_stale = true;
        assert!(matches!(reg.promote_to_best_root(), Promotion::Promoted(_)));
        assert_eq!(reg.volume.root(), Path::new("/Volumes/naspi-1"));
        assert_eq!(reg.volume.name(), "share", "the display name survives a promotion");
    }

    #[test]
    fn promotion_is_a_no_op_when_the_active_root_is_already_the_best() {
        let mut reg = registration_over(&["/Volumes/naspi", "/Volumes/naspi-1"]);
        assert!(matches!(reg.promote_to_best_root(), Promotion::AlreadyBest));
        assert_eq!(reg.volume.root(), Path::new("/Volumes/naspi"));
    }

    #[test]
    fn stale_mount_errnos_are_told_apart_from_ordinary_file_errors() {
        for errno in [libc::ENOTCONN, libc::ETIMEDOUT, libc::EHOSTDOWN, libc::ESTALE] {
            assert!(
                is_stale_mount_errno(errno),
                "errno {errno}: the mount is gone, not the file"
            );
        }
        // A missing file, a permission wall, or a full disk says nothing about
        // the mount, and promoting on one would rotate a healthy volume's root.
        for errno in [libc::ENOENT, libc::EACCES, libc::ENOSPC, libc::EEXIST] {
            assert!(!is_stale_mount_errno(errno), "errno {errno} is about the file");
        }
    }

    /// A path-addressed backend that re-roots (like every real one does) and
    /// RECORDS every `note_root_mount_gone` the registry sends it, so a test can
    /// ask which root was told its mount is gone. The log is shared across the
    /// instances a promotion creates; production keeps the answer per instance.
    struct RootSpy {
        root: PathBuf,
        told: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    }

    /// A spy rooted at `root`, plus a reader for what it has been told.
    fn root_spy(root: &str) -> (Arc<RootSpy>, impl Fn() -> Vec<PathBuf>) {
        let told = Arc::new(std::sync::Mutex::new(Vec::new()));
        let spy = Arc::new(RootSpy {
            root: PathBuf::from(root),
            told: Arc::clone(&told),
        });
        (spy, move || told.lock_ignore_poison().clone())
    }

    impl Volume for RootSpy {
        fn name(&self) -> &str {
            "spy"
        }

        fn root(&self) -> &Path {
            &self.root
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn rerooted(&self, new_root: &Path) -> Option<Arc<dyn Volume>> {
            Some(Arc::new(Self {
                root: new_root.to_path_buf(),
                told: Arc::clone(&self.told),
            }))
        }

        fn note_root_mount_gone(&self) {
            self.told.lock_ignore_poison().push(self.root.clone());
        }

        fn list_directory<'a>(
            &'a self,
            _path: &'a Path,
            _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn get_metadata<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }

        fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async { false })
        }

        fn is_directory<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }
    }

    #[test]
    fn a_volume_left_on_a_dead_mount_is_told_its_paths_are_gone() {
        // The share is reachable through exactly one mount and that mount dies.
        // A backend riding its own transport (a direct `SmbVolume`) keeps
        // serving, so the registration stays — but every path it hands out now
        // names a mount that isn't there, and only the registry knows.
        let manager = VolumeManager::new();
        let (spy, told) = root_spy("/Volumes/naspi");
        manager.register("share", spy);

        let outcome = manager.mark_root_stale("share", Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, StaleRootOutcome::Unchanged), "nowhere to promote to");
        assert_eq!(
            told(),
            vec![PathBuf::from("/Volumes/naspi")],
            "the volume has to hear that its mount is gone"
        );
    }

    #[test]
    fn a_volume_promoted_onto_a_live_mount_is_told_nothing() {
        // The promotion IS the fix here: the new root is a real mount, so the
        // volume's paths are openable again and nothing should be marked.
        let manager = doubly_mounted_share();
        let (spy, told) = root_spy("/Volumes/naspi");
        manager.register("share", spy);

        let outcome = manager.mark_root_stale("share", Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, StaleRootOutcome::Promoted { .. }));
        assert!(told().is_empty(), "a live root needs no warning");
    }

    #[test]
    fn a_promotion_onto_an_already_dead_sibling_still_tells_the_volume() {
        // Both mounts are gone: one proved it with an errno, the other with an
        // unmount event. The promotion lands on a corpse, so the honest answer
        // travels with it.
        let manager = VolumeManager::new();
        let (spy, told) = root_spy("/Volumes/naspi");
        manager.register("share", spy);
        manager.register("share", Arc::new(LocalPosixVolume::new("naspi", "/Volumes/naspi-1")));

        manager.mark_root_stale("share", Path::new("/Volumes/naspi-1"));
        assert!(told().is_empty(), "the ACTIVE root still looks alive");

        manager.remove_root(Path::new("/Volumes/naspi"));
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi-1")
        );
        assert_eq!(told(), vec![PathBuf::from("/Volumes/naspi-1")]);
    }

    /// A registry holding one share reached through two mount points, the
    /// shortest one active. The shape every multi-root test starts from.
    fn doubly_mounted_share() -> VolumeManager {
        use crate::file_system::LocalPosixVolume;

        let manager = VolumeManager::new();
        manager.register("share", Arc::new(LocalPosixVolume::new("naspi", "/Volumes/naspi")));
        manager.register("share", Arc::new(LocalPosixVolume::new("naspi", "/Volumes/naspi-1")));
        manager
    }

    #[test]
    fn losing_the_active_mount_promotes_a_sibling_instead_of_unregistering() {
        // Ejecting one of two mounts of a share used to take the whole share
        // away until the app restarted (discovery runs at launch only), because
        // the unmount path unregistered the ID it found by root.
        let manager = doubly_mounted_share();

        let outcome = manager.remove_root(Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, RootRemoval::Promoted { .. }), "a sibling survives");
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi-1")
        );
    }

    #[test]
    fn losing_the_last_mount_unregisters_the_volume() {
        let manager = doubly_mounted_share();
        manager.remove_root(Path::new("/Volumes/naspi"));

        let outcome = manager.remove_root(Path::new("/Volumes/naspi-1"));
        assert!(
            matches!(outcome, RootRemoval::Unregistered { .. }),
            "the last root gone means gone"
        );
        assert!(manager.get("share").is_none());
    }

    #[test]
    fn losing_a_fallback_mount_leaves_the_active_one_alone() {
        let manager = doubly_mounted_share();

        let outcome = manager.remove_root(Path::new("/Volumes/naspi-1"));
        assert!(matches!(outcome, RootRemoval::SiblingDropped { .. }));
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi")
        );
        assert_eq!(manager.known_roots("share"), vec![PathBuf::from("/Volumes/naspi")]);
    }

    #[test]
    fn a_backend_that_cant_reroot_keeps_its_registration() {
        // `InMemoryVolume` takes the conservative `rerooted` default, standing in
        // for a backend whose transport is anchored to its root. Losing the
        // active mount must not unregister such a volume while another mount
        // reaches the same filesystem: its transport may not ride the mount at
        // all, so dropping it would kill something that still works.
        let manager = VolumeManager::new();
        manager.register(
            "share",
            Arc::new(InMemoryVolume::new("Direct SMB").with_root("/Volumes/naspi")),
        );
        manager.register(
            "share",
            Arc::new(InMemoryVolume::new("Second mount").with_root("/Volumes/naspi-1")),
        );

        let outcome = manager.remove_root(Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, RootRemoval::ActiveRootStranded { .. }));
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi"),
            "it stays where its transport is anchored"
        );
    }

    #[test]
    fn a_stranded_root_stays_findable_even_though_its_mount_is_gone() {
        // The entry is still anchored to `/Volumes/naspi` (the backend declined to
        // re-root), so the set must still contain it, or `find_by_root` stops
        // recognizing the volume by the very root `volume.root()` reports and a
        // later event for that path lands nowhere.
        let manager = VolumeManager::new();
        manager.register(
            "share",
            Arc::new(InMemoryVolume::new("Direct SMB").with_root("/Volumes/naspi")),
        );
        manager.register(
            "share",
            Arc::new(InMemoryVolume::new("Second mount").with_root("/Volumes/naspi-1")),
        );

        manager.remove_root(Path::new("/Volumes/naspi"));

        assert_eq!(
            manager.find_by_root(Path::new("/Volumes/naspi")).map(|(id, _)| id),
            Some("share".to_string()),
            "the stranded active root is still one of the volume's known roots"
        );
        assert!(
            manager.known_roots("share").contains(&PathBuf::from("/Volumes/naspi")),
            "and it reports as such"
        );
    }

    #[test]
    fn a_stale_mount_errno_hands_the_id_to_a_live_sibling() {
        // Liveness outranks path shape. macOS leaves a wedged `/Volumes/naspi`
        // in place and lands the reconnect at `/Volumes/naspi-1`; picking the
        // shortest path then picks the corpse, on every launch, forever.
        let manager = doubly_mounted_share();
        assert_eq!(
            manager.get("share").expect("registered").root(),
            Path::new("/Volumes/naspi"),
            "while both look alive, the shortest path is active"
        );

        let outcome = manager.mark_root_stale("share", Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, StaleRootOutcome::Promoted { .. }));
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi-1")
        );
    }

    #[test]
    fn a_failed_operation_promotes_only_on_an_errno_that_proves_the_mount_is_gone() {
        use crate::file_system::LocalPosixVolume;
        use crate::file_system::volume::{VolumeError, note_root_failure};

        // Drives the global registry (that's what `note_root_failure` reads), so
        // it uses ids no other test touches.
        let id = "cmdr-test-stale-errno-share";
        let manager = get_volume_manager();
        manager.unregister(id);
        manager.register(id, Arc::new(LocalPosixVolume::new("naspi", "/Volumes/cmdr-test-stale")));
        manager.register(
            id,
            Arc::new(LocalPosixVolume::new("naspi", "/Volumes/cmdr-test-stale-1")),
        );

        // A missing file says nothing about the mount.
        note_root_failure(
            id,
            &VolumeError::IoError {
                message: "no such file".to_string(),
                raw_os_error: Some(libc::ENOENT),
            },
        );
        assert_eq!(
            manager.get(id).expect("registered").root(),
            Path::new("/Volumes/cmdr-test-stale"),
            "an ordinary file error must not rotate a healthy volume's root"
        );

        note_root_failure(
            id,
            &VolumeError::IoError {
                message: "socket is not connected".to_string(),
                raw_os_error: Some(libc::ENOTCONN),
            },
        );
        assert_eq!(
            manager.get(id).expect("still registered").root(),
            Path::new("/Volumes/cmdr-test-stale-1"),
            "a stale-mount errno moves the volume to the mount that still answers"
        );

        manager.unregister(id);
    }

    // ── Retirement through the unmount path ─────────────────────

    use crate::file_system::volume::manager::test_support::RetiringVolume;

    /// Losing the LAST mount really does end the volume, so its background work
    /// has to learn that. `remove_root` is a second way out of the registry
    /// beside `unregister`, and a volume that leaves either way is equally gone.
    #[test]
    fn losing_the_last_mount_retires_the_volume() {
        let manager = VolumeManager::new();
        let (volume, is_retired) = RetiringVolume::at("/Volumes/naspi");
        manager.register("naspi", volume);

        manager.remove_root(Path::new("/Volumes/naspi"));

        assert!(
            is_retired(),
            "the volume left the registry, so nothing may keep acting for it"
        );
    }

    /// The share is still there, reached through a surviving mount, and the
    /// promoted instance shares the state the background work hangs off. Retiring
    /// here would stop the watcher of a share that is perfectly healthy.
    #[test]
    fn promoting_a_surviving_mount_retires_nobody() {
        let manager = VolumeManager::new();
        let (volume, is_retired) = RetiringVolume::at("/Volumes/naspi");
        manager.register("naspi", Arc::clone(&volume) as Arc<dyn Volume>);
        manager.register("naspi", volume.rerooted(Path::new("/Volumes/naspi-1")).unwrap());

        let outcome = manager.remove_root(Path::new("/Volumes/naspi"));

        assert!(matches!(outcome, RootRemoval::Promoted { .. }), "expected a promotion");
        assert!(!is_retired(), "the share is still registered, just at another mount");
    }
}

//! The set of mount roots one volume ID owns, and the rules for picking which
//! of them is active.
//!
//! One filesystem can be reached through several mount points and they all
//! derive one volume ID (an SMB share keys on `(server, port, share)`, a local
//! disk on its filesystem UUID). So a registry entry holds the SET of roots
//! carrying its ID with exactly one ACTIVE — `volume.root()` — and promotes a
//! survivor when the active one dies. Rationale and the flows that drive it:
//! `../DETAILS.md` § "A volume ID owns a set of mount roots".

use super::Volume;
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
    /// re-rooted, so the registration stays where it is. The volume keeps
    /// serving whoever holds it (a direct `SmbVolume` rides smb2, not the mount),
    /// which beats unregistering a share that's still reachable.
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
/// Typed errno matching, never message text (`.claude/rules/no-string-matching.md`):
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
    use super::*;
    use crate::file_system::volume::LocalPosixVolume;

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
            assert!(is_stale_mount_errno(errno), "errno {errno} proves the mount is gone");
        }
        // A missing file, a permission wall, or a full disk says nothing about
        // the mount, and promoting on one would rotate a healthy volume's root.
        for errno in [libc::ENOENT, libc::EACCES, libc::ENOSPC, libc::EEXIST] {
            assert!(!is_stale_mount_errno(errno), "errno {errno} is about the file");
        }
    }
}

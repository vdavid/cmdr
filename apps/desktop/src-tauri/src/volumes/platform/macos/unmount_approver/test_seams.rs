//! The seams a test hands the approver and its gate: an index nothing really starts, stops the test
//! can hold open, and a host that acts only on the test's own volumes.
//!
//! Shared by the pure callback tests and the real-image lane, which differ only in where presence
//! comes from: a set the test writes, or the machine's own mount table.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use cmdr_index::{IndexVolumeKind, RemovableStop};

use super::callbacks::{Approver, AskedDisk, Host};
use crate::file_system::volume::drive_release::{DriveRelease, IndexDoor, ResumeBatch, Ticket};
use crate::ignore_poison::IgnorePoison;
use crate::volumes::disk_units::MountedVolume;

/// The index behind the gate: what's indexing, what intent says to resume, and every start a resume
/// ran. Nothing here touches a real index.
#[derive(Default)]
pub(super) struct FakeIndex {
    indexing: Mutex<HashSet<String>>,
    intent: Mutex<HashSet<String>>,
    started: Mutex<Vec<String>>,
    /// How many stops actually found an index to stop.
    stopped: AtomicUsize,
    /// Each volume's root, for the presence answer a person's start reads.
    roots: Mutex<HashMap<String, PathBuf>>,
}

impl FakeIndex {
    /// The volume is indexing, and its intent says it should be.
    pub(super) fn indexes(&self, volume_id: &str) {
        self.indexing.lock_ignore_poison().insert(volume_id.to_string());
        self.intent.lock_ignore_poison().insert(volume_id.to_string());
    }

    pub(super) fn note_root(&self, volume_id: &str, root: &Path) {
        self.roots
            .lock_ignore_poison()
            .insert(volume_id.to_string(), root.to_path_buf());
    }

    pub(super) fn is_indexing(&self, volume_id: &str) -> bool {
        self.indexing.lock_ignore_poison().contains(volume_id)
    }

    /// Every volume a resume started again, in order.
    pub(super) fn started(&self) -> Vec<String> {
        self.started.lock_ignore_poison().clone()
    }

    /// How many stops found an index to stop.
    pub(super) fn stops_that_found_an_index(&self) -> usize {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Stop the volume, answering what a real stop would.
    fn stop(&self, volume_id: &str) -> RemovableStop {
        if self.indexing.lock_ignore_poison().remove(volume_id) {
            self.stopped.fetch_add(1, Ordering::SeqCst);
            RemovableStop::Released
        } else {
            RemovableStop::NothingToStop
        }
    }
}

impl IndexDoor for FakeIndex {
    fn volume_kind(&self, volume_id: &str) -> Option<IndexVolumeKind> {
        self.is_indexing(volume_id).then_some(IndexVolumeKind::LocalExternal)
    }

    fn drives_to_resume(&self) -> Vec<String> {
        self.intent
            .lock_ignore_poison()
            .iter()
            .filter(|volume_id| !self.is_indexing(volume_id))
            .cloned()
            .collect()
    }

    fn start_resumed(&self, volume_id: String, ticket: Ticket) {
        self.indexing.lock_ignore_poison().insert(volume_id.clone());
        self.started.lock_ignore_poison().push(volume_id);
        drop(ticket);
    }

    fn is_ejecting(&self, _volume_id: &str) -> bool {
        false
    }

    fn is_listed(&self, volume_id: &str) -> bool {
        let root = self.roots.lock_ignore_poison().get(volume_id).cloned();
        match root {
            Some(root) => cmdr_local::mount_table::is_mount_point(&root.to_string_lossy()) != Some(false),
            None => true,
        }
    }
}

/// The stops the host runs: they record who was asked and whether the volume was still mounted then,
/// and a held one blocks until the test opens it.
#[derive(Default)]
pub(super) struct Stops {
    state: Mutex<StopsState>,
    opened: Condvar,
}

#[derive(Default)]
struct StopsState {
    asked: Vec<(String, bool)>,
    held: HashSet<String>,
    open: HashSet<String>,
}

impl Stops {
    /// This volume's stop blocks until [`Self::open`], so a test can act while an ask is inside it.
    pub(super) fn hold(&self, volume_id: &str) {
        self.state.lock_ignore_poison().held.insert(volume_id.to_string());
    }

    pub(super) fn open(&self, volume_id: &str) {
        self.state.lock_ignore_poison().open.insert(volume_id.to_string());
        self.opened.notify_all();
    }

    /// Every volume a stop was asked about, in order.
    pub(super) fn asked(&self) -> Vec<String> {
        self.state
            .lock_ignore_poison()
            .asked
            .iter()
            .map(|(volume_id, _)| volume_id.clone())
            .collect()
    }

    /// Whether every stop so far ran while its volume was still mounted, which is the whole point of
    /// a pre-unmount hook.
    pub(super) fn all_ran_while_mounted(&self) -> bool {
        self.state
            .lock_ignore_poison()
            .asked
            .iter()
            .all(|(_, was_mounted)| *was_mounted)
    }

    fn run(&self, volume_id: &str, was_mounted: bool) {
        let mut state = self.state.lock_ignore_poison();
        state.asked.push((volume_id.to_string(), was_mounted));
        while state.held.contains(volume_id) && !state.open.contains(volume_id) {
            state = self.opened.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// Where a test's presence answers come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Presence {
    /// The machine's own mount table: the real-image lane really unmounts things.
    MountTable,
    /// What the test says is mounted.
    AsTold,
}

/// The app seams, over the test's own volumes only.
pub(super) struct FakeHost {
    /// The BSD nodes this approver acts on. Every other disk approves at once, which is what keeps a
    /// test session from answering for the rest of the Mac.
    pub(super) nodes: Mutex<HashSet<String>>,
    /// Mount point to volume id: the registry an ask reads.
    pub(super) volumes: Mutex<HashMap<PathBuf, String>>,
    pub(super) index: Arc<FakeIndex>,
    pub(super) stops: Arc<Stops>,
    pub(super) busy: Mutex<HashSet<String>>,
    pub(super) ejecting: Mutex<HashSet<String>>,
    /// Volumes Cmdr's own eject is taking down alone, leaving the rest of the disk up.
    pub(super) ejecting_alone: Mutex<HashSet<String>>,
    /// Every volume a VANISH stop was asked about, kept apart from the pre-unmount stops so a test
    /// can tell "let go of before the unmount" from "cleaned up after the drive went".
    vanish_stops: Mutex<Vec<String>>,
    presence: Presence,
    /// What's mounted, for [`Presence::AsTold`].
    pub(super) mounted: Mutex<HashSet<PathBuf>>,
    idles: AtomicUsize,
    resumes: Mutex<Vec<ResumeBatch>>,
    /// Every repo release an ask made: the mount roots, and the deadline it had.
    repo_releases: Mutex<Vec<(Vec<PathBuf>, Instant)>>,
    /// What a repo release answers when no real portal is set; `true` unless a test says the repos
    /// won't let go.
    pub(super) repos_let_go: AtomicBool,
    /// A real portal to release repos through, for the real-image lane.
    pub(super) git: Mutex<Option<Arc<cmdr_git::GitPortal>>>,
}

impl FakeHost {
    /// Every repo release an ask made, in order.
    pub(super) fn repo_releases(&self) -> Vec<(Vec<PathBuf>, Instant)> {
        self.repo_releases.lock_ignore_poison().clone()
    }

    /// How many idle callbacks the approver has handled.
    pub(super) fn idles(&self) -> usize {
        self.idles.load(Ordering::SeqCst)
    }

    /// The resume batches an idle handed to the gate, so a test can wait for them to settle.
    pub(super) fn take_resumes(&self) -> Vec<ResumeBatch> {
        std::mem::take(&mut *self.resumes.lock_ignore_poison())
    }

    /// The volume mounted at `path`, however the test said so.
    pub(super) fn volume_at(&self, path: &Path) -> Option<String> {
        self.volumes.lock_ignore_poison().get(path).cloned()
    }

    /// Every volume stopped because its drive went away, in order.
    pub(super) fn vanish_stops(&self) -> Vec<String> {
        self.vanish_stops.lock_ignore_poison().clone()
    }

    fn is_mounted(&self, path: &Path) -> bool {
        match self.presence {
            Presence::MountTable => cmdr_local::mount_table::is_mount_point(&path.to_string_lossy()) == Some(true),
            Presence::AsTold => self.mounted.lock_ignore_poison().contains(path),
        }
    }

    fn root_of(&self, volume_id: &str) -> Option<PathBuf> {
        self.volumes
            .lock_ignore_poison()
            .iter()
            .find(|(_, id)| *id == volume_id)
            .map(|(path, _)| path.clone())
    }
}

impl Host for FakeHost {
    fn acts_on(&self, bsd_name: &str) -> bool {
        self.nodes.lock_ignore_poison().contains(bsd_name)
    }

    fn volume_at_active_root(&self, path: &Path) -> Option<String> {
        self.volume_at(path)
    }

    fn is_mounted_at(&self, _bsd_name: &str, path: &Path) -> bool {
        self.is_mounted(path)
    }

    fn stop(&self, volume_id: &str) -> RemovableStop {
        let was_mounted = self.root_of(volume_id).is_some_and(|root| self.is_mounted(&root));
        self.stops.run(volume_id, was_mounted);
        self.index.stop(volume_id)
    }

    fn stop_after_vanish(&self, volume_id: &str) -> RemovableStop {
        self.vanish_stops.lock_ignore_poison().push(volume_id.to_string());
        self.index.stop(volume_id)
    }

    fn is_indexing(&self, volume_id: &str) -> bool {
        self.index.is_indexing(volume_id)
    }

    fn busy_volume_ids(&self) -> Vec<String> {
        self.busy.lock_ignore_poison().iter().cloned().collect()
    }

    fn is_ejecting(&self, volume_id: &str) -> bool {
        self.ejecting.lock_ignore_poison().contains(volume_id)
    }

    fn is_ejecting_alone(&self, volume_id: &str) -> bool {
        self.ejecting_alone.lock_ignore_poison().contains(volume_id)
    }

    fn note_idle(&self) {
        self.idles.fetch_add(1, Ordering::SeqCst);
    }

    fn note_resume(&self, batch: ResumeBatch) {
        self.resumes.lock_ignore_poison().push(batch);
    }

    fn let_go_of_repos(&self, mount_roots: &[PathBuf], deadline: Instant) -> bool {
        self.repo_releases
            .lock_ignore_poison()
            .push((mount_roots.to_vec(), deadline));
        let portal = self.git.lock_ignore_poison().clone();
        let Some(portal) = portal else {
            return self.repos_let_go.load(Ordering::SeqCst);
        };
        let mut all_let_go = true;
        for root in mount_roots {
            all_let_go &= portal.release_volume(root, deadline);
        }
        all_let_go
    }
}

/// A gate, its index, the host, and the approver over them.
pub(super) struct Fixture {
    pub(super) gate: DriveRelease,
    pub(super) index: Arc<FakeIndex>,
    pub(super) host: Arc<FakeHost>,
    pub(super) approver: Arc<Approver>,
}

impl Fixture {
    /// Register a volume mounted at `path` as `volume_id` on the BSD node `bsd_name`, indexing.
    pub(super) fn indexed_volume(&self, volume_id: &str, bsd_name: &str, path: &Path) {
        self.registered_volume(volume_id, bsd_name, path);
        self.index.indexes(volume_id);
    }

    /// Register a volume mounted at `path` as `volume_id` on the BSD node `bsd_name`, with no index
    /// on it: a disk image, or a drive the person never turned indexing on for.
    pub(super) fn registered_volume(&self, volume_id: &str, bsd_name: &str, path: &Path) {
        self.host.nodes.lock_ignore_poison().insert(bsd_name.to_string());
        self.host
            .volumes
            .lock_ignore_poison()
            .insert(path.to_path_buf(), volume_id.to_string());
        self.host.mounted.lock_ignore_poison().insert(path.to_path_buf());
        self.index.note_root(volume_id, path);
    }
}

/// A fixture on a fake clock, for the pure callback tests.
pub(super) fn fixture() -> Fixture {
    build(Presence::AsTold, DriveRelease::with_fake_clock)
}

/// A fixture on the real clock and the machine's own mount table, for the real-image lane.
pub(super) fn real_fixture() -> Fixture {
    build(Presence::MountTable, DriveRelease::with_door)
}

fn build(presence: Presence, gate: impl FnOnce(Arc<dyn IndexDoor>) -> DriveRelease) -> Fixture {
    let index = Arc::new(FakeIndex::default());
    let host = Arc::new(FakeHost {
        nodes: Mutex::default(),
        volumes: Mutex::default(),
        index: Arc::clone(&index),
        stops: Arc::new(Stops::default()),
        busy: Mutex::default(),
        ejecting: Mutex::default(),
        ejecting_alone: Mutex::default(),
        vanish_stops: Mutex::default(),
        presence,
        mounted: Mutex::default(),
        idles: AtomicUsize::new(0),
        resumes: Mutex::default(),
        repo_releases: Mutex::default(),
        repos_let_go: AtomicBool::new(true),
        git: Mutex::default(),
    });
    let gate = gate(Arc::clone(&index) as Arc<dyn IndexDoor>);
    let approver = Approver::new(gate.clone(), Arc::clone(&host) as Arc<dyn Host>);
    Fixture {
        gate,
        index,
        host,
        approver,
    }
}

/// The disk a test's ask is about.
pub(super) fn asked(bsd_name: &str, whole_unit: u32, path: &Path) -> AskedDisk {
    AskedDisk {
        bsd_name: bsd_name.to_string(),
        volume_uuid: Some(format!("uuid-of-{bsd_name}")),
        whole_unit,
        path: Some(path.to_path_buf()),
    }
}

/// One volume of the asked disk's whole unit, as the group lookup answers.
pub(super) fn mounted(bsd_name: &str, whole_unit: u32, path: &Path) -> MountedVolume {
    MountedVolume {
        bsd_name: bsd_name.to_string(),
        whole_unit,
        volume_uuid: Some(format!("uuid-of-{bsd_name}")),
        path: path.to_path_buf(),
    }
}

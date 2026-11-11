//! What the approver does for each DiskArbitration callback, over injected seams.
//!
//! Every callback runs on the session's one serial queue, so these run one at a time and in DA's
//! delivery order. ❌ The ask path touches no filesystem and takes no SQLite connection: the one
//! wait it makes is the gate's own, bounded by the chain's deadline.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::log_private::LogPrivate;
use cmdr_index::RemovableStop;

use super::ask::{self, Answer, Chain, Unwaited, VolumeAnswer};
use super::causes::{Causes, Vanished};
use super::records::{DiskVolume, Records};
use crate::file_system::volume::drive_release::{
    DriveRelease, LateRelease, Release, ResumeBatch, ResumeCandidate, ResumeOwner,
};
use crate::ignore_poison::IgnorePoison;
use crate::volumes::disk_units::MountedVolume;

/// What an ask asks of the app: the registry, the index, and the mount table. The app's answers are
/// `AppHost`; a lane test hands in its own, so its session acts only on its own disk image and its
/// stop is injected.
pub(crate) trait Host: Send + Sync + 'static {
    /// Whether the approver acts on this BSD node at all, rather than approving at once. The app
    /// acts on every disk.
    fn acts_on(&self, bsd_name: &str) -> bool;
    /// The registered volume whose ACTIVE root is `path`. A volume reachable through a spare mount
    /// keeps working when that mount goes, so only the active root counts.
    fn volume_at_active_root(&self, path: &Path) -> Option<String>;
    /// Whether `bsd_name` is still mounted at `path`, from the non-blocking mount table.
    fn is_mounted_at(&self, bsd_name: &str, path: &Path) -> bool;
    /// Stop the volume's index and wait for it to let go of the drive.
    fn stop(&self, volume_id: &str) -> RemovableStop;
    /// Stop the volume's index after its drive went away with nobody asking Cmdr to let go of it
    /// first. Nothing waits on the answer: the mount is already gone, so this only releases the
    /// watcher and the handles still pointed at it.
    fn stop_after_vanish(&self, volume_id: &str) -> RemovableStop;
    /// Whether the volume has a local external index right now.
    fn is_indexing(&self, volume_id: &str) -> bool;
    /// The volumes a write op is busy on.
    fn busy_volume_ids(&self) -> Vec<String>;
    /// Whether Cmdr's own eject of the volume is in flight. Such a volume is the flight's to stop
    /// and to resume.
    fn is_ejecting(&self, volume_id: &str) -> bool;
    /// Whether Cmdr's own eject is taking this volume down alone, leaving the rest of its disk up.
    /// Only then does an ask know it isn't the first of a whole-disk request's.
    fn is_ejecting_alone(&self, volume_id: &str) -> bool;
    /// Let go of every git repo on the volumes mounted at `mount_roots`, waiting until `deadline`
    /// for the reads still holding one, and answer whether all let go. A plain unmount fails while
    /// any repo there has a pack file mapped. In memory only: no filesystem.
    fn let_go_of_repos(&self, mount_roots: &[PathBuf], deadline: Instant) -> bool;
    /// An idle callback came. For a test that waits for one.
    fn note_idle(&self) {}
    /// An idle handed these volumes back to the gate. For a test that waits for the batch.
    fn note_resume(&self, batch: ResumeBatch) {
        drop(batch);
    }
}

/// The disk a callback is about, read from its description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AskedDisk {
    pub(crate) bsd_name: String,
    pub(crate) volume_uuid: Option<String>,
    pub(crate) whole_unit: u32,
    /// Where its volume is mounted, `None` for a disk with no mounted volume.
    pub(crate) path: Option<PathBuf>,
}

/// The volumes one ask found on its whole disk, and whether it could see all of them.
///
/// ❗ Two answers, ❌ never one list, the same rule `DiskMounts` and `HolderScan` carry. The
/// volumes are what this ask will let go of; `unit_unreadable` says the mount table wouldn't
/// answer, so the disk may hold volumes that aren't in the list and this ask can't promise it
/// stopped them. Only a READ unit may be approved.
struct AskGroup {
    /// The asked volume, plus every registered volume found on the same whole disk.
    volumes: Vec<DiskVolume>,
    /// The mount table wouldn't answer, so `volumes` is what could be seen, not what's there.
    unit_unreadable: bool,
}

/// How long the stop after a vanish waits for the index to let go of the drive.
///
/// Nothing waits on the answer, and nothing can be refused: the mount is already gone, so this only
/// bounds how long the stopping thread goes before it says so in the log. The same tier as the
/// eject's own index-stop deadline, so both paths call a stop "still releasing" by one clock.
pub(super) const VANISH_STOP_WAIT: Duration = Duration::from_secs(15);

// DEFAULT-OK: no ask has come yet, so there's no chain to join and nothing is remembered.
#[derive(Debug, Default)]
struct State {
    chain: Chain,
    records: Records,
    causes: Causes,
}

/// The approver's own state, minus DiskArbitration.
pub(crate) struct Approver {
    gate: DriveRelease,
    host: Arc<dyn Host>,
    state: Arc<Mutex<State>>,
    owner: Arc<dyn ResumeOwner>,
}

impl Approver {
    pub(crate) fn new(gate: DriveRelease, host: Arc<dyn Host>) -> Arc<Self> {
        let state = Arc::new(Mutex::new(State::default()));
        let owner = Arc::new(RecordOwner {
            state: Arc::clone(&state),
            host: Arc::clone(&host),
        });
        Arc::new(Self {
            gate,
            host,
            state,
            owner,
        })
    }

    /// DiskArbitration asks whether `disk` may unmount. `volumes_on_unit` answers which volumes are
    /// mounted on the same whole disk (❗ `None` when nobody could read the mount table, ❌ never an
    /// empty list), and is called only once a Cmdr volume is on it.
    ///
    /// Blocks until the disk's every volume has let go or the chain's deadline passes, which is the
    /// one wait the DA queue may make.
    pub(crate) fn on_unmount_ask(
        &self,
        disk: &AskedDisk,
        volumes_on_unit: impl FnOnce(u32) -> Option<Vec<MountedVolume>>,
    ) -> Answer {
        if !self.host.acts_on(&disk.bsd_name) {
            return Answer::Approve;
        }
        let Some(path) = disk.path.as_deref() else {
            return Answer::Approve;
        };
        let Some(volume_id) = self.host.volume_at_active_root(path) else {
            return Answer::Approve;
        };
        let AskGroup {
            volumes: group,
            unit_unreadable,
        } = self.group_of(disk, path, volume_id, volumes_on_unit);
        let ids: Vec<String> = group.iter().map(|volume| volume.volume_id.clone()).collect();
        {
            // Every volume the ask can see, recorded while it's still mounted: once its disk is
            // gone, nothing can look up what was on it.
            let mut state = self.state.lock_ignore_poison();
            for volume in &group {
                state
                    .causes
                    .saw_volume(&volume.bsd_name, volume.whole_unit, &volume.volume_id);
            }
        }

        let now = self.gate.now();
        let (generation, deadline) = {
            let mut state = self.state.lock_ignore_poison();
            (
                state.records.begin_ask(disk.whole_unit, &group),
                state.chain.deadline_for(now),
            )
        };
        self.gate.set_unmount_pending(&ids);

        // A plain unmount fails while a repo on the disk has a pack file mapped, so the repos let
        // go first, under the same deadline. In memory only, and milliseconds unless a status or
        // commit walk is mid-read, which it stops.
        let roots: Vec<PathBuf> = group.iter().map(|volume| volume.path.clone()).collect();
        let repos_let_go = self.host.let_go_of_repos(&roots, deadline);

        // Read before the release, and after the group went unmount-pending, so no background start
        // can land on a volume between this read and the answer (a person's waits the unmount out).
        // Past the deadline the ask answers from these alone; inside it, a volume with no work never
        // dissents over a stop the deadline cut short.
        let indexing: Vec<bool> = ids.iter().map(|id| self.host.is_indexing(id)).collect();
        let ticket_in_flight: Vec<bool> = ids.iter().map(|id| self.gate.holds_ticket(id)).collect();
        let had_work: HashMap<&str, bool> = ids
            .iter()
            .zip(indexing.iter().zip(&ticket_in_flight))
            .map(|(id, (indexing, ticket))| (id.as_str(), *indexing || *ticket))
            .collect();
        let unwaited = (now >= deadline).then(|| Unwaited {
            indexing: indexing.contains(&true),
            ticket_in_flight: ticket_in_flight.contains(&true),
            busy: self.is_busy(&ids) || !repos_let_go,
        });

        let release = self.release(&group, generation, deadline);
        {
            let mut state = self.state.lock_ignore_poison();
            for released in &release.volumes {
                let Some(volume) = group.iter().find(|volume| volume.volume_id == released.volume_id) else {
                    continue;
                };
                // A volume Cmdr's own eject is taking down is that flight's to resume.
                if self.host.is_ejecting(&volume.volume_id) {
                    continue;
                }
                state
                    .records
                    .note_release(volume, released.outcome, released.epoch, generation);
            }
        }

        let released = match unwaited {
            Some(disk_facts) => ask::without_waiting(disk_facts),
            None => ask::after_release(
                release.volumes.iter().map(|released| VolumeAnswer {
                    outcome: released.outcome,
                    // A volume the release answered for but this ask never read counts as having work.
                    had_work: had_work.get(released.volume_id.as_str()).copied().unwrap_or(true),
                }),
                self.is_busy(&ids) || !repos_let_go,
            ),
        };
        // ❗ An ask that couldn't read its disk refuses, whatever the volumes it COULD see answered:
        // it can't name the disk's other volumes, so it can't have let go of them, and approving
        // would unmount a sibling under a live FSEvents watcher. ❌ An unreadable mount table is
        // never an empty disk — the same collapse this area has shipped three times.
        let answer = if unit_unreadable { Answer::Dissent } else { released };
        // This ask's OWN runtime: past DiskArbitration's window, the approvals after it on this disk
        // may have been skipped, which is what makes a later disappearance's cause unknowable.
        self.state.lock_ignore_poison().causes.asked(
            &disk.bsd_name,
            disk.whole_unit,
            self.gate.now().saturating_duration_since(now),
        );
        log::info!(
            target: "unmount_approver",
            "{} of {:?} is about to unmount: {answer:?} after letting go of {ids:?} (git repos let go: {repos_let_go}; {:?} of the chain's budget left)",
            // allowed-log-private: a BSD device name (`disk4s1`), the kernel's, not the user's
            disk.bsd_name,
            LogPrivate::dir_path(path),
            deadline.saturating_duration_since(self.gate.now()),
        );
        answer
    }

    /// DiskArbitration asks before an eject too, and the approver answers at once: the drive was let
    /// go of at the unmount approval that precedes it, so there's nothing left to wait for. The ask
    /// matters because it's what later tells a disappearance that the disk was ejected, not pulled.
    pub(crate) fn on_eject_approved(&self, whole_unit: u32) {
        self.state.lock_ignore_poison().causes.eject_approved(whole_unit);
    }

    /// Let go of every volume whose drive left without Cmdr being asked to let go of it first.
    ///
    /// ❗ On a thread of its own: `release` blocks on the gate's condvar, and the one wait a
    /// DiskArbitration queue may make is an ASK's, bounded by that ask's deadline.
    fn let_go_of_what_vanished(&self, vanished: Vec<Vanished>) {
        let owed: Vec<Vanished> = vanished
            .into_iter()
            .filter(|volume| volume.cause.needs_a_stop())
            .collect();
        if owed.is_empty() {
            return;
        }
        for volume in &owed {
            log::warn!(
                target: "unmount_approver",
                "{} ({}) left the mount table with nobody asking Cmdr to let go of it first ({:?}); stopping its index now",
                volume.volume_id,
                // allowed-log-private: a BSD device name (`disk4s1`), the kernel's, not the user's
                volume.bsd_name,
                // allowed-log-private: the disappearance's cause, an enum of our own
                volume.cause
            );
        }
        let gate = self.gate.clone();
        let host = Arc::clone(&self.host);
        let spawned = std::thread::Builder::new()
            .name("unmount-approver-vanish".to_string())
            .spawn(move || {
                let ids: Vec<String> = owed.into_iter().map(|volume| volume.volume_id).collect();
                // ❌ No resume: a drive that vanished is owed nothing back. Its index is marked for a
                // rebuild by the crate's own stop when deletes were in flight.
                gate.release(
                    &ids,
                    gate.now() + VANISH_STOP_WAIT,
                    move |volume_id| host.stop_after_vanish(volume_id),
                    |late| {
                        log::info!(
                            target: "unmount_approver",
                            "{} answered its vanish stop late: {:?}",
                            late.volume_id,
                            late.outcome
                        );
                    },
                );
            });
        if let Err(e) = spawned {
            crate::log_error!(
                target: "unmount_approver",
                "Couldn't start the stop for a drive that vanished: {e}; its index keeps reading a drive that isn't there"
            );
        }
    }

    /// Every registered volume mounted on the asked disk's whole unit, the asked one first.
    ///
    /// **Why the whole unit**: DiskArbitration links a whole-disk request's asks by BSD unit and
    /// they arrive back to back, so the first ask has to stop the unit's group or the second ask
    /// spends its own window waiting.
    fn group_of(
        &self,
        disk: &AskedDisk,
        path: &Path,
        volume_id: String,
        volumes_on_unit: impl FnOnce(u32) -> Option<Vec<MountedVolume>>,
    ) -> AskGroup {
        let mut volumes = vec![DiskVolume {
            volume_id,
            bsd_name: disk.bsd_name.clone(),
            volume_uuid: disk.volume_uuid.clone(),
            whole_unit: disk.whole_unit,
            path: path.to_path_buf(),
        }];
        // ❗ Cmdr's own one-volume eject: the siblings stay mounted, so they keep their indexes. A
        // Finder unmount of one volume looks like a whole-disk request's first ask from here.
        let ejecting_alone = volumes
            .first()
            .is_some_and(|asked| self.host.is_ejecting_alone(&asked.volume_id));
        if ejecting_alone {
            return AskGroup {
                volumes,
                unit_unreadable: false,
            };
        }
        let Some(on_unit) = volumes_on_unit(disk.whole_unit) else {
            log::warn!(
                target: "unmount_approver",
                "Nobody could say what else is mounted on {}'s disk, so this ask can only let go of {:?} and must refuse the unmount",
                // allowed-log-private: a BSD device name (`disk4s1`), the kernel's, not the user's
                disk.bsd_name,
                LogPrivate::dir_path(path)
            );
            return AskGroup {
                volumes,
                unit_unreadable: true,
            };
        };
        for mounted in on_unit {
            let Some(volume_id) = self.host.volume_at_active_root(&mounted.path) else {
                continue;
            };
            if volumes.iter().any(|volume| volume.volume_id == volume_id) {
                continue;
            }
            volumes.push(DiskVolume {
                volume_id,
                bsd_name: mounted.bsd_name,
                volume_uuid: mounted.volume_uuid,
                whole_unit: mounted.whole_unit,
                path: mounted.path,
            });
        }
        AskGroup {
            volumes,
            unit_unreadable: false,
        }
    }

    /// Let go of the whole group under one deadline. A stop the deadline cut short keeps running and
    /// records what it eventually answered, so a late release is still owed a resume.
    fn release(&self, group: &[DiskVolume], generation: u64, deadline: Instant) -> Release {
        let ids: Vec<String> = group.iter().map(|volume| volume.volume_id.clone()).collect();
        let stop = {
            let host = Arc::clone(&self.host);
            move |volume_id: &str| host.stop(volume_id)
        };
        let record_late = {
            let host = Arc::clone(&self.host);
            let state = Arc::clone(&self.state);
            let volumes: HashMap<String, DiskVolume> = group
                .iter()
                .map(|volume| (volume.volume_id.clone(), volume.clone()))
                .collect();
            move |late: LateRelease| {
                let Some(volume) = volumes.get(&late.volume_id) else {
                    return;
                };
                if host.is_ejecting(&late.volume_id) {
                    return;
                }
                state
                    .lock_ignore_poison()
                    .records
                    .note_late_release(volume, late.outcome, late.epoch, generation);
            }
        };
        self.gate.release(&ids, deadline, stop, record_late)
    }

    fn is_busy(&self, ids: &[String]) -> bool {
        let busy = self.host.busy_volume_ids();
        ids.iter().any(|id| busy.contains(id))
    }

    /// DiskArbitration's queue went quiet: no unmount is pending any more, and whatever an ask
    /// stopped and still owes back goes to the gate, which settles and checks it off this queue.
    pub(crate) fn on_idle(&self) {
        self.gate.clear_all_unmount_pending();
        let candidates = self.state.lock_ignore_poison().records.idle();
        self.host.note_idle();
        if candidates.is_empty() {
            return;
        }
        log::info!(target: "unmount_approver", "DiskArbitration is idle; handing back {candidates:?}");
        let batch = self.gate.resume(candidates, Arc::clone(&self.owner));
        self.host.note_resume(batch);
    }

    /// A disk appeared. A WHOLE disk's facts start over, since DiskArbitration hands a freed unit to
    /// the next disk at once; any disk carrying a Cmdr volume is recorded, because a pull has no
    /// other way to learn what was mounted on the disk it takes away.
    pub(crate) fn on_appeared(&self, disk: &AskedDisk, is_whole: bool) {
        if is_whole {
            let mut state = self.state.lock_ignore_poison();
            state.records.appeared(disk.whole_unit);
            state.causes.appeared(disk.whole_unit);
        }
        // Asked without the approver's own lock held: `volume_at_active_root` takes the volume
        // manager's, and ❌ nothing here may hold two locks at once.
        let Some(volume_id) = disk
            .path
            .as_deref()
            .and_then(|path| self.host.volume_at_active_root(path))
        else {
            return;
        };
        self.state
            .lock_ignore_poison()
            .causes
            .saw_volume(&disk.bsd_name, disk.whole_unit, &volume_id);
    }

    /// A whole disk disappeared: nothing on it can be resumed, its volumes' pending flags go, and
    /// any volume that hadn't already reported its own unmount went away with the disk.
    pub(crate) fn on_disappeared(&self, whole_unit: u32) {
        let (cleared, vanished) = {
            let mut state = self.state.lock_ignore_poison();
            (
                state.records.disappeared(whole_unit),
                state.causes.disappeared(whole_unit),
            )
        };
        self.gate.clear_unmount_pending(&cleared);
        self.let_go_of_what_vanished(vanished);
    }

    /// A volume's path cleared, so its unmount happened, whether Cmdr was asked or not.
    pub(crate) fn on_volume_path_cleared(&self, bsd_name: &str) {
        let (cleared, vanished) = {
            let mut state = self.state.lock_ignore_poison();
            (
                state.records.volume_path_cleared(bsd_name),
                state.causes.volume_path_cleared(bsd_name),
            )
        };
        self.gate.clear_unmount_pending(&Vec::from_iter(cleared));
        self.let_go_of_what_vanished(Vec::from_iter(vanished));
    }
}

/// The approver as a resume owner: it stands behind a record until a newer ask on its disk, and
/// reads presence from the mount table, ❌ never from a callback disk's frozen description.
struct RecordOwner {
    state: Arc<Mutex<State>>,
    host: Arc<dyn Host>,
}

impl ResumeOwner for RecordOwner {
    fn name(&self) -> &'static str {
        "the unmount approver"
    }

    fn still_owns(&self, candidate: &ResumeCandidate) -> bool {
        self.state.lock_ignore_poison().records.still_owns(candidate)
    }

    fn is_listed(&self, candidate: &ResumeCandidate) -> bool {
        let state = self.state.lock_ignore_poison();
        let Some(volume) = state.records.volume_of(candidate) else {
            return false;
        };
        let (bsd_name, path) = (volume.bsd_name.clone(), volume.path.clone());
        drop(state);
        self.host.is_mounted_at(&bsd_name, &path)
    }

    fn is_ejected_by_another_owner(&self, candidate: &ResumeCandidate) -> bool {
        self.host.is_ejecting(&candidate.volume_id)
    }

    fn consume(&self, candidate: &ResumeCandidate) {
        self.state.lock_ignore_poison().records.consume(candidate);
    }
}

#[cfg(test)]
#[path = "callbacks_tests.rs"]
mod tests;

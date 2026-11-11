//! What the approver remembers between callbacks: each whole disk's ask generation, the volumes an
//! ask stopped and owes back, and which volumes have an unmount pending.
//!
//! Pure: every callback feeds it in DiskArbitration's delivery order, on the session's one queue,
//! and the resume owner reads it from the gate's thread.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::file_system::volume::drive_release::{ResumeCandidate, VolumeRelease};

/// A registered volume mounted on a disk an ask is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiskVolume {
    pub(super) volume_id: String,
    /// Its BSD node, `diskNsM`.
    pub(super) bsd_name: String,
    pub(super) volume_uuid: Option<String>,
    /// The BSD unit of its whole disk: the `N` of every `diskNsM` on it.
    pub(super) whole_unit: u32,
    /// Its mount point when the ask came.
    pub(super) path: PathBuf,
}

impl DiskVolume {
    /// Records are keyed by BSD node AND volume UUID: DiskArbitration hands a freed node to the next
    /// disk at once.
    fn same_volume(&self, other: &DiskVolume) -> bool {
        self.bsd_name == other.bsd_name && self.volume_uuid == other.volume_uuid
    }
}

/// A volume an ask stopped while it was indexing, owed a resume if its unmount doesn't happen.
#[derive(Debug, Clone)]
struct Record {
    volume: DiskVolume,
    /// The epoch the release that stopped it (or carried it forward) set.
    epoch: u64,
    /// Its whole disk's ask generation at that release.
    generation: u64,
}

// DEFAULT-OK: nothing asked yet, so no generations, no records, and no pending unmounts.
#[derive(Debug, Default)]
pub(super) struct Records {
    last_generation: u64,
    /// Each whole disk's current ask generation, by BSD unit.
    generations: HashMap<u32, u64>,
    records: Vec<Record>,
    /// The volumes an ask marked unmount-pending, by BSD node, until an idle, their disk's
    /// disappearance, or their volume path clearing.
    pending: HashMap<String, DiskVolume>,
}

impl Records {
    /// A new ask on the whole disk `whole_unit` about `disk`, every registered volume mounted on it:
    /// moves the disk's generation, which voids every candidate offered before, and marks the
    /// volumes unmount-pending. Answers the new generation.
    pub(super) fn begin_ask(&mut self, whole_unit: u32, disk: &[DiskVolume]) -> u64 {
        self.last_generation += 1;
        let generation = self.last_generation;
        self.generations.insert(whole_unit, generation);
        for volume in disk {
            self.pending.insert(volume.bsd_name.clone(), volume.clone());
        }
        generation
    }

    /// What a release did for `volume` in an ask of `generation`: a volume it stopped while indexing
    /// is recorded, and an earlier record of the same volume is carried forward to this epoch and
    /// generation whatever the release answered.
    pub(super) fn note_release(&mut self, volume: &DiskVolume, outcome: VolumeRelease, epoch: u64, generation: u64) {
        if let Some(record) = self.record_mut(volume) {
            // Carried forward: an earlier ask's record is owed back at this ask's epoch, whatever
            // this release found left to stop.
            record.volume = volume.clone();
            record.epoch = epoch;
            record.generation = generation;
        } else if stopped_an_index(outcome) {
            self.records.push(Record {
                volume: volume.clone(),
                epoch,
                generation,
            });
        }
    }

    /// A release's continuation answered after its ask did. A newer ask's record of the same volume
    /// wins over it.
    pub(super) fn note_late_release(
        &mut self,
        volume: &DiskVolume,
        outcome: VolumeRelease,
        epoch: u64,
        generation: u64,
    ) {
        if !stopped_an_index(outcome) {
            return;
        }
        match self.record_mut(volume) {
            Some(record) if record.generation > generation => {}
            Some(record) => {
                record.volume = volume.clone();
                record.epoch = epoch;
                record.generation = generation;
            }
            None => self.records.push(Record {
                volume: volume.clone(),
                epoch,
                generation,
            }),
        }
    }

    /// The record of `volume`, by BSD node and volume UUID.
    fn record_mut(&mut self, volume: &DiskVolume) -> Option<&mut Record> {
        self.records.iter_mut().find(|record| record.volume.same_volume(volume))
    }

    /// Whatever an ask's whole disk forgets: its generation, its records, and its pending flags.
    fn forget(&mut self, whole_unit: u32) {
        self.generations.remove(&whole_unit);
        self.records.retain(|record| record.volume.whole_unit != whole_unit);
        self.pending.retain(|_, volume| volume.whole_unit != whole_unit);
    }

    /// An idle: DiskArbitration's queue is quiet, so no unmount is pending any more. Answers every
    /// record as a resume candidate; records stay until a resume consumes them.
    pub(super) fn idle(&mut self) -> Vec<ResumeCandidate> {
        self.pending.clear();
        self.records
            .iter()
            .map(|record| ResumeCandidate {
                volume_id: record.volume.volume_id.clone(),
                epoch: record.epoch,
            })
            .collect()
    }

    /// Whether `candidate`'s record still stands: it's there with the candidate's epoch, and no newer
    /// ask came for its whole disk.
    pub(super) fn still_owns(&self, candidate: &ResumeCandidate) -> bool {
        self.record_of(candidate)
            .is_some_and(|record| self.generations.get(&record.volume.whole_unit) == Some(&record.generation))
    }

    /// `candidate`'s record, matched on the epoch it was offered with, so a record carried forward
    /// since is never mistaken for it.
    fn record_of(&self, candidate: &ResumeCandidate) -> Option<&Record> {
        self.records
            .iter()
            .find(|record| record.volume.volume_id == candidate.volume_id && record.epoch == candidate.epoch)
    }

    /// The volume behind `candidate`'s record, for its presence check.
    pub(super) fn volume_of(&self, candidate: &ResumeCandidate) -> Option<&DiskVolume> {
        self.record_of(candidate).map(|record| &record.volume)
    }

    /// `candidate`'s resume took its ticket or failed a check. A record carried forward since (a
    /// newer epoch) stays.
    pub(super) fn consume(&mut self, candidate: &ResumeCandidate) {
        self.records
            .retain(|record| record.volume.volume_id != candidate.volume_id || record.epoch != candidate.epoch);
    }

    /// The whole disk `whole_unit` appeared: whatever was remembered about an earlier disk with that
    /// unit is void.
    pub(super) fn appeared(&mut self, whole_unit: u32) {
        self.forget(whole_unit);
    }

    /// The whole disk `whole_unit` disappeared: its records go, and the volume ids whose unmount
    /// pending flag it clears are answered.
    pub(super) fn disappeared(&mut self, whole_unit: u32) -> Vec<String> {
        let cleared = self
            .pending
            .values()
            .filter(|volume| volume.whole_unit == whole_unit)
            .map(|volume| volume.volume_id.clone())
            .collect();
        self.forget(whole_unit);
        cleared
    }

    /// `bsd_name`'s volume path cleared, so its unmount happened: the volume id whose pending flag
    /// that clears.
    pub(super) fn volume_path_cleared(&mut self, bsd_name: &str) -> Option<String> {
        self.pending.remove(bsd_name).map(|volume| volume.volume_id)
    }
}

/// Whether a release stopped an index the volume's owner owes back.
fn stopped_an_index(outcome: VolumeRelease) -> bool {
    matches!(outcome, VolumeRelease::Released { was_indexing: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: u32 = 7;
    const OTHER_UNIT: u32 = 9;

    fn volume(volume_id: &str, bsd_name: &str, whole_unit: u32) -> DiskVolume {
        DiskVolume {
            volume_id: volume_id.to_string(),
            bsd_name: bsd_name.to_string(),
            volume_uuid: Some(format!("uuid-of-{volume_id}")),
            whole_unit,
            path: PathBuf::from(format!("/Volumes/{volume_id}")),
        }
    }

    fn candidate(volume_id: &str, epoch: u64) -> ResumeCandidate {
        ResumeCandidate {
            volume_id: volume_id.to_string(),
            epoch,
        }
    }

    const STOPPED: VolumeRelease = VolumeRelease::Released { was_indexing: true };

    #[test]
    fn a_volume_stopped_while_indexing_is_offered_at_idle_and_its_record_stands_until_consumed() {
        let a = volume("vol-a", "disk7s2", UNIT);
        let mut records = Records::default();
        let generation = records.begin_ask(UNIT, std::slice::from_ref(&a));
        records.note_release(&a, STOPPED, 11, generation);

        // The unmount was refused, so DA went quiet with the volume still mounted.
        let offered = records.idle();
        assert_eq!(offered, [candidate("vol-a", 11)]);
        assert!(records.still_owns(&offered[0]));
        assert_eq!(records.volume_of(&offered[0]), Some(&a));

        records.consume(&offered[0]);
        assert!(records.idle().is_empty(), "a consumed record is never offered again");
        assert!(!records.still_owns(&offered[0]));
    }

    #[test]
    fn a_volume_that_wasnt_indexing_is_never_recorded() {
        let a = volume("vol-a", "disk7s2", UNIT);
        let mut records = Records::default();
        let generation = records.begin_ask(UNIT, std::slice::from_ref(&a));
        records.note_release(&a, VolumeRelease::Released { was_indexing: false }, 11, generation);
        records.note_release(&a, VolumeRelease::NothingToStop, 12, generation);
        records.note_release(&a, VolumeRelease::StillReleasing, 13, generation);
        assert!(records.idle().is_empty());
    }

    #[test]
    fn a_newer_ask_on_the_whole_disk_voids_the_candidate_offered_before_it() {
        let a = volume("vol-a", "disk7s2", UNIT);
        let elsewhere = volume("vol-z", "disk9s1", OTHER_UNIT);
        let mut records = Records::default();
        let generation = records.begin_ask(UNIT, std::slice::from_ref(&a));
        records.note_release(&a, STOPPED, 11, generation);
        let elsewhere_generation = records.begin_ask(OTHER_UNIT, std::slice::from_ref(&elsewhere));
        records.note_release(&elsewhere, STOPPED, 12, elsewhere_generation);
        let offered = records.idle();

        records.begin_ask(UNIT, &[]);

        assert!(!records.still_owns(&candidate("vol-a", 11)));
        assert!(
            records.still_owns(&candidate("vol-z", 12)),
            "an ask on another disk leaves this one's records alone"
        );
        assert_eq!(offered.len(), 2);
    }

    #[test]
    fn the_second_ask_of_a_refused_unmount_disk_carries_the_first_volumes_record_forward() {
        let a = volume("vol-a", "disk7s2", UNIT);
        let b = volume("vol-b", "disk7s3", UNIT);
        let mut records = Records::default();

        // Ask 1 (B's volume) stops both.
        let first = records.begin_ask(UNIT, &[a.clone(), b.clone()]);
        records.note_release(&a, STOPPED, 11, first);
        records.note_release(&b, STOPPED, 12, first);
        // B unmounted; ask 2 (A's) finds only A on the disk, with nothing left to stop.
        let second = records.begin_ask(UNIT, std::slice::from_ref(&a));
        records.note_release(&a, VolumeRelease::NothingToStop, 21, second);
        // A's unmount is refused.
        let offered = records.idle();

        assert!(offered.contains(&candidate("vol-a", 21)));
        assert!(
            records.still_owns(&candidate("vol-a", 21)),
            "carried to the new epoch and generation"
        );
        assert!(!records.still_owns(&candidate("vol-a", 11)));
        assert!(
            !records.still_owns(&candidate("vol-b", 12)),
            "B wasn't on the disk for ask 2, so it isn't carried"
        );

        // The void candidate of A's first epoch never spends the carried record.
        records.consume(&candidate("vol-a", 11));
        assert!(records.still_owns(&candidate("vol-a", 21)));
    }

    #[test]
    fn a_continuations_late_release_records_unless_a_newer_ask_recorded_the_volume() {
        let a = volume("vol-a", "disk7s2", UNIT);
        let mut records = Records::default();
        let generation = records.begin_ask(UNIT, std::slice::from_ref(&a));
        records.note_release(&a, VolumeRelease::StillReleasing, 11, generation);
        records.note_late_release(&a, STOPPED, 11, generation);
        assert_eq!(records.idle(), [candidate("vol-a", 11)]);

        let newer = records.begin_ask(UNIT, std::slice::from_ref(&a));
        records.note_release(&a, VolumeRelease::StillReleasing, 21, newer);
        // The first ask's continuation lands after the newer ask carried the record forward.
        records.note_late_release(&a, STOPPED, 11, generation);
        assert_eq!(records.idle(), [candidate("vol-a", 21)]);
    }

    #[test]
    fn a_disk_appearing_or_disappearing_drops_its_records() {
        let a = volume("vol-a", "disk7s2", UNIT);
        let z = volume("vol-z", "disk9s1", OTHER_UNIT);
        let mut records = Records::default();
        let generation = records.begin_ask(UNIT, std::slice::from_ref(&a));
        records.note_release(&a, STOPPED, 11, generation);
        let other = records.begin_ask(OTHER_UNIT, std::slice::from_ref(&z));
        records.note_release(&z, STOPPED, 12, other);

        assert_eq!(
            records.disappeared(UNIT),
            ["vol-a"],
            "A's pending flag clears with its disk"
        );
        records.appeared(OTHER_UNIT);

        assert!(records.idle().is_empty());
        assert!(!records.still_owns(&candidate("vol-a", 11)));
        assert!(!records.still_owns(&candidate("vol-z", 12)));
    }

    #[test]
    fn an_idle_or_a_cleared_volume_path_ends_a_pending_unmount() {
        let a = volume("vol-a", "disk7s2", UNIT);
        let b = volume("vol-b", "disk7s3", UNIT);
        let mut records = Records::default();
        records.begin_ask(UNIT, &[a.clone(), b.clone()]);

        assert_eq!(records.volume_path_cleared("disk7s3").as_deref(), Some("vol-b"));
        assert_eq!(records.volume_path_cleared("disk7s3"), None, "only once");

        records.idle();
        assert_eq!(
            records.volume_path_cleared("disk7s2"),
            None,
            "the idle already ended A's"
        );
        assert!(records.disappeared(UNIT).is_empty());
    }
}

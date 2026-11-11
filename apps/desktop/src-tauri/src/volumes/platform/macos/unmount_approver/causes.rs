//! Why a volume's mount went away: pure, decided from DiskArbitration's own callbacks.
//!
//! An unmount Cmdr was asked to approve already let go of the drive, so nothing is owed. One nobody
//! asked about — a raw `/sbin/umount`, a pulled cable, an approval DA skipped — leaves an index
//! reading a drive that isn't there, and that's what has to be stopped.
//!
//! ❌ Never decided on timing: every fact below arrives as a callback on the session's one serial
//! queue, in DiskArbitration's delivery order. The one duration read is an ask's OWN runtime, which
//! the client measured itself.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::ask::DA_RESPONSE_WINDOW;

/// Why a volume left the mount table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cause {
    /// DiskArbitration asked about this volume's unmount, so the ask already let go of the drive.
    Asked,
    /// Its disk was ejected after Cmdr approved the eject.
    Ejected,
    /// It unmounted with no ask: a raw `/sbin/umount`, or an unmount DiskArbitration made while
    /// this session was skipped for approvals.
    Unasked,
    /// Its whole disk went away with no eject approval: a pulled cable, or a forced detach.
    Pulled,
    /// Its whole disk went away, and an ask on that disk had overrun DiskArbitration's response
    /// window, so a skipped eject approval is indistinguishable from a pull. Acts like
    /// [`Cause::Pulled`]; only the log wording differs.
    Unknown,
}

impl Cause {
    /// Whether the drive left WITHOUT Cmdr having let go of it first, so its index is still holding
    /// a filesystem that isn't there and has to be stopped now.
    pub(super) fn needs_a_stop(self) -> bool {
        matches!(self, Self::Unasked | Self::Pulled | Self::Unknown)
    }
}

/// A Cmdr volume that left the mount table, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Vanished {
    pub(super) volume_id: String,
    /// Its BSD node, for the log.
    pub(super) bsd_name: String,
    pub(super) cause: Cause,
}

/// What one whole disk's callbacks have said since it appeared.
// DEFAULT-OK: a disk nothing has been asked about yet has no asks, no eject approval, and no
// overrun. Its volumes arrive on their own callbacks.
#[derive(Debug, Default)]
struct DiskFacts {
    /// The Cmdr volumes known to be mounted on this disk, by BSD node. An entry leaves when that
    /// volume's path clears or its disk disappears, which are the only two ways a mount ends.
    volumes: HashMap<String, String>,
    /// The BSD nodes whose unmount this session was asked to approve.
    asked: HashSet<String>,
    /// An eject approval came for this disk.
    ejected: bool,
    /// An ask on this disk ran past [`DA_RESPONSE_WINDOW`], so DiskArbitration may have skipped this
    /// session for the approvals after it.
    an_ask_overran: bool,
}

impl DiskFacts {
    /// Whatever an `Appeared` voids: the decisions, never the volume map, whose entries are
    /// maintained by their own exits. ❗ DiskArbitration delivers a whole disk's `Appeared` and its
    /// volumes' in an order this must not depend on.
    fn forget_the_decisions(&mut self) {
        self.asked.clear();
        self.ejected = false;
        self.an_ask_overran = false;
    }
}

/// What the approver remembers about why each disk's volumes went away.
// DEFAULT-OK: no callback has come yet, so no disk is known.
#[derive(Debug, Default)]
pub(super) struct Causes {
    disks: HashMap<u32, DiskFacts>,
}

impl Causes {
    /// A Cmdr volume is mounted at `bsd_name` on the whole disk `whole_unit`. Fed by every callback
    /// that carries a volume path: a disk appearing, a description change that set one, and each
    /// ask's own group.
    ///
    /// ❗ This is the ONLY way a pull learns which volume to stop, since a disk that's gone can't be
    /// looked up in the mount table any more.
    pub(super) fn saw_volume(&mut self, bsd_name: &str, whole_unit: u32, volume_id: &str) {
        self.disks
            .entry(whole_unit)
            .or_default()
            .volumes
            .insert(bsd_name.to_string(), volume_id.to_string());
    }

    /// A whole disk appeared. DiskArbitration hands a freed BSD unit to the next disk at once, so
    /// whatever was decided about an earlier disk with that unit is void.
    pub(super) fn appeared(&mut self, whole_unit: u32) {
        self.disks.entry(whole_unit).or_default().forget_the_decisions();
    }

    /// An unmount ask for `bsd_name` answered, having taken `took`. An ask that ran past
    /// DiskArbitration's response window leaves its disk's later approvals in doubt.
    pub(super) fn asked(&mut self, bsd_name: &str, whole_unit: u32, took: Duration) {
        let facts = self.disks.entry(whole_unit).or_default();
        facts.asked.insert(bsd_name.to_string());
        facts.an_ask_overran |= took >= DA_RESPONSE_WINDOW;
    }

    /// An eject approval for the whole disk `whole_unit`, which the approver always answers at once.
    /// It's what tells a later `Disappeared` that the disk was ejected rather than pulled.
    pub(super) fn eject_approved(&mut self, whole_unit: u32) {
        self.disks.entry(whole_unit).or_default().ejected = true;
    }

    /// `bsd_name`'s volume path cleared, so its unmount happened. `None` when no Cmdr volume was
    /// known there.
    pub(super) fn volume_path_cleared(&mut self, bsd_name: &str) -> Option<Vanished> {
        let (_, facts) = self
            .disks
            .iter_mut()
            .find(|(_, facts)| facts.volumes.contains_key(bsd_name))?;
        let volume_id = facts.volumes.remove(bsd_name)?;
        let cause = if facts.asked.remove(bsd_name) {
            Cause::Asked
        } else {
            Cause::Unasked
        };
        Some(Vanished {
            volume_id,
            bsd_name: bsd_name.to_string(),
            cause,
        })
    }

    /// The whole disk `whole_unit` disappeared: every Cmdr volume still known on it went with it.
    /// Empty when each had already reported its own unmount, which is what an ordinary eject does.
    pub(super) fn disappeared(&mut self, whole_unit: u32) -> Vec<Vanished> {
        let Some(mut facts) = self.disks.remove(&whole_unit) else {
            return Vec::new();
        };
        let cause = match (facts.ejected, facts.an_ask_overran) {
            (true, _) => Cause::Ejected,
            (false, true) => Cause::Unknown,
            (false, false) => Cause::Pulled,
        };
        let mut vanished: Vec<Vanished> = facts
            .volumes
            .drain()
            .map(|(bsd_name, volume_id)| Vanished {
                volume_id,
                bsd_name,
                cause,
            })
            .collect();
        // A map's order is arbitrary; the log and every test read one order.
        vanished.sort_by(|left, right| left.bsd_name.cmp(&right.bsd_name));
        vanished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: u32 = 7;
    const OTHER_UNIT: u32 = 9;
    /// An ask that answered well inside DiskArbitration's window.
    const PROMPT: Duration = Duration::from_millis(200);

    /// A disk with one Cmdr volume on it, freshly appeared.
    fn one_volume_disk() -> Causes {
        let mut causes = Causes::default();
        causes.appeared(UNIT);
        causes.saw_volume("disk7s2", UNIT, "vol-a");
        causes
    }

    #[test]
    fn an_unmount_cmdr_was_asked_about_owes_nothing_further() {
        let mut causes = one_volume_disk();

        causes.asked("disk7s2", UNIT, PROMPT);
        let vanished = causes.volume_path_cleared("disk7s2").expect("a Cmdr volume left");

        assert_eq!(vanished.cause, Cause::Asked);
        assert_eq!(vanished.volume_id, "vol-a");
        assert!(
            !vanished.cause.needs_a_stop(),
            "the ask let go of the drive before the unmount, so nothing is owed"
        );
        assert!(
            causes.volume_path_cleared("disk7s2").is_none(),
            "the volume is gone, so it reports once"
        );
    }

    #[test]
    fn a_raw_umount_nobody_asked_about_has_to_be_stopped_now() {
        let mut causes = one_volume_disk();

        let vanished = causes.volume_path_cleared("disk7s2").expect("a Cmdr volume left");

        assert_eq!(
            vanished.cause,
            Cause::Unasked,
            "`/sbin/umount` bypasses DiskArbitration, so the index is still holding a drive that's gone"
        );
        assert!(vanished.cause.needs_a_stop());
    }

    #[test]
    fn a_volume_cmdr_knows_nothing_about_reports_nothing() {
        let mut causes = one_volume_disk();
        assert_eq!(causes.volume_path_cleared("disk7s3"), None);
        assert!(causes.disappeared(OTHER_UNIT).is_empty());
    }

    #[test]
    fn an_eject_reports_nothing_because_each_volume_already_said_it_unmounted() {
        let mut causes = one_volume_disk();

        // `diskutil eject`: the unmount is asked about, the path clears, then the eject approval
        // comes, then the disk goes.
        causes.asked("disk7s2", UNIT, PROMPT);
        let unmounted = causes.volume_path_cleared("disk7s2").expect("a Cmdr volume left");
        causes.eject_approved(UNIT);

        assert_eq!(unmounted.cause, Cause::Asked);
        assert!(
            causes.disappeared(UNIT).is_empty(),
            "nothing is left on the disk to report"
        );
    }

    #[test]
    fn a_disk_that_goes_away_with_its_volume_still_mounted_was_pulled() {
        let mut causes = one_volume_disk();

        let vanished = causes.disappeared(UNIT);

        assert_eq!(
            vanished,
            [Vanished {
                volume_id: "vol-a".to_string(),
                bsd_name: "disk7s2".to_string(),
                cause: Cause::Pulled,
            }]
        );
        assert!(vanished[0].cause.needs_a_stop());
    }

    #[test]
    fn an_eject_approval_before_the_disk_goes_means_it_was_ejected_not_pulled() {
        let mut causes = one_volume_disk();

        // No description change reached us for the volume, but the eject approval did.
        causes.eject_approved(UNIT);
        let vanished = causes.disappeared(UNIT);

        assert_eq!(vanished.len(), 1);
        assert_eq!(vanished[0].cause, Cause::Ejected);
        assert!(!vanished[0].cause.needs_a_stop());
    }

    #[test]
    fn an_ask_that_overran_the_response_window_makes_a_later_disappearance_unknown() {
        let mut causes = one_volume_disk();

        // DiskArbitration skips a session that timed out for every approval until it copies its
        // callback queue again, so the eject approval that would have said "ejected" may never have
        // reached us.
        causes.asked("disk7s2", UNIT, DA_RESPONSE_WINDOW);
        let vanished = causes.disappeared(UNIT);

        assert_eq!(vanished.len(), 1);
        assert_eq!(vanished[0].cause, Cause::Unknown);
        assert!(
            vanished[0].cause.needs_a_stop(),
            "an unknown cause is treated as a pull: stopping an index twice is cheap, leaving one on a gone drive isn't"
        );
    }

    #[test]
    fn every_volume_of_a_pulled_disk_is_reported_and_a_sibling_disk_is_left_alone() {
        let mut causes = one_volume_disk();
        causes.saw_volume("disk7s3", UNIT, "vol-b");
        causes.appeared(OTHER_UNIT);
        causes.saw_volume("disk9s1", OTHER_UNIT, "vol-z");

        let vanished = causes.disappeared(UNIT);

        assert_eq!(
            vanished.iter().map(|v| v.volume_id.as_str()).collect::<Vec<_>>(),
            ["vol-a", "vol-b"]
        );
        assert!(vanished.iter().all(|v| v.cause == Cause::Pulled));
        assert_eq!(
            causes.disappeared(OTHER_UNIT).len(),
            1,
            "the other disk's volume is still its own to report"
        );
    }

    #[test]
    fn a_disk_reappearing_on_the_same_unit_starts_from_nothing_decided() {
        let mut causes = one_volume_disk();
        causes.asked("disk7s2", UNIT, DA_RESPONSE_WINDOW);
        causes.eject_approved(UNIT);
        assert_eq!(causes.disappeared(UNIT)[0].cause, Cause::Ejected);

        // DiskArbitration hands the freed unit straight to the next disk.
        causes.appeared(UNIT);
        causes.saw_volume("disk7s2", UNIT, "vol-new");

        assert_eq!(
            causes.disappeared(UNIT)[0].cause,
            Cause::Pulled,
            "the earlier disk's eject approval and overrun say nothing about this one"
        );
    }

    #[test]
    fn a_volume_seen_before_its_disks_appeared_callback_survives_it() {
        // DiskArbitration's delivery order between a whole disk and its volumes isn't something
        // this may depend on: an `Appeared` voids the DECISIONS, never which volumes are mounted.
        let mut causes = Causes::default();
        causes.saw_volume("disk7s2", UNIT, "vol-a");
        causes.appeared(UNIT);

        assert_eq!(causes.disappeared(UNIT).len(), 1, "the volume is still known");
    }
}

//! An ask's pure decisions: how long it may spend letting go of a disk, and what it answers.
//!
//! DiskArbitration times every approval callback from when it QUEUED it, and a session runs its
//! callbacks one at a time, so an ask that starts while an earlier ask's window is still open may
//! have been queued during that window. Every ask in such a chain shares the first one's budget.

use std::time::{Duration, Instant};

use crate::file_system::volume::drive_release::VolumeRelease;

/// How long DiskArbitration waits for an approval answer, counted from when it queued the callback
/// (`diskarbitrationd/DAQueue.c:178-180`). Past it, DA takes the silence as approval.
pub(super) const DA_RESPONSE_WINDOW: Duration = Duration::from_secs(10);

/// How long one chain of asks may spend letting go. The 3 s left of [`DA_RESPONSE_WINDOW`] cover
/// the client's wake-up, the asks queued behind the stop, and scheduling under load; 7 s still
/// covers an index shutdown's 5 s live-loop drain.
pub(super) const APPROVAL_STOP_BUDGET: Duration = Duration::from_secs(7);

/// The asks DiskArbitration may have queued inside one response window, and the budget they share.
///
/// Time-based, never gap-based: a queue that stalls between two asks would otherwise start a new
/// chain and hand the queued ask a fresh budget its DA timer has already spent.
// DEFAULT-OK: no ask yet, so the next one starts a chain.
#[derive(Debug, Default)]
pub(super) struct Chain {
    start: Option<Instant>,
}

impl Chain {
    /// The instant an ask starting at `now` has to answer by: the chain's, when its window is still
    /// open, otherwise its own, which starts a new chain.
    pub(super) fn deadline_for(&mut self, now: Instant) -> Instant {
        let start = match self.start {
            Some(start) if now < start + DA_RESPONSE_WINDOW => start,
            _ => {
                self.start = Some(now);
                now
            }
        };
        start + APPROVAL_STOP_BUDGET
    }
}

/// What an ask tells DiskArbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answer {
    Approve,
    /// `kDAReturnBusy`. A non-force request is refused; under force DA ignores it.
    Dissent,
}

/// One volume's part in an ask's answer: what its release came to, and whether it had anything to
/// let go of when the ask marked the drive leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VolumeAnswer {
    pub(super) outcome: VolumeRelease,
    /// It had a local external index, or a start held its ticket, once the ask marked it
    /// unmount-pending. With neither, nothing can start on it until the unmount settles.
    pub(super) had_work: bool,
}

/// The answer once the disk's release came back: dissent while any volume that had work is still
/// letting go of the drive, or a write op is busy on it.
///
/// ❗ A volume with no work never dissents, even when the deadline cut its stop short: an ask that
/// starts with milliseconds of the chain's budget left can't even collect a no-op stop's answer, and
/// refusing over it made a non-indexed drive's eject fail whenever another drive's asks had spent
/// the window (2026-11-02, the disk-images lane with production Cmdr running).
pub(super) fn after_release(volumes: impl IntoIterator<Item = VolumeAnswer>, busy: bool) -> Answer {
    let still_releasing = volumes
        .into_iter()
        .any(|volume| volume.had_work && volume.outcome == VolumeRelease::StillReleasing);
    if still_releasing || busy {
        Answer::Dissent
    } else {
        Answer::Approve
    }
}

/// What an ask whose deadline already passed knows about its disk without waiting on anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Unwaited {
    /// A volume on the disk has a local external index.
    pub(super) indexing: bool,
    /// A start of a volume on the disk still holds its ticket.
    pub(super) ticket_in_flight: bool,
    /// A write op is busy on a volume of the disk.
    pub(super) busy: bool,
}

/// The answer of an ask with no time left: approve only a disk with nothing to let go of.
pub(super) fn without_waiting(disk: Unwaited) -> Answer {
    if disk.indexing || disk.ticket_in_flight || disk.busy {
        Answer::Dissent
    } else {
        Answer::Approve
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTHING: Unwaited = Unwaited {
        indexing: false,
        ticket_in_flight: false,
        busy: false,
    };

    fn worked(outcome: VolumeRelease) -> VolumeAnswer {
        VolumeAnswer {
            outcome,
            had_work: true,
        }
    }

    fn idle(outcome: VolumeRelease) -> VolumeAnswer {
        VolumeAnswer {
            outcome,
            had_work: false,
        }
    }

    #[test]
    fn a_disk_with_nothing_to_stop_or_everything_released_approves() {
        assert_eq!(
            after_release([idle(VolumeRelease::NothingToStop)], false),
            Answer::Approve
        );
        assert_eq!(
            after_release(
                [
                    worked(VolumeRelease::Released { was_indexing: true }),
                    idle(VolumeRelease::NothingToStop)
                ],
                false
            ),
            Answer::Approve
        );
    }

    #[test]
    fn a_volume_still_releasing_dissents() {
        assert_eq!(
            after_release(
                [
                    worked(VolumeRelease::Released { was_indexing: true }),
                    worked(VolumeRelease::StillReleasing)
                ],
                false
            ),
            Answer::Dissent
        );
    }

    #[test]
    fn a_volume_with_no_work_whose_stop_the_deadline_cut_short_still_approves() {
        assert_eq!(
            after_release([idle(VolumeRelease::StillReleasing)], false),
            Answer::Approve,
            "background work that was never there can't block an eject"
        );
    }

    #[test]
    fn a_busy_write_op_dissents_even_once_everything_let_go() {
        assert_eq!(
            after_release([worked(VolumeRelease::Released { was_indexing: true })], true),
            Answer::Dissent
        );
    }

    #[test]
    fn an_ask_with_no_time_left_approves_only_a_disk_with_nothing_to_let_go_of() {
        assert_eq!(without_waiting(NOTHING), Answer::Approve);
        assert_eq!(
            without_waiting(Unwaited {
                indexing: true,
                ..NOTHING
            }),
            Answer::Dissent
        );
        assert_eq!(
            without_waiting(Unwaited {
                ticket_in_flight: true,
                ..NOTHING
            }),
            Answer::Dissent,
            "a start in flight is work to stop"
        );
        assert_eq!(without_waiting(Unwaited { busy: true, ..NOTHING }), Answer::Dissent);
    }

    #[test]
    fn an_ask_queued_behind_a_five_second_ask_gets_the_two_seconds_left() {
        let mut chain = Chain::default();
        let t0 = Instant::now();
        assert_eq!(chain.deadline_for(t0), t0 + APPROVAL_STOP_BUDGET);
        assert_eq!(
            chain.deadline_for(t0 + Duration::from_secs(5)),
            t0 + APPROVAL_STOP_BUDGET
        );
    }

    #[test]
    fn an_ask_after_a_stall_inside_the_window_still_joins_the_chain() {
        let mut chain = Chain::default();
        let t0 = Instant::now();
        chain.deadline_for(t0);
        // The first ask answered at once, then the queue stalled well past any gap a gap-based
        // chain would allow before the next ask ran.
        let after_the_stall = t0 + Duration::from_millis(9_900);
        assert_eq!(
            chain.deadline_for(after_the_stall),
            t0 + APPROVAL_STOP_BUDGET,
            "its DA timer may have started inside the first window, so its deadline has passed"
        );
    }

    #[test]
    fn an_ask_after_the_window_starts_a_new_chain() {
        let mut chain = Chain::default();
        let t0 = Instant::now();
        chain.deadline_for(t0);
        let later = t0 + DA_RESPONSE_WINDOW;
        assert_eq!(chain.deadline_for(later), later + APPROVAL_STOP_BUDGET);
        assert_eq!(
            chain.deadline_for(later + Duration::from_secs(1)),
            later + APPROVAL_STOP_BUDGET,
            "and the asks after it join that one"
        );
    }
}

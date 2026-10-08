//! What a launch does with the index it finds on disk.
//!
//! One pure function over the facts a launch can read before it acts, so the
//! whole routing table sits in one place and can be read as a table. A wrong cell
//! costs either a wasted full rescan or a silently stale index, and both are
//! invisible until somebody reports something strange — which is why this is
//! separated from the side effects it selects between.
//!
//! ❌ It answers for a LOCAL (guarded-walker) volume only. A share or a phone is
//! routed by `resume_or_scan_network` before this is consulted.

/// What a launch does with the index it finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchRoute {
    /// Roll the existing index forward from the filesystem journal. Nothing walks.
    ReplayTheJournal,
    /// Walk the volume whole, through `start_scan` — which then decides
    /// reconcile-in-place (a completed index, sizes stay visible) or
    /// truncate-and-rebuild (anything else).
    ScanTheVolume,
    /// Hand the volume to the phase machine, adding to whatever is already there.
    CoverInPhases,
    /// Throw this index away first, then cover in phases. The rows are real but
    /// nothing can say which ground they cover, so resuming into them would serve
    /// them as current with nothing watching them.
    RebuildThenCoverInPhases,
}

/// The facts a launch reads off a volume's own database before it decides.
///
/// Every field is a question with one answer at launch; ❌ nothing here is a
/// runtime state that can change while the decision is being made.
#[derive(Debug, Clone, Copy)]
pub(super) struct IndexOnDisk {
    /// A scan (or a full coverage run) completed on this index at least once —
    /// `scan_completed_at` is set.
    pub scan_completed: bool,
    /// The index holds rows beyond the root sentinel.
    pub has_rows: bool,
    /// The index remembers which ground a walk covered on it (the persisted
    /// branch set). This is what tells a phased partial from a legacy interrupted
    /// scan: `start_scan` clears the set before a whole-volume walk, so an
    /// interrupted bulk build has none while a phased (or search-walked) volume
    /// does.
    pub has_covered_branches: bool,
    /// This volume has a journal and a stored event id worth replaying from.
    pub journal_replayable: bool,
    /// The journal has moved so far past the stored id that replaying it costs
    /// more than walking the volume.
    pub journal_gap_too_wide: bool,
    /// The product's phased-first-index switch. Off restores the bulk-build path.
    pub phased_first_index: bool,
    /// This index is marked as one a drive that went away may have taken rows
    /// from (`store::INDEX_NEEDS_REBUILD_KEY`), so nothing else it says about
    /// itself can be trusted.
    pub needs_rebuild: bool,
    /// The index's rows were written under an exclusion policy this build no
    /// longer applies (`scanner::index_predates_exclusion_policy`). Nothing in it
    /// counts as covered, and rows the current policy cuts may still sit in it.
    pub predates_exclusion_policy: bool,
}

/// Route one launch. The table, top to bottom:
///
/// - an index marked for a rebuild ⇒ throw it away and build it again, ahead of
///   every other row. It is the one fact that outranks a completion marker: an
///   index a vanishing drive deleted from looks finished, and replaying a journal
///   or reconciling in place over it would carry the holes forward. With the
///   phased switch off there is no machine to rebuild into, so it walks whole,
///   which is the same repair by the other path;
/// - a populated index built under an older exclusion policy ⇒ the same rebuild,
///   for the same reason: it can look finished while holding rows the current
///   policy cuts, and only a truncating walk re-stamps the policy, so replaying
///   or reconciling would leave its coverage distrusted for good;
/// - a replayable journal with too wide a gap ⇒ walk it whole (today's behavior,
///   and reachable only on a volume that already completed a scan);
/// - a replayable journal ⇒ replay;
/// - a completed index with no journal to replay ⇒ walk it whole, which
///   reconciles in place (the path a `LocalExternal` drive takes at every mount,
///   and the boot disk on Linux) — ❌ the phased answers must never swallow this,
///   or a finished external drive is treated as one nobody ever indexed;
/// - the phased-first-index switch off ⇒ walk it whole, whatever else is true.
///   With no phase machine to resume into, a phased partial takes today's
///   truncating rebuild: self-healing, and the behavior the person who flipped
///   the switch asked for;
/// - rows nothing can account for ⇒ rebuild, then cover in phases;
/// - anything else ⇒ cover in phases. That is every never-completed index: a
///   fresh install, a phased partial, and a volume a search walked.
pub(super) fn launch_route(index: &IndexOnDisk) -> LaunchRoute {
    if index.needs_rebuild || (index.has_rows && index.predates_exclusion_policy) {
        return if index.phased_first_index {
            LaunchRoute::RebuildThenCoverInPhases
        } else {
            LaunchRoute::ScanTheVolume
        };
    }
    if index.journal_replayable {
        return if index.journal_gap_too_wide {
            LaunchRoute::ScanTheVolume
        } else {
            LaunchRoute::ReplayTheJournal
        };
    }
    if index.scan_completed || !index.phased_first_index {
        return LaunchRoute::ScanTheVolume;
    }
    if index.has_rows && !index.has_covered_branches {
        return LaunchRoute::RebuildThenCoverInPhases;
    }
    LaunchRoute::CoverInPhases
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A volume nobody has ever indexed: no marker, no rows, no branches, no
    /// journal to replay.
    const FRESH: IndexOnDisk = IndexOnDisk {
        scan_completed: false,
        has_rows: false,
        has_covered_branches: false,
        journal_replayable: false,
        journal_gap_too_wide: false,
        phased_first_index: true,
        needs_rebuild: false,
        predates_exclusion_policy: false,
    };

    /// A volume the phase machine covered part of and never finished: rows, no
    /// completion marker, and a record of the ground those rows describe.
    const PHASED_PARTIAL: IndexOnDisk = IndexOnDisk {
        has_rows: true,
        has_covered_branches: true,
        ..FRESH
    };

    /// A first bulk scan somebody interrupted: rows, no completion marker, and
    /// nothing saying which ground they cover — `start_scan` cleared the branch
    /// set before it walked.
    const LEGACY_PARTIAL: IndexOnDisk = IndexOnDisk {
        has_rows: true,
        has_covered_branches: false,
        ..FRESH
    };

    /// A fully indexed volume with a journal behind it: the boot disk.
    const COMPLETED_JOURNALED: IndexOnDisk = IndexOnDisk {
        scan_completed: true,
        has_rows: true,
        has_covered_branches: true,
        journal_replayable: true,
        ..FRESH
    };

    /// A fully indexed volume with no journal: an external drive, or a Linux boot
    /// disk.
    const COMPLETED_UNJOURNALED: IndexOnDisk = IndexOnDisk {
        scan_completed: true,
        has_rows: true,
        has_covered_branches: true,
        ..FRESH
    };

    #[test]
    fn a_volume_nobody_has_indexed_is_covered_in_phases() {
        assert_eq!(launch_route(&FRESH), LaunchRoute::CoverInPhases);
    }

    /// The property the whole design exists for: what a previous session covered
    /// is added to, ❌ never thrown away and re-walked.
    #[test]
    fn a_stopped_phased_index_resumes_instead_of_rebuilding() {
        assert_eq!(launch_route(&PHASED_PARTIAL), LaunchRoute::CoverInPhases);
    }

    /// Its opposite number, and the reason the branch set is the discriminator.
    /// The rows are real, but nothing records which ground they cover — so
    /// nothing can watch that ground or mark it stale, and resuming into it would
    /// serve last session's rows as current. It goes.
    #[test]
    fn a_legacy_interrupted_partial_is_rebuilt() {
        assert_eq!(launch_route(&LEGACY_PARTIAL), LaunchRoute::RebuildThenCoverInPhases);
    }

    /// An install that already finished its first index loses nothing: it replays
    /// exactly as it does today and the phase machine never runs on it.
    #[test]
    fn an_upgraded_fully_scanned_volume_replays_and_never_phases() {
        assert_eq!(launch_route(&COMPLETED_JOURNALED), LaunchRoute::ReplayTheJournal);
    }

    /// The routing hole the phased answer would open if it took the whole
    /// fallthrough: `has_event_journal()` is `Local`-only, so a completed external
    /// drive lands here at every mount and must still reconcile in place.
    #[test]
    fn a_completed_external_volume_still_reconciles_at_mount() {
        assert_eq!(launch_route(&COMPLETED_UNJOURNALED), LaunchRoute::ScanTheVolume);
    }

    /// A gap too wide to replay keeps today's meaning — and it is reachable only
    /// on a volume that completed a scan, since replaying at all requires one. So
    /// no volume being covered in phases can ever take it.
    #[test]
    fn a_wide_journal_gap_during_phasing_does_not_truncate() {
        let wide_gap_on_a_completed_volume = IndexOnDisk {
            journal_gap_too_wide: true,
            ..COMPLETED_JOURNALED
        };
        assert_eq!(
            launch_route(&wide_gap_on_a_completed_volume),
            LaunchRoute::ScanTheVolume,
            "a completed volume with an unreplayable gap walks itself, exactly as it does today"
        );

        let wide_gap_while_phasing = IndexOnDisk {
            journal_gap_too_wide: true,
            ..PHASED_PARTIAL
        };
        assert_eq!(
            launch_route(&wide_gap_while_phasing),
            LaunchRoute::CoverInPhases,
            "❌ a partially covered volume must never be truncated by a journal gap: it has no journal \
             position to be behind, and the branch resume's epoch bump is what admits the staleness"
        );
    }

    /// The kill switch's own row. With the switch off there is no phase machine to
    /// resume into, so a phased partial takes today's truncating rebuild. That is
    /// the right answer — self-healing, and what the person who flipped it asked
    /// for — but it is an unstated cell unless it is written down.
    #[test]
    fn the_kill_switch_routes_a_phased_partial_to_the_legacy_rebuild() {
        let killed = IndexOnDisk {
            phased_first_index: false,
            ..PHASED_PARTIAL
        };
        assert_eq!(launch_route(&killed), LaunchRoute::ScanTheVolume);

        assert_eq!(
            launch_route(&IndexOnDisk {
                phased_first_index: false,
                ..FRESH
            }),
            LaunchRoute::ScanTheVolume,
            "and a fresh volume gets the bulk first scan back"
        );
        assert_eq!(
            launch_route(&IndexOnDisk {
                phased_first_index: false,
                ..COMPLETED_JOURNALED
            }),
            LaunchRoute::ReplayTheJournal,
            "❌ but it never costs a completed volume its replay: the switch restores the BUILD path, \
             not a rescan of everything already indexed"
        );
    }

    /// ❗ The row the rebuild marker exists for. A drive that vanished mid-delete
    /// leaves an index that looks perfectly finished — completion marker, rows,
    /// branch set, a replayable journal — and is missing whatever the deletes took.
    /// Every other row would trust it, so this one is read first.
    #[test]
    fn an_index_marked_for_a_rebuild_is_rebuilt_however_finished_it_looks() {
        let marked = IndexOnDisk {
            needs_rebuild: true,
            ..COMPLETED_JOURNALED
        };
        assert_eq!(
            launch_route(&marked),
            LaunchRoute::RebuildThenCoverInPhases,
            "a journal replay over an index with holes in it carries the holes forward"
        );
        assert_eq!(
            launch_route(&IndexOnDisk {
                needs_rebuild: true,
                ..COMPLETED_UNJOURNALED
            }),
            LaunchRoute::RebuildThenCoverInPhases,
            "and a reconcile in place would keep serving what it can't see is gone"
        );
    }

    /// With no phase machine to rebuild into, the marker still gets its rebuild:
    /// the volume walks whole, which is the same repair by the other path.
    #[test]
    fn a_marked_index_is_walked_whole_when_the_phases_are_switched_off() {
        assert_eq!(
            launch_route(&IndexOnDisk {
                needs_rebuild: true,
                phased_first_index: false,
                ..COMPLETED_JOURNALED
            }),
            LaunchRoute::ScanTheVolume
        );
    }

    /// ❗ An index built under an older exclusion policy is rebuilt, however
    /// finished it looks. A journal replay would keep every row the current policy
    /// cuts (a mount the boot index now stops at, say) and leave coverage
    /// distrusted for good, because only a truncating walk re-stamps the policy.
    #[test]
    fn an_index_built_under_an_older_exclusion_policy_is_rebuilt() {
        for (finished, what) in [
            (
                COMPLETED_JOURNALED,
                "a journal replay keeps the rows the new policy cuts",
            ),
            (COMPLETED_UNJOURNALED, "a reconcile in place never re-stamps the policy"),
        ] {
            assert_eq!(
                launch_route(&IndexOnDisk {
                    predates_exclusion_policy: true,
                    ..finished
                }),
                LaunchRoute::RebuildThenCoverInPhases,
                "{what}"
            );
        }
        assert_eq!(
            launch_route(&IndexOnDisk {
                predates_exclusion_policy: true,
                phased_first_index: false,
                ..COMPLETED_JOURNALED
            }),
            LaunchRoute::ScanTheVolume,
            "with the phases off it walks whole, and `start_scan` truncates an index that predates the policy"
        );
    }

    /// An empty index has nothing an older policy could have let in, so a stale
    /// or absent stamp on it changes nothing: the walk about to run stamps it.
    #[test]
    fn an_empty_index_with_an_old_stamp_routes_as_usual() {
        assert_eq!(
            launch_route(&IndexOnDisk {
                predates_exclusion_policy: true,
                ..FRESH
            }),
            LaunchRoute::CoverInPhases
        );
    }
}

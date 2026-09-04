//! What a rollback left alone, grouped by reason — the per-file half of a rollback
//! report.
//!
//! Apart from `rollback.rs` because it's a pure tally with one property worth pinning on
//! its own: the breakdown must be COMPLETE (every skip counted) while staying bounded for
//! a 1M-item op. It manages that by keeping one entry per [`SkipReason`] variant, so
//! counts are exact and never a sample — there's no cut to disclose (invariant 9).
//!
//! Both reversal families tally through this: the journal-driven Roll back and the
//! in-flight one a cancel runs (`file_system::write_operations::reversal`).

use std::path::Path;

use crate::operation_log::types::SkipReason;

/// One reason a rollback left items alone: the complete count for that reason plus one
/// example file, so a report can NAME a file when the reason applies to just one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkipBreakdown {
    pub reason: SkipReason,
    /// Every item skipped for this reason, not a sample.
    pub count: u64,
    /// The leaf name of the FIRST item this reason applied to, at the location the undo
    /// found it — the name the file carries right now, which is the one the user is
    /// looking at. A name, not a path: full paths live in the operation log.
    pub example_name: String,
}

/// Accumulates a run's skips into per-reason groups, in first-seen order (the engine's
/// own reversal order, so the result is deterministic for a given operation).
#[derive(Debug, Default)]
pub(crate) struct SkipTally {
    groups: Vec<SkipBreakdown>,
}

impl SkipTally {
    /// Count one skipped item. `path` is where the undo found the item; its leaf name
    /// becomes the group's example if this is the first item for `reason`.
    pub(crate) fn record(&mut self, reason: SkipReason, path: &Path) {
        // A linear scan over at most one entry per SkipReason variant.
        if let Some(group) = self.groups.iter_mut().find(|g| g.reason == reason) {
            group.count += 1;
            return;
        }
        self.groups.push(SkipBreakdown {
            reason,
            count: 1,
            example_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        });
    }

    pub(crate) fn into_breakdowns(self) -> Vec<SkipBreakdown> {
        self.groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Groups keep the complete count per reason and the first file's name, in the order
    /// the reasons were first seen.
    #[test]
    fn groups_count_every_skip_and_keep_the_first_name() {
        let mut tally = SkipTally::default();
        tally.record(SkipReason::Drift, Path::new("/a/first.pdf"));
        tally.record(SkipReason::RestoreTargetOccupied, Path::new("/a/taken.pdf"));
        tally.record(SkipReason::Drift, Path::new("/a/second.pdf"));

        let groups = tally.into_breakdowns();

        assert_eq!(groups.len(), 2, "one group per reason, not per item");
        assert_eq!(groups[0].reason, SkipReason::Drift);
        assert_eq!(groups[0].count, 2, "the count is complete, not a sample");
        assert_eq!(
            groups[0].example_name, "first.pdf",
            "the first file for the reason, by name"
        );
        assert_eq!(groups[1].reason, SkipReason::RestoreTargetOccupied);
        assert_eq!(groups[1].count, 1);
        assert_eq!(groups[1].example_name, "taken.pdf");
    }

    /// An empty run reports no groups at all — not a zero-count group that would render
    /// as "left 0 files alone".
    #[test]
    fn nothing_skipped_is_no_groups() {
        assert!(SkipTally::default().into_breakdowns().is_empty());
    }
}

//! The per-volume transfer gauge: is a user-initiated write operation touching this
//! volume right now?
//!
//! File transfers sit between user-interactive work and indexing in the priority
//! order (see the module docs in [`super`]): the user asked for them and is watching
//! a progress bar, so indexing on the same volume stands aside while one runs.
//!
//! Fed from the ONE write-operation lifecycle choke point
//! (`write_operations::state::register_operation_status` /
//! `unregister_operation_status`) — the same two functions that maintain the
//! eject-guard busy set, so every op kind that guards eject (copy, move, delete,
//! trash, drag-out promises) also raises this gauge, and the unregister fires from
//! the manager's panic-safe guard. A gauge (count), not a flag: overlapping
//! operations on one volume stay "active" until the LAST one finishes.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use crate::ignore_poison::RwLockIgnorePoison;

/// Active-operation count per volume id. A missing key means no transfer.
static GAUGE: LazyLock<RwLock<HashMap<String, usize>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Record that a write operation touching `volume_ids` started. Call exactly once
/// per operation, paired with [`note_transfer_finished`] over the SAME ids (the
/// write-op status cache stores them, so the pair can't drift).
pub fn note_transfer_started(volume_ids: &[String]) {
    if volume_ids.is_empty() {
        return;
    }
    let mut gauge = GAUGE.write_ignore_poison();
    for id in volume_ids {
        *gauge.entry(id.clone()).or_insert(0) += 1;
    }
}

/// Record that a write operation touching `volume_ids` finished (any exit path:
/// success, cancel, error, panic-guard cleanup). Saturating: an unmatched finish
/// (a bug) can't underflow into a permanently-busy volume.
pub fn note_transfer_finished(volume_ids: &[String]) {
    if volume_ids.is_empty() {
        return;
    }
    let mut gauge = GAUGE.write_ignore_poison();
    for id in volume_ids {
        if let Some(count) = gauge.get_mut(id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                gauge.remove(id);
            }
        }
    }
}

/// Whether a user-initiated write operation is touching `volume_id` right now, so
/// background indexing on it should stand aside. An uncontended read lock over a
/// map with one entry per busy volume (usually empty).
pub fn transfer_active(volume_id: &str) -> bool {
    GAUGE.read_ignore_poison().contains_key(volume_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Volume ids are unique per test: the gauge is process-global and tests run in
    /// parallel, so shared ids would cross-talk.
    #[test]
    fn a_started_transfer_marks_its_volumes_active_until_it_finishes() {
        let ids = vec!["test://transfers/src".to_string(), "test://transfers/dst".to_string()];
        assert!(!transfer_active("test://transfers/src"), "nothing started yet");
        note_transfer_started(&ids);
        assert!(transfer_active("test://transfers/src"), "source is busy");
        assert!(transfer_active("test://transfers/dst"), "destination is busy");
        note_transfer_finished(&ids);
        assert!(!transfer_active("test://transfers/src"));
        assert!(!transfer_active("test://transfers/dst"));
    }

    /// Overlapping operations: the volume stays active until the LAST one finishes.
    /// A boolean flag instead of a count would go quiet when the FIRST op ends.
    #[test]
    fn overlapping_transfers_keep_the_volume_active_until_the_last_ends() {
        let ids = vec!["test://transfers/overlap".to_string()];
        note_transfer_started(&ids);
        note_transfer_started(&ids);
        note_transfer_finished(&ids);
        assert!(
            transfer_active("test://transfers/overlap"),
            "one op finished, but another still runs"
        );
        note_transfer_finished(&ids);
        assert!(!transfer_active("test://transfers/overlap"));
    }

    /// An unmatched finish must not underflow or poison future starts.
    #[test]
    fn an_unmatched_finish_is_harmless() {
        let ids = vec!["test://transfers/unmatched".to_string()];
        note_transfer_finished(&ids); // no matching start: a no-op
        assert!(!transfer_active("test://transfers/unmatched"));
        note_transfer_started(&ids);
        assert!(
            transfer_active("test://transfers/unmatched"),
            "a later start still works"
        );
        note_transfer_finished(&ids);
        assert!(!transfer_active("test://transfers/unmatched"));
    }

    /// A transfer on one volume says nothing about another (the per-volume scope
    /// every consumer depends on).
    #[test]
    fn a_transfer_on_one_volume_leaves_others_clear() {
        note_transfer_started(&["test://transfers/busy".to_string()]);
        assert!(!transfer_active("test://transfers/other"));
        note_transfer_finished(&["test://transfers/busy".to_string()]);
    }
}

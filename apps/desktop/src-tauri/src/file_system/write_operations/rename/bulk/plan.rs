//! Which rows of a batch rename run, and in what order, decided before the first
//! hop: each destination spelled as the new name it is, every row whose
//! destination something outside the batch holds (exactly or as a Unicode
//! look-alike) dropped, and the rest ordered into direct steps, case-only steps,
//! and cycles. The hops themselves live in `bulk.rs`.

use std::collections::{HashMap, HashSet};

use super::super::super::look_alike::{ListedFolders, NewEntry, spelled_new_path};
use super::super::super::source_binding::normalized_path;
use super::super::same_local_file;
use super::{BulkRenameOutcome, BulkRenameRow};
use crate::file_system::volume::{Volume, VolumeError};

/// Every destination is a NEW name: spelled the way `volume` wants new names
/// before anything plans, runs, or journals, so all three see where each row
/// really lands.
pub(super) fn spelled_destinations(volume: &dyn Volume, rows: Vec<BulkRenameRow>) -> Vec<BulkRenameRow> {
    rows.into_iter()
        .map(|row| BulkRenameRow {
            destination: spelled_new_path(volume, &row.destination),
            ..row
        })
        .collect()
}

/// One collision-safe unit in a batch rename. Direct steps consume a free
/// destination. A cycle rotates through one temporary name, while a case-only
/// change uses one because the volume may treat both spellings as the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RenamePlanStep {
    Direct(usize),
    Cycle(Vec<usize>),
    CaseOnly(usize),
}

/// Orders active rows without filesystem access. The rename graph is
/// functional after preflight: every source and destination has at most one
/// owner. Removing rows whose destination is currently free peels all acyclic
/// chains in execution order; the remaining components are cycles.
pub(super) fn build_execution_plan(rows: &[BulkRenameRow], active: &[bool]) -> Vec<RenamePlanStep> {
    let mut remaining: HashSet<usize> = rows
        .iter()
        .enumerate()
        .filter(|(index, row)| active[*index] && row.source != row.destination)
        .map(|(index, _)| index)
        .collect();
    let mut plan = Vec::with_capacity(remaining.len());

    loop {
        let source_to_index: HashMap<String, usize> = remaining
            .iter()
            .map(|index| (normalized_path(&rows[*index].source), *index))
            .collect();
        let mut ready: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|index| {
                let source = normalized_path(&rows[*index].source);
                let destination = normalized_path(&rows[*index].destination);
                source == destination || !source_to_index.contains_key(&destination)
            })
            .collect();
        ready.sort_unstable();
        if ready.is_empty() {
            break;
        }
        for index in ready {
            if !remaining.remove(&index) {
                continue;
            }
            if normalized_path(&rows[index].source) == normalized_path(&rows[index].destination) {
                plan.push(RenamePlanStep::CaseOnly(index));
            } else {
                plan.push(RenamePlanStep::Direct(index));
            }
        }
    }

    while let Some(start) = remaining.iter().min().copied() {
        let source_to_index: HashMap<String, usize> = remaining
            .iter()
            .map(|index| (normalized_path(&rows[*index].source), *index))
            .collect();
        let mut cycle = vec![start];
        let mut current = start;
        loop {
            let destination = normalized_path(&rows[current].destination);
            let next = source_to_index[&destination];
            if next == start {
                break;
            }
            cycle.push(next);
            current = next;
        }
        for index in &cycle {
            remaining.remove(index);
        }
        plan.push(RenamePlanStep::Cycle(cycle));
    }
    plan
}

/// Drops every active row whose destination is already taken by something outside the
/// batch. Each pass frees the destinations of the rows it drops, so it repeats until a
/// pass changes nothing.
pub(super) fn settle_local_conflicts(rows: &[BulkRenameRow], active: &mut [bool]) {
    loop {
        let mut changed = false;
        for index in rows_with_unclaimed_destination(rows, active) {
            let row = &rows[index];
            if let Ok(destination_meta) = std::fs::symlink_metadata(&row.destination) {
                let destination_is_source = std::fs::symlink_metadata(&row.source)
                    .is_ok_and(|source_meta| same_local_file(&source_meta, &destination_meta));
                if !destination_is_source {
                    active[index] = false;
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

/// The remote twin of [`settle_local_conflicts`]. A byte-exact volume finds a
/// destination only by its own bytes, so a miss asks once more whether the folder
/// holds the name under another Unicode spelling (`look_alike.rs`): such a
/// look-alike is taken like an exact clash, two of them are too many to guess
/// between, and the row's own source is a respell. A look-alike some other active
/// row moves away never gets here: its destination counts as claimed. The folder
/// a pane shows is read from that pane's listing, the rest once each.
pub(super) async fn settle_remote_conflicts(
    rows: &[BulkRenameRow],
    active: &mut [bool],
    outcomes: &mut [BulkRenameOutcome],
    volume: &dyn Volume,
    volume_id: &str,
) {
    let mut folders = ListedFolders::with_pane_listings(volume, volume_id);
    loop {
        let mut changed = false;
        for index in rows_with_unclaimed_destination(rows, active) {
            let row = &rows[index];
            let clash = match volume.get_metadata(&row.destination).await {
                Ok(_) => Some(BulkRenameOutcome::Skipped),
                Err(VolumeError::NotFound(_)) => {
                    match folders.place_new_entry(&row.destination, Some(&row.source)).await {
                        Ok(NewEntry::Free(_)) => None,
                        Ok(NewEntry::Taken(_) | NewEntry::Ambiguous) => Some(BulkRenameOutcome::Skipped),
                        // Couldn't list the folder, so couldn't tell: ❌ never "free".
                        Err(_) => Some(BulkRenameOutcome::Failed),
                    }
                }
                // The rename itself answers for a destination it can't reach.
                Err(_) => None,
            };
            if let Some(outcome) = clash {
                active[index] = false;
                outcomes[index] = outcome;
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

/// The active rows that change their name and whose destination no active row
/// vacates, in row order. Whatever sits at such a destination is outside the batch.
fn rows_with_unclaimed_destination(rows: &[BulkRenameRow], active: &[bool]) -> Vec<usize> {
    let sources: HashSet<String> = rows
        .iter()
        .zip(active.iter())
        .filter(|(_, active)| **active)
        .map(|(row, _)| normalized_path(&row.source))
        .collect();
    rows.iter()
        .enumerate()
        .filter(|(index, row)| active[*index] && row.source != row.destination)
        .filter(|(_, row)| !sources.contains(&normalized_path(&row.destination)))
        .map(|(index, _)| index)
        .collect()
}

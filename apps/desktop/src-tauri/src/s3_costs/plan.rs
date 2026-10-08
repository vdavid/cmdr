//! Turns a dialog's operation and its scan into the billed work per provider.
//! Pure: the sides arrive as empty `Workload`s, one per S3 end, and the files as
//! the scan saw them.

use cmdr_s3::cost::Workload;
use serde::Deserialize;

use crate::file_system::volume::ScannedFile;
use crate::file_system::write_operations::{ConflictResolution, ScanCostFacts};

/// The operation a dialog is about to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CostedOperation {
    Copy,
    Move,
    Delete,
}

/// A file the dialog's conflict check found at the destination under a name
/// a copied file takes: the two files' sizes and dates, as the check's one
/// destination listing saw them (on S3, the destination's upload time).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownClash {
    pub source_size: u64,
    pub dest_size: u64,
    /// Unix seconds.
    pub source_modified: Option<u64>,
    /// Unix seconds.
    pub dest_modified: Option<u64>,
}

/// The clashes the conflict check found, and the policy the dialog will
/// answer them with.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClashPlan {
    pub resolution: ConflictResolution,
    pub clashes: Vec<KnownClash>,
}

/// An existing destination file the operation writes over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Overwrite {
    /// The bytes written over it.
    pub incoming_size: u64,
    pub replaced: ScannedFile,
}

/// The clashes `plan.resolution` overwrites, decided the way the transfer
/// does (`transfer/volume/conflict.rs`): strictly smaller, strictly older
/// with both dates known. Stop asks about each one as it comes, so none is
/// assumed; Skip and Rename overwrite nothing.
pub(super) fn overwritten(plan: &ClashPlan) -> Vec<Overwrite> {
    let overwrites = |clash: &KnownClash| match plan.resolution {
        ConflictResolution::Overwrite => true,
        ConflictResolution::OverwriteSmaller => clash.dest_size < clash.source_size,
        ConflictResolution::OverwriteOlder => matches!(
            (clash.source_modified, clash.dest_modified),
            (Some(source), Some(dest)) if dest < source
        ),
        ConflictResolution::Stop | ConflictResolution::Skip | ConflictResolution::Rename => false,
    };
    plan.clashes
        .iter()
        .filter(|clash| overwrites(clash))
        .map(|clash| Overwrite {
            incoming_size: clash.source_size,
            replaced: ScannedFile {
                size: clash.dest_size,
                modified_at: clash.dest_modified,
            },
        })
        .collect()
}

/// Each end of the operation: an empty workload when it's an S3 place, and
/// whether the two ends copy on the server (`S3Volume::copies_on_server_from`).
pub(super) struct Sides {
    pub source: Option<Workload>,
    pub destination: Option<Workload>,
    pub server_copy: bool,
}

/// The billed work per S3 provider the operation touches: the source's first,
/// then the destination's. A server-side copy bills one account, so it's one
/// workload. `overwrites` are the destination files the copy writes over.
///
/// ❗ Counted request for request the way the engine sends them, LISTs and
/// HEADs included (`backend_suites/s3_engine_integration_test.rs` pins the
/// totals against the requests a fixture saw): the scan stats each selected
/// item and lists each folder, the copy lists each source folder again as it
/// walks, the destination is readied and each selected name probed, every
/// folder is made, and a file inside a folder the operation made skips the
/// no-overwrite check (`WriteMode::CreateNewInFreshFolder`). The destination is
/// taken to exist and the selection not to clash with a folder there; source
/// folders are taken to carry markers, as Cmdr's own do.
pub(super) fn plan(
    operation: CostedOperation,
    sides: Sides,
    facts: &ScanCostFacts,
    overwrites: &[Overwrite],
) -> Vec<Workload> {
    let Sides {
        mut source,
        mut destination,
        server_copy,
    } = sides;
    let files = files_of(facts);
    let (selected_files, nested_files) = split_selected(&files, &facts.selected_file_sizes);
    let pages = listing_pages(facts);

    if operation == CostedOperation::Delete {
        if let Some(source) = source.as_mut() {
            stat_selection(source, facts);
            (0..pages).for_each(|_| source.list_folder());
            for file in &files {
                source.delete_object(file.size, file.modified_at);
            }
            (0..facts.dirs).for_each(|_| source.delete_folder());
        }
        return source.into_iter().collect();
    }

    // The source side: the scan's stats and listings, and the walk's own
    // listing of each source folder.
    if let Some(source) = source.as_mut() {
        stat_selection(source, facts);
        (0..2 * pages).for_each(|_| source.list_folder());
    }

    if server_copy && let Some(both) = source.as_mut() {
        open_destination(both, operation, true, facts);
        nested_files
            .iter()
            .for_each(|file| both.copy_on_server_fresh(file.size));
        selected_files.iter().for_each(|file| both.copy_on_server(file.size));
        // The copy replaces each one in a single request.
        for overwrite in overwrites {
            both.replace_object(overwrite.replaced.size, overwrite.replaced.modified_at);
        }
        destination = None;
    } else {
        if let Some(source) = source.as_mut() {
            files.iter().for_each(|file| source.download(file.size));
        }
        if let Some(destination) = destination.as_mut() {
            open_destination(destination, operation, false, facts);
            nested_files.iter().for_each(|file| destination.upload_fresh(file.size));
            selected_files.iter().for_each(|file| destination.upload(file.size));
            for overwrite in overwrites {
                destination.replace_object(overwrite.replaced.size, overwrite.replaced.modified_at);
                destination.upload_over(overwrite.incoming_size);
            }
        }
    }
    if operation == CostedOperation::Move
        && let Some(source) = source.as_mut()
    {
        // The source sweep: each folder level read and cleared in one batch,
        // each selected file deleted on its own batch with the others.
        (0..facts.dirs).for_each(|_| source.sweep_folder());
        nested_files
            .iter()
            .for_each(|file| source.swept_object(file.size, file.modified_at));
        selected_files
            .iter()
            .for_each(|file| source.delete_object(file.size, file.modified_at));
    }
    source.into_iter().chain(destination).collect()
}

/// The scan's stat of each selected item.
fn stat_selection(work: &mut Workload, facts: &ScanCostFacts) {
    (0..facts.selected_folders).for_each(|_| work.stat_selection(true));
    facts
        .selected_file_sizes
        .iter()
        .for_each(|_| work.stat_selection(false));
}

/// The destination readied, each selected name probed, and every folder made.
/// A move within one place checks its source and destination first.
fn open_destination(work: &mut Workload, operation: CostedOperation, same_account: bool, facts: &ScanCostFacts) {
    work.open_destination();
    if operation == CostedOperation::Move && same_account {
        work.check_move_within();
    }
    let selected = facts.selected_folders + facts.selected_file_sizes.len();
    (0..selected).for_each(|_| work.probe_name());
    (0..facts.dirs).for_each(|_| work.make_folder());
}

/// The selected files (each matched by size, in order) apart from the files
/// inside the selected folders.
fn split_selected(files: &[ScannedFile], selected_sizes: &[u64]) -> (Vec<ScannedFile>, Vec<ScannedFile>) {
    let mut nested = files.to_vec();
    let mut selected = Vec::with_capacity(selected_sizes.len());
    for size in selected_sizes {
        match nested.iter().position(|file| file.size == *size) {
            Some(at) => selected.push(nested.remove(at)),
            None => selected.push(ScannedFile {
                size: *size,
                modified_at: None,
            }),
        }
    }
    (selected, nested)
}

/// The listing pages a walk of the selected folders reads: one per folder,
/// and one more per thousand files (exact for a tree whose big folders hold
/// its files; close otherwise).
fn listing_pages(facts: &ScanCostFacts) -> usize {
    if facts.dirs == 0 {
        return 0;
    }
    facts.dirs + facts.files / 1_000
}

/// Every file the scan saw, or, when it kept no per-file list, its byte total
/// spread evenly over its file count (no dates, so no early-deletion charge).
fn files_of(facts: &ScanCostFacts) -> Vec<ScannedFile> {
    if let Some(files) = &facts.per_file {
        return files.clone();
    }
    let count = facts.files as u64;
    if count == 0 {
        return Vec::new();
    }
    let (each, rest) = (facts.bytes / count, facts.bytes % count);
    (0..count)
        .map(|index| ScannedFile {
            size: each + u64::from(index < rest),
            modified_at: None,
        })
        .collect()
}

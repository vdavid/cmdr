//! Structural validation of a staged fresh ZIP before it may be published:
//! the three byte counts agree, and the archive reads back as exactly the
//! planned tree.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use cmdr_archive::read::{QuarantineReason, SanitizedName, sanitize_entry_name};
use cmdr_archive::{ArchiveFormat, ArchiveIndex, ArchiveVolume};
use cmdr_fs::volume::host::VolumeHost;

use super::super::types::{ArchiveNameRefusal, WriteOperationError};
use super::edit_error::EditError;
use super::fresh_plan::FreshPlan;
use super::outage_wait::StepError;
use cmdr_fs::volume::{Volume, VolumeError};

/// What a staged ZIP must read back as: every planned entry at the path the
/// archive reader gives it, plus the ancestor directories the reader
/// synthesizes. Comparing raw entry counts misfires, because the reader treats
/// `\` as a separator: a file named `a\b.txt` reads back as `a/` + `b.txt`.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ExpectedIndex {
    /// `(path, is_dir)` for every node, root excluded.
    pub(super) nodes: BTreeSet<(String, bool)>,
    /// Planned names the reader would quarantine. Any is a validation failure:
    /// Cmdr never publishes an archive it can't show in full.
    pub(super) quarantined: usize,
}

impl ExpectedIndex {
    pub(super) fn of_plan(plan: &FreshPlan) -> Self {
        Self::of_names(
            plan.entries
                .iter()
                .map(|entry| (entry.name.as_str(), entry.is_directory)),
        )
    }

    pub(super) fn of_names<'a>(names: impl IntoIterator<Item = (&'a str, bool)>) -> Self {
        let mut expected = Self {
            nodes: BTreeSet::new(),
            quarantined: 0,
        };
        for (name, is_dir) in names {
            let SanitizedName::Accepted(path) = sanitize_entry_name(name) else {
                expected.quarantined += 1;
                continue;
            };
            let mut ancestor = path.as_str();
            while let Some((parent, _)) = ancestor.rsplit_once('/') {
                expected.nodes.insert((parent.to_string(), true));
                ancestor = parent;
            }
            expected.nodes.insert((path, is_dir));
        }
        expected
    }

    fn of_index(index: &ArchiveIndex) -> Self {
        let mut nodes = BTreeSet::new();
        let mut stack = vec![String::new()];
        while let Some(dir) = stack.pop() {
            for node in index.list(&dir).unwrap_or_default() {
                if node.is_dir {
                    stack.push(node.path.clone());
                }
                nodes.insert((node.path, node.is_dir));
            }
        }
        Self {
            nodes,
            quarantined: index.quarantined().len(),
        }
    }
}

/// Refuses a plan whose entry names the archive reader would quarantine or
/// merge, before the producer spawns: otherwise the whole compress (and any
/// upload) runs only for [`validate_stage`] to reject it, and the message could
/// name no file.
///
/// Two entries collide when they read back as one path (a file `a\b.txt` next to
/// a real `a/b.txt`), or when one is a FILE at a path another needs as a folder
/// (`a` next to `a\b.txt`). Two folders at one path merge harmlessly and pass.
/// ❗ [`validate_stage`]'s set comparison can't catch either: one entry shadows
/// the other and both read back as a single node.
pub(super) fn check_entry_names(plan: &FreshPlan) -> Result<(), WriteOperationError> {
    // Archive path → (the entry that claimed it, is it a folder).
    let mut claimed: HashMap<String, (&str, bool)> = HashMap::new();
    let mut accepted = Vec::with_capacity(plan.entries.len());
    for entry in &plan.entries {
        let raw = entry.name.trim_end_matches('/');
        let path = match sanitize_entry_name(&entry.name) {
            SanitizedName::Accepted(path) => path,
            SanitizedName::Quarantined(reason) => {
                return Err(WriteOperationError::ArchiveEntryNameRefused {
                    entry: raw.to_string(),
                    reason: match reason {
                        QuarantineReason::ParentTraversal => ArchiveNameRefusal::ParentTraversal,
                        QuarantineReason::TooDeep => ArchiveNameRefusal::TooDeep,
                        QuarantineReason::Empty => ArchiveNameRefusal::Empty,
                    },
                });
            }
        };
        match claimed.get(&path) {
            // Two folders at one path merge.
            Some(&(_, true)) if entry.is_directory => {}
            Some(&(other, _)) => return Err(collision(raw, other, &path)),
            None => {
                claimed.insert(path.clone(), (raw, entry.is_directory));
            }
        }
        accepted.push((raw, path));
    }
    for (raw, path) in &accepted {
        let mut ancestor = path.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if let Some(&(file, false)) = claimed.get(parent) {
                return Err(collision(raw, file, parent));
            }
            ancestor = parent;
        }
    }
    Ok(())
}

fn collision(entry: &str, other: &str, archive_path: &str) -> WriteOperationError {
    WriteOperationError::ArchiveEntryNamesCollide {
        entry: entry.to_string(),
        other: other.to_string(),
        archive_path: archive_path.to_string(),
    }
}

/// Checks a staged ZIP before it may be published. A read the volume refused
/// comes back as the volume's own error (`StepError::Fault`), so the caller
/// can wait out a dropped share and check again; a mismatch ends the compress.
pub(super) async fn validate_stage(
    volume: &Arc<dyn Volume>,
    path: &Path,
    writer_bytes: u64,
    producer_bytes: u64,
    expected: &ExpectedIndex,
) -> Result<(), StepError> {
    let meta = volume
        .get_metadata(path)
        .await
        .map_err(|error| unreadable(path, error))?;
    let stat_bytes = meta.size.unwrap_or(u64::MAX);
    if writer_bytes != producer_bytes || stat_bytes != producer_bytes {
        return Err(StepError::Ended(EditError::Op(WriteOperationError::WriteError {
            path: path.display().to_string(),
            message: format!(
                "ZIP byte counts disagree (producer {producer_bytes}, writer {writer_bytes}, destination {stat_bytes})"
            )
            .into(),
        })));
    }
    let reader_path = volume
        .supports_local_fs_access()
        .then(|| volume.local_path())
        .flatten()
        .map_or_else(
            || path.to_path_buf(),
            |root| cmdr_fs::volume::root_anchored(&root, path),
        );
    let archive = ArchiveVolume::new(
        Arc::clone(volume),
        reader_path,
        ArchiveFormat::Zip,
        VolumeHost::detached(),
    );
    let index = archive.index().await.map_err(|error| unreadable(path, error))?;
    if expected.quarantined > 0 || ExpectedIndex::of_index(&index) != *expected {
        return Err(StepError::Ended(EditError::Op(WriteOperationError::WriteError {
            path: path.display().to_string(),
            message: "the staged ZIP did not read back as the planned entries".into(),
        })));
    }
    Ok(())
}

/// A read of the stage the volume refused: a fault an outage wait judges,
/// reported as a read failure of the stage otherwise.
fn unreadable(path: &Path, error: VolumeError) -> StepError {
    let otherwise = EditError::Op(WriteOperationError::ReadError {
        path: path.display().to_string(),
        message: cmdr_fs::error_detail::ErrorDetail::of(error.clone()),
    });
    StepError::fault(error, otherwise)
}

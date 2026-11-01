//! Which volume each row is on, and the rows no volume holds.
//!
//! Every row echoes `volumeId`, the volume registry's answer for its path. A
//! `missing` row under a mount area with nothing mounted there is a dead mount
//! point, not a lost file, so it reads `noVolumeAtPath`, and the call carries the
//! resolver's `nearby` once (`tools/DETAILS.md` § The location contract).
//! Only `missing` rows are classified, so a call where every path exists reads no
//! volume facts at all.

use serde::Serialize;

use super::FileRow;
use crate::location::resolve::{LocationInput, LocationMiss, NearbyVolume, VolumeFacts, resolve};

/// One answered path, with the volume it's on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectRow {
    /// Absent when no volume holds the path (`noVolumeAtPath`, `unsupportedVolume`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_id: Option<String>,
    #[serde(flatten)]
    pub row: FileRow,
}

impl InspectRow {
    /// The requested path this row answers for.
    pub(crate) fn path(&self) -> &str {
        self.row.path()
    }
}

/// The runner's rows, placed: each with its volume, plus where to look instead
/// for the first row under a dead mount point.
#[derive(Debug)]
pub(crate) struct PlacedRows {
    pub rows: Vec<Option<InspectRow>>,
    pub nearby: Option<Vec<NearbyVolume>>,
}

/// Whether any row needs the volume facts to be placed.
pub(crate) fn needs_facts(rows: &[Option<FileRow>]) -> bool {
    rows.iter().flatten().any(|row| matches!(row, FileRow::Missing { .. }))
}

/// Place every row. `facts` is `None` when [`needs_facts`] said no; `owner` is the
/// volume registry's answer for a path.
pub(crate) fn place_rows(
    rows: Vec<Option<FileRow>>,
    facts: Option<&VolumeFacts>,
    owner: &dyn Fn(&str) -> Option<String>,
) -> PlacedRows {
    let mut nearby = None;
    let rows = rows
        .into_iter()
        .map(|row| {
            row.map(|row| match (&row, facts) {
                (FileRow::Missing { path }, Some(facts)) if facts.under_mount_area(path) => {
                    let input = LocationInput {
                        volume_id: None,
                        path: path.clone(),
                    };
                    match resolve(facts, &input) {
                        Err(LocationMiss::NoVolumeAtPath { nearby: found, .. }) => {
                            nearby.get_or_insert(found);
                            InspectRow {
                                volume_id: None,
                                row: FileRow::NoVolumeAtPath { path: path.clone() },
                            }
                        }
                        _ => owned(row, owner),
                    }
                }
                _ => owned(row, owner),
            })
        })
        .collect();
    PlacedRows { rows, nearby }
}

fn owned(row: FileRow, owner: &dyn Fn(&str) -> Option<String>) -> InspectRow {
    let volume_id = match &row {
        FileRow::UnsupportedVolume { .. } | FileRow::NoVolumeAtPath { .. } => None,
        _ => owner(row.path()),
    };
    InspectRow { volume_id, row }
}

/// The volume registry's owner of `path`: the deepest registered root over it,
/// else the boot disk for a plain path. In memory, no syscall.
pub(crate) fn registry_owner(path: &str) -> Option<String> {
    crate::file_system::volume::manager::get_volume_manager()
        .mount_id_for_path(path)
        .or_else(|| (!crate::tools::is_virtual_path(path)).then(|| cmdr_index::ROOT_VOLUME_ID.to_string()))
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;

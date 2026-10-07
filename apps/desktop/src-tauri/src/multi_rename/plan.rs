//! The preview: every row's new name and whether it can be renamed to it.
//!
//! Pure over the folder's entries, so the same answer drives the sheet's live
//! preview and the apply, which recomputes it rather than trusting names the
//! frontend sends back.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{Local, NaiveDateTime, TimeZone};
use cmdr_fs::name_fold::fold_name;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::validation::{ValidationError, validate_filename};

use super::mask::{Counter, Mask, MaskError, RowFacts};
use super::transform::{CaseChange, Replace, ReplaceError, Transform};

/// Everything the sheet sets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameSpec {
    pub name_mask: String,
    pub extension_mask: String,
    pub search: String,
    pub replace: String,
    pub case_sensitive: bool,
    pub first_only: bool,
    pub include_extension: bool,
    pub regex: bool,
    pub substitute: bool,
    pub case: CaseChange,
    pub remove_diacritics: bool,
    pub counter_start: i64,
    pub counter_step: i64,
    pub counter_digits: u32,
}

/// Why the spec itself can't run (the sheet shows it under the field).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SpecError {
    NameMask { error: MaskError },
    ExtensionMask { error: MaskError },
    BadRegex { detail: String },
}

/// One row of the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    /// The pane row (backend index, no `..`).
    pub row: usize,
    pub old_name: String,
    pub new_name: String,
    pub status: RowStatus,
}

/// Whether a row can be renamed to its new name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RowStatus {
    Ready,
    /// The new name is the old one: nothing to do.
    Unchanged,
    /// The new name isn't a name a file can have.
    InvalidName {
        reason: InvalidNameReason,
    },
    /// Another row of the batch gets the same name.
    Duplicate,
    /// Something that stays in the folder already has the name.
    TargetExists,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum InvalidNameReason {
    Empty,
    DisallowedCharacter {
        character: String,
    },
    TooLong,
    /// `.` and `..` name the folder itself and its parent.
    Reserved,
}

impl RowStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// The parsed spec, built once per preview.
pub(crate) struct Compiled {
    name_mask: Mask,
    extension_mask: Mask,
    transform: Transform,
    counter: Counter,
}

impl Compiled {
    pub(crate) fn new(spec: &MultiRenameSpec) -> Result<Self, SpecError> {
        let name_mask = Mask::parse(&spec.name_mask).map_err(|error| SpecError::NameMask { error })?;
        let extension_mask = Mask::parse(&spec.extension_mask).map_err(|error| SpecError::ExtensionMask { error })?;
        let replace = (!spec.search.is_empty()).then(|| Replace {
            search: spec.search.clone(),
            replace: spec.replace.clone(),
            case_sensitive: spec.case_sensitive,
            first_only: spec.first_only,
            include_extension: spec.include_extension,
            regex: spec.regex,
            substitute: spec.substitute,
        });
        let transform = Transform {
            replace,
            case: spec.case,
            remove_diacritics: spec.remove_diacritics,
        };
        // A broken regex is a spec problem, not a per-row one: ask once.
        transform
            .apply("", "")
            .map_err(|ReplaceError::BadRegex { detail }| SpecError::BadRegex { detail })?;
        Ok(Self {
            name_mask,
            extension_mask,
            transform,
            counter: Counter {
                start: spec.counter_start,
                step: spec.counter_step,
                digits: spec.counter_digits.clamp(1, super::mask::MAX_COUNTER_DIGITS),
            },
        })
    }

    /// The new full name for `entry` at `position` in the rename order.
    fn new_name(&self, entry: &FileEntry, dir: &Path, position: usize) -> String {
        // Composed, so a range never splits a letter from its accent in a name an
        // SMB share or HFS stores decomposed.
        let file_name: String = entry.name.nfc().collect();
        let parent = dir.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        let grandparent = dir
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let facts = RowFacts {
            file_name: &file_name,
            is_directory: entry.is_directory,
            parent: &parent,
            grandparent: &grandparent,
            modified: entry.modified_at.and_then(local_time),
            position,
        };
        let name = self.name_mask.render(&facts, &self.counter);
        let extension = self.extension_mask.render(&facts, &self.counter);
        // `Compiled::new` proved the regex; a later failure can't happen, so the
        // unchanged parts are the safe answer.
        let (name, extension) = self.transform.apply(&name, &extension).unwrap_or((name, extension));
        if extension.is_empty() {
            name
        } else {
            format!("{name}.{extension}")
        }
    }
}

fn local_time(unix_seconds: u64) -> Option<NaiveDateTime> {
    let seconds = i64::try_from(unix_seconds).ok()?;
    Local.timestamp_opt(seconds, 0).single().map(|t| t.naive_local())
}

/// The preview for `rows` (in rename order) of the folder at `dir`, whose every
/// entry, hidden ones included, is in `siblings`.
pub(crate) fn preview(
    compiled: &Compiled,
    dir: &Path,
    rows: &[(usize, &FileEntry)],
    siblings: &[FileEntry],
) -> Vec<PreviewRow> {
    let mut preview: Vec<PreviewRow> = rows
        .iter()
        .enumerate()
        .map(|(position, (row, entry))| {
            let new_name = compiled.new_name(entry, dir, position);
            let status = match invalid(&new_name) {
                Some(reason) => RowStatus::InvalidName { reason },
                // The same name in another Unicode form is the same name.
                None if new_name.nfc().eq(entry.name.nfc()) => RowStatus::Unchanged,
                None => RowStatus::Ready,
            };
            PreviewRow {
                row: *row,
                old_name: entry.name.clone(),
                new_name,
                status,
            }
        })
        .collect();

    // Until nothing changes: a row that turns out blocked STAYS, so the name it
    // holds is taken again, which can block a row renaming into it (a → b while
    // b → c is blocked by a c that stays).
    loop {
        let blocked = settle(&mut preview, siblings);
        if blocked == 0 {
            return preview;
        }
    }
}

/// One pass over the ready rows: flags duplicates and names held by an entry
/// that stays. Returns how many rows it blocked.
fn settle(preview: &mut [PreviewRow], siblings: &[FileEntry]) -> usize {
    // Names that stay in the folder: every sibling except the ones this batch
    // renames away. A row renaming to its own name in another case stays itself.
    let leaving: HashSet<String> = preview
        .iter()
        .filter(|p| p.status.is_ready())
        .map(|p| p.old_name.clone())
        .collect();
    let staying: HashSet<String> = siblings
        .iter()
        .filter(|s| !leaving.contains(&s.name))
        .map(|s| fold_name(&s.name).into_owned())
        .collect();

    let mut claims: HashMap<String, usize> = HashMap::new();
    for p in preview.iter().filter(|p| p.status.is_ready()) {
        *claims.entry(fold_name(&p.new_name).into_owned()).or_default() += 1;
    }
    let mut blocked = 0;
    for p in preview.iter_mut().filter(|p| p.status.is_ready()) {
        let key = fold_name(&p.new_name).into_owned();
        let own = fold_name(&p.old_name).into_owned();
        if claims.get(&key).copied().unwrap_or(0) > 1 {
            p.status = RowStatus::Duplicate;
            blocked += 1;
        } else if key != own && staying.contains(&key) {
            p.status = RowStatus::TargetExists;
            blocked += 1;
        }
    }
    blocked
}

fn invalid(name: &str) -> Option<InvalidNameReason> {
    if name == "." || name == ".." {
        return Some(InvalidNameReason::Reserved);
    }
    match validate_filename(name) {
        Ok(()) => None,
        Err(ValidationError::Empty) => Some(InvalidNameReason::Empty),
        Err(ValidationError::DisallowedCharacter { character }) => {
            Some(InvalidNameReason::DisallowedCharacter { character })
        }
        Err(ValidationError::NameTooLong { .. } | ValidationError::PathTooLong { .. }) => {
            Some(InvalidNameReason::TooLong)
        }
    }
}

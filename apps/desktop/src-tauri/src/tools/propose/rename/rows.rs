//! Which rows of a rename plan can't be staged, and the refusal that names them.
//!
//! Every per-row refusal is a typed [`RowProblem`] against the row's source path, collected over
//! the WHOLE plan before anything refuses, so the model reads every row to drop or fix in one
//! answer rather than learning them one retry at a time. A plan-wide problem (no rows, a volume
//! that isn't connected) stays a plain refusal: there's no row to name.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// Why one row can't be staged. The token is what the model branches on; [`Self::detail`] is
/// the sentence it reads beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum RowProblem {
    /// Not an absolute path, or one with an empty, `.`, or `..` segment: its folder can't be
    /// read off it with certainty.
    NotAPlainPath,
    /// The same source as an earlier row.
    RepeatedSource,
    /// The new name is a path, a dot entry, or not a valid filename.
    InvalidName,
    /// The new name is one an earlier row already takes (ignoring case and Unicode form).
    RepeatedName,
    /// A different `volumeId` from the plan's.
    OtherVolume,
    /// A different folder from the plan's: one plan renames inside one folder.
    OtherFolder,
    InsideArchive,
    InsideRepositoryHistory,
    InsideBackupSnapshots,
    /// The volume says it's a folder.
    Folder,
    /// The volume didn't answer for it in time, or refused to say.
    Unreachable,
    /// Nothing is there. Only ever a refusal when it's true of EVERY row: one missing row is a
    /// "(doesn't exist)" row in the review, but a plan where nothing exists names the wrong folder.
    Missing,
}

impl RowProblem {
    fn detail(self) -> &'static str {
        match self {
            Self::NotAPlainPath => "Each sourcePath must be the file's full path, with no empty, `.`, or `..` parts.",
            Self::RepeatedSource => "A source file can appear only once in a rename plan.",
            Self::InvalidName => "Each destinationName must be one valid filename, not a path.",
            Self::RepeatedName => "Destination names must be unique in the folder, ignoring case.",
            Self::OtherVolume => "Every row must use the same volumeId; these use a different one from the rest.",
            Self::OtherFolder => {
                "Every rename must stay in one folder; these are in a different one from the rest. Propose them as a separate plan."
            }
            Self::InsideArchive => "Rename plans can't include files inside an archive.",
            Self::InsideRepositoryHistory => "Rename plans can't include files inside a repository's history.",
            Self::InsideBackupSnapshots => "Rename plans can't include files inside a backup's snapshots.",
            Self::Folder => "Rename plans can contain files, not folders.",
            Self::Unreachable => {
                "The volume didn't answer for these in time, or wouldn't say. Try again in a moment, or tell the user the volume isn't responding."
            }
            Self::Missing => {
                "None of these files exist. Check the folder with list_dir or list_pane_files and use the names it shows."
            }
        }
    }
}

/// One row the plan can't stage, by its source path as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RowRejection {
    pub source_path: String,
    pub problem: RowProblem,
}

impl RowRejection {
    pub(super) fn new(source_path: &str, problem: RowProblem) -> Self {
        Self {
            source_path: source_path.to_string(),
            problem,
        }
    }
}

/// The checks a row passes on its own, with no I/O and no other row: a plain path, outside the
/// read-only views, and a valid new name. `None` when it passes them all.
pub(super) fn own_problem(source_path: &str, volume_id: &str, destination_name: &str) -> Option<RowProblem> {
    if !is_plain_path(source_path) {
        return Some(RowProblem::NotAPlainPath);
    }
    if super::plan::is_archive_inner_path(source_path) {
        return Some(RowProblem::InsideArchive);
    }
    // A repo's snapshots are read-only for the same reason: `.git/branches/…` has no file to
    // rename. Asked separately from the archive line above, which is a different question (a
    // portal path has no archive suffix to find).
    if crate::file_system::git::wiring::portal_serves(Path::new(source_path)) {
        return Some(RowProblem::InsideRepositoryHistory);
    }
    // And a restic backup's snapshots, for the same reason. Confirmed, ❌ not the bare name: a
    // real folder called `Browse snapshots` outside a repo holds real files a plan may rename.
    // From the hub's memo, no I/O.
    if crate::file_system::restic::wiring::memo_says_in_a_restic_view(volume_id, Path::new(source_path)) {
        return Some(RowProblem::InsideBackupSnapshots);
    }
    if super::plan::validate_destination_name(destination_name).is_err() {
        return Some(RowProblem::InvalidName);
    }
    None
}

/// Whether `path` names a file by a full path whose folder is plain to read: `/a/b.png`, or
/// `sftp://host/a/b.png`. Refused: a relative path, a trailing slash, and an empty, `.`, or `..`
/// segment, each of which would let the plan's one-folder check compare one folder spelled two
/// ways, or a name that isn't the file's.
pub(super) fn is_plain_path(path: &str) -> bool {
    let rest = match path.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() && !scheme.contains('/') => rest,
        _ => match path.strip_prefix('/') {
            Some(rest) => rest,
            None => return false,
        },
    };
    !rest.is_empty() && rest.split('/').all(|segment| !matches!(segment, "" | "." | ".."))
}

/// What the model reads: every rejected row, grouped by problem so 40 rows on one unreachable
/// share read as one sentence and one list.
pub(super) fn rows_rejected_content(rejections: &[RowRejection]) -> Value {
    let groups: Vec<Value> = by_problem(rejections)
        .into_iter()
        .map(|(problem, paths)| {
            serde_json::json!({ "problem": problem, "detail": problem.detail(), "sourcePaths": paths })
        })
        .collect();
    serde_json::json!({
        "readyForReview": false,
        "rowsRejected": groups,
        "guidance": "Nothing was staged. Drop or fix these rows and send the whole plan again. A file that doesn't exist needs no fixing: the review shows it as missing.",
    })
}

/// The same refusal as a log line: each problem and up to `max_named` of its paths. Paths only,
/// never the proposed names (`DETAILS.md` § A refused plan says so, in the log).
pub(super) fn rows_rejected_reason(rejections: &[RowRejection], max_named: usize) -> String {
    let clauses: Vec<String> = by_problem(rejections)
        .into_iter()
        .map(|(problem, paths)| {
            let named: Vec<&str> = paths.iter().take(max_named).copied().collect();
            let rest = match paths.len().saturating_sub(named.len()) {
                0 => String::new(),
                more => format!(" and {more} more"),
            };
            format!("{problem:?} ({}{rest})", named.join(", "))
        })
        .collect();
    let count = rejections.len();
    let noun = if count == 1 { "row" } else { "rows" };
    format!("{count} {noun} couldn't be staged: {}", clauses.join("; "))
}

fn by_problem(rejections: &[RowRejection]) -> BTreeMap<RowProblem, Vec<&str>> {
    let mut groups: BTreeMap<RowProblem, Vec<&str>> = BTreeMap::new();
    for rejection in rejections {
        groups
            .entry(rejection.problem)
            .or_default()
            .push(rejection.source_path.as_str());
    }
    groups
}

/// The value most of `values` share, the earliest on a tie: what the plan's one volume or one
/// folder is, so a single stray row is the one named, ❌ not the 39 that agree.
pub(super) fn most_common<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for value in values {
        match counts.iter_mut().find(|(seen, _)| *seen == value) {
            Some((_, count)) => *count += 1,
            None => counts.push((value, 1)),
        }
    }
    let mut best: Option<(&str, usize)> = None;
    for (value, count) in counts {
        if best.is_none_or(|(_, top)| count > top) {
            best = Some((value, count));
        }
    }
    best.map(|(value, _)| value)
}

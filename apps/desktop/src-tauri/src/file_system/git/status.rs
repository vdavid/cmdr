//! Per-entry git status for a working-tree directory.
//!
//! v1 shells out to `git status --porcelain=v2 --untracked-files=normal`
//! because gix's `Repository::status()` iterator missed staged additions in
//! our fixture-driven tests against a single-commit repo. The shell-out has
//! a 5 s timeout via the IPC layer and gives us identical semantics to the
//! command line. See `CLAUDE.md` § "gix vs shell-out outcome" for details.
//!
//! The `git` binary is part of the project's system requirements; no new
//! external dependency is taken on.

use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;

/// Single-character status code.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntryStatusCode {
    /// Modified in worktree relative to index.
    Modified,
    /// Added (in index but not in HEAD's tree).
    Added,
    /// Deleted in worktree relative to index, or in index relative to HEAD.
    Deleted,
    /// Renamed.
    Renamed,
    /// Tracked as a copy.
    Copied,
    /// Type changed (file ↔ symlink ↔ submodule).
    TypeChange,
    /// Untracked.
    Untracked,
    /// Ignored.
    Ignored,
    /// Conflicted (merge state).
    Conflicted,
}

impl EntryStatusCode {
    /// One-glyph render for the Full-mode status column.
    #[allow(dead_code, reason = "Public helper used by frontend tests and future Rust callers")]
    pub fn glyph(&self) -> &'static str {
        match self {
            EntryStatusCode::Modified => "M",
            EntryStatusCode::Added => "A",
            EntryStatusCode::Deleted => "D",
            EntryStatusCode::Renamed => "R",
            EntryStatusCode::Copied => "C",
            EntryStatusCode::TypeChange => "T",
            EntryStatusCode::Untracked => "?",
            EntryStatusCode::Ignored => "!",
            EntryStatusCode::Conflicted => "U",
        }
    }

    /// Long form, for `aria-label` / tooltip.
    #[allow(dead_code, reason = "Public helper used by frontend tests and future Rust callers")]
    pub fn label(&self) -> &'static str {
        match self {
            EntryStatusCode::Modified => "Modified",
            EntryStatusCode::Added => "Added",
            EntryStatusCode::Deleted => "Deleted",
            EntryStatusCode::Renamed => "Renamed",
            EntryStatusCode::Copied => "Copied",
            EntryStatusCode::TypeChange => "Type changed",
            EntryStatusCode::Untracked => "Untracked",
            EntryStatusCode::Ignored => "Ignored",
            EntryStatusCode::Conflicted => "Conflicted",
        }
    }
}

/// One status entry, surfaced to the frontend as `{ path, code }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryStatus {
    /// Path relative to the repo's working tree root, with `/` separators.
    pub relative_path: String,
    pub code: EntryStatusCode,
}

/// Lists the per-entry status for the worktree.
///
/// `dir_in_worktree` scopes the call to a subtree; it's passed to git as a
/// pathspec via `--`. An empty / repo-root scope returns the whole worktree.
pub fn list_status(repo: &RepoHandle, dir_in_worktree: &Path) -> Result<Vec<EntryStatus>, FriendlyGitError> {
    let local = repo.to_thread_local();
    let work_dir = local
        .workdir()
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::BareRepo, ""))?
        .to_path_buf();

    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(&work_dir)
        .args(["status", "--porcelain=v2", "--untracked-files=normal", "-z", "--"]);

    // If the caller scoped to a sub-path inside the worktree, pass it as a pathspec.
    if let Ok(rel) = dir_in_worktree.strip_prefix(&work_dir)
        && !rel.as_os_str().is_empty()
    {
        cmd.arg(rel);
    }

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    if !output.status.success() {
        // Index lock is the most common transient failure here.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let kind = if stderr.contains("index.lock") || stderr.contains("Unable to create") {
            FriendlyGitErrorKind::IndexLocked
        } else {
            FriendlyGitErrorKind::CorruptRepo
        };
        return Err(FriendlyGitError {
            kind,
            path: work_dir.display().to_string(),
            raw: Some(stderr.into_owned()),
        });
    }

    Ok(parse_porcelain_v2(&output.stdout))
}

/// Parses git's `--porcelain=v2 -z` NUL-separated output.
///
/// Each record is one of:
/// - `1 XY ...` ordinary changed entries (one path)
/// - `2 XY ...` renamed/copied entries (NUL-separated `<path>\0<orig>`)
/// - `u XY ...` unmerged entries (one path)
/// - `? <path>` untracked
/// - `! <path>` ignored
fn parse_porcelain_v2(stdout: &[u8]) -> Vec<EntryStatus> {
    let mut out = Vec::new();
    let mut iter = stdout.split(|b| *b == 0).peekable();
    while let Some(record) = iter.next() {
        if record.is_empty() {
            continue;
        }
        let rec = String::from_utf8_lossy(record).into_owned();
        let mut chars = rec.chars();
        match chars.next() {
            Some('?') => {
                if let Some(path) = rec.get(2..) {
                    out.push(EntryStatus {
                        relative_path: path.to_string(),
                        code: EntryStatusCode::Untracked,
                    });
                }
            }
            Some('!') => {
                if let Some(path) = rec.get(2..) {
                    out.push(EntryStatus {
                        relative_path: path.to_string(),
                        code: EntryStatusCode::Ignored,
                    });
                }
            }
            Some('1') => {
                // `1 XY <sub> <mode_h> <mode_i> <mode_w> <h_h> <h_i> <path>`
                let parts: Vec<&str> = rec.splitn(9, ' ').collect();
                if parts.len() >= 9 {
                    let xy = parts[1];
                    let path = parts[8];
                    if let Some(code) = code_from_xy(xy) {
                        out.push(EntryStatus {
                            relative_path: path.to_string(),
                            code,
                        });
                    }
                }
            }
            Some('2') => {
                // Rename/copy: header followed by NUL `<orig>` field.
                let parts: Vec<&str> = rec.splitn(10, ' ').collect();
                if parts.len() >= 10 {
                    let xy = parts[1];
                    let path = parts[9];
                    let _orig = iter.next();
                    let code = if xy.starts_with('C') || xy.contains('C') {
                        EntryStatusCode::Copied
                    } else {
                        EntryStatusCode::Renamed
                    };
                    out.push(EntryStatus {
                        relative_path: path.to_string(),
                        code,
                    });
                }
            }
            Some('u') => {
                // Unmerged: `u XY <sub> <m1> <m2> <m3> <mw> <h1> <h2> <h3> <path>`
                let parts: Vec<&str> = rec.splitn(11, ' ').collect();
                if parts.len() >= 11 {
                    out.push(EntryStatus {
                        relative_path: parts[10].to_string(),
                        code: EntryStatusCode::Conflicted,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// Maps the `XY` columns from porcelain v2 to a single status code.
/// `X` is the index-vs-HEAD column, `Y` is the worktree-vs-index column.
/// We pick the more "user-meaningful" one: a staged add with a worktree
/// modification still shows as Added in the column.
fn code_from_xy(xy: &str) -> Option<EntryStatusCode> {
    let mut chars = xy.chars();
    let x = chars.next()?;
    let y = chars.next()?;
    // Index changes (X) take precedence – they reflect what's about to land.
    let from = |c: char| -> Option<EntryStatusCode> {
        match c {
            'M' => Some(EntryStatusCode::Modified),
            'A' => Some(EntryStatusCode::Added),
            'D' => Some(EntryStatusCode::Deleted),
            'R' => Some(EntryStatusCode::Renamed),
            'C' => Some(EntryStatusCode::Copied),
            'T' => Some(EntryStatusCode::TypeChange),
            'U' => Some(EntryStatusCode::Conflicted),
            '.' | ' ' => None,
            _ => None,
        }
    };
    from(x).or_else(|| from(y))
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parses_untracked_and_modified_and_added() {
        // Hand-crafted -z output mimicking the CLI we shell out to.
        let stdout = b"1 .M N... 100644 100644 100644 deadbeef deadbeef README.md\0\
1 A. N... 000000 100644 100644 0000000000000000000000000000000000000000 f2ad README.added.txt\0\
? untracked.txt\0\
! ignored.log\0";
        let entries = parse_porcelain_v2(stdout);
        let codes: Vec<EntryStatusCode> = entries.iter().map(|e| e.code).collect();
        assert!(codes.contains(&EntryStatusCode::Modified));
        assert!(codes.contains(&EntryStatusCode::Added));
        assert!(codes.contains(&EntryStatusCode::Untracked));
        assert!(codes.contains(&EntryStatusCode::Ignored));
    }

    #[test]
    fn empty_input_parses_to_empty() {
        let entries = parse_porcelain_v2(b"");
        assert!(entries.is_empty());
    }
}

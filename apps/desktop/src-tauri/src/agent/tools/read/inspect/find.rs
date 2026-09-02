//! The `find` ask: which lines of a text file match a query, found with the viewer's own
//! search. One `Matcher` per call (built at param time, so a bad regex is `INVALID_PARAMS`
//! before any file is opened), `backend.search` streaming the file under the path's cancel
//! flag, then the matches grouped by line, capped at [`MAX_FIND_LINES`], each carried
//! line fetched by the byte offset the scan reported and cut to a snippet around its
//! first match. The UTF-16 column the scan hands back (the viewer's JS-facing unit) is
//! turned into a byte index by `range_read::clamp_utf16_offset_to_byte`, the one such
//! conversion in the tree.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::agent::tools::read::is_false;
use crate::file_viewer::range_read::clamp_utf16_offset_to_byte;
use crate::file_viewer::{FileViewerBackend, MAX_SEARCH_MATCHES, Matcher, SearchMatch, SeekTarget, ViewerError};
use crate::search::format_size;

/// The most matching lines one row carries. `totalMatches` stays honest above it: the
/// model learns a log mentions the tenant 4,000 times without reading 4,000 lines.
pub(crate) const MAX_FIND_LINES: usize = 50;
/// A matched line is cut to this many chars around its first match: enough context to
/// quote, never a whole minified bundle because one token in it matched.
pub(crate) const FIND_SNIPPET_CHARS: usize = 300;

/// One matching line, as the model reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindLine {
    /// 1-based, as the window's `startLine`, so "read around line 812" is `startLine: 812`.
    pub line: usize,
    /// Matches on this line.
    pub matches: usize,
    /// The line, cut to [`FIND_SNIPPET_CHARS`] around the first match; `…` marks a cut end.
    pub text: String,
}

/// What `find` found in one text file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindHits {
    /// Every match the scan saw, on carried lines or not.
    pub total_matches: usize,
    /// `true` when the scan stopped at the viewer's own cap (`MAX_SEARCH_MATCHES`, 10,000):
    /// `totalMatches` is then a floor and the tail of the file was not searched.
    #[serde(skip_serializing_if = "is_false")]
    pub matches_capped: bool,
    /// The first [`MAX_FIND_LINES`] matching lines, in file order.
    pub lines: Vec<FindLine>,
    /// How many lines `lines` holds.
    pub returned_lines: usize,
    /// `true` when matching lines exist past the carried ones.
    pub truncated: bool,
    /// `true` when the path's deadline stopped the scan before the end of the file:
    /// `totalMatches` covers only `bytesScanned` of `totalBytes`.
    #[serde(skip_serializing_if = "is_false")]
    pub scan_incomplete: bool,
    /// Where an incomplete scan stopped, by the backend's own progress count. Absent
    /// when the scan finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_scanned: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_scanned_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes_human: Option<String>,
}

/// The matches on one line, as the scan reported them: it streams the file in order, so
/// a line's matches arrive together and `first_column` is the earliest.
struct LineGroup {
    /// 0-based, as `SearchMatch::line`.
    line: usize,
    /// Byte offset of the line's start: exact on every backend, unlike a line number on
    /// the ByteSeek one, so it is how the line's text is fetched.
    byte_offset: u64,
    /// UTF-16 column of the first match.
    first_column: usize,
    matches: usize,
}

fn group_by_line(matches: &[SearchMatch]) -> Vec<LineGroup> {
    let mut groups: Vec<LineGroup> = Vec::new();
    for m in matches {
        match groups.last_mut() {
            Some(group) if group.line == m.line => group.matches += 1,
            _ => groups.push(LineGroup {
                line: m.line,
                byte_offset: m.byte_offset,
                first_column: m.column,
                matches: 1,
            }),
        }
    }
    groups
}

/// Search `backend` with `matcher` under `cancel` and shape the hits. Blocking: streams
/// the whole file (or as much as the flag allows), then reads one line per carried hit.
pub(crate) fn find_hits(
    backend: &dyn FileViewerBackend,
    matcher: &Matcher,
    cancel: &AtomicBool,
) -> Result<FindHits, ViewerError> {
    let found: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress = Mutex::new(0u64);
    let scanned = backend.search(matcher, cancel, &found, &progress)?;
    let found = found.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);

    let total_matches = found.len();
    let matches_capped = total_matches >= MAX_SEARCH_MATCHES;
    let total_bytes = backend.total_bytes();
    // The cap is its own reason to stop; only a flag-stopped scan that didn't reach the
    // end is incomplete. (`scanned` is the backend's progress count: raw bytes for the
    // streaming backends, decoded lengths for the in-memory one, which never goes
    // through the deadline anyway.)
    let scan_incomplete = !matches_capped && cancel.load(Ordering::Relaxed) && scanned < total_bytes;

    let groups = group_by_line(&found);
    let mut lines = Vec::with_capacity(groups.len().min(MAX_FIND_LINES));
    for group in groups.iter().take(MAX_FIND_LINES) {
        let chunk = backend.get_lines(&SeekTarget::ByteOffset(group.byte_offset), 1)?;
        let raw = chunk.lines.first().map(String::as_str).unwrap_or_default();
        // The backends keep `\r` on CRLF files; the model gains nothing from it.
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        lines.push(FindLine {
            line: group.line + 1,
            matches: group.matches,
            text: snippet_around(line, group.first_column, FIND_SNIPPET_CHARS),
        });
    }

    let returned_lines = lines.len();
    Ok(FindHits {
        total_matches,
        matches_capped,
        truncated: returned_lines < groups.len(),
        lines,
        returned_lines,
        scan_incomplete,
        bytes_scanned: scan_incomplete.then_some(scanned),
        bytes_scanned_human: scan_incomplete.then(|| format_size(scanned)),
        total_bytes: scan_incomplete.then_some(total_bytes),
        total_bytes_human: scan_incomplete.then(|| format_size(total_bytes)),
    })
}

/// Cut `line` to `max_chars` chars around the match starting at UTF-16 column
/// `match_column_utf16`, with `…` at each cut end. Pure.
///
/// A line that fits is returned whole. Otherwise a third of the budget goes before the
/// match and the rest to the match and what follows it (the evidence usually comes
/// after the term), and the cut slides back at the end of the line so the snippet stays
/// full. The column is converted through the UTF-16 clamp, never read as a char index:
/// on a line of emoji those differ by a factor of two.
pub(crate) fn snippet_around(line: &str, match_column_utf16: usize, max_chars: usize) -> String {
    let total_chars = line.chars().count();
    if total_chars <= max_chars {
        return line.to_string();
    }
    let match_byte = clamp_utf16_offset_to_byte(line, u32::try_from(match_column_utf16).unwrap_or(u32::MAX));
    let match_char = line[..match_byte].chars().count();
    let start = match_char.saturating_sub(max_chars / 3).min(total_chars - max_chars);
    let end = start + max_chars;

    let mut out = String::with_capacity(max_chars * 4 + 6);
    if start > 0 {
        out.push('…');
    }
    out.extend(line.chars().skip(start).take(max_chars));
    if end < total_chars {
        out.push('…');
    }
    out
}

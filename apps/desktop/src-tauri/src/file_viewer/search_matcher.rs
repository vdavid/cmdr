//! Search matcher that drives both literal and regex search across all backends.
//!
//! The viewer streams the file line by line and feeds each line to one `Matcher`.
//! A single `Matcher` instance is built once per `search_start` and shared across
//! lines, so the regex DFA / NFA is compiled exactly once per search.
//!
//! ## Why reject cross-line patterns
//!
//! Our search engine streams line by line: a regex like `(?s).` or one containing a
//! literal `\n` would silently never match because no line ever contains a newline
//! byte. Failing fast at build time is better UX than reporting "no matches" for a
//! query that can never match.
//!
//! `(?m)` is accepted: it only changes `^` / `$` semantics within the current line
//! slice; it does not cross newlines, so streaming is safe.
//!
//! ## Why bound DFA / NFA size
//!
//! The watchdog (`session.rs`) is the hard backstop for >1 s search cancellation,
//! and it assumes the per-call cost of `regex::Regex::find_iter` stays bounded by
//! `O(haystack * pattern)` (the `regex` crate's linear-time guarantee). The
//! guarantee holds as long as the lazy DFA stays under `dfa_size_limit`. We cap
//! both `size_limit` and `dfa_size_limit` at 8 MB so a pathological pattern is
//! rejected at build time instead of blowing up the watchdog budget at scan time.
//!
//! ## Huge-line chunking
//!
//! For lines longer than `HUGE_LINE_THRESHOLD` (1 MB), `find_matches` slices the
//! line into 1 MB windows with a 256-byte overlap so a needle straddling a window
//! boundary is still found. Matches starting in the overlap region "belong to" the
//! next chunk; otherwise the same hit would be reported twice.

use std::ops::ControlFlow;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use regex::{Regex, RegexBuilder};

use crate::ignore_poison::IgnorePoison;

use super::{MAX_SEARCH_MATCHES, SearchMatch};

/// Maximum line length before `find_matches` switches to chunked scanning.
const HUGE_LINE_THRESHOLD: usize = 1 << 20; // 1 MB

/// Overlap between adjacent chunks when scanning a huge line. Must exceed the
/// longest plausible match for the chunked scan to be correct; 256 bytes is
/// comfortable for any user-typed needle.
const HUGE_LINE_OVERLAP: usize = 256;

/// Per-pattern memory caps. Pairs with the watchdog's 1 s budget: the `regex` crate's
/// linear-time guarantee holds while the lazy DFA stays under `dfa_size_limit`.
const REGEX_SIZE_LIMIT: usize = 8 << 20; // 8 MB
const REGEX_DFA_SIZE_LIMIT: usize = 8 << 20; // 8 MB

/// Mode flags for building a `Matcher`. Crosses the IPC boundary via serde +
/// specta with camelCase field names (`useRegex` and `caseSensitive`).
#[derive(Debug, Clone, Copy, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchMode {
    pub use_regex: bool,
    pub case_sensitive: bool,
}

/// Why `Matcher::build` rejected a query.
#[derive(Debug, Clone)]
pub enum MatcherBuildError {
    /// The regex failed to compile or exceeded the size limits.
    InvalidRegex(String),
    /// The pattern would need to cross a newline boundary to match. Our search
    /// engine streams line by line, so we reject these patterns explicitly rather
    /// than letting the user wonder why nothing matches.
    MultilineNotSupported,
}

impl std::fmt::Display for MatcherBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRegex(msg) => write!(f, "Invalid regex: {}", msg),
            Self::MultilineNotSupported => write!(
                f,
                "Multiline patterns aren't supported. The viewer searches line by line."
            ),
        }
    }
}

impl std::error::Error for MatcherBuildError {}

/// One built matcher, shared across every line of a single search.
#[derive(Debug)]
pub enum Matcher {
    /// Plain substring search via `str::find`. Faster than regex for the common
    /// "find this literal" case and avoids the regex builder's overhead.
    Literal {
        needle: String,
        /// Pre-lowercased haystack form of the needle, only used when
        /// `case_insensitive` is true.
        needle_lower: String,
        case_insensitive: bool,
    },
    /// Compiled regex. The `regex` crate handles case insensitivity via the
    /// `(?i)` inline flag added at build time when requested.
    Regex(Regex),
}

impl Matcher {
    /// Build a matcher from a query string. Returns `Err` on invalid regex or
    /// patterns we know can never match in our streaming model.
    pub fn build(query: &str, mode: SearchMode) -> Result<Self, MatcherBuildError> {
        if mode.use_regex {
            // Reject patterns that need to cross newlines: `(?s)` makes `.` match `\n`,
            // and any literal newline byte in the pattern can never match a single line.
            if pattern_crosses_newline(query) {
                return Err(MatcherBuildError::MultilineNotSupported);
            }

            let mut builder = RegexBuilder::new(query);
            builder
                .case_insensitive(!mode.case_sensitive)
                .size_limit(REGEX_SIZE_LIMIT)
                .dfa_size_limit(REGEX_DFA_SIZE_LIMIT);
            let regex = builder
                .build()
                .map_err(|e| MatcherBuildError::InvalidRegex(e.to_string()))?;
            Ok(Matcher::Regex(regex))
        } else {
            let needle_lower = if !mode.case_sensitive {
                query.to_lowercase()
            } else {
                String::new()
            };
            Ok(Matcher::Literal {
                needle: query.to_string(),
                needle_lower,
                case_insensitive: !mode.case_sensitive,
            })
        }
    }

    /// Iterate matches in `line`, invoking `callback(start, end)` for each one in
    /// byte-offset order. The callback returns `ControlFlow::Break(())` to stop
    /// iteration early; this is how the search loop honours per-match cancellation
    /// without scanning the whole line first.
    ///
    /// For lines longer than `HUGE_LINE_THRESHOLD`, the scan is chunked with an
    /// overlap of `HUGE_LINE_OVERLAP` so boundary-straddling matches are still
    /// found exactly once.
    pub fn find_matches<F>(&self, line: &str, mut callback: F)
    where
        F: FnMut(usize, usize) -> ControlFlow<()>,
    {
        // Empty literal needle and empty regex matches: skip iterating, never report.
        if let Matcher::Literal { needle, .. } = self
            && needle.is_empty()
        {
            return;
        }

        if line.len() <= HUGE_LINE_THRESHOLD {
            // Top-level scan; ignore the ControlFlow result since there's nothing further
            // to iterate after the only slice.
            let _ = self.find_in_slice(line, 0, &mut callback);
            return;
        }
        self.find_chunked(line, &mut callback);
    }

    /// Scan a single slice (no chunking). `base_offset` is added to each reported
    /// position so chunked callers see offsets relative to the original line.
    fn find_in_slice<F>(&self, slice: &str, base_offset: usize, callback: &mut F) -> ControlFlow<()>
    where
        F: FnMut(usize, usize) -> ControlFlow<()>,
    {
        match self {
            Matcher::Literal {
                needle,
                needle_lower,
                case_insensitive,
            } => {
                if *case_insensitive {
                    let hay_lower = slice.to_lowercase();
                    // `to_lowercase()` can change byte length (Turkish dotless i, German ß).
                    // When that happens we fall back to a char-by-char scan to keep offsets
                    // in the original slice's byte space; this keeps the search engine
                    // correct without paying for the slow path on ASCII-only input.
                    if hay_lower.len() == slice.len() {
                        let mut start = 0;
                        while let Some(rel) = hay_lower[start..].find(needle_lower.as_str()) {
                            let abs = start + rel;
                            let end = abs + needle_lower.len();
                            if let ControlFlow::Break(()) = callback(base_offset + abs, base_offset + end) {
                                return ControlFlow::Break(());
                            }
                            start = end;
                        }
                    } else {
                        // Slow path: byte-for-byte unequal lowercasing. We compare
                        // the lowercased needle against each suffix's lowercased form.
                        let needle_str = needle_lower.as_str();
                        let mut i = 0;
                        while i < slice.len() {
                            let suffix = &slice[i..];
                            let suffix_lower = suffix.to_lowercase();
                            if suffix_lower.starts_with(needle_str) {
                                // Walk chars to find the byte-length in the original slice
                                // that corresponds to `needle_str.chars().count()` chars.
                                let needle_chars = needle_str.chars().count();
                                let consumed: usize = suffix.chars().take(needle_chars).map(|c| c.len_utf8()).sum();
                                if let ControlFlow::Break(()) = callback(base_offset + i, base_offset + i + consumed) {
                                    return ControlFlow::Break(());
                                }
                                i += consumed.max(1);
                            } else {
                                // Advance one char.
                                let step = slice[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                                i += step;
                            }
                        }
                    }
                } else {
                    let mut start = 0;
                    while let Some(rel) = slice[start..].find(needle.as_str()) {
                        let abs = start + rel;
                        let end = abs + needle.len();
                        if let ControlFlow::Break(()) = callback(base_offset + abs, base_offset + end) {
                            return ControlFlow::Break(());
                        }
                        start = end;
                    }
                }
            }
            Matcher::Regex(re) => {
                for m in re.find_iter(slice) {
                    // Zero-width matches (for example `^` with no anchor target) would
                    // make the iteration above loop forever; `regex::find_iter` already
                    // advances past zero-width matches, but we skip reporting them since
                    // they aren't user-meaningful.
                    if m.start() == m.end() {
                        continue;
                    }
                    if let ControlFlow::Break(()) = callback(base_offset + m.start(), base_offset + m.end()) {
                        return ControlFlow::Break(());
                    }
                }
            }
        }
        ControlFlow::Continue(())
    }

    /// Chunked scan for lines >`HUGE_LINE_THRESHOLD`. Each chunk extends by
    /// `HUGE_LINE_OVERLAP` bytes into the next so a needle on the boundary is
    /// still in scope; matches starting in the overlap region are kept for the
    /// next chunk to avoid double-reporting.
    fn find_chunked<F>(&self, line: &str, callback: &mut F)
    where
        F: FnMut(usize, usize) -> ControlFlow<()>,
    {
        let total_len = line.len();
        let mut chunk_start = 0;
        while chunk_start < total_len {
            // Pick a chunk window of up to HUGE_LINE_THRESHOLD bytes ending at a
            // char boundary, with HUGE_LINE_OVERLAP extra bytes on the right.
            let chunk_end_raw = (chunk_start + HUGE_LINE_THRESHOLD).min(total_len);
            let chunk_end = floor_char_boundary(line, chunk_end_raw);
            let scan_end_raw = (chunk_end + HUGE_LINE_OVERLAP).min(total_len);
            let scan_end = floor_char_boundary(line, scan_end_raw);

            // Cutoff: matches starting at or past this absolute offset belong to the next chunk.
            // For the last chunk (chunk_end == total_len), keep matches up to the end.
            let cutoff = if chunk_end == total_len { total_len } else { chunk_end };

            let slice = &line[chunk_start..scan_end];
            let base = chunk_start;
            let cutoff_local = cutoff;

            let cf = self.find_in_slice(slice, base, &mut |start, end| {
                if start >= cutoff_local {
                    // This match starts in the overlap; the next chunk will report it.
                    ControlFlow::Continue(())
                } else {
                    callback(start, end)
                }
            });

            if let ControlFlow::Break(()) = cf {
                return;
            }

            if chunk_end == total_len {
                break;
            }
            // Advance by one chunk window (without the overlap), but at least one byte.
            chunk_start = chunk_end;
        }
    }
}

/// Round `index` down to the previous char boundary in `s`. Cheaper than
/// `str::floor_char_boundary` (which is unstable) and good enough since we only
/// chunk at large indices.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// True if the regex pattern would force a cross-newline match in our streaming
/// model. We reject:
/// - `(?s)` (any single-line / dot-matches-newline flag),
/// - any literal `\n` byte in the pattern (the user typed an actual newline),
/// - `\n` escapes (the user wrote `\n` in the pattern to match a newline).
///
/// We don't try to reject every cross-line pattern (the user could still encode
/// `\x0a`), but the common forms surface a friendly error.
fn pattern_crosses_newline(pattern: &str) -> bool {
    if pattern.contains('\n') {
        return true;
    }
    // Walk the pattern looking for unescaped `(?s)` flag groups and `\n` escapes.
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'n' {
                return true;
            }
            // Skip the escape and its target so we don't misinterpret `\\(?s)` later.
            i += 2;
            continue;
        }
        if b == b'(' && i + 2 < bytes.len() && bytes[i + 1] == b'?' && contains_s_flag(&bytes[i + 2..]) {
            return true;
        }
        i += 1;
    }
    false
}

/// Outcome of scanning a single line with a `Matcher`. Used by every backend's
/// `search` impl to converge on a single per-match cancellation and limit-handling
/// protocol.
#[must_use]
pub enum LineScan {
    /// The line was scanned to completion and the search may continue.
    Done,
    /// The match limit was reached while scanning this line. The caller should
    /// stop reading further lines but is welcome to finish reporting progress.
    HitLimit,
    /// The cancel flag was observed while scanning this line. The caller should
    /// return immediately.
    Cancelled,
}

/// Scan `line` with `matcher`, pushing any matches into `results` with the given
/// `line_number` and `line_byte_offset` metadata. Checks `cancel` between matches
/// (per the per-match cancellation contract in step 1.2 of the viewer-search plan).
///
/// Stops as soon as either:
/// - `cancel` is set (returns `Cancelled`),
/// - or `results.len()` reaches `MAX_SEARCH_MATCHES` (returns `HitLimit`).
pub fn scan_line_with_matcher(
    matcher: &Matcher,
    line: &str,
    line_number: usize,
    line_byte_offset: u64,
    cancel: &AtomicBool,
    results: &Mutex<Vec<SearchMatch>>,
) -> LineScan {
    let mut outcome = LineScan::Done;
    matcher.find_matches(line, |start_byte, end_byte| {
        if cancel.load(Ordering::Relaxed) {
            outcome = LineScan::Cancelled;
            return ControlFlow::Break(());
        }
        // Column / length in UTF-16 code units, matching the JS string model the FE
        // uses to render highlights.
        let col_utf16: usize = line[..start_byte].chars().map(|c| c.len_utf16()).sum();
        let len_utf16: usize = line[start_byte..end_byte].chars().map(|c| c.len_utf16()).sum();
        let mut matches = results.lock_ignore_poison();
        matches.push(SearchMatch {
            line: line_number,
            column: col_utf16,
            length: len_utf16,
            byte_offset: line_byte_offset,
        });
        if matches.len() >= MAX_SEARCH_MATCHES {
            outcome = LineScan::HitLimit;
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    outcome
}

/// Given the bytes immediately after `(?` in a regex, look at the flag list up
/// to the first `)` or `:` and return true if `s` appears. We're scanning a flag
/// group like `(?ms:foo)` or `(?-i)`. Flags are ASCII single chars plus `-`.
fn contains_s_flag(after_question: &[u8]) -> bool {
    for &b in after_question {
        if b == b')' || b == b':' {
            return false;
        }
        if b == b's' {
            return true;
        }
        // Flag-list chars: letters, `-`. Anything else means we mis-parsed; bail out.
        if !(b.is_ascii_alphabetic() || b == b'-') {
            return false;
        }
    }
    false
}

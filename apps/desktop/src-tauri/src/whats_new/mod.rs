//! Parses the repo-root `CHANGELOG.md` (embedded at compile time) into a typed,
//! user-facing model for the "What's new" popup.
//!
//! The changelog is the source of truth: whatever lands in a release's prose lead
//! and its Added / Changed / Fixed / Security sections renders verbatim in the
//! app. This parser only strips machinery the user shouldn't see (trailing
//! commit-link groups, the `Non-app` section, unknown sections) and never grows
//! "fix up bad entries" logic. Garbage in the popup gets fixed in `CHANGELOG.md`,
//! never patched here. See `CLAUDE.md`.
//!
//! Resilience over strictness: malformed input must never panic or block startup.
//! Anything that doesn't parse is skipped and logged at debug.

use std::sync::OnceLock;

use semver::Version;
use serde::Serialize;
use specta::Type;

/// The repo-root changelog, embedded at build time. The path runs up from this
/// module (`src/whats_new/`) through `src-tauri/`, `desktop/`, `apps/` to the
/// repo root. It breaks at COMPILE time if files move, which is the good failure
/// mode. Cargo tracks this input, so a changelog edit triggers a rebuild on the
/// next `cargo build` (but NOT a live `pnpm dev` rebuild; see `CLAUDE.md`).
const CHANGELOG_MD: &str = include_str!("../../../../../CHANGELOG.md");

/// One released version's user-facing notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WhatsNewRelease {
    /// Semver string, for example `"0.26.0"`.
    pub version: String,
    /// Release date as written, for example `"2026-06-11"`. Display-only, never parsed.
    pub date: String,
    /// The prose lead: paragraphs between the heading and the first `###` section. Markdown.
    pub lead: Option<String>,
    /// The displayable sections in changelog order (Added / Changed / Fixed / Security).
    pub sections: Vec<WhatsNewSection>,
}

/// One titled section of a release (Added / Changed / Fixed / Security).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WhatsNewSection {
    /// One of `Added`, `Changed`, `Fixed`, `Security`.
    pub title: String,
    /// Bulleted entries, commit-link groups already stripped, other links flattened to text.
    pub entries: Vec<String>,
}

/// The section names we render, in their canonical display order. `Non-app` and
/// any name not in this set is dropped (logged at debug).
const DISPLAYABLE_SECTIONS: [&str; 4] = ["Added", "Changed", "Fixed", "Security"];

/// Returns the displayable releases in `since < v <= current`, newest first,
/// truncated to `max`.
///
/// `since = None` means no lower bound (the latest `max`). A `since` that isn't
/// valid semver is treated as no lower bound and logged. `current` that isn't
/// valid semver yields an empty result (nothing is `<= current`).
pub fn releases_between(since: Option<&str>, current: &str, max: usize) -> Vec<WhatsNewRelease> {
    let Some(current_version) = parse_version(current) else {
        log::debug!(target: "whats_new", "current version {current:?} is not valid semver; no releases");
        return vec![];
    };

    let lower_bound = since.and_then(|raw| match parse_version(raw) {
        Some(v) => Some(v),
        None => {
            log::debug!(target: "whats_new", "since {raw:?} is not valid semver; treating as no lower bound");
            None
        }
    });

    parsed_releases()
        .iter()
        .filter(|release| {
            let Some(v) = parse_version(&release.version) else {
                return false;
            };
            let within_upper = v <= current_version;
            let within_lower = lower_bound.as_ref().is_none_or(|low| v > *low);
            within_upper && within_lower
        })
        .take(max)
        .cloned()
        .collect()
}

/// Lenient semver parse: accepts a leading `v` and trims surrounding whitespace.
fn parse_version(raw: &str) -> Option<Version> {
    let trimmed = raw.trim().trim_start_matches('v');
    Version::parse(trimmed).ok()
}

/// All displayable releases, newest first, parsed once and cached.
fn parsed_releases() -> &'static [WhatsNewRelease] {
    static CACHE: OnceLock<Vec<WhatsNewRelease>> = OnceLock::new();
    CACHE.get_or_init(|| parse_changelog(CHANGELOG_MD))
}

/// A line classified during the top-down walk.
enum Line<'a> {
    /// `## [x.y.z] - YYYY-MM-DD`
    ReleaseHeading { version: &'a str, date: &'a str },
    /// `## [Unreleased]` or any other `## ...` we skip.
    OtherH2,
    /// `### Section`
    SectionHeading { title: &'a str },
    /// Any other content line (prose, bullets, blanks).
    Content(&'a str),
}

fn classify_line(line: &str) -> Line<'_> {
    if let Some(rest) = line.strip_prefix("## ") {
        if let Some((version, date)) = parse_release_heading(rest) {
            return Line::ReleaseHeading { version, date };
        }
        return Line::OtherH2;
    }
    if let Some(title) = line.strip_prefix("### ") {
        return Line::SectionHeading { title: title.trim() };
    }
    Line::Content(line)
}

/// Parses the `[x.y.z] - YYYY-MM-DD` body of a release heading. Returns `None`
/// for `[Unreleased]` and anything that doesn't fit the shape.
fn parse_release_heading(rest: &str) -> Option<(&str, &str)> {
    let rest = rest.trim();
    let inner = rest.strip_prefix('[')?;
    let close = inner.find(']')?;
    let version = &inner[..close];
    if !version_looks_numeric(version) {
        return None;
    }
    let after = inner[close + 1..].trim_start();
    let date = after.strip_prefix('-')?.trim();
    if date.is_empty() {
        return None;
    }
    Some((version, date))
}

/// A release tag is numeric if its first segment starts with a digit, which
/// rejects `[Unreleased]` without committing to a strict semver shape here.
fn version_looks_numeric(version: &str) -> bool {
    version.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Walks the changelog top-down, collecting releases until the displayable list
/// is exhausted. Skips `[Unreleased]`, drops `Non-app` and unknown sections, and
/// omits any release with no lead and no displayable section.
fn parse_changelog(markdown: &str) -> Vec<WhatsNewRelease> {
    let mut releases = Vec::new();
    let mut builder: Option<ReleaseBuilder> = None;

    for line in markdown.lines() {
        match classify_line(line) {
            Line::ReleaseHeading { version, date } => {
                if let Some(done) = builder.take().and_then(ReleaseBuilder::finish) {
                    releases.push(done);
                }
                builder = Some(ReleaseBuilder::new(version, date));
            }
            Line::OtherH2 => {
                // `[Unreleased]` or any other H2: end the current release, start no new one.
                if let Some(done) = builder.take().and_then(ReleaseBuilder::finish) {
                    releases.push(done);
                }
            }
            Line::SectionHeading { title } => {
                if let Some(b) = builder.as_mut() {
                    b.start_section(title);
                }
            }
            Line::Content(content) => {
                if let Some(b) = builder.as_mut() {
                    b.push_content(content);
                }
            }
        }
    }

    if let Some(done) = builder.and_then(ReleaseBuilder::finish) {
        releases.push(done);
    }

    releases
}

/// Accumulates one release's lead and sections as lines stream in.
struct ReleaseBuilder {
    version: String,
    date: String,
    lead_lines: Vec<String>,
    sections: Vec<SectionBuilder>,
    /// `Some(displayable_index_into_sections)` while inside a kept section,
    /// `None` while inside a dropped section or before the first `###`.
    current_section: SectionState,
}

/// A kept section's title plus the raw source lines under it (parsed into entries
/// only at `finish`, so the `WhatsNewSection` type stays purely about output).
struct SectionBuilder {
    title: String,
    raw_lines: Vec<String>,
}

enum SectionState {
    /// Before the first `###`: content goes to the lead.
    Lead,
    /// Inside a kept section, pointing at its slot in `sections`.
    Kept(usize),
    /// Inside a dropped (`Non-app` / unknown) section: content is discarded.
    Dropped,
}

impl ReleaseBuilder {
    fn new(version: &str, date: &str) -> Self {
        Self {
            version: version.to_string(),
            date: date.to_string(),
            lead_lines: Vec::new(),
            sections: Vec::new(),
            current_section: SectionState::Lead,
        }
    }

    fn start_section(&mut self, title: &str) {
        if let Some(canonical) = DISPLAYABLE_SECTIONS.iter().find(|s| s.eq_ignore_ascii_case(title)) {
            self.sections.push(SectionBuilder {
                title: (*canonical).to_string(),
                raw_lines: Vec::new(),
            });
            self.current_section = SectionState::Kept(self.sections.len() - 1);
        } else {
            log::debug!(target: "whats_new", "dropping section {title:?} in {}", self.version);
            self.current_section = SectionState::Dropped;
        }
    }

    fn push_content(&mut self, content: &str) {
        match self.current_section {
            SectionState::Lead => self.lead_lines.push(content.to_string()),
            SectionState::Kept(index) => self.sections[index].raw_lines.push(content.to_string()),
            SectionState::Dropped => {}
        }
    }

    /// Finalizes the release: builds the lead, post-processes entries, drops empty
    /// sections, and returns `None` if nothing displayable remains.
    fn finish(self) -> Option<WhatsNewRelease> {
        let lead = build_lead(&self.lead_lines);

        let sections: Vec<WhatsNewSection> = self
            .sections
            .into_iter()
            .filter_map(|section| {
                let entries = parse_entries(&section.raw_lines);
                if entries.is_empty() {
                    None
                } else {
                    Some(WhatsNewSection {
                        title: section.title,
                        entries,
                    })
                }
            })
            .collect();

        if lead.is_none() && sections.is_empty() {
            log::debug!(target: "whats_new", "omitting release {} (no lead, no displayable section)", self.version);
            return None;
        }

        Some(WhatsNewRelease {
            version: self.version,
            date: self.date,
            lead,
            sections,
        })
    }
}

/// Joins the lead lines into paragraphs, preserving blank-line paragraph breaks.
/// Returns `None` when there's no prose.
fn build_lead(lines: &[String]) -> Option<String> {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in lines {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" "));
                current.clear();
            }
        } else {
            current.push(line.trim());
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }

    let joined = paragraphs.join("\n\n");
    if joined.trim().is_empty() { None } else { Some(joined) }
}

/// Turns a section's raw source lines into entries: groups each bullet with its
/// wrapped continuation lines, strips the trailing commit-link group, and
/// flattens any remaining markdown links to plain text.
fn parse_entries(raw_lines: &[String]) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    let mut current: Option<String> = None;

    for line in raw_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(bullet) = strip_bullet_marker(line) {
            if let Some(done) = current.take() {
                entries.push(finalize_entry(&done));
            }
            current = Some(bullet.to_string());
        } else if let Some(buffer) = current.as_mut() {
            // A wrapped continuation line of the current bullet.
            buffer.push(' ');
            buffer.push_str(trimmed);
        }
        // A non-bullet line before any bullet is ignored (shouldn't occur in a section).
    }
    if let Some(done) = current.take() {
        entries.push(finalize_entry(&done));
    }

    entries
}

/// Returns the entry text after a `- ` / `* ` / `+ ` bullet marker, or `None`
/// when the line isn't a top-level bullet. Indented (continuation) lines are not
/// bullets, so we only match markers at column zero.
fn strip_bullet_marker(line: &str) -> Option<&str> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest.trim());
        }
    }
    None
}

/// Strips the trailing commit-link group, then flattens other markdown links.
fn finalize_entry(entry: &str) -> String {
    let without_links = strip_trailing_commit_group(entry);
    flatten_markdown_links(&without_links)
}

/// Removes the trailing ` ([hash](url), [hash](url), …)` parenthetical that the
/// release flow appends. Hashes are 6-8 hex chars; an entry may carry several
/// comma-separated links (already joined from wrapped lines by `parse_entries`).
///
/// The group is recognized structurally: the entry must end with `)`, the
/// matching `(` opens a parenthetical, and every comma-separated item inside is a
/// bare `[hex](url)` commit link. If any item isn't, the parenthetical is real
/// content and we leave it alone.
fn strip_trailing_commit_group(entry: &str) -> String {
    let trimmed = entry.trim_end();
    if !trimmed.ends_with(')') {
        return trimmed.to_string();
    }
    let Some(open) = find_matching_open_paren(trimmed) else {
        return trimmed.to_string();
    };
    let inner = &trimmed[open + 1..trimmed.len() - 1];
    if !inner.split(',').map(str::trim).all(is_commit_link) {
        return trimmed.to_string();
    }
    trimmed[..open].trim_end().to_string()
}

/// Finds the byte index of the `(` that matches the final `)` of `s`, honoring
/// nesting. Returns `None` if unbalanced.
fn find_matching_open_paren(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in s.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// True when `item` is exactly a `[hex](url)` commit link with a 6-8 hex hash.
fn is_commit_link(item: &str) -> bool {
    let Some(rest) = item.strip_prefix('[') else {
        return false;
    };
    let Some(close) = rest.find(']') else {
        return false;
    };
    let hash = &rest[..close];
    if !(6..=8).contains(&hash.len()) || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    let after = &rest[close + 1..];
    after.starts_with('(') && after.ends_with(')') && after.len() >= 2
}

/// Flattens any `[text](url)` markdown link to its `text`, leaving all other
/// markdown (bold, italic, `code`, quotes) verbatim. Malformed links are left
/// as-is.
fn flatten_markdown_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'['
            && let Some((label, consumed)) = parse_link_at(&text[i..])
        {
            out.push_str(label);
            i += consumed;
            continue;
        }
        // Push one full UTF-8 char to keep multibyte content intact.
        let ch = text[i..].chars().next().expect("index is on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// If `s` starts with a `[label](url)` link, returns its label and the number of
/// bytes consumed.
fn parse_link_at(s: &str) -> Option<(&str, usize)> {
    let after_open = s.strip_prefix('[')?;
    let close_bracket = after_open.find(']')?;
    let label = &after_open[..close_bracket];
    let after_label = &after_open[close_bracket + 1..];
    if !after_label.starts_with('(') {
        return None;
    }
    let close_paren = after_label.find(')')?;
    // total = '[' + label + ']' + '(' + url + ')'
    let consumed = 1 + close_bracket + 1 + close_paren + 1;
    Some((label, consumed))
}

#[cfg(test)]
mod tests;

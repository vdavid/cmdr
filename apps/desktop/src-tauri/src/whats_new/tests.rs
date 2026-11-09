//! Unit tests for the changelog parser and slicer.
//!
//! These run against the hand-written `FIXTURE` string below, not the live
//! `CHANGELOG.md`, so they don't churn with every release. The one exception is
//! `smoke_real_changelog_parses`, which parses the embedded file and acts as the
//! drift alarm if the changelog format ever changes.

use super::*;

/// A hand-written changelog covering every shape the parser must handle. It is
/// NOT the real changelog: editing the real one must not break these tests.
const FIXTURE: &str = r#"# Changelog

All notable changes to Cmdr will be documented in this file.

## [Unreleased]

### Added

- This must never appear in any result (deadbeef)

## [0.10.0] - 2026-07-01

Double-digit minor lead. This release sorts after 0.9.0 by semver, not by string
order.

### Added

- A multi-line entry whose commit hashes wrap across two source lines and carry
  several of them (abc123,
  d4e5f6a7)
- Keep inline **bold** and `code` and a [docs page](https://getcmdr.com/docs) flattened to text
  (1a2b3c4)

### Non-app

- This entire section is dropped (ffffff)

## [0.9.0] - 2026-06-15

First lead paragraph.

Second lead paragraph after a blank line.

### Changed

- A six-char-hash entry (abc123)

### Surprise

- An unknown section that must be dropped (beef12)

## [0.8.0] - 2026-06-01

### Fixed

- An entry with no lead above it (12345678)

## [0.7.0] - 2026-05-01

### Non-app

- Only a Non-app section, so this whole release is omitted
  (777aaa)

## [0.6.0] - 2026-04-01

Lead with a real (parenthetical aside) that must survive, plus a security note.

### Security

- Patch a thing while keeping a trailing (non-hash aside)
  (0a0a0a0a)
- Speed up the scan (~40x speed-up!)
- Bump the dep (smb2 0.8.0)
"#;

/// A larger fixture for the slicing/cap tests: 10 trivial releases, 0.20.0 down
/// to 0.11.0, each with a one-line lead and one Added entry.
fn many_releases_fixture() -> String {
    let mut md = String::from("# Changelog\n\n## [Unreleased]\n\n### Added\n\n- nope\n\n");
    for minor in (11..=20).rev() {
        md.push_str(&format!(
            "## [0.{minor}.0] - 2026-01-{minor:02}\n\nLead for 0.{minor}.0.\n\n### Added\n\n- Entry for 0.{minor}.0\n\n"
        ));
    }
    md
}

fn parse(md: &str) -> Vec<WhatsNewRelease> {
    parse_changelog(md)
}

fn versions(releases: &[WhatsNewRelease]) -> Vec<String> {
    releases.iter().map(|r| r.version.clone()).collect()
}

/// The embedded changelog's raw lines between `## [version]` and the next `###` or `##`.
fn raw_lead_source(version: &str) -> String {
    let heading = format!("## [{version}]");
    CHANGELOG_MD
        .lines()
        .skip_while(|line| !line.starts_with(&heading))
        .skip(1)
        .take_while(|line| !line.starts_with("### ") && !line.starts_with("## "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parses a one-release changelog whose lead is `lead` and returns that lead's HTML.
fn lead_html_of(lead: &str) -> Option<String> {
    let md = format!("# Changelog\n\n## [1.0.0] - 2026-07-14\n\n{lead}\n\n### Added\n\n- Something (abc123)\n");
    parse(&md).remove(0).lead_html
}

#[test]
fn skips_unreleased_block() {
    let releases = parse(FIXTURE);
    assert!(!versions(&releases).contains(&"Unreleased".to_string()));
    // The Unreleased entry text must not leak into any release.
    for release in &releases {
        for section in &release.sections {
            assert!(!section.entries_html.iter().any(|e| e.contains("must never appear")));
        }
    }
}

#[test]
fn recognizes_release_headings_in_order() {
    let releases = parse(FIXTURE);
    // 0.7.0 is omitted (Non-app only), so it's absent.
    assert_eq!(versions(&releases), vec!["0.10.0", "0.9.0", "0.8.0", "0.6.0"]);
    assert_eq!(releases[0].date, "2026-07-01");
}

#[test]
fn renders_wrapped_prose_lead_as_one_paragraph() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    // The soft source wrap stays a `\n` inside the paragraph, which HTML collapses to a space.
    assert_eq!(
        r.lead_html.as_deref(),
        Some("<p>Double-digit minor lead. This release sorts after 0.9.0 by semver, not by string\norder.</p>\n")
    );
}

#[test]
fn renders_multi_paragraph_lead_as_separate_paragraphs() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.9.0").unwrap();
    assert_eq!(
        r.lead_html.as_deref(),
        Some("<p>First lead paragraph.</p>\n<p>Second lead paragraph after a blank line.</p>\n")
    );
}

#[test]
fn renders_numbered_list_lead() {
    assert_eq!(
        lead_html_of("**Big release.**\n\n1. First highlight.\n2. Second highlight.\n3. Third highlight."),
        Some(
            "<p><strong>Big release.</strong></p>\n<ol>\n<li>First highlight.</li>\n<li>Second highlight.</li>\n<li>Third highlight.</li>\n</ol>\n"
                .to_string()
        )
    );
}

#[test]
fn renders_wrapped_numbered_list_item_as_one_item() {
    // The changelog formatter wraps long highlights onto an indented continuation line.
    assert_eq!(
        lead_html_of(
            "1. First highlight.\n2. Second highlight that runs long enough that the formatter wraps it onto the next\n   line right here.\n3. Third highlight."
        ),
        Some(
            "<ol>\n<li>First highlight.</li>\n<li>Second highlight that runs long enough that the formatter wraps it onto the next\nline right here.</li>\n<li>Third highlight.</li>\n</ol>\n"
                .to_string()
        )
    );
}

#[test]
fn renders_nested_list_under_a_numbered_highlight() {
    // The 0.44.0 lead's shape. Pre-fix the parser trimmed the indentation away and the
    // app's renderer merged everything into one `<ul>` whose first bullet was empty.
    let lead = "\
Thanks for the feedback!

Some highlights:

1. SFTP support. Try `⌘K`!
2. Context menu updates:
   - On files: a Share menu.
   - On Cmdr in your Dock: tabs and bookmarks.";
    assert_eq!(
        lead_html_of(lead),
        Some(
            "<p>Thanks for the feedback!</p>\n<p>Some highlights:</p>\n<ol>\n<li>SFTP support. Try <code>⌘K</code>!</li>\n<li>Context menu updates:\n<ul>\n<li>On files: a Share menu.</li>\n<li>On Cmdr in your Dock: tabs and bookmarks.</li>\n</ul>\n</li>\n</ol>\n"
                .to_string()
        )
    );
}

#[test]
fn release_with_no_lead_has_none() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.8.0").unwrap();
    assert_eq!(r.lead_html, None);
    assert_eq!(r.sections.len(), 1);
    assert_eq!(r.sections[0].title, "Fixed");
}

#[test]
fn drops_non_app_section() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    assert!(r.sections.iter().all(|s| s.title != "Non-app"));
    for section in &r.sections {
        assert!(
            !section
                .entries_html
                .iter()
                .any(|e| e.contains("entire section is dropped"))
        );
    }
}

#[test]
fn drops_unknown_section() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.9.0").unwrap();
    assert_eq!(
        r.sections.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
        vec!["Changed"]
    );
}

#[test]
fn omits_release_with_only_non_app_and_no_lead() {
    let releases = parse(FIXTURE);
    assert!(!versions(&releases).contains(&"0.7.0".to_string()));
}

#[test]
fn strips_multi_hash_wrapped_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    let added = r.sections.iter().find(|s| s.title == "Added").unwrap();
    assert_eq!(
        added.entries_html[0],
        "A multi-line entry whose commit hashes wrap across two source lines and carry several of them"
    );
}

#[test]
fn strips_six_char_hash_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.9.0").unwrap();
    let changed = r.sections.iter().find(|s| s.title == "Changed").unwrap();
    assert_eq!(changed.entries_html[0], "A six-char-hash entry");
}

#[test]
fn strips_eight_char_hash_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.8.0").unwrap();
    let fixed = r.sections.iter().find(|s| s.title == "Fixed").unwrap();
    assert_eq!(fixed.entries_html[0], "An entry with no lead above it");
}

#[test]
fn renders_entry_inline_markdown_and_flattens_a_real_link() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    let added = r.sections.iter().find(|s| s.title == "Added").unwrap();
    // Inline HTML with no wrapping `<p>`: the dialog puts each entry in its own `<li>`.
    assert_eq!(
        added.entries_html[1],
        "Keep inline <strong>bold</strong> and <code>code</code> and a docs page flattened to text"
    );
}

#[test]
fn escapes_html_specials_in_an_entry() {
    let md = "# Changelog\n\n## [1.0.0] - 2026-07-14\n\n### Added\n\n- Look for `Settings > Navigation & file ops` (abc123)\n";
    let releases = parse(md);
    assert_eq!(
        releases[0].sections[0].entries_html[0],
        "Look for <code>Settings &gt; Navigation &amp; file ops</code>"
    );
}

#[test]
fn keeps_a_real_trailing_parenthetical_that_is_not_a_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.6.0").unwrap();
    let security = r.sections.iter().find(|s| s.title == "Security").unwrap();
    // The commit group is stripped, but the "(non-hash aside)" stays.
    assert_eq!(
        security.entries_html[0],
        "Patch a thing while keeping a trailing (non-hash aside)"
    );
    // A trailing aside with no commit group at all survives untouched: the hash
    // shape is what makes a parenthetical machinery, not its position.
    assert_eq!(security.entries_html[1], "Speed up the scan (~40x speed-up!)");
    assert_eq!(security.entries_html[2], "Bump the dep (smb2 0.8.0)");
    // And the lead's real aside survives too.
    assert!(r.lead_html.as_ref().unwrap().contains("(parenthetical aside)"));
}

// --- releases_between slicing, driven by parse_changelog output directly so the
// tests don't depend on the embedded file. ---

/// Mirrors `releases_between` but against an arbitrary parsed list, so the
/// slicing logic is testable without the global cache.
fn slice(all: &[WhatsNewRelease], since: Option<&str>, current: &str, max: usize) -> Vec<String> {
    let current_v = parse_version(current).unwrap();
    let lower = since.and_then(parse_version);
    all.iter()
        .filter(|r| {
            let v = parse_version(&r.version).unwrap();
            v <= current_v && lower.as_ref().is_none_or(|low| v > *low)
        })
        .take(max)
        .map(|r| r.version.clone())
        .collect()
}

#[test]
fn slice_caps_at_max_skip_eight_show_five() {
    let all = parse(&many_releases_fixture());
    // since 0.12.0, current 0.20.0 → 0.20 down to 0.13 is in range (8 releases), capped to 5.
    let got = slice(&all, Some("0.12.0"), "0.20.0", 5);
    assert_eq!(got, vec!["0.20.0", "0.19.0", "0.18.0", "0.17.0", "0.16.0"]);
}

#[test]
fn slice_since_equals_current_is_empty() {
    let all = parse(&many_releases_fixture());
    assert!(slice(&all, Some("0.20.0"), "0.20.0", 5).is_empty());
}

#[test]
fn slice_since_older_than_oldest_is_all_in_range_still_capped() {
    let all = parse(&many_releases_fixture());
    let got = slice(&all, Some("0.1.0"), "0.20.0", 5);
    assert_eq!(got.len(), 5);
    assert_eq!(got[0], "0.20.0");
}

#[test]
fn slice_no_lower_bound_max_one_is_current_only() {
    let all = parse(&many_releases_fixture());
    assert_eq!(slice(&all, None, "0.20.0", 1), vec!["0.20.0"]);
}

#[test]
fn slice_no_lower_bound_max_five_is_latest_five() {
    let all = parse(&many_releases_fixture());
    assert_eq!(
        slice(&all, None, "0.20.0", 5),
        vec!["0.20.0", "0.19.0", "0.18.0", "0.17.0", "0.16.0"]
    );
}

#[test]
fn slice_garbage_since_treated_as_no_lower_bound() {
    let all = parse(&many_releases_fixture());
    // "not-a-version" → no lower bound, so capped latest five.
    assert_eq!(slice(&all, Some("not-a-version"), "0.20.0", 5).len(), 5);
}

#[test]
fn semver_ordering_double_digit_components() {
    // 0.9.0 < 0.10.0 by semver, even though "0.10.0" < "0.9.0" as strings.
    assert!(parse_version("0.9.0").unwrap() < parse_version("0.10.0").unwrap());
    let all = parse(FIXTURE);
    // 0.10.0 is newest in the fixture; with current 0.10.0 and since 0.9.0 it's the only one.
    let got = slice(&all, Some("0.9.0"), "0.10.0", 5);
    assert_eq!(got, vec!["0.10.0"]);
}

#[test]
fn releases_between_garbage_current_is_empty() {
    assert!(releases_between(None, "garbage", 5).is_empty());
}

// --- The drift alarm over the real embedded changelog. ---

#[test]
fn smoke_real_changelog_parses() {
    let current = env!("CARGO_PKG_VERSION");
    let latest_five = releases_between(None, current, 5);

    assert!(
        !latest_five.is_empty(),
        "expected at least one displayable release in the real changelog"
    );
    assert!(latest_five.len() <= 5);

    // The current version is present and is the newest.
    assert_eq!(
        latest_five[0].version, current,
        "newest displayable release must be the current version"
    );

    // Each parsed release has a lead (the release flow mandates one per release).
    for release in &latest_five {
        assert!(
            release.lead_html.is_some(),
            "release {} is missing its lead",
            release.version
        );
        assert!(
            !release.date.is_empty(),
            "release {} is missing its date",
            release.version
        );
    }

    // No commit-ref residue leaked into any rendered entry: neither a bare hash
    // group (what the changelog carries) nor a URL (the deprecated form).
    for release in &latest_five {
        for section in &release.sections {
            assert!(DISPLAYABLE_SECTIONS.contains(&section.title.as_str()));
            for entry in &section.entries_html {
                assert!(
                    // Owner-agnostic, so it guards both repo names (#476).
                    !entry.contains("/cmdr/commit/"),
                    "commit link leaked into entry: {entry:?}"
                );
                assert_eq!(
                    strip_trailing_commit_group(entry),
                    *entry,
                    "commit hash group leaked into entry: {entry:?}"
                );
            }
        }
    }
}

#[test]
fn real_changelog_renders_as_commonmark_blocks() {
    // What the popup actually shows for the latest five real releases. Every lead is block
    // HTML, never raw markdown or the `<br />`-joined soup a toy renderer produces, and no
    // list item opens straight into a nested list (the empty bullet 0.44.0 shipped with).
    let current = env!("CARGO_PKG_VERSION");
    for release in releases_between(None, current, 5) {
        let version = &release.version;
        let lead = release.lead_html.as_deref().unwrap_or_default();
        // The lead reaches the renderer byte-faithful: exactly the source lines between the
        // release heading and its first section, found here without the parser's walk.
        assert_eq!(
            lead,
            render_block_html(&raw_lead_source(version)),
            "release {version}'s lead was transformed on its way to the renderer"
        );
        assert!(
            lead.starts_with('<') && lead.trim_end().ends_with('>'),
            "release {version}'s lead isn't block HTML: {lead:?}"
        );
        assert!(
            !lead.contains("<br"),
            "release {version}'s lead has a hard break: {lead:?}"
        );
        for empty_bullet in ["<li><ul>", "<li><ol>", "<li>\n<ul>", "<li>\n<ol>"] {
            assert!(
                !lead.contains(empty_bullet),
                "release {version}'s lead has a list item with no text of its own: {lead:?}"
            );
        }
        for section in &release.sections {
            for entry in &section.entries_html {
                assert!(
                    !entry.contains("<p>") && !entry.contains("<li>"),
                    "release {version} has an entry that rendered as a block: {entry:?}"
                );
            }
        }
    }
}

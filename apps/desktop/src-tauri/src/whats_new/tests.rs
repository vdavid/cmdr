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

- This must never appear in any result ([deadbeef](https://github.com/vdavid/cmdr/commit/deadbeef))

## [0.10.0] - 2026-07-01

Double-digit minor lead. This release sorts after 0.9.0 by semver, not by string
order.

### Added

- A multi-line entry whose commit links wrap across two source lines and carry
  several hashes ([abc123](https://github.com/vdavid/cmdr/commit/abc123),
  [d4e5f6a7](https://github.com/vdavid/cmdr/commit/d4e5f6a7))
- Keep inline **bold** and `code` and a [docs page](https://getcmdr.com/docs) flattened to text
  ([1a2b3c4](https://github.com/vdavid/cmdr/commit/1a2b3c4))

### Non-app

- This entire section is dropped ([ffffff](https://github.com/vdavid/cmdr/commit/ffffff))

## [0.9.0] - 2026-06-15

First lead paragraph.

Second lead paragraph after a blank line.

### Changed

- A six-char-hash entry ([abc123](https://github.com/vdavid/cmdr/commit/abc123))

### Surprise

- An unknown section that must be dropped ([beef12](https://github.com/vdavid/cmdr/commit/beef12))

## [0.8.0] - 2026-06-01

### Fixed

- An entry with no lead above it ([12345678](https://github.com/vdavid/cmdr/commit/12345678))

## [0.7.0] - 2026-05-01

### Non-app

- Only a Non-app section, so this whole release is omitted
  ([777aaa](https://github.com/vdavid/cmdr/commit/777aaa))

## [0.6.0] - 2026-04-01

Lead with a real (parenthetical aside) that must survive, plus a security note.

### Security

- Patch a thing while keeping a trailing (non-link aside)
  ([0a0a0a0a](https://github.com/vdavid/cmdr/commit/0a0a0a0a))
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

#[test]
fn skips_unreleased_block() {
    let releases = parse(FIXTURE);
    assert!(!versions(&releases).contains(&"Unreleased".to_string()));
    // The Unreleased entry text must not leak into any release.
    for release in &releases {
        for section in &release.sections {
            assert!(!section.entries.iter().any(|e| e.contains("must never appear")));
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
fn extracts_wrapped_prose_lead() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    let lead = r.lead.as_ref().unwrap();
    assert!(lead.starts_with("Double-digit minor lead."));
    // Wrapped prose lines reflow onto one line with a space (a soft source `\n` and a
    // space render identically, so the sentence reads as one line either way).
    assert_eq!(
        lead,
        "Double-digit minor lead. This release sorts after 0.9.0 by semver, not by string order."
    );
}

#[test]
fn preserves_numbered_list_lead() {
    // A lead written as a bold headline plus a Markdown numbered list must reach the
    // renderer with each `N.` marker at the start of its own line; otherwise snarkdown
    // (app) and marked (website) show literal "1. 2. 3." text instead of an <ol>.
    let md = "\
# Changelog

## [1.0.0] - 2026-07-14

**Big release.**

1. First highlight.
2. Second highlight.
3. Third highlight.

### Added

- Something ([abc123](https://github.com/vdavid/cmdr/commit/abc123))
";
    let releases = parse(md);
    assert_eq!(
        releases[0].lead.as_deref(),
        Some("**Big release.**\n\n1. First highlight.\n2. Second highlight.\n3. Third highlight.")
    );
}

#[test]
fn reflows_wrapped_numbered_list_item() {
    // A numbered highlight that soft-wraps across two source lines (the changelog
    // formatter caps line length) must reach the renderer as ONE line per item.
    // Otherwise the bare continuation line closes snarkdown's <ol> and the next `N.`
    // opens a fresh list that restarts at "1." (marked survives it via lazy
    // continuation, snarkdown does not).
    let md = "\
# Changelog

## [1.0.0] - 2026-07-14

1. First highlight.
2. Second highlight that runs long enough that the formatter wraps it onto the next
   line right here.
3. Third highlight.

### Added

- Something ([abc123](https://github.com/vdavid/cmdr/commit/abc123))
";
    let releases = parse(md);
    assert_eq!(
        releases[0].lead.as_deref(),
        Some(
            "1. First highlight.\n2. Second highlight that runs long enough that the formatter wraps it onto the next line right here.\n3. Third highlight."
        )
    );
}

#[test]
fn extracts_multi_paragraph_lead() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.9.0").unwrap();
    assert_eq!(
        r.lead.as_deref(),
        Some("First lead paragraph.\n\nSecond lead paragraph after a blank line.")
    );
}

#[test]
fn release_with_no_lead_has_none() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.8.0").unwrap();
    assert_eq!(r.lead, None);
    assert_eq!(r.sections.len(), 1);
    assert_eq!(r.sections[0].title, "Fixed");
}

#[test]
fn drops_non_app_section() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    assert!(r.sections.iter().all(|s| s.title != "Non-app"));
    for section in &r.sections {
        assert!(!section.entries.iter().any(|e| e.contains("entire section is dropped")));
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
fn strips_multi_link_wrapped_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    let added = r.sections.iter().find(|s| s.title == "Added").unwrap();
    let entry = &added.entries[0];
    assert_eq!(
        entry,
        "A multi-line entry whose commit links wrap across two source lines and carry several hashes"
    );
}

#[test]
fn strips_six_char_hash_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.9.0").unwrap();
    let changed = r.sections.iter().find(|s| s.title == "Changed").unwrap();
    assert_eq!(changed.entries[0], "A six-char-hash entry");
}

#[test]
fn strips_eight_char_hash_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.8.0").unwrap();
    let fixed = r.sections.iter().find(|s| s.title == "Fixed").unwrap();
    assert_eq!(fixed.entries[0], "An entry with no lead above it");
}

#[test]
fn keeps_inline_markdown_and_flattens_non_commit_link() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.10.0").unwrap();
    let added = r.sections.iter().find(|s| s.title == "Added").unwrap();
    // Second entry keeps bold + code, flattens the docs link to its label, strips the commit group.
    assert_eq!(
        added.entries[1],
        "Keep inline **bold** and `code` and a docs page flattened to text"
    );
}

#[test]
fn keeps_a_real_trailing_parenthetical_that_is_not_a_commit_group() {
    let releases = parse(FIXTURE);
    let r = releases.iter().find(|r| r.version == "0.6.0").unwrap();
    let security = r.sections.iter().find(|s| s.title == "Security").unwrap();
    // The commit group is stripped, but the "(non-link aside)" stays.
    assert_eq!(
        security.entries[0],
        "Patch a thing while keeping a trailing (non-link aside)"
    );
    // And the lead's real aside survives too.
    assert!(r.lead.as_ref().unwrap().contains("(parenthetical aside)"));
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

    // Each parsed release has a lead (the release flow mandates a lead per release: a bold
    // headline, optionally followed by a short numbered list of the highlights).
    for release in &latest_five {
        assert!(
            release.lead.is_some(),
            "release {} is missing its prose lead",
            release.version
        );
        assert!(
            !release.date.is_empty(),
            "release {} is missing its date",
            release.version
        );
    }

    // No commit-link residue leaked into any rendered entry.
    for release in &latest_five {
        for section in &release.sections {
            assert!(DISPLAYABLE_SECTIONS.contains(&section.title.as_str()));
            for entry in &section.entries {
                assert!(
                    !entry.contains("github.com/vdavid/cmdr/commit/"),
                    "commit link leaked into entry: {entry:?}"
                );
            }
        }
    }
}

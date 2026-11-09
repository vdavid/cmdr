//! Tests for the advisory list: parsing, the version window, the link allowlist, and a canary over
//! the committed file.

use super::*;

fn version(raw: &str) -> Version {
    Version::parse(raw).expect("test versions are semver")
}

fn advisory_json(id: &str, fixed_in: &str, severity: &str) -> String {
    format!(
        r#"{{"id":"{id}","fixedIn":"{fixed_in}","severity":"{severity}","url":"https://github.com/vdavid/cmdr/security/advisories/{id}"}}"#
    )
}

fn list_json(entries: &[String]) -> String {
    format!(r#"{{"advisories":[{}]}}"#, entries.join(","))
}

#[test]
fn a_well_formed_list_parses() {
    let json = list_json(&[advisory_json("GHSA-aaaa-bbbb-cccc", "0.52.0", "high")]);
    let advisories = parse(&json, "test").expect("well-formed list");
    assert_eq!(
        advisories,
        vec![Advisory {
            id: "GHSA-aaaa-bbbb-cccc".to_string(),
            fixed_in: version("0.52.0"),
            severity: Severity::High,
            url: "https://github.com/vdavid/cmdr/security/advisories/GHSA-aaaa-bbbb-cccc".to_string(),
        }]
    );
}

#[test]
fn an_empty_list_parses_to_nothing() {
    assert_eq!(parse(r#"{"advisories":[]}"#, "test"), Some(vec![]));
}

/// Malformed input is "no advisory", never an error: the update carrying it is the fix itself.
#[test]
fn malformed_input_is_no_list() {
    for (label, json) in [
        ("not json", "not json at all".to_string()),
        ("wrong shape", r#"[]"#.to_string()),
        (
            "unknown severity",
            list_json(&[advisory_json("GHSA-x", "0.52.0", "catastrophic")]),
        ),
        ("not semver", list_json(&[advisory_json("GHSA-x", "0.52", "high")])),
        (
            "unknown field",
            r#"{"advisories":[{"id":"a","fixedIn":"0.52.0","severity":"low","url":"https://getcmdr.com/changelog","summary":"x"}]}"#
                .to_string(),
        ),
    ] {
        assert_eq!(parse(&json, "test"), None, "{label} must parse to no list");
    }
}

#[test]
fn severities_order_from_low_to_critical() {
    assert!(Severity::Low < Severity::Medium);
    assert!(Severity::Medium < Severity::High);
    assert!(Severity::High < Severity::Critical);
}

fn three_advisories() -> Vec<Advisory> {
    let json = list_json(&[
        advisory_json("A-0.51", "0.51.0", "critical"),
        advisory_json("B-0.52", "0.52.0", "medium"),
        advisory_json("C-0.54", "0.54.0", "high"),
    ]);
    parse(&json, "test").expect("well-formed list")
}

fn ids(advisories: &[&Advisory]) -> Vec<String> {
    advisories.iter().map(|a| a.id.clone()).collect()
}

/// The window is `after < fixedIn <= upto`: the running build already has what it was built with,
/// and the staged build has everything up to and including itself.
#[test]
fn the_window_excludes_the_running_version_and_includes_the_new_one() {
    let advisories = three_advisories();
    assert_eq!(
        ids(&fixed_between(&advisories, &version("0.51.0"), &version("0.52.0"))),
        vec!["B-0.52"]
    );
    assert!(fixed_between(&advisories, &version("0.52.0"), &version("0.53.0")).is_empty());
}

/// An install that skips releases still learns about a fix shipped in one it never ran: the list
/// is cumulative.
#[test]
fn a_skipped_release_still_counts() {
    let advisories = three_advisories();
    assert_eq!(
        ids(&fixed_between(&advisories, &version("0.50.0"), &version("0.55.0"))),
        vec!["A-0.51", "C-0.54", "B-0.52"],
        "most severe first"
    );
}

#[test]
fn a_downgrade_or_no_change_has_no_window() {
    let advisories = three_advisories();
    assert!(fixed_between(&advisories, &version("0.55.0"), &version("0.50.0")).is_empty());
    assert!(fixed_between(&advisories, &version("0.52.0"), &version("0.52.0")).is_empty());
}

#[test]
fn allowed_advisory_links_pass() {
    for url in [
        "https://github.com/vdavid/cmdr/security/advisories/GHSA-aaaa-bbbb-cccc",
        // The org name the repo moved to (#476). Builds shipped before the move vet links with
        // their own allowlist, so this one has to be in a release before any advisory uses it.
        "https://github.com/getcmdr/cmdr/security/advisories/GHSA-aaaa-bbbb-cccc",
        "https://getcmdr.com/changelog",
        "https://getcmdr.com/changelog/",
        "https://getcmdr.com/changelog#v0.54.0",
    ] {
        assert!(is_allowed_advisory_url(url), "{url} must be allowed");
    }
}

/// A notice is something users act on, so its link has to go where we say it goes.
#[test]
fn lookalike_and_trick_links_are_refused() {
    for url in [
        // Wrong scheme.
        "http://github.com/vdavid/cmdr/security/advisories/GHSA-x",
        "javascript:alert(1)",
        // Lookalike hosts.
        "https://github.com.evil.example/vdavid/cmdr/security/advisories/GHSA-x",
        "https://evilgithub.com/vdavid/cmdr/security/advisories/GHSA-x",
        "https://www.getcmdr.com/changelog",
        "https://getcmdr.co/changelog",
        // Userinfo: the host is the part after the `@`.
        "https://github.com@evil.example/vdavid/cmdr/security/advisories/GHSA-x",
        "https://user:pass@getcmdr.com/changelog",
        // Right host, wrong place.
        "https://github.com/vdavid/cmdr/issues/1",
        "https://github.com/vdavid/cmdr/security/advisories/",
        "https://github.com/evil/cmdr/security/advisories/GHSA-x",
        "https://github.com/getcmdr/cmdr/security/advisories/",
        "https://github.com/getcmdr/evil/security/advisories/GHSA-x",
        "https://github.com/getcmdr/cmdr/security/advisories/../../../../evil/repo",
        "https://getcmdr.com/changelog-but-not-really",
        "https://getcmdr.com/",
        // A path that only reads right before normalization.
        "https://github.com/vdavid/cmdr/security/advisories/../../../../evil/repo",
        // An explicit port.
        "https://getcmdr.com:8443/changelog",
        // Not a URL.
        "",
        "getcmdr.com/changelog",
    ] {
        assert!(!is_allowed_advisory_url(url), "{url} must be refused");
    }
}

#[test]
fn an_advisory_for_the_popup_drops_a_link_that_isnt_allowed() {
    let json = r#"{"advisories":[
        {"id":"ok","fixedIn":"0.52.0","severity":"low","url":"https://getcmdr.com/changelog"},
        {"id":"bad","fixedIn":"0.52.0","severity":"high","url":"https://evil.example/changelog"}
    ]}"#;
    let advisories = parse(json, "test").expect("well-formed list");
    let shown = for_display(fixed_between(&advisories, &version("0.51.0"), &version("0.52.0")));
    assert_eq!(
        shown,
        vec![
            SecurityAdvisory {
                id: "bad".to_string(),
                fixed_in: "0.52.0".to_string(),
                severity: Severity::High,
                url: None,
            },
            SecurityAdvisory {
                id: "ok".to_string(),
                fixed_in: "0.52.0".to_string(),
                severity: Severity::Low,
                url: Some("https://getcmdr.com/changelog".to_string()),
            },
        ]
    );
}

#[test]
fn fixed_since_ignores_a_since_that_isnt_semver() {
    assert!(embedded_fixed_since("not a version", env!("CARGO_PKG_VERSION")).is_empty());
}

/// The canary over the committed file, like `smoke_real_changelog_parses`: it must parse, every
/// link must pass the allowlist, and no `fixedIn` may be newer than this build.
#[test]
fn the_committed_list_parses_and_every_link_is_allowed() {
    let advisories = parse(EMBEDDED_ADVISORIES_JSON, "embedded").expect("the committed advisory list must parse");
    let current = version(env!("CARGO_PKG_VERSION"));
    for advisory in &advisories {
        assert!(
            is_allowed_advisory_url(&advisory.url),
            "{} links to {}, which the allowlist refuses",
            advisory.id,
            advisory.url
        );
        assert!(
            advisory.fixed_in <= current,
            "{} is fixed in {}, newer than this build ({current})",
            advisory.id,
            advisory.fixed_in
        );
    }
    assert_eq!(embedded().len(), advisories.len());
}

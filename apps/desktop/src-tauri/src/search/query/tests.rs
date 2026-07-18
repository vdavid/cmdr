use super::*;

// ── glob_to_regex ────────────────────────────────────────────────

#[test]
fn glob_to_regex_star() {
    assert_eq!(glob_to_regex("*.pdf"), r"^.*\.pdf$");
}

#[test]
fn glob_to_regex_question() {
    assert_eq!(glob_to_regex("file?.txt"), r"^file.\.txt$");
}

#[test]
fn glob_to_regex_escapes_metacharacters() {
    assert_eq!(glob_to_regex("a+b(c)"), r"^a\+b\(c\)$");
}

#[test]
fn glob_to_regex_literal() {
    assert_eq!(glob_to_regex("readme"), "^readme$");
}

// ── glob_to_regex (property-based) ───────────────────────────────
//
// The output of `glob_to_regex` is fed directly into `regex::Regex::new`
// by the search engine. A glob that escapes incorrectly would either
// panic the regex parser or silently match more than the user intended.
// These properties pin (a) the output is always a syntactically valid
// regex and (b) it matches the user's literal intent when no glob
// metacharacters are present.

mod glob_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// For any glob string, the produced regex compiles successfully
        /// and is anchored end-to-end.
        #[test]
        fn output_is_valid_anchored_regex(glob in ".*") {
            let pattern = glob_to_regex(&glob);
            prop_assert!(pattern.starts_with('^'), "regex must start with ^: {}", pattern);
            prop_assert!(pattern.ends_with('$'), "regex must end with $: {}", pattern);
            let compiled = regex::Regex::new(&pattern);
            prop_assert!(
                compiled.is_ok(),
                "regex must compile, got error for glob {:?}: {:?}",
                glob,
                compiled.err()
            );
        }

        /// For globs with no `*` or `?` (after the regex metachar set is
        /// taken into account), the compiled regex matches the original
        /// string literally and nothing else of different content.
        #[test]
        fn literal_globs_match_themselves(
            glob in "[A-Za-z0-9 ._+(){}\\[\\]^$|\\\\]{0,30}"
                .prop_filter("no glob metacharacters", |s: &String| {
                    !s.contains('*') && !s.contains('?')
                })
        ) {
            let pattern = glob_to_regex(&glob);
            let compiled = regex::Regex::new(&pattern).expect("must compile");
            prop_assert!(
                compiled.is_match(&glob),
                "regex {:?} must match its own literal glob {:?}",
                pattern, glob
            );
            // It must not match a string with a different last character.
            // Skip strings ending in `]` or other edge codepoints because
            // appending arbitrary content might collide with grapheme
            // clusters in surprising ways, so prepend instead.
            let modified = format!("X{glob}Y");
            prop_assert!(
                !compiled.is_match(&modified) || modified == glob,
                "regex for literal glob {:?} must not match longer {:?}",
                glob, modified
            );
        }

        /// For globs containing only `*` wildcards interleaved with
        /// literal segments, the compiled regex matches any string
        /// produced by replacing each `*` with the empty string OR an
        /// arbitrary literal segment.
        #[test]
        fn star_matches_arbitrary_content(
            prefix in "[A-Za-z0-9_]{0,5}",
            middle in "[A-Za-z0-9_]{0,10}",
            suffix in "[A-Za-z0-9_]{0,5}"
        ) {
            let glob = format!("{prefix}*{suffix}");
            let pattern = glob_to_regex(&glob);
            let compiled = regex::Regex::new(&pattern).expect("must compile");
            let candidate = format!("{prefix}{middle}{suffix}");
            prop_assert!(
                compiled.is_match(&candidate),
                "regex {:?} for glob {:?} must match {:?}",
                pattern, glob, candidate
            );
        }
    }
}

// ── summarize_query ──────────────────────────────────────────────

fn make_query(
    name_pattern: Option<&str>,
    pattern_type: PatternType,
    min_size: Option<u64>,
    max_size: Option<u64>,
    modified_after: Option<u64>,
    modified_before: Option<u64>,
    is_directory: Option<bool>,
) -> SearchQuery {
    SearchQuery {
        name_pattern: name_pattern.map(|s| s.to_string()),
        pattern_type,
        min_size,
        max_size,
        modified_after,
        modified_before,
        is_directory,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    }
}

#[test]
fn summarize_empty_query() {
    let q = make_query(None, PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q), "(all entries)");
}

#[test]
fn summarize_name_only() {
    let q = make_query(Some("tes"), PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q), "\"tes\"");
}

#[test]
fn summarize_glob_pattern() {
    let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q), "\"*.pdf\"");
}

#[test]
fn summarize_regex_pattern() {
    let q = make_query(Some("Q[1-4].*"), PatternType::Regex, None, None, None, None, None);
    assert_eq!(summarize_query(&q), "\"Q[1-4].*\" (regex)");
}

#[test]
fn summarize_size_min() {
    let q = make_query(None, PatternType::Glob, Some(2 * 1024 * 1024), None, None, None, None);
    assert_eq!(summarize_query(&q), "size >= 2 MB");
}

#[test]
fn summarize_size_max() {
    let q = make_query(None, PatternType::Glob, None, Some(500 * 1024), None, None, None);
    assert_eq!(summarize_query(&q), "size <= 500 KB");
}

#[test]
fn summarize_size_range() {
    let q = make_query(
        None,
        PatternType::Glob,
        Some(1024 * 1024),
        Some(5 * 1024 * 1024 * 1024),
        None,
        None,
        None,
    );
    assert_eq!(summarize_query(&q), "size 1 MB\u{2013}5 GB");
}

#[test]
fn summarize_date_after() {
    // 2025-01-01 00:00:00 UTC = 1735689600
    let q = make_query(None, PatternType::Glob, None, None, Some(1_735_689_600), None, None);
    assert_eq!(summarize_query(&q), "last mod after 2025-01-01");
}

#[test]
fn summarize_date_before() {
    // 2026-03-01 00:00:00 UTC = 1772265600
    let q = make_query(None, PatternType::Glob, None, None, None, Some(1_772_323_200), None);
    assert_eq!(summarize_query(&q), "last mod before 2026-03-01");
}

#[test]
fn summarize_date_range() {
    let q = make_query(
        None,
        PatternType::Glob,
        None,
        None,
        Some(1_735_689_600),
        Some(1_772_323_200),
        None,
    );
    assert_eq!(summarize_query(&q), "last mod 2025-01-01\u{2013}2026-03-01");
}

#[test]
fn summarize_dirs_only() {
    let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, Some(true));
    assert_eq!(summarize_query(&q), "\"*.pdf\", dirs only");
}

#[test]
fn summarize_files_only() {
    let q = make_query(None, PatternType::Glob, None, None, None, None, Some(false));
    assert_eq!(summarize_query(&q), "files only");
}

#[test]
fn summarize_combined() {
    let q = make_query(
        Some("tes"),
        PatternType::Glob,
        Some(2 * 1024 * 1024),
        None,
        None,
        Some(1_772_323_200),
        None,
    );
    assert_eq!(summarize_query(&q), "\"tes\", size >= 2 MB, last mod before 2026-03-01");
}

#[test]
fn summarize_size_bytes() {
    let q = make_query(None, PatternType::Glob, Some(500), None, None, None, None);
    assert_eq!(summarize_query(&q), "size >= 500 B");
}

#[test]
fn summarize_size_gb() {
    let q = make_query(
        None,
        PatternType::Glob,
        Some(1024 * 1024 * 1024),
        None,
        None,
        None,
        None,
    );
    assert_eq!(summarize_query(&q), "size >= 1 GB");
}

#[test]
fn summarize_empty_name_pattern() {
    let q = make_query(Some(""), PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q), "(all entries)");
}

// ── canonicalize_scope_path ─────────────────────────────────────

#[cfg(unix)]
#[test]
fn canonicalize_scope_path_resolves_symlinks() {
    use std::os::unix::fs::symlink;

    // A real dir with a child, plus a symlink pointing at the real dir — the
    // /tmp → /private/tmp shape the index stores canonically but scopes report
    // symlinked.
    let base = std::env::temp_dir().join(format!("cmdr-scope-canon-{}", std::process::id()));
    let real = base.join("real");
    std::fs::create_dir_all(real.join("child")).expect("create real dir");
    let link = base.join("link");
    let _ = std::fs::remove_file(&link);
    symlink(&real, &link).expect("create symlink");

    // A path THROUGH the symlink canonicalizes to the real path (both fully
    // symlink-resolved), so a scope typed against the symlink now matches the
    // index's stored real path.
    let through_link = link.join("child").to_string_lossy().into_owned();
    let want = std::fs::canonicalize(real.join("child"))
        .expect("canonicalize real")
        .to_string_lossy()
        .into_owned();
    assert_eq!(canonicalize_scope_path(&through_link), want);

    // A non-existent path keeps the literal (best-effort: the offline-index case).
    let missing = link.join("nope").to_string_lossy().into_owned();
    assert_eq!(canonicalize_scope_path(&missing), missing);

    std::fs::remove_dir_all(&base).ok();
}

// ── parse_scope ─────────────────────────────────────────────────

#[test]
fn parse_scope_basic_include() {
    let scope = parse_scope("~/projects");
    let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert!(scope.exclude_patterns.is_empty());
}

#[test]
fn parse_scope_basic_exclude() {
    let scope = parse_scope("!node_modules");
    assert!(scope.include_paths.is_empty());
    assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
}

#[test]
fn parse_scope_mixed() {
    let scope = parse_scope("~/projects, !node_modules, !.git");
    let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert_eq!(scope.exclude_patterns, vec!["node_modules", ".git"]);
}

#[test]
fn parse_scope_multiple_includes() {
    let scope = parse_scope("~/projects, ~/Documents");
    let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
    assert_eq!(
        scope.include_paths,
        vec![format!("{home}/projects"), format!("{home}/Documents")]
    );
}

#[test]
fn parse_scope_quoted_commas_double() {
    let scope = parse_scope("\"path,with,commas\"");
    assert_eq!(scope.include_paths, vec!["path,with,commas"]);
}

#[test]
fn parse_scope_quoted_commas_single() {
    let scope = parse_scope("'path,with,commas'");
    assert_eq!(scope.include_paths, vec!["path,with,commas"]);
}

#[test]
fn parse_scope_backslash_escaped_commas() {
    let scope = parse_scope("path\\,with\\,commas");
    assert_eq!(scope.include_paths, vec!["path,with,commas"]);
}

#[test]
fn parse_scope_empty_segments() {
    let scope = parse_scope("~/projects, , !.git");
    let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert_eq!(scope.exclude_patterns, vec![".git"]);
}

#[test]
fn parse_scope_bare_exclude_wildcard() {
    let scope = parse_scope("!.*");
    assert_eq!(scope.exclude_patterns, vec![".*"]);
}

#[test]
fn parse_scope_absolute_exclude_path() {
    let scope = parse_scope("!/Users/alice/Downloads");
    assert_eq!(scope.exclude_patterns, vec!["/Users/alice/Downloads"]);
}

#[test]
fn parse_scope_empty_input() {
    let scope = parse_scope("");
    assert!(scope.include_paths.is_empty());
    assert!(scope.exclude_patterns.is_empty());
}

#[test]
fn parse_scope_whitespace_trimming() {
    let scope = parse_scope("  ~/projects  ,  !node_modules  ");
    let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
}

// ── split_scope_segments + parse_scope (property-based) ──────────
//
// The scope parser has nested escape/quote rules. Property tests probe
// the round-trip and count invariants that don't require asserting a
// specific canonical form.

// ── resolve_include_scope (mount-relative scope resolution) ──────
//
// Regression tests for the two live-QA failures: a volume-root scope
// (`/Volumes/naspi`) must search the WHOLE volume, and a path that isn't in the
// index must be reported as unresolved rather than silently returning nothing.

use crate::indexing::ReadPool;
use crate::indexing::store::{IndexStore, ROOT_ID};

/// A mount-rooted index (SMB shape: `ROOT_ID` is the mount root) with a single
/// `/photos` folder, returned as a `ReadPool`. Mount root is a path that can't
/// exist, so `canonicalize_scope_path` keeps the literal deterministically.
fn mount_rooted_pool() -> (tempfile::TempDir, ReadPool) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-test.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::insert_entry_v2(&conn, ROOT_ID, "photos", true, false, None, None, None, None).unwrap();
    let pool = ReadPool::new(db_path).expect("read pool");
    (dir, pool)
}

const TEST_MOUNT: &str = "/Volumes/cmdr-test-nas";

#[test]
fn scope_at_volume_root_searches_whole_volume() {
    let (_dir, pool) = mount_rooted_pool();
    // The mount root itself strips to "/" ⇒ whole volume ⇒ NO include restriction
    // (empty ids), not `[i64::MIN]` (which would match nothing — the live bug).
    let r = resolve_include_scope(&[TEST_MOUNT.to_string()], &pool, Some(TEST_MOUNT));
    assert!(
        r.include_ids.is_empty(),
        "volume-root scope ⇒ no restriction (whole volume)"
    );
    assert!(r.unresolved.is_empty());
}

#[test]
fn scope_subpath_resolves_to_its_index_id() {
    let (_dir, pool) = mount_rooted_pool();
    let scope = format!("{TEST_MOUNT}/photos");
    let r = resolve_include_scope(&[scope], &pool, Some(TEST_MOUNT));
    // `/Volumes/cmdr-test-nas/photos` → strip mount → `/photos` → resolves.
    assert_eq!(r.include_ids.len(), 1);
    assert_ne!(r.include_ids[0], i64::MIN);
    assert!(r.unresolved.is_empty());
}

#[test]
fn scope_path_not_in_index_is_reported_unresolved() {
    let (_dir, pool) = mount_rooted_pool();
    let scope = format!("{TEST_MOUNT}/does-not-exist");
    let r = resolve_include_scope(std::slice::from_ref(&scope), &pool, Some(TEST_MOUNT));
    // Nothing resolved ⇒ match nothing AND surface the path honestly.
    assert_eq!(r.include_ids, vec![i64::MIN]);
    assert_eq!(r.unresolved, vec![scope]);
}

#[test]
fn scope_outside_mount_root_is_unresolved() {
    let (_dir, pool) = mount_rooted_pool();
    // A path not under the volume's mount root can't map into its index.
    let r = resolve_include_scope(&["/Volumes/other/x".to_string()], &pool, Some(TEST_MOUNT));
    assert_eq!(r.include_ids, vec![i64::MIN]);
    assert_eq!(r.unresolved, vec!["/Volumes/other/x".to_string()]);
}

mod scope_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// For inputs containing no special characters (no comma, no
        /// quotes, no backslash), the result is exactly `[input]` and
        /// the round-trip `segments.join(",") == input` holds.
        #[test]
        fn plain_input_round_trips(input in "[^,\"'\\\\]*") {
            let segments = split_scope_segments(&input);
            prop_assert_eq!(segments.len(), 1, "no commas → exactly 1 segment");
            prop_assert_eq!(&segments[0], &input, "the only segment must equal input");
            prop_assert_eq!(segments.join(","), input.clone(), "round-trip via join must match");
        }

        /// For inputs containing only safe characters and unquoted commas
        /// (no quotes, no backslashes), the segment count equals the
        /// comma count + 1.
        #[test]
        fn comma_count_matches_segment_count(input in "[^\"'\\\\]*") {
            let segments = split_scope_segments(&input);
            let comma_count = input.chars().filter(|&c| c == ',').count();
            prop_assert_eq!(
                segments.len(),
                comma_count + 1,
                "expected {} segments for input {:?}, got {:?}",
                comma_count + 1, input, segments
            );
            // And the join round-trips for this character class.
            prop_assert_eq!(segments.join(","), input);
        }

        /// `parse_scope` never panics, and the count of resolved
        /// include/exclude entries is bounded by the segment count.
        #[test]
        fn parse_scope_never_overcounts(input in ".*") {
            let scope = parse_scope(&input);
            let segments = split_scope_segments(&input);
            prop_assert!(
                scope.include_paths.len() + scope.exclude_patterns.len() <= segments.len(),
                "parse_scope produced more entries than segments: input={:?}, scope={:?}, segments={:?}",
                input, scope, segments
            );
        }
    }
}

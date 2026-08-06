//! The exclusion rules both evaluators share, and the live one in particular.

use super::*;
use crate::search::types::{PatternType, SearchQuery};

/// A query with no exclusions of its own and the system tier left at its default.
fn query() -> SearchQuery {
    SearchQuery {
        name_pattern: Some("report".to_string()),
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: Some(false),
        exclude_system_dirs: None,
    }
}

/// A query with only the excludes it names: the system tier is off, so a test of
/// a user exclude can't be passed by the tier catching the same name.
fn with_excludes(names: &[&str]) -> SearchQuery {
    SearchQuery {
        exclude_dir_names: Some(names.iter().map(|n| n.to_string()).collect()),
        exclude_system_dirs: Some(false),
        ..query()
    }
}

// ── The system tier ──────────────────────────────────────────────────

#[test]
fn the_system_tier_is_on_by_default_and_off_when_the_query_says_so() {
    // `excludeSystemDirs` absent means exclude (the dialog's default), and only an
    // explicit `false` turns the tier off — the same three-way reading the arena
    // scan has always had.
    let on = ExcludeRules::from_query(&query(), true);
    assert!(
        on.excludes_walked("/Users/me/app/node_modules/left-pad/index.js", None),
        "a file inside node_modules is out by default"
    );

    let off = ExcludeRules::from_query(
        &SearchQuery {
            exclude_system_dirs: Some(false),
            ..query()
        },
        true,
    );
    assert!(
        !off.excludes_walked("/Users/me/app/node_modules/left-pad/index.js", None),
        "and in when the query turns the tier off"
    );
    assert!(off.is_empty(), "with nothing else excluded, there's nothing to check");
}

#[test]
fn a_walked_entry_is_judged_by_its_ancestors_not_its_own_name() {
    // Excluding `node_modules` hides what's INSIDE it. The folder itself is a
    // legitimate hit for someone who searched for it, which is why the arena walk
    // starts at the parent — and why this one drops the leaf before looking.
    let rules = ExcludeRules::from_query(&query(), true);
    assert!(
        !rules.excludes_walked("/Users/me/app/node_modules", None),
        "the excluded directory itself is still a candidate"
    );
    assert!(
        rules.excludes_walked("/Users/me/app/node_modules/pkg", None),
        "but what sits inside it is not"
    );
}

// ── Case folding ─────────────────────────────────────────────────────

#[test]
fn case_folding_comes_from_the_compiled_query_both_ways() {
    // The rule arrives from `CompiledQuery::case_insensitive`; folding here again
    // is how an unindexed drive starts excluding under a different alphabet than
    // the pattern matched with.
    let insensitive = ExcludeRules::from_query(&with_excludes(&["Archive"]), true);
    assert!(insensitive.excludes_walked("/p/archive/out.o", None));

    let sensitive = ExcludeRules::from_query(&with_excludes(&["Archive"]), false);
    assert!(!sensitive.excludes_walked("/p/archive/out.o", None), "case matters now");
    assert!(sensitive.excludes_walked("/p/Archive/out.o", None));
}

#[test]
fn an_uppercase_directory_on_disk_is_excluded_under_a_case_insensitive_query() {
    // The set is KEYED through `fold`, so the lookup has to fold the same way. Off
    // macOS `normalize_for_comparison` is a deliberate no-op, so a lookup that
    // reached for it directly compared a raw name against a lowercased key: every
    // system exclude with a capital in it (`Caches`, `Logs`, `WebKit`, `.Trash`)
    // silently stopped excluding anything there.
    let rules = ExcludeRules::from_query(&with_excludes(&["caches"]), true);
    assert!(rules.excludes_walked("/p/Library/Caches/thing.db", None));

    let tier = ExcludeRules::from_query(&query(), true);
    assert!(
        tier.excludes_walked("/p/Library/Caches/thing.db", None),
        "the system tier too"
    );
}

// ── User excludes: globs and path prefixes ───────────────────────────

#[test]
fn a_wildcard_exclude_matches_a_directory_name_as_a_glob() {
    let rules = ExcludeRules::from_query(&with_excludes(&["*.bundle"]), true);
    assert!(rules.excludes_walked("/apps/Thing.bundle/Contents/Info.plist", None));
    assert!(!rules.excludes_walked("/apps/Thing.app/Contents/Info.plist", None));
}

#[test]
fn an_exclude_with_a_slash_is_a_path_prefix_in_the_space_the_walk_reports() {
    // A path exclude is absolute, so it has to be compared against the path the
    // user would recognize — mount-absolute on a share, not the mount-relative
    // form the index stores.
    let rules = ExcludeRules::from_query(&with_excludes(&["/Volumes/naspi/Photos"]), true);
    assert!(rules.excludes_walked("/Volumes/naspi/Photos/2019/a.jpg", Some("/Volumes/naspi")));
    assert!(!rules.excludes_walked("/Volumes/naspi/Docs/a.pdf", Some("/Volumes/naspi")));
}

#[test]
fn the_volume_root_is_where_the_ancestor_walk_stops() {
    // The arena's walk stops at the volume root, so a mount root that happens to
    // contain an excluded NAME can't exclude the whole share. Without the strip,
    // one unlucky mount point would empty every search on that drive.
    let rules = ExcludeRules::from_query(&with_excludes(&["archive"]), true);
    assert!(
        !rules.excludes_walked("/Volumes/archive/notes/a.txt", Some("/Volumes/archive")),
        "the mount root's own name is above the volume, so it isn't an ancestor"
    );
    assert!(
        rules.excludes_walked("/Volumes/archive/notes/archive/a.txt", Some("/Volumes/archive")),
        "a directory of that name INSIDE the volume still excludes"
    );
}

// ── Cheap-path guards ────────────────────────────────────────────────

#[test]
fn a_name_only_rule_set_never_materializes_a_path() {
    // The arena's evaluator reconstructs an entry's whole path ONLY for a path
    // prefix, and reconstruction is per candidate on a scan of millions. So a rule
    // set with no prefixes has to say so, or the hot path pays for a string it
    // would only throw away.
    let rules = ExcludeRules::from_query(&with_excludes(&["node_modules"]), true);
    assert!(!rules.has_path_prefixes());
    assert!(rules.has_name_rules());
}

#[test]
fn a_rule_set_with_no_names_never_walks_the_components() {
    // `has_name_rules` is what lets the live evaluator skip the component walk,
    // and a path-prefix-only rule set must still answer correctly.
    let rules = ExcludeRules::from_query(&with_excludes(&["/tmp/scratch"]), true);
    assert!(!rules.has_name_rules());
    assert!(rules.has_path_prefixes());
    assert!(rules.excludes_walked("/tmp/scratch/a.txt", None));
    assert!(!rules.excludes_walked("/tmp/keep/a.txt", None));
}

//! Component-aware absolute-path prefix helpers.
//!
//! Shared by the reconciler's rescan ancestor-collapse and the removal-storm
//! coalescing (`event_loop::storm`). All operate on already-canonical absolute
//! paths (`/a/b/c`, forward slashes, no trailing slash except the root `/`) and
//! compare by COMPONENT, never by raw substring — so `/a/bc` is never treated as
//! a child of `/a/b`.

use std::collections::HashSet;

/// Walk an absolute path's non-empty components. `/` yields nothing; `/a/b`
/// yields `"a"`, `"b"`.
///
/// ⚠️ An iterator, ❌ not a `Vec`: these helpers sit on the live event path, once
/// per filesystem event. Two heap allocations per comparison made this the single
/// most expensive thing about resuming an interrupted index
/// (`lifecycle/phases/DETAILS.md` § "What a resume costs"). Callers that genuinely
/// need a slice collect it themselves.
fn components(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|c| !c.is_empty())
}

/// Number of path components. `/` is 0, `/a/b` is 2. Used to sort shallow-first.
pub(crate) fn depth(path: &str) -> usize {
    components(path).count()
}

/// Whether `path` is a STRICT descendant of `prefix` (a proper sub-path, never
/// equal). Component-aware: `/a/b/c` is a descendant of `/a/b`, but `/a/bc` is
/// not. The root `/` is an ancestor of everything but itself.
pub(crate) fn is_strict_descendant(path: &str, prefix: &str) -> bool {
    let mut walked = components(path);
    for expected in components(prefix) {
        if walked.next() != Some(expected) {
            return false;
        }
    }
    // Every one of the prefix's components matched, so what decides it is whether
    // the path has anything left: equal paths are ❌ not descendants.
    walked.next().is_some()
}

/// Whether `path` IS `prefix` or sits under it, component-aware. The inclusive
/// half of [`is_strict_descendant`], for a caller asking "is this inside that
/// folder", where the folder itself counts.
pub(crate) fn is_at_or_under(path: &str, prefix: &str) -> bool {
    path == prefix || is_strict_descendant(path, prefix)
}

/// `path` itself, then each of its ancestors, deepest first, ending at `/`.
///
/// The lookup half of [`is_strict_descendant`] for a caller holding a SORTED or
/// keyed collection: instead of asking every entry whether it contains `path`,
/// ask the collection for each of these in turn. A path is a handful of
/// components deep however many entries the collection holds, so the answer stops
/// costing what the collection is worth (the branch set, `watch/branches.rs`).
///
/// Borrows `path` and allocates nothing: every item is a slice of it.
pub(crate) fn self_and_ancestors(path: &str) -> impl Iterator<Item = &str> {
    let mut next = Some(path);
    std::iter::from_fn(move || {
        let current = next?;
        next = match current.rfind('/') {
            // `/a` -> `/`. The volume root is a branch like any other and
            // contains everything, so the chain has to reach it.
            Some(0) if current.len() > 1 => Some("/"),
            Some(cut) if cut > 0 => Some(&current[..cut]),
            // `/` (and anything relative) has nowhere left to go.
            _ => None,
        };
        Some(current)
    })
}

/// The string every strict descendant of `path` starts with, and nothing else
/// does: `/a/b` becomes `/a/b/`, and the root is already its own.
///
/// The range half of [`is_strict_descendant`]: over keys held in sorted order,
/// `range(prefix..).take_while(starts_with(prefix))` is exactly the descendants,
/// found in the time they take to yield rather than the time the whole collection
/// takes to scan. The trailing separator is what keeps it component-aware, so
/// `/a/bc` never answers to `/a/b`.
pub(crate) fn descendant_range_prefix(path: &str) -> String {
    if path.ends_with('/') {
        return path.to_string();
    }
    format!("{path}/")
}

/// The path truncated to at most `max_depth` leading components. `/a/b/c/d`
/// capped at 2 is `/a/b`; a path already `<= max_depth` deep is returned as-is
/// (re-canonicalized). Used ONLY as a grouping key for removal-storm detection —
/// never as a rescan anchor (the anchor is the group's deepest common ancestor,
/// which may reach deeper than this cap).
pub(crate) fn capped_prefix(path: &str, max_depth: usize) -> String {
    let comps: Vec<&str> = components(path).collect();
    if comps.is_empty() {
        return "/".to_string();
    }
    let take = comps.len().min(max_depth);
    format!("/{}", comps[..take].join("/"))
}

/// The deepest common ancestor of a set of absolute paths, as an absolute path.
/// Component-wise longest common prefix: for `["/a/b/x", "/a/b/y"]` it's `/a/b`.
/// Returns `None` for an empty input; the root `/` when the paths share nothing.
pub(crate) fn deepest_common_ancestor<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut iter = paths.into_iter();
    let first = iter.next()?;
    let mut common: Vec<&str> = components(first).collect();
    for path in iter {
        let shared = common.iter().zip(components(path)).take_while(|(a, b)| *a == b).count();
        common.truncate(shared);
        if common.is_empty() {
            break;
        }
    }
    Some(if common.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", common.join("/"))
    })
}

/// Compute parent path from a normalized path.
pub(crate) fn compute_parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Expand origin directories to the recursive-size refresh set: every origin plus
/// every ancestor up to `/`, deduplicated.
///
/// The `index-dir-updated` emit and the "size updating" hourglass both need this
/// wider set (a file's size change propagates to every ancestor's `dir_stats`), so
/// it's rebuilt here, ONCE per drained batch over the deduplicated origins, rather
/// than per event. Consumers that care about which listings changed take the
/// origins instead.
pub(crate) fn with_ancestor_closure(origins: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(origins.len());
    let mut seen: HashSet<String> = HashSet::with_capacity(origins.len());
    let mut push = |path: String, out: &mut Vec<String>| {
        if seen.insert(path.clone()) {
            out.push(path);
        }
    };
    for origin in origins {
        push(origin.clone(), &mut out);
        for ancestor in collect_ancestor_paths(origin) {
            push(ancestor, &mut out);
        }
    }
    out
}

/// Collect all ancestor paths from the immediate parent up to "/".
/// Used to notify the frontend that dir_stats changed along the entire ancestor chain
/// (since propagate_delta updates all ancestors, not just the direct parent).
pub(crate) fn collect_ancestor_paths(path: &str) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut current = path.to_string();
    loop {
        let parent = compute_parent_path(&current);
        if parent.is_empty() || parent == current {
            break;
        }
        ancestors.push(parent.clone());
        if parent == "/" {
            break;
        }
        current = parent;
    }
    ancestors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_counts_components() {
        assert_eq!(depth("/"), 0);
        assert_eq!(depth("/a"), 1);
        assert_eq!(depth("/a/b/c"), 3);
    }

    #[test]
    fn strict_descendant_is_component_aware() {
        assert!(is_strict_descendant("/a/b/c", "/a/b"));
        assert!(is_strict_descendant("/a/b", "/a"));
        assert!(is_strict_descendant("/a/b", "/"));
        // Equal is NOT a strict descendant.
        assert!(!is_strict_descendant("/a/b", "/a/b"));
        // Substring-but-not-component is NOT a descendant.
        assert!(!is_strict_descendant("/a/bc", "/a/b"));
        // An ancestor is not a descendant of its child.
        assert!(!is_strict_descendant("/a", "/a/b"));
    }

    #[test]
    fn self_and_ancestors_walks_deepest_first_down_to_the_root() {
        let chain: Vec<&str> = self_and_ancestors("/a/b/c").collect();
        assert_eq!(chain, vec!["/a/b/c", "/a/b", "/a", "/"]);
        // The root is its own only candidate, and never yielded twice.
        assert_eq!(self_and_ancestors("/").collect::<Vec<_>>(), vec!["/"]);
        assert_eq!(self_and_ancestors("/a").collect::<Vec<_>>(), vec!["/a", "/"]);
        // Every step is a COMPONENT boundary, so `/a/bc` never yields `/a/b`.
        assert!(!self_and_ancestors("/a/bc").any(|p| p == "/a/b"));
        // The chain agrees with the predicate it stands in for: every entry after
        // the path itself is a strict ancestor of it, and nothing is missed.
        for candidate in self_and_ancestors("/a/b/c").skip(1) {
            assert!(is_strict_descendant("/a/b/c", candidate), "{candidate}");
        }
    }

    #[test]
    fn descendant_range_prefix_bounds_a_sorted_scan() {
        assert_eq!(descendant_range_prefix("/a/b"), "/a/b/");
        assert_eq!(descendant_range_prefix("/"), "/");
        // What the prefix is FOR: a `starts_with` test over sorted keys has to
        // agree with the component-aware predicate, sibling traps included.
        let prefix = descendant_range_prefix("/a/b");
        for key in ["/a/b/c", "/a/b/c/d"] {
            assert!(key.starts_with(&prefix) && is_strict_descendant(key, "/a/b"), "{key}");
        }
        for key in ["/a/b", "/a/bc", "/a", "/a/c"] {
            assert!(!key.starts_with(&prefix) && !is_strict_descendant(key, "/a/b"), "{key}");
        }
    }

    #[test]
    fn capped_prefix_truncates_and_passes_through() {
        assert_eq!(capped_prefix("/a/b/c/d", 2), "/a/b");
        assert_eq!(capped_prefix("/a/b/c/d", 8), "/a/b/c/d");
        assert_eq!(capped_prefix("/a", 2), "/a");
        assert_eq!(capped_prefix("/", 8), "/");
        // Component-exact truncation, never a raw byte cut.
        assert_eq!(capped_prefix("/aaa/bbb/ccc", 2), "/aaa/bbb");
    }

    #[test]
    fn deepest_common_ancestor_cases() {
        assert_eq!(deepest_common_ancestor(["/a/b/x", "/a/b/y"]), Some("/a/b".to_string()));
        assert_eq!(
            deepest_common_ancestor(["/a/b/c/d.rs", "/a/b/c/e.rs", "/a/b/c/sub/f.rs"]),
            Some("/a/b/c".to_string())
        );
        // Divergent trees share only the root.
        assert_eq!(deepest_common_ancestor(["/a/x", "/b/y"]), Some("/".to_string()));
        // A single path is its own DCA.
        assert_eq!(deepest_common_ancestor(["/a/b/c"]), Some("/a/b/c".to_string()));
        assert_eq!(deepest_common_ancestor(std::iter::empty::<&str>()), None);
    }

    #[test]
    fn compute_parent_path_cases() {
        assert_eq!(compute_parent_path("/Users/foo/bar.txt"), "/Users/foo");
        assert_eq!(compute_parent_path("/Users"), "/");
        assert_eq!(compute_parent_path("/"), "/");
    }

    /// The recursive-size refresh set is REBUILT from the origins, so the FE emit and
    /// the "size updating" hourglass keep seeing every ancestor up to `/` — the fact
    /// they genuinely need. Splitting the two facts must not narrow this one.
    #[test]
    fn ancestor_closure_rebuilds_the_recursive_size_refresh_set() {
        let closure = with_ancestor_closure(&["/Users/test/proj/pkg".to_string()]);
        let mut got: Vec<&str> = closure.iter().map(String::as_str).collect();
        got.sort_unstable();
        assert_eq!(
            got,
            vec!["/", "/Users", "/Users/test", "/Users/test/proj", "/Users/test/proj/pkg"],
            "the origin plus every ancestor up to the root"
        );

        // Two origins on the same chain fold into ONE closure, no duplicates.
        let shared = with_ancestor_closure(&["/a/b/c".to_string(), "/a/b".to_string()]);
        let mut got: Vec<&str> = shared.iter().map(String::as_str).collect();
        got.sort_unstable();
        assert_eq!(got, vec!["/", "/a", "/a/b", "/a/b/c"]);

        // The root itself has no ancestors and claims nothing beyond itself.
        assert_eq!(with_ancestor_closure(&["/".to_string()]), vec!["/".to_string()]);
        assert!(
            with_ancestor_closure(&[]).is_empty(),
            "an empty batch expands to nothing"
        );
    }
}

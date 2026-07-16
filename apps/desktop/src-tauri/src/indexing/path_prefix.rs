//! Component-aware absolute-path prefix helpers.
//!
//! Shared by the reconciler's rescan ancestor-collapse and the removal-storm
//! coalescing (`event_loop::storm`). All operate on already-canonical absolute
//! paths (`/a/b/c`, forward slashes, no trailing slash except the root `/`) and
//! compare by COMPONENT, never by raw substring — so `/a/bc` is never treated as
//! a child of `/a/b`.

/// Split an absolute path into its non-empty components. `/` yields an empty
/// slice; `/a/b` yields `["a", "b"]`.
fn components(path: &str) -> Vec<&str> {
    path.split('/').filter(|c| !c.is_empty()).collect()
}

/// Number of path components. `/` is 0, `/a/b` is 2. Used to sort shallow-first.
pub(crate) fn depth(path: &str) -> usize {
    components(path).len()
}

/// Whether `path` is a STRICT descendant of `prefix` (a proper sub-path, never
/// equal). Component-aware: `/a/b/c` is a descendant of `/a/b`, but `/a/bc` is
/// not. The root `/` is an ancestor of everything but itself.
pub(crate) fn is_strict_descendant(path: &str, prefix: &str) -> bool {
    let (pc, xc) = (components(path), components(prefix));
    if pc.len() <= xc.len() {
        return false;
    }
    pc[..xc.len()] == xc[..]
}

/// The path truncated to at most `max_depth` leading components. `/a/b/c/d`
/// capped at 2 is `/a/b`; a path already `<= max_depth` deep is returned as-is
/// (re-canonicalized). Used ONLY as a grouping key for removal-storm detection —
/// never as a rescan anchor (the anchor is the group's deepest common ancestor,
/// which may reach deeper than this cap).
pub(crate) fn capped_prefix(path: &str, max_depth: usize) -> String {
    let comps = components(path);
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
    let mut common: Vec<&str> = components(first);
    for path in iter {
        let comps = components(path);
        let shared = common.iter().zip(comps.iter()).take_while(|(a, b)| a == b).count();
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
}

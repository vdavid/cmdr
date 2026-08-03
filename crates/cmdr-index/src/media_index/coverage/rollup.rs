//! The subtree arithmetic both coverage caches roll their per-dir counts up with.
//!
//! A leaf, so neither the eligible denominator nor the accounted numerator has to
//! import the other for it.

use std::collections::HashMap;

use crate::media_index::paths::parent_dir;

/// Build a subtree rollup from a per-dir count map: every dir maps to the sum over
/// itself and all its descendant dirs. Each `(dir, count)` adds `count` to `dir` and
/// each of its ANCESTORS (so an ancestor dir holding no direct images still reports its
/// descendants' total), terminating at the root. Pure, so the arithmetic is
/// unit-testable.
pub(super) fn build_subtree_rollup(per_folder: &HashMap<String, u64>) -> HashMap<String, u64> {
    let mut rollup: HashMap<String, u64> = HashMap::new();
    for (dir, &count) in per_folder {
        let mut cursor = dir.as_str();
        loop {
            *rollup.entry(cursor.to_string()).or_default() += count;
            if cursor == "/" {
                break;
            }
            cursor = parent_dir(cursor);
        }
    }
    rollup
}

#[cfg(test)]
mod tests {
    use super::build_subtree_rollup;
    use std::collections::HashMap;

    #[test]
    fn build_subtree_rollup_sums_over_a_dir_and_its_descendants() {
        // `/a` holds no direct images but two descendant dirs do, so its subtree total is
        // their sum; the root `/` totals everything; a leaf reports only its own count.
        let per_folder: HashMap<String, u64> = [
            ("/a/b".to_string(), 2u64),
            ("/a/c".to_string(), 3),
            ("/x".to_string(), 1),
        ]
        .into_iter()
        .collect();
        let rollup = build_subtree_rollup(&per_folder);
        assert_eq!(rollup.get("/a/b").copied(), Some(2), "leaf is its own count");
        assert_eq!(rollup.get("/a/c").copied(), Some(3));
        assert_eq!(rollup.get("/a").copied(), Some(5), "/a rolls up its two child dirs");
        assert_eq!(rollup.get("/x").copied(), Some(1));
        assert_eq!(rollup.get("/").copied(), Some(6), "the root totals the whole volume");
        assert_eq!(rollup.get("/missing"), None, "a dir with nothing under it is absent");
    }
}

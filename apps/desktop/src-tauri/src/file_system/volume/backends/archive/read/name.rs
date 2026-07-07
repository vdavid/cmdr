//! Entry-name sanitization: the Zip Slip guarantee.
//!
//! Zip entry names are attacker-controlled and, per the format, can be any
//! bytes: `..` traversal, absolute paths, Windows `\` separators, drive
//! letters. Left as-is they'd let a malicious archive reference a path OUTSIDE
//! the archive root (Zip Slip, <https://snyk.io/research/zip-slip-vulnerability>).
//!
//! [`sanitize_entry_name`] normalizes every raw name into a safe inner path and
//! is the single choke point every entry passes through before it enters the
//! index. Its guarantee: **no [`SanitizedName::Accepted`] path, joined under any
//! root, escapes that root.** We enforce it at the index layer (not only at
//! extraction time) so an escaping path never even becomes a browsable entry —
//! defense in depth.
//!
//! Rules:
//! - `\` is normalized to `/` (Windows-authored archives).
//! - Empty and `.` path components are dropped, so leading, trailing, and
//!   doubled slashes collapse.
//! - Absolute paths are **clamped to the root** (leading slashes stripped), not
//!   rejected: the entry stays visible and can't escape. This matches `unzip`'s
//!   behavior for `/`-prefixed names.
//! - A `..` component is **quarantined** (rejected): unlike an absolute path, it
//!   can't be safely clamped to a single in-root location, so the whole entry is
//!   dropped rather than guessed at.
//! - A name that normalizes to nothing (`/`, `.`, ``) is quarantined as empty.
//! - A name with more than [`MAX_COMPONENT_DEPTH`] path components is quarantined
//!   as a depth bomb (see the constant's docs).

/// Maximum number of path components (nesting depth) an accepted entry may have.
///
/// The synthetic-tree builder materializes one node per ancestor prefix of every
/// entry, so a single entry named `a/a/a/…` with N components costs O(N) nodes
/// whose path strings sum to O(N²) bytes. A zip name field is a `u16`, so N can
/// reach ~32k in a 64 KB name — ~1 GB of ancestor strings from one entry, a
/// browse-time memory-amplification DoS. Capping the depth kills that quadratic
/// blowup at its source (the per-entry axis; the total-node-count backstop in
/// `index.rs` covers the many-entries axis).
///
/// 256 is an order of magnitude beyond any real archive's nesting (real trees
/// rarely pass ~40 deep), so a deeper entry is hostile, not legitimate — it's
/// quarantined, leaving the rest of the archive browsable.
pub const MAX_COMPONENT_DEPTH: usize = 256;

/// The result of sanitizing one raw entry name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SanitizedName {
    /// A safe inner path: `/`-separated, no leading or trailing slash, no `.`
    /// or `..` components, never empty. Safe to join under any root.
    Accepted(String),
    /// The name can't be safely placed inside the archive root and is dropped.
    Quarantined(QuarantineReason),
}

/// Why a raw entry name was quarantined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineReason {
    /// The name contains a `..` component (parent-directory traversal).
    ParentTraversal,
    /// The name normalizes to nothing (e.g. `/`, `.`, or empty).
    Empty,
    /// The name nests deeper than [`MAX_COMPONENT_DEPTH`] components (a depth
    /// bomb — see that constant).
    TooDeep,
}

/// Normalizes a raw zip entry name into a safe inner path. See the module docs
/// for the full rule set and the safety guarantee.
pub fn sanitize_entry_name(raw: &str) -> SanitizedName {
    // Windows-authored archives use `\` as the separator; normalize first so
    // component splitting and the `..` check see a single separator.
    let normalized = raw.replace('\\', "/");

    let mut components: Vec<&str> = Vec::new();
    for component in normalized.split('/') {
        match component {
            // Empty (leading/trailing/doubled slash) and `.` carry no path
            // information — drop them. Dropping a leading empty component is
            // exactly what clamps an absolute path to the root.
            "" | "." => {}
            // Traversal can't be safely clamped to one in-root location.
            ".." => return SanitizedName::Quarantined(QuarantineReason::ParentTraversal),
            other => components.push(other),
        }
    }

    if components.is_empty() {
        return SanitizedName::Quarantined(QuarantineReason::Empty);
    }
    // Cap nesting depth to defuse the ancestor-materialization bomb (see
    // MAX_COMPONENT_DEPTH). Checked here, the single choke point, so a depth
    // bomb never reaches the tree builder.
    if components.len() > MAX_COMPONENT_DEPTH {
        return SanitizedName::Quarantined(QuarantineReason::TooDeep);
    }
    SanitizedName::Accepted(components.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Component, Path};

    fn accepted(raw: &str) -> String {
        match sanitize_entry_name(raw) {
            SanitizedName::Accepted(p) => p,
            other => panic!("expected Accepted for {raw:?}, got {other:?}"),
        }
    }

    #[test]
    fn plain_relative_names_pass_through() {
        assert_eq!(accepted("readme.txt"), "readme.txt");
        assert_eq!(accepted("dir/sub/file.txt"), "dir/sub/file.txt");
    }

    #[test]
    fn trailing_and_doubled_slashes_collapse() {
        // A directory entry keeps its path; the trailing slash is dropped.
        assert_eq!(accepted("dir/"), "dir");
        assert_eq!(accepted("a//b///c"), "a/b/c");
    }

    #[test]
    fn backslashes_normalize_to_forward_slashes() {
        assert_eq!(accepted("dir\\sub\\file.txt"), "dir/sub/file.txt");
    }

    #[test]
    fn absolute_paths_are_clamped_to_root() {
        // Leading slash stripped, entry stays visible inside the archive root.
        assert_eq!(accepted("/etc/passwd"), "etc/passwd");
        assert_eq!(accepted("\\Windows\\system32"), "Windows/system32");
    }

    #[test]
    fn double_dot_components_are_quarantined() {
        // Pre-fix this would have passed a traversal name straight through.
        assert_eq!(
            sanitize_entry_name("../evil.txt"),
            SanitizedName::Quarantined(QuarantineReason::ParentTraversal)
        );
        assert_eq!(
            sanitize_entry_name("a/../../b"),
            SanitizedName::Quarantined(QuarantineReason::ParentTraversal)
        );
        assert_eq!(
            sanitize_entry_name("dir\\..\\..\\evil"),
            SanitizedName::Quarantined(QuarantineReason::ParentTraversal)
        );
    }

    #[test]
    fn name_at_the_depth_cap_is_accepted() {
        // Exactly MAX_COMPONENT_DEPTH components is still fine.
        let name = vec!["a"; MAX_COMPONENT_DEPTH].join("/");
        assert_eq!(accepted(&name), name);
    }

    #[test]
    fn name_beyond_the_depth_cap_is_quarantined() {
        // One component past the cap is a depth bomb.
        let name = vec!["a"; MAX_COMPONENT_DEPTH + 1].join("/");
        assert_eq!(
            sanitize_entry_name(&name),
            SanitizedName::Quarantined(QuarantineReason::TooDeep)
        );
    }

    #[test]
    fn dot_and_empty_names_are_quarantined_as_empty() {
        assert_eq!(
            sanitize_entry_name("/"),
            SanitizedName::Quarantined(QuarantineReason::Empty)
        );
        assert_eq!(
            sanitize_entry_name("."),
            SanitizedName::Quarantined(QuarantineReason::Empty)
        );
        assert_eq!(
            sanitize_entry_name(""),
            SanitizedName::Quarantined(QuarantineReason::Empty)
        );
    }

    #[test]
    fn double_dot_only_matches_whole_component() {
        // `..foo` and `foo..` are legitimate filenames, not traversal.
        assert_eq!(accepted("..foo"), "..foo");
        assert_eq!(accepted("foo..bar"), "foo..bar");
        assert_eq!(accepted("dir/..foo/x"), "dir/..foo/x");
    }

    /// The core Zip Slip invariant: every accepted path, joined under an
    /// arbitrary root, resolves to a location strictly inside that root. We
    /// assert it structurally — no `ParentDir` / `RootDir` / `Prefix`
    /// components survive — which holds regardless of the root.
    #[test]
    fn accepted_paths_never_escape_the_root() {
        let hostile = [
            "/etc/passwd",
            "\\\\server\\share\\file",
            "a/b/c",
            "dir\\sub\\file.txt",
            "..foo/bar",
            "/////deep",
        ];
        for raw in hostile {
            if let SanitizedName::Accepted(inner) = sanitize_entry_name(raw) {
                let joined = Path::new("/archive/root").join(&inner);
                for comp in joined.components() {
                    assert!(
                        matches!(comp, Component::Normal(_) | Component::RootDir),
                        "accepted path {inner:?} (from {raw:?}) produced unsafe component {comp:?}"
                    );
                }
                // And the inner path itself is relative with no traversal.
                assert!(!inner.starts_with('/'), "inner {inner:?} is absolute");
                assert!(
                    !inner.split('/').any(|c| c == ".." || c.is_empty()),
                    "inner {inner:?} has an unsafe component"
                );
            }
        }
    }
}

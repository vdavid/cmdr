//! macOS firmlink normalization.
//!
//! Parses `/usr/share/firmlinks` to build a prefix-replacement map that converts
//! Data-volume paths (for example, `/System/Volumes/Data/Users/foo`) to their
//! canonical firmlinked counterparts (`/Users/foo`).

use std::sync::LazyLock;

const FIRMLINKS_PATH: &str = "/usr/share/firmlinks";
const DATA_VOLUME_PREFIX: &str = "/System/Volumes/Data/";

/// Firmlink prefix pairs: `(data_volume_prefix, canonical_prefix)`.
///
/// Sorted by longest data-volume prefix first so that more-specific entries
/// match before less-specific ones (for example, `/System/Volumes/Data/usr/local`
/// before `/System/Volumes/Data/usr`).
static FIRMLINK_MAP: LazyLock<Vec<(String, String)>> = LazyLock::new(load_firmlinks);

/// Parse `/usr/share/firmlinks` and build prefix replacement pairs.
///
/// Each line has the format `{root_path}\t{relative_path}` where the data-volume
/// counterpart lives at `/System/Volumes/Data/{relative_path}`.
///
/// Returns an empty vec if the file doesn't exist or can't be read (non-macOS).
fn load_firmlinks() -> Vec<(String, String)> {
    let content = match std::fs::read_to_string(FIRMLINKS_PATH) {
        Ok(c) => c,
        Err(e) => {
            log::debug!("Could not read {FIRMLINKS_PATH}: {e}");
            return Vec::new();
        }
    };

    let mut pairs: Vec<(String, String)> = content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.splitn(2, '\t');
            let root_path = parts.next()?.trim();
            let relative = parts.next()?.trim();
            if root_path.is_empty() || relative.is_empty() {
                return None;
            }

            // Data-volume path: /System/Volumes/Data/{relative}
            let data_prefix = format!("{DATA_VOLUME_PREFIX}{relative}");
            // Canonical path: the root_path itself (for example, /Users)
            Some((data_prefix, root_path.to_string()))
        })
        .collect();

    // Sort by longest data_prefix first for correct longest-prefix matching
    pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    log::debug!("Loaded {} firmlink prefix pairs from {FIRMLINKS_PATH}", pairs.len());
    pairs
}

/// Normalize a path by replacing any `/System/Volumes/Data/` prefix with the
/// canonical firmlinked path.
///
/// Example: `/System/Volumes/Data/Users/foo` becomes `/Users/foo`.
/// Paths that don't match any firmlink are returned unchanged.
pub fn normalize_path(path: &str) -> String {
    if !path.starts_with(DATA_VOLUME_PREFIX) {
        return path.to_string();
    }

    for (data_prefix, canonical_prefix) in FIRMLINK_MAP.iter() {
        if path.starts_with(data_prefix.as_str()) {
            let suffix = &path[data_prefix.len()..];
            return format!("{canonical_prefix}{suffix}");
        }
    }

    path.to_string()
}

/// Check if a path is under `/System/Volumes/Data/` and matches a known firmlink,
/// meaning it would be a duplicate of the canonical firmlinked path during scanning.
pub fn is_data_volume_firmlink_duplicate(path: &str) -> bool {
    if !path.starts_with(DATA_VOLUME_PREFIX) {
        return false;
    }

    FIRMLINK_MAP
        .iter()
        .any(|(data_prefix, _)| path == data_prefix.as_str() || path.starts_with(&format!("{data_prefix}/")))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a test firmlink map from raw lines (same format as /usr/share/firmlinks).
    fn parse_test_lines(lines: &str) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = lines
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                let mut parts = line.splitn(2, '\t');
                let root_path = parts.next()?.trim();
                let relative = parts.next()?.trim();
                if root_path.is_empty() || relative.is_empty() {
                    return None;
                }
                let data_prefix = format!("{DATA_VOLUME_PREFIX}{relative}");
                Some((data_prefix, root_path.to_string()))
            })
            .collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        pairs
    }

    fn normalize_with(pairs: &[(String, String)], path: &str) -> String {
        if !path.starts_with(DATA_VOLUME_PREFIX) {
            return path.to_string();
        }
        for (data_prefix, canonical_prefix) in pairs {
            if path.starts_with(data_prefix.as_str()) {
                let suffix = &path[data_prefix.len()..];
                return format!("{canonical_prefix}{suffix}");
            }
        }
        path.to_string()
    }

    fn is_dup_with(pairs: &[(String, String)], path: &str) -> bool {
        if !path.starts_with(DATA_VOLUME_PREFIX) {
            return false;
        }
        pairs
            .iter()
            .any(|(data_prefix, _)| path == data_prefix.as_str() || path.starts_with(&format!("{data_prefix}/")))
    }

    #[test]
    fn parse_firmlinks_format() {
        let input = "/Users\tUsers\n/Library\tLibrary\n/usr/local\tusr/local\n";
        let pairs = parse_test_lines(input);
        assert_eq!(pairs.len(), 3);

        // Should be sorted by longest data prefix first
        assert!(pairs[0].0.len() >= pairs[1].0.len());
    }

    #[test]
    fn normalize_users_path() {
        let pairs = parse_test_lines("/Users\tUsers\n");
        assert_eq!(
            normalize_with(&pairs, "/System/Volumes/Data/Users/foo/bar"),
            "/Users/foo/bar"
        );
    }

    #[test]
    fn normalize_nested_firmlink() {
        let pairs = parse_test_lines("/usr/local\tusr/local\n");
        assert_eq!(
            normalize_with(&pairs, "/System/Volumes/Data/usr/local/bin/tool"),
            "/usr/local/bin/tool"
        );
    }

    #[test]
    fn normalize_exact_match() {
        let pairs = parse_test_lines("/Users\tUsers\n");
        assert_eq!(normalize_with(&pairs, "/System/Volumes/Data/Users"), "/Users");
    }

    #[test]
    fn normalize_no_match_passes_through() {
        let pairs = parse_test_lines("/Users\tUsers\n");
        // Path under Data volume but not matching any firmlink
        assert_eq!(
            normalize_with(&pairs, "/System/Volumes/Data/SomethingElse/file"),
            "/System/Volumes/Data/SomethingElse/file"
        );
    }

    #[test]
    fn normalize_non_data_volume_unchanged() {
        let pairs = parse_test_lines("/Users\tUsers\n");
        assert_eq!(normalize_with(&pairs, "/Users/foo"), "/Users/foo");
        assert_eq!(normalize_with(&pairs, "/tmp/test"), "/tmp/test");
    }

    #[test]
    fn is_duplicate_detects_firmlink_paths() {
        let pairs = parse_test_lines("/Users\tUsers\n/Library\tLibrary\n");

        assert!(is_dup_with(&pairs, "/System/Volumes/Data/Users"));
        assert!(is_dup_with(&pairs, "/System/Volumes/Data/Users/foo"));
        assert!(is_dup_with(&pairs, "/System/Volumes/Data/Library"));
        assert!(is_dup_with(&pairs, "/System/Volumes/Data/Library/Caches/stuff"));
    }

    #[test]
    fn is_duplicate_rejects_non_firmlink_paths() {
        let pairs = parse_test_lines("/Users\tUsers\n");

        assert!(!is_dup_with(&pairs, "/Users/foo"));
        assert!(!is_dup_with(&pairs, "/System/Volumes/Data/SomethingElse"));
        assert!(!is_dup_with(&pairs, "/tmp/file"));
    }

    #[test]
    fn empty_firmlinks_file() {
        let pairs = parse_test_lines("");
        assert!(pairs.is_empty());

        // Normalization should be identity with no firmlinks
        assert_eq!(
            normalize_with(&pairs, "/System/Volumes/Data/Users/foo"),
            "/System/Volumes/Data/Users/foo"
        );
        assert!(!is_dup_with(&pairs, "/System/Volumes/Data/Users/foo"));
    }

    #[test]
    fn longest_prefix_wins() {
        // /System/Library/Caches is more specific than /System
        // (though /System isn't actually a firmlink, this tests the sorting logic)
        let input = "/System/Library/Caches\tSystem/Library/Caches\n/opt\topt\n";
        let pairs = parse_test_lines(input);

        assert_eq!(
            normalize_with(&pairs, "/System/Volumes/Data/System/Library/Caches/foo"),
            "/System/Library/Caches/foo"
        );
    }

    // Integration test: uses the real LazyLock-loaded map on macOS
    #[test]
    fn real_firmlink_map_loads() {
        // Just verify the lazy static doesn't panic
        let _ = &*FIRMLINK_MAP;
    }

    #[test]
    fn normalize_path_uses_real_map() {
        // On macOS, /Users should be a known firmlink
        // On non-macOS, normalize_path is identity (empty map)
        let result = normalize_path("/System/Volumes/Data/Users/foo");
        // On macOS: should become /Users/foo
        // On non-macOS: stays unchanged (no firmlinks file)
        if cfg!(target_os = "macos") {
            assert_eq!(result, "/Users/foo");
        } else {
            assert_eq!(result, "/System/Volumes/Data/Users/foo");
        }
    }

    #[test]
    fn is_data_volume_firmlink_duplicate_uses_real_map() {
        if cfg!(target_os = "macos") {
            assert!(is_data_volume_firmlink_duplicate("/System/Volumes/Data/Users"));
            assert!(is_data_volume_firmlink_duplicate("/System/Volumes/Data/Users/test"));
        }
        // Always false for non-Data paths
        assert!(!is_data_volume_firmlink_duplicate("/Users/test"));
    }
}

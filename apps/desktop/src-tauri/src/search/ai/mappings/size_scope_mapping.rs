use crate::commands::file_system::expand_tilde;

use super::{GB, KB, MB};

// ── Size mapping ─────────────────────────────────────────────────────

/// Map a `size` enum value to a (min_size, max_size) byte range.
pub fn size_to_filter(s: &str) -> (Option<u64>, Option<u64>) {
    match s {
        "empty" => (None, Some(0)),
        "tiny" => (None, Some(100 * KB)),
        "small" => (None, Some(MB)),
        "large" => (Some(100 * MB), None),
        "huge" => (Some(GB), None),
        _ if s.starts_with('>') => parse_size_value(&s[1..]).map_or((None, None), |v| (Some(v), None)),
        _ if s.starts_with('<') => parse_size_value(&s[1..]).map_or((None, None), |v| (None, Some(v))),
        _ => (None, None),
    }
}

/// Parse a size string like "50mb", "1gb", "500kb" into bytes.
fn parse_size_value(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if let Some(num_str) = s.strip_suffix("gb") {
        let num: f64 = num_str.parse().ok()?;
        Some((num * GB as f64) as u64)
    } else if let Some(num_str) = s.strip_suffix("mb") {
        let num: f64 = num_str.parse().ok()?;
        Some((num * MB as f64) as u64)
    } else if let Some(num_str) = s.strip_suffix("kb") {
        let num: f64 = num_str.parse().ok()?;
        Some((num * KB as f64) as u64)
    } else {
        // Try parsing as plain bytes
        s.parse().ok()
    }
}

// ── Scope mapping ────────────────────────────────────────────────────

/// Result of scope resolution: paths to search in, plus an optional name prefix filter.
pub struct ScopeResult {
    pub paths: Vec<String>,
    pub name_prefix: Option<&'static str>,
}

/// Map a `scope` enum value to search paths and optional name prefix.
pub fn scope_to_paths(s: &str) -> ScopeResult {
    let home = dirs::home_dir().unwrap_or_default();
    match s {
        "downloads" => ScopeResult {
            paths: vec![home.join("Downloads").to_string_lossy().into_owned()],
            name_prefix: None,
        },
        "documents" => ScopeResult {
            paths: vec![home.join("Documents").to_string_lossy().into_owned()],
            name_prefix: None,
        },
        "desktop" => ScopeResult {
            paths: vec![home.join("Desktop").to_string_lossy().into_owned()],
            name_prefix: None,
        },
        "dotfiles" => ScopeResult {
            paths: vec![home.to_string_lossy().into_owned()],
            name_prefix: Some("."),
        },
        path => ScopeResult {
            paths: vec![expand_tilde(path)],
            name_prefix: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Size mapping ─────────────────────────────────────────────────

    #[test]
    fn size_all_enum_values() {
        assert_eq!(size_to_filter("empty"), (None, Some(0)));
        assert_eq!(size_to_filter("tiny"), (None, Some(100 * KB)));
        assert_eq!(size_to_filter("small"), (None, Some(MB)));
        assert_eq!(size_to_filter("large"), (Some(100 * MB), None));
        assert_eq!(size_to_filter("huge"), (Some(GB), None));
    }

    #[test]
    fn size_greater_than() {
        let (min, max) = size_to_filter(">50mb");
        assert_eq!(min, Some(50 * MB));
        assert!(max.is_none());
    }

    #[test]
    fn size_less_than() {
        let (min, max) = size_to_filter("<1gb");
        assert!(min.is_none());
        assert_eq!(max, Some(GB));
    }

    #[test]
    fn size_invalid_returns_none() {
        let (min, max) = size_to_filter("medium");
        assert!(min.is_none());
        assert!(max.is_none());
    }

    #[test]
    fn size_greater_than_gb() {
        let (min, max) = size_to_filter(">2gb");
        assert_eq!(min, Some(2 * GB));
        assert!(max.is_none());
    }

    // ── Scope mapping ────────────────────────────────────────────────

    #[test]
    fn scope_downloads() {
        let result = scope_to_paths("downloads");
        assert_eq!(result.paths.len(), 1);
        assert!(result.paths[0].ends_with("Downloads"));
        assert!(result.name_prefix.is_none());
    }

    #[test]
    fn scope_dotfiles() {
        let result = scope_to_paths("dotfiles");
        assert_eq!(result.name_prefix, Some("."));
    }

    #[test]
    fn scope_literal_path() {
        let result = scope_to_paths("~/projects");
        assert_eq!(result.paths.len(), 1);
        // Should have expanded tilde
        assert!(!result.paths[0].starts_with('~'));
    }
}

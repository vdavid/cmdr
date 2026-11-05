use super::{GIB, KIB, MIB};
use crate::query::{expand_tilde, parse_size};

// ── Size mapping ─────────────────────────────────────────────────────

/// Map a `size` enum value to a (min_size, max_size) byte range.
pub fn size_to_filter(s: &str) -> (Option<u64>, Option<u64>) {
    match s {
        "empty" => (None, Some(0)),
        "tiny" => (None, Some(100 * KIB)),
        "small" => (None, Some(MIB)),
        "large" => (Some(100 * MIB), None),
        "huge" => (Some(GIB), None),
        _ if s.starts_with('>') => parse_size(&s[1..]).map_or((None, None), |v| (Some(v), None)),
        _ if s.starts_with('<') => parse_size(&s[1..]).map_or((None, None), |v| (None, Some(v))),
        _ => (None, None),
    }
}

// ── Scope mapping ────────────────────────────────────────────────────

/// Result of scope resolution: paths to search in, plus an optional name prefix filter.
pub struct ScopeResult {
    pub paths: Vec<String>,
    pub name_prefix: Option<&'static str>,
}

/// Map a `scope` enum value to search paths and optional name prefix. `home` is the
/// user's home folder, the host's: with none, a named folder reads as relative and a
/// `~` path stays as typed.
pub fn scope_to_paths(s: &str, home: Option<&str>) -> ScopeResult {
    let base = home.unwrap_or("");
    match s {
        "downloads" => ScopeResult {
            paths: vec![join(base, "Downloads")],
            name_prefix: None,
        },
        "documents" => ScopeResult {
            paths: vec![join(base, "Documents")],
            name_prefix: None,
        },
        "desktop" => ScopeResult {
            paths: vec![join(base, "Desktop")],
            name_prefix: None,
        },
        "dotfiles" => ScopeResult {
            paths: vec![base.to_string()],
            name_prefix: Some("."),
        },
        path => ScopeResult {
            paths: vec![expand_tilde(path, home).into_owned()],
            name_prefix: None,
        },
    }
}

/// `home` joined with one folder name, the way `Path::join` does it (no doubled
/// `/`, an empty home leaves the name alone), in `&str` work so it reads the same in
/// a browser.
fn join(home: &str, name: &str) -> String {
    if home.is_empty() {
        name.to_string()
    } else if home.ends_with('/') {
        format!("{home}{name}")
    } else {
        format!("{home}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Size mapping ─────────────────────────────────────────────────

    #[test]
    fn size_all_enum_values() {
        assert_eq!(size_to_filter("empty"), (None, Some(0)));
        assert_eq!(size_to_filter("tiny"), (None, Some(100 * KIB)));
        assert_eq!(size_to_filter("small"), (None, Some(MIB)));
        assert_eq!(size_to_filter("large"), (Some(100 * MIB), None));
        assert_eq!(size_to_filter("huge"), (Some(GIB), None));
    }

    #[test]
    fn size_greater_than() {
        // MB is the SI megabyte, so the bound matches the "MB" the user and the UI write.
        let (min, max) = size_to_filter(">50mb");
        assert_eq!(min, Some(50_000_000));
        assert!(max.is_none());
    }

    #[test]
    fn size_less_than() {
        let (min, max) = size_to_filter("<1gb");
        assert!(min.is_none());
        assert_eq!(max, Some(1_000_000_000));
    }

    #[test]
    fn size_in_iec_units_is_base_1024() {
        assert_eq!(size_to_filter(">50mib").0, Some(50 * 1_048_576));
        assert_eq!(size_to_filter("<1gib").1, Some(1_073_741_824));
        assert_eq!(size_to_filter(">500kib").0, Some(512_000));
        assert_eq!(size_to_filter(">500kb").0, Some(500_000));
        assert_eq!(size_to_filter(">1tb").0, Some(1_000_000_000_000));
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
        assert_eq!(min, Some(2_000_000_000));
        assert!(max.is_none());
    }

    // ── Scope mapping ────────────────────────────────────────────────

    const HOME: Option<&str> = Some("/Users/test");

    #[test]
    fn scope_downloads() {
        let result = scope_to_paths("downloads", HOME);
        assert_eq!(result.paths, ["/Users/test/Downloads"]);
        assert!(result.name_prefix.is_none());
    }

    #[test]
    fn scope_dotfiles() {
        let result = scope_to_paths("dotfiles", HOME);
        assert_eq!(result.paths, ["/Users/test"]);
        assert_eq!(result.name_prefix, Some("."));
    }

    #[test]
    fn scope_literal_path() {
        let result = scope_to_paths("~/projects", HOME);
        assert_eq!(result.paths, ["/Users/test/projects"]);
    }

    #[test]
    fn a_named_folder_joins_the_home_the_way_a_path_join_does() {
        assert_eq!(scope_to_paths("documents", Some("/")).paths, ["/Documents"]);
        assert_eq!(scope_to_paths("desktop", None).paths, ["Desktop"]);
        assert_eq!(scope_to_paths("~/x", None).paths, ["~/x"]);
    }
}

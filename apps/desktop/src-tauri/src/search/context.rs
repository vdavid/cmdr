//! What a search needs from its host besides the query: the name rules, the home
//! folder, and the importance weights.
//!
//! All three used to be read ambiently (`cfg!(target_os)`, `dirs::home_dir()`, a
//! static map). As values they make the core host-independent: a test runs the macOS
//! rules on Linux, and a browser host that presents as a Mac says so instead of
//! silently taking the Linux answer its `cfg` would give it.

use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

use super::ranking::ImportanceWeights;

/// How a volume's names compare: the case rule a query gets when it doesn't say,
/// whether a typed pattern is NFD-normalized to meet the names it's matched against,
/// and the fold that keys a name for an exclusion (and, in an in-memory source, a
/// scope lookup).
///
/// ❌ Never derive any of these elsewhere: a second derivation that disagrees is how
/// one drive answers two ways (`matcher.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameRules {
    /// The case rule for a query whose `case_sensitive` is `None`.
    pub case_insensitive_by_default: bool,
    /// Whether a typed pattern (and the ranking stem) is NFD-normalized first, so it
    /// matches the decomposed names APFS stores without folding every candidate.
    pub nfd_patterns: bool,
    /// How a name is folded into a comparison key.
    pub fold: NameFold,
}

/// How a name becomes the key two spellings of it share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFold {
    /// NFD, then lowercase: how APFS compares names, and the key the macOS drive index
    /// stores (`cmdr_fs::name_fold::normalize_for_comparison`'s macOS branch).
    NfdLowercase,
    /// The name as it is: a case- and form-sensitive filesystem, where two names that
    /// differ in either are two files.
    Exact,
}

impl NameFold {
    /// `name` folded into its key.
    pub fn apply<'a>(self, name: &'a str) -> Cow<'a, str> {
        match self {
            Self::NfdLowercase => Cow::Owned(name.nfd().collect::<String>().to_lowercase()),
            Self::Exact => Cow::Borrowed(name),
        }
    }
}

impl NameRules {
    /// A Mac: case-insensitive by default, NFD patterns, the APFS fold.
    pub const MACOS: Self = Self {
        case_insensitive_by_default: true,
        nfd_patterns: true,
        fold: NameFold::NfdLowercase,
    };

    /// Linux: case-sensitive by default, patterns as typed, no fold.
    pub const LINUX: Self = Self {
        case_insensitive_by_default: false,
        nfd_patterns: false,
        fold: NameFold::Exact,
    };

    /// The rules of the machine this build targets: what the desktop app has always
    /// answered, decided once here instead of at each `cfg!` it replaces.
    pub const fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::MACOS
        } else {
            Self::LINUX
        }
    }

    /// `pattern` in the form names are matched in: NFD under [`Self::nfd_patterns`],
    /// as typed otherwise.
    pub fn normalize_pattern<'a>(&self, pattern: &'a str) -> Cow<'a, str> {
        if self.nfd_patterns {
            Cow::Owned(pattern.nfd().collect())
        } else {
            Cow::Borrowed(pattern)
        }
    }
}

/// Everything one search reads besides the query and the rows.
#[derive(Debug, Clone, Copy)]
pub struct SearchContext<'a> {
    /// The volume's name rules.
    pub rules: NameRules,
    /// The absolute home folder, written as `~` in a result's parent path; `None`
    /// when the host has none.
    pub home_dir: Option<&'a str>,
    /// Folder importance for ranking; empty for an unscored volume.
    pub weights: &'a ImportanceWeights,
}

impl<'a> SearchContext<'a> {
    /// This machine's rules with no home folder: the context the engine's own tests
    /// and benches search under.
    #[cfg(test)]
    pub(crate) fn for_test(weights: &'a ImportanceWeights) -> Self {
        Self {
            rules: NameRules::host(),
            home_dir: None,
            weights,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rules are a value now, so what this machine always answered has to be what
    /// `host()` says: the fold above all, which the drive index keys `name_folded` on.
    #[test]
    fn the_host_rules_are_what_this_machine_always_answered() {
        let host = NameRules::host();
        assert_eq!(host.case_insensitive_by_default, cfg!(target_os = "macos"));
        assert_eq!(host.nfd_patterns, cfg!(target_os = "macos"));
        for name in [
            "Report.PDF",
            "caf\u{e9}",
            "cafe\u{301}",
            "\u{c5}ngstr\u{f6}m",
            "line\nbreak",
            "",
        ] {
            assert_eq!(
                host.fold.apply(name),
                cmdr_fs::name_fold::normalize_for_comparison(name),
                "{name:?}"
            );
        }
    }

    #[test]
    fn the_mac_fold_is_nfd_then_lowercase_and_linux_keeps_the_name() {
        assert_eq!(NameRules::MACOS.fold.apply("Caf\u{e9}"), "cafe\u{301}");
        assert_eq!(NameRules::LINUX.fold.apply("Caf\u{e9}"), "Caf\u{e9}");
        assert_eq!(NameRules::MACOS.normalize_pattern("caf\u{e9}"), "cafe\u{301}");
        assert_eq!(NameRules::LINUX.normalize_pattern("caf\u{e9}"), "caf\u{e9}");
    }
}

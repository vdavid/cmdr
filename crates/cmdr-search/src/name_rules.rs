//! How a volume's names compare: the case default, which Unicode forms a pattern
//! takes, and the fold that keys a name.
//!
//! A leaf: the matcher, the exclusion set, ranking, and the in-memory source all read
//! it, and it reads none of them. [`SearchContext`](super::context::SearchContext)
//! carries it to a search.

use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

/// How a volume's names compare: the case rule a query gets when it doesn't say,
/// whether a typed pattern matches a name stored in either Unicode normal form, and
/// the fold that keys a name for an exclusion (and, in an in-memory source, a
/// scope lookup).
///
/// ❌ Never derive any of these elsewhere: a second derivation that disagrees is how
/// one drive answers two ways (`matcher.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameRules {
    /// The case rule for a query whose `case_sensitive` is `None`.
    pub case_insensitive_by_default: bool,
    /// Whether a typed pattern (and the ranking stem) meets a name in EITHER Unicode
    /// normal form: APFS keeps whichever form the writer used (Finder writes NFD, most
    /// downloads and other systems' files NFC), so the pattern compiles in both
    /// rather than every candidate being normalized per row.
    pub either_normal_form: bool,
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
    /// A Mac: case-insensitive by default, a pattern in either normal form, the APFS
    /// fold.
    pub const MACOS: Self = Self {
        case_insensitive_by_default: true,
        either_normal_form: true,
        fold: NameFold::NfdLowercase,
    };

    /// Linux: case-sensitive by default, patterns as typed, no fold.
    pub const LINUX: Self = Self {
        case_insensitive_by_default: false,
        either_normal_form: false,
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

    /// `pattern` in every form a name it should find may be stored in.
    ///
    /// Under [`Self::either_normal_form`] that's NFD first (Finder's form, the common
    /// one on a Mac, so the first try usually answers), then NFC when it differs. As
    /// typed, alone, otherwise. An ASCII pattern has one form, so it never pays for a
    /// second regex.
    pub(crate) fn pattern_forms<'a>(&self, pattern: &'a str) -> PatternForms<'a> {
        if !self.either_normal_form || pattern.is_ascii() {
            return PatternForms {
                primary: Cow::Borrowed(pattern),
                other: None,
            };
        }
        let nfd: String = pattern.nfd().collect();
        let nfc: String = pattern.nfc().collect();
        let other = (nfc != nfd).then_some(nfc);
        PatternForms {
            primary: Cow::Owned(nfd),
            other,
        }
    }
}

/// A pattern in the forms [`NameRules::pattern_forms`] gives it: always one, and a
/// second only when the volume stores either form and the two differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatternForms<'a> {
    /// The form tried first.
    pub(crate) primary: Cow<'a, str>,
    /// The other normal form, when it spells the pattern differently.
    pub(crate) other: Option<String>,
}

impl PatternForms<'_> {
    /// Every form, primary first.
    #[cfg(test)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.primary.as_ref()).chain(self.other.as_deref())
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
        assert_eq!(host.either_normal_form, cfg!(target_os = "macos"));
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
    }

    #[test]
    fn a_mac_pattern_takes_both_forms_and_linux_keeps_the_one_typed() {
        for typed in ["caf\u{e9}", "cafe\u{301}"] {
            let mac = NameRules::MACOS.pattern_forms(typed);
            assert_eq!(
                mac.iter().collect::<Vec<_>>(),
                ["cafe\u{301}", "caf\u{e9}"],
                "{typed:?}"
            );
            let linux = NameRules::LINUX.pattern_forms(typed);
            assert_eq!((linux.primary.as_ref(), linux.other), (typed, None));
        }
        // ASCII, or a non-ASCII letter with no decomposition, has one form.
        assert_eq!(NameRules::MACOS.pattern_forms("report").other, None);
        assert_eq!(NameRules::MACOS.pattern_forms("\u{3a9}mega").other, None);
    }
}

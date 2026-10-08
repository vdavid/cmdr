//! `MaxUpdateVersion`: "never update past this version", parsed once and compared on the release
//! core. Grammar and examples: `DETAILS.md` § Key catalog.

use std::fmt;

/// The highest version an organization lets this Mac update to.
///
/// How many parts the admin wrote decides the bound: `"0.52"` allows every 0.52.x (core
/// `< 0.53.0`), `"0.52.3"` allows up to and including 0.52.3, and `"1"` allows anything below 2.0.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCeiling {
    Major(u64),
    Minor(u64, u64),
    Patch(u64, u64, u64),
}

impl UpdateCeiling {
    /// One to three dot-separated non-negative integers, an optional leading `v`, nothing else.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let numbers = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let parts = numbers
            .split('.')
            .map(|part| {
                // Digits only: `u64::from_str` would also take a leading `+`.
                if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                part.parse::<u64>().ok()
            })
            .collect::<Option<Vec<u64>>>()?;
        match parts[..] {
            [major] => Some(Self::Major(major)),
            [major, minor] => Some(Self::Minor(major, minor)),
            [major, minor, patch] => Some(Self::Patch(major, minor, patch)),
            _ => None,
        }
    }

    /// The plist forms an admin can write: a string in the grammar above, or a non-negative
    /// integer, read as a major. ❌ Never a `<real>`: `0.5` and `0.50` are the same real, so the
    /// admin's intent is already lost.
    pub fn from_plist(value: &plist::Value) -> Option<Self> {
        match value {
            plist::Value::String(text) => Self::parse(text),
            plist::Value::Integer(n) => n.as_unsigned().map(Self::Major),
            _ => None,
        }
    }

    /// Whether `version` is at or below the ceiling. Compares the release core only
    /// (`major.minor.patch`): in semver `0.53.0-rc.1 < 0.53.0`, so a full compare would let a 0.53
    /// prerelease through a `"0.52"` ceiling. Only the macOS updater asks, so it exists only there.
    #[cfg(any(target_os = "macos", test))]
    pub fn allows(&self, version: &semver::Version) -> bool {
        let core = (version.major, version.minor, version.patch);
        // Comparing only the parts the admin wrote is the "up to the last x.y.*" reading.
        match *self {
            Self::Major(major) => core.0 <= major,
            Self::Minor(major, minor) => (core.0, core.1) <= (major, minor),
            Self::Patch(major, minor, patch) => core <= (major, minor, patch),
        }
    }
}

impl fmt::Display for UpdateCeiling {
    /// The canonical spelling, as the admin meant it: `0.52`, `0.52.3`, `1`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Major(major) => write!(f, "{major}"),
            Self::Minor(major, minor) => write!(f, "{major}.{minor}"),
            Self::Patch(major, minor, patch) => write!(f, "{major}.{minor}.{patch}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> semver::Version {
        semver::Version::parse(text).expect("a valid test version")
    }

    #[test]
    fn the_grammar_takes_one_to_three_parts_and_an_optional_v() {
        assert_eq!(UpdateCeiling::parse("0.52"), Some(UpdateCeiling::Minor(0, 52)));
        assert_eq!(UpdateCeiling::parse("0.52.3"), Some(UpdateCeiling::Patch(0, 52, 3)));
        assert_eq!(UpdateCeiling::parse("1"), Some(UpdateCeiling::Major(1)));
        assert_eq!(UpdateCeiling::parse("v0.52"), Some(UpdateCeiling::Minor(0, 52)));
        assert_eq!(UpdateCeiling::parse("  0.52 "), Some(UpdateCeiling::Minor(0, 52)));
    }

    #[test]
    fn anything_else_is_unparseable() {
        for text in [
            "",
            "latest",
            "0.52.3.1",
            "0..52",
            ".52",
            "0.52.",
            "-1",
            "+1",
            "0.52-rc.1",
            "vv1",
            "v",
            "1 2",
        ] {
            assert_eq!(UpdateCeiling::parse(text), None, "{text:?} must not parse");
        }
    }

    #[test]
    fn an_integer_is_a_major_and_a_real_is_refused() {
        assert_eq!(
            UpdateCeiling::from_plist(&plist::Value::Integer(1.into())),
            Some(UpdateCeiling::Major(1))
        );
        assert_eq!(UpdateCeiling::from_plist(&plist::Value::Integer((-1).into())), None);
        assert_eq!(UpdateCeiling::from_plist(&plist::Value::Real(0.52)), None);
        assert_eq!(
            UpdateCeiling::from_plist(&plist::Value::String("0.52".into())),
            Some(UpdateCeiling::Minor(0, 52))
        );
        assert_eq!(UpdateCeiling::from_plist(&plist::Value::Boolean(true)), None);
    }

    #[test]
    fn a_minor_ceiling_allows_every_patch_of_that_minor() {
        let ceiling = UpdateCeiling::Minor(0, 52);
        assert!(ceiling.allows(&version("0.52.0")));
        assert!(ceiling.allows(&version("0.52.99")));
        assert!(ceiling.allows(&version("0.51.7")));
        assert!(!ceiling.allows(&version("0.53.0")));
        assert!(!ceiling.allows(&version("1.0.0")));
    }

    #[test]
    fn a_prerelease_of_the_next_minor_is_refused() {
        assert!(!UpdateCeiling::Minor(0, 52).allows(&version("0.53.0-rc.1")));
        assert!(!UpdateCeiling::Major(0).allows(&version("1.0.0-beta.1")));
        assert!(!UpdateCeiling::Patch(0, 52, 3).allows(&version("0.52.4-rc.1")));
    }

    #[test]
    fn a_patch_ceiling_is_inclusive() {
        let ceiling = UpdateCeiling::Patch(0, 52, 3);
        assert!(ceiling.allows(&version("0.52.3")));
        assert!(ceiling.allows(&version("0.52.3+build.7")));
        assert!(!ceiling.allows(&version("0.52.4")));
    }

    #[test]
    fn a_major_ceiling_allows_everything_below_the_next_major() {
        let ceiling = UpdateCeiling::Major(1);
        assert!(ceiling.allows(&version("1.99.99")));
        assert!(!ceiling.allows(&version("2.0.0")));
    }

    #[test]
    fn it_prints_as_the_admin_meant_it() {
        assert_eq!(UpdateCeiling::Minor(0, 52).to_string(), "0.52");
        assert_eq!(UpdateCeiling::Patch(0, 52, 3).to_string(), "0.52.3");
        assert_eq!(UpdateCeiling::Major(1).to_string(), "1");
    }
}

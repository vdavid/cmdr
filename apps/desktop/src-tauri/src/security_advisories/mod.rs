//! The committed list of fixed vulnerabilities users should know about
//! (`resources/security-advisories.json`), and the questions asked of it.
//!
//! Two readers, two copies of the same file:
//!
//! - The macOS updater reads the copy inside a STAGED bundle, once its code signature and version
//!   have passed (`updater/advisories.rs`), so the restart toast can say the update fixes a
//!   security issue.
//! - The What's new popup reads this build's own copy, embedded below, so the notice still reaches
//!   someone who never saw the toast.
//!
//! The list is cumulative, so a jump from 0.50 to 0.55 still learns about a fix shipped in 0.52.
//! Resilience over strictness: a list that doesn't parse is "no advisory" plus a warn, never an
//! error, because the update carrying it is the fix itself. See `CLAUDE.md`.

use std::sync::OnceLock;

use semver::Version;
use serde::{Deserialize, Serialize};
use specta::Type;

/// The list as committed, embedded at build time. Runs up from `src/security_advisories/` to
/// `src-tauri/`; moving either breaks the build, which is the good failure mode.
const EMBEDDED_ADVISORIES_JSON: &str = include_str!("../../resources/security-advisories.json");

/// How severe a fixed vulnerability is: the CVSS v4.0 rating its public note carries. Ordered, so
/// `max` is the most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// The file's shape. `deny_unknown_fields` so a typo'd key reads as a malformed list rather than
/// as an advisory silently missing its link.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvisoryFile {
    advisories: Vec<RawAdvisory>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAdvisory {
    id: String,
    fixed_in: String,
    severity: Severity,
    url: String,
}

/// One fixed vulnerability, as the list names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advisory {
    /// Stable id: the GHSA id, or a date slug when the fix has no GitHub advisory.
    pub id: String,
    /// The release that fixed it.
    pub fixed_in: Version,
    pub severity: Severity,
    /// The public note. Only ever shown after [`is_allowed_advisory_url`] says yes.
    pub url: String,
}

/// One advisory as the What's new popup shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecurityAdvisory {
    pub id: String,
    pub fixed_in: String,
    pub severity: Severity,
    /// `None` when the list names a link the allowlist refuses: the notice still shows, without it.
    pub url: Option<String>,
}

/// Parses a list, or answers `None` (logged at warn) when it doesn't parse. `source` names which
/// copy, for the log.
pub fn parse(json: &str, source: &str) -> Option<Vec<Advisory>> {
    let file: AdvisoryFile = match serde_json::from_str(json) {
        Ok(file) => file,
        Err(e) => {
            // allowed-log-private: `source` is a fixed label for where the list came from
            log::warn!(target: "security_advisories", "The {source} security advisory list doesn't parse, so it names no advisory: {e}");
            return None;
        }
    };
    file.advisories
        .into_iter()
        .map(|raw| match Version::parse(&raw.fixed_in) {
            Ok(fixed_in) => Some(Advisory {
                id: raw.id,
                fixed_in,
                severity: raw.severity,
                url: raw.url,
            }),
            Err(e) => {
                // allowed-log-private: `source` is a fixed label for where the list came from
                log::warn!(target: "security_advisories", "The {source} security advisory list names {} as fixed in {:?}, which isn't semver ({e}), so it names no advisory", raw.id, raw.fixed_in);
                None
            }
        })
        .collect()
}

/// The advisories fixed in `after < fixedIn <= upto`, most severe first (then newest, then by id,
/// so the order is stable).
pub fn fixed_between<'a>(advisories: &'a [Advisory], after: &Version, upto: &Version) -> Vec<&'a Advisory> {
    let mut fixed: Vec<&Advisory> = advisories
        .iter()
        .filter(|advisory| *after < advisory.fixed_in && advisory.fixed_in <= *upto)
        .collect();
    fixed.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| b.fixed_in.cmp(&a.fixed_in))
            .then_with(|| a.id.cmp(&b.id))
    });
    fixed
}

/// Where a notice may link: the host, and the path the link must sit under. A GitHub advisory
/// needs an id after its prefix; the changelog may be the page itself or anything below it.
///
/// Both repo names pass: the repo moved from `vdavid/cmdr` to `getcmdr/cmdr` (#476).
/// The update toast vets links with the RUNNING app's list, so a name has to ship here before any
/// advisory uses it.
const ALLOWED_ADVISORY_PLACES: [(&str, &str, AllowedPath); 3] = [
    ("github.com", "/vdavid/cmdr/security/advisories/", AllowedPath::Below),
    ("github.com", "/getcmdr/cmdr/security/advisories/", AllowedPath::Below),
    ("getcmdr.com", "/changelog", AllowedPath::ItselfOrBelow),
];

#[derive(Clone, Copy)]
enum AllowedPath {
    /// Strictly below the prefix (which ends in `/`).
    Below,
    /// The prefix itself, or below it.
    ItselfOrBelow,
}

impl AllowedPath {
    fn admits(self, prefix: &str, path: &str) -> bool {
        match self {
            Self::Below => path.len() > prefix.len() && path.starts_with(prefix),
            Self::ItselfOrBelow => {
                path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
            }
        }
    }
}

/// Whether `raw` is a link a security notice may open: exactly `https`, no userinfo or port, and
/// under one of the allowed places, Cmdr's GitHub security advisories or the getcmdr.com changelog. The
/// URL must already be in canonical form, so nothing that only reads right before normalization
/// (a `..` segment, mixed case, a backslash) passes. The `desktop-security-advisories` check holds
/// the committed list to the same rule.
pub fn is_allowed_advisory_url(raw: &str) -> bool {
    let Ok(url) = url::Url::parse(raw) else {
        return false;
    };
    if url.as_str() != raw
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    let (Some(url::Host::Domain(host)), path) = (url.host(), url.path()) else {
        return false;
    };
    ALLOWED_ADVISORY_PLACES
        .iter()
        .any(|(allowed_host, prefix, rule)| host == *allowed_host && rule.admits(prefix, path))
}

/// The link a notice may carry for `advisory`, or `None` (logged at warn) when the allowlist
/// refuses it. Defense in depth: the list is signed, and the check and the canary test vet it too.
pub fn vetted_link(advisory: &Advisory) -> Option<String> {
    if is_allowed_advisory_url(&advisory.url) {
        Some(advisory.url.clone())
    } else {
        // allowed-log-private: an advisory URL from our own feed
        log::warn!(target: "security_advisories", "Not linking security advisory {}: {:?} isn't an allowed advisory link", advisory.id, advisory.url);
        None
    }
}

/// The popup's view of `advisories`, each link vetted.
pub fn for_display(advisories: Vec<&Advisory>) -> Vec<SecurityAdvisory> {
    advisories
        .into_iter()
        .map(|advisory| SecurityAdvisory {
            id: advisory.id.clone(),
            fixed_in: advisory.fixed_in.to_string(),
            severity: advisory.severity,
            url: vetted_link(advisory),
        })
        .collect()
}

/// This build's own list, parsed once. Empty when it doesn't parse (the canary test keeps the
/// committed file from getting there).
pub fn embedded() -> &'static [Advisory] {
    static CACHE: OnceLock<Vec<Advisory>> = OnceLock::new();
    CACHE.get_or_init(|| parse(EMBEDDED_ADVISORIES_JSON, "embedded").unwrap_or_default())
}

/// The advisories this build's list says were fixed in `since < fixedIn <= current`, most severe
/// first. A `since` or `current` that isn't semver answers nothing.
pub fn embedded_fixed_since(since: &str, current: &str) -> Vec<SecurityAdvisory> {
    let (Ok(since), Ok(current)) = (Version::parse(since.trim()), Version::parse(current.trim())) else {
        return vec![];
    };
    for_display(fixed_between(embedded(), &since, &current))
}

#[cfg(test)]
mod tests;

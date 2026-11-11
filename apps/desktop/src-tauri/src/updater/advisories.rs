//! The security notice an install carries, read from the STAGED bundle's own advisory list once
//! `installer::vet_staged_bundle` trusts that bundle.
//!
//! ❗ Never from `latest.json`: the manifest isn't signed, so a field there would let whoever can
//! serve it show a fake "critical" notice with a phishing link, or hide a real one. The staged
//! bundle's `Contents/Resources/` is covered by both the minisign signature and the Developer ID
//! code signature. `DETAILS.md` § Security notices.

use std::path::Path;

use semver::Version;

use crate::security_advisories::{self, Severity};

/// Where the list sits inside a bundle (`tauri.macos.conf.json` → `bundle.macOS.files`).
const BUNDLE_ADVISORIES_PATH: &str = "Contents/Resources/security-advisories.json";

/// What the restart toast says about security: the update fixes `count` advisories, the most
/// severe of them `severity`, whose public note is `url`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SecurityNotice {
    pub severity: Severity,
    pub count: u32,
    /// The most severe advisory's link, or `None` when the allowlist refuses it.
    pub url: Option<String>,
}

/// The notice for moving from `running` to `staged`, from the staged bundle's list `json`: the
/// advisories in `running < fixedIn <= staged`, or `None` when there are none or the list doesn't
/// parse (logged at warn by the parser).
pub(super) fn notice_from(json: &str, running: &Version, staged: &Version) -> Option<SecurityNotice> {
    let advisories = security_advisories::parse(json, "staged bundle's")?;
    let fixed = security_advisories::fixed_between(&advisories, running, staged);
    let most_severe = fixed.first()?;
    Some(SecurityNotice {
        severity: most_severe.severity,
        count: u32::try_from(fixed.len()).unwrap_or(u32::MAX),
        url: security_advisories::vetted_link(most_severe),
    })
}

/// Reads the staged bundle's list, or `None` (logged at warn) when it can't: an install never
/// waits on, or fails over, its advisory list.
pub(super) fn read_bundle_list(staged_app: &Path) -> Option<String> {
    let path = staged_app.join(BUNDLE_ADVISORIES_PATH);
    match std::fs::read_to_string(&path) {
        Ok(json) => Some(json),
        Err(e) => {
            log::warn!(
                "Couldn't read the update's security advisory list at {}, so it names no advisory: {e}",
                cmdr_fs::log_private::LogPrivate::file_path(&path)
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(raw: &str) -> Version {
        Version::parse(raw).expect("test versions are semver")
    }

    const LIST: &str = r#"{"advisories":[
        {"id":"GHSA-low1-0000-0000","fixedIn":"0.51.0","severity":"low","url":"https://github.com/vdavid/cmdr/security/advisories/GHSA-low1-0000-0000"},
        {"id":"GHSA-crit-0000-0000","fixedIn":"0.52.0","severity":"critical","url":"https://github.com/vdavid/cmdr/security/advisories/GHSA-crit-0000-0000"},
        {"id":"2026-10-16-medium","fixedIn":"0.54.0","severity":"medium","url":"https://getcmdr.com/changelog"}
    ]}"#;

    /// 0.50 straight to 0.55 skips the releases that fixed them, and still hears about all three.
    #[test]
    fn a_jump_over_several_releases_counts_every_fix_and_links_the_worst() {
        assert_eq!(
            notice_from(LIST, &version("0.50.0"), &version("0.55.0")),
            Some(SecurityNotice {
                severity: Severity::Critical,
                count: 3,
                url: Some("https://github.com/vdavid/cmdr/security/advisories/GHSA-crit-0000-0000".to_string()),
            })
        );
    }

    #[test]
    fn only_fixes_newer_than_the_running_build_count() {
        assert_eq!(
            notice_from(LIST, &version("0.52.0"), &version("0.54.0")),
            Some(SecurityNotice {
                severity: Severity::Medium,
                count: 1,
                url: Some("https://getcmdr.com/changelog".to_string()),
            })
        );
    }

    #[test]
    fn an_update_with_no_fix_in_its_window_has_no_notice() {
        assert_eq!(notice_from(LIST, &version("0.54.0"), &version("0.55.0")), None);
    }

    /// The update is the security fix itself, so a broken list costs the notice, never the update.
    #[test]
    fn a_malformed_list_or_an_unknown_severity_has_no_notice() {
        assert_eq!(notice_from("{", &version("0.50.0"), &version("0.55.0")), None);
        let unknown = LIST.replace("\"critical\"", "\"apocalyptic\"");
        assert_eq!(notice_from(&unknown, &version("0.50.0"), &version("0.55.0")), None);
    }

    #[test]
    fn a_most_severe_link_the_allowlist_refuses_keeps_the_notice_without_it() {
        let list = LIST.replace(
            "https://github.com/vdavid/cmdr/security/advisories/GHSA-crit-0000-0000",
            "https://github.com.evil.example/advisory",
        );
        let notice = notice_from(&list, &version("0.50.0"), &version("0.55.0")).expect("still a notice");
        assert_eq!(notice.severity, Severity::Critical);
        assert_eq!(notice.url, None);
    }

    #[test]
    fn a_bundle_with_no_list_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_bundle_list(&dir.path().join("Cmdr.app")), None);
    }
}

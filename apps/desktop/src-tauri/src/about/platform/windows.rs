//! Windows: compiles, isn't a target. No xattrs, no use history, no code signing
//! Cmdr reads; every answer says so.

use cmdr_about::card::OsCaps;
use cmdr_about::fact::{AboutTarget, Fact, NotApplicableWhy, UnknownWhy};
use cmdr_about::owner::KnownApp;
use cmdr_about::payload::{AppRef, Signature, Usage};
use cmdr_about::provenance::Download;

use super::StatExtras;

pub(crate) fn os_caps() -> OsCaps {
    OsCaps {
        use_history: false,
        code_signing: false,
        default_app: false,
        install_receipts: false,
    }
}

pub(crate) fn stat_extras(_metadata: &std::fs::Metadata) -> StatExtras {
    StatExtras {
        executable: false,
        online_only: false,
        physical_size: None,
        hard_links: None,
    }
}

pub(crate) fn download(_path: &str) -> Fact<Download> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn download_record(_read: super::ReadXattr<'_>) -> Fact<Download> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn last_opened(_path: &str) -> Fact<Usage> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn default_app(_path: &str) -> Fact<AppRef> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn signature(_path: &str, _target: AboutTarget, _extension: Option<&str>) -> Fact<Signature> {
    Fact::NotApplicable(NotApplicableWhy::NoCodeSigningOnThisOs)
}

pub(crate) fn installed_apps() -> Vec<KnownApp> {
    Vec::new()
}

pub(crate) fn app_with_id(_id: &str) -> Option<KnownApp> {
    None
}

pub(crate) fn receipts(_path: &str) -> Result<Vec<String>, UnknownWhy> {
    Err(UnknownWhy::NoRecord)
}

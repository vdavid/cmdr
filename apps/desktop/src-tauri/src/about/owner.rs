//! The owner row's lookups: the installed apps, an app elsewhere on this disk,
//! and the installer receipts. `cmdr_about::owner` reads the path and decides;
//! this module only answers what it asks.
//!
//! ❌ Nothing here opens anything inside another app's container: the question
//! comes from the path's NAME (`OwnerQuestion::of`), and the lookups read the app
//! folders, LaunchServices, and the receipts.

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::owner::{KnownApp, OwnerClaim, OwnerLookups, OwnerQuestion};

use super::platform;

/// Where the owner row's lookups come from: the OS, or a test's fixed answers.
#[derive(Clone, Copy)]
pub(crate) struct AppSource {
    /// The apps in the OS's app folders, from a cached scan.
    pub(crate) installed: fn() -> Vec<KnownApp>,
    /// The app with this id elsewhere on this computer's own disk.
    pub(crate) with_id: fn(&str) -> Option<KnownApp>,
    /// The receipts listing the path.
    pub(crate) receipts: fn(&str) -> Result<Vec<String>, UnknownWhy>,
}

impl AppSource {
    /// The OS's own answers.
    pub(crate) fn of_the_os() -> Self {
        Self {
            installed: platform::installed_apps,
            with_id: platform::app_with_id,
            receipts: platform::receipts,
        }
    }
}

/// The owner of the item at `path`, which the path's own `question` is about.
/// Blocking (an app scan, LaunchServices, a `pkgutil` run): the about pool only.
pub(crate) fn owner_fact(question: &OwnerQuestion, path: &str, source: AppSource) -> Fact<OwnerClaim> {
    let installed = || {
        crate::file_system::framework_pool::note_activity("scan the installed apps", "owner");
        (source.installed)()
    };
    let with_id = |id: &str| {
        crate::file_system::framework_pool::note_activity("LaunchServices app lookup", "owner");
        (source.with_id)(id)
    };
    let receipts = || {
        crate::file_system::framework_pool::note_activity("pkgutil --file-info", "owner");
        (source.receipts)(path)
    };
    question.answer(&OwnerLookups {
        installed_apps: &installed,
        app_with_id: &with_id,
        receipts: &receipts,
    })
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;

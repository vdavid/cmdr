//! Cmdr's own "Show in Finder", which has to reach Finder even while Cmdr holds the
//! `NSFileViewer` key.
//!
//! The key redirects every reveal on the machine, ours included, so without this the action
//! lands back in a Cmdr pane. Naming Finder doesn't help: `open -a Finder -R` is redirected
//! too, and `NSFileViewer = com.apple.finder` sends Finder a plain open, which opens the FILE in
//! its default app (both measured on macOS 27.0.1, 2026-11-02). An EMPTY value in this
//! process's argument domain is what works: AppKit logs "Finder replacement  not found" and
//! does a real Finder reveal, file selected. `DETAILS.md` § "Cmdr's own Show in Finder".

use std::path::Path;
use std::sync::Once;

use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSArray, NSMutableCopying, NSString, NSURL, NSUserDefaults};

use super::registration::{GlobalDomain, PRODUCTION_BUNDLE_ID, ViewerPreference};

/// Whether the key sends reveals to a Cmdr, this one or any other instance of it. A dev build's
/// "Show in Finder" must not land in the installed copy either.
fn key_names_a_cmdr(key: Option<&str>) -> bool {
    key.is_some_and(|key| key == PRODUCTION_BUNDLE_ID || key.starts_with(&format!("{PRODUCTION_BUNDLE_ID}-")))
}

/// Makes this process's own reveals skip the `NSFileViewer` key.
///
/// In-memory and this process only: the machine-wide key stays as it is, so other apps' reveals
/// still come here, and `GlobalDomain` reads the real value, since it names the global domain.
///
/// Never undone, and that's safe: after someone hands the key to another app, this process's
/// "Show in Finder" going to Finder is what the action says.
fn skip_the_key_in_this_process() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let defaults = NSUserDefaults::standardUserDefaults();
        let domain_name = NSString::from_str("NSArgumentDomain");
        // Setting a volatile domain replaces it, so whatever the command line put there stays.
        let existing = defaults.volatileDomainForName(&domain_name);
        let merged = existing.mutableCopy();
        let key = NSString::from_str("NSFileViewer");
        let value = NSString::from_str("");
        let value: &AnyObject = &value;
        // SAFETY: a defaults domain maps `NSString` keys to property-list values, and an
        // `NSString` is one. The dictionary retains the value and copies the key.
        unsafe { merged.setObject_forKey(value, ProtocolObject::from_ref(&*key)) };
        // SAFETY: `merged` holds only property-list values: what was there, plus the string above.
        unsafe { defaults.setVolatileDomain_forName(&merged, &domain_name) };
    });
}

/// Show `path` in Finder, selected.
///
/// On its own thread, because the sync command that calls this must not wait on a path that may
/// sit on a dead mount, and with a big stack, since this is a synchronous call into a macOS
/// framework (`file_system/CLAUDE.md`).
pub fn reveal_in_finder(path: &str) -> Result<(), String> {
    let path = path.to_string();
    std::thread::Builder::new()
        .name("reveal-in-finder".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            if key_names_a_cmdr(GlobalDomain.read().as_deref()) {
                skip_the_key_in_this_process();
            }
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
            NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
            log::debug!(
                target: "reveal",
                "Showed {} in Finder",
                cmdr_fs::log_private::LogPrivate::path(Path::new(&path).to_str().unwrap_or_default())
            );
        })
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_naming_any_cmdr_counts() {
        assert!(key_names_a_cmdr(Some("com.veszelovszki.cmdr")));
        assert!(key_names_a_cmdr(Some("com.veszelovszki.cmdr-dev-my-slug")));
    }

    #[test]
    fn another_file_manager_or_no_key_keeps_its_reveals() {
        // Someone who chose Path Finder gets Path Finder, from us like from every other app.
        assert!(!key_names_a_cmdr(Some("com.cocoatech.PathFinder")));
        assert!(!key_names_a_cmdr(Some("com.veszelovszki.cmdrish")));
        assert!(!key_names_a_cmdr(None));
    }
}

//! The Settings row's switch for the Finder extension (`apps/desktop/finder-extension/`): a
//! top-level "Open in Cmdr" in Finder's right-click menu.
//!
//! macOS keeps the on/off itself, under `pluginkit`, and the person can flip it in System Settings
//! too, so like the `NSFileViewer` key this reads through to the OS and stores nothing. The OS side
//! sits behind [`ExtensionHost`] so the rules are testable without touching the real registry.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use super::registration::{RevealHandlerBlocker, own_bundle_id};

/// What the row shows, and whether this copy may switch it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinderExtensionStatus {
    pub enabled: bool,
    /// Why the switch won't take a yes, or `None` when the user may operate it. Same reasons
    /// as the reveal handler, and the same rule: it never blocks switching OFF.
    pub blocked_by: Option<RevealHandlerBlocker>,
}

/// The extension's registration with macOS, behind a seam.
pub trait ExtensionHost {
    /// Whether macOS has it switched on.
    fn is_enabled(&self) -> bool;
    /// Register it and switch it on or off.
    fn set_enabled(&self, enabled: bool);
}

/// The switch's rules over an [`ExtensionHost`].
pub struct FinderExtension<H: ExtensionHost> {
    host: H,
    /// Why this copy may not switch it on, before "it's already on" has its say.
    blocker: Option<RevealHandlerBlocker>,
}

impl<H: ExtensionHost> FinderExtension<H> {
    pub fn new(host: H, blocker: Option<RevealHandlerBlocker>) -> Self {
        Self { host, blocker }
    }

    pub fn status(&self) -> FinderExtensionStatus {
        let enabled = self.host.is_enabled();
        FinderExtensionStatus {
            enabled,
            // ❗ Being on beats the blocker: a copy that moved out of Applications must still be
            // able to switch this off, or the person is stranded with it running.
            blocked_by: if enabled { None } else { self.blocker },
        }
    }

    /// Switch it, and report where that left things.
    pub fn set_enabled(&self, enabled: bool) -> FinderExtensionStatus {
        let status = self.status();
        if enabled && status.blocked_by.is_some() {
            return status;
        }
        if enabled != status.enabled {
            self.host.set_enabled(enabled);
        }
        self.status()
    }
}

/// The extension's bundle id: the production app's, plus a suffix. macOS requires that prefix.
const EXTENSION_ID: &str = "com.veszelovszki.cmdr.finder-extension";
const PLUGINKIT: &str = "/usr/bin/pluginkit";

/// Whether a `pluginkit -m -i <id>` line says the extension is on. Each line starts with its state:
/// `+` is on, `-` is off, and a space is the default, which is off for a Finder extension.
fn line_says_enabled(output: &str) -> bool {
    output.lines().any(|line| line.trim_start_matches(' ').starts_with('+'))
}

/// The real `pluginkit`.
pub struct Pluginkit {
    appex: Option<PathBuf>,
}

impl Pluginkit {
    pub fn new(appex: Option<PathBuf>) -> Self {
        Self { appex }
    }
}

impl ExtensionHost for Pluginkit {
    fn is_enabled(&self) -> bool {
        let output = Command::new(PLUGINKIT).args(["-m", "-i", EXTENSION_ID]).output();
        match output {
            Ok(output) => line_says_enabled(&String::from_utf8_lossy(&output.stdout)),
            Err(err) => {
                log::warn!(target: "reveal", "Couldn't run pluginkit: {err}");
                false
            }
        }
    }

    fn set_enabled(&self, enabled: bool) {
        // Registered first, since macOS may not have scanned the bundle yet right after an install.
        if enabled && let Some(appex) = &self.appex {
            run_pluginkit(&[std::ffi::OsStr::new("-a"), appex.as_os_str()]);
        }
        let election = if enabled { "use" } else { "ignore" };
        run_pluginkit(&["-e", election, "-i", EXTENSION_ID].map(std::ffi::OsStr::new));
    }
}

fn run_pluginkit(args: &[&std::ffi::OsStr]) {
    match Command::new(PLUGINKIT).args(args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => log::warn!(target: "reveal", "pluginkit ended with {status}"),
        Err(err) => log::warn!(target: "reveal", "Couldn't run pluginkit: {err}"),
    }
}

/// Where this copy's extension sits, if it carries one. Only a release bundle does.
fn bundled_extension(executable: &Path) -> Option<PathBuf> {
    // `Cmdr.app/Contents/MacOS/Cmdr` → `Cmdr.app/Contents/PlugIns/…`.
    let appex = executable.parent()?.parent()?.join("PlugIns/CmdrFinderExtension.appex");
    appex.is_dir().then_some(appex)
}

/// The switch over the real `pluginkit`, for this copy of Cmdr.
pub fn for_this_copy() -> FinderExtension<Pluginkit> {
    let appex = std::env::current_exe().ok().and_then(|exe| bundled_extension(&exe));
    let blocker = if own_bundle_id().is_none() || appex.is_none() {
        Some(RevealHandlerBlocker::NotProductionBuild)
    } else if !crate::install_location::running_copy_is_installed() {
        Some(RevealHandlerBlocker::NotInApplications)
    } else {
        None
    };
    FinderExtension::new(Pluginkit::new(appex), blocker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct FakeHost {
        enabled: Cell<bool>,
        sets: RefCell<Vec<bool>>,
    }

    impl ExtensionHost for &FakeHost {
        fn is_enabled(&self) -> bool {
            self.enabled.get()
        }

        fn set_enabled(&self, enabled: bool) {
            self.sets.borrow_mut().push(enabled);
            self.enabled.set(enabled);
        }
    }

    const NOT_INSTALLED: Option<RevealHandlerBlocker> = Some(RevealHandlerBlocker::NotInApplications);

    #[test]
    fn it_reports_what_macos_says() {
        let host = FakeHost::default();
        let status = FinderExtension::new(&host, None).status();
        assert!(!status.enabled);
        assert_eq!(status.blocked_by, None);
    }

    #[test]
    fn switching_on_reports_the_state_it_left() {
        let host = FakeHost::default();
        assert!(FinderExtension::new(&host, None).set_enabled(true).enabled);
        assert_eq!(*host.sets.borrow(), vec![true]);
    }

    #[test]
    fn a_blocked_copy_refuses_to_switch_on_and_says_why() {
        let host = FakeHost::default();
        let status = FinderExtension::new(&host, NOT_INSTALLED).set_enabled(true);
        assert!(!status.enabled);
        assert_eq!(status.blocked_by, NOT_INSTALLED);
        assert!(host.sets.borrow().is_empty());
    }

    #[test]
    fn a_blocked_copy_can_still_switch_off() {
        // ❗ It moved out of Applications while on. A dead switch would strand it running.
        let host = FakeHost::default();
        host.enabled.set(true);
        let extension = FinderExtension::new(&host, NOT_INSTALLED);
        assert_eq!(extension.status().blocked_by, None);
        assert!(!extension.set_enabled(false).enabled);
        assert_eq!(*host.sets.borrow(), vec![false]);
    }

    #[test]
    fn asking_for_the_state_it_is_in_writes_nothing() {
        let host = FakeHost::default();
        FinderExtension::new(&host, None).set_enabled(false);
        assert!(host.sets.borrow().is_empty());
    }

    #[test]
    fn pluginkit_output_reads_by_its_leading_mark() {
        assert!(line_says_enabled("+    com.veszelovszki.cmdr.finder-extension(1.0)\n"));
        assert!(!line_says_enabled("-    com.veszelovszki.cmdr.finder-extension(1.0)\n"));
        assert!(!line_says_enabled("     com.veszelovszki.cmdr.finder-extension(1.0)\n"));
        assert!(!line_says_enabled("(no matches)\n"));
    }

    #[test]
    fn a_bundle_with_no_extension_has_none_to_offer() {
        assert_eq!(
            bundled_extension(Path::new("/nowhere/Cmdr.app/Contents/MacOS/Cmdr")),
            None
        );
    }
}

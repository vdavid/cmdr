//! Who macOS hands a "Show in Finder" to.
//!
//! One global-domain preference key, `NSFileViewer`, decides it. AppKit reads it before
//! it opens Finder for `activateFileViewerSelectingURLs:`, `selectFile:inFileViewerRootedAtPath:`,
//! and `open -R`. Writing our bundle id there is the whole mechanism; the key is
//! undocumented, so `DETAILS.md` carries the evidence and the fragility warning.
//!
//! The OS side sits behind [`ViewerPreference`] so the state machine is testable without
//! touching the real global domain — a test that wrote it would rewire the machine it runs on.

use objc2_core_foundation::{
    CFPreferencesCopyValue, CFPreferencesSetValue, CFPreferencesSynchronize, CFString, kCFPreferencesAnyApplication,
    kCFPreferencesAnyHost, kCFPreferencesCurrentUser,
};
use serde::{Deserialize, Serialize};

/// The global-domain key AppKit consults for a reveal. `defaults write -g NSFileViewer …`
/// writes the same one.
const NS_FILE_VIEWER_KEY: &str = "NSFileViewer";

/// The bundle identifier a shipped Cmdr carries (`tauri.conf.json` § `identifier`). Dev,
/// per-worktree, and E2E instances carry a `-<instance>` suffix instead, which is what
/// keeps them out of the key. See [`own_bundle_id`].
const PRODUCTION_BUNDLE_ID: &str = "com.veszelovszki.cmdr";

/// What the Settings row shows, read through to the OS every time it asks.
///
/// Not a stored setting: the user can hand the key to another app from outside Cmdr,
/// so a cached flag would lie. `DETAILS.md` § "Not a stored setting".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RevealHandlerState {
    /// Cmdr holds the key: reveals from other apps land here.
    Registered,
    /// Nobody holds it. Reveals go to Finder, which is the true default.
    NotRegistered,
    /// Another file manager holds it. Turning Cmdr on takes it over, one click,
    /// but nothing does so silently.
    HeldByOtherApp {
        bundle_id: String,
        /// `None` when the holder isn't installed any more (a stale key), so the UI
        /// can fall back to the raw id.
        display_name: Option<String>,
    },
}

/// Everything the Settings row needs: where the key stands, and whether this copy of Cmdr
/// may change it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealHandlerStatus {
    pub state: RevealHandlerState,
    /// Why the switch won't take a yes, or `None` when the user may operate it.
    pub blocked_by: Option<RevealHandlerBlocker>,
}

/// Why Cmdr won't take the `NSFileViewer` key right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RevealHandlerBlocker {
    /// A debug build, a dev / worktree / E2E instance: see [`own_bundle_id`]. These builds
    /// come and go, and one that took the key and then got deleted would leave a dangling
    /// `NSFileViewer` that breaks reveal machine-wide. Outranks every other blocker, and
    /// unlike them it holds in both directions: the key can never name such a build.
    NotProductionBuild,
    /// This copy of Cmdr isn't in `/Applications` or `~/Applications`, so it's one of the
    /// copies that gets moved or deleted. Registering it would leave a dangling key.
    NotInApplications,
}

/// The global preferences domain, behind a seam.
///
/// Reads and writes are separate so the state machine can enforce "clear only what's
/// ours"; `display_name` is here too because resolving a bundle id to a human name is an
/// OS query like the others, and a test must not depend on which apps are installed.
pub trait ViewerPreference {
    /// The bundle id currently in `NSFileViewer`, or `None` when the key is absent.
    fn read(&self) -> Option<String>;
    /// Put `bundle_id` in the key.
    fn write(&self, bundle_id: &str);
    /// Remove the key. ❌ Never "set it to Finder": absence is the default, and a
    /// hardcoded `com.apple.finder` would survive a user who later prefers another app.
    fn clear(&self);
    /// Human name for an installed app, for "Currently: Path Finder".
    fn display_name(&self, bundle_id: &str) -> Option<String>;
}

/// The `NSFileViewer` state machine: reads the key, and writes it only when this build
/// is allowed to.
pub struct RevealRegistration<P: ViewerPreference> {
    prefs: P,
    /// The bundle id this build may claim, or `None` when it may never write the key.
    own_bundle_id: Option<String>,
    /// What the machine says about where this copy sits, before the never-block-off rule
    /// in [`Self::blocked_by`] has its say.
    install_blocker: Option<RevealHandlerBlocker>,
}

impl<P: ViewerPreference> RevealRegistration<P> {
    pub fn new(prefs: P, own_bundle_id: Option<String>, install_blocker: Option<RevealHandlerBlocker>) -> Self {
        Self {
            prefs,
            own_bundle_id,
            install_blocker,
        }
    }

    /// Where the key stands, and whether the user may move it.
    pub fn status(&self) -> RevealHandlerStatus {
        let state = self.state();
        RevealHandlerStatus {
            blocked_by: self.blocked_by(&state),
            state,
        }
    }

    /// Why the switch won't take a yes, or `None` when it will.
    ///
    /// ❗ Holding the key beats the install blocker: handing it back has to stay possible
    /// from wherever this copy has ended up. Someone who switched this on and then moved
    /// Cmdr out of `/Applications` would otherwise be stranded registered, which is the
    /// exact dangling key the blocker exists to prevent.
    ///
    /// A non-production build is blocked unconditionally, and that strands nobody: the key
    /// never reads as ITS OWN (`state` needs an id to compare), so there's nothing for it to
    /// hand back.
    fn blocked_by(&self, state: &RevealHandlerState) -> Option<RevealHandlerBlocker> {
        if self.own_bundle_id.is_none() {
            return Some(RevealHandlerBlocker::NotProductionBuild);
        }
        if matches!(state, RevealHandlerState::Registered) {
            return None;
        }
        self.install_blocker
    }

    /// Read through to the OS and word what it says. A non-production build reads the key
    /// like any copy, so its Settings row can still show who holds it.
    fn state(&self) -> RevealHandlerState {
        match self.prefs.read() {
            None => RevealHandlerState::NotRegistered,
            Some(current) if self.own_bundle_id.as_deref() == Some(current.as_str()) => RevealHandlerState::Registered,
            Some(current) => {
                let display_name = self.prefs.display_name(&current);
                RevealHandlerState::HeldByOtherApp {
                    bundle_id: current,
                    display_name,
                }
            }
        }
    }

    /// Take the key (`enabled`) or give it up, and report where that left things.
    ///
    /// Turning off clears the key ONLY when it still names us. Another app may have
    /// taken it since the row was drawn, and clearing then would silently unregister
    /// somebody else's file manager.
    ///
    /// A build that isn't allowed to write touches nothing in either direction, and answers
    /// with its blocked status.
    pub fn set_enabled(&self, enabled: bool) -> RevealHandlerStatus {
        let Some(own) = self.own_bundle_id.as_deref() else {
            return self.status();
        };
        if enabled {
            let status = self.status();
            if let Some(blocker) = status.blocked_by {
                log::warn!(target: "reveal::registration", "Not taking the reveal handler: {blocker:?}");
                return status;
            }
            self.prefs.write(own);
        } else if self.prefs.read().as_deref() == Some(own) {
            self.prefs.clear();
        }
        self.status()
    }
}

/// The bundle id this process may write into the key, or `None` when it may not.
///
/// Three independent conditions, each of which alone disqualifies a build:
///
/// - a debug build (`pnpm dev` straight off `tauri.conf.json` still carries the
///   production identifier, so the identifier check alone wouldn't catch it);
/// - any harness environment (`prod_instance`), which covers a release-mode E2E lane;
/// - a bundle id that isn't the production one, which is every `--worktree` and E2E
///   instance (`apps/desktop/scripts/instance-id.ts` suffixes them).
pub fn own_bundle_id() -> Option<String> {
    if cfg!(debug_assertions) {
        return None;
    }
    if crate::prod_instance::non_prod_env_var_in(&|name| std::env::var_os(name).is_some()).is_some() {
        return None;
    }
    let running = running_bundle_id()?;
    (running == PRODUCTION_BUNDLE_ID).then_some(running)
}

/// This process's own bundle identifier, straight from `NSBundle`. ❌ Never a hardcoded
/// string: the dev, worktree, and E2E instances each run under their own id, and the
/// point of the check is to tell them apart.
fn running_bundle_id() -> Option<String> {
    use objc2_foundation::NSBundle;
    Some(NSBundle::mainBundle().bundleIdentifier()?.to_string())
}

/// Reads and writes the real `Apple Global Domain` (`kCFPreferencesAnyApplication`,
/// current user, any host) — the triple `defaults write -g` uses.
///
/// CFPreferences and LaunchServices are both thread-safe (`src-tauri/DETAILS.md` §
/// "Which Apple APIs skip the main-thread rule"), so nothing here needs a `MainThreadMarker`.
pub struct GlobalDomain;

impl GlobalDomain {
    /// The domain triple `defaults write -g` uses: any application, current user, any
    /// host. Wrapped once here so the three FFI statics are read in one place.
    fn triple() -> (&'static CFString, &'static CFString, &'static CFString) {
        // SAFETY: the three CoreFoundation domain constants are immutable `'static`
        // `CFStringRef`s that the framework initialises before any Rust in this process
        // runs; reading them is `unsafe` only because they cross the FFI boundary.
        unsafe {
            (
                kCFPreferencesAnyApplication,
                kCFPreferencesCurrentUser,
                kCFPreferencesAnyHost,
            )
        }
    }

    /// Write `value` (or remove the key when `None`) and flush it to disk.
    fn set(value: Option<&CFString>) {
        let key = CFString::from_str(NS_FILE_VIEWER_KEY);
        let (application, user, host) = Self::triple();
        // SAFETY: `CFPreferencesSetValue` borrows its arguments and copies what it keeps;
        // `key` and `value` outlive the call, and the domain constants are `'static`. A
        // `None` value is CoreFoundation's documented "remove this key" spelling.
        unsafe {
            CFPreferencesSetValue(&key, value.map(|v| v.as_ref()), application, user, host);
        }
        // Without the flush the change sits in this process's `cfprefsd` cache, so an app
        // asked to reveal a second later would still get the old answer.
        if !CFPreferencesSynchronize(application, user, host) {
            log::warn!(target: "reveal::registration", "Couldn't flush the global preferences domain");
        }
    }
}

impl ViewerPreference for GlobalDomain {
    fn read(&self) -> Option<String> {
        let key = CFString::from_str(NS_FILE_VIEWER_KEY);
        let (application, user, host) = Self::triple();
        let value = CFPreferencesCopyValue(&key, application, user, host)?;
        // Anything but a string is somebody else's garbage in a shared domain; read it as
        // "no viewer set" rather than guessing.
        value.downcast_ref::<CFString>().map(|s| s.to_string())
    }

    fn write(&self, bundle_id: &str) {
        Self::set(Some(&CFString::from_str(bundle_id)));
    }

    fn clear(&self) {
        Self::set(None);
    }

    fn display_name(&self, bundle_id: &str) -> Option<String> {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSString;
        use std::path::PathBuf;

        let workspace = NSWorkspace::sharedWorkspace();
        let url = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?;
        let path = PathBuf::from(url.path()?.to_string());
        Some(crate::file_system::open_with::read_app_display_name(&path))
    }
}

#[cfg(test)]
mod tests {
    //! Every test runs against an in-memory [`FakePreference`]. ❌ None of them may reach
    //! `GlobalDomain`: writing the real key would rewire the machine running the suite.
    use super::*;
    use std::sync::Mutex;

    const OURS: &str = "com.veszelovszki.cmdr";
    const THEIRS: &str = "com.cocoatech.PathFinder";

    #[derive(Default)]
    struct FakeState {
        value: Option<String>,
        writes: Vec<String>,
        clears: usize,
    }

    #[derive(Default)]
    struct FakePreference {
        state: Mutex<FakeState>,
    }

    impl FakePreference {
        fn holding(bundle_id: &str) -> Self {
            let fake = Self::default();
            fake.state.lock().expect("fake lock").value = Some(bundle_id.to_string());
            fake
        }

        fn writes(&self) -> Vec<String> {
            self.state.lock().expect("fake lock").writes.clone()
        }

        fn clears(&self) -> usize {
            self.state.lock().expect("fake lock").clears
        }
    }

    impl ViewerPreference for &FakePreference {
        fn read(&self) -> Option<String> {
            self.state.lock().expect("fake lock").value.clone()
        }

        fn write(&self, bundle_id: &str) {
            let mut state = self.state.lock().expect("fake lock");
            state.writes.push(bundle_id.to_string());
            state.value = Some(bundle_id.to_string());
        }

        fn clear(&self) {
            let mut state = self.state.lock().expect("fake lock");
            state.clears += 1;
            state.value = None;
        }

        fn display_name(&self, bundle_id: &str) -> Option<String> {
            (bundle_id == THEIRS).then(|| "Path Finder".to_string())
        }
    }

    /// A copy installed in an Applications folder: nothing standing in the way.
    fn ours(prefs: &FakePreference) -> RevealRegistration<&FakePreference> {
        RevealRegistration::new(prefs, Some(OURS.to_string()), None)
    }

    /// The same copy, running from `~/Downloads`, a mounted disk image, or Gatekeeper's
    /// translocated shadow copy.
    fn ours_uninstalled(prefs: &FakePreference) -> RevealRegistration<&FakePreference> {
        RevealRegistration::new(
            prefs,
            Some(OURS.to_string()),
            Some(RevealHandlerBlocker::NotInApplications),
        )
    }

    /// The status of a switch the user may operate.
    fn unblocked(state: RevealHandlerState) -> RevealHandlerStatus {
        RevealHandlerStatus {
            state,
            blocked_by: None,
        }
    }

    /// The status of a switch that won't take a yes.
    fn blocked(state: RevealHandlerState) -> RevealHandlerStatus {
        RevealHandlerStatus {
            state,
            blocked_by: Some(RevealHandlerBlocker::NotInApplications),
        }
    }

    fn held_by_path_finder() -> RevealHandlerState {
        RevealHandlerState::HeldByOtherApp {
            bundle_id: THEIRS.to_string(),
            display_name: Some("Path Finder".to_string()),
        }
    }

    #[test]
    fn an_absent_key_reads_as_not_registered() {
        let prefs = FakePreference::default();
        assert_eq!(ours(&prefs).status(), unblocked(RevealHandlerState::NotRegistered));
    }

    #[test]
    fn our_own_bundle_id_reads_as_registered() {
        let prefs = FakePreference::holding(OURS);
        assert_eq!(ours(&prefs).status(), unblocked(RevealHandlerState::Registered));
    }

    #[test]
    fn another_app_reads_as_held_by_it_and_names_it() {
        let prefs = FakePreference::holding(THEIRS);
        assert_eq!(ours(&prefs).status(), unblocked(held_by_path_finder()));
    }

    #[test]
    fn an_uninstalled_holder_still_reports_its_bundle_id() {
        // A stale key: the app that took it is gone. The row still has to say something
        // truthful, so the id stands in for the name.
        let prefs = FakePreference::holding("com.example.deleted");
        assert_eq!(
            ours(&prefs).status(),
            unblocked(RevealHandlerState::HeldByOtherApp {
                bundle_id: "com.example.deleted".to_string(),
                display_name: None,
            })
        );
    }

    #[test]
    fn enabling_writes_our_bundle_id_and_reports_registered() {
        let prefs = FakePreference::default();
        assert_eq!(
            ours(&prefs).set_enabled(true),
            unblocked(RevealHandlerState::Registered)
        );
        assert_eq!(prefs.writes(), vec![OURS]);
    }

    #[test]
    fn enabling_over_another_app_takes_the_key() {
        // Deliberate: the Settings row names the incumbent, and taking over is one click.
        // Nothing here happens without the user asking.
        let prefs = FakePreference::holding(THEIRS);
        assert_eq!(
            ours(&prefs).set_enabled(true),
            unblocked(RevealHandlerState::Registered)
        );
        assert_eq!(prefs.writes(), vec![OURS]);
    }

    #[test]
    fn disabling_clears_the_key_when_it_is_ours() {
        let prefs = FakePreference::holding(OURS);
        assert_eq!(
            ours(&prefs).set_enabled(false),
            unblocked(RevealHandlerState::NotRegistered)
        );
        assert_eq!(prefs.clears(), 1);
        assert!(prefs.writes().is_empty(), "clearing must never write a replacement");
    }

    #[test]
    fn disabling_leaves_another_apps_key_alone() {
        // The user turned Cmdr off after somebody else took the key. Clearing here would
        // unregister THEIR file manager.
        let prefs = FakePreference::holding(THEIRS);
        assert_eq!(ours(&prefs).set_enabled(false), unblocked(held_by_path_finder()));
        assert_eq!(prefs.clears(), 0);
        assert!(prefs.writes().is_empty());
    }

    #[test]
    fn disabling_an_absent_key_is_a_no_op() {
        let prefs = FakePreference::default();
        assert_eq!(
            ours(&prefs).set_enabled(false),
            unblocked(RevealHandlerState::NotRegistered)
        );
        assert_eq!(prefs.clears(), 0);
    }

    /// A debug build, or a dev, worktree, or E2E instance: [`own_bundle_id`] said `None`.
    fn not_production(prefs: &FakePreference) -> RevealRegistration<&FakePreference> {
        RevealRegistration::new(prefs, None, None)
    }

    /// The status every operation on a non-production build answers.
    fn not_production_status(state: RevealHandlerState) -> RevealHandlerStatus {
        RevealHandlerStatus {
            state,
            blocked_by: Some(RevealHandlerBlocker::NotProductionBuild),
        }
    }

    #[test]
    fn a_build_that_may_not_write_reads_the_key_but_refuses_to_move_it() {
        // A dev or E2E build. It must be structurally incapable of rewiring the user's
        // Mac: a dangling `NSFileViewer` left by a deleted build breaks reveal everywhere.
        // It still reads the key, so its Settings row shows where things stand.
        let prefs = FakePreference::holding(THEIRS);
        let registration = not_production(&prefs);
        let expected = not_production_status(held_by_path_finder());

        assert_eq!(registration.status(), expected);
        assert_eq!(registration.set_enabled(true), expected);
        assert_eq!(registration.set_enabled(false), expected);
        assert!(prefs.writes().is_empty());
        assert_eq!(prefs.clears(), 0);
    }

    #[test]
    fn a_build_that_may_not_write_reads_an_absent_key_as_not_registered() {
        let prefs = FakePreference::default();
        let registration = not_production(&prefs);

        assert_eq!(
            registration.set_enabled(true),
            not_production_status(RevealHandlerState::NotRegistered)
        );
        assert!(prefs.writes().is_empty());
    }

    #[test]
    fn a_build_that_may_not_write_leaves_the_installed_copys_key_alone() {
        // `pnpm dev` off the plain config carries the production bundle id, so the key can
        // name that id while this build runs. It belongs to the installed copy: reading it
        // as `Registered` would lift the block, and switching off would clear the key out
        // from under the real Cmdr.
        let prefs = FakePreference::holding(OURS);
        let registration = not_production(&prefs);
        let expected = not_production_status(RevealHandlerState::HeldByOtherApp {
            bundle_id: OURS.to_string(),
            display_name: None,
        });

        assert_eq!(registration.status(), expected);
        assert_eq!(registration.set_enabled(false), expected);
        assert_eq!(prefs.clears(), 0);
    }

    #[test]
    fn not_being_a_production_build_outranks_the_applications_folder() {
        // A dev build runs from `target/`, so both reasons hold. The row names the one no
        // move would fix.
        let prefs = FakePreference::default();
        let registration = RevealRegistration::new(&prefs, None, Some(RevealHandlerBlocker::NotInApplications));

        assert_eq!(
            registration.status(),
            not_production_status(RevealHandlerState::NotRegistered)
        );
    }

    #[test]
    fn a_copy_outside_applications_refuses_to_take_the_key() {
        // The copy most likely to be deleted is the one nothing should point at. A
        // deleted holder leaves a dangling key, and reveal stops working machine-wide.
        let prefs = FakePreference::default();
        assert_eq!(
            ours_uninstalled(&prefs).set_enabled(true),
            blocked(RevealHandlerState::NotRegistered)
        );
        assert!(prefs.writes().is_empty());
    }

    #[test]
    fn a_copy_outside_applications_refuses_to_take_the_key_from_another_app() {
        let prefs = FakePreference::holding(THEIRS);
        assert_eq!(
            ours_uninstalled(&prefs).set_enabled(true),
            blocked(held_by_path_finder())
        );
        assert!(prefs.writes().is_empty());
        assert_eq!(prefs.clears(), 0);
    }

    #[test]
    fn a_copy_outside_applications_that_holds_the_key_may_still_hand_it_back() {
        // ❗ The gate blocks taking the key, never giving it up. Somebody who switched
        // this on and then moved Cmdr out of Applications must not be stranded
        // registered: that IS the dangling key.
        let prefs = FakePreference::holding(OURS);
        let registration = ours_uninstalled(&prefs);

        assert_eq!(
            registration.status(),
            unblocked(RevealHandlerState::Registered),
            "a switch the user can't touch would strand them registered"
        );
        assert_eq!(
            registration.set_enabled(false),
            blocked(RevealHandlerState::NotRegistered)
        );
        assert_eq!(prefs.clears(), 1);
    }
}

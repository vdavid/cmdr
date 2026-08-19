//! Following the OS's language and region while the app runs.
//!
//! `appearance.language: 'system'` means the language the user reads NOW, not
//! the one they read when Cmdr launched, and the same goes for the region their
//! dates and numbers follow. macOS nudges people to restart their apps after a
//! change like that; an app that just does the right thing is better than one
//! that asks.
//!
//! macOS posts locale changes in bursts (System Settings writes language,
//! region, and calendar as separate preferences), and most of those bursts don't
//! move either answer. So [`LocaleWatcher`] sits between the notification and
//! the app: it collapses a burst into one re-read and stays silent unless the
//! answer actually changed. Both halves matter downstream, where an
//! announcement re-renders every open `t()` in every window.

// The watcher itself is macOS-only (nothing else posts a locale notification),
// so everything it needs comes in under the same gate as the tests that exercise
// it on every platform.
#[cfg(any(target_os = "macos", test))]
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[cfg(any(target_os = "macos", test))]
use std::time::Duration;

#[cfg(any(target_os = "macos", test))]
use crate::ignore_poison::IgnorePoison as _;

#[cfg(target_os = "macos")]
use log::{info, warn};
#[cfg(target_os = "macos")]
use tauri::{AppHandle, Runtime};

/// How long we wait for a notification burst to settle before re-reading the
/// preferences. Long enough to swallow the several notifications one System
/// Settings change posts, short enough that the UI follows while the user is
/// still looking at it.
#[cfg(target_os = "macos")]
const SETTLE_WINDOW: Duration = Duration::from_millis(300);

/// Turns a stream of "something locale-ish changed" notifications into the rare
/// event the app actually cares about: the OS's answer moved.
///
/// Two filters, in order. A burst collapses into ONE re-read (the first
/// notification arms a settle timer; the rest ride on it, because the timer
/// re-reads live state when it fires anyway). Then the fresh answer is compared
/// against what everyone already has, and a match announces nothing.
///
/// Generic in the answer so the comparison is the whole answer, not a piece of
/// it: production watches [`crate::intl::OsLocales`], where a region change with
/// the language untouched still has to reach the formatters.
#[cfg(any(target_os = "macos", test))]
pub(crate) struct LocaleWatcher<T> {
    /// How long to wait for a burst to settle before re-reading.
    settle_window: Duration,
    /// Re-reads the OS preferences and resolves the app's answer from them.
    resolve: Box<dyn Fn() -> T + Send + Sync>,
    /// Tells the app the answer moved. Never called with an answer the app has.
    announce: Box<dyn Fn(T) + Send + Sync>,
    /// The answer the app is running on: the startup resolution, then whatever
    /// we last announced.
    announced: Mutex<T>,
    /// Whether a settle timer is already counting down for the current burst.
    settling: AtomicBool,
}

#[cfg(any(target_os = "macos", test))]
impl<T: PartialEq + Clone + Send + Sync + 'static> LocaleWatcher<T> {
    /// Builds a watcher seeded with the CURRENT resolution, so the first
    /// notification is compared against what the app is already running on
    /// rather than announcing a change that never happened.
    pub(crate) fn new(
        settle_window: Duration,
        resolve: impl Fn() -> T + Send + Sync + 'static,
        announce: impl Fn(T) + Send + Sync + 'static,
    ) -> Arc<Self> {
        let current = resolve();
        Arc::new(Self {
            settle_window,
            resolve: Box::new(resolve),
            announce: Box::new(announce),
            announced: Mutex::new(current),
            settling: AtomicBool::new(false),
        })
    }

    /// Feeds one notification in. Cheap and non-blocking: the re-read happens on
    /// a settle timer, off whatever thread the notification arrived on.
    pub(crate) fn notify(self: &Arc<Self>) {
        // Already counting down: this notification is part of a burst the
        // running timer will cover, because the timer reads live state.
        if self.settling.swap(true, Ordering::SeqCst) {
            return;
        }
        let watcher = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(watcher.settle_window);
            // Disarm BEFORE reading, so a change landing during the read opens a
            // fresh window instead of being swallowed by this one.
            watcher.settling.store(false, Ordering::SeqCst);
            watcher.announce_if_moved();
        });
    }

    /// Re-reads the answer and announces it only if it differs from the one the
    /// app is running on.
    fn announce_if_moved(&self) {
        let resolved = (self.resolve)();
        let mut announced = self.announced.lock_ignore_poison();
        if *announced == resolved {
            return;
        }
        *announced = resolved.clone();
        drop(announced);
        (self.announce)(resolved);
    }
}

/// Starts following live macOS language and region changes, emitting
/// [`crate::system_events::OsLocalesChanged`] whenever either answer moves.
///
/// Called from the Tauri `setup` hook, which runs on the main thread; the
/// observer registration itself has no main-thread requirement, but keeping it
/// beside the other system observers is what makes it discoverable.
///
/// Two notification names feed one watcher, and they carry different halves of
/// the answer. `AppleLanguagePreferencesChangedNotification` is the distributed
/// notification the System Settings pane posts when the LANGUAGE order moves;
/// it's undocumented in the same way `text_size.rs`'s accessibility notification
/// is, with the same fallback story (if Apple stops posting it, the language
/// still resolves correctly on the next launch).
/// `NSCurrentLocaleDidChangeNotification` is the documented signal and tracks
/// `AppleLocale`, which is where the REGION override lives, so it's what carries
/// a region change to the formatters. Overlap costs nothing, since a burst
/// collapses into one re-read and an unchanged answer announces nothing. The
/// evidence for both claims, and why `defaults write` alone can't test this, is
/// in `DETAILS.md`.
///
/// This is also the seam for anything else that has to be rebuilt in the new
/// language: the emit site below is the one place that knows the answer moved.
#[cfg(target_os = "macos")]
pub fn observe_os_locale_changes<R: Runtime>(app_handle: AppHandle<R>) {
    use std::ptr::NonNull;

    use objc2_foundation::{
        NSCurrentLocaleDidChangeNotification, NSDistributedNotificationCenter, NSNotification, NSNotificationCenter,
        NSString,
    };
    use tauri_specta::Event as _;

    use crate::system_events::OsLocalesChanged;

    let watcher = LocaleWatcher::new(SETTLE_WINDOW, super::resolved_os_locales, move |locales| {
        info!(
            "OS locales changed: language {:?}, formats {:?}",
            locales.ui, locales.format
        );
        // The native menu bar can't re-render itself off a rune the way the
        // webview does, so it's rebuilt here. Only when the catalog the native
        // side reads actually moved: a region change (or a language change under
        // a pinned `appearance.language`) leaves every label as it was.
        if super::native_strings::refresh_active_locale() {
            let for_rebuild = app_handle.clone();
            if let Err(e) = app_handle.run_on_main_thread(move || {
                if let Err(e) = crate::menu::rebuild_menu_bar(&for_rebuild) {
                    warn!("Couldn't rebuild the menu bar in the new language: {e}");
                }
            }) {
                warn!("Couldn't reach the main thread to rebuild the menu bar: {e}");
            }
        }
        if let Err(e) = (OsLocalesChanged { locales }).emit(&app_handle) {
            warn!("Failed to emit os-locales-changed event: {e}");
        }
    });

    let default_center = NSNotificationCenter::defaultCenter();
    let distributed_center = NSDistributedNotificationCenter::defaultCenter();
    let language_preferences_changed = NSString::from_str("AppleLanguagePreferencesChangedNotification");

    let for_default = Arc::clone(&watcher);
    let default_block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        for_default.notify();
    });
    let distributed_block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        watcher.notify();
    });

    // SAFETY: both names are valid notification names (one the Foundation
    // constant, one an `NSString` we own) and both centres are live singletons.
    // Each block is a live `RcBlock` with the expected
    // `(NonNull<NSNotification>) -> ()` signature. The centres retain the
    // observers for the lifetime of the app; we never remove them, because we
    // want updates for the whole session.
    unsafe {
        default_center.addObserverForName_object_queue_usingBlock(
            Some(NSCurrentLocaleDidChangeNotification),
            None,
            None,
            &default_block,
        );
        distributed_center.addObserverForName_object_queue_usingBlock(
            Some(&language_preferences_changed),
            None,
            None,
            &distributed_block,
        );
    }
}

/// No-op off macOS.
///
/// Linux has no equivalent signal: the desktop language and formats live in the
/// session's environment (`LANG` / `LC_MESSAGES` / `LC_NUMERIC`), which is fixed
/// for the life of the process, and no portal or D-Bus name broadcasts a change.
/// A logged-in user changing either gets it at their next login, which is also
/// when Cmdr restarts. ❌ Don't go looking for a watcher here; there isn't one.
#[cfg(not(target_os = "macos"))]
pub fn observe_os_locale_changes<R: tauri::Runtime>(_app_handle: tauri::AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    use crate::test_support::wait_until;

    /// A stand-in for the OS: the answer a test can move, plus the tallies that
    /// say how often the watcher looked and how often it spoke.
    struct FakeOs {
        answer: Mutex<String>,
        reads: AtomicUsize,
        announcements: Mutex<Vec<String>>,
    }

    impl FakeOs {
        fn new(initial: &str) -> Arc<Self> {
            Arc::new(Self {
                answer: Mutex::new(initial.to_string()),
                reads: AtomicUsize::new(0),
                announcements: Mutex::new(Vec::new()),
            })
        }

        fn set_answer(&self, locale: &str) {
            *self.answer.lock_ignore_poison() = locale.to_string();
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }

        fn announcements(&self) -> Vec<String> {
            self.announcements.lock_ignore_poison().clone()
        }

        fn watcher(self: &Arc<Self>) -> Arc<LocaleWatcher<String>> {
            let reader = Arc::clone(self);
            let announcer = Arc::clone(self);
            LocaleWatcher::new(
                SETTLE,
                move || {
                    reader.reads.fetch_add(1, Ordering::SeqCst);
                    reader.answer.lock_ignore_poison().clone()
                },
                move |locale| {
                    announcer.announcements.lock_ignore_poison().push(locale);
                },
            )
        }
    }

    /// Short enough to keep the suite quick, long enough that a burst issued in
    /// one statement lands well inside it even on a loaded machine.
    const SETTLE: Duration = Duration::from_millis(50);

    /// Generous next to [`SETTLE`]: a timeout here means broken, not slow.
    const PATIENCE: Duration = Duration::from_secs(5);

    #[test]
    fn a_burst_collapses_into_one_announcement() {
        // System Settings writes language, region, and calendar separately, so
        // one user action posts several notifications. Re-resolving per
        // notification would re-render every open `t()` in every window, more
        // than once, for one change.
        let os = FakeOs::new("en");
        let watcher = os.watcher();
        os.set_answer("hu");

        for _ in 0..5 {
            watcher.notify();
        }

        wait_until(PATIENCE, "the notification burst to settle", || {
            !os.announcements().is_empty()
        });
        assert_eq!(os.announcements(), vec!["hu".to_string()]);
        // One read for the seed, one for the whole burst.
        assert_eq!(os.reads(), 2, "the burst should cost a single re-read");
    }

    #[test]
    fn an_unchanged_answer_announces_nothing() {
        // Most locale notifications don't move the UI language at all (a region
        // or calendar tweak posts the same burst). Announcing anyway would
        // re-render the whole app for nothing.
        let os = FakeOs::new("hu");
        let watcher = os.watcher();

        for _ in 0..3 {
            watcher.notify();
        }

        wait_until(PATIENCE, "the watcher to re-read the preferences", || os.reads() >= 2);
        assert!(os.announcements().is_empty(), "an unchanged answer must stay quiet");
    }

    #[test]
    fn a_region_change_is_announced_even_though_the_language_stayed() {
        // The user moves System Settings > Region from United States to Sweden
        // and leaves their language alone. Nothing about the copy changes, and
        // everything about the dates and number grouping does, so the watcher
        // has to compare the WHOLE answer rather than its language half.
        use crate::intl::OsLocales;

        let answer = Arc::new(Mutex::new(OsLocales {
            ui: Some("en".to_string()),
            format: Some("en-US".to_string()),
        }));
        let announcements = Arc::new(Mutex::new(Vec::new()));

        let reader = Arc::clone(&answer);
        let announcer = Arc::clone(&announcements);
        let watcher = LocaleWatcher::new(
            SETTLE,
            move || reader.lock_ignore_poison().clone(),
            move |locales: OsLocales| announcer.lock_ignore_poison().push(locales),
        );

        answer.lock_ignore_poison().format = Some("en-SE".to_string());
        watcher.notify();

        wait_until(PATIENCE, "the region change to be announced", || {
            !announcements.lock_ignore_poison().is_empty()
        });
        assert_eq!(
            announcements.lock_ignore_poison().clone(),
            vec![OsLocales {
                ui: Some("en".to_string()),
                format: Some("en-SE".to_string()),
            }]
        );
    }

    #[test]
    fn a_later_change_is_announced_and_then_settles_again() {
        // The watcher's memory has to track what it last said, or a second
        // change either goes missing or gets announced twice.
        let os = FakeOs::new("en");
        let watcher = os.watcher();

        os.set_answer("hu");
        watcher.notify();
        wait_until(PATIENCE, "the first change to be announced", || {
            os.announcements().len() == 1
        });

        os.set_answer("sv");
        watcher.notify();
        wait_until(PATIENCE, "the second change to be announced", || {
            os.announcements().len() == 2
        });
        assert_eq!(os.announcements(), vec!["hu".to_string(), "sv".to_string()]);

        // Same answer as the one everyone has: nothing to say.
        let reads_before = os.reads();
        watcher.notify();
        wait_until(PATIENCE, "the watcher to re-read after the third burst", || {
            os.reads() > reads_before
        });
        assert_eq!(os.announcements().len(), 2);
    }
}

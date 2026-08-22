//! Which transient files a listing leaves out of the pane.
//!
//! Cmdr writes a file under a temporary name and renames it once the last byte
//! lands, so a crash can never leave a half-written file at a real name
//! (`write_operations/transfer/staged_write.rs`). The mint itself, the naming
//! convention, and the in-flight registry are `cmdr_fs::staging`'s, so a backend
//! crate can stage a write without reaching into the app; everything re-exported
//! below keeps its `crate::file_system::staging::…` path. What's left here is the
//! part that belongs to the product: whether the user sees them.
//!
//! The cost of staging is that a copy makes files appear under names like
//! `photo.jpg.cmdr-tmp-3f2a…` for as long as it takes to write them, and a
//! directory watcher happily reports those to the pane. macOS apps do the same
//! thing when they save (`file.txt.sb-1e64c894-…`), so the pane has two kinds of
//! other people's scratch to deal with.
//!
//! ## Two categories, two rules, and why they differ
//!
//! **Cmdr's own** (`.cmdr-tmp-*`, `.cmdr-temp-*`) hides by OWNERSHIP. One a
//! running operation put there is noise that'll be gone in moments; one nobody
//! owns is a LEFTOVER from an interrupted transfer, and hiding that would be
//! lying about what's on disk — and about a Cmdr bug worth seeing. So:
//!
//! > Hide a scratch file while an operation has it open. Show every other one.
//!
//! **Other apps'** (`.sb-`, macOS safe-save) hides by NAME, because we have no
//! ownership signal for a file another process is writing and no way to tell a
//! live one from an abandoned one. The coarser rule is defensible here precisely
//! because it isn't ours: an abandoned `.sb-` is evidence of TextEdit's day, not
//! of a Cmdr bug, so nothing is lost by not surfacing it. ❌ Don't reach for this
//! rule for Cmdr's own scratch, where ownership is knowable and the leftover is
//! the interesting case. The ownership signal itself (the RAII mint, the weak
//! liveness token behind it, and why a guard alone isn't enough) is documented
//! where it lives, in `cmdr_fs::staging`.

#[cfg(test)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub use cmdr_fs::staging::{StagingTemp, is_staging_temp_in_flight};

#[cfg(test)]
use crate::ignore_poison::IgnorePoison;

/// Whether the user asked to see Cmdr's scratch files anyway
/// (`advanced.showStagingTempFiles`). Off by default.
static SHOW_STAGING_TEMPS: AtomicBool = AtomicBool::new(false);

/// Applies the `advanced.showStagingTempFiles` setting. Seeded at startup from
/// `load_settings`, then pushed on every change (settings § live-apply rule).
pub fn set_show_staging_temps(show: bool) {
    SHOW_STAGING_TEMPS.store(show, Ordering::Relaxed);
}

/// Whether Cmdr's scratch files are being shown.
pub fn show_staging_temps() -> bool {
    SHOW_STAGING_TEMPS.load(Ordering::Relaxed)
}

/// The marker macOS safe-save writes into its scratch name.
///
/// `NSDocument`'s atomic save makes `file.txt.sb-1e64c894-vFWIzN` next to the
/// original, writes the new version there, swaps it in, and deletes it. TextEdit,
/// Preview, and anything else built on AppKit's document machinery do this on
/// every save.
const SAFE_SAVE_MARKER: &str = ".sb-";

/// Whether `name` is another app's macOS safe-save scratch.
///
/// Name-only, and type-agnostic: safe-save creates a temp DIRECTORY as well as
/// the file inside it, and both are equally uninteresting.
fn is_safe_save_name(name: &str) -> bool {
    name.contains(SAFE_SAVE_MARKER)
}

/// Whether the user asked to see other apps' safe-save scratch
/// (`advanced.showSafeSaveFiles`). ON by default: unlike Cmdr's own temps, these
/// are someone else's business, and quietly hiding another app's files by name is
/// a bigger claim to make on a user's behalf than hiding our own.
static SHOW_SAFE_SAVE_FILES: AtomicBool = AtomicBool::new(true);

/// Applies the `advanced.showSafeSaveFiles` setting. Seeded at startup from
/// `load_settings`, then pushed on every change (settings § live-apply rule).
pub fn set_show_safe_save_files(show: bool) {
    SHOW_SAFE_SAVE_FILES.store(show, Ordering::Relaxed);
}

/// Whether safe-save scratch is being shown.
pub fn show_safe_save_files() -> bool {
    SHOW_SAFE_SAVE_FILES.load(Ordering::Relaxed)
}

/// The listing layer's question: should `name` be left out of the pane?
///
/// ❌ Ask this on the READ path, never when filling the cache. The cache holds
/// what's on disk; hiding happens when the frontend asks for a range. That's
/// what keeps the two from ever disagreeing: an entry the pane never received
/// can't get stuck there, and one it did receive is re-tested on the next fetch.
/// Filtering the WATCHER instead would produce exactly that stuck entry — a
/// listing shows the temp, the watcher skips the removal that would clear it,
/// and it stays in the pane pointing at nothing. The `.sb-` filter lived there
/// until 2026-08-01 and had precisely that bug.
pub fn is_hidden_from_listings(name: &str) -> bool {
    could_be_hidden_from_listings(name)
        && ((!show_staging_temps() && is_staging_temp_in_flight(name))
            || (!show_safe_save_files() && is_safe_save_name(name)))
}

/// Whether `name` is scratch by NAME: one of Cmdr's, or another app's safe-save.
///
/// Pure and stable, unlike [`is_hidden_from_listings`] — it reads no setting and
/// no ownership registry, so its answer for a given name never changes. That's
/// the point: the gate above means a name this rejects is visible now and stays
/// visible until the listing itself changes, which is what lets the listing layer
/// cache a row map once and re-ask about only these few names per read
/// (`listing/visible_rows.rs`). ❗ Keep the gate: without it the implication is
/// only true by inspection of three other functions, and a row map built on it
/// would start handing panes the wrong file.
pub fn could_be_hidden_from_listings(name: &str) -> bool {
    cmdr_fs::staging::is_staging_temp_name(name) || is_safe_save_name(name)
}

/// Serializes tests whose expectations depend on either visibility setting, and
/// restores both on drop.
///
/// The settings are process-wide, so a test flipping one would otherwise change
/// the answer under every concurrently running test that asserts something is
/// hidden. Take this in any test that reads or writes them, including the ones
/// relying on the defaults.
///
/// ❌ ONE AT A TIME. Taking a second guard while the first is alive re-locks a
/// non-reentrant `Mutex` on the same thread, which deadlocks the whole test
/// binary with no timeout to end it — every other test taking the guard then
/// piles up behind it, and the run looks like a hang rather than a failure.
/// Scope the first in a block before taking the second. [`set_both`](Self::set_both)
/// asserts on a same-thread re-take so the mistake fails one test loudly.
#[cfg(test)]
pub(crate) struct ShowTempsGuard {
    previous_staging: bool,
    previous_safe_save: bool,
    _lock: std::sync::MutexGuard<'static, ()>,
}

/// Serializes the settings across tests.
#[cfg(test)]
static SHOW_TEMPS_LOCK: Mutex<()> = Mutex::new(());

/// Which thread holds [`SHOW_TEMPS_LOCK`], so a same-thread re-take can be caught
/// before it deadlocks. Sound to read without holding the lock: only the holder
/// ever stores its own id, so seeing our own id proves we already hold it.
#[cfg(test)]
static SHOW_TEMPS_OWNER: Mutex<Option<std::thread::ThreadId>> = Mutex::new(None);

#[cfg(test)]
impl ShowTempsGuard {
    /// Sets the Cmdr-scratch setting, leaving safe-save at its default (shown).
    pub(crate) fn set(show: bool) -> Self {
        Self::set_both(show, true)
    }

    pub(crate) fn set_both(show_staging: bool, show_safe_save: bool) -> Self {
        let me = std::thread::current().id();
        assert!(
            *SHOW_TEMPS_OWNER.lock_ignore_poison() != Some(me),
            "ShowTempsGuard is already held by this thread: scope the first one in a block \
             before taking a second, or it deadlocks the test binary"
        );

        let lock = SHOW_TEMPS_LOCK.lock_ignore_poison();
        *SHOW_TEMPS_OWNER.lock_ignore_poison() = Some(me);
        let guard = Self {
            _lock: lock,
            previous_staging: show_staging_temps(),
            previous_safe_save: show_safe_save_files(),
        };
        set_show_staging_temps(show_staging);
        set_show_safe_save_files(show_safe_save);
        guard
    }
}

#[cfg(test)]
impl Drop for ShowTempsGuard {
    fn drop(&mut self) {
        set_show_staging_temps(self.previous_staging);
        set_show_safe_save_files(self.previous_safe_save);
        // Before `_lock` releases (fields drop after this body).
        *SHOW_TEMPS_OWNER.lock_ignore_poison() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    /// Stands in for a running operation: the temps it mints stay hidden until
    /// the returned `Arc` is dropped, which is what settling does for real.
    fn running_operation() -> Arc<()> {
        Arc::new(())
    }

    fn name_of(temp: &StagingTemp) -> String {
        temp.path()
            .file_name()
            .expect("a minted temp always has a file name")
            .to_string_lossy()
            .into_owned()
    }

    /// The whole point of the guard: while it lives the file is hidden, and the
    /// moment it drops it isn't.
    #[test]
    fn a_live_guard_hides_its_temp_and_dropping_it_stops() {
        let _show = ShowTempsGuard::set(false);
        let op = running_operation();
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), Some(Arc::downgrade(&op)));
        let name = name_of(&temp);

        assert!(is_hidden_from_listings(&name));
        drop(temp);
        assert!(!is_hidden_from_listings(&name));
    }

    /// David's first edge case: an operation that wedges and leaves its temps on
    /// disk must not leave them hidden too. Its guard may never drop — the driver
    /// abandons a task that won't wind down, and the task keeps holding it — so
    /// the operation's liveness is what has to answer.
    #[test]
    fn a_leftover_from_a_dead_operation_is_visible_even_if_its_guard_leaked() {
        let _show = ShowTempsGuard::set(false);
        let op = running_operation();
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), Some(Arc::downgrade(&op)));
        let name = name_of(&temp);
        assert!(is_hidden_from_listings(&name), "hidden while the operation runs");

        drop(op);

        // The guard deliberately stays alive, standing in for the abandoned task.
        assert!(
            !is_hidden_from_listings(&name),
            "a leftover nobody is running for must be visible"
        );
        drop(temp);
    }

    /// Ordinary files are never touched.
    #[test]
    fn an_ordinary_file_is_never_hidden() {
        let _show = ShowTempsGuard::set(false);
        let _temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        assert!(!is_hidden_from_listings("photo.jpg"));
        assert!(!is_hidden_from_listings(".gitignore"));
    }

    /// Safe-save scratch shows by default: it's another app's business, and
    /// hiding someone else's files by name is a bigger claim than hiding our own.
    #[test]
    fn safe_save_scratch_is_shown_by_default() {
        let _show = ShowTempsGuard::set_both(false, true);
        assert!(!is_hidden_from_listings("notes.txt.sb-1e64c894-vFWIzN"));
    }

    /// Turning the setting off hides it purely by name: no operation owns another
    /// app's scratch, so there's nothing else to go on.
    #[test]
    fn safe_save_scratch_hides_by_name_alone() {
        let _show = ShowTempsGuard::set_both(false, false);
        assert!(is_hidden_from_listings("notes.txt.sb-1e64c894-vFWIzN"));
        assert!(
            is_hidden_from_listings("notes.txt.sb-1e64c894"),
            "the temp DIRECTORY hides too, not just the file inside it"
        );
        assert!(!is_hidden_from_listings("notes.txt"));
    }

    /// The two categories are independent switches, so neither setting can
    /// silently change what the other one does.
    #[test]
    fn the_two_scratch_settings_are_independent() {
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        let cmdr_name = name_of(&temp);
        let safe_save_name = "notes.txt.sb-1e64c894-vFWIzN";

        // One guard at a time: the second must not be taken until the first has
        // dropped (see `ShowTempsGuard`).
        {
            let _show = ShowTempsGuard::set_both(true, false);
            assert!(!is_hidden_from_listings(&cmdr_name), "Cmdr scratch shown");
            assert!(is_hidden_from_listings(safe_save_name), "safe-save hidden");
        }
        {
            let _show = ShowTempsGuard::set_both(false, true);
            assert!(is_hidden_from_listings(&cmdr_name), "Cmdr scratch hidden");
            assert!(!is_hidden_from_listings(safe_save_name), "safe-save shown");
        }
    }

    /// The Settings > Advanced escape hatch overrides the hiding, not the
    /// registry: what's in flight is still in flight, it's just shown.
    #[test]
    fn the_setting_shows_in_flight_temps() {
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        let name = name_of(&temp);

        let _show = ShowTempsGuard::set(true);
        assert!(!is_hidden_from_listings(&name));
        assert!(is_staging_temp_in_flight(&name), "still in flight, just not hidden");
    }
}

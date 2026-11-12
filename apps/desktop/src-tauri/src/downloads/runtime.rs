//! Process-wide handle to the running [`super::DownloadsWatcher`].
//!
//! The watcher is FDA-gated: it's alive whenever
//! `crate::fda_gate::is_fda_pending_runtime() == false`. `lib.rs` calls
//! [`refresh_runtime`] at startup and on main-window focus transitions to
//! keep the state aligned. The Settings pane also calls it on mount as a
//! belt-and-braces re-check (the focus event may have fired on a stale gate
//! read).
//!
//! Stored in a `Mutex<Option<...>>` rather than `OnceLock` because the
//! handle's lifetime tracks the FDA gate: it can flip from `None` to `Some`
//! and back over a single session (the deny path keeps the app running and
//! re-enables features as the user grants individual TCC prompts; an allow
//! requires a restart but a mid-session revoke in System Settings is also
//! possible).

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use tauri::AppHandle;

use super::global_shortcut::{GlobalShortcutManager, RegistrationError, RegistrationStatus, TauriRegistrar};
use super::watcher::DEFAULT_IGNORE_TTL;
use super::{DownloadsWatcher, WatcherError, desired_running};
use cmdr_menu::spec::PerPlatform;

/// Process-global handle. `None` when the watcher isn't running.
static RUNTIME: Mutex<Option<DownloadsWatcher>> = Mutex::new(None);

/// Process-global global-shortcut manager. Lazily constructed on first
/// `apply_global_go_to_latest_shortcut` call so tests don't need a real
/// `AppHandle`.
static GLOBAL_SHORTCUT: Mutex<Option<GlobalShortcutManager<TauriRegistrar>>> = Mutex::new(None);

/// Last `(enabled, binding)` pair `refresh_global_go_to_latest_shortcut` applied.
/// Lets the focus-event refresh short-circuit when nothing changed since the
/// previous call. The headline path is `Cmd-Tab Chrome → Cmdr`: focus fires,
/// settings on disk haven't moved, no disk read or plugin round-trip needed.
static LAST_APPLIED_SHORTCUT: Mutex<Option<(bool, String)>> = Mutex::new(None);

/// Pure helper: compare `(enabled, binding)` against `cache`. Returns `true`
/// when the cache mismatches (or is empty) and updates it to the new value;
/// returns `false` when the cache already matches (caller should skip the
/// re-apply). Extracted so the cache-skip logic is unit-testable without an
/// `AppHandle` or the process-global cache.
fn cache_changed_and_update(cache: &Mutex<Option<(bool, String)>>, enabled: bool, binding: &str) -> bool {
    let mut guard = cache.lock().expect("last-applied shortcut cache poisoned");
    if let Some((cached_enabled, cached_binding)) = guard.as_ref()
        && *cached_enabled == enabled
        && cached_binding == binding
    {
        return false;
    }
    *guard = Some((enabled, binding.to_string()));
    true
}

fn global_shortcut_manager(
    app: &AppHandle,
) -> std::sync::MutexGuard<'_, Option<GlobalShortcutManager<TauriRegistrar>>> {
    let mut guard = GLOBAL_SHORTCUT.lock().expect("global_shortcut runtime poisoned");
    if guard.is_none() {
        *guard = Some(GlobalShortcutManager::new(TauriRegistrar::new(app.clone())));
    }
    guard
}

/// Start the watcher if the FDA gate is open and we aren't already running;
/// stop it if the gate is closed and we are. Idempotent.
///
/// Returns `Err` only when the watcher couldn't start due to `notify`
/// errors (missing Downloads dir, permission-denied watch attach). Callers
/// log and move on; the next focus event will retry.
pub fn refresh_runtime(app: &AppHandle) -> Result<(), WatcherError> {
    let should_run = desired_running(crate::fda_gate::is_fda_pending_runtime());
    let mut guard = RUNTIME.lock().expect("downloads runtime poisoned");
    match (should_run, guard.is_some()) {
        (true, false) => {
            let watcher = DownloadsWatcher::start(app)?;
            *guard = Some(watcher);
            log::info!(target: "downloads::watcher", "Downloads watcher started (FDA gate open)");
        }
        (false, true) => {
            if let Some(watcher) = guard.take() {
                watcher.stop();
                log::info!(target: "downloads::watcher", "Downloads watcher stopped (FDA gate closed)");
            }
        }
        _ => {
            // Already aligned with desired state; nothing to do.
        }
    }
    Ok(())
}

/// The binding a fresh install runs with, per OS, each in its own spelling: the same physical
/// chord everywhere for now. Mirrors the registry's `platformDefault` for
/// `behavior.fileSystemWatching.globalGoToLatestShortcut.binding` (`settings/definitions/behavior.ts`),
/// which a missing settings key falls back to before any window loads. Data, so no `cfg`:
/// [`default_binding`] picks this build's arm.
pub const DEFAULT_BINDINGS: PerPlatform<&str> = PerPlatform {
    macos: "\u{2303}\u{2325}\u{2318}J", // ⌃⌥⌘J
    windows: "Ctrl+Alt+Super+J",
    linux: "Ctrl+Alt+Super+J",
};

/// This OS's default binding ([`DEFAULT_BINDINGS`]).
pub fn default_binding() -> &'static str {
    DEFAULT_BINDINGS.current()
}

/// Each spelling's modifier token → its accelerator name: macOS glyphs, then the words Windows
/// and Linux write.
const MODIFIER_TOKENS: [(&str, &str); 8] = [
    ("\u{2303}", "Control"), // ⌃
    ("\u{2325}", "Alt"),     // ⌥
    ("\u{21E7}", "Shift"),   // ⇧
    ("\u{2318}", "Super"),   // ⌘
    ("Ctrl+", "Control"),
    ("Alt+", "Alt"),
    ("Shift+", "Shift"),
    ("Super+", "Super"),
];

/// Translate the stored binding (the canonical combo the recorder writes on this OS: `'⌃⌥⌘J'`
/// on macOS, `'Ctrl+Alt+Super+J'` on Windows and Linux) into the accelerator string the Tauri
/// global-shortcut plugin understands (`'Control+Alt+Super+J'`). Returns `None` for empty or
/// modifier-only input — global shortcuts always need a non-modifier key.
///
/// Mirrors `apps/desktop/src/lib/downloads/global-shortcut-binding.ts`; both are unit-tested.
pub fn binding_to_accelerator(binding: &str) -> Option<String> {
    let mut modifiers: Vec<&'static str> = Vec::new();
    let mut rest = binding;
    // A token never eats the last character, so a key that looks like a token's tail stays the key.
    while let Some((token, name)) = MODIFIER_TOKENS
        .iter()
        .find(|(token, _)| rest.starts_with(token) && rest.len() > token.len())
    {
        if !modifiers.contains(name) {
            modifiers.push(*name);
        }
        rest = &rest[token.len()..];
    }
    let key = rest.trim();
    let modifier_only = MODIFIER_TOKENS
        .iter()
        .any(|(token, _)| token.trim_end_matches('+') == key);
    if key.is_empty() || modifier_only || modifiers.is_empty() {
        return None;
    }
    let mut out = modifiers.join("+");
    out.push('+');
    out.push_str(&key.to_uppercase());
    Some(out)
}

/// Everything that decides whether the go-to-latest hotkey is registered.
struct ShortcutGates {
    /// The user's toggle.
    enabled: bool,
    /// The FDA gate is open (the user decided about Full Disk Access).
    fda_open: bool,
    /// The binding parsed to an accelerator.
    has_accelerator: bool,
    /// `test_mode::may_register_global_hotkeys`: false in an automated run.
    may_register_global_hotkeys: bool,
    /// The OS lets a shortcut fire from any app (`PlatformCapabilities::global_shortcuts`):
    /// false on a Wayland session, where the X11 grab fires only inside XWayland windows.
    os_offers_global_shortcuts: bool,
}

/// Whether the hotkey should be registered right now. The setting stays as the
/// user left it either way; only the OS registration follows this.
fn should_register_shortcut(gates: ShortcutGates) -> bool {
    gates.enabled
        && gates.fda_open
        && gates.has_accelerator
        && gates.may_register_global_hotkeys
        && gates.os_offers_global_shortcuts
}

/// Apply a Settings change (toggle + binding) to the live global-shortcut
/// registration. Idempotent. Re-evaluates the FDA gate so the call also
/// covers the "I just toggled FDA" path.
///
/// Returns the resulting status snapshot the FE row should display.
pub fn apply_global_go_to_latest_shortcut(
    app: &AppHandle,
    enabled: bool,
    binding: &str,
) -> Result<super::commands::GlobalGoToLatestShortcutState, RegistrationError> {
    let accelerator = binding_to_accelerator(binding);
    let should_run = should_register_shortcut(ShortcutGates {
        enabled,
        fda_open: !crate::fda_gate::is_fda_pending_runtime(),
        has_accelerator: accelerator.is_some(),
        may_register_global_hotkeys: crate::test_mode::may_register_global_hotkeys(),
        os_offers_global_shortcuts: crate::platform::capabilities().global_shortcuts,
    });

    let mut guard = global_shortcut_manager(app);
    // `unwrap` is safe: `global_shortcut_manager` always populates `Some`.
    let mgr = guard.as_mut().expect("global_shortcut manager initialized");

    let (status, register_error) = if should_run {
        // Safe: `should_run` implies `accelerator.is_some()`.
        let accel = accelerator.as_deref().expect("should_run implies accelerator present");
        match mgr.register(accel) {
            Ok(()) => (RegistrationStatus::Registered, None),
            Err(err) => (mgr.registration_status(accel), Some(err)),
        }
    } else {
        mgr.unregister();
        (RegistrationStatus::NotRegistered, None)
    };

    if let Some(err) = register_error {
        return Err(err);
    }

    // Sync the focus-refresh cache so a subsequent focus event doesn't undo
    // (or pointlessly re-run) what the FE just applied.
    cache_changed_and_update(&LAST_APPLIED_SHORTCUT, enabled, binding);

    Ok(super::commands::GlobalGoToLatestShortcutState {
        status,
        binding: binding.to_string(),
        enabled,
    })
}

/// Refresh the global-shortcut registration based on the FDA gate alone.
/// Used by the focus-event lifecycle to undo/redo the registration without
/// the FE re-issuing a `set_global_go_to_latest_shortcut` call. Reads the
/// last-known `enabled` and `binding` from the Settings file via the
/// existing settings loader.
pub fn refresh_global_go_to_latest_shortcut(app: &AppHandle) {
    let (enabled, binding) =
        crate::settings::early_load_global_go_to_latest_shortcut().unwrap_or((true, String::from(default_binding())));
    if !cache_changed_and_update(&LAST_APPLIED_SHORTCUT, enabled, &binding) {
        // Cmd-Tab back from another app is the headline trigger; settings on
        // disk haven't moved since the previous refresh, so skip the plugin
        // round-trip.
        log::debug!(
            target: "downloads::global_shortcut",
            "Refresh skipped: cached (enabled={enabled}, binding={binding}) matches",
        );
        return;
    }
    if let Err(err) = apply_global_go_to_latest_shortcut(app, enabled, &binding) {
        // Debug, not warn: `GlobalShortcutManager::register` already warned about it.
        log::debug!(
            target: "downloads::global_shortcut",
            "Focus-driven refresh of global shortcut failed: {err}",
        );
    }
}

/// Is the watcher currently running?
pub fn is_running() -> bool {
    RUNTIME.lock().expect("downloads runtime poisoned").is_some()
}

/// Apply a closure to the running watcher, returning its value if the
/// watcher exists. `None` when the watcher is dormant (FDA gate closed or
/// startup hasn't completed).
pub fn with_watcher<R>(f: impl FnOnce(&DownloadsWatcher) -> R) -> Option<R> {
    let guard = RUNTIME.lock().expect("downloads runtime poisoned");
    guard.as_ref().map(f)
}

/// Register a Cmdr-own pending write so the watcher suppresses the matching
/// FS event. Call this just before issuing the write syscall.
///
/// Silently no-ops in two cases:
/// 1. The watcher isn't running (FDA gate closed, or startup hasn't reached
///    `refresh_runtime` yet).
/// 2. `path` isn't under the resolved Downloads root — the filter lives
///    inside [`super::IgnoreSet::note_pending`], so call sites invoke
///    unconditionally without per-call prefix guards.
///
/// Uses [`DEFAULT_IGNORE_TTL`] (5 s).
pub fn note_pending_write_for_cmdr(path: &Path) {
    note_pending_write_for_cmdr_with_ttl(path, DEFAULT_IGNORE_TTL);
}

/// As [`note_pending_write_for_cmdr`] but with a caller-chosen TTL. Tests
/// use this to shrink/grow the window; production code should use the
/// default.
pub fn note_pending_write_for_cmdr_with_ttl(path: &Path, ttl: Duration) {
    // Before the watcher check: tests run with no watcher installed.
    #[cfg(test)]
    super::own_write_recorder::note(path);
    with_watcher(|w| w.note_pending_write(path.to_path_buf(), ttl));
}

/// Test-only: install `watcher` as the process-global handle and return a
/// guard that uninstalls (and stops) it on drop. Used by write-op tests to
/// drive the Cmdr-own-write hook contract end-to-end against a tempdir-backed
/// watcher.
///
/// Asserts that no watcher is currently installed; mixing two install
/// scopes in the same process would silently overwrite. The guard's drop
/// only restores `None`, so production tests that legitimately have a
/// running watcher (none in this crate today) would still need a tighter
/// scoping primitive — file that bridge if it's ever needed.
#[cfg(test)]
pub fn install_for_test(watcher: DownloadsWatcher) -> TestInstallGuard {
    let mut guard = RUNTIME.lock().expect("downloads runtime poisoned");
    assert!(
        guard.is_none(),
        "install_for_test: a watcher is already installed; tests must run serial or scope their installs"
    );
    *guard = Some(watcher);
    TestInstallGuard { _private: () }
}

/// RAII guard that uninstalls the test watcher on drop. See
/// [`install_for_test`].
#[cfg(test)]
pub struct TestInstallGuard {
    _private: (),
}

#[cfg(test)]
impl Drop for TestInstallGuard {
    fn drop(&mut self) {
        let mut guard = RUNTIME.lock().expect("downloads runtime poisoned");
        if let Some(watcher) = guard.take() {
            watcher.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the Cmdr-own-write hook helpers. The process-global `RUNTIME` is
    //! shared across all tests in this crate; serialize installs through
    //! `INSTALL_LOCK` so concurrent threads (nextest defaults to
    //! `test-threads = num-cpus`) don't race on the `assert!(guard.is_none())`
    //! in `install_for_test`.
    use super::*;
    use crate::downloads::DownloadsWatcher;
    use crate::downloads::watcher::{DownloadDetectedEvent, EventSink};
    use std::sync::Mutex as StdMutex;
    use std::sync::mpsc;
    use std::sync::{Arc, OnceLock};

    fn all_gates_open() -> ShortcutGates {
        ShortcutGates {
            enabled: true,
            fda_open: true,
            has_accelerator: true,
            may_register_global_hotkeys: true,
            os_offers_global_shortcuts: true,
        }
    }

    #[test]
    fn both_spellings_of_a_binding_become_the_same_accelerator() {
        assert_eq!(
            binding_to_accelerator("\u{2303}\u{2325}\u{2318}J").as_deref(),
            Some("Control+Alt+Super+J")
        );
        assert_eq!(
            binding_to_accelerator("Ctrl+Alt+Super+J").as_deref(),
            Some("Control+Alt+Super+J")
        );
        assert_eq!(
            binding_to_accelerator("Ctrl+Shift+k").as_deref(),
            Some("Control+Shift+K")
        );
        assert_eq!(binding_to_accelerator("Ctrl++").as_deref(), Some("Control++"));
    }

    #[test]
    fn a_binding_without_a_modifier_or_a_key_registers_nothing() {
        for binding in ["", "J", "\u{2318}\u{21E7}", "Ctrl+Alt", "Ctrl+"] {
            assert_eq!(binding_to_accelerator(binding), None, "{binding}");
        }
    }

    #[test]
    fn the_default_binding_registers_the_same_chord_on_every_os() {
        for platform in cmdr_menu::spec::Platform::ALL {
            assert_eq!(
                binding_to_accelerator(DEFAULT_BINDINGS.on(platform)).as_deref(),
                Some("Control+Alt+Super+J"),
                "{platform:?}"
            );
        }
    }

    #[test]
    fn the_hotkey_stays_off_where_the_os_has_no_global_shortcuts() {
        assert!(!should_register_shortcut(ShortcutGates {
            os_offers_global_shortcuts: false,
            ..all_gates_open()
        }));
    }

    #[test]
    fn the_hotkey_registers_when_every_gate_is_open() {
        assert!(should_register_shortcut(all_gates_open()));
    }

    /// An E2E app holding `⌃⌥⌘J` caught the developer's own press mid-suite: the
    /// first-trigger warn toast leaked into `mcp-archive-password.spec.ts` and failed it.
    #[test]
    fn an_automated_run_never_registers_the_hotkey() {
        assert!(!should_register_shortcut(ShortcutGates {
            may_register_global_hotkeys: false,
            ..all_gates_open()
        }));
    }

    #[test]
    fn the_hotkey_stays_off_while_the_fda_gate_is_closed() {
        assert!(!should_register_shortcut(ShortcutGates {
            fda_open: false,
            ..all_gates_open()
        }));
    }

    fn install_lock() -> &'static StdMutex<()> {
        static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| StdMutex::new(()))
    }

    /// Test sink that forwards `download-detected` events to an mpsc channel.
    struct ChannelSink(mpsc::Sender<DownloadDetectedEvent>);

    impl EventSink for ChannelSink {
        fn emit(&self, event: DownloadDetectedEvent) {
            let _ = self.0.send(event);
        }
    }

    fn unhidden_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("cmdr-downloads-runtime-test-")
            .tempdir()
            .expect("tempdir")
    }

    #[test]
    fn note_pending_write_for_cmdr_is_noop_when_watcher_absent() {
        // No watcher installed; the helper must silently no-op (no panic, no
        // mutex poison, no stale state). This is the production startup case
        // where write ops can fire before `refresh_runtime` lands.
        let _lock = install_lock().lock().unwrap_or_else(|p| p.into_inner());
        // Defensive: clear any leftover from a previously panicking test.
        {
            let mut g = RUNTIME.lock().expect("downloads runtime poisoned");
            if let Some(w) = g.take() {
                w.stop();
            }
        }
        assert!(!is_running(), "precondition: no watcher installed");

        note_pending_write_for_cmdr(Path::new("/tmp/anything"));
        note_pending_write_for_cmdr_with_ttl(Path::new("/tmp/x"), Duration::from_millis(50));

        assert!(!is_running(), "helpers must not install a watcher");
    }

    #[test]
    fn note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end() {
        // End-to-end safety net for the Cmdr-own-write hook contract. Mirrors the
        // headline regression case from the plan: a Cmdr-own write into the
        // watched dir, registered via the public helper, must NOT produce a
        // `download-detected` event.
        let _lock = install_lock().lock().unwrap_or_else(|p| p.into_inner());
        // Clean any leftover.
        {
            let mut g = RUNTIME.lock().expect("downloads runtime poisoned");
            if let Some(w) = g.take() {
                w.stop();
            }
        }

        let downloads_root = unhidden_tempdir();
        let (tx, rx) = mpsc::channel::<DownloadDetectedEvent>();
        let sink: Arc<dyn EventSink> = Arc::new(ChannelSink(tx));
        let watcher = DownloadsWatcher::scripted_at(downloads_root.path().to_path_buf(), sink);
        let _guard = install_for_test(watcher);

        // Canonicalize parent so the recorded path matches what `notify`
        // delivers (macOS firmlinks: /var → /private/var).
        let canonical_root = std::fs::canonicalize(downloads_root.path()).expect("canonicalize");
        let dest = canonical_root.join("cmdr-wrote-this.bin");
        let control = canonical_root.join("browser-wrote-this.bin");

        // Hook → write. Production write-op call sites follow this order.
        note_pending_write_for_cmdr(&dest);
        std::fs::write(&dest, b"payload").expect("write");
        std::fs::write(&control, b"payload").expect("write");

        // The batch FSEvents would report, handed to the INSTALLED watcher: the
        // hook has to have reached the same instance the events go through.
        let created = |path: &Path| {
            let event = notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![path.to_path_buf()],
                attrs: Default::default(),
            };
            notify_debouncer_full::DebouncedEvent::new(event, std::time::Instant::now())
        };
        RUNTIME
            .lock()
            .expect("downloads runtime poisoned")
            .as_ref()
            .expect("the test watcher is installed")
            .deliver(vec![created(&dest), created(&control)]);

        // The unregistered sibling surfacing proves the batch was handled, so
        // the Cmdr-own write's silence is suppression, not a watch that saw nothing.
        let surfaced: Vec<String> = rx.try_iter().map(|e| e.file_name).collect();
        assert_eq!(surfaced, vec!["browser-wrote-this.bin"]);
    }

    #[test]
    fn cache_changed_and_update_returns_true_first_time_and_false_when_unchanged() {
        // Headline contract: the focus-refresh fast path skips the plugin
        // round-trip when nothing has moved. Two refreshes in a row with the
        // same (enabled, binding) must return `true` then `false`, so the
        // caller calls `apply_global_go_to_latest_shortcut` only once.
        let cache: Mutex<Option<(bool, String)>> = Mutex::new(None);

        // First refresh: cache empty → caller should apply.
        assert!(cache_changed_and_update(&cache, true, "\u{2303}\u{2325}\u{2318}J"));
        // Second refresh with same args: cache hit → caller should skip.
        assert!(!cache_changed_and_update(&cache, true, "\u{2303}\u{2325}\u{2318}J"));

        // Toggle changes → re-apply.
        assert!(cache_changed_and_update(&cache, false, "\u{2303}\u{2325}\u{2318}J"));
        assert!(!cache_changed_and_update(&cache, false, "\u{2303}\u{2325}\u{2318}J"));

        // Binding changes → re-apply.
        assert!(cache_changed_and_update(&cache, false, "\u{2318}\u{21E7}K"));
        assert!(!cache_changed_and_update(&cache, false, "\u{2318}\u{21E7}K"));
    }

    #[test]
    fn note_pending_write_for_cmdr_outside_downloads_silently_noops() {
        // The IgnoreSet's downloads-root prefix gate is the locked-in
        // scoping decision: call sites invoke unconditionally and paths
        // outside the watched root no-op without touching the map.
        // Indirectly verified via the public helper.
        let _lock = install_lock().lock().unwrap_or_else(|p| p.into_inner());
        {
            let mut g = RUNTIME.lock().expect("downloads runtime poisoned");
            if let Some(w) = g.take() {
                w.stop();
            }
        }

        let downloads_root = unhidden_tempdir();
        let (tx, _rx) = mpsc::channel::<DownloadDetectedEvent>();
        let sink: Arc<dyn EventSink> = Arc::new(ChannelSink(tx));
        let watcher = DownloadsWatcher::scripted_at(downloads_root.path().to_path_buf(), sink);
        let _guard = install_for_test(watcher);

        // Path outside the watched tempdir: helper must succeed silently.
        note_pending_write_for_cmdr(Path::new("/usr/local/elsewhere/nope.bin"));
        // No assertion on internal state needed; the absence of a panic
        // and the watcher still being installed is the contract.
        assert!(is_running());
    }
}

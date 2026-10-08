//! System-wide go-to-latest-download hotkey (default `⌃⌥⌘J`).
//!
//! Thin wrapper around `tauri-plugin-global-shortcut` so the rest of the
//! crate sees a typed-error registration API and the production code path
//! is decoupled from the plugin for testing.
//!
//! ## State machine
//!
//! The registrar tracks at most one active binding per process. `register`
//! is idempotent: re-registering the same binding is a no-op; re-registering
//! a different binding unregisters the previous one first. `unregister`
//! drops the current binding (if any) and is also idempotent.
//!
//! ## FDA gate
//!
//! The hotkey is registered iff
//! `settings.globalGoToLatestShortcut.enabled == true` AND
//! `fda_gate::is_fda_pending_runtime() == false`. The lifecycle wiring in
//! `lib.rs` calls [`refresh_runtime`](super::runtime::refresh_runtime) at startup, on main-window focus, and
//! when the FE flips the setting via [`set_global_go_to_latest_shortcut`](super::commands::set_global_go_to_latest_shortcut); this
//! module only owns the typed register/unregister/status surface.
//!
//! ## macOS permission scope
//!
//! No Accessibility or Input Monitoring grant required. The plugin uses
//! Carbon's `RegisterEventHotKey` (a system-API event hook in-process),
//! which is distinct from key-logging APIs that need TCC grants. The user
//! sees no extra prompt.

use std::str::FromStr;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

/// `global-shortcut-fired`: the system-wide go-to-latest hotkey (default
/// `⌃⌥⌘J`) fired. Payloadless for now — the FE bridge calls
/// `goToLatestDownload(explorer)` directly. A unit struct so future per-binding
/// metadata can be added without breaking the event registration.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct GlobalShortcutFired;

/// Typed errors from a registration attempt. The FE branches on `kind`;
/// never match on the message string.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RegistrationError {
    /// The accelerator string couldn't be parsed. Detected pre-plugin via
    /// `Shortcut::from_str` so we never reach the plugin's stringly error
    /// surface for the one failure mode the user can act on directly
    /// ("typo in the combo").
    InvalidBinding {
        /// The rejected binding so the FE can surface it (for debugging only;
        /// the row already shows the binding the user picked).
        binding: String,
    },
    /// The OS refused the hotkey, most likely because another app holds the
    /// combo. The plugin's `Error::GlobalHotkey` arm: it flattens
    /// `global_hotkey`'s typed `AlreadyRegistered` / `FailedToRegister` into a
    /// string, so the arm is the typed signal and the reason isn't knowable.
    /// The row says "Another app may be using that combo".
    Unavailable { message: String },
    /// Any other plugin failure (its internal channel, the Tauri runtime).
    /// Nothing the user can act on; the message is for the log only.
    PluginError { message: String },
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBinding { binding } => write!(f, "Invalid global shortcut binding: {binding}"),
            Self::Unavailable { message } => write!(f, "The OS refused the global shortcut: {message}"),
            Self::PluginError { message } => write!(f, "Global shortcut plugin error: {message}"),
        }
    }
}

impl From<tauri_plugin_global_shortcut::Error> for RegistrationError {
    fn from(err: tauri_plugin_global_shortcut::Error) -> Self {
        match err {
            tauri_plugin_global_shortcut::Error::GlobalHotkey(message) => Self::Unavailable { message },
            other => Self::PluginError {
                message: other.to_string(),
            },
        }
    }
}

/// Snapshot of the registrar's state for the Settings row indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationStatus {
    /// The binding is live; the hotkey will fire from any app.
    Registered,
    /// Nothing is currently registered (disabled, FDA gate closed, empty
    /// binding, or the most recent attempt failed).
    NotRegistered,
}

/// Abstraction over the plugin so the state-machine tests don't need a real
/// `AppHandle`. Production uses [`TauriRegistrar`]; tests use an in-memory
/// fake.
pub trait Registrar {
    /// Register the binding. A combo the OS refuses (another app holds it)
    /// lands in `Unavailable`, an unparsable one in `InvalidBinding`, anything
    /// else in `PluginError`.
    fn plugin_register(&self, binding: &str) -> Result<(), RegistrationError>;
    /// Unregister the binding. Idempotent; missing-binding errors are
    /// swallowed because the caller's mental model is "make sure it's gone."
    fn plugin_unregister(&self, binding: &str);
}

/// Production registrar: thin pass-through to the Tauri plugin. Owns its
/// `AppHandle` clone so the manager can live in process-global state without
/// borrow gymnastics.
pub struct TauriRegistrar {
    app: AppHandle,
}

impl TauriRegistrar {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Registrar for TauriRegistrar {
    fn plugin_register(&self, binding: &str) -> Result<(), RegistrationError> {
        // Pre-parse via `Shortcut::from_str` so the one user-actionable
        // failure mode (typo in the combo) is detected cheaply before we
        // touch the plugin. What survives the parse but fails to register is
        // classified by the plugin's error arm (`From` above), never by its
        // message.
        if Shortcut::from_str(binding).is_err() {
            return Err(RegistrationError::InvalidBinding {
                binding: binding.to_string(),
            });
        }
        self.app
            .global_shortcut()
            .register(binding)
            .map_err(RegistrationError::from)
    }

    fn plugin_unregister(&self, binding: &str) {
        // Best-effort unregister: any error here is logged but not surfaced.
        // The most common "error" is "wasn't registered," which is exactly
        // the post-condition the caller wanted.
        if let Err(err) = self.app.global_shortcut().unregister(binding) {
            log::debug!(
                target: "downloads::global_shortcut",
                "unregister({binding}) reported: {err}",
            );
        }
    }
}

/// State machine on top of any [`Registrar`]. Holds at most one active
/// binding.
pub struct GlobalShortcutManager<R: Registrar> {
    registrar: R,
    state: Mutex<ManagerState>,
}

#[derive(Debug, Default, Clone)]
struct ManagerState {
    /// `Some(binding)` when we've successfully registered it. `None` when
    /// we're not currently holding any binding.
    active: Option<String>,
}

impl<R: Registrar> GlobalShortcutManager<R> {
    pub fn new(registrar: R) -> Self {
        Self {
            registrar,
            state: Mutex::new(ManagerState::default()),
        }
    }

    /// Register `binding`. Idempotent for the currently active binding;
    /// swaps cleanly when a different binding arrives, and when the new one
    /// is refused, puts the previous one back so the user keeps a hotkey.
    pub fn register(&self, binding: &str) -> Result<(), RegistrationError> {
        let mut state = self.state.lock().expect("global_shortcut state poisoned");

        if state.active.as_deref() == Some(binding) {
            // Already active — re-registering would re-acquire and risk
            // double-events. Idempotent re-call.
            return Ok(());
        }

        // Swap: drop the previous binding (if any) before attaching the new one
        // so the OS doesn't briefly hold two registrations.
        let prev = state.active.take();
        if let Some(prev) = &prev {
            self.registrar.plugin_unregister(prev);
        }

        match self.registrar.plugin_register(binding) {
            Ok(()) => {
                state.active = Some(binding.to_string());
                log::info!(
                    target: "downloads::global_shortcut",
                    "Registered global shortcut: {binding}",
                );
                Ok(())
            }
            Err(err) => {
                log::warn!(
                    target: "downloads::global_shortcut",
                    "Global shortcut register({binding}) failed: {err}",
                );
                if let Some(prev) = prev {
                    self.restore(&mut state, prev);
                }
                Err(err)
            }
        }
    }

    /// Re-register the binding a refused swap just dropped. We released it
    /// ourselves a moment ago, so this normally succeeds; if it doesn't, the
    /// user is left without a hotkey and the log says why.
    fn restore(&self, state: &mut ManagerState, prev: String) {
        match self.registrar.plugin_register(&prev) {
            Ok(()) => {
                log::info!(
                    target: "downloads::global_shortcut",
                    "Kept the previous global shortcut: {prev}",
                );
                state.active = Some(prev);
            }
            Err(err) => {
                log::warn!(
                    target: "downloads::global_shortcut",
                    "Couldn't restore the previous global shortcut {prev}: {err}",
                );
            }
        }
    }

    /// Drop the active binding, if any. Idempotent.
    pub fn unregister(&self) {
        let mut state = self.state.lock().expect("global_shortcut state poisoned");
        if let Some(prev) = state.active.take() {
            self.registrar.plugin_unregister(&prev);
            log::info!(
                target: "downloads::global_shortcut",
                "Unregistered global shortcut: {prev}",
            );
        }
    }

    /// Status for the requested `binding`. The Settings row consults this on
    /// mount and after every flip.
    pub fn registration_status(&self, binding: &str) -> RegistrationStatus {
        let state = self.state.lock().expect("global_shortcut state poisoned");
        if state.active.as_deref() == Some(binding) {
            RegistrationStatus::Registered
        } else {
            RegistrationStatus::NotRegistered
        }
    }
}

/// Plugin builder with the handler that forwards every triggered shortcut to
/// the frontend via the `global-shortcut-fired` event. Called once from
/// `lib.rs` when constructing the Tauri builder.
///
/// We use one shared handler instead of per-shortcut handlers because the
/// plugin's design is "any registered shortcut routes through the
/// callback"; the FE bridge doesn't care which binding triggered (there's
/// only ever one active for now), so the routing is trivial.
pub fn plugin_builder() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri::Manager as _;
    use tauri_specta::Event as _;
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app: &AppHandle, _shortcut, event: ShortcutEvent| {
            // Fire on key-down only; key-up would double-trigger.
            if event.state() != ShortcutState::Pressed {
                return;
            }
            // The whole point of the global hotkey is "I'm in Chrome, take me
            // to my download." Going to the file alone isn't enough — the user
            // can't see the result behind the foreground app. Raise the main
            // window before emitting so the jump lands on a visible, focused pane.
            // unminimize → show covers the minimized / hidden cases; set_focus
            // brings it in front of the current app and onto the active Space.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                if let Err(err) = window.set_focus() {
                    log::warn!(
                        target: "downloads::global_shortcut",
                        "Failed to focus main window on global shortcut: {err}",
                    );
                }
            }
            // Payloadless: the FE bridge calls `goToLatestDownload(explorer)`
            // directly. The unit struct keeps room for future per-binding
            // metadata without breaking the event registration.
            if let Err(err) = GlobalShortcutFired.emit(app) {
                log::warn!(
                    target: "downloads::global_shortcut",
                    "Failed to emit global-shortcut-fired: {err}",
                );
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    //! Tests run against an in-memory `FakeRegistrar` so we never touch the
    //! real Tauri plugin. The plugin's behavior is its own contract; we test
    //! the state machine on top.
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRegistrarState {
        register_calls: Vec<String>,
        unregister_calls: Vec<String>,
        active: Option<String>,
        force_error: Option<RegistrationError>,
    }

    struct FakeRegistrar {
        state: Mutex<FakeRegistrarState>,
    }

    impl FakeRegistrar {
        fn new() -> Self {
            Self {
                state: Mutex::new(FakeRegistrarState::default()),
            }
        }

        fn set_next_error(&self, err: RegistrationError) {
            self.state.lock().unwrap().force_error = Some(err);
        }

        fn register_calls(&self) -> Vec<String> {
            self.state.lock().unwrap().register_calls.clone()
        }

        fn unregister_calls(&self) -> Vec<String> {
            self.state.lock().unwrap().unregister_calls.clone()
        }

        fn active(&self) -> Option<String> {
            self.state.lock().unwrap().active.clone()
        }
    }

    impl Registrar for FakeRegistrar {
        fn plugin_register(&self, binding: &str) -> Result<(), RegistrationError> {
            let mut state = self.state.lock().unwrap();
            state.register_calls.push(binding.to_string());
            if let Some(err) = state.force_error.take() {
                return Err(err);
            }
            state.active = Some(binding.to_string());
            Ok(())
        }

        fn plugin_unregister(&self, binding: &str) {
            let mut state = self.state.lock().unwrap();
            state.unregister_calls.push(binding.to_string());
            if state.active.as_deref() == Some(binding) {
                state.active = None;
            }
        }
    }

    #[test]
    fn register_attaches_and_reports_registered() {
        let mgr = GlobalShortcutManager::new(FakeRegistrar::new());
        mgr.register("Control+Alt+Super+J").expect("first register");
        assert!(matches!(
            mgr.registration_status("Control+Alt+Super+J"),
            RegistrationStatus::Registered
        ));
    }

    /// Wrap `FakeRegistrar` in an `Arc` and `impl Registrar` on a small
    /// newtype so both the manager and the test body see the same backing
    /// store. Used by tests that need to assert on `register_calls` /
    /// `unregister_calls` after the manager has consumed the registrar.
    fn shared_registrar() -> (std::sync::Arc<FakeRegistrar>, GlobalShortcutManager<SharedRegistrar>) {
        let shared = std::sync::Arc::new(FakeRegistrar::new());
        let mgr = GlobalShortcutManager::new(SharedRegistrar(std::sync::Arc::clone(&shared)));
        (shared, mgr)
    }

    struct SharedRegistrar(std::sync::Arc<FakeRegistrar>);
    impl Registrar for SharedRegistrar {
        fn plugin_register(&self, binding: &str) -> Result<(), RegistrationError> {
            self.0.plugin_register(binding)
        }
        fn plugin_unregister(&self, binding: &str) {
            self.0.plugin_unregister(binding)
        }
    }

    #[test]
    fn register_same_binding_twice_is_idempotent() {
        // The second `register` of the same binding must NOT hit the plugin
        // again — re-registering would re-acquire and risk double-events on
        // some platforms. Directly probe `register_calls` via the shared
        // registrar so the assertion is a true idempotency contract, not just
        // a status read.
        let (shared, mgr) = shared_registrar();
        mgr.register("Control+Alt+Super+J").expect("first");
        mgr.register("Control+Alt+Super+J").expect("idempotent");
        assert_eq!(shared.register_calls(), vec!["Control+Alt+Super+J"]);
        assert!(shared.unregister_calls().is_empty());
        assert!(matches!(
            mgr.registration_status("Control+Alt+Super+J"),
            RegistrationStatus::Registered
        ));
    }

    #[test]
    fn register_new_binding_unregisters_previous() {
        let (shared, mgr) = shared_registrar();

        mgr.register("Control+Alt+Super+J").expect("first");
        mgr.register("Super+Shift+K").expect("swap");

        assert_eq!(shared.register_calls(), vec!["Control+Alt+Super+J", "Super+Shift+K"]);
        assert_eq!(shared.unregister_calls(), vec!["Control+Alt+Super+J"]);
        assert_eq!(shared.active().as_deref(), Some("Super+Shift+K"));
    }

    #[test]
    fn plugin_error_does_not_promote_to_registered() {
        // With no previous binding to fall back to, a refused first
        // registration stays `NotRegistered`, so a re-attempt happens cleanly
        // on the next user flip.
        let registrar = FakeRegistrar::new();
        registrar.set_next_error(RegistrationError::PluginError {
            message: "HotKey already registered".to_string(),
        });
        let mgr = GlobalShortcutManager::new(registrar);

        let result = mgr.register("Control+Alt+Super+J");
        assert!(matches!(result, Err(RegistrationError::PluginError { .. })));
        assert!(matches!(
            mgr.registration_status("Control+Alt+Super+J"),
            RegistrationStatus::NotRegistered
        ));
    }

    #[test]
    fn a_refused_rebind_keeps_the_previous_binding_working() {
        // The swap drops the old combo before asking for the new one, so a
        // refusal would otherwise leave the user with no hotkey at all.
        let (shared, mgr) = shared_registrar();
        mgr.register("Control+Alt+Super+J").expect("first");
        shared.set_next_error(RegistrationError::PluginError {
            message: "RegisterEventHotKey failed for KeyK".to_string(),
        });

        let result = mgr.register("Control+Alt+Super+K");

        assert!(result.is_err());
        assert_eq!(shared.active().as_deref(), Some("Control+Alt+Super+J"));
        assert!(matches!(
            mgr.registration_status("Control+Alt+Super+J"),
            RegistrationStatus::Registered
        ));
    }

    #[test]
    fn the_os_refusing_a_combo_reads_as_unavailable_and_nothing_else_does() {
        // The plugin flattens `global_hotkey`'s typed errors into a string, so
        // its enum arm is the only typed signal; the message is never read.
        let refused = tauri_plugin_global_shortcut::Error::GlobalHotkey("RegisterEventHotKey failed for KeyK".into());
        assert!(matches!(
            RegistrationError::from(refused),
            RegistrationError::Unavailable { .. }
        ));

        let internal = tauri_plugin_global_shortcut::Error::RecvError(std::sync::mpsc::RecvError);
        assert!(matches!(
            RegistrationError::from(internal),
            RegistrationError::PluginError { .. }
        ));
    }

    #[test]
    fn unregister_clears_active_state_idempotently() {
        let mgr = GlobalShortcutManager::new(FakeRegistrar::new());
        mgr.register("Control+Alt+Super+J").expect("register");
        mgr.unregister();
        assert!(matches!(
            mgr.registration_status("Control+Alt+Super+J"),
            RegistrationStatus::NotRegistered
        ));
        // Second unregister: no panic, still NotRegistered.
        mgr.unregister();
        assert!(matches!(
            mgr.registration_status("Control+Alt+Super+J"),
            RegistrationStatus::NotRegistered
        ));
    }

    #[test]
    fn registration_status_for_unknown_binding_is_not_registered() {
        let mgr = GlobalShortcutManager::new(FakeRegistrar::new());
        mgr.register("Control+Alt+Super+J").expect("register");
        // A different binding was never touched: NotRegistered.
        assert!(matches!(
            mgr.registration_status("Super+Shift+K"),
            RegistrationStatus::NotRegistered
        ));
    }

    #[test]
    fn invalid_binding_error_leaves_status_not_registered() {
        let registrar = FakeRegistrar::new();
        registrar.set_next_error(RegistrationError::InvalidBinding {
            binding: "Garbage+@".to_string(),
        });
        let mgr = GlobalShortcutManager::new(registrar);

        let result = mgr.register("Garbage+@");
        assert!(matches!(result, Err(RegistrationError::InvalidBinding { .. })));
        assert!(matches!(
            mgr.registration_status("Garbage+@"),
            RegistrationStatus::NotRegistered
        ));
    }
}

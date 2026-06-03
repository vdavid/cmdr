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
//! `lib.rs` calls [`refresh_runtime`] at startup, on main-window focus, and
//! when the FE flips the setting via [`set_global_go_to_latest_shortcut`]; this
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

/// Typed errors from a registration attempt. The FE branches on `kind`;
/// never match on the message string.
///
/// Two variants is deliberately the whole surface. `InvalidBinding` is the
/// only failure we can disambiguate cheaply (via `Shortcut::from_str` BEFORE
/// the plugin call). Every other plugin failure — including the "another app
/// holds it" case — lands in `PluginError` carrying the underlying message.
/// The Settings row renders the message tail when one is present; there's no
/// user action that depends on distinguishing "in use by another app" from
/// "allocation failure" (both mean "pick a different combo or move on"), so a
/// single bucket keeps us off the brittle string-match path.
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
    /// Any plugin failure: conflict with another app, allocation, OS IO, etc.
    /// Carries the underlying message for both the log line and the Settings
    /// row's "Couldn't register: …" tail.
    PluginError { message: String },
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBinding { binding } => write!(f, "Invalid global shortcut binding: {binding}"),
            Self::PluginError { message } => write!(f, "Global shortcut plugin error: {message}"),
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
    /// Register the binding. Returns `Err(Conflict)` when the plugin reports
    /// "already registered to another listener" and the caller has not
    /// previously registered this binding through us; other plugin errors
    /// land in `PluginError`.
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
        // touch the plugin. Everything that survives the parse but fails to
        // register lands in `PluginError` carrying the plugin's own message,
        // which the Settings row renders verbatim under "Couldn't register".
        // No substring branching, no locale-dependent string match.
        if Shortcut::from_str(binding).is_err() {
            return Err(RegistrationError::InvalidBinding {
                binding: binding.to_string(),
            });
        }
        self.app
            .global_shortcut()
            .register(binding)
            .map_err(|err| RegistrationError::PluginError {
                message: err.to_string(),
            })
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
    /// swaps cleanly when a different binding arrives.
    pub fn register(&self, binding: &str) -> Result<(), RegistrationError> {
        let mut state = self.state.lock().expect("global_shortcut state poisoned");

        if state.active.as_deref() == Some(binding) {
            // Already active — re-registering would re-acquire and risk
            // double-events. Idempotent re-call.
            return Ok(());
        }

        // Swap: drop the previous binding (if any) before attaching the new one
        // so the OS doesn't briefly hold two registrations.
        if let Some(prev) = state.active.take() {
            self.registrar.plugin_unregister(&prev);
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
                Err(err)
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
    use tauri::{Emitter as _, Manager as _};
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
            // The payload is currently empty: the FE bridge calls
            // `goToLatestDownload(explorer)` directly. We pass an empty
            // object so future per-binding metadata (which combo, modifiers,
            // etc.) is additive without breaking the event shape.
            if let Err(err) = app.emit("global-shortcut-fired", serde_json::json!({})) {
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
        // Any plugin failure (conflict, allocation, OS IO) lands in
        // `PluginError`; the Settings row renders the message tail under
        // "Couldn't register". Status stays `NotRegistered` so a re-attempt
        // happens cleanly on the next user flip.
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

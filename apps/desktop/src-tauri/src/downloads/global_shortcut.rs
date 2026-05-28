//! System-wide reveal-latest-download hotkey (default `⌃⌥⌘J`).
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
//! `settings.globalRevealShortcut.enabled == true` AND
//! `fda_gate::is_fda_pending_runtime() == false`. The lifecycle wiring in
//! `lib.rs` calls [`refresh_runtime`] at startup, on main-window focus, and
//! when the FE flips the setting via [`set_global_reveal_shortcut`]; this
//! module only owns the typed register/unregister/status surface.
//!
//! ## macOS permission scope
//!
//! No Accessibility or Input Monitoring grant required. The plugin uses
//! Carbon's `RegisterEventHotKey` (a system-API event hook in-process),
//! which is distinct from key-logging APIs that need TCC grants. The user
//! sees no extra prompt.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutEvent, ShortcutState};

/// Typed errors from a registration attempt. The FE branches on `kind`;
/// never match on the message string.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RegistrationError {
    /// Another app already holds the combo. Surface the message
    /// "Couldn't register: in use by another app." in the Settings row.
    Conflict,
    /// The accelerator string couldn't be parsed by the plugin.
    InvalidBinding {
        /// The rejected binding so the FE can surface it (for debugging only;
        /// the row already shows the binding the user picked).
        binding: String,
    },
    /// Any other plugin failure (allocation, IO with the OS, etc.). Carries
    /// the underlying message for the log line; the FE shows a generic
    /// fallback string.
    PluginError { message: String },
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => write!(f, "Global shortcut conflict (already in use)"),
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
    /// Nothing is currently registered (disabled, FDA gate closed, or empty
    /// binding).
    NotRegistered,
    /// Last attempt failed because another app holds the combo. The FE
    /// surfaces the conflict copy; nothing fires until the user picks a
    /// different combo.
    Conflict,
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
        // The plugin doesn't expose typed error codes; we have to introspect
        // the error string ONCE here to map conflict vs invalid-binding.
        // This is the locked-in single string-match site; the rest of the
        // crate branches on `RegistrationError::kind` instead.
        //
        // Allowed because (a) the plugin's `tauri_plugin_global_shortcut::Error`
        // is `#[non_exhaustive]` with no `code()`/`kind()` accessor, and (b)
        // the matched substrings are stable plugin-internal English (no user
        // locale, no upstream copy churn beyond breaking-change bumps).
        // allowed-error-string-match: tauri-plugin-global-shortcut has no typed error codes
        match self.app.global_shortcut().register(binding) {
            Ok(()) => Ok(()),
            Err(err) => {
                let msg = err.to_string();
                let lower = msg.to_lowercase();
                // allowed-error-string-match: see module comment
                if lower.contains("already") || lower.contains("in use") || lower.contains("registered") {
                    Err(RegistrationError::Conflict)
                // allowed-error-string-match: see module comment
                } else if lower.contains("parse") || lower.contains("invalid") || lower.contains("accelerator") {
                    Err(RegistrationError::InvalidBinding {
                        binding: binding.to_string(),
                    })
                } else {
                    Err(RegistrationError::PluginError { message: msg })
                }
            }
        }
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
/// binding + a remembered conflict so the Settings row can show "couldn't
/// register" without re-attempting.
pub struct GlobalShortcutManager<R: Registrar> {
    registrar: R,
    state: Mutex<ManagerState>,
}

#[derive(Debug, Default, Clone)]
struct ManagerState {
    /// `Some(binding)` when we've successfully registered it. `None` when
    /// we're not currently holding any binding.
    active: Option<String>,
    /// `Some(binding)` when our most recent attempt at `binding` failed with
    /// `Conflict`. Drives the [`RegistrationStatus::Conflict`] surface.
    conflicting: Option<String>,
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
            // Already active — clear any conflict memory for OTHER bindings
            // (a stale conflict for the previous binding is irrelevant once
            // we've successfully attached to this one). Idempotent re-call.
            state.conflicting = None;
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
                state.conflicting = None;
                log::info!(
                    target: "downloads::global_shortcut",
                    "Registered global shortcut: {binding}",
                );
                Ok(())
            }
            Err(RegistrationError::Conflict) => {
                state.conflicting = Some(binding.to_string());
                log::warn!(
                    target: "downloads::global_shortcut",
                    "Global shortcut conflict for {binding}; another app holds it",
                );
                Err(RegistrationError::Conflict)
            }
            Err(other) => {
                // Invalid binding / generic plugin error: don't remember as a
                // "conflict" since the situation is "this binding is broken,"
                // not "another app holds it."
                state.conflicting = None;
                log::warn!(
                    target: "downloads::global_shortcut",
                    "Global shortcut register({binding}) failed: {other}",
                );
                Err(other)
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
        state.conflicting = None;
    }

    /// Status for the requested `binding`. The Settings row consults this on
    /// mount and after every flip.
    pub fn registration_status(&self, binding: &str) -> RegistrationStatus {
        let state = self.state.lock().expect("global_shortcut state poisoned");
        if state.active.as_deref() == Some(binding) {
            RegistrationStatus::Registered
        } else if state.conflicting.as_deref() == Some(binding) {
            RegistrationStatus::Conflict
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
    use tauri::Emitter as _;
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app: &AppHandle, _shortcut, event: ShortcutEvent| {
            // Fire on key-down only; key-up would double-trigger.
            if event.state() != ShortcutState::Pressed {
                return;
            }
            // The payload is currently empty: the FE bridge calls
            // `revealLatestDownload(explorer)` directly. We pass an empty
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
        mgr.register("Control+Alt+Meta+J").expect("first register");
        assert!(matches!(
            mgr.registration_status("Control+Alt+Meta+J"),
            RegistrationStatus::Registered
        ));
    }

    #[test]
    fn register_same_binding_twice_is_idempotent() {
        let registrar = FakeRegistrar::new();
        let mgr = GlobalShortcutManager::new(registrar);
        mgr.register("Control+Alt+Meta+J").expect("first");
        mgr.register("Control+Alt+Meta+J").expect("idempotent");
        // The second call must NOT hit the plugin again — registering the
        // same binding twice would re-acquire and risk double-events.
        // Direct field probe via fresh registrar would lose state; assert
        // via status instead: still Registered, no conflict promoted.
        assert!(matches!(
            mgr.registration_status("Control+Alt+Meta+J"),
            RegistrationStatus::Registered
        ));
    }

    #[test]
    fn register_new_binding_unregisters_previous() {
        // Wrap the FakeRegistrar in an Arc and have it impl Registrar on
        // `Arc<FakeRegistrar>` so both manager and test see the same backing
        // store. Done via a small newtype.
        use std::sync::Arc;

        struct SharedRegistrar(Arc<FakeRegistrar>);
        impl Registrar for SharedRegistrar {
            fn plugin_register(&self, binding: &str) -> Result<(), RegistrationError> {
                self.0.plugin_register(binding)
            }
            fn plugin_unregister(&self, binding: &str) {
                self.0.plugin_unregister(binding)
            }
        }

        let shared = Arc::new(FakeRegistrar::new());
        let mgr = GlobalShortcutManager::new(SharedRegistrar(Arc::clone(&shared)));

        mgr.register("Control+Alt+Meta+J").expect("first");
        mgr.register("Meta+Shift+K").expect("swap");

        let register_calls = shared.register_calls();
        let unregister_calls = shared.unregister_calls();
        assert_eq!(register_calls, vec!["Control+Alt+Meta+J", "Meta+Shift+K"]);
        assert_eq!(unregister_calls, vec!["Control+Alt+Meta+J"]);
        assert_eq!(shared.active().as_deref(), Some("Meta+Shift+K"));
    }

    #[test]
    fn register_conflict_records_status_and_clears_active() {
        let registrar = FakeRegistrar::new();
        registrar.set_next_error(RegistrationError::Conflict);
        let mgr = GlobalShortcutManager::new(registrar);

        let result = mgr.register("Control+Alt+Meta+J");
        assert!(matches!(result, Err(RegistrationError::Conflict)));
        assert!(matches!(
            mgr.registration_status("Control+Alt+Meta+J"),
            RegistrationStatus::Conflict
        ));
    }

    #[test]
    fn unregister_clears_active_state_idempotently() {
        let mgr = GlobalShortcutManager::new(FakeRegistrar::new());
        mgr.register("Control+Alt+Meta+J").expect("register");
        mgr.unregister();
        assert!(matches!(
            mgr.registration_status("Control+Alt+Meta+J"),
            RegistrationStatus::NotRegistered
        ));
        // Second unregister: no panic, still NotRegistered.
        mgr.unregister();
        assert!(matches!(
            mgr.registration_status("Control+Alt+Meta+J"),
            RegistrationStatus::NotRegistered
        ));
    }

    #[test]
    fn registration_status_for_unknown_binding_is_not_registered() {
        let mgr = GlobalShortcutManager::new(FakeRegistrar::new());
        mgr.register("Control+Alt+Meta+J").expect("register");
        // A different binding was never touched: NotRegistered.
        assert!(matches!(
            mgr.registration_status("Meta+Shift+K"),
            RegistrationStatus::NotRegistered
        ));
    }

    #[test]
    fn invalid_binding_error_does_not_promote_to_conflict() {
        let registrar = FakeRegistrar::new();
        registrar.set_next_error(RegistrationError::InvalidBinding {
            binding: "Garbage+@".to_string(),
        });
        let mgr = GlobalShortcutManager::new(registrar);

        let result = mgr.register("Garbage+@");
        assert!(matches!(result, Err(RegistrationError::InvalidBinding { .. })));
        // Invalid is its own failure mode; status must NOT report Conflict.
        assert!(matches!(
            mgr.registration_status("Garbage+@"),
            RegistrationStatus::NotRegistered
        ));
    }
}

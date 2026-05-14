use serde::Deserialize;

#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Warning,
    Error,
}

#[derive(Deserialize, specta::Type)]
pub struct FrontendLogEntry {
    pub level: LogLevel,
    pub category: String,
    pub message: String,
}

/// Receives batched log entries from the frontend and re-emits them through the Rust `log` facade.
/// This ensures frontend logs appear in the terminal and log file alongside Rust logs.
#[tauri::command]
#[specta::specta]
pub fn batch_fe_logs(entries: Vec<FrontendLogEntry>) {
    for entry in &entries {
        let target = format!("FE:{}", entry.category);
        match entry.level {
            LogLevel::Debug => log::debug!(target: &target, "{}", entry.message),
            LogLevel::Info => log::info!(target: &target, "{}", entry.message),
            LogLevel::Warn | LogLevel::Warning => log::warn!(target: &target, "{}", entry.message),
            LogLevel::Error => crate::log_error!(target: &target, "{}", entry.message),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the frontend log bridge path.
    //!
    //! Locked behind a process-global mutex because `auto_dispatcher` keeps state in
    //! statics, so running these alongside the dispatcher's own tests in parallel would
    //! race on the shared `STATE` and `ENABLED` flag.
    use super::*;
    use crate::error_reporter::auto_dispatcher::{reset_for_test, set_enabled, snapshot_for_test};
    use std::sync::Mutex;

    static FE_LOG_BRIDGE_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Regression: a frontend log entry at `Error` level must trip the auto-dispatcher
    /// the same way a Rust-side `log_error!` would. Before the FE-bridge migration to
    /// `log_error!`, the bridge re-emitted via `log::error!` directly, so user-facing
    /// errors surfaced from Svelte never opened a Flow B debounce window.
    #[test]
    fn fe_error_entry_trips_auto_dispatcher() {
        let _guard = FE_LOG_BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test();
        set_enabled(true);

        let synthetic_message = "synthetic FE bridge failure for the test";
        batch_fe_logs(vec![FrontendLogEntry {
            level: LogLevel::Error,
            category: "viewer".to_string(),
            message: synthetic_message.to_string(),
        }]);

        let snapshot = snapshot_for_test().expect("FE error entry should open a debounce window");
        assert_eq!(snapshot.0, "FE:viewer", "category should carry the FE: prefix");
        assert_eq!(
            snapshot.1, synthetic_message,
            "first message should be the FE entry's text"
        );
        assert!(snapshot.2 >= 1, "error_count should reflect at least one error");

        reset_for_test();
    }

    /// Non-error FE log levels must NOT touch the dispatcher: only `Error` reaches the
    /// `log_error!` arm of `batch_fe_logs`. Guards against a future regression where
    /// someone wires every level through the macro by accident.
    #[test]
    fn fe_non_error_entries_do_not_trip_auto_dispatcher() {
        let _guard = FE_LOG_BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test();
        set_enabled(true);

        batch_fe_logs(vec![
            FrontendLogEntry {
                level: LogLevel::Debug,
                category: "viewer".to_string(),
                message: "debug noise".to_string(),
            },
            FrontendLogEntry {
                level: LogLevel::Info,
                category: "viewer".to_string(),
                message: "info noise".to_string(),
            },
            FrontendLogEntry {
                level: LogLevel::Warn,
                category: "viewer".to_string(),
                message: "warn noise".to_string(),
            },
        ]);

        assert!(
            snapshot_for_test().is_none(),
            "non-error FE log levels must not open a debounce window",
        );

        reset_for_test();
    }
}

/// Changes the **stdout** log threshold at runtime (called when the
/// `developer.verboseLogging` setting is toggled).
///
/// Per-output filtering: this only affects the terminal/stderr chain. The file chain
/// stays at Debug regardless, so error report bundles always carry useful context.
#[tauri::command]
#[specta::specta]
pub fn set_log_level(level: String) {
    let filter = match level.as_str() {
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" | "warning" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    };
    crate::logging::dispatch::set_stdout_threshold(filter);
    log::info!("Stdout log threshold set to {filter} (file target stays at Debug)");
}

//! Cloud AI provider API key storage.
//!
//! Delegates to `crate::secrets::store()` for platform-agnostic secret storage so each provider's
//! key sits in the OS-native secret backend (macOS Keychain, Linux Secret Service, etc.) instead
//! of `settings.json`. One entry per provider keyed as `ai.apiKey.<providerId>`.

use crate::pluralize::pluralize;
use crate::secrets::SecretStoreError;
use log::{debug, info};
use serde::{Deserialize, Serialize};

/// Builds the secret-store key for a given provider id.
fn store_key(provider_id: &str) -> String {
    format!("ai.apiKey.{provider_id}")
}

/// Error types surfaced over IPC for AI API key operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum AiApiKeyError {
    NotFound(String),
    AccessDenied(String),
    Other(String),
}

impl std::fmt::Display for AiApiKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "AI API key not found: {msg}"),
            Self::AccessDenied(msg) => write!(f, "AI API key access denied: {msg}"),
            Self::Other(msg) => write!(f, "AI API key error: {msg}"),
        }
    }
}

impl std::error::Error for AiApiKeyError {}

impl From<SecretStoreError> for AiApiKeyError {
    fn from(e: SecretStoreError) -> Self {
        match e {
            SecretStoreError::NotFound(msg) => Self::NotFound(msg),
            SecretStoreError::AccessDenied(msg) => Self::AccessDenied(msg),
            SecretStoreError::Other(msg) => Self::Other(msg),
        }
    }
}

/// Saves the API key for a provider. Overwrites any existing entry. Logs at INFO without ever
/// touching the key value: the *change event* is the actionable signal for postmortem debugging
/// (when did the key get set? did the save reach the keychain?), the key itself is not.
pub fn save(provider_id: &str, api_key: &str) -> Result<(), AiApiKeyError> {
    let key = store_key(provider_id);
    let key_len = api_key.len();
    crate::secrets::store().set(&key, api_key.as_bytes())?;
    info!(
        "AI API key saved for provider {provider_id} ({})",
        pluralize(key_len as u64, "byte")
    );
    Ok(())
}

/// Returns the stored API key for a provider, or an error if none is stored.
pub fn get(provider_id: &str) -> Result<String, AiApiKeyError> {
    let key = store_key(provider_id);
    let data = crate::secrets::store().get(&key)?;
    String::from_utf8(data).map_err(|e| AiApiKeyError::Other(format!("Stored key is not valid UTF-8: {e}")))
}

/// Deletes the API key for a provider. Returns `Ok(())` even if no entry existed (idempotent).
pub fn delete(provider_id: &str) -> Result<(), AiApiKeyError> {
    let key = store_key(provider_id);
    match crate::secrets::store().delete(&key) {
        Ok(()) => {
            info!("AI API key deleted for provider {provider_id}");
            Ok(())
        }
        Err(SecretStoreError::NotFound(_)) => {
            debug!("AI API key delete for {provider_id} was a no-op (none stored)");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Returns true if an API key is stored for the provider.
pub fn has(provider_id: &str) -> bool {
    get(provider_id).is_ok()
}

// --- Tauri commands ---

#[tauri::command]
#[specta::specta]
pub fn save_ai_api_key(provider_id: String, api_key: String) -> Result<(), AiApiKeyError> {
    save(&provider_id, &api_key)
}

/// Returns the stored API key for the provider, or an empty string if none is stored.
/// Returning empty (rather than an error) on missing keys keeps the call sites simple: they all
/// pass the value through to `configure_ai`, which already treats empty-string as "not configured."
#[tauri::command]
#[specta::specta]
pub fn get_ai_api_key(provider_id: String) -> Result<String, AiApiKeyError> {
    match get(&provider_id) {
        Ok(key) => Ok(key),
        Err(AiApiKeyError::NotFound(_)) => Ok(String::new()),
        Err(e) => Err(e),
    }
}

#[tauri::command]
#[specta::specta]
pub fn delete_ai_api_key(provider_id: String) -> Result<(), AiApiKeyError> {
    delete(&provider_id)
}

#[tauri::command]
#[specta::specta]
pub fn has_ai_api_key(provider_id: String) -> bool {
    has(&provider_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Per-test isolation: each test runs in its own data dir so the PlainFileStore's JSON file
    /// doesn't race across nextest's per-test processes (which would share the prod app-support
    /// dir otherwise: secrets `save` succeeds but the subsequent `get` sees another test's write).
    ///
    /// Must be called BEFORE the first secret store access in the test: the secret store backend
    /// is a `LazyLock` and reads these env vars exactly once.
    ///
    /// SAFETY: `std::env::set_var` is racy across threads, but each nextest test runs in its own
    /// process, so the only env-var writes happen here on the test's main thread before any code
    /// that reads them.
    fn isolate_secrets() {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cmdr-api-keys-test-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&dir).expect("create test data dir");
        // SAFETY: `std::env::set_var` is unsound only under concurrent env access. Each nextest test
        // runs in its own process, and `isolate_secrets` is called at the top of each test on that
        // process's single (main) thread before any code reads these vars (the secret store samples
        // them once via `LazyLock`), so no other thread can be touching the environment here.
        unsafe {
            std::env::set_var("CMDR_DATA_DIR", &dir);
            std::env::set_var("CMDR_SECRET_STORE", "file");
        }
    }

    #[test]
    fn save_and_get_roundtrip() {
        isolate_secrets();
        save("openai", "sk-test-abc123").unwrap();
        assert_eq!(get("openai").unwrap(), "sk-test-abc123");
    }

    #[test]
    fn get_missing_returns_not_found() {
        isolate_secrets();
        match get("openai") {
            Err(AiApiKeyError::NotFound(_)) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn has_reflects_save_and_delete() {
        isolate_secrets();
        assert!(!has("openai"));
        save("openai", "sk-test").unwrap();
        assert!(has("openai"));
        delete("openai").unwrap();
        assert!(!has("openai"));
    }

    #[test]
    fn delete_missing_is_idempotent() {
        isolate_secrets();
        delete("openai").unwrap();
        delete("openai").unwrap();
    }

    #[test]
    fn save_overwrites_existing() {
        isolate_secrets();
        save("openai", "sk-first").unwrap();
        save("openai", "sk-second").unwrap();
        assert_eq!(get("openai").unwrap(), "sk-second");
    }
}

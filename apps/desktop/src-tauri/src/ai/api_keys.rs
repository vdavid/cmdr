//! Cloud AI provider API key storage.
//!
//! Delegates to `crate::secrets::store()` for platform-agnostic secret storage so each provider's
//! key sits in the OS-native secret backend (macOS Keychain, Linux Secret Service, etc.) instead
//! of `settings.json`. One entry per provider keyed as `ai.apiKey.<providerId>`.
//!
//! **A stored key never crosses IPC back to a webview.** There is deliberately no "read the key"
//! command: `configure_ai` and `check_ai_connection` take a provider id and read the key here, in
//! the backend. The only key-shaped thing a renderer can obtain is [`AiApiKeyStatus`], which
//! carries "is one set" plus an opaque fingerprint. See `docs/security.md` § "AI API keys".

use crate::pluralize::pluralize;
use crate::secrets::SecretStoreError;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

/// Hex characters of the SHA-256 digest we expose as a key fingerprint. 16 hex chars (64 bits) is
/// far past collision range for the handful of keys one user holds, and reveals nothing about a
/// high-entropy secret.
const FINGERPRINT_HEX_LEN: usize = 16;

/// What a renderer may know about a stored key: whether there is one, and an opaque handle that
/// changes when the key changes. Never the key itself.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiApiKeyStatus {
    /// True when a non-empty key is stored for this provider.
    pub is_set: bool,
    /// Truncated SHA-256 of the key, or empty when none is stored. The settings UI uses it to
    /// fingerprint its model-list cache, which must miss when the key changes.
    pub fingerprint: String,
}

/// Truncated SHA-256 of the key, as lowercase hex.
fn fingerprint(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    let mut hex = String::with_capacity(FINGERPRINT_HEX_LEN);
    for byte in digest.iter().take(FINGERPRINT_HEX_LEN / 2) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Returns whether a key is stored for the provider, plus its fingerprint. A missing key is
/// `is_set: false`, not an error; a secret store that can't be READ (locked keyring, denied
/// Keychain ACL) is an error, because the UI has to tell those two apart.
pub fn status(provider_id: &str) -> Result<AiApiKeyStatus, AiApiKeyError> {
    match get(provider_id) {
        Ok(key) if key.is_empty() => Ok(AiApiKeyStatus {
            is_set: false,
            fingerprint: String::new(),
        }),
        Ok(key) => Ok(AiApiKeyStatus {
            is_set: true,
            fingerprint: fingerprint(&key),
        }),
        Err(AiApiKeyError::NotFound(_)) => Ok(AiApiKeyStatus {
            is_set: false,
            fingerprint: String::new(),
        }),
        Err(e) => Err(e),
    }
}

/// Reads the key for a provider for backend-internal use, mapping "none stored" to an empty
/// string. Returns the secret-store failure so callers can surface it: a key that exists but
/// can't be read is a broken keyring the user needs to hear about, not a missing key.
pub(crate) fn read_for_backend(provider_id: &str) -> (String, Option<AiApiKeyError>) {
    match get(provider_id) {
        Ok(key) => (key, None),
        Err(AiApiKeyError::NotFound(_)) => (String::new(), None),
        Err(e) => {
            log::warn!("Couldn't read the AI API key for provider {provider_id}: {e}");
            (String::new(), Some(e))
        }
    }
}

// --- Tauri commands ---

#[tauri::command]
#[specta::specta]
pub fn save_ai_api_key(provider_id: String, api_key: String) -> Result<(), AiApiKeyError> {
    save(&provider_id, &api_key)
}

/// Returns whether a key is stored for the provider, plus an opaque fingerprint.
///
/// ❌ Don't add a command that returns the key itself. The backend reads it directly wherever it's
/// needed (`configure_ai`, `check_ai_connection`), so a compromised webview has nothing to ask for.
#[tauri::command]
#[specta::specta]
pub fn get_ai_api_key_status(provider_id: String) -> Result<AiApiKeyStatus, AiApiKeyError> {
    status(&provider_id)
}

#[tauri::command]
#[specta::specta]
pub fn delete_ai_api_key(provider_id: String) -> Result<(), AiApiKeyError> {
    delete(&provider_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Per-test isolation: each test runs in its own data dir so the PlainFileStore's JSON file
    /// doesn't race across nextest's per-test processes (which would share the prod app-support
    /// dir otherwise: secrets `save` succeeds but the subsequent `get` sees another test's write).
    /// The name is random, so no two runs can ever collide on it.
    ///
    /// **Bind the returned `TempDir` for the whole test** (`let _dir = isolate_secrets();`).
    /// Dropping it deletes the directory, so `let _ = …` would pull the store out from under the
    /// test. Holding it is also what keeps `$TMPDIR` clean: the dir goes away when the test ends,
    /// instead of one surviving per test per run.
    ///
    /// Must be called BEFORE the first secret store access in the test: the secret store backend
    /// is a `LazyLock` and reads these env vars exactly once.
    #[must_use = "dropping the TempDir deletes the store's dir; bind it for the test's lifetime"]
    fn isolate_secrets() -> TempDir {
        let dir = TempDir::new().expect("create test data dir");
        // SAFETY: `std::env::set_var` is unsound only under concurrent env access. Each nextest test
        // runs in its own process, and `isolate_secrets` is called at the top of each test on that
        // process's single (main) thread before any code reads these vars (the secret store samples
        // them once via `LazyLock`), so no other thread can be touching the environment here.
        unsafe {
            std::env::set_var("CMDR_DATA_DIR", dir.path());
            std::env::set_var("CMDR_SECRET_STORE", "file");
        }
        dir
    }

    #[test]
    fn save_and_get_roundtrip() {
        let _dir = isolate_secrets();
        save("openai", "sk-test-abc123").unwrap();
        assert_eq!(get("openai").unwrap(), "sk-test-abc123");
    }

    #[test]
    fn get_missing_returns_not_found() {
        let _dir = isolate_secrets();
        match get("openai") {
            Err(AiApiKeyError::NotFound(_)) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn status_reflects_save_and_delete() {
        let _dir = isolate_secrets();
        assert!(!status("openai").unwrap().is_set);
        save("openai", "sk-test").unwrap();
        assert!(status("openai").unwrap().is_set);
        delete("openai").unwrap();
        assert!(!status("openai").unwrap().is_set);
    }

    #[test]
    fn delete_missing_is_idempotent() {
        let _dir = isolate_secrets();
        delete("openai").unwrap();
        delete("openai").unwrap();
    }

    #[test]
    fn save_overwrites_existing() {
        let _dir = isolate_secrets();
        save("openai", "sk-first").unwrap();
        save("openai", "sk-second").unwrap();
        assert_eq!(get("openai").unwrap(), "sk-second");
    }

    #[test]
    fn status_reports_unset_without_a_fingerprint() {
        let _dir = isolate_secrets();
        let status = status("openai").unwrap();
        assert!(!status.is_set);
        assert_eq!(status.fingerprint, "");
    }

    #[test]
    fn status_reports_set_with_a_fingerprint() {
        let _dir = isolate_secrets();
        save("openai", "sk-test-abc123").unwrap();
        let status = status("openai").unwrap();
        assert!(status.is_set);
        assert_eq!(status.fingerprint.len(), FINGERPRINT_HEX_LEN);
    }

    /// The fingerprint is the model cache's change-detector, so two different keys must never
    /// share one (a revoked-then-replaced key would otherwise serve the old model list).
    #[test]
    fn status_fingerprint_changes_with_the_key() {
        let _dir = isolate_secrets();
        save("openai", "sk-first").unwrap();
        let first = status("openai").unwrap().fingerprint;
        save("openai", "sk-second").unwrap();
        let second = status("openai").unwrap().fingerprint;
        assert_ne!(first, second);
    }

    /// The whole point of the fingerprint: it goes to a renderer, so it must not be the key.
    #[test]
    fn status_fingerprint_does_not_contain_the_key() {
        let _dir = isolate_secrets();
        save("openai", "sk-test-abc123").unwrap();
        let fingerprint = status("openai").unwrap().fingerprint;
        assert!(!fingerprint.contains("sk-test-abc123"));
    }
}

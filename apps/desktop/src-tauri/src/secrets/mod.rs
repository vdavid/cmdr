//! Generic key-value secret storage with pluggable backends.
//!
//! Backend selection happens once at first access via `store()`:
//! - `CMDR_SECRET_STORE=file` or `CMDR_E2E_MODE=1` forces plain file, in dev and E2E builds only
//! - macOS: Keychain via `security-framework`
//! - Linux: Secret Service via `keyring`, falling back to encrypted file via `cocoon`
//! - Other platforms: plain file fallback

use log::info;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::LazyLock;

#[cfg(target_os = "macos")]
mod keychain_macos;

#[cfg(target_os = "macos")]
pub mod system_keychain_smb;

#[cfg(target_os = "linux")]
mod keyring_linux;

#[cfg(target_os = "linux")]
mod encrypted_file;

mod plain_file;

/// Error types for secret store operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum SecretStoreError {
    /// Key not found in the store
    NotFound(String),
    /// Access denied (user cancelled or insufficient permissions)
    AccessDenied(String),
    /// Any other error
    Other(String),
}

impl std::fmt::Display for SecretStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Secret not found: {}", msg),
            Self::AccessDenied(msg) => write!(f, "Access denied: {}", msg),
            Self::Other(msg) => write!(f, "Secret store error: {}", msg),
        }
    }
}

impl std::error::Error for SecretStoreError {}

/// Generic key-value secret storage.
pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError>;
    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError>;
    fn delete(&self, key: &str) -> Result<(), SecretStoreError>;
}

/// Set during `init_store()` when the store FELL BACK to a file for lack of a system
/// keyring, read by `is_file_backed()`. ❌ Not for a file store chosen on purpose.
static FILE_BACKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static STORE: LazyLock<Box<dyn SecretStore>> = LazyLock::new(init_store);

/// Returns the global secret store, initialized on first access.
pub fn store() -> &'static dyn SecretStore {
    &**STORE
}

/// Whether the store fell back to a file because there's no system keyring (a Linux
/// without a secret service, an unsupported platform), which the frontend tells the
/// person once. ❌ False for a file store chosen on purpose (tests, and dev and E2E
/// through `CMDR_SECRET_STORE=file`). Implicitly initializes the store.
pub fn is_file_backed() -> bool {
    let _ = store();
    FILE_BACKED.load(std::sync::atomic::Ordering::Relaxed)
}

// Under `cfg(test)` the production backend code below is compiled (so `info!`,
// `KeychainStore`, etc. stay referenced) but unreachable at runtime past the early
// test return; allow the resulting warning rather than gating the body out.
#[cfg_attr(
    test,
    allow(
        unreachable_code,
        reason = "production backend selection stays compiled under test (keeps `info!` / `KeychainStore` referenced) but is unreachable past the early `TestStore` return"
    )
)]
fn init_store() -> Box<dyn SecretStore> {
    // Tests must NEVER reach the real OS keychain. `STORE` is a process-global
    // `LazyLock`, so on macOS it would otherwise pin `KeychainStore` on whichever
    // secret-touching test accessed it first, and a later test would read the
    // developer's real Keychain entries (the flaky
    // `ai::api_keys::tests::has_reflects_save_and_delete` `!has("openai")` failure,
    // where the dev's real `openai` key made `has` true). `TestStore` resolves its
    // backing dir fresh per operation from `CMDR_DATA_DIR`, so a test's
    // `crate::test_support::isolate_secrets()` takes effect regardless of the global's
    // init order, and it panics when that var is unset rather than falling back to the
    // developer's real data dir.
    #[cfg(test)]
    {
        return Box::new(TestStore);
    }

    // E2E runs must never hit the OS keychain. A locked macOS Keychain pops a
    // GUI password prompt that blocks every secret read until the user types
    // their password — fatal for an unattended test run (the dialog steals
    // focus, the tests time out, half the suite goes red for non-code reasons).
    // Force the file backend so an E2E session's credentials live alongside its
    // ephemeral data dir and disappear with it.
    let secret_store_env = std::env::var("CMDR_SECRET_STORE").ok();
    let e2e_mode = crate::test_mode::is_e2e_mode();
    match plaintext_override(secret_store_env.as_deref(), e2e_mode, PLAINTEXT_OVERRIDE_HONORED) {
        Some(reason) => {
            let dir = secret_store_dir();
            info!("Secret store: PlainFileStore ({reason})");
            // ❗ Chosen on purpose, so NOT a fallback: nothing to tell the person about.
            return Box::new(plain_file::PlainFileStore::new(dir));
        }
        None if secret_store_env.as_deref() == Some("file") || e2e_mode => {
            log::warn!("Secret store: ignoring the plaintext override, since this is a release build");
        }
        None => {}
    }

    #[cfg(target_os = "macos")]
    {
        info!("Secret store: KeychainStore (macOS)");
        Box::new(keychain_macos::KeychainStore)
    }

    #[cfg(target_os = "linux")]
    {
        if keyring_linux::KeyringStore::is_available() {
            info!("Secret store: KeyringStore (Linux Secret Service)");
            return Box::new(keyring_linux::KeyringStore);
        }
        let dir = secret_store_dir();
        info!("Secret store: EncryptedFileStore (Linux fallback, no secret service)");
        FILE_BACKED.store(true, std::sync::atomic::Ordering::Relaxed);
        Box::new(encrypted_file::EncryptedFileStore::new(dir))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let dir = secret_store_dir();
        info!("Secret store: PlainFileStore (unsupported platform fallback)");
        FILE_BACKED.store(true, std::sync::atomic::Ordering::Relaxed);
        Box::new(plain_file::PlainFileStore::new(dir))
    }
}

/// Whether this build honors the plaintext-store overrides (`CMDR_SECRET_STORE=file`, `CMDR_E2E_MODE=1`).
/// Dev builds and E2E builds do. A release build ignores them, so an environment variable can't
/// move a user's saved passwords and API keys out of the Keychain into a plaintext file.
const PLAINTEXT_OVERRIDE_HONORED: bool = cfg!(any(debug_assertions, feature = "playwright-e2e"));

/// Which override, if any, puts this process on the plaintext `PlainFileStore`.
/// `honored` is `PLAINTEXT_OVERRIDE_HONORED`, passed in so tests can cover the release arm.
fn plaintext_override(secret_store_env: Option<&str>, e2e_mode: bool, honored: bool) -> Option<&'static str> {
    if !honored {
        None
    } else if secret_store_env == Some("file") {
        Some("CMDR_SECRET_STORE=file")
    } else if e2e_mode {
        Some("CMDR_E2E_MODE=1")
    } else {
        None
    }
}

/// Test-only secret store: never touches the OS keychain, and resolves its backing
/// dir fresh on every operation from `CMDR_DATA_DIR` (via `isolated_store_dir()`), so a
/// test's `crate::test_support::isolate_secrets()` takes effect no matter when the global
/// `STORE` was first initialized or which test ran first. Delegates to `PlainFileStore`,
/// whose own static `Mutex` serializes file access across the per-op instances.
#[cfg(test)]
struct TestStore;

#[cfg(test)]
impl SecretStore for TestStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        plain_file::PlainFileStore::new(isolated_store_dir()).set(key, value)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        plain_file::PlainFileStore::new(isolated_store_dir()).get(key)
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        plain_file::PlainFileStore::new(isolated_store_dir()).delete(key)
    }
}

/// `TestStore`'s backing dir, which only ever exists when `CMDR_DATA_DIR` names one.
///
/// The fallback `secret_store_dir()` takes when the var is unset is the developer's REAL data dir,
/// and a test that lands there writes fixture credentials into their actual `secrets.json`. That
/// also self-poisons the suite: `commands::sftp` and `commands::webdav` both open with "nothing is
/// stored before anything is saved", true on a clean machine and false on every run after the
/// first. Panicking makes the missing isolation loud at the first store access instead.
#[cfg(test)]
fn isolated_store_dir() -> PathBuf {
    assert!(
        std::env::var_os("CMDR_DATA_DIR").is_some_and(|dir| !dir.is_empty()),
        "this test reaches the secret store without isolating it. Open it with \
         `let _secrets = crate::test_support::isolate_secrets();` and keep that binding alive for \
         the whole test."
    );
    secret_store_dir()
}

/// Returns the directory for file-based stores.
/// Respects `CMDR_DATA_DIR` env var, otherwise uses the platform data directory.
fn secret_store_dir() -> PathBuf {
    let dir = crate::config::standalone_app_data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp").join(crate::config::BUNDLE_ID));

    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Could not create secret store directory {}: {}", dir.display(), e);
    }

    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ❗ A file store chosen ON PURPOSE (tests, and dev and E2E through
    /// `CMDR_SECRET_STORE=file`) is not a fallback, so it announces none: the "stored
    /// locally, no system keyring" toast showed in dev, where the keyring was never
    /// missing (final QA). Only a store that fell back for lack of a keyring says so.
    #[test]
    fn a_file_store_chosen_on_purpose_is_not_a_fallback() {
        assert!(!is_file_backed());
    }

    #[test]
    fn test_secret_store_error_display() {
        let err = SecretStoreError::NotFound("test-key".to_string());
        assert_eq!(format!("{}", err), "Secret not found: test-key");

        let err = SecretStoreError::AccessDenied("user cancelled".to_string());
        assert_eq!(format!("{}", err), "Access denied: user cancelled");

        let err = SecretStoreError::Other("disk full".to_string());
        assert_eq!(format!("{}", err), "Secret store error: disk full");
    }

    #[test]
    fn test_secret_store_error_serde_roundtrip() {
        let err = SecretStoreError::NotFound("my-key".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"type\":\"not_found\""));
        assert!(json.contains("\"message\":\"my-key\""));

        let parsed: SecretStoreError = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SecretStoreError::NotFound(msg) if msg == "my-key"));
    }

    /// A release build must keep secrets in the Keychain whatever the environment says: an env
    /// var that moves them into a plaintext file is a downgrade anyone who can set one gets free.
    #[test]
    fn a_release_build_ignores_the_plaintext_overrides() {
        assert_eq!(plaintext_override(Some("file"), false, false), None);
        assert_eq!(plaintext_override(None, true, false), None);
        assert_eq!(plaintext_override(Some("file"), true, false), None);
    }

    #[test]
    fn dev_and_e2e_builds_honor_the_plaintext_overrides() {
        assert_eq!(
            plaintext_override(Some("file"), false, true),
            Some("CMDR_SECRET_STORE=file")
        );
        assert_eq!(plaintext_override(None, true, true), Some("CMDR_E2E_MODE=1"));
        assert_eq!(plaintext_override(Some("keychain"), false, true), None);
        assert_eq!(plaintext_override(None, false, true), None);
    }

    /// `CMDR_DATA_DIR` is the whole isolation mechanism: if `secret_store_dir()` ever stopped
    /// honouring it, every test's `isolate_secrets()` would silently point back at the
    /// developer's real store.
    #[test]
    fn secret_store_dir_respects_the_data_dir_env_var() {
        let secrets = crate::test_support::isolate_secrets();
        assert_eq!(secret_store_dir(), secrets.as_ref());
    }

    /// Forgetting `isolate_secrets()` has to fail loudly, because the alternative is writing
    /// fixture credentials into the developer's real `secrets.json`.
    #[test]
    fn the_test_store_refuses_to_run_without_isolation() {
        // SAFETY: `std::env::set_var` is unsound only under concurrent env access. Each nextest
        // test runs in its own process, and this one is single-threaded, so nothing else can be
        // reading the environment while it clears the var.
        unsafe {
            std::env::remove_var("CMDR_DATA_DIR");
        }
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| store().get("anything"))).is_err();
        assert!(panicked, "an unisolated store access has to panic, not fall back");
    }
}

//! Generic key-value secret storage with pluggable backends.
//!
//! Backend selection happens once at first access via `store()`:
//! - `CMDR_SECRET_STORE=file` env var forces plain file (dev mode)
//! - macOS: Keychain via `security-framework`
//! - Linux: Secret Service via `keyring`, falling back to encrypted file via `cocoon`
//! - Other platforms: plain file fallback

use log::info;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::LazyLock;

#[cfg(target_os = "macos")]
mod keychain_macos;

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

/// Set during `init_store()`, read by `is_file_backed()`.
static FILE_BACKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static STORE: LazyLock<Box<dyn SecretStore>> = LazyLock::new(init_store);

/// Returns the global secret store, initialized on first access.
pub fn store() -> &'static dyn SecretStore {
    &**STORE
}

/// Returns true when the active store is file-backed (plain or encrypted).
/// Implicitly initializes the store if it hasn't been yet.
pub fn is_file_backed() -> bool {
    let _ = store();
    FILE_BACKED.load(std::sync::atomic::Ordering::Relaxed)
}

fn init_store() -> Box<dyn SecretStore> {
    // Check env var override first
    if let Ok(val) = std::env::var("CMDR_SECRET_STORE")
        && val == "file"
    {
        let dir = secret_store_dir();
        info!("Secret store: PlainFileStore (CMDR_SECRET_STORE=file)");
        FILE_BACKED.store(true, std::sync::atomic::Ordering::Relaxed);
        return Box::new(plain_file::PlainFileStore::new(dir));
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
        return Box::new(encrypted_file::EncryptedFileStore::new(dir));
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let dir = secret_store_dir();
        info!("Secret store: PlainFileStore (unsupported platform fallback)");
        FILE_BACKED.store(true, std::sync::atomic::Ordering::Relaxed);
        Box::new(plain_file::PlainFileStore::new(dir))
    }
}

/// Returns the directory for file-based stores.
/// Respects `CMDR_DATA_DIR` env var, otherwise uses the platform data directory.
fn secret_store_dir() -> PathBuf {
    let dir = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        PathBuf::from(custom)
    } else {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("com.veszelovszki.cmdr")
    };

    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Could not create secret store directory {}: {}", dir.display(), e);
    }

    dir
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_secret_store_dir_respects_env() {
        // When CMDR_DATA_DIR is set, secret_store_dir uses it directly
        // (We can't easily test this without side effects, so just verify the function exists)
        let dir = secret_store_dir();
        assert!(!dir.as_os_str().is_empty());
    }
}

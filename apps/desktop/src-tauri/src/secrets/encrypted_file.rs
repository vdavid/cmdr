//! Cocoon-encrypted file backend for Linux systems without a secret service.
//!
//! Stores secrets as a JSON map encrypted with ChaCha20-Poly1305 (via `cocoon`).
//! The encryption key is derived from `/etc/machine-id` (standard on systemd systems),
//! falling back to a fixed string where file permissions (0600) provide the primary protection.

use super::{SecretStore, SecretStoreError};
use cocoon::Cocoon;
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

/// Mutex for serializing file access across threads.
static FILE_STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Internal serialization format: keys map to byte arrays (JSON number arrays).
#[derive(Serialize, Deserialize, Default)]
struct StoreContents(HashMap<String, Vec<u8>>);

/// Stores secrets in a cocoon-encrypted file.
pub struct EncryptedFileStore {
    path: PathBuf,
}

impl EncryptedFileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join("credentials.enc"),
        }
    }
}

/// Reads `/etc/machine-id` as encryption password. Falls back to a fixed string
/// on non-systemd systems where file permissions (0600) provide the primary protection.
fn encryption_password() -> Vec<u8> {
    std::fs::read_to_string("/etc/machine-id")
        .unwrap_or_else(|_| "cmdr-credential-store".to_string())
        .trim()
        .as_bytes()
        .to_vec()
}

fn read_store(path: &PathBuf) -> StoreContents {
    if !path.exists() {
        return StoreContents::default();
    }
    let Ok(encrypted) = std::fs::read(path) else {
        warn!("Could not read encrypted credential file, starting fresh");
        return StoreContents::default();
    };
    let password = encryption_password();
    let cocoon = Cocoon::new(&password);
    match cocoon.unwrap(&encrypted) {
        Ok(decrypted) => serde_json::from_slice(&decrypted).unwrap_or_else(|e| {
            warn!("Credential file has invalid format ({}), starting fresh", e);
            StoreContents::default()
        }),
        Err(e) => {
            warn!("Could not decrypt credential file ({:?}), starting fresh", e);
            StoreContents::default()
        }
    }
}

fn write_store(path: &PathBuf, contents: &StoreContents) -> Result<(), SecretStoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SecretStoreError::Other(format!("Could not create credential directory: {}", e)))?;
    }
    let json = serde_json::to_vec(contents)
        .map_err(|e| SecretStoreError::Other(format!("Could not serialize secrets: {}", e)))?;
    let password = encryption_password();
    let mut cocoon = Cocoon::new(&password);
    let encrypted = cocoon
        .wrap(&json)
        .map_err(|e| SecretStoreError::Other(format!("Could not encrypt secrets: {:?}", e)))?;
    std::fs::write(path, &encrypted)
        .map_err(|e| SecretStoreError::Other(format!("Could not write credential file: {}", e)))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| SecretStoreError::Other(format!("Could not set file permissions: {}", e)))?;
    Ok(())
}

impl SecretStore for EncryptedFileStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let mut contents = read_store(&self.path);
        contents.0.insert(key.to_string(), value.to_vec());
        write_store(&self.path, &contents)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let contents = read_store(&self.path);
        contents
            .0
            .get(key)
            .cloned()
            .ok_or_else(|| SecretStoreError::NotFound(format!("No secret found for key: {}", key)))
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let mut contents = read_store(&self.path);
        if contents.0.remove(key).is_none() {
            return Err(SecretStoreError::NotFound(format!("No secret found for key: {}", key)));
        }
        write_store(&self.path, &contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let password = encryption_password();
        let data = b"test data for encryption";
        let mut cocoon = Cocoon::new(&password);
        let encrypted = cocoon.wrap(data).expect("wrap should succeed");
        let decrypted = Cocoon::new(&password)
            .unwrap(&encrypted)
            .expect("unwrap should succeed");
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_store_contents_serde_roundtrip() {
        let mut contents = StoreContents::default();
        contents.0.insert("key1".to_string(), b"hello".to_vec());
        contents.0.insert("key2".to_string(), b"world".to_vec());

        let json = serde_json::to_vec(&contents).unwrap();
        let parsed: StoreContents = serde_json::from_slice(&json).unwrap();

        assert_eq!(parsed.0.get("key1").unwrap(), b"hello");
        assert_eq!(parsed.0.get("key2").unwrap(), b"world");
    }

    #[test]
    fn test_read_store_missing_file() {
        let contents = read_store(&PathBuf::from("/tmp/nonexistent-cmdr-test-file.enc"));
        assert!(contents.0.is_empty());
    }
}

//! Plain JSON file backend for dev mode.
//!
//! Stores secrets as an unencrypted JSON file for convenience during development.
//! File permissions are set to 0600 on Unix systems.

use super::{SecretStore, SecretStoreError};
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

/// Mutex for serializing file access across threads.
static FILE_STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Internal serialization format: keys map to byte arrays (JSON number arrays).
#[derive(Serialize, Deserialize, Default)]
struct StoreContents(HashMap<String, Vec<u8>>);

/// Stores secrets as plain JSON on disk.
pub struct PlainFileStore {
    path: PathBuf,
}

impl PlainFileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join("secrets.json"),
        }
    }
}

fn read_store(path: &PathBuf) -> StoreContents {
    if !path.exists() {
        return StoreContents::default();
    }
    match std::fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).unwrap_or_else(|e| {
            warn!("Secret file has invalid format ({}), starting fresh", e);
            StoreContents::default()
        }),
        Err(e) => {
            warn!("Could not read secret file ({}), starting fresh", e);
            StoreContents::default()
        }
    }
}

fn write_store(path: &PathBuf, contents: &StoreContents) -> Result<(), SecretStoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SecretStoreError::Other(format!("Could not create secret directory: {}", e)))?;
    }
    let json = serde_json::to_string_pretty(contents)
        .map_err(|e| SecretStoreError::Other(format!("Could not serialize secrets: {}", e)))?;
    std::fs::write(path, json.as_bytes())
        .map_err(|e| SecretStoreError::Other(format!("Could not write secret file: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| SecretStoreError::Other(format!("Could not set file permissions: {}", e)))?;
    }

    Ok(())
}

impl SecretStore for PlainFileStore {
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
    fn test_store_contents_serde_roundtrip() {
        let mut contents = StoreContents::default();
        contents.0.insert("key1".to_string(), b"hello".to_vec());

        let json = serde_json::to_string_pretty(&contents).unwrap();
        let parsed: StoreContents = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.0.get("key1").unwrap(), b"hello");
    }

    #[test]
    fn test_read_store_missing_file() {
        let contents = read_store(&PathBuf::from("/tmp/nonexistent-cmdr-test-secrets.json"));
        assert!(contents.0.is_empty());
    }

    #[test]
    fn test_plain_file_store_roundtrip() {
        let dir = std::env::temp_dir().join("cmdr-test-plain-file-store");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let store = PlainFileStore::new(dir.clone());

        // Set and get
        store.set("test-key", b"test-value").unwrap();
        let value = store.get("test-key").unwrap();
        assert_eq!(value, b"test-value");

        // Overwrite
        store.set("test-key", b"new-value").unwrap();
        let value = store.get("test-key").unwrap();
        assert_eq!(value, b"new-value");

        // Delete
        store.delete("test-key").unwrap();
        assert!(matches!(store.get("test-key"), Err(SecretStoreError::NotFound(_))));

        // Delete non-existent
        assert!(matches!(
            store.delete("no-such-key"),
            Err(SecretStoreError::NotFound(_))
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

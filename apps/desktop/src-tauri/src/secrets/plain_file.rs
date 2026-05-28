//! Plain JSON file backend for dev mode.
//!
//! Stores secrets as an unencrypted JSON file for convenience during development.
//! File permissions are set to 0600 on Unix systems.

use super::{SecretStore, SecretStoreError};
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

/// Mutex for serializing file access across threads.
static FILE_STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Internal serialization format: keys map to byte arrays (JSON number arrays).
#[derive(Serialize, Deserialize, Default, Debug)]
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

fn read_store(path: &PathBuf) -> Result<StoreContents, SecretStoreError> {
    if !path.exists() {
        return Ok(StoreContents::default());
    }
    let data =
        std::fs::read(path).map_err(|e| SecretStoreError::Other(format!("Could not read secret file: {}", e)))?;
    // Pre-fix this swallowed the parse error and returned `default()`, which
    // then got persisted back on the next `set` — silent data loss on every
    // restart after a half-written file. Surface the error so callers see it;
    // leave the file in place for forensic inspection.
    serde_json::from_slice(&data).map_err(|e| {
        warn!(
            "Secret file at {} couldn't be parsed: {}; leaving in place",
            path.display(),
            e
        );
        SecretStoreError::Other(format!("Couldn't parse secret file: {}", e))
    })
}

fn write_store(path: &PathBuf, contents: &StoreContents) -> Result<(), SecretStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| SecretStoreError::Other("Secret path has no parent directory".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| SecretStoreError::Other(format!("Could not create secret directory: {}", e)))?;

    let json = serde_json::to_string_pretty(contents)
        .map_err(|e| SecretStoreError::Other(format!("Could not serialize secrets: {}", e)))?;

    // Atomic write: create a temp file in the same dir with the final
    // permissions baked in (no umask window), fsync the bytes, then
    // atomically rename into place. A crash between `write` and `rename`
    // leaves the original file intact; pre-fix `std::fs::write` truncated
    // first and the next launch silently dropped every stored secret.
    let temp_name = format!(".secrets.json.cmdr-tmp-{}", uuid::Uuid::new_v4());
    let temp_path = parent.join(temp_name);

    let mut open_opts = std::fs::OpenOptions::new();
    open_opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open_opts.mode(0o600);
    }

    let mut file = open_opts
        .open(&temp_path)
        .map_err(|e| SecretStoreError::Other(format!("Could not create secret temp file: {}", e)))?;

    if let Err(e) = file.write_all(json.as_bytes()).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(SecretStoreError::Other(format!("Could not write secret file: {}", e)));
    }
    drop(file);

    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        SecretStoreError::Other(format!("Could not finalize secret file: {}", e))
    })?;

    Ok(())
}

impl SecretStore for PlainFileStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let mut contents = read_store(&self.path)?;
        contents.0.insert(key.to_string(), value.to_vec());
        write_store(&self.path, &contents)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let contents = read_store(&self.path)?;
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
        let mut contents = read_store(&self.path)?;
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
        let contents = read_store(&PathBuf::from("/tmp/nonexistent-cmdr-test-secrets.json")).unwrap();
        assert!(contents.0.is_empty());
    }

    #[test]
    fn test_read_store_corrupt_file_returns_error_not_default() {
        // Regression for the low-severity audit finding: pre-fix, a corrupt
        // (half-written) secrets.json would parse-fail and silently get
        // replaced with `StoreContents::default()` on the next `set`, losing
        // every SMB credential and AI API key on disk.
        let dir = std::env::temp_dir().join("cmdr-test-plain-file-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.json");
        std::fs::write(&path, b"{\"bad\":json,").unwrap();

        let result = read_store(&path);
        assert!(
            matches!(result, Err(SecretStoreError::Other(_))),
            "corrupt file must surface an error, got {:?}",
            result
        );

        // The file must NOT be replaced by the read.
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"{\"bad\":json,",
            "corrupt file must stay on disk for forensic inspection"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_store_uses_atomic_rename() {
        // Regression for the low-severity audit finding: the write must go
        // through a temp file + rename so a crash mid-write can't truncate
        // the on-disk secrets and lose everything.
        let dir = std::env::temp_dir().join("cmdr-test-plain-file-atomic");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let store = PlainFileStore::new(dir.clone());
        store.set("k", b"v").unwrap();

        // The final secrets file is in place.
        assert!(dir.join("secrets.json").exists());

        // No temp files remain after a successful write.
        let leftover_temps: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("cmdr-tmp-"))
            .collect();
        assert!(leftover_temps.is_empty(), "atomic write must clean up its temp file");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("secrets.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "secrets file must be 0o600 from creation");
        }

        let _ = std::fs::remove_dir_all(&dir);
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

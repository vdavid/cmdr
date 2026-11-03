//! Cocoon-encrypted file backend for Linux systems without a secret service.
//!
//! Stores secrets as a JSON map encrypted with ChaCha20-Poly1305 (via `cocoon`).
//! The encryption key is derived from `/etc/machine-id` (standard on systemd systems), read at
//! every operation, falling back to a fixed string only where there is no machine id (file
//! permissions, 0600, are the primary protection there). A machine id that exists but can't be
//! read fails the operation.
//!
//! A new machine id (a restored home folder, a cloned VM) makes the file unreadable for good.
//! The first save or forget after that sets it aside and starts an empty store.

use super::{SecretStore, SecretStoreError};
use cocoon::Cocoon;
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::OpenOptionsExt;
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
    /// Where the cocoon password comes from.
    key: KeySource,
}

/// Where a store's cocoon password comes from.
enum KeySource {
    /// This machine's id, read at EVERY operation (`machine_key`), so a read that failed once
    /// never keys the store for its whole life.
    MachineId(PathBuf),
    /// A fixed password, so a test can play "the same file on another machine".
    #[cfg(test)]
    Fixed(Vec<u8>),
}

/// The password for a machine with no machine id (non-systemd): file permissions (0600) are the
/// primary protection there.
const FALLBACK_PASSWORD: &[u8] = b"cmdr-credential-store";

impl EncryptedFileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self::with_machine_id(dir, PathBuf::from("/etc/machine-id"))
    }

    /// A store keyed by the machine id at `machine_id`.
    fn with_machine_id(dir: PathBuf, machine_id: PathBuf) -> Self {
        Self {
            path: dir.join("credentials.enc"),
            key: KeySource::MachineId(machine_id),
        }
    }

    /// A store under `password` rather than this machine's id.
    #[cfg(test)]
    fn with_password(dir: PathBuf, password: Vec<u8>) -> Self {
        Self {
            path: dir.join("credentials.enc"),
            key: KeySource::Fixed(password),
        }
    }

    /// This machine's cocoon password, read now.
    ///
    /// ❗ A machine id that exists but can't be read is an ERROR, ❌ never the fallback (#537): the
    /// fallback would fail to open this machine's own file, and a save would then set it aside as
    /// if the home folder had moved machines. Only a machine with NO id keys by the fallback,
    /// which is then its real key for good.
    fn password(&self) -> Result<Vec<u8>, SecretStoreError> {
        match &self.key {
            KeySource::MachineId(path) => machine_key(path),
            #[cfg(test)]
            KeySource::Fixed(password) => Ok(password.clone()),
        }
    }
}

/// The machine id at `path` as a cocoon password: the fallback where there is none (absent or
/// blank), an error where it can't be read right now.
fn machine_key(path: &std::path::Path) -> Result<Vec<u8>, SecretStoreError> {
    match std::fs::read_to_string(path) {
        Ok(id) if !id.trim().is_empty() => Ok(id.trim().as_bytes().to_vec()),
        Ok(_) => Ok(FALLBACK_PASSWORD.to_vec()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FALLBACK_PASSWORD.to_vec()),
        Err(e) => {
            warn!(
                "Couldn't read the machine id at {}: {e}",
                cmdr_fs::log_private::LogPrivate::file_path(path)
            );
            Err(SecretStoreError::Other(format!("Couldn't read the machine id: {e}")))
        }
    }
}

/// Why a store file didn't read.
#[derive(Debug)]
enum ReadFailure {
    /// The file couldn't be read at all (permissions, I/O). Nothing is known about its contents.
    Io(SecretStoreError),
    /// The file was read but doesn't decrypt under this machine's key, or doesn't parse. A home
    /// folder restored onto a new install or a cloned VM lands here: `/etc/machine-id` changed,
    /// and no key this machine has will open the file again.
    Unreadable(SecretStoreError),
}

impl From<ReadFailure> for SecretStoreError {
    fn from(failure: ReadFailure) -> Self {
        match failure {
            ReadFailure::Io(e) | ReadFailure::Unreadable(e) => e,
        }
    }
}

fn read_store(path: &PathBuf, password: &[u8]) -> Result<StoreContents, ReadFailure> {
    if !path.exists() {
        return Ok(StoreContents::default());
    }
    let encrypted = std::fs::read(path).map_err(|e| {
        ReadFailure::Io(SecretStoreError::Other(format!(
            "Could not read encrypted credential file: {}",
            e
        )))
    })?;
    let cocoon = Cocoon::new(password);
    let decrypted = cocoon.unwrap(&encrypted).map_err(|e| {
        warn!(
            "Couldn't decrypt credential file at {}: {:?}",
            cmdr_fs::log_private::LogPrivate::file_path(path),
            e
        );
        ReadFailure::Unreadable(SecretStoreError::Other(format!(
            "Couldn't decrypt credential file: {:?}",
            e
        )))
    })?;
    // Pre-fix the parse-failure arm silently returned `default()` and the
    // next `set` overwrote the half-written file with empty state, losing
    // every stored secret. Surface the error so a read sees it, and a write
    // sets the file aside (`read_for_write`) rather than overwriting it.
    serde_json::from_slice(&decrypted).map_err(|e| {
        warn!(
            "Credential file at {} couldn't be parsed: {}",
            cmdr_fs::log_private::LogPrivate::file_path(path),
            e
        );
        ReadFailure::Unreadable(SecretStoreError::Other(format!(
            "Couldn't parse credential file: {}",
            e
        )))
    })
}

fn write_store(path: &PathBuf, password: &[u8], contents: &StoreContents) -> Result<(), SecretStoreError> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| SecretStoreError::Other("Credential path has no parent directory".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| SecretStoreError::Other(format!("Could not create credential directory: {}", e)))?;

    let json = serde_json::to_vec(contents)
        .map_err(|e| SecretStoreError::Other(format!("Could not serialize secrets: {}", e)))?;
    let mut cocoon = Cocoon::new(password);
    let encrypted = cocoon
        .wrap(&json)
        .map_err(|e| SecretStoreError::Other(format!("Could not encrypt secrets: {:?}", e)))?;

    // Atomic write: temp file with final permissions baked in, fsync, rename.
    // Pre-fix a crash mid-write truncated the file and the next launch
    // silently dropped every stored credential.
    let temp_name = format!(".credentials.enc.cmdr-tmp-{}", uuid::Uuid::new_v4());
    let temp_path = parent.join(temp_name);

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|e| SecretStoreError::Other(format!("Could not create credential temp file: {}", e)))?;

    if let Err(e) = file.write_all(&encrypted).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(SecretStoreError::Other(format!(
            "Could not write credential file: {}",
            e
        )));
    }
    drop(file);

    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        SecretStoreError::Other(format!("Could not finalize credential file: {}", e))
    })?;
    Ok(())
}

impl EncryptedFileStore {
    /// The contents a `set` or `delete` builds on. A file this machine can't read is set aside
    /// (`state_recovery`, which tells the person once) and the store starts empty, ❌ never left
    /// in the way: every later save would read it first and fail, so no password could ever be
    /// saved again. `get` and `keys` leave it alone; a read decides nothing.
    fn read_for_write(&self, password: &[u8]) -> Result<StoreContents, SecretStoreError> {
        match read_store(&self.path, password) {
            Ok(contents) => Ok(contents),
            Err(ReadFailure::Io(e)) => Err(e),
            Err(ReadFailure::Unreadable(_)) => {
                // A rename that doesn't happen still starts fresh: the write that follows
                // replaces a file no key on this machine opens, and the notice says nothing
                // was kept.
                crate::state_recovery::set_aside(&self.path, crate::state_recovery::StateFile::Credentials);
                Ok(StoreContents::default())
            }
        }
    }
}

impl SecretStore for EncryptedFileStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let password = self.password()?;
        let mut contents = self.read_for_write(&password)?;
        contents.0.insert(key.to_string(), value.to_vec());
        write_store(&self.path, &password, &contents)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let contents = read_store(&self.path, &self.password()?)?;
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
        let password = self.password()?;
        let mut contents = self.read_for_write(&password)?;
        if contents.0.remove(key).is_none() {
            return Err(SecretStoreError::NotFound(format!("No secret found for key: {}", key)));
        }
        write_store(&self.path, &password, &contents)
    }

    fn keys(&self) -> Result<Vec<String>, SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        Ok(read_store(&self.path, &self.password()?)?.0.into_keys().collect())
    }
}

#[cfg(test)]
#[path = "encrypted_file_test.rs"]
mod tests;

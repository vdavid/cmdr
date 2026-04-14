//! Linux Secret Service backend via the `keyring` crate.

use super::{SecretStore, SecretStoreError};
use log::debug;

const SERVICE_NAME: &str = "Cmdr";

/// Stores secrets via the Linux Secret Service (GNOME Keyring, KDE Wallet).
pub struct KeyringStore;

impl KeyringStore {
    /// Checks whether a secret service is reachable and functional on this system.
    /// Does a real write-read-delete round-trip to catch locked keyrings that silently
    /// accept writes without persisting (a known issue on some GNOME/KDE setups).
    pub fn is_available() -> bool {
        let entry = match keyring::Entry::new(SERVICE_NAME, "cmdr-probe") {
            Ok(e) => e,
            Err(_) => return false,
        };
        if entry.set_password("probe").is_err() {
            return false;
        }
        let ok = entry.get_password().is_ok();
        let _ = entry.delete_credential();
        ok
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        debug!("Keyring: setting secret for key: {}", key);
        let password = String::from_utf8(value.to_vec())
            .map_err(|e| SecretStoreError::Other(format!("Value is not valid UTF-8: {}", e)))?;
        let entry = keyring::Entry::new(SERVICE_NAME, key)
            .map_err(|e| SecretStoreError::Other(format!("Failed to create keyring entry: {}", e)))?;
        entry
            .set_password(&password)
            .map_err(|e| SecretStoreError::Other(format!("Failed to store secret: {}", e)))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        debug!("Keyring: getting secret for key: {}", key);
        let entry = keyring::Entry::new(SERVICE_NAME, key)
            .map_err(|e| SecretStoreError::Other(format!("Failed to create keyring entry: {}", e)))?;
        match entry.get_password() {
            Ok(password) => Ok(password.into_bytes()),
            Err(keyring::Error::NoEntry) => {
                Err(SecretStoreError::NotFound(format!("No secret found for key: {}", key)))
            }
            Err(e) => Err(SecretStoreError::Other(format!("Failed to get secret: {}", e))),
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        debug!("Keyring: deleting secret for key: {}", key);
        let entry = keyring::Entry::new(SERVICE_NAME, key)
            .map_err(|e| SecretStoreError::Other(format!("Failed to create keyring entry: {}", e)))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => {
                Err(SecretStoreError::NotFound(format!("No secret found for key: {}", key)))
            }
            Err(e) => Err(SecretStoreError::Other(format!("Failed to delete secret: {}", e))),
        }
    }
}

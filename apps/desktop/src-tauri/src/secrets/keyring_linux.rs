//! Linux Secret Service backend via the `keyring-core` crate plus the zbus-based
//! Secret Service store. Replaced the legacy `keyring = "3"` crate during the v4
//! ecosystem split (see `Cargo.toml` for context).

use super::{SecretStore, SecretStoreError};
use log::{debug, warn};
use std::sync::Once;

const SERVICE_NAME: &str = "Cmdr";

/// keyring-core uses a process-wide default store. We install ours exactly once on
/// first use; subsequent calls are no-ops. Failure is non-fatal: `Entry::new` will
/// return an error which we surface via `SecretStoreError::Other`, matching the prior
/// behaviour.
fn ensure_default_store() {
    static INIT: Once = Once::new();
    INIT.call_once(|| match zbus_secret_service_keyring_store::store::Store::new() {
        Ok(store) => {
            keyring_core::set_default_store(store);
            debug!("keyring-core default store set: zbus secret-service");
        }
        Err(e) => {
            warn!("Failed to initialize zbus secret-service store: {}", e);
        }
    });
}

/// Stores secrets via the Linux Secret Service (GNOME Keyring, KDE Wallet).
pub struct KeyringStore;

impl KeyringStore {
    /// Checks whether a secret service is reachable and functional on this system.
    /// Does a real write-read-delete round-trip to catch locked keyrings that silently
    /// accept writes without persisting (a known issue on some GNOME/KDE setups).
    pub fn is_available() -> bool {
        ensure_default_store();
        let entry = match keyring_core::Entry::new(SERVICE_NAME, "cmdr-probe") {
            Ok(e) => e,
            Err(_) => return false,
        };
        if entry.set_secret(b"probe").is_err() {
            return false;
        }
        let ok = entry.get_secret().is_ok();
        let _ = entry.delete_credential();
        ok
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        debug!("Keyring: setting secret for key: {}", key);
        ensure_default_store();
        let entry = keyring_core::Entry::new(SERVICE_NAME, key)
            .map_err(|e| SecretStoreError::Other(format!("Failed to create keyring entry: {}", e)))?;
        entry
            .set_secret(value)
            .map_err(|e| SecretStoreError::Other(format!("Failed to store secret: {}", e)))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        debug!("Keyring: getting secret for key: {}", key);
        ensure_default_store();
        let entry = keyring_core::Entry::new(SERVICE_NAME, key)
            .map_err(|e| SecretStoreError::Other(format!("Failed to create keyring entry: {}", e)))?;
        match entry.get_secret() {
            Ok(secret) => Ok(secret),
            Err(keyring_core::Error::NoEntry) => {
                Err(SecretStoreError::NotFound(format!("No secret found for key: {}", key)))
            }
            Err(e) => Err(SecretStoreError::Other(format!("Failed to get secret: {}", e))),
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        debug!("Keyring: deleting secret for key: {}", key);
        ensure_default_store();
        let entry = keyring_core::Entry::new(SERVICE_NAME, key)
            .map_err(|e| SecretStoreError::Other(format!("Failed to create keyring entry: {}", e)))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring_core::Error::NoEntry) => {
                Err(SecretStoreError::NotFound(format!("No secret found for key: {}", key)))
            }
            Err(e) => Err(SecretStoreError::Other(format!("Failed to delete secret: {}", e))),
        }
    }
}

//! macOS Keychain backend via `security-framework`.

use super::{SecretStore, SecretStoreError};
use log::debug;
use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

const SERVICE_NAME: &str = "Cmdr";

/// Stores secrets in the macOS Keychain.
pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        debug!("Keychain: setting secret for key: {}", key);
        set_generic_password(SERVICE_NAME, key, value)
            .map_err(|e| SecretStoreError::Other(format!("Failed to save to Keychain: {}", e)))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        debug!("Keychain: getting secret for key: {}", key);
        get_generic_password(SERVICE_NAME, key).map_err(|e| classify_security_error(key, e))
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        debug!("Keychain: deleting secret for key: {}", key);
        delete_generic_password(SERVICE_NAME, key).map_err(|e| classify_security_error(key, e))
    }
}

fn classify_security_error(key: &str, error: security_framework::base::Error) -> SecretStoreError {
    let msg = format!("{}", error);
    if msg.contains("not found") || msg.contains("No such") || msg.contains("errSecItemNotFound") {
        SecretStoreError::NotFound(format!("No secret found for key: {}", key))
    } else if msg.contains("denied") || msg.contains("cancelled") {
        SecretStoreError::AccessDenied(msg)
    } else {
        SecretStoreError::Other(msg)
    }
}

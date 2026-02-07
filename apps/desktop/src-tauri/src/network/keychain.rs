//! Keychain integration for SMB credentials.
//!
//! Uses macOS Security.framework via the security-framework crate
//! to securely store and retrieve SMB credentials.
//!
//! Credentials are cached in-memory after first access to avoid
//! repeated Keychain dialogs during a session.

use log::{debug, warn};
use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Service name used for Keychain items.
/// This appears in Keychain Access.app.
const SERVICE_NAME: &str = "Cmdr";

/// In-memory cache for credentials to avoid repeated Keychain dialogs.
/// Key is the account name (like "smb://server" or "smb://server/share").
static CREDENTIAL_CACHE: std::sync::LazyLock<RwLock<HashMap<String, SmbCredentials>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Credentials for SMB authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmbCredentials {
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
}

/// Error types for Keychain operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum KeychainError {
    /// Credentials not found in Keychain
    NotFound(String),
    /// Access denied (user cancelled or insufficient permissions)
    AccessDenied(String),
    /// Other Keychain error
    Other(String),
}

impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Credentials not found: {}", msg),
            Self::AccessDenied(msg) => write!(f, "Keychain access denied: {}", msg),
            Self::Other(msg) => write!(f, "Keychain error: {}", msg),
        }
    }
}

impl std::error::Error for KeychainError {}

/// Creates the account name used for Keychain storage.
/// Format: "smb://{server}/{share}" or "smb://{server}" for server-level credentials.
fn make_account_name(server: &str, share: Option<&str>) -> String {
    match share {
        Some(s) => format!("smb://{}/{}", server.to_lowercase(), s),
        None => format!("smb://{}", server.to_lowercase()),
    }
}

/// Parses a stored password entry to extract username and password.
/// Format: "username\0password" (null-separated)
fn parse_password_entry(data: &[u8]) -> Option<SmbCredentials> {
    let text = String::from_utf8_lossy(data);
    let parts: Vec<&str> = text.splitn(2, '\0').collect();
    if parts.len() == 2 {
        Some(SmbCredentials {
            username: parts[0].to_string(),
            password: parts[1].to_string(),
        })
    } else {
        None
    }
}

/// Creates a password entry for storage.
/// Format: "username\0password" (null-separated)
fn make_password_entry(username: &str, password: &str) -> Vec<u8> {
    format!("{}\0{}", username, password).into_bytes()
}

/// Saves SMB credentials to the Keychain.
///
/// # Arguments
/// * `server` - Server hostname or IP
/// * `share` - Optional share name (None for server-level credentials)
/// * `username` - Username for authentication
/// * `password` - Password for authentication
pub fn save_credentials(
    server: &str,
    share: Option<&str>,
    username: &str,
    password: &str,
) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);
    let entry = make_password_entry(username, password);

    debug!("Saving credentials to Keychain for account: {}", account);

    set_generic_password(SERVICE_NAME, &account, &entry).map_err(|e| {
        let msg = format!("Failed to save credentials: {}", e);
        warn!("{}", msg);
        KeychainError::Other(msg)
    })?;

    // Update the in-memory cache
    if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
        cache.insert(
            account,
            SmbCredentials {
                username: username.to_string(),
                password: password.to_string(),
            },
        );
    }

    Ok(())
}

/// Retrieves SMB credentials from the Keychain.
///
/// # Arguments
/// * `server` - Server hostname or IP
/// * `share` - Optional share name (None for server-level credentials)
///
/// # Returns
/// * `Some(SmbCredentials)` if found
/// * `None` if not found
pub fn get_credentials(server: &str, share: Option<&str>) -> Result<SmbCredentials, KeychainError> {
    let account = make_account_name(server, share);

    // Check in-memory cache first to avoid Keychain dialog
    if let Ok(cache) = CREDENTIAL_CACHE.read()
        && let Some(creds) = cache.get(&account)
    {
        debug!("Returning cached credentials for account: {}", account);
        return Ok(creds.clone());
    }

    debug!("Getting credentials from Keychain for account: {}", account);

    match get_generic_password(SERVICE_NAME, &account) {
        Ok(data) => {
            let creds = parse_password_entry(&data)
                .ok_or_else(|| KeychainError::Other("Invalid credential format in Keychain".to_string()))?;

            // Cache the credentials for future use
            if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
                cache.insert(account, creds.clone());
            }

            Ok(creds)
        }
        Err(e) => {
            // Check if it's a "not found" error
            let msg = format!("{}", e);
            if msg.contains("not found") || msg.contains("No such") || msg.contains("errSecItemNotFound") {
                Err(KeychainError::NotFound(format!("No credentials found for {}", account)))
            } else if msg.contains("denied") || msg.contains("cancelled") {
                Err(KeychainError::AccessDenied(msg))
            } else {
                Err(KeychainError::Other(msg))
            }
        }
    }
}

/// Deletes SMB credentials from the Keychain.
///
/// # Arguments
/// * `server` - Server hostname or IP
/// * `share` - Optional share name (None for server-level credentials)
pub fn delete_credentials(server: &str, share: Option<&str>) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);

    debug!("Deleting credentials from Keychain for account: {}", account);

    // Remove from cache first
    if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
        cache.remove(&account);
    }

    delete_generic_password(SERVICE_NAME, &account).map_err(|e| {
        let msg = format!("{}", e);
        if msg.contains("not found") || msg.contains("No such") {
            KeychainError::NotFound(format!("No credentials found for {}", account))
        } else {
            KeychainError::Other(msg)
        }
    })
}

/// Checks if credentials exist in the Keychain without retrieving them.
/// This is useful for checking if we should try stored credentials first.
///
/// # Arguments
/// * `server` - Server hostname or IP
/// * `share` - Optional share name
pub fn has_credentials(server: &str, share: Option<&str>) -> bool {
    get_credentials(server, share).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_account_name_server_only() {
        let account = make_account_name("TEST_SERVER", None);
        assert_eq!(account, "smb://test_server");
    }

    #[test]
    fn test_make_account_name_with_share() {
        let account = make_account_name("TEST_SERVER", Some("Documents"));
        assert_eq!(account, "smb://test_server/Documents");
    }

    #[test]
    fn test_make_account_name_case_insensitive_server() {
        let account1 = make_account_name("TEST_SERVER", Some("Share"));
        let account2 = make_account_name("test_server", Some("Share"));
        assert_eq!(account1, account2);
    }

    #[test]
    fn test_parse_password_entry() {
        let entry = make_password_entry("david", "secret123");
        let creds = parse_password_entry(&entry).unwrap();
        assert_eq!(creds.username, "david");
        assert_eq!(creds.password, "secret123");
    }

    #[test]
    fn test_parse_password_entry_with_special_chars() {
        let entry = make_password_entry("user@domain.com", "p@ss:w0rd!");
        let creds = parse_password_entry(&entry).unwrap();
        assert_eq!(creds.username, "user@domain.com");
        assert_eq!(creds.password, "p@ss:w0rd!");
    }

    #[test]
    fn test_parse_password_entry_with_null_in_password() {
        // Password containing null byte should work (only first null is separator)
        let entry = b"user\0pass\0word".to_vec();
        let creds = parse_password_entry(&entry).unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass\0word");
    }

    #[test]
    fn test_parse_password_entry_invalid() {
        let invalid = b"no-separator-here".to_vec();
        assert!(parse_password_entry(&invalid).is_none());
    }

    // Note: Actual Keychain operations can't be unit tested without mocking.
    // Integration tests would need to run on macOS with proper entitlements.
}

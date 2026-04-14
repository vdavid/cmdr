//! Keychain integration for SMB credentials.
//!
//! Delegates to `crate::secrets::store()` for platform-agnostic secret storage.
//! Credentials are cached in-memory after first access to avoid
//! repeated backend lookups during a session.

use crate::secrets::SecretStoreError;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory cache for credentials to avoid repeated backend lookups.
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
    /// Credentials not found
    NotFound(String),
    /// Access denied (user cancelled or insufficient permissions)
    AccessDenied(String),
    /// Other error
    Other(String),
}

impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Credentials not found: {}", msg),
            Self::AccessDenied(msg) => write!(f, "Credential access denied: {}", msg),
            Self::Other(msg) => write!(f, "Credential error: {}", msg),
        }
    }
}

impl std::error::Error for KeychainError {}

impl From<SecretStoreError> for KeychainError {
    fn from(e: SecretStoreError) -> Self {
        match e {
            SecretStoreError::NotFound(msg) => KeychainError::NotFound(msg),
            SecretStoreError::AccessDenied(msg) => KeychainError::AccessDenied(msg),
            SecretStoreError::Other(msg) => KeychainError::Other(msg),
        }
    }
}

/// Creates the account name used for credential storage.
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

/// Saves SMB credentials to the secret store.
pub fn save_credentials(
    server: &str,
    share: Option<&str>,
    username: &str,
    password: &str,
) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);
    let entry = make_password_entry(username, password);

    debug!("Saving credentials for account: {}", account);

    crate::secrets::store().set(&account, &entry)?;

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

/// Retrieves SMB credentials from the secret store.
pub fn get_credentials(server: &str, share: Option<&str>) -> Result<SmbCredentials, KeychainError> {
    let account = make_account_name(server, share);

    // Check in-memory cache first
    if let Ok(cache) = CREDENTIAL_CACHE.read()
        && let Some(creds) = cache.get(&account)
    {
        debug!("Returning cached credentials for account: {}", account);
        return Ok(creds.clone());
    }

    debug!("Getting credentials for account: {}", account);

    let data = crate::secrets::store().get(&account)?;
    let creds = parse_password_entry(&data)
        .ok_or_else(|| KeychainError::Other("Invalid credential format in store".to_string()))?;

    // Cache the credentials for future use
    if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
        cache.insert(account, creds.clone());
    }

    Ok(creds)
}

/// Deletes SMB credentials from the secret store.
pub fn delete_credentials(server: &str, share: Option<&str>) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);

    debug!("Deleting credentials for account: {}", account);

    // Remove from cache first
    if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
        cache.remove(&account);
    }

    crate::secrets::store().delete(&account)?;

    Ok(())
}

/// Checks if credentials exist without retrieving them.
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
}

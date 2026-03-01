//! Credential storage for SMB on Linux.
//!
//! Uses the `keyring` crate which delegates to the platform's secret service
//! (GNOME Keyring, KDE Wallet, or similar via the Secret Service D-Bus API).
//!
//! Credentials are cached in-memory after first access to avoid repeated
//! D-Bus round-trips during a session.

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Service name used for keyring items.
const SERVICE_NAME: &str = "Cmdr";

/// In-memory cache for credentials to avoid repeated secret service lookups.
/// Key is the account name (like "smb://server" or "smb://server/share").
static CREDENTIAL_CACHE: std::sync::LazyLock<RwLock<HashMap<String, SmbCredentials>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Credentials for SMB authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmbCredentials {
    pub username: String,
    pub password: String,
}

/// Error types for credential operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum KeychainError {
    NotFound(String),
    AccessDenied(String),
    Other(String),
}

impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Credentials not found: {}", msg),
            Self::AccessDenied(msg) => write!(f, "Secret service access denied: {}", msg),
            Self::Other(msg) => write!(f, "Credential error: {}", msg),
        }
    }
}

impl std::error::Error for KeychainError {}

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
fn parse_password_entry(data: &str) -> Option<SmbCredentials> {
    let parts: Vec<&str> = data.splitn(2, '\0').collect();
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
fn make_password_entry(username: &str, password: &str) -> String {
    format!("{}\0{}", username, password)
}

/// Saves SMB credentials to the secret service.
pub fn save_credentials(
    server: &str,
    share: Option<&str>,
    username: &str,
    password: &str,
) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);
    let entry_data = make_password_entry(username, password);

    debug!("Saving credentials to secret service for account: {}", account);

    let entry = keyring::Entry::new(SERVICE_NAME, &account)
        .map_err(|e| KeychainError::Other(format!("Failed to create keyring entry: {}", e)))?;

    entry.set_password(&entry_data).map_err(|e| {
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

/// Retrieves SMB credentials from the secret service.
pub fn get_credentials(server: &str, share: Option<&str>) -> Result<SmbCredentials, KeychainError> {
    let account = make_account_name(server, share);

    // Check in-memory cache first
    if let Ok(cache) = CREDENTIAL_CACHE.read()
        && let Some(creds) = cache.get(&account)
    {
        debug!("Returning cached credentials for account: {}", account);
        return Ok(creds.clone());
    }

    debug!("Getting credentials from secret service for account: {}", account);

    let entry = keyring::Entry::new(SERVICE_NAME, &account)
        .map_err(|e| KeychainError::Other(format!("Failed to create keyring entry: {}", e)))?;

    match entry.get_password() {
        Ok(data) => {
            let creds = parse_password_entry(&data)
                .ok_or_else(|| KeychainError::Other("Invalid credential format".to_string()))?;

            // Cache the credentials for future use
            if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
                cache.insert(account, creds.clone());
            }

            Ok(creds)
        }
        Err(keyring::Error::NoEntry) => Err(KeychainError::NotFound(format!("No credentials found for {}", account))),
        Err(keyring::Error::Ambiguous(_)) => Err(KeychainError::Other(format!(
            "Multiple credentials found for {}",
            account
        ))),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("denied") || msg.contains("cancelled") {
                Err(KeychainError::AccessDenied(msg))
            } else {
                Err(KeychainError::Other(msg))
            }
        }
    }
}

/// Deletes SMB credentials from the secret service.
pub fn delete_credentials(server: &str, share: Option<&str>) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);

    debug!("Deleting credentials from secret service for account: {}", account);

    // Remove from cache first
    if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
        cache.remove(&account);
    }

    let entry = keyring::Entry::new(SERVICE_NAME, &account)
        .map_err(|e| KeychainError::Other(format!("Failed to create keyring entry: {}", e)))?;

    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Err(KeychainError::NotFound(format!("No credentials found for {}", account))),
        Err(e) => Err(KeychainError::Other(format!("{}", e))),
    }
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
    fn test_parse_password_entry_invalid() {
        assert!(parse_password_entry("no-separator-here").is_none());
    }
}

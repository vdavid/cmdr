//! Credential storage for SMB on Linux.
//!
//! Uses a two-tier strategy:
//! 1. Secret Service D-Bus API via the `keyring` crate (GNOME Keyring, KDE Wallet)
//! 2. Encrypted file store via `cocoon` as fallback when no secret service is available
//!
//! Credentials are cached in-memory after first access to avoid repeated
//! backend lookups during a session.

use cocoon::Cocoon;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

/// Service name used for keyring items.
const SERVICE_NAME: &str = "Cmdr";

/// In-memory cache for credentials to avoid repeated backend lookups.
/// Key is the account name (like "smb://server" or "smb://server/share").
static CREDENTIAL_CACHE: std::sync::LazyLock<RwLock<HashMap<String, SmbCredentials>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Mutex for serializing file-based credential store access.
static FILE_STORE_LOCK: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

/// Whether the file-based fallback has been used this session.
static USING_FILE_FALLBACK: AtomicBool = AtomicBool::new(false);

/// Whether we've already logged the fallback info message.
static FILE_FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

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

// --- Account naming ---

/// Creates the account name used for credential storage.
/// Format: "smb://{server}/{share}" or "smb://{server}" for server-level credentials.
fn make_account_name(server: &str, share: Option<&str>) -> String {
    match share {
        Some(s) => format!("smb://{}/{}", server.to_lowercase(), s),
        None => format!("smb://{}", server.to_lowercase()),
    }
}

// --- Secret Service helpers (keyring crate) ---

/// Format: "username\0password" (null-separated, for keyring string storage)
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

fn make_password_entry(username: &str, password: &str) -> String {
    format!("{}\0{}", username, password)
}

fn secret_service_save(account: &str, username: &str, password: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, account).map_err(|e| format!("{}", e))?;
    let data = make_password_entry(username, password);
    entry.set_password(&data).map_err(|e| format!("{}", e))
}

/// Returns `Ok(Some(creds))` if found, `Ok(None)` if not found or service unavailable.
fn secret_service_get(account: &str) -> Result<Option<SmbCredentials>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, account).map_err(|e| format!("{}", e))?;
    match entry.get_password() {
        Ok(data) => Ok(parse_password_entry(&data)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("{}", e)),
    }
}

/// Returns `Ok(true)` if deleted, `Ok(false)` if not found.
fn secret_service_delete(account: &str) -> Result<bool, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, account).map_err(|e| format!("{}", e))?;
    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("{}", e)),
    }
}

// --- Encrypted file store ---

/// Credential entry stored in the encrypted file.
#[derive(Serialize, Deserialize)]
struct FileCredentialEntry {
    username: String,
    password: String,
}

type FileCredentialMap = HashMap<String, FileCredentialEntry>;

fn credential_file_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("cmdr")
        .join("credentials.enc")
}

/// Reads `/etc/machine-id` as encryption password (standard on systemd systems:
/// Ubuntu, Fedora, Arch). Falls back to a fixed string on non-systemd systems
/// where file permissions (0600) provide the primary protection.
fn encryption_password() -> Vec<u8> {
    std::fs::read_to_string("/etc/machine-id")
        .unwrap_or_else(|_| "cmdr-credential-store".to_string())
        .trim()
        .as_bytes()
        .to_vec()
}

/// Reads and decrypts the credential file. Returns an empty map on any failure
/// (missing file, corrupted data, wrong key) so callers can recover gracefully.
fn read_credential_file() -> FileCredentialMap {
    let path = credential_file_path();
    if !path.exists() {
        return HashMap::new();
    }
    let Ok(encrypted) = std::fs::read(&path) else {
        warn!("Could not read credential file, starting fresh");
        return HashMap::new();
    };
    let password = encryption_password();
    let cocoon = Cocoon::new(&password);
    match cocoon.unwrap(&encrypted) {
        Ok(decrypted) => serde_json::from_slice(&decrypted).unwrap_or_else(|e| {
            warn!("Credential file has invalid format ({}), starting fresh", e);
            HashMap::new()
        }),
        Err(e) => {
            warn!("Could not decrypt credential file ({:?}), starting fresh", e);
            HashMap::new()
        }
    }
}

fn write_credential_file(creds: &FileCredentialMap) -> Result<(), String> {
    let path = credential_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Could not create credential directory: {}", e))?;
    }
    let json = serde_json::to_vec(creds).map_err(|e| format!("Could not serialize credentials: {}", e))?;
    let password = encryption_password();
    let mut cocoon = Cocoon::new(&password);
    let encrypted = cocoon
        .wrap(&json)
        .map_err(|e| format!("Could not encrypt credentials: {:?}", e))?;
    std::fs::write(&path, &encrypted).map_err(|e| format!("Could not write credential file: {}", e))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("Could not set credential file permissions: {}", e))?;
    Ok(())
}

fn mark_file_fallback() {
    USING_FILE_FALLBACK.store(true, Ordering::Relaxed);
    if !FILE_FALLBACK_LOGGED.swap(true, Ordering::Relaxed) {
        info!("No system keyring detected, using encrypted file-based credential storage");
    }
}

fn file_save(account: &str, username: &str, password: &str) -> Result<(), KeychainError> {
    let _lock = FILE_STORE_LOCK
        .lock()
        .map_err(|e| KeychainError::Other(format!("Lock error: {}", e)))?;
    let mut creds = read_credential_file();
    creds.insert(
        account.to_string(),
        FileCredentialEntry {
            username: username.to_string(),
            password: password.to_string(),
        },
    );
    write_credential_file(&creds).map_err(KeychainError::Other)?;
    mark_file_fallback();
    Ok(())
}

fn file_get(account: &str) -> Option<SmbCredentials> {
    let _lock = FILE_STORE_LOCK.lock().ok()?;
    let creds = read_credential_file();
    let entry = creds.get(account)?;
    mark_file_fallback();
    Some(SmbCredentials {
        username: entry.username.clone(),
        password: entry.password.clone(),
    })
}

/// Returns `Ok(true)` if deleted, `Ok(false)` if not found.
fn file_delete(account: &str) -> Result<bool, KeychainError> {
    let _lock = FILE_STORE_LOCK
        .lock()
        .map_err(|e| KeychainError::Other(format!("Lock error: {}", e)))?;
    let mut creds = read_credential_file();
    if creds.remove(account).is_none() {
        return Ok(false);
    }
    write_credential_file(&creds).map_err(KeychainError::Other)?;
    Ok(true)
}

// --- Cache helpers ---

fn cache_put(account: &str, creds: &SmbCredentials) {
    if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
        cache.insert(account.to_string(), creds.clone());
    }
}

fn cache_remove(account: &str) {
    if let Ok(mut cache) = CREDENTIAL_CACHE.write() {
        cache.remove(account);
    }
}

fn cache_get(account: &str) -> Option<SmbCredentials> {
    CREDENTIAL_CACHE.read().ok()?.get(account).cloned()
}

// --- Public API ---

/// Returns whether the encrypted file fallback is being used instead of the system keyring.
pub fn is_using_file_fallback() -> bool {
    USING_FILE_FALLBACK.load(Ordering::Relaxed)
}

/// Saves SMB credentials. Tries Secret Service first, falls back to encrypted file.
pub fn save_credentials(
    server: &str,
    share: Option<&str>,
    username: &str,
    password: &str,
) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);
    let creds = SmbCredentials {
        username: username.to_string(),
        password: password.to_string(),
    };

    debug!("Saving credentials for account: {}", account);

    // Try Secret Service first, then verify it actually persisted.
    // Locked keyrings may accept writes silently without persisting them.
    match secret_service_save(&account, username, password) {
        Ok(()) => match secret_service_get(&account) {
            Ok(Some(_)) => {
                cache_put(&account, &creds);
                return Ok(());
            }
            _ => debug!("Secret service save appeared to succeed but read-back failed (keyring likely locked), trying file backend"),
        },
        Err(e) => debug!("Secret service save failed, trying file backend: {}", e),
    }

    // Fall back to encrypted file
    file_save(&account, username, password)?;
    cache_put(&account, &creds);
    Ok(())
}

/// Retrieves SMB credentials. Checks cache, then Secret Service, then encrypted file.
pub fn get_credentials(server: &str, share: Option<&str>) -> Result<SmbCredentials, KeychainError> {
    let account = make_account_name(server, share);

    // Check in-memory cache first
    if let Some(creds) = cache_get(&account) {
        debug!("Returning cached credentials for account: {}", account);
        return Ok(creds);
    }

    debug!("Getting credentials for account: {}", account);

    // Try Secret Service
    match secret_service_get(&account) {
        Ok(Some(creds)) => {
            cache_put(&account, &creds);
            return Ok(creds);
        }
        Ok(None) => {} // Not found in secret service — try file backend
        Err(e) => debug!("Secret service lookup failed, trying file backend: {}", e),
    }

    // Try encrypted file
    if let Some(creds) = file_get(&account) {
        cache_put(&account, &creds);
        return Ok(creds);
    }

    Err(KeychainError::NotFound(format!("No credentials found for {}", account)))
}

/// Deletes SMB credentials from all backends.
pub fn delete_credentials(server: &str, share: Option<&str>) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);
    debug!("Deleting credentials for account: {}", account);

    cache_remove(&account);

    let ss_deleted = secret_service_delete(&account).unwrap_or(false);
    let file_deleted = file_delete(&account).unwrap_or(false);

    if ss_deleted || file_deleted {
        Ok(())
    } else {
        Err(KeychainError::NotFound(format!("No credentials found for {}", account)))
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

    #[test]
    fn test_credential_file_path() {
        let path = credential_file_path();
        assert!(path.to_string_lossy().contains("cmdr"));
        assert!(path.to_string_lossy().ends_with("credentials.enc"));
    }

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
    fn test_file_credential_map_serialization() {
        let mut creds = FileCredentialMap::new();
        creds.insert(
            "smb://server/share".to_string(),
            FileCredentialEntry {
                username: "user".to_string(),
                password: "pass".to_string(),
            },
        );
        let json = serde_json::to_vec(&creds).unwrap();
        let parsed: FileCredentialMap = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed["smb://server/share"].username, "user");
        assert_eq!(parsed["smb://server/share"].password, "pass");
    }
}

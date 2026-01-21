//! Network stubs for Linux/non-macOS platforms.
//!
//! Provides minimal implementations that return empty results.
//! Network browsing is not supported on Linux in this stub implementation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Types (mirroring the macOS implementation)
// ============================================================================

/// A discovered network host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHost {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    pub port: u16,
}

/// State of network discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryState {
    Idle,
    Searching,
    Active,
}

/// Authentication mode detected for a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    GuestAllowed,
    CredsRequired,
    Unknown,
}

/// Information about a discovered share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    pub name: String,
    pub is_disk: bool,
    pub comment: Option<String>,
}

/// Result of a share listing operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareListResult {
    pub shares: Vec<ShareInfo>,
    pub auth_mode: AuthMode,
    pub from_cache: bool,
}

/// Error types for share listing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum ShareListError {
    HostUnreachable(String),
    Timeout(String),
    AuthRequired(String),
    SigningRequired(String),
    AuthFailed(String),
    ProtocolError(String),
    ResolutionFailed(String),
}

/// Connection mode used for the last successful connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    Guest,
    Credentials,
}

/// Authentication options available for a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthOptions {
    GuestOnly,
    CredentialsOnly,
    GuestOrCredentials,
}

/// Information about a known network share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownNetworkShare {
    pub server_name: String,
    pub share_name: String,
    pub protocol: String,
    pub last_connected_at: String,
    pub last_connection_mode: ConnectionMode,
    pub last_known_auth_options: AuthOptions,
    pub username: Option<String>,
}

/// Credentials for SMB authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmbCredentials {
    pub username: String,
    pub password: String,
}

/// Error types for Keychain operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum KeychainError {
    NotFound(String),
    AccessDenied(String),
    Other(String),
}

/// Result of a successful mount operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountResult {
    pub mount_path: String,
    pub already_mounted: bool,
}

/// Errors that can occur during mount operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MountError {
    HostUnreachable { message: String },
    ShareNotFound { message: String },
    AuthRequired { message: String },
    AuthFailed { message: String },
    PermissionDenied { message: String },
    Timeout { message: String },
    Cancelled { message: String },
    ProtocolError { message: String },
    MountPathConflict { message: String },
}

// ============================================================================
// Commands
// ============================================================================

/// Gets all currently discovered network hosts (stub: returns empty).
#[tauri::command]
pub fn list_network_hosts() -> Vec<NetworkHost> {
    vec![]
}

/// Gets the current discovery state (stub: always idle).
#[tauri::command]
pub fn get_network_discovery_state() -> DiscoveryState {
    DiscoveryState::Idle
}

/// Resolves a network host by ID (stub: returns None).
#[tauri::command]
pub async fn resolve_host(_host_id: String) -> Option<NetworkHost> {
    None
}

/// Lists shares available on a network host (stub: returns error).
#[tauri::command]
pub async fn list_shares_on_host(
    _host_id: String,
    hostname: String,
    _ip_address: Option<String>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::ProtocolError(format!(
        "Network browsing not supported on Linux (host: {})",
        hostname
    )))
}

/// Prefetches shares for a host (stub: no-op).
#[tauri::command]
pub async fn prefetch_shares(_host_id: String, _hostname: String, _ip_address: Option<String>, _port: u16) {
    // No-op
}

/// Gets auth mode detected for a host (stub: returns Unknown).
#[tauri::command]
pub fn get_host_auth_mode(_host_id: String) -> AuthMode {
    AuthMode::Unknown
}

/// Gets all known network shares (stub: returns empty).
#[tauri::command]
pub fn get_known_shares() -> Vec<KnownNetworkShare> {
    vec![]
}

/// Gets a specific known share by server and share name (stub: returns None).
#[tauri::command]
pub fn get_known_share_by_name(_server_name: String, _share_name: String) -> Option<KnownNetworkShare> {
    None
}

/// Updates or adds a known network share (stub: no-op).
#[tauri::command]
pub fn update_known_share(
    _app: tauri::AppHandle,
    _server_name: String,
    _share_name: String,
    _last_connection_mode: ConnectionMode,
    _last_known_auth_options: AuthOptions,
    _username: Option<String>,
) {
    // No-op
}

/// Gets username hints for servers (stub: returns empty).
#[tauri::command]
pub fn get_username_hints() -> HashMap<String, String> {
    HashMap::new()
}

/// Saves SMB credentials (stub: returns error).
#[tauri::command]
pub fn save_smb_credentials(
    _server: String,
    _share: Option<String>,
    _username: String,
    _password: String,
) -> Result<(), KeychainError> {
    Err(KeychainError::Other("Keychain not supported on Linux".to_string()))
}

/// Retrieves SMB credentials (stub: returns error).
#[tauri::command]
pub fn get_smb_credentials(_server: String, _share: Option<String>) -> Result<SmbCredentials, KeychainError> {
    Err(KeychainError::NotFound("Keychain not supported on Linux".to_string()))
}

/// Checks if credentials exist (stub: returns false).
#[tauri::command]
pub fn has_smb_credentials(_server: String, _share: Option<String>) -> bool {
    false
}

/// Deletes SMB credentials (stub: returns error).
#[tauri::command]
pub fn delete_smb_credentials(_server: String, _share: Option<String>) -> Result<(), KeychainError> {
    Err(KeychainError::NotFound("Keychain not supported on Linux".to_string()))
}

/// Lists shares with credentials (stub: returns error).
#[tauri::command]
pub async fn list_shares_with_credentials(
    _host_id: String,
    hostname: String,
    _ip_address: Option<String>,
    _port: u16,
    _username: Option<String>,
    _password: Option<String>,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::ProtocolError(format!(
        "Network browsing not supported on Linux (host: {})",
        hostname
    )))
}

/// Mounts an SMB share (stub: returns error).
#[tauri::command]
pub async fn mount_network_share(
    server: String,
    share: String,
    _username: Option<String>,
    _password: Option<String>,
) -> Result<MountResult, MountError> {
    Err(MountError::ProtocolError {
        message: format!("SMB mounting not supported on Linux ({}//{})", server, share),
    })
}

// ============================================================================
// Non-command functions (kept for API compatibility, not called on Linux)
// ============================================================================

/// Starts network discovery (stub: no-op).
#[allow(dead_code)]
pub fn start_discovery<R: tauri::Runtime>(_app: tauri::AppHandle<R>) {
    // No-op on Linux
}

/// Loads known shares from disk (stub: no-op).
#[allow(dead_code)]
pub fn load_known_shares<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) {
    // No-op on Linux
}

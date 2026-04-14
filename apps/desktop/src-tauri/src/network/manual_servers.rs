//! Manual server storage and injection.
//!
//! Handles user-added SMB servers: address parsing, TCP reachability checks,
//! persistence to `manual-servers.json`, and injection into the discovery state.

use crate::network::{HostSource, NetworkHost, on_host_found, on_host_lost};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Runtime};

const DEFAULT_SMB_PORT: u16 = 445;
const REACHABILITY_TIMEOUT_SECS: u64 = 5;
const MANUAL_SERVERS_FILENAME: &str = "manual-servers.json";

/// Protects the read-modify-write cycle on `manual-servers.json`.
/// Without this, concurrent `add_manual_server` / `remove_manual_server` calls
/// can read the same on-disk state and one write clobbers the other.
static STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn get_store_lock() -> &'static Mutex<()> {
    STORE_LOCK.get_or_init(|| Mutex::new(()))
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A parsed server address from user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAddress {
    pub host: String,
    pub port: u16,
    pub share_path: Option<String>,
}

/// Error from parsing a server address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnsupportedProtocol(String),
    Ipv6NotSupported,
    InvalidPort(String),
    Malformed(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Empty => write!(f, "Enter a server address"),
            ParseError::UnsupportedProtocol(proto) => {
                write!(f, "Only SMB shares are supported right now (got {}://)", proto)
            }
            ParseError::Ipv6NotSupported => {
                write!(
                    f,
                    "IPv6 addresses aren't supported yet. Use an IPv4 address or hostname."
                )
            }
            ParseError::InvalidPort(msg) => write!(f, "{}", msg),
            ParseError::Malformed(msg) => write!(f, "{}", msg),
        }
    }
}

/// A persisted manual server entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualServerEntry {
    pub id: String,
    pub display_name: String,
    pub address: String,
    pub port: u16,
    pub added_at: String,
}

/// The on-disk store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualServersStore {
    #[serde(default)]
    servers: Vec<ManualServerEntry>,
}

/// Result of successfully adding a manual server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualConnectResult {
    pub host: NetworkHost,
    pub share_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Address parsing
// ---------------------------------------------------------------------------

/// Parses user input into a structured address.
///
/// Accepts bare hostnames/IPs, host:port, and `smb://` URLs.
/// Rejects unsupported protocols and IPv6.
pub fn parse_server_address(input: &str) -> Result<ParsedAddress, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    // Check for unsupported protocols
    if let Some(proto) = extract_protocol(trimmed) {
        let proto_lower = proto.to_lowercase();
        if proto_lower != "smb" {
            return Err(ParseError::UnsupportedProtocol(proto_lower));
        }
        return parse_smb_url(trimmed);
    }

    // Check for IPv6 (contains colons that aren't a single host:port separator, or starts with [)
    if trimmed.starts_with('[') {
        return Err(ParseError::Ipv6NotSupported);
    }

    // Count colons to distinguish IPv6 from host:port
    let colon_count = trimmed.chars().filter(|c| *c == ':').count();
    if colon_count > 1 {
        return Err(ParseError::Ipv6NotSupported);
    }

    // Bare host or host:port
    if colon_count == 1 {
        let (host_part, port_str) = trimmed.split_once(':').expect("colon exists");
        let host = host_part.trim().to_string();
        if host.is_empty() {
            return Err(ParseError::Malformed(
                "Couldn't parse this address. Try a hostname, IP, or smb:// URL.".to_string(),
            ));
        }
        let port = parse_port(port_str.trim())?;
        validate_host(&host)?;
        Ok(ParsedAddress {
            host,
            port,
            share_path: None,
        })
    } else {
        let host = trimmed.to_string();
        validate_host(&host)?;
        Ok(ParsedAddress {
            host,
            port: DEFAULT_SMB_PORT,
            share_path: None,
        })
    }
}

/// Extracts the protocol prefix (before `://`) if present.
fn extract_protocol(input: &str) -> Option<String> {
    let lower = input.to_lowercase();
    if let Some(idx) = lower.find("://") {
        let proto = &input[..idx];
        // Only consider it a protocol if it's all alphabetic
        if proto.chars().all(|c| c.is_ascii_alphabetic()) {
            return Some(proto.to_string());
        }
    }
    None
}

/// Parses an `smb://` URL.
fn parse_smb_url(input: &str) -> Result<ParsedAddress, ParseError> {
    // Strip the scheme
    let after_scheme = &input[input.find("://").unwrap() + 3..];

    if after_scheme.is_empty() {
        return Err(ParseError::Malformed(
            "Couldn't parse this address. Try a hostname, IP, or smb:// URL.".to_string(),
        ));
    }

    // Strip user info (user@ or user:pass@)
    let after_userinfo = if let Some(at_idx) = after_scheme.find('@') {
        // Only treat @ as userinfo separator if it's before the first /
        let slash_idx = after_scheme.find('/').unwrap_or(after_scheme.len());
        if at_idx < slash_idx {
            &after_scheme[at_idx + 1..]
        } else {
            after_scheme
        }
    } else {
        after_scheme
    };

    // Split host:port from path
    let (host_port, path) = if let Some(slash_idx) = after_userinfo.find('/') {
        let path_part = &after_userinfo[slash_idx + 1..];
        let share_path = if path_part.is_empty() {
            None
        } else {
            Some(path_part.trim_end_matches('/').to_string())
        };
        (&after_userinfo[..slash_idx], share_path)
    } else {
        (after_userinfo, None)
    };

    // Parse host and optional port
    let (host, port) = if let Some(colon_idx) = host_port.rfind(':') {
        let host_part = &host_port[..colon_idx];
        let port_str = &host_port[colon_idx + 1..];
        if port_str.is_empty() {
            (host_part.to_string(), DEFAULT_SMB_PORT)
        } else {
            (host_part.to_string(), parse_port(port_str)?)
        }
    } else {
        (host_port.to_string(), DEFAULT_SMB_PORT)
    };

    if host.is_empty() {
        return Err(ParseError::Malformed(
            "Couldn't parse this address. Try a hostname, IP, or smb:// URL.".to_string(),
        ));
    }

    validate_host(&host)?;

    Ok(ParsedAddress {
        host,
        port,
        share_path: path,
    })
}

/// Validates a host string (rejects IPv6, empty, obviously invalid).
fn validate_host(host: &str) -> Result<(), ParseError> {
    if host.is_empty() {
        return Err(ParseError::Malformed(
            "Couldn't parse this address. Try a hostname, IP, or smb:// URL.".to_string(),
        ));
    }
    // If it looks like an IPv6 address
    if host.contains(':') || host.starts_with('[') {
        return Err(ParseError::Ipv6NotSupported);
    }
    // Basic character validation: alphanumeric, dots, dashes
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(ParseError::Malformed(
            "Couldn't parse this address. Try a hostname, IP, or smb:// URL.".to_string(),
        ));
    }
    Ok(())
}

/// Parses and validates a port string.
fn parse_port(s: &str) -> Result<u16, ParseError> {
    match s.parse::<u32>() {
        Ok(p) if (1..=65535).contains(&p) => Ok(p as u16),
        Ok(_) => Err(ParseError::InvalidPort("Port must be between 1 and 65535".to_string())),
        Err(_) => Err(ParseError::InvalidPort("Port must be between 1 and 65535".to_string())),
    }
}

// ---------------------------------------------------------------------------
// ID generation
// ---------------------------------------------------------------------------

/// Generates a deterministic ID for a manual server.
///
/// Format: `manual-{address}-{port}` with dots/colons replaced by dashes.
pub fn generate_server_id(address: &str, port: u16) -> String {
    let sanitized = address.replace(['.', ':'], "-");
    format!("manual-{}-{}", sanitized, port)
}

// ---------------------------------------------------------------------------
// Display name
// ---------------------------------------------------------------------------

/// Generates a display name for a manual server.
///
/// Bare address for default port, address:port for non-default.
fn display_name(address: &str, port: u16) -> String {
    if port == DEFAULT_SMB_PORT {
        address.to_string()
    } else {
        format!("{}:{}", address, port)
    }
}

// ---------------------------------------------------------------------------
// NetworkHost mapping
// ---------------------------------------------------------------------------

/// Whether the address looks like an IP address.
fn is_ip_address(host: &str) -> bool {
    host.parse::<IpAddr>().is_ok()
}

/// Creates a `NetworkHost` from parsed address info.
pub fn create_network_host(address: &str, port: u16) -> NetworkHost {
    let id = generate_server_id(address, port);
    let name = display_name(address, port);
    let is_ip = is_ip_address(address);

    NetworkHost {
        id,
        name,
        // hostname is always set so the share listing pipeline picks it up
        hostname: Some(address.to_string()),
        ip_address: if is_ip { Some(address.to_string()) } else { None },
        port,
        source: HostSource::Manual,
    }
}

// ---------------------------------------------------------------------------
// TCP reachability
// ---------------------------------------------------------------------------

/// Checks that the host:port is reachable via TCP with a timeout.
pub async fn check_reachability(host: &str, port: u16) -> Result<(), String> {
    use tokio::net::TcpStream;
    use tokio::time::{Duration, timeout};

    let addr = format!("{}:{}", host, port);
    debug!("Checking TCP reachability: {}", addr);

    // Try to resolve + connect. For hostnames, tokio::net::TcpStream::connect
    // does DNS resolution internally.
    match timeout(
        Duration::from_secs(REACHABILITY_TIMEOUT_SECS),
        TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_stream)) => {
            debug!("Reachable: {}", addr);
            Ok(())
        }
        Ok(Err(e)) => {
            debug!("Unreachable: {} — {}", addr, e);
            Err(format!("Couldn't reach {} — {}", addr, e))
        }
        Err(_) => {
            debug!("Timed out connecting to {}", addr);
            Err(format!(
                "Couldn't reach {} — connection timed out after {}s",
                addr, REACHABILITY_TIMEOUT_SECS
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Atomic file writes
// ---------------------------------------------------------------------------

/// Atomically writes content to a file using write-to-temp + rename.
/// On failure, the original file (if any) remains intact.
fn atomic_write_json(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Removes a stale `.tmp` file left over from a crash during atomic write.
fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Returns the path to the manual servers store file.
fn get_store_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .ok()
        .map(|dir| dir.join(MANUAL_SERVERS_FILENAME))
}

/// Reads the store from a path on disk.
fn read_store_from_path(path: &Path) -> ManualServersStore {
    cleanup_tmp_file(path);

    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => ManualServersStore::default(),
    }
}

/// Writes the store to a path on disk.
fn write_store_to_path(path: &Path, store: &ManualServersStore) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    match serde_json::to_string_pretty(store) {
        Ok(json) => {
            if let Err(e) = atomic_write_json(path, &json) {
                warn!("Couldn't write manual servers store: {}", e);
            }
        }
        Err(e) => warn!("Couldn't serialize manual servers store: {}", e),
    }
}

/// Loads the store from disk.
fn read_store<R: Runtime>(app: &AppHandle<R>) -> ManualServersStore {
    let Some(path) = get_store_path(app) else {
        return ManualServersStore::default();
    };
    read_store_from_path(&path)
}

/// Adds a server entry to the store file at the given path, protected by `STORE_LOCK`.
/// Extracted so it can be tested without an `AppHandle`.
fn add_server_entry_to_path(path: &Path, entry: ManualServerEntry) {
    let _guard = get_store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read_store_from_path(path);
    if let Some(existing) = store.servers.iter_mut().find(|s| s.id == entry.id) {
        *existing = entry;
    } else {
        store.servers.push(entry);
    }
    write_store_to_path(path, &store);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Adds a manual server: parses input, checks reachability, persists, and injects into discovery state.
pub async fn add_manual_server<R: Runtime>(
    input: &str,
    app_handle: &AppHandle<R>,
) -> Result<ManualConnectResult, String> {
    let parsed = parse_server_address(input).map_err(|e| e.to_string())?;

    // Check TCP reachability
    check_reachability(&parsed.host, parsed.port).await?;

    // Build the network host
    let host = create_network_host(&parsed.host, parsed.port);

    // Persist to disk
    if let Some(path) = get_store_path(app_handle) {
        let entry = ManualServerEntry {
            id: host.id.clone(),
            display_name: host.name.clone(),
            address: parsed.host.clone(),
            port: parsed.port,
            added_at: chrono::Utc::now().to_rfc3339(),
        };
        add_server_entry_to_path(&path, entry);
    }

    info!("Added manual server: {} (id={})", host.name, host.id);

    // Inject into discovery state
    on_host_found(host.clone(), app_handle);

    Ok(ManualConnectResult {
        host,
        share_path: parsed.share_path,
    })
}

/// Removes a server entry by ID from the store file at the given path, protected by `STORE_LOCK`.
/// Returns `true` if the server was found and removed, `false` if not found.
fn remove_server_entry_from_path(path: &Path, server_id: &str) -> bool {
    let _guard = get_store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read_store_from_path(path);
    let original_len = store.servers.len();
    store.servers.retain(|s| s.id != server_id);
    if store.servers.len() == original_len {
        return false;
    }
    write_store_to_path(path, &store);
    true
}

/// Removes a manual server by ID from storage and discovery state.
pub fn remove_manual_server<R: Runtime>(server_id: &str, app_handle: &AppHandle<R>) -> Result<(), String> {
    let Some(path) = get_store_path(app_handle) else {
        return Err(format!("Server '{}' not found", server_id));
    };

    if !remove_server_entry_from_path(&path, server_id) {
        return Err(format!("Server '{}' not found", server_id));
    }

    // Remove from discovery state and notify frontend
    on_host_lost(server_id, app_handle);

    info!("Removed manual server: {}", server_id);
    Ok(())
}

/// Loads persisted manual servers and injects them into discovery state.
///
/// Called at startup, before the frontend subscribes to events.
pub fn load_manual_servers<R: Runtime>(app_handle: &AppHandle<R>) {
    let store = read_store(app_handle);

    if store.servers.is_empty() {
        return;
    }

    info!("Loading {} persisted manual server(s)", store.servers.len());

    for entry in &store.servers {
        let host = create_network_host(&entry.address, entry.port);
        on_host_found(host, app_handle);
        debug!("Loaded manual server: {} (id={})", entry.display_name, entry.id);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_server_address: all input formats --

    #[test]
    fn parse_bare_ip() {
        let r = parse_server_address("192.168.1.100").unwrap();
        assert_eq!(r.host, "192.168.1.100");
        assert_eq!(r.port, 445);
        assert_eq!(r.share_path, None);
    }

    #[test]
    fn parse_ip_with_port() {
        let r = parse_server_address("192.168.1.100:9445").unwrap();
        assert_eq!(r.host, "192.168.1.100");
        assert_eq!(r.port, 9445);
        assert_eq!(r.share_path, None);
    }

    #[test]
    fn parse_bare_hostname() {
        let r = parse_server_address("mynas").unwrap();
        assert_eq!(r.host, "mynas");
        assert_eq!(r.port, 445);
        assert_eq!(r.share_path, None);
    }

    #[test]
    fn parse_hostname_with_underscore() {
        let r = parse_server_address("my_nas").unwrap();
        assert_eq!(r.host, "my_nas");
        assert_eq!(r.port, 445);
        assert_eq!(r.share_path, None);
    }

    #[test]
    fn parse_hostname_with_domain() {
        let r = parse_server_address("mynas.local").unwrap();
        assert_eq!(r.host, "mynas.local");
        assert_eq!(r.port, 445);
        assert_eq!(r.share_path, None);
    }

    #[test]
    fn parse_smb_url_basic() {
        let r = parse_server_address("smb://mynas").unwrap();
        assert_eq!(r.host, "mynas");
        assert_eq!(r.port, 445);
        assert_eq!(r.share_path, None);
    }

    #[test]
    fn parse_smb_url_with_port() {
        let r = parse_server_address("smb://mynas:9445").unwrap();
        assert_eq!(r.host, "mynas");
        assert_eq!(r.port, 9445);
        assert_eq!(r.share_path, None);
    }

    #[test]
    fn parse_smb_url_with_share() {
        let r = parse_server_address("smb://mynas/docs").unwrap();
        assert_eq!(r.host, "mynas");
        assert_eq!(r.port, 445);
        assert_eq!(r.share_path, Some("docs".to_string()));
    }

    #[test]
    fn parse_smb_url_with_user() {
        let r = parse_server_address("smb://user@mynas/docs").unwrap();
        assert_eq!(r.host, "mynas");
        assert_eq!(r.port, 445);
        assert_eq!(r.share_path, Some("docs".to_string()));
    }

    #[test]
    fn parse_smb_url_with_port_and_share() {
        let r = parse_server_address("smb://mynas:9445/docs").unwrap();
        assert_eq!(r.host, "mynas");
        assert_eq!(r.port, 9445);
        assert_eq!(r.share_path, Some("docs".to_string()));
    }

    #[test]
    fn parse_smb_url_trailing_slash() {
        let r = parse_server_address("smb://mynas/docs/").unwrap();
        assert_eq!(r.share_path, Some("docs".to_string()));
    }

    #[test]
    fn parse_with_whitespace() {
        let r = parse_server_address("  192.168.1.100  ").unwrap();
        assert_eq!(r.host, "192.168.1.100");
    }

    #[test]
    fn parse_uppercase_smb() {
        let r = parse_server_address("SMB://MyNas").unwrap();
        assert_eq!(r.host, "MyNas");
        assert_eq!(r.port, 445);
    }

    // -- parse_server_address: error cases --

    #[test]
    fn parse_empty() {
        assert_eq!(parse_server_address(""), Err(ParseError::Empty));
        assert_eq!(parse_server_address("  "), Err(ParseError::Empty));
    }

    #[test]
    fn parse_unsupported_protocols() {
        assert!(matches!(parse_server_address("afp://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "afp"));
        assert!(matches!(parse_server_address("nfs://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "nfs"));
        assert!(matches!(parse_server_address("ftp://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "ftp"));
        assert!(matches!(parse_server_address("vnc://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "vnc"));
    }

    #[test]
    fn parse_ipv6_rejected() {
        assert_eq!(parse_server_address("[::1]:9445"), Err(ParseError::Ipv6NotSupported));
        assert_eq!(parse_server_address("fe80::1"), Err(ParseError::Ipv6NotSupported));
        assert_eq!(parse_server_address("::1"), Err(ParseError::Ipv6NotSupported));
    }

    #[test]
    fn parse_port_out_of_range() {
        assert!(matches!(
            parse_server_address("mynas:0"),
            Err(ParseError::InvalidPort(_))
        ));
        assert!(matches!(
            parse_server_address("mynas:65536"),
            Err(ParseError::InvalidPort(_))
        ));
        assert!(matches!(
            parse_server_address("mynas:99999"),
            Err(ParseError::InvalidPort(_))
        ));
    }

    #[test]
    fn parse_port_not_a_number() {
        assert!(matches!(
            parse_server_address("mynas:abc"),
            Err(ParseError::InvalidPort(_))
        ));
    }

    #[test]
    fn parse_malformed_smb_url() {
        assert!(matches!(parse_server_address("smb://"), Err(ParseError::Malformed(_))));
    }

    #[test]
    fn parse_invalid_characters() {
        assert!(matches!(parse_server_address("my nas"), Err(ParseError::Malformed(_))));
        assert!(matches!(parse_server_address("my@nas"), Err(ParseError::Malformed(_))));
    }

    // -- ID generation --

    #[test]
    fn id_deterministic() {
        let id1 = generate_server_id("192.168.1.100", 9445);
        let id2 = generate_server_id("192.168.1.100", 9445);
        assert_eq!(id1, id2);
        assert_eq!(id1, "manual-192-168-1-100-9445");
    }

    #[test]
    fn id_different_ports() {
        let id1 = generate_server_id("mynas", 445);
        let id2 = generate_server_id("mynas", 9445);
        assert_ne!(id1, id2);
    }

    #[test]
    fn id_format_ip() {
        assert_eq!(generate_server_id("192.168.1.100", 445), "manual-192-168-1-100-445");
    }

    #[test]
    fn id_format_hostname() {
        assert_eq!(generate_server_id("mynas", 445), "manual-mynas-445");
    }

    #[test]
    fn id_format_hostname_with_local() {
        assert_eq!(generate_server_id("mynas.local", 445), "manual-mynas-local-445");
    }

    // -- Serialization round-trip --

    #[test]
    fn server_entry_serialization_round_trip() {
        let entry = ManualServerEntry {
            id: "manual-192-168-1-100-9445".to_string(),
            display_name: "192.168.1.100:9445".to_string(),
            address: "192.168.1.100".to_string(),
            port: 9445,
            added_at: "2026-04-02T10:00:00Z".to_string(),
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("\"displayName\""));
        assert!(json.contains("\"addedAt\""));

        let parsed: ManualServerEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, entry.id);
        assert_eq!(parsed.address, entry.address);
        assert_eq!(parsed.port, entry.port);
    }

    #[test]
    fn store_serialization_round_trip() {
        let store = ManualServersStore {
            servers: vec![ManualServerEntry {
                id: "manual-mynas-445".to_string(),
                display_name: "mynas".to_string(),
                address: "mynas".to_string(),
                port: 445,
                added_at: "2026-04-02T10:00:00Z".to_string(),
            }],
        };

        let json = serde_json::to_string_pretty(&store).unwrap();
        let parsed: ManualServersStore = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.servers[0].id, "manual-mynas-445");
    }

    #[test]
    fn store_deserialize_empty() {
        let store: ManualServersStore = serde_json::from_str("{}").unwrap();
        assert!(store.servers.is_empty());
    }

    // -- NetworkHost field mapping --

    #[test]
    fn host_mapping_bare_ip() {
        let host = create_network_host("192.168.1.100", 445);
        assert_eq!(host.name, "192.168.1.100");
        assert_eq!(host.hostname, Some("192.168.1.100".to_string()));
        assert_eq!(host.ip_address, Some("192.168.1.100".to_string()));
        assert_eq!(host.port, 445);
        assert_eq!(host.source, HostSource::Manual);
    }

    #[test]
    fn host_mapping_ip_with_port() {
        let host = create_network_host("192.168.1.100", 9445);
        assert_eq!(host.name, "192.168.1.100:9445");
        assert_eq!(host.hostname, Some("192.168.1.100".to_string()));
        assert_eq!(host.ip_address, Some("192.168.1.100".to_string()));
        assert_eq!(host.port, 9445);
    }

    #[test]
    fn host_mapping_hostname() {
        let host = create_network_host("mynas", 445);
        assert_eq!(host.name, "mynas");
        assert_eq!(host.hostname, Some("mynas".to_string()));
        assert_eq!(host.ip_address, None);
        assert_eq!(host.port, 445);
    }

    #[test]
    fn host_mapping_hostname_with_local() {
        let host = create_network_host("mynas.local", 445);
        assert_eq!(host.name, "mynas.local");
        assert_eq!(host.hostname, Some("mynas.local".to_string()));
        assert_eq!(host.ip_address, None);
        assert_eq!(host.port, 445);
    }

    // -- Display name --

    #[test]
    fn display_name_default_port() {
        assert_eq!(display_name("192.168.1.100", 445), "192.168.1.100");
        assert_eq!(display_name("mynas", 445), "mynas");
    }

    #[test]
    fn display_name_custom_port() {
        assert_eq!(display_name("192.168.1.100", 9445), "192.168.1.100:9445");
        assert_eq!(display_name("mynas", 9445), "mynas:9445");
    }

    // -- ManualConnectResult serialization --

    #[test]
    fn connect_result_serialization() {
        let result = ManualConnectResult {
            host: create_network_host("192.168.1.100", 9445),
            share_path: Some("docs".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"sharePath\""));
        assert!(json.contains("\"docs\""));

        let parsed: ManualConnectResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.share_path, Some("docs".to_string()));
        assert_eq!(parsed.host.id, "manual-192-168-1-100-9445");
    }

    // -- Concurrency tests for file-backed persistence --

    /// Helper: creates a `ManualServerEntry` with a unique address.
    fn test_entry(index: usize) -> ManualServerEntry {
        let address = format!("10.0.0.{}", index);
        ManualServerEntry {
            id: generate_server_id(&address, 445),
            display_name: address.clone(),
            address,
            port: 445,
            added_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    /// Concurrent `add_server_entry_to_path` calls must not lose any writes.
    /// Before the `STORE_LOCK` fix, this would fail because two threads could
    /// read the same on-disk state and one write would clobber the other.
    #[test]
    fn concurrent_add_server_no_lost_writes() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(MANUAL_SERVERS_FILENAME);

        let thread_count = 20;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
        let mut handles = Vec::new();

        for i in 0..thread_count {
            let barrier = barrier.clone();
            let path = path.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                add_server_entry_to_path(&path, test_entry(i));
            }));
        }

        for h in handles {
            h.join().expect("thread panicked");
        }

        let store = read_store_from_path(&path);
        assert_eq!(
            store.servers.len(),
            thread_count,
            "Expected {} servers but got {} — a concurrent write was lost",
            thread_count,
            store.servers.len()
        );
    }

    /// Concurrent adds and removes must not corrupt the store.
    #[test]
    fn concurrent_add_and_remove() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(MANUAL_SERVERS_FILENAME);

        // Pre-populate with servers 0..10 that will be removed
        for i in 0..10 {
            add_server_entry_to_path(&path, test_entry(i));
        }
        assert_eq!(read_store_from_path(&path).servers.len(), 10);

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(20));
        let mut handles = Vec::new();

        // 10 threads remove servers 0..10
        for i in 0..10 {
            let barrier = barrier.clone();
            let path = path.clone();
            let id = test_entry(i).id;
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                remove_server_entry_from_path(&path, &id);
            }));
        }

        // 10 threads add servers 100..110
        for i in 100..110 {
            let barrier = barrier.clone();
            let path = path.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                add_server_entry_to_path(&path, test_entry(i));
            }));
        }

        for h in handles {
            h.join().expect("thread panicked");
        }

        let store = read_store_from_path(&path);
        // All old servers removed, all new servers added
        assert_eq!(
            store.servers.len(),
            10,
            "Expected 10 servers (old removed, new added) but got {}",
            store.servers.len()
        );
        // Verify none of the old servers remain
        for i in 0..10 {
            assert!(
                !store.servers.iter().any(|s| s.id == test_entry(i).id),
                "Server {} should have been removed",
                i
            );
        }
        // Verify all new servers are present
        for i in 100..110 {
            assert!(
                store.servers.iter().any(|s| s.id == test_entry(i).id),
                "Server {} should have been added",
                i
            );
        }
    }

    /// Rapid sequential adds of distinct servers should all be persisted.
    #[test]
    fn rapid_sequential_adds() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(MANUAL_SERVERS_FILENAME);

        let count = 50;
        for i in 0..count {
            add_server_entry_to_path(&path, test_entry(i));
        }

        let store = read_store_from_path(&path);
        assert_eq!(store.servers.len(), count);
    }

    /// Upserts to the same server entry should not create duplicates.
    #[test]
    fn concurrent_upserts_same_server() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(MANUAL_SERVERS_FILENAME);

        let thread_count = 20;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
        let mut handles = Vec::new();

        for _ in 0..thread_count {
            let barrier = barrier.clone();
            let path = path.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                // All threads upsert the same server (same ID)
                add_server_entry_to_path(&path, test_entry(42));
            }));
        }

        for h in handles {
            h.join().expect("thread panicked");
        }

        let store = read_store_from_path(&path);
        assert_eq!(
            store.servers.len(),
            1,
            "Concurrent upserts to the same server created {} duplicates",
            store.servers.len() - 1
        );
    }
}

// ---------------------------------------------------------------------------
// Integration tests (require Docker SMB containers)
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "smb-e2e"))]
mod integration_tests {
    use super::*;

    /// Verifies TCP reachability against a Docker SMB container.
    ///
    /// Requires: `./test/smb-servers/start.sh minimal`
    #[tokio::test]
    async fn reachability_docker_smb_guest() {
        let port = smb2::testing::guest_port();
        let result = check_reachability("localhost", port).await;
        assert!(
            result.is_ok(),
            "Docker SMB container should be reachable on port {port}. Start it with: ./test/smb-servers/start.sh minimal"
        );
    }

    /// Verifies that an unreachable port returns an error.
    #[tokio::test]
    async fn reachability_unreachable_port() {
        let result = check_reachability("localhost", 19999).await;
        assert!(result.is_err(), "Nothing should be listening on port 19999");
    }

    /// Exercises the full manual server pipeline: parse → create host → generate ID.
    #[test]
    fn manual_server_pipeline() {
        let parsed = parse_server_address("localhost:9445").unwrap();
        assert_eq!(parsed.host, "localhost");
        assert_eq!(parsed.port, 9445);

        let host = create_network_host(&parsed.host, parsed.port);
        assert_eq!(host.source, HostSource::Manual);
        assert_eq!(host.id, "manual-localhost-9445");
        assert_eq!(host.name, "localhost:9445");
        assert_eq!(host.hostname, Some("localhost".to_string()));
        assert_eq!(host.ip_address, None);
        assert_eq!(host.port, 9445);

        // ID is deterministic — same inputs produce same ID
        let id = generate_server_id(&parsed.host, parsed.port);
        assert_eq!(id, host.id);
    }
}

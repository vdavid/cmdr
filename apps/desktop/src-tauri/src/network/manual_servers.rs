//! Manual server storage and injection.
//!
//! Handles user-added SMB servers: address parsing, TCP reachability checks
//! (`manual_servers_reachability.rs`), persistence to `manual-servers.json`, and
//! injection into the discovery state.

use crate::network::server_identity::SmbServer;
use crate::network::{HostSource, NetworkHost, on_host_found, on_host_lost};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Runtime};

const DEFAULT_SMB_PORT: u16 = 445;
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
    /// The account an `smb://user@host` address names. ❗ Never its password: a
    /// `user:password@` spelling keeps only the user.
    pub username: Option<String>,
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
    /// A name a person typed, or empty for a server nobody named. ❗ Read it
    /// through [`label`](Self::label) and [`is_named`](Self::is_named), ❌ never
    /// raw: entries written before names existed hold the derived address here.
    #[serde(default)]
    pub display_name: String,
    pub address: String,
    pub port: u16,
    pub added_at: String,
    /// The account the person typed for this host, in the add or edit sheet or
    /// as `smb://user@host`. ❗ A preference, ❌ not identity (the address is):
    /// it prefills the first sign-in and keeps the share listing from answering
    /// as guest (`typed_username`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// What an add or an edit sets on a host besides its address.
#[derive(Debug, Clone, Default)]
pub struct HostEdit {
    /// The Name field; empty leaves the host unnamed.
    pub name: String,
    /// The account to sign in as, or `None` for no preference.
    pub username: Option<String>,
}

impl ManualServerEntry {
    /// Whether a person named this server.
    ///
    /// ❗ A stored name that spells the entry's own derived label (`host`, or
    /// `host:port`) is NOT a name: that is what every entry held before the add
    /// form had a name field, and reading it as chosen would let the address
    /// outrank the Bonjour name a person recognizes. Rewriting the file isn't
    /// needed, because this reading and [`label`](Self::label) agree on it.
    pub fn is_named(&self) -> bool {
        let name = self.display_name.trim();
        !name.is_empty() && name != discovery_name(&self.address, self.port)
    }

    /// What the UI calls this server: the name a person typed, else the
    /// address (with the port when it isn't 445).
    pub fn label(&self) -> String {
        if self.is_named() {
            self.display_name.trim().to_string()
        } else {
            discovery_name(&self.address, self.port)
        }
    }
}

/// The on-disk store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualServersStore {
    #[serde(default)]
    servers: Vec<ManualServerEntry>,
}

/// Why adding a manual server didn't go through.
///
/// ❗ Two answers, because only one of them can be added anyway: a server that
/// didn't answer may be asleep or off-network right now, while an address that
/// doesn't parse is a typo, and saving it on purpose helps nobody.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum AddServerError {
    /// The address isn't one this reads (`ParseError`), with why, for the log.
    InvalidAddress { message: String },
    /// Nothing answered on the address's SMB port within the probe's budget.
    Unreachable {
        message: String,
        /// Something besides the server worth checking, when the way the probe
        /// failed points at one.
        hint: Option<UnreachableHint>,
    },
}

impl std::fmt::Display for AddServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress { message } | Self::Unreachable { message, .. } => f.write_str(message),
        }
    }
}

/// Whether an add probes the server before saving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachability {
    /// TCP-connect to the SMB port first; a server that doesn't answer isn't saved.
    Check,
    /// Save without asking: the person pressed "Add anyway" after the check failed.
    Skip,
}

/// Result of successfully adding a manual server.
///
/// Only serialized (Rust → frontend); no `Deserialize` needed.
/// `NetworkHost` has no `Deserialize` either (prevents specta type-split issues).
#[derive(Debug, Clone, Serialize, specta::Type)]
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
            username: None,
        })
    } else {
        let host = trimmed.to_string();
        validate_host(&host)?;
        Ok(ParsedAddress {
            host,
            port: DEFAULT_SMB_PORT,
            share_path: None,
            username: None,
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
    // Strip the scheme. The caller only routes here after `extract_protocol`
    // matched a `smb://` prefix, so `://` is present; fall back to a Malformed
    // error rather than panicking if that ever stops holding.
    let scheme_end = input.find("://").ok_or_else(|| {
        ParseError::Malformed("Couldn't parse this address. Try a hostname, IP, or smb:// URL.".to_string())
    })?;
    let after_scheme = &input[scheme_end + 3..];

    if after_scheme.is_empty() {
        return Err(ParseError::Malformed(
            "Couldn't parse this address. Try a hostname, IP, or smb:// URL.".to_string(),
        ));
    }

    // Split off user info (user@ or user:pass@), keeping only the user
    let slash_idx = after_scheme.find('/').unwrap_or(after_scheme.len());
    let (username, after_userinfo) = match after_scheme.find('@') {
        // Only treat @ as userinfo separator if it's before the first /
        Some(at_idx) if at_idx < slash_idx => {
            let user = after_scheme[..at_idx].split(':').next().unwrap_or_default().trim();
            (
                (!user.is_empty()).then(|| user.to_string()),
                &after_scheme[at_idx + 1..],
            )
        }
        _ => (None, after_scheme),
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
        username,
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

/// The name a manual server goes by in the discovery list, and so the name its
/// share history is filed under: the bare address on 445, `address:port` off it.
/// Also how the servers listing spells an SMB host's address.
pub fn discovery_name(address: &str, port: u16) -> String {
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
    let name = discovery_name(address, port);
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
// Atomic file writes
// ---------------------------------------------------------------------------

/// Durably writes content to a file using write-to-temp + fsync + rename + parent-dir fsync.
/// On failure, the original file (if any) remains intact. The fsyncs make the write survive a
/// power loss, not just process death: `manual-servers.json` holds user-entered SMB servers that
/// aren't rediscoverable via mDNS, so a torn / zero-length write here loses real config. See
/// `crate::config::durable_write_json` for the durability rationale.
fn atomic_write_json(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    crate::config::durable_write_json(path, &tmp, content)
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

/// Every manually-typed SMB server, as the store holds it.
///
/// ❗ Reads the FILE. There is no in-memory mirror here (the discovery host map
/// is where a loaded entry ends up, and that map is about what's reachable
/// rather than what's saved), so a caller on a hot path wants to ask once.
pub fn all<R: Runtime>(app: &AppHandle<R>) -> Vec<ManualServerEntry> {
    read_store(app).servers
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
///
/// ❗ Re-adding a host keeps the name and account it had when the new add brings none: adding
/// an address someone saved earlier is an ordinary move, and it must not unname
/// their server behind their back. Renaming is [`name_server_entry_at_path`]'s.
fn add_server_entry_to_path(path: &Path, mut entry: ManualServerEntry) {
    let _guard = get_store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read_store_from_path(path);
    if let Some(existing) = store.servers.iter_mut().find(|s| s.id == entry.id) {
        if !entry.is_named() && existing.is_named() {
            entry.display_name = existing.display_name.clone();
        }
        if entry.username.is_none() {
            entry.username = existing.username.clone();
        }
        *existing = entry;
    } else {
        store.servers.push(entry);
    }
    write_store_to_path(path, &store);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Adds a manual server: parses input, checks reachability, persists, and injects into discovery
/// state. `details` is what the person typed beside the address: a name (empty
/// leaves it unnamed) and an account, which falls back to the one an
/// `smb://user@host` address names.
pub async fn add_manual_server<R: Runtime>(
    input: &str,
    details: &HostEdit,
    reachability: Reachability,
    app_handle: &AppHandle<R>,
) -> Result<ManualConnectResult, AddServerError> {
    let parsed = checked_parse(input, reachability).await?;

    // Build the network host
    let host = create_network_host(&parsed.host, parsed.port);

    let username = typed_account(details.username.as_deref()).or_else(|| parsed.username.clone());

    // Persist to disk
    if let Some(path) = get_store_path(app_handle) {
        let entry = ManualServerEntry {
            id: host.id.clone(),
            display_name: details.name.trim().to_string(),
            address: parsed.host.clone(),
            port: parsed.port,
            added_at: chrono::Utc::now().to_rfc3339(),
            username: username.clone(),
        };
        add_server_entry_to_path(&path, entry);
    }

    // A share the address named (`smb://sven@host/Container`) is the place the
    // person meant to save, so it becomes a row under the host now, before any
    // mount: `docs/specs/saved-smb-shares.md`.
    if let Some(share) = parsed.share_path.as_deref().and_then(|path| path.split('/').next()) {
        crate::network::smb_saved_shares::remember_named_share(&host.name, share, username.as_deref());
    }

    // The address, never `host.name` (a `host:port` display form) or the typed display name:
    // `Loaded manual server` logs the same address, so one server reads as one token.
    info!(
        "Added manual server: host={:?}, port={}, serverId={:?}",
        parsed.host, parsed.port, host.id
    );
    // The saved lists changed (the host's account, maybe its name), even where no
    // volume did: the hub re-reads them on `volumes-changed`.
    crate::volume_broadcast::emit_volumes_changed();

    // Inject into discovery state
    on_host_found(host.clone(), app_handle);

    Ok(ManualConnectResult {
        host,
        share_path: parsed.share_path,
    })
}

/// Names the host `server_id` names and sets its account, protected by
/// `STORE_LOCK`, and answers the entry as stored. An empty name unnames it; no
/// account clears the preference.
///
/// A host the share history knows but nobody typed in has no entry yet, and
/// naming it SAVES it: the manual store is the one place a name can live, and a
/// NAS someone only ever opened from the discovery list is still theirs to name.
/// `address` and `port` are what that new entry dials; `None` when they don't mint
/// `server_id`, since such a pair would disagree about which host it is.
///
/// ❗ The name and account only, on an entry that exists: the address and port are its
/// identity (they mint the id and the host the discovery list carries), so an
/// edit that wants another address is a Forget and an Add.
fn name_server_entry_at_path(
    path: &Path,
    server_id: &str,
    address: &str,
    port: u16,
    edit: &HostEdit,
) -> Option<ManualServerEntry> {
    let _guard = get_store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read_store_from_path(path);
    let entry = if let Some(existing) = store.servers.iter_mut().find(|s| s.id == server_id) {
        existing.display_name = edit.name.trim().to_string();
        existing.username = typed_account(edit.username.as_deref());
        existing.clone()
    } else {
        if generate_server_id(address, port) != server_id {
            return None;
        }
        let entry = ManualServerEntry {
            id: server_id.to_string(),
            display_name: edit.name.trim().to_string(),
            address: address.to_string(),
            port,
            added_at: chrono::Utc::now().to_rfc3339(),
            username: typed_account(edit.username.as_deref()),
        };
        store.servers.push(entry.clone());
        entry
    };
    write_store_to_path(path, &store);
    Some(entry)
}

/// A typed account, trimmed, or `None` when nothing was typed.
fn typed_account(username: Option<&str>) -> Option<String> {
    username.map(str::trim).filter(|u| !u.is_empty()).map(str::to_string)
}

/// The account typed for `server`, among `entries`.
///
/// ❗ By [`SmbServer::is`]: the same port, and the same machine under any name it
/// goes by (so the Bonjour name finds a host typed as an IP once discovery paired the
/// two). An account typed for `localhost:11482` once reached the guest-only
/// `localhost:11480` and made it demand a password.
fn typed_username_in(entries: &[ManualServerEntry], server: &SmbServer, hosts: &[NetworkHost]) -> Option<String> {
    entries
        .iter()
        .filter(|entry| entry.username.is_some())
        .find(|entry| SmbServer::new(&entry.address, entry.port).is(server, hosts))
        .and_then(|entry| entry.username.clone())
}

/// The account the person typed for the host `server_name` names, if any.
///
/// ❗ Two jobs, one answer: it is the first sign-in's prefill (`get_username_hint`),
/// and a host that has one is never listed as guest (`smb_client::GuestAttempt`).
/// Reads the store file, like [`all`].
pub fn typed_username<R: Runtime>(app: &AppHandle<R>, server: &SmbServer) -> Option<String> {
    let entries = all(app);
    if entries.iter().all(|entry| entry.username.is_none()) {
        return None;
    }
    typed_username_in(&entries, server, &crate::network::fresh_discovered_hosts())
}

/// Names a saved SMB host, saving it first when only the share history knew it.
/// See [`name_server_entry_at_path`]. Answers whether there was a host to name.
///
/// A host saved this way joins the discovery list the way a typed one does, so
/// it stays reachable when mDNS goes quiet.
pub fn name_manual_server<R: Runtime>(
    server_id: &str,
    address: &str,
    port: u16,
    edit: &HostEdit,
    app_handle: &AppHandle<R>,
) -> bool {
    let Some(path) = get_store_path(app_handle) else {
        return false;
    };
    let was_saved = read_store_from_path(&path).servers.iter().any(|s| s.id == server_id);
    let Some(entry) = name_server_entry_at_path(&path, server_id, address, port, edit) else {
        return false;
    };
    if !was_saved {
        on_host_found(create_network_host(&entry.address, entry.port), app_handle);
    }
    true
}

/// Reads `input` and, unless the add skips it, probes the server's SMB port.
async fn checked_parse(input: &str, reachability: Reachability) -> Result<ParsedAddress, AddServerError> {
    let parsed = parse_server_address(input).map_err(|e| AddServerError::InvalidAddress { message: e.to_string() })?;
    if reachability == Reachability::Check {
        check_reachability(&parsed.host, parsed.port).await?;
    }
    Ok(parsed)
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

    info!("Removed manual server: serverId={server_id:?}");
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
        debug!(
            "Loaded manual server: host={:?}, port={}, serverId={:?}",
            entry.address, entry.port, entry.id
        );
    }
}

#[path = "manual_servers_account.rs"]
mod account;
pub use account::set_account;

#[path = "manual_servers_reachability.rs"]
mod reachability;
#[cfg(test)]
use reachability::unreachable_hint;
pub use reachability::{UnreachableHint, check_reachability};

#[cfg(test)]
#[path = "manual_servers_test.rs"]
mod tests;

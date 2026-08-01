//! SMB upgrade helpers: establish direct smb2 connections for OS-mounted SMB volumes.
//!
//! Shared across three upgrade paths:
//! 1. **Startup** (`file_system::upgrade_existing_smb_mounts`): scans existing mounts
//! 2. **Mount-time** (`volumes::watcher::try_upgrade_smb_mount`): FSEvents detects new mount
//! 3. **Manual** (`commands::network::upgrade_to_smb_volume`): user clicks "Connect directly"

use crate::network::get_discovered_hosts;

/// Derives the SMB volume ID from `statfs(mount_path)` (macOS) or
/// `/proc/mounts` (Linux). Returns `None` if the path isn't an SMB mount.
///
/// Used so the mount-time `register_smb_volume` derives the same canonical ID
/// as the OS-event watcher (which only has the mount path to work with). The
/// caller passed `server` may be an mDNS service name or display string that
/// statfs would normalize to an IP, so deriving from statfs is what makes the
/// two sites agree.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn volume_id_from_statfs(mount_path: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    let info = crate::volumes::get_smb_mount_info(mount_path)?;
    #[cfg(target_os = "linux")]
    let info = crate::volumes_linux::get_smb_mount_info(mount_path)?;
    Some(crate::file_system::volume::smb_volume_id(
        &info.server,
        info.port,
        &info.share,
    ))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn volume_id_from_statfs(_mount_path: &str) -> Option<String> {
    None
}

/// Why a direct connection couldn't be established, as a typed reason rather
/// than a sentence.
///
/// Word-free by design: the frontend renders the copy from the message catalog
/// (`$lib/errors/` convention — classification in Rust, words on the frontend).
/// A raw `No route to host (os error 65)` has no business reaching a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeFailure {
    /// Nothing answered on the SMB port: the server is off, asleep, or not on
    /// this network right now.
    Unreachable,
    /// It answered, but the handshake ran out of time.
    TooSlow,
    /// It answered and then something we can't act on went wrong.
    Unexpected,
}

impl UpgradeFailure {
    /// Classifies a connect failure by io kind and smb2 error kind, never by
    /// message text (`no-string-matching`).
    pub(crate) fn from_smb_error(err: &smb2::Error) -> Self {
        use std::io::ErrorKind as Io;
        if let smb2::Error::Io(io_err) = err {
            return match io_err.kind() {
                Io::HostUnreachable | Io::NetworkUnreachable | Io::ConnectionRefused | Io::NotConnected => {
                    Self::Unreachable
                }
                Io::TimedOut => Self::TooSlow,
                _ => Self::Unexpected,
            };
        }
        match err.kind() {
            smb2::ErrorKind::TimedOut => Self::TooSlow,
            smb2::ErrorKind::ConnectionLost => Self::Unreachable,
            _ => Self::Unexpected,
        }
    }
}

/// Result of an SMB volume upgrade attempt.
#[derive(serde::Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpgradeResult {
    /// Upgrade succeeded: volume now uses direct smb2.
    Success,
    /// Credentials needed: frontend should show login form.
    CredentialsNeeded {
        server: String,
        share: String,
        port: u16,
        /// Friendly display name for the server (mDNS hostname or IP).
        display_name: String,
        /// Username hint from stored credentials or the OS mount.
        username_hint: Option<String>,
        /// Optional message explaining why credentials are needed.
        message: Option<String>,
    },
    /// Couldn't reach the server (DNS, network, unreachable, too slow).
    NetworkError {
        reason: UpgradeFailure,
        /// Friendly server name for the frontend to name in its copy.
        display_name: String,
    },
}

/// Internal error type for upgrade attempts, distinguishing auth from network failures.
pub(crate) enum UpgradeError {
    Auth,
    Network {
        reason: UpgradeFailure,
        display_name: String,
    },
}

/// Delays between direct-connect attempts.
///
/// The first connect to a private LAN address shortly after launch routinely
/// comes back `EHOSTUNREACH` while the route and the macOS Local Network
/// permission settle, and the identical attempt moments later succeeds (three
/// times in one session on 2026-08-01, each followed by a clean connect).
/// Deliberately short: someone is watching a "Connecting directly…" toast.
const CONNECT_RETRY_BACKOFF: [std::time::Duration; 2] = [
    std::time::Duration::from_millis(300),
    std::time::Duration::from_millis(1200),
];

/// How long the attempts themselves may have taken before we stop retrying.
///
/// This is what keeps a genuinely-down server failing promptly. An `EHOSTUNREACH`
/// comes back instantly, so a real blip gets its retries; an attempt that ate the
/// 10 s connect timeout already answered the question, and stacking another would
/// triple a user's wait for nothing.
const CONNECT_RETRY_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Runs `connect` and retries a failure that never reached the server.
///
/// Retries only when the attempts have been cheap AND the failure is one a
/// moment's wait can fix. An auth rejection is final (retrying risks locking the
/// account; the "Sign in" flow owns that recovery), and so is anything the
/// server itself answered with.
async fn connect_with_retry<T, F, Fut>(mut connect: F) -> Result<T, smb2::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, smb2::Error>>,
{
    let started = std::time::Instant::now();
    for delay in CONNECT_RETRY_BACKOFF {
        match connect().await {
            Ok(value) => return Ok(value),
            Err(e) => {
                if UpgradeFailure::from_smb_error(&e) != UpgradeFailure::Unreachable
                    || started.elapsed() >= CONNECT_RETRY_BUDGET
                {
                    return Err(e);
                }
                log::debug!("Direct connect didn't reach the server ({e}); retrying in {delay:?}");
                tokio::time::sleep(delay).await;
            }
        }
    }
    connect().await
}

/// Whether `volume_id` already resolves to a HEALTHY direct smb2 volume, in
/// which case an upgrade has nothing to do.
///
/// Every upgrade path checks this immediately before connecting, not just at
/// entry: all three paths wait up to 1.5 s for mDNS first, and the startup pass
/// waits up to 15 s, so another path can finish the job during the wait. Without
/// the re-check we paid a TCP connect, a negotiate, and a session setup to
/// replace a perfectly good volume.
///
/// `Disconnected` deliberately does NOT count. That's the manual "Connect
/// directly" recovery path after a share dropped; short-circuiting it would
/// dead-end the user on a broken volume.
pub(crate) fn is_already_direct(volume_id: &str) -> bool {
    use crate::file_system::volume::SmbConnectionState;
    matches!(
        crate::file_system::get_volume_manager()
            .get(volume_id)
            .and_then(|v| v.smb_connection_state()),
        Some(SmbConnectionState::Direct)
    )
}

/// Whether an existing-mount upgrade pass is in flight.
static UPGRADE_PASS_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A single in-flight run of `file_system::upgrade_existing_smb_mounts`.
///
/// `ensure_network_discovery_started` fires on EVERY user networking action
/// (opening Network, "Connect to server…", clicking "Connect directly"), and
/// each pass waits up to 15 s for mDNS before it does anything. Two clicks nine
/// seconds apart stacked two passes that both fired blind. Holding this guard
/// for the lifetime of the pass means extra triggers are dropped instead of
/// queued; the running pass re-scans after its wait, so it still picks up
/// anything that mounted in the meantime.
pub(crate) struct UpgradePass;

impl UpgradePass {
    /// `Some` if no pass is in flight, `None` if one already is.
    pub(crate) fn begin() -> Option<Self> {
        use std::sync::atomic::Ordering;
        UPGRADE_PASS_PENDING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for UpgradePass {
    fn drop(&mut self) {
        UPGRADE_PASS_PENDING.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Register `new_volume` under `volume_id`, retiring any predecessor first.
///
/// **The predecessor is superseded, never unmounted.** A re-register (a manual
/// "Connect directly", an `NSWorkspaceDidMountNotification`, a redundant
/// upgrade pass) means a newer instance owns the id, NOT that the device went
/// away. Anything that grabbed an `Arc` before the swap is still using the old
/// instance: a running transfer holds `src_vol` / `dst_vol` clones for its whole
/// duration (`write_operations::transfer::volume_copy`), a viewer holds a read
/// stream, the indexer holds a scan session. `on_unmount` here dropped the smb2
/// session under all of them and killed a live copy with `DeviceDisconnected` on
/// a healthy connection. `Volume::on_superseded` retires the id-scoped parts
/// (watcher, scan pool, state events) and leaves the session to be released when
/// the last `Arc` drops.
///
/// `SmbVolume::on_superseded` is lock-free, but the trait's DEFAULT delegates to
/// `on_unmount`, which uses `blocking_write()` / `blocking_lock()` (designed for
/// the sync FSEvents thread) and panics inside a tokio runtime. `spawn_blocking`
/// keeps that default legal for any backend. Awaited so the retirement completes
/// before `register` swaps the new volume in.
async fn register_replacing_predecessor(
    volume_id: &str,
    new_volume: std::sync::Arc<dyn crate::file_system::volume::Volume>,
) {
    let manager = crate::file_system::get_volume_manager();
    if let Some(prev) = manager.get(volume_id) {
        log::debug!("Replacing existing volume at id={volume_id}; retiring the predecessor (session stays up)");
        let _ = tokio::task::spawn_blocking(move || prev.on_superseded()).await;
    }
    manager.register(volume_id, new_volume);

    // Tell the frontend the volume's connection state changed (os_mount → direct).
    // The auto-upgrade paths often coincide with an FSEvents mount event that triggers
    // a broadcast anyway, but the after-sign-in and already-mounted paths have no
    // mount event at all: without this, the picker keeps the stale os_mount dot.
    crate::volume_broadcast::emit_volumes_changed();
}

/// Tries to establish a direct smb2 connection and register as `SmbVolume`.
///
/// Best-effort: logs a warning and returns quietly on failure. The FSEvents
/// watcher will register a `LocalPosixVolume` as fallback.
pub(crate) async fn register_smb_volume(
    server: &str,
    share: &str,
    mount_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) {
    use crate::file_system::volume::smb::connect_smb_volume;
    use std::sync::Arc;

    // Resolve mDNS service names (like "Naspolya._smb._tcp.local") to an IP
    let resolved_server = resolve_server_address(server);

    // Derive the volume ID before connect so SmbVolume's internal ID, the
    // ID we pass to `connect_smb_volume`, and the ID the OS-event watcher
    // computes via `volume_id_for_mount` all agree. Statfs is the canonical
    // source — `server` as passed in may be an mDNS service name or display
    // string that wouldn't match what the watcher later sees.
    let volume_id = volume_id_from_statfs(mount_path)
        .unwrap_or_else(|| crate::file_system::volume::smb_volume_id(server, port, share));

    // Another path (a manual "Connect directly", the mount-time upgrade, an
    // earlier pass) may have finished the job while we waited on mDNS. Replacing
    // a healthy direct volume costs a whole session setup and hands every
    // in-flight holder to a superseded instance for no reason.
    if is_already_direct(&volume_id) {
        log::debug!("{volume_id} is already a direct smb2 connection; skipping the upgrade");
        return;
    }

    log::debug!(
        "Establishing smb2 connection for SmbVolume: {}:{}/{}",
        resolved_server,
        port,
        share
    );

    let params =
        crate::file_system::volume::smb::SmbConnectionParams::new(&resolved_server, share, port, username, password);
    match connect_with_retry(|| connect_smb_volume(share, mount_path, &volume_id, params.clone())).await {
        Ok(volume) => {
            // Overwrite-with-retire so SmbVolume always wins over any
            // LocalPosixVolume the watcher may have registered in the race
            // window, and any prior SmbVolume is retired (not torn down) before
            // we replace it.
            register_replacing_predecessor(&volume_id, Arc::new(volume)).await;
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
            // The session is installed and Direct. If the user had indexing
            // enabled for this volume (a persisted index DB with a completed
            // scan), resume it — the backend-autonomous recovery that keeps a NAS
            // index from silently going dark after a disconnect/restart. No-op for
            // a never-enabled share.
            crate::index_host::index().resume_after_reconnect(volume_id.clone());
        }
        Err(e) => {
            // Log-only path: nothing here reaches a person, so the raw error is
            // exactly what we want (it's the diagnostic). The volume stays on the
            // OS mount, which still works, just slower.
            log::warn!(
                "Couldn't establish an smb2 connection for {}/{} ({:?}): {}. Staying on the OS mount.",
                server,
                share,
                UpgradeFailure::from_smb_error(&e),
                e
            );
        }
    }
}

/// Resolve the mount's hostname (with mDNS wait), look up stored Keychain
/// credentials, and register the OS-mounted SMB share as a direct smb2 volume.
///
/// Single entry point for the two fire-and-forget auto-upgrade paths — startup
/// (`file_system::upgrade_existing_smb_mounts`) and mount-time
/// (`volumes::watcher::try_upgrade_smb_mount`). They were byte-for-byte
/// identical except the startup copy used the one-shot `resolve_ip_to_hostname`,
/// so it looked up creds by LAN IP and missed hostname-keyed creds → guest →
/// `STATUS_LOGON_FAILURE`. Keeping both callers here means the resolver choice
/// can't drift between them again. (The manual "Connect directly" path uses
/// `try_smb_upgrade` instead, because it surfaces `CredentialsNeeded` to prompt.)
///
/// Uses `resolve_ip_to_hostname_with_wait` (polls the mDNS host cache up to
/// 1500 ms), not the one-shot resolver: macOS auto-remounts give us the LAN IP
/// via statfs, but stored creds are keyed by the mDNS hostname (e.g.
/// `smb://naspolya/share`). A no-wait lookup races mDNS and misses. Fails open —
/// if mDNS never warms, the IP-keyed lookup still runs, then guest.
pub(crate) async fn resolve_and_register_smb_volume(server: &str, share: &str, mount_path: &str, port: u16) {
    let hostname = resolve_ip_to_hostname_with_wait(server, std::time::Duration::from_millis(1500)).await;
    let creds = get_keychain_password(server, hostname.as_deref(), share).await;
    let (username, password) = match &creds {
        Some((u, p)) => (Some(u.as_str()), Some(p.as_str())),
        None => (None, None),
    };
    register_smb_volume(server, share, mount_path, username, password, port).await;
}

/// Attempts the smb2 connection and registers the volume. Returns `Ok(())` on success.
pub(crate) async fn try_smb_upgrade(
    server: &str,
    share: &str,
    mount_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
    volume_id: &str,
) -> Result<(), UpgradeError> {
    use crate::file_system::volume::smb::connect_smb_volume;
    use crate::network::smb_util::is_auth_error;
    use std::sync::Arc;

    // Resolve mDNS service names to connectable addresses
    let resolved_server = resolve_server_address(server);
    let display = friendly_server_name(server);

    // Same re-check as the auto path: the 1.5 s mDNS wait upstream is enough
    // time for another path to have upgraded this volume already.
    if is_already_direct(volume_id) {
        log::debug!("{volume_id} is already a direct smb2 connection; nothing to upgrade");
        return Ok(());
    }

    let params =
        crate::file_system::volume::smb::SmbConnectionParams::new(&resolved_server, share, port, username, password);
    match connect_with_retry(|| connect_smb_volume(share, mount_path, volume_id, params.clone())).await {
        Ok(volume) => {
            register_replacing_predecessor(volume_id, Arc::new(volume)).await;
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
            // Manual "Connect directly" also installs a Direct session; resume the
            // drive index the same way the auto-upgrade path does (no-op unless the
            // user had it enabled), so the two install paths stay consistent.
            crate::index_host::index().resume_after_reconnect(volume_id.to_string());
            Ok(())
        }
        Err(e) => {
            if is_auth_error(&e) {
                Err(UpgradeError::Auth)
            } else {
                let reason = UpgradeFailure::from_smb_error(&e);
                // The raw error stays in the log where it's useful; the caller
                // gets the typed reason and the frontend writes the sentence.
                log::warn!(
                    "Couldn't establish an smb2 connection for {}/{} ({:?}): {}",
                    resolved_server,
                    share,
                    reason,
                    e
                );
                Err(UpgradeError::Network {
                    reason,
                    display_name: display,
                })
            }
        }
    }
}

/// Looks up the mDNS hostname for an IP address from discovered hosts.
///
/// Returns the hostname (like "naspolya") without `.local` suffix.
pub(crate) fn resolve_ip_to_hostname(ip: &str) -> Option<String> {
    let hosts = get_discovered_hosts();
    for host in &hosts {
        if host.ip_address.as_deref() == Some(ip) {
            // Return the service name (lowercased), which is what Keychain keys use
            return Some(host.name.to_lowercase());
        }
    }
    None
}

/// Returns true if `ip` is a literal IPv4 address in a private range (RFC 1918 or
/// link-local 169.254/16). mDNS can only help for those: public/VPN/Tailscale IPs
/// won't show up in the local mDNS cache, so there's no point waiting on them.
///
/// Returns `false` for non-IP strings (hostnames), since `resolve_ip_to_hostname`
/// only matches discovered hosts by exact IP.
pub(crate) fn is_private_ipv4(ip: &str) -> bool {
    use std::net::Ipv4Addr;
    let Ok(addr) = ip.parse::<Ipv4Addr>() else {
        return false;
    };
    addr.is_private() || addr.is_link_local()
}

/// Like `resolve_ip_to_hostname`, but waits briefly for mDNS to populate the
/// discovered-host cache when the lookup misses on the first try. Solves the
/// startup race where macOS auto-remounts an SMB share, FSEvents fires before
/// mDNS has had time to find the host, and `statfs`-derived IP-only Keychain
/// lookups miss the credentials we have keyed by hostname.
///
/// Only waits for private-range IPv4 addresses (where mDNS is plausible) and only
/// if `is_network_enabled()`. Otherwise returns whatever the immediate sync
/// lookup gave us. Polls every 100ms up to `timeout`. The caller is responsible
/// for kicking off discovery via `network::ensure_mdns_started` before calling
/// this; the wait alone won't start the daemon.
pub(crate) async fn resolve_ip_to_hostname_with_wait(ip: &str, timeout: std::time::Duration) -> Option<String> {
    // Fast path: already in the cache.
    if let Some(hostname) = resolve_ip_to_hostname(ip) {
        return Some(hostname);
    }
    // No point waiting for non-private IPs (Tailscale, public DNS, etc.) or when
    // networking is disabled by the user.
    if !is_private_ipv4(ip) || !crate::network::is_network_enabled() {
        return None;
    }

    let poll_interval = std::time::Duration::from_millis(100);
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        tokio::time::sleep(poll_interval).await;
        if let Some(hostname) = resolve_ip_to_hostname(ip) {
            log::debug!(
                "Resolved IP {} to hostname {} after waiting {:?}",
                ip,
                hostname,
                start.elapsed()
            );
            return Some(hostname);
        }
    }
    log::debug!(
        "Couldn't resolve IP {} to a hostname via mDNS within {:?}; proceeding without",
        ip,
        timeout
    );
    None
}

/// Resolves a server address from `statfs` to a connectable address.
///
/// `statfs` can return different formats depending on how the mount was created:
/// - An IP address like `192.168.1.111`: usable as-is
/// - A DNS hostname like `fileserver.corp.example.com`: usable as-is
/// - An mDNS service name like `Naspolya._smb._tcp.local`: NOT resolvable by DNS, must be resolved
///   to an IP via the mDNS discovery state
///
/// Returns the resolved IP if possible, otherwise the original string.
pub(crate) fn resolve_server_address(server: &str) -> String {
    // Detect mDNS service names (contain "._tcp" or "._udp")
    if !server.contains("._tcp") && !server.contains("._udp") {
        return server.to_string();
    }

    // Extract the service/display name (everything before the first "._")
    let service_name = server.split("._").next().unwrap_or(server);

    // Look up the discovered host by name (case-insensitive)
    let hosts = get_discovered_hosts();
    for host in &hosts {
        if host.name.eq_ignore_ascii_case(service_name) {
            if let Some(ref ip) = host.ip_address {
                log::debug!("Resolved mDNS service name {} to IP {}", server, ip);
                return ip.clone();
            }
            // Host found but no IP yet; try the hostname
            if let Some(ref hostname) = host.hostname {
                log::debug!("Resolved mDNS service name {} to hostname {}", server, hostname);
                return hostname.clone();
            }
        }
    }

    log::warn!(
        "Could not resolve mDNS service name {} (no matching discovered host)",
        server
    );
    server.to_string()
}

/// Extracts the friendly display name from a server address.
///
/// For mDNS service names like `Naspolya._smb._tcp.local`, returns `Naspolya`.
/// For IPs or hostnames, tries `resolve_ip_to_hostname`, falls back to the raw string.
pub(crate) fn friendly_server_name(server: &str) -> String {
    // mDNS service name: extract the part before "._"
    if server.contains("._tcp") || server.contains("._udp") {
        return server.split("._").next().unwrap_or(server).to_string();
    }
    // IP address: try to resolve to mDNS hostname
    resolve_ip_to_hostname(server).unwrap_or_else(|| server.to_string())
}

/// The server-name forms another app (Finder) might have keyed an SMB password under for
/// this server, gathered from the discovery state. Finder typically uses the full mDNS
/// service name (`Naspolya._smb._tcp.local`), while we mount by IP — so for each
/// discovered host that's the same identity as `server`, we contribute its advertised
/// name, its `.local` hostname, and the synthesized `{name}._smb._tcp.local` service form.
/// Pure over `hosts` for testability; the live wrapper feeds `get_discovered_hosts()`.
/// macOS-only: every caller reads the system keychain, and an ungated definition fails the
/// Linux build via `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) fn system_keychain_aliases(server: &str) -> Vec<String> {
    system_keychain_aliases_from(server, &get_discovered_hosts())
}

// `test` keeps the pure helper compiling for the unit tests below, which run on Linux too.
#[cfg(any(target_os = "macos", test))]
fn system_keychain_aliases_from(server: &str, hosts: &[crate::network::NetworkHost]) -> Vec<String> {
    use crate::network::server_identity::same_server;
    let mut out = Vec::new();
    for h in hosts {
        let matches = same_server(&h.name, server, hosts)
            || h.hostname.as_deref().is_some_and(|hn| same_server(hn, server, hosts))
            || h.ip_address.as_deref() == Some(server);
        if matches {
            out.push(h.name.clone());
            out.push(format!("{}._smb._tcp.local", h.name));
            if let Some(hn) = &h.hostname {
                out.push(hn.clone());
            }
        }
    }
    out
}

/// Tries to retrieve SMB credentials from the Keychain.
///
/// Tries multiple keys: by IP (from statfs), by hostname (from mDNS discovery),
/// at both share-level and server-level.
pub(crate) async fn get_keychain_password(
    server_ip: &str,
    hostname: Option<&str>,
    share: &str,
) -> Option<(String, String)> {
    let server_ip = server_ip.to_string();
    let hostname = hostname.map(|s| s.to_string());
    let share = share.to_string();

    tokio::task::spawn_blocking(move || {
        use crate::network::keychain;

        // Build a list of server names to try (hostname first, then IP)
        let mut servers_to_try: Vec<&str> = Vec::new();
        if let Some(ref h) = hostname {
            servers_to_try.push(h);
        }
        servers_to_try.push(&server_ip);

        for server in &servers_to_try {
            // Try share-level credentials first (more specific)
            if let Ok(creds) = keychain::get_credentials(server, Some(&share)) {
                log::debug!("Found Keychain credentials via {}/{}", server, share);
                return Some((creds.username, creds.password));
            }
            // Try server-level credentials
            if let Ok(creds) = keychain::get_credentials(server, None) {
                log::debug!("Found Keychain credentials via {} (server-level)", server);
                return Some((creds.username, creds.password));
            }
        }

        log::debug!("No Keychain credentials for {:?} / {} / {}", hostname, server_ip, share);
        None
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn system_keychain_aliases_include_the_mdns_service_form_for_an_ip() {
        use crate::network::{HostSource, NetworkHost};
        let hosts = [NetworkHost {
            id: "naspolya".into(),
            name: "Naspolya".into(),
            hostname: Some("Naspolya.local".into()),
            ip_address: Some("192.168.1.111".into()),
            port: 445,
            source: HostSource::Discovered,
        }];
        // We mount by IP; Finder keyed its password by the mDNS service name. The alias
        // set must include that form so the keychain lookup can find it.
        let aliases = system_keychain_aliases_from("192.168.1.111", &hosts);
        assert!(
            aliases.contains(&"Naspolya._smb._tcp.local".to_string()),
            "got {aliases:?}"
        );
        assert!(aliases.contains(&"Naspolya.local".to_string()));
        assert!(aliases.contains(&"Naspolya".to_string()));
    }

    #[test]
    fn system_keychain_aliases_empty_for_an_unknown_server() {
        assert!(system_keychain_aliases_from("10.9.9.9", &[]).is_empty());
    }

    #[test]
    fn is_private_ipv4_recognizes_rfc1918_and_link_local() {
        assert!(is_private_ipv4("10.0.0.1"));
        assert!(is_private_ipv4("192.168.1.111"));
        assert!(is_private_ipv4("172.16.5.7"));
        assert!(is_private_ipv4("169.254.1.2"), "link-local should count");
    }

    #[test]
    fn is_private_ipv4_rejects_public_and_special() {
        assert!(!is_private_ipv4("8.8.8.8"));
        assert!(!is_private_ipv4("100.64.0.1"), "Tailscale/CGNAT not private");
        assert!(!is_private_ipv4("127.0.0.1"), "loopback not private");
        assert!(!is_private_ipv4("naspolya"), "hostnames return false");
        assert!(!is_private_ipv4(""));
        assert!(!is_private_ipv4("::1"), "IPv6 currently returns false");
    }

    /// `resolve_ip_to_hostname_with_wait` must return immediately (no polling)
    /// when the IP isn't a private-range IPv4 — Tailscale/public DNS won't show
    /// up in mDNS so there's nothing to wait for.
    #[tokio::test]
    async fn wait_helper_returns_immediately_for_non_private_ip() {
        let start = std::time::Instant::now();
        let result = resolve_ip_to_hostname_with_wait("8.8.8.8", Duration::from_millis(500)).await;
        let elapsed = start.elapsed();
        assert_eq!(result, None);
        assert!(
            elapsed < Duration::from_millis(50),
            "expected fast path (< 50ms), took {:?}",
            elapsed
        );
    }

    /// `network.enabled` is a process-global, so the two tests that flip it must
    /// not run concurrently: one setting it `false` while the other polls made
    /// the other short-circuit and fail (whichever lost the race).
    /// Async-aware: both tests `await` while holding it.
    static NETWORK_FLAG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// `resolve_ip_to_hostname_with_wait` must short-circuit when the runtime
    /// `network.enabled` flag is off, even for a private IP — mDNS isn't running
    /// so polling would just burn the timeout.
    #[tokio::test]
    async fn wait_helper_short_circuits_when_network_disabled() {
        let _serialized = NETWORK_FLAG_LOCK.lock().await;
        let prev = crate::network::is_network_enabled();
        crate::network::set_network_enabled_flag(false);

        let start = std::time::Instant::now();
        let result = resolve_ip_to_hostname_with_wait("192.168.1.111", Duration::from_millis(500)).await;
        let elapsed = start.elapsed();

        // Restore before assertions so other tests aren't poisoned by panics.
        crate::network::set_network_enabled_flag(prev);

        assert_eq!(result, None);
        assert!(
            elapsed < Duration::from_millis(50),
            "expected fast path (< 50ms), took {:?}",
            elapsed
        );
    }

    /// Times out gracefully when no host ever shows up in the cache (and falls
    /// back to `None` so the caller can use IP-only Keychain lookup).
    #[tokio::test]
    async fn wait_helper_times_out_gracefully() {
        let _serialized = NETWORK_FLAG_LOCK.lock().await;
        // Ensure network is "enabled" so we exercise the polling path.
        let prev = crate::network::is_network_enabled();
        crate::network::set_network_enabled_flag(true);

        // Use a unique private IP that no test has ever populated, so the cache
        // miss is deterministic.
        let timeout = Duration::from_millis(300);
        let start = std::time::Instant::now();
        let result = resolve_ip_to_hostname_with_wait("10.255.255.254", timeout).await;
        let elapsed = start.elapsed();

        crate::network::set_network_enabled_flag(prev);

        assert_eq!(result, None);
        assert!(
            elapsed >= timeout,
            "should have polled until timeout; elapsed {:?}",
            elapsed
        );
        // Generous upper bound — single poll interval slack.
        assert!(
            elapsed < timeout + Duration::from_millis(250),
            "shouldn't blow past timeout by much; elapsed {:?}",
            elapsed
        );
    }

    // ── register_replacing_predecessor ─────────────────────────────────

    /// A minimal `Volume` impl that records its lifecycle hooks and, like a
    /// real `SmbVolume`, stops serving requests once its session is torn down.
    /// Used to verify `register_replacing_predecessor` retires the displaced
    /// volume without breaking the holders still using it.
    mod tracking {
        use crate::file_system::listing::metadata::FileEntry;
        use crate::file_system::volume::{SpaceInfo, Volume, VolumeError};
        use std::future::Future;
        use std::path::{Path, PathBuf};
        use std::pin::Pin;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        pub(super) struct TrackingVolume {
            pub(super) on_unmount_called: Arc<AtomicBool>,
            pub(super) on_superseded_called: Arc<AtomicBool>,
            root: PathBuf,
            smb_state: Option<crate::file_system::volume::SmbConnectionState>,
        }

        /// The hook flags of one `TrackingVolume`, for assertions.
        pub(super) struct Hooks {
            pub(super) unmounted: Arc<AtomicBool>,
            pub(super) superseded: Arc<AtomicBool>,
        }

        impl TrackingVolume {
            /// A volume that isn't an SMB volume at all (`smb_connection_state()`
            /// is `None`), which is what an OS-mounted share looks like before
            /// its upgrade.
            pub(super) fn create(label: &str) -> (Arc<dyn Volume>, Hooks) {
                Self::create_with_smb_state(label, None)
            }

            /// A volume that reports an SMB connection state, for the
            /// "is this already upgraded?" checks.
            pub(super) fn create_with_smb_state(
                label: &str,
                smb_state: Option<crate::file_system::volume::SmbConnectionState>,
            ) -> (Arc<dyn Volume>, Hooks) {
                let unmounted = Arc::new(AtomicBool::new(false));
                let superseded = Arc::new(AtomicBool::new(false));
                let vol = Arc::new(Self {
                    on_unmount_called: Arc::clone(&unmounted),
                    on_superseded_called: Arc::clone(&superseded),
                    root: PathBuf::from(format!("/tmp/tracking-{label}")),
                    smb_state,
                }) as Arc<dyn Volume>;
                (vol, Hooks { unmounted, superseded })
            }
        }

        impl Volume for TrackingVolume {
            fn name(&self) -> &str {
                "tracking"
            }
            fn root(&self) -> &Path {
                &self.root
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn list_directory<'a>(
                &'a self,
                _path: &'a Path,
                _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync + 'a)>,
            ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
                Box::pin(async { Err(VolumeError::NotSupported) })
            }
            fn get_metadata<'a>(
                &'a self,
                _path: &'a Path,
            ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
                Box::pin(async { Err(VolumeError::NotSupported) })
            }
            fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
                Box::pin(async { false })
            }
            /// Stands in for any real request on a held volume reference: it
            /// succeeds while the session is up and reports the session gone
            /// once `on_unmount` has torn it down, exactly like `SmbVolume`'s
            /// `check_connection` gate.
            fn is_directory<'a>(
                &'a self,
                _path: &'a Path,
            ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
                let torn_down = self.on_unmount_called.load(Ordering::Relaxed);
                Box::pin(async move {
                    if torn_down {
                        Err(VolumeError::DeviceDisconnected("session torn down".to_string()))
                    } else {
                        Ok(true)
                    }
                })
            }
            fn get_space_info<'a>(
                &'a self,
            ) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
                Box::pin(async { Err(VolumeError::NotSupported) })
            }
            fn smb_connection_state(&self) -> Option<crate::file_system::volume::SmbConnectionState> {
                self.smb_state
            }
            fn on_unmount(&self) {
                self.on_unmount_called.store(true, Ordering::Relaxed);
            }
            /// Retires without tearing the session down, so holders keep working.
            fn on_superseded(&self) {
                self.on_superseded_called.store(true, Ordering::Relaxed);
            }
        }
    }

    /// `register_replacing_predecessor` must retire the displaced volume via
    /// `on_superseded`, NOT `on_unmount`. The device is still there and the
    /// predecessor may still be serving in-flight work; unmounting it tears a
    /// live session out from under those holders.
    #[tokio::test]
    async fn predecessor_is_superseded_not_unmounted() {
        use std::sync::atomic::Ordering;

        let volume_id = "test-register-replacing-predecessor-replace";
        let manager = crate::file_system::get_volume_manager();

        let (old_volume, old_hooks) = tracking::TrackingVolume::create("old");
        let (new_volume, new_hooks) = tracking::TrackingVolume::create("new");

        manager.register(volume_id, old_volume);
        assert!(!old_hooks.superseded.load(Ordering::Relaxed));

        register_replacing_predecessor(volume_id, std::sync::Arc::clone(&new_volume)).await;

        assert!(
            old_hooks.superseded.load(Ordering::Relaxed),
            "displaced volume's on_superseded must have been called"
        );
        assert!(
            !old_hooks.unmounted.load(Ordering::Relaxed),
            "displaced volume must NOT be unmounted: the device is still there and in-flight work still holds it"
        );
        assert!(
            !new_hooks.superseded.load(Ordering::Relaxed) && !new_hooks.unmounted.load(Ordering::Relaxed),
            "new volume gets no lifecycle hook"
        );

        let current = manager.get(volume_id).expect("new volume should be registered");
        assert!(
            std::sync::Arc::ptr_eq(&current, &new_volume),
            "new volume should be the one registered under volume_id"
        );

        manager.unregister(volume_id);
    }

    /// The lifecycle invariant behind the whole swap: an operation that grabbed
    /// an `Arc` to the volume before an upgrade keeps working on it afterwards.
    ///
    /// This is the real-world failure it pins. A copy to a NAS held `src_vol` /
    /// `dst_vol` clones (`volume_copy.rs`) while a redundant SMB upgrade
    /// replaced the volume; the swap called `on_unmount` on the predecessor,
    /// which dropped the smb2 session, and the running copy died with
    /// `DeviceDisconnected` on a connection that was demonstrably healthy.
    #[tokio::test]
    async fn a_held_volume_reference_keeps_working_across_a_replace() {
        let volume_id = "test-register-replacing-predecessor-held-reference";
        let manager = crate::file_system::get_volume_manager();
        manager.unregister(volume_id);

        let (old_volume, _) = tracking::TrackingVolume::create("busy");
        manager.register(volume_id, old_volume);

        // What a running transfer holds: an `Arc` clone taken before the swap.
        let held = manager.get(volume_id).expect("registered above");
        assert!(
            held.is_directory(Path::new("/anything")).await.is_ok(),
            "the held reference works before the swap"
        );

        let (new_volume, _) = tracking::TrackingVolume::create("upgraded");
        register_replacing_predecessor(volume_id, new_volume).await;

        assert!(
            held.is_directory(Path::new("/anything")).await.is_ok(),
            "the held reference must survive the swap: an upgrade is not a disconnect"
        );

        manager.unregister(volume_id);
    }

    // ── Connect retry ──────────────────────────────────────────────────

    fn io_error(kind: std::io::ErrorKind) -> smb2::Error {
        smb2::Error::Io(std::io::Error::new(kind, "test"))
    }

    /// The first direct connect to a LAN address right after launch routinely
    /// comes back `EHOSTUNREACH` while the route and the macOS Local Network
    /// permission settle; the identical attempt moments later succeeds. That
    /// class of failure is worth one more try.
    #[tokio::test]
    async fn a_connect_that_never_reached_the_server_is_retried() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = AtomicUsize::new(0);

        let result = connect_with_retry(|| async {
            if attempts.fetch_add(1, Ordering::Relaxed) == 0 {
                Err(io_error(std::io::ErrorKind::HostUnreachable))
            } else {
                Ok("connected")
            }
        })
        .await;

        assert_eq!(result.ok(), Some("connected"));
        assert_eq!(attempts.load(Ordering::Relaxed), 2, "one retry, then success");
    }

    /// Retrying a rejected password is pointless and risks locking the account.
    /// The "Sign in" flow owns that recovery.
    #[tokio::test]
    async fn a_rejected_credential_is_never_retried() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = AtomicUsize::new(0);

        let result: Result<&str, _> = connect_with_retry(|| async {
            attempts.fetch_add(1, Ordering::Relaxed);
            Err(smb2::Error::Auth {
                message: "bad password".to_string(),
            })
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::Relaxed), 1, "auth failures are final");
    }

    /// A server that's genuinely gone must still fail promptly: the retries are
    /// capped in count, not just in delay.
    #[tokio::test]
    async fn retries_are_bounded_so_a_dead_server_fails_fast() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = AtomicUsize::new(0);

        let started = std::time::Instant::now();
        let result: Result<&str, _> = connect_with_retry(|| async {
            attempts.fetch_add(1, Ordering::Relaxed);
            Err(io_error(std::io::ErrorKind::HostUnreachable))
        })
        .await;
        let elapsed = started.elapsed();

        assert!(result.is_err());
        assert_eq!(
            attempts.load(Ordering::Relaxed),
            CONNECT_RETRY_BACKOFF.len() + 1,
            "one initial attempt plus one per backoff step, then give up"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "someone is watching a 'Connecting directly…' toast; took {:?}",
            elapsed
        );
    }

    /// An attempt that burned the whole connect timeout has already answered the
    /// question. Retrying it would stack another 10 s onto a user's wait, so the
    /// budget stops the loop even though the failure kind looks retryable.
    #[tokio::test]
    async fn a_slow_first_attempt_spends_the_retry_budget() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = AtomicUsize::new(0);

        let result: Result<&str, _> = connect_with_retry(|| async {
            attempts.fetch_add(1, Ordering::Relaxed);
            // allowed-test-sleep: the slow attempt IS the subject — it's what spends the retry budget.
            tokio::time::sleep(CONNECT_RETRY_BUDGET + Duration::from_millis(50)).await;
            Err(io_error(std::io::ErrorKind::HostUnreachable))
        })
        .await;

        assert!(result.is_err());
        assert_eq!(
            attempts.load(Ordering::Relaxed),
            1,
            "a slow attempt spends the budget; don't stack another"
        );
    }

    // ── Upgrade idempotence and pass coalescing ────────────────────────

    /// An upgrade of a volume that is ALREADY a healthy direct smb2 connection
    /// must cost nothing: no TCP connect, no negotiate, no session setup, and
    /// above all no swap of a perfectly good volume.
    ///
    /// The startup pass used to snapshot its eligibility list up to 15 s before
    /// acting, so it happily re-upgraded volumes another path had already
    /// upgraded in the meantime. It replaced one volume three times in 15
    /// seconds, and the third replacement landed mid-copy.
    #[tokio::test]
    async fn upgrading_an_already_direct_volume_costs_nothing() {
        use crate::file_system::volume::{SmbConnectionState, smb_volume_id};

        // TEST-NET-2 (RFC 5737): reserved, never routed. If the upgrade doesn't
        // short-circuit, it tries to connect here and fails.
        let server = "198.51.100.7";
        let share = "unreachable";
        let volume_id = smb_volume_id(server, 445, share);
        let manager = crate::file_system::get_volume_manager();

        let (direct, _) =
            tracking::TrackingVolume::create_with_smb_state("already-direct", Some(SmbConnectionState::Direct));
        manager.register(&volume_id, std::sync::Arc::clone(&direct));

        let result = try_smb_upgrade(server, share, "/Volumes/unreachable", None, None, 445, &volume_id).await;

        assert!(
            result.is_ok(),
            "an already-direct volume is the desired end state, so the upgrade succeeds trivially"
        );
        let current = manager.get(&volume_id).expect("still registered");
        assert!(
            std::sync::Arc::ptr_eq(&current, &direct),
            "the healthy volume must not be swapped out from under its holders"
        );

        manager.unregister(&volume_id);
    }

    /// The same short-circuit on the auto-upgrade path (startup and mount-time),
    /// which is the one that fired the redundant upgrades.
    #[tokio::test]
    async fn the_auto_upgrade_path_skips_an_already_direct_volume() {
        use crate::file_system::volume::{SmbConnectionState, smb_volume_id};

        let server = "198.51.100.8";
        let share = "unreachable";
        let volume_id = smb_volume_id(server, 445, share);
        let manager = crate::file_system::get_volume_manager();

        let (direct, _) =
            tracking::TrackingVolume::create_with_smb_state("already-direct-auto", Some(SmbConnectionState::Direct));
        manager.register(&volume_id, std::sync::Arc::clone(&direct));

        let start = std::time::Instant::now();
        register_smb_volume(server, share, "/Volumes/unreachable", None, None, 445).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(200),
            "must short-circuit before any connect attempt; took {:?}",
            elapsed
        );
        let current = manager.get(&volume_id).expect("still registered");
        assert!(
            std::sync::Arc::ptr_eq(&current, &direct),
            "the healthy volume must not be swapped"
        );

        manager.unregister(&volume_id);
    }

    /// Only a HEALTHY direct volume is "nothing left to do". A DISCONNECTED
    /// `SmbVolume` still wants the work: that's the manual "Connect directly"
    /// recovery path after a share dropped. Skipping it there would dead-end the
    /// user on a broken volume.
    #[test]
    fn only_a_healthy_direct_volume_short_circuits_the_upgrade() {
        use crate::file_system::volume::SmbConnectionState;
        let manager = crate::file_system::get_volume_manager();

        for (label, state, expected) in [
            ("direct", Some(SmbConnectionState::Direct), true),
            ("disconnected", Some(SmbConnectionState::Disconnected), false),
            ("os-mount-state", Some(SmbConnectionState::OsMount), false),
            ("plain-local", None, false),
        ] {
            let volume_id = format!("test-already-direct-{label}");
            let (vol, _) = tracking::TrackingVolume::create_with_smb_state(label, state);
            manager.register(&volume_id, vol);
            assert_eq!(is_already_direct(&volume_id), expected, "{label} volume");
            manager.unregister(&volume_id);
        }
    }

    /// `ensure_network_discovery_started` runs on every user networking action,
    /// and each upgrade pass waits up to 15 s for mDNS before acting. Two clicks
    /// nine seconds apart stacked two passes, both firing blind. Only one pass
    /// may be in flight.
    #[test]
    fn a_second_upgrade_pass_is_dropped_while_one_is_pending() {
        let first = UpgradePass::begin().expect("the first pass runs");
        assert!(
            UpgradePass::begin().is_none(),
            "a second pass while one is still pending must be dropped, not stacked"
        );
        drop(first);
        assert!(
            UpgradePass::begin().is_some(),
            "once the pass finishes, the next networking action starts a new one"
        );
    }

    /// When no predecessor exists, `register_replacing_predecessor` just
    /// registers — no lifecycle hook (there's nothing to call it on), no panic.
    #[tokio::test]
    async fn register_with_no_predecessor_just_registers() {
        use std::sync::atomic::Ordering;

        let volume_id = "test-register-replacing-predecessor-fresh";
        let manager = crate::file_system::get_volume_manager();
        manager.unregister(volume_id); // belt-and-suspenders in case a prior test leaked.

        let (new_volume, new_hooks) = tracking::TrackingVolume::create("fresh");
        register_replacing_predecessor(volume_id, std::sync::Arc::clone(&new_volume)).await;

        assert!(!new_hooks.superseded.load(Ordering::Relaxed));
        assert!(!new_hooks.unmounted.load(Ordering::Relaxed));
        assert!(manager.get(volume_id).is_some());

        manager.unregister(volume_id);
    }
}

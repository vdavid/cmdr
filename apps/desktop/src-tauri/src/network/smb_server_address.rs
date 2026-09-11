//! Who an SMB server string from `statfs` is, as far as this machine can tell: an
//! address to dial, a name to show, and the credentials saved for it.
//!
//! `statfs` names a server however its mount did (an IP, a DNS hostname, or an mDNS
//! service instance name), while discovery and the credential store know it by
//! other names. The auto-upgrade paths (`smb_upgrade`) and "Connect directly"
//! (`smb_connect_directly`) both read a mount's server through here, so they can't
//! disagree about which server it is.

use crate::network::get_discovered_hosts;

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

/// A server string from `statfs`, once we know whether anything can dial it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ServerAddress {
    /// Ready to dial: an IP, a DNS hostname, or an mDNS service name discovery
    /// has already resolved.
    Connectable(String),
    /// An mDNS SERVICE instance name (`Naspolya._smb._tcp.local.`) that discovery
    /// hasn't seen yet.
    ///
    /// Not a hostname, so `getaddrinfo` can never resolve it: dialing it spends
    /// the resolver's timeout and fails every single time. Measured at 8.2 s on
    /// one report, which also cost the user a "this share is stuck on the kernel
    /// mount" warning for a share that connected fine 30 s later, once discovery
    /// went active (ERR-48RZX).
    UndiscoveredService,
}

/// Resolves a server address from `statfs` to something dialable.
///
/// `statfs` can return different formats depending on how the mount was created:
/// - An IP address like `192.168.1.111`: usable as-is
/// - A DNS hostname like `fileserver.corp.example.com`: usable as-is
/// - An mDNS service name like `Naspolya._smb._tcp.local`: NOT resolvable by DNS, must be resolved
///   to an IP via the mDNS discovery state
///
/// The last kind is the only one that can come back
/// [`UndiscoveredService`](ServerAddress::UndiscoveredService), and only until
/// discovery finds it.
pub(crate) fn resolve_server_address(server: &str) -> ServerAddress {
    // Detect mDNS service names (contain "._tcp" or "._udp")
    if !server.contains("._tcp") && !server.contains("._udp") {
        return ServerAddress::Connectable(server.to_string());
    }

    // Extract the service/display name (everything before the first "._")
    let service_name = server.split("._").next().unwrap_or(server);

    // Look up the discovered host by name (case-insensitive)
    let hosts = get_discovered_hosts();
    for host in &hosts {
        if host.name.eq_ignore_ascii_case(service_name) {
            if let Some(ref ip) = host.ip_address {
                log::debug!("Resolved mDNS service name {} to IP {}", server, ip);
                return ServerAddress::Connectable(ip.clone());
            }
            // Host found but no IP yet; try the hostname
            if let Some(ref hostname) = host.hostname {
                log::debug!("Resolved mDNS service name {} to hostname {}", server, hostname);
                return ServerAddress::Connectable(hostname.clone());
            }
        }
    }

    // DEBUG rather than WARN: an upgrade pass that runs before discovery settles
    // hits this routinely, and the pass that runs after it resolves the name and
    // connects. Handing the unresolved name to the dialer instead is what turned
    // this into a visible failure.
    log::debug!(
        "mDNS service name {} isn't discovered yet, so there's nothing to dial for it",
        server
    );
    ServerAddress::UndiscoveredService
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

// `test` keeps the pure helper compiling for its unit tests, which run on Linux too.
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
#[path = "smb_server_address_test.rs"]
mod tests;

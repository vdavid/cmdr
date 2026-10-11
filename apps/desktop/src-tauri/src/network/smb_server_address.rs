//! Who an SMB server string from `statfs` is, as far as this machine can tell: an
//! address to dial, a name to show, and the credentials saved for it.
//!
//! `statfs` names a server however its mount did (an IP, a DNS hostname, or an mDNS
//! service instance name), while discovery and the credential store know it by
//! other names. The auto-upgrade paths (`smb_upgrade`) and "Connect directly"
//! (`smb_connect_directly`) both read a mount's server through here, so they can't
//! disagree about which server it is.

use crate::network::discovery_gate::{self, DiscoveryLease};
use crate::network::{NetworkHost, cached_discovered_hosts, fresh_discovered_hosts};
use std::time::Duration;

/// The mDNS name discovery pairs with `ip` right now, lowercased (what Keychain
/// keys use). Fresh evidence only: a credential picked off a stale pairing could
/// belong to whichever server had that IP before.
pub(crate) fn resolve_ip_to_hostname(ip: &str) -> Option<String> {
    name_for_ip_in(ip, &fresh_discovered_hosts())
}

fn name_for_ip_in(ip: &str, hosts: &[NetworkHost]) -> Option<String> {
    hosts
        .iter()
        .find(|host| host.ip_address.as_deref() == Some(ip))
        .map(|host| host.name.to_lowercase())
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

/// What discovery could vouch for about a mount's server, with the browse kept
/// running while the caller acts on it.
pub(crate) struct DiscoveredServer {
    /// The mDNS name for an IP `server`, lowercased: what Cmdr keys its Keychain
    /// entries by. `None` when discovery doesn't know it, and for a server that isn't
    /// an IP.
    pub hostname: Option<String>,
    /// Keeps the identity answers the caller reads next (`resolve_server_address`,
    /// the per-share switch, the Keychain aliases) on fresh evidence.
    _browse: Option<DiscoveryLease>,
}

/// Holds the mDNS browse and waits, up to `timeout`, for discovery to vouch for
/// `server`: an IP to be paired with its mDNS name, or an mDNS service name to get
/// an address. Keep the answer alive for as long as the caller reads identity.
///
/// Solves the race where macOS auto-remounts an SMB share, FSEvents fires before
/// discovery has found the host, and a `statfs`-derived IP misses the credentials
/// keyed by hostname, or a service name has nothing to dial. With the browse off
/// until something needs it, every upgrade meets this race, not only the one at
/// login.
///
/// Waits only where mDNS can answer: a private-range IPv4 or a service name, with
/// `network.enabled` on. Anything else is looked up once. Polls every 100 ms.
pub(crate) async fn discover_server(server: &str, timeout: Duration) -> DiscoveredServer {
    let browse = crate::network::is_network_enabled().then(discovery_gate::hold);
    let waits = browse.is_some() && (is_private_ipv4(server) || is_service_name(server));
    let known = || {
        if is_service_name(server) {
            service_address(server).map(|_| None)
        } else {
            resolve_ip_to_hostname(server).map(Some)
        }
    };

    let poll_interval = Duration::from_millis(100);
    let start = std::time::Instant::now();
    let mut answer = known();
    while answer.is_none() && waits && start.elapsed() < timeout {
        tokio::time::sleep(poll_interval).await;
        answer = known();
    }
    if waits && answer.is_none() {
        log::debug!("Discovery didn't vouch for server={server:?} within {timeout:?}; proceeding without");
    }
    DiscoveredServer {
        hostname: answer.flatten(),
        _browse: browse,
    }
}

fn is_service_name(server: &str) -> bool {
    server.contains("._tcp") || server.contains("._udp")
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
    if !is_service_name(server) {
        return ServerAddress::Connectable(server.to_string());
    }
    if let Some(address) = service_address(server) {
        log::debug!("Resolved mDNS service name server={server:?} to host={address:?}");
        return ServerAddress::Connectable(address);
    }

    // DEBUG rather than WARN: an upgrade pass that runs before discovery settles
    // hits this routinely, and the pass that runs after it resolves the name and
    // connects. Handing the unresolved name to the dialer instead is what turned
    // this into a visible failure.
    log::debug!(
        "mDNS service name server={:?} isn't discovered yet, so there's nothing to dial for it",
        server
    );
    ServerAddress::UndiscoveredService
}

/// The address the running browse resolved the service name `server` to: its IP,
/// else its hostname. Fresh evidence only: dialing a name at the address it had
/// under an earlier browse could reach another server.
fn service_address(server: &str) -> Option<String> {
    let service_name = server.split("._").next().unwrap_or(server);
    fresh_discovered_hosts()
        .into_iter()
        .filter(|host| host.name.eq_ignore_ascii_case(service_name))
        .find_map(|host| host.ip_address.or(host.hostname))
}

/// Extracts the friendly display name from a server address.
///
/// For mDNS service names like `Naspolya._smb._tcp.local`, returns `Naspolya`.
/// For IPs, the mDNS name discovery last paired it with (a stale pairing is fine for a
/// label), else the raw string.
pub(crate) fn friendly_server_name(server: &str) -> String {
    friendly_name_in(server, &cached_discovered_hosts())
}

/// [`friendly_server_name`] over `hosts`. ❗ The host half of a discovery name only: a
/// typed server off 445 is discovered as `host:port`, and a caller that adds the port
/// itself read "public on [127.0.0.1:11480]:11480".
fn friendly_name_in(server: &str, hosts: &[NetworkHost]) -> String {
    if is_service_name(server) {
        return server.split("._").next().unwrap_or(server).to_string();
    }
    name_for_ip_in(server, hosts).map_or_else(
        || server.to_string(),
        |name| {
            crate::network::server_identity::SmbServer::from_name(&name)
                .host()
                .to_string()
        },
    )
}

/// The server-name forms another app (Finder) might have keyed an SMB password under for
/// this server, gathered from the discovery state. Finder typically uses the full mDNS
/// service name (`Naspolya._smb._tcp.local`), while we mount by IP — so for each
/// discovered host that's the same identity as `server`, we contribute its advertised
/// name, its `.local` hostname, and the synthesized `{name}._smb._tcp.local` service form.
/// Pure over `hosts` for testability; the live wrapper feeds `fresh_discovered_hosts()`.
/// macOS-only: every caller reads the system keychain, and an ungated definition fails the
/// Linux build via `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) fn system_keychain_aliases(server: &str) -> Vec<String> {
    system_keychain_aliases_from(server, &fresh_discovered_hosts())
}

// `test` keeps the pure helper compiling for its unit tests, which run on Linux too.
#[cfg(any(target_os = "macos", test))]
fn system_keychain_aliases_from(server: &str, hosts: &[NetworkHost]) -> Vec<String> {
    use crate::network::server_identity::same_machine;
    let mut out = Vec::new();
    for h in hosts {
        let matches = same_machine(&h.name, server, hosts)
            || h.hostname.as_deref().is_some_and(|hn| same_machine(hn, server, hosts))
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
/// Tries multiple keys: by hostname (from mDNS discovery), then by IP (from statfs),
/// at both share-level and server-level.
///
/// ❗ Keyed with the `port` (`server_identity::smb_server`), which is how the sign-in
/// sheet saves them: it files a server off 445 under its discovery name
/// (`localhost:11482`). Without it the upgrade never found that password and the direct
/// session went out as a guest while the macOS mount was signed in. Off 445, the
/// port-less keys are tried LAST, so a password saved before keys carried the port
/// still works; on 445 the keys are exactly the port-less ones.
pub(crate) async fn get_keychain_password(
    server_ip: &str,
    hostname: Option<&str>,
    port: u16,
    share: &str,
) -> Option<(String, String)> {
    use crate::network::server_identity::smb_server;

    let mut names: Vec<String> = hostname
        .into_iter()
        .chain(std::iter::once(server_ip))
        .map(str::to_string)
        .collect();
    // The keys this port files under first, then (off 445) the port-less ones older saves used.
    let mut servers_to_try: Vec<String> = names.iter().map(|name| smb_server(name, port)).collect();
    let with_port = servers_to_try.len();
    if port != 445 {
        servers_to_try.append(&mut names);
    }
    let share = share.to_string();

    tokio::task::spawn_blocking(move || {
        use crate::network::keychain;

        for (index, server) in servers_to_try.iter().enumerate() {
            // A port-less key answering for a server off 445 is noted, so "Also forget the
            // saved password" can take that entry too (`keychain::note_found_under_portless`).
            let note = |server: &str| {
                if index >= with_port {
                    keychain::note_found_under_portless(&smb_server(server, port), server);
                }
            };
            // Try share-level credentials first (more specific)
            if let Ok(creds) = keychain::get_credentials(server, Some(&share)) {
                log::debug!("Found Keychain credentials via server={:?}, share={:?}", server, share);
                note(server);
                return Some((creds.username, creds.password));
            }
            // Try server-level credentials
            if let Ok(creds) = keychain::get_credentials(server, None) {
                log::debug!("Found Keychain credentials via server={:?} (server-level)", server);
                note(server);
                return Some((creds.username, creds.password));
            }
        }

        let tried = servers_to_try
            .iter()
            .map(|server| format!("server={server:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        log::debug!("No Keychain credentials for [{tried}], share={share:?}");
        None
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
#[path = "smb_server_address_test.rs"]
mod tests;

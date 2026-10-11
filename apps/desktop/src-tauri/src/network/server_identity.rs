//! Server identity equivalence for SMB.
//!
//! The same physical server shows up under different names depending on who reports it:
//! mDNS advertises `Naspolya._smb._tcp.local`, DNS knows `naspolya.local`, Cmdr mounts
//! by IP (`192.168.1.111`), and `statfs` echoes back whichever form the mount used.
//! Comparing these as strings treats one server as three, which made
//! `disambiguated_mount_path` mount a second copy of an already-mounted share with
//! `ForceNewSession` (fresh auth, guest, dead end) instead of reusing the existing one.
//!
//! `same_machine` derives an identifier set for each input (normalized name forms plus
//! IP, enriched from the mDNS discovery state) and calls two inputs the same server when
//! the sets intersect. When discovery knows nothing, two different-looking strings stay
//! different: that's the safe direction (worst case is a disambiguated mount path, the
//! pre-existing behavior).

use super::NetworkHost;
#[cfg(target_os = "macos")]
use crate::volumes::{SmbMountInfo, smb_mounts};
#[cfg(target_os = "linux")]
use crate::volumes_linux::{SmbMountInfo, smb_mounts};
use std::collections::HashSet;
use std::net::IpAddr;

/// Returns true when `a` and `b` name the same MACHINE, with `hosts` (the discovery
/// state, `super::fresh_discovered_hosts()`, for a live caller) supplying name ↔ IP
/// equivalence.
///
/// ❗ A machine, ❌ not an SMB server: one machine can run several (the Docker fixtures
/// are ten on `localhost`), so "is this the same server" is [`SmbServer::is`], which
/// also compares the port. Reach for this only where no port exists to compare (a
/// Finder keychain item, a store row written before rows carried one).
pub fn same_machine(a: &str, b: &str, hosts: &[NetworkHost]) -> bool {
    !identifiers(a, hosts).is_disjoint(&identifiers(b, hosts))
}

/// An SMB server: the host it dials and the port. The one thing "which server is
/// this" compares, so a port-blind answer can't be written by accident.
///
/// ❗ Never a display name: a name a person typed is a label, and matching on it split
/// a renamed host in two (QA 2026-09-25). A discovery name (`localhost:11482`) is fine
/// to read one from ([`SmbServer::from_name`]): it IS the host and port, spelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbServer {
    host: String,
    port: u16,
}

impl SmbServer {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
        }
    }

    /// Reads a discovery name or label: `host` is on 445, `host:port` and `[v6]:port`
    /// carry their port. A bare IPv6 literal is on 445.
    pub fn from_name(name: &str) -> Self {
        match split_port(name) {
            Some((host, port)) => Self::new(host, port),
            None => Self::new(name, SMB_PORT),
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether `other` is this server: the same port, and the same machine under any
    /// name it goes by.
    pub fn is(&self, other: &SmbServer, hosts: &[NetworkHost]) -> bool {
        self.port == other.port && same_machine(&self.host, &other.host, hosts)
    }
}

/// Every name `host` goes by (its hostname, its IP, the host half of its discovery
/// name), each on ITS port: what a mount from it can be recognized by.
pub fn smb_servers_of(host: &NetworkHost) -> Vec<SmbServer> {
    let named = SmbServer::from_name(&host.name);
    host.hostname
        .iter()
        .chain(host.ip_address.iter())
        .map(String::as_str)
        .chain(std::iter::once(named.host()))
        .map(|name| SmbServer::new(name, host.port))
        .collect()
}

/// Whether the SMB mount `info` is from one of `targets`: the same server by
/// [`SmbServer::is`], so the port has to match and the machine may go by any name
/// discovery pairs it with.
pub fn mount_is_from(info: &SmbMountInfo, targets: &[SmbServer], hosts: &[NetworkHost]) -> bool {
    let mounted = SmbServer::new(&info.server, info.port);
    targets.iter().any(|target| target.is(&mounted, hosts))
}

/// The mount points of every SMB mount from one of `targets`, read off the kernel's
/// mount table snapshot (`volumes::smb_mounts`): no call per mount, so a hung share
/// can't stall it, and it answers in time to decide what a menu offers.
pub fn smb_mounts_from(targets: &[SmbServer]) -> Vec<String> {
    let hosts = super::fresh_discovered_hosts();
    smb_mounts()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, info)| mount_is_from(info, targets, &hosts))
        .map(|(mount_point, _)| mount_point)
        .collect()
}

/// The share of every SMB mount from one of `targets`, off the same mount-table snapshot
/// as [`smb_mounts_from`].
pub fn smb_shares_mounted_from(targets: &[SmbServer]) -> Vec<String> {
    let hosts = super::fresh_discovered_hosts();
    smb_mounts()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, info)| mount_is_from(info, targets, &hosts))
        .map(|(_, info)| info.share)
        .collect()
}

/// Lowercases, NFC-folds, and strips the trailing dot of a fully qualified name.
///
/// The NFC fold pairs the spellings one accented server name arrives in: composed
/// from mDNS, decomposed from `statfs`. Without it the two forms are two identities,
/// and a password saved under one is never found under the other.
fn normalize(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    s.trim_end_matches('.').nfc().flat_map(char::to_lowercase).collect()
}

/// The stable key for a server's stored credentials.
///
/// The same NAS reaches the credential store under different names depending on the
/// caller: the frontend saves under the mDNS instance name (`Naspolya`), while the
/// OS-mount upgrade path looks up by the `statfs` server (`Naspolya._smb._tcp.local`)
/// or the resolved hostname (`naspolya.local`). Keying credentials on the raw string
/// splits one server's password across several entries, so a password saved on mount
/// is never found on the next connect. `credential_key` collapses every name form to
/// the same bare identity (IP literals pass through unchanged — they have no bare form).
///
/// ❗ A PORT off 445 is part of the identity and stays on the key (`nas:11482`), in
/// whichever form it arrives: the discovery name a sheet saves under
/// (`nas.local:11482`) and [`smb_server`]'s host + port from a mount fold to the same
/// key. One machine can run several SMB servers, each with its own accounts. On 445
/// (or with no port) the key is exactly the bare name, as it always was.
pub fn credential_key(server: &str) -> String {
    let normalized = normalize(server);
    match split_port(&normalized) {
        Some((host, port)) if port != SMB_PORT => format!("{}:{port}", bare_name(host)),
        Some((host, _)) => bare_name(host),
        None => bare_name(&normalized),
    }
}

/// SMB's own port, which a server name leaves unsaid.
const SMB_PORT: u16 = 445;

/// How an SMB server with a port is spelled for [`credential_key`]: the host on 445,
/// `host:port` off it (`[v6]:port` for an IPv6 literal, whose own colons would
/// otherwise read as a port).
pub fn smb_server(host: &str, port: u16) -> String {
    match port {
        SMB_PORT => host.to_string(),
        _ if host.contains(':') => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    }
}

/// `host:port` or `[v6]:port` into its halves. A bare IPv6 literal has no port to
/// split (its last group would read as one), so only the bracketed form does.
fn split_port(s: &str) -> Option<(&str, u16)> {
    if let Some(rest) = s.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        return Some((host, port.parse().ok()?));
    }
    let (host, port) = s.rsplit_once(':')?;
    if host.is_empty() || host.contains(':') {
        return None;
    }
    Some((host, port.parse().ok()?))
}

/// Extracts the bare name from any of the forms a server name arrives in:
/// `Naspolya._smb._tcp.local` → `naspolya`, `naspolya.local` → `naspolya`,
/// `naspolya` → `naspolya`. Input must already be normalized.
fn bare_name(normalized: &str) -> String {
    if normalized.contains("._tcp") || normalized.contains("._udp") {
        // mDNS service name: the instance label is everything before the first
        // service label (`._smb`, `._afpovertcp`, ...).
        if let Some(instance) = normalized.split("._").next() {
            return instance.to_string();
        }
    }
    normalized.trim_end_matches(".local").to_string()
}

/// The set of identifiers a server string is known by: its normalized form, its bare
/// name, and (via the discovery state) the IP ↔ name pairings.
fn identifiers(s: &str, hosts: &[NetworkHost]) -> HashSet<String> {
    let normalized = normalize(s);
    let mut ids = HashSet::new();

    if normalized.parse::<IpAddr>().is_ok() {
        // IP literal: add the IP, plus every name form of the discovered host
        // carrying that IP.
        ids.insert(normalized.clone());
        for host in hosts {
            if host.ip_address.as_deref().map(normalize) == Some(normalized.clone()) {
                ids.insert(bare_name(&normalize(&host.name)));
                if let Some(hostname) = &host.hostname {
                    ids.insert(bare_name(&normalize(hostname)));
                }
            }
        }
    } else {
        // Name form: add the bare name, plus the IP of the discovered host whose
        // name or hostname matches it.
        let bare = bare_name(&normalized);
        ids.insert(bare.clone());
        for host in hosts {
            let host_names = [
                Some(bare_name(&normalize(&host.name))),
                host.hostname.as_deref().map(|h| bare_name(&normalize(h))),
            ];
            if host_names.into_iter().flatten().any(|n| n == bare)
                && let Some(ip) = &host.ip_address
            {
                ids.insert(normalize(ip));
            }
        }
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::HostSource;

    /// An accented server name reaches us composed from mDNS and decomposed from
    /// `statfs`, and the credential key is what pairs the two. Reported as ERR-ABXW4.
    #[test]
    fn credential_key_folds_unicode_normalization() {
        assert_eq!(
            credential_key("Caf\u{e9}-NAS.local"),
            credential_key("Cafe\u{301}-NAS.local")
        );
    }

    fn naspolya() -> NetworkHost {
        NetworkHost {
            id: "naspolya-smb-tcp-local".to_string(),
            name: "Naspolya".to_string(),
            hostname: Some("Naspolya.local".to_string()),
            ip_address: Some("192.168.1.111".to_string()),
            port: 445,
            source: HostSource::Discovered,
        }
    }

    fn raspberrypi() -> NetworkHost {
        NetworkHost {
            id: "raspberrypi-smb-tcp-local".to_string(),
            name: "raspberrypi".to_string(),
            hostname: Some("raspberrypi.local".to_string()),
            ip_address: Some("192.168.1.150".to_string()),
            port: 445,
            source: HostSource::Discovered,
        }
    }

    /// The incident case: Cmdr mounts by IP while `statfs` reports the existing mount
    /// by mDNS service name. These MUST compare equal, otherwise the mount path
    /// disambiguation treats the same NAS as a second server and forces a doomed
    /// second session.
    #[test]
    fn test_ip_matches_mdns_service_name_via_discovery() {
        let hosts = [naspolya(), raspberrypi()];
        assert!(same_machine("192.168.1.111", "Naspolya._smb._tcp.local", &hosts));
        assert!(same_machine("Naspolya._smb._tcp.local", "192.168.1.111", &hosts));
        assert!(same_machine("192.168.1.111", "naspolya.local", &hosts));
        assert!(same_machine("192.168.1.111", "Naspolya", &hosts));
    }

    #[test]
    fn test_different_servers_stay_different() {
        let hosts = [naspolya(), raspberrypi()];
        assert!(!same_machine("192.168.1.150", "Naspolya._smb._tcp.local", &hosts));
        assert!(!same_machine("192.168.1.111", "192.168.1.150", &hosts));
        assert!(!same_machine("raspberrypi.local", "naspolya.local", &hosts));
    }

    /// Name-form equivalence needs no discovery data: all name shapes of the same
    /// instance normalize to the same bare name.
    #[test]
    fn test_name_forms_match_without_discovery() {
        assert!(same_machine("NASPOLYA.local", "naspolya._smb._tcp.local", &[]));
        assert!(same_machine("Naspolya", "naspolya.local.", &[]));
        assert!(same_machine("localhost", "LOCALHOST", &[]));
    }

    /// Without discovery data, an IP and a name can't be proven equivalent. Treating
    /// them as different servers is the safe fallback (worst case: a disambiguated
    /// mount path, which is the pre-existing behavior).
    #[test]
    fn test_ip_vs_name_unknown_without_discovery() {
        assert!(!same_machine("192.168.1.111", "naspolya._smb._tcp.local", &[]));
    }

    #[test]
    fn test_exact_strings_always_match() {
        assert!(same_machine("192.168.1.111", "192.168.1.111", &[]));
        assert!(same_machine("some-nas", "some-nas", &[]));
    }

    /// Every name form of one server must produce the SAME credential key, so a password
    /// saved by the frontend (mDNS instance name) is found by the upgrade path (`statfs`
    /// service name / resolved hostname). This is the keying split-brain that left a
    /// just-saved Naspolya password unusable on the next connect.
    #[test]
    fn test_credential_key_collapses_name_forms() {
        assert_eq!(credential_key("Naspolya"), "naspolya");
        assert_eq!(credential_key("Naspolya.local"), "naspolya");
        assert_eq!(credential_key("Naspolya.local."), "naspolya");
        assert_eq!(credential_key("Naspolya._smb._tcp.local"), "naspolya");
        assert_eq!(credential_key("Naspolya._smb._tcp.local."), "naspolya");
    }

    /// ❗ A port off 445 is part of the key, in every form it arrives in; on 445 the
    /// key is exactly today's, so nothing saved on a standard NAS moves.
    #[test]
    fn credential_key_keeps_a_port_off_445_and_drops_445() {
        assert_eq!(credential_key("nas.local:11482"), "nas:11482");
        assert_eq!(credential_key(&smb_server("Nas", 11482)), "nas:11482");
        assert_eq!(
            credential_key(&smb_server("nas.local", 445)),
            credential_key("nas.local")
        );
        assert_eq!(credential_key("nas:445"), "nas");
        assert_eq!(credential_key(&smb_server("fe80::1", 11482)), "fe80::1:11482");
        // A bare IPv6 literal has no port: its last group is not one.
        assert_eq!(credential_key("fe80::1"), "fe80::1");
    }

    /// IP literals have no bare form; they pass through (lowercased) unchanged so two
    /// different hosts can't collide on one credential entry.
    #[test]
    fn test_credential_key_passes_ip_through() {
        assert_eq!(credential_key("192.168.1.111"), "192.168.1.111");
        assert_eq!(credential_key("localhost"), "localhost");
    }
}

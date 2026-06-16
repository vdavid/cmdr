//! Server identity equivalence for SMB.
//!
//! The same physical server shows up under different names depending on who reports it:
//! mDNS advertises `Naspolya._smb._tcp.local`, DNS knows `naspolya.local`, Cmdr mounts
//! by IP (`192.168.1.111`), and `statfs` echoes back whichever form the mount used.
//! Comparing these as strings treats one server as three, which made
//! `disambiguated_mount_path` mount a second copy of an already-mounted share with
//! `ForceNewSession` (fresh auth, guest, dead end) instead of reusing the existing one.
//!
//! `same_server` derives an identifier set for each input (normalized name forms plus
//! IP, enriched from the mDNS discovery state) and calls two inputs the same server when
//! the sets intersect. When discovery knows nothing, two different-looking strings stay
//! different: that's the safe direction (worst case is a disambiguated mount path, the
//! pre-existing behavior).

use super::NetworkHost;
use std::collections::HashSet;
use std::net::IpAddr;

/// Returns true when `a` and `b` refer to the same server, consulting the live mDNS
/// discovery state for name ↔ IP equivalence.
///
/// macOS-only: the NetFS mount-dedup path (`mount.rs`) has no host list at hand, so it
/// needs this self-fetching wrapper. The Linux `gio mount` path and `smb_upgrade` already
/// hold a `NetworkHost` slice and call [`same_server`] directly.
#[cfg(target_os = "macos")]
pub fn same_server_live(a: &str, b: &str) -> bool {
    same_server(a, b, &super::get_discovered_hosts())
}

/// Pure core of [`same_server_live`]: equivalence against an explicit host list.
pub fn same_server(a: &str, b: &str, hosts: &[NetworkHost]) -> bool {
    !identifiers(a, hosts).is_disjoint(&identifiers(b, hosts))
}

/// Lowercases and strips the trailing dot of a fully qualified name.
fn normalize(s: &str) -> String {
    s.trim_end_matches('.').to_lowercase()
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
pub fn credential_key(server: &str) -> String {
    bare_name(&normalize(server))
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
        assert!(same_server("192.168.1.111", "Naspolya._smb._tcp.local", &hosts));
        assert!(same_server("Naspolya._smb._tcp.local", "192.168.1.111", &hosts));
        assert!(same_server("192.168.1.111", "naspolya.local", &hosts));
        assert!(same_server("192.168.1.111", "Naspolya", &hosts));
    }

    #[test]
    fn test_different_servers_stay_different() {
        let hosts = [naspolya(), raspberrypi()];
        assert!(!same_server("192.168.1.150", "Naspolya._smb._tcp.local", &hosts));
        assert!(!same_server("192.168.1.111", "192.168.1.150", &hosts));
        assert!(!same_server("raspberrypi.local", "naspolya.local", &hosts));
    }

    /// Name-form equivalence needs no discovery data: all name shapes of the same
    /// instance normalize to the same bare name.
    #[test]
    fn test_name_forms_match_without_discovery() {
        assert!(same_server("NASPOLYA.local", "naspolya._smb._tcp.local", &[]));
        assert!(same_server("Naspolya", "naspolya.local.", &[]));
        assert!(same_server("localhost", "LOCALHOST", &[]));
    }

    /// Without discovery data, an IP and a name can't be proven equivalent. Treating
    /// them as different servers is the safe fallback (worst case: a disambiguated
    /// mount path, which is the pre-existing behavior).
    #[test]
    fn test_ip_vs_name_unknown_without_discovery() {
        assert!(!same_server("192.168.1.111", "naspolya._smb._tcp.local", &[]));
    }

    #[test]
    fn test_exact_strings_always_match() {
        assert!(same_server("192.168.1.111", "192.168.1.111", &[]));
        assert!(same_server("some-nas", "some-nas", &[]));
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

    /// IP literals have no bare form; they pass through (lowercased) unchanged so two
    /// different hosts can't collide on one credential entry.
    #[test]
    fn test_credential_key_passes_ip_through() {
        assert_eq!(credential_key("192.168.1.111"), "192.168.1.111");
        assert_eq!(credential_key("localhost"), "localhost");
    }
}

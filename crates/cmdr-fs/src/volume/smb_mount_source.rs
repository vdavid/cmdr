//! The authority half of an SMB mount source: `user:password@host:port`.
//!
//! Both platform twins read a mount source (macOS `statfs.f_mntfromname`, Linux
//! the `/proc/mounts` device field) and split it the same way, so the split
//! lives here once. What differs stays in each twin: macOS records the source
//! percent-escaped and decodes each field after the split.
//!
//! The two forms that make this more than a `rsplit_once(':')`: an IPv6 host is
//! bracketed, and a guest mount records its empty password, as in
//! `//guest:@[::1]:18445/public` (verified on macOS 26 via `mount`, after
//! `mount_smbfs -N` through an `[::1]` forwarder to the guest fixture,
//! 2026-09-11).

/// A mount source's authority, split into the parts a caller keys and dials on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmbAuthority<'a> {
    /// The user name, without the password that may follow it after a `:`.
    pub username: Option<&'a str>,
    /// The host, with an IPv6 literal's brackets taken off: `smb_volume_id` keys
    /// on this and the smb2 dialer takes it unbracketed.
    pub host: &'a str,
    /// The port, when the source names one that parses.
    pub port: Option<u16>,
}

/// Splits `authority` (everything between the leading `//` and the share) into
/// user, host, and port.
///
/// A bare IPv6 literal (more than one colon, no brackets) can't carry a port, so
/// its colons all stay in the host.
pub fn split_authority(authority: &str) -> SmbAuthority<'_> {
    let (username, host_port) = match authority.rsplit_once('@') {
        Some((user_info, host_port)) => (Some(user_info.split_once(':').map_or(user_info, |(user, _)| user)), host_port),
        None => (None, authority),
    };

    if let Some(bracketed) = host_port.strip_prefix('[')
        && let Some((host, after)) = bracketed.split_once(']')
    {
        let port = after.strip_prefix(':').and_then(|port| port.parse().ok());
        return SmbAuthority { username, host, port };
    }

    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, _)) if host.contains(':') => (host_port, None),
        Some((host, port)) => (host, port.parse().ok()),
        None => (host_port, None),
    };
    SmbAuthority { username, host, port }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority(username: Option<&'static str>, host: &'static str, port: Option<u16>) -> SmbAuthority<'static> {
        SmbAuthority { username, host, port }
    }

    #[test]
    fn a_named_user_and_an_ipv4_host() {
        assert_eq!(split_authority("david@192.168.1.111"), authority(Some("david"), "192.168.1.111", None));
    }

    #[test]
    fn a_host_with_a_port_and_no_user() {
        assert_eq!(split_authority("localhost:11480"), authority(None, "localhost", Some(11480)));
    }

    /// The guest form macOS records: the empty password is not part of the name.
    #[test]
    fn the_empty_guest_password_is_not_part_of_the_username() {
        assert_eq!(split_authority("guest:@localhost:11484"), authority(Some("guest"), "localhost", Some(11484)));
    }

    #[test]
    fn a_bracketed_ipv6_host_with_a_port_comes_back_unbracketed() {
        assert_eq!(split_authority("guest:@[::1]:18445"), authority(Some("guest"), "::1", Some(18445)));
    }

    #[test]
    fn a_bracketed_ipv6_host_without_a_port() {
        assert_eq!(split_authority("[fe80::1]"), authority(None, "fe80::1", None));
    }

    /// Splitting on the last colon would read `1` as the port and `fe80:` as the host.
    #[test]
    fn a_bare_ipv6_literal_keeps_every_colon() {
        assert_eq!(split_authority("fe80::1"), authority(None, "fe80::1", None));
    }

    #[test]
    fn a_port_that_does_not_parse_is_no_port() {
        assert_eq!(split_authority("nas:smb"), authority(None, "nas", None));
    }
}

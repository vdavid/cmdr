//! Reaching an SMB server: the address string, the two share-listing calls,
//! asking whether an identity may open one share, and asking which server it is.

use crate::volume::SmbConnectionParams;
use cmdr_fs::log_private::LogPrivate;
use log::debug;
use smb2::{ClientConfig, SmbClient};
use std::time::Duration;

/// The smb2 address to dial `hostname` (an IP or a host name) on `port`.
///
/// smb2 uses ONE string for both the TCP dial and the UNC server name
/// (`\\server\IPC$`), and some servers reject a `.local` name there. So an mDNS
/// `.local` name (`nas.local`, `nas.local.`) is resolved here, the system resolver
/// answering it through mDNS, and dialed by its IPv4 address, which every server
/// accepts in a UNC path (the discovered-IP path has always done this).
///
/// ❗ Never dials the `.local`-stripped name when the name resolves (#912): `nas`
/// alone isn't an mDNS name, so a typed `nas.local` with no discovered IP never
/// connected. A name with only an IPv6 address is dialed as typed (a link-local
/// address needs its scope, which an addr string can't carry); one that doesn't
/// resolve at all falls back to the stripped name, which unicast DNS may know.
pub async fn smb_addr(hostname: &str, port: u16) -> String {
    smb_addr_with(
        hostname,
        port,
        |host, port| async move { resolve_ipv4(&host, port).await },
    )
    .await
}

/// How long the system resolver may take to answer a `.local` name. mDNS answers
/// in milliseconds when the host is up; a host that's gone costs this once per dial.
const LOCAL_NAME_RESOLVE_LIMIT: Duration = Duration::from_secs(5);

/// [`smb_addr`] over a resolver, so a test needs no network.
async fn smb_addr_with<F, Fut>(hostname: &str, port: u16, resolve: F) -> String
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = Resolved>,
{
    let Some(stripped) = mdns_host(hostname) else {
        return format!("{hostname}:{port}");
    };
    match resolve(hostname.to_string(), port).await {
        Resolved::V4(ip) => format!("{ip}:{port}"),
        Resolved::V6Only => format!("{hostname}:{port}"),
        Resolved::Nothing => format!("{stripped}:{port}"),
    }
}

/// What the system resolver said about a `.local` name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolved {
    V4(std::net::Ipv4Addr),
    V6Only,
    Nothing,
}

/// The host half of an mDNS `.local` name (`nas` for `nas.local` and `nas.local.`),
/// or `None` for any other name or an IP.
fn mdns_host(hostname: &str) -> Option<&str> {
    let name = hostname.strip_suffix('.').unwrap_or(hostname);
    let cut = name.len().checked_sub(".local".len())?;
    let (host, suffix) = name.split_at_checked(cut)?;
    (suffix.eq_ignore_ascii_case(".local") && !host.is_empty()).then_some(host)
}

/// Asks the system resolver for `hostname`, bounded by [`LOCAL_NAME_RESOLVE_LIMIT`].
async fn resolve_ipv4(hostname: &str, port: u16) -> Resolved {
    let lookup = tokio::net::lookup_host((hostname, port));
    let Ok(Ok(addrs)) = tokio::time::timeout(LOCAL_NAME_RESOLVE_LIMIT, lookup).await else {
        debug!(
            "smb_addr: host={:?} didn't resolve; dialing it without .local",
            LogPrivate::host(hostname)
        );
        return Resolved::Nothing;
    };
    let addrs: Vec<std::net::SocketAddr> = addrs.collect();
    addrs
        .iter()
        .find_map(|addr| match addr.ip() {
            std::net::IpAddr::V4(ip) => Some(Resolved::V4(ip)),
            std::net::IpAddr::V6(_) => None,
        })
        .unwrap_or(if addrs.is_empty() {
            Resolved::Nothing
        } else {
            Resolved::V6Only
        })
}

/// How long NEGOTIATE and SESSION_SETUP may stay silent before a volume's connect
/// gives up with a typed `Error::Timeout`.
///
/// Both are answered from the server's memory, so 10 s is generous for a busy or
/// slow NAS; a spun-down disk is woken by the first CREATE or TreeConnect, which
/// run on smb2's own 30 s deadline (see [`connect_volume_client`]). It's sized
/// against the transfer outage wait's 15 s per-attempt bound: a server that
/// accepts TCP and never answers fails here at ~10 s, and a dead host fails the
/// 10 s TCP connect, so either way the attempt ends on its own with a real error
/// and the outer bound stays a safety net. A give-up that was too eager costs one
/// retry against a server that's warm by then. The ordering is pinned by
/// `tests::the_handshake_deadline_ends_an_attempt_before_the_transfer_abandons_it`.
pub(crate) const HANDSHAKE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Connects a volume's long-lived client with the handshake bounded by
/// [`HANDSHAKE_RESPONSE_TIMEOUT`], then hands the session smb2's default request
/// deadline for everything after, TreeConnect included.
///
/// smb2 has one deadline knob per connection, and `ClientConfig::response_timeout`
/// is the only way to reach the NEGOTIATE inside `SmbClient::connect`, hence the
/// swap. Without it, a server that accepts the TCP connection and never answers
/// holds the caller (and a reconnect's `reconnect_lock`) for the full 30 s.
pub(crate) async fn connect_volume_client(config: ClientConfig) -> Result<SmbClient, smb2::Error> {
    connect_volume_client_within(config, HANDSHAKE_RESPONSE_TIMEOUT).await
}

/// [`connect_volume_client`] with the handshake deadline as a parameter, so a test
/// can drive it in milliseconds.
async fn connect_volume_client_within(mut config: ClientConfig, handshake: Duration) -> Result<SmbClient, smb2::Error> {
    config.response_timeout = Some(handshake);
    let client = SmbClient::connect(config).await?;
    client
        .connection()
        .set_response_timeout(ClientConfig::default().response_timeout);
    Ok(client)
}

/// Determines the server address string for smb2, preferring a discovered IP over
/// a hostname, which goes through [`smb_addr`].
async fn build_addr(hostname: &str, ip_address: Option<&str>, port: u16) -> String {
    match ip_address {
        Some(ip) => format!("{}:{}", ip, port),
        None => smb_addr(hostname, port).await,
    }
}

/// Attempts to list shares as guest (anonymous).
pub async fn try_list_shares_as_guest(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    timeout: Duration,
) -> Result<Vec<smb2::ShareInfo>, smb2::Error> {
    let addr = build_addr(hostname, ip_address, port).await;
    debug!(
        "try_list_shares_as_guest: host={:?}, port={port}",
        LogPrivate::host(ip_address.unwrap_or(hostname))
    );

    let config = ClientConfig {
        addr,
        timeout,
        username: "Guest".to_string(),
        password: String::new(),
        domain: String::new(),
        auto_reconnect: false,
        compression: false,
        // Share enumeration never leaves `IPC$`, so there is no share name for
        // a referral to resolve. Off, so a listing can't spend round-trips on
        // it.
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
        // The listing has no session to keep, so `timeout` bounds every step,
        // the handshake and `NetShareEnum` included, and a silent server ends
        // in a typed `Error::Timeout` before the caller's own bound cuts it.
        response_timeout: Some(timeout),
        ..Default::default()
    };

    let mut client = SmbClient::connect(config).await?;
    client.list_shares().await
}

/// Attempts to list shares with credentials.
pub async fn try_list_shares_authenticated(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    username: &str,
    password: &str,
    timeout: Duration,
) -> Result<Vec<smb2::ShareInfo>, smb2::Error> {
    let addr = build_addr(hostname, ip_address, port).await;
    debug!(
        "try_list_shares_authenticated: host={:?}, port={port}, user={:?}",
        LogPrivate::host(ip_address.unwrap_or(hostname)),
        LogPrivate::user(username)
    );

    let config = ClientConfig {
        addr,
        timeout,
        username: username.to_string(),
        password: password.to_string(),
        domain: String::new(),
        auto_reconnect: false,
        compression: false,
        // Same as the guest listing above: `IPC$` only, nothing to resolve,
        // and `timeout` bounds every step.
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
        response_timeout: Some(timeout),
        ..Default::default()
    };

    let mut client = SmbClient::connect(config).await?;
    client.list_shares().await
}

/// Asks the server whether `params`' identity may open its share: session setup,
/// one TreeConnect, and a tree disconnect, with nothing kept.
///
/// For a caller holding a vaguer answer from somewhere else, like a kernel mount
/// that could only say "not found", that needs the server's own. The error comes
/// back untouched, because WHICH status arrived at WHICH command is the answer:
/// `STATUS_ACCESS_DENIED` at TreeConnect is a share turning this identity away,
/// `STATUS_BAD_NETWORK_NAME` there is a share that doesn't exist, and a refusal at
/// SessionSetup never got as far as the share.
///
/// `timeout` bounds each protocol step, not the whole exchange; a caller with a
/// hard budget bounds the future too.
pub async fn try_open_share(params: &SmbConnectionParams, timeout: Duration) -> Result<(), smb2::Error> {
    let config = ClientConfig {
        addr: smb_addr(&params.server, params.port).await,
        timeout,
        username: params.username.clone(),
        password: params.password.clone(),
        domain: String::new(),
        auto_reconnect: false,
        compression: false,
        // This probe exists to report what the server really says about the
        // share, so it has to reach a DFS namespace root the same way
        // `session_build::build_session` does. Without it a namespace answers
        // `STATUS_BAD_NETWORK_NAME` and the probe reports "no such share"
        // about a share the caller can actually open.
        dfs_enabled: true,
        dfs_target_overrides: Default::default(),
        dfs_target_policy: crate::link_consent::dfs_target_policy(),
        // What makes "`timeout` bounds each protocol step" hold for the
        // handshake and TreeConnect, not only the TCP connect.
        response_timeout: Some(timeout),
        ..Default::default()
    };

    let mut client = SmbClient::connect(config).await?;
    let tree = client.connect_share(&params.share_name).await?;
    // Courtesy only: the answer is already in, and the socket closing on drop ends
    // the tree anyway.
    if let Err(e) = client.disconnect_share(&tree).await {
        debug!(
            "try_open_share: tree disconnect from share={:?} didn't go through: {}",
            LogPrivate::volume(&params.share_name),
            e
        );
    }
    Ok(())
}

/// Asks the server at `hostname:port` for its GUID with a NEGOTIATE alone: no
/// session setup, no credentials, nothing kept.
///
/// For a saved share recorded before the app kept server GUIDs, which pairs one
/// with the share name to prove two addresses reach one share. ❗ What answers on a
/// saved address isn't proven to be the server the share was saved from (a hotel
/// network can hand the same LAN IP to anything), so the app only FILLS a missing
/// GUID with this, ❌ never overwrites one. The GUID comes back as the server sent
/// it, all zeros included: what an all-zero one means is the caller's rule.
///
/// `timeout` bounds the TCP connect and the NEGOTIATE reply each, through smb2's
/// own deadlines.
pub async fn probe_server_guid(hostname: &str, port: u16, timeout: Duration) -> Result<smb2::pack::Guid, smb2::Error> {
    let mut connection = smb2::client::Connection::connect(&smb_addr(hostname, port).await, timeout).await?;
    connection.set_response_timeout(Some(timeout));
    connection.negotiate().await?;
    // `negotiate` stores the params before it returns `Ok`.
    Ok(connection
        .params()
        .map_or(smb2::pack::Guid::ZERO, |params| params.server_guid))
}

#[cfg(test)]
#[path = "connection_integration_test.rs"]
mod integration_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_discovered_ip_is_dialed_on_any_port() {
        assert_eq!(
            build_addr("nas.local", Some("192.168.1.50"), 445).await,
            "192.168.1.50:445"
        );
        assert_eq!(build_addr("nas.local", Some("10.0.0.5"), 9445).await, "10.0.0.5:9445");
    }

    /// A resolver that answers `answer`, and panics if a name that isn't `.local`
    /// reaches it.
    fn resolver(answer: Resolved) -> impl FnOnce(String, u16) -> std::future::Ready<Resolved> {
        move |host, _| {
            assert!(mdns_host(&host).is_some(), "only a .local name is resolved, got {host}");
            std::future::ready(answer)
        }
    }

    /// ❗ **A typed `.local` name with no discovered IP dials its resolved address**
    /// (#912), ❌ never the stripped name: `nas` alone isn't an mDNS name, so the TCP
    /// connect failed and the server never connected. The IPv4 address also keeps
    /// `.local` out of the UNC path, which some servers reject.
    #[tokio::test]
    async fn a_local_name_dials_its_resolved_ipv4_address() {
        let ip = std::net::Ipv4Addr::new(192, 168, 1, 50);
        for typed in ["nas.local", "nas.local.", "NAS.LOCAL"] {
            assert_eq!(
                smb_addr_with(typed, 11481, resolver(Resolved::V4(ip))).await,
                "192.168.1.50:11481",
                "{typed}"
            );
        }
    }

    /// A `.local` name with only an IPv6 address is dialed as typed (an addr string
    /// can't carry a link-local scope); one nothing resolves falls back to the
    /// stripped name, which unicast DNS may know.
    #[tokio::test]
    async fn a_local_name_without_ipv4_is_dialed_as_typed_or_stripped() {
        assert_eq!(
            smb_addr_with("nas.local", 445, resolver(Resolved::V6Only)).await,
            "nas.local:445"
        );
        assert_eq!(
            smb_addr_with("nas.local", 445, resolver(Resolved::Nothing)).await,
            "nas:445"
        );
    }

    /// Anything that isn't a `.local` name goes out exactly as given, without a lookup.
    #[tokio::test]
    async fn other_names_and_ips_dial_as_given() {
        for (typed, addr) in [
            ("nas", "nas:445"),
            ("192.168.1.50", "192.168.1.50:445"),
            ("files.example.com", "files.example.com:445"),
            ("my.localhost", "my.localhost:445"),
            (".local", ".local:445"),
        ] {
            let no_lookup = |host: String, _: u16| -> std::future::Ready<Resolved> { panic!("looked up {host}") };
            assert_eq!(smb_addr_with(typed, 445, no_lookup).await, addr);
        }
    }

    /// The real resolver, on a name every machine resolves: `localhost` isn't a
    /// `.local` name, so this pins only that an IP literal answers as itself.
    #[tokio::test]
    async fn the_system_resolver_answers_an_ip_literal() {
        assert_eq!(
            resolve_ipv4("127.0.0.1", 445).await,
            Resolved::V4(std::net::Ipv4Addr::LOCALHOST)
        );
    }

    /// Regression anchor for `#406`: a server that accepts the TCP connection and
    /// never answers NEGOTIATE (a NAS whose `smbd` is still starting, a middlebox
    /// swallowing the frame) used to hold `build_session` forever, and with it
    /// `reconnect_lock` for every caller queued behind it. The connect has to give
    /// up ON ITS OWN, with a typed error, at the handshake deadline.
    #[tokio::test]
    async fn a_volume_connect_to_a_silent_server_gives_up_at_the_handshake_deadline() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        // Accept and hold every connection open without a byte of reply.
        let silent = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });

        let config = ClientConfig {
            addr,
            ..Default::default()
        };
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            connect_volume_client_within(config, Duration::from_millis(300)),
        )
        .await;
        silent.abort();

        match outcome {
            Ok(Err(e)) => assert!(matches!(e, smb2::Error::Timeout), "expected a typed timeout, got {e:?}"),
            Ok(Ok(_)) => panic!("a server that never answers can't produce a session"),
            Err(_) => panic!("the connect was still waiting on a silent server after 5 s"),
        }
    }

    /// The transfer's outage wait abandons a reconnect attempt after 15 s
    /// (`OutagePolicy::attempt_timeout` in the app). The handshake deadline has to
    /// end a silent-server attempt first, so the wait sees a real error and its
    /// own bound stays a safety net.
    #[test]
    fn the_handshake_deadline_ends_an_attempt_before_the_transfer_abandons_it() {
        const TRANSFER_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);
        assert!(HANDSHAKE_RESPONSE_TIMEOUT < TRANSFER_ATTEMPT_TIMEOUT);
    }
}

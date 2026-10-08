//! The Add server probe: a TCP connect to the SMB port before a typed host is saved,
//! and what its failure suggests checking besides the server.
//! A child of `manual_servers`, whose `AddServerError` it answers with.

use std::net::{IpAddr, SocketAddr};

use log::debug;
use serde::Serialize;

use super::AddServerError;

const REACHABILITY_TIMEOUT_SECS: u64 = 5;

/// What else to check when the reachability probe didn't get through. Word-free:
/// the Add sheet words it under the "couldn't reach" sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UnreachableHint {
    /// This Mac refused the route to a LAN address (`EHOSTUNREACH` /
    /// `ENETUNREACH`), which is also how a stuck macOS Local Network permission
    /// shows (ERR-XGS9X). Only a hint: with no mount to compare against, a server
    /// that's off can answer the same.
    LocalNetworkPermission,
}

/// The hint for a probe that failed with `err` after dialing `peers`: the Local
/// Network permission when the kernel refused the route and every address is on
/// the LAN (RFC 1918, link-local, or IPv6 unique-local), which is all that
/// permission gates. Read by io kind, ❌ never the message.
pub(super) fn unreachable_hint(err: &std::io::Error, peers: &[SocketAddr]) -> Option<UnreachableHint> {
    use std::io::ErrorKind as Io;
    let refused_route = matches!(err.kind(), Io::HostUnreachable | Io::NetworkUnreachable);
    let on_lan = |peer: &SocketAddr| match peer.ip() {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
    };
    (refused_route && !peers.is_empty() && peers.iter().all(on_lan)).then_some(UnreachableHint::LocalNetworkPermission)
}

/// Checks that the host:port is reachable via TCP with a timeout, answering
/// [`AddServerError::Unreachable`] (with a hint when the failure points at one)
/// when it isn't.
pub async fn check_reachability(host: &str, port: u16) -> Result<(), AddServerError> {
    use tokio::net::TcpStream;
    use tokio::time::{Duration, timeout};

    let addr = format!("{}:{}", host, port);
    debug!("Checking TCP reachability: host={host:?}, port={port}");
    let unreachable = |message: String, hint| AddServerError::Unreachable { message, hint };

    // Resolve first, so the hint can tell which addresses were dialed, then try
    // each in turn, the same thing `TcpStream::connect(&addr)` does internally.
    let probe = async {
        let peers: Vec<SocketAddr> = match tokio::net::lookup_host(&addr).await {
            Ok(found) => found.collect(),
            // A name that doesn't resolve dialed nothing: no peers to hint about.
            Err(e) => return Err((e, Vec::new())),
        };
        match TcpStream::connect(&peers[..]).await {
            Ok(_stream) => Ok(()),
            Err(e) => Err((e, peers)),
        }
    };
    match timeout(Duration::from_secs(REACHABILITY_TIMEOUT_SECS), probe).await {
        Ok(Ok(())) => {
            debug!("Reachable: host={host:?}, port={port}");
            Ok(())
        }
        Ok(Err((e, peers))) => {
            let hint = unreachable_hint(&e, &peers);
            debug!(
                "Unreachable: host={:?}, port={}, source=os, error_kind={:?}, code={:?}, hint={:?}, detail={:?}",
                host,
                port,
                e.kind(),
                e.raw_os_error(),
                hint,
                cmdr_fs::log_detail::LogDetail(&e.to_string())
            );
            Err(unreachable(format!("Couldn't reach {}: {}", addr, e), hint))
        }
        Err(_) => {
            debug!("Timed out connecting to host={host:?}, port={port}");
            Err(unreachable(
                format!(
                    "Couldn't reach {}: connection timed out after {}s",
                    addr, REACHABILITY_TIMEOUT_SECS
                ),
                None,
            ))
        }
    }
}

//! Reaching an SMB server: the address string, the two share-listing calls, and
//! asking whether an identity may open one share.

use crate::volume::SmbConnectionParams;
use log::debug;
use smb2::{ClientConfig, SmbClient};
use std::time::Duration;

/// Builds an smb2 address string from a hostname/IP and port.
///
/// Strips `.local` suffix from hostnames because smb2 uses the addr host
/// component in UNC paths (`\\server\IPC$`), and some servers reject `.local`.
pub fn build_smb_addr(hostname: &str, port: u16) -> String {
    let host = hostname.strip_suffix(".local").unwrap_or(hostname);
    format!("{}:{}", host, port)
}

/// Determines the server address string for smb2, preferring an IP over a hostname.
///
/// An IP needs no `.local` handling, so it's formatted directly; a hostname goes
/// through [`build_smb_addr`] for the strip.
fn build_addr(hostname: &str, ip_address: Option<&str>, port: u16) -> String {
    match ip_address {
        Some(ip) => format!("{}:{}", ip, port),
        None => build_smb_addr(hostname, port),
    }
}

/// Attempts to list shares as guest (anonymous).
pub async fn try_list_shares_as_guest(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    timeout: Duration,
) -> Result<Vec<smb2::ShareInfo>, smb2::Error> {
    let addr = build_addr(hostname, ip_address, port);
    debug!("try_list_shares_as_guest: addr={}", addr);

    let config = ClientConfig {
        addr,
        timeout,
        username: "Guest".to_string(),
        password: String::new(),
        domain: String::new(),
        auto_reconnect: false,
        compression: false,
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
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
    let addr = build_addr(hostname, ip_address, port);
    debug!("try_list_shares_authenticated: addr={}, user={}", addr, username);

    let config = ClientConfig {
        addr,
        timeout,
        username: username.to_string(),
        password: password.to_string(),
        domain: String::new(),
        auto_reconnect: false,
        compression: false,
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
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
        addr: build_smb_addr(&params.server, params.port),
        timeout,
        username: params.username.clone(),
        password: params.password.clone(),
        domain: String::new(),
        auto_reconnect: false,
        compression: false,
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
    };

    let mut client = SmbClient::connect(config).await?;
    let tree = client.connect_share(&params.share_name).await?;
    // Courtesy only: the answer is already in, and the socket closing on drop ends
    // the tree anyway.
    if let Err(e) = client.disconnect_share(&tree).await {
        debug!(
            "try_open_share: tree disconnect from {} didn't go through: {}",
            params.share_name, e
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "connection_integration_test.rs"]
mod integration_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_addr_with_ip() {
        assert_eq!(build_addr("nas.local", Some("192.168.1.50"), 445), "192.168.1.50:445");
    }

    #[test]
    fn test_build_addr_strips_local_suffix() {
        assert_eq!(build_addr("nas.local", None, 445), "nas:445");
    }

    #[test]
    fn test_build_addr_no_local_suffix() {
        assert_eq!(build_addr("nas", None, 445), "nas:445");
    }

    /// `build_smb_addr` is what the SMB backend calls to reach a server, so the
    /// `.local` strip has to hold on the PUBLIC entry point, not only on the
    /// private variant the share-listing path uses.
    #[test]
    fn build_smb_addr_strips_local_and_keeps_the_port_separate() {
        assert_eq!(build_smb_addr("nas.local", 445), "nas:445");
        assert_eq!(build_smb_addr("nas", 445), "nas:445");
        assert_eq!(build_smb_addr("192.168.1.50", 10480), "192.168.1.50:10480");
    }

    #[test]
    fn test_build_addr_non_standard_port() {
        assert_eq!(build_addr("nas.local", Some("10.0.0.5"), 9445), "10.0.0.5:9445");
    }
}

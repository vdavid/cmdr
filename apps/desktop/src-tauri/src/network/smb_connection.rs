//! SMB connection establishment utilities.
//!
//! Provides functions for connecting to SMB servers and listing shares
//! using the smb2 library (pure Rust implementation).

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

/// Determines the server address string for smb2.
/// Prefers IP address over hostname. Strips `.local` suffix from hostnames
/// because smb2 uses the addr host component in UNC paths (`\\server\IPC$`),
/// and some servers reject `.local` in UNC paths.
fn build_addr(hostname: &str, ip_address: Option<&str>, port: u16) -> String {
    let host = if let Some(ip) = ip_address {
        ip.to_string()
    } else {
        hostname.strip_suffix(".local").unwrap_or(hostname).to_string()
    };
    format!("{}:{}", host, port)
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
    };

    let mut client = SmbClient::connect(config).await?;
    client.list_shares().await
}

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

    #[test]
    fn test_build_addr_non_standard_port() {
        assert_eq!(build_addr("nas.local", Some("10.0.0.5"), 9445), "10.0.0.5:9445");
    }
}

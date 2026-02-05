//! SMB connection establishment utilities.
//!
//! Provides functions for establishing SMB connections and listing shares
//! using the smb-rs library (pure Rust implementation).

use log::debug;
use smb::Client;
use smb_rpc::interface::ShareInfo1;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::timeout;

/// Establishes SMB connection and returns the name to use for IPC operations.
/// Connects via IP address when available (preferred), falling back to hostname resolution.
pub async fn establish_smb_connection<'a>(
    client: &Client,
    server_name: &'a str,
    hostname: &'a str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<&'a str, String> {
    if let Some(ip) = ip_address {
        // Use IP address for connection to bypass mDNS resolution issues
        let socket_addr: SocketAddr = format!("{}:{}", ip, port)
            .parse()
            .map_err(|e| format!("Invalid IP {}: {}", ip, e))?;

        debug!(
            "Connecting to server_name='{}' at socket_addr='{}'",
            server_name, socket_addr
        );

        client
            .connect_to_address(server_name, socket_addr)
            .await
            .map_err(|e| format!("Connect to {} failed: {}", ip, e))?;

        debug!(
            "connect_to_address succeeded, now calling ipc_connect with server_name='{}'",
            server_name
        );

        // After connect_to_address, use server_name for IPC (without .local)
        Ok(server_name)
    } else {
        // No IP - try hostname resolution (may fail for .local)
        debug!("No IP address provided, using hostname='{}' for ipc_connect", hostname);
        Ok(hostname)
    }
}

/// Attempts to list shares as guest (anonymous).
/// Connects via IP address when available (preferred), falling back to hostname resolution.
pub async fn try_list_shares_as_guest(
    client: &Client,
    server_name: &str,
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    timeout_duration: Duration,
) -> Result<Vec<ShareInfo1>, String> {
    timeout(timeout_duration, async {
        let connect_name = establish_smb_connection(client, server_name, hostname, ip_address, port).await?;

        // Connect to IPC$ with "Guest" user
        debug!("Calling ipc_connect with connect_name='{}'", connect_name);
        client
            .ipc_connect(connect_name, "Guest", String::new())
            .await
            .map_err(|e| format!("IPC connect failed: {}", e))?;

        // List shares
        client
            .list_shares(connect_name)
            .await
            .map_err(|e| format!("list_shares failed: {}", e))
    })
    .await
    .map_err(|_| format!("Timeout after {}s", timeout_duration.as_secs()))?
}

/// Attempts to list shares with credentials.
/// Connects via IP address when available (preferred), falling back to hostname resolution.
#[allow(clippy::too_many_arguments, reason = "Internal function, parameters are all needed")]
pub async fn try_list_shares_authenticated(
    client: &Client,
    server_name: &str,
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    username: &str,
    password: &str,
    timeout_duration: Duration,
) -> Result<Vec<ShareInfo1>, String> {
    timeout(timeout_duration, async {
        let connect_name = establish_smb_connection(client, server_name, hostname, ip_address, port).await?;

        // Connect to IPC$ with credentials
        client
            .ipc_connect(connect_name, username, password.to_string())
            .await
            .map_err(|e| format!("IPC connect failed: {}", e))?;

        // List shares
        client
            .list_shares(connect_name)
            .await
            .map_err(|e| format!("list_shares failed: {}", e))
    })
    .await
    .map_err(|_| format!("Timeout after {}s", timeout_duration.as_secs()))?
}

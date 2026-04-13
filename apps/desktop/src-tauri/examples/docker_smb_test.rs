//! Quick test for Docker SMB servers with custom ports.
//!
//! Run with:
//!   cargo run --example docker_smb_test
//!
//! NOTE: This example only works on macOS/Linux (requires the `smb2` crate).

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod inner {
    use smb2::{ClientConfig, SmbClient};
    use std::time::Duration;

    const TEST_PORT: u16 = 9445; // smb-guest Docker container
    const TEST_IP: &str = "127.0.0.1";

    #[tokio::main]
    pub async fn main() {
        println!("Testing Docker SMB container at {}:{}", TEST_IP, TEST_PORT);

        let config = ClientConfig {
            addr: format!("{}:{}", TEST_IP, TEST_PORT),
            timeout: Duration::from_secs(5),
            username: "Guest".to_string(),
            password: String::new(),
            domain: String::new(),
            auto_reconnect: false,
            compression: false,
            dfs_enabled: false,
            dfs_target_overrides: Default::default(),
        };

        // Step 1: Connect
        println!("Step 1: Connecting as Guest...");
        let mut client = match SmbClient::connect(config).await {
            Ok(client) => {
                println!("  Connected");
                client
            }
            Err(e) => {
                println!("  Connect failed: {:?}", e);
                return;
            }
        };

        // Step 2: List shares
        println!("Step 2: Listing shares...");
        match client.list_shares().await {
            Ok(shares) => {
                println!("  Found {} shares:", shares.len());
                for share in shares {
                    println!(
                        "    - {} (type={}, comment={:?})",
                        share.name, share.share_type, share.comment
                    );
                }
            }
            Err(e) => {
                println!("  list_shares failed: {:?}", e);
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() {
    inner::main();
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    println!("This example only works on macOS/Linux (requires the `smb2` crate).");
}

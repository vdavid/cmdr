//! Quick test for Docker SMB servers with custom ports.
//!
//! Run with:
//!   cargo run --example docker_smb_test
//!
//! NOTE: This example only works on macOS (requires the `smb` crate).

#[cfg(target_os = "macos")]
mod inner {
    use smb::{Client, ClientConfig};
    use std::net::SocketAddr;

    const TEST_PORT: u16 = 9445; // smb-guest Docker container
    const TEST_IP: &str = "127.0.0.1";

    #[tokio::main]
    pub async fn main() {
        println!("Testing Docker SMB container at {}:{}", TEST_IP, TEST_PORT);

        // Create client with unsigned guest access allowed (required for test Docker servers)
        let mut config = ClientConfig::default();
        config.connection.allow_unsigned_guest_access = true;
        let client = Client::new(config);

        // Step 1: Connect to address with custom port
        // KEY FIX: Use IP address as server name to ensure consistent connection lookup
        let socket_addr: SocketAddr = format!("{}:{}", TEST_IP, TEST_PORT).parse().unwrap();
        println!("Step 1: connect_to_address('{}', {:?})", TEST_IP, socket_addr);

        match client.connect_to_address(TEST_IP, socket_addr).await {
            Ok(_conn) => println!("  ✅ connect_to_address succeeded"),
            Err(e) => {
                println!("  ❌ connect_to_address failed: {:?}", e);
                return;
            }
        }

        // Step 2: Try ipc_connect with the IP address as server name
        println!("Step 2: ipc_connect('{}', 'Guest', '')", TEST_IP);

        match client.ipc_connect(TEST_IP, "Guest", String::new()).await {
            Ok(_) => println!("  ✅ ipc_connect succeeded"),
            Err(e) => {
                println!("  ❌ ipc_connect failed: {:?}", e);

                // Try an alternative approach - let's see if the connection is really established
                println!("\nDiagnostic: Checking if connection exists for '{}'...", TEST_IP);
                match client.get_connection(TEST_IP).await {
                    Ok(_conn) => println!("  Connection exists"),
                    Err(e) => println!("  No connection found: {:?}", e),
                }
                return;
            }
        }

        // Step 3: List shares
        println!("Step 3: list_shares('{}')", TEST_IP);

        match client.list_shares(TEST_IP).await {
            Ok(shares) => {
                println!("  ✅ Found {} shares:", shares.len());
                for share in shares {
                    println!("    - {:?}", share.netname);
                }
            }
            Err(e) => {
                println!("  ❌ list_shares failed: {:?}", e);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn main() {
    inner::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("This example only works on macOS (requires the `smb` crate).");
}

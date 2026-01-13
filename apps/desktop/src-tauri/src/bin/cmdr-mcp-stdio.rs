//! MCP STDIO bridge binary.
//!
//! This binary provides STDIO transport for MCP clients that prefer to spawn
//! a subprocess rather than connect via HTTP. It reads JSON-RPC messages from
//! stdin and forwards them to the HTTP server at localhost:9224, then writes
//! responses to stdout.
//!
//! Usage:
//!   cmdr-mcp-stdio [--port PORT]
//!
//! Environment variables:
//!   CMDR_MCP_PORT - Port of the HTTP server (default: 9224)

use reqwest::Client;
use serde_json::Value;
use std::env;
use std::io::{self, BufRead, Write};

const DEFAULT_PORT: u16 = 9224;

fn get_port() -> u16 {
    // Check command line args first
    let args: Vec<String> = env::args().collect();
    for i in 0..args.len() {
        if args[i] == "--port"
            && let Some(port_str) = args.get(i + 1)
                && let Ok(port) = port_str.parse() {
                    return port;
                }
    }

    // Fall back to environment variable
    env::var("CMDR_MCP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

#[tokio::main]
async fn main() {
    let port = get_port();
    let url = format!("http://127.0.0.1:{}/mcp", port);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    eprintln!("cmdr-mcp-stdio: Bridging STDIO to {}", url);

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("cmdr-mcp-stdio: Error reading stdin: {}", e);
                continue;
            }
        };

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse as JSON to validate
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                let _ = writeln!(stdout, "{}", error_response);
                let _ = stdout.flush();
                continue;
            }
        };

        // Forward to HTTP server
        let response = match client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&request)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "error": {
                        "code": -32603,
                        "message": format!("HTTP error: {}", e)
                    }
                });
                let _ = writeln!(stdout, "{}", error_response);
                let _ = stdout.flush();
                continue;
            }
        };

        // Get response body
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "error": {
                        "code": -32603,
                        "message": format!("Response error: {}", e)
                    }
                });
                let _ = writeln!(stdout, "{}", error_response);
                let _ = stdout.flush();
                continue;
            }
        };

        // Write response to stdout (newline-delimited)
        let _ = writeln!(stdout, "{}", body.trim());
        let _ = stdout.flush();
    }
}

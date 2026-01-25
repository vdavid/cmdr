//! Port availability checking for MCP server configuration.

use std::net::TcpListener;

/// Check if a port is available for binding.
#[tauri::command]
pub fn check_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Find the next available port starting from the given port.
/// Returns None if no port is found within 100 attempts.
#[tauri::command]
pub fn find_available_port(start_port: u16) -> Option<u16> {
    const MAX_ATTEMPTS: u16 = 100;

    for offset in 0..MAX_ATTEMPTS {
        let port = start_port.saturating_add(offset);
        if port > 65535 - offset {
            break; // Avoid overflow
        }
        if check_port_available(port) {
            return Some(port);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_port_available() {
        // Port 0 lets the OS assign a port, so we can't reliably test specific ports.
        // Instead, test that the function doesn't panic and returns a boolean.
        let _result = check_port_available(9999);
    }

    #[test]
    fn test_find_available_port() {
        // Should find some available port in a reasonable range
        let result = find_available_port(49152); // Start in dynamic/private port range
                                                 // The result depends on the system state, so we just check it doesn't panic
                                                 // and returns Some if any port is available
        if let Some(port) = result {
            assert!(port >= 49152);
            assert!(port < 49152 + 100);
        }
    }
}

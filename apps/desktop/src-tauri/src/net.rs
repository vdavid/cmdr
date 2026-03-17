//! Network utilities shared across modules.

use std::net::TcpListener;

/// Check if a port is available for binding on localhost.
pub fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Find an available port on localhost starting from `start_port`, scanning up to 100 ports.
pub fn find_available_port(start_port: u16) -> Option<u16> {
    for offset in 0..100 {
        let port = start_port.saturating_add(offset);
        if is_port_available(port) {
            return Some(port);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_port_available() {
        // High port that's likely free — just verify it doesn't panic
        let _ = is_port_available(49999);
    }

    #[test]
    fn test_find_available_port() {
        let result = find_available_port(49000);
        assert!(result.is_some());
        if let Some(port) = result {
            assert!(port >= 49000);
            assert!(port < 49100);
        }
    }
}

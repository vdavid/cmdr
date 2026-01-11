//! MCP server configuration.

use std::env;

/// Configuration for the MCP server, read from environment variables.
#[derive(Debug, Clone)]
pub struct McpConfig {
    /// Whether the MCP server is enabled
    pub enabled: bool,
    /// Port to listen on
    pub port: u16,
}

impl McpConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        let enabled = env::var("CMDR_MCP_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(cfg!(debug_assertions)); // Default: enabled in debug builds

        let port = env::var("CMDR_MCP_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(9224);

        Self { enabled, port }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = McpConfig {
            enabled: true,
            port: 9224,
        };

        assert_eq!(config.port, 9224);
        assert!(config.enabled);
    }

    #[test]
    fn test_from_env_returns_config() {
        let config = McpConfig::from_env();
        assert!(config.port > 0);
    }

    #[test]
    fn test_default_impl() {
        let config = McpConfig::default();
        assert!(config.port > 0);
    }
}

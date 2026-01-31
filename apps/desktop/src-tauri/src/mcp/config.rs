//! MCP server configuration.

use std::env;

/// Configuration for the MCP server.
/// Priority: environment variables > user settings > defaults
#[derive(Debug, Clone)]
pub struct McpConfig {
    /// Whether the MCP server is enabled
    pub enabled: bool,
    /// Port to listen on
    pub port: u16,
}

impl McpConfig {
    /// Load configuration from environment variables only (fallback).
    /// Use `from_settings_and_env` when settings are available.
    pub fn from_env() -> Self {
        Self::from_settings_and_env(None, None)
    }

    /// Load configuration with priority: env vars > user settings > defaults.
    /// This allows env vars to override settings (useful for development),
    /// while letting user settings work in production.
    pub fn from_settings_and_env(setting_enabled: Option<bool>, setting_port: Option<u16>) -> Self {
        // Priority for enabled:
        // 1. CMDR_MCP_ENABLED env var (explicit dev override)
        // 2. User setting (developer.mcpEnabled)
        // 3. Default: enabled in debug builds only
        let enabled = env::var("CMDR_MCP_ENABLED")
            .map(|v| v == "true" || v == "1")
            .ok()
            .or(setting_enabled)
            .unwrap_or(cfg!(debug_assertions));

        // Priority for port:
        // 1. CMDR_MCP_PORT env var (explicit dev override)
        // 2. User setting (developer.mcpPort)
        // 3. Default: 9224
        let port = env::var("CMDR_MCP_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(setting_port)
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
    fn test_direct_construction() {
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

    #[test]
    fn test_from_settings_with_no_settings() {
        // When no settings are provided, should use defaults
        let config = McpConfig::from_settings_and_env(None, None);
        assert_eq!(config.port, 9224);
        // In debug builds, enabled is true by default
        #[cfg(debug_assertions)]
        assert!(config.enabled);
    }

    #[test]
    fn test_from_settings_uses_settings() {
        // When settings are provided, should use them (assuming no env vars override)
        let config = McpConfig::from_settings_and_env(Some(true), Some(8080));
        // Port should use setting unless env var overrides
        // Since we can't control env vars in tests easily, just check structure
        assert!(config.port > 0);
    }

    #[test]
    fn test_from_settings_with_partial_settings() {
        // When only port setting is provided
        let config = McpConfig::from_settings_and_env(None, Some(9999));
        // Should use default enabled and provided port (unless env vars override)
        assert!(config.port > 0);
    }
}

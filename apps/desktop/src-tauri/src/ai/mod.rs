//! Local AI features powered by Falcon-H1R-7B via llama-server.
//!
//! In dev mode, all AI features return mock data (no download, no process).
//! In release mode, manages the llama-server process and communicates via HTTP.

#[cfg(not(debug_assertions))]
pub mod client;
pub mod manager;
pub mod suggestions;

use serde::{Deserialize, Serialize};

/// Current state of the AI subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AiStatus {
    /// AI not installed, offer not yet shown or dismissed
    Unavailable,
    /// Offer shown, waiting for user action
    Offer,
    /// Downloading binary + model
    Downloading,
    /// Setting up (chmod, starting server)
    Installing,
    /// Server running and healthy
    Available,
}

/// Progress info emitted during download.
#[cfg(not(debug_assertions))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    /// Bytes per second
    pub speed: u64,
    /// Estimated seconds remaining
    pub eta_seconds: u64,
}

/// Persisted AI state (stored in ai-state.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiState {
    pub installed: bool,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub model_version: String,
    /// Unix timestamp (seconds) until which the offer is dismissed
    #[serde(default)]
    pub dismissed_until: Option<u64>,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            installed: false,
            port: None,
            pid: None,
            model_version: String::from("falcon-h1r-7b-q4km"),
            dismissed_until: None,
        }
    }
}

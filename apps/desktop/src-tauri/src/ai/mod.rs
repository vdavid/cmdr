//! AI features. Two paths:
//!
//! - **Local LLM** via bundled `llama-server`. Requires Apple Silicon (M1+) because the
//!   binary is ARM64-only. Gated by [`is_local_ai_supported`].
//! - **Cloud AI** (OpenAI / Anthropic / Gemini / any OpenAI-compatible endpoint, BYOK).
//!   Works on any hardware, including Intel Macs.
//!
//! Don't conflate the two: an Intel user can absolutely use AI features, just not the
//! local path. Code that turns off "AI" wholesale on non-aarch64 is a bug.
//!
//! ## Model registry
//!
//! Available models are defined in [`AVAILABLE_MODELS`]. To add a new model:
//! 1. Find the GGUF file on HuggingFace
//! 2. Get the exact file size: `curl -sIL "<url>" | grep -i content-length`
//! 3. Add a new entry to `AVAILABLE_MODELS`
//! 4. Update `DEFAULT_MODEL_ID` if the new model should be the default

pub mod api_keys;
pub mod client;
#[cfg(test)]
mod client_integration_test;
#[cfg(test)]
mod client_local_llama_test;
#[cfg(test)]
mod client_real_anthropic_test;
#[cfg(test)]
mod client_real_groq_test;
#[cfg(test)]
mod client_real_openai_test;
#[cfg(test)]
mod client_streaming_test;
pub mod connection_check;
mod download;
pub mod extract;
pub mod install;
pub mod manager;
mod process;
pub mod server;
pub mod state;
mod stream_registry;
pub mod suggestions;
#[cfg(test)]
mod suggestions_streaming_test;
pub mod translate;
pub mod translate_error;

pub use translate_error::{AiTranslateError, AiTranslateErrorKind};

use serde::{Deserialize, Serialize};

/// Returns true if local AI features (llama-server) can run on this hardware.
///
/// Requires Apple Silicon (M1+) because the bundled llama-server binary is ARM64-only.
///
/// Only used to gate local-only operations (start_ai_server, start_ai_download).
/// Provider-agnostic commands (get_ai_status, get_folder_suggestions, etc.) check
/// the configured provider instead.
pub fn is_local_ai_supported() -> bool {
    cfg!(target_arch = "aarch64")
}

/// Current state of the AI subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AiStatus {
    Unavailable,
    /// Waiting for user action.
    Offer,
    Downloading,
    /// chmod, starting server.
    Installing,
    Available,
}

/// Progress info emitted during model download (`ai-download-progress`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "ai-download-progress")]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    /// Bytes per second.
    pub speed: u64,
    pub eta_seconds: u64,
}

/// Payloadless AI lifecycle events. Each kebab-cases to its wire name. The
/// install flow emits them in sequence (`ai-extracting` → repeated
/// `ai-download-progress` → `ai-verifying` → `ai-installing` →
/// `ai-install-complete`); `ai-starting` / `ai-server-ready` bracket a server
/// boot on a returning launch.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiStarting;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiServerReady;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiVerifying;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiInstalling;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiInstallComplete;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiExtracting;

// ============================================================================
// Model registry
// ============================================================================

/// Information about an available AI model.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    /// GGUF filename stored locally.
    pub filename: &'static str,
    pub url: &'static str,
    /// For download verification. Get via: `curl -sIL "<url>" | grep -i content-length`
    pub size_bytes: u64,
    /// Bytes per token for KV cache (used for memory estimation).
    /// Derived from empirical measurement: ctx_size * kv_bytes_per_token = KV cache size.
    pub kv_bytes_per_token: u64,
    /// Base memory overhead in bytes (model weights + compute buffers).
    pub base_overhead_bytes: u64,
}

/// Available AI models. Add new models here when upgrading.
/// The first model in the list with `id == DEFAULT_MODEL_ID` is the default.
pub const AVAILABLE_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "ministral-3b-instruct-q4km",
        display_name: "Ministral 3B",
        filename: "ministral-3b-instruct-q4km.gguf",
        url: "https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512-GGUF/resolve/main/Ministral-3-3B-Instruct-2512-Q4_K_M.gguf",
        size_bytes: 2_147_023_008,          // ~2.0 GB
        kv_bytes_per_token: 106_496,        // ~0.1016 MiB per token
        base_overhead_bytes: 3_500_000_000, // ~3.5 GB (model weights + compute buffers)
    },
    ModelInfo {
        id: "falcon-h1r-7b-q4km",
        display_name: "Falcon H1R 7B",
        filename: "falcon-h1r-7b-q4km.gguf",
        url: "https://huggingface.co/tiiuae/Falcon-H1R-7B-GGUF/resolve/main/Falcon-H1R-7B-Q4_K_M.gguf",
        size_bytes: 4_598_343_712, // ~4.28 GB
        kv_bytes_per_token: 106_496,
        base_overhead_bytes: 3_500_000_000,
    },
];

/// Default model ID for new installations.
/// When adding a newer/better model, update this to make it the new default.
pub const DEFAULT_MODEL_ID: &str = "ministral-3b-instruct-q4km";

/// Returns the model info for the given ID, or None if not found.
pub fn get_model_by_id(id: &str) -> Option<&'static ModelInfo> {
    AVAILABLE_MODELS.iter().find(|m| m.id == id)
}

/// Returns the default model info.
pub fn get_default_model() -> &'static ModelInfo {
    get_model_by_id(DEFAULT_MODEL_ID).expect("DEFAULT_MODEL_ID must exist in AVAILABLE_MODELS")
}

/// Persisted AI state (stored in ai-state.json).
/// This tracks installation state. Model selection is stored in user settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiState {
    pub installed: bool,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    /// Matches `ModelInfo.id`.
    #[serde(default = "default_model_id")]
    pub installed_model_id: String,
    /// Unix timestamp (seconds).
    #[serde(default)]
    pub dismissed_until: Option<u64>,
    /// Verified by file size.
    #[serde(default)]
    pub model_download_complete: bool,
    /// Unix timestamp, for stale cleanup.
    #[serde(default)]
    pub partial_download_started: Option<u64>,
}

fn default_model_id() -> String {
    DEFAULT_MODEL_ID.to_string()
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            installed: false,
            port: None,
            pid: None,
            installed_model_id: default_model_id(),
            dismissed_until: None,
            model_download_complete: false,
            partial_download_started: None,
        }
    }
}

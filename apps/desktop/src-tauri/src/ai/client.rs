//! HTTP client for the local llama-server (OpenAI-compatible API).

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Error types for AI client operations.
#[derive(Debug, Clone)]
pub enum AiError {
    /// Server is not running or not reachable
    Unavailable,
    /// Request timed out
    Timeout,
    /// Server returned an error
    ServerError(String),
    /// Failed to parse the response
    ParseError(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "AI server unavailable"),
            Self::Timeout => write!(f, "AI request timed out"),
            Self::ServerError(msg) => write!(f, "AI server error: {msg}"),
            Self::ParseError(msg) => write!(f, "AI response parse error: {msg}"),
        }
    }
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

/// Sends a chat completion request to the local llama-server.
///
/// Returns the assistant's response text, or an error.
/// Times out after 10 seconds.
pub async fn chat_completion(port: u16, prompt: &str) -> Result<String, AiError> {
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");

    let request_body = ChatCompletionRequest {
        model: String::from("local-model"), // llama-server uses whatever model it loaded
        messages: vec![
            ChatMessage {
                role: String::from("system"),
                content: String::from(
                    "You are a pattern-matching assistant. Carefully observe the style, language, and formatting of existing items, then generate new items that match exactly. Output only what is requested, no formatting or explanation.",
                ),
            },
            ChatMessage {
                role: String::from("user"),
                content: prompt.to_string(),
            },
        ],
        temperature: 0.6,
        top_p: 0.95,
        max_tokens: 150, // Just need 5 folder names
        stream: false,
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| AiError::ServerError(e.to_string()))?;

    let response = client.post(&url).json(&request_body).send().await.map_err(|e| {
        if e.is_timeout() {
            AiError::Timeout
        } else if e.is_connect() {
            AiError::Unavailable
        } else {
            AiError::ServerError(e.to_string())
        }
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AiError::ServerError(format!("HTTP {status}: {body}")));
    }

    // Get raw body for debugging
    let body = response.text().await.map_err(|e| AiError::ParseError(e.to_string()))?;
    log::trace!("AI chat_completion: raw response body: {body}");

    let parsed: ChatCompletionResponse =
        serde_json::from_str(&body).map_err(|e| AiError::ParseError(format!("JSON parse error: {e}")))?;

    let content = parsed
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| AiError::ParseError(String::from("No choices in response")))?;

    log::trace!("AI chat_completion: extracted content: {content}");
    Ok(content)
}

/// Checks if the llama-server is healthy.
pub async fn health_check(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");

    let client = match reqwest::Client::builder().timeout(Duration::from_secs(2)).build() {
        Ok(c) => c,
        Err(e) => {
            log::debug!("AI health_check: failed to build client: {e}");
            return false;
        }
    };

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                true
            } else {
                let body = response.text().await.unwrap_or_default();
                log::debug!("AI health_check: HTTP {status}, body: {body}");
                false
            }
        }
        Err(e) => {
            log::trace!("AI health_check: connection error (expected during startup): {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_error_display() {
        assert_eq!(AiError::Unavailable.to_string(), "AI server unavailable");
        assert_eq!(AiError::Timeout.to_string(), "AI request timed out");
        assert_eq!(
            AiError::ServerError(String::from("bad")).to_string(),
            "AI server error: bad"
        );
        assert_eq!(
            AiError::ParseError(String::from("oops")).to_string(),
            "AI response parse error: oops"
        );
    }
}

//! Shared orchestration for the AI "translate" commands (drive search and
//! selection). The two pipelines keep their own prompts and response parsers —
//! only the chat-call plumbing (fire the request with the empty-response retry,
//! time it, map the client error to a typed [`AiTranslateError`]) is shared here.
//! Backend resolution lives in [`super::manager::resolve_translate_backend`].

use genai::chat::ChatOptions;

use super::AiTranslateError;
use super::client::{self, AiBackend};

/// Runs one translate-style chat completion.
///
/// Fires the request through [`client::chat_completion_with_empty_retry`], logs
/// timing, and maps a client error to a typed [`AiTranslateError`]. `label` names
/// the caller in the logs (for example `"AI search"`, `"AI selection"`). The
/// caller supplies the (pipeline-specific) system prompt, user query, and chat
/// options, and parses the returned raw response itself.
pub async fn translate_once(
    backend: &AiBackend,
    system_prompt: &str,
    user_query: &str,
    options: &ChatOptions,
    label: &str,
) -> Result<String, AiTranslateError> {
    let t0 = std::time::Instant::now();
    let response = client::chat_completion_with_empty_retry(backend, system_prompt, user_query, options)
        .await
        .map_err(|e| {
            log::warn!(
                target: "ai::translate",
                "{label}: chat_completion failed after {:.1}s for query={user_query:?}: {e}",
                t0.elapsed().as_secs_f64()
            );
            AiTranslateError::from(e)
        })?;

    log::info!(
        target: "ai::translate",
        "{label}: chat_completion returned {} chars in {:.1}s",
        response.len(),
        t0.elapsed().as_secs_f64()
    );
    log::debug!(target: "ai::translate", "{label}: raw response: {response:?}");

    Ok(response)
}

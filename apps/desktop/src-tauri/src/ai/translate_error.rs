//! Typed error for the AI natural-language translation commands.
//!
//! Search and Selection both translate a prompt into a structured query via
//! [`crate::ai::client::chat_completion`]. When that fails (provider off, key rejected,
//! quota / rate limit, timeout, empty answer), the dialogs need to show a SPECIFIC toast,
//! not a generic "something went wrong". A bare `String` error would force the frontend to
//! string-match the message (banned by the `no-string-matching` rule), so we cross the IPC
//! boundary with a typed `kind` plus a human-readable `message`. The frontend branches on
//! `kind`; `message` is detail for logs, never for control flow.

use serde::Serialize;

use super::client::AiError;

/// Coarse, frontend-branchable classification of an AI translation failure.
///
/// Keep in lockstep with the `AiErrorKind` switch in
/// `apps/desktop/src/lib/ai/translate-error-toast.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AiTranslateErrorKind {
    /// AI is turned off (`provider = "off"`).
    Off,
    /// Provider is selected but not usable yet (no key, local server down, wrong provider).
    NotConfigured,
    /// The provider rejected the API key (HTTP 401 / 403).
    AuthFailed,
    /// The provider is rate-limiting requests or the account is out of quota (HTTP 429).
    RateLimited,
    /// The request timed out.
    Timeout,
    /// Couldn't reach the provider (DNS / connection refused).
    Unavailable,
    /// The model returned no usable text (often a reasoning model burning the token budget).
    EmptyResponse,
    /// The provider returned some other HTTP error or otherwise misbehaved.
    ServerError,
    /// Couldn't parse the provider's response.
    ParseError,
    /// The configured provider value isn't recognized.
    UnknownProvider,
}

/// Typed error returned by `translate_search_query` / `translate_selection_query`.
///
/// `message` is a human-readable detail string for logs and the toast's secondary line; the
/// frontend chooses the headline + tone from `kind`, never by parsing `message`.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiTranslateError {
    pub kind: AiTranslateErrorKind,
    pub message: String,
}

impl AiTranslateError {
    pub fn new(kind: AiTranslateErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AiTranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for AiTranslateError {}

impl From<AiError> for AiTranslateError {
    fn from(e: AiError) -> Self {
        use AiTranslateErrorKind as K;
        let kind = match e {
            AiError::Unavailable => K::Unavailable,
            AiError::Timeout => K::Timeout,
            AiError::AuthFailed(_) => K::AuthFailed,
            AiError::RateLimited(_) => K::RateLimited,
            AiError::EmptyResponse => K::EmptyResponse,
            AiError::ServerError(_) => K::ServerError,
            AiError::ParseError(_) => K::ParseError,
        };
        Self::new(kind, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_each_ai_error_to_its_kind() {
        use AiTranslateErrorKind as K;
        let cases = [
            (AiError::Unavailable, K::Unavailable),
            (AiError::Timeout, K::Timeout),
            (AiError::AuthFailed("x".into()), K::AuthFailed),
            (AiError::RateLimited("x".into()), K::RateLimited),
            (AiError::EmptyResponse, K::EmptyResponse),
            (AiError::ServerError("x".into()), K::ServerError),
            (AiError::ParseError("x".into()), K::ParseError),
        ];
        for (err, expected) in cases {
            assert_eq!(AiTranslateError::from(err).kind, expected);
        }
    }

    #[test]
    fn carries_the_detail_message() {
        // The detail flows through verbatim (the source error's Display), so logs / the
        // toast's secondary line keep the provider's wording. We compare against Display
        // rather than substring-matching the message (the no-string-matching rule).
        let src = AiError::RateLimited("HTTP 429: out of quota".into());
        let expected = src.to_string();
        let err = AiTranslateError::from(src);
        assert_eq!(err.kind, AiTranslateErrorKind::RateLimited);
        assert_eq!(err.message, expected);
    }
}

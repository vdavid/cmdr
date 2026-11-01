//! Request headers that can't leak the bearer token into a log.
//!
//! ❗ **`auth::guard` extracts [`SafeHeaders`], never a bare `HeaderMap`, and that
//! is the whole point.** `HeaderMap`'s own `Debug` prints every value,
//! so a `{:?}` on one writes `Authorization: Bearer <token>` into `cmdr.log`.
//! A leaked token can escape through copied logs, reports, or an authenticated
//! `cmdr://logs` read; the log redactor covers home paths, SMB URIs, and emails,
//! not arbitrary bearer secrets.
//!
//! That shape has leaked before, on a route a patch to its sibling missed. That
//! is what a rule saying "don't log the map" costs. With the raw map out of the
//! guard's signature there is nothing to reach for: the next person who writes
//! `{:?}` on the headers is safe by construction. Everything a
//! validator needs still reaches the map through [`Deref`](std::ops::Deref).

use axum::extract::FromRequestParts;
use axum::http::{HeaderMap, header, request::Parts};
use std::convert::Infallible;

/// The request headers, wearing a `Debug` that redacts the bearer token.
pub(crate) struct SafeHeaders(HeaderMap);

impl SafeHeaders {
    /// Wraps a map directly, for tests; the guard gets its own from the extractor below.
    #[cfg(test)]
    pub(crate) fn new(headers: HeaderMap) -> Self {
        Self(headers)
    }
}

impl std::ops::Deref for SafeHeaders {
    type Target = HeaderMap;

    fn deref(&self) -> &HeaderMap {
        &self.0
    }
}

impl std::fmt::Debug for SafeHeaders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut rendered = f.debug_map();
        for (name, value) in self.0.iter() {
            if name == header::AUTHORIZATION {
                rendered.entry(&name.as_str(), &"<redacted>");
            } else {
                rendered.entry(&name.as_str(), value);
            }
        }
        rendered.finish()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for SafeHeaders {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // The same clone axum's own `HeaderMap` extractor does, so wrapping costs
        // nothing over extracting the map directly.
        Ok(SafeHeaders(parts.headers.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Headers as a client sends them to open the SSE stream or POST a call.
    fn headers_with_token() -> SafeHeaders {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer super-secret".parse().unwrap());
        headers.insert(header::ORIGIN, "http://127.0.0.1".parse().unwrap());
        headers.insert(header::USER_AGENT, "claude-code".parse().unwrap());
        SafeHeaders::new(headers)
    }

    #[test]
    fn a_header_dump_never_carries_the_bearer_token() {
        // A token in the log file defeats the boundary if those logs are copied, reported, or
        // viewed through an already authenticated client.
        let dumped = format!("{:?}", headers_with_token());

        assert!(!dumped.contains("super-secret"), "{dumped}");
        assert!(dumped.contains("<redacted>"), "{dumped}");
    }

    #[test]
    fn every_other_header_still_reads() {
        // The dump is the only view of what a client actually sent; redacting
        // more than the one secret would make it useless for debugging.
        let dumped = format!("{:?}", headers_with_token());

        assert!(dumped.contains("127.0.0.1"), "{dumped}");
        assert!(dumped.contains("claude-code"), "{dumped}");
    }

    #[test]
    fn a_request_with_no_authorization_header_dumps_unchanged() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, "text/event-stream".parse().unwrap());

        let dumped = format!("{:?}", SafeHeaders::new(headers));

        assert!(dumped.contains("text/event-stream"), "{dumped}");
        assert!(!dumped.contains("<redacted>"), "{dumped}");
    }

    #[test]
    fn the_map_itself_still_reaches_every_validator() {
        // The redaction is for the `Debug` only: `validate_origin` and
        // `validate_token` must still see the real values, through `Deref`.
        let headers = headers_with_token();

        assert_eq!(
            headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()),
            Some("Bearer super-secret")
        );
    }
}

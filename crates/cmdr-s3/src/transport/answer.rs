//! What comes back: an [`Answer`] read whole under a cap, or an [`Opened`]
//! whose body is still on the wire, and why an exchange produced neither
//! ([`ExchangeError`]).

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::liveness::Liveness;
use http::{HeaderMap, StatusCode};

use super::map_transport_error;

/// The most of an [`Answer`]'s body Cmdr holds. The biggest answer it asks for
/// is a 1 000-key `ListObjectsV2` page, about 1 MiB of XML; past this the
/// server is hostile or broken, and reading on would hold it all in memory.
pub(crate) const MAX_ANSWER_BODY: usize = 16 * 1024 * 1024;

/// Why an exchange didn't produce an [`Answer`].
#[derive(Debug)]
pub(crate) enum ExchangeError {
    /// The request or the body read failed on the wire.
    Transport(reqwest::Error),
    /// The answer's body ran past `limit` bytes; the rest was never read.
    BodyTooLarge { limit: usize },
}

impl From<reqwest::Error> for ExchangeError {
    fn from(err: reqwest::Error) -> Self {
        Self::Transport(err)
    }
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(err) => err.fmt(f),
            // allowed-pluralize-noun: `limit` is a cap in mebibytes, never 1.
            Self::BodyTooLarge { limit } => write!(f, "the server's answer ran past {limit} bytes"),
        }
    }
}

impl std::error::Error for ExchangeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            Self::BodyTooLarge { .. } => None,
        }
    }
}

/// `response`'s whole body, noting every chunk as heard, or `BodyTooLarge` once
/// it runs past `limit` (or announces it will).
pub(super) async fn read_capped(
    liveness: &Liveness,
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, ExchangeError> {
    if response
        .content_length()
        .is_some_and(|announced| announced > limit as u64)
    {
        return Err(ExchangeError::BodyTooLarge { limit });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        liveness.heard();
        if body.len() + chunk.len() > limit {
            return Err(ExchangeError::BodyTooLarge { limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// What a request came back with. The body is read whole, capped at
/// [`MAX_ANSWER_BODY`]: every answer this carries is a small XML document or
/// nothing (a HEAD).
#[derive(Debug)]
pub(crate) struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Answer {
    /// The body as text, lossily: XML from a well-behaved server, an HTML page
    /// or nothing from anything else.
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// A header's value, when it's there and printable.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The object's length ([`object_length`]).
    pub(crate) fn object_length(&self) -> Option<u64> {
        object_length(&self.headers)
    }
}

/// An object's length off a HEAD or GET: `Content-Length`, else GCS's
/// `x-goog-stored-content-length`, which is all GCS sends for a HEAD of an
/// object stored with `Content-Encoding: gzip` (live, 2026-10-02).
fn object_length(headers: &HeaderMap) -> Option<u64> {
    let read = |name: &str| headers.get(name)?.to_str().ok()?.parse().ok();
    read("content-length").or_else(|| read("x-goog-stored-content-length"))
}

/// An answer whose body is still on the wire (`S3Client::open`).
pub(crate) struct Opened {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub(super) response: reqwest::Response,
    /// The client's silence watch: every chunk is the server being there.
    pub(super) liveness: Arc<Liveness>,
    pub(super) volume_id: String,
    pub(super) path: String,
}

impl Opened {
    /// A header's value, when it's there and printable.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The object's length ([`object_length`]).
    pub(crate) fn object_length(&self) -> Option<u64> {
        object_length(&self.headers)
    }

    /// The next piece of the body as it arrives, `None` at its end. A
    /// transport failure comes back in the `Volume` vocabulary. Copied out
    /// once, because `VolumeReadStream` hands out owned bytes anyway.
    pub(crate) async fn chunk(&mut self) -> Option<Result<Vec<u8>, VolumeError>> {
        match self.response.chunk().await {
            Ok(Some(chunk)) => {
                self.liveness.heard();
                Some(Ok(chunk.to_vec()))
            }
            Ok(None) => None,
            Err(e) => Some(Err(map_transport_error(&e, &self.volume_id, &self.path))),
        }
    }

    /// The rest of the body as text, for an error answer (a small XML
    /// document). Whatever doesn't arrive within `budget` is left out, so a
    /// misbehaving server can't hold a failed read open.
    pub(crate) async fn text(mut self, budget: Duration) -> String {
        let mut body = Vec::new();
        let _ = tokio::time::timeout(budget, async {
            while let Some(Ok(chunk)) = self.chunk().await {
                body.extend_from_slice(&chunk);
            }
        })
        .await;
        String::from_utf8_lossy(&body).into_owned()
    }
}

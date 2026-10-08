//! A request before and after signing, as plain values.
//!
//! Nothing here sends anything: the transport turns a [`SignedRequest`] into a
//! `reqwest` call, so `reqwest` never leaks into the protocol layer.

use http::{HeaderMap, HeaderName, HeaderValue, Method};
use url::Url;

use crate::encoding::wire_query;

/// What travels after the headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Body {
    /// No body.
    Empty,
    /// A small body built in memory (an XML document). Signed with its real
    /// hash: it costs nothing, and it's what the strictest servers want.
    Bytes(Vec<u8>),
    /// A body the transport streams from elsewhere, `length` bytes long. Signed
    /// `UNSIGNED-PAYLOAD`, so the bytes never have to be read twice.
    Streamed {
        /// The exact byte count, sent as `Content-Length`.
        length: u64,
    },
}

/// A request ready to sign.
#[derive(Debug, Clone)]
pub(crate) struct S3Request {
    pub method: Method,
    /// `http` or `https`.
    pub scheme: String,
    /// The `Host` header value: the host, plus `:port` when it isn't the
    /// scheme's default. A virtual-hosted request's host carries the bucket.
    pub host: String,
    /// The path, already percent-encoded (`encoding::encode_key`).
    pub path: String,
    /// Raw (unencoded) query parameters; a valueless one has an empty value.
    pub query: Vec<(String, String)>,
    /// Headers to send and sign. Names are lowercase by construction.
    pub headers: HeaderMap,
    pub body: Body,
    /// The bucket the request is about, `None` for the account root. What
    /// the transport routes an AWS account root's requests by
    /// (`routing.rs`).
    pub bucket: Option<String>,
    /// How the request is signed and spelled.
    pub dialect: Dialect,
}

/// The signing dialect a request goes out in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// AWS SigV4: `AWS4-HMAC-SHA256`, `x-amz-*` headers. Every provider.
    Amz,
    /// GCS's own V4: `GOOG4-HMAC-SHA256`, every `x-amz-*` header spelled
    /// `x-goog-*`. Only for a request carrying a GCS-only header (its
    /// create-only `x-goog-if-generation-match`), which GCS refuses beside any
    /// `x-amz-*` one (`400 ExcessHeaderValues`, live, 2026-10-02).
    Goog,
}

impl S3Request {
    /// An empty-bodied request for `path` on `host`.
    pub(crate) fn new(method: Method, scheme: &str, host: &str, path: String) -> Self {
        Self {
            method,
            scheme: scheme.to_string(),
            host: host.to_string(),
            path,
            query: Vec::new(),
            headers: HeaderMap::new(),
            body: Body::Empty,
            bucket: None,
            dialect: Dialect::Amz,
        }
    }

    /// Adds a query parameter.
    pub(crate) fn query(mut self, name: &str, value: &str) -> Self {
        self.query.push((name.to_string(), value.to_string()));
        self
    }

    /// Sets a header. A value that can't be a header value (a control
    /// character) is dropped by the caller's construction, never here: every
    /// value this crate sets is ASCII it built itself or a percent-encoded key.
    pub(crate) fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// The URL the request goes to.
    pub(crate) fn url(&self) -> Url {
        let query = wire_query(&self.query);
        let mut text = format!("{}://{}{}", self.scheme, self.host, self.path);
        if !query.is_empty() {
            text.push('?');
            text.push_str(&query);
        }
        // Every part was built from validated pieces: a parsed endpoint's
        // scheme and host, and paths and queries made only of unreserved
        // characters, `%XX`, `/`, `=`, and `&`.
        Url::parse(&text).expect("an S3 request URL is assembled from parsed, encoded parts")
    }
}

/// A signed request: what the transport sends.
#[derive(Debug, Clone)]
pub(crate) struct SignedRequest {
    pub method: Method,
    pub url: Url,
    /// Every header, `Authorization` and the `x-amz-*` signing headers included.
    pub headers: HeaderMap,
    pub body: Body,
}

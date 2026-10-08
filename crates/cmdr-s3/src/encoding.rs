//! Percent-encoding the way SigV4 and S3 want it.
//!
//! One rule for everything: every byte except the RFC 3986 unreserved set
//! (`A–Z a–z 0–9 - . _ ~`) becomes `%XX` with uppercase hex, and a space is
//! `%20`, never `+`. A path keeps its `/` separators; a query component
//! encodes them too.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Everything but the unreserved set.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// Encodes one query name or value, or one path segment.
pub(crate) fn encode_component(input: &str) -> String {
    utf8_percent_encode(input, COMPONENT).to_string()
}

/// A key that can't travel in an HTTP URL as itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyError {
    /// An object operation got an empty key.
    Empty,
    /// A segment is `.` or `..`. Every URL parser on the way (the `url` crate,
    /// then `reqwest`) resolves those, and an encoded `%2E%2E` counts as `..`
    /// too (WHATWG URL § 4.4), so `a/../b` would reach the object `b`. Refused
    /// rather than sent: a delete would land on the wrong object.
    DotSegment,
}

/// Encodes a key segment by segment, keeping its `/` separators. S3 encodes
/// the canonical URI ONCE (every other AWS service encodes it twice), so this
/// output is both the wire path and the signed one.
pub(crate) fn encode_key(key: &str) -> Result<String, KeyError> {
    if key.is_empty() {
        return Err(KeyError::Empty);
    }
    let mut out = String::with_capacity(key.len() + 8);
    for (i, segment) in key.split('/').enumerate() {
        if segment == "." || segment == ".." {
            return Err(KeyError::DotSegment);
        }
        if i > 0 {
            out.push('/');
        }
        out.push_str(&encode_component(segment));
    }
    Ok(out)
}

/// The canonical query string: names and values encoded, pairs sorted by
/// encoded name then value, a valueless parameter as `name=`.
pub(crate) fn canonical_query(params: &[(String, String)]) -> String {
    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(name, value)| (encode_component(name), encode_component(value)))
        .collect();
    encoded.sort();
    let pairs: Vec<String> = encoded
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    pairs.join("&")
}

/// The query as it goes on the wire: the canonical form, except a valueless
/// parameter travels bare (`?uploads`, `?delete`). That's what the AWS SDKs
/// send, and every server canonicalizes it back to `uploads=` when verifying.
pub(crate) fn wire_query(params: &[(String, String)]) -> String {
    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(name, value)| (encode_component(name), encode_component(value)))
        .collect();
    encoded.sort();
    let pairs: Vec<String> = encoded
        .into_iter()
        .map(|(name, value)| {
            if value.is_empty() {
                name
            } else {
                format!("{name}={value}")
            }
        })
        .collect();
    pairs.join("&")
}

#[cfg(test)]
#[path = "encoding_test.rs"]
mod encoding_test;

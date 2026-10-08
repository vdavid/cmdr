//! Why a connect attempt didn't produce a working volume, and the probe's
//! refusal table.
//!
//! ❌ Nothing here is prose a user reads: the host words every variant. ❌
//! Nothing here reads a `<Message>` or a transport error's text: every
//! decision is a status, an S3 `<Code>`, a header, or one of
//! `reqwest::Error`'s typed predicates.

use http::StatusCode;

use crate::error::{S3Error, S3ErrorCode};

/// Why a connection attempt didn't produce a working volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S3ConnectError {
    /// The provider's fields can't make an endpoint (a region with a dot in
    /// it, an endpoint URL with a path).
    InvalidProvider,
    /// The store held nothing to offer, so nothing was tried.
    NeedsCredentials,
    /// The server named the keys as wrong: `SignatureDoesNotMatch` (the secret)
    /// or `InvalidAccessKeyId` (the key id). ❗ Only those two codes: S3 can't
    /// say "wrong secret" any other way.
    KeysRejected,
    /// The server refused this place: a 403 on the bucket. ❗ Ambiguous by
    /// nature (a HEAD has no body, and Garage answers a wrong secret with
    /// `AccessDenied`), so the words cover a wrong key AND a key without rights.
    AccessDenied,
    /// The account root needs `ListBuckets`, and the server answered
    /// `AccessDenied`: a bucket-scoped key, or (Garage) a wrong secret. Typing a
    /// bucket name is the way in for the first.
    BucketListRefused,
    /// No bucket by that name, or not one this endpoint serves.
    NoSuchBucket,
    /// The bucket lives in another region than the endpoint's. `region` is the
    /// right one when the server said (`x-amz-bucket-region`).
    WrongRegion {
        /// The bucket's region, when the server named it.
        region: Option<String>,
    },
    /// The server refused the request's timestamp (`RequestTimeTooSkewed`):
    /// this Mac's clock is off by more than 15 minutes.
    ClockSkewed,
    /// The TLS handshake was refused, which a self-signed endpoint produces.
    CertificateUntrusted,
    /// Something answered, but not as S3: an HTML page, a bare 404.
    NotAnS3Endpoint,
    /// The probe didn't finish inside the connect budget.
    TimedOut,
    /// The server couldn't be reached: DNS, refused, no route. A log
    /// diagnostic, never shown.
    Unreachable(String),
    /// Anything else: a 5xx, a body that wouldn't read. A log diagnostic.
    Transport(String),
    /// The user called the connect off.
    Cancelled,
}

/// What `ListBuckets` said about the keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BucketList {
    /// A listing came back: the keys work.
    Listed,
    /// Refused in a way a bucket might still get past (a bucket-scoped key).
    FallBack,
    /// Refused for good.
    Refused(S3ConnectError),
}

/// What `HeadBucket` said about the place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BucketCheck {
    Open,
    Refused(S3ConnectError),
}

/// Judges the probe's `ListBuckets` answer. `wants_bucket` is whether the
/// place is a bucket (which a refused listing may still reach) rather than the
/// account root (which needs the listing).
///
/// The table, in order:
///
/// - **2xx with a `ListAllMyBucketsResult`**: the keys work.
/// - **`SignatureDoesNotMatch` / `InvalidAccessKeyId`, or any 401** (R2's
///   answer to an unknown key id): the keys, whatever the place.
/// - **`RequestTimeTooSkewed`**: this Mac's clock.
/// - **5xx, or a throttle**: the server's trouble, not the keys'.
/// - **`AccessDenied`, or any other S3 error**: the account root can't go on
///   (`BucketListRefused` for `AccessDenied`); a bucket may still answer, so
///   it falls back to `HeadBucket`.
/// - **Anything without an S3 `<Error>` body** (an HTML page, a 200 that isn't
///   XML, a bare 404): not S3.
pub(crate) fn judge_list_buckets(status: StatusCode, body: &str, wants_bucket: bool) -> BucketList {
    if status.is_success() {
        return match crate::xml::parse_list_buckets(body) {
            Ok(_) => BucketList::Listed,
            Err(_) => BucketList::Refused(S3ConnectError::NotAnS3Endpoint),
        };
    }
    // R2 answers a key id it doesn't know with `401 Unauthorized`.
    if status == StatusCode::UNAUTHORIZED {
        return BucketList::Refused(S3ConnectError::KeysRejected);
    }
    let error = S3Error::from_response(status, body);
    match error.code {
        S3ErrorCode::SignatureDoesNotMatch | S3ErrorCode::InvalidAccessKeyId => {
            BucketList::Refused(S3ConnectError::KeysRejected)
        }
        S3ErrorCode::RequestTimeTooSkewed => BucketList::Refused(S3ConnectError::ClockSkewed),
        _ if status.is_server_error() || error.is_retryable() => {
            BucketList::Refused(S3ConnectError::Transport(error.to_string()))
        }
        S3ErrorCode::NoBody => BucketList::Refused(S3ConnectError::NotAnS3Endpoint),
        _ if wants_bucket => BucketList::FallBack,
        S3ErrorCode::AccessDenied => BucketList::Refused(S3ConnectError::BucketListRefused),
        _ => BucketList::Refused(S3ConnectError::Transport(error.to_string())),
    }
}

/// Judges the probe's `HeadBucket` answer. `region` is the
/// `x-amz-bucket-region` header, when there was one.
///
/// A HEAD carries no body, so this is status and header alone: 404 is a
/// missing bucket, 401 the keys (R2), a redirect is a bucket in another region
/// (AWS names it in the header), and 403 is [`S3ConnectError::AccessDenied`], which can't tell a
/// wrong key from a key without rights here.
pub(crate) fn judge_head_bucket(status: StatusCode, region: Option<&str>) -> BucketCheck {
    if status.is_success() {
        return BucketCheck::Open;
    }
    BucketCheck::Refused(match status {
        StatusCode::NOT_FOUND => S3ConnectError::NoSuchBucket,
        StatusCode::UNAUTHORIZED => S3ConnectError::KeysRejected,
        StatusCode::FORBIDDEN => S3ConnectError::AccessDenied,
        s if s.is_redirection() || region.is_some() => S3ConnectError::WrongRegion {
            region: region.map(str::to_string),
        },
        s if s.is_server_error() => S3ConnectError::Transport(format!("HTTP {}", s.as_u16())),
        _ => S3ConnectError::NotAnS3Endpoint,
    })
}

#[cfg(test)]
#[path = "refusal_test.rs"]
mod refusal_test;

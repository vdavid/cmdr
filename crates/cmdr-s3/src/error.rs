//! What an S3 server said went wrong, as a typed value.
//!
//! Classified on the `<Error><Code>` and the HTTP status, ❌ never on
//! `<Message>`: providers word messages differently and change them freely,
//! while the codes are AWS's contract that every compatible server copies.
//! A HEAD response has no body, so it classifies by status alone.

use std::fmt;

use http::StatusCode;

/// The `<Code>` of an S3 error. The named variants are the ones some code path
/// acts on; everything else keeps its code text in `Other` for the logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum S3ErrorCode {
    AccessDenied,
    /// The signing region is wrong; the body's `<Region>` names the right one.
    AuthorizationHeaderMalformed,
    BadDigest,
    /// AWS: a concurrent delete beat a conditional write (409).
    ConditionalRequestConflict,
    EntityTooLarge,
    EntityTooSmall,
    ExpiredToken,
    InternalError,
    InvalidAccessKeyId,
    InvalidArgument,
    InvalidBucketName,
    InvalidDigest,
    /// The object is archived (Glacier Flexible Retrieval, Deep Archive) and
    /// must be restored before it can be read or copied.
    InvalidObjectState,
    /// GCS: the name holds a character it can't store (CR or LF).
    InvalidObjectName,
    /// A part's ETag didn't match, or (R2) the parts weren't all one size.
    InvalidPart,
    InvalidPartOrder,
    InvalidRange,
    InvalidRequest,
    MethodNotAllowed,
    NoSuchBucket,
    NoSuchKey,
    NoSuchUpload,
    /// The server doesn't implement a header or operation we sent (B2 answers
    /// this to `If-None-Match`).
    NotImplemented,
    /// The bucket lives at another endpoint; the body's `<Endpoint>` says where.
    PermanentRedirect,
    /// A conditional write lost: the object exists (412).
    PreconditionFailed,
    /// Wasabi's throttle.
    RequestRateLimitExceeded,
    RequestTimeTooSkewed,
    RequestTimeout,
    ServiceUnavailable,
    SignatureDoesNotMatch,
    SlowDown,
    /// R2's throttle (429), for more than one write a second to one key.
    TooManyRequests,
    /// A code no path acts on, kept verbatim for the logs.
    Other(String),
    /// No error body: a HEAD response, or one that didn't parse. The status is
    /// all there is.
    NoBody,
}

impl S3ErrorCode {
    pub(crate) fn from_code(code: &str) -> Self {
        match code {
            "AccessDenied" => Self::AccessDenied,
            "AuthorizationHeaderMalformed" => Self::AuthorizationHeaderMalformed,
            "BadDigest" => Self::BadDigest,
            "ConditionalRequestConflict" => Self::ConditionalRequestConflict,
            "EntityTooLarge" => Self::EntityTooLarge,
            "EntityTooSmall" => Self::EntityTooSmall,
            "ExpiredToken" => Self::ExpiredToken,
            "InternalError" => Self::InternalError,
            "InvalidAccessKeyId" => Self::InvalidAccessKeyId,
            "InvalidArgument" => Self::InvalidArgument,
            "InvalidBucketName" => Self::InvalidBucketName,
            "InvalidDigest" => Self::InvalidDigest,
            "InvalidObjectState" => Self::InvalidObjectState,
            "InvalidObjectName" => Self::InvalidObjectName,
            "InvalidPart" => Self::InvalidPart,
            "InvalidPartOrder" => Self::InvalidPartOrder,
            "InvalidRange" => Self::InvalidRange,
            "InvalidRequest" => Self::InvalidRequest,
            "MethodNotAllowed" => Self::MethodNotAllowed,
            "NoSuchBucket" => Self::NoSuchBucket,
            "NoSuchKey" => Self::NoSuchKey,
            "NoSuchUpload" => Self::NoSuchUpload,
            "NotImplemented" => Self::NotImplemented,
            "PermanentRedirect" => Self::PermanentRedirect,
            "PreconditionFailed" => Self::PreconditionFailed,
            "RequestRateLimitExceeded" => Self::RequestRateLimitExceeded,
            "RequestTimeTooSkewed" => Self::RequestTimeTooSkewed,
            "RequestTimeout" => Self::RequestTimeout,
            "ServiceUnavailable" => Self::ServiceUnavailable,
            "SignatureDoesNotMatch" => Self::SignatureDoesNotMatch,
            "SlowDown" => Self::SlowDown,
            "TooManyRequests" => Self::TooManyRequests,
            other => Self::Other(other.to_string()),
        }
    }
}

/// One S3 error: the status, the code, and what the body adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct S3Error {
    pub status: StatusCode,
    pub code: S3ErrorCode,
    /// `<Message>`, for the logs only. ❌ Never branch on it.
    pub message: Option<String>,
    pub request_id: Option<String>,
    /// `<Region>` on `AuthorizationHeaderMalformed`: the bucket's real region.
    pub region: Option<String>,
    /// `<Endpoint>` on `PermanentRedirect`.
    pub endpoint: Option<String>,
}

impl S3Error {
    /// An error known only by its status.
    pub(crate) fn from_status(status: StatusCode) -> Self {
        Self {
            status,
            code: S3ErrorCode::NoBody,
            message: None,
            request_id: None,
            region: None,
            endpoint: None,
        }
    }

    /// The error a non-2xx response carries: its `<Error>` body when it has
    /// one, else the status alone.
    pub(crate) fn from_response(status: StatusCode, body: &str) -> Self {
        crate::xml::parse_error_body(body).map_or_else(
            || Self::from_status(status),
            |mut error| {
                error.status = status;
                error
            },
        )
    }

    /// The key, bucket, or upload isn't there.
    pub(crate) fn is_not_found(&self) -> bool {
        match self.code {
            S3ErrorCode::NoSuchKey | S3ErrorCode::NoSuchBucket | S3ErrorCode::NoSuchUpload => true,
            S3ErrorCode::NoBody => self.status == StatusCode::NOT_FOUND,
            _ => false,
        }
    }

    /// A conditional write found the object already there.
    pub(crate) fn is_precondition_failed(&self) -> bool {
        match self.code {
            S3ErrorCode::PreconditionFailed => true,
            S3ErrorCode::NoBody => self.status == StatusCode::PRECONDITION_FAILED,
            _ => false,
        }
    }

    /// The server doesn't do what we asked. On a conditional write, the
    /// profile downgrades to check-then-write (`profile.rs`).
    pub(crate) fn is_not_implemented(&self) -> bool {
        match self.code {
            S3ErrorCode::NotImplemented => true,
            S3ErrorCode::NoBody => self.status == StatusCode::NOT_IMPLEMENTED,
            _ => false,
        }
    }

    /// Worth retrying after a back-off: a throttle or a transient server fault.
    pub(crate) fn is_retryable(&self) -> bool {
        match self.code {
            S3ErrorCode::SlowDown
            | S3ErrorCode::ServiceUnavailable
            | S3ErrorCode::InternalError
            | S3ErrorCode::TooManyRequests
            | S3ErrorCode::RequestRateLimitExceeded
            | S3ErrorCode::RequestTimeout => true,
            S3ErrorCode::NoBody | S3ErrorCode::Other(_) => matches!(
                self.status,
                StatusCode::INTERNAL_SERVER_ERROR
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::TOO_MANY_REQUESTS
                    | StatusCode::BAD_GATEWAY
                    | StatusCode::GATEWAY_TIMEOUT
            ),
            _ => false,
        }
    }

    /// The server asking us to slow down (`SlowDown`, a 503, a 429, Wasabi's
    /// and R2's own codes), as opposed to a fault: a server-side copy halves
    /// how many parts it keeps in flight on one of these.
    pub(crate) fn is_throttle(&self) -> bool {
        match self.code {
            S3ErrorCode::SlowDown
            | S3ErrorCode::ServiceUnavailable
            | S3ErrorCode::TooManyRequests
            | S3ErrorCode::RequestRateLimitExceeded => true,
            S3ErrorCode::NoBody | S3ErrorCode::Other(_) => matches!(
                self.status,
                StatusCode::SERVICE_UNAVAILABLE | StatusCode::TOO_MANY_REQUESTS
            ),
            _ => false,
        }
    }
}

impl fmt::Display for S3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.code {
            S3ErrorCode::NoBody => write!(f, "HTTP {}", self.status.as_u16())?,
            S3ErrorCode::Other(code) => write!(f, "{code} (HTTP {})", self.status.as_u16())?,
            code => write!(f, "{code:?} (HTTP {})", self.status.as_u16())?,
        }
        if let Some(id) = &self.request_id {
            write!(f, ", request {id}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod error_test;

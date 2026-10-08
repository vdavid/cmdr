//! The error classification, by code and by status.

use http::StatusCode;

use super::{S3Error, S3ErrorCode};

#[test]
fn a_response_body_supplies_the_code_and_the_status_comes_from_the_response() {
    let error = S3Error::from_response(
        StatusCode::NOT_FOUND,
        "<Error><Code>NoSuchKey</Code><Message>The resource you requested does not exist</Message>\
         <Resource>/mybucket/myfoto.jpg</Resource><RequestId>4442587FB7D0A2F9</RequestId></Error>",
    );
    assert_eq!(error.status, StatusCode::NOT_FOUND);
    assert_eq!(error.code, S3ErrorCode::NoSuchKey);
    assert!(error.is_not_found());
    assert_eq!(error.to_string(), "NoSuchKey (HTTP 404), request 4442587FB7D0A2F9");
}

#[test]
fn a_bodyless_response_classifies_by_status() {
    let head_404 = S3Error::from_response(StatusCode::NOT_FOUND, "");
    assert_eq!(head_404.code, S3ErrorCode::NoBody);
    assert!(head_404.is_not_found());
    assert!(S3Error::from_status(StatusCode::PRECONDITION_FAILED).is_precondition_failed());
    assert!(S3Error::from_status(StatusCode::NOT_IMPLEMENTED).is_not_implemented());
    assert!(S3Error::from_status(StatusCode::SERVICE_UNAVAILABLE).is_retryable());
    assert!(!S3Error::from_status(StatusCode::FORBIDDEN).is_retryable());
    assert_eq!(head_404.to_string(), "HTTP 404");
}

#[test]
fn an_html_error_page_from_a_proxy_classifies_by_status() {
    let error = S3Error::from_response(
        StatusCode::BAD_GATEWAY,
        "<html><body><h1>502 Bad Gateway</h1></body></html>",
    );
    assert_eq!(error.code, S3ErrorCode::NoBody);
    assert!(error.is_retryable());
}

#[test]
fn the_code_wins_over_the_status() {
    // A 403 that says `InvalidObjectState` is an archived object, not a refusal.
    let archived = S3Error::from_response(
        StatusCode::FORBIDDEN,
        "<Error><Code>InvalidObjectState</Code><Message>The operation is not valid for the object's storage class</Message></Error>",
    );
    assert_eq!(archived.code, S3ErrorCode::InvalidObjectState);
    // A 404 naming the bucket is about the bucket.
    let bucket = S3Error::from_response(StatusCode::NOT_FOUND, "<Error><Code>NoSuchBucket</Code></Error>");
    assert_eq!(bucket.code, S3ErrorCode::NoSuchBucket);
    // A 501 to a conditional header, B2-style.
    let b2 = S3Error::from_response(
        StatusCode::NOT_IMPLEMENTED,
        "<Error><Code>NotImplemented</Code></Error>",
    );
    assert!(b2.is_not_implemented());
    // A 412 with its code.
    let clash = S3Error::from_response(
        StatusCode::PRECONDITION_FAILED,
        "<Error><Code>PreconditionFailed</Code></Error>",
    );
    assert!(clash.is_precondition_failed());
}

#[test]
fn every_throttle_is_retryable() {
    for (status, code) in [
        (StatusCode::SERVICE_UNAVAILABLE, "SlowDown"),
        (StatusCode::TOO_MANY_REQUESTS, "TooManyRequests"),
        (StatusCode::SERVICE_UNAVAILABLE, "ServiceUnavailable"),
        (StatusCode::INTERNAL_SERVER_ERROR, "InternalError"),
        (StatusCode::FORBIDDEN, "RequestRateLimitExceeded"),
    ] {
        let error = S3Error::from_response(status, &format!("<Error><Code>{code}</Code></Error>"));
        assert!(error.is_retryable(), "{code}");
    }
    let denied = S3Error::from_response(StatusCode::FORBIDDEN, "<Error><Code>AccessDenied</Code></Error>");
    assert!(!denied.is_retryable());
}

#[test]
fn an_unknown_code_is_kept_verbatim() {
    let error = S3Error::from_response(
        StatusCode::BAD_REQUEST,
        "<Error><Code>XAmzContentSHA256Mismatch</Code></Error>",
    );
    assert_eq!(error.code, S3ErrorCode::Other("XAmzContentSHA256Mismatch".into()));
    assert_eq!(error.to_string(), "XAmzContentSHA256Mismatch (HTTP 400)");
    assert!(!error.is_not_found());
}

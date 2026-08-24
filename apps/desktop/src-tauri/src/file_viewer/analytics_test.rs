//! Unit tests for the viewer's analytics vocabulary.
//!
//! These pin the two things a dashboard reads months later: that the buckets step
//! where the viewer's own behavior steps, and that a failure is told apart by a
//! token rather than by a message nobody may ship a second time.

use super::ViewerError;
use super::analytics::{content_token, failure_token, size_bucket};
use super::content_kind::ViewerContentKind;

const MB: u64 = 1024 * 1024;

#[test]
fn size_buckets_step_at_the_backend_boundaries() {
    assert_eq!(size_bucket(0), "<1MB");
    // The `FullLoad` ceiling: one byte over and the viewer opens on `ByteSeek`.
    assert_eq!(size_bucket(MB - 1), "<1MB");
    assert_eq!(size_bucket(MB), "1-10MB");
    assert_eq!(size_bucket(10 * MB - 1), "1-10MB");
    assert_eq!(size_bucket(10 * MB), "10-100MB");
    assert_eq!(size_bucket(100 * MB), "100MB-1GB");
    assert_eq!(size_bucket(1024 * MB), "1GB+");
    assert_eq!(size_bucket(u64::MAX), "1GB+");
}

#[test]
fn content_tokens_cover_every_kind() {
    assert_eq!(content_token(ViewerContentKind::Text), "text");
    assert_eq!(content_token(ViewerContentKind::Image), "image");
    assert_eq!(content_token(ViewerContentKind::Pdf), "pdf");
}

#[test]
fn failure_tokens_carry_no_payload() {
    // Every variant that holds a string holds one the user typed or the OS wrote;
    // the token must survive the trip without any of it.
    let cases: [(ViewerError, &str); 4] = [
        (
            ViewerError::NotFound {
                path: "/Users/someone/taxes 2025.pdf".to_string(),
            },
            "not_found",
        ),
        (
            ViewerError::Io {
                message: "Permission denied (os error 13)".to_string(),
            },
            "io",
        ),
        (ViewerError::ExtractTooLarge { size: 9, cap: 2 }, "extract_too_large"),
        (
            ViewerError::Archive {
                message: "unsupported codec".to_string(),
            },
            "archive",
        ),
    ];
    for (error, expected) in cases {
        let token = failure_token(&error);
        assert_eq!(token, expected);
        assert!(!token.contains(' '), "a token must stay a single word, got {token}");
    }
}

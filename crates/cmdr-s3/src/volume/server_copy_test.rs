//! What a server-side copy reads off its source's HEAD, and how its window of
//! parts in flight breathes.

use std::time::{Duration, UNIX_EPOCH};

use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};

use cmdr_fs::volume::VolumeError;

use super::{SourceObject, Window, part_refusal};
use crate::transport::Answer;

fn head(pairs: &[(&str, &str)]) -> Answer {
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    Answer {
        status: StatusCode::OK,
        headers,
        body: Vec::new(),
    }
}

#[test]
fn a_source_with_its_own_mtime_keeps_it_through_a_restated_copy() {
    let object = SourceObject::from_head(&head(&[
        ("content-length", "42"),
        ("etag", "\"abc\""),
        ("x-amz-meta-mtime", "1354040105.5"),
        ("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT"),
    ]));
    assert_eq!(object.size, 42);
    assert_eq!(object.etag.as_deref(), Some("\"abc\""));
    assert_eq!(
        object.mtime,
        Some(UNIX_EPOCH + Duration::from_millis(1_354_040_105_500))
    );
    assert_eq!(object.restated().mtime, object.mtime);
}

/// ❗ The date survives a copy even when the source never carried an mtime:
/// its upload time becomes the copy's mtime, and the rest of its metadata
/// (system headers and user metadata) is restated beside it.
#[test]
fn a_source_without_an_mtime_has_its_upload_time_written_as_one() {
    let object = SourceObject::from_head(&head(&[
        ("content-length", "7"),
        ("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT"),
        ("content-type", "image/jpeg"),
        ("x-amz-meta-camera", "x100"),
        ("x-amz-meta-cmdr-write", "token-of-another-write"),
        ("x-amz-request-id", "not-metadata"),
    ]));
    let metadata = object.restated();
    assert_eq!(
        metadata.mtime,
        httpdate::parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT").ok()
    );
    assert_eq!(metadata.write_token, None);
    let mut carried = metadata.carried.clone();
    carried.sort();
    assert_eq!(
        carried,
        vec![
            ("content-type".to_string(), "image/jpeg".to_string()),
            ("x-amz-meta-camera".to_string(), "x100".to_string()),
        ],
        "another write's token and non-metadata headers stay behind"
    );
}

#[test]
fn a_source_with_no_date_at_all_is_copied_as_it_is() {
    let object = SourceObject::from_head(&head(&[("content-length", "1")]));
    assert_eq!(object.mtime, None);
    assert_eq!(object.restated().mtime, object.mtime);
}

#[test]
fn the_window_halves_on_a_throttle_and_grows_back_one_part_at_a_time() {
    let mut window = Window::new(16);
    assert_eq!(window.width, 16);
    window.throttled();
    assert_eq!(window.width, 8);
    window.throttled();
    window.throttled();
    window.throttled();
    window.throttled();
    assert_eq!(window.width, 1, "never below one part");
    window.landed();
    window.landed();
    assert_eq!(window.width, 3);
    for _ in 0..40 {
        window.landed();
    }
    assert_eq!(window.width, 16, "never past the ceiling");
}

fn refused(status: u16, code: &str) -> crate::error::S3Error {
    crate::error::S3Error::from_response(
        StatusCode::from_u16(status).unwrap(),
        &format!("<Error><Code>{code}</Code><Message>m</Message></Error>"),
    )
}

/// ❗ A part copy's range comes from the source's HEAD, so a 416 means the
/// source shrank since: Spaces ignores the ETag pin and answers that way when
/// a smaller object replaced the source mid-copy (live, 2026-10-02).
#[test]
fn a_part_copy_past_the_sources_end_is_the_source_changing() {
    let gone = std::sync::atomic::AtomicBool::new(false);
    let shrank = part_refusal(&refused(416, "InvalidRange"), true, "/b/src", "/b/dst", &gone);
    assert!(
        matches!(&shrank, VolumeError::SourceChanged(path) if path == "/b/src"),
        "{shrank:?}"
    );
    let pinned = part_refusal(&refused(412, "PreconditionFailed"), true, "/b/src", "/b/dst", &gone);
    assert!(matches!(pinned, VolumeError::SourceChanged(_)), "{pinned:?}");
    let denied = part_refusal(&refused(403, "AccessDenied"), true, "/b/src", "/b/dst", &gone);
    assert!(matches!(denied, VolumeError::PermissionDenied { .. }), "{denied:?}");
}

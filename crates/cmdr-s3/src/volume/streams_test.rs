use http::StatusCode;

use super::{Answered, judge_get};

#[test]
fn a_206_is_the_window_asked_for_and_names_the_full_length() {
    assert_eq!(
        judge_get(StatusCode::PARTIAL_CONTENT, Some("bytes 100-199/1000"), Some(100), 100),
        Answered::Window { total: 1000, skip: 0 }
    );
}

#[test]
fn a_206_without_content_range_counts_the_offset_in() {
    assert_eq!(
        judge_get(StatusCode::PARTIAL_CONTENT, None, Some(900), 100),
        Answered::Window { total: 1000, skip: 0 }
    );
}

#[test]
fn a_200_to_a_ranged_request_is_skipped_to_locally() {
    // RFC 9110 lets a server ignore `Range`: the body is the whole object.
    assert_eq!(
        judge_get(StatusCode::OK, None, Some(1000), 100),
        Answered::Window { total: 1000, skip: 100 }
    );
}

#[test]
fn a_whole_read_skips_nothing() {
    assert_eq!(
        judge_get(StatusCode::OK, None, Some(1000), 0),
        Answered::Window { total: 1000, skip: 0 }
    );
}

#[test]
fn a_416_is_past_the_end_and_still_names_the_length() {
    assert_eq!(
        judge_get(StatusCode::RANGE_NOT_SATISFIABLE, Some("bytes */1000"), None, 1000),
        Answered::PastTheEnd { total: 1000 }
    );
}

#[test]
fn every_other_status_is_a_refusal_whose_body_says_why() {
    for status in [
        StatusCode::FORBIDDEN,
        StatusCode::NOT_FOUND,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::MOVED_PERMANENTLY,
    ] {
        assert_eq!(judge_get(status, None, Some(10), 0), Answered::Refused, "{status}");
    }
}

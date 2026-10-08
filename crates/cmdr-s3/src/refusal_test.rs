//! The connect probe's refusal table, one row per answer a real server gives.
//! The fixture README's "What each server answered" is where the bodies come
//! from.

use http::StatusCode;

use super::{BucketCheck, BucketList, S3ConnectError, judge_head_bucket, judge_list_buckets};

const LISTED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner><ID>o</ID></Owner>
  <Buckets><Bucket><Name>cmdr-test</Name><CreationDate>2026-10-01T10:00:00.000Z</CreationDate></Bucket></Buckets>
</ListAllMyBucketsResult>"#;

fn error_body(code: &str) -> String {
    format!("<?xml version=\"1.0\"?><Error><Code>{code}</Code><Message>whatever the server says</Message></Error>")
}

#[test]
fn a_listing_means_the_keys_work() {
    assert_eq!(judge_list_buckets(StatusCode::OK, LISTED, false), BucketList::Listed);
    assert_eq!(judge_list_buckets(StatusCode::OK, LISTED, true), BucketList::Listed);
}

#[test]
fn a_wrong_secret_is_the_keys_whatever_the_place() {
    // VersityGW's answer to a wrong secret.
    let body = error_body("SignatureDoesNotMatch");
    for wants_bucket in [false, true] {
        assert_eq!(
            judge_list_buckets(StatusCode::FORBIDDEN, &body, wants_bucket),
            BucketList::Refused(S3ConnectError::KeysRejected)
        );
    }
}

#[test]
fn an_unknown_key_id_is_the_keys() {
    assert_eq!(
        judge_list_buckets(StatusCode::FORBIDDEN, &error_body("InvalidAccessKeyId"), false),
        BucketList::Refused(S3ConnectError::KeysRejected)
    );
}

/// R2's answer to a well-formed key id it doesn't know: `401` with
/// `<Code>Unauthorized</Code>` on a GET, a bodyless `401` on a HEAD (live,
/// 2026-10-02). Without this row a mistyped key id there read as "not an S3
/// endpoint" on a bucket and as transport trouble on the account root.
#[test]
fn a_401_is_the_keys_whatever_the_place() {
    for wants_bucket in [false, true] {
        for body in [error_body("Unauthorized"), String::new()] {
            assert_eq!(
                judge_list_buckets(StatusCode::UNAUTHORIZED, &body, wants_bucket),
                BucketList::Refused(S3ConnectError::KeysRejected),
                "{body:?}"
            );
        }
    }
    assert_eq!(
        judge_head_bucket(StatusCode::UNAUTHORIZED, None),
        BucketCheck::Refused(S3ConnectError::KeysRejected)
    );
}

#[test]
fn access_denied_on_the_account_root_says_the_list_was_refused() {
    // ❗ Ambiguous by nature: Garage answers a WRONG secret this way, and AWS a
    // right key without `s3:ListAllMyBuckets`. The refusal's sentence covers
    // both, so it can't be `KeysRejected`.
    assert_eq!(
        judge_list_buckets(StatusCode::FORBIDDEN, &error_body("AccessDenied"), false),
        BucketList::Refused(S3ConnectError::BucketListRefused)
    );
}

#[test]
fn access_denied_with_a_bucket_typed_falls_back_to_the_bucket() {
    // A bucket-scoped key (R2 non-admin tokens, B2 keys without
    // `listAllBucketNames`) can't list buckets and can still open its own.
    assert_eq!(
        judge_list_buckets(StatusCode::FORBIDDEN, &error_body("AccessDenied"), true),
        BucketList::FallBack
    );
}

#[test]
fn a_skewed_clock_is_its_own_refusal() {
    assert_eq!(
        judge_list_buckets(StatusCode::FORBIDDEN, &error_body("RequestTimeTooSkewed"), false),
        BucketList::Refused(S3ConnectError::ClockSkewed)
    );
}

#[test]
fn an_html_page_is_not_an_s3_endpoint() {
    let page = "<!doctype html><html><body>Welcome to nginx</body></html>";
    assert_eq!(
        judge_list_buckets(StatusCode::OK, page, false),
        BucketList::Refused(S3ConnectError::NotAnS3Endpoint)
    );
    assert_eq!(
        judge_list_buckets(StatusCode::NOT_FOUND, page, true),
        BucketList::Refused(S3ConnectError::NotAnS3Endpoint)
    );
}

#[test]
fn a_server_fault_is_transport_trouble() {
    assert!(matches!(
        judge_list_buckets(StatusCode::SERVICE_UNAVAILABLE, "", false),
        BucketList::Refused(S3ConnectError::Transport(_))
    ));
    assert!(matches!(
        judge_list_buckets(StatusCode::SERVICE_UNAVAILABLE, &error_body("SlowDown"), false),
        BucketList::Refused(S3ConnectError::Transport(_))
    ));
}

#[test]
fn a_bucket_that_answers_is_open() {
    assert_eq!(judge_head_bucket(StatusCode::OK, None), BucketCheck::Open);
}

#[test]
fn a_missing_bucket_is_named_as_missing() {
    assert_eq!(
        judge_head_bucket(StatusCode::NOT_FOUND, None),
        BucketCheck::Refused(S3ConnectError::NoSuchBucket)
    );
}

#[test]
fn a_bucket_in_another_region_names_the_region() {
    // AWS answers a HEAD on the wrong regional endpoint 301 with the bucket's
    // real region in `x-amz-bucket-region`.
    assert_eq!(
        judge_head_bucket(StatusCode::MOVED_PERMANENTLY, Some("us-west-2")),
        BucketCheck::Refused(S3ConnectError::WrongRegion {
            region: Some("us-west-2".to_string())
        })
    );
    assert_eq!(
        judge_head_bucket(StatusCode::MOVED_PERMANENTLY, None),
        BucketCheck::Refused(S3ConnectError::WrongRegion { region: None })
    );
}

#[test]
fn a_refused_bucket_is_access_denied() {
    // ❗ A HEAD has no body, so a 403 can't tell a wrong secret from a key
    // without rights to this bucket. `AccessDenied`'s sentence asks about both.
    assert_eq!(
        judge_head_bucket(StatusCode::FORBIDDEN, None),
        BucketCheck::Refused(S3ConnectError::AccessDenied)
    );
}

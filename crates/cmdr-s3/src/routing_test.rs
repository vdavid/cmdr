//! Reading an answer for its bucket's region, and the per-bucket memory.

use http::StatusCode;

use super::{BucketRegions, RegionHint, read_hint};

const MALFORMED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Error><Code>AuthorizationHeaderMalformed</Code><Message>the region 'us-east-1' is wrong; expecting 'eu-west-1'</Message><Region>eu-west-1</Region><RequestId>r</RequestId></Error>"#;

const REDIRECT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Error><Code>PermanentRedirect</Code><Message>use the endpoint below</Message><Endpoint>far.s3.eu-west-1.amazonaws.com</Endpoint><Bucket>far</Bucket></Error>"#;

fn hint(misrouted: bool, region: Option<&str>) -> RegionHint {
    RegionHint {
        misrouted,
        region: region.map(str::to_string),
    }
}

#[test]
fn a_redirect_is_misrouted_and_names_the_region_in_its_header() {
    for status in [StatusCode::MOVED_PERMANENTLY, StatusCode::TEMPORARY_REDIRECT] {
        assert_eq!(
            read_hint(status, Some("eu-west-1"), REDIRECT),
            hint(true, Some("eu-west-1")),
            "{status}"
        );
    }
}

#[test]
fn a_redirect_without_the_header_is_misrouted_with_no_region() {
    // A HEAD has no body, and `<Endpoint>` is a hostname, not a region.
    assert_eq!(read_hint(StatusCode::MOVED_PERMANENTLY, None, ""), hint(true, None));
    assert_eq!(
        read_hint(StatusCode::MOVED_PERMANENTLY, None, REDIRECT),
        hint(true, None)
    );
}

#[test]
fn a_wrong_signing_region_is_misrouted_and_its_body_names_the_region() {
    assert_eq!(
        read_hint(StatusCode::BAD_REQUEST, None, MALFORMED),
        hint(true, Some("eu-west-1"))
    );
}

#[test]
fn any_other_400_is_not_about_the_region() {
    let invalid = "<Error><Code>InvalidArgument</Code><Message>no</Message></Error>";
    assert_eq!(read_hint(StatusCode::BAD_REQUEST, None, invalid), hint(false, None));
}

#[test]
fn a_success_still_teaches_the_region_its_header_names() {
    // `HeadBucket` answers `x-amz-bucket-region` on a 200 too.
    assert_eq!(
        read_hint(StatusCode::OK, Some("us-east-1"), ""),
        hint(false, Some("us-east-1"))
    );
    assert_eq!(read_hint(StatusCode::OK, None, ""), hint(false, None));
    assert_eq!(
        read_hint(StatusCode::FORBIDDEN, Some("ap-south-1"), ""),
        hint(false, Some("ap-south-1"))
    );
}

#[test]
fn a_region_a_hostname_cant_carry_is_never_a_hint() {
    for junk in ["x.evil.com", "evil.com/", "", "EU-WEST-1"] {
        assert_eq!(
            read_hint(StatusCode::MOVED_PERMANENTLY, Some(junk), ""),
            hint(true, None),
            "{junk:?}"
        );
    }
}

#[test]
fn the_memory_keeps_one_region_per_bucket_and_says_when_it_changed() {
    let regions = BucketRegions::default();
    assert_eq!(regions.region_of("far"), None);

    assert!(regions.learn("far", "eu-west-1"));
    assert!(!regions.learn("far", "eu-west-1"));
    assert_eq!(regions.region_of("far").as_deref(), Some("eu-west-1"));
    assert_eq!(regions.region_of("near"), None);

    assert!(regions.learn("far", "ap-south-1"));
    assert_eq!(regions.region_of("far").as_deref(), Some("ap-south-1"));
}

#[test]
fn the_memory_ignores_a_region_a_hostname_cant_carry() {
    let regions = BucketRegions::default();
    assert!(!regions.learn("far", "x.evil.com"));
    assert_eq!(regions.region_of("far"), None);
}

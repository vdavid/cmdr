//! The signer against AWS's published S3 examples.
//!
//! Every vector comes from the S3 API reference's "Signature Calculations for
//! the Authorization Header: Transferring Payload in a Single Chunk" and
//! "Authenticating Requests: Using Query Parameters" examples: the example key
//! pair, bucket `examplebucket`, `us-east-1`, 2013-05-24T00:00:00Z. The same
//! vectors are pinned by `s3s-project/s3s` (`crates/s3s-sigv4/src/methods.rs`)
//! and `durch/rust-s3`, which is where they were cross-checked on 2026-10-01
//! (the AWS pages now redirect to the API index).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use http::{HeaderName, HeaderValue, Method};

use super::{AmzTime, Credentials, MAX_PRESIGN_EXPIRY, PresignError, Scope, presign, sign, trim_all};
use crate::encoding::encode_key;
use crate::request::{Body, S3Request};

const HOST: &str = "examplebucket.s3.amazonaws.com";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const CREDENTIAL: &str = "AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request";

fn credentials() -> Credentials {
    Credentials::new("AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY")
}

fn example_time() -> AmzTime {
    AmzTime::new(UNIX_EPOCH + Duration::from_secs(1_369_353_600))
}

fn header(request: &super::SignedRequest, name: &str) -> String {
    request
        .headers
        .get(name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .to_str()
        .unwrap()
        .to_string()
}

fn sign_example(request: S3Request) -> super::SignedRequest {
    let credentials = credentials();
    let time = example_time();
    sign(
        request,
        &Scope {
            credentials: &credentials,
            region: "us-east-1",
            time: &time,
        },
    )
}

#[test]
fn the_signing_time_is_compact_utc() {
    assert_eq!(example_time().stamp(), "20130524T000000Z");
}

#[test]
fn get_object_matches_the_aws_example() {
    let request = S3Request::new(Method::GET, "https", HOST, "/test.txt".to_string())
        .header(HeaderName::from_static("range"), HeaderValue::from_static("bytes=0-9"));

    let signed = sign_example(request);

    assert_eq!(header(&signed, "x-amz-date"), "20130524T000000Z");
    assert_eq!(header(&signed, "x-amz-content-sha256"), EMPTY_SHA256);
    assert_eq!(
        header(&signed, "authorization"),
        format!(
            "AWS4-HMAC-SHA256 Credential={CREDENTIAL},SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,\
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        )
    );
    assert_eq!(signed.url.as_str(), "https://examplebucket.s3.amazonaws.com/test.txt");
}

#[test]
fn put_object_matches_the_aws_example_with_a_hashed_payload_and_an_encoded_key() {
    let mut request = S3Request::new(
        Method::PUT,
        "https",
        HOST,
        format!("/{}", encode_key("test$file.text").unwrap()),
    )
    .header(
        HeaderName::from_static("date"),
        HeaderValue::from_static("Fri, 24 May 2013 00:00:00 GMT"),
    )
    .header(
        HeaderName::from_static("x-amz-storage-class"),
        HeaderValue::from_static("REDUCED_REDUNDANCY"),
    );
    request.body = Body::Bytes(b"Welcome to Amazon S3.".to_vec());

    let signed = sign_example(request);

    assert_eq!(
        header(&signed, "x-amz-content-sha256"),
        "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072"
    );
    assert_eq!(
        header(&signed, "authorization"),
        format!(
            "AWS4-HMAC-SHA256 Credential={CREDENTIAL},\
             SignedHeaders=date;host;x-amz-content-sha256;x-amz-date;x-amz-storage-class,\
             Signature=98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd"
        )
    );
    assert_eq!(
        signed.url.as_str(),
        "https://examplebucket.s3.amazonaws.com/test%24file.text"
    );
}

#[test]
fn a_valueless_query_parameter_matches_the_lifecycle_example() {
    let request = S3Request::new(Method::GET, "https", HOST, "/".to_string()).query("lifecycle", "");

    let signed = sign_example(request);

    assert!(header(&signed, "authorization").ends_with(
        "SignedHeaders=host;x-amz-content-sha256;x-amz-date,\
         Signature=fea454ca298b7da1c68078a5d1bdbfbbe0d65c699e0f91ac7a200a0136783543"
    ));
    assert_eq!(signed.url.as_str(), "https://examplebucket.s3.amazonaws.com/?lifecycle");
}

#[test]
fn query_parameters_are_sorted_like_the_list_objects_example() {
    // Added out of order on purpose: the signer sorts.
    let request = S3Request::new(Method::GET, "https", HOST, "/".to_string())
        .query("prefix", "J")
        .query("max-keys", "2");

    let signed = sign_example(request);

    assert!(
        header(&signed, "authorization")
            .ends_with("Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7")
    );
    assert_eq!(
        signed.url.as_str(),
        "https://examplebucket.s3.amazonaws.com/?max-keys=2&prefix=J"
    );
}

#[test]
fn a_streamed_body_is_signed_unsigned_payload() {
    let mut request = S3Request::new(Method::PUT, "https", HOST, "/big.bin".to_string());
    request.body = Body::Streamed { length: 10 };

    let signed = sign_example(request);

    assert_eq!(header(&signed, "x-amz-content-sha256"), "UNSIGNED-PAYLOAD");
}

/// ❗ The host travels in the URL alone: an explicit `host` header beside
/// HTTP/2's `:authority` makes Google's front end reset the stream
/// (`PROTOCOL_ERROR`), so every GCS request failed in the app, whose `reqwest`
/// speaks HTTP/2 (live, GCS, 2026-10-02). The transport derives `Host` or
/// `:authority` from the URL, so the URL's spelling is what gets signed.
#[test]
fn the_signed_request_carries_no_explicit_host_header() {
    let signed = sign_example(S3Request::new(Method::GET, "https", HOST, "/a".to_string()));

    assert!(
        signed.headers.get("host").is_none(),
        "the host must come from the URL, never a header of its own"
    );
}

#[test]
fn a_non_default_port_is_signed_and_kept_in_the_url() {
    let with_port = sign_example(S3Request::new(
        Method::GET,
        "http",
        "127.0.0.1:17480",
        "/bucket/a".to_string(),
    ));
    let without = sign_example(S3Request::new(
        Method::GET,
        "http",
        "127.0.0.1",
        "/bucket/a".to_string(),
    ));

    assert_eq!(with_port.url.as_str(), "http://127.0.0.1:17480/bucket/a");
    assert_ne!(
        header(&with_port, "authorization"),
        header(&without, "authorization"),
        "the port is part of the signed host"
    );
}

#[test]
fn the_host_is_signed_as_the_url_spells_it() {
    // The URL lowercases a host and drops a scheme's default port; the
    // transport sends that spelling, so the signature must cover it too.
    let respelled = sign_example(S3Request::new(
        Method::GET,
        "https",
        "ExampleBucket.S3.amazonaws.com:443",
        "/test.txt".to_string(),
    ));
    let canonical = sign_example(S3Request::new(Method::GET, "https", HOST, "/test.txt".to_string()));

    assert_eq!(
        respelled.url.as_str(),
        "https://examplebucket.s3.amazonaws.com/test.txt"
    );
    assert_eq!(header(&respelled, "authorization"), header(&canonical, "authorization"));
}

#[test]
fn header_values_are_trimmed_and_inner_spaces_collapsed_before_signing() {
    assert_eq!(trim_all("  a   b  c "), "a b c");
    assert_eq!(trim_all("plain"), "plain");
}

#[test]
fn a_presigned_get_matches_the_aws_query_auth_example() {
    let credentials = credentials();
    let time = example_time();
    let request = S3Request::new(Method::GET, "https", HOST, "/test.txt".to_string());

    let url = presign(
        &request,
        &Scope {
            credentials: &credentials,
            region: "us-east-1",
            time: &time,
        },
        Duration::from_secs(86_400),
    )
    .unwrap();

    assert_eq!(
        url.as_str(),
        "https://examplebucket.s3.amazonaws.com/test.txt?X-Amz-Algorithm=AWS4-HMAC-SHA256\
         &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request\
         &X-Amz-Date=20130524T000000Z&X-Amz-Expires=86400&X-Amz-SignedHeaders=host\
         &X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
    );
}

#[test]
fn a_presigned_url_lives_at_most_seven_days() {
    let credentials = credentials();
    let time = AmzTime::new(SystemTime::now());
    let scope = Scope {
        credentials: &credentials,
        region: "auto",
        time: &time,
    };
    let request = S3Request::new(Method::GET, "https", HOST, "/a".to_string());

    assert!(presign(&request, &scope, MAX_PRESIGN_EXPIRY).is_ok());
    assert_eq!(
        presign(&request, &scope, MAX_PRESIGN_EXPIRY + Duration::from_secs(1)),
        Err(PresignError::ExpiryOutOfRange)
    );
    assert_eq!(
        presign(&request, &scope, Duration::ZERO),
        Err(PresignError::ExpiryOutOfRange)
    );
}

/// ❗ GCS's own dialect, for a request carrying `x-goog-if-generation-match`
/// (which GCS refuses beside any `x-amz-*` header): `GOOG4-HMAC-SHA256`, a
/// `storage/goog4_request` scope keyed from `GOOG4<secret>`, `x-goog-date` and
/// `x-goog-content-sha256`, and every `x-amz-*` header spelled `x-goog-*`. The
/// signature was computed independently with `openssl dgst -mac HMAC` from
/// the canonical request GCS's V4 spec gives (AWS's example key pair and
/// time, region `auto`).
#[test]
fn a_gcs_dialect_request_signs_as_goog4_with_its_headers_spelled_x_goog() {
    let mut request = S3Request::new(Method::PUT, "https", "storage.googleapis.com", "/b/k".to_string());
    request.dialect = crate::request::Dialect::Goog;
    let request = request
        .header(
            HeaderName::from_static("x-amz-meta-mtime"),
            HeaderValue::from_static("1"),
        )
        .header(
            HeaderName::from_static("x-goog-if-generation-match"),
            HeaderValue::from_static("0"),
        );
    let credentials = credentials();
    let time = example_time();
    let signed = sign(
        request,
        &Scope {
            credentials: &credentials,
            region: "auto",
            time: &time,
        },
    );

    assert!(
        signed.headers.keys().all(|name| !name.as_str().starts_with("x-amz-")),
        "{:?}",
        signed.headers.keys().collect::<Vec<_>>()
    );
    assert_eq!(header(&signed, "x-goog-meta-mtime"), "1");
    assert_eq!(header(&signed, "x-goog-date"), "20130524T000000Z");
    assert_eq!(header(&signed, "x-goog-content-sha256"), EMPTY_SHA256);
    assert_eq!(
        header(&signed, "authorization"),
        "GOOG4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/auto/storage/goog4_request,\
         SignedHeaders=host;x-goog-content-sha256;x-goog-date;x-goog-if-generation-match;x-goog-meta-mtime,\
         Signature=6b48bb01aa54e0137ebe804932fa44fd35ea159c8ba2f2bc07ea7382551bdd9e"
    );
}

/// ❗ Written GCS's way, Cmdr's metadata goes out as `x-goog-meta-mtime` and
/// `x-goog-meta-cmdr-write`, and GCS answers an S3-signed HEAD of that object
/// with `x-amz-meta-mtime` and `x-amz-meta-cmdr-write` (live,
/// `live_create_new_metadata_reads_back_under_the_names_cmdr_reads`,
/// 2026-10-02): the names every reader (the stat, the landing check, the
/// cut-off cleanup) asks for on every provider.
#[test]
fn metadata_written_gcs_s_way_is_spelled_x_goog_and_read_back_as_x_amz() {
    use crate::metadata::{MTIME_HEADER, WRITE_TOKEN_HEADER};
    assert_eq!(MTIME_HEADER, "x-amz-meta-mtime");
    assert_eq!(WRITE_TOKEN_HEADER, "x-amz-meta-cmdr-write");

    let mut request = S3Request::new(Method::PUT, "https", "storage.googleapis.com", "/b/k".to_string());
    request.dialect = crate::request::Dialect::Goog;
    let request = request
        .header(HeaderName::from_static(MTIME_HEADER), HeaderValue::from_static("1"))
        .header(
            HeaderName::from_static(WRITE_TOKEN_HEADER),
            HeaderValue::from_static("t"),
        );
    let credentials = credentials();
    let time = example_time();
    let signed = sign(
        request,
        &Scope {
            credentials: &credentials,
            region: "auto",
            time: &time,
        },
    );
    assert_eq!(header(&signed, "x-goog-meta-mtime"), "1");
    assert_eq!(header(&signed, "x-goog-meta-cmdr-write"), "t");
    assert!(signed.headers.get(MTIME_HEADER).is_none());
    assert!(signed.headers.get(WRITE_TOKEN_HEADER).is_none());
}

#[test]
fn debug_never_prints_the_secret() {
    let printed = format!("{:?}", credentials());
    assert!(printed.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(!printed.contains("wJalrXUtnFEMI"));
}

/// ❗ A create-only copy on GCS goes out in GCS's dialect
/// (`x-goog-if-generation-match`), so its source pin travels as
/// `x-goog-copy-source-if-match`, beside `x-goog-copy-source`; GCS refuses
/// its own headers beside any `x-amz-*` one. A multipart ETag goes unpinned
/// there (`ProviderProfile::refuses_multipart_copy_pin`).
#[test]
fn a_create_only_gcs_copy_carries_its_pin_spelled_x_goog() {
    use crate::ops::{CopySource, MetadataDirective, Overwrite, copy_object};
    let gcs = crate::profile::ProviderProfile::from_preset(&crate::profile::Preset::Gcs).expect("a profile");
    let source = CopySource {
        bucket: "b",
        key: "src",
    };
    let signed = |pin: &str| {
        let built = copy_object(
            &gcs,
            source,
            Some(pin),
            "b",
            "dst",
            Overwrite::Refuse,
            &MetadataDirective::Copy,
        )
        .expect("builds");
        sign_example(built.request)
    };

    let single = signed("\"abc\"");
    assert!(
        single.headers.keys().all(|name| !name.as_str().starts_with("x-amz-")),
        "{:?}",
        single.headers.keys().collect::<Vec<_>>()
    );
    assert_eq!(header(&single, "x-goog-copy-source-if-match"), "\"abc\"");
    assert_eq!(header(&single, "x-goog-copy-source"), "/b/src");
    assert_eq!(header(&single, "x-goog-if-generation-match"), "0");

    let multipart = signed("\"abc-2\"");
    assert!(multipart.headers.get("x-goog-copy-source-if-match").is_none());
}

//! Each response parser against the documents AWS's API reference shows,
//! plus the shapes compatible servers vary on.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use http::StatusCode;

use super::*;
use crate::error::{S3Error, S3ErrorCode};

const NS: &str = r#"xmlns="http://s3.amazonaws.com/doc/2006-03-01/""#;

fn at(secs: u64) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

fn embedded(result: Result<impl std::fmt::Debug, BodyError>) -> S3Error {
    match result {
        Err(BodyError::Embedded(error)) => *error,
        other => panic!("expected an embedded error, got {other:?}"),
    }
}

const COPY_ERROR_IN_200: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Error><Code>SlowDown</Code><Message>Please reduce your request rate.</Message><RequestId>4442587FB7D0A2F9</RequestId></Error>"#;

#[test]
fn list_buckets_reads_names_dates_regions_and_the_continuation_token() {
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult {NS}>
  <Owner><ID>abc</ID><DisplayName>me</DisplayName></Owner>
  <Buckets>
    <Bucket><Name>photos</Name><CreationDate>2019-12-11T23:32:47+00:00</CreationDate><BucketRegion>eu-north-1</BucketRegion></Bucket>
    <Bucket><Name>backups</Name><CreationDate>2006-02-03T16:45:09.000Z</CreationDate></Bucket>
  </Buckets>
  <ContinuationToken>next-page</ContinuationToken>
</ListAllMyBucketsResult>"#
    );

    let page = parse_list_buckets(&body).unwrap();

    assert_eq!(
        page.buckets,
        vec![
            BucketEntry {
                name: "photos".into(),
                created: at(1_576_107_167),
                region: Some("eu-north-1".into())
            },
            BucketEntry {
                name: "backups".into(),
                created: at(1_138_985_109),
                region: None
            },
        ]
    );
    assert_eq!(page.continuation_token.as_deref(), Some("next-page"));
}

#[test]
fn an_account_with_no_buckets_lists_none() {
    let page = parse_list_buckets(&format!(
        "<ListAllMyBucketsResult {NS}><Buckets/></ListAllMyBucketsResult>"
    ))
    .unwrap();
    assert!(page.buckets.is_empty());
    assert_eq!(page.continuation_token, None);
}

#[test]
fn list_objects_reads_folders_files_and_paging() {
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult {NS}>
  <Name>bucket</Name><Prefix>photos/</Prefix><KeyCount>4</KeyCount><MaxKeys>1000</MaxKeys><Delimiter>/</Delimiter>
  <IsTruncated>true</IsTruncated>
  <Contents>
    <Key>photos/a &amp; b.jpg</Key><LastModified>2009-10-12T17:50:30.000Z</LastModified>
    <ETag>&quot;fba9dede5f27731c9771645a39863328&quot;</ETag><Size>434234</Size><StorageClass>STANDARD</StorageClass>
  </Contents>
  <Contents>
    <Key>photos/old.tif</Key><LastModified>2010-11-10T20:48:33.000Z</LastModified><Size>0</Size><StorageClass>DEEP_ARCHIVE</StorageClass>
  </Contents>
  <CommonPrefixes><Prefix>photos/2024/</Prefix></CommonPrefixes>
  <CommonPrefixes><Prefix>photos/ spaced /</Prefix></CommonPrefixes>
  <NextContinuationToken>1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=</NextContinuationToken>
</ListBucketResult>"#
    );

    let page = parse_list_objects(&body).unwrap();

    assert_eq!(
        page.prefixes,
        vec!["photos/2024/".to_string(), "photos/ spaced /".to_string()]
    );
    assert_eq!(
        page.objects,
        vec![
            ObjectEntry {
                key: "photos/a & b.jpg".into(),
                size: 434_234,
                last_modified: at(1_255_369_830),
                etag: Some("\"fba9dede5f27731c9771645a39863328\"".into()),
                storage_class: StorageClass::Standard,
            },
            ObjectEntry {
                key: "photos/old.tif".into(),
                size: 0,
                last_modified: at(1_289_422_113),
                etag: None,
                storage_class: StorageClass::DeepArchive,
            },
        ]
    );
    assert!(page.objects[1].storage_class.is_archived());
    assert!(page.is_truncated);
    assert_eq!(
        page.next_continuation_token.as_deref(),
        Some("1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=")
    );
}

#[test]
fn a_url_encoded_listing_decodes_keys_and_prefixes_with_plus_as_space() {
    // AWS's `encoding-type=url` writes a space as `+` and a literal plus as `%2B`.
    let body = format!(
        r#"<ListBucketResult {NS}><EncodingType>url</EncodingType><IsTruncated>false</IsTruncated>
  <Contents><Key>a+b%2Bc/%E2%9C%93%01.txt</Key><Size>1</Size></Contents>
  <CommonPrefixes><Prefix>dir+one/</Prefix></CommonPrefixes>
</ListBucketResult>"#
    );

    let page = parse_list_objects(&body).unwrap();

    assert_eq!(page.objects[0].key, "a b+c/\u{2713}\u{1}.txt");
    assert_eq!(page.prefixes, vec!["dir one/".to_string()]);
    assert!(!page.is_truncated);
    assert_eq!(page.next_continuation_token, None);
}

#[test]
fn a_listing_without_encoding_type_keeps_percent_and_plus_literally() {
    let body = format!(
        "<ListBucketResult {NS}><IsTruncated>false</IsTruncated><Contents><Key>100%+ok</Key><Size>1</Size></Contents></ListBucketResult>"
    );
    assert_eq!(parse_list_objects(&body).unwrap().objects[0].key, "100%+ok");
}

#[test]
fn an_unknown_storage_class_is_kept_and_readable() {
    let body = format!(
        "<ListBucketResult {NS}><Contents><Key>k</Key><Size>1</Size><StorageClass>GLACIER_IR</StorageClass></Contents></ListBucketResult>"
    );
    let page = parse_list_objects(&body).unwrap();
    assert_eq!(page.objects[0].storage_class, StorageClass::Other("GLACIER_IR".into()));
    assert!(!page.objects[0].storage_class.is_archived());
}

#[test]
fn an_object_without_a_size_is_malformed() {
    let body = format!("<ListBucketResult {NS}><Contents><Key>k</Key></Contents></ListBucketResult>");
    assert_eq!(parse_list_objects(&body), Err(BodyError::Malformed));
}

#[test]
fn the_wrong_document_is_malformed() {
    let buckets = format!("<ListAllMyBucketsResult {NS}><Buckets/></ListAllMyBucketsResult>");
    assert_eq!(parse_list_objects(&buckets), Err(BodyError::Malformed));
    assert_eq!(parse_list_buckets("not xml"), Err(BodyError::Malformed));
}

#[test]
fn initiate_multipart_reads_the_upload_id() {
    let body = format!(
        "<InitiateMultipartUploadResult {NS}><Bucket>b</Bucket><Key>big.iso</Key>\
         <UploadId>VXBsb2FkIElEIGZvciA2aWWpbmcncyBteS1tb3ZpZS5tMnRzIHVwbG9hZA</UploadId></InitiateMultipartUploadResult>"
    );
    assert_eq!(
        parse_initiate_multipart(&body).unwrap().upload_id,
        "VXBsb2FkIElEIGZvciA2aWWpbmcncyBteS1tb3ZpZS5tMnRzIHVwbG9hZA"
    );
}

#[test]
fn initiate_multipart_without_an_upload_id_is_malformed() {
    let body = format!("<InitiateMultipartUploadResult {NS}><Key>k</Key></InitiateMultipartUploadResult>");
    assert_eq!(parse_initiate_multipart(&body), Err(BodyError::Malformed));
}

#[test]
fn complete_multipart_reads_the_etag() {
    let body = format!(
        "<CompleteMultipartUploadResult {NS}><Location>https://b.s3.amazonaws.com/big.iso</Location>\
         <Bucket>b</Bucket><Key>big.iso</Key><ETag>\"3858f62230ac3c915f300c664312c11f-9\"</ETag></CompleteMultipartUploadResult>"
    );
    assert_eq!(
        parse_complete_multipart(&body).unwrap().etag.as_deref(),
        Some("\"3858f62230ac3c915f300c664312c11f-9\"")
    );
}

#[test]
fn complete_multipart_can_fail_inside_a_200() {
    let body = "<Error><Code>InternalError</Code><Message>We encountered an internal error. Please try again.</Message>\
                <RequestId>656c76696e6727732072657175657374</RequestId><HostId>Uuag1LuByRx9e6j5Onimru9pO4ZVKnJ2Qz7/C1NPcfTWAtRPfTaOFg==</HostId></Error>";
    let error = embedded(parse_complete_multipart(body));
    assert_eq!(error.code, S3ErrorCode::InternalError);
    assert_eq!(error.request_id.as_deref(), Some("656c76696e6727732072657175657374"));
    assert!(error.is_retryable());
}

#[test]
fn copy_object_and_copy_part_results_both_parse() {
    let object = format!(
        "<CopyObjectResult {NS}><LastModified>2009-10-12T17:50:30.000Z</LastModified>\
         <ETag>\"9b2cf535f27731c974343645a3985328\"</ETag></CopyObjectResult>"
    );
    let part = format!(
        "<CopyPartResult {NS}><LastModified>2010-11-10T20:48:33.000Z</LastModified>\
         <ETag>\"b20b8e3e2d7bbd4b5d2a0d2d0a2a0c5f\"</ETag></CopyPartResult>"
    );

    assert_eq!(
        parse_copy_result(&object).unwrap(),
        CopyOutcome {
            etag: Some("\"9b2cf535f27731c974343645a3985328\"".into()),
            last_modified: at(1_255_369_830)
        }
    );
    assert_eq!(parse_copy_result(&part).unwrap().last_modified, at(1_289_422_113));
}

#[test]
fn a_copy_can_fail_inside_a_200() {
    let error = embedded(parse_copy_result(COPY_ERROR_IN_200));
    assert_eq!(error.code, S3ErrorCode::SlowDown);
    assert_eq!(error.status, StatusCode::OK);
    assert!(error.is_retryable());
}

#[test]
fn list_multipart_uploads_reads_keys_ids_and_markers() {
    let body = format!(
        r#"<ListMultipartUploadsResult {NS}>
  <Bucket>b</Bucket><KeyMarker></KeyMarker><UploadIdMarker></UploadIdMarker>
  <NextKeyMarker>my-movie.m2ts</NextKeyMarker><NextUploadIdMarker>YW55IGlkZWEgd2h5</NextUploadIdMarker>
  <MaxUploads>3</MaxUploads><IsTruncated>true</IsTruncated>
  <Upload><Key>my-divisor</Key><UploadId>XMgbGlrZSBlbHZpbmcncyBub3QgaGF2aW5nIG11Y2ggbHVjaw</UploadId>
    <Initiator><ID>arn:aws:iam::111122223333:user/user1</ID></Initiator><StorageClass>STANDARD</StorageClass>
    <Initiated>2010-11-10T20:48:33.000Z</Initiated></Upload>
  <Upload><Key>my-movie.m2ts</Key><UploadId>YW55IGlkZWEgd2h5</UploadId><Initiated>2009-10-12T17:50:30.000Z</Initiated></Upload>
</ListMultipartUploadsResult>"#
    );

    let page = parse_list_multipart_uploads(&body).unwrap();

    assert_eq!(
        page.uploads,
        vec![
            UploadEntry {
                key: "my-divisor".into(),
                upload_id: "XMgbGlrZSBlbHZpbmcncyBub3QgaGF2aW5nIG11Y2ggbHVjaw".into(),
                initiated: at(1_289_422_113),
            },
            UploadEntry {
                key: "my-movie.m2ts".into(),
                upload_id: "YW55IGlkZWEgd2h5".into(),
                initiated: at(1_255_369_830)
            },
        ]
    );
    assert!(page.is_truncated);
    assert_eq!(page.next_key_marker.as_deref(), Some("my-movie.m2ts"));
    assert_eq!(page.next_upload_id_marker.as_deref(), Some("YW55IGlkZWEgd2h5"));
}

#[test]
fn a_url_encoded_upload_listing_decodes_keys() {
    let body = format!(
        "<ListMultipartUploadsResult {NS}><EncodingType>url</EncodingType><IsTruncated>false</IsTruncated>\
         <Upload><Key>a+b%2F</Key><UploadId>u</UploadId></Upload></ListMultipartUploadsResult>"
    );
    assert_eq!(parse_list_multipart_uploads(&body).unwrap().uploads[0].key, "a b/");
}

#[test]
fn delete_result_separates_deleted_keys_from_per_key_failures() {
    let body = format!(
        r#"<DeleteResult {NS}>
  <Deleted><Key>sample1.txt</Key></Deleted>
  <Deleted><Key>gone.txt</Key><DeleteMarker>true</DeleteMarker><DeleteMarkerVersionId>NeQt5xeFTfgPJD8B4CGWnkSLtluMr11s</DeleteMarkerVersionId></Deleted>
  <Error><Key>sample2.txt</Key><Code>AccessDenied</Code><Message>Access Denied</Message></Error>
  <Error><Key>odd.txt</Key><Code>SomethingNew</Code></Error>
</DeleteResult>"#
    );

    let outcome = parse_delete_result(&body).unwrap();

    assert_eq!(outcome.deleted, vec!["sample1.txt".to_string(), "gone.txt".to_string()]);
    assert_eq!(
        outcome.failed,
        vec![
            DeleteFailure {
                key: "sample2.txt".into(),
                code: S3ErrorCode::AccessDenied,
                message: Some("Access Denied".into())
            },
            DeleteFailure {
                key: "odd.txt".into(),
                code: S3ErrorCode::Other("SomethingNew".into()),
                message: None
            },
        ]
    );
}

#[test]
fn a_quiet_delete_with_no_failures_is_empty() {
    let outcome = parse_delete_result(&format!("<DeleteResult {NS}/>")).unwrap();
    assert!(outcome.deleted.is_empty() && outcome.failed.is_empty());
}

#[test]
fn a_delete_can_fail_as_a_whole_inside_a_200() {
    assert_eq!(
        embedded(parse_delete_result(COPY_ERROR_IN_200)).code,
        S3ErrorCode::SlowDown
    );
}

#[test]
fn an_error_body_reads_code_message_request_id_and_the_routing_hints() {
    let error = parse_error_body(
        "<Error><Code>AuthorizationHeaderMalformed</Code><Message>The authorization header is malformed; \
         the region 'us-east-1' is wrong; expecting 'eu-north-1'</Message><Region>eu-north-1</Region>\
         <RequestId>ABC</RequestId></Error>",
    )
    .unwrap();
    assert_eq!(error.code, S3ErrorCode::AuthorizationHeaderMalformed);
    assert_eq!(error.region.as_deref(), Some("eu-north-1"));
    assert_eq!(error.request_id.as_deref(), Some("ABC"));
    assert!(error.message.is_some());

    let redirect = parse_error_body(
        "<Error><Code>PermanentRedirect</Code><Endpoint>photos.s3.eu-north-1.amazonaws.com</Endpoint></Error>",
    )
    .unwrap();
    assert_eq!(redirect.endpoint.as_deref(), Some("photos.s3.eu-north-1.amazonaws.com"));
}

#[test]
fn a_body_that_isnt_an_error_document_yields_none() {
    assert_eq!(parse_error_body(""), None);
    assert_eq!(parse_error_body("<html><body>502 Bad Gateway</body></html>"), None);
    assert_eq!(parse_error_body("<Error><Message>no code</Message></Error>"), None);
}

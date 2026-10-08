//! Which date an object shows, and which objects show as archived.

use super::super::listing::children_of;
use super::super::test_support::make_test_volume;
use super::{cold_from_head, modified_from_head, stored_mtime};
use crate::xml::parse_list_objects;

/// One `ListObjectsV2` page with an object in every storage class that
/// matters: two that need a restore, and two that read at once.
const PAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>b</Name><Prefix>old/</Prefix><KeyCount>4</KeyCount><IsTruncated>false</IsTruncated>
  <Contents><Key>old/flexible.tar</Key><Size>10</Size><StorageClass>GLACIER</StorageClass></Contents>
  <Contents><Key>old/deep.tar</Key><Size>10</Size><StorageClass>DEEP_ARCHIVE</StorageClass></Contents>
  <Contents><Key>old/instant.tar</Key><Size>10</Size><StorageClass>GLACIER_IR</StorageClass></Contents>
  <Contents><Key>old/plain.tar</Key><Size>10</Size></Contents>
</ListBucketResult>"#;

#[test]
fn a_listing_marks_the_objects_that_need_a_restore_and_only_those() {
    let volume = make_test_volume(Some("b"));
    let page = parse_list_objects(PAGE).expect("a well-formed page");
    let cold: Vec<(String, bool)> = children_of(&page, "old/")
        .into_iter()
        .filter_map(|child| volume.child_entry("/b/old", child))
        .map(|entry| (entry.name, entry.in_cold_storage))
        .collect();
    assert_eq!(
        cold,
        [
            ("flexible.tar".to_string(), true),
            ("deep.tar".to_string(), true),
            // Glacier Instant Retrieval reads on demand: no badge.
            ("instant.tar".to_string(), false),
            ("plain.tar".to_string(), false),
        ]
    );
}

#[test]
fn a_head_names_cold_storage_by_class_or_by_archive_tier() {
    // `x-amz-storage-class` is absent for STANDARD.
    assert!(!cold_from_head(None, None));
    assert!(cold_from_head(Some("GLACIER"), None));
    assert!(cold_from_head(Some("DEEP_ARCHIVE"), None));
    assert!(!cold_from_head(Some("GLACIER_IR"), None));
    assert!(!cold_from_head(Some("STANDARD_IA"), None));
    // Intelligent-Tiering's archive tiers show only on a HEAD.
    assert!(cold_from_head(Some("INTELLIGENT_TIERING"), Some("ARCHIVE_ACCESS")));
    assert!(cold_from_head(Some("INTELLIGENT_TIERING"), Some("DEEP_ARCHIVE_ACCESS")));
    assert!(!cold_from_head(Some("INTELLIGENT_TIERING"), None));
}

const LAST_MODIFIED: &str = "Wed, 01 Oct 2025 10:00:00 GMT";
const LAST_MODIFIED_SECS: u64 = 1_759_312_800;

#[test]
fn the_sources_mtime_wins_over_the_upload_time() {
    // rclone's key and format, so a file Cmdr or rclone uploaded shows the date
    // it had on the disk it came from.
    assert_eq!(
        modified_from_head(Some("1354040105.123456789"), Some(LAST_MODIFIED)),
        Some(1_354_040_105)
    );
}

#[test]
fn without_an_mtime_the_upload_time_shows() {
    assert_eq!(modified_from_head(None, Some(LAST_MODIFIED)), Some(LAST_MODIFIED_SECS));
}

#[test]
fn an_mtime_that_doesnt_parse_falls_back_to_the_upload_time() {
    // Another tool's idea of the key, or garbage: never a made-up date.
    assert_eq!(
        modified_from_head(Some("yesterday"), Some(LAST_MODIFIED)),
        Some(LAST_MODIFIED_SECS)
    );
}

#[test]
fn no_date_at_all_is_no_date() {
    assert_eq!(modified_from_head(None, None), None);
}

#[test]
fn an_object_answer_carries_its_stored_mtime_to_the_nanosecond() {
    // What a read stream hands a copy's destination: the source's own date,
    // sub-second part included, so S3 → S3 keeps it exactly.
    let exact = std::time::UNIX_EPOCH + std::time::Duration::new(1_354_040_105, 123_456_789);
    assert_eq!(
        stored_mtime(Some("1354040105.123456789"), Some("Wed, 01 Oct 2026 10:00:00 GMT")),
        Some(exact)
    );
    // No mtime metadata: the upload time is the only date the object has.
    assert_eq!(
        stored_mtime(None, Some("Wed, 28 Nov 2012 18:15:05 GMT")),
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_354_126_505))
    );
    assert_eq!(stored_mtime(Some("not a time"), None), None);
}

//! The response bodies, one parser each.
//!
//! ❗ Every parser checks the root element first: `CompleteMultipartUpload`,
//! `CopyObject`, and `UploadPartCopy` can answer `200 OK` with an `<Error>`
//! body, so a success status alone proves nothing.

use std::time::SystemTime;

use http::StatusCode;
use percent_encoding::percent_decode_str;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::{Element, Malformed, parse_tree};
use crate::error::{S3Error, S3ErrorCode};

/// Why a 2xx body didn't yield its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BodyError {
    /// Not the document we expected.
    Malformed,
    /// An `<Error>` inside a success status.
    Embedded(Box<S3Error>),
}

impl From<Malformed> for BodyError {
    fn from(_: Malformed) -> Self {
        Self::Malformed
    }
}

/// One bucket from `ListBuckets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BucketEntry {
    pub name: String,
    pub created: Option<SystemTime>,
    /// `BucketRegion`, which AWS added in 2024; absent elsewhere.
    pub region: Option<String>,
}

/// One page of `ListBuckets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BucketPage {
    pub buckets: Vec<BucketEntry>,
    /// Present when more buckets follow (AWS paginates past 10,000).
    pub continuation_token: Option<String>,
}

/// Where an object's bytes live, from a listing's `StorageClass`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StorageClass {
    /// `STANDARD`, or no `StorageClass` element at all.
    Standard,
    /// `GLACIER` (Flexible Retrieval): reads fail until restored.
    Glacier,
    /// `DEEP_ARCHIVE`: reads fail until restored.
    DeepArchive,
    /// Every other class (`STANDARD_IA`, `GLACIER_IR`, `INTELLIGENT_TIERING`,
    /// …), all readable immediately.
    Other(String),
}

impl StorageClass {
    /// The class a listing's `StorageClass` or a HEAD's `x-amz-storage-class`
    /// names; absent is `STANDARD`.
    pub(crate) fn from_text(text: Option<&str>) -> Self {
        match text {
            None | Some("STANDARD") => Self::Standard,
            Some("GLACIER") => Self::Glacier,
            Some("DEEP_ARCHIVE") => Self::DeepArchive,
            Some(other) => Self::Other(other.to_string()),
        }
    }

    /// A read answers `InvalidObjectState` until the object is restored.
    /// Intelligent-Tiering's archive tiers don't show in a listing (only HEAD's
    /// `x-amz-archive-status` says), so they read as not archived here.
    pub(crate) fn is_archived(&self) -> bool {
        matches!(self, Self::Glacier | Self::DeepArchive)
    }
}

/// One object from `ListObjectsV2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectEntry {
    pub key: String,
    pub size: u64,
    pub last_modified: Option<SystemTime>,
    /// Optional: some servers omit it.
    pub etag: Option<String>,
    pub storage_class: StorageClass,
}

/// One page of `ListObjectsV2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectPage {
    /// `CommonPrefixes`, each ending in the delimiter: the "folders".
    pub prefixes: Vec<String>,
    pub objects: Vec<ObjectEntry>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}

/// `InitiateMultipartUploadResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InitiatedUpload {
    pub upload_id: String,
}

/// `CompleteMultipartUploadResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedUpload {
    pub etag: Option<String>,
}

/// `CopyObjectResult` or `CopyPartResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CopyOutcome {
    pub etag: Option<String>,
    pub last_modified: Option<SystemTime>,
}

/// One unfinished upload from `ListMultipartUploads`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UploadEntry {
    pub key: String,
    pub upload_id: String,
    pub initiated: Option<SystemTime>,
}

/// One page of `ListMultipartUploads`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UploadPage {
    pub uploads: Vec<UploadEntry>,
    pub is_truncated: bool,
    pub next_key_marker: Option<String>,
    pub next_upload_id_marker: Option<String>,
}

/// One key `DeleteObjects` couldn't delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeleteFailure {
    pub key: String,
    pub code: S3ErrorCode,
    /// For the logs only.
    pub message: Option<String>,
}

/// `DeleteResult`. A key that didn't exist counts as deleted (S3's rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeleteOutcome {
    /// Empty in quiet mode, where only failures are reported.
    pub deleted: Vec<String>,
    pub failed: Vec<DeleteFailure>,
}

pub(crate) fn parse_list_buckets(body: &str) -> Result<BucketPage, BodyError> {
    let root = document(body, "ListAllMyBucketsResult")?;
    let mut buckets = Vec::new();
    if let Some(list) = root.child("Buckets") {
        for bucket in list.children("Bucket") {
            buckets.push(BucketEntry {
                name: bucket.value("Name").ok_or(BodyError::Malformed)?.to_string(),
                created: bucket.value("CreationDate").and_then(parse_timestamp),
                region: bucket.value("BucketRegion").map(str::to_string),
            });
        }
    }
    Ok(BucketPage {
        buckets,
        continuation_token: root.value("ContinuationToken").map(str::to_string),
    })
}

pub(crate) fn parse_list_objects(body: &str) -> Result<ObjectPage, BodyError> {
    let root = document(body, "ListBucketResult")?;
    let keys = KeyText::of(&root);
    let mut prefixes = Vec::new();
    for common in root.children("CommonPrefixes") {
        prefixes.push(keys.read(common, "Prefix")?);
    }
    let mut objects = Vec::new();
    for contents in root.children("Contents") {
        objects.push(ObjectEntry {
            key: keys.read(contents, "Key")?,
            size: contents
                .value("Size")
                .and_then(|size| size.parse().ok())
                .ok_or(BodyError::Malformed)?,
            last_modified: contents.value("LastModified").and_then(parse_timestamp),
            etag: contents.value("ETag").map(str::to_string),
            storage_class: StorageClass::from_text(contents.value("StorageClass")),
        });
    }
    Ok(ObjectPage {
        prefixes,
        objects,
        is_truncated: flag(&root, "IsTruncated"),
        next_continuation_token: root.value("NextContinuationToken").map(str::to_string),
    })
}

pub(crate) fn parse_initiate_multipart(body: &str) -> Result<InitiatedUpload, BodyError> {
    let root = document(body, "InitiateMultipartUploadResult")?;
    Ok(InitiatedUpload {
        upload_id: root.value("UploadId").ok_or(BodyError::Malformed)?.to_string(),
    })
}

pub(crate) fn parse_complete_multipart(body: &str) -> Result<CompletedUpload, BodyError> {
    let root = document(body, "CompleteMultipartUploadResult")?;
    Ok(CompletedUpload {
        etag: root.value("ETag").map(str::to_string),
    })
}

/// Both `CopyObjectResult` and `UploadPartCopy`'s `CopyPartResult`.
pub(crate) fn parse_copy_result(body: &str) -> Result<CopyOutcome, BodyError> {
    let root = parse_tree(body)?;
    if root.name == "Error" {
        return Err(BodyError::Embedded(Box::new(
            error_from(&root).ok_or(BodyError::Malformed)?,
        )));
    }
    if root.name != "CopyObjectResult" && root.name != "CopyPartResult" {
        return Err(BodyError::Malformed);
    }
    Ok(CopyOutcome {
        etag: root.value("ETag").map(str::to_string),
        last_modified: root.value("LastModified").and_then(parse_timestamp),
    })
}

pub(crate) fn parse_list_multipart_uploads(body: &str) -> Result<UploadPage, BodyError> {
    let root = document(body, "ListMultipartUploadsResult")?;
    let keys = KeyText::of(&root);
    let mut uploads = Vec::new();
    for upload in root.children("Upload") {
        uploads.push(UploadEntry {
            key: keys.read(upload, "Key")?,
            upload_id: upload.value("UploadId").ok_or(BodyError::Malformed)?.to_string(),
            initiated: upload.value("Initiated").and_then(parse_timestamp),
        });
    }
    let next_key_marker = match root.child("NextKeyMarker") {
        Some(_) => Some(keys.read(&root, "NextKeyMarker")?).filter(|marker| !marker.is_empty()),
        None => None,
    };
    Ok(UploadPage {
        uploads,
        is_truncated: flag(&root, "IsTruncated"),
        next_key_marker,
        next_upload_id_marker: root.value("NextUploadIdMarker").map(str::to_string),
    })
}

pub(crate) fn parse_delete_result(body: &str) -> Result<DeleteOutcome, BodyError> {
    let root = document(body, "DeleteResult")?;
    let mut deleted = Vec::new();
    for entry in root.children("Deleted") {
        deleted.push(entry.raw("Key").ok_or(BodyError::Malformed)?.to_string());
    }
    let mut failed = Vec::new();
    for entry in root.children("Error") {
        failed.push(DeleteFailure {
            key: entry.raw("Key").ok_or(BodyError::Malformed)?.to_string(),
            code: S3ErrorCode::from_code(entry.value("Code").ok_or(BodyError::Malformed)?),
            message: entry.value("Message").map(str::to_string),
        });
    }
    Ok(DeleteOutcome { deleted, failed })
}

/// An `<Error>` document, or `None` when `body` isn't one. The status is a
/// placeholder; `S3Error::from_response` sets the real one.
pub(crate) fn parse_error_body(body: &str) -> Option<S3Error> {
    let root = parse_tree(body).ok()?;
    if root.name != "Error" {
        return None;
    }
    error_from(&root)
}

/// The root of `body`, which must be `expected` or an `<Error>`.
fn document(body: &str, expected: &str) -> Result<Element, BodyError> {
    let root = parse_tree(body)?;
    if root.name == "Error" {
        return Err(BodyError::Embedded(Box::new(
            error_from(&root).ok_or(BodyError::Malformed)?,
        )));
    }
    if root.name != expected {
        return Err(BodyError::Malformed);
    }
    Ok(root)
}

/// The `S3Error` an `<Error>` element describes; `None` without a `<Code>`.
fn error_from(element: &Element) -> Option<S3Error> {
    let code = element.value("Code")?;
    Some(S3Error {
        status: StatusCode::OK,
        code: S3ErrorCode::from_code(code),
        message: element.value("Message").map(str::to_string),
        request_id: element.value("RequestId").map(str::to_string),
        region: element.value("Region").map(str::to_string),
        endpoint: element.value("Endpoint").map(str::to_string),
    })
}

/// How a listing spells its keys: as themselves, or URL-encoded when the
/// response says `<EncodingType>url</EncodingType>` (we ask for that, because
/// XML 1.0 can't carry some characters a key may hold).
#[derive(Clone, Copy)]
enum KeyText {
    Literal,
    UrlEncoded,
}

impl KeyText {
    fn of(root: &Element) -> Self {
        if root.value("EncodingType") == Some("url") {
            Self::UrlEncoded
        } else {
            Self::Literal
        }
    }

    /// A key-shaped child, untrimmed, decoded if the listing encoded it.
    fn read(self, element: &Element, name: &str) -> Result<String, BodyError> {
        let raw = element.raw(name).ok_or(BodyError::Malformed)?;
        match self {
            Self::Literal => Ok(raw.to_string()),
            // AWS writes a space as `+` and a literal plus as `%2B` (what
            // botocore's `unquote_plus` undoes), so `+` goes first.
            Self::UrlEncoded => percent_decode_str(&raw.replace('+', " "))
                .decode_utf8()
                .map(|key| key.into_owned())
                .map_err(|_| BodyError::Malformed),
        }
    }
}

/// `true` only for a literal `true`; absent means `false`.
fn flag(element: &Element, name: &str) -> bool {
    element.value(name) == Some("true")
}

/// An ISO 8601 timestamp as S3 writes them (`2009-10-12T17:50:30.000Z`).
pub(crate) fn parse_timestamp(text: &str) -> Option<SystemTime> {
    OffsetDateTime::parse(text, &Rfc3339).ok().map(SystemTime::from)
}

#[cfg(test)]
#[path = "parse_test.rs"]
mod parse_test;

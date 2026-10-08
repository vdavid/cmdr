//! One builder per S3 operation Cmdr uses, each producing an unsigned
//! [`S3Request`] addressed through the provider profile.
//!
//! The three writes that can refuse to overwrite (`PutObject`,
//! `CompleteMultipartUpload`, `CopyObject`) return a [`Built`] whose
//! `check_first` says the profile has no header for it right now: the caller
//! must HEAD the destination before sending. Destructuring `Built` is what
//! keeps that from being forgotten.

use std::time::{Duration, SystemTime};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::{HeaderName, HeaderValue, Method};
use md5::{Digest as _, Md5};
use url::Url;

use crate::encoding::{encode_component, encode_key};
use crate::metadata::{MTIME_HEADER, WRITE_TOKEN_HEADER, format_mtime};
use crate::profile::{ConditionalOp, LocateError, NoOverwrite, ProviderProfile};
use crate::request::{Body, Dialect, S3Request};
use crate::sigv4::{AmzTime, Credentials, Scope, presign};
use crate::xml::build::{CompletedPart, complete_multipart_upload_body, delete_objects_body};

/// `DeleteObjects` takes at most this many keys per call.
pub(crate) const MAX_DELETE_KEYS: usize = 1_000;

/// Why a request couldn't be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuildError {
    Locate(LocateError),
    /// The profile copies within one bucket only (Spaces); stream instead.
    CrossBucketCopy,
    /// More than [`MAX_DELETE_KEYS`] keys, or none.
    DeleteBatchSize,
}

impl From<LocateError> for BuildError {
    fn from(error: LocateError) -> Self {
        Self::Locate(error)
    }
}

/// Whether a write may replace an existing object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Overwrite {
    Replace,
    /// Refuse if the destination exists, by header when the profile has one.
    Refuse,
}

/// A conditional write, and whether the caller must check for the
/// destination itself before sending it.
#[derive(Debug)]
pub(crate) struct Built {
    pub request: S3Request,
    /// `true` when `Overwrite::Refuse` was asked and the profile has no header
    /// for this operation: HEAD first, and report a clash noticed afterwards.
    pub check_first: bool,
}

/// The user metadata Cmdr writes on an object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ObjectMetadata {
    /// The source file's modification time (`x-amz-meta-mtime`).
    pub mtime: Option<SystemTime>,
    /// The identity of the PUT writing it (`x-amz-meta-cmdr-write`), so a
    /// cut-off write can tell its own leftover from anyone else's object.
    pub write_token: Option<String>,
    /// Headers a server-side copy restates from its source when it can't
    /// keep them by `COPY` (a multipart copy, or a copy that adds the mtime):
    /// `content-type`, `cache-control`, and the like, plus every other
    /// `x-amz-meta-*`. Lowercase names, values as the source's HEAD served
    /// them.
    pub carried: Vec<(String, String)>,
}

/// What a copy does with the source's metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MetadataDirective {
    /// Keep the source's (S3's default): a rename keeps its mtime.
    Copy,
    Replace(ObjectMetadata),
}

/// The object a copy reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CopySource<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
}

/// A byte range: `start` to `end` inclusive, or to the end of the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteRange {
    pub start: u64,
    pub end: Option<u64>,
}

/// `ListObjectsV2`'s knobs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ListObjectsParams<'a> {
    pub prefix: &'a str,
    /// `/` for a folder view, `None` for a recursive one.
    pub delimiter: Option<&'a str>,
    pub continuation_token: Option<&'a str>,
    pub max_keys: Option<u32>,
}

pub(crate) fn list_buckets(profile: &ProviderProfile, continuation_token: Option<&str>) -> S3Request {
    let location = profile
        .locate(None, None)
        .expect("the account root always has a location");
    let request = S3Request::new(Method::GET, &profile.scheme, &location.host, location.path);
    match continuation_token {
        Some(token) => request.query("continuation-token", token),
        None => request,
    }
}

/// `HeadBucket`: does the bucket exist, and may this key reach it.
pub(crate) fn head_bucket(profile: &ProviderProfile, bucket: &str) -> Result<S3Request, BuildError> {
    at(profile, Method::HEAD, bucket, None)
}

/// `ListObjectsV2`, asking for URL-encoded keys (XML 1.0 can't carry every
/// character a key may hold; `xml::parse` decodes them).
pub(crate) fn list_objects(
    profile: &ProviderProfile,
    bucket: &str,
    params: &ListObjectsParams<'_>,
) -> Result<S3Request, BuildError> {
    let mut request = at(profile, Method::GET, bucket, None)?
        .query("list-type", "2")
        .query("encoding-type", "url");
    if !params.prefix.is_empty() {
        request = request.query("prefix", &profile.normalize_key(params.prefix));
    }
    if let Some(delimiter) = params.delimiter {
        request = request.query("delimiter", delimiter);
    }
    if let Some(token) = params.continuation_token {
        request = request.query("continuation-token", token);
    }
    if let Some(max) = params.max_keys {
        request = request.query("max-keys", &max.to_string());
    }
    Ok(request)
}

pub(crate) fn head_object(profile: &ProviderProfile, bucket: &str, key: &str) -> Result<S3Request, BuildError> {
    at(profile, Method::HEAD, bucket, Some(key))
}

pub(crate) fn get_object(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    range: Option<ByteRange>,
) -> Result<S3Request, BuildError> {
    let request = at(profile, Method::GET, bucket, Some(key))?;
    Ok(match range {
        Some(range) => request.header(name("range"), value(&format_range(range))),
        None => request,
    })
}

/// A single-request upload of `length` streamed bytes.
pub(crate) fn put_object(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    length: u64,
    overwrite: Overwrite,
    metadata: &ObjectMetadata,
) -> Result<Built, BuildError> {
    let mut request = with_metadata(at(profile, Method::PUT, bucket, Some(key))?, metadata);
    request.body = Body::Streamed { length };
    Ok(guarded(profile, ConditionalOp::Put, request, overwrite))
}

/// Starts a multipart upload. The metadata goes here, not on the parts.
pub(crate) fn create_multipart_upload(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    metadata: &ObjectMetadata,
) -> Result<S3Request, BuildError> {
    let request = at(profile, Method::POST, bucket, Some(key))?.query("uploads", "");
    Ok(with_metadata(request, metadata))
}

/// One part of `length` streamed bytes.
pub(crate) fn upload_part(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    upload_id: &str,
    part_number: u32,
    length: u64,
) -> Result<S3Request, BuildError> {
    let mut request = part(profile, bucket, key, upload_id, part_number)?;
    request.body = Body::Streamed { length };
    Ok(request)
}

/// One part copied server-side from `source`'s inclusive byte `range`.
/// `source_etag`, when known, pins every part to the one version of the
/// source the copy started from (`x-amz-copy-source-if-match`): an object
/// replaced mid-copy fails the part rather than stitching two versions.
#[allow(
    clippy::too_many_arguments,
    reason = "one part's whole address: the upload, the part number, the source, its range, and the version pin"
)]
pub(crate) fn upload_part_copy(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    upload_id: &str,
    part_number: u32,
    source: CopySource<'_>,
    (first, last): (u64, u64),
    source_etag: Option<&str>,
) -> Result<S3Request, BuildError> {
    let request = part(profile, bucket, key, upload_id, part_number)?
        .header(name("x-amz-copy-source"), copy_source(profile, source, bucket)?)
        .header(name("x-amz-copy-source-range"), value(&format!("bytes={first}-{last}")));
    Ok(match source_etag.and_then(|etag| HeaderValue::from_str(etag).ok()) {
        Some(etag) => request.header(name("x-amz-copy-source-if-match"), etag),
        None => request,
    })
}

pub(crate) fn complete_multipart_upload(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    upload_id: &str,
    parts: &[CompletedPart],
    overwrite: Overwrite,
) -> Result<Built, BuildError> {
    let mut request = at(profile, Method::POST, bucket, Some(key))?
        .query("uploadId", upload_id)
        .header(name("content-type"), HeaderValue::from_static("application/xml"));
    request.body = Body::Bytes(complete_multipart_upload_body(parts).into_bytes());
    Ok(guarded(profile, ConditionalOp::CompleteMultipart, request, overwrite))
}

pub(crate) fn abort_multipart_upload(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    upload_id: &str,
) -> Result<S3Request, BuildError> {
    Ok(at(profile, Method::DELETE, bucket, Some(key))?.query("uploadId", upload_id))
}

/// `ListMultipartUploads` under `prefix`, resuming after the markers a
/// previous page returned.
pub(crate) fn list_multipart_uploads(
    profile: &ProviderProfile,
    bucket: &str,
    prefix: &str,
    markers: Option<(&str, &str)>,
) -> Result<S3Request, BuildError> {
    let mut request = at(profile, Method::GET, bucket, None)?
        .query("uploads", "")
        .query("encoding-type", "url");
    if !prefix.is_empty() {
        request = request.query("prefix", &profile.normalize_key(prefix));
    }
    if let Some((key_marker, upload_id_marker)) = markers {
        request = request
            .query("key-marker", key_marker)
            .query("upload-id-marker", upload_id_marker);
    }
    Ok(request)
}

/// A server-side copy in one request (up to 5 GB; past that, multipart with
/// `upload_part_copy`). `source_etag`, when known, pins the copy to that
/// version of the source (`x-amz-copy-source-if-match`), as `upload_part_copy`
/// does, except a multipart ETag where the provider refuses that pin
/// (`ProviderProfile::refuses_multipart_copy_pin`, GCS). ❗ May fail inside a `200`: parse the body.
pub(crate) fn copy_object(
    profile: &ProviderProfile,
    source: CopySource<'_>,
    source_etag: Option<&str>,
    bucket: &str,
    key: &str,
    overwrite: Overwrite,
    directive: &MetadataDirective,
) -> Result<Built, BuildError> {
    let mut request = at(profile, Method::PUT, bucket, Some(key))?
        .header(name("x-amz-copy-source"), copy_source(profile, source, bucket)?);
    let pinnable = |etag: &&str| !(profile.refuses_multipart_copy_pin && etag.contains('-'));
    if let Some(etag) = source_etag
        .filter(pinnable)
        .and_then(|etag| HeaderValue::from_str(etag).ok())
    {
        request = request.header(name("x-amz-copy-source-if-match"), etag);
    }
    if let MetadataDirective::Replace(metadata) = directive {
        request = with_metadata(
            request.header(name("x-amz-metadata-directive"), HeaderValue::from_static("REPLACE")),
            metadata,
        );
    }
    Ok(guarded(profile, ConditionalOp::Copy, request, overwrite))
}

pub(crate) fn delete_object(profile: &ProviderProfile, bucket: &str, key: &str) -> Result<S3Request, BuildError> {
    at(profile, Method::DELETE, bucket, Some(key))
}

/// `DeleteObjects` for 1–1,000 keys, quiet (only failures come back). Carries
/// `Content-MD5`, which AWS still requires here.
pub(crate) fn delete_objects(profile: &ProviderProfile, bucket: &str, keys: &[&str]) -> Result<S3Request, BuildError> {
    if keys.is_empty() || keys.len() > MAX_DELETE_KEYS {
        return Err(BuildError::DeleteBatchSize);
    }
    let normalized: Vec<_> = keys.iter().map(|key| profile.normalize_key(key)).collect();
    let refs: Vec<&str> = normalized.iter().map(AsRef::as_ref).collect();
    let body = delete_objects_body(&refs, true).into_bytes();
    let md5 = STANDARD.encode(Md5::digest(&body));
    let mut request = at(profile, Method::POST, bucket, None)?
        .query("delete", "")
        .header(name("content-md5"), value(&md5))
        .header(name("content-type"), HeaderValue::from_static("application/xml"));
    request.body = Body::Bytes(body);
    Ok(request)
}

/// "Copy share link": a presigned GET for `key`, valid for `expires` (at most
/// seven days). A signature computed offline, so it's free and instant.
pub(crate) fn share_link(
    profile: &ProviderProfile,
    credentials: &Credentials,
    target: LinkTarget<'_>,
    now: SystemTime,
    expires: Duration,
) -> Result<Url, ShareLinkError> {
    let request = get_object(profile, target.bucket, target.key, None).map_err(ShareLinkError::Build)?;
    let rerouted = target
        .region
        .filter(|region| *region != profile.region)
        .and_then(|region| Some((profile.reroute(request.clone(), region)?, region)));
    let (request, region) = rerouted.unwrap_or((request, profile.region.as_str()));
    let time = AmzTime::new(now);
    let scope = Scope {
        credentials,
        region,
        time: &time,
    };
    presign(&request, &scope, expires).map_err(|_| ShareLinkError::ExpiryOutOfRange)
}

/// The object a share link reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LinkTarget<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
    /// The bucket's own region when it isn't the profile's (a routed account
    /// root, `routing.rs`); `None` signs for the profile's.
    pub region: Option<&'a str>,
}

/// Why a share link wasn't minted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShareLinkError {
    Build(BuildError),
    /// Zero, or longer than seven days.
    ExpiryOutOfRange,
}

/// A request to `bucket` (and `key`) with no body yet.
fn at(profile: &ProviderProfile, method: Method, bucket: &str, key: Option<&str>) -> Result<S3Request, BuildError> {
    let location = profile.locate(Some(bucket), key)?;
    let mut request = S3Request::new(method, &profile.scheme, &location.host, location.path);
    request.bucket = Some(bucket.to_string());
    Ok(request)
}

fn part(
    profile: &ProviderProfile,
    bucket: &str,
    key: &str,
    upload_id: &str,
    part_number: u32,
) -> Result<S3Request, BuildError> {
    Ok(at(profile, Method::PUT, bucket, Some(key))?
        .query("partNumber", &part_number.to_string())
        .query("uploadId", upload_id))
}

/// Adds the no-overwrite header the profile has for `op`, or flags the
/// request for check-then-write.
fn guarded(profile: &ProviderProfile, op: ConditionalOp, request: S3Request, overwrite: Overwrite) -> Built {
    if overwrite == Overwrite::Replace {
        return Built {
            request,
            check_first: false,
        };
    }
    let star = HeaderValue::from_static("*");
    match profile.no_overwrite(op) {
        NoOverwrite::IfNoneMatch => Built {
            request: request.header(name("if-none-match"), star),
            check_first: false,
        },
        NoOverwrite::CloudflareCopyHeader => Built {
            request: request.header(name("cf-copy-destination-if-none-match"), star),
            check_first: false,
        },
        NoOverwrite::GoogGenerationMatch => {
            let mut request = request.header(name("x-goog-if-generation-match"), HeaderValue::from_static("0"));
            request.dialect = Dialect::Goog;
            Built {
                request,
                check_first: false,
            }
        }
        NoOverwrite::CheckThenWrite => Built {
            request,
            check_first: true,
        },
    }
}

fn with_metadata(request: S3Request, metadata: &ObjectMetadata) -> S3Request {
    let request = match metadata.mtime {
        Some(mtime) => request.header(name(MTIME_HEADER), value(&format_mtime(mtime))),
        None => request,
    };
    let mut request = match &metadata.write_token {
        Some(token) => request.header(name(WRITE_TOKEN_HEADER), value(token)),
        None => request,
    };
    for (carried, text) in &metadata.carried {
        // Both came off a HEAD's headers, so both parse; one that somehow
        // doesn't is left out rather than failing the copy.
        if let (Ok(carried), Ok(text)) = (HeaderName::from_bytes(carried.as_bytes()), HeaderValue::from_str(text)) {
            request = request.header(carried, text);
        }
    }
    request
}

/// `x-amz-copy-source`: `/bucket/key`, the key percent-encoded per segment.
fn copy_source(
    profile: &ProviderProfile,
    source: CopySource<'_>,
    destination_bucket: &str,
) -> Result<HeaderValue, BuildError> {
    if !profile.cross_bucket_copy() && source.bucket != destination_bucket {
        return Err(BuildError::CrossBucketCopy);
    }
    if source.bucket.is_empty() || source.bucket.contains('/') {
        return Err(LocateError::InvalidBucket.into());
    }
    let key = encode_key(&profile.normalize_key(source.key)).map_err(LocateError::Key)?;
    Ok(value(&format!("/{}/{key}", encode_component(source.bucket))))
}

fn format_range(range: ByteRange) -> String {
    match range.end {
        Some(end) => format!("bytes={}-{end}", range.start),
        None => format!("bytes={}-", range.start),
    }
}

fn name(text: &'static str) -> HeaderName {
    HeaderName::from_static(text)
}

/// A header value this module builds: digits, base64, an rclone mtime, or
/// a percent-encoded path, all printable ASCII.
fn value(text: &str) -> HeaderValue {
    HeaderValue::from_str(text).expect("an S3 header value built here is printable ASCII")
}

#[cfg(test)]
#[path = "ops_test.rs"]
mod ops_test;

//! Which region each bucket lives in, so an account root on a provider that
//! routes by region (AWS, Wasabi: `ProviderProfile::routes_by_region`)
//! reaches every bucket, not only the ones in the region its connection names.
//!
//! A request signed for, and sent to, the wrong region comes back as a
//! `301 PermanentRedirect` (a `307` while a new bucket's DNS settles, a `400
//! AuthorizationHeaderMalformed` when only the signature's region is off).
//! The bucket's real region rides on `x-amz-bucket-region`, which `HeadBucket`
//! answers on every status, or on the error body's `<Region>`. `ListBuckets`
//! names every bucket's region upfront (`BucketRegion`). The transport learns
//! from all three and sends each bucket's requests to its own endpoint.
//!
//! Pure values: the transport owns the requests (`transport/`).

use std::collections::HashMap;
use std::sync::Mutex;

use http::StatusCode;

use crate::error::{S3Error, S3ErrorCode};

/// The header AWS names a bucket's region in.
pub(crate) const BUCKET_REGION_HEADER: &str = "x-amz-bucket-region";

/// What one answer says about where its bucket lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegionHint {
    /// The request went to the wrong region: send it again, to `region`.
    pub misrouted: bool,
    /// The bucket's region, when the answer named one a hostname can carry.
    pub region: Option<String>,
}

/// Reads an answer for a region hint: the header first, then (for an error
/// body) `<Region>`.
pub(crate) fn read_hint(status: StatusCode, header: Option<&str>, body: &str) -> RegionHint {
    let error = (!status.is_success() && !body.is_empty()).then(|| S3Error::from_response(status, body));
    let misrouted = status.is_redirection()
        || (status == StatusCode::BAD_REQUEST
            && error
                .as_ref()
                .is_some_and(|e| e.code == S3ErrorCode::AuthorizationHeaderMalformed));
    let region = header
        .or_else(|| error.as_ref().and_then(|e| e.region.as_deref()))
        .filter(|region| is_region(region))
        .map(str::to_string);
    RegionHint { misrouted, region }
}

/// A region that can go into a hostname: lowercase letters, digits, and `-`
/// (the profile's own rule for every host part).
fn is_region(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The regions learned so far, per bucket. Lives as long as its client, so a
/// reconnect learns afresh.
#[derive(Debug, Default)]
pub(crate) struct BucketRegions {
    known: Mutex<HashMap<String, String>>,
}

impl BucketRegions {
    /// `bucket`'s region, when known.
    pub(crate) fn region_of(&self, bucket: &str) -> Option<String> {
        self.lock().get(bucket).cloned()
    }

    /// Records `bucket`'s region, and says whether that changed what was known.
    /// A region a hostname can't carry is ignored.
    pub(crate) fn learn(&self, bucket: &str, region: &str) -> bool {
        if !is_region(region) {
            return false;
        }
        let mut known = self.lock();
        if known.get(bucket).is_some_and(|old| old == region) {
            return false;
        }
        known.insert(bucket.to_string(), region.to_string());
        true
    }

    /// The map, whatever a panicking holder left: each entry is written
    /// whole, so a poisoned lock still holds only real regions.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.known.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "routing_test.rs"]
mod routing_test;

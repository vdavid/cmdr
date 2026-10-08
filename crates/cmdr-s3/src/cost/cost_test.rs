//! The estimator against hand-computed amounts, each citing the bundled table's
//! row it uses (`s3-prices.json`), and the table's parse refusals.

use super::prices::{ProviderProblem, RequestKind, TableProblem};
use super::*;
use crate::S3Provider;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const DAY: u64 = 86_400;
/// A fixed "now" so ages are exact.
const NOW: u64 = 1_790_000_000;

fn aws() -> S3Provider {
    S3Provider::Aws {
        region: "us-east-1".into(),
    }
}
fn r2() -> S3Provider {
    S3Provider::R2 {
        account_id: "0123456789abcdef0123456789abcdef".into(),
    }
}
fn b2() -> S3Provider {
    S3Provider::B2 {
        region: "us-west-004".into(),
    }
}
fn wasabi() -> S3Provider {
    S3Provider::Wasabi {
        region: "eu-central-1".into(),
    }
}
fn hetzner() -> S3Provider {
    S3Provider::Hetzner {
        location: "fsn1".into(),
    }
}
fn spaces() -> S3Provider {
    S3Provider::DigitalOcean { region: "fra1".into() }
}

fn workload(provider: &S3Provider) -> Workload {
    Workload::for_provider_at(provider, NOW)
}

fn estimate(work: &Workload) -> Estimate {
    Estimate::of(&PriceTable::bundled(), work).expect("a priced provider")
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "expected {expected}, got {actual}");
}

fn requests_in(estimate: &Estimate, class: &str) -> u64 {
    estimate
        .line_items
        .iter()
        .find_map(|item| match item {
            LineItem::Requests { class: name, count, .. } if name == class => Some(*count),
            _ => None,
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The bundled table
// ---------------------------------------------------------------------------

#[test]
fn bundled_table_parses_and_prices_every_preset() {
    let table = PriceTable::bundled();
    for provider in [aws(), r2(), b2(), wasabi(), hetzner(), S3Provider::Gcs, spaces()] {
        assert!(
            Estimate::of(&table, &workload(&provider)).is_some(),
            "{} has no prices",
            provider.kind_name()
        );
    }
}

#[test]
fn other_provider_has_no_estimate() {
    let other = S3Provider::Other {
        endpoint: "http://127.0.0.1:9000".parse().unwrap(),
        region: None,
        path_style: true,
    };
    let mut work = workload(&other);
    work.upload(MIB);
    assert_eq!(Estimate::of(&PriceTable::bundled(), &work), None);
}

// ---------------------------------------------------------------------------
// AWS: "PUT, COPY, POST, LIST" $5.00/M, "GET and all other requests" $0.40/M,
// egress $0.09/GB, deletes free.
// ---------------------------------------------------------------------------

#[test]
fn aws_small_uploads_are_a_put_and_a_verifying_head_each() {
    let mut work = workload(&aws());
    for _ in 0..1_000 {
        work.upload(MIB);
    }
    let estimate = estimate(&work);
    // 1,000 PUTs × $5/M = $0.005; 1,000 HEADs × $0.40/M = $0.0004.
    assert_eq!(requests_in(&estimate, "PUT, COPY, POST, LIST"), 1_000);
    assert_eq!(requests_in(&estimate, "GET and all other requests"), 1_000);
    close(estimate.total, 0.0054);
    assert_eq!(estimate.currency, "USD");
    assert_eq!(estimate.provider_label, "AWS");
}

#[test]
fn aws_big_upload_goes_in_64_mib_parts() {
    let mut work = workload(&aws());
    work.upload(GIB);
    let estimate = estimate(&work);
    // 1 GiB / 64 MiB = 16 parts, plus Create and Complete: 18 class-A requests
    // (18 × $5/M = $0.00009), and one verifying HEAD ($0.0000004).
    assert_eq!(requests_in(&estimate, "PUT, COPY, POST, LIST"), 18);
    assert_eq!(requests_in(&estimate, "GET and all other requests"), 1);
    close(estimate.total, 0.0000904);
}

#[test]
fn aws_download_bills_egress() {
    let mut work = workload(&aws());
    work.download(10 * GIB);
    let estimate = estimate(&work);
    // One GET ($0.0000004) and 10 GB × $0.09 = $0.90.
    close(estimate.total, 0.9000004);
    let egress = estimate.line_items.iter().find_map(|item| match item {
        LineItem::Egress { bytes, amount } => Some((*bytes, *amount)),
        _ => None,
    });
    let (bytes, amount) = egress.expect("an egress line");
    assert_eq!(bytes, 10 * GIB);
    close(amount, 0.9);
}

#[test]
fn aws_deletes_are_free_and_batch_a_thousand_keys() {
    let mut work = workload(&aws());
    for _ in 0..2_500 {
        work.delete_object(MIB, Some(NOW - DAY));
    }
    let estimate = estimate(&work);
    // Three `DeleteObjects` of up to 1,000 keys, in "DELETE and CANCEL" at $0.
    assert_eq!(requests_in(&estimate, "DELETE and CANCEL"), 3);
    close(estimate.total, 0.0);
}

#[test]
fn aws_folder_delete_is_a_listing_and_a_delete() {
    let mut work = workload(&aws());
    for _ in 0..1_000 {
        work.delete_folder();
    }
    let estimate = estimate(&work);
    // 1,000 LISTs × $5/M = $0.005; the marker deletes are free.
    assert_eq!(requests_in(&estimate, "PUT, COPY, POST, LIST"), 1_000);
    assert_eq!(requests_in(&estimate, "DELETE and CANCEL"), 1_000);
    close(estimate.total, 0.005);
}

// ---------------------------------------------------------------------------
// R2: Class A $4.50/M, Class B $0.36/M, egress free.
// ---------------------------------------------------------------------------

#[test]
fn r2_server_copy_heads_the_source_copies_and_verifies() {
    let mut work = workload(&r2());
    for _ in 0..100 {
        work.copy_on_server(MIB);
    }
    let estimate = estimate(&work);
    // 100 `CopyObject` × $4.50/M = $0.00045; 200 HEADs × $0.36/M = $0.000072.
    assert_eq!(requests_in(&estimate, "Class A"), 100);
    assert_eq!(requests_in(&estimate, "Class B"), 200);
    close(estimate.total, 0.000522);
}

#[test]
fn r2_big_server_copy_goes_in_part_copies() {
    let mut work = workload(&r2());
    work.copy_on_server(GIB);
    let estimate = estimate(&work);
    // Create + 16 `UploadPartCopy` + Complete = 18 Class A; HEAD source and
    // HEAD after = 2 Class B. 18 × $4.50/M + 2 × $0.36/M.
    assert_eq!(requests_in(&estimate, "Class A"), 18);
    assert_eq!(requests_in(&estimate, "Class B"), 2);
    close(estimate.total, 0.00008172);
}

#[test]
fn r2_download_has_no_egress_charge() {
    let mut work = workload(&r2());
    work.download(100 * GIB);
    let estimate = estimate(&work);
    // One GET at $0.36/M; egress is $0.
    close(estimate.total, 0.00000036);
}

// ---------------------------------------------------------------------------
// B2: every class free, egress free (within 3x storage), check-then-write.
// ---------------------------------------------------------------------------

#[test]
fn b2_is_free_but_still_counts_the_check_before_a_write() {
    let mut work = workload(&b2());
    work.upload(MIB);
    let estimate = estimate(&work);
    // B2 takes no `If-None-Match`, so a write HEADs first: PUT + HEAD before +
    // HEAD after, all in the one free class.
    assert_eq!(requests_in(&estimate, "Class A, B, and C"), 3);
    close(estimate.total, 0.0);
}

// ---------------------------------------------------------------------------
// Wasabi: free requests and egress; storage $0.00780273/GB-month (= $7.99 /
// 1,024 / 30 per GB-day), 90-day minimum, 4 KB minimum object.
// ---------------------------------------------------------------------------

#[test]
fn wasabi_deleting_a_young_object_bills_its_remaining_days() {
    let mut work = workload(&wasabi());
    work.delete_object(GIB, Some(NOW - 10 * DAY));
    let estimate = estimate(&work);
    // 1 GB × (90 − 10) days × $0.00780273 / 30.
    let expected = 80.0 * 0.00780273 / 30.0;
    close(estimate.total, expected);
    assert!(
        estimate
            .line_items
            .iter()
            .any(|item| matches!(item, LineItem::EarlyDeletion { objects: 1, .. }))
    );
}

#[test]
fn wasabi_bills_a_tiny_young_object_as_4_kb() {
    let mut work = workload(&wasabi());
    work.delete_object(1, Some(NOW));
    let estimate = estimate(&work);
    // 4,096 bytes for 90 days.
    let expected = 4096.0 / GIB as f64 * 90.0 * 0.00780273 / 30.0;
    close(estimate.total, expected);
}

#[test]
fn wasabi_old_or_undated_objects_cost_nothing_to_delete() {
    let mut work = workload(&wasabi());
    work.delete_object(GIB, Some(NOW - 100 * DAY));
    work.delete_object(GIB, Some(NOW - 90 * DAY));
    work.delete_object(GIB, None);
    let estimate = estimate(&work);
    close(estimate.total, 0.0);
    assert!(
        !estimate
            .line_items
            .iter()
            .any(|item| matches!(item, LineItem::EarlyDeletion { .. }))
    );
}

#[test]
fn wasabi_treats_a_date_in_the_future_as_brand_new() {
    let mut work = workload(&wasabi());
    work.delete_object(GIB, Some(NOW + 5 * DAY));
    let estimate = estimate(&work);
    close(estimate.total, 90.0 * 0.00780273 / 30.0);
}

#[test]
fn wasabi_partial_day_rounds_down_to_whole_days_of_age() {
    let mut work = workload(&wasabi());
    // 10 days and 23 hours old: still 10 whole days, 80 left.
    work.delete_object(GIB, Some(NOW - 10 * DAY - 23 * 3_600));
    close(estimate(&work).total, 80.0 * 0.00780273 / 30.0);
}

// ---------------------------------------------------------------------------
// Overwrites: the replaced object, and the temp key off the short-body
// allowlist.
// ---------------------------------------------------------------------------

#[test]
fn wasabi_overwriting_a_young_object_bills_its_remaining_days_with_no_delete_request() {
    let mut work = workload(&wasabi());
    work.replace_object(GIB, Some(NOW - 10 * DAY));
    close(estimate(&work).total, 80.0 * 0.00780273 / 30.0);
    assert!(work.requests.is_empty(), "the write replaces it: {:?}", work.requests);
}

#[test]
fn aws_overwriting_an_object_costs_nothing_extra() {
    let mut work = workload(&aws());
    work.replace_object(GIB, Some(NOW));
    work.upload_over(MIB);
    close(estimate(&work).total, 0.0);
    assert!(
        work.requests.is_empty(),
        "AWS refuses a short body, so the PUT goes straight to the key"
    );
}

#[test]
fn wasabi_upload_over_an_object_goes_straight_to_the_key() {
    let mut work = workload(&wasabi());
    work.upload_over(MIB);
    close(estimate(&work).total, 0.0);
    assert!(
        work.requests.is_empty(),
        "Wasabi refuses a short body (live), so the PUT goes straight to the key: {:?}",
        work.requests
    );
}

/// Off the `refuses_short_body` allowlist ("Other" alone, which no table
/// prices) a one-PUT overwrite goes as a one-part multipart upload: a HEAD
/// finding the original, and Create, one part, and Complete in place of the
/// PUT. Nothing is written beside it.
#[test]
fn an_upload_over_an_object_off_the_short_body_allowlist_goes_as_one_part() {
    let other = S3Provider::Other {
        endpoint: url::Url::parse("http://127.0.0.1:17480").expect("a URL"),
        region: None,
        path_style: true,
    };
    let mut work = workload(&other);
    work.upload(GIB / 32);
    work.upload_over(GIB / 32);
    assert_eq!(work.requests.get(&RequestKind::PutObject), None);
    assert_eq!(work.requests.get(&RequestKind::CreateMultipartUpload), Some(&1));
    assert_eq!(work.requests.get(&RequestKind::UploadPart), Some(&1));
    assert_eq!(work.requests.get(&RequestKind::CompleteMultipartUpload), Some(&1));
    // The upload's verifying HEAD and its no-overwrite check, plus the one
    // finding the original.
    assert_eq!(work.requests.get(&RequestKind::HeadObject), Some(&3));
    assert!(work.dated_deletions.is_empty());
}

#[test]
fn a_multipart_upload_over_an_object_goes_straight_to_the_key() {
    let mut work = workload(&hetzner());
    work.upload_over(GIB);
    assert!(
        work.requests.is_empty(),
        "only its completion publishes: {:?}",
        work.requests
    );
}

// ---------------------------------------------------------------------------
// Hetzner: requests free, storage and egress inside the base price, EUR.
// ---------------------------------------------------------------------------

#[test]
fn hetzner_prices_in_euros_and_charges_nothing_per_operation() {
    let mut work = workload(&hetzner());
    work.upload(GIB);
    work.download(GIB);
    work.delete_object(1, Some(NOW));
    let estimate = estimate(&work);
    assert_eq!(estimate.currency, "EUR");
    assert_eq!(estimate.provider_label, "Hetzner");
    close(estimate.total, 0.0);
}

/// The no-overwrite HEADs follow each write's own allowlist entry: Hetzner
/// enforces `If-None-Match` on a PUT only, R2 on a PUT and a completion.
#[test]
fn the_check_before_a_write_follows_that_writes_allowlist_entry() {
    let heads = |provider: &S3Provider, size: u64| {
        let mut work = workload(provider);
        work.upload(size);
        work.requests.get(&RequestKind::HeadObject).copied()
    };
    // The verifying HEAD only.
    assert_eq!(heads(&hetzner(), MIB), Some(1));
    assert_eq!(heads(&r2(), GIB), Some(1));
    // Plus a HEAD before the upload starts and another before its completion.
    assert_eq!(heads(&hetzner(), GIB), Some(3));
}

/// R2 refuses a last part larger than the rest, so a short tail is its own
/// part there; everywhere a preset reaches, too.
#[test]
fn a_short_tail_counts_as_its_own_part() {
    let mut work = workload(&r2());
    work.upload(130 * MIB);
    assert_eq!(work.requests.get(&RequestKind::UploadPart), Some(&3));
}

// ---------------------------------------------------------------------------
// GCS: Class A $5/M (every XML PUT and POST, a batch delete included), Class B
// $0.40/M, a single delete free, egress $0.12/GB. No `UploadPartCopy`.
// ---------------------------------------------------------------------------

/// ❗ Off the `enforces_copy_source_pin` allowlist a multipart server-side
/// copy HEADs its source once more right before the completion
/// (`server_copy.rs::source_unchanged`); a one-request copy doesn't. Counted by
/// the engine's request tally on Hetzner and Spaces (live, 2026-10-02).
#[test]
fn a_multipart_copy_off_the_pin_allowlist_heads_its_source_again() {
    let heads = |provider: &S3Provider, size: u64| {
        let mut work = workload(provider);
        work.copy_on_server(size);
        work.counted_requests().get("HeadObject").copied().unwrap_or(0)
    };
    // Source, verify, and the two no-overwrite checks a multipart write makes
    // on a check-then-write provider, plus the pin's stand-in.
    assert_eq!(heads(&hetzner(), GIB), 5);
    // One `CopyObject`: source, verify, one no-overwrite check; no stand-in.
    assert_eq!(heads(&hetzner(), MIB), 3);
    // AWS enforces the pin and refuses an occupied name by header.
    assert_eq!(
        heads(
            &S3Provider::Aws {
                region: "us-east-1".into()
            },
            GIB
        ),
        2
    );
}

#[test]
fn gcs_copies_a_big_object_in_one_request() {
    let mut work = workload(&S3Provider::Gcs);
    work.copy_on_server(GIB);
    let estimate = estimate(&work);
    // One `CopyObject` (no parts there): 1 Class A. HEAD source and the
    // verifying HEAD; no HEAD before, since the copy carries GCS's create-only
    // precondition: 2 Class B. 1 × $5/M + 2 × $0.40/M.
    assert_eq!(requests_in(&estimate, "Class A"), 1);
    assert_eq!(requests_in(&estimate, "Class B"), 2);
    close(estimate.total, 0.0000058);
}

#[test]
fn gcs_bills_a_batch_delete_as_class_a_and_downloads_as_egress() {
    let mut work = workload(&S3Provider::Gcs);
    work.delete_object(MIB, None);
    work.download(GIB);
    let estimate = estimate(&work);
    // One `DeleteObjects` ($5/M) and one GET ($0.40/M), plus 1 GiB at $0.12.
    assert_eq!(requests_in(&estimate, "Class A"), 1);
    close(estimate.total, 0.12 + 0.000005 + 0.0000004);
}

// ---------------------------------------------------------------------------
// Spaces: requests free, storage and downloads inside the $5 base price.
// ---------------------------------------------------------------------------

#[test]
fn spaces_charges_nothing_per_operation() {
    let mut work = workload(&spaces());
    work.upload(GIB);
    work.download(GIB);
    work.copy_on_server(GIB);
    let estimate = estimate(&work);
    assert_eq!(estimate.provider_label, "DigitalOcean Spaces");
    close(estimate.total, 0.0);
}

// ---------------------------------------------------------------------------
// Parsing a served table
// ---------------------------------------------------------------------------

fn problem_of(json: &str) -> Result<PriceTable, TableProblem> {
    PriceTable::parse(json).map_err(|error| error.0)
}

fn table_with(provider_json: &str) -> String {
    format!(r#"{{"schemaVersion": 1, "providers": {{"aws": {provider_json}}}}}"#)
}

const ALL_FREE_CLASSES: &str = r#""requestClasses": [{"name": "All", "perMillion": 0, "operations": ["PutObject",
    "CopyObject", "CreateMultipartUpload", "UploadPart", "UploadPartCopy", "CompleteMultipartUpload",
    "ListObjectsV2", "GetObject", "HeadObject", "DeleteObject", "DeleteObjects", "AbortMultipartUpload"]}]"#;

fn provider_json(classes: &str, egress: &str) -> String {
    format!(
        r#"{{"label": "AWS", "currency": "USD", "asOf": "2026-10-01", "source": "https://example.com",
        {classes}, "egressPerGb": {egress}, "storagePerGbMonth": 0.023, "minimumStorageDays": 0,
        "minimumBillableObjectBytes": 0}}"#
    )
}

#[test]
fn a_minimal_table_parses() {
    let json = table_with(&provider_json(ALL_FREE_CLASSES, "0.09"));
    let table = PriceTable::parse(&json).expect("valid");
    let mut work = workload(&aws());
    work.download(GIB);
    close(Estimate::of(&table, &work).unwrap().total, 0.09);
}

#[test]
fn malformed_json_is_refused() {
    assert!(matches!(problem_of("{not json"), Err(TableProblem::Malformed(_))));
}

#[test]
fn a_newer_schema_is_refused() {
    let json = r#"{"schemaVersion": 2, "providers": {}}"#;
    assert_eq!(problem_of(json), Err(TableProblem::UnsupportedVersion(2)));
}

#[test]
fn an_unpriced_operation_is_refused() {
    let classes = r#""requestClasses": [{"name": "Some", "perMillion": 1, "operations": ["PutObject"]}]"#;
    let json = table_with(&provider_json(classes, "0"));
    assert_eq!(
        problem_of(&json),
        Err(TableProblem::InvalidProvider {
            provider: "aws".into(),
            problem: ProviderProblem::Unpriced(RequestKind::CopyObject),
        })
    );
}

#[test]
fn an_operation_priced_twice_is_refused() {
    let classes = ALL_FREE_CLASSES.replace(
        r#"[{"name": "All""#,
        r#"[{"name": "Twice", "perMillion": 1, "operations": ["GetObject"]}, {"name": "All""#,
    );
    let json = table_with(&provider_json(&classes, "0"));
    assert_eq!(
        problem_of(&json),
        Err(TableProblem::InvalidProvider {
            provider: "aws".into(),
            problem: ProviderProblem::PricedTwice(RequestKind::GetObject),
        })
    );
}

#[test]
fn a_negative_price_is_refused() {
    let json = table_with(&provider_json(ALL_FREE_CLASSES, "-0.01"));
    assert_eq!(
        problem_of(&json),
        Err(TableProblem::InvalidProvider {
            provider: "aws".into(),
            problem: ProviderProblem::BadNumber("egressPerGb"),
        })
    );
}

#[test]
fn unknown_operations_providers_and_fields_are_ignored() {
    // A newer server may name operations and providers this build doesn't
    // know; they mustn't cost the user their estimate.
    let classes = ALL_FREE_CLASSES.replace(r#""PutObject","#, r#""PutObject", "PutBucketCors","#);
    let provider = provider_json(&classes, "0").replace(r#""label""#, r#""newField": true, "label""#);
    let json = format!(r#"{{"schemaVersion": 1, "providers": {{"aws": {provider}, "newcloud": {provider}}}}}"#);
    assert!(PriceTable::parse(&json).is_ok());
}

/// Into a folder the operation made, a one-request copy sends only its source
/// HEAD: no no-overwrite HEAD and no verifying one (`server_copy.rs`). A copy
/// in parts keeps its verify and, off the pin allowlist, its stand-in.
#[test]
fn a_copy_into_a_fresh_folder_heads_only_its_source() {
    let heads = |size: u64| {
        let mut work = workload(&hetzner());
        work.copy_on_server_fresh(size);
        work.counted_requests().get("HeadObject").copied().unwrap_or(0)
    };
    assert_eq!(heads(MIB), 1);
    assert_eq!(heads(GIB), 3);
}

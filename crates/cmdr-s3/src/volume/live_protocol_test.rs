//! What each real provider does at the protocol level, one finding per line:
//! conditional writes, short bodies, multipart shapes, `UploadPartCopy`,
//! cross-bucket copies, batch delete, bucket calls and their errors, names,
//! metadata, and checksums. The profile's allowlists are set from these
//! findings (`crates/cmdr-s3/DETAILS.md` § "Verified providers").
//!
//! ❗ Each cell also asserts the profile against what it saw, so an allowlist
//! that trusts a header a provider ignores fails the run.
//!
//! Skips without `CMDR_S3_LIVE=1` (`live_support.rs`); the runner is
//! `apps/desktop/test/s3-servers/live.sh`.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use http::Method;

use super::live_support::*;
use crate::encoding::{encode_component, encode_key};
use crate::ops::{self, ListObjectsParams};
use crate::profile::{ConditionalOp, NoOverwrite};
use crate::request::{Body, S3Request};
use crate::transport::{Answer, S3Client, UploadBody};
use crate::xml::build::{CompletedPart, delete_objects_body};
use crate::xml::{parse_delete_result, parse_list_buckets, parse_list_objects};

/// A `CopyObject` or `UploadPartCopy` from any bucket to any bucket, built by
/// hand so a profile that forbids cross-bucket copies can still be asked.
pub(super) fn copy_request(
    client: &S3Client,
    (from_bucket, from_key): (&str, &str),
    (to_bucket, to_key): (&str, &str),
    extra: &[(&str, &str)],
) -> S3Request {
    let profile = client.profile();
    let location = profile
        .locate(Some(to_bucket), Some(to_key))
        .expect("a live key builds");
    let mut request = S3Request::new(Method::PUT, &profile.scheme, &location.host, location.path);
    request.bucket = Some(to_bucket.to_string());
    let source = format!(
        "/{}/{}",
        encode_component(from_bucket),
        encode_key(&profile.normalize_key(from_key)).expect("a live key encodes")
    );
    with_headers(with_headers(request, &[("x-amz-copy-source", source.as_str())]), extra)
}

/// One `UploadPartCopy` of `range` (inclusive) or of the whole source.
async fn part_copy(
    live: &Live,
    client: &S3Client,
    (from_bucket, from_key): (&str, &str),
    (key, upload_id, number): (&str, &str, u32),
    range: Option<(u64, u64)>,
    extra: &[(&str, &str)],
) -> Result<CompletedPart, String> {
    let mut request = copy_request(client, (from_bucket, from_key), (&live.bucket, key), extra)
        .query("partNumber", &number.to_string())
        .query("uploadId", upload_id);
    if let Some((first, last)) = range {
        request = with_headers(
            request,
            &[("x-amz-copy-source-range", &format!("bytes={first}-{last}"))],
        );
    }
    let answer = live.send(client, request).await;
    let text = answer.text();
    if !answer.status.is_success() || text.contains("<Error>") {
        return Err(verdict(&answer));
    }
    let etag = crate::xml::parse_copy_result(&text)
        .ok()
        .and_then(|outcome| outcome.etag)
        .ok_or_else(|| format!("{} without an ETag", answer.status.as_u16()))?;
    Ok(CompletedPart { number, etag })
}

/// What a no-overwrite write did: refused and kept the old bytes, or wrote.
fn outcome(answer: &Answer, before: u64, after: Option<u64>) -> (bool, String) {
    let kept = after == Some(before);
    let enforced = answer.status.as_u16() == 412 && kept;
    let said = verdict(answer);
    let finding = match (enforced, kept) {
        (true, _) => format!("enforced ({said})"),
        (false, true) => format!("refused without 412 ({said}), old object kept"),
        (false, false) => format!("IGNORED ({said}), object overwritten"),
    };
    (enforced, finding)
}

/// Sends one no-overwrite `op` carrying `header` over the occupied `taken`,
/// with a body of a length nothing else there has: did it refuse?
async fn try_over(
    live: &Live,
    client: &S3Client,
    prefix: &str,
    op: ConditionalOp,
    header: (&str, &str),
    len: usize,
) -> bool {
    let taken = format!("{prefix}taken.txt");
    breathe().await;
    let before = live.length(client, &taken).await.expect("the occupied key is there");
    let answer = match op {
        ConditionalOp::Put => live.put(client, &taken, &pattern(len, 1), &[header]).await,
        ConditionalOp::CompleteMultipart => {
            let upload_id = live.create_upload(client, &taken).await;
            let part = live
                .upload_part(client, &taken, &upload_id, 1, pattern(len, 2))
                .await
                .expect("a one-part upload's part lands");
            let answer = live.complete(client, &taken, &upload_id, &[part], &[header]).await;
            if !answer.status.is_success() {
                live.abort(client, &taken, &upload_id).await;
            }
            answer
        }
        ConditionalOp::Copy => {
            let source = format!("{prefix}source-{len}.txt");
            assert!(
                live.put(client, &source, &pattern(len, 3), &[])
                    .await
                    .status
                    .is_success()
            );
            let copy = copy_request(client, (&live.bucket, &source), (&live.bucket, &taken), &[header]);
            live.send(client, copy).await
        }
    };
    let (enforced, finding) = outcome(&answer, before, live.length(client, &taken).await);
    report(live, &format!("{} on {op:?}", header.0), &finding);
    enforced
}

/// One no-overwrite Put or Copy built the way the volume builds it
/// (`Overwrite::Refuse`, whatever the profile lists), over the occupied
/// `taken` and onto a free key: did it refuse the first and write the second?
async fn try_built(live: &Live, client: &S3Client, prefix: &str, op: ConditionalOp, len: usize) -> bool {
    let taken = format!("{prefix}taken.txt");
    let free = format!("{prefix}free-built-{len}.txt");
    breathe().await;
    let before = live.length(client, &taken).await.expect("the occupied key is there");
    let source = format!("{prefix}source-built-{len}.txt");
    if op == ConditionalOp::Copy {
        assert!(
            live.put(client, &source, &pattern(len, 3), &[])
                .await
                .status
                .is_success()
        );
    }
    let send = |key: String| {
        let source = source.clone();
        async move {
            let built = match op {
                ConditionalOp::Copy => ops::copy_object(
                    client.profile(),
                    ops::CopySource {
                        bucket: &live.bucket,
                        key: &source,
                    },
                    None,
                    &live.bucket,
                    &key,
                    ops::Overwrite::Refuse,
                    &ops::MetadataDirective::Copy,
                ),
                _ => ops::put_object(
                    client.profile(),
                    &live.bucket,
                    &key,
                    len as u64,
                    ops::Overwrite::Refuse,
                    &ops::ObjectMetadata::default(),
                ),
            }
            .expect("a live key builds");
            let mut request = built.request;
            if op != ConditionalOp::Copy {
                request.body = Body::Bytes(pattern(len, 1));
            }
            live.send(client, request).await
        }
    };
    let over = send(taken.clone()).await;
    let (enforced, finding) = outcome(&over, before, live.length(client, &taken).await);
    let onto_free = send(free.clone()).await;
    let wrote = onto_free.status.is_success() && live.length(client, &free).await == Some(len as u64);
    report(
        live,
        &format!("{op:?} built with Overwrite::Refuse"),
        format!(
            "over an object: {finding}; on a free key: {} (landed: {wrote})",
            verdict(&onto_free)
        ),
    );
    enforced && wrote
}

/// ❗ The allowlist's ground truth: does each write refuse to overwrite on
/// `If-None-Match: *`, R2's copy header, or GCS's generation precondition? A
/// positive control on a free key proves the header itself isn't refused.
#[tokio::test(flavor = "multi_thread")]
async fn live_conditional_writes_per_operation() {
    use ConditionalOp::{CompleteMultipart, Copy, Put};
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("conditional");
        assert!(
            live.put(&client, &format!("{prefix}taken.txt"), b"first", &[])
                .await
                .status
                .is_success()
        );
        let mut len = 10;
        let mut next_len = || {
            len += 3;
            len
        };

        let none_match = ("if-none-match", "*");
        let mut enforced = std::collections::HashMap::new();
        for op in [Put, CompleteMultipart, Copy] {
            enforced.insert(
                (op, none_match.0),
                try_over(&live, &client, &prefix, op, none_match, next_len()).await,
            );
        }
        let free = live
            .put(&client, &format!("{prefix}free.txt"), b"x", &[none_match])
            .await;
        report(&live, "if-none-match on Put, free key", verdict(&free));
        let cf = ("cf-copy-destination-if-none-match", "*");
        enforced.insert(
            (Copy, cf.0),
            try_over(&live, &client, &prefix, Copy, cf, next_len()).await,
        );
        let generation = ("x-goog-if-generation-match", "0");
        if live.provider == crate::S3Provider::Gcs {
            // GCS's own precondition, refused beside any `x-amz-*` header (400
            // `ExcessHeaderValues`): asked through the builders, which sign it
            // in GCS's dialect, over the occupied key and on a free one.
            for op in [Put, Copy] {
                enforced.insert(
                    (op, generation.0),
                    try_built(&live, &client, &prefix, op, next_len()).await,
                );
            }
        }

        let profile = client.profile();
        for op in [Put, CompleteMultipart, Copy] {
            let listed = profile.no_overwrite(op);
            let header = match listed {
                NoOverwrite::IfNoneMatch => Some(none_match.0),
                NoOverwrite::CloudflareCopyHeader => Some(cf.0),
                NoOverwrite::GoogGenerationMatch => Some(generation.0),
                NoOverwrite::CheckThenWrite => None,
            };
            if let Some(header) = header {
                assert!(
                    enforced.get(&(op, header)).copied().unwrap_or(false),
                    "[{}] {op:?} is allowlisted as {listed:?}, which the provider doesn't enforce",
                    live.name
                );
            }
        }
        live.clean(&client, &prefix).await;
    }
}

/// A body of `promised` bytes that sends `sent` and then fails, the way a
/// cancelled or broken source cuts a PUT off.
fn cut_body(sent: usize) -> UploadBody {
    Box::pin(futures_util::stream::iter([
        Ok(Bytes::from(pattern(sent, 7))),
        Err(io::Error::other("the source broke")),
    ]))
}

/// ❗ Does a PUT cut off before its `Content-Length` publish what arrived
/// (VersityGW does)? Over an existing object and on a free key, looked at
/// right away and again after a while.
#[tokio::test(flavor = "multi_thread")]
async fn live_a_cut_off_put_publishes_nothing() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("short-body");
        let existing = format!("{prefix}existing.bin");
        let original = pattern(1_000, 1);
        assert!(live.put(&client, &existing, &original, &[]).await.status.is_success());
        let free = format!("{prefix}free.bin");
        breathe().await;

        for key in [&existing, &free] {
            let built = ops::put_object(
                client.profile(),
                &live.bucket,
                key,
                (4 * MIB) as u64,
                ops::Overwrite::Replace,
                &ops::ObjectMetadata::default(),
            )
            .expect("builds");
            let sent = client.upload(built.request, cut_body(2 * MIB)).await;
            let how = match &sent {
                Ok(answer) => format!("answered {}", verdict(answer)),
                Err(_) => "the request failed on our side".to_string(),
            };
            // allowed-test-sleep: a real provider's settle time is what's measured; there's no condition to wait on
            tokio::time::sleep(Duration::from_secs(2)).await;
            let soon = live.length(&client, key).await;
            // allowed-test-sleep: a real provider's settle time is what's measured; there's no condition to wait on
            tokio::time::sleep(Duration::from_secs(15)).await;
            let later = live.length(&client, key).await;
            let which = if key == &existing {
                "over an object"
            } else {
                "on a free key"
            };
            let finding = match (key == &existing, soon, later) {
                (true, Some(1_000), Some(1_000)) => "refused, original kept".to_string(),
                (false, None, None) => "refused, nothing published".to_string(),
                _ => format!("PUBLISHED something: {soon:?} B after 2 s, {later:?} B after 17 s"),
            };
            report(&live, &format!("cut-off PUT {which} ({how})"), &finding);
            let refused = finding.starts_with("refused");
            if client.profile().refuses_short_body {
                assert!(refused, "[{}] on the short-body allowlist, but {finding}", live.name);
            }
        }
        live.clean(&client, &prefix).await;
    }
}

/// A part body that sends 2 MiB, waits for `go`, then fails.
fn stalled_body(go: Arc<tokio::sync::Notify>) -> UploadBody {
    Box::pin(futures_util::stream::unfold(0u8, move |step| {
        let go = Arc::clone(&go);
        async move {
            match step {
                0 => Some((Ok(Bytes::from(pattern(2 * MIB, 3))), 1)),
                1 => {
                    go.notified().await;
                    Some((Err(io::Error::other("cut off")), 2))
                }
                _ => None,
            }
        }
    }))
}

/// An `UploadPart` cut off while an abort races it: the abort's answer, and
/// whether the upload or an object survives.
#[tokio::test(flavor = "multi_thread")]
async fn live_a_cut_off_part_racing_an_abort_leaves_nothing() {
    for live in live_targets() {
        let client = Arc::new(live.client());
        let prefix = live_prefix("part-abort");
        let key = format!("{prefix}raced.bin");
        let upload_id = live.create_upload(&client, &key).await;
        let go = Arc::new(tokio::sync::Notify::new());
        let request =
            ops::upload_part(client.profile(), &live.bucket, &key, &upload_id, 1, (6 * MIB) as u64).expect("builds");
        let part = {
            let (client, go) = (Arc::clone(&client), Arc::clone(&go));
            tokio::spawn(async move { client.upload(request, stalled_body(go)).await.map(|a| verdict(&a)) })
        };
        // allowed-test-sleep: a real provider's settle time is what's measured; there's no condition to wait on
        tokio::time::sleep(Duration::from_secs(2)).await;
        let aborted = live.abort(&client, &key, &upload_id).await;
        go.notify_one();
        let part = part.await.expect("the part task ends");
        // allowed-test-sleep: a real provider's settle time is what's measured; there's no condition to wait on
        tokio::time::sleep(Duration::from_secs(2)).await;
        let listed = live.uploads_under(&client, &prefix).await;
        let object = live.length(&client, &key).await;
        report(
            &live,
            "cut-off UploadPart racing AbortMultipartUpload",
            format!(
                "abort {}, part {}, upload still listed: {}, object: {object:?}",
                verdict(&aborted),
                part.unwrap_or_else(|_| "failed on our side".to_string()),
                listed
                    .as_ref()
                    .map_or_else(|e| e.clone(), |l| l.iter().any(|(_, id)| id == &upload_id).to_string()),
            ),
        );
        assert!(
            object.is_none(),
            "[{}] a raced, aborted upload published an object",
            live.name
        );
        live.clean(&client, &prefix).await;
    }
}

/// Uploads parts of `sizes` (MiB) and completes: the completion's verdict and
/// the object's length after.
pub(super) async fn shaped_upload(live: &Live, client: &S3Client, key: &str, sizes: &[usize]) -> (String, Option<u64>) {
    let upload_id = live.create_upload(client, key).await;
    let mut parts = Vec::new();
    for (index, mib) in sizes.iter().enumerate() {
        let number = index as u32 + 1;
        match live
            .upload_part(client, key, &upload_id, number, pattern(mib * MIB, number as u8))
            .await
        {
            Ok(part) => parts.push(part),
            Err(why) => {
                live.abort(client, key, &upload_id).await;
                return (format!("part {number} refused: {why}"), None);
            }
        }
    }
    let answer = live.complete(client, key, &upload_id, &parts, &[]).await;
    let said = verdict(&answer);
    if !answer.status.is_success() || said.contains("embedded") {
        live.abort(client, key, &upload_id).await;
    }
    (said, live.length(client, key).await)
}

/// Equal parts, a last part larger or smaller than the rest, a first part
/// larger than the second, and a part under the 5 MiB minimum that isn't last.
#[tokio::test(flavor = "multi_thread")]
async fn live_multipart_part_shapes() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("shapes");
        for (label, sizes) in [
            ("equal parts 5+5+5 MiB", &[5, 5, 5][..]),
            ("last part larger 5+5+7 MiB", &[5, 5, 7][..]),
            ("last part smaller 5+5+2 MiB", &[5, 5, 2][..]),
            ("first part larger 6+5+5 MiB", &[6, 5, 5][..]),
            ("a 4 MiB part before the last 4+5 MiB", &[4, 5][..]),
        ] {
            let key = format!("{prefix}{}.bin", label.replace(' ', "-"));
            let (said, length) = shaped_upload(&live, &client, &key, sizes).await;
            let expected = sizes.iter().sum::<usize>() * MIB;
            let landed = length == Some(expected as u64);
            report(
                &live,
                &format!("multipart {label}"),
                format!("{said}, landed whole: {landed}"),
            );
        }
        live.clean(&client, &prefix).await;
    }
}

/// `UploadPartCopy` ranged, whole, with a source under 5 MiB as the last part,
/// and pinned to a wrong ETag.
#[tokio::test(flavor = "multi_thread")]
async fn live_upload_part_copy_shapes() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("part-copy");
        let big = format!("{prefix}big.bin");
        let small = format!("{prefix}small.bin");
        assert!(
            live.put(&client, &big, &pattern(12 * MIB, 9), &[])
                .await
                .status
                .is_success()
        );
        assert!(
            live.put(&client, &small, &pattern(MIB, 4), &[])
                .await
                .status
                .is_success()
        );
        let b = live.bucket.clone();
        let five = (5 * MIB) as u64;

        for (label, parts) in [
            (
                "ranged 0..5 MiB + 5..12 MiB",
                vec![
                    (big.as_str(), Some((0, five - 1))),
                    (big.as_str(), Some((five, (12 * MIB) as u64 - 1))),
                ],
            ),
            ("whole source, no range", vec![(big.as_str(), None)]),
            (
                "a 1 MiB source as the last part",
                vec![
                    (big.as_str(), Some((0, five - 1))),
                    (small.as_str(), Some((0, MIB as u64 - 1))),
                ],
            ),
        ] {
            let key = format!("{prefix}{}.bin", label.replace(' ', "-"));
            let upload_id = live.create_upload(&client, &key).await;
            let mut done = Vec::new();
            let mut refused = None;
            for (index, (source, range)) in parts.iter().enumerate() {
                match part_copy(
                    &live,
                    &client,
                    (&b, source),
                    (&key, &upload_id, index as u32 + 1),
                    *range,
                    &[],
                )
                .await
                {
                    Ok(part) => done.push(part),
                    Err(why) => {
                        refused = Some(format!("part {} refused: {why}", index + 1));
                        break;
                    }
                }
            }
            let finding = match refused {
                Some(why) => {
                    live.abort(&client, &key, &upload_id).await;
                    why
                }
                None => {
                    let answer = live.complete(&client, &key, &upload_id, &done, &[]).await;
                    if !answer.status.is_success() {
                        live.abort(&client, &key, &upload_id).await;
                    }
                    format!(
                        "complete {}, length {:?}",
                        verdict(&answer),
                        live.length(&client, &key).await
                    )
                }
            };
            report(&live, &format!("UploadPartCopy {label}"), finding);
        }

        let key = format!("{prefix}pinned.bin");
        let upload_id = live.create_upload(&client, &key).await;
        let pinned = part_copy(
            &live,
            &client,
            (&b, &big),
            (&key, &upload_id, 1),
            Some((0, five - 1)),
            &[("x-amz-copy-source-if-match", "\"0123456789abcdef0123456789abcdef\"")],
        )
        .await;
        report(
            &live,
            "UploadPartCopy pinned to a wrong ETag",
            match pinned {
                Ok(_) => "IGNORED, the part copied".to_string(),
                Err(why) => format!("refused: {why}"),
            },
        );
        live.abort(&client, &key, &upload_id).await;
        live.clean(&client, &prefix).await;
    }
}

/// `CopyObject` and `UploadPartCopy` from this bucket into a second one the
/// same key reaches (only where the runner names one).
#[tokio::test(flavor = "multi_thread")]
async fn live_cross_bucket_copy() {
    for live in live_targets() {
        let Some(other) = live.bucket_2.clone() else {
            report(&live, "cross-bucket copy", "unverified: no second bucket for this key");
            continue;
        };
        let client = live.client();
        let prefix = live_prefix("cross-bucket");
        let source = format!("{prefix}source.bin");
        assert!(
            live.put(&client, &source, &pattern(6 * MIB, 2), &[])
                .await
                .status
                .is_success()
        );
        let to = format!("{prefix}copied.bin");
        let answer = live
            .send(
                &client,
                copy_request(&client, (&live.bucket, &source), (&other, &to), &[]),
            )
            .await;
        let head = ops::head_object(client.profile(), &other, &to).expect("builds");
        let landed = live.send(&client, head).await;
        report(
            &live,
            "cross-bucket CopyObject",
            format!(
                "{}, the copy's length: {:?}",
                verdict(&answer),
                landed.header("content-length")
            ),
        );
        let ok = answer.status.is_success() && !answer.text().contains("<Error>");

        // `UploadPartCopy` from this bucket into an upload in the other one.
        let parted = format!("{prefix}parted.bin");
        let create = ops::create_multipart_upload(client.profile(), &other, &parted, &ops::ObjectMetadata::default())
            .expect("builds");
        let created = live.send(&client, create).await;
        let upload_id = crate::xml::parse_initiate_multipart(&created.text())
            .expect("an upload id")
            .upload_id;
        let request = copy_request(&client, (&live.bucket, &source), (&other, &parted), &[])
            .query("partNumber", "1")
            .query("uploadId", &upload_id);
        let part = live.send(&client, request).await;
        report(&live, "cross-bucket UploadPartCopy", verdict(&part));
        let abort = ops::abort_multipart_upload(client.profile(), &other, &parted, &upload_id).expect("builds");
        live.send(&client, abort).await;
        assert!(
            ok || !client.profile().cross_bucket_copy(),
            "[{}] the profile copies across buckets, but the provider refused",
            live.name
        );
        // The rest of the cleanup works in `bucket`; this one copy is in `other`.
        let request = ops::delete_object(client.profile(), &other, &to).expect("builds");
        live.send(&client, request).await;
        live.clean(&client, &prefix).await;
    }
}

/// `DeleteObjects`: three real keys, a thousand absent ones, and one more than
/// the documented maximum.
#[tokio::test(flavor = "multi_thread")]
async fn live_batch_delete() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("batch-delete");
        let keys: Vec<String> = (0..3).map(|n| format!("{prefix}doomed-{n}.txt")).collect();
        for key in &keys {
            assert!(live.put(&client, key, b"doomed", &[]).await.status.is_success());
        }
        let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let request = ops::delete_objects(client.profile(), &live.bucket, &refs).expect("builds");
        let answer = live.send(&client, request).await;
        let parsed = answer
            .status
            .is_success()
            .then(|| parse_delete_result(&answer.text()).map(|o| o.failed.len()));
        let left = live.keys_under(&client, &prefix).await.len();
        report(
            &live,
            "DeleteObjects of three keys",
            format!("{}, failures reported: {parsed:?}, keys left: {left}", verdict(&answer)),
        );

        let absent: Vec<String> = (0..1_000).map(|n| format!("{prefix}absent-{n}")).collect();
        let refs: Vec<&str> = absent.iter().map(String::as_str).collect();
        let request = ops::delete_objects(client.profile(), &live.bucket, &refs).expect("builds");
        report(
            &live,
            "DeleteObjects of 1,000 absent keys",
            verdict(&live.send(&client, request).await),
        );

        // One past the maximum, built by hand (the builder refuses it).
        let absent: Vec<String> = (0..1_001).map(|n| format!("{prefix}absent-{n}")).collect();
        let refs: Vec<&str> = absent.iter().map(String::as_str).collect();
        let mut request = ops::delete_objects(client.profile(), &live.bucket, &refs[..1_000]).expect("builds");
        let body = delete_objects_body(&refs, true).into_bytes();
        let md5 = STANDARD.encode(<md5::Md5 as md5::Digest>::digest(&body));
        request = with_headers(request, &[("content-md5", &md5)]);
        request.body = Body::Bytes(body);
        report(
            &live,
            "DeleteObjects of 1,001 keys",
            verdict(&live.send(&client, request).await),
        );
        live.clean(&client, &prefix).await;
    }
}

/// `ListMultipartUploads` finds an upload and `AbortMultipartUpload` removes it.
#[tokio::test(flavor = "multi_thread")]
async fn live_list_and_abort_uploads() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("uploads");
        let key = format!("{prefix}open.bin");
        let upload_id = live.create_upload(&client, &key).await;
        let before = live.uploads_under(&client, &prefix).await;
        let unprefixed = live.uploads_under(&client, "").await;
        live.upload_part(&client, &key, &upload_id, 1, pattern(MIB, 5))
            .await
            .expect("a part lands");
        // allowed-test-sleep: a real provider's settle time is what's measured; there's no condition to wait on
        tokio::time::sleep(Duration::from_secs(3)).await;
        let later = live.uploads_under(&client, &prefix).await;
        let aborted = live.abort(&client, &key, &upload_id).await;
        let after = live.uploads_under(&client, &prefix).await;
        let again = live.abort(&client, &key, &upload_id).await;
        let part_after = live.upload_part(&client, &key, &upload_id, 2, pattern(MIB, 6)).await;
        let has = |l: &Result<Vec<(String, String)>, String>| {
            l.as_ref()
                .map_or_else(|e| e.clone(), |l| l.iter().any(|(_, id)| id == &upload_id).to_string())
        };
        report(
            &live,
            "ListMultipartUploads + AbortMultipartUpload",
            format!(
                "listed at once: {}, unprefixed: {}, with a part 3 s later: {}, abort {}, listed after: {}, second abort {}, a part after the abort: {}",
                has(&before),
                has(&unprefixed),
                has(&later),
                verdict(&aborted),
                has(&after),
                verdict(&again),
                part_after.map_or_else(|why| format!("refused, {why}"), |_| "ACCEPTED".to_string())
            ),
        );
        live.clean(&client, &prefix).await;
    }
}

/// `ListBuckets` with a bucket-scoped key, `HeadBucket` on the bucket and on a
/// missing one, and the errors for a missing bucket and a wrong secret.
#[tokio::test(flavor = "multi_thread")]
async fn live_bucket_calls_and_their_errors() {
    for live in live_targets() {
        let client = live.client();
        let answer = live.send(&client, ops::list_buckets(client.profile(), None)).await;
        let count = answer
            .status
            .is_success()
            .then(|| parse_list_buckets(&answer.text()).map(|page| page.buckets.len()));
        report(
            &live,
            "ListBuckets",
            format!("{}, buckets: {count:?}", verdict(&answer)),
        );

        let head = ops::head_bucket(client.profile(), &live.bucket).expect("builds");
        report(&live, "HeadBucket", verdict(&live.send(&client, head).await));
        let missing = "cmdr-live-no-such-bucket-4d1e";
        let head = ops::head_bucket(client.profile(), missing).expect("builds");
        report(
            &live,
            "HeadBucket of a missing bucket",
            verdict(&live.send(&client, head).await),
        );
        let list = ops::list_objects(client.profile(), missing, &ListObjectsParams::default()).expect("builds");
        report(
            &live,
            "ListObjectsV2 of a missing bucket",
            verdict(&live.send(&client, list).await),
        );

        let wrong = live.client_with("0000000000000000000000000000000000000000");
        let list = ops::list_objects(wrong.profile(), &live.bucket, &ListObjectsParams::default()).expect("builds");
        report(
            &live,
            "ListObjectsV2 with a wrong secret",
            verdict(&live.send(&wrong, list).await),
        );
        let head = ops::head_bucket(wrong.profile(), &live.bucket).expect("builds");
        report(
            &live,
            "HeadBucket with a wrong secret",
            verdict(&live.send(&wrong, head).await),
        );
        let answer = live.send(&wrong, ops::list_buckets(wrong.profile(), None)).await;
        report(&live, "ListBuckets with a wrong secret", verdict(&answer));

        report(
            &live,
            "connect to the account root",
            format!("{:?}", live.connect(None).await.err()),
        );
        report(
            &live,
            "connect to a missing bucket",
            format!("{:?}", live.connect(Some(missing)).await.err()),
        );
        report(
            &live,
            "connect to the bucket",
            format!("{:?}", live.connect(Some(&live.bucket)).await.err()),
        );
    }
}

/// A delimited listing, `encoding-type=url` echoed or not, and awkward keys
/// (a space and a `+`) coming back byte for byte.
#[tokio::test(flavor = "multi_thread")]
async fn live_listing_with_a_delimiter_and_awkward_names() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("listing");
        let keys = [
            "a b+c.txt",
            "x + y/z.txt",
            "dir/one.txt",
            "dir/sub/two.txt",
            "plain.txt",
        ];
        for key in keys {
            assert!(
                live.put(&client, &format!("{prefix}{key}"), b"k", &[])
                    .await
                    .status
                    .is_success()
            );
        }
        let params = ListObjectsParams {
            prefix: &prefix,
            delimiter: Some("/"),
            continuation_token: None,
            max_keys: None,
        };
        let request = ops::list_objects(client.profile(), &live.bucket, &params).expect("builds");
        let answer = live.send(&client, request).await;
        let text = answer.text();
        let page = parse_list_objects(&text).expect("a listing");
        let mut objects: Vec<String> = page.objects.iter().map(|o| o.key[prefix.len()..].to_string()).collect();
        let mut folders: Vec<String> = page.prefixes.iter().map(|p| p[prefix.len()..].to_string()).collect();
        objects.sort();
        folders.sort();
        report(
            &live,
            "ListObjectsV2 with a delimiter",
            format!(
                "objects {objects:?}, folders {folders:?}, encoding-type echoed: {}",
                text.contains("<EncodingType>url</EncodingType>")
            ),
        );
        assert_eq!(objects, ["a b+c.txt", "plain.txt"], "[{}]", live.name);
        assert_eq!(folders, ["dir/", "x + y/"], "[{}]", live.name);
        live.clean(&client, &prefix).await;
    }
}

/// What a provider does with a decomposed (NFD) key next to its composed (NFC)
/// twin: two objects, or one.
#[tokio::test(flavor = "multi_thread")]
async fn live_unicode_normalization_of_keys() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("unicode");
        let nfd = format!("{prefix}cafe\u{301}.txt");
        let nfc = format!("{prefix}caf\u{e9}.txt");
        let mut put = live.raw_object(&client, Method::PUT, &nfd);
        put.body = Body::Bytes(b"decomposed".to_vec());
        let nfd_put = live.send(&client, put).await;
        breathe().await;
        let head_nfc = live.send(&client, live.raw_object(&client, Method::HEAD, &nfc)).await;
        let listed = live.keys_under(&client, &prefix).await;
        let forms: Vec<&str> = listed
            .iter()
            .map(|k| {
                if k == &nfd {
                    "NFD"
                } else if k == &nfc {
                    "NFC"
                } else {
                    "other"
                }
            })
            .collect();
        let mut put = live.raw_object(&client, Method::PUT, &nfc);
        put.body = Body::Bytes(b"composed!".to_vec());
        live.send(&client, put).await;
        let both = live.keys_under(&client, &prefix).await.len();
        let nfd_now = live.send(&client, live.raw_object(&client, Method::GET, &nfd)).await;
        report(
            &live,
            "an NFD key",
            format!(
                "PUT {}, HEAD of its NFC twin {}, listed as {forms:?}; after a PUT of the NFC twin: {both} object(s), the NFD key reads {:?}",
                verdict(&nfd_put),
                head_nfc.status.as_u16(),
                String::from_utf8_lossy(&nfd_now.body)
            ),
        );
        let one_object = both == 1;
        assert_eq!(
            client.profile().nfc_keys,
            one_object,
            "[{}] `nfc_keys` must match whether the twins are one object",
            live.name
        );
        let keys = live.keys_under(&client, &prefix).await;
        for key in keys {
            live.send(&client, live.raw_object(&client, Method::DELETE, &key)).await;
        }
        live.clean(&client, &prefix).await;
    }
}

/// CRC-32 (IEEE), the checksum AWS SDKs send by default on PUT.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// CRC-64/NVME, the SDKs' newer default.
fn crc64nvme(bytes: &[u8]) -> u64 {
    let mut crc = !0u64;
    for byte in bytes {
        crc ^= u64::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0x9A6C_9329_AC4B_C9B5
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn the_checksums_match_their_published_check_values() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc64nvme(b"123456789"), 0xAE8B_1486_0A79_9888);
}

/// `x-amz-meta-mtime` round trip (the header name a HEAD answers with), and
/// whether the SDKs' default checksum headers are accepted, and checked.
#[tokio::test(flavor = "multi_thread")]
async fn live_metadata_and_checksum_headers() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("metadata");
        let key = format!("{prefix}dated.txt");
        let mtime = "1354040105.123456789";
        live.put(&client, &key, b"dated", &[("x-amz-meta-mtime", mtime)]).await;
        let head = live.head(&client, &key).await;
        let names: Vec<String> = head
            .headers
            .keys()
            .map(|name| name.as_str().to_string())
            .filter(|name| name.contains("meta"))
            .collect();
        report(
            &live,
            "x-amz-meta-mtime round trip",
            format!(
                "HEAD answers {:?}, meta headers {names:?}",
                head.header("x-amz-meta-mtime")
            ),
        );
        assert_eq!(head.header("x-amz-meta-mtime"), Some(mtime), "[{}]", live.name);

        let bytes = b"checksummed body";
        let crc = STANDARD.encode(crc32(bytes).to_be_bytes());
        let answer = live
            .put(
                &client,
                &format!("{prefix}crc32.txt"),
                bytes,
                &[
                    ("x-amz-checksum-crc32", &crc),
                    ("x-amz-sdk-checksum-algorithm", "CRC32"),
                ],
            )
            .await;
        report(&live, "PUT with x-amz-checksum-crc32", verdict(&answer));
        let wrong = STANDARD.encode(crc32(b"something else").to_be_bytes());
        let answer = live
            .put(
                &client,
                &format!("{prefix}crc32-wrong.txt"),
                bytes,
                &[
                    ("x-amz-checksum-crc32", &wrong),
                    ("x-amz-sdk-checksum-algorithm", "CRC32"),
                ],
            )
            .await;
        report(&live, "PUT with a WRONG x-amz-checksum-crc32", verdict(&answer));
        let crc = STANDARD.encode(crc64nvme(bytes).to_be_bytes());
        let answer = live
            .put(
                &client,
                &format!("{prefix}crc64.txt"),
                bytes,
                &[
                    ("x-amz-checksum-crc64nvme", &crc),
                    ("x-amz-sdk-checksum-algorithm", "CRC64NVME"),
                ],
            )
            .await;
        report(&live, "PUT with x-amz-checksum-crc64nvme", verdict(&answer));
        live.clean(&client, &prefix).await;
    }
}

/// Leftovers older than this are a crashed run's. ❗ Younger ones may belong
/// to a run still going: several runners share one bucket (R2's key reaches
/// only one), and a sweep of everything under `cmdr-live/` once deleted a
/// running flow's objects out from under it.
const STALE_AFTER: Duration = Duration::from_secs(3 * 60 * 60);

fn is_stale(when: Option<std::time::SystemTime>) -> bool {
    when.and_then(|t| t.elapsed().ok()).is_some_and(|age| age > STALE_AFTER)
}

/// Every key under `cmdr-live/` in `bucket` older than [`STALE_AFTER`].
async fn stale_keys(live: &Live, client: &S3Client, bucket: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let params = ListObjectsParams {
            prefix: LIVE_ROOT,
            delimiter: None,
            continuation_token: token.as_deref(),
            max_keys: None,
        };
        let request = ops::list_objects(client.profile(), bucket, &params).expect("builds");
        let answer = live.send(client, request).await;
        let Ok(page) = parse_list_objects(&answer.text()) else {
            return keys;
        };
        keys.extend(
            page.objects
                .into_iter()
                .filter(|o| is_stale(o.last_modified))
                .map(|o| o.key),
        );
        match page.next_continuation_token {
            Some(next) if page.is_truncated => token = Some(next),
            _ => return keys,
        }
    }
}

/// Removes what crashed live runs left under `cmdr-live/`, open uploads
/// included, in both buckets: everything older than [`STALE_AFTER`]. The
/// runner calls it last; each cell already deletes its own.
#[tokio::test(flavor = "multi_thread")]
async fn live_cleanup_removes_every_leftover() {
    for live in live_targets() {
        let client = live.client();
        let request = ops::list_multipart_uploads(client.profile(), &live.bucket, LIVE_ROOT, None).expect("builds");
        let answer = live.send(&client, request).await;
        if let Ok(page) = crate::xml::parse_list_multipart_uploads(&answer.text()) {
            for upload in page.uploads.into_iter().filter(|u| is_stale(u.initiated)) {
                live.abort(&client, &upload.key, &upload.upload_id).await;
            }
        }
        let stale = stale_keys(&live, &client, &live.bucket).await;
        live.delete_each(&client, &stale).await;
        if let Some(other) = &live.bucket_2 {
            for key in stale_keys(&live, &client, other).await {
                let request = ops::delete_object(client.profile(), other, &key).expect("builds");
                live.send(&client, request).await;
            }
        }
        let left = stale_keys(&live, &client, &live.bucket).await.len();
        let fresh = live.keys_under(&client, LIVE_ROOT).await.len();
        report(
            &live,
            "cleanup",
            format!("stale objects left: {left}, recent ones left to their runs: {fresh}"),
        );
        assert_eq!(left, 0, "[{}]", live.name);
    }
}

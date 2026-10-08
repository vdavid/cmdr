//! What a one-request `CopyObject` does with its source pin and its ETag on
//! each real provider: the evidence behind pinning a single copy, GCS's
//! `refuses_multipart_copy_pin`, and proving a lost answer by the write token
//! (`crates/cmdr-s3/DETAILS.md` § "Verified providers").
//!
//! Skips without `CMDR_S3_LIVE=1` (`live_support.rs`); the runner is
//! `apps/desktop/test/s3-servers/live.sh`.

use super::live_protocol_test::{copy_request, shaped_upload};
use super::live_support::*;
use crate::ops;

/// What a one-request `CopyObject` answers as its ETag, for a single-part
/// source and a multipart one, and whether it enforces
/// `x-amz-copy-source-if-match`: the evidence for proving a lost answer by
/// the write token rather than the ETag (`DETAILS.md` § "Server-side copy").
#[tokio::test(flavor = "multi_thread")]
async fn live_copy_object_etags() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("copy-etag");
        let single = format!("{prefix}single.bin");
        assert!(
            live.put(&client, &single, &pattern(4096, 3), &[])
                .await
                .status
                .is_success()
        );
        let multi = format!("{prefix}multi.bin");
        let (said, _) = shaped_upload(&live, &client, &multi, &[5, 5]).await;
        for (label, source) in [("single-part source", &single), ("multipart source", &multi)] {
            let head = ops::head_object(client.profile(), &live.bucket, source).expect("builds");
            let source_etag = live.send(&client, head).await.header("etag").map(str::to_string);
            let to = format!("{source}.copy");
            let pin = source_etag.clone().unwrap_or_default();
            let answer = live
                .send(
                    &client,
                    copy_request(
                        &client,
                        (&live.bucket, source),
                        (&live.bucket, &to),
                        &[("x-amz-copy-source-if-match", pin.as_str())],
                    ),
                )
                .await;
            let answered = crate::xml::parse_copy_result(&answer.text())
                .ok()
                .and_then(|copied| copied.etag);
            let head = ops::head_object(client.profile(), &live.bucket, &to).expect("builds");
            let landed = live.send(&client, head).await.header("etag").map(str::to_string);
            let same = |etag: &Option<String>| match (etag, &source_etag) {
                (Some(one), Some(other)) => one.trim_matches('"') == other.trim_matches('"'),
                _ => false,
            };
            report(
                &live,
                &format!("CopyObject ETag, {label}"),
                format!(
                    "{}; source {source_etag:?}, answered {answered:?} (same: {}), HEAD after {landed:?} (same: {})",
                    verdict(&answer),
                    same(&answered),
                    same(&landed)
                ),
            );
        }
        report(&live, "CopyObject ETag, the multipart source's upload", said);
        // GCS refuses a pinned copy of a multipart source: unpinned, and with
        // the pin unquoted, to tell the pin from the copy.
        let head = ops::head_object(client.profile(), &live.bucket, &multi).expect("builds");
        let multi_etag = live
            .send(&client, head)
            .await
            .header("etag")
            .unwrap_or_default()
            .to_string();
        let unquoted = multi_etag.trim_matches('"').to_string();
        for (label, extra) in [
            ("unpinned", Vec::new()),
            (
                "pinned unquoted",
                vec![("x-amz-copy-source-if-match", unquoted.as_str())],
            ),
        ] {
            let to = format!("{multi}.{}", label.replace(' ', "-"));
            let answer = live
                .send(
                    &client,
                    copy_request(&client, (&live.bucket, &multi), (&live.bucket, &to), &extra),
                )
                .await;
            report(
                &live,
                &format!("CopyObject of a multipart source, {label}"),
                verdict(&answer),
            );
        }
        let to = format!("{single}.wrong-pin");
        let refused = live
            .send(
                &client,
                copy_request(
                    &client,
                    (&live.bucket, &single),
                    (&live.bucket, &to),
                    &[("x-amz-copy-source-if-match", "\"0123456789abcdef0123456789abcdef\"")],
                ),
            )
            .await;
        let landed = live.length(&client, &to).await;
        report(
            &live,
            "CopyObject with a wrong source pin",
            format!("{}, a copy landed: {}", verdict(&refused), landed.is_some()),
        );
        assert!(
            landed.is_none() || !client.profile().enforces_copy_source_pin,
            "[{}] the profile trusts the copy-source pin, but a CopyObject ignored it",
            live.name
        );
        live.clean(&client, &prefix).await;
    }
}

/// A pinned, create-only `CopyObject` as the backend builds it
/// (`ops::copy_object` with `Overwrite::Refuse`): on GCS that's GCS's dialect,
/// the pin spelled `x-goog-copy-source-if-match`. The right single-part pin
/// lands, a wrong one is refused (`412`, which `server_copy.rs` reads as
/// `SourceChanged` once the source no longer matches), and a multipart ETag
/// lands where the profile sends it unpinned (GCS).
#[tokio::test(flavor = "multi_thread")]
async fn live_create_only_copy_with_a_source_pin() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("pinned-copy");
        let single = format!("{prefix}single.bin");
        let put = live.put(&client, &single, &pattern(4096, 5), &[]).await;
        assert!(put.status.is_success(), "[{}] the seed: {}", live.name, verdict(&put));
        let multi = format!("{prefix}multi.bin");
        let (said, _) = shaped_upload(&live, &client, &multi, &[5, 5]).await;
        report(&live, "pinned create-only copy, the multipart source's upload", said);
        let etag_of = |key: String| {
            let client = &client;
            let live = &live;
            async move {
                let head = ops::head_object(client.profile(), &live.bucket, &key).expect("builds");
                live.send(client, head)
                    .await
                    .header("etag")
                    .unwrap_or_default()
                    .to_string()
            }
        };
        let single_etag = etag_of(single.clone()).await;
        let multi_etag = etag_of(multi.clone()).await;
        let wrong = "\"0123456789abcdef0123456789abcdef\"".to_string();
        let mut outcomes = Vec::new();
        for (label, source, pin) in [
            ("single-part, the right pin", &single, &single_etag),
            ("single-part, a wrong pin", &single, &wrong),
            ("multipart ETag", &multi, &multi_etag),
        ] {
            let to = format!("{source}.{}", label.replace([' ', ','], "-"));
            let built = ops::copy_object(
                client.profile(),
                ops::CopySource {
                    bucket: &live.bucket,
                    key: source,
                },
                Some(pin),
                &live.bucket,
                &to,
                ops::Overwrite::Refuse,
                &ops::MetadataDirective::Copy,
            )
            .expect("builds");
            let answer = live.send(&client, built.request).await;
            let landed = live.length(&client, &to).await.is_some();
            report(
                &live,
                &format!("pinned create-only copy, {label}"),
                format!("{}, landed: {landed}", verdict(&answer)),
            );
            outcomes.push((label, answer.status.as_u16(), landed));
        }
        assert!(
            outcomes[0].2 && outcomes[2].2,
            "[{}] a pinned create-only copy that should land didn't: {outcomes:?}",
            live.name
        );
        assert!(
            outcomes[1].1 == 412 || !client.profile().enforces_copy_source_pin,
            "[{}] the profile trusts the pin, but a wrong one wasn't refused: {outcomes:?}",
            live.name
        );
        live.clean(&client, &prefix).await;
    }
}

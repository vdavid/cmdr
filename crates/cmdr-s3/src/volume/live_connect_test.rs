//! How each real provider answers a connect, the way the app makes one: the
//! refusals a user can cause (a wrong secret or key id, a missing bucket, a
//! bucket in another region, a key scoped to another bucket), and on AWS, an
//! account root reaching a bucket in another region with no error at all.
//!
//! Skips without `CMDR_S3_LIVE=1` (`live_support.rs`); the runner is
//! `apps/desktop/test/s3-servers/live.sh all live_connect`. Findings:
//! `docs/notes/s3/live-verification-2026-10.md`.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeEventSink};
use cmdr_fs::volume::{ServerCopyProgress, ShareLinkExpiry, Volume, VolumeError, WriteMode};
use tokio_util::sync::CancellationToken;

use super::live_support::*;
use super::testing::{BytesSource, read_back};
use super::{S3Volume, connect_s3_volume};
use crate::S3ConnectError;
use crate::params::{S3ConnectionParams, S3Provider};

/// A bucket name nobody owns.
const MISSING: &str = "cmdr-live-no-such-bucket-4d1e";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// `live`'s secret, read where the runner put it.
fn secret_of(live: &Live) -> String {
    env(&format!("CMDR_S3_LIVE_{}_SECRET", live.name.to_uppercase())).expect("a live target has its secret")
}

/// Connects to one place with any keys, the way the app does.
async fn dial(
    provider: &S3Provider,
    key_id: &str,
    secret: &str,
    bucket: Option<&str>,
) -> Result<S3Volume, S3ConnectError> {
    let params =
        S3ConnectionParams::new(provider.clone(), key_id, bucket).map_err(|_| S3ConnectError::InvalidProvider)?;
    let credentials = InMemoryCredentials::new().with_entry(&params.credential_service(), Some(key_id), key_id, secret);
    let host = VolumeHost::builder()
        .credentials(Arc::new(credentials))
        .events(Arc::new(RecordingVolumeEvents::new()) as Arc<dyn VolumeEventSink>)
        .build();
    let volume_id = cmdr_fs::volume::s3_volume_id(params.host(), params.port(), key_id, bucket);
    connect_s3_volume("live-connect", &volume_id, params, host, CancellationToken::new()).await
}

/// A connect's outcome as a finding: `connected`, or the refusal.
fn said(outcome: &Result<S3Volume, S3ConnectError>) -> String {
    match outcome {
        Ok(_) => "connected".to_string(),
        Err(e) => format!("{e:?}"),
    }
}

/// `key_id` with its last character changed, so it's well-formed but no key.
fn wrong_key_id(key_id: &str) -> String {
    let mut chars: Vec<char> = key_id.chars().collect();
    if let Some(last) = chars.last_mut() {
        *last = if *last == 'Q' { 'R' } else { 'Q' };
    }
    chars.into_iter().collect()
}

/// The same provider in another region, where the bucket doesn't live.
fn elsewhere(provider: &S3Provider) -> Option<S3Provider> {
    Some(match provider {
        S3Provider::Aws { .. } => S3Provider::Aws {
            region: "us-east-1".to_string(),
        },
        S3Provider::Wasabi { .. } => S3Provider::Wasabi {
            region: "us-east-1".to_string(),
        },
        S3Provider::B2 { .. } => S3Provider::B2 {
            region: "us-west-004".to_string(),
        },
        S3Provider::Hetzner { .. } => S3Provider::Hetzner {
            location: "fsn1".to_string(),
        },
        S3Provider::DigitalOcean { .. } => S3Provider::DigitalOcean {
            region: "nyc3".to_string(),
        },
        _ => return None,
    })
}

/// ❗ Every refusal a user can cause at connect, per provider, and what each
/// maps to. The keys' own refusals must be `KeysRejected` everywhere; the rest
/// are recorded as found.
#[tokio::test(flavor = "multi_thread")]
async fn live_connect_refusals() {
    for live in live_targets() {
        let secret = secret_of(&live);
        let bucket = Some(live.bucket.as_str());
        let wrong_secret = "0000000000000000000000000000000000000000";
        let mut misses = Vec::new();

        for (what, place) in [("the bucket", bucket), ("the account root", None)] {
            let outcome = dial(&live.provider, &live.key_id, wrong_secret, place).await;
            report(&live, &format!("connect to {what} with a wrong secret"), said(&outcome));
            // R2 answers `ListBuckets` from a bucket-scoped key with
            // `AccessDenied` whatever the secret, and a HEAD has no body, so
            // there a wrong secret reads as the ambiguous refusal.
            let r2_ambiguity = matches!(live.provider, S3Provider::R2 { .. })
                && matches!(
                    outcome,
                    Err(S3ConnectError::AccessDenied | S3ConnectError::BucketListRefused)
                );
            if !matches!(outcome, Err(S3ConnectError::KeysRejected)) && !r2_ambiguity {
                misses.push(format!("a wrong secret on {what}: {}", said(&outcome)));
            }
            let outcome = dial(&live.provider, &wrong_key_id(&live.key_id), &secret, place).await;
            report(&live, &format!("connect to {what} with a wrong key id"), said(&outcome));
            if !matches!(outcome, Err(S3ConnectError::KeysRejected)) {
                misses.push(format!("a wrong key id on {what}: {}", said(&outcome)));
            }
        }

        let outcome = dial(&live.provider, &live.key_id, &secret, Some(MISSING)).await;
        report(&live, "connect to a missing bucket", said(&outcome));

        if let Some(other) = elsewhere(&live.provider) {
            for (what, place) in [("the bucket", bucket), ("the account root", None)] {
                let outcome = dial(&other, &live.key_id, &secret, place).await;
                report(
                    &live,
                    &format!("connect to {what} through another region's endpoint ({other:?})"),
                    said(&outcome),
                );
            }
        }

        // A key that reaches the second bucket only.
        if let (Some(scoped_id), Some(scoped_secret), Some(its_bucket)) = (
            env(&format!("CMDR_S3_LIVE_{}_SCOPED_KEY_ID", live.name.to_uppercase())),
            env(&format!("CMDR_S3_LIVE_{}_SCOPED_SECRET", live.name.to_uppercase())),
            live.bucket_2.as_deref(),
        ) {
            for (what, place) in [
                ("its own bucket", Some(its_bucket)),
                ("another bucket", bucket),
                ("the account root", None),
            ] {
                let outcome = dial(&live.provider, &scoped_id, &scoped_secret, place).await;
                report(
                    &live,
                    &format!("connect to {what} with a key scoped to one bucket"),
                    said(&outcome),
                );
            }
        }
        assert!(misses.is_empty(), "[{}] {misses:#?}", live.name);
    }
}

/// Off AWS nothing routes per bucket, so what does an account root reached
/// through another region's endpoint make of a bucket that lives elsewhere?
/// Recorded, not asserted: it decides whether those presets need routing.
#[tokio::test(flavor = "multi_thread")]
async fn live_connect_account_root_through_another_region() {
    for live in live_targets()
        .into_iter()
        .filter(|live| !matches!(live.provider, S3Provider::Aws { .. }))
    {
        let Some(other) = elsewhere(&live.provider) else {
            continue;
        };
        let secret = secret_of(&live);
        let root = match dial(&other, &live.key_id, &secret, None).await {
            Ok(root) => root,
            Err(e) => {
                report(&live, &format!("account root through {other:?}"), format!("{e:?}"));
                continue;
            }
        };
        let names = root
            .list_directory(root.root(), None)
            .await
            .map(|entries| entries.into_iter().map(|e| e.name).collect::<Vec<_>>());
        let bucket = root.root().join(&live.bucket);
        let listed = root.list_directory(&bucket, None).await.map(|e| e.len());
        let path = bucket.join(format!("{}elsewhere.txt", live_prefix("routing")));
        let written = put(&root, &path, b"elsewhere").await;
        let back = if written.is_ok() {
            String::from_utf8_lossy(&read_back(&root, &path).await).into_owned()
        } else {
            String::new()
        };
        let deleted = root.delete(&path).await;
        report(
            &live,
            &format!("account root through {other:?}"),
            format!(
                "lists {names:?}; the bucket lists {listed:?}; write {written:?}, reads back {back:?}, delete {deleted:?}"
            ),
        );
    }
}

/// ❗ The crate negotiates what the app ships (`Cargo.toml` declares reqwest's
/// `http2`): HTTP/2 wherever a provider offers it, so this suite meets what
/// the app's requests meet. Hetzner, GCS, and Spaces offer it; R2, AWS, B2,
/// and Wasabi answer over HTTP/1.1 only (verified with this cell and
/// `curl --http2`, 2026-10-02). A change either way fails here, on purpose.
#[tokio::test(flavor = "multi_thread")]
async fn live_connect_speaks_http2_where_offered() {
    for live in live_targets() {
        let client = live.client();
        let request = crate::ops::head_bucket(client.profile(), &live.bucket).expect("builds");
        let answer = live.send(&client, request).await;
        let version = client.negotiated();
        report(
            &live,
            "HTTP version negotiated",
            format!("{version:?} (HeadBucket {})", answer.status.as_u16()),
        );
        let offers_http2 = matches!(
            live.provider,
            S3Provider::Hetzner { .. } | S3Provider::Gcs | S3Provider::DigitalOcean { .. }
        );
        let expected = if offers_http2 {
            http::Version::HTTP_2
        } else {
            http::Version::HTTP_11
        };
        assert_eq!(version, Some(expected), "[{}]", live.name);
    }
}

/// A copy's progress hook that never pauses or cancels.
struct Silent;

impl ServerCopyProgress for Silent {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

async fn put(volume: &S3Volume, path: &Path, bytes: &[u8]) -> Result<u64, VolumeError> {
    let source = BytesSource::new(bytes.to_vec());
    let length = cmdr_fs::volume::VolumeReadStream::total_size(&source);
    volume
        .write_from_stream(path, WriteMode::CreateNew, length, Box::new(source), &|_| {
            ControlFlow::Continue(())
        })
        .await
}

/// ❗ An account root on the routing allowlist (AWS, Wasabi) reaches a bucket
/// in another region than its endpoint's with no error: listed upfront,
/// learned from a redirect, and asked before an upload. A bucket place there
/// is refused with the region to use instead. Runs for each provider whose
/// `CMDR_S3_LIVE_<NAME>_FAR_BUCKET` (and `_FAR_REGION`) is set.
#[tokio::test(flavor = "multi_thread")]
async fn live_connect_routes_each_bucket_to_its_region() {
    for live in live_targets() {
        let upper = live.name.to_uppercase();
        let Some(far) = env(&format!("CMDR_S3_LIVE_{upper}_FAR_BUCKET")) else {
            continue;
        };
        let far_region = env(&format!("CMDR_S3_LIVE_{upper}_FAR_REGION")).expect("the far bucket's region");
        let secret = secret_of(&live);
        let mut misses = Vec::new();
        let root = dial(&live.provider, &live.key_id, &secret, None)
            .await
            .unwrap_or_else(|e| panic!("[{}] the account root didn't connect: {e:?}", live.name));
        let in_far = |volume: &S3Volume, key: &str| -> PathBuf { volume.root().join(&far).join(key) };
        let key = format!("{}hello.txt", live_prefix("routing"));

        // Listed upfront: `ListBuckets` names each bucket's region.
        let names = root
            .list_directory(root.root(), None)
            .await
            .map(|entries| entries.into_iter().map(|e| e.name).collect::<Vec<_>>());
        report(&live, "account root lists", format!("{names:?}"));
        let all_three = names.as_ref().is_ok_and(|names| {
            [Some(&live.bucket), live.bucket_2.as_ref(), Some(&far)]
                .into_iter()
                .flatten()
                .all(|b| names.contains(b))
        });
        if !all_three {
            misses.push(format!("the root listing: {names:?}"));
        }
        let listed = root.list_directory(&root.root().join(&far), None).await;
        report(
            &live,
            &format!("list the {far_region} bucket"),
            format!("{:?}", listed.as_ref().map(Vec::len)),
        );
        if listed.is_err() {
            misses.push(format!("listing the far bucket: {listed:?}"));
        }
        let written = put(&root, &in_far(&root, &key), b"from far away").await;
        let back = read_back(&root, &in_far(&root, &key)).await;
        report(
            &live,
            &format!("write and read back in the {far_region} bucket"),
            format!("{written:?}, intact: {}", back == b"from far away"),
        );
        if written.is_err() || back != b"from far away" {
            misses.push(format!("a write to the far bucket: {written:?}"));
        }

        // Learned from a redirect: a fresh root that never listed.
        let fresh = dial(&live.provider, &live.key_id, &secret, None)
            .await
            .expect("connects");
        let stat = fresh.get_metadata(&in_far(&fresh, &key)).await;
        let back = read_back(&fresh, &in_far(&fresh, &key)).await;
        report(
            &live,
            "a fresh root stats and reads the far object first thing",
            format!(
                "size {:?}, intact: {}",
                stat.as_ref().map(|s| s.size),
                back == b"from far away"
            ),
        );
        if stat.is_err() || back != b"from far away" {
            misses.push(format!("a cold stat of the far object: {stat:?}"));
        }

        // Asked before an upload: a fresh root whose first request is a write.
        let cold = dial(&live.provider, &live.key_id, &secret, None)
            .await
            .expect("connects");
        let cold_key = format!("{}cold.txt", live_prefix("routing"));
        let written = put(&cold, &in_far(&cold, &cold_key), b"cold write").await;
        report(
            &live,
            "a fresh root's first request is a write to the far bucket",
            format!("{written:?}"),
        );
        if written.is_err() {
            misses.push(format!("a cold write to the far bucket: {written:?}"));
        }

        // A share link signed for the far region fetches unsigned.
        let link = root.share_link(&in_far(&root, &key), ShareLinkExpiry::OneHour).await;
        let fetched = match link {
            Ok(link) => match cmdr_http::client_builder()
                .build()
                .expect("a plain client builds")
                .get(link.into_url())
                .send()
                .await
            {
                Ok(answer) => format!("{}", answer.status()),
                Err(e) => format!("didn't fetch: {}", e.is_connect()),
            },
            Err(e) => format!("didn't mint: {e:?}"),
        };
        report(&live, "a share link to the far object, fetched unsigned", &fetched);
        if !fetched.starts_with("200") {
            misses.push(format!("the far share link: {fetched}"));
        }

        // A server-side copy from the home region into the far one.
        let source = root
            .root()
            .join(&live.bucket)
            .join(format!("{}source.txt", live_prefix("routing")));
        let copied_to = in_far(&root, &format!("{}copied.txt", live_prefix("routing")));
        let seeded = put(&root, &source, b"crossing regions").await;
        let copied = root
            .copy_on_server(&root, &source, &copied_to, WriteMode::CreateNew, &Silent)
            .await;
        // ❗ A copy that says it landed must have: an `Ok` with nothing there
        // would let a move delete its source.
        let back = match &copied {
            Ok(_) => read_back(&root, &copied_to).await,
            Err(_) => Vec::new(),
        };
        if copied.is_ok() && back != b"crossing regions" {
            misses.push(format!("the cross-region copy said {copied:?} but didn't land"));
        }
        report(
            &live,
            &format!("server-side copy {} to {far_region}", live.bucket),
            format!(
                "seed {seeded:?}, copy {copied:?}, intact: {}",
                back == b"crossing regions"
            ),
        );

        // A bucket place in another region is refused, naming the region.
        let place = dial(&live.provider, &live.key_id, &secret, Some(&far)).await;
        report(
            &live,
            &format!("connect to the {far_region} bucket as a place"),
            said(&place),
        );
        if !matches!(&place, Err(S3ConnectError::WrongRegion { region: Some(r) }) if *r == far_region) {
            misses.push(format!("the far bucket place: {}", said(&place)));
        }

        for path in [in_far(&root, &key), in_far(&root, &cold_key), copied_to, source] {
            let _ = root.delete(&path).await;
        }
        assert!(misses.is_empty(), "[{}] {misses:#?}", live.name);
    }
}

//! The live-provider harness: which real accounts this run reaches, and the
//! raw request helpers the live cells probe them with.
//!
//! ❗ Every live cell skips cleanly unless `CMDR_S3_LIVE=1` and that provider's
//! variables are set, so CI and `pnpm check` never need an account. The runner
//! that exports them from the secret store:
//! `apps/desktop/test/s3-servers/live.sh`. ❌ Never print a secret: the
//! variables reach the client and nothing else.
//!
//! Objects live under `cmdr-live/<run>/` and each cell deletes its own;
//! `live_cleanup_removes_every_leftover` sweeps whatever a crashed run left.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeEventSink};
use futures_util::StreamExt as _;
use http::{HeaderName, HeaderValue, Method};
use tokio_util::sync::CancellationToken;

use super::{S3Volume, connect_s3_volume};
use crate::encoding::encode_key;
use crate::error::S3Error;
use crate::ops::{self, ListObjectsParams, ObjectMetadata, Overwrite};
use crate::params::{S3ConnectionParams, S3Provider};
use crate::request::{Body, S3Request};
use crate::sigv4::Credentials;
use crate::transport::{Answer, QUERY_BUDGET, S3Client, UploadBody};
use crate::xml::build::CompletedPart;
use crate::xml::{parse_initiate_multipart, parse_list_multipart_uploads, parse_list_objects};

pub(super) const MIB: usize = 1024 * 1024;

/// The accounts this run reaches and the prefix every live object sits
/// under, one list with the app crate's engine flows (`testing::live`).
pub(super) use super::testing::live::{LIVE_ROOT, LiveAccount as Live, live_accounts as live_targets, live_prefix};

/// Prints one finding, the line the docs are written from.
#[allow(
    clippy::print_stdout,
    reason = "the findings ARE this harness's output, read with `--nocapture`"
)]
pub(super) fn report(live: &Live, what: &str, finding: impl std::fmt::Display) {
    println!("LIVE [{}] {what}: {finding}", live.name);
}

/// An answer as a finding: `200`, or `412 PreconditionFailed`.
pub(super) fn verdict(answer: &Answer) -> String {
    if answer.status.is_success() {
        let text = answer.text();
        // A copy or a completion can fail inside a 200.
        if text.contains("<Error>") {
            let error = S3Error::from_response(http::StatusCode::INTERNAL_SERVER_ERROR, &text);
            return format!("{} with an embedded {:?}", answer.status.as_u16(), error.code);
        }
        return answer.status.as_u16().to_string();
    }
    let error = S3Error::from_response(answer.status, &answer.text());
    format!("{} {:?}", answer.status.as_u16(), error.code)
}

impl Live {
    /// The same account reached through the "Other" preset, which is off
    /// every allowlist: what a user who picks "Other" for a known provider
    /// gets. Hetzner only, whose endpoint is a plain host.
    pub(super) fn as_other(&self) -> Option<Self> {
        let S3Provider::Hetzner { location } = &self.provider else {
            return None;
        };
        let endpoint = url::Url::parse(&format!("https://{location}.your-objectstorage.com")).ok()?;
        Some(Self {
            name: "hetzner-as-other",
            provider: S3Provider::Other {
                endpoint,
                region: Some(location.clone()),
                path_style: true,
            },
            key_id: self.key_id.clone(),
            secret: self.secret.clone(),
            bucket: self.bucket.clone(),
            bucket_2: None,
        })
    }

    pub(super) fn params(&self, bucket: Option<&str>) -> S3ConnectionParams {
        S3ConnectionParams::new(self.provider.clone(), &self.key_id, bucket).expect("a live provider is valid")
    }

    /// A signed client with the right secret, or with `secret` instead.
    pub(super) fn client_with(&self, secret: &str) -> S3Client {
        let profile = self.params(None).profile().expect("a live profile builds");
        S3Client::new(profile, Credentials::new(&self.key_id, secret.to_string())).expect("a live client builds")
    }

    pub(super) fn client(&self) -> S3Client {
        self.client_with(&self.secret)
    }

    fn host(&self, state_root: Option<&std::path::Path>) -> VolumeHost {
        let credentials = InMemoryCredentials::new().with_entry(
            &self.params(None).credential_service(),
            Some(&self.key_id),
            &self.key_id,
            &self.secret,
        );
        let builder = VolumeHost::builder()
            .credentials(Arc::new(credentials))
            .events(Arc::new(RecordingVolumeEvents::new()) as Arc<dyn VolumeEventSink>);
        match state_root {
            Some(dir) => builder.state_root(dir).build(),
            None => builder.build(),
        }
    }

    /// Connects to one place the way the app does.
    pub(super) async fn connect(&self, bucket: Option<&str>) -> Result<S3Volume, crate::S3ConnectError> {
        self.connect_to(bucket, None).await
    }

    /// [`Self::connect`] with the backend's durable state (the unfinished-
    /// upload record) under `dir`, for the crash-recovery cells.
    pub(super) async fn connect_with_state(
        &self,
        bucket: Option<&str>,
        dir: &std::path::Path,
    ) -> Result<S3Volume, crate::S3ConnectError> {
        self.connect_to(bucket, Some(dir)).await
    }

    async fn connect_to(
        &self,
        bucket: Option<&str>,
        state_root: Option<&std::path::Path>,
    ) -> Result<S3Volume, crate::S3ConnectError> {
        let params = self.params(bucket);
        let volume_id = cmdr_fs::volume::s3_volume_id(params.host(), params.port(), &self.key_id, bucket);
        connect_s3_volume(
            self.name,
            &volume_id,
            params,
            self.host(state_root),
            CancellationToken::new(),
        )
        .await
    }

    /// Sends `request` (held in memory) and answers what came back.
    pub(super) async fn send(&self, client: &S3Client, request: S3Request) -> Answer {
        client
            .exchange(request, QUERY_BUDGET)
            .await
            .unwrap_or_else(|e| panic!("[{}] a live request didn't go out: {e}", self.name))
    }

    /// PUTs `bytes` at `key` with `extra` headers, overwriting.
    pub(super) async fn put(&self, client: &S3Client, key: &str, bytes: &[u8], extra: &[(&str, &str)]) -> Answer {
        let built = ops::put_object(
            client.profile(),
            &self.bucket,
            key,
            bytes.len() as u64,
            Overwrite::Replace,
            &ObjectMetadata::default(),
        )
        .expect("a live key builds");
        let mut request = with_headers(built.request, extra);
        request.body = Body::Bytes(bytes.to_vec());
        self.send(client, request).await
    }

    pub(super) async fn head(&self, client: &S3Client, key: &str) -> Answer {
        let request = ops::head_object(client.profile(), &self.bucket, key).expect("a live key builds");
        self.send(client, request).await
    }

    /// The object's length, or `None` when it's gone.
    pub(super) async fn length(&self, client: &S3Client, key: &str) -> Option<u64> {
        let answer = self.head(client, key).await;
        answer
            .status
            .is_success()
            .then(|| answer.header("content-length").and_then(|len| len.parse().ok()))
            .flatten()
    }

    /// Starts a multipart upload of `key` and answers its id.
    pub(super) async fn create_upload(&self, client: &S3Client, key: &str) -> String {
        let request = ops::create_multipart_upload(client.profile(), &self.bucket, key, &ObjectMetadata::default())
            .expect("a live key builds");
        let answer = self.send(client, request).await;
        assert!(
            answer.status.is_success(),
            "[{}] create upload: {}",
            self.name,
            verdict(&answer)
        );
        parse_initiate_multipart(&answer.text())
            .expect("an upload id")
            .upload_id
    }

    /// Uploads one part of `bytes` and answers its ETag, or the failure.
    pub(super) async fn upload_part(
        &self,
        client: &S3Client,
        key: &str,
        upload_id: &str,
        number: u32,
        bytes: Vec<u8>,
    ) -> Result<CompletedPart, String> {
        let request = ops::upload_part(
            client.profile(),
            &self.bucket,
            key,
            upload_id,
            number,
            bytes.len() as u64,
        )
        .expect("a live key builds");
        let body: UploadBody = Box::pin(futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from(
            bytes,
        ))]));
        let answer = client.upload(request, body).await.map_err(|e| e.to_string())?;
        match answer.header("etag") {
            Some(etag) if answer.status.is_success() => Ok(CompletedPart {
                number,
                etag: etag.to_string(),
            }),
            _ => Err(verdict(&answer)),
        }
    }

    /// Completes an upload with `extra` headers.
    pub(super) async fn complete(
        &self,
        client: &S3Client,
        key: &str,
        upload_id: &str,
        parts: &[CompletedPart],
        extra: &[(&str, &str)],
    ) -> Answer {
        let built = ops::complete_multipart_upload(
            client.profile(),
            &self.bucket,
            key,
            upload_id,
            parts,
            Overwrite::Replace,
        )
        .expect("a live key builds");
        let request = with_headers(built.request, extra);
        client
            .exchange(request, crate::transport::COMPLETE_BUDGET)
            .await
            .unwrap_or_else(|e| panic!("[{}] complete didn't go out: {e}", self.name))
    }

    pub(super) async fn abort(&self, client: &S3Client, key: &str, upload_id: &str) -> Answer {
        let request =
            ops::abort_multipart_upload(client.profile(), &self.bucket, key, upload_id).expect("a live key builds");
        self.send(client, request).await
    }

    /// Every unfinished upload under `prefix`, as `(key, upload id)`.
    pub(super) async fn uploads_under(&self, client: &S3Client, prefix: &str) -> Result<Vec<(String, String)>, String> {
        let request = ops::list_multipart_uploads(client.profile(), &self.bucket, prefix, None).expect("builds");
        let answer = self.send(client, request).await;
        if !answer.status.is_success() {
            return Err(verdict(&answer));
        }
        let page = parse_list_multipart_uploads(&answer.text()).map_err(|e| format!("{e:?}"))?;
        Ok(page.uploads.into_iter().map(|u| (u.key, u.upload_id)).collect())
    }

    /// Every key under `prefix`, recursively.
    pub(super) async fn keys_under(&self, client: &S3Client, prefix: &str) -> Vec<String> {
        let mut keys = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let params = ListObjectsParams {
                prefix,
                delimiter: None,
                continuation_token: token.as_deref(),
                max_keys: None,
            };
            let request = ops::list_objects(client.profile(), &self.bucket, &params).expect("builds");
            let answer = self.send(client, request).await;
            assert!(answer.status.is_success(), "[{}] list: {}", self.name, verdict(&answer));
            let page = parse_list_objects(&answer.text()).expect("a listing");
            keys.extend(page.objects.into_iter().map(|o| o.key));
            match page.next_continuation_token {
                Some(next) if page.is_truncated => token = Some(next),
                _ => return keys,
            }
        }
    }

    /// Deletes `keys` one request each (works on every provider, batch
    /// support or not), ignoring what's already gone.
    pub(super) async fn delete_each(&self, client: &S3Client, keys: &[String]) {
        let deletes = keys.iter().map(|key| async move {
            let request = ops::delete_object(client.profile(), &self.bucket, key).expect("builds");
            let _ = client.exchange(request, QUERY_BUDGET).await;
        });
        futures_util::stream::iter(deletes)
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await;
    }

    /// Removes everything under `prefix`, uploads included.
    pub(super) async fn clean(&self, client: &S3Client, prefix: &str) {
        if let Ok(uploads) = self.uploads_under(client, prefix).await {
            for (key, id) in uploads {
                self.abort(client, &key, &id).await;
            }
        }
        let keys = self.keys_under(client, prefix).await;
        self.delete_each(client, &keys).await;
    }

    /// A raw request for `key` exactly as given: no NFC composition, for the
    /// cell that asks what a provider does with a decomposed key.
    pub(super) fn raw_object(&self, client: &S3Client, method: Method, key: &str) -> S3Request {
        let profile = client.profile();
        let mut request = S3Request::new(
            method,
            &profile.scheme,
            &profile.endpoint_host,
            format!("/{}/{}", self.bucket, encode_key(key).expect("a live key encodes")),
        );
        request.bucket = Some(self.bucket.clone());
        request
    }
}

/// `request` with `extra` headers added.
pub(super) fn with_headers(mut request: S3Request, extra: &[(&str, &str)]) -> S3Request {
    for (name, value) in extra {
        request = request.header(
            HeaderName::from_bytes(name.as_bytes()).expect("a header name"),
            HeaderValue::from_str(value).expect("a header value"),
        );
    }
    request
}

/// `len` bytes of a repeating pattern tagged with `tag`.
pub(super) fn pattern(len: usize, tag: u8) -> Vec<u8> {
    (0..len).map(|i| tag.wrapping_add((i % 251) as u8)).collect()
}

/// Seconds elapsed since `start`, two decimals.
pub(super) fn seconds(start: std::time::Instant) -> String {
    format!("{:.2} s", start.elapsed().as_secs_f64())
}

/// A pause between two writes to one key: R2 allows one write a second per key.
pub(super) async fn breathe() {
    tokio::time::sleep(Duration::from_millis(1_100)).await;
}

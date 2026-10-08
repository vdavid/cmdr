//! One S3 server a suite runs against: a Docker fixture or a real account.
//!
//! A scenario written against [`S3Target`] runs unchanged on both, which is
//! what lets the app's engine suites (`backend_suites/s3_*`) check the same
//! claims on VersityGW and Garage in the lane and on R2, GCS, AWS, and the rest
//! by hand (`apps/desktop/test/s3-servers/live-engine.sh`).
//!
//! ❗ The two differ in cleanup. A fixture's objects persist across runs until
//! the fixture itself expires them, and nothing relies on a clean bucket
//! (`scratch_prefix`); a live account bills
//! every byte kept, so whoever drives a live scenario calls
//! [`S3Target::clean_run`] after it, which removes everything this run wrote.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeEventSink};
use futures_util::StreamExt as _;
use http::{HeaderName, HeaderValue};
use tokio_util::sync::CancellationToken;

use super::live::{LiveAccount, live_accounts, live_run_root};
use super::{
    FIXTURE_ACCESS_KEY, FIXTURE_BUCKET, FIXTURE_BUCKET_2, FixtureService, GARAGE, VERSITYGW, connect_fixture,
    fixture_params, fixture_secret, scratch_prefix,
};
use crate::S3Volume;
use crate::ops::{self, ListObjectsParams, ObjectMetadata, Overwrite};
use crate::params::{S3ConnectionParams, S3Provider};
use crate::request::Body;
use crate::sigv4::Credentials;
use crate::transport::{QUERY_BUDGET, S3Client};
use crate::volume::connect_s3_volume;

/// `error` and every source under it, for a panic that has to say why a
/// request didn't go out (reqwest's own message stops at "error sending
/// request").
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// A Docker fixture or a real account.
#[derive(Clone)]
pub enum S3Target {
    /// A Docker fixture server (`apps/desktop/test/s3-servers/`).
    Fixture(FixtureService),
    /// A real account (`testing::live`).
    Live(Arc<LiveAccount>),
}

/// What to put at one key.
pub struct Seed<'a> {
    /// The full key, prefix included.
    pub key: &'a str,
    /// The object's bytes. Empty for a folder marker (`a/`).
    pub bytes: &'a [u8],
    /// The `x-amz-meta-mtime` to write, rclone's format.
    pub mtime: Option<SystemTime>,
}

impl S3Target {
    /// Every real account this run reaches; empty unless `CMDR_S3_LIVE=1`
    /// (`testing::live`).
    pub fn live_all() -> Vec<Self> {
        live_accounts()
            .into_iter()
            .map(|account| Self::Live(Arc::new(account)))
            .collect()
    }

    /// What findings are printed under: `versitygw`, `garage`, `r2`, `aws`, ….
    pub fn name(&self) -> &str {
        match self {
            Self::Fixture(service) if *service == GARAGE => "garage",
            Self::Fixture(service) if *service == VERSITYGW => "versitygw",
            Self::Fixture(service) => service.key,
            Self::Live(account) => account.name,
        }
    }

    /// Whether this is a real account, which bills and must be cleaned.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }

    /// The provider preset the volume is built with: "Other" for a fixture.
    pub fn provider(&self) -> S3Provider {
        self.params(None).provider().clone()
    }

    /// The bucket every cell works in, under a prefix of its own.
    pub fn bucket(&self) -> &str {
        match self {
            Self::Fixture(_) => FIXTURE_BUCKET,
            Self::Live(account) => &account.bucket,
        }
    }

    /// A second bucket the same key reaches, for a cross-bucket cell; a live
    /// account may have none.
    pub fn bucket_2(&self) -> Option<&str> {
        match self {
            Self::Fixture(_) => Some(FIXTURE_BUCKET_2),
            Self::Live(account) => account.bucket_2.as_deref(),
        }
    }

    /// A key prefix no other cell or run uses. On a live account it sits under
    /// this run's root, which [`Self::clean_run`] removes.
    pub fn prefix(&self, label: &str) -> String {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        match self {
            Self::Fixture(_) => scratch_prefix(label),
            Self::Live(_) => format!("{}{}-{label}/", live_run_root(), NEXT.fetch_add(1, Ordering::Relaxed)),
        }
    }

    /// The params for one place on the target: a bucket, or the account root.
    pub fn params(&self, bucket: Option<&str>) -> S3ConnectionParams {
        match self {
            Self::Fixture(service) => fixture_params(*service, bucket),
            Self::Live(account) => S3ConnectionParams::new(account.provider.clone(), &account.key_id, bucket)
                .expect("a live provider is valid"),
        }
    }

    /// Connects to one place the way the app does, panicking with what to
    /// check when it can't.
    pub async fn connect(&self, bucket: Option<&str>) -> S3Volume {
        let account = match self {
            Self::Fixture(service) => return connect_fixture(*service, bucket).await,
            Self::Live(account) => account,
        };
        let params = self.params(bucket);
        let volume_id = cmdr_fs::volume::s3_volume_id(params.host(), params.port(), &account.key_id, bucket);
        let credentials = InMemoryCredentials::new().with_entry(
            &params.credential_service(),
            Some(&account.key_id),
            &account.key_id,
            &account.secret,
        );
        let host = VolumeHost::builder()
            .credentials(Arc::new(credentials))
            .events(Arc::new(RecordingVolumeEvents::new()) as Arc<dyn VolumeEventSink>)
            .build();
        connect_s3_volume(account.name, &volume_id, params, host, CancellationToken::new())
            .await
            .unwrap_or_else(|e| panic!("[{}] the live account refused a connection: {e:?}", account.name))
    }

    /// A signed client straight to the protocol, for seeding and probing
    /// without the volume under test.
    pub(crate) fn client(&self) -> S3Client {
        let profile = self.params(None).profile().expect("a target's profile builds");
        let credentials = match self {
            Self::Fixture(_) => Credentials::new(FIXTURE_ACCESS_KEY, fixture_secret()),
            Self::Live(account) => Credentials::new(&account.key_id, account.secret.clone()),
        };
        S3Client::new(profile, credentials).expect("a target's client builds")
    }

    /// Puts objects in `bucket`, panicking on the first that doesn't land: up
    /// to 32 at a time on a fixture, eight on a live account (Hetzner answers
    /// 32 with `SlowDown`), where a throttled or faulted PUT goes again after
    /// a pause.
    pub async fn seed(&self, bucket: &str, seeds: &[Seed<'_>]) {
        self.seed_with(bucket, seeds, &[]).await;
    }

    /// [`Self::seed`] with `extra` headers on every PUT, such as
    /// `x-amz-storage-class`.
    pub async fn seed_with(&self, bucket: &str, seeds: &[Seed<'_>], extra: &[(&str, &str)]) {
        let client = Arc::new(self.client());
        let width = if self.is_live() { 8 } else { 32 };
        for batch in seeds.chunks(width) {
            let mut puts = Vec::with_capacity(batch.len());
            for seed in batch {
                let metadata = ObjectMetadata {
                    mtime: seed.mtime,
                    write_token: None,
                    carried: Vec::new(),
                };
                let built = ops::put_object(
                    client.profile(),
                    bucket,
                    seed.key,
                    seed.bytes.len() as u64,
                    Overwrite::Replace,
                    &metadata,
                )
                .unwrap_or_else(|e| panic!("building a PUT for {:?}: {e:?}", seed.key));
                let mut request = built.request;
                for (name, value) in extra {
                    request = request.header(
                        HeaderName::from_bytes(name.as_bytes()).expect("a header name"),
                        HeaderValue::from_str(value).expect("a header value"),
                    );
                }
                request.body = Body::Bytes(seed.bytes.to_vec());
                let client = Arc::clone(&client);
                let key = seed.key.to_string();
                puts.push(tokio::spawn(async move {
                    let mut pause = Duration::from_millis(500);
                    let answer = loop {
                        let answer = client
                            .exchange(request.clone(), QUERY_BUDGET)
                            .await
                            .unwrap_or_else(|e| panic!("seeding {key:?}: {}", chain(&e)));
                        // A throttle, or a transient fault: B2 answered one PUT
                        // of a 1,005-object seeding with `500 InternalError`.
                        let transient = !answer.status.is_success()
                            && crate::error::S3Error::from_response(answer.status, &answer.text()).is_retryable();
                        if !transient || pause > Duration::from_secs(8) {
                            break answer;
                        }
                        tokio::time::sleep(pause).await;
                        pause *= 2;
                    };
                    assert!(
                        answer.status.is_success(),
                        "seeding {key:?} answered {}: {}",
                        answer.status,
                        answer.text()
                    );
                }));
            }
            for put in puts {
                put.await.expect("a seeding task finished");
            }
        }
    }

    /// The value of `header` on the object at `key`, as stored; `None` when
    /// the header or the object is missing.
    pub async fn stored_header(&self, bucket: &str, key: &str, header: &str) -> Option<String> {
        let client = self.client();
        let request = ops::head_object(client.profile(), bucket, key).expect("a key builds");
        let answer = client
            .exchange(request, QUERY_BUDGET)
            .await
            .unwrap_or_else(|e| panic!("probing {key:?}: {}", chain(&e)));
        answer
            .status
            .is_success()
            .then(|| answer.header(header).map(str::to_string))
            .flatten()
    }

    /// The `x-amz-meta-mtime` an object carries, as stored.
    pub async fn stored_mtime_header(&self, bucket: &str, key: &str) -> Option<String> {
        self.stored_header(bucket, key, crate::metadata::MTIME_HEADER).await
    }

    /// The `x-amz-meta-cmdr-write` token an object carries: every write and
    /// server-side copy Cmdr makes stamps a fresh one.
    pub async fn stored_write_token(&self, bucket: &str, key: &str) -> Option<String> {
        self.stored_header(bucket, key, crate::metadata::WRITE_TOKEN_HEADER)
            .await
    }

    /// The ETag an object carries, as stored: a multipart upload's ends in
    /// `-<parts>`.
    pub async fn stored_etag(&self, bucket: &str, key: &str) -> Option<String> {
        self.stored_header(bucket, key, "etag").await
    }

    /// Every unfinished upload under `prefix`, as `(key, upload id)`.
    pub async fn unfinished_uploads(&self, bucket: &str, prefix: &str) -> Vec<(String, String)> {
        let client = self.client();
        let mut found = Vec::new();
        let mut markers: Option<(String, String)> = None;
        loop {
            let request = ops::list_multipart_uploads(
                client.profile(),
                bucket,
                prefix,
                markers.as_ref().map(|(key, id)| (key.as_str(), id.as_str())),
            )
            .expect("a listing builds");
            let answer = client
                .exchange(request, QUERY_BUDGET)
                .await
                .unwrap_or_else(|e| panic!("listing uploads under {prefix:?}: {}", chain(&e)));
            assert!(answer.status.is_success(), "listing uploads answered {}", answer.status);
            let page = crate::xml::parse_list_multipart_uploads(&answer.text()).expect("a ListMultipartUploadsResult");
            found.extend(page.uploads.into_iter().map(|upload| (upload.key, upload.upload_id)));
            match (page.is_truncated, page.next_key_marker, page.next_upload_id_marker) {
                (true, Some(key), Some(id)) => markers = Some((key, id)),
                _ => break,
            }
        }
        found
    }

    /// Starts a multipart upload straight through the protocol, with no Cmdr
    /// record of it: what another tool's live upload looks like to a sweep.
    pub async fn start_foreign_upload(&self, bucket: &str, key: &str) -> String {
        let client = self.client();
        let request = ops::create_multipart_upload(client.profile(), bucket, key, &ObjectMetadata::default())
            .expect("a key builds");
        let answer = client
            .exchange(request, QUERY_BUDGET)
            .await
            .unwrap_or_else(|e| panic!("starting an upload of {key:?}: {}", chain(&e)));
        assert!(
            answer.status.is_success(),
            "starting an upload answered {}",
            answer.status
        );
        crate::xml::parse_initiate_multipart(&answer.text())
            .expect("an InitiateMultipartUploadResult")
            .upload_id
    }

    /// Aborts one upload straight through the protocol, for a cell's cleanup.
    pub async fn abort_upload(&self, bucket: &str, key: &str, upload_id: &str) {
        let client = self.client();
        let request = ops::abort_multipart_upload(client.profile(), bucket, key, upload_id).expect("a key builds");
        let _ = client.exchange(request, QUERY_BUDGET).await;
    }

    /// Every key under `prefix`, recursively.
    pub async fn keys_under(&self, bucket: &str, prefix: &str) -> Vec<String> {
        let client = self.client();
        let mut keys = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let params = ListObjectsParams {
                prefix,
                delimiter: None,
                continuation_token: token.as_deref(),
                max_keys: None,
            };
            let request = ops::list_objects(client.profile(), bucket, &params).expect("a listing builds");
            let answer = client
                .exchange(request, QUERY_BUDGET)
                .await
                .unwrap_or_else(|e| panic!("listing {prefix:?}: {}", chain(&e)));
            assert!(
                answer.status.is_success(),
                "listing {prefix:?} answered {}",
                answer.status
            );
            let page = crate::xml::parse_list_objects(&answer.text()).expect("a listing");
            keys.extend(page.objects.into_iter().map(|o| o.key));
            match page.next_continuation_token {
                Some(next) if page.is_truncated => token = Some(next),
                _ => return keys,
            }
        }
    }

    /// Removes everything under `prefix` in `bucket`, unfinished uploads
    /// included, one request per key (works whether the provider batches or
    /// not).
    pub async fn clean(&self, bucket: &str, prefix: &str) {
        for (key, id) in self.unfinished_uploads(bucket, prefix).await {
            self.abort_upload(bucket, &key, &id).await;
        }
        let client = self.client();
        let keys = self.keys_under(bucket, prefix).await;
        let deletes = keys.iter().map(|key| {
            let client = &client;
            async move {
                let request = ops::delete_object(client.profile(), bucket, key).expect("a key builds");
                let _ = client.exchange(request, QUERY_BUDGET).await;
            }
        });
        futures_util::stream::iter(deletes)
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await;
    }

    /// Removes everything this run wrote on a live account, in both buckets.
    /// A no-op on a fixture, whose objects stay (`scratch_prefix`).
    pub async fn clean_run(&self) {
        if !self.is_live() {
            return;
        }
        let root = live_run_root();
        self.clean(self.bucket(), &root).await;
        if let Some(second) = self.bucket_2() {
            self.clean(second, &root).await;
        }
    }
}

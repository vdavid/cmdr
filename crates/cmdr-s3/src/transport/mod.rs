//! The HTTP client: signs each request, sends it, and reads the answer.
//!
//! ❗ **`reqwest` is confined to this module.** Everything else works in
//! [`S3Request`]s going out and [`Answer`]s coming back, and hands a transport
//! failure straight back here to be judged ([`map_transport_error`],
//! [`classify_connect_error`]), so a client swap is one module's problem.
//! `answer.rs` holds what comes back; this file signs, routes, and sends.

mod answer;

use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::liveness::Liveness;
use cmdr_fs::volume::tls::has_tls_refusal;
use futures_util::Stream;
use http::{Method, StatusCode};
use log::debug;

use answer::read_capped;
pub(crate) use answer::{Answer, ExchangeError, MAX_ANSWER_BODY, Opened};

use crate::ops;
use crate::profile::ProviderProfile;
use crate::refusal::{BucketCheck, BucketList, S3ConnectError, judge_head_bucket, judge_list_buckets};
use crate::request::{Body, S3Request};
use crate::routing::{BUCKET_REGION_HEADER, BucketRegions, read_hint};
use crate::sigv4::{AmzTime, Credentials, Scope, sign};

/// The connect timeout on every request, and the connect probe's total budget
/// per request.
///
/// ❌ Never `ClientBuilder::read_timeout`: its sleep runs from the request
/// going out until the response HEADERS arrive, so it's a total budget on an
/// upload's whole body phase (`crates/cmdr-webdav/src/transport.rs` has the
/// evidence). The read and write paths (M4, M5) carry none.
pub(crate) const REQUEST_BUDGET: Duration = Duration::from_secs(10);

/// One listing page's or one HEAD's total budget: bounded work, though a
/// page on a slow or throttled server may take a while.
pub(crate) const QUERY_BUDGET: Duration = Duration::from_secs(60);

/// The bytes of a streamed request body ([`S3Client::upload`]): every piece the
/// transport sends, in order. An `Err` aborts the request on the wire, which is
/// how a refused source or a cancel keeps S3 from publishing anything.
pub(crate) type UploadBody = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

/// A `CompleteMultipartUpload`'s total budget. Longer than a query's: AWS may
/// take minutes to assemble a big object, sending whitespace meanwhile, which
/// the body read counts as heard.
pub(crate) const COMPLETE_BUDGET: Duration = Duration::from_secs(15 * 60);

/// What [`S3Client::relearn`] reads off an answer.
#[derive(Debug, Clone, Copy)]
struct Heard<'a> {
    status: StatusCode,
    /// `x-amz-bucket-region`.
    region_header: Option<&'a str>,
    /// The error body, `""` when it isn't read.
    body: &'a str,
}

/// One signed client for one account on one endpoint.
pub(crate) struct S3Client {
    http: reqwest::Client,
    /// The silence watch's line to the server: the same settings with pooling
    /// OFF, so every probe dials fresh. ❗ A pooled probe could ride the very
    /// connection that went quiet.
    fresh: reqwest::Client,
    profile: ProviderProfile,
    credentials: Credentials,
    /// What the server has said lately (`cmdr_fs::volume::liveness`). Dies
    /// with this client: a reconnect builds a new one.
    liveness: Arc<Liveness>,
    /// Each bucket's region, on a routed account root only
    /// ([`Self::route_each_bucket`]); `None` sends everything to the profile's.
    regions: Option<BucketRegions>,
    /// Every request signed so far, by S3 operation, for a cell comparing
    /// what a write path sent with its `cost::Workload`.
    #[cfg(any(test, feature = "testing"))]
    sent: std::sync::Mutex<std::collections::BTreeMap<&'static str, u64>>,
    /// The HTTP version the last answer came back over, for a live cell
    /// proving the crate negotiates what the app does (HTTP/2 where offered).
    #[cfg(test)]
    negotiated: std::sync::Mutex<Option<http::Version>>,
}

impl S3Client {
    /// Builds the client. Redirects are off: S3 answers a request for a bucket
    /// in another region with a 301, and following it would re-send a request
    /// signed for the wrong host.
    pub(crate) fn new(profile: ProviderProfile, credentials: Credentials) -> Result<Self, S3ConnectError> {
        Self::build(profile, credentials, |builder| builder)
    }

    /// A client that dials `addr` for every one of `hosts`, for a cell that
    /// plays AWS's regional endpoints on one local server.
    #[cfg(test)]
    pub(crate) fn resolving(
        profile: ProviderProfile,
        credentials: Credentials,
        hosts: &[String],
        addr: std::net::SocketAddr,
    ) -> Self {
        Self::build(profile, credentials, |mut builder| {
            for host in hosts {
                builder = builder.resolve(host, addr);
            }
            builder
        })
        .expect("a test client builds")
    }

    fn build(
        profile: ProviderProfile,
        credentials: Credentials,
        tweak: impl Fn(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> Result<Self, S3ConnectError> {
        let builder = || {
            tweak(
                cmdr_http::client_builder()
                    .user_agent("Cmdr")
                    .connect_timeout(REQUEST_BUDGET)
                    .redirect(reqwest::redirect::Policy::none())
                    // ❗ Bytes as stored, never decoded: an object kept with a
                    // `Content-Encoding` must copy as itself, at the length its
                    // `Content-Length` and ETag describe. Whatever decoders
                    // other crates unify into the app's reqwest (`genai` brings
                    // `gzip`), each is off here (`transport_test.rs`).
                    .no_gzip()
                    .no_brotli()
                    .no_deflate()
                    .no_zstd(),
            )
        };
        let build_failed = |e: reqwest::Error| S3ConnectError::Transport(e.to_string());
        Ok(Self {
            http: builder().build().map_err(build_failed)?,
            // `0` turns hyper-util's pool off outright (verified on 0.1.20,
            // `pool::Config::is_enabled`, 2026-09-23).
            fresh: builder().pool_max_idle_per_host(0).build().map_err(build_failed)?,
            profile,
            credentials,
            liveness: Arc::new(Liveness::new()),
            regions: None,
            #[cfg(any(test, feature = "testing"))]
            sent: std::sync::Mutex::default(),
            #[cfg(test)]
            negotiated: std::sync::Mutex::default(),
        })
    }

    /// Sends each bucket's requests to that bucket's own region, learned as
    /// answers name it (`routing.rs`). For an account root on the profile's
    /// routing allowlist only (`ProviderProfile::routes_by_region`: AWS and
    /// Wasabi): a bucket place keeps the connect probe's wrong-region refusal.
    /// A no-op everywhere else.
    pub(crate) fn route_each_bucket(&mut self) {
        if self.profile.routes_by_region() {
            self.regions = Some(BucketRegions::default());
        }
    }

    /// Whether this client routes per bucket.
    #[cfg(test)]
    pub(crate) fn routes_each_bucket(&self) -> bool {
        self.regions.is_some()
    }

    /// Records a bucket's region a listing named (`ListBuckets`'
    /// `BucketRegion`), so its first request goes straight there.
    pub(crate) fn learn_bucket_region(&self, bucket: &str, region: &str) {
        if let Some(regions) = &self.regions {
            regions.learn(bucket, region);
        }
    }

    /// `request` addressed to its bucket's region, with the region to sign
    /// for: the profile's own unless the bucket is known to live elsewhere.
    fn route(&self, request: S3Request) -> (S3Request, String) {
        let known = self
            .regions
            .as_ref()
            .zip(request.bucket.as_deref())
            .and_then(|(regions, bucket)| regions.region_of(bucket))
            .filter(|region| *region != self.profile.region);
        match known {
            Some(region) => match self.profile.reroute(request.clone(), &region) {
                Some(rerouted) => (rerouted, region),
                None => (request, self.profile.region.clone()),
            },
            None => (request, self.profile.region.clone()),
        }
    }

    /// The bucket a request may be re-sent for, when this client routes.
    fn routed_bucket(&self, request: &S3Request) -> Option<String> {
        self.regions.as_ref().and(request.bucket.clone())
    }

    /// Learns from an answer to a request for `bucket` signed for `signed_for`,
    /// and says whether to send it again: it went to the wrong region and the
    /// right one is now known. A misrouted answer that names no region asks
    /// `HeadBucket`, which names it on every status.
    async fn relearn(&self, bucket: &str, answer: Heard<'_>, signed_for: &str) -> bool {
        let Some(regions) = &self.regions else {
            return false;
        };
        let hint = read_hint(answer.status, answer.region_header, answer.body);
        if let Some(region) = &hint.region {
            regions.learn(bucket, region);
        }
        if !hint.misrouted {
            return false;
        }
        if hint.region.is_none() {
            self.discover_region(bucket).await;
        }
        regions.region_of(bucket).is_some_and(|region| region != signed_for)
    }

    /// Asks `HeadBucket` where `bucket` lives when nothing has said yet:
    /// before an upload, whose body can't be sent twice, and a share link.
    async fn ensure_region(&self, bucket: &str) {
        if self
            .regions
            .as_ref()
            .is_some_and(|regions| regions.region_of(bucket).is_none())
        {
            self.discover_region(bucket).await;
        }
    }

    /// One `HeadBucket` on the profile's own endpoint, which answers
    /// `x-amz-bucket-region` whatever its status. A failure teaches nothing;
    /// the request it was for then comes back as it is.
    async fn discover_region(&self, bucket: &str) {
        let (Some(regions), Ok(head)) = (&self.regions, ops::head_bucket(&self.profile, bucket)) else {
            return;
        };
        let region = self.profile.region.clone();
        if let Ok(answer) = self.send(head, &region, REQUEST_BUDGET).await
            && let Some(named) = answer.header(BUCKET_REGION_HEADER)
        {
            regions.learn(bucket, named);
        }
    }

    /// Counts `request` under its S3 operation. A no-op outside tests.
    #[cfg_attr(not(any(test, feature = "testing")), allow(clippy::unused_self))]
    fn note_sent(&self, request: &S3Request) {
        #[cfg(any(test, feature = "testing"))]
        {
            let operation = operation_of(request);
            // Which request went where, in order, for a cell attributing its
            // counts (`RUST_LOG=s3_sent=trace`).
            log::trace!(target: "s3_sent", "{operation} {} {:?}", request.path, request.query);
            *self
                .sent
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(operation)
                .or_default() += 1;
        }
        #[cfg(not(any(test, feature = "testing")))]
        let _ = request;
    }

    /// Records the HTTP version an answer came over. A no-op outside tests.
    #[cfg_attr(not(test), allow(clippy::unused_self, reason = "only tests read what's recorded"))]
    fn note_version(&self, version: http::Version) {
        #[cfg(test)]
        {
            *self
                .negotiated
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(version);
        }
        #[cfg(not(test))]
        let _ = version;
    }

    /// The HTTP version the last answer came back over.
    #[cfg(test)]
    pub(crate) fn negotiated(&self) -> Option<http::Version> {
        *self
            .negotiated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every request signed so far, by S3 operation (`ListObjectsV2`,
    /// `PutObject`, ...), and starts counting again from zero.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn take_sent(&self) -> std::collections::BTreeMap<&'static str, u64> {
        std::mem::take(&mut *self.sent.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// Whether a request with `method` must ask for the stored bytes with
    /// `Accept-Encoding: gzip` (a read on a provider that transcodes, GCS).
    /// ❗ Added after signing, never signed: GCS's front end rewrites the
    /// header before it checks the signature (`SignatureDoesNotMatch`, live,
    /// 2026-10-02). The client's decoders are off, so the body stays verbatim.
    fn asks_for_stored_bytes(&self, method: &Method) -> bool {
        self.profile.transcodes_gzip && (*method == Method::GET || *method == Method::HEAD)
    }

    /// The provider profile every request is built against.
    pub(crate) fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    /// This client's silence watch.
    pub(crate) fn liveness(&self) -> &Arc<Liveness> {
        &self.liveness
    }

    /// Signs `request` now, sends it, and reads the whole answer within
    /// `budget`, noting the headers and every body chunk as the server being
    /// there. ❗ Every request goes out through here, or its bytes never count
    /// against silence.
    ///
    /// ❗ For bodies held in memory only. A `Body::Streamed` request goes out
    /// with no body and the server refuses it (S3 answers a missing
    /// `Content-Length` with 411); the streaming write path brings its own
    /// sender.
    ///
    /// On a routed account root (AWS, Wasabi), a request that went to the wrong region goes
    /// once more, to the region the answer named ([`Self::route_each_bucket`]).
    pub(crate) async fn exchange(&self, request: S3Request, budget: Duration) -> Result<Answer, ExchangeError> {
        let Some(bucket) = self.routed_bucket(&request) else {
            let region = self.profile.region.clone();
            return self.send(request, &region, budget).await;
        };
        let (routed, region) = self.route(request.clone());
        let answer = self.send(routed, &region, budget).await?;
        let heard = Heard {
            status: answer.status,
            region_header: answer.header(BUCKET_REGION_HEADER),
            body: &answer.text(),
        };
        if !self.relearn(&bucket, heard, &region).await {
            return Ok(answer);
        }
        let (routed, region) = self.route(request);
        self.send(routed, &region, budget).await
    }

    /// One signed exchange for `region`, as it is.
    async fn send(&self, request: S3Request, region: &str, budget: Duration) -> Result<Answer, ExchangeError> {
        let time = AmzTime::new(SystemTime::now());
        let scope = Scope {
            credentials: &self.credentials,
            region,
            time: &time,
        };
        self.note_sent(&request);
        let signed = sign(request, &scope);
        let writes = signed.method == Method::POST || signed.method == Method::PUT;
        let reads = self.asks_for_stored_bytes(&signed.method);
        let mut builder = self
            .http
            .request(signed.method, signed.url)
            .headers(signed.headers)
            .timeout(budget);
        if reads {
            builder = builder.header(reqwest::header::ACCEPT_ENCODING, "gzip");
        }
        // ❗ A write says its length even when it's zero: hyper sends none for
        // an empty body, and GCS answers `411` to a bodyless POST
        // (`CreateMultipartUpload`), R2, Hetzner, and GCS to an empty PUT (a
        // folder marker) (live, 2026-10-02).
        match signed.body {
            Body::Bytes(bytes) if bytes.is_empty() && writes => {
                builder = builder.header(reqwest::header::CONTENT_LENGTH, 0);
            }
            Body::Bytes(bytes) => builder = builder.body(bytes),
            Body::Empty if writes => {
                builder = builder.header(reqwest::header::CONTENT_LENGTH, 0);
            }
            Body::Empty | Body::Streamed { .. } => {}
        }
        let response = builder.send().await?;
        self.liveness.heard();
        self.note_version(response.version());
        let status = response.status();
        let headers = response.headers().clone();
        let body = read_capped(&self.liveness, response, MAX_ANSWER_BODY).await?;
        Ok(Answer { status, headers, body })
    }

    /// Signs `request` (a `Body::Streamed { length }`, so `UNSIGNED-PAYLOAD`)
    /// and sends `body` as its bytes with `Content-Length: length`, then reads
    /// the answer whole (a PUT answers nothing, an error a small XML document).
    ///
    /// ❗ `Content-Length` always, ❌ never a chunked body: S3 answers 411 to a
    /// PUT without one, and a body that ends short makes hyper abort the
    /// request, which S3 never publishes. ❌ No `.timeout()`: an upload has no
    /// total budget, and silence is the watch's to judge (the body source
    /// counts every piece it hands over as heard).
    ///
    /// ❗ The body goes once, so on a routed account root a bucket's region is
    /// asked for first when nothing has named it yet.
    pub(crate) async fn upload(&self, request: S3Request, body: UploadBody) -> Result<Answer, ExchangeError> {
        let length = match request.body {
            Body::Streamed { length } => length,
            Body::Empty | Body::Bytes(_) => 0,
        };
        if let Some(bucket) = self.routed_bucket(&request) {
            self.ensure_region(&bucket).await;
        }
        let (request, region) = self.route(request);
        let time = AmzTime::new(SystemTime::now());
        let scope = Scope {
            credentials: &self.credentials,
            region: &region,
            time: &time,
        };
        self.note_sent(&request);
        let signed = sign(request, &scope);
        let response = self
            .http
            .request(signed.method, signed.url)
            .headers(signed.headers)
            .header(reqwest::header::CONTENT_LENGTH, length)
            .body(reqwest::Body::wrap_stream(body))
            .send()
            .await?;
        self.liveness.heard();
        self.note_version(response.version());
        let status = response.status();
        let headers = response.headers().clone();
        let body = read_capped(&self.liveness, response, MAX_ANSWER_BODY).await?;
        Ok(Answer { status, headers, body })
    }

    /// Signs `request` now and sends it, handing back the answer with its body
    /// still on the wire, for a GET whose body may be any size.
    ///
    /// ❗ No `.timeout()` on the request: that would be a total budget on the
    /// whole body, and a multi-GB download has none. Only the wait for the
    /// headers is bounded (`QUERY_BUDGET`); the body's budget is per chunk, in
    /// the caller ([`Opened::chunk`] counts each one as heard).
    ///
    /// On a routed account root, a redirect to another region is followed once,
    /// like [`Self::exchange`]'s.
    pub(crate) async fn open(&self, request: S3Request, volume_id: &str, path: &str) -> Result<Opened, VolumeError> {
        let Some(bucket) = self.routed_bucket(&request) else {
            let region = self.profile.region.clone();
            return self.open_once(request, &region, volume_id, path).await;
        };
        let (routed, region) = self.route(request.clone());
        let opened = self.open_once(routed, &region, volume_id, path).await?;
        // A redirect only: judging a 400 would mean reading its body, which
        // the caller reads itself.
        let heard = Heard {
            status: opened.status,
            region_header: opened.header(BUCKET_REGION_HEADER),
            body: "",
        };
        if !opened.status.is_redirection() || !self.relearn(&bucket, heard, &region).await {
            return Ok(opened);
        }
        let (routed, region) = self.route(request);
        self.open_once(routed, &region, volume_id, path).await
    }

    /// One signed GET for `region`, as it is.
    async fn open_once(
        &self,
        request: S3Request,
        region: &str,
        volume_id: &str,
        path: &str,
    ) -> Result<Opened, VolumeError> {
        let time = AmzTime::new(SystemTime::now());
        let scope = Scope {
            credentials: &self.credentials,
            region,
            time: &time,
        };
        self.note_sent(&request);
        let signed = sign(request, &scope);
        let reads = self.asks_for_stored_bytes(&signed.method);
        let mut builder = self.http.request(signed.method, signed.url).headers(signed.headers);
        if reads {
            builder = builder.header(reqwest::header::ACCEPT_ENCODING, "gzip");
        }
        let sent = builder.send();
        let response = match tokio::time::timeout(QUERY_BUDGET, sent).await {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => return Err(map_transport_error(&e, volume_id, path)),
            Err(_elapsed) => return Err(VolumeError::ConnectionTimeout(path.to_string())),
        };
        self.liveness.heard();
        self.note_version(response.version());
        Ok(Opened {
            status: response.status(),
            headers: response.headers().clone(),
            response,
            liveness: Arc::clone(&self.liveness),
            volume_id: volume_id.to_string(),
            path: path.to_string(),
        })
    }

    /// A presigned GET for `key`, valid for `expires` from now (`ops::share_link`).
    /// Here because the credentials are: computed offline, nothing is sent,
    /// except one `HeadBucket` on a routed account root that doesn't know the
    /// bucket's region yet (a link to the wrong region only redirects).
    /// ❗ The URL carries a signature that reads the object; ❌ never log it.
    pub(crate) async fn share_link(
        &self,
        bucket: &str,
        key: &str,
        expires: Duration,
    ) -> Result<url::Url, ops::ShareLinkError> {
        self.ensure_region(bucket).await;
        let region = self.regions.as_ref().and_then(|regions| regions.region_of(bucket));
        let target = ops::LinkTarget {
            bucket,
            key,
            region: region.as_deref(),
        };
        ops::share_link(&self.profile, &self.credentials, target, SystemTime::now(), expires)
    }

    /// Whether the server answers at all, on a fresh connection: an unsigned
    /// HEAD on the endpoint, any status counting. The watch applies the budget.
    pub(crate) async fn ping(&self) -> bool {
        let url = format!("{}://{}/", self.profile.scheme, self.profile.endpoint_host);
        self.fresh.head(url).send().await.is_ok()
    }

    /// The connect probe: `ListBuckets`, then `HeadBucket` when the place is a
    /// bucket, judged by `refusal.rs`'s table.
    ///
    /// ❗ `ListBuckets` goes first even for a bucket: it's the one call whose
    /// error BODY tells a wrong secret (`SignatureDoesNotMatch`) from a key
    /// without rights. A HEAD has no body to say which.
    pub(crate) async fn probe(&self, bucket: Option<&str>) -> Result<(), S3ConnectError> {
        debug!(target: "volume", "s3 probe: ListBuckets on {}", self.profile.endpoint_host);
        let listed = self
            .exchange(ops::list_buckets(&self.profile, None), REQUEST_BUDGET)
            .await
            .map_err(|e| classify_connect_error(&e))?;
        let bucket = match (
            judge_list_buckets(listed.status, &listed.text(), bucket.is_some()),
            bucket,
        ) {
            (BucketList::Refused(error), _) => return Err(error),
            (BucketList::Listed, None) => return Ok(()),
            (BucketList::FallBack, None) => return Err(S3ConnectError::BucketListRefused),
            (BucketList::Listed | BucketList::FallBack, Some(bucket)) => bucket,
        };
        // A name that can't be a bucket (a `/` in it) isn't one this endpoint has.
        let head = ops::head_bucket(&self.profile, bucket).map_err(|_| S3ConnectError::NoSuchBucket)?;
        let answer = self
            .exchange(head, REQUEST_BUDGET)
            .await
            .map_err(|e| classify_connect_error(&e))?;
        match judge_head_bucket(answer.status, answer.header("x-amz-bucket-region")) {
            BucketCheck::Open => Ok(()),
            BucketCheck::Refused(error) => Err(error),
        }
    }
}

/// The S3 operation `request` is, by its method, query, and copy-source
/// header: the names the price table (`cost/prices.rs`) and the API reference
/// spell.
#[cfg(any(test, feature = "testing"))]
fn operation_of(request: &S3Request) -> &'static str {
    let has = |name: &str| request.query.iter().any(|(key, _)| key == name);
    let copies = request.headers.contains_key("x-amz-copy-source");
    // A path-style request names the bucket in its first segment; a
    // virtual-hosted one in its host.
    let path = request.path.trim_start_matches('/');
    let key = match request.bucket.as_deref() {
        Some(bucket) if path == bucket => "",
        Some(bucket) if path.starts_with(&format!("{bucket}/")) => &path[bucket.len() + 1..],
        _ => path,
    };
    match request.method {
        Method::GET if request.bucket.is_none() => "ListBuckets",
        Method::GET if has("uploads") => "ListMultipartUploads",
        Method::GET if has("list-type") => "ListObjectsV2",
        Method::GET => "GetObject",
        Method::HEAD if key.is_empty() => "HeadBucket",
        Method::HEAD => "HeadObject",
        Method::PUT if copies && has("partNumber") => "UploadPartCopy",
        Method::PUT if copies => "CopyObject",
        Method::PUT if has("partNumber") => "UploadPart",
        Method::PUT => "PutObject",
        Method::POST if has("uploads") => "CreateMultipartUpload",
        Method::POST if has("uploadId") => "CompleteMultipartUpload",
        Method::POST if has("delete") => "DeleteObjects",
        Method::DELETE if has("uploadId") => "AbortMultipartUpload",
        Method::DELETE => "DeleteObject",
        _ => "Other",
    }
}

/// Turns a `reqwest` failure (no status came back) into the `Volume`
/// vocabulary, by its typed predicates.
///
/// A connection that couldn't be made or was cut mid-flight is the volume
/// being gone, which is what starts the reconnect loop; a timeout is its own
/// variant, ❌ never read as a lost server (a slow page isn't one). An answer
/// past [`MAX_ANSWER_BODY`] is an `IoError`, like a body that won't parse.
pub(crate) fn map_exchange_error(err: &ExchangeError, volume_id: &str, path: &str) -> VolumeError {
    match err {
        ExchangeError::Transport(err) => map_transport_error(err, volume_id, path),
        ExchangeError::BodyTooLarge { .. } => VolumeError::IoError {
            message: err.to_string(),
            raw_os_error: None,
        },
    }
}

/// [`map_exchange_error`] for a `reqwest` failure on a path with no buffered
/// answer (a streaming GET).
pub(crate) fn map_transport_error(err: &reqwest::Error, volume_id: &str, path: &str) -> VolumeError {
    if err.is_timeout() {
        return VolumeError::ConnectionTimeout(path.to_string());
    }
    if err.is_connect() || err.is_request() {
        debug!(
            "S3 path={path:?}: source=backend, backend=s3, error_kind=disconnected, detail={:?}",
            cmdr_fs::log_detail::LogDetail(&err.to_string())
        );
        return VolumeError::DeviceDisconnected(volume_id.to_string());
    }
    VolumeError::IoError {
        message: err.to_string(),
        raw_os_error: None,
    }
}

/// Classifies a `reqwest` failure on the CONNECT probe.
///
/// A TLS refusal reaches here as a connect error whose source chain carries an
/// `io::Error` of kind `InvalidData`, which is how `tokio-rustls` wraps every
/// handshake refusal. ❗ Judged by the typed `ErrorKind`, ❌ never the message.
pub(crate) fn classify_connect_error(err: &ExchangeError) -> S3ConnectError {
    let err = match err {
        ExchangeError::Transport(err) => err,
        // A probe answer past the cap isn't an S3 server answering it.
        ExchangeError::BodyTooLarge { .. } => return S3ConnectError::Transport(err.to_string()),
    };
    if err.is_timeout() {
        return S3ConnectError::TimedOut;
    }
    if err.is_connect() {
        if has_tls_refusal(err) {
            return S3ConnectError::CertificateUntrusted;
        }
        return S3ConnectError::Unreachable(err.to_string());
    }
    if err.is_request() {
        return S3ConnectError::Unreachable(err.to_string());
    }
    S3ConnectError::Transport(err.to_string())
}

#[cfg(test)]
#[path = "transport_routing_test.rs"]
mod transport_routing_test;
#[cfg(test)]
#[path = "transport_test.rs"]
mod transport_test;

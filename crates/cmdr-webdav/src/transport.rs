//! The HTTP client, and every request shape this backend sends.
//!
//! ❗ **`reqwest` is confined to this file, `errors.rs` (status codes and the
//! typed error predicates), and `streams.rs` / `writes.rs` /
//! `chunked_upload.rs` (the bodies that stream).** Everything else works in
//! `Url`s, status codes, and [`PropfindEntry`]s, so a client swap is at most
//! five files' problem.

use std::sync::Arc;
use std::time::Duration;

use log::debug;
use reqwest::header::{HeaderValue, WWW_AUTHENTICATE};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::chunking::ChunkedUploads;
use crate::errors::{WebdavConnectError, classify_connect_error};
use crate::propfind::{PropfindEntry, every_propstat_succeeded, parse_multistatus};
use crate::urls::join_remote;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::liveness::Liveness;

/// The connect budget, per request: the PROBE's total budget and the idle
/// budget between body chunks on a download (`streams.rs`). A transfer as a
/// whole has no ceiling.
///
/// ❌ Never `ClientBuilder::read_timeout` (verified on reqwest 0.13.4,
/// `PendingRequest::poll`, 2026-09-01): that sleep is armed when the request
/// goes out and is not reset until the response HEADERS arrive, so it is a
/// total budget for the whole upload phase. A PUT that takes longer than it
/// to stream its body fails with `TimedOut`, and every upload past a few MB
/// on a slow link is lost. Idle detection lives on the response body instead.
pub(crate) const REQUEST_BUDGET: Duration = Duration::from_secs(10);

/// A PROPFIND's total budget: bounded work, but a listing on a slow NAS may
/// legitimately take a while.
const PROPFIND_BUDGET: Duration = Duration::from_secs(60);

/// The most a PROPFIND answer may hold. A `Depth: 1` response carries roughly
/// 500–700 bytes per child, so this is about 200 000 entries in one folder;
/// past it the server is hostile or broken, and reading on would hold the whole
/// body in memory.
pub(crate) const MAX_LISTING_BODY: usize = 128 * 1024 * 1024;

/// The most the connect probe's `Depth: 0` answer may hold: one resource's
/// properties fit in a few KiB.
const MAX_PROBE_BODY: usize = 1024 * 1024;

/// The total budget of one non-streaming verb (MOVE, COPY, DELETE, MKCOL, and
/// `create_file`'s small in-memory PUT): a server that accepted the connection
/// and then hangs is cut instead of holding the operation until the user
/// cancels. Generous, because a server-side COPY of a large file or a
/// recursive DELETE can legitimately take minutes.
///
/// ❌ Never on the streaming PUT or a GET: their bodies stream for as long as
/// they take, and their stall detection lives elsewhere (`streams.rs`'s idle
/// budget for downloads; the upload has none, see `REQUEST_BUDGET`).
pub(crate) const MUTATION_BUDGET: Duration = Duration::from_secs(10 * 60);

/// The methods HTTP itself doesn't name.
pub(crate) fn method(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).unwrap_or(Method::GET)
}

/// The `Depth` header a PROPFIND goes out with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    /// The resource itself.
    Zero,
    /// The resource and its direct children.
    One,
}

/// The properties every PROPFIND asks for, quota included: the extra elements
/// cost nothing on a server without them, which answers a 404 `propstat` the
/// parser skips.
///
/// ❗ `oc:size` is NOT standard. It's ownCloud's, inherited by Nextcloud, and
/// it's here because an unlimited Nextcloud account answers RFC 4331's
/// `quota-used-bytes` with `0` while holding real bytes, so the standard alone
/// can't report space for the commonest Nextcloud setup there is. Declaring the
/// namespace is harmless for every other server. `DETAILS.md` § What a real
/// server answers.
const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:oc="http://owncloud.org/ns"><D:prop>
<D:resourcetype/><D:getcontentlength/><D:getlastmodified/><D:creationdate/><D:getetag/>
<D:quota-available-bytes/><D:quota-used-bytes/><oc:size/>
</D:prop></D:propfind>"#;

/// The connect probe's question: the listing's properties plus Nextcloud's
/// `creation_time`, whose presence in a 2xx `propstat` is how a server says it
/// keeps creation dates (`X-OC-CTime`) and takes a folder's dates by PROPPATCH
/// (`DETAILS.md` § "Dates"). ❗ Asked by the probe ONLY: a server without it
/// answers a 404 `propstat` per entry, which on a big listing is real bytes.
const PROBE_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns"><D:prop>
<D:resourcetype/><D:getcontentlength/><D:getlastmodified/><D:creationdate/><D:getetag/>
<D:quota-available-bytes/><D:quota-used-bytes/><oc:size/><nc:creation_time/>
</D:prop></D:propfind>"#;

/// The header a Nextcloud answers every authenticated DAV request with, naming
/// the account's user id (its `UserIdHeaderPlugin`; verified on
/// `nextcloud:35.0.0-apache` by `curl` on both DAV endpoints, 2026-10-14).
/// ❗ The user id, which can differ from the login name (an email login, LDAP).
const USER_ID_HEADER: &str = "X-User-Id";

/// The most a PROPPATCH answer may hold: one resource's handful of statuses.
const MAX_PROPPATCH_BODY: usize = 64 * 1024;

/// How long the https-first probe (`https_first.rs`) waits before the `http`
/// address is dialed as typed. Short: a person is waiting on the Add.
const HTTPS_PROBE_BUDGET: Duration = Duration::from_secs(4);

/// The compliance-class header every WebDAV server sends on `OPTIONS` (RFC 4918
/// § 10.1), and the probe's proof it reached one.
const DAV_HEADER: &str = "DAV";

/// Whether a WebDAV server answers at `url`: for an `https` URL, over TLS with
/// a certificate this Mac trusts. Any status counts (the question is the
/// server, not the account), ❗ but only with a `DAV` header: a 443 serving
/// another site, or a redirect back to `http`, answers without one. ❗ No
/// credentials go out, and a plain-`http` port, an untrusted certificate, a
/// refusal, and silence all answer `false`.
pub(crate) async fn answers_as_webdav(url: Url, cancel: &CancellationToken) -> bool {
    let Ok(client) = cmdr_http::client_builder()
        .user_agent("Cmdr")
        .connect_timeout(HTTPS_PROBE_BUDGET)
        .timeout(HTTPS_PROBE_BUDGET)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    tokio::select! {
        () = cancel.cancelled() => false,
        sent = client.request(Method::OPTIONS, url).send() => {
            sent.is_ok_and(|answer| answer.headers().contains_key(DAV_HEADER))
        }
    }
}

/// One authenticated client for one account on one server.
pub(crate) struct WebdavClient {
    http: reqwest::Client,
    /// The silence watchdog's line to the server: the same settings with
    /// pooling OFF, so every probe dials fresh. ❗ A pooled probe could ride
    /// the very connection that went quiet, or skip the dial that tells a
    /// server gone from a server busy.
    fresh: reqwest::Client,
    base: Url,
    username: String,
    password: String,
    /// What the server has said lately (`cmdr_fs::volume::liveness`). Dies with this
    /// client: a reconnect builds a new one.
    liveness: Arc<Liveness>,
    /// Set by the connect probe on a Nextcloud (`chunking.rs`).
    chunked_uploads: Option<ChunkedUploads>,
}

impl WebdavClient {
    /// Builds the client. Redirects are off: a followed MOVE or COPY would
    /// resend its `Destination` against a URL the user never named.
    ///
    /// The pool keeps `reqwest`'s TCP keepalive (15 s idle, then 3 probes
    /// 15 s apart; verified on reqwest 0.13.4, `ClientBuilder::new`,
    /// 2026-09-23), so an idle pooled connection whose far end vanished is
    /// reaped by the OS rather than handed to the next request.
    ///
    /// `pinned` is the certificate fingerprint a person trusted for this
    /// server, if any: TLS trusts the OS's verdict plus that one certificate
    /// (`tls_trust.rs`). ❗ Read by the caller for every client, so a reconnect
    /// sees a pin approved since the last one.
    pub(crate) fn new(
        base: Url,
        username: &str,
        password: &str,
        pinned: Option<String>,
    ) -> Result<Self, WebdavConnectError> {
        let tls = crate::tls_trust::client_config(pinned).map_err(|e| WebdavConnectError::Transport(e.to_string()))?;
        let builder = || {
            cmdr_http::client_builder()
                .user_agent("Cmdr")
                .connect_timeout(REQUEST_BUDGET)
                .redirect(reqwest::redirect::Policy::none())
                // ❗ Bytes as served, never decoded: a file sent with a
                // `Content-Encoding` must copy as itself, at the length its
                // `Content-Length` says. Whatever decoders other crates unify
                // into the app's reqwest (`genai` brings `gzip`), each is off
                // here (`transport_test.rs`).
                .no_gzip()
                .no_brotli()
                .no_deflate()
                .no_zstd()
                .tls_backend_preconfigured(tls.clone())
        };
        let build_failed = |e: reqwest::Error| WebdavConnectError::Transport(e.to_string());
        Ok(Self {
            // ❗ The jar keeps the server's session, which spares Nextcloud a
            // password hash per request (~150 ms each without it, ~20 ms with
            // it). Basic still goes on every request, so an expired session
            // falls back to the password. `DETAILS.md` § "Session cookies".
            http: builder().cookie_store(true).build().map_err(build_failed)?,
            // `0` turns hyper-util's pool off outright (verified on 0.1.20,
            // `pool::Config::is_enabled`, 2026-09-23).
            fresh: builder().pool_max_idle_per_host(0).build().map_err(build_failed)?,
            base,
            username: username.to_string(),
            password: password.to_string(),
            liveness: Arc::new(Liveness::new()),
            chunked_uploads: None,
        })
    }

    /// This client's silence watch.
    pub(crate) fn liveness(&self) -> &Arc<Liveness> {
        &self.liveness
    }

    /// Sends `request` and notes the answer's headers as the server being
    /// there. ❗ Every request that expects an answer goes out through here or
    /// through [`Self::propfind`], or its bytes never count against silence.
    pub(crate) async fn send(&self, request: RequestBuilder) -> Result<Response, reqwest::Error> {
        let response = request.send().await?;
        self.liveness.heard();
        Ok(response)
    }

    /// Whether the server answers at all, on a fresh connection: an `OPTIONS`
    /// on the base URL, any status counting. The watchdog applies the budget.
    pub(crate) async fn ping(&self) -> bool {
        self.fresh
            .request(Method::OPTIONS, self.base.clone())
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .is_ok()
    }

    /// The path prefix under which this client addresses everything, decoded.
    pub(crate) fn base_path(&self) -> String {
        percent_encoding::percent_decode_str(self.base.path())
            .decode_utf8_lossy()
            .into_owned()
    }

    /// The URL for a root-relative remote path like `/Photos/a b.jpg`, each
    /// segment percent-encoded. `collection` appends the trailing slash a
    /// collection is addressed with.
    pub(crate) fn url_for(&self, remote_path: &str, collection: bool) -> Url {
        join_remote(&self.base, remote_path, collection)
    }

    /// Where this server takes a chunked upload, when it's a Nextcloud
    /// (`chunking.rs`). Answered by the connect probe, so it dies with this
    /// client and every reconnect asks again.
    pub(crate) fn chunked_uploads(&self) -> Option<&ChunkedUploads> {
        self.chunked_uploads.as_ref()
    }

    /// A request with this account's credentials on it.
    pub(crate) fn request(&self, method: Method, url: Url) -> RequestBuilder {
        self.http
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
    }

    /// A PROPFIND at `url`, parsed. Answers the raw status when the server
    /// didn't say 207, and `Ok(Err(..))` for a transport failure.
    ///
    /// ❗ A redirect on a collection addressed without its slash is retried
    /// once WITH it: nginx and some NAS firmware answer 301 there rather than
    /// serving the listing.
    pub(crate) async fn propfind(&self, url: Url, depth: Depth) -> Result<PropfindOutcome, reqwest::Error> {
        self.propfind_within(url, depth, MAX_LISTING_BODY).await
    }

    /// [`propfind`](Self::propfind) with the body capped at `limit` bytes.
    pub(crate) async fn propfind_within(
        &self,
        url: Url,
        depth: Depth,
        limit: usize,
    ) -> Result<PropfindOutcome, reqwest::Error> {
        let response = self.send_propfind(url.clone(), depth).await?;
        let response = if response.status().is_redirection() && !url.path().ends_with('/') {
            let mut with_slash = url;
            let path = format!("{}/", with_slash.path());
            with_slash.set_path(&path);
            self.send_propfind(with_slash, depth).await?
        } else {
            response
        };
        let status = response.status();
        if status != StatusCode::MULTI_STATUS {
            return Ok(PropfindOutcome::Status(status));
        }
        let Some(body) = self.read_body(response, limit).await? else {
            return Ok(PropfindOutcome::TooLarge { limit });
        };
        Ok(match parse_multistatus(&String::from_utf8_lossy(&body)) {
            Ok(entries) => PropfindOutcome::Entries(entries),
            Err(_) => PropfindOutcome::NotMultistatus,
        })
    }

    /// A whole response body, noting every chunk as the server being there:
    /// ❗ a big listing on a slow server trickles in for a long time, and that
    /// is exactly the wait that must never read as silence.
    ///
    /// `None` once the body runs past `limit` bytes (or announces it will),
    /// with the rest left unread.
    async fn read_body(&self, mut response: Response, limit: usize) -> Result<Option<Vec<u8>>, reqwest::Error> {
        if response
            .content_length()
            .is_some_and(|announced| announced > limit as u64)
        {
            return Ok(None);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            self.liveness.heard();
            if body.len() + chunk.len() > limit {
                return Ok(None);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Some(body))
    }

    async fn send_propfind(&self, url: Url, depth: Depth) -> Result<Response, reqwest::Error> {
        self.send(
            self.request(method("PROPFIND"), url)
                .header("Depth", if depth == Depth::Zero { "0" } else { "1" })
                .header(reqwest::header::CONTENT_TYPE, "application/xml; charset=utf-8")
                .body(PROPFIND_BODY)
                .timeout(PROPFIND_BUDGET),
        )
        .await
    }

    /// One PROPPATCH of `props` (already-escaped `<d:prop>` children, in the
    /// `d:` and `nc:` namespaces) on `url`.
    pub(crate) async fn proppatch(&self, url: Url, props: &str) -> Result<ProppatchOutcome, reqwest::Error> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<d:propertyupdate xmlns:d="DAV:" xmlns:nc="http://nextcloud.org/ns"><d:set><d:prop>{props}</d:prop></d:set></d:propertyupdate>"#
        );
        let response = self
            .send(
                self.request(method("PROPPATCH"), url)
                    .header(reqwest::header::CONTENT_TYPE, "application/xml; charset=utf-8")
                    .body(body)
                    .timeout(MUTATION_BUDGET),
            )
            .await?;
        if response.status() != StatusCode::MULTI_STATUS {
            return Ok(ProppatchOutcome::Status(response.status()));
        }
        let Some(body) = self.read_body(response, MAX_PROPPATCH_BODY).await? else {
            return Ok(ProppatchOutcome::Unreadable);
        };
        Ok(every_propstat_succeeded(&String::from_utf8_lossy(&body))
            .map_or(ProppatchOutcome::Unreadable, ProppatchOutcome::Multistatus))
    }

    /// The connect probe: PROPFIND `Depth: 0` on `root`, judged in connect
    /// terms. On a 207 it also notes whether the server takes chunked uploads:
    /// a Nextcloud names the signed-in account in `X-User-Id` on every DAV
    /// answer, which is both the tell and the id its uploads home is filed
    /// under (`chunking.rs`).
    pub(crate) async fn probe(&mut self, root_url: Url) -> Result<Vec<PropfindEntry>, WebdavConnectError> {
        debug!(
            target: "volume",
            "webdav probe: PROPFIND host={:?} path={:?}",
            LogPrivate::host(root_url.host_str().unwrap_or_default()),
            LogPrivate::dir_path(root_url.path())
        );
        let request = self
            .request(method("PROPFIND"), root_url)
            .header("Depth", "0")
            .header(reqwest::header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(PROBE_BODY)
            .timeout(REQUEST_BUDGET);
        let response = request.send().await.map_err(|e| {
            classify_connect_error(
                &e,
                self.base.host_str().unwrap_or_default(),
                self.base.port_or_known_default().unwrap_or(80),
            )
        })?;
        let status = response.status();
        match status {
            StatusCode::MULTI_STATUS => {
                let user_id = response.headers().get(USER_ID_HEADER).and_then(|v| v.to_str().ok());
                self.chunked_uploads = ChunkedUploads::detect(&self.base, user_id);
                if self.chunked_uploads.is_some() {
                    debug!(target: "volume", "webdav probe: a Nextcloud, so large files upload in chunks");
                }
                // A `Depth: 0` answer describes one resource; a body past the
                // cap isn't a WebDAV server answering it.
                let body = self
                    .read_body(response, MAX_PROBE_BODY)
                    .await
                    .map_err(|e| WebdavConnectError::Transport(e.to_string()))?
                    .ok_or(WebdavConnectError::NotAWebdavServer)?;
                parse_multistatus(&String::from_utf8_lossy(&body)).map_err(|_| WebdavConnectError::NotAWebdavServer)
            }
            StatusCode::UNAUTHORIZED => {
                if offers_basic(response.headers().get_all(WWW_AUTHENTICATE).iter()) {
                    Err(WebdavConnectError::AuthenticationRejected)
                } else {
                    Err(WebdavConnectError::AuthMethodUnsupported)
                }
            }
            StatusCode::FORBIDDEN => Err(WebdavConnectError::AuthenticationRejected),
            s if s.is_server_error() => Err(WebdavConnectError::Transport(format!("HTTP {s}"))),
            // 200 with HTML, 404 on the DAV root, 405 on PROPFIND: something
            // answered and it isn't a WebDAV endpoint at this path.
            _ => Err(WebdavConnectError::NotAWebdavServer),
        }
    }
}

/// What a PROPPATCH produced.
pub(crate) enum ProppatchOutcome {
    /// A 207 that applied every property (`true`) or refused them (`false`).
    Multistatus(bool),
    /// A 207 whose body wasn't a `multistatus`, or ran past its cap.
    Unreadable,
    /// Anything but a 207.
    Status(StatusCode),
}

/// What a PROPFIND produced.
pub(crate) enum PropfindOutcome {
    /// A parsed `multistatus`.
    Entries(Vec<PropfindEntry>),
    /// A 207 whose body wasn't a `multistatus`.
    NotMultistatus,
    /// A 207 whose body ran past `limit` bytes; the rest was never read.
    TooLarge { limit: usize },
    /// Anything but a 207.
    Status(StatusCode),
}

/// Whether any `WWW-Authenticate` challenge names the Basic scheme. The
/// scheme token is the first word of each challenge (RFC 7235 § 2.1), compared
/// case-insensitively; a Digest-only server answers `false`.
pub(crate) fn offers_basic<'a>(mut challenges: impl Iterator<Item = &'a HeaderValue>) -> bool {
    challenges.any(|value| {
        value.to_str().is_ok_and(|text| {
            text.split(',')
                .filter_map(|challenge| challenge.split_whitespace().next())
                .any(|scheme| scheme.eq_ignore_ascii_case("basic"))
        })
    })
}

#[cfg(test)]
#[path = "transport_test.rs"]
mod transport_test;

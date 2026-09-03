//! The HTTP client, and every request shape this backend sends.
//!
//! ❗ **`reqwest` is confined to this file, `errors.rs` (status codes and the
//! typed error predicates), and `streams.rs` / `writes.rs` (the two bodies
//! that stream).** Everything else works in `Url`s, status codes, and
//! [`PropfindEntry`]s, so a client swap is at most four files' problem.

use std::time::Duration;

use log::debug;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::{HeaderValue, WWW_AUTHENTICATE};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use url::Url;

use crate::errors::{WebdavConnectError, classify_connect_error};
use crate::propfind::{PropfindEntry, parse_multistatus};

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

/// Everything but the unreserved characters gets encoded in a path segment:
/// spaces, `#`, `?`, `%`, and every other reserved byte included.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

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

/// One authenticated client for one account on one server.
pub(crate) struct WebdavClient {
    http: reqwest::Client,
    base: Url,
    username: String,
    password: String,
}

impl WebdavClient {
    /// Builds the client. Redirects are off: a followed MOVE or COPY would
    /// resend its `Destination` against a URL the user never named.
    pub(crate) fn new(base: Url, username: &str, password: &str) -> Result<Self, WebdavConnectError> {
        let http = reqwest::Client::builder()
            .user_agent("Cmdr")
            .connect_timeout(REQUEST_BUDGET)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| WebdavConnectError::Transport(e.to_string()))?;
        Ok(Self {
            http,
            base,
            username: username.to_string(),
            password: password.to_string(),
        })
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
        let mut url = self.base.clone();
        let mut path = self.base.path().trim_end_matches('/').to_string();
        for segment in remote_path.split('/').filter(|s| !s.is_empty()) {
            path.push('/');
            path.push_str(&utf8_percent_encode(segment, SEGMENT).to_string());
        }
        if collection || path.is_empty() {
            path.push('/');
        }
        url.set_path(&path);
        url
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
        let body = response.text().await?;
        Ok(match parse_multistatus(&body) {
            Ok(entries) => PropfindOutcome::Entries(entries),
            Err(_) => PropfindOutcome::NotMultistatus,
        })
    }

    async fn send_propfind(&self, url: Url, depth: Depth) -> Result<Response, reqwest::Error> {
        self.request(method("PROPFIND"), url)
            .header("Depth", if depth == Depth::Zero { "0" } else { "1" })
            .header(reqwest::header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(PROPFIND_BODY)
            .timeout(PROPFIND_BUDGET)
            .send()
            .await
    }

    /// The connect probe: PROPFIND `Depth: 0` on `root`, judged in connect
    /// terms.
    pub(crate) async fn probe(&self, root_url: Url) -> Result<Vec<PropfindEntry>, WebdavConnectError> {
        debug!(target: "volume", "webdav probe: PROPFIND {}", root_url);
        let request = self
            .request(method("PROPFIND"), root_url)
            .header("Depth", "0")
            .header(reqwest::header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(PROPFIND_BODY)
            .timeout(REQUEST_BUDGET);
        let response = request.send().await.map_err(|e| classify_connect_error(&e))?;
        let status = response.status();
        match status {
            StatusCode::MULTI_STATUS => {
                let body = response
                    .text()
                    .await
                    .map_err(|e| WebdavConnectError::Transport(e.to_string()))?;
                parse_multistatus(&body).map_err(|_| WebdavConnectError::NotAWebdavServer)
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

/// What a PROPFIND produced.
pub(crate) enum PropfindOutcome {
    /// A parsed `multistatus`.
    Entries(Vec<PropfindEntry>),
    /// A 207 whose body wasn't a `multistatus`.
    NotMultistatus,
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

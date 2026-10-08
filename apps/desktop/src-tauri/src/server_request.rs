//! One request to Cmdr's own api server, and the typed answer when it doesn't land.
//!
//! Every sender that talks to `api.getcmdr.com` (the crash report, the error report and its amend,
//! the update check) fails the same handful of ways, and downstream cares about the same split for
//! all of them: did the request not get through (no network, a timeout, a server having a bad
//! moment), or did Cmdr's own server turn it down (a 4xx, a body this client can't read)? The first
//! is the person's network and logs at warn; the second means Cmdr and its server disagree, which is
//! worth an error report. The frontend reads that split off the variant
//! (`$lib/error-messages/server-request.ts`) and words each variant from the catalog.
//!
//! ❌ `detail` is for logs only: reqwest's cause chain or the server's own explanation, never a
//! sentence a person reads.
//!
//! [`send`] is also the managed-policy gate for all of them (the heartbeat, both report pipelines,
//! the amend, the update check and download, the S3 price list): each caller names its
//! [`Egress`], and a pipeline the organization turned off ends as
//! [`ServerRequestError::BlockedByPolicy`] before any byte leaves. That's the guarantee; the
//! commands above refuse earlier only to skip building a bundle. License validation, feedback, and
//! beta signup use their own clients because no key covers them (`managed_policy/DETAILS.md`).
#![cfg_attr(
    feature = "playwright-e2e",
    allow(
        dead_code,
        reason = "E2E builds compile out the report senders' network paths, which are what call these"
    )
)]

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::managed_policy::Egress;

/// Cap on the server's own explanation carried in [`ServerRequestError::Refused`], so a stray HTML
/// error page can't flood a log line.
const MAX_SERVER_DETAIL_CHARS: usize = 200;

/// Why a request to Cmdr's api server didn't come back with a usable answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ServerRequestError {
    /// The request never got an answer: no network, a DNS failure, a refused connection, TLS.
    Unreachable { detail: String },
    /// The server didn't answer within the request's time budget.
    TimedOut { detail: String },
    /// The server answered with a non-2xx status. `detail` is its own explanation, trimmed.
    Refused { status: u16, detail: String },
    /// A 2xx whose body this client can't read: Cmdr and its server disagree on the contract.
    BadResponse { detail: String },
    /// Cmdr couldn't put the request together (building the HTTP client, encoding the payload).
    Unexpected { detail: String },
    /// The organization's managed policy turns this pipeline off, so nothing was sent. Not a
    /// failure: ❌ never log it at warn or error.
    BlockedByPolicy,
}

impl std::fmt::Display for ServerRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable { detail } => write!(f, "couldn't reach the server: {detail}"),
            Self::TimedOut { detail } => write!(f, "the server didn't answer in time: {detail}"),
            Self::Refused { status, detail } if detail.is_empty() => write!(f, "the server answered {status}"),
            Self::Refused { status, detail } => write!(f, "the server answered {status}: {detail}"),
            Self::BadResponse { detail } => write!(f, "couldn't read the server's answer: {detail}"),
            Self::Unexpected { detail } => write!(f, "couldn't build the request: {detail}"),
            Self::BlockedByPolicy => f.write_str("the organization's policy turns this off"),
        }
    }
}

impl ServerRequestError {
    /// Cmdr's own side of the request broke before anything went out.
    pub fn unexpected(detail: impl std::fmt::Display) -> Self {
        Self::Unexpected {
            detail: detail.to_string(),
        }
    }

    /// A request that got no answer, either while sending it or while reading the body back.
    pub fn from_transport(err: &reqwest::Error) -> Self {
        let detail = describe_error_chain(err);
        if err.is_timeout() {
            Self::TimedOut { detail }
        } else {
            Self::Unreachable { detail }
        }
    }
}

/// Renders an error and its full `source()` chain. `reqwest::Error`'s `Display` only prints the
/// outermost layer (`error sending request for url …`), which hides the real cause (DNS lookup, TCP
/// connect timeout, TLS handshake, etc.).
pub(crate) fn describe_error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut out = err.to_string();
    let mut src = err.source();
    while let Some(cause) = src {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        src = cause.source();
    }
    out
}

/// Sends `request` for the `egress` pipeline and passes a 2xx response through. Anything else
/// becomes a [`ServerRequestError`]: [`ServerRequestError::BlockedByPolicy`] before anything leaves
/// when the managed policy (read fresh) turns the pipeline off, a transport failure, or
/// [`ServerRequestError::Refused`] carrying the server's own explanation.
///
/// ❗ This is the one gate for every api-server pipeline: a sender names its [`Egress`] here and
/// can't skip the check.
pub async fn send(egress: Egress, request: reqwest::RequestBuilder) -> Result<reqwest::Response, ServerRequestError> {
    check_policy(egress).await?;
    let response = request
        .send()
        .await
        .map_err(|e| ServerRequestError::from_transport(&e))?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    // The server's own explanation (`{"error": "..."}`) rides along for the log: a bare status once
    // hid which validation tripped for a whole release. An unreadable body just leaves it empty.
    let body = response.text().await.unwrap_or_default();
    Err(ServerRequestError::Refused {
        status: status.as_u16(),
        detail: body.trim().chars().take(MAX_SERVER_DETAIL_CHARS).collect(),
    })
}

/// [`ServerRequestError::BlockedByPolicy`] when the managed policy, read fresh, turns `egress` off.
/// [`send`] always asks; a command asks too before it builds a bundle nobody may send.
pub async fn check_policy(egress: Egress) -> Result<(), ServerRequestError> {
    if crate::managed_policy::for_egress().await.allows(egress) {
        return Ok(());
    }
    log::info!(target: "managed_policy", "Not sending {egress:?}: the organization's policy turns it off");
    Err(ServerRequestError::BlockedByPolicy)
}

/// Reads a 2xx body as JSON.
///
/// The body is read in full BEFORE parsing, so a connection that drops or stalls mid-body stays a
/// transport failure instead of passing for a malformed answer. The parse failure's `detail` is
/// serde's message alone, never the body: an upload's answer carries the amend credential.
pub async fn read_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, ServerRequestError> {
    let body = response
        .bytes()
        .await
        .map_err(|e| ServerRequestError::from_transport(&e))?;
    serde_json::from_slice(&body).map_err(|e| ServerRequestError::BadResponse { detail: e.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::testing;
    use serde_json::json;
    use std::error::Error;
    use std::fmt;
    use std::time::Duration;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(timeout: Duration) -> reqwest::Client {
        cmdr_http::client_builder()
            .timeout(timeout)
            .build()
            .expect("a plain client builds")
    }

    async fn get_json(url: &str, timeout: Duration) -> Result<serde_json::Value, ServerRequestError> {
        let response = send(Egress::UpdateCheck, client(timeout).get(url)).await?;
        read_json(response).await
    }

    async fn server_answering(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(response).mount(&server).await;
        server
    }

    #[tokio::test]
    async fn a_2xx_json_body_comes_back_parsed() {
        let server = server_answering(ResponseTemplate::new(200).set_body_json(json!({ "id": "ERR-AB23X" }))).await;
        let value = get_json(&server.uri(), Duration::from_secs(5))
            .await
            .expect("a 2xx lands");
        assert_eq!(value, json!({ "id": "ERR-AB23X" }));
    }

    #[tokio::test]
    async fn a_non_2xx_is_refused_with_the_status_and_the_servers_own_words() {
        let server =
            server_answering(ResponseTemplate::new(422).set_body_json(json!({ "error": "userNote must be a string" })))
                .await;
        let err = get_json(&server.uri(), Duration::from_secs(5))
            .await
            .expect_err("a 422 is a refusal");
        let ServerRequestError::Refused { status, detail } = err else {
            panic!("expected Refused, got {err:?}");
        };
        assert_eq!(status, 422);
        assert_eq!(detail, r#"{"error":"userNote must be a string"}"#);
    }

    #[tokio::test]
    async fn a_refusal_trims_a_long_explanation() {
        let server = server_answering(ResponseTemplate::new(503).set_body_string("x".repeat(5_000))).await;
        let err = get_json(&server.uri(), Duration::from_secs(5))
            .await
            .expect_err("a 503 is a refusal");
        let ServerRequestError::Refused { status: 503, detail } = err else {
            panic!("expected a 503 Refused, got {err:?}");
        };
        assert_eq!(detail.chars().count(), MAX_SERVER_DETAIL_CHARS);
    }

    #[tokio::test]
    async fn a_2xx_that_isnt_json_is_a_bad_response() {
        let server = server_answering(ResponseTemplate::new(200).set_body_string("<html>maintenance</html>")).await;
        let err = get_json(&server.uri(), Duration::from_secs(5))
            .await
            .expect_err("HTML isn't the JSON this client expects");
        assert!(
            matches!(err, ServerRequestError::BadResponse { .. }),
            "expected BadResponse, got {err:?}"
        );
    }

    #[tokio::test]
    async fn a_port_nobody_listens_on_is_unreachable() {
        // Bind to learn a free port, then close it, so the connect is refused outright.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("bind loopback")
            .local_addr()
            .expect("a bound socket has an address")
            .port();
        let err = get_json(&format!("http://127.0.0.1:{port}/"), Duration::from_secs(5))
            .await
            .expect_err("nothing listens there");
        assert!(
            matches!(err, ServerRequestError::Unreachable { .. }),
            "expected Unreachable, got {err:?}"
        );
    }

    #[tokio::test]
    async fn a_server_slower_than_the_budget_has_timed_out() {
        let server = server_answering(ResponseTemplate::new(200).set_delay(Duration::from_secs(5))).await;
        let err = get_json(&server.uri(), Duration::from_millis(100))
            .await
            .expect_err("the answer comes after the budget");
        assert!(
            matches!(err, ServerRequestError::TimedOut { .. }),
            "expected TimedOut, got {err:?}"
        );
    }

    /// Every gated pipeline, with the managed key that turns it off.
    const GATED: [(Egress, &str); 6] = [
        (Egress::Heartbeat, testing::DISABLE_USAGE_STATS),
        (Egress::CrashReport, testing::DISABLE_CRASH_AND_ERROR_REPORTS),
        (Egress::ErrorReport, testing::DISABLE_CRASH_AND_ERROR_REPORTS),
        (Egress::ErrorReportAmend, testing::DISABLE_CRASH_AND_ERROR_REPORTS),
        (Egress::UpdateCheck, testing::DISABLE_UPDATES),
        (Egress::UpdateDownload, testing::DISABLE_UPDATES),
    ];

    async fn requests_reaching(server: &MockServer) -> usize {
        server.received_requests().await.map_or(0, |requests| requests.len())
    }

    #[tokio::test]
    async fn a_pipeline_the_policy_turns_off_sends_nothing() {
        for (egress, key) in GATED {
            let server = server_answering(ResponseTemplate::new(200)).await;
            let _policy = testing::override_for_test(testing::forcing(&[key]));

            let result = send(egress, client(Duration::from_secs(5)).get(server.uri())).await;

            assert_eq!(
                result.map(|_| ()),
                Err(ServerRequestError::BlockedByPolicy),
                "{egress:?} under {key}"
            );
            assert_eq!(
                requests_reaching(&server).await,
                0,
                "{egress:?} under {key} reached the server"
            );
        }
    }

    #[tokio::test]
    async fn a_pipeline_is_untouched_by_the_keys_that_dont_cover_it() {
        for (egress, key) in GATED {
            let others: Vec<&str> = testing::ALL_KEYS.into_iter().filter(|k| *k != key).collect();
            // `DisableUpdates` and `MaxUpdateVersion` (unparseable, so updates off) cover the
            // same pipelines; leave both out for an update pipeline.
            let others: Vec<&str> = if matches!(egress, Egress::UpdateCheck | Egress::UpdateDownload) {
                others
                    .into_iter()
                    .filter(|k| *k != testing::MAX_UPDATE_VERSION)
                    .collect()
            } else {
                others
            };
            let server = server_answering(ResponseTemplate::new(200)).await;
            let _policy = testing::override_for_test(testing::forcing(&others));

            send(egress, client(Duration::from_secs(5)).get(server.uri()))
                .await
                .unwrap_or_else(|e| panic!("{egress:?} should send under {others:?}: {e:?}"));
            assert_eq!(requests_reaching(&server).await, 1, "{egress:?}");
        }
    }

    #[tokio::test]
    async fn the_s3_price_list_sends_under_every_key() {
        let server = server_answering(ResponseTemplate::new(200)).await;
        let _policy = testing::override_for_test(testing::forcing(&testing::ALL_KEYS));

        send(Egress::S3PriceList, client(Duration::from_secs(5)).get(server.uri()))
            .await
            .expect("no key turns the price list off");
        assert_eq!(requests_reaching(&server).await, 1);
    }

    #[test]
    fn a_policy_refusal_has_no_detail_on_the_wire() {
        let value = serde_json::to_value(ServerRequestError::BlockedByPolicy).expect("serializes");
        assert_eq!(value, json!({ "type": "blockedByPolicy" }));
    }

    /// The frontend switches on `type` and reads `status` as a number; the shape is the contract.
    #[test]
    fn the_wire_shape_is_internally_tagged_camel_case() {
        let value = serde_json::to_value(ServerRequestError::Refused {
            status: 404,
            detail: "not found".to_string(),
        })
        .expect("serializes");
        assert_eq!(
            value,
            json!({ "type": "refused", "status": 404, "detail": "not found" })
        );
        let value = serde_json::to_value(ServerRequestError::TimedOut {
            detail: "deadline".to_string(),
        })
        .expect("serializes");
        assert_eq!(value, json!({ "type": "timedOut", "detail": "deadline" }));
    }

    #[derive(Debug)]
    struct ChainErr {
        msg: &'static str,
        source: Option<Box<dyn Error + 'static>>,
    }

    impl fmt::Display for ChainErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.msg)
        }
    }

    impl Error for ChainErr {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_deref()
        }
    }

    #[test]
    fn describe_error_chain_renders_only_outer_when_no_source() {
        let err = ChainErr {
            msg: "outer",
            source: None,
        };
        assert_eq!(describe_error_chain(&err), "outer");
    }

    #[test]
    fn describe_error_chain_walks_full_source_chain() {
        let inner = ChainErr {
            msg: "io broken pipe",
            source: None,
        };
        let middle = ChainErr {
            msg: "hyper transport",
            source: Some(Box::new(inner)),
        };
        let outer = ChainErr {
            msg: "reqwest send",
            source: Some(Box::new(middle)),
        };
        assert_eq!(
            describe_error_chain(&outer),
            "reqwest send: hyper transport: io broken pipe"
        );
    }

    /// Sanity-check against an actual `reqwest::Error` for a name that can never resolve
    /// (`.invalid` TLD per RFC 6761). `#[ignore]`'d because it depends on the local resolver
    /// Run with
    /// `cargo nextest run -p cmdr describe_error_chain --run-ignored=ignored-only --no-capture`
    /// to see what reqwest 0.13's source() chain actually surfaces. The `eprintln!` is
    /// allowed locally because the whole point of these tests is to render the chain into
    /// stderr for human inspection; they're verification harnesses, not production code.
    #[tokio::test]
    #[ignore = "network-dependent; run manually to verify reqwest chain content"]
    #[allow(clippy::print_stderr, reason = "verification harness; see fn doc")]
    async fn describe_error_chain_unwraps_reqwest_dns_failure() {
        let err = cmdr_http::client_builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .get("http://nonexistent-host-for-cmdr-tests.invalid/")
            .send()
            .await
            .expect_err("request to .invalid should fail");
        let msg = describe_error_chain(&err);
        eprintln!("DNS-failure chain: {msg}");
        assert!(msg.len() > 60, "chain too short, source() likely empty: {msg}");
    }

    /// Sanity-check against an actual connect timeout (RFC 5737 unreachable address).
    /// `#[ignore]` for the same reason as the DNS test.
    #[tokio::test]
    #[ignore = "network-dependent; run manually to verify reqwest chain content"]
    #[allow(clippy::print_stderr, reason = "verification harness; see fn doc")]
    async fn describe_error_chain_unwraps_reqwest_connect_timeout() {
        let err = cmdr_http::client_builder()
            .connect_timeout(Duration::from_millis(500))
            .build()
            .unwrap()
            .get("http://10.255.255.1/")
            .send()
            .await
            .expect_err("connect to 10.255.255.1 should time out");
        let msg = describe_error_chain(&err);
        eprintln!("connect-timeout chain: {msg}");
        // reqwest 0.13 wording, captured from a one-shot run on macOS:
        //   error sending request for url (http://10.255.255.1/): client error (Connect): tcp connect error: deadline has elapsed
        // Match on the "tcp connect" cause rather than a "timeout" keyword; reqwest words it
        // as "deadline has elapsed", not "timeout".
        // `#[ignore]` verification harness pinning reqwest 0.13's `source()` chain wording. Not classification; production renders the chain into log strings, never branches on the words. Manual run only.
        let chain = msg.to_lowercase();
        let matched = ["tcp connect", "deadline", "timed out"]
            .iter()
            .any(|needle| chain.contains(needle));
        assert!(matched, "expected connect/deadline-shaped cause in chain: {msg}");
    }
}

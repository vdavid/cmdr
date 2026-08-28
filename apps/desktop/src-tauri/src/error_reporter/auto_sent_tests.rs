//! Unit tests for the auto-sent stash and the amend request: what the stash keeps, what it
//! refuses to hand out, the two ways an amend gives up before it touches the network, and the
//! shape of the request when it doesn't.
//!
//! The network tests point [`auto_sent::amend`] at a `wiremock` server, which is what the URL
//! parameter is for. They matter more than they look: this is the one request that carries a
//! person's email address, and it carries a credential in a field name only this code sets.
//!
//! The stash is process-global, so every test here holds `auto_sent::TEST_LOCK`.

use super::auto_sent::{self, AutoSentPreview};
use super::{AmendKey, BuildMode, BundleKind, BundleManifest, LogLevelSnapshot, ResolvedSettings};
use serde_json::{Value, json};
use std::future::Future;
use std::sync::MutexGuard;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn locked() -> MutexGuard<'static, ()> {
    let guard = match auto_sent::TEST_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    auto_sent::clear_for_test();
    guard
}

fn preview(id: &str) -> AutoSentPreview {
    AutoSentPreview {
        size_bytes: 4_096,
        manifest: manifest(id),
        sample_first: vec!["oldest kept line".to_string()],
        sample_last: vec!["newest kept line".to_string()],
        total_redacted_lines: 42,
    }
}

fn manifest(id: &str) -> BundleManifest {
    BundleManifest {
        id: id.to_string(),
        kind: BundleKind::Auto,
        build_mode: BuildMode::Release,
        app_version: "0.0.0-test".to_string(),
        os_version: "test-os".to_string(),
        arch: "test-arch".to_string(),
        active_settings: ResolvedSettings {
            indexing_enabled: true,
            ai_provider: "none".to_string(),
            mcp_enabled: false,
            mcp_port: 0,
            verbose_logging: false,
            max_log_storage_mb: 100,
            error_reports_enabled: true,
            crash_reports_enabled: true,
        },
        log_levels: LogLevelSnapshot {
            stdout_default: "info".to_string(),
            stdout_current: "info".to_string(),
            file_chain: "debug".to_string(),
            stdout_module_overrides: Vec::new(),
        },
        breadcrumbs: Vec::new(),
        user_note: None,
        diag_id: "diag_test".to_string(),
        email: None,
        system: crate::diagnostics_snapshot::SystemSnapshot::collect_full(std::path::Path::new("")),
        generated_at: "2026-08-28T00:00:00Z".to_string(),
    }
}

#[test]
fn nothing_is_stashed_before_an_auto_send() {
    let _lock = locked();
    assert!(auto_sent::snapshot().is_none());
}

#[test]
fn a_stashed_report_reports_its_preview_and_that_it_can_take_a_note() {
    let _lock = locked();
    auto_sent::record(
        "ERR-AB23X".to_string(),
        Some(AmendKey::for_test("key-one")),
        preview("ERR-AB23X"),
    );

    let snapshot = auto_sent::snapshot().expect("a report was just stashed");
    assert_eq!(snapshot.id, "ERR-AB23X");
    assert!(snapshot.can_amend);
    assert_eq!(snapshot.preview.size_bytes, 4_096);
    assert_eq!(snapshot.preview.total_redacted_lines, 42);
    assert_eq!(snapshot.preview.manifest.id, "ERR-AB23X");
    assert_eq!(snapshot.preview.sample_first, vec!["oldest kept line".to_string()]);
    assert_eq!(snapshot.preview.sample_last, vec!["newest kept line".to_string()]);
}

#[test]
fn a_report_the_server_gave_no_key_for_says_it_cannot_take_a_note() {
    let _lock = locked();
    auto_sent::record("ERR-AB23X".to_string(), None, preview("ERR-AB23X"));

    let snapshot = auto_sent::snapshot().expect("a report was just stashed");
    assert!(
        !snapshot.can_amend,
        "no credential means the UI must not offer to add a note"
    );
}

#[test]
fn a_second_auto_send_replaces_the_first() {
    let _lock = locked();
    auto_sent::record(
        "ERR-AB23X".to_string(),
        Some(AmendKey::for_test("key-one")),
        preview("ERR-AB23X"),
    );
    auto_sent::record("ERR-ZZ99Y".to_string(), None, preview("ERR-ZZ99Y"));

    let snapshot = auto_sent::snapshot().expect("the second report is stashed");
    assert_eq!(snapshot.id, "ERR-ZZ99Y", "the toast is deduped to one, so is the stash");
    assert!(!snapshot.can_amend, "the replacement's own key state, not the first's");
}

#[test]
fn a_snapshot_cannot_carry_the_credential_anywhere() {
    let _lock = locked();
    auto_sent::record(
        "ERR-AB23X".to_string(),
        Some(AmendKey::for_test("s3cret-amend-key")),
        preview("ERR-AB23X"),
    );

    // The snapshot type has no field for it, so this is really a guard on the whole
    // downstream path (IPC payload, log lines) staying that way.
    let printed = format!("{:?}", auto_sent::snapshot().expect("stashed"));
    assert!(
        !printed.contains("s3cret"),
        "the credential must not be reachable through a snapshot: {printed}",
    );
}

/// Drives an async body to completion on a private runtime.
///
/// `#[tokio::test]` would mean holding `TEST_LOCK` across an `.await` (clippy's
/// `await_holding_lock`, and a real hazard with a `std::sync::Mutex`). Blocking on the whole
/// body instead keeps the guard honest. Multi-threaded with IO enabled because the network
/// tests below run a `wiremock` server and a `reqwest` client against each other.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a tokio runtime always builds")
        .block_on(future)
}

/// A stash holding one amendable report, and a `wiremock` server standing in for the api server.
async fn stashed_report_and_server(key: &str) -> MockServer {
    auto_sent::record(
        "ERR-AB23X".to_string(),
        Some(AmendKey::for_test(key)),
        preview("ERR-AB23X"),
    );
    MockServer::start().await
}

/// The URL the command layer would build, against the mock instead of the api server.
fn amend_url(server: &MockServer, id: &str) -> String {
    format!("{}/error-report/{id}/amend", server.uri())
}

fn mount_amend(server: &MockServer, response: ResponseTemplate) -> impl Future<Output = ()> {
    Mock::given(method("POST"))
        .and(path("/error-report/ERR-AB23X/amend"))
        .respond_with(response)
        .mount(server)
}

async fn only_request(server: &MockServer) -> Request {
    let mut requests = server.received_requests().await.expect("wiremock records requests");
    assert_eq!(requests.len(), 1, "expected exactly one request");
    requests.remove(0)
}

#[test]
fn an_amend_puts_the_credential_the_note_and_the_address_in_the_body() {
    let _lock = locked();
    block_on(async {
        let server = stashed_report_and_server("s3cret-amend-key").await;
        mount_amend(&server, ResponseTemplate::new(200).set_body_json(json!({ "ok": true }))).await;

        let target = auto_sent::amend_target().expect("a stashed report with a credential");
        let url = amend_url(&server, &target.id);
        let id = auto_sent::amend(
            target,
            &url,
            Some("it happened while copying to my NAS".to_string()),
            super::AttachedEmail::from_flow_a_dialog(Some("tester@example.com".to_string())),
        )
        .await
        .expect("the amend lands");

        assert_eq!(id, "ERR-AB23X", "the caller gets back the report it amended");

        let request = only_request(&server).await;
        let body: Value = serde_json::from_slice(&request.body).expect("a JSON body");
        assert_eq!(body["amendKey"], "s3cret-amend-key", "the credential rides `amendKey`");
        assert_eq!(body["note"], "it happened while copying to my NAS");
        assert_eq!(body["email"], "tester@example.com");
        let content_type = request
            .headers
            .get("content-type")
            .expect("a content-type")
            .to_str()
            .expect("an ASCII content-type");
        assert!(content_type.starts_with("application/json"), "got {content_type}");
    });
}

#[test]
fn an_amend_omits_a_note_and_an_address_the_user_did_not_give() {
    let _lock = locked();
    block_on(async {
        let server = stashed_report_and_server("key-one").await;
        mount_amend(&server, ResponseTemplate::new(200)).await;

        let target = auto_sent::amend_target().expect("a stashed report with a credential");
        let url = amend_url(&server, &target.id);
        auto_sent::amend(target, &url, None, None)
            .await
            .expect("the amend lands");

        let body: Value = serde_json::from_slice(&only_request(&server).await.body).expect("a JSON body");
        assert!(body.get("note").is_none(), "an absent note is absent, not null: {body}");
        assert!(
            body.get("email").is_none(),
            "an absent address is absent, not null: {body}"
        );
    });
}

#[test]
fn a_rejected_amend_folds_the_servers_own_explanation_into_the_message() {
    let _lock = locked();
    block_on(async {
        let server = stashed_report_and_server("key-one").await;
        mount_amend(
            &server,
            ResponseTemplate::new(403).set_body_json(json!({ "error": "amend key does not match" })),
        )
        .await;

        let target = auto_sent::amend_target().expect("a stashed report with a credential");
        let url = amend_url(&server, &target.id);
        let message = auto_sent::amend(target, &url, Some("a note".to_string()), None)
            .await
            .expect_err("a 403 is an error");

        // A bare status code once hid a payload bug for a whole release; `upload` folds the
        // server's own words in for the same reason.
        assert!(
            message.contains("amend key does not match"),
            "the server's explanation must survive into the message: {message}",
        );
        assert!(message.contains("403"), "and so must the status: {message}");
    });
}

#[test]
fn a_rejected_amend_with_nothing_to_say_reports_the_status_alone() {
    let _lock = locked();
    block_on(async {
        let server = stashed_report_and_server("key-one").await;
        mount_amend(&server, ResponseTemplate::new(500)).await;

        let target = auto_sent::amend_target().expect("a stashed report with a credential");
        let url = amend_url(&server, &target.id);
        let message = auto_sent::amend(target, &url, Some("a note".to_string()), None)
            .await
            .expect_err("a 500 is an error");

        assert!(message.contains("500"), "got {message}");
    });
}

#[test]
fn a_second_amend_reuses_the_same_credential_and_is_an_ordinary_request() {
    let _lock = locked();
    block_on(async {
        let server = stashed_report_and_server("key-one").await;
        mount_amend(&server, ResponseTemplate::new(200)).await;

        for note in ["first thought", "second thought"] {
            let target = auto_sent::amend_target().expect("the credential is still there");
            let url = amend_url(&server, &target.id);
            auto_sent::amend(target, &url, Some(note.to_string()), None)
                .await
                .expect("both amends land");
        }

        // Amendments accumulate server-side, so nothing here is special-cased: two plain POSTs,
        // same credential, and the report is still amendable afterwards.
        let requests = server.received_requests().await.expect("wiremock records requests");
        assert_eq!(requests.len(), 2, "two amends, two requests");
        for request in &requests {
            let body: Value = serde_json::from_slice(&request.body).expect("a JSON body");
            assert_eq!(body["amendKey"], "key-one");
        }
        assert!(
            auto_sent::snapshot().expect("still stashed").can_amend,
            "a landed amend must not close the door on the next one",
        );
    });
}

#[test]
fn amending_with_nothing_auto_sent_says_so_without_calling_out() {
    let _lock = locked();
    assert!(auto_sent::amend_target().is_err(), "no stash, nothing to amend");
}

#[test]
fn amending_a_report_with_no_credential_says_so_without_calling_out() {
    let _lock = locked();
    auto_sent::record("ERR-AB23X".to_string(), None, preview("ERR-AB23X"));

    assert!(
        auto_sent::amend_target().is_err(),
        "no credential, so there's no request to make"
    );
}

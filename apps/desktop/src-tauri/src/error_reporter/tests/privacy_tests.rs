//! Archive-level privacy contract shared by the streaming and legacy ZIP pipelines.

use super::{read_zip_entries, sample_manifest};
use crate::error_reporter::bundle_builder::{build_bundle_legacy_window, build_bundle_streaming};
use crate::error_reporter::{BuiltBundle, BundleManifest, BundleScope, breadcrumbs, state_history};
use crate::redact;
use crate::test_support::TestDir;
use chrono::{DateTime, Utc};
use std::time::SystemTime;

const PRIVACY_REPORT_ID: &str = "ERR-AB23X";
const EXPLICIT_NOTE: &str = "EXPLICIT-NOTE-SENTINEL: I consent to share /Users/explicit-consent/Exact note.txt";
const EXPLICIT_EMAIL: &str = "explicit-email-sentinel@example.test";
const PRIVATE_LOG_PATH: &str = "/Users/private-account/Plans/client secret";
/// Tool output repeats the line's keyed host bare, next to a path and an address. The report
/// keeps the sentence and the NT status but not the identities, which the local log (and this
/// fixture) carry whole.
const EXTERNAL_STDERR: &str = "do_connect: Connection to PRIVATE-REMOTE-HOST failed (Error NT_STATUS_BAD_NETWORK_NAME) reading \"/Users/private-account/Plans/client secret.txt\" via 10.9.8.7";
const EXTERNAL_STDOUT_TAIL: &str = "Sharename listing ended early. ";
const LEGACY_BREADCRUMB_SENTINEL: &str = "PRIVATE-LEGACY-BREADCRUMB-SENTINEL";
fn privacy_manifest(redaction: &redact::RedactionContext) -> BundleManifest {
    let mut manifest = sample_manifest();
    manifest.id = PRIVACY_REPORT_ID.to_string();
    manifest.breadcrumbs = vec![breadcrumbs::Breadcrumb {
        at: "2027-01-15T08:00:00+00:00".to_string(),
        event: breadcrumbs::BreadcrumbEvent::Command {
            command_id: "pane.switch".to_string(),
        },
    }];
    manifest.state_history = state_history::privacy_fixture_for_test(redaction);
    manifest.user_note = Some(EXPLICIT_NOTE.to_string());
    manifest.email = Some(EXPLICIT_EMAIL.to_string());
    manifest
}

fn timestamp(now: DateTime<Utc>) -> String {
    now.with_timezone(&chrono::Local)
        .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
        .to_string()
}

fn current_privacy_log(now: DateTime<Utc>) -> String {
    let stamp = now.with_timezone(&chrono::Local).format("%Y-%m-%dT%H:%M:%S%.3f%:z");
    format!(
        "{stamp} INFO path={path:?}\n\
         {stamp} INFO smb://PRIVATE-REMOTE-USER:PRIVATE-REMOTE-PASSWORD@PRIVATE-REMOTE-HOST.local/PRIVATE-REMOTE-SHARE/{name}?PRIVATE-QUERY-KEY=PRIVATE-QUERY-SECRET#PRIVATE-FRAGMENT-SECRET\n\
         {stamp} INFO server=Some(\"PRIVATE-SERVER-IDENTITY\") user=PRIVATE-ACCOUNT-IDENTITY\n",
        path = PRIVATE_LOG_PATH,
        name = state_history::PRIVACY_TEST_RAW_NAME,
    )
}

/// Ordinary current records around a multi-line backtrace, in both the active and the
/// rotated file, so each pipeline has to carry typed diagnostics and continuations through.
fn typed_log(now: DateTime<Utc>, suffix: &str) -> String {
    let before = timestamp(now - chrono::Duration::seconds(5));
    let backtrace = timestamp(now - chrono::Duration::seconds(1));
    let after = timestamp(now);
    format!(
        "{before} INFO  privacy_test  SAFE-BEFORE-{suffix}\n\
         {backtrace} DEBUG ·error_reporter::backtrace  Backtrace for retained typed diagnostic:\n\
            0: cmdr_lib::privacy_test::retained_frame_{suffix}\n\
            1: std::panicking::try\n\
         {after} WARN  cmdr_smb::volume::session  SmbVolume::read(share=\"PRIVATE-REMOTE-SHARE\"): backend=smb2, error_kind=ConnectionLost\n\
         {after} DEBUG ·network::discovery_cache  Host ADDED: serverId=\"PRIVATE-CLIENT-ID-{suffix}\", server=\"PRIVATE-CLIENT-NAME-{suffix}\"\n\
         {after} WARN  network::smb_smbclient  smbclient share listing stopped: host=\"PRIVATE-REMOTE-HOST\", port=445, code=Some(1), nt_status=NT_STATUS_BAD_NETWORK_NAME, stderr={stderr:?}, stdout={stdout:?}\n\
         {after} INFO  privacy_test  SAFE-AFTER-{suffix}\n",
        stderr = cmdr_fs::log_detail::LogDetail(EXTERNAL_STDERR),
        stdout = cmdr_fs::log_detail::LogDetail(&EXTERNAL_STDOUT_TAIL.repeat(40)),
    )
}

fn assert_privacy_archive(bundle: &BuiltBundle) -> BundleManifest {
    let entries = read_zip_entries(&bundle.zip_bytes);
    let archive_text = entries
        .iter()
        .map(|(name, body)| format!("{name}\n{body}"))
        .collect::<Vec<_>>()
        .join("\n");
    for private in [
        "private-account",
        "test_user_4-smith",
        "client secret",
        " secret",
        PRIVATE_LOG_PATH,
        state_history::PRIVACY_TEST_RAW_NAME,
        state_history::PRIVACY_TEST_RAW_PATH,
        state_history::PRIVACY_TEST_EXTERNAL_PROSE,
        "PRIVATE-VOLUME-NAME-SENTINEL",
        "PRIVATE-REMOTE-USER",
        "PRIVATE-REMOTE-PASSWORD",
        "PRIVATE-REMOTE-HOST",
        "PRIVATE-REMOTE-SHARE",
        "PRIVATE-QUERY-KEY",
        "PRIVATE-QUERY-SECRET",
        "PRIVATE-FRAGMENT-SECRET",
        "PRIVATE-SERVER-IDENTITY",
        "PRIVATE-ACCOUNT-IDENTITY",
        "PRIVATE-FREE-FORM-PAYLOAD-SENTINEL",
        "PRIVATE-CLIENT-ID",
        "PRIVATE-CLIENT-NAME",
        LEGACY_BREADCRUMB_SENTINEL,
    ] {
        assert!(
            !archive_text.contains(private),
            "private sentinel {private:?} survived:\n{archive_text}"
        );
    }

    // These two fields are deliberately outside the privacy transform: the person previewed
    // them and explicitly attached them to this report.
    assert!(archive_text.contains(EXPLICIT_NOTE));
    assert!(archive_text.contains(EXPLICIT_EMAIL));

    let manifest: BundleManifest = serde_json::from_str(entries.get("manifest.json").expect("manifest entry")).unwrap();
    assert_eq!(manifest.user_note.as_deref(), Some(EXPLICIT_NOTE));
    assert_eq!(manifest.email.as_deref(), Some(EXPLICIT_EMAIL));
    assert_eq!(manifest.breadcrumbs.len(), 1);

    for file_name in ["logs/cmdr.log", "logs/cmdr.log.1"] {
        let log = entries.get(file_name).unwrap_or_else(|| panic!("missing {file_name}"));
        let suffix = if file_name.ends_with(".1") { "ROTATED" } else { "ACTIVE" };
        for retained in [
            format!("SAFE-BEFORE-{suffix}"),
            format!("cmdr_lib::privacy_test::retained_frame_{suffix}"),
            "std::panicking::try".to_string(),
            "backend=smb2, error_kind=ConnectionLost".to_string(),
            "code=Some(1), nt_status=NT_STATUS_BAD_NETWORK_NAME, stderr=\"do_connect: Connection to <".to_string(),
            "failed (Error NT_STATUS_BAD_NETWORK_NAME) reading".to_string(),
            "stdout=\"Sharename listing ended early.".to_string(),
            "·network::discovery_cache  Host ADDED: serverId=\"<server-id:".to_string(),
            format!("SAFE-AFTER-{suffix}"),
        ] {
            assert!(
                log.contains(&retained),
                "safe record {retained:?} missing from {file_name}: {log}"
            );
        }
    }

    for file_name in ["logs/cmdr.log", "logs/cmdr.log.1"] {
        let log = entries.get(file_name).unwrap_or_else(|| panic!("missing {file_name}"));
        let line = log
            .lines()
            .find(|line| line.contains("smbclient share listing stopped"))
            .expect("external-text line survives");
        assert!(!line.contains("10.9.8.7"), "address inside stderr survived: {line}");
        let stdout = line.split("stdout=\"").nth(1).expect("stdout field");
        let value_chars = stdout.trim_end_matches('"').chars().count();
        assert!(
            value_chars <= redact::REPORT_DETAIL_MAX_CHARS,
            "stdout must be capped in the report, got a {value_chars}-char value"
        );
    }

    let state = manifest.state_history.first().expect("typed state history");
    assert_eq!(state.generation, 73);
    assert_eq!(state.focused, Some(state_history::PaneSide::Right));
    assert!(state.show_hidden);
    assert_eq!(state.recent_listing_error_count, 5);
    let volume = state.volumes.first().expect("volume state");
    assert_eq!(volume.kind, Some(state_history::ReportVolumeKind::Smb));
    assert_eq!(
        volume.connection,
        Some(state_history::ReportConnectionState::NeedsSignIn)
    );
    let failure = state.recent_listing_failures.first().expect("typed listing failure");
    assert_eq!(failure.reason.as_deref(), Some("permissionDenied"));
    assert_eq!(failure.volume_id, volume.volume_id, "one volume, one token");
    let pane = state.panes.first().expect("pane state");
    assert_eq!(pane.backend, Some(state_history::ReportBackend::Smb));
    assert_eq!(pane.connection, Some(state_history::ReportConnectionState::NeedsSignIn));
    assert_eq!(pane.selected_files, 1);
    assert_eq!(pane.selected_folders, 1);
    assert_eq!(pane.tab_count, 1);
    let cursor = pane.cursor.as_ref().expect("cursor identity");
    let operation = state.operations.first().expect("operation state");
    assert_eq!(
        operation.operation_id.as_deref(),
        Some("0199a2e7-47d8-7c31-a897-c58f415f4f91")
    );
    assert_eq!(operation.files_done, 7);
    assert_eq!(operation.files_total, 19);
    assert_eq!(operation.bytes_done, 11);
    assert_eq!(operation.bytes_total, 23);
    assert_eq!(operation.current_file.as_deref(), Some(cursor.name.as_str()));

    let log = entries.get("logs/cmdr.log").expect("redacted log entry");
    assert!(
        log.contains(&cursor.name),
        "the same raw name must correlate between typed state and logs: {log}"
    );
    manifest
}

#[test]
fn both_zip_pipelines_apply_one_report_context_to_every_diagnostic_surface() {
    let legacy_payload = serde_json::json!({
        "kind": "command",
        "message": LEGACY_BREADCRUMB_SENTINEL,
        "ctx": { "freeForm": "PRIVATE-FREE-FORM-PAYLOAD-SENTINEL" }
    });
    assert!(
        serde_json::from_value::<breadcrumbs::BreadcrumbEvent>(legacy_payload).is_err(),
        "the legacy free-form shape must not be able to enter either manifest"
    );

    let dir = TestDir::new("error-reporter-privacy-archives");
    let log = dir.join("cmdr.log");
    let rotated_log = dir.join("cmdr.log.1");
    let now = Utc::now();
    std::fs::write(
        &log,
        format!("{}{}", current_privacy_log(now), typed_log(now, "ACTIVE")),
    )
    .expect("write adversarial active log");
    std::fs::write(&rotated_log, typed_log(now, "ROTATED")).expect("write adversarial rotated log");

    // Preview/send rebuilds construct the context again from the same report ID. Drive each
    // production ZIP pipeline with a separately-created context to pin that reuse contract.
    let streaming_context = redact::RedactionContext::for_report(PRIVACY_REPORT_ID);
    let streaming = build_bundle_streaming(
        PRIVACY_REPORT_ID.to_string(),
        privacy_manifest(&streaming_context),
        vec![log.clone(), rotated_log.clone()],
        now - chrono::Duration::hours(1),
        SystemTime::now(),
        &streaming_context,
    )
    .expect("streaming privacy bundle");

    let legacy_context = redact::RedactionContext::for_report(PRIVACY_REPORT_ID);
    let legacy = build_bundle_legacy_window(
        PRIVACY_REPORT_ID.to_string(),
        privacy_manifest(&legacy_context),
        vec![log, rotated_log],
        BundleScope::Window { first_error_at: now },
        now,
        SystemTime::now(),
        &legacy_context,
    )
    .expect("legacy privacy bundle");

    let streaming_manifest = assert_privacy_archive(&streaming);
    let legacy_manifest = assert_privacy_archive(&legacy);
    assert_eq!(
        streaming_manifest.state_history[0].panes[0].path, legacy_manifest.state_history[0].panes[0].path,
        "one report ID must reproduce one correlation context across rebuilds"
    );

    let other_context = redact::RedactionContext::for_report("ERR-CD45Y");
    let other_state = state_history::privacy_fixture_for_test(&other_context);
    assert_ne!(
        streaming_manifest.state_history[0].panes[0].path, other_state[0].panes[0].path,
        "a different report must not reuse the first report's tokens"
    );
}

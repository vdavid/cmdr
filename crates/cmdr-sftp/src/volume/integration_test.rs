//! What only a real server can answer: the auth ladder, the trust decisions
//! against real host keys, the reading surface, and the two crate hazards.
//!
//! ❗ **Every `#[ignore]`d test in this crate is a Docker cell**, by
//! construction: the integration lane runs `--run-ignored only` over the whole
//! package, so anything ignored here runs in CI whatever it's called. A
//! measurement that must NOT gate CI needs its own env gate rather than an
//! `#[ignore]`.
//!
//! The servers, the ports, and what each one is for:
//! `apps/desktop/test/sftp-servers/README.md`.

use std::path::Path;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::host_keys::InMemoryHostKeys;
use cmdr_fs::volume::{Volume, VolumeError};

use super::testing::*;
use super::{SftpConnectOutcome, connect_sftp_volume};
use crate::transport::HostKeyPromptKind;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

// ── The four auth rungs ──────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_password_from_the_store_signs_in_and_lists() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    let entries = volume.list_directory(Path::new("."), None).await.expect(FIXTURE);
    assert!(
        entries.iter().any(|e| e.name == "hello.txt"),
        "the fixture's landmark file must be in the root listing, found {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(entries.iter().any(|e| e.name == "photos" && e.is_directory));
    // `.` and `..` are protocol entries, never pane entries.
    assert!(!entries.iter().any(|e| e.name == "." || e.name == ".."));
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn an_unencrypted_key_file_signs_in_where_a_password_is_refused() {
    // `sftp-fixture-keyonly` sets `PasswordAuthentication no`, so the ladder has
    // to reach its key rung to get anywhere.
    let params = fixture_params("KEYONLY", 12481).with_key_file(fixture_key_path("keyonly"));
    let host = fixture_host(&params, None);
    let volume = connect_fixture(&host, params).await;

    assert!(volume.exists(Path::new("hello.txt")).await);
    assert_eq!(
        volume.auth_rung(),
        crate::auth::AuthRungUsed::KeyFile {
            passphrase_protected: false
        }
    );
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_passphrase_protected_key_unlocks_from_the_secret_store() {
    // ❗ And the rung it records is what says this session can't rebuild itself
    // unattended: the passphrase dies with the session it unlocked.
    let params = fixture_params("PASSPHRASE", 12482).with_key_file(fixture_key_path("passphrase"));
    let host = fixture_host(&params, Some(FIXTURE_KEY_PASSPHRASE));
    let volume = connect_fixture(&host, params).await;

    assert!(volume.exists(Path::new("hello.txt")).await);
    let rung = volume.auth_rung();
    assert_eq!(
        rung,
        crate::auth::AuthRungUsed::KeyFile {
            passphrase_protected: true
        }
    );
    assert_eq!(
        crate::auth::reconnect_policy(rung),
        crate::auth::ReconnectPolicy::NeedsCredentials
    );
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_single_prompt_keyboard_interactive_server_signs_in() {
    // `PasswordAuthentication no` + `KbdInteractiveAuthentication yes` is what a
    // hardened server without 2FA looks like, and PAM asks exactly one hidden
    // question. Anything longer is real 2FA and stops for a human.
    let params = fixture_params("KBDINT", 12483);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    assert!(volume.exists(Path::new("hello.txt")).await);
    assert_eq!(volume.auth_rung(), crate::auth::AuthRungUsed::KeyboardInteractive);
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_server_that_refuses_every_rung_says_so_typed() {
    // ❗ A typed variant, never a message match: the app puts a different thing
    // in front of the user for a rejection than for an unreachable server.
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some("not-the-password"));
    let volume_id = "sftp-wrong-password";

    // First contact needs approving before auth is even reached.
    let first = connect_sftp_volume("fixture", volume_id, params.clone(), host.clone())
        .await
        .expect(FIXTURE);
    let SftpConnectOutcome::NeedsHostKeyApproval(prompt) = first else {
        panic!("a fresh store must ask about the host key first");
    };
    super::approve_host_key(&host, &prompt.host, prompt.port, &prompt.algorithm, &prompt.fingerprint);

    let refused = connect_sftp_volume("fixture", volume_id, params, host).await;
    assert!(
        matches!(refused, Err(crate::SftpConnectError::AuthenticationRejected)),
        "got {refused:?}",
        refused = refused.map(|_| "a session")
    );
}

// ── Host-key trust, against real keys ────────────────────────────────

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_server_offering_two_key_types_is_not_a_changed_key() {
    // ❗ The cell the whole `(host, port, algorithm)` + pin design exists for.
    // `sftp-fixture-twokeys` holds an ed25519 AND an rsa host key. Approve
    // whichever it presents, reconnect, and it must come back Trusted rather
    // than crying man-in-the-middle — that alarm has to stay believable.
    let params = fixture_params("TWOKEYS", 12484);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));

    let volume = connect_fixture(&host, params.clone()).await;
    assert!(volume.exists(Path::new("hello.txt")).await);
    drop(volume);

    for attempt in 0..3 {
        let again = connect_sftp_volume("fixture", "sftp-twokeys", params.clone(), host.clone())
            .await
            .expect(FIXTURE);
        assert!(
            matches!(again, SftpConnectOutcome::Connected(_)),
            "reconnect {attempt} to a two-key server asked again instead of recognizing the pinned key"
        );
    }
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_key_that_is_not_the_one_we_stored_reads_as_changed() {
    // Two real servers with two real identities. Storing one's fingerprint
    // against the other's address is exactly what a man-in-the-middle looks like
    // from the client's side, and it must NOT take the first-contact path.
    let stock = fixture_params("OPENSSH", 12480);
    let stock_host = fixture_host(&stock, Some(FIXTURE_PASSWORD));
    let stock_key = first_contact_prompt(&stock_host, stock.clone()).await;

    let impostor = fixture_params("CHANGEDKEY", 12485);
    let store = InMemoryHostKeys::new().with_entry(
        &impostor.host,
        impostor.port,
        &stock_key.algorithm,
        &stock_key.fingerprint,
    );
    let host = VolumeHost::builder().host_keys(std::sync::Arc::new(store)).build();

    let prompt = first_contact_prompt(&host, impostor).await;
    assert_eq!(
        prompt.kind,
        HostKeyPromptKind::Changed,
        "a key that isn't the stored one must never share the one-click path a first-seen key takes"
    );
    assert_ne!(prompt.fingerprint, stock_key.fingerprint);
}

// ── The reading surface ──────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn metadata_answers_the_three_questions_the_panes_ask() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    let file = volume.get_metadata(Path::new("ten-bytes.txt")).await.expect(FIXTURE);
    assert_eq!(file.name, "ten-bytes.txt");
    assert_eq!(file.size, Some(10));
    assert!(!file.is_directory);
    assert!(file.modified_at.is_some(), "a pane column reads this");

    let dir = volume.get_metadata(Path::new("photos")).await.expect(FIXTURE);
    assert!(dir.is_directory);
    assert_eq!(dir.size, None, "a directory's size is the walker's answer, not stat's");

    assert!(volume.is_directory(Path::new("photos")).await.expect(FIXTURE));
    assert!(!volume.is_directory(Path::new("hello.txt")).await.expect(FIXTURE));
    assert!(volume.exists(Path::new("hello.txt")).await);
    assert!(!volume.exists(Path::new("nothing-here.txt")).await);

    let missing = volume.get_metadata(Path::new("nothing-here.txt")).await;
    assert!(matches!(missing, Err(VolumeError::NotFound(_))), "got {missing:?}");
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_path_outside_the_volume_root_is_refused_by_the_backend_not_the_server() {
    // The server would happily serve `/etc/passwd` to this account. Refusing is
    // ours to do, and anchoring would have asked for `/srv/data/etc/passwd`.
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    let outside = volume.list_directory(Path::new("/etc"), None).await;
    assert!(matches!(outside, Err(VolumeError::NotFound(_))), "got {outside:?}");
    assert!(!volume.exists(Path::new("/etc/passwd")).await);
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_big_directory_and_a_deep_nest_both_come_back_whole() {
    let params = fixture_params("BIGDIR", 12489);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    let many = volume.list_directory(Path::new("many"), None).await.expect(FIXTURE);
    assert_eq!(many.len(), 5_000, "a paged readdir must not stop at the first batch");

    let mut deep = String::from("deep");
    for level in 0..40 {
        deep.push_str(&format!("/level-{level}"));
    }
    assert!(volume.is_directory(Path::new(&deep)).await.expect(FIXTURE));
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_listing_progress_report_lands_once_with_the_whole_tally() {
    // ❗ The seam is a `dyn` trait object: one call per LISTING, never one per
    // entry. `RecordingListings::change_count` is the same instrument for the
    // mutation side.
    use std::sync::Mutex;
    let params = fixture_params("BIGDIR", 12489);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    let reports = Mutex::new(Vec::new());
    let record = |progress: cmdr_fs::volume::ListingProgress| {
        reports.lock().expect("no panics in this closure").push(progress);
    };
    let entries = volume
        .list_directory(Path::new("many"), Some(&record))
        .await
        .expect(FIXTURE);

    let reports = reports.into_inner().expect("no panics in this closure");
    assert_eq!(reports.len(), 1, "5 000 entries must not produce 5 000 seam calls");
    assert_eq!(reports[0].files, entries.len());
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_name_that_is_not_utf8_takes_the_whole_session_down() {
    // ⚠️ SFTP v3 filenames are BYTES, which is new for Cmdr, and the damage is
    // WORSE than "one unlistable directory": `openssh-sftp-client` deserializes
    // names through a strict `ssh_format`, and the failure happens in its own
    // read task, which then exits. Every later request on that session answers
    // `BackgroundTaskFailure`, so the connection is gone, not just the listing.
    //
    // Measured against `sftp-fixture-oddnames` on `openssh-sftp-client` 0.15.7,
    // 2026-08-22. Still the right failure to have over a lossy one — a U+FFFD
    // name shows in the pane, addresses nothing, and a folder copy writes it at
    // the destination — but it is why the byte-backed vendoring escape hatch is
    // a real plan rather than a footnote.
    let params = fixture_params("ODDNAMES", 12490);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;

    // Names that ARE valid UTF-8, however awkward, come back intact.
    let fine = volume.list_directory(Path::new("utf8"), None).await.expect(FIXTURE);
    assert!(fine.iter().any(|e| e.name == "🦀.txt"));
    assert!(fine.iter().any(|e| e.name.starts_with("naïve")));
    assert!(fine.iter().any(|e| e.name == "a b  c.txt"));

    let refused = volume.list_directory(Path::new("latin1"), None).await;
    assert!(
        matches!(refused, Err(VolumeError::DeviceDisconnected(_))),
        "a byte name kills the engine's read task, so the honest answer is a lost session, got {refused:?}"
    );

    // ❗ And it stays lost: nothing on this session works again, which is what
    // makes this a reconnect rather than a per-directory error.
    let after = volume.list_directory(Path::new("utf8"), None).await;
    assert!(
        matches!(after, Err(VolumeError::DeviceDisconnected(_))),
        "the session must read as gone rather than half-alive, got {after:?}"
    );
}

// ── The quirk servers still connect and list ─────────────────────────

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn every_quirk_server_is_reachable_and_serves_its_export() {
    // The three quirk fixtures sit in front of the real `sftp-server` through a
    // proxy, and the later milestones lean on them entirely. This cell is what
    // says the proxy itself is sound, so a red byte-path test later reads as a
    // byte-path bug rather than as a broken fixture.
    for (service, fallback) in [("NOPOSIXRENAME", 12486), ("SHORTREADS", 12487), ("SMALLLIMITS", 12488)] {
        let params = fixture_params(service, fallback);
        let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
        let volume = connect_fixture(&host, params).await;
        let entries = volume.list_directory(Path::new("."), None).await.expect(service);
        assert!(
            entries.iter().any(|e| e.name == "hello.txt"),
            "{service} did not serve its export"
        );
    }
}

// ── The two crate hazards ────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn disconnecting_drops_the_session_instead_of_closing_it() {
    // ❗ `Sftp::close()` awaits a read task that only ends at reader EOF, which a
    // `russh` channel never reaches, so calling it here would hang this test
    // forever rather than fail it. Dropping is the clean shutdown, and this cell
    // is what stops someone "fixing" that back.
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    assert!(volume.exists(Path::new("hello.txt")).await);

    tokio::time::timeout(std::time::Duration::from_secs(5), volume.disconnect())
        .await
        .expect("disconnecting must return promptly; a hang here means someone reached for close()");

    let after = volume.list_directory(Path::new("."), None).await;
    assert!(
        matches!(after, Err(VolumeError::DeviceDisconnected(_))),
        "a disconnected volume fails fast rather than hanging, got {after:?}"
    );
}

#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn abandoning_a_connect_does_not_panic_the_engines_task() {
    // ❗ `openssh-sftp-client`'s `tasks.rs` does `tx.send(extensions).unwrap()`,
    // which panics in a spawned task if the `Sftp::new` future is dropped before
    // the server's hello arrives. `connect_sftp_volume` runs the dial in a task
    // and awaits the JOIN HANDLE, so dropping this future abandons the handle
    // and never the dial. A regression here surfaces as a panic in the test
    // binary rather than as a failing assertion.
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));

    for _ in 0..10 {
        let dial = connect_sftp_volume("fixture", "sftp-abandoned", params.clone(), host.clone());
        // Long enough to be mid-handshake, short enough to be nowhere near done.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(2), dial).await;
    }

    // The wait IS the subject: the panic fires when an abandoned dial reaches the
    // server's hello, so there is nothing to poll for — a condition that never
    // becomes true is exactly what passing looks like.
    // allowed-test-sleep: waiting out the window in which an abandoned dial would panic
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let volume = connect_fixture(&host, params).await;
    assert!(volume.exists(Path::new("hello.txt")).await);
}

// ── Helpers ──────────────────────────────────────────────────────────

/// The private key the entrypoint generated for a key-auth fixture.
///
/// Written to the bind mount at container start rather than checked in: a
/// private key in a repo is a private key on the internet.
fn fixture_key_path(service: &str) -> std::path::PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("this crate sits two levels under the repo root");
    repo_root
        .join("apps/desktop/test/sftp-servers/.keys")
        .join(service)
        .join("id_ed25519")
}

/// Dials once against an empty store and returns the approval prompt.
async fn first_contact_prompt(
    host: &VolumeHost,
    params: crate::params::SftpConnectionParams,
) -> crate::transport::HostKeyPrompt {
    match connect_sftp_volume("fixture", "sftp-first-contact", params, host.clone()).await {
        Ok(SftpConnectOutcome::NeedsHostKeyApproval(prompt)) => prompt,
        other => panic!(
            "expected an approval prompt from a fresh store, got {:?}",
            other.is_ok()
        ),
    }
}

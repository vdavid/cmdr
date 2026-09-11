//! Tests for `share_access.rs`: reading the server's answer, and what a mount's
//! "not found" becomes for each identity.
//!
//! The live half (NetFS's `ENOENT` for a guest-refused share, and the fixture
//! server's own TreeConnect answer) is pinned by
//! `mount_test.rs::smb_integration_mount_guest_refused_share_asks_for_credentials`.

use super::*;
use smb2::types::status::NtStatus;
use std::time::Instant;

fn protocol(status: NtStatus, command: Command) -> Result<(), smb2::Error> {
    Err(smb2::Error::Protocol { status, command })
}

fn not_found() -> MountError {
    MountError::ShareNotFound {
        server: "observermch".to_string(),
        share: "data".to_string(),
    }
}

/// The wire tag the frontend branches on, which is what these answers are FOR.
fn wire_type(error: &MountError) -> String {
    serde_json::to_value(error).expect("a MountError serializes")["type"]
        .as_str()
        .expect("MountError is internally tagged")
        .to_string()
}

#[test]
fn verdict_reads_each_answer_by_status_and_command() {
    let cases: Vec<(&str, Result<(), smb2::Error>, ShareVerdict)> = vec![
        ("the share opened", Ok(()), ShareVerdict::Opens),
        (
            "TreeConnect access denied, ERR-SHUSC's server refusing a guest",
            protocol(NtStatus::ACCESS_DENIED, Command::TreeConnect),
            ShareVerdict::RefusedAtShare,
        ),
        (
            "TreeConnect bad network name",
            protocol(NtStatus::BAD_NETWORK_NAME, Command::TreeConnect),
            ShareVerdict::NoSuchShare,
        ),
        (
            "a TreeConnect answer about neither access nor existence",
            protocol(NtStatus::OBJECT_NAME_INVALID, Command::TreeConnect),
            ShareVerdict::Unknown,
        ),
        (
            "SessionSetup logon failure",
            protocol(NtStatus::LOGON_FAILURE, Command::SessionSetup),
            ShareVerdict::RefusedAtSignIn,
        ),
        (
            "SessionSetup account restriction, macOS smbd refusing a guest",
            protocol(NtStatus::ACCOUNT_RESTRICTION, Command::SessionSetup),
            ShareVerdict::RefusedAtSignIn,
        ),
        (
            "SessionSetup access denied: the same status as the share refusal, at a different step",
            protocol(NtStatus::ACCESS_DENIED, Command::SessionSetup),
            ShareVerdict::RefusedAtSignIn,
        ),
        (
            "an auth exchange smb2 couldn't complete",
            Err(smb2::Error::Auth {
                message: "no challenge".to_string(),
            }),
            ShareVerdict::RefusedAtSignIn,
        ),
        (
            "nothing listening",
            Err(smb2::Error::Io(std::io::Error::from(
                std::io::ErrorKind::ConnectionRefused,
            ))),
            ShareVerdict::Unknown,
        ),
        ("timed out", Err(smb2::Error::Timeout), ShareVerdict::Unknown),
        (
            "dropped mid-exchange",
            Err(smb2::Error::Disconnected),
            ShareVerdict::Unknown,
        ),
    ];
    for (what, outcome, expected) in &cases {
        assert_eq!(verdict_from(outcome), *expected, "{what}");
    }
}

/// Every identity against every verdict. A new `ShareVerdict` variant has to be
/// added to both rows here, or the table stops being the whole contract.
#[test]
fn clarified_answers_every_identity_and_verdict() {
    let guest = ShareAttempt::new("observermch", "data", 445, None, None);
    let account = ShareAttempt::new("observermch", "data", 445, Some("ada"), Some("hunter2"));
    let table = [
        (&guest, ShareVerdict::RefusedAtShare, "auth_required"),
        (&guest, ShareVerdict::RefusedAtSignIn, "auth_required"),
        (&guest, ShareVerdict::NoSuchShare, "share_not_found"),
        (&guest, ShareVerdict::Opens, "share_not_found"),
        (&guest, ShareVerdict::Unknown, "share_not_found"),
        (&account, ShareVerdict::RefusedAtShare, "permission_denied"),
        (&account, ShareVerdict::RefusedAtSignIn, "share_not_found"),
        (&account, ShareVerdict::NoSuchShare, "share_not_found"),
        (&account, ShareVerdict::Opens, "share_not_found"),
        (&account, ShareVerdict::Unknown, "share_not_found"),
    ];
    for (attempt, verdict, expected) in table {
        let answer = clarified(not_found(), attempt, verdict);
        assert_eq!(
            wire_type(&answer),
            expected,
            "{:?} with {verdict:?} answered {answer:?}",
            attempt.identity
        );
    }
}

/// A clarified answer carries what the pane words: the server and share the
/// attempt named, and for a refused account, which account it was.
#[test]
fn clarified_answers_carry_the_names_the_pane_words() {
    let guest = ShareAttempt::new("observermch", "data", 445, None, None);
    assert_eq!(
        clarified(not_found(), &guest, ShareVerdict::RefusedAtShare),
        MountError::AuthRequired {
            server: "observermch".to_string(),
            share: "data".to_string(),
        }
    );

    let account = ShareAttempt::new("observermch", "data", 445, Some("ada"), Some("hunter2"));
    assert_eq!(
        clarified(not_found(), &account, ShareVerdict::RefusedAtShare),
        MountError::PermissionDenied {
            server: "observermch".to_string(),
            share: "data".to_string(),
            username: "ada".to_string(),
        }
    );
}

/// A kept answer is the mount's own, not a rebuilt one.
#[test]
fn clarified_keeps_the_original_error_untouched() {
    let guest = ShareAttempt::new("observermch", "data", 445, None, None);
    let original = MountError::ShareNotFound {
        server: "192.168.1.5".to_string(),
        share: "Data".to_string(),
    };
    assert_eq!(clarified(original.clone(), &guest, ShareVerdict::NoSuchShare), original);
}

/// The answers reach logs and error reports, so the password must never be in one.
#[test]
fn clarified_answers_never_carry_the_password() {
    let account = ShareAttempt::new("observermch", "data", 445, Some("ada"), Some("hunter2"));
    for verdict in [ShareVerdict::RefusedAtShare, ShareVerdict::RefusedAtSignIn] {
        let debug = format!("{:?}", clarified(not_found(), &account, verdict));
        // allowed-error-string-match: asserting a secret is absent from a diagnostic, not classifying an error
        assert!(!debug.contains("hunter2"), "{verdict:?} leaked the password: {debug}");
    }
}

#[test]
fn an_attempt_is_a_guest_one_only_without_a_username() {
    assert_eq!(
        ShareAttempt::new("nas", "data", 445, None, None).identity,
        SignInIdentity::Guest
    );
    // Someone who typed "guest" as their account offered an account.
    assert_eq!(
        ShareAttempt::new("nas", "data", 445, Some("guest"), Some("")).identity,
        SignInIdentity::Account
    );
}

/// The probe names the share the way the mount URL does, or an accented share
/// answers `STATUS_BAD_NETWORK_NAME` and reads as missing after all.
#[test]
fn an_attempt_folds_the_share_name_to_nfc() {
    let attempt = ShareAttempt::new("nas", "Re\u{301}gi NAS", 445, None, None);
    assert_eq!(attempt.params.share_name, "R\u{e9}gi NAS");
}

/// The probe runs inside the mount's timeout. With nothing left it must keep the
/// mount's answer WITHOUT dialing: 192.0.2.1 is a documentation address nothing
/// answers on, so a dial here would sit out the whole probe limit.
#[tokio::test]
async fn a_spent_budget_keeps_the_mount_answer_without_dialing() {
    let attempt = ShareAttempt::new("192.0.2.1", "data", 445, None, None);
    let started = Instant::now();
    let answer = clarify_share_not_found(not_found(), &attempt, Duration::ZERO).await;
    assert_eq!(wire_type(&answer), "share_not_found");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "a spent budget dialed anyway ({:?})",
        started.elapsed()
    );
}

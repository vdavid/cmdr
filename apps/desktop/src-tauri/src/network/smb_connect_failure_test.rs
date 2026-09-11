//! Tests for `smb_connect_failure.rs`: every answer read by status and command,
//! and what each refusal says about who was turned away.

use super::*;
use smb2::types::status::NtStatus;

fn protocol(status: NtStatus, command: Command) -> smb2::Error {
    smb2::Error::Protocol { status, command }
}

/// The step is the answer. `STATUS_ACCESS_DENIED` at TreeConnect is a share turning
/// away an identity the server signed in; the same status at SessionSetup never got
/// that far. ERR-SHUSC's Samba server answered a guest with the first, and reading it
/// as the second told the user guest access was off and asked for a password that
/// would have worked.
#[test]
fn a_refusal_is_read_by_status_and_command() {
    let cases: Vec<(&str, smb2::Error, Option<RefusedAt>)> = vec![
        (
            "TreeConnect access denied, ERR-SHUSC's server refusing a guest",
            protocol(NtStatus::ACCESS_DENIED, Command::TreeConnect),
            Some(RefusedAt::Share),
        ),
        (
            "SessionSetup access denied: the same status, one step earlier",
            protocol(NtStatus::ACCESS_DENIED, Command::SessionSetup),
            Some(RefusedAt::SignIn),
        ),
        (
            "SessionSetup logon failure, a wrong password",
            protocol(NtStatus::LOGON_FAILURE, Command::SessionSetup),
            Some(RefusedAt::SignIn),
        ),
        (
            "SessionSetup account restriction, macOS smbd refusing a guest",
            protocol(NtStatus::ACCOUNT_RESTRICTION, Command::SessionSetup),
            Some(RefusedAt::SignIn),
        ),
        (
            "an auth exchange smb2 couldn't complete",
            smb2::Error::Auth {
                message: "no challenge".to_string(),
            },
            Some(RefusedAt::SignIn),
        ),
        (
            "TreeConnect bad network name: a missing share is no verdict on the identity",
            protocol(NtStatus::BAD_NETWORK_NAME, Command::TreeConnect),
            None,
        ),
        (
            "TreeConnect logon failure: not an answer TreeConnect gives about access",
            protocol(NtStatus::LOGON_FAILURE, Command::TreeConnect),
            None,
        ),
        (
            "nothing listening",
            smb2::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused)),
            None,
        ),
        ("timed out", smb2::Error::Timeout, None),
        ("dropped mid-exchange", smb2::Error::Disconnected, None),
    ];
    for (what, error, expected) in &cases {
        assert_eq!(RefusedAt::of(error), *expected, "{what}");
    }
}

#[test]
fn a_refusal_carries_who_the_attempt_went_out_as() {
    let denied_at_share = protocol(NtStatus::ACCESS_DENIED, Command::TreeConnect);
    assert_eq!(
        Refusal::of(&denied_at_share, SignInIdentity::Guest),
        Some(Refusal {
            identity: SignInIdentity::Guest,
            at: RefusedAt::Share,
        })
    );
    assert_eq!(Refusal::of(&smb2::Error::Timeout, SignInIdentity::Account), None);
}

/// What `try_smb_upgrade` hands "Connect directly": a refusal keeps who and where,
/// so the sheet can ask for the right thing, and everything else is a network answer.
#[test]
fn a_failed_upgrade_is_a_refusal_or_a_network_answer() {
    let at_share = UpgradeError::from_connect_error(
        &protocol(NtStatus::ACCESS_DENIED, Command::TreeConnect),
        Some("ada"),
        "Naspolya".to_string(),
    );
    assert!(
        matches!(
            at_share,
            UpgradeError::Refused(Refusal {
                identity: SignInIdentity::Account,
                at: RefusedAt::Share,
            })
        ),
        "an account the share turned away signed in fine, got {at_share:?}"
    );

    let wrong_password = UpgradeError::from_connect_error(
        &protocol(NtStatus::LOGON_FAILURE, Command::SessionSetup),
        Some("ada"),
        "Naspolya".to_string(),
    );
    assert!(
        matches!(
            wrong_password,
            UpgradeError::Refused(Refusal {
                identity: SignInIdentity::Account,
                at: RefusedAt::SignIn,
            })
        ),
        "got {wrong_password:?}"
    );

    let slow = UpgradeError::from_connect_error(&smb2::Error::Timeout, None, "Naspolya".to_string());
    assert!(
        matches!(
            slow,
            UpgradeError::Network {
                reason: UpgradeFailure::TooSlow,
                ..
            }
        ),
        "got {slow:?}"
    );
}

/// The reason the fallback log can't just print `UpgradeFailure`.
///
/// `UpgradeFailure` crosses IPC to pick the network-error copy, and a refusal never
/// reaches that surface (it goes to `CredentialsNeeded` instead), so it has no
/// refusal variant and folds a rejected password into `Unexpected`. That's what made
/// a stale Keychain password read as a flaky server in the log while the share sat
/// silently on the kernel mount.
///
/// If someone gives `UpgradeFailure` a refusal variant, this fails: fold the refusal
/// branch of `log_direct_connect_failure` back into the generic one at the same
/// time, so there's one classification rather than two that can disagree.
#[test]
fn a_refusal_is_invisible_to_upgrade_failure_so_the_log_reads_the_refusal_itself() {
    let rejected = smb2::Error::Auth {
        message: "STATUS_LOGON_FAILURE during SessionSetup".to_string(),
    };

    assert!(
        Refusal::of(&rejected, SignInIdentity::Account).is_some(),
        "a rejected password must be recognizable as a refusal, or the log can't name it"
    );
    assert_eq!(
        UpgradeFailure::from_smb_error(&rejected),
        UpgradeFailure::Unexpected,
        "UpgradeFailure has no refusal variant; the fallback log must not use it to describe a refusal"
    );
}

#[test]
fn an_attempt_is_a_guest_one_only_without_a_username() {
    assert_eq!(SignInIdentity::from_username(None), SignInIdentity::Guest);
    // Someone who typed "guest" as their account offered an account.
    assert_eq!(SignInIdentity::from_username(Some("guest")), SignInIdentity::Account);
}

/// Each of the four refusals needs its own advice, because each has a different fix.
///
/// A guest refused at sign-in is not a wrong password: there was no password
/// (ERR-48RZX sent someone into Keychain for an entry that was never written). A
/// guest or an account refused at the SHARE signed in fine, so neither "guest access
/// is off" nor "the password isn't accepted" is true (ERR-SHUSC).
#[test]
fn every_refusal_gets_advice_that_fits_who_was_turned_away_and_where() {
    let advice = |identity, at| Refusal { identity, at }.advice();
    let guest_at_sign_in = advice(SignInIdentity::Guest, RefusedAt::SignIn);
    let guest_at_share = advice(SignInIdentity::Guest, RefusedAt::Share);
    let account_at_sign_in = advice(SignInIdentity::Account, RefusedAt::SignIn);
    let account_at_share = advice(SignInIdentity::Account, RefusedAt::Share);

    let all = [guest_at_sign_in, guest_at_share, account_at_sign_in, account_at_share];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b, "two refusals with different fixes share one piece of advice");
        }
    }

    for guest in [guest_at_sign_in, guest_at_share] {
        assert!(
            guest.contains("guest"),
            "a guest refusal has to say it was a guest: {guest}"
        );
        assert!(
            !guest.contains("password isn't"),
            "a guest offered no password, so none was rejected: {guest}"
        );
    }
    assert!(
        account_at_sign_in.contains("password"),
        "an account refused at sign-in points at its password: {account_at_sign_in}"
    );
    for at_share in [guest_at_share, account_at_share] {
        assert!(
            at_share.contains("share"),
            "a share refusal names the share: {at_share}"
        );
        assert!(
            !at_share.contains("turned off"),
            "the server let this identity sign in, so nothing is turned off: {at_share}"
        );
    }
    assert!(
        !account_at_share.contains("password"),
        "the password worked, so the advice mustn't send anyone to fix it: {account_at_share}"
    );
}

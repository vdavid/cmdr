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
    let failed = |error| FailedDial {
        error,
        slowest_attempt: std::time::Duration::from_millis(5),
    };
    let at_share = UpgradeError::from_failed_dial(
        &failed(protocol(NtStatus::ACCESS_DENIED, Command::TreeConnect)),
        MountEvidence::Answered,
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

    let wrong_password = UpgradeError::from_failed_dial(
        &failed(protocol(NtStatus::LOGON_FAILURE, Command::SessionSetup)),
        MountEvidence::Answered,
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

    let slow = UpgradeError::from_failed_dial(
        &failed(smb2::Error::Timeout),
        MountEvidence::Answered,
        None,
        "Naspolya".to_string(),
    );
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

/// `STATUS_BAD_NETWORK_NAME` at TreeConnect is the one direct-connect failure
/// repeating cannot fix, so it gets its own reason and the notice drops its
/// retry button for it.
///
/// Reading it as `Unexpected` is what ERR-HYPZG looked like from the outside: a
/// share the server genuinely doesn't have, and a share published as a DFS
/// namespace root, both produced the same "something went wrong" notice with a
/// button that could never work. smb2 0.22 resolves the namespace, so what still
/// reaches here really is a missing share.
#[test]
fn a_missing_share_is_read_as_such_and_its_look_alikes_are_not() {
    let cases: Vec<(&str, smb2::Error, UpgradeFailure)> = vec![
        (
            "the server has no share by that name",
            protocol(NtStatus::BAD_NETWORK_NAME, Command::TreeConnect),
            UpgradeFailure::ShareNotOnServer,
        ),
        (
            "the same status one step earlier is not about a share at all",
            protocol(NtStatus::BAD_NETWORK_NAME, Command::SessionSetup),
            UpgradeFailure::Unexpected,
        ),
        (
            // The namespace is real and its storage may come back, so this is a
            // condition that can pass: `Unreachable`, never `ShareNotOnServer`.
            "a DFS namespace whose every target refused",
            smb2::Error::DfsNoReachableTarget {
                namespace: r"\\example.com\dana".to_string(),
                target_count: 2,
                source: Box::new(smb2::Error::Timeout),
            },
            UpgradeFailure::Unreachable,
        ),
        (
            // A scale-out cluster redirect carries `STATUS_BAD_NETWORK_NAME` on
            // the wire too; smb2 gives it its own variant so it can't be misread
            // here as a missing share.
            "a cluster redirect, which shares the status and means something else",
            smb2::Error::ShareRedirected {
                share: "archive".to_string(),
            },
            UpgradeFailure::Unexpected,
        ),
        ("timed out", smb2::Error::Timeout, UpgradeFailure::TooSlow),
    ];
    for (what, error, expected) in &cases {
        assert_eq!(UpgradeFailure::from_smb_error(error), *expected, "{what}");
    }
}

// ── A connection this Mac refused ──────────────────────────────────

/// A dial that failed the way smb2 reports a TCP connect that got nowhere: one
/// attempt per address, each with the io kind it failed with (`None` is no answer
/// within the budget).
fn connect_failed(kinds: &[Option<std::io::ErrorKind>]) -> smb2::Error {
    let attempts = kinds
        .iter()
        .zip(10u8..)
        .map(|(&error_kind, last_octet)| smb2::transport::ConnectAttempt {
            addr: std::net::SocketAddr::from(([192, 168, 0, last_octet], 445)),
            error_kind,
        })
        .collect();
    smb2::Error::ConnectFailed {
        host: "192.168.0.10".to_string(),
        attempts,
    }
}

fn dial(error: smb2::Error, slowest_attempt: std::time::Duration) -> FailedDial {
    FailedDial { error, slowest_attempt }
}

/// ERR-XGS9X (macOS 27.0, 2026-09-30): the kernel mount of a share listed fine
/// while every one of Cmdr's own dials to the same address came back
/// `EHOSTUNREACH` in 1–3 ms. The user's Local Network permission was stuck on;
/// switching it off and on fixed it at once. So this Mac refusing the route is
/// read from three things together, and ❌ never from the errno alone: a server
/// that's off, or a router that gave up on it, says `EHOSTUNREACH` too.
#[test]
fn a_route_this_mac_refused_is_told_apart_from_a_server_that_isnt_there() {
    use std::io::ErrorKind as Io;
    use std::time::Duration;
    let fast = Duration::from_millis(3);
    let slow = Duration::from_secs(3);
    let blocked = UpgradeFailure::BlockedByThisMac;
    let unreachable = UpgradeFailure::Unreachable;

    // (what, error, slowest attempt, mount, expected)
    let cases: Vec<(&str, smb2::Error, Duration, MountEvidence, UpgradeFailure)> = vec![
        (
            "ERR-XGS9X: instant EHOSTUNREACH while the mount answers",
            connect_failed(&[Some(Io::HostUnreachable)]),
            fast,
            MountEvidence::Answered,
            blocked,
        ),
        (
            "instant ENETUNREACH while the mount answers",
            connect_failed(&[Some(Io::NetworkUnreachable)]),
            fast,
            MountEvidence::Answered,
            blocked,
        ),
        (
            "the same answer as a bare io error",
            smb2::Error::Io(std::io::Error::from_raw_os_error(libc::EHOSTUNREACH)),
            fast,
            MountEvidence::Answered,
            blocked,
        ),
        (
            "every address refused the same way",
            connect_failed(&[Some(Io::HostUnreachable), Some(Io::NetworkUnreachable)]),
            fast,
            MountEvidence::Answered,
            blocked,
        ),
        (
            "no mount to vouch for the server: it may just be off",
            connect_failed(&[Some(Io::HostUnreachable)]),
            fast,
            MountEvidence::NoAnswer,
            unreachable,
        ),
        (
            "a slow EHOSTUNREACH came from the network (a router giving up), not from this Mac",
            connect_failed(&[Some(Io::HostUnreachable)]),
            slow,
            MountEvidence::Answered,
            unreachable,
        ),
        (
            "a slow one with no mount either",
            connect_failed(&[Some(Io::HostUnreachable)]),
            slow,
            MountEvidence::NoAnswer,
            unreachable,
        ),
        (
            "refused: something listens there and said no, so the route was fine",
            connect_failed(&[Some(Io::ConnectionRefused)]),
            fast,
            MountEvidence::Answered,
            unreachable,
        ),
        (
            "one address refused the route, another never answered",
            connect_failed(&[Some(Io::HostUnreachable), None]),
            fast,
            MountEvidence::Answered,
            unreachable,
        ),
        (
            "a timeout is its own answer",
            smb2::Error::Timeout,
            fast,
            MountEvidence::Answered,
            UpgradeFailure::TooSlow,
        ),
    ];
    for (what, error, slowest, mount, expected) in cases {
        assert_eq!(
            UpgradeFailure::of_dial(&dial(error, slowest), mount),
            expected,
            "{what}"
        );
    }
}

/// The ceiling sits far above what a refusal inside this Mac takes (1–3 ms in
/// ERR-XGS9X) and far below what an answer from the network does.
#[test]
fn the_speed_ceiling_is_inclusive() {
    use std::io::ErrorKind as Io;
    let at_ceiling = dial(connect_failed(&[Some(Io::HostUnreachable)]), BLOCKED_DIAL_CEILING);
    assert_eq!(
        UpgradeFailure::of_dial(&at_ceiling, MountEvidence::Answered),
        UpgradeFailure::BlockedByThisMac
    );
    let past = dial(
        connect_failed(&[Some(Io::HostUnreachable)]),
        BLOCKED_DIAL_CEILING + std::time::Duration::from_millis(1),
    );
    assert_eq!(
        UpgradeFailure::of_dial(&past, MountEvidence::Answered),
        UpgradeFailure::Unreachable
    );
}

/// "Connect directly" gets the same reading the log does, so its toast can say
/// what to switch.
#[test]
fn a_refused_route_reaches_connect_directly_as_a_network_answer() {
    let refused_route = dial(
        connect_failed(&[Some(std::io::ErrorKind::HostUnreachable)]),
        std::time::Duration::from_millis(2),
    );
    let answer = UpgradeError::from_failed_dial(&refused_route, MountEvidence::Answered, None, "Mars".to_string());
    assert!(
        matches!(
            answer,
            UpgradeError::Network {
                reason: UpgradeFailure::BlockedByThisMac,
                ..
            }
        ),
        "got {answer:?}"
    );
}

/// The log line is where the next report gets read, so it names the io kind and,
/// where the error carries one, the raw errno: EHOSTUNREACH at a glance.
#[test]
fn the_log_detail_names_the_io_kind_and_errno() {
    let bare = smb2::Error::Io(std::io::Error::from_raw_os_error(libc::EHOSTUNREACH));
    let detail = dial_detail(&bare);
    assert!(detail.contains("HostUnreachable"), "{detail}");
    assert!(detail.contains(&format!("errno={}", libc::EHOSTUNREACH)), "{detail}");

    let per_address = connect_failed(&[Some(std::io::ErrorKind::HostUnreachable), None]);
    let detail = dial_detail(&per_address);
    assert!(detail.contains("HostUnreachable"), "{detail}");
    assert!(detail.contains("no answer"), "{detail}");

    assert_eq!(
        dial_detail(&smb2::Error::Timeout),
        "io_kind=none",
        "a failure that isn't io says so"
    );
}

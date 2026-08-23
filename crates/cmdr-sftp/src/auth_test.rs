//! The ladder's order, and the per-rung reconnect rules.

use super::*;

fn params() -> SftpConnectionParams {
    SftpConnectionParams::new("naspolya", 22, "ada", "/srv/data")
}

#[test]
fn the_ladder_starts_with_what_costs_the_user_nothing() {
    let rungs = ladder(&params().with_key_file("/home/ada/.ssh/id_ed25519"));
    assert_eq!(
        rungs,
        vec![
            AuthRung::Agent,
            AuthRung::KeyFile(PathBuf::from("/home/ada/.ssh/id_ed25519")),
            AuthRung::Password,
            AuthRung::KeyboardInteractive,
        ]
    );
}

#[test]
fn a_rung_with_nothing_behind_it_is_left_out() {
    // No agent and no key file means the two secret-backed rungs are the whole
    // ladder, which is what a fixture exercising password auth needs.
    let rungs = ladder(&params().without_agent());
    assert_eq!(rungs, vec![AuthRung::Password, AuthRung::KeyboardInteractive]);
}

#[test]
fn a_passphrase_protected_key_gets_the_same_one_try_a_password_does() {
    // The passphrase comes from the same store the password does, and the store
    // is re-read on every dial, so there is nothing about this rung that a single
    // unattended try can't carry. One try rather than a loop: a refused key is a
    // spent authentication attempt like any other.
    assert_eq!(
        reconnect_policy(AuthRungUsed::KeyFile {
            passphrase_protected: true
        }),
        ReconnectPolicy::RetryOnceFromStore
    );
}

#[test]
fn an_unencrypted_key_and_the_agent_reconnect_freely() {
    assert_eq!(
        reconnect_policy(AuthRungUsed::KeyFile {
            passphrase_protected: false
        }),
        ReconnectPolicy::Freely
    );
    assert_eq!(reconnect_policy(AuthRungUsed::Agent), ReconnectPolicy::Freely);
}

#[test]
fn a_password_gets_exactly_one_fresh_read_from_the_store() {
    // Once, because the password may have been changed since; not in a loop,
    // because repeated wrong passwords lock accounts.
    assert_eq!(
        reconnect_policy(AuthRungUsed::Password),
        ReconnectPolicy::RetryOnceFromStore
    );
}

#[test]
fn keyboard_interactive_never_reconnects_unattended() {
    // This is where 2FA lives: there is nobody to answer the server's prompts.
    assert_eq!(
        reconnect_policy(AuthRungUsed::KeyboardInteractive),
        ReconnectPolicy::NeedsCredentials
    );
}

// ── The two toggles, and the precondition between them ───────────────

#[test]
fn the_toggle_being_off_outranks_every_rung() {
    // "Never redial unattended" means exactly that, whatever is stored and
    // whatever proved the last session. ❗ The toggle is asked FIRST, so the
    // answer never depends on the rung when it's off.
    for rung in [
        AuthRungUsed::Agent,
        AuthRungUsed::KeyFile {
            passphrase_protected: false,
        },
        AuthRungUsed::KeyFile {
            passphrase_protected: true,
        },
        AuthRungUsed::Password,
        AuthRungUsed::KeyboardInteractive,
    ] {
        for secret_stored in [false, true] {
            assert_eq!(
                unattended_reconnect(false, rung, secret_stored),
                UnattendedReconnect::TurnedOff,
                "{rung:?} with secret_stored={secret_stored}"
            );
        }
    }
}

#[test]
fn the_rungs_that_need_no_secret_are_ready_with_an_empty_store() {
    // Nothing has to be read for either of these, so an empty store says nothing
    // about whether they can come back.
    for rung in [
        AuthRungUsed::Agent,
        AuthRungUsed::KeyFile {
            passphrase_protected: false,
        },
    ] {
        assert_eq!(unattended_reconnect(true, rung, false), UnattendedReconnect::Ready);
    }
}

#[test]
fn a_toggle_that_is_on_and_cannot_work_says_so_rather_than_reading_as_ready() {
    // ❗ The precondition, stated instead of implied: the two rungs that redial
    // out of the secret store can't do it with nothing in it. The frontend must
    // be able to warn about this rather than infer it.
    for rung in [
        AuthRungUsed::Password,
        AuthRungUsed::KeyFile {
            passphrase_protected: true,
        },
    ] {
        assert_eq!(
            unattended_reconnect(true, rung, false),
            UnattendedReconnect::NeedsStoredSecret,
            "{rung:?}"
        );
        assert_eq!(
            unattended_reconnect(true, rung, true),
            UnattendedReconnect::Ready,
            "{rung:?}"
        );
    }
}

#[test]
fn keyboard_interactive_says_the_toggle_cannot_help_it() {
    // The server asks the questions and there is nobody to answer them, so a
    // stored secret changes nothing. ❌ Never `NeedsStoredSecret`: that would send
    // the user off to remember a secret that still wouldn't buy a reconnect.
    for secret_stored in [false, true] {
        assert_eq!(
            unattended_reconnect(true, AuthRungUsed::KeyboardInteractive, secret_stored),
            UnattendedReconnect::RungCannot
        );
    }
}

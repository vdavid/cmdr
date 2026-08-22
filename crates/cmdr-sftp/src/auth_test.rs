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
fn a_passphrase_protected_key_cannot_reconnect_unattended() {
    // The passphrase is a secret, so it isn't held past the session it unlocked.
    // Pretending otherwise would mean either holding it (which we don't) or
    // silently failing every reconnect.
    assert_eq!(
        reconnect_policy(AuthRungUsed::KeyFile {
            passphrase_protected: true
        }),
        ReconnectPolicy::NeedsCredentials
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

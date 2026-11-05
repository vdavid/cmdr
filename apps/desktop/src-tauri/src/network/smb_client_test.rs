//! How the share listing treats a host the person means to sign in to.

use super::*;
use crate::network::smb_cache::cache_shares;

fn listing(auth_mode: AuthMode) -> ShareListResult {
    ShareListResult {
        shares: Vec::new(),
        auth_mode,
        from_cache: false,
    }
}

/// ❗ **A host someone typed an account for never answers with a guest
/// listing.** On a `map to guest = bad user` Samba a guest "succeeds" with an
/// almost-empty list, and the person has already said they want to sign in. So
/// a guest listing in the cache doesn't count, and nothing is dialed as guest:
/// the answer is "sign in", which the frontend takes to the Keychain and then
/// the sheet. `host.invalid` proves nothing was dialed, since reaching it would
/// be a `HostUnreachable`.
#[tokio::test]
async fn a_host_that_wants_an_account_asks_for_one_instead_of_listing_as_guest() {
    cache_shares("wants-account-guest-cached", &listing(AuthMode::GuestAllowed), 60_000);

    let answer = list_shares(
        "wants-account-guest-cached",
        "host.invalid",
        None,
        445,
        None,
        GuestAttempt::Skip,
        Some(2_000),
        Some(0),
    )
    .await;

    assert!(
        matches!(answer, Err(ShareListError::AuthRequired { .. })),
        "got {answer:?}"
    );
}

/// ❗ **Credentials the caller passed are the question, so guest is never tried
/// first.** Someone pressed "Sign in as…" or the listing is using the stored
/// password: on a `map to guest = bad user` Samba a guest leg "succeeds" with the
/// guest's list, and that list would come back instead of the account's.
#[test]
fn credentials_always_skip_the_guest_leg() {
    assert_eq!(GuestAttempt::Try.given_credentials(true), GuestAttempt::Skip);
    assert_eq!(GuestAttempt::Skip.given_credentials(true), GuestAttempt::Skip);
    assert_eq!(
        GuestAttempt::Try.given_credentials(false),
        GuestAttempt::Try,
        "no credentials: guest first, as ever"
    );
    assert_eq!(GuestAttempt::Skip.given_credentials(false), GuestAttempt::Skip);
}

/// An account's own listing in the cache still answers: it is exactly what the
/// person asked for, and re-dialing it on every open would be wasted round-trips.
#[tokio::test]
async fn an_account_listing_in_the_cache_still_answers_a_host_that_wants_one() {
    cache_shares("wants-account-creds-cached", &listing(AuthMode::CredsRequired), 60_000);

    let answer = list_shares(
        "wants-account-creds-cached",
        "host.invalid",
        None,
        445,
        None,
        GuestAttempt::Skip,
        Some(2_000),
        Some(0),
    )
    .await
    .expect("the cached account listing");

    assert!(answer.from_cache);
    assert_eq!(answer.auth_mode, AuthMode::CredsRequired);
}

fn share(name: &str, comment: &str) -> cmdr_smb::ShareInfo {
    cmdr_smb::ShareInfo {
        name: name.to_string(),
        is_disk: true,
        comment: Some(comment.to_string()),
    }
}

/// ❗ **A share the server lists twice reaches the pane once** (cmdr-reports#81,
/// ERR-6JV77). A macOS file server answered an authenticated `NetShareEnum` with
/// one name twice, and the share list keys its rows on the name, so Svelte threw
/// `each_key_duplicate` and the whole window crashed on every open of that host.
/// The name is the share's identity, folded the way `smb_volume_id` folds it (NFC,
/// case-insensitive, as a tree connect reads it), so the first spelling wins and
/// the order stays the server's.
#[test]
fn a_share_the_server_lists_twice_reaches_the_pane_once() {
    let listed = ShareListResult {
        shares: vec![
            share("Projects", "first"),
            share("Public", ""),
            share("Projects", "second"),
            share("projects", "case differs"),
            share("Cafe\u{301}", "decomposed"),
            share("Caf\u{e9}", "composed"),
        ],
        auth_mode: AuthMode::CredsRequired,
        from_cache: false,
    };

    let answer = with_unique_share_names(listed);

    let names: Vec<(&str, Option<&str>)> = answer
        .shares
        .iter()
        .map(|s| (s.name.as_str(), s.comment.as_deref()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Projects", Some("first")),
            ("Public", Some("")),
            ("Cafe\u{301}", Some("decomposed")),
        ]
    );
    assert_eq!(answer.auth_mode, AuthMode::CredsRequired);
}

// ── Always require SMB encryption ──────────────────────────────────

/// Requires encryption for one test and lifts it when the test ends. Nextest runs
/// each test in its own process, so no other test sees it.
struct EncryptionRequired;

impl EncryptionRequired {
    fn on() -> Self {
        cmdr_smb::SmbEncryption::set_required(true);
        Self
    }
}

impl Drop for EncryptionRequired {
    fn drop(&mut self) {
        cmdr_smb::SmbEncryption::set_required(false);
    }
}

/// ❗ The CLIs (`smbutil`, `smbclient`) know nothing about the requirement, so no
/// listing may fall back to them while it's on.
#[test]
fn no_cli_fallback_while_encryption_is_required() {
    assert!(cli_fallback_allowed(), "off by default");
    let _on = EncryptionRequired::on();
    assert!(!cli_fallback_allowed());
}

/// A guest session can't be encrypted but an account's can, so the guest leg's
/// encryption refusal asks for an account, the way an auth refusal does.
#[test]
fn a_guest_refused_for_encryption_asks_for_an_account() {
    assert!(guest_refusal_wants_an_account(&smb2::Error::EncryptionRequired));
    assert!(!guest_refusal_wants_an_account(&smb2::Error::EncryptionUnavailable {
        share: None
    }));
    assert!(!guest_refusal_wants_an_account(&smb2::Error::Timeout));
}

fn fixture_port(env: &str, default: u16) -> u16 {
    std::env::var(env).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// End to end under the requirement: a guest-friendly server's guest listing turns
/// into "sign in" (no plaintext guest list, no CLI fallback), and the account's
/// listing comes back over an encrypted session.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_required_encryption_sends_a_guest_listing_to_sign_in() {
    let _on = EncryptionRequired::on();
    let guest_port = fixture_port("SMB_CONSUMER_GUEST_PORT", 10480);
    let auth_port = fixture_port("SMB_CONSUMER_AUTH_PORT", 10481);

    let guest = list_shares(
        "smb-integration-encryption-guest",
        "127.0.0.1",
        None,
        guest_port,
        None,
        GuestAttempt::Try,
        None,
        Some(0),
    )
    .await;
    assert!(
        matches!(guest, Err(ShareListError::AuthRequired { .. })),
        "got {guest:?}"
    );

    let account = list_shares(
        "smb-integration-encryption-account",
        "127.0.0.1",
        None,
        auth_port,
        Some(("testuser", "testpass")),
        GuestAttempt::Try,
        None,
        Some(0),
    )
    .await
    .expect("an account's listing encrypts");
    assert!(!account.shares.is_empty());
}

/// A share listing whose reply was refused for its signature reaches the security log under the
/// host, like a share's own refusal does under its volume (#1000).
#[test]
fn a_refused_signature_on_a_listing_reaches_the_security_log() {
    use crate::security_log::{SecurityEvent, SmbScope, SmbSignatureSource, testing};
    let capture = testing::capture_events();
    let refused: Result<ShareListResult, ShareListError> = Err(ShareListError::SignatureInvalid {
        message: "the reply's signature didn't match".to_string(),
    });
    record_security_refusal("naspolya", &refused);
    record_security_refusal("naspolya", &refused);
    assert_eq!(
        capture.events(),
        vec![SecurityEvent::SmbSignatureRejectionsStarted {
            scope: SmbScope::Host("naspolya".into()),
            source: SmbSignatureSource::ShareListing,
        }]
    );
}

/// A share listing the server didn't confirm the negotiation for reaches the security log under
/// the host, like a share's own refusal does under its volume (#931). Any other listing outcome
/// records nothing.
#[test]
fn an_unconfirmed_negotiation_on_a_listing_reaches_the_security_log() {
    use crate::security_log::{SecurityEvent, SmbScope, testing};
    let capture = testing::capture_events();
    let unconfirmed: Result<ShareListResult, ShareListError> = Err(ShareListError::NegotiationInvalid {
        message: "the server answered a different dialect".to_string(),
    });
    record_security_refusal("naspolya", &unconfirmed);
    record_security_refusal(
        "naspolya",
        &Err(ShareListError::Timeout {
            message: "slow".to_string(),
        }),
    );
    record_security_refusal("naspolya", &Ok(listing(AuthMode::GuestAllowed)));
    assert_eq!(
        capture.events(),
        vec![SecurityEvent::SmbNegotiationRejectionsStarted {
            scope: SmbScope::Host("naspolya".into())
        }]
    );
}

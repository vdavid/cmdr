//! What "Always require SMB encryption" does against real servers.
//!
//! The fixtures are Samba at its default (`server smb encrypt = default`): it
//! offers a cipher on SMB 3.1.1 and asks for none. So the `auth` fixture with an
//! account is the server that CAN encrypt, and a guest session on the `guest`
//! fixture is the connection that can't (a guest session has no keys). No fixture
//! negotiates below SMB 3.0, so "no cipher at all" is covered only by smb2's own
//! unit tests (`session_cipher`).
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`.

use super::session_build::build_session;
use super::*;
use crate::encryption::RequiredForTest;
use std::time::Duration;

const STEP_TIMEOUT: Duration = Duration::from_secs(5);

fn account() -> SmbConnectionParams {
    SmbConnectionParams::new(
        "127.0.0.1",
        "private",
        smb2::testing::auth_port(),
        Some("testuser"),
        Some("testpass"),
    )
}

fn guest() -> SmbConnectionParams {
    SmbConnectionParams::new("127.0.0.1", "public", smb2::testing::guest_port(), None, None)
}

/// The baseline the setting changes: a server that only offers encryption gets
/// a plaintext session unless Cmdr asks.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_encryption_off_leaves_an_offering_server_in_the_clear() {
    let _switch = RequiredForTest::set(false);
    let (client, _tree) = build_session(&account(), None).await.expect("the account signs in");
    assert!(!client.connection().should_encrypt());
}

/// A volume's session (the main one, every reconnect, the scan pool, and each
/// transfer session all come from `build_session`) encrypts when required.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_required_encryption_encrypts_a_volume_session() {
    let _switch = RequiredForTest::set(true);
    let (client, _tree) = build_session(&account(), None)
        .await
        .expect("a server offering a cipher accepts an encrypted session");
    assert!(client.connection().should_encrypt());
}

/// A guest session can't encrypt, so a required one is refused at sign-in with
/// the typed error, ❌ never a quiet plaintext session.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_required_encryption_refuses_a_guest_volume_session() {
    let _switch = RequiredForTest::set(true);
    match build_session(&guest(), None).await {
        Err(smb2::Error::EncryptionRequired) => {}
        Err(other) => panic!("expected EncryptionRequired, got {other:?}"),
        Ok(_) => panic!("a guest session can't be encrypted, so it must not come up"),
    }
}

/// The share listing and the open-share probe build their own sessions; both
/// follow the switch too, and the listing's answer classifies as its own kind.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_required_encryption_reaches_the_listing_and_the_probe() {
    let _switch = RequiredForTest::set(true);

    let guest_listing = crate::try_list_shares_as_guest("127.0.0.1", None, smb2::testing::guest_port(), STEP_TIMEOUT)
        .await
        .expect_err("a guest listing can't be encrypted");
    assert!(
        matches!(
            crate::classify_error(&guest_listing),
            crate::ShareListError::EncryptionRequired { .. }
        ),
        "got {guest_listing:?}"
    );

    let shares = crate::try_list_shares_authenticated(
        "127.0.0.1",
        None,
        smb2::testing::auth_port(),
        "testuser",
        "testpass",
        STEP_TIMEOUT,
    )
    .await
    .expect("an account's listing encrypts");
    assert!(!shares.is_empty());

    crate::try_open_share(&account(), STEP_TIMEOUT)
        .await
        .expect("the probe opens the share over an encrypted session");
    match crate::try_open_share(&guest(), STEP_TIMEOUT).await {
        Err(smb2::Error::EncryptionRequired) => {}
        other => panic!("a guest probe must be refused for encryption, got {other:?}"),
    }
}

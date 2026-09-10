//! What `try_open_share` hears back from a real server: one cell per answer the
//! app's `network::share_access` reads by status and command.
//!
//! The `both` fixture is the shape ERR-SHUSC's server had: guests may list its
//! shares, and its `private` share lets only `testuser` in.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`.

use super::*;
use smb2::types::Command;
use smb2::types::status::NtStatus;

const STEP_TIMEOUT: Duration = Duration::from_secs(5);

fn both(share: &str, username: Option<&str>, password: Option<&str>) -> SmbConnectionParams {
    SmbConnectionParams::new("127.0.0.1", share, smb2::testing::both_port(), username, password)
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_share_refuses_a_guest_at_the_share() {
    // The session comes up (Samba maps the unknown `Guest` to its guest account),
    // and the SHARE turns it away. That's what makes it a sign-in question rather
    // than a missing share.
    match try_open_share(&both("private", None, None), STEP_TIMEOUT).await {
        Err(smb2::Error::Protocol { status, command }) => {
            assert_eq!(status, NtStatus::ACCESS_DENIED);
            assert_eq!(command, Command::TreeConnect);
        }
        other => panic!("a guest on `private` must be refused at TreeConnect, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_share_lets_the_allowed_account_in() {
    try_open_share(&both("private", Some("testuser"), Some("testpass")), STEP_TIMEOUT)
        .await
        .unwrap_or_else(|e| panic!("`testuser` must be able to open `private`: {e}"));
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_share_names_a_missing_share() {
    match try_open_share(&both("no-such-share", None, None), STEP_TIMEOUT).await {
        Err(smb2::Error::Protocol { status, command }) => {
            assert_eq!(status, NtStatus::BAD_NETWORK_NAME);
            assert_eq!(command, Command::TreeConnect);
        }
        other => panic!("a share the server doesn't have must answer bad network name, got {other:?}"),
    }
}

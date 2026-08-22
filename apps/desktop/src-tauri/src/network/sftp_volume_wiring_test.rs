//! The lifecycle a connect and a disconnect owe the rest of the app: a volume in
//! the registry, an entry in the server list, and a session that is GONE
//! afterwards.
//!
//! App-side because that is what these assert on. The dial itself, the trust
//! table, and the byte path are the crate's own cells
//! (`crates/cmdr-sftp/DETAILS.md` § "Which side a test lives on").
//!
//! ❗ Every `sftp_integration_` cell here needs the Docker stack:
//! `apps/desktop/test/sftp-servers/start.sh`.

use std::path::Path;
use std::time::Duration;

use cmdr_sftp::volume::testing::{FIXTURE_PASSWORD, FIXTURE_ROOT, FIXTURE_USER, fixture_port};

use crate::network::sftp_volume_wiring::{self, SftpConnection};
use crate::network::{keychain, sftp_host_keys, sftp_known_servers};
use cmdr_sftp::SftpConnectionParams;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// Params for the stock OpenSSH fixture, with the ssh-agent left out.
///
/// ❗ Agent off on purpose: a developer's own agent would answer for the password
/// rung these cells are exercising, and they would pass without testing it.
fn stock_params() -> SftpConnectionParams {
    SftpConnectionParams::new("127.0.0.1", fixture_port("OPENSSH", 12480), FIXTURE_USER, FIXTURE_ROOT)
        .without_agent()
}

/// Seeds the real secret store and the real trust store, the way a user who had
/// signed in once would leave them.
///
/// The trust store is in-memory in a test binary (nothing has named a file), and
/// the secret store is the test backend, so this leaves nothing on disk.
async fn signed_in_already(params: &SftpConnectionParams) {
    keychain::save_credentials(
        &params.credential_service(),
        Some(&params.username),
        &params.username,
        FIXTURE_PASSWORD,
    )
    .expect("the test secret store always accepts");

    // First contact, then the approval, which is exactly what the frontend does.
    let first = sftp_volume_wiring::connect_and_register("fixture", params.clone()).await;
    let SftpConnection::NeedsHostKeyApproval(prompt) = first else {
        // Another cell in this binary may have approved the same fixture already;
        // that is a connected volume, not a failure.
        return;
    };
    sftp_volume_wiring::approve_host_key(&prompt.host, prompt.port, &prompt.algorithm, &prompt.fingerprint)
        .await
        .expect(FIXTURE);
}

/// A successful connect leaves three things behind: a volume under its id, a
/// server in the list, and the rung the session came up on.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_connecting_registers_the_volume_and_remembers_the_server() {
    let params = stock_params();
    signed_in_already(&params).await;

    let outcome = sftp_volume_wiring::connect_and_register("Fixture server", params.clone()).await;
    let SftpConnection::Connected { volume_id, rung } = outcome else {
        panic!("a fixture with its key approved and its password stored must connect");
    };
    assert_eq!(rung, cmdr_sftp::auth::AuthRungUsed::Password);

    let manager = crate::file_system::volume::manager::get_volume_manager();
    let volume = manager.get(&volume_id).expect("a connect registers the volume it made");
    assert!(
        volume.exists(Path::new("hello.txt")).await,
        "the registered volume is the live one, not a placeholder"
    );

    let remembered = sftp_known_servers::all()
        .into_iter()
        .find(|entry| entry.host == params.host && entry.port == params.port && entry.username == params.username)
        .expect("a successful connect remembers the server");
    assert_eq!(remembered.display_name, "Fixture server");
    assert_eq!(remembered.remote_root, FIXTURE_ROOT);
    assert!(!remembered.use_agent);

    sftp_volume_wiring::disconnect(&volume_id).await;
}

/// ❗ **Disconnecting DROPS the session; it never closes it.**
///
/// `Sftp::close()` awaits a read task that only ends at reader EOF, which an SSH
/// channel never reaches, so a `close()` anywhere on this path would hang this
/// cell forever rather than fail it. The timeout is the assertion.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_disconnecting_drops_the_session_and_unregisters_the_volume() {
    let params = stock_params();
    signed_in_already(&params).await;
    let SftpConnection::Connected { volume_id, .. } =
        sftp_volume_wiring::connect_and_register("fixture", params).await
    else {
        panic!("a fixture with its key approved and its password stored must connect");
    };
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let volume = manager.get(&volume_id).expect("just registered");

    let disconnected = tokio::time::timeout(Duration::from_secs(5), sftp_volume_wiring::disconnect(&volume_id))
        .await
        .expect("a hang here means someone reached for `Sftp::close()`");

    assert!(disconnected);
    assert!(
        manager.get(&volume_id).is_none(),
        "a disconnected server is out of the registry, not a dead entry in it"
    );
    assert!(
        matches!(
            volume.list_directory(Path::new("."), None).await,
            Err(cmdr_fs::volume::VolumeError::DeviceDisconnected(_))
        ),
        "whoever still holds the volume fails fast rather than hanging on a dead session"
    );
}

/// Disconnecting something that isn't an SFTP volume answers no rather than
/// tearing down whatever is under that id.
#[tokio::test]
async fn disconnecting_a_volume_that_is_not_sftp_does_nothing() {
    assert!(
        !sftp_volume_wiring::disconnect("sftp-nothing-is-registered-here").await,
        "an unknown id is a no, not a panic"
    );
}

/// A server nobody has approved asks first, and ❗ holds no session while it
/// waits: the dial is dropped, and approving is followed by a fresh one.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_an_unapproved_server_asks_before_it_connects() {
    // A port of its own so this cell is first contact whatever else ran before
    // it; `sftp-fixture-twokeys` is a different identity from the stock server.
    let params = SftpConnectionParams::new("127.0.0.1", fixture_port("TWOKEYS", 12484), FIXTURE_USER, FIXTURE_ROOT)
        .without_agent();
    for algorithm in sftp_host_keys::list_trusted_host_keys()
        .into_iter()
        .filter(|entry| entry.host == params.host && entry.port == params.port)
        .map(|entry| entry.algorithm)
    {
        sftp_host_keys::forget_trusted_host_key(&params.host, params.port, &algorithm);
    }

    let outcome = sftp_volume_wiring::connect_and_register("fixture", params.clone()).await;
    let SftpConnection::NeedsHostKeyApproval(prompt) = outcome else {
        panic!("a server with no approved key must ask about it before anything else");
    };
    assert_eq!(prompt.kind, cmdr_sftp::transport::HostKeyPromptKind::Unknown);
    assert!(prompt.fingerprint.starts_with("SHA256:"));

    let volume_id = cmdr_fs::volume::sftp_volume_id(&params.host, params.port, &params.username);
    assert!(
        crate::file_system::volume::manager::get_volume_manager()
            .get(&volume_id)
            .is_none(),
        "❗ nothing is registered while a key is waiting to be approved"
    );
}

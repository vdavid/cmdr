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

use std::path::{Path, PathBuf};
use std::time::Duration;

use cmdr_fs::volume::ConnectionState;
use cmdr_sftp::volume::testing::{FIXTURE_PASSWORD, FIXTURE_ROOT, FIXTURE_USER, fixture_port};

use crate::network::one_shot_credentials::SecretOffer;
use crate::network::saved_server_fields::SavedServerOutcome;
use crate::network::sftp_known_servers::KnownSftpServer;
use crate::network::sftp_volume_wiring::{self, SftpConnection};
use crate::network::{keychain, sftp_host_keys, sftp_known_servers};
use cmdr_sftp::{SftpConnectionParams, SftpVolume};

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// Params for the stock OpenSSH fixture, with the ssh-agent left out.
///
/// ❗ Agent off on purpose: a developer's own agent would answer for the password
/// rung these cells are exercising, and they would pass without testing it.
fn stock_params() -> SftpConnectionParams {
    SftpConnectionParams::new("127.0.0.1", fixture_port("OPENSSH", 12480), FIXTURE_USER, FIXTURE_ROOT).without_agent()
}

/// Seeds the secret store and the real trust store, the way a user who had signed
/// in once would leave them.
///
/// The trust store is in-memory in a test binary (nothing has named a file), and
/// the secret store is the test backend, which writes inside the caller's
/// `isolate_secrets()` scratch dir and goes with it.
async fn signed_in_already(params: &SftpConnectionParams) {
    keychain::save_credentials(
        &params.credential_service(),
        Some(&params.username),
        &params.username,
        FIXTURE_PASSWORD,
    )
    .expect("the test secret store always accepts");

    // First contact, then the approval, which is exactly what the frontend does.
    let first =
        sftp_volume_wiring::connect_and_register("fixture", None, params.clone(), "fixture-attempt", None).await;
    let SftpConnection::NeedsHostKeyApproval(prompt) = first else {
        // Another cell in this binary may have approved the same fixture already;
        // that is a connected volume, not a failure.
        return;
    };
    sftp_volume_wiring::approve_host_key(&prompt.host, prompt.port, &prompt.algorithm, &prompt.fingerprint)
        .await
        .expect(FIXTURE);
}

/// A successful connect leaves two things behind: a volume under its id, and a
/// server in the list.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_connecting_registers_the_volume_and_remembers_the_server() {
    let _secrets = crate::test_support::isolate_secrets();
    let params = stock_params();
    signed_in_already(&params).await;

    let outcome =
        sftp_volume_wiring::connect_and_register("Fixture server", None, params.clone(), "fixture-attempt", None).await;
    let SftpConnection::Connected { volume_id } = outcome else {
        panic!("a fixture with its key approved and its password stored must connect");
    };

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

/// ❗ **A reconnect never puts an unpinned server back in the switcher.**
///
/// The connect path asks for `pinned: true`, which is how a NEW place lands in
/// the switcher on its first successful connect. A server the user has since
/// unpinned has to survive the next connect unpinned, or the unpin undoes itself
/// the moment the session comes back. `sftp_known_servers::remember` is where the
/// stored value wins.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_reconnecting_leaves_an_unpinned_server_unpinned() {
    let _secrets = crate::test_support::isolate_secrets();
    let params = stock_params();
    signed_in_already(&params).await;

    let first =
        sftp_volume_wiring::connect_and_register("Fixture server", None, params.clone(), "fixture-attempt", None).await;
    let SftpConnection::Connected { volume_id, .. } = first else {
        panic!("a fixture with its key approved and its password stored must connect");
    };
    let saved = |params: &SftpConnectionParams| {
        sftp_known_servers::all()
            .into_iter()
            .find(|entry| entry.host == params.host && entry.port == params.port && entry.username == params.username)
            .expect("a successful connect remembers the server")
    };
    assert!(saved(&params).pinned, "a first connect pins the new place");

    // The user unpins it, then the session drops and comes back.
    let mut unpinned = saved(&params);
    unpinned.pinned = false;
    sftp_known_servers::forget(&params.host, params.port, &params.username);
    sftp_known_servers::remember(unpinned);
    sftp_volume_wiring::disconnect(&volume_id).await;

    let again =
        sftp_volume_wiring::connect_and_register("Fixture server", None, params.clone(), "fixture-attempt-2", None)
            .await;
    let SftpConnection::Connected { volume_id, .. } = again else {
        panic!("the same fixture connects again");
    };
    assert!(!saved(&params).pinned, "the unpin survives the reconnect");

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
    let _secrets = crate::test_support::isolate_secrets();
    let params = stock_params();
    signed_in_already(&params).await;
    let SftpConnection::Connected { volume_id, .. } =
        sftp_volume_wiring::connect_and_register("fixture", None, params, "fixture-attempt", None).await
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
    let _secrets = crate::test_support::isolate_secrets();
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

    let outcome =
        sftp_volume_wiring::connect_and_register("fixture", None, params.clone(), "fixture-attempt", None).await;
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

// ── Calling a connect off ────────────────────────────────────────────

/// Cancelling an attempt nobody is running is a plain no.
///
/// ❗ Not an error: a click that lands just after a connect finished is ordinary,
/// and there is nothing wrong to report about it.
#[tokio::test]
async fn cancelling_an_attempt_nobody_is_running_is_a_plain_no() {
    assert!(!sftp_volume_wiring::cancel_connect(
        "sftp-no-such-attempt-was-ever-started"
    ));
}

/// A connect the user calls off ends at once and ❗ leaves nothing behind: no
/// volume in the registry, no server in the saved list.
///
/// `192.0.2.1` is reserved for documentation (RFC 5737) and routed nowhere, so
/// this dial hangs exactly the way a typo'd hostname does. ❗ That hang IS the
/// subject: without the cancel it holds for the whole handshake budget, and a
/// sign-in dialog with it.
#[tokio::test]
async fn cancelling_a_hanging_connect_ends_it_and_registers_nothing() {
    let _secrets = crate::test_support::isolate_secrets();
    const ATTEMPT: &str = "sftp-cancel-a-hanging-connect";
    let params = SftpConnectionParams::new("192.0.2.1", 22, "nobody", "/nowhere").without_agent();
    let volume_id = cmdr_fs::volume::sftp_volume_id(&params.host, params.port, &params.username);

    let dialing = params.clone();
    let connecting =
        tokio::spawn(
            async move { sftp_volume_wiring::connect_and_register("Nowhere", None, dialing, ATTEMPT, None).await },
        );

    // The attempt is cancelable from the moment the dial is in the air, which is
    // the whole reason the id is the caller's.
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the connect attempt to be cancelable", || {
        sftp_volume_wiring::cancel_connect(ATTEMPT)
    })
    .await;

    let outcome = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .expect("a cancelled connect answers long before the handshake budget runs out")
        .expect("the connect task must not panic");
    assert!(
        matches!(outcome, SftpConnection::Cancelled),
        "a cancelled connect says so, rather than reporting the address as unreachable"
    );

    assert!(
        crate::file_system::volume::manager::get_volume_manager()
            .get(&volume_id)
            .is_none(),
        "❗ a cancelled connect registers nothing"
    );
    assert!(
        !sftp_known_servers::all()
            .into_iter()
            .any(|entry| entry.host == params.host && entry.port == params.port),
        "❗ a cancelled connect remembers no server either"
    );
    assert!(
        !keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "❗ and it writes no secret: only `save_sftp_credentials` ever does"
    );

    // The entry is gone with the attempt, so a second cancel finds nothing.
    assert!(!sftp_volume_wiring::cancel_connect(ATTEMPT));
}

// ── A secret that is used once and never stored ──────────────────────

/// Approves the fixture's host key without leaving a secret behind, so a cell
/// about where the secret came from starts with an empty store.
async fn host_key_approved(params: &SftpConnectionParams) {
    let first =
        sftp_volume_wiring::connect_and_register("fixture", None, params.clone(), "fixture-approve", None).await;
    let SftpConnection::NeedsHostKeyApproval(prompt) = first else {
        // Another cell in this binary may have approved the same fixture already.
        return;
    };
    sftp_volume_wiring::approve_host_key(&prompt.host, prompt.port, &prompt.algorithm, &prompt.fingerprint)
        .await
        .expect(FIXTURE);
}

/// ❗ **A one-shot secret connects and stays out of the store.**
///
/// "Connect once without remembering" used to mean save → dial → delete, and a
/// Keychain entry that exists for a second is not the user's choice. The offer
/// with `remember: false` runs the dial against `one_shot_credentials`, so the
/// server comes up with nothing written behind it.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_one_shot_secret_connects_and_leaves_the_store_empty() {
    let _secrets = crate::test_support::isolate_secrets();
    let params = stock_params();
    host_key_approved(&params).await;
    assert!(
        !keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "the cell starts with nothing stored, which is what makes the dial's source unambiguous"
    );

    let outcome = sftp_volume_wiring::connect_and_register(
        "Fixture server",
        None,
        params.clone(),
        "sftp-one-shot",
        Some(SecretOffer {
            secret: FIXTURE_PASSWORD.to_string(),
            remember: false,
        }),
    )
    .await;
    let SftpConnection::Connected { volume_id } = outcome else {
        panic!("the offered secret is what proves this dial; the agent is off and nothing is stored");
    };

    assert!(
        !keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "❗ a one-shot secret is never written: that is the whole meaning of the switch"
    );

    sftp_volume_wiring::disconnect(&volume_id).await;
}

/// The other half of the switch: `remember: true` writes the secret first, so the
/// next dial (and every unattended reconnect) reads it back the ordinary way.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_remembered_secret_is_in_the_store_after_the_dial() {
    let _secrets = crate::test_support::isolate_secrets();
    let params = stock_params();
    host_key_approved(&params).await;

    let outcome = sftp_volume_wiring::connect_and_register(
        "Fixture server",
        None,
        params.clone(),
        "sftp-remembered",
        Some(SecretOffer {
            secret: FIXTURE_PASSWORD.to_string(),
            remember: true,
        }),
    )
    .await;
    let SftpConnection::Connected { volume_id, .. } = outcome else {
        panic!("a remembered secret is saved before the dial, so the dial reads it back");
    };

    assert!(
        keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "the switch means exactly one thing: the secret is in the store"
    );

    sftp_volume_wiring::disconnect(&volume_id).await;
}

/// ❗ **Forgetting a server drops its session too.** Leaving the session up would
/// keep a switcher row that no store knows about and no second "Forget" can
/// reach. A tab on a forgotten server becomes a home tab; the ordering that
/// makes that work is `crate::commands::DETAILS.md` § `servers.rs`.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_forgetting_a_server_drops_its_session_and_unregisters_it() {
    let _secrets = crate::test_support::isolate_secrets();
    let params = stock_params();
    signed_in_already(&params).await;
    let SftpConnection::Connected { volume_id, .. } =
        sftp_volume_wiring::connect_and_register("fixture", None, params, "fixture-forget-attempt", None).await
    else {
        panic!("a fixture with its key approved and its password stored must connect");
    };
    let manager = crate::file_system::volume::manager::get_volume_manager();
    assert!(manager.get(&volume_id).is_some(), "just registered");

    let forgotten = tokio::time::timeout(
        Duration::from_secs(5),
        crate::commands::servers::forget_server(volume_id.clone()),
    )
    .await
    .expect("a hang here means someone reached for `Sftp::close()`");

    assert!(forgotten);
    assert!(
        manager.get(&volume_id).is_none(),
        "a forgotten server is out of the registry, not a live volume no store can name"
    );
    assert!(
        !sftp_known_servers::all()
            .iter()
            .any(|e| cmdr_fs::volume::sftp_volume_id(&e.host, e.port, &e.username) == volume_id),
        "and out of the saved list"
    );
}

// ── Editing a connected place ────────────────────────────────────────

/// The saved entry an edit sheet sends for the fixture account, unnamed, at
/// `remote_root`.
fn edited(params: &SftpConnectionParams, remote_root: &str) -> KnownSftpServer {
    KnownSftpServer {
        host: params.host.clone(),
        port: params.port,
        username: params.username.clone(),
        display_name: String::new(),
        remote_root: remote_root.to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
        pinned: true,
        last_connected_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// What the app addresses `remote` by on the fixture account.
fn app_path(params: &SftpConnectionParams, remote: &str) -> String {
    format!(
        "{}{remote}",
        cmdr_fs::volume::sftp_app_root(&params.host, params.port, &params.username)
    )
}

/// ❗ **Saving an edit to a connected place applies it live, over the SAME
/// session.** The reported bug's own shape: connected at a narrow root, then
/// widened. The registry serves the wider root at once, nothing redials, the
/// shared retirement flag stays clear, and the reconnect loop still brings the
/// session back after a loss, because it belongs to the connection rather than
/// to the instance the edit replaced.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_widening_a_connected_root_applies_live_over_the_same_session() {
    let _secrets = crate::test_support::isolate_secrets();
    let params = SftpConnectionParams {
        remote_root: PathBuf::from(format!("{FIXTURE_ROOT}/photos")),
        ..stock_params()
    };
    signed_in_already(&params).await;
    let SftpConnection::Connected { volume_id, .. } =
        sftp_volume_wiring::connect_and_register("", None, params.clone(), "sftp-edit-widen", None).await
    else {
        panic!("a fixture with its key approved and its password stored must connect");
    };
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let narrow = manager.get(&volume_id).expect("just registered");
    let session_before = narrow
        .as_any()
        .downcast_ref::<SftpVolume>()
        .expect("an SFTP volume")
        .session_witness()
        .await
        .expect("a live session");

    let outcome = sftp_volume_wiring::save_without_connecting(edited(&params, FIXTURE_ROOT)).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    let wide = manager.get(&volume_id).expect("still registered");
    let wide_root = app_path(&params, FIXTURE_ROOT);
    assert_eq!(wide.root(), Path::new(&wide_root), "the registry serves the wider root");
    assert!(
        wide.exists(Path::new(&app_path(&params, &format!("{FIXTURE_ROOT}/hello.txt"))))
            .await,
        "a file only the wider root covers lists through the live session"
    );
    let sftp = wide.as_any().downcast_ref::<SftpVolume>().expect("an SFTP volume");
    let session_after = sftp.session_witness().await.expect("still a live session");
    assert!(
        std::sync::Weak::ptr_eq(&session_before, &session_after),
        "❗ no redial: the session that served the old root serves the new one"
    );
    assert!(
        !wide.retirement().expect("keeps a flag").is_retired(),
        "❗ the swap retired nothing"
    );
    assert_eq!(
        sftp_known_servers::find(&params.host, params.port, &params.username)
            .expect("still saved")
            .remote_root,
        FIXTURE_ROOT
    );

    sftp.simulate_session_loss().await;
    assert!(
        wide.list_directory(Path::new(&wide_root), None).await.is_err(),
        "an operation is what notices the loss and starts the loop"
    );
    cmdr_fs::testing::wait_until_async(
        Duration::from_secs(30),
        "the reconnect loop to bring the session back",
        || wide.connection_state().is_some_and(ConnectionState::is_live),
    )
    .await;
    assert!(
        wide.list_directory(Path::new(&wide_root), None).await.is_ok(),
        "the redialed session serves the wider root"
    );

    sftp_volume_wiring::disconnect(&volume_id).await;
}

/// ❗ **A root the server doesn't have is refused over the live session, and
/// NOTHING moves**: not the store, not the registry.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_connected_root_the_server_lacks_is_refused_and_writes_nothing() {
    let _secrets = crate::test_support::isolate_secrets();
    let params = stock_params();
    signed_in_already(&params).await;
    let SftpConnection::Connected { volume_id, .. } =
        sftp_volume_wiring::connect_and_register("", None, params.clone(), "sftp-edit-missing-root", None).await
    else {
        panic!("a fixture with its key approved and its password stored must connect");
    };

    let outcome = sftp_volume_wiring::save_without_connecting(edited(&params, "/srv/cmdr-no-such-root")).await;

    assert_eq!(outcome, SavedServerOutcome::RootNotFound);
    assert_eq!(
        sftp_known_servers::find(&params.host, params.port, &params.username)
            .expect("still saved")
            .remote_root,
        FIXTURE_ROOT,
        "❗ a refusal writes nothing"
    );
    let manager = crate::file_system::volume::manager::get_volume_manager();
    assert_eq!(
        manager.get(&volume_id).expect("still registered").root(),
        Path::new(&app_path(&params, FIXTURE_ROOT))
    );

    sftp_volume_wiring::disconnect(&volume_id).await;
}

//! The wire vocabulary, which is the whole point of this module: the frontend
//! branches on these values and never on a message.

use super::*;

/// Every rung the backend can report has a wire name, and the two key-file cases
/// are two names.
///
/// ❗ Flattening them would hand the frontend one "key file" rung whose reconnect
/// behavior differs invisibly: one comes back on its own, the other can't.
#[test]
fn every_auth_rung_crosses_as_its_own_value() {
    assert_eq!(SftpAuthRung::from(AuthRungUsed::Agent), SftpAuthRung::Agent);
    assert_eq!(
        SftpAuthRung::from(AuthRungUsed::KeyFile {
            passphrase_protected: false
        }),
        SftpAuthRung::KeyFile
    );
    assert_eq!(
        SftpAuthRung::from(AuthRungUsed::KeyFile {
            passphrase_protected: true
        }),
        SftpAuthRung::EncryptedKeyFile
    );
    assert_eq!(SftpAuthRung::from(AuthRungUsed::Password), SftpAuthRung::Password);
    assert_eq!(
        SftpAuthRung::from(AuthRungUsed::KeyboardInteractive),
        SftpAuthRung::KeyboardInteractive
    );
}

/// The secret store is keyed per account, ❌ never per host.
///
/// Two accounts on one server sharing an entry means a reconnect can retry the
/// wrong account's secret, and enough of those lock an account.
#[test]
fn the_credential_service_carries_the_port_and_the_scope_carries_the_account() {
    assert_eq!(credential_key("naspolya", 22), "naspolya:22");
    assert_ne!(
        credential_key("naspolya", 22),
        credential_key("naspolya", 2222),
        "a jump box and a container on one machine are different servers"
    );
}

/// The connect outcome is tagged, so the frontend switches on a field rather
/// than sniffing which key is present.
#[test]
fn the_connect_outcome_names_itself_on_the_wire() {
    let refused = serde_json::to_value(SftpConnectResult::AuthenticationRejected).expect("serializes");
    assert_eq!(refused["outcome"], "authentication_rejected");

    let needs = serde_json::to_value(SftpConnectResult::NeedsHostKeyApproval(HostKeyPrompt {
        host: "naspolya".to_string(),
        port: 22,
        algorithm: "ssh-ed25519".to_string(),
        fingerprint: "SHA256:aaa".to_string(),
        kind: cmdr_sftp::transport::HostKeyPromptKind::Changed,
    }))
    .expect("serializes");
    assert_eq!(needs["outcome"], "needs_host_key_approval");
    assert_eq!(
        needs["kind"], "changed",
        "❗ a changed key must be distinguishable from a first-seen one without reading prose"
    );
}

/// The three credential commands have to agree on the key, or a saved password
/// is invisible to the check that decides whether to show a sign-in form.
///
/// Round-tripped through the real store (the test backend), because the risk
/// isn't in any one of them: it's the three passing the service and scope
/// differently.
#[tokio::test]
async fn the_credential_trio_agrees_on_where_a_secret_lives() {
    let _secrets = crate::test_support::isolate_secrets();
    let host = "credential-trio.sftp.test";

    assert!(
        !has_sftp_credentials(host.to_string(), 22, "ada".to_string()).await,
        "nothing is stored before anything is saved"
    );

    save_sftp_credentials(host.to_string(), 22, "ada".to_string(), "pa55".to_string())
        .await
        .expect("the test store always accepts");
    assert!(has_sftp_credentials(host.to_string(), 22, "ada".to_string()).await);

    // ❗ Another account on the same server is a different entry, not a shared
    // one: a reconnect that retried the wrong account's secret is how an account
    // gets locked.
    assert!(!has_sftp_credentials(host.to_string(), 22, "grace".to_string()).await);
    // And so is the same account on another port.
    assert!(!has_sftp_credentials(host.to_string(), 2222, "ada".to_string()).await);

    delete_sftp_credentials(host.to_string(), 22, "ada".to_string())
        .await
        .expect("the test store always accepts");
    assert!(!has_sftp_credentials(host.to_string(), 22, "ada".to_string()).await);
}

/// The known-servers commands round-trip through the same store the connect path
/// writes.
#[tokio::test]
async fn the_known_servers_trio_round_trips() {
    let host = "known-servers-trio.sftp.test";
    update_known_sftp_server(
        host.to_string(),
        22,
        "ada".to_string(),
        "Trio".to_string(),
        "/srv/data".to_string(),
        None,
        true,
        false,
    );

    let mine: Vec<_> = get_known_sftp_servers()
        .into_iter()
        .filter(|entry| entry.host == host)
        .collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].display_name, "Trio");
    assert!(
        !mine[0].auto_reconnect,
        "❗ the switch is the user's, so editing a server has to be able to turn it off"
    );

    assert!(forget_known_sftp_server(host.to_string(), 22, "ada".to_string()));
    assert!(
        !get_known_sftp_servers().iter().any(|entry| entry.host == host),
        "a forgotten server is gone from the list"
    );
}

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

/// ❌ **A sign-in button that can only answer `NotSupported` must never be
/// shown.** The two rungs that come back on their own have no secret a person
/// could supply, and this is the answer the banner reads.
#[test]
fn only_the_rungs_a_typed_secret_can_mend_offer_a_sign_in() {
    assert_eq!(
        SftpSignInPrompt::for_rung(SftpAuthRung::Agent),
        SftpSignInPrompt::Nothing
    );
    assert_eq!(
        SftpSignInPrompt::for_rung(SftpAuthRung::KeyFile),
        SftpSignInPrompt::Nothing
    );
    assert_eq!(
        SftpSignInPrompt::for_rung(SftpAuthRung::EncryptedKeyFile),
        SftpSignInPrompt::KeyPassphrase,
        "❗ the passphrase is used for that session and never saved"
    );
    assert_eq!(
        SftpSignInPrompt::for_rung(SftpAuthRung::Password),
        SftpSignInPrompt::Password
    );
    assert_eq!(
        SftpSignInPrompt::for_rung(SftpAuthRung::KeyboardInteractive),
        SftpSignInPrompt::Password
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
    assert_eq!(refused["outcome"], "authenticationRejected");

    let needs = serde_json::to_value(SftpConnectResult::NeedsHostKeyApproval(HostKeyPrompt {
        host: "naspolya".to_string(),
        port: 22,
        algorithm: "ssh-ed25519".to_string(),
        fingerprint: "SHA256:aaa".to_string(),
        kind: cmdr_sftp::transport::HostKeyPromptKind::Changed,
    }))
    .expect("serializes");
    assert_eq!(needs["outcome"], "needsHostKeyApproval");
    assert_eq!(
        needs["kind"], "changed",
        "❗ a changed key must be distinguishable from a first-seen one without reading prose"
    );
}

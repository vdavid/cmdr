//! Tests for `smb_connect_directly.rs`: what "Connect directly" answers when
//! there's no OS-mounted SMB share behind the volume anymore.

use super::*;
use crate::file_system::volume::LocalPosixVolume;
use crate::test_support::TestDir;
use std::sync::Arc;

/// An id no test ever registers a volume under.
const NEVER_REGISTERED: &str = "test-connect-directly-never-registered";

/// A volume registered for the length of one test, under an id no other test uses.
struct Registered(String);

impl Registered {
    fn at(label: &str, root: &std::path::Path) -> Self {
        let id = format!("test-connect-directly-{label}");
        get_volume_manager().register(&id, Arc::new(LocalPosixVolume::new(label, root)));
        Self(id)
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.0);
    }
}

/// The ERR-SHUSC shape: the notice outlived its volume, and every press of its
/// button ran into an id nothing is registered under.
#[tokio::test]
async fn an_id_nothing_is_registered_under_answers_volume_gone() {
    let answer = connect_directly(NEVER_REGISTERED).await;

    assert!(matches!(answer, UpgradeResult::VolumeGone), "got {answer:?}");
}

/// The volume is still registered, but its mount point went away before the
/// mount watcher caught up.
#[tokio::test]
async fn a_volume_whose_mount_point_is_gone_answers_volume_gone() {
    let dir = TestDir::new("connect_directly_unmounted");
    let volume = Registered::at("unmounted", &dir.join("unmounted"));

    let answer = connect_directly(&volume.0).await;

    assert!(matches!(answer, UpgradeResult::VolumeGone), "got {answer:?}");
}

/// Something is mounted there, just not an SMB share.
#[tokio::test]
async fn a_volume_on_another_filesystem_answers_not_smb_mount() {
    let dir = TestDir::new("connect_directly_local");
    let volume = Registered::at("local", &dir);

    let answer = connect_directly(&volume.0).await;

    assert!(matches!(answer, UpgradeResult::NotSmbMount), "got {answer:?}");
}

/// The sign-in sheet's door looks first too, so a share that went away while the
/// user typed comes back as gone rather than as a breakdown.
#[tokio::test]
async fn the_sign_in_door_answers_volume_gone_too() {
    let answer = connect_directly_with_credentials(
        NEVER_REGISTERED,
        Some("david".to_string()),
        Some("hunter2".to_string()),
        false,
    )
    .await;

    assert!(matches!(answer, UpgradeResult::VolumeGone), "got {answer:?}");
}

/// What the sign-in sheet opens saying, for each refusal.
///
/// A guest turned away anywhere offered nothing, so it's asked for a sign-in. An
/// account the SHARE turned away signed in fine: answering "wrong password" kept the
/// sheet asking for a password that worked, round after round (ERR-SHUSC).
#[test]
fn each_refusal_tells_the_sheet_what_to_ask_for() {
    use crate::network::smb_connect_failure::{RefusedAt, SignInIdentity};

    let table = [
        (
            SignInIdentity::Guest,
            RefusedAt::SignIn,
            CredentialsNeededReason::NoCredential,
        ),
        (
            SignInIdentity::Guest,
            RefusedAt::Share,
            CredentialsNeededReason::NoCredential,
        ),
        (
            SignInIdentity::Account,
            RefusedAt::SignIn,
            CredentialsNeededReason::CredentialRejected,
        ),
        (
            SignInIdentity::Account,
            RefusedAt::Share,
            CredentialsNeededReason::AccountNotPermitted,
        ),
    ];
    for (identity, at, expected) in table {
        assert_eq!(
            CredentialsNeededReason::from(Refusal { identity, at }),
            expected,
            "{identity:?} refused at {at:?}"
        );
    }
}

/// The wire shape the frontend switches on: a camelCase tag, and no English
/// sentence beside it.
#[test]
fn the_reason_crosses_ipc_as_a_tag() {
    let answer = UpgradeResult::CredentialsNeeded {
        server: "observermch".to_string(),
        share: "data".to_string(),
        port: 445,
        display_name: "observermch".to_string(),
        username_hint: Some("ada".to_string()),
        reason: CredentialsNeededReason::AccountNotPermitted,
    };
    let json = serde_json::to_value(&answer).expect("an UpgradeResult serializes");

    assert_eq!(json["status"], "credentialsNeeded");
    assert_eq!(json["reason"], "accountNotPermitted");
    assert_eq!(json.get("message"), None);
}

/// The saved-password door looks before it could raise the Keychain consent
/// dialog, so a gone share never costs the user a system prompt.
#[tokio::test]
async fn the_saved_password_door_answers_volume_gone_too() {
    let answer = connect_directly_with_system_saved_password(NEVER_REGISTERED).await;

    assert!(matches!(answer, UpgradeResult::VolumeGone), "got {answer:?}");
}

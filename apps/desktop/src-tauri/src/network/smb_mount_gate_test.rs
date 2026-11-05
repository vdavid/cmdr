//! The kernel-mount gate under "Always require SMB encryption" (#966): what
//! `smbutil statshares` says, and what the gate does with it. Nextest runs each
//! test in its own process, so a switch flipped here reaches no other test.

use super::*;
use crate::test_support::TestDir;

/// `smbutil statshares -m` on a share mounted from the `auth` fixture as is (Samba
/// offers a cipher and asks for none, so macOS doesn't encrypt), trimmed to its
/// header and the rows around the one read (macOS 27.0.1, 2026-11-05).
const IN_THE_CLEAR: &str = "
==================================================================================================
SHARE                         ATTRIBUTE TYPE                VALUE
==================================================================================================
private
                              SERVER_NAME                   127.0.0.1
                              SMB_VERSION                   SMB_3.1.1
                              SMB_ENCRYPT_ALGORITHMS        AES_128_CCM_ENABLED
                              SMB_ENCRYPT_ALGORITHMS        AES_256_GCM_ENABLED
                              SMB_CURR_ENCRYPT_ALGORITHM    OFF
                              SMB_SIGN_ALGORITHMS           AES_128_CMAC_ENABLED
                              SMB_CURR_SIGN_ALGORITHM       AES_128_GMAC
                              SIGNING_ON                    TRUE

--------------------------------------------------------------------------------------------------
";

/// The same fixture with `smb encrypt = required`, which makes macOS encrypt.
const ENCRYPTED: &str = "
==================================================================================================
SHARE                         ATTRIBUTE TYPE                VALUE
==================================================================================================
private
                              SERVER_NAME                   127.0.0.1
                              SMB_VERSION                   SMB_3.1.1
                              SMB_ENCRYPT_ALGORITHMS        AES_128_CCM_ENABLED
                              SMB_CURR_ENCRYPT_ALGORITHM    AES-128-GCM
                              SMB_CURR_SIGN_ALGORITHM       AES_128_GMAC
                              ENCRYPTION_REQUIRED           TRUE

--------------------------------------------------------------------------------------------------
";

/// What `smbutil statshares -m` prints for a path that isn't an SMB mount (exit 64).
const NOT_A_MOUNT: &str = "
==================================================================================================
SHARE                         ATTRIBUTE TYPE                VALUE
==================================================================================================

--------------------------------------------------------------------------------------------------
";

#[test]
fn statshares_says_which_mounts_are_encrypted() {
    assert_eq!(parse_statshares(IN_THE_CLEAR), MountEncryption::NotEncrypted);
    assert_eq!(parse_statshares(ENCRYPTED), MountEncryption::Encrypted);
}

/// No row, or a row with no value, proves nothing, and the gate keeps such a mount refused.
#[test]
fn a_table_without_the_row_proves_nothing() {
    assert_eq!(parse_statshares(NOT_A_MOUNT), MountEncryption::Unknown);
    assert_eq!(parse_statshares(""), MountEncryption::Unknown);
    assert_eq!(
        parse_statshares("                              SMB_CURR_ENCRYPT_ALGORITHM\n"),
        MountEncryption::Unknown
    );
}

fn gate_over(dir: &TestDir) -> UnverifiedSmbMount {
    UnverifiedSmbMount::new(LocalPosixVolume::on_mount(
        "share",
        dir.as_ref(),
        MountClass::SmbShare,
        crate::volume_host::host(),
    ))
}

/// A mount that never proves encrypted (a scratch folder isn't an SMB mount at all,
/// so the probe can't tell) is refused while encryption is required, listing,
/// reading, and writing alike, with the typed error that names the setting.
#[tokio::test]
async fn an_unproven_mount_is_refused_while_encryption_is_required() {
    let dir = TestDir::new("smb-gate-refused");
    std::fs::write(dir.join("a.txt"), b"a").expect("writes the fixture");
    cmdr_smb::SmbEncryption::set_required(true);
    let gate = gate_over(&dir);

    assert!(matches!(
        gate.list_directory(&dir, None).await,
        Err(VolumeError::EncryptionRequired(_))
    ));
    assert!(matches!(
        gate.get_metadata(&dir.join("a.txt")).await,
        Err(VolumeError::EncryptionRequired(_))
    ));
    assert!(matches!(
        gate.open_read_stream(&dir.join("a.txt")).await,
        Err(VolumeError::EncryptionRequired(_))
    ));
    assert!(matches!(
        gate.create_directory(&dir.join("new")).await,
        Err(VolumeError::EncryptionRequired(_))
    ));
    assert!(!dir.join("new").exists(), "nothing went through the mount");
    cmdr_smb::SmbEncryption::set_required(false);
}

/// The requirement lifted: the gate hands the listing to the mount at once, before
/// the registry has swapped it out.
#[tokio::test]
async fn a_lifted_requirement_lets_the_listing_through() {
    let dir = TestDir::new("smb-gate-lifted");
    std::fs::write(dir.join("a.txt"), b"a").expect("writes the fixture");
    cmdr_smb::SmbEncryption::set_required(false);
    let gate = gate_over(&dir);

    let entries = gate.list_directory(&dir, None).await.expect("lists through the mount");
    assert_eq!(entries.len(), 1);
}

/// Flipping the requirement re-registers the SMB kernel mounts the registry serves,
/// both ways, and leaves every other volume alone.
#[tokio::test]
async fn a_flip_moves_every_smb_mount_behind_the_gate_and_back() {
    let share = TestDir::new("smb-gate-flip-share");
    let disk = TestDir::new("smb-gate-flip-disk");
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let (share_id, disk_id) = ("test-smb-gate-flip-share", "test-smb-gate-flip-disk");
    cmdr_smb::SmbEncryption::set_required(false);
    manager.register(share_id, mount_volume("share", &share, MountClass::SmbShare));
    manager.register(disk_id, mount_volume("disk", &disk, MountClass::LocalDisk));

    cmdr_smb::SmbEncryption::set_required(true);
    follow_requirement().await;
    let gated = manager.get(share_id).expect("the share stays registered");
    assert!(
        gated.as_any().is::<UnverifiedSmbMount>(),
        "the share is behind the gate"
    );
    assert!(refuses(share_id).await, "a scratch folder never proves encrypted");
    assert!(
        manager
            .get(disk_id)
            .expect("registered")
            .as_any()
            .is::<LocalPosixVolume>()
    );

    cmdr_smb::SmbEncryption::set_required(false);
    follow_requirement().await;
    assert!(
        manager
            .get(share_id)
            .expect("registered")
            .as_any()
            .is::<LocalPosixVolume>(),
        "the requirement lifted hands the share back to the plain mount"
    );
    assert!(!refuses(share_id).await);

    manager.unregister(share_id);
    manager.unregister(disk_id);
}

/// Every SMB kernel mount goes through the gate while encryption is required, and
/// none does while it isn't; other mounts never do.
#[tokio::test]
async fn the_gate_stands_only_for_an_smb_mount_under_the_requirement() {
    let dir = TestDir::new("smb-gate-choice");
    cmdr_smb::SmbEncryption::set_required(false);
    assert!(
        mount_volume("share", &dir, MountClass::SmbShare)
            .as_any()
            .is::<LocalPosixVolume>()
    );
    cmdr_smb::SmbEncryption::set_required(true);
    assert!(
        mount_volume("share", &dir, MountClass::SmbShare)
            .as_any()
            .is::<UnverifiedSmbMount>()
    );
    assert!(
        mount_volume("disk", &dir, MountClass::OtherNetwork)
            .as_any()
            .is::<LocalPosixVolume>()
    );
    cmdr_smb::SmbEncryption::set_required(false);
}

use super::*;

/// ❗ A mounted SMB/AFP share has no scheme, so a refusal on one used to read the
/// errno (or nothing), which sent a delete to Finder's Locked box. The file server
/// said no, so the advice is the server's. The errno still rides for the details,
/// and the folder goes: `access(W_OK)` proves nothing about a share.
#[test]
fn a_refusal_on_a_mounted_share_is_the_servers_whatever_the_errno() {
    for errno in [None, Some(libc::EACCES), Some(libc::EPERM)] {
        let WriteOperationError::PermissionDenied {
            refusal,
            errno: kept,
            refused_folder,
            ..
        } = WriteOperationError::permission_denied_on(
            "/Volumes/testshare/a.txt".to_string(),
            "denied".to_string(),
            errno,
            Some("/Volumes/testshare".to_string()),
            None,
            true,
        )
        else {
            panic!("expected PermissionDenied");
        };
        assert_eq!(refusal, PermissionRefusal::Server, "{errno:?}");
        assert_eq!(kept, errno);
        assert_eq!(refused_folder, None);
    }
}

#[test]
fn a_refusal_off_a_share_still_reads_its_errno_and_keeps_its_folder() {
    let WriteOperationError::PermissionDenied {
        refusal,
        refused_folder,
        ..
    } = WriteOperationError::permission_denied_on(
        "/Users/test_user_1/a.txt".to_string(),
        "denied".to_string(),
        Some(libc::EACCES),
        Some("/Users/test_user_1".to_string()),
        None,
        false,
    )
    else {
        panic!("expected PermissionDenied");
    };
    assert_eq!(refusal, PermissionRefusal::FolderPermissions);
    assert_eq!(refused_folder.as_deref(), Some("/Users/test_user_1"));
}

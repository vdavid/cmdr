//! What `path_exists` says when a local `stat` fails for a reason other than "not
//! there": "couldn't tell", ❌ never "gone". A FUSE mount that answers EIO or
//! ETIMEDOUT (pCloud's `pcloudfs`, cmdr-reports#4) otherwise reads as deleted, and
//! the pane's walk-up leaves the drive for the boot disk's home folder.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{LocalPosixVolume, Volume};
use crate::test_support::TestDir;

use super::{local_miss_is_definite, path_exists};

#[test]
fn only_not_found_and_not_a_folder_are_a_definite_miss() {
    for errno in [libc::ENOENT, libc::ENOTDIR] {
        assert!(
            local_miss_is_definite(&io::Error::from_raw_os_error(errno)),
            "errno {errno}"
        );
    }
    for errno in [
        libc::EIO,
        libc::ETIMEDOUT,
        libc::EACCES,
        libc::ENXIO,
        libc::EPERM,
        libc::ENOTCONN,
    ] {
        assert!(
            !local_miss_is_definite(&io::Error::from_raw_os_error(errno)),
            "errno {errno}"
        );
    }
}

/// A real non-NotFound error end to end: `stat` under a folder nobody may search
/// answers EACCES, which says nothing about whether the child is there.
#[tokio::test]
async fn a_stat_the_os_refuses_is_couldnt_tell() {
    // Root bypasses the search bit (CAP_DAC_OVERRIDE), so EACCES can't be produced
    // there, and the local Linux test lane runs in Docker as root.
    // SAFETY: geteuid takes no arguments, touches no memory, and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = TestDir::new("path-exists-refused");
    let locked = dir.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::create_dir(locked.join("child")).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let id = format!("path-exists-errors-{}", uuid::Uuid::new_v4());
    let volume = LocalPosixVolume::new("Temp", &*dir);
    get_volume_manager().register(&id, Arc::new(volume) as Arc<dyn Volume>);
    let child = locked.join("child").to_string_lossy().into_owned();
    let refused = path_exists(Some(id.clone()), child).await;
    let gone = path_exists(Some(id.clone()), dir.join("nope").to_string_lossy().into_owned()).await;
    get_volume_manager().unregister(&id);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(refused.timed_out && !refused.data, "{refused:?}");
    assert!(!gone.timed_out && !gone.data, "{gone:?}");
}

//! What each OS's own SMB client does for Cmdr: mount a share as a kernel mount, list a host's
//! shares with the OS's command-line tool when smb2 can't, and say whether its sign-in helper
//! can wedge. A platform seam (`docs/guides/platform-seams.md`).

use std::future::Future;

use cmdr_smb::{ShareListError, ShareListResult};

use super::{MountError, MountResult};
use crate::network::server_identity::SmbServer;

#[cfg_attr(target_os = "macos", path = "macos/mod.rs")]
#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

/// What `smbutil view` printed, read into shares. The macOS arm runs the command; the tests
/// read its output on every OS.
#[cfg(any(target_os = "macos", test))]
mod smbutil_output;

type ShareList = Result<ShareListResult, ShareListError>;
type MountAnswer = Result<MountResult, MountError>;

const _: fn(&str, &str, Option<&str>, Option<&str>, u16) -> MountAnswer = mount_share_sync;
const _: fn(&[SmbServer]) -> Vec<String> = unmount_smb_shares_from_host;
const _: fn() -> Option<u32> = sign_in_helper_pid;
const _: bool = CAN_WEDGE;
// An `async fn` doesn't coerce to a fn pointer, so these pins name each one's arguments and the
// output its future resolves to.
const _: fn() = || {
    fn resolves_to<T, F: Future<Output = T>>(_: F) {}
    resolves_to::<ShareList, _>(list_shares_cli_as_guest("", None, 0));
    resolves_to::<ShareList, _>(list_shares_cli_with_keychain("", None, 0));
    resolves_to::<Option<ShareList>, _>(list_shares_cli_with_password("", None, 0, "", ""));
};

/// Mounts `share` on `server` through the OS, blocking until it answers. Call it through
/// [`super::mount_share`], which bounds it, reads a give-up, and words a "not found".
pub(crate) use imp::mount_share_sync;

/// Unmounts every SMB share the OS has mounted from `targets`, answering the mount paths that
/// went.
pub(crate) use imp::unmount_smb_shares_from_host;

/// Whether this OS's mount can wedge in a sign-in helper (macOS's `NetAuthSysAgent`).
pub(crate) use imp::CAN_WEDGE;

/// The sign-in helper's pid right now, or `None` when it isn't running or this OS has none.
pub(crate) use imp::sign_in_helper_pid;

/// The host's shares as a guest, from the OS's own command-line client.
pub(crate) use imp::list_shares_cli_as_guest;

/// The host's shares under the credentials the OS itself keeps for it (macOS's Keychain), from
/// its own command-line client.
pub(crate) use imp::list_shares_cli_with_keychain;

/// The host's shares under an explicit username and password, from the OS's own command-line
/// client, or `None` when that client can't take a password without putting it in argv.
pub(crate) use imp::list_shares_cli_with_password;

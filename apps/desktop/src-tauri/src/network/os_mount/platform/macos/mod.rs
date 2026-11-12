//! macOS: NetFS mounts (`mount.rs`), `smbutil view` for share lists (`smbutil.rs`), and the
//! NetFS sign-in helper (`NetAuthSysAgent`) a mount can wedge in.

mod mount;
mod smbutil;

use cmdr_smb::{ShareListError, ShareListResult};

pub(crate) use mount::{mount_share_sync, unmount_smb_shares_from_host};
pub(crate) use smbutil::{list_shares_cli_as_guest, list_shares_cli_with_keychain};

/// NetFS signs in through `NetAuthSysAgent`, which can wedge.
pub(crate) const CAN_WEDGE: bool = true;

/// A NetFS mount, on a Mac with a Local Network permission that can refuse Cmdr's own route.
pub(crate) const MOUNT_LOG_WORDS: super::MountLogWords = super::MountLogWords {
    mount: "the macOS kernel mount",
    machine: "this Mac",
    route_blockers: "the Local Network permission, a firewall app",
};

/// The helper's process name, for the restart check.
const SIGN_IN_HELPER: &str = "NetAuthSysAgent";

/// The sign-in helper's pid right now, or `None` when it isn't running (launchd starts it on
/// demand).
pub(crate) fn sign_in_helper_pid() -> Option<u32> {
    pids_named(SIGN_IN_HELPER).into_iter().next()
}

/// `None`: `smbutil view` has no argv-free channel for an explicit password (the
/// `//user:password@host` URL is the only one, and it leaks the cleartext password into the
/// `ps`-readable process argument list). So Cmdr never shells out to smbutil with credentials,
/// and `smb_client.rs` surfaces the underlying smb2 failure instead. The guest and Keychain
/// listings (smbutil reads the system Keychain itself, no password in argv) are kept.
pub(crate) async fn list_shares_cli_with_password(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
    _username: &str,
    _password: &str,
) -> Option<Result<ShareListResult, ShareListError>> {
    None
}

/// The pids of every process named `name`, by `proc_listallpids` + `proc_name`
/// (what `pgrep` reads, without a subprocess). A process running as root is
/// listed too.
fn pids_named(name: &str) -> Vec<u32> {
    // SAFETY: a null buffer with size 0 is the documented way to ask
    // `proc_listallpids` how many pids there are; it writes nothing.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    // Headroom for processes started between the two calls.
    let mut pids: Vec<libc::c_int> = vec![0; count + 64];
    let Ok(buffer_bytes) = libc::c_int::try_from(pids.len() * size_of::<libc::c_int>()) else {
        return Vec::new();
    };
    // SAFETY: `pids` owns `buffer_bytes` bytes of writable, `c_int`-aligned memory,
    // and `proc_listallpids` writes at most that many.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), buffer_bytes) };
    let Ok(filled) = usize::try_from(filled) else {
        return Vec::new();
    };
    pids.truncate(filled.min(pids.len()));
    pids.into_iter()
        .filter_map(|pid| u32::try_from(pid).ok())
        .filter(|&pid| pid != 0 && process_name(pid).as_deref() == Some(name))
        .collect()
}

/// A process's name, as `proc_name` reads it, or `None` when it's gone.
fn process_name(pid: u32) -> Option<String> {
    let pid = libc::c_int::try_from(pid).ok()?;
    // `PROC_PIDPATHINFO_MAXSIZE` is generous; a name is at most `2 * MAXCOMLEN` (32).
    let mut buffer = [0u8; 256];
    let size = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is `size` bytes of writable memory, and `proc_name` writes at
    // most that many, answering how many it wrote.
    let written = unsafe { libc::proc_name(pid, buffer.as_mut_ptr().cast(), size) };
    let written = usize::try_from(written).ok().filter(|&n| n > 0)?;
    let bytes = buffer.get(..written)?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pid lookup the restart check rides on finds a process by its name.
    #[test]
    fn a_running_process_is_found_by_name() {
        let own = std::process::id();
        let name = process_name(own).expect("this process has a name");
        assert!(pids_named(&name).contains(&own));
        assert!(pids_named("no-such-process-cmdr-test").is_empty());
    }
}

//! The half of SMB that goes through the OS rather than smb2: mounting a share as a kernel
//! mount, the OS's own share-list tools for a server smb2 can't list, and macOS's NetFS sign-in
//! helper wedging under a mount. The per-OS code is behind `platform/`; what's here is shared.
//!
//! The rest of SMB's OS-mount handling reads the mount table rather than calling the OS, so it
//! lives beside the code it serves: `smb_upgrade` (taking a kernel mount over), `smb_mount_gate`
//! (an unencrypted mount under the encryption requirement), `os_mount_notice` (the "still on the
//! kernel mount" notice), and `keychain` (the SMB credentials, through `crate::secrets`).

mod mount_watch;
pub(crate) mod netfs_wedge;
mod platform;
// Asks the server itself why a mount found no share; `mount_share` below asks it for every OS.
mod share_access;

use serde::{Deserialize, Serialize};

pub(crate) use platform::{
    list_shares_cli_as_guest, list_shares_cli_with_keychain, list_shares_cli_with_password,
    unmount_smb_shares_from_host,
};

/// Result of a successful mount operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MountResult {
    /// For example, "/Volumes/Documents".
    pub mount_path: String,
    pub already_mounted: bool,
}

/// Why a mount didn't go through, as the data a person's message is built from.
///
/// ❌ No sentences: the frontend words every variant from the catalog
/// (`file-explorer/network/mount-error-messages.ts`), so a translated UI never shows
/// an English one (ERR-SHUSC). `server` is the address the mount used; the pane
/// names the host by its own name where it has one. Every OS's arm answers this one type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum MountError {
    /// Nothing answered at the server's address: the connection was refused or had no route.
    HostUnreachable { server: String },
    /// The server didn't answer within the mount's budget.
    Timeout { server: String },
    /// macOS's network sign-in helper (`NetAuthSysAgent`) looks wedged: a mount
    /// waited it out while the server answered on its SMB port, or an earlier one
    /// is still stuck in it. Only restarting the helper clears it
    /// (`network/os_mount/netfs_wedge.rs`). macOS only.
    SignInHelperStuck { server: String },
    /// The server has no share by that name, or offers none to this identity.
    ShareNotFound { server: String, share: String },
    /// The server wants a sign-in before it opens the share: a guest was turned away.
    AuthRequired { server: String, share: String },
    /// The server turned the offered username and password away.
    AuthFailed { server: String },
    /// The account signed in, and the share doesn't let it in.
    PermissionDenied {
        server: String,
        share: String,
        username: String,
    },
    /// The mount was canceled before it finished.
    Cancelled { share: String },
    /// The server offers no SMB version this computer's client speaks.
    UnsupportedProtocol { server: String },
    /// The server accepted the sign-in, then wouldn't mount the share.
    MountRefused { server: String, share: String },
    /// The system reported the share connected, and no mount of it is there.
    MountMissing { server: String, share: String },
    /// Linux only: `gio` isn't installed, so there's nothing to mount with.
    GvfsMissing,
    /// Anything the variants above don't describe.
    Unexpected {
        server: String,
        share: String,
        /// What NetFS, `gio`, or the mount task said, for the log. ❌ Never shown to a person.
        detail: String,
    },
}

/// How long a mount attempt may run before we give up on it, in milliseconds.
/// Shared by both platform backends, so a share on a slow server gets the same
/// patience on macOS and on Linux.
const DEFAULT_MOUNT_TIMEOUT_MS: u64 = 20_000;
// Reasonable patience: long enough for a slow NAS, short enough that a dead one doesn't look alive.
const _: () = assert!(DEFAULT_MOUNT_TIMEOUT_MS >= 10_000 && DEFAULT_MOUNT_TIMEOUT_MS <= 60_000);

/// Mounts an SMB share through the platform backend's blocking
/// `platform::mount_share_sync`, on the blocking pool under the mount timeout, until
/// `cancel` fires.
///
/// A panicked task and an expired budget come back as the typed `MountError` the
/// frontend words, and a mount that could only say "not found" spends what's left
/// of the budget asking the server which it was
/// (`share_access::clarify_share_not_found`). A guest's `AuthFailed` reads as the
/// sign-in it is (`share_access::refusal_for_identity`). One function for both
/// platforms, so the timeout and these readings can't drift apart between macOS
/// and Linux.
///
/// ❗ A timeout or a cancel doesn't stop the NetFS call: its thread stays blocked
/// inside NetFS. `netfs_wedge` reads a long wait on a server that answers as a
/// wedged `NetAuthSysAgent`, and while such a call is still stuck, this answers
/// `SignInHelperStuck` right away rather than queue another call behind it.
pub async fn mount_share(
    server: String,
    share: String,
    username: Option<String>,
    password: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<MountResult, MountError> {
    use netfs_wedge::{GaveUpWhy, NETFS_CALLS};

    if let Some(refusal) = netfs_wedge::refusal_while_stuck(&NETFS_CALLS, platform::sign_in_helper_pid(), &server) {
        return Err(refusal);
    }
    let attempt = share_access::ShareAttempt::new(&server, &share, port, username.as_deref(), password.as_deref());
    let started = std::time::Instant::now();
    let timeout_duration = std::time::Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_MOUNT_TIMEOUT_MS));
    // Rides the blocking work, not this future: the call outlives our timeout when the
    // system's mount service wedges, and that's exactly when its log lines matter.
    let watch = mount_watch::MountWatch::start(&server, &share, mount_watch::SLOW_MOUNT);
    let call = NETFS_CALLS.start();
    let call_id = call.id();
    let mount_future = tokio::task::spawn_blocking(move || {
        // Held until NetFS lets the thread go, which can be long after the mount gave up on it.
        let _call = call;
        let result = platform::mount_share_sync(&server, &share, username.as_deref(), password.as_deref(), port);
        watch.finish(result.is_ok());
        result
    });

    let gave_up = tokio::select! {
        joined = tokio::time::timeout(timeout_duration, mount_future) => match joined {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(join_error)) => Ok(Err(MountError::Unexpected {
                server: attempt.server().to_string(),
                share: attempt.share().to_string(),
                detail: format!("the mount task didn't finish: {join_error}"),
            })),
            Err(_timeout) => Err(GaveUpWhy::TimedOut),
        },
        () = cancel.cancelled() => Err(GaveUpWhy::Cancelled),
    };
    let result = match gave_up {
        Ok(result) => result,
        Err(why) => Err(read_given_up(&attempt, port, call_id, why, started.elapsed()).await),
    };

    match result {
        Err(not_found @ MountError::ShareNotFound { .. }) => {
            let budget_left = timeout_duration.saturating_sub(started.elapsed());
            Err(share_access::clarify_share_not_found(not_found, &attempt, budget_left).await)
        }
        Err(refusal) => Err(share_access::refusal_for_identity(refusal, &attempt)),
        Ok(mounted) => Ok(mounted),
    }
}

/// What [`mount_share`] answers for a NetFS call it stopped waiting on.
async fn read_given_up(
    attempt: &share_access::ShareAttempt,
    port: u16,
    call: netfs_wedge::CallId,
    why: netfs_wedge::GaveUpWhy,
    waited: std::time::Duration,
) -> MountError {
    log::warn!(
        "Mounting share={:?} on server={:?} stopped waiting after {} s ({why:?})",
        cmdr_fs::log_private::LogPrivate::volume(attempt.share()),
        cmdr_fs::log_private::LogPrivate::host(attempt.server()),
        waited.as_secs()
    );
    let mount = netfs_wedge::GivenUpMount {
        server: attempt.server(),
        share: attempt.share(),
        port,
    };
    netfs_wedge::read_given_up(
        &netfs_wedge::NETFS_CALLS,
        call,
        why,
        waited,
        mount,
        platform::sign_in_helper_pid,
    )
    .await
}

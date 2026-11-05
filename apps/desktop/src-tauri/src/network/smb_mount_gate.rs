//! "Always require SMB encryption" on the kernel mount: while it's on, an SMB share
//! macOS (or GVFS) mounted is browsed only when that mount's own session is
//! encrypted (#966).
//!
//! Cmdr's direct smb2 session follows the requirement itself (`crates/cmdr-smb/DETAILS.md`
//! § "Requiring encryption"). The kernel mount doesn't: macOS negotiates it on its
//! own terms, and against a server that only OFFERS encryption it runs in the clear.
//! So under the requirement every SMB kernel mount registers as an
//! [`UnverifiedSmbMount`], which refuses with `VolumeError::EncryptionRequired` until
//! the mount proves encrypted (`smbutil statshares`), at which point the plain
//! `LocalPosixVolume` takes its place. The direct, encrypted session replacing it
//! (the usual way out) goes through `smb_upgrade` as for any kernel mount.
//!
//! Why refuse instead of browsing through the mount with a warning: the setting's
//! promise is that Cmdr doesn't use an unencrypted SMB connection, and the
//! organization's `RequireSMBEncryption` makes the same promise for them.
//!
//! Why "unless proven encrypted", ❌ never "unless proven unencrypted": a probe that
//! can't answer (no `smbutil`, a hung mount, Linux, a changed output) must not open
//! the share in the clear.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::MountClass;
use cmdr_local::LocalPosixVolume;
use tokio::sync::OnceCell;

use crate::file_system::volume::{ListingProgress, Volume, VolumeError};

/// What a kernel mount's own SMB session says about encryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MountEncryption {
    /// The session runs an encryption algorithm.
    Encrypted,
    /// The session runs in the clear.
    NotEncrypted,
    /// The probe couldn't tell. Treated as not encrypted.
    Unknown,
}

/// The volume a kernel-mounted SMB share registers as, given the requirement as it
/// stands: the plain mount when encryption isn't required, else the gate.
///
/// ❗ Every site that builds a volume for an SMB kernel mount goes through here
/// (`mount_registration`, the startup registration of attached volumes, Linux's
/// mount watcher, and `smb_upgrade::return_to_os_mount`), or the requirement has a
/// hole the size of that site.
pub(crate) fn os_mount_volume(name: &str, root: &Path) -> Arc<dyn Volume> {
    let mount = LocalPosixVolume::on_mount(name, root, MountClass::SmbShare, crate::volume_host::host());
    if !cmdr_smb::SmbEncryption::is_required() {
        return Arc::new(mount);
    }
    let gate = Arc::new(UnverifiedSmbMount::new(mount));
    // Verify in the background, and hand the share to the plain mount once it proves
    // encrypted, so the gate stands only where it has to. The spawn is the app's
    // runtime, which a sync registration site (the FSEvents thread) can reach.
    let verifying = Arc::clone(&gate);
    tauri::async_runtime::spawn(async move { verifying.step_aside_if_encrypted().await });
    gate
}

/// Builds a volume for `root` with the mount class registration probed: an SMB
/// share through [`os_mount_volume`], anything else as the plain mount.
pub(crate) fn mount_volume(name: &str, root: &Path, mount: MountClass) -> Arc<dyn Volume> {
    match mount {
        MountClass::SmbShare => os_mount_volume(name, root),
        MountClass::LocalDisk | MountClass::OtherNetwork => Arc::new(LocalPosixVolume::on_mount(
            name,
            root,
            mount,
            crate::volume_host::host(),
        )),
    }
}

/// The requirement changed: every SMB kernel mount the registry serves switches
/// between the plain mount and the gate to match. A share on Cmdr's own session
/// isn't touched (its session follows the switch when it's next built).
///
/// Each swap goes through `smb_upgrade::register_replacing_predecessor`, so the
/// replaced volume is superseded (work running on it finishes) and open panes
/// re-list through the new one.
pub(crate) async fn follow_requirement() {
    let required = cmdr_smb::SmbEncryption::is_required();
    let manager = crate::file_system::volume::manager::get_volume_manager();
    for (volume_id, volume) in manager.list_volumes_with_handles() {
        let is_gate = volume.as_any().is::<UnverifiedSmbMount>();
        let is_plain_smb_mount =
            volume.as_any().is::<LocalPosixVolume>() && volume.mount_class() == Some(MountClass::SmbShare);
        if (required && is_plain_smb_mount) || (!required && is_gate) {
            log::debug!(
                target: "network::smb_encryption",
                "Re-registering the SMB mount at {} for the encryption requirement",
                LogPrivate::dir_path(volume.root())
            );
            let replacement = os_mount_volume(volume.name(), volume.root());
            crate::network::smb_upgrade::register_replacing_predecessor(&volume_id, replacement).await;
        }
    }
}

/// Whether the share `volume_id` is refused for encryption right now: the registry
/// serves it through the gate, and the mount didn't prove encrypted.
///
/// The fallback notice asks this before saying "you're still connected through
/// macOS", which would be untrue of a refused mount.
pub(crate) async fn refuses(volume_id: &str) -> bool {
    let Some(volume) = crate::file_system::volume::manager::get_volume_manager().get(volume_id) else {
        return false;
    };
    match volume.as_any().downcast_ref::<UnverifiedSmbMount>() {
        Some(gate) => !gate.admitted().await,
        None => false,
    }
}

/// A kernel-mounted SMB share under "Always require SMB encryption" whose session
/// hasn't proven encrypted. Refuses everything with `VolumeError::EncryptionRequired`,
/// except a listing or a stat once the probe says encrypted (the plain mount takes
/// over a moment later anyway).
///
/// Keeps the plain mount it stands in for: the probe's answer and a lifted
/// requirement both hand calls to it, which is what lets a pane opened in the
/// probe's few milliseconds, or after the switch went off, list normally.
pub(crate) struct UnverifiedSmbMount {
    mount: LocalPosixVolume,
    verdict: Arc<OnceCell<MountEncryption>>,
}

/// How long `smbutil statshares` may take. It reads the kernel's own state for one
/// mount (tens of milliseconds); the limit only bounds a wedged mount.
#[cfg(target_os = "macos")]
const STATSHARES_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

impl UnverifiedSmbMount {
    fn new(mount: LocalPosixVolume) -> Self {
        Self {
            mount,
            verdict: Arc::new(OnceCell::new()),
        }
    }

    /// Whether calls may go to the mount: the requirement is off now, or the mount's
    /// session is encrypted. Probes once; every caller shares the answer.
    async fn admitted(&self) -> bool {
        if !cmdr_smb::SmbEncryption::is_required() {
            return true;
        }
        let root = self.mount.root().to_path_buf();
        *self.verdict.get_or_init(|| probe(root)).await == MountEncryption::Encrypted
    }

    /// Hands the share to the plain mount once the probe says encrypted, if the
    /// registry still serves it through this gate.
    async fn step_aside_if_encrypted(self: Arc<Self>) {
        if !self.admitted().await {
            log::info!(
                target: "network::smb_encryption",
                "Not browsing the SMB mount at {} through macOS: its connection isn't encrypted, and SMB encryption is required",
                LogPrivate::dir_path(self.mount.root())
            );
            return;
        }
        let manager = crate::file_system::volume::manager::get_volume_manager();
        let Some((volume_id, current)) = manager.find_by_root(self.mount.root()) else {
            return;
        };
        let still_this_gate = current
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|gate| std::ptr::eq(gate, self.as_ref()));
        if still_this_gate {
            let plain: Arc<dyn Volume> = Arc::new(LocalPosixVolume::on_mount(
                self.mount.name(),
                self.mount.root(),
                MountClass::SmbShare,
                crate::volume_host::host(),
            ));
            crate::network::smb_upgrade::register_replacing_predecessor(&volume_id, plain).await;
        }
    }

    fn refusal(&self) -> VolumeError {
        VolumeError::EncryptionRequired(self.mount.root().to_string_lossy().into_owned())
    }
}

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl Volume for UnverifiedSmbMount {
    fn name(&self) -> &str {
        self.mount.name()
    }

    fn root(&self) -> &Path {
        self.mount.root()
    }

    /// The same share at another of its roots: a new gate, probing that mount.
    fn rerooted(&self, new_root: &Path) -> Option<Arc<dyn Volume>> {
        Some(os_mount_volume(self.mount.name(), new_root))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn mount_class(&self) -> Option<MountClass> {
        Some(MountClass::SmbShare)
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        Box::pin(async move {
            if self.admitted().await {
                self.mount.list_directory(path, on_progress).await
            } else {
                Err(self.refusal())
            }
        })
    }

    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        Box::pin(async move {
            if self.admitted().await {
                self.mount.get_metadata(path).await
            } else {
                Err(self.refusal())
            }
        })
    }

    fn create_file<'a>(&'a self, _path: &'a Path, _content: &'a [u8]) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move { Err(self.refusal()) })
    }

    fn create_directory<'a>(&'a self, _path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move { Err(self.refusal()) })
    }

    fn delete<'a>(&'a self, _path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move { Err(self.refusal()) })
    }

    fn rename<'a>(&'a self, _from: &'a Path, _to: &'a Path, _force: bool) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move { Err(self.refusal()) })
    }

    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Fut<'a, Result<Box<dyn cmdr_fs::volume::VolumeReadStream>, VolumeError>> {
        Box::pin(async move { Err(self.refusal()) })
    }
}

/// Asks the kernel whether the mount at `root` runs an encrypted session.
#[cfg(target_os = "macos")]
async fn probe(root: PathBuf) -> MountEncryption {
    // C locale, so the attribute table stays the one `parse_statshares` reads.
    let mut cmd = crate::subprocess::command_in_c_locale("smbutil");
    cmd.arg("statshares").arg("-m").arg(&root);
    match crate::subprocess::output_within("smbutil statshares", STATSHARES_LIMIT, &mut cmd).await {
        Ok(output) if output.status.success() => parse_statshares(&String::from_utf8_lossy(&output.stdout)),
        Ok(output) => {
            log::debug!(
                target: "network::smb_encryption",
                "smbutil statshares didn't describe {} (exit {:?}); treating it as not encrypted",
                LogPrivate::dir_path(&root),
                output.status.code()
            );
            MountEncryption::Unknown
        }
        Err(e) => {
            log::debug!(
                target: "network::smb_encryption",
                "smbutil statshares didn't answer for {}: {e}; treating it as not encrypted",
                LogPrivate::dir_path(&root)
            );
            MountEncryption::Unknown
        }
    }
}

/// GVFS doesn't say whether its SMB session is encrypted, so a Linux kernel mount
/// never proves it, and the requirement keeps it refused.
#[cfg(target_os = "linux")]
async fn probe(_root: PathBuf) -> MountEncryption {
    MountEncryption::Unknown
}

/// Reads `smbutil statshares -m <root>`'s attribute table: the
/// `SMB_CURR_ENCRYPT_ALGORITHM` row is `OFF` for a session in the clear and the
/// cipher's name (`AES-128-GCM`) for an encrypted one. No row, or no value, is
/// `Unknown`.
///
/// Verified on macOS 27.0.1, against Samba on Alpine (the `auth` fixture as is, and
/// a copy with `smb encrypt = required`) and a QNAP, `smbutil statshares -m` on each
/// mount, 2026-11-05. Snapshot: `smb_mount_gate_test.rs`.
pub(crate) fn parse_statshares(stdout: &str) -> MountEncryption {
    stdout
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next() == Some("SMB_CURR_ENCRYPT_ALGORITHM")).then(|| fields.next())
        })
        .flatten()
        .map_or(MountEncryption::Unknown, |value| {
            if value == "OFF" {
                MountEncryption::NotEncrypted
            } else {
                MountEncryption::Encrypted
            }
        })
}

#[cfg(test)]
#[path = "smb_mount_gate_test.rs"]
mod tests;

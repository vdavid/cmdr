//! Which refusal a "permission denied" read was: one Full Disk Access opens, or
//! one nothing Cmdr can do opens.
//!
//! Rust folds both `EPERM` and `EACCES` into `ErrorKind::PermissionDenied`, and on
//! macOS they mean different things (verified on macOS 27.0, a probe over
//! `/System/Library/AssetsV2`, `/usr/sbin/authserver`, `/Library/Caches`,
//! `/private/var/root`, and `~/Library/Mail`, run both with and without Full Disk
//! Access, 2026-11-11; `DETAILS.md` § "Ground the walk couldn't read"):
//!
//! - `EACCES` is plain POSIX permissions (a root-only folder). TCC never answers
//!   with it, so Full Disk Access changes nothing. This includes SIP folders that
//!   are also mode 700, like `/usr/sbin/authserver`: the permission bits refuse
//!   before SIP is asked.
//! - `EPERM` on a folder carrying `SF_RESTRICTED` is SIP. Nothing opens it.
//! - `EPERM` where even `lstat` of the folder answers `EPERM` is a data vault
//!   (`/Library/Caches/com.apple.aned`). Nothing opens it either.
//! - `EPERM` on anything else is TCC, the one case Full Disk Access fixes.
//!
//! Classified on the errno and the flag, ❌ never on the message text.

use std::io;
use std::path::Path;

use crate::indexing::store::UnreadableCause;

/// Whether the folder that refused us is sealed beyond any permission the user
/// can grant, as its own `lstat` answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seal {
    /// SIP (`SF_RESTRICTED`) or a data vault (`lstat` itself refused).
    #[cfg_attr(
        all(any(target_os = "linux", target_os = "windows"), not(test)),
        expect(dead_code, reason = "only macOS seals a folder; elsewhere `seal_of` answers `Open`")
    )]
    Sealed,
    /// Nothing about the folder itself says it's sealed.
    Open,
}

/// The cause a permission-denied read earns. Call it only for an error whose
/// `kind()` is `PermissionDenied`.
pub(super) fn refusal_cause(path: &Path, err: &io::Error) -> UnreadableCause {
    cause_for(err.raw_os_error(), || seal_of(path))
}

/// The rule itself, apart from the one `lstat` it may need, so a test can pin it
/// without a SIP folder to hand.
fn cause_for(errno: Option<i32>, seal: impl FnOnce() -> Seal) -> UnreadableCause {
    // Only macOS has a permission the user can grant to open a folder. Everywhere
    // else a refusal is file permissions, which Cmdr can't change either.
    if !cfg!(target_os = "macos") || errno != Some(libc::EPERM) {
        return UnreadableCause::OffLimits;
    }
    match seal() {
        Seal::Sealed => UnreadableCause::OffLimits,
        Seal::Open => UnreadableCause::Denied,
    }
}

/// `SF_RESTRICTED` from `<sys/stat.h>`: the flag SIP sets on what it protects
/// (`ls -lO` prints it as `restricted`). `libc` doesn't export it.
#[cfg(target_os = "macos")]
const SF_RESTRICTED: u32 = 0x0008_0000;

#[cfg(target_os = "macos")]
fn seal_of(path: &Path) -> Seal {
    use std::os::macos::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.st_flags() & SF_RESTRICTED != 0 => Seal::Sealed,
        Ok(_) => Seal::Open,
        Err(e) if e.raw_os_error() == Some(libc::EPERM) => Seal::Sealed,
        // Gone, or anything else: no evidence it's sealed, so keep the offer that
        // might help.
        Err(_) => Seal::Open,
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn seal_of(_path: &Path) -> Seal {
    Seal::Open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eacces_is_off_limits_whatever_the_folder_says() {
        // `/usr/sbin/authserver` is SIP AND mode 700: the bits answer EACCES first.
        for seal in [Seal::Sealed, Seal::Open] {
            assert_eq!(cause_for(Some(libc::EACCES), || seal), UnreadableCause::OffLimits);
        }
    }

    #[test]
    fn an_errno_less_refusal_is_off_limits() {
        assert_eq!(cause_for(None, || Seal::Open), UnreadableCause::OffLimits);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn eperm_on_a_sealed_folder_is_off_limits_and_on_an_open_one_is_full_disk_access() {
        assert_eq!(
            cause_for(Some(libc::EPERM), || Seal::Sealed),
            UnreadableCause::OffLimits
        );
        assert_eq!(cause_for(Some(libc::EPERM), || Seal::Open), UnreadableCause::Denied);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn eperm_off_macos_is_off_limits() {
        assert_eq!(cause_for(Some(libc::EPERM), || Seal::Open), UnreadableCause::OffLimits);
    }

    /// The real flag on a real folder: `/System/Library` is SIP-protected on every
    /// Mac with SIP on, CI runners included, and a user's home folder never is.
    #[cfg(target_os = "macos")]
    #[test]
    fn seal_reads_sf_restricted_off_the_folder() {
        assert_eq!(seal_of(Path::new("/System/Library")), Seal::Sealed);
        let home = std::env::var_os("HOME").expect("HOME");
        assert_eq!(seal_of(Path::new(&home)), Seal::Open);
    }
}

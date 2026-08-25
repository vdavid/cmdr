//! Whether the running `.app` bundle sits somewhere an update can actually be written into.
//!
//! Two macOS arrangements make the bundle unwritable, and neither is fixable from inside the app:
//! App Translocation (Cmdr opened straight from `~/Downloads`, so Gatekeeper runs it from a
//! randomized read-only mount) and a still-mounted `.dmg`. Both surface as `EROFS` rather than
//! `EPERM`, so the installer's escalate-on-`PermissionDenied` arm never fires for them, and
//! escalating wouldn't help anyway: a read-only mount refuses root too.
//!
//! Detecting this BEFORE the download matters. Without the gate, such an install downloads ~63 MB
//! and rewrites nothing, once per poll interval, for as long as it runs.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Why this install can't apply an update, when the reason is where the bundle SITS rather than
/// anything about the update itself.
///
/// Both variants ask the same thing of the user (move Cmdr into Applications), and they're kept
/// apart so the log and the `update_check` event can say which arrangement produced the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BundleWriteBlocker {
    /// macOS App Translocation: Cmdr was opened from where it was downloaded, so it's running
    /// from a randomized read-only mount under `/private/var/folders/…/AppTranslocation/`.
    Translocated,
    /// The bundle lives on a read-only volume, which in practice means a `.dmg` still mounted.
    ReadOnlyVolume,
}

impl std::fmt::Display for BundleWriteBlocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Translocated => f.write_str("running translocated (opened from its download location)"),
            Self::ReadOnlyVolume => f.write_str("running from a read-only volume"),
        }
    }
}

/// Classifies a bundle path. Translocation is reported in preference to the read-only volume it
/// implies: a translocated app is on a read-only mount by construction, and naming the outer cause
/// is what tells a reader which arrangement they're looking at.
pub fn classify(bundle: &Path) -> Option<BundleWriteBlocker> {
    if is_translocated(bundle) {
        return Some(BundleWriteBlocker::Translocated);
    }
    if is_on_read_only_volume(bundle) {
        return Some(BundleWriteBlocker::ReadOnlyVolume);
    }
    None
}

/// Asks macOS whether `path` is being served through App Translocation.
///
/// `SecTranslocateIsTranslocatedURL` is the supported answer (Security.framework, macOS 10.12+).
/// The alternative, testing the path for `/AppTranslocation/`, is a private layout that Apple
/// never promised and that we'd have no way to notice breaking.
///
/// Any failure answers `false`: a translocated app that we fail to recognize still gets caught by
/// the read-only-volume check below, so a false negative here costs only the sharper log line.
fn is_translocated(path: &Path) -> bool {
    use core_foundation::base::{CFType, CFTypeRef, TCFType};
    use core_foundation::url::{CFURL, CFURLRef};
    use std::ffi::c_void;

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecTranslocateIsTranslocatedURL(path: CFURLRef, is_translocated: *mut u8, error: *mut *const c_void) -> u8;
    }

    let Some(url) = CFURL::from_path(path, true) else {
        return false;
    };

    let mut translocated: u8 = 0;
    let mut error: *const c_void = std::ptr::null();

    // SAFETY: `url` is a live CFURL owned by this scope and outlives the call. `translocated` and
    // `error` are stack slots the callee writes through; both are initialized before the call, and
    // `translocated` is read only when the call reports success. The `error` out-param is a
    // Create-rule CFErrorRef when non-null, balanced by the `CFType` wrapper below.
    let ok = unsafe { SecTranslocateIsTranslocatedURL(url.as_concrete_TypeRef(), &mut translocated, &mut error) };

    if !error.is_null() {
        // SAFETY: non-null here means the callee handed back a +1 CFError; wrapping it under the
        // Create rule transfers that reference so the drop at end of scope releases it exactly once.
        drop(unsafe { CFType::wrap_under_create_rule(error as CFTypeRef) });
    }

    ok != 0 && translocated != 0
}

/// Whether the volume holding `path` is mounted read-only, via `statfs`'s `MNT_RDONLY`.
///
/// This is the catch-all: it covers a mounted `.dmg`, a read-only network share, and translocation
/// itself, all without knowing which one it's looking at.
fn is_on_read_only_volume(path: &Path) -> bool {
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };

    // SAFETY: `libc::statfs` is a C struct of plain integers, so an all-zero bit pattern is a
    // valid (if meaningless) value to hand the call as scratch space.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is a live NUL-terminated C string, and `stat` is a writable `statfs` the
    // call fills in; its fields are read only after `rc == 0`.
    let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) };

    // `f_flags` is `u32` while `MNT_RDONLY` is declared `i32`; the constant is a single low bit,
    // so the cast is exact.
    rc == 0 && (stat.f_flags & libc::MNT_RDONLY as u32) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo itself is on a writable APFS volume and isn't translocated, so the happy path has
    /// to answer "nothing in the way". A blocker here would gate every update on this machine.
    #[test]
    fn a_normal_writable_path_has_no_blocker() {
        let here = std::env::current_dir().expect("the test process has a working directory");
        assert_eq!(classify(&here), None);
    }

    /// `/` is read-only on macOS 11+ (the signed system volume), which gives the `statfs` arm a
    /// real read-only mount to answer about without staging a disk image.
    #[test]
    #[cfg(target_os = "macos")]
    fn the_sealed_system_volume_reads_as_read_only() {
        assert!(
            is_on_read_only_volume(Path::new("/")),
            "macOS 11+ mounts the system volume read-only; if this fails, MNT_RDONLY isn't being read right"
        );
    }

    /// A path nothing can `statfs` must not be reported as read-only: a blocker is a hard stop on
    /// updating, so the failure direction has to be "keep going".
    #[test]
    fn an_unstattable_path_is_not_a_blocker() {
        assert!(!is_on_read_only_volume(Path::new("/no/such/path/for/cmdr/tests")));
    }

    /// A path with an interior NUL can't become a C string. Same reasoning: fail open.
    #[test]
    fn a_path_with_an_interior_nul_is_not_a_blocker() {
        use std::ffi::OsStr;
        let path = Path::new(OsStr::from_bytes(b"/tmp/cm\0dr"));
        assert!(!is_on_read_only_volume(path));
    }

    /// A normal path isn't translocated, and the call must survive being asked (the out-params and
    /// the CFError release are the parts worth exercising for real).
    #[test]
    fn a_normal_path_is_not_translocated() {
        let here = std::env::current_dir().expect("the test process has a working directory");
        assert!(!is_translocated(&here));
    }

    /// A path that doesn't exist still has to answer, not trap: the CFURL is constructible, so the
    /// framework call runs and reports its own failure through the out-params.
    #[test]
    fn a_missing_path_answers_rather_than_trapping() {
        assert!(!is_translocated(Path::new("/no/such/path/for/cmdr/tests")));
    }
}

use std::ffi::OsStr;
use std::path::Path;

use super::*;

fn sidecar_on(name: &str, volume_keeps_sidecars: bool, existing_sibling: Option<&str>) -> bool {
    is_sidecar_name(OsStr::new(name), volume_keeps_sidecars, |sibling| {
        existing_sibling.is_some_and(|s| OsStr::new(s) == sibling)
    })
}

#[test]
fn a_paired_dot_underbar_name_on_a_sidecar_volume_is_a_sidecar() {
    assert!(sidecar_on("._child.txt", true, Some("child.txt")));
}

/// A folder's xattrs live in `._folder` in its parent, the same as a file's.
#[test]
fn a_folders_sidecar_counts_too() {
    assert!(sidecar_on("._folder", true, Some("folder")));
}

/// Nothing reads an orphan as anyone's xattrs, so it's the only copy of what it holds.
#[test]
fn an_orphan_is_an_ordinary_file() {
    assert!(!sidecar_on("._gone.txt", true, None));
}

/// APFS: a person's own `._foo` is just a file, sibling or not.
#[test]
fn nothing_is_a_sidecar_on_a_volume_with_native_xattrs() {
    assert!(!sidecar_on("._foo", false, Some("foo")));
    assert!(!sidecar_on("._foo", false, None));
}

/// The kernel's own pairing test needs a name longer than the prefix.
#[test]
fn a_bare_prefix_is_never_a_sidecar() {
    assert!(!sidecar_on("._", true, Some("")));
}

#[test]
fn a_name_without_the_prefix_is_never_a_sidecar() {
    assert!(!sidecar_on("child.txt", true, Some("hild.txt")));
    assert!(!sidecar_on("_.child", true, Some("child")));
    assert!(!sidecar_on(".child", true, Some("child")));
}

/// The probe asks the volume only when a `._` name turns up, and once per device.
#[test]
#[cfg_attr(miri, ignore = "stats the sibling, which Miri's isolation refuses")]
fn the_rule_probes_lazily_and_once_per_device() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Never statted for real: no sibling of `._a` lives under a missing root.
    let dir = Path::new("/nonexistent-cmdr-sidecar-probe");
    let probes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&probes);
    let rule = SidecarRule::with_probe(move |_| {
        counter.fetch_add(1, Ordering::Relaxed);
        true
    });

    assert!(!rule.is_sidecar(dir, 7, OsStr::new("plain.txt")));
    assert_eq!(probes.load(Ordering::Relaxed), 0, "a plain name never probes");

    rule.is_sidecar(dir, 7, OsStr::new("._a"));
    rule.is_sidecar(dir, 7, OsStr::new("._b"));
    assert_eq!(probes.load(Ordering::Relaxed), 1, "one probe per device");

    rule.is_sidecar(dir, 8, OsStr::new("._c"));
    assert_eq!(probes.load(Ordering::Relaxed), 2, "a new device probes again");
}

/// The boot volume is APFS, which stores xattrs natively (verified on macOS 27.0,
/// `getattrlist`, 2026-10-12).
#[cfg(target_os = "macos")]
#[test]
#[cfg_attr(miri, ignore = "calls `getattrlist`, which Miri can't cross")]
fn the_boot_volume_keeps_no_sidecars() {
    assert_eq!(native_xattrs(Path::new("/")), Some(true));
    assert!(!volume_keeps_sidecars(Path::new("/")));
}

/// `devfs` has no native xattrs: the one always-mounted volume that proves the
/// probe can say so without attaching a FAT image (verified on macOS 27.0,
/// `getattrlist`, 2026-10-12).
#[cfg(target_os = "macos")]
#[test]
#[cfg_attr(miri, ignore = "calls `getattrlist`, which Miri can't cross")]
fn devfs_reports_no_native_xattrs() {
    assert_eq!(native_xattrs(Path::new("/dev")), Some(false));
    assert!(volume_keeps_sidecars(Path::new("/dev")));
}

#[test]
#[cfg_attr(miri, ignore = "calls `getattrlist`, which Miri can't cross")]
fn an_unreadable_path_keeps_no_sidecars() {
    assert!(!volume_keeps_sidecars(Path::new("/definitely/not/a/path/cmdr")));
}

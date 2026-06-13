//! macOS permission checking and System Settings helpers.
//!
//! # Full Disk Access: detect vs. register (single source of truth)
//!
//! This module doc is the canonical place for how FDA works in Cmdr.
//! `onboarding/DETAILS.md` and `docs/architecture.md` point here; they must not
//! re-describe the mechanism (it drifts when copied). Two separate jobs, often
//! conflated:
//!
//! 1. **Detect** whether we have FDA: read 1 byte from a TCC-protected *file*
//!    (`fda_probe_files`). `Ok` = granted, `PermissionDenied` = not. Works on
//!    every macOS version; it's what `check_full_disk_access` and the quiet
//!    poller return.
//! 2. **Register** the app in System Settings → Privacy & Security → Full Disk
//!    Access (the greyed, toggleable row) so the user need not use the `+`
//!    button. This is the macOS-version-dependent part:
//!    - macOS <= 12: a denied file `read()` registers the bundle.
//!    - macOS 13+ (Ventura/Sonoma/Tahoe): a denied file `read()` is refused
//!      *without* listing a notarized app. The access that still registers is a
//!      raw `open()` on a TCC-protected *directory* (`fda_probe_dirs`), NOT
//!      `opendir`/`read_dir`.
//!
//! So on a denial `check_full_disk_access` fires *both* register paths: the
//! legacy file `mmap` / `NSData` / parent `read_dir` (old macOS) and a directory
//! `open()` on each protected dir (macOS 13+). The quiet poller
//! (`check_full_disk_access_quiet`) is detection-only, no register side effects.
//!
//! ## What's verified, and what isn't
//!
//! KNOWN (traced with `fs_usage` on macOS 26.5.1, 2026-06): Path Finder's whole
//! FDA probe is `open(~/Library/Mail)`, and it appears in the list the instant
//! it runs; a notarized app's denied file `read()` does NOT list it on 13+.
//! UNVERIFIED: that our mirrored directory `open()` actually lists *Cmdr*. It
//! only manifests on a real notarized build, so confirm on the next release
//! (`tccutil reset SystemPolicyAllFiles com.veszelovszki.cmdr`, launch, check
//! the list). Until then the onboarding `+` step-tip stays as the backstop.

use std::ffi::CString;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Files we read 1 byte from to detect FDA (`Ok` = granted, `PermissionDenied`
/// = not). Walk-until-exists, because `NotFound` doesn't reach TCC. On macOS <=
/// 12 a denied read here also registers the bundle. See the module doc for the
/// detect-vs-register split.
fn fda_probe_files() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    vec![
        home.join("Library/Safari/History.db"), // TCC-protected, present after Safari use
        home.join("Library/Safari/Bookmarks.plist"), // TCC-protected, present after first Safari launch
        home.join("Library/Mail/V10/MailData/Envelope Index"), // Mail user
        home.join("Library/Messages/chat.db"),  // Messages user
        home.join("Library/Application Support/com.apple.TCC/TCC.db"), // always exists, TCC-protected
        home.join("Library/Application Support/AddressBook/AddressBook-v22.abcddb"), // Contacts user
    ]
}

/// Protected directories we raw-`open()` (read-only, no read) to register the
/// bundle in the FDA list on macOS 13+. Raw `open()`, NOT `opendir`/`read_dir`.
/// The TCC dir always exists; the rest cover Mail/Safari/Messages users. See the
/// module doc for why a directory open (not a file read) is what registers on
/// 13+.
fn fda_probe_dirs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    vec![
        home.join("Library/Mail"),                              // present for Mail users
        home.join("Library/Safari"),                            // present after Safari use
        home.join("Library/Messages"),                          // present for Messages users
        home.join("Library/Application Support/com.apple.TCC"), // always exists, TCC-protected
    ]
}

/// Tries to open `path` and read at least one byte from it. The read is what
/// trips TCC; `open()` alone has been observed not to register the bundle.
fn try_read_byte(path: &Path) -> std::io::Result<()> {
    let mut f = File::open(path)?;
    let mut buf = [0u8; 1];
    // We don't care if we got 0 bytes (empty file) or 1, both mean the read
    // syscall reached the kernel and was allowed. The variant we care about
    // is the `Err` case, which is what TCC denial returns.
    let _ = f.read(&mut buf)?;
    Ok(())
}

/// Tries to `open()` `path` read-only without reading from it. This is the
/// access that lists the bundle in Full Disk Access on macOS 13+ when `path` is
/// a protected directory (mirrors Path Finder's `open(~/Library/Mail)`). Works
/// on dirs and files; we deliberately don't `read()` (a directory read returns
/// `EISDIR`). The `Err(PermissionDenied)` case is the FDA denial that registers.
fn try_open_path(path: &Path) -> std::io::Result<()> {
    File::open(path).map(|_| ())
}

/// Tries to mmap the first byte of `path`. Different syscall path than
/// `read()` (mmap goes through the VM subsystem); on some macOS versions
/// this is observed to trigger tccd registration where plain `read()`
/// doesn't.
fn try_mmap_byte(path: &Path) -> std::io::Result<()> {
    let cpath =
        CString::new(path.as_os_str().as_bytes()).map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, e))?;
    // Safety: passing valid pointers, validating return values, calling
    // the matching `munmap`/`close` on every path out.
    unsafe {
        let fd = libc::open(cpath.as_ptr(), libc::O_RDONLY);
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let len: libc::size_t = 1;
        let ptr = libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, fd, 0);
        if ptr == libc::MAP_FAILED {
            let err = std::io::Error::last_os_error();
            libc::close(fd);
            return Err(err);
        }
        // Force the fault by reading the byte through the mapping.
        let _ = std::ptr::read_volatile(ptr as *const u8);
        libc::munmap(ptr, len);
        libc::close(fd);
    }
    Ok(())
}

/// Tries to read `path` via `NSData dataWithContentsOfFile:`. Goes through
/// Foundation's higher-level file API rather than raw POSIX, in case Tahoe
/// only triggers tccd registration for Foundation-routed reads.
fn try_nsdata_read(path: &Path) -> std::io::Result<()> {
    use objc2_foundation::{NSData, NSString};
    let path_str = NSString::from_str(&path.to_string_lossy());
    // `dataWithContentsOfFile:` returns nil on failure (no error detail).
    // We don't care about the data; we only want the syscall to land at
    // tccd. Any nil result is treated as "denied" for our purposes.
    let data = NSData::dataWithContentsOfFile(&path_str);
    if data.is_some() {
        Ok(())
    } else {
        Err(std::io::Error::from(ErrorKind::PermissionDenied))
    }
}

/// Tries to list the parent directory of `path` via `read_dir`. The
/// pre-Tahoe Cmdr probe used `read_dir(~/Library/Mail)` directly, which
/// some users reported as the trigger that put Cmdr in the FDA list. Kept
/// as one of the multi-trigger fallbacks.
fn try_read_dir_parent(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or(std::io::Error::from(ErrorKind::InvalidInput))?;
    std::fs::read_dir(parent).map(|_| ())
}

/// Reads the `CMDR_MOCK_FDA` test override, if set and recognized.
///
/// Mirrors `CMDR_MOCK_LICENSE`: `granted` forces `Some(true)`, `denied` /
/// `notgranted` force `Some(false)`. Any other value (or an unset var)
/// returns `None`, so the caller falls through to the real probe. The
/// wizard distinguishes "denied" (user clicked Deny last step) vs
/// "notgranted" (user clicked Allow but TCC still says no) via the
/// persisted `fullDiskAccessChoice` setting; this mock only controls the
/// OS-level signal so all four step-2 banner branches can be tested without
/// ever opening real System Settings.
///
/// `quiet` suppresses the per-call debug log so the 500 ms onboarding
/// poller (`check_full_disk_access_quiet`) doesn't spam the log.
fn mock_fda_override(quiet: bool) -> Option<bool> {
    let mock = std::env::var("CMDR_MOCK_FDA").ok()?;
    match mock.as_str() {
        "granted" => {
            if !quiet {
                log::debug!(target: "fda_probe", "CMDR_MOCK_FDA=granted → returning true (test override)");
            }
            Some(true)
        }
        "denied" | "notgranted" => {
            if !quiet {
                log::debug!(target: "fda_probe", "CMDR_MOCK_FDA={} → returning false (test override)", mock);
            }
            Some(false)
        }
        other => {
            if !quiet {
                log::warn!(target: "fda_probe", "CMDR_MOCK_FDA={:?} not recognized; falling through to real probe", other);
            }
            None
        }
    }
}

/// Side-effect-free FDA probe: reads one byte from each candidate protected
/// file until one returns a definitive `Ok` (granted) or `PermissionDenied`
/// (not granted). Unlike `check_full_disk_access`, it does NOT fire the
/// `mmap` / `NSData` / `read_dir` registration triggers on a denial, so it's
/// safe to call repeatedly (the onboarding 500 ms grant-detection poller
/// uses it). It's a pure read with no logging in the steady state, keeping
/// CPU, syscalls, and the log clean.
fn probe_fda_quiet() -> bool {
    for path in fda_probe_files() {
        match try_read_byte(&path) {
            Ok(()) => return true,
            Err(e) if e.kind() == ErrorKind::PermissionDenied => return false,
            Err(_) => continue, // NotFound etc.: not a definitive signal, try the next path.
        }
    }
    // No probed file existed. Treat as "no FDA": better to keep polling than
    // to falsely report a grant.
    false
}

/// Polls FDA status without TCC-registration side effects.
///
/// Same return contract as `check_full_disk_access` (`true` = granted) and
/// honors the same `CMDR_MOCK_FDA` override, but skips the multi-trigger
/// `mmap` / `NSData` / `read_dir` storm and the per-call logging. Built for
/// the onboarding FDA step, which calls this every 500 ms while visible and
/// not-yet-granted to flip to a success state the moment the user toggles
/// Cmdr on in System Settings. Keep `check_full_disk_access` for the
/// one-shot registration moments (it's the one that gets Cmdr into the FDA
/// list); this one is purely for detection.
#[tauri::command]
#[specta::specta]
pub fn check_full_disk_access_quiet() -> bool {
    if let Some(mocked) = mock_fda_override(true) {
        return mocked;
    }
    probe_fda_quiet()
}

/// Detects FDA by probing TCC-protected files, and on a denial fires every
/// known list-registration trigger so Cmdr shows up in System Settings. Returns
/// `true` if granted. See the module doc for the detect-vs-register mechanism
/// and the macOS-version split. For repeated, side-effect-free polling, use
/// `check_full_disk_access_quiet`.
#[tauri::command]
#[specta::specta]
pub fn check_full_disk_access() -> bool {
    if let Some(mocked) = mock_fda_override(false) {
        return mocked;
    }
    for path in fda_probe_files() {
        match try_read_byte(&path) {
            Ok(()) => {
                log::debug!(target: "fda_probe", "FDA probe: read OK on {:?} → FDA granted", path);
                return true;
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                log::debug!(target: "fda_probe", "FDA probe: PermissionDenied on {:?} via read() → FDA NOT granted; firing extra triggers", path);
                // Best-effort extra triggers. We don't care about results,
                // only that tccd hears about us through different syscall
                // paths. Each one is independently logged so we can see in
                // the TCC log which one (if any) finally registers the bundle.
                match try_mmap_byte(&path) {
                    Ok(()) => {
                        log::debug!(target: "fda_probe", "FDA probe extra: mmap OK on {:?} (FDA actually granted? unexpected)", path)
                    }
                    Err(e) => {
                        log::debug!(target: "fda_probe", "FDA probe extra: mmap on {:?} → {} ({:?})", path, e, e.kind())
                    }
                }
                match try_nsdata_read(&path) {
                    Ok(()) => {
                        log::debug!(target: "fda_probe", "FDA probe extra: NSData OK on {:?} (FDA actually granted? unexpected)", path)
                    }
                    Err(e) => {
                        log::debug!(target: "fda_probe", "FDA probe extra: NSData on {:?} → {} ({:?})", path, e, e.kind())
                    }
                }
                match try_read_dir_parent(&path) {
                    Ok(()) => log::debug!(target: "fda_probe", "FDA probe extra: read_dir(parent of {:?}) OK", path),
                    Err(e) => {
                        log::debug!(target: "fda_probe", "FDA probe extra: read_dir(parent of {:?}) → {} ({:?})", path, e, e.kind())
                    }
                }
                // macOS 13+/Tahoe: a raw open() on a protected DIRECTORY is the
                // access that actually lists the bundle in Full Disk Access (the
                // file read above no longer registers notarized apps there). Fire
                // it on every existing protected dir. See `fda_probe_dirs`.
                for dir in fda_probe_dirs() {
                    match try_open_path(&dir) {
                        Ok(()) => {
                            log::debug!(target: "fda_probe", "FDA probe extra: open dir {:?} OK (FDA actually granted? unexpected)", dir)
                        }
                        Err(e) => {
                            log::debug!(target: "fda_probe", "FDA probe extra: open dir {:?} → {} ({:?})", dir, e, e.kind())
                        }
                    }
                }
                return false;
            }
            Err(e) => {
                log::debug!(target: "fda_probe", "FDA probe: skipping {:?}: {} ({:?})", path, e, e.kind());
                continue;
            }
        }
    }
    log::warn!(target: "fda_probe", "FDA probe: no candidate path produced a definitive signal; treating as no FDA");
    // No probed file existed. Treat as "no FDA": better to show the prompt
    // than skip it.
    false
}

/// Returns the macOS major version (e.g. `13` for Ventura, `14` for Sonoma).
///
/// Used by the onboarding modal to tailor copy + the deep-link host:
/// Ventura+ has the new System Settings app with the
/// `PrivacySecurity.extension` URL host and an alphabetical FDA list; older
/// macOS uses the legacy System Preferences `preference.security` host with
/// new entries appended at the end.
#[tauri::command]
#[specta::specta]
pub fn get_macos_major_version() -> u32 {
    let Ok(output) = std::process::Command::new("sw_vers").arg("-productVersion").output() else {
        return 13; // assume modern if sw_vers is unavailable
    };
    let Ok(version) = String::from_utf8(output.stdout) else {
        return 13;
    };
    version
        .trim()
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(13)
}

/// Opens System Settings directly on the Full Disk Access pane.
///
/// Picks the deep-link host based on macOS version: Ventura+ uses the new
/// `PrivacySecurity.extension`, older macOS uses the legacy
/// `preference.security` host. Both anchor on `Privacy_AllFiles`.
#[tauri::command]
#[specta::specta]
pub fn open_privacy_settings() -> Result<(), String> {
    let url = if get_macos_major_version() >= 13 {
        "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles"
    } else {
        "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
    };
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok(())
}

/// Opens System Settings > Appearance.
#[tauri::command]
#[specta::specta]
pub fn open_appearance_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Appearance-Settings.extension")
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok(())
}

/// Opens an `x-apple.systempreferences:` deep link.
///
/// The frontend uses this for friendly-error markdown links that point at specific
/// System Settings panes. We don't go through the Tauri opener plugin because its
/// default URL allowlist only covers `http`/`https`/`mailto`/`tel` and would reject
/// the `x-apple.systempreferences:` scheme silently. Restricting the input to that
/// scheme keeps the surface tight (no arbitrary URL execution from the webview).
#[tauri::command]
#[specta::specta]
pub fn open_system_settings_url(url: String) -> Result<(), String> {
    if !url.starts_with("x-apple.systempreferences:") {
        return Err(format!(
            "Refusing to open URL with scheme other than `x-apple.systempreferences:`: {url}"
        ));
    }
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_full_disk_access_returns_bool() {
        // Just verify it doesn't panic - the return value is a bool by type system
        let _result: bool = check_full_disk_access();
    }

    #[test]
    fn quiet_probe_returns_bool_without_panicking() {
        // The 500 ms onboarding poller calls this repeatedly; it must never panic.
        // The return value is a bool by the type system; we only check the call path.
        let _result: bool = check_full_disk_access_quiet();
    }

    // `mock_fda_override` reads a process-global env var, so these run serially under one
    // test to avoid racing the var across parallel test threads. They lock in the contract
    // the onboarding poller relies on: `granted` → Some(true), `denied`/`notgranted` →
    // Some(false), anything else → None (fall through to the real probe).
    #[test]
    fn mock_override_parses_known_values_and_ignores_unknown() {
        // Save + restore so we don't leak state into other tests in this process.
        let saved = std::env::var("CMDR_MOCK_FDA").ok();

        // Safety: tests in this module touch the same var serially; no other thread reads
        // it concurrently within this single test.
        unsafe {
            std::env::set_var("CMDR_MOCK_FDA", "granted");
            assert_eq!(mock_fda_override(true), Some(true));
            assert!(check_full_disk_access_quiet());

            std::env::set_var("CMDR_MOCK_FDA", "denied");
            assert_eq!(mock_fda_override(true), Some(false));
            assert!(!check_full_disk_access_quiet());

            std::env::set_var("CMDR_MOCK_FDA", "notgranted");
            assert_eq!(mock_fda_override(true), Some(false));

            std::env::set_var("CMDR_MOCK_FDA", "nonsense");
            assert_eq!(mock_fda_override(true), None);

            std::env::remove_var("CMDR_MOCK_FDA");
            assert_eq!(mock_fda_override(true), None);

            match saved {
                Some(v) => std::env::set_var("CMDR_MOCK_FDA", v),
                None => std::env::remove_var("CMDR_MOCK_FDA"),
            }
        }
    }
}

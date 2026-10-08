//! Get Info: opens Finder's Get Info window for a file.
//!
//! macOS has no public API for that window, so Cmdr asks Finder over AppleScript
//! through a spawned `osascript`. macOS counts that as Cmdr controlling Finder
//! (Privacy & Security > Automation, `kTCCServiceAppleEvents`) and attributes it to
//! Cmdr as the responsible process: the first Get Info shows the "Cmdr wants access
//! to control Finder" prompt, carrying `NSAppleEventsUsageDescription` from
//! `Info.plist` as its reason.
//!
//! Why `osascript` and never an in-process Apple Event: under the hardened runtime,
//! tccd refuses an in-process sender without the
//! `com.apple.security.automation.apple-events` entitlement and never prompts,
//! while `osascript` carries Apple's own prompting entitlement, so the prompt shows
//! and Cmdr's bundle stays entitlement-free (`Entitlements.plist`). (verified on
//! macOS 27.0 with a hardened, entitlement-free probe app spawning `osascript`,
//! tccd log "promptPolicy = 2" plus "Prompting for access to indirect object
//! Finder", 2026-10-08)
//!
//! Once the user has said no, every later `osascript` ask dies with -1743 and no
//! window, which nothing would ever report. So `open_get_info` (macOS-only, hence no
//! doc link: Linux rustdoc can't resolve it) first reads the
//! stored answer without prompting, and returns [`GetInfoError::AutomationDenied`]
//! for the frontend to explain.

/// Why `open_get_info` couldn't ask Finder for the window.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GetInfoError {
    /// The user turned off Cmdr's control of Finder (System Settings > Privacy &
    /// Security > Automation), so macOS would drop the ask without a word.
    AutomationDenied,
    /// `osascript` couldn't be spawned. Carries the OS errno where there is one, so
    /// nothing has to read the message.
    LaunchRefused { errno: Option<i32> },
    /// The ask didn't finish inside the command's deadline.
    TimedOut,
}

impl std::fmt::Display for GetInfoError {
    /// ❗ For logs only; the frontend words the variant.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutomationDenied => f.write_str("automation of Finder is turned off"),
            Self::LaunchRefused { errno } => write!(f, "osascript launch refused (errno {errno:?})"),
            Self::TimedOut => f.write_str("timed out"),
        }
    }
}

impl std::error::Error for GetInfoError {}

/// Asks Finder to open its Get Info window for `path`, unless the user has turned
/// that off. Returns once `osascript` is spawned; the window (or the first-time
/// prompt) appears on its own.
#[cfg(target_os = "macos")]
pub fn open_get_info(path: &std::path::Path) -> Result<(), GetInfoError> {
    let automation = finder_automation_from_status(ae_permission::finder_status());
    if automation == FinderAutomation::Denied {
        log::info!(target: "get_info", "Finder automation is turned off for Cmdr, so not asking Finder");
        return Err(GetInfoError::AutomationDenied);
    }

    // The path goes in as an argument via `on run argv`, never spliced into the
    // script text, so a quote in a filename can't inject AppleScript.
    let script = r#"on run argv
        tell application "Finder"
            activate
            open information window of (POSIX file (item 1 of argv) as alias)
        end tell
    end run"#;

    std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| GetInfoError::LaunchRefused {
            errno: e.raw_os_error(),
        })
}

/// What macOS has stored about Cmdr controlling Finder.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinderAutomation {
    Allowed,
    /// No answer yet: the next ask shows the prompt.
    NotAskedYet,
    Denied,
    /// The question couldn't be answered (Finder not running, say). `osascript`
    /// launches Finder and asks again, so this proceeds.
    Unknown,
}

/// Reads `AEDeterminePermissionToAutomateTarget`'s status. The codes are
/// `AppleEvents.h`'s: `noErr`, `errAEEventNotPermitted`, and
/// `errAEEventWouldRequireUserConsent`.
#[cfg(target_os = "macos")]
fn finder_automation_from_status(status: i32) -> FinderAutomation {
    match status {
        0 => FinderAutomation::Allowed,
        -1743 => FinderAutomation::Denied,
        -1744 => FinderAutomation::NotAskedYet,
        _ => FinderAutomation::Unknown,
    }
}

/// `AEDeterminePermissionToAutomateTarget`, asked without prompting. CoreServices'
/// AE API has no Rust binding we carry, so the few symbols are declared here.
#[cfg(target_os = "macos")]
mod ae_permission {
    use std::ffi::c_void;

    /// `typeApplicationBundleID` (`'bund'`).
    const TYPE_APPLICATION_BUNDLE_ID: u32 = u32::from_be_bytes(*b"bund");
    /// `typeWildCard` (`'****'`): any event class and id.
    const TYPE_WILD_CARD: u32 = u32::from_be_bytes(*b"****");
    const FINDER_BUNDLE_ID: &[u8] = b"com.apple.finder";

    /// `AEDesc`. ❗ `AEDataModel.h` wraps it in `#pragma pack(push, 2)`, so the
    /// pointer sits at offset 4 (12 bytes total), not the natural 8.
    #[repr(C, packed(2))]
    pub(super) struct AEDesc {
        descriptor_type: u32,
        data_handle: *mut c_void,
    }

    // SAFETY: the C signatures from `AE.framework`'s `AEDataModel.h` and
    // `AppleEvents.h` (macOS 10.14+, below Cmdr's 10.15 floor). `AECreateDesc` copies
    // `data_size` bytes from `data_ptr` into a new descriptor the caller frees with
    // `AEDisposeDesc`; `AEDeterminePermissionToAutomateTarget` only reads `target`.
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn AECreateDesc(type_code: u32, data_ptr: *const c_void, data_size: isize, result: *mut AEDesc) -> i16;
        fn AEDisposeDesc(desc: *mut AEDesc) -> i16;
        fn AEDeterminePermissionToAutomateTarget(
            target: *const AEDesc,
            event_class: u32,
            event_id: u32,
            ask_user_if_needed: u8,
        ) -> i32;
    }

    /// The raw status for "may Cmdr send Finder any event", never prompting.
    pub(super) fn finder_status() -> i32 {
        let mut target = AEDesc {
            descriptor_type: 0,
            data_handle: std::ptr::null_mut(),
        };
        // SAFETY: `FINDER_BUNDLE_ID` is a live static slice whose length is passed
        // alongside it, and `target` is a valid, writable `AEDesc` on this stack.
        let created = unsafe {
            AECreateDesc(
                TYPE_APPLICATION_BUNDLE_ID,
                FINDER_BUNDLE_ID.as_ptr().cast(),
                isize::try_from(FINDER_BUNDLE_ID.len()).expect("a 16-byte bundle id fits an isize"),
                &mut target,
            )
        };
        if created != 0 {
            return i32::from(created);
        }
        // SAFETY: `target` was filled by the successful `AECreateDesc` above and is
        // only read; `0` is `false` for the C `Boolean`, so this never prompts.
        let status = unsafe { AEDeterminePermissionToAutomateTarget(&target, TYPE_WILD_CARD, TYPE_WILD_CARD, 0) };
        // SAFETY: `target` holds the descriptor `AECreateDesc` allocated, disposed
        // exactly once here.
        unsafe { AEDisposeDesc(&mut target) };
        status
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn reads_the_permission_answer_by_status_code() {
        assert_eq!(finder_automation_from_status(0), FinderAutomation::Allowed);
        assert_eq!(finder_automation_from_status(-1743), FinderAutomation::Denied);
        assert_eq!(finder_automation_from_status(-1744), FinderAutomation::NotAskedYet);
        // `procNotFound`: Finder isn't running, so the database wasn't consulted.
        assert_eq!(finder_automation_from_status(-600), FinderAutomation::Unknown);
    }

    #[test]
    fn the_descriptor_matches_the_two_byte_packed_c_layout() {
        assert_eq!(size_of::<ae_permission::AEDesc>(), 12);
    }
}

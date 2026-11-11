//! What a start asks the FSEvents journal before it streams: where the journal is
//! now, and which journal it is. Both are round trips to `fseventsd`.

#[cfg(target_os = "macos")]
use std::path::Path;

#[cfg(all(test, target_os = "macos"))]
use super::fake_journal;

/// Get the current FSEvents event ID, as seen by a volume rooted at `root`.
///
/// Useful for determining `sinceWhen` at the start of a scan.
/// Returns `0` on non-macOS platforms (no event ID concept).
///
/// The ID is system-wide, so the real journal ignores `root`; it's there so a test
/// can fake one volume's journal (`fake_journal`, test builds only) without touching
/// anybody else's.
///
/// ⚠️ It's a round trip to `fseventsd`, ❌ not a local read: 0.2 ms idle, up to 3.7 s
/// with the daemon pegged at 100% CPU by other processes' disk churn (verified on
/// macOS 27, `sample` + timing logs, 2026-10-07). Treat it as blocking I/O.
#[cfg(target_os = "macos")]
pub fn current_event_id(root: &Path) -> u64 {
    #[cfg(test)]
    if fake_journal::covers(root) {
        return fake_journal::next_event_id();
    }
    #[cfg(not(test))]
    let _ = root;
    // SAFETY: FSEventsGetCurrentEventId takes no arguments and has no preconditions.
    unsafe { cmdr_fsevent_stream::ffi::FSEventsGetCurrentEventId() }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn current_event_id(_root: &std::path::Path) -> u64 {
    0
}

/// The identity of the FSEvents journal on the device holding `root`, as 32 hex
/// digits, or `None` when it can't be read.
///
/// A wiped and recreated journal gets a new one, which is what makes a stored
/// replay point from the old journal meaningless. ⚠️ `None` means UNKNOWN, ❌ never
/// "changed": fseventsd answers null for a moment right after a mount (verified on
/// macOS 27.0.1, disk images and an FSEvents probe, 2026-10-29). Like
/// [`current_event_id`], a round trip to `fseventsd`: blocking I/O. A fake-journal
/// root (test builds) reads `None`.
#[cfg(target_os = "macos")]
pub fn journal_uuid(root: &Path) -> Option<String> {
    use core_foundation::base::TCFType;
    use core_foundation::uuid::{CFUUID, CFUUIDGetUUIDBytes};

    #[cfg(test)]
    if fake_journal::covers(root) {
        return None;
    }
    let dev = i32::try_from(cmdr_fs::file_identity::device_of(&std::fs::metadata(root).ok()?)?.raw()).ok()?;
    // SAFETY: takes a plain device number and returns an owned ref or null.
    let uuid_ref = unsafe { cmdr_fsevent_stream::ffi::FSEventsCopyUUIDForDevice(dev) };
    if uuid_ref.is_null() {
        return None;
    }
    // SAFETY: non-null and owned by us (create rule), so the wrapper releases it.
    let uuid = unsafe { CFUUID::wrap_under_create_rule(uuid_ref) };
    // SAFETY: a valid CFUUID for the call's duration.
    let b = unsafe { CFUUIDGetUUIDBytes(uuid.as_concrete_TypeRef()) };
    let bytes = [
        b.byte0, b.byte1, b.byte2, b.byte3, b.byte4, b.byte5, b.byte6, b.byte7, b.byte8, b.byte9, b.byte10, b.byte11,
        b.byte12, b.byte13, b.byte14, b.byte15,
    ];
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn journal_uuid(_root: &std::path::Path) -> Option<String> {
    None
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn current_event_id_returns_nonzero() {
        let id = current_event_id(Path::new("/"));
        assert!(id > 0, "system FSEvents event ID should be nonzero");
    }

    #[test]
    fn the_boot_disk_journal_has_a_stable_uuid() {
        let uuid = journal_uuid(Path::new("/")).expect("the boot disk always has a journal");
        assert_eq!(uuid.len(), 32);
        assert!(uuid.bytes().all(|byte| byte.is_ascii_hexdigit()), "{uuid}");
        assert_eq!(journal_uuid(Path::new("/")), Some(uuid));
    }
}

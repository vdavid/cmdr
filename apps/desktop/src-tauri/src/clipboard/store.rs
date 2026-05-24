//! Shared in-process clipboard store used by the mock backend and by the
//! prod backend's `CMDR_CLIPBOARD_BACKEND=mock` runtime override.
//!
//! Holds the most recent "copy" payload (a set of file URLs plus the
//! newline-joined plain-text representation that the prod path also writes
//! to NSPasteboard). Single entry, last-write-wins, mirroring the macOS
//! pasteboard semantics: a fresh write clears any previous contents.

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Debug, Default)]
pub struct ClipboardEntry {
    pub paths: Vec<PathBuf>,
    pub text: String,
}

static STORE: LazyLock<Mutex<Option<ClipboardEntry>>> = LazyLock::new(|| Mutex::new(None));

/// Replaces the store with a fresh entry derived from the given paths.
/// The text field is the newline-joined stringified paths, matching what
/// `pasteboard::write_file_urls_to_clipboard` writes to NSPasteboardTypeString.
pub fn write_paths(paths: &[PathBuf]) {
    let text = paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\n");
    let entry = ClipboardEntry {
        paths: paths.to_vec(),
        text,
    };
    let mut guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(entry);
}

/// Returns the stored file URLs, or an empty Vec when the store is empty.
pub fn read_paths() -> Vec<PathBuf> {
    let guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|e| e.paths.clone()).unwrap_or_default()
}

/// Returns the stored text, or None when the store is empty.
pub fn read_text() -> Option<String> {
    let guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|e| e.text.clone())
}

/// Clears the store. Exposed primarily for tests and the mock admin surface.
#[allow(
    dead_code,
    reason = "Unused in the prod-feature build; consumed by the E2E admin surface and unit tests."
)]
pub fn clear() {
    let mut guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

/// Returns a snapshot of the current entry. Used by E2E to verify clipboard
/// state without round-tripping through the read functions.
#[allow(
    dead_code,
    reason = "Unused in the prod-feature build; consumed by the E2E admin surface and unit tests."
)]
pub fn snapshot() -> Option<ClipboardEntry> {
    let guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    // Serialize tests since they share global state.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn write_and_read_round_trip() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let paths = vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")];
        write_paths(&paths);
        assert_eq!(read_paths(), paths);
        assert_eq!(read_text().as_deref(), Some("/tmp/one.txt\n/tmp/two.txt"));
    }

    #[test]
    fn second_write_replaces_first() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        write_paths(&[PathBuf::from("/tmp/a")]);
        write_paths(&[PathBuf::from("/tmp/b"), PathBuf::from("/tmp/c")]);
        assert_eq!(read_paths(), vec![PathBuf::from("/tmp/b"), PathBuf::from("/tmp/c")]);
    }

    #[test]
    fn read_empty_returns_empty() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert!(read_paths().is_empty());
        assert!(read_text().is_none());
        assert!(snapshot().is_none());
    }

    #[test]
    fn clear_empties_the_store() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        write_paths(&[PathBuf::from("/tmp/x")]);
        clear();
        assert!(read_paths().is_empty());
        assert!(snapshot().is_none());
    }
}

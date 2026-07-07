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

/// Typed pasteboard flavors for the "paste clipboard content as a file" flow
/// (`public.png` / `public.tiff` / `public.jpeg` / `com.adobe.pdf` /
/// `public.utf8-plain-text`). Injected by tests and the E2E mock, read by the
/// payload picker (`super::payload::pick_clipboard_payload`). Kept in a separate
/// static from the file-URL `ClipboardEntry` so the copy/cut/paste-files flow
/// and this content-paste flow never clobber each other.
#[derive(Clone, Debug, Default)]
pub struct ClipboardData {
    pub png: Option<Vec<u8>>,
    pub tiff: Option<Vec<u8>>,
    pub jpeg: Option<Vec<u8>>,
    pub pdf: Option<Vec<u8>>,
    pub text: Option<String>,
}

static DATA_STORE: LazyLock<Mutex<ClipboardData>> = LazyLock::new(|| Mutex::new(ClipboardData::default()));

/// Replaces the injected clipboard flavors. The unit-test injection entry point
/// that lets a paste-as-file test set several flavors at once (a real clipboard
/// carries multiple), so precedence tests are honest. `#[cfg(test)]` is
/// compile-time proof that no prod / E2E build includes it — no prod caller
/// exists (the E2E mock and env-mock only READ, via `read_clipboard_data`).
#[cfg(test)]
pub fn write_clipboard_data(data: ClipboardData) {
    *DATA_STORE.lock().unwrap_or_else(|e| e.into_inner()) = data;
}

/// Returns the injected clipboard flavors (default/empty when none set). Read by
/// the E2E mock backend and the prod backend's `CMDR_CLIPBOARD_BACKEND=mock` override.
pub fn read_clipboard_data() -> ClipboardData {
    DATA_STORE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Clears the injected clipboard flavors. Unit-test-only reset (see
/// `write_clipboard_data`).
#[cfg(test)]
pub fn clear_clipboard_data() {
    *DATA_STORE.lock().unwrap_or_else(|e| e.into_inner()) = ClipboardData::default();
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

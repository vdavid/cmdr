//! Clipboard file operations for copy/cut/paste workflows.
//!
//! macOS exposes three free functions backed by NSPasteboard (`pasteboard`)
//! in prod and by an in-process store (`mock`) under the `playwright-e2e`
//! Cargo feature. Both impls share the same `store` module so a runtime
//! `CMDR_CLIPBOARD_BACKEND=mock` env opt-out in the prod path delegates to
//! the same data the mock module reads. See `CLAUDE.md` for the rationale.

#[cfg(target_os = "macos")]
mod store;

/// Payload picking, markdown sniffing, and content mapping. Compiled for both
/// the prod and E2E backends (both feed it an already-read `ClipboardData`).
#[cfg(target_os = "macos")]
mod payload;

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
mod pasteboard;

#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
mod mock;

mod state;

pub use state::clear_cut_state;
#[cfg(target_os = "macos")]
pub use state::{get_cut_state, set_cut_state};

/// The kind of clipboard content pasted as a file. Drives the paste toast's
/// noun (text / image / PDF); the file name's extension carries the finer detail
/// (`.md` / `.txt` / `.png` / `.jpg` / `.pdf`). Ungated so the Linux command
/// stub can name it in its signature.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "constructed only by the macOS paste path; Linux keeps the type for the stub command's wire signature"
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PastedKind {
    Text,
    Image,
    Pdf,
}

/// Result of pasting clipboard content as a file: the created file's name and
/// its content kind. At the command boundary, `Option<PastedClipboardFile>`'s
/// `None` is the typed "nothing pasteable" no-op (not an error).
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PastedClipboardFile {
    pub name: String,
    pub kind: PastedKind,
}

/// What `paste_clipboard_as_file` answers within its reply deadline. A refusal
/// inside the deadline is the command's `Err(MutationError)`. Same contract as
/// `MutationReply` (`write_operations/mutation_reply.rs`), with the created
/// file riding along.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "Linux keeps the reply type for its unsupported clipboard command's wire signature"
    )
)]
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PasteClipboardReply {
    /// It ended in time: the file it created, or `None` for nothing pasteable.
    Done {
        /// The created file.
        file: Option<PastedClipboardFile>,
    },
    /// The write is still running; a [`ClipboardPasteSettled`] with this id follows.
    StillRunning {
        /// Names this one paste on the settle event.
        pending_id: String,
    },
}

/// `clipboard-paste-settled`: how a paste that answered `StillRunning` ended.
/// Broadcast; the waiting caller picks its own by `pending_id`.
#[derive(Clone, Debug, serde::Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardPasteSettled {
    /// The id the `StillRunning` reply carried.
    pub pending_id: String,
    /// How it ended.
    pub outcome: ClipboardPasteOutcome,
}

/// How a paste that outlived its deadline ended.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the clipboard settle event is shared across platforms but emitted only on macOS"
    )
)]
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ClipboardPasteOutcome {
    /// It landed as `file`.
    Landed {
        /// The created file.
        file: Option<PastedClipboardFile>,
    },
    /// It didn't, for this reason: the same refusal an in-time reply carries.
    Refused {
        /// Why.
        error: crate::file_system::write_operations::MutationError,
    },
}

#[cfg(target_os = "macos")]
pub use payload::{ClipboardPayload, payload_to_content, pick_clipboard_payload};

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
pub use pasteboard::{
    read_file_urls_from_clipboard, read_pasteboard_data, read_text_from_clipboard, write_file_urls_to_clipboard,
};

#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
pub use mock::{
    read_file_urls_from_clipboard, read_pasteboard_data, read_text_from_clipboard, write_file_urls_to_clipboard,
};

/// E2E-only admin surface: returns a snapshot of the in-process clipboard
/// store. Used by Playwright specs to assert clipboard contents without
/// reading back through IPC (and without touching the real pasteboard).
#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
#[allow(
    dead_code,
    reason = "Exported for future Playwright specs that assert clipboard contents directly."
)]
pub fn snapshot_mock_clipboard() -> Option<store::ClipboardEntry> {
    store::snapshot()
}

/// E2E-only admin surface: clears the in-process clipboard store.
#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
#[allow(
    dead_code,
    reason = "Exported for future Playwright specs that reset clipboard state between tests."
)]
pub fn clear_mock_clipboard() {
    store::clear();
}

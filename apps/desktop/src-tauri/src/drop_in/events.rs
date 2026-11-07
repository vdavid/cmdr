//! The typed events a drop from another app sends the frontend. Always compiled: the
//! emit sites are macOS-only, and `collect_events!` can't `#[cfg]`-gate an entry.

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

/// What an incoming drag carries, as far as a drop into Cmdr goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DragPayloadKind {
    /// Real files (`public.file-url`): Finder, most apps. The drop carries their paths.
    FileUrls,
    /// File promises and no file URLs: a browser image, a Mail attachment, a Photos item.
    /// The files exist only once Cmdr asks for them on drop.
    Promises,
    /// Raw image bytes and nothing else Cmdr can turn into a file.
    ImageData,
    /// A drag Cmdr itself started. Its identity is recorded at drag start, so the
    /// pasteboard is never read for it.
    InApp,
    /// Nothing Cmdr can make a file from (plain text, a bare link).
    Nothing,
}

/// `drag-payload-kind`: emitted on drag enter, before wry's own enter event, so the
/// frontend knows whether to light drop targets up at all.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "drag-payload-kind")]
pub struct DragPayloadKindEvent {
    /// What the drag carries.
    pub kind: DragPayloadKind,
}

/// Where a promised drop came from, as far as the pasteboard and the OS say. The
/// frontend turns it into the transfer dialog's source line; ❌ never a path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DropSource {
    /// The app the drag came from (the frontmost app while the drag entered), when it
    /// isn't Cmdr itself.
    pub app_name: Option<String>,
    /// The `public.url` on the pasteboard: for a browser image, the image's or the
    /// link's address.
    pub page_url: Option<String>,
}

/// `promise-drop-started`: a promise or image drop landed and Cmdr started receiving
/// it into a fresh drop folder. Emitted before wry's own drop event.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "promise-drop-started")]
pub struct PromiseDropStarted {
    /// Names this drop on the ready/failed event and on `release_promise_drop`.
    pub drop_id: String,
    /// How many files the drop promised.
    pub item_count: usize,
    /// Where it came from, for the dialog's source line.
    pub source: DropSource,
}

/// `promise-drop-ready`: the drop's files are on disk, at `paths`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "promise-drop-ready")]
pub struct PromiseDropReady {
    /// The drop's id from [`PromiseDropStarted`].
    pub drop_id: String,
    /// The received files, inside the drop folder.
    pub paths: Vec<String>,
    /// Promised files that didn't arrive (the source app refused or failed them).
    pub failed_count: usize,
}

/// Why a promised drop produced no file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PromiseDropFailure {
    /// The source app answered every promise with an error, or with nothing.
    NothingReceived,
    /// The drop folder couldn't be made.
    NoDropFolder,
    /// The source app didn't deliver within the receive window.
    TimedOut,
}

/// `promise-drop-failed`: the drop produced no file.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "promise-drop-failed")]
pub struct PromiseDropFailed {
    /// The drop's id from [`PromiseDropStarted`].
    pub drop_id: String,
    /// Why.
    pub reason: PromiseDropFailure,
}

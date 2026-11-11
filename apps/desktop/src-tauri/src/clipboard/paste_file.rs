//! Writes clipboard content (an already-read payload) into a directory as a new
//! file, the app half of "paste clipboard content as a file".
//!
//! Decoupled from NSPasteboard and the IPC edge: `write_payload_to_dir` takes an
//! already-read `ClipboardPayload` plus a target directory, so the pick →
//! sniff → unique-name → write path is unit-testable against a `TempDir` with no
//! Tauri runtime or `MainThreadMarker`. The command layer
//! (`commands/clipboard.rs`, through `platform/macos.rs`) reads the payload on the main thread and calls in.
//! The write itself is the engine's instant op
//! (`Operations::create_numbered_file`): `pasted.<ext>`, `pasted (1).<ext>`,
//! … as one managed, journaled create.

use std::path::{Path, PathBuf};

use crate::operations_host::{operations, refuse_instant_not_installed};
use cmdr_operation_log::types::Initiator;
use cmdr_operations::{MUTATION_REPLY_DEADLINE, MutationError, Operations, Replied, reply_or_hand_off};

use super::payload::PastedContent;
use super::{
    ClipboardPasteOutcome, ClipboardPasteSettled, ClipboardPayload, PasteClipboardReply, PastedClipboardFile,
    payload_to_content,
};

/// [`write_payload_to_dir`] under the instant-mutation reply deadline: the
/// created file (or `None`) if the write ended in time, else `StillRunning`,
/// with the real end handed to `on_settled` as a `ClipboardPasteSettled`. The
/// write is never dropped mid-flight (`crates/cmdr-operations/src/mutation_reply.rs`).
pub(crate) async fn write_payload_replying(
    volume_id: Option<String>,
    dir: PathBuf,
    payload: ClipboardPayload,
    on_settled: impl FnOnce(ClipboardPasteSettled) + Send + 'static,
) -> Result<PasteClipboardReply, MutationError> {
    let replied = reply_or_hand_off(
        MUTATION_REPLY_DEADLINE,
        async move { write_payload_to_dir(volume_id, &dir, payload).await },
        move |pending_id, result| {
            let outcome = match result {
                Ok(file) => ClipboardPasteOutcome::Landed { file },
                Err(error) => ClipboardPasteOutcome::Refused { error },
            };
            on_settled(ClipboardPasteSettled { pending_id, outcome });
        },
    )
    .await?;
    Ok(match replied {
        Replied::Done(file) => PasteClipboardReply::Done { file },
        Replied::StillRunning { pending_id } => PasteClipboardReply::StillRunning { pending_id },
    })
}

/// Writes the clipboard `payload` into `dir` as `pasted.<ext>` (unique-named on
/// collision), returning the created file's name + kind. `payload` resolving to
/// nothing pasteable → `Ok(None)`. `dir` is already tilde-expanded by the caller.
/// Paste is always user-initiated (no MCP path), so it journals as `User`.
pub(crate) async fn write_payload_to_dir(
    volume_id: Option<String>,
    dir: &Path,
    payload: ClipboardPayload,
) -> Result<Option<PastedClipboardFile>, MutationError> {
    let Some(content) = payload_to_content(payload) else {
        return Ok(None);
    };
    let engine = operations().map_err(refuse_instant_not_installed)?;
    write_content_on(engine, volume_id, dir, content).await.map(Some)
}

/// Writes already-picked `content` into `dir` on `engine`: [`write_payload_to_dir`]
/// past its pick, on the engine the caller names.
pub(crate) async fn write_content_on(
    engine: &Operations,
    volume_id: Option<String>,
    dir: &Path,
    content: PastedContent,
) -> Result<PastedClipboardFile, MutationError> {
    let name = engine
        .create_numbered_file(volume_id, dir, "pasted", content.ext, &content.bytes, Initiator::User)
        .await?;
    Ok(PastedClipboardFile {
        name,
        kind: content.kind,
    })
}

#[cfg(test)]
#[path = "paste_file_tests.rs"]
mod paste_file_tests;

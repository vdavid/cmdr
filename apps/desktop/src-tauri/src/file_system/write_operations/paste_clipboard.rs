//! Writes clipboard content (an already-read payload) into a directory as a new
//! file, the backend half of "paste clipboard content as a file".
//!
//! Decoupled from NSPasteboard and the IPC edge: `write_payload_to_dir` takes an
//! already-read `ClipboardPayload` plus a target directory, so the pick →
//! sniff → unique-name → write path is unit-testable against a `TempDir` with no
//! Tauri runtime or `MainThreadMarker`. The command layer
//! (`commands/clipboard.rs`) reads the payload on the main thread and calls in.
//!
//! Naming/dedup reuses the ONE ` (N)` convention via `conflict::numbered_name`
//! (the same helper `find_unique_name` uses), and writes atomically through
//! `Volume::create_file` (`O_CREAT|O_EXCL`): the loop tries `pasted.<ext>`,
//! `pasted (1).<ext>`, … retrying on the TYPED `VolumeError::AlreadyExists`, so
//! there's no pre-scan-then-write TOCTOU window and it works on any writable
//! volume (local or network).

use std::path::{Path, PathBuf};

use crate::clipboard::{ClipboardPayload, PastedClipboardFile, payload_to_content};
use crate::file_system::{VolumeError, get_volume_manager};
use crate::operation_log::types::{EntryType, Initiator, OpKind};

use super::conflict::numbered_name;
use super::manager;
use super::types::WriteOperationType;

/// Writes the clipboard `payload` into `dir` as `pasted.<ext>` (unique-named on
/// collision), returning the created file's name + kind. `payload` resolving to
/// nothing pasteable → `Ok(None)`. `dir` is already tilde-expanded by the caller.
///
/// The write loops candidate names from `conflict::numbered_name`, calling
/// `Volume::create_file` (atomic O_EXCL create+write) and retrying on the typed
/// `AlreadyExists`. Emits the create synthetic listing diff on success so the new
/// file lands in the pane and the cursor-land plumbing works like mkfile.
pub(crate) async fn write_payload_to_dir(
    volume_id: Option<String>,
    dir: &Path,
    payload: ClipboardPayload,
) -> Result<Option<PastedClipboardFile>, String> {
    let Some(content) = payload_to_content(payload) else {
        return Ok(None);
    };

    let volume_id_str = volume_id.clone().unwrap_or_else(|| "root".to_string());
    let volume = get_volume_manager()
        .get(&volume_id_str)
        .ok_or_else(|| format!("Volume not found: {volume_id_str}"))?;

    // Route through the managed `CreateFile` instant op (David-approved M2f): the
    // write is a real mutation, so it registers a brief `Running` record, marks
    // the volume busy, and — via the journal bracket — records a one-item
    // `CreateFile` operation exactly like mkfile, without a bespoke recorder. It
    // journals under the REAL volume id (`"root"` for the local drive), so a paste
    // onto SMB / MTP is captured too. Paste is always user-initiated (no MCP path).
    let descriptor = super::create::instant_descriptor(WriteOperationType::CreateFile, volume_id.as_deref(), "pasted");
    let op_id = descriptor.operation_id.clone();
    super::journal::open_volume_op(&op_id, OpKind::CreateFile, Initiator::User, &volume_id_str, None, 1);

    // Atomic O_EXCL create+write, retrying the shared `pasted (N).<ext>` names on
    // the typed AlreadyExists (no pre-scan TOCTOU; works on any writable volume).
    // The whole write runs INSIDE the managed op so its `Running` record brackets
    // the retries.
    let write = async {
        let mut counter: u32 = 0;
        loop {
            let name = numbered_name("pasted", Some(content.ext), counter);
            let path = dir.join(&name);
            // Register the destination with the downloads watcher's ignore set
            // before the syscall (no-op outside ~/Downloads).
            crate::downloads::note_pending_write_for_cmdr(&path);
            match volume.create_file(&path, &content.bytes).await {
                Ok(()) => break Ok::<(PathBuf, String), String>((path, name)),
                Err(VolumeError::AlreadyExists(_)) => {
                    counter = counter
                        .checked_add(1)
                        .ok_or("Too many name collisions for the pasted file")?;
                }
                Err(VolumeError::PermissionDenied(_)) => {
                    return Err(format!("Permission denied: can't write into '{}'", dir.display()));
                }
                Err(e) => return Err(format!("Couldn't write the pasted file: {e}")),
            }
        }
    };
    let result = manager::manager().run_instant(descriptor, write).await;

    super::journal::journal_instant_create(
        &op_id,
        OpKind::CreateFile,
        EntryType::File,
        &volume_id_str,
        result.as_ref().ok().map(|(path, _)| path.as_path()),
    );
    let (new_path, final_name) = result?;

    // Synthetic listing diff so the new file appears and the FE cursor-lands
    // (mirrors create_file). Local-FS volumes only.
    if super::create::should_emit_synthetic_diff(volume_id.as_deref()) {
        super::create::emit_synthetic_entry_diff(volume_id.as_deref(), &new_path, dir);
    }

    Ok(Some(PastedClipboardFile {
        name: final_name,
        kind: content.kind,
    }))
}

#[cfg(test)]
#[path = "paste_clipboard_tests.rs"]
mod paste_clipboard_tests;

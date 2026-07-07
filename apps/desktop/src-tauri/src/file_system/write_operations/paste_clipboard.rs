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

use std::path::Path;

use crate::clipboard::{ClipboardPayload, PastedClipboardFile, payload_to_content};
use crate::file_system::{VolumeError, get_volume_manager};

use super::conflict::numbered_name;

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

    // Atomic O_EXCL create+write, retrying the shared `pasted (N).<ext>` names on
    // the typed AlreadyExists (no pre-scan TOCTOU; works on any writable volume).
    let mut counter: u32 = 0;
    let final_name = loop {
        let name = numbered_name("pasted", Some(content.ext), counter);
        let path = dir.join(&name);
        // Register the destination with the downloads watcher's ignore set before
        // the syscall (no-op outside ~/Downloads).
        crate::downloads::note_pending_write_for_cmdr(&path);
        match volume.create_file(&path, &content.bytes).await {
            Ok(()) => break name,
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
    };

    // Synthetic listing diff so the new file appears and the FE cursor-lands
    // (mirrors create_file). Local-FS volumes only.
    if super::create::should_emit_synthetic_diff(volume_id.as_deref()) {
        let new_path = dir.join(&final_name);
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

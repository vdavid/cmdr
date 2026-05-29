# Batch trash with partial failures reports `write-complete`, hiding the failed items from the user

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/delete/trash.rs:185-239`

## What
`trash_files_with_progress` collects per-item errors in a local `errors: Vec<TrashItemError>` but only surfaces them via `log::warn!`. As long as at least one item trashed successfully (`items_done > 0`), the function emits `write-complete` and returns `Ok(())`. The FE has no signal that some items failed; the progress dialog closes cleanly, the user assumes "trashed" means "all trashed," and the failed items stay sitting in the directory.

`write-source-item-done` is only emitted for the successful items, so the FE can de-select them. The failed items stay selected — but the dialog closes and the toast says "Moved 49 of 50 items to Trash" only if the FE reads `WriteCompleteEvent.files_processed` and compares against the original selection size. There's no `files_failed` field on `WriteCompleteEvent`, no list of failed paths, no error event at all unless EVERY item failed (the `if items_done == 0 && !errors.is_empty()` branch is the only error emit site).

## Why it matters
User selects 50 files for Trash. 49 succeed; one fails because of an ACL bit, a locked-by-another-process status, or because it's on a network mount that briefly hung. The dialog closes successfully. The user goes about their work assuming the cleanup is done. The 50th file sits there. Hours / days later, it surprises them — they thought it was gone.

For the destructive-operation lens this is medium and not high because the failure mode is "data NOT moved to Trash" (the data is still there, so no loss); but the UX contract "you trashed these N items" is broken silently, and a user who actually NEEDED a sensitive file gone (an angry-email draft, a compromising photo) thinks they're safe when they're not.

## Evidence
`trash.rs:188-239`:
```rust
// If all items failed, emit error
if items_done == 0 && !errors.is_empty() {
    // ⚠ only ALL-FAIL triggers an error event.
    let error_summary = errors.iter()...;
    events.emit_error(WriteErrorEvent::new(...));
    return Err(...);
}

// Emit completion (may include partial errors)
events.emit_complete(WriteCompleteEvent {
    operation_id: operation_id.to_string(),
    operation_type: WriteOperationType::Trash,
    files_processed: items_done,
    files_skipped: 0,
    bytes_processed: bytes_done,
});

// Log partial failures
if !errors.is_empty() {
    log::warn!(
        "Trash operation {} completed with {} errors out of {} items",
        operation_id,
        errors.len(),
        items_total
    );
    for error in &errors {
        log::warn!("  Failed: {}: {}", error.path.display(), error.message);
    }
}

Ok(())
```

There's no `files_failed` field on `WriteCompleteEvent` (see `types.rs::WriteCompleteEvent`).

## Suggested fix
Add a `partial_failures: Vec<{ path: String, message: String }>` field to `WriteCompleteEvent` (specta-regen after). When the trash batch returns with some failures, populate this field. The FE dialog can then render "Trashed 49 of 50 items; 1 failed: <name>" with an expander for full details, instead of the unconditional success toast. The data-safety bar of the lens is met because no data is at risk here — the bar is "tell the user the truth about what happened."

A lighter touch: introduce a separate `write-partial-failure` event that fires when `errors.is_empty()` is false but the operation is not Err. The FE listens for it on every operation type.

The same pattern would help the volume-delete partial-failure path (which currently errors out the entire op via `?` propagation on the first failed delete — see `walker.rs:825`) and the cross-volume copy / move post-loop branches.

## Notes
- This is the only `write_operations` code path where partial failure is even attempted as a concept (the others bail on first failure). The framework for surfacing it just isn't wired through.
- The doc in `delete/CLAUDE.md` says "Partial failure is supported: if some items fail, others still succeed" — accurate as a runtime statement, but the user-facing signal is the gap.

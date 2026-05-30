# SMB streaming write leaves a partial file on the server when a WRITE/finish fails

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/volume/backends/smb.rs:2009-2049` (`write_from_stream` chunk loop, `finish`, and the compound fallback at `:1998-2004`).

## What
The SMB streaming-write path cleans up the partial destination file only on the **user-cancel** branch (`on_progress` returns `Break` → `writer.abort()` + best-effort `delete_file`). If a `write_chunk` fails mid-stream from a network drop / session loss, `self.handle_smb_result(..., write_result)?` propagates the error with the handle already created and partial bytes already on the wire — no `abort`, no `delete_file`. The same applies to a `finish()` failure. The result is a truncated file left at the destination name on the server.

## Why it matters
Copying a large file to a NAS over Wi-Fi / Tailscale; the link drops partway. The share is left with a partial file at the destination name. On a cross-volume **move** the source is preserved (source delete runs only after a successful copy), but the destination silently holds a corrupt file the user may later mistake for the complete copy — and there's no prompt telling them to discard it. The cancel path got this right; the error path is the gap.

## Evidence
```rust
// smb.rs:2009-2046
while let Some(chunk_result) = stream.next_chunk().await {
    let chunk = chunk_result?;
    if chunk.is_empty() { continue; }
    let write_result = writer.write_chunk(&chunk).await;
    self.handle_smb_result("write_from_stream(write_chunk)", write_result)?;  // ← error: no abort, no delete
    bytes_read += chunk.len() as u64;
    if on_progress(bytes_read, size) == std::ops::ControlFlow::Break(()) {
        let _ = writer.abort().await;                                          // ← cancel path cleans up
        if let Ok((tree, mut conn)) = self.clone_session().await {
            let _ = tree.delete_file(&mut conn, &smb_path).await;              // ← cancel path cleans up
        }
        return Err(VolumeError::Cancelled(...));
    }
}
let finish_result = writer.finish().await;
self.handle_smb_result("write_from_stream(finish)", finish_result)?;          // ← finish error: no delete
```

## Suggested fix
Wrap the streaming write in a guard so any early `Err` return (mid-stream `write_chunk` failure, `finish` failure, the compound-fallback failures) runs the same `writer.abort()` + best-effort `delete_file` cleanup the cancel branch already performs. A `scopeguard`-style "delete on non-success" wrapper disarmed on the success return keeps it uniform. Note the cleanup needs a live session (delete after a reconnect), so document that a server unreachable at error time still leaves the partial, recognizable by name.

## Notes
The cross-volume **file→file Overwrite** path is already safe by design (safe-replace into a `.cmdr-tmp-` sibling, documented in transfer CLAUDE.md). This finding is about the non-overwrite write into a fresh destination name, where a mid-stream error leaves a partial at the real name. Sibling to the MTP partial-upload finding.

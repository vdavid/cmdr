# `atomic_write_json` renames a temp it never fsync'd, so a power loss can leave a zero-length config file

**Severity:** low **Lens:** A — Data safety **Confidence:** high

## Location

`network/manual_servers.rs:367-372` `network/known_shares.rs:62-66` `search/history.rs:171-176`
`selection/history.rs:145-149`

## What

All four "atomic JSON" persistence helpers do `fs::write(tmp, content)` then `fs::rename(tmp, path)`. The `rename(2)`
makes the _directory entry_ swap atomic, but nothing flushes the temp file's **data** to disk before the rename, and the
parent directory is never fsync'd after it. On a power loss / hard crash in the window after `rename` returns but before
the filesystem flushes the temp's data blocks, the journaled rename can land while the data is still only in the page
cache — leaving the destination file zero-length or holding a torn write. The doc comments and module CLAUDE.md call
this pattern "atomic", which overstates the crash guarantee: it's atomic against process death, not against power loss.

## Why it matters

Of the four stores, `manual-servers.json` is the one that holds genuinely user-entered config (the SMB servers a user
added via "Connect to server…", which aren't rediscoverable via mDNS for non-broadcasting NAS boxes). A power loss right
after the user adds a server can wipe the whole list rather than just losing the last edit, because the rename can
replace the good file with a zero-length one. `known-shares.json`, `search-history.json`, and `selection-history.json`
are lower stakes (connection history / recent queries — re-accumulated through normal use), and the history readers
already quarantine a corrupt file and start fresh, so the blast radius there is "history resets," which is
annoyance-class.

## Evidence

```rust
// network/manual_servers.rs
/// Atomically writes content to a file using write-to-temp + rename.
/// On failure, the original file (if any) remains intact.
fn atomic_write_json(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
```

Compare with the project's own data-loss-class write path, which does flush
(`file_system/volume/backends/local_posix.rs:594-620`): `write_from_stream` calls `file.sync_data()` on the file and
`sync_all()` on the parent dir before reporting success, precisely to survive eject/sleep/power-loss. The config-store
helpers predate / sit outside that durability discipline.

## Suggested fix

In `atomic_write_json`, open the temp with `OpenOptions`, `write_all`, then `f.sync_all()` (or `sync_data()`) before
dropping it, and after the `fs::rename` open the parent directory and `sync_all()` it so the rename itself is durable.
This mirrors `flush_created_destinations` / `LocalPosixVolume::write_from_stream`. Keep it best-effort-logged like those
sites if a filesystem rejects directory fsync. Given that three of the four stores are annoyance-class and already
self-heal, the change is most worth making for `manual_servers.rs`; applying it uniformly to the shared shape is cheap
and avoids the next "which store was the important one" question.

## Notes

The delete/trash CLAUDE.md explicitly reasons that _delete_ durability is annoyance-class and not worth an fsync, while
_copy/move_ are data-loss-class and do flush. These config writes were never slotted into that taxonomy. They're closer
to copy/move (the only on-disk copy of small, hand-entered or accumulated state) than to delete, but the data volume is
tiny so the cost of flushing is negligible. No CLAUDE.md documents the "atomic == process-death-only, not power-loss"
caveat, so this isn't a documented trade-off.

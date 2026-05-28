# Regular-file copy conflict check misses dangling symlink at destination

**Severity:** medium **Lens:** A — Data safety **Confidence:** high

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/transfer/copy.rs:702` (regular-file path) vs.
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/copy.rs:648` (symlink path)

## What

`copy_single_item` has two parallel branches for "does the destination already exist?":

- **Symlink branch (line 648):** `if dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok()` — correctly catches
  dangling symlinks (`.exists()` returns `false` for them; `symlink_metadata` succeeds).
- **Regular-file branch (line 702):** `if dest_path.exists()` — `.exists()` follows symlinks. A dangling symlink at
  `dest_path` returns `false`. The branch falls through to "no conflict, treat it as fresh write."

The copy then opens `dest_path` for writing via `copy_file_with_strategy`, which on POSIX follows the symlink and writes
to wherever the symlink points (or fails if the target dir doesn't exist).

## Why it matters

Two paths to user-visible damage:

1. **Silent clobber via symlink redirection.** If `~/Downloads/keepers/README` is a symlink to `~/important/notes.md`,
   and the user copies a fresh `README` into `~/Downloads/keepers/`, the conflict dialog never fires. The copy follows
   the symlink and overwrites `~/important/notes.md` with no warning. The user expected to either be prompted or to land
   a new file in `keepers/`; they got neither.
2. **Surprise IoError on dangling symlinks.** If the symlink target doesn't exist, `fs::File::create` returns `ENOENT`
   (the target's parent doesn't exist) — but pointed at the wrong path. The error message is correct but confusing; the
   operation aborts mid-batch and rolls back.

Both cases break the principle from `AGENTS.md` § "Protect the user's data". The conflict resolution flow's whole point
is "don't silently clobber"; missing the dest-is-symlink case is exactly the silent clobber the system exists to
prevent.

## Evidence

Symlink branch — correct:

```rust
// Handle symlink
let (actual_dest, needs_safe_overwrite) = if dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok() {
    match resolve_conflict(...)? {
        Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
        None => { *files_done += 1; *bytes_done += metadata.len(); return Ok(()); }
    }
} else {
    (dest_path.clone(), false)
};
```

Regular-file branch — buggy:

```rust
} else {
    // Handle regular file
    let (actual_dest, needs_safe_overwrite) = if dest_path.exists() {  // ← misses dangling symlink
        match resolve_conflict(...)? { ... }
    } else {
        (dest_path.clone(), false)
    };
```

## Suggested fix

Mirror the symlink branch's check:

```rust
let (actual_dest, needs_safe_overwrite) = if dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok() {
    match resolve_conflict(source, &dest_path, ...)? {
        Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
        None => { *files_done += 1; *bytes_done += metadata.len(); return Ok(()); }
    }
} else {
    (dest_path.clone(), false)
};
```

Better: extract a `dest_exists_or_is_symlink(dest_path)` helper and call it from both branches so the contract is
single-sourced. The same fix probably wants mirroring in `move_op.rs::move_with_rename:151` (`dest_path.exists()`) and
`merge_move_directory:277` (`dest_child.exists()`) — the move flow has identical shape.

## Notes

A regression test is straightforward: create a dangling symlink at the dest path, attempt a copy of a regular file,
assert the conflict dialog fires (or `Skip` / `Stop` resolution is honored). Goes in `copy_integration_test.rs`.

# `find_unique_name` has a TOCTOU race on the chosen filename

**Severity:** low **Lens:** A — Data safety **Confidence:** high

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:543-563`

## What

`find_unique_name` loops `name (1)`, `name (2)`, ... and returns the first path where `new_path.exists()` is `false`.
Between the existence check and the eventual `fs::copy` / `fs::rename` that lands the file at that path, another process
(or another Cmdr operation against the same dir) can create the file with the same name. The conflict-resolution
`Rename` resolution then either clobbers the unrelated file (if it landed first), or fails with a confusing error
mid-copy.

## Why it matters

The window is small — milliseconds — but it's open on every "Rename" conflict resolution and every "apply rename to all"
follow-up. The likely victims:

- A backup tool (Time Machine, Arq, Restic) writing in parallel into the same dir.
- A second Cmdr operation (the user has two transfer dialogs going).
- A cloud-sync agent (Dropbox, iCloud) materializing files.

Worst case is a silent overwrite of an unrelated file (data loss). More common case is the copy aborts mid-batch with a
`File exists` error and the user sees a confusing failure for a file they thought they were renaming around.

## Evidence

```rust
pub(super) fn find_unique_name(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = ...;
    let extension = ...;

    let mut counter = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);
        if !new_path.exists() {        // ← check
            return new_path;           //   gap before the write
        }
        counter += 1;
    }
}
```

The caller then does an unrelated copy/rename to that path, without re-checking.

## Suggested fix

Push the existence check down to the actual write. Two practical shapes:

1. **O_CREAT|O_EXCL at the write site.** Open the destination with
   `OpenOptions::new().write(true).create_new(true).open(...)`; on `EEXIST`, bump the counter and retry from the helper.
   This makes the uniqueness check atomic with the write.
2. **Reserve via temp file.** Create `name (N).cmdr-tmp-<uuid>` via O_EXCL up front; if it lands, rename it to
   `name (N)` immediately. The temp create is atomic; the rename can still fail on a same-name race, but the failure is
   recoverable inside the helper instead of bubbling out half-way through a copy.

Option 1 is simpler and matches what the symlink-handling branch already does implicitly via
`std::os::unix::fs::symlink` (which is O_EXCL semantically). The same applies to the directory-conflict branch in
`copy_single_item` that calls `find_unique_name(&blocking)` and then `fs::rename(&blocking, &unique_path)`.

## Notes

The race is also present in the symlink-rename branch at `copy.rs:594` and the regular-file conflict-resolution path. A
single typed helper that returns an already-created file handle would close all these sites at once. Probably worth the
refactor before launch — file managers race with cloud agents constantly.

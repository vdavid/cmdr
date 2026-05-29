# Rename conflict resolution leaves an O_EXCL placeholder that the APFS clonefile / Linux `copy_file_range` paths refuse to overwrite

**Severity:** high
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:559-607` (`find_unique_name`)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/copy_strategy.rs:108-136` (macOS strategy dispatch)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/macos_copy.rs:265-361` (`COPYFILE_EXCL` semantics)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/linux_copy.rs:32-53` (`create_new(true)`)

## What
`find_unique_name` atomically reserves a unique filename by creating an empty placeholder via `OpenOptions::write(true).create_new(true).open(...)`. The doc explicitly relies on downstream copy ops to truncate-overwrite the placeholder ("`copyfile(3)` / `copy_file_range(2)` open the dest with create+truncate"). That assumption is wrong on two of the three production paths:

1. **macOS APFS clonefile / `copy_single_file_native`**: `copy_file_with_strategy` passes `overwrite = false` → `CopyOptions { exclusive: true }` → `COPYFILE_EXCL` flag is set. Per Apple's docs, `COPYFILE_EXCL` "If a file at the destination path exists, copyfile() will not overwrite it." The placeholder created by `find_unique_name` exists, so `copyfile` returns `EEXIST`, the Rename branch returns `WriteOperationError::DestinationExists`, and the placeholder stays on disk as a zero-byte file.

2. **Linux non-network / `copy_single_file_linux`**: also called with `overwrite = false`, which routes to `OpenOptions::new().write(true).create_new(true).open(destination)`. `create_new` IS `O_CREAT|O_EXCL`. The pre-existing placeholder makes the open fail with `EEXIST`, same outcome as macOS — the copy is dropped on the floor.

3. **macOS non-APFS / chunked copy / Linux network**: chunked_copy_with_metadata uses `File::create(dest)` which is `O_CREAT|O_TRUNC` — it DOES overwrite the placeholder, so the Rename-resolved copy lands correctly. Same for `safe_overwrite_file → fs::copy` on Linux.

## Why it matters
A user copies a folder of files into a destination with same-named existing files, picks "Rename all" so they keep both. On macOS APFS (the modal Cmdr environment — every modern Mac since 2017) and on Linux local-FS, every single file in the operation that triggers Rename gets a placeholder `*(1).ext` created and then no real content written. The user sees:
- Zero-byte `*(1).ext`, `*(2).ext`, etc. files at the destination.
- Either `write-error` on the first such file (aborting the operation) or — depending on how `copy_single_item` propagates the error — possibly a partial-success cascade.

So every cross-conflict copy with the Rename resolution silently produces zero-byte placeholders on the primary platform Cmdr targets. This isn't just a UX issue: the user is told "Rename: copying as `file (1).txt`" and the destination contains zero-byte `file (1).txt`. The actual file content is nowhere on disk — they have to redo the copy.

There's no test coverage for the Rename + APFS clonefile path. The closest is `test_copy_fails_on_existing_exclusive` in `macos_copy.rs:548-564`, which actually CONFIRMS the failing behavior ("Original content should be preserved" when an existing file blocks the COPYFILE_EXCL copy).

## Evidence
`helpers.rs:577-606`:
```rust
pub(super) fn find_unique_name(path: &Path) -> PathBuf {
    ...
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);

        match fs::OpenOptions::new().write(true).create_new(true).open(&new_path) {
            Ok(_) => return new_path,
            //                 ⚠ Returns with empty placeholder still on disk.
            ...
        }
    }
}
```

`copy_strategy.rs:108-127` (macOS APFS path, `needs_safe_overwrite=false` for Rename):
```rust
if is_same_apfs_volume(source, dest) {
    ...
    if needs_safe_overwrite {
        safe_overwrite_file(...)
    } else {
        copy_single_file_native(source, dest, false, Some(&context))
        //                                     ⚠ overwrite=false → COPYFILE_EXCL
    }
}
```

`macos_copy.rs:367-376` (translation of `overwrite=false` to flags):
```rust
pub fn copy_single_file_native(
    source: &Path,
    destination: &Path,
    overwrite: bool,
    context: Option<&CopyProgressContext>,
) -> Result<u64, WriteOperationError> {
    let options = CopyOptions {
        exclusive: !overwrite, // ⚠ false → exclusive=true
        recursive: false,
        preserve_symlinks: true,
    };
    copy_file_native(source, destination, options, context)?;
    ...
}
```

`macos_copy.rs:281-303` (the actual `COPYFILE_EXCL` flag set):
```rust
if options.exclusive {
    flags |= COPYFILE_EXCL;
}

if options.recursive && source.is_dir() {
    flags |= COPYFILE_RECURSIVE;
}

// If not exclusive and destination exists, remove it first
if !options.exclusive && destination.exists() {
    ...
}
```

`linux_copy.rs:47-53`:
```rust
let dst_file = if overwrite {
    fs::File::create(destination)
} else {
    fs::OpenOptions::new().write(true).create_new(true).open(destination)
    //                                  ⚠ Fails because placeholder exists.
}
.map_err(|e| map_io_error(e, source, destination))?;
```

## Suggested fix
Three options, in increasing order of correctness:

1. **Quick fix**: pass `needs_safe_overwrite = true` (or a new `replace_placeholder = true` flag) for the Rename branch in `apply_resolution`. That routes the macOS APFS path through `safe_overwrite_file` (which copies to a fresh temp+rename, working around the placeholder) and the Linux path through... wait, `safe_overwrite_file` on Linux uses `fs::copy` which truncates, so that works too. This is a minimal fix.

2. **Better fix**: in `find_unique_name`, after picking the name, IMMEDIATELY remove the placeholder before returning (`fs::remove_file(&new_path)`). Yes, this reintroduces the TOCTOU window the placeholder was meant to close — but a microsecond-scale race window is strictly better than 100% data loss on the modal platform. Pair with a documented warning that "concurrent process may race the rename" is the trade-off.

3. **Correct fix**: change `copy_single_file_native` (and `copy_single_file_linux`) to support a "overwrite this empty placeholder" mode that drops `COPYFILE_EXCL` / `O_EXCL` while still rejecting non-empty pre-existing files. That preserves the placeholder's race-safety AND lets the copy land. The cost is a stat-before-copy on the Rename path; only the Rename branch needs the new mode.

A workaround pending the fix: change the Rename branch's downstream call to `chunked_copy_with_metadata` only (the path that DOES truncate). Loses APFS clonefile speed, but works.

## Notes
- Coverage gap: no integration test drives a `ConflictResolution::Rename` end-to-end on APFS. `find_unique_name_tests` only tests that the placeholder is created; it doesn't verify the subsequent copy succeeds. `copy_integration_test.rs` doesn't cover Rename. Add one.
- The `find_unique_name` doc is currently misleading: it asserts "`copyfile(3)` / `copy_file_range(2)` open the dest with create+truncate" — they don't, when called with the exclusive flag the code passes.
- This is a behavioral regression introduced by the "atomically reserve" fix (the find_unique_name doc references it as "Pre-fix this returned the first non-existing candidate after an `if !new_path.exists()` check"). The pre-fix path had a TOCTOU race; the post-fix path has a 100% rename-failure on the dominant platform. Both are bugs; the trade-off currently lands on the worse side.

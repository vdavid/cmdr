# Updater can lose TCC (FDA) grants if it crashes mid-sync

**Severity:** medium
**Lens:** E — macOS pitfalls
**Confidence:** medium

## Location

- `apps/desktop/src-tauri/src/updater/installer.rs` — `sync_bundle()` (lines 167–185), `copy_file_creating_dirs()` (lines 247–269), `delete_stale_files()` (lines 303–317), `sync_with_admin_privileges()` (lines 358–372).
- `apps/desktop/src-tauri/src/updater/CLAUDE.md` — claims "syncs files into the bundle … keeps macOS TCC permissions intact across updates."

## What

The whole premise of the custom updater is that **the `.app` bundle directory keeps its inode and `com.apple.macl` xattr** so TCC (Full Disk Access) survives the upgrade. The implementation honors this for the *bundle directory itself*, but the inside-bundle mutations are not crash-safe in a way that preserves the TCC-relevant state across a power loss or mid-install panic:

1. **No bundle-level snapshot.** There's no temp-bundle-rename-swap (a la `rsync --link-dest` + `mv`). Files are written one-by-one into the live `Cmdr.app/Contents/`. A crash partway through leaves the bundle with new `Resources/`, new `_CodeSignature/`, new `Info.plist`, but possibly an old `MacOS/Cmdr` binary (or vice versa). The next launch could SIGKILL on code-signature mismatch.
2. **`delete_stale_files()` runs after** the new files are written, but uses `fs::remove_file` non-atomically. A crash here leaves orphan files behind. Less dangerous, but means the bundle is in an undefined-but-not-fatal state.
3. **`sync_with_admin_privileges()` is `rsync -a --delete` shelled via osascript.** rsync's own atomicity is per-file (it uses temp + rename internally), but if osascript is killed mid-rsync the bundle is in the same mixed-version state as path 1.

The `com.apple.macl` xattr lives on the bundle directory, not on individual files. As long as `fs::create_dir_all` / `rsync` never replace or recreate `Cmdr.app/` or `Cmdr.app/Contents/`, the xattr survives. The current code only ever writes *into* those directories, never replaces them — so the TCC grant itself almost certainly survives even a mid-install crash. **The risk isn't losing FDA; it's launching with a half-installed app that crashes on signature mismatch and forces the user through a reinstall.** If the user reinstalls from a fresh `.dmg`, the new download creates a new bundle (new inode) and FDA is gone — that's the realistic loss path.

## Why it matters

- **AGENTS.md Principle 4** ("Protect the user's data … Design for the crash mid-operation"). The updater is exactly the kind of write path that should assume the hostile case.
- A SIGKILL on launch (code signature invalid) is one of the worst UX outcomes: the user can't open the app to even see an error message. They'll Google, find "delete and reinstall," and lose FDA in the process.
- The CLAUDE.md decision "Updating the binary last minimizes the window where the code signature is inconsistent" is correct as far as it goes, but "minimizes" ≠ "eliminates." There IS a window.

## Evidence

`sync_bundle()` in `installer.rs:167–185`:

```rust
sync_subtree(src, dest, Path::new("Resources"))?;
sync_file_if_exists(src, dest, Path::new("Info.plist"))?;
sync_subtree(src, dest, Path::new("_CodeSignature"))?;
sync_file_if_exists(src, dest, Path::new("CodeResources"))?;
sync_subtree(src, dest, Path::new("MacOS"))?;
// ... then delete_stale_files
```

Each phase commits to disk independently. Crash between phase 4 and phase 5 leaves new `_CodeSignature/` against the old `MacOS/Cmdr`.

`copy_file_creating_dirs()` (lines 247–269) IS atomic per-file (temp + rename), which protects against partial file writes. But there's no journal across files.

## Suggested fix

Either:

1. **Sidecar bundle swap with directory rename.** Stage the new bundle as a sibling (`Cmdr.app.new/`), then do `renameat2(..., RENAME_EXCHANGE)` (or `fs::rename` if `.new` is empty target) to swap. macOS supports `renameatx_np` with `RENAME_SWAP` since 10.12. This preserves the original bundle's inode for the `Cmdr.app` *name* path while the contents swap atomically. Note: `com.apple.macl` xattr survives swap because it's keyed by inode AND path; verify experimentally on a notarized build before committing.
2. **Or: ship a recovery launcher** that, on next launch, detects a half-committed bundle (e.g. a sentinel file written first/cleared last by the updater) and re-pulls the tarball to retry.
3. **At minimum, add a sentinel file** (`Contents/.cmdr-install-in-progress`) written before phase 1 and removed after phase 5 + delete_stale_files. The launcher (or a Tauri pre-init hook) refuses to launch if the sentinel is present, prompting the user to retry the update.

The sentinel option is the cheapest mitigation: it converts "SIGKILL on launch" into "in-app 'update interrupted, retry?' dialog," which the user can act on without losing FDA.

## Notes

- Hard to repro without a real notarized build + power-pulling the machine. Confidence is medium because the worst-case (TCC loss via reinstall) requires a specific user response to a specific failure mode.
- Out of scope for this finding: `sync_with_admin_privileges()` runs `rsync -a` which does NOT preserve `com.apple.macl` xattrs unless `--xattrs` is added. Currently uses `-a` only. On the admin path, the bundle dir itself is never replaced (rsync syncs Contents/), so the xattr on `Cmdr.app/` survives — but if Apple ever moves the xattr onto Contents/ or below, this becomes load-bearing. Worth a sanity check next time you touch this.

# Volume backends

Per-backend `Volume` impls. Trait shape, capabilities, streaming patterns, "Building a new volume": `../CLAUDE.md`
+ `../DETAILS.md`.

## Module map

- **Only one backend still lives IN the app**: `local_posix.rs`, with its own tests. `InMemoryVolume` rides with the
  trait in `cmdr-fs`. It's split by concern the way the remote backends are: the struct and the query/mutation methods
  here, `local_posix/{scan,streams}.rs` beside it. A trait impl can't span files, so a moved method stays a one-line
  delegation to a `pub(super)` inherent body (SMB carries the pattern further; see `crates/cmdr-smb/CLAUDE.md`).
- **Every other backend is a crate**, imported by crate name at its call sites, never re-exported through here:
  `cmdr-archive`, `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-adb`, and `cmdr-mtp`. ❌ Don't add a
  `pub use <crate>::*;` module here to spare a call site the crate name: it puts the backend back in a directory it
  doesn't live in, and it becomes the place app-side tests drift to.
- **A crate backend's app-side tests live beside the app code they assert on**, not here. SMB's are in
  `file_system/write_operations/` (transfers, remote archives, the shared `smb_test_support.rs`),
  `file_system/listing/` (the pane-close watcher cell), and `file_system/volume/` (index scan, media fetch); the archive
  watch cell is in `file_system/listing/`. Which side a cell belongs on: `crates/cmdr-smb/DETAILS.md` § "Which side a
  test lives on".
- The app-side tracker, provider, and connect wiring for ADB and MTP live in `src-tauri/src/adb/` and
  `src-tauri/src/mtp/`, not here.

## Local and MTP must-knows

MTP is `crates/cmdr-mtp` now; the two below are what a caller reaching it from the app still has to know.

- **Feed the progress callbacks** in `list_directory` and in a copy SCAN (`scan_for_copy_batch_with_boundary`);
  ❌ never quiet one to `_on_progress`. They drive the pane's only "Loaded N files…" readout and the transfer dialog's
  only climbing counter, and the scan one is the watchdog's proof the device is answering: a silent backend gets cut
  off as unresponsive.
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file** (+ best-effort parent-dir fsync) before returning:
  every cross-volume copy landing on local disk flows through it, and `flush()` alone loses data on eject.
- **MTP has no single-file stat**, so `get_metadata` lists the whole parent: avoid it in hot paths. Ranged reads and
  read sessions are canonical in `crates/cmdr-mtp/src/connection/CLAUDE.md`.
- **`cmdr_mtp::volume::testing` is the ONLY way a test outside the backend reads it**: two numbers out
  (`list_directory_call_count`, `reset_list_directory_call_count`) and one in (`set_read_window`), gated on
  `any(test, feature = "testing")`, ❌ never `cfg(test)` alone. Don't grow it into a way to reach backend state.

Per-backend decisions, supersede-vs-unmount, and the SMB auto-upgrade lifecycle: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.

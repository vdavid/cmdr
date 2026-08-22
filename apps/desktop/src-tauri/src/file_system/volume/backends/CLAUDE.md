# Volume backends

Per-backend `Volume` impls. Trait shape, capabilities, streaming patterns, "Building a new volume": `../CLAUDE.md`
+ `../DETAILS.md`.

## Module map

- `local_posix.rs` and `mtp/` are implemented here; `archive.rs` and `smb.rs` are one-line re-exports of
  `crates/cmdr-archive` and `crates/cmdr-smb`, each carrying the app-side half of its suites. `InMemoryVolume` rides
  with the trait in `cmdr-fs`. MTP splits by concern the way both remote backends do: `volume_impl` is the whole
  `impl Volume`, with `streams`, `mapping`, and `scan` beside it (SMB carries the pattern further; see
  `crates/cmdr-smb/CLAUDE.md`).

## SMB is a crate now

`crates/cmdr-smb/` holds the backend; `smb.rs` here is a re-export of it plus the app-side half of its suites. The
must-knows moved with it: `crates/cmdr-smb/CLAUDE.md`. What's still this side is the auto-upgrade lifecycle
(`DETAILS.md` § "SMB auto-upgrade lifecycle", which is `network/`'s) and the cells that drive this app's transfer
pipeline, registry, listing cache, or media enrichment.

## Local and MTP must-knows

- **Feed the progress callbacks** in `list_directory` and in a copy SCAN (`scan_for_copy_batch_with_progress`);
  ❌ never quiet one to `_on_progress`. They drive the pane's only "Loaded N files…" readout and the transfer dialog's
  only climbing counter, and the scan one is the watchdog's proof the device is answering: a silent backend gets cut
  off as unresponsive.
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file** (+ best-effort parent-dir fsync) before returning:
  every cross-volume copy landing on local disk flows through it, and `flush()` alone loses data on eject.
- **MTP has no single-file stat**, so `get_metadata` lists the whole parent: avoid it in hot paths. Ranged reads and
  read sessions are canonical in `mtp/connection/CLAUDE.md`.

Per-backend decisions, supersede-vs-unmount, and the SMB auto-upgrade lifecycle: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.

# File system module

Sync status, volume management, and file watching. The engines are crates: listing `crates/cmdr-listing`, writes
`crates/cmdr-operations`.

## Module map

- Submodules with their own docs: `volume/`, `sync_status/` (cloud badges). Around them, a dozen leaves (`watcher.rs`, `google_drive/`, `share.rs`, …), one line each in DETAILS § Module map.
- `mod.rs` is a facade: it re-exports downward and bootstraps the volume registry (`init_volume_manager`), which is why
  it may know every backend.

## Gotchas

- **Reach the volume-manager singleton at `volume::manager::get_volume_manager()`; ❌ never re-export it from here**: a
  facade that also hands out the accessor welds the subtree into one cycle. `volume/DETAILS.md` § "Key decisions".
- **Transient scratch hides on the listing READ path only** (`cmdr-listing`'s `CachedListing::rows`): a watcher-side
  skip strands an entry in the pane forever. Cmdr's own (`.cmdr-tmp-*`) hides by OWNERSHIP, other apps' (`.sb-`) by
  NAME. § "Hiding transient scratch".
- **Tag writes touch ONLY `_kMDItemUserTags`** (`FinderInfo` holds folder icons), as **binary** plist, merging
  via `read_tags_strict` (❌ never `read_tags`).
- **Never call macOS frameworks from rayon or any constrained-stack pool**: FileProvider XPC blows the 2 MB stack and
  can block forever. Use pooled, hard-capped 8 MB OS threads (`framework_pool.rs`). § "Threading".
- **Watcher rules** (each has a `DETAILS.md` section; read them before touching `watcher.rs`):
  - ❌ Never `tokio::spawn` from a watcher OS thread (no reactor: it panics). Use `tauri::async_runtime::spawn`.
  - The watcher is the engine's first `ListingObserver` and keeps only the OS half: changes go to
    `Listings::observe_children`, full re-reads to `Listings::refresh`.
  - Arm listing watches with `start_watching_detached`, ❌ never `start_watching` (p90 653 ms inline). A late arm
    misses earlier changes, so `arm_missed_a_change` (mtime) gates a re-read, ❌ never an unconditional one.
  - Rebase event paths (`rebase_event_path`), or firmlinks (`/tmp`, `/var`, `/etc`) plus a symlinked watch root drop
    every event.
  - A created, removed, or renamed watch ROOT forces a full re-read; ❌ don't add `Modify(Metadata(_))`, since every
    child change bumps the dir's mtime.
  - Test builds arm no OS stream; a test needing real delivery calls `watch_for_real` first. § "Tests arm no OS
    stream".
  - A re-sorted row is one `DiffChangeType::Move`, ❌ never a remove plus an add (cursor and selection ride along by
    identity). § "Reordered rows".
- **A watch on an OS-mounted network share is `WatchCoverage::ThisMachineOnly`, never `EveryWriter`** (FSEvents on
  `smbfs` sees this machine's writes only). ❌ Don't refuse to arm it over that; what it must not do is let a delete
  walker or copy scan skip a read. `volume/DETAILS.md` § "Trait capability model".
- **"Open terminal here" asks `NSWorkspace` per known app, ❌ never scans `/Applications`**; recipes are a pure
  `launch_argv`. New terminals need a sourced, dated `KNOWN_TERMINALS` entry. § "Open terminal here".
- **F4's editors come from LaunchServices' editor-ROLE plain-text query** (`text_editor.rs`): ❌ never the UTI crate
  (10.15 floor) or a made-up `.txt` URL (no apps). § "Text editor".
- **`cloud_actions.rs` is iCloud Drive only**, gated by `CloudProvider::supports_eviction`; the cross-provider-looking
  `NSFileProviderManager` methods need the bundled extension. Don't widen it. Provider identity lives once, in
  `cmdr_fs::cloud_provider`, which the volume switcher reads too.
- **Google Drive (`google_drive/`), three ❌:** never gate on a path prefix (mirror mode keeps real files outside
  `~/Library/CloudStorage` with no xattr, so we gate on a resolved item ID); never resolve a mirrored file by NAME;
  never read a mirror `stable_id` from the stream-mode `metadata_sqlite_db` (different id space). `mirror_db.rs` walks
  up to a root by inode, down by indexed `(parent, name)`, then proves the row with the file's inode. Read-only, and
  every failure means no menu item: a wrong link points at someone else's file. § "Mirror mode".

Read `DETAILS.md` before non-trivial work here: open-with, cloud actions, threading, and watchers.

# File system module

Directory listing, file writing, sync status, volume management, and file watching.

## Module map

- Submodules with their own docs: `listing/`, `write_operations/`, `volume/`, `sync_status/` (cloud badges).
- `watcher.rs` (FSEvents listing updates), `staging.rs` (scratch visibility; the `StagingTemp` mint itself is
  `cmdr_fs::staging`), `index_provider.rs` (the app's `VolumeProvider`, so the index never imports `VolumeManager`),
  `backend_settings.rs` (live per-backend knobs), `cloud_actions.rs`, `open_with.rs`, `tags.rs` (Finder tags),
  `terminal.rs` ("open terminal here").
- `mod.rs` is a facade: it re-exports downward and bootstraps the volume registry (`init_volume_manager`), which is why
  it may know every backend.

## Gotchas

- **Reach the volume-manager singleton at `volume::manager::get_volume_manager()`; ❌ never re-export it from here**: a
  facade that also hands out the accessor welds the subtree into one cycle. `volume/DETAILS.md` § "Key decisions".
- **Transient scratch hides on the listing READ path, nowhere but `CachedListing::rows`**: a watcher-side skip strands
  an entry in the pane forever. Cmdr's own (`.cmdr-tmp-*`) hides by OWNERSHIP, other apps' (`.sb-`) by NAME.
  `is_hidden_from_listings` is GATED on the pure `could_be_hidden_from_listings`, which is what lets the listing layer
  cache row numbers and re-ask about only the scratch-named few. § "Hiding transient scratch".
- **Tag writes (`tags.rs`) touch ONLY `_kMDItemUserTags`, never `com.apple.FinderInfo`** (that blob carries
  `kHasCustomIcon`, so zeroing it destroys custom folder icons), and encode a **binary** plist (`plist` defaults to
  XML).
- **Never call macOS frameworks from rayon or any constrained-stack pool**: NSURL/FileProvider XPC round-trips blow the
  2 MB worker stack and can block forever. Use pooled, hard-capped 8 MB OS threads (`sync_status/pool.rs`); a per-call
  `std::thread::scope` is NOT enough. § "Threading".
- **Watcher rules.** Each has its own section in `DETAILS.md`; read them before touching `watcher.rs`:
  - ❌ Never `tokio::spawn` from a watcher OS thread (no reactor: it panics). Use `tauri::async_runtime::spawn`, and
    `caching::spawn_full_refresh` for FullRefresh dispatch.
  - Arm listing watches with `start_watching_detached`, ❌ never `start_watching` (an inline arm was p90 653 ms of dead
    time), keeping all four rules that made it cheap.
  - Rebase event paths (`rebase_event_path`), or firmlinks (`/tmp`, `/var`, `/etc`) and a symlinked watch root drop
    every event.
  - A created, removed, or renamed watch ROOT forces a full re-read; ❌ don't add `Modify(Metadata(_))` to that
    trigger, since every child change bumps the dir's mtime.
  - A row that jumped its sorted position is one `DiffChangeType::Move`, not a remove plus an add: the pane rides the
    cursor and the selection along a move by identity. § "Reordered rows".
- **A watch on an OS-mounted network share is `WatchCoverage::ThisMachineOnly`, never `EveryWriter`**: FSEvents on
  `smbfs` sees this machine's writes only. ❌ Don't refuse to arm the watch over that; what it must not do is let a
  delete walker or copy scan skip a read. `volume/DETAILS.md` § "Trait capability model".
- **"Open terminal here" asks `NSWorkspace` whether each known app is installed; ❌ never scans `/Applications`**, and
  its recipes are a pure `launch_argv` so every app's argv is unit-tested without launching anything. A new terminal is
  one entry in `KNOWN_TERMINALS`, and it owes the bundle id's verification source and date. § "Open terminal here".
- **`cloud_actions.rs` is iCloud Drive only**, gated by `is_in_icloud_drive`; the cross-provider-looking
  `NSFileProviderManager` methods need the bundled extension. Don't widen it.

Open-with internals, cloud-actions rationale, and the full threading/watcher story: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.

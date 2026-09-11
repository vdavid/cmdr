# File system module

Directory listing, file writing, sync status, volume management, and file watching.

## Module map

- Submodules with their own docs: `listing/`, `write_operations/`, `volume/`, `sync_status/` (cloud badges). Around
  them, a dozen leaves (`watcher.rs`, `google_drive/`, `share.rs`, …), one line each in DETAILS § Module map.
- `mod.rs` is a facade: it re-exports downward and bootstraps the volume registry (`init_volume_manager`), which is why
  it may know every backend.

## Gotchas

- **Reach the volume-manager singleton at `volume::manager::get_volume_manager()`; ❌ never re-export it from here**: a
  facade that also hands out the accessor welds the subtree into one cycle. `volume/DETAILS.md` § "Key decisions".
- **Transient scratch hides on the listing READ path, nowhere but `CachedListing::rows`**: a watcher-side skip strands
  an entry in the pane forever. Cmdr's own (`.cmdr-tmp-*`) hides by OWNERSHIP, other apps' (`.sb-`) by NAME, and
  `is_hidden_from_listings` is GATED on the pure `could_be_hidden_from_listings`, which is what keeps cached row numbers
  valid. § "Hiding transient scratch".
- **Tag writes (`tags.rs`) touch ONLY `_kMDItemUserTags`, never `com.apple.FinderInfo`** (zeroing it destroys custom
  folder icons), and encode a **binary** plist.
- **Never call macOS frameworks from rayon or any constrained-stack pool**: FileProvider XPC blows the 2 MB stack and
  can block forever. Use pooled, hard-capped 8 MB OS threads (`sync_status/pool.rs`), not a per-call
  `std::thread::scope`. § "Threading".
- **Watcher rules.** Each has its own section in `DETAILS.md`; read them before touching `watcher.rs`:
  - ❌ Never `tokio::spawn` from a watcher OS thread (no reactor: it panics). Use `tauri::async_runtime::spawn`, and
    `caching::spawn_full_refresh` for FullRefresh dispatch.
  - Arm listing watches with `start_watching_detached`, ❌ never `start_watching` (an inline arm was p90 653 ms of dead
    time), keeping all four rules that made it cheap.
  - Rebase event paths (`rebase_event_path`), or firmlinks (`/tmp`, `/var`, `/etc`) plus a symlinked watch root drop
    every event.
  - A created, removed, or renamed watch ROOT forces a full re-read; ❌ don't add `Modify(Metadata(_))`, since every
    child change bumps the dir's mtime.
  - A row that jumped its sorted position is one `DiffChangeType::Move`, never a remove plus an add: the pane rides the
    cursor and selection along by identity. § "Reordered rows".
- **A watch on an OS-mounted network share is `WatchCoverage::ThisMachineOnly`, never `EveryWriter`** (FSEvents on
  `smbfs` sees this machine's writes only). ❌ Don't refuse to arm it over that; what it must not do is let a delete
  walker or copy scan skip a read. `volume/DETAILS.md` § "Trait capability model".
- **"Open terminal here" asks `NSWorkspace` per known app, ❌ never scans `/Applications`**; recipes are a pure
  `launch_argv`. New terminals need a sourced, dated `KNOWN_TERMINALS` entry. § "Open terminal here".
- **F4's editors come from LaunchServices' editor-ROLE plain-text query** (`text_editor.rs`): ❌ never the UTI crate
  (10.15 floor) or a made-up `.txt` URL (no apps). § "Text editor".
- **`cloud_actions.rs` is iCloud Drive only**, gated by `CloudProvider::supports_eviction`; the cross-provider-looking
  `NSFileProviderManager` methods need the bundled extension. Don't widen it. Provider identity lives once, in
  `cloud_provider.rs`, which the volume switcher reads too.
- **Google Drive (`google_drive/`), three ❌:** never gate on a path prefix (mirror mode keeps real files outside
  `~/Library/CloudStorage` with no xattr, so we gate on a resolved item ID); never resolve a mirrored file by NAME;
  never read a mirror `stable_id` from the stream-mode `metadata_sqlite_db` (different id space). `mirror_db.rs` walks
  up to a root by inode, down by indexed `(parent, name)`, then proves the row with the file's inode. Read-only, and
  every failure means no menu item: a wrong link points at someone else's file. § "Mirror mode".

Open-with internals, cloud-actions rationale, and the full threading/watcher story: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.

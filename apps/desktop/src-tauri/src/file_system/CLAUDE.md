# File system module

Directory listing, file writing, sync status, volume management, and file watching.

## Module map

- Submodules: `listing/`, `write_operations/`, `volume/`, `sync_status/` (cloud badges) — each has its own `CLAUDE.md`.
- `watcher.rs` (FSEvents incremental listing updates), `staging.rs` (the two scratch-visibility settings and the
  predicate listings filter on; the `StagingTemp` mint itself is `cmdr_fs::staging`, re-exported here),
  `index_provider.rs` (the app's `VolumeProvider`, so the index never imports `VolumeManager`).
- `backend_settings.rs` (the live per-backend knobs a storage backend reads, resolved through a table keyed by settings
  namespace), `cloud_actions.rs` (iCloud offline/remove-download), `open_with.rs` (candidate apps + launch), `tags.rs` (macOS
  Finder tags via `_kMDItemUserTags`).
- `mod.rs` is a facade: it re-exports downward and bootstraps the volume registry (`init_volume_manager`), which is why
  it may know every backend.

## Gotchas

- **The volume-manager singleton is `volume::manager::get_volume_manager()`, and ❌ never re-exported from here.** A
  facade that both re-exports downward and hands out the accessor everything reaches for welds the whole subtree into
  one cycle, and a per-backend crate can't import a facade at all. `volume/DETAILS.md` § "Key decisions".

- **Transient scratch hides on the listing READ path, never in a watcher** (`staging.rs`): a watcher-side skip strands
  an entry in the pane forever. ❌ Filter nowhere but `listing/operations.rs::visible_entries`. Cmdr's own
  (`.cmdr-tmp-*`, minted via `StagingTemp`) hides by OWNERSHIP so a wedge's leftovers stay visible; other apps' (`.sb-`)
  hides by NAME, having no ownership signal. `DETAILS.md` § "Hiding transient scratch".
- **Tag writes (`tags.rs`) touch ONLY `_kMDItemUserTags`, never `com.apple.FinderInfo`.** That blob carries
  `kHasCustomIcon`; zeroing it destroys custom folder icons. Encode the **binary** plist (`to_writer_binary` — `plist`
  defaults to XML). Pinned by `tags::write_tests::tagging_preserves_finder_info_custom_icon_flag`.
- **Never use rayon (or any constrained-stack pool) for calls into macOS frameworks.** NSURL/FileProvider XPC
  round-trips blow rayon's 2 MB worker stack and can block forever. Use pooled, hard-capped OS threads with 8 MB stacks
  (`sync_status/pool.rs` is the reference); a per-call `std::thread::scope` is NOT enough. `DETAILS.md` § "Threading".
- **Never `tokio::spawn` from a watcher OS thread** (notify-rs, git, SMB, MTP, archive): no reactor is running, so it
  panics. Use `tauri::async_runtime::spawn`; FullRefresh dispatch funnels through `caching::spawn_full_refresh` for
  exactly this reason. `DETAILS.md` § "Watcher threading".
- **Watcher event paths must be rebased into the listing's path space** (`watcher.rs::rebase_event_path`). Raw
  `path.parent() == dir_path` drops every event under `/tmp`, `/var`, `/etc` (firmlinks) and under a symlinked watch
  root (Google Drive's `My Drive`). `DETAILS.md` § "Watcher path rebasing".
- **A watch root that is itself created, removed, or renamed forces a full re-read** (`watcher.rs`), because macOS
  reports a wholesale replacement (`git checkout`, `rsync --delete`, a build regenerating an output dir) as
  Remove+Create on the ROOT plus one Create per NEW child, and never a remove for the old ones. ❌ Don't add
  `Modify(Metadata(_))` to that trigger: every ordinary child change bumps the dir's mtime, so it would re-read on
  everything. `DETAILS.md` § "Replacing a watch root".
- **A watch on an OS-mounted network share is `WatchCoverage::ThisMachineOnly`, never `EveryWriter`** (`watcher.rs`
  decides it once at arm time, so the oracle stays a pure in-memory read). FSEvents on `smbfs` is a local-VFS notifier:
  it delivers this machine's writes through the mount and NOTHING another client does to the share. The pane still
  updates from the user's own work, so ❌ don't "fix" it by refusing to arm the watch; what it must not do is let a
  delete walker or copy scan skip a read. `volume/DETAILS.md` § "Trait capability model".
- **`cloud_actions.rs` is iCloud Drive only**, gated by `is_in_icloud_drive`. The cross-provider-looking
  `NSFileProviderManager` methods are reserved for the app bundling the extension. Don't widen it.

Open-with internals, cloud-actions rationale, and the full threading/watcher story: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.

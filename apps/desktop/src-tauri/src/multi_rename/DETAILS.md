# Multi-Rename Tool: the desktop host, details

The engine, the TC semantics, and the host seam are `crates/cmdr-multi-rename/DETAILS.md`. This is what the app answers
that seam with, where it keeps the feature's files, and what its IPC commands add around the engine.

## The app's engine (`app_host.rs`)

- `app_multi_rename()` is a `LazyLock<MultiRename>` built over `crate::listing_host::listings()` and
  `tauri::async_runtime::handle()`. **Decision/Why: a `LazyLock`, not a slot `setup()` fills** (the
  `file_viewer/app_host.rs` pattern): the app's unit tests install nothing and still need an engine, and a test's own
  runtime would die with its test while the static outlives it. First touched by an IPC command or the MCP tool, so
  after `setup()` installed the listing engine.
- **Tauri's runtime is the one the IPC commands' blocking work always ran on**, so moving the engine into a crate moved
  no work between pools.
- `max_sessions`: `MAX_SESSIONS` (4) in production, 1,024 under `cfg!(test)`. **Why here**: the app's tests share this
  engine and run in parallel, and a `cfg!(test)` inside the crate can't see the app's test build. Eviction has its own
  tests in the crate.

## Host answers (`impl MultiRenameHost for AppHost`)

- **Media reads** (`media_loader` → `volume_loader`): `MAX_PARALLEL_READS` at once, each through `read_media_from` over
  a `VolumeByteSource` on the engine's runtime (the volume resolved per file from the volume manager, so a reconnect's
  new volume is the one read), under one file's budget, `io_budget_for_volume(10 s)`. `volume_gone` answers true once
  the volume is unregistered or its connection isn't live, so a dropped share gives up at once. The MCP tool builds
  its own with `volume_loader(.., Handle::current())` and a no-op emit.
- **Results**: `results_supported` is `true`; `results_path` (`app_files.rs`) is
  `<app cache dir>/multi-rename/<pid>-<session>.txt`, `None` without a cache dir (the sheet then words
  `CouldntWriteNames`). The cache dir is per-user and per-instance, and "Delete all Cmdr data" removes it whole.
- **`rename_place`**: ONE `get_volume_manager().resolve(volume_id, dir)`. Its `routed` folder answers `ReadOnly`; else
  each ready row's old and new path through `row_route` (`portal_serves`, then `path_routes_over_its_parent`: the git
  portal first, so a `.zip` inside a snapshot is the portal's); then `volume_id == "root"` is `LocalDisk`; then the
  resolved volume, or `NotConnected`. `run_test.rs` pins the order (a portal row on a volume nothing serves answers
  `ReadOnly` before `NotConnected`).
- **`io_budget`** forwards `io_budget_for_volume`; **`operations`** clones the installed write engine's handle and maps
  "not installed" to `CouldntStart`.
- **The event**: `MultiRenameMediaProgress` (the `tauri_specta::Event`, `multi-rename-media-progress`) is built `From`
  the crate's `MediaProgressEvent`; `wire_test.rs` pins that the two serialize the same, since the demo sends the
  crate's payload as the event body.

## IPC (`commands/multi_rename.rs`)

- Every command wraps an engine method in the app's deadlines: `blocking_*` / `timeout_detached_*`, and the preview
  runs off the IPC thread with a 5 s deadline, two at a time (`BlockingBudget` `PREVIEWS`), since the sheet re-issues
  it on every edit. The overview, field samples, and prioritizing run on blocking threads.
- `open_multi_rename` starts the media loader with an emitter over its `AppHandle`.
- `apply_multi_rename` adds `history_entries` to `FIELD_HISTORY` once the rename has started.

## Files (`app_files.rs`, `preset_store.rs`)

- **Presets**: `multi-rename-presets.json`, one `PresetStore` (`preset_store::PRESETS`); `presets.rs` in the crate is
  the pure list logic, so the store depends on it and not the other way around. `PresetStore::edit` runs each list op
  as one locked step with one durable temp+rename write. The file keeps the `crate::recents` envelope
  (`_schemaVersion: 1`, `entries`) plus `seeded`, so an older build's file reads as stored (newest first) and an older
  build reads ours. A file that won't read goes through `state_recovery::read_state_file` (set aside, the person told
  once); the lock discipline is `recents`' (two locks, never held at once).
  - **Seeding**: no file (a new install) is unseeded; so is a file from before seeding (no `seeded` field), which is
    how an existing user gets the seeds; a file set aside as unreadable counts as seeded (the person's list is in the
    copy). **Known limit:** an older build rewrites the file without `seeded`, so after a downgrade-then-upgrade the
    seeds are offered again: a deleted one returns, and a renamed one comes back beside the renamed copy.
- **Last settings**: `LAST_SPEC`, a one-entry `RecentsFile<LastSpec>` in `multi-rename-last.json`.
- **Field history**: `FIELD_HISTORY`, a `RecentsFile<FieldHistoryEntry>` in `multi-rename-history.json`, capped at
  `MAX_FIELD_HISTORY` (200). Seeded through `RecentsFile::load_seeded` only when the file is missing; the seed reaches
  disk with the first rename, so an emptied or set-aside history never gets it back.
- **`RecentEntry` impls live here** (they name the app's file and its `state_recovery` key; the orphan rule allows it
  because the trait is the app's).

## Tests

- `session_test.rs`, `run_test.rs`, `results_test.rs`: sessions, apply, and Results over a real `TestDir` through the
  listing host and `app_multi_rename` (`run_test.rs`'s helpers are shared).
- `media_test.rs` (a stand-in reader: order, throttled progress, freshness, cancel on close, a panicking reader, a dead
  volume, a reader answering `NotSupported` for every file, samples, `StillReading` versus `PreviewOutOfDate`) and
  `media_real_test.rs` (real files through the real read path).
- `wire_test.rs`: the serde JSON of every wire shape the demo mirrors.
- `preset_store_test.rs`, `app_files_test.rs`: the files.

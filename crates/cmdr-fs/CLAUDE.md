# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, as a workspace crate with **no `tauri` in
its dependency tree**. It exists so the index subsystems can reach `Volume` and `FileEntry` without reaching the app.

The app re-exports every item here from its original path, so `crate::file_system::volume::VolumeError`,
`crate::pluralize`, `crate::icons::special_folders`, and the rest all still resolve. Prefer the original path when
editing app code; use `cmdr_fs::…` from another crate.

## Module map

- `volume/`: the `Volume` trait plus `VolumeReadStream` / `SequentialExtract`, the data types it exchanges (`types.rs`),
  the volume ID helpers (`ids.rs`), `InMemoryVolume`, `friendly_error/` (typed, word-free error classification — see its
  own `CLAUDE.md`), and `host/` — everything a backend needs from the app around it, as named traits. Read
  `crates/cmdr-fs/src/volume/host/CLAUDE.md` before writing a new backend or moving an existing one out of the app.
- `entry.rs`: `FileEntry`, `TagRef`, `get_icon_id`, and the uid/gid → name caches. What every listing yields.
- `icons/`: the two per-entry icon classifiers `get_icon_id` calls — `special_folders` (a `HashMap` lookup) and
  `packages` (a suffix test). Nothing that touches the disk.
- `archive_format.rs`: name → `ArchiveFormat`, the single source of truth for archive detection.
- `filesystem_kind.rs`: `FilesystemKind` / `MaxFileSize` / `FilesystemInfo` (classification only).
- `firmlinks.rs`: `normalize_path`, the macOS firmlink canonicalization (`/System/Volumes/Data/x` ⇒ `/x`). Pure path
  work with no host or index behind it, and the index and the app's watchers have to agree on it.
- `sqlite_util.rs`: the ONE process-wide SQLite page-cache slab, the connection factories that install it, the
  role-split per-connection budgets (`apply_page_cache`, `apply_statement_cache`), the
  per-connection budgets, the per-thread read-connection cache, and freelist reclamation. Shared by all five stores
  (three index DBs, the agent's, the operation log's), which is why it can't live in either end.
- `staging.rs`: `StagingTemp`, the ONLY way to name a scratch file, plus the markers every one carries and the in-flight
  registry that says whether a live operation still owns one. Whether the user SEES one is the app's
  (`file_system::staging`).
- `tcc_paths.rs`, `ignore_poison.rs`, `pluralize.rs`, `thread_qos.rs`, `process_memory.rs`, `testing.rs`.

## Must-knows

- **`#![deny(missing_docs)]` holds here.** A new `pub` item, field, or enum variant needs a doc comment. Several of
  these types cross IPC through `specta::Type`, so the comment lands in `bindings.ts` too.
- **`specta` is pinned to `=2.0.0-rc.24`, identical to the app's.** Two `specta` crates in one graph and these `Type`
  impls stop satisfying `tauri-specta`, which breaks bindings generation.
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy; the local-FS behavior lives app-side in
  `file_system::listing::mutation::patch_listing_after_local_mutation`. `DETAILS.md` § "What the app kept".
- **❌ Never gate BEHAVIOR on `cfg(test)` in this crate; use `any(test, feature = "testing")`.** `cfg(test)` is set only
  while compiling a crate's own test target, so in a consumer's test build the arm silently flips and production
  behavior runs inside their suite. `thread_qos`'s no-op was `not(test)` and started applying the real Utility QoS to
  the app's background threads, starving a walker test past its watchdog. It compiles clean and surfaces as someone
  else's flake. `DETAILS.md` § "Gotcha: `cfg(test)`-conditioned BEHAVIOR".
- **Turn the `testing` feature on through a dev-dependency, never a normal one.** That's what keeps it out of shipped
  builds. It gates `testing::TestDir`, `wait_until` / `wait_until_async`, and the QoS no-op together.
- **Nothing here produces user-facing prose.** Errors carry typed reasons and structured params; the frontend renders
  every word. `pluralize` and `FileEntry`'s `display_size` are the named exceptions — see `DETAILS.md`.

Composition rationale (why each item is here, what was cut to make it fit, and what deliberately stayed in the app):
`DETAILS.md`. Read it before moving anything else down or pushing anything back up.

# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, as a workspace crate with **no `tauri` in
its dependency tree**. It exists so the index subsystems can reach `Volume` and `FileEntry` without reaching the app.

The app re-exports every item here from its original path, so `crate::file_system::volume::VolumeError`,
`crate::pluralize`, `crate::icons::special_folders`, and the rest all still resolve. Prefer the original path when
editing app code; use `cmdr_fs::…` from another crate.

## Module map

- `volume/`: the `Volume` trait plus `VolumeReadStream` / `SequentialExtract`, the data types it exchanges (`types.rs`),
  the volume ID helpers (`ids.rs`), `InMemoryVolume`, and `friendly_error/` (typed, word-free error classification — see
  its own `CLAUDE.md`).
- `entry.rs`: `FileEntry`, `TagRef`, `get_icon_id`, and the uid/gid → name caches. What every listing yields.
- `icons/`: the two per-entry icon classifiers `get_icon_id` calls — `special_folders` (a `HashMap` lookup) and
  `packages` (a suffix test). Nothing that touches the disk.
- `archive_format.rs`: name → `ArchiveFormat`, the single source of truth for archive detection.
- `filesystem_kind.rs`: `FilesystemKind` / `MaxFileSize` / `FilesystemInfo` (classification only).
- `tcc_paths.rs`, `ignore_poison.rs`, `pluralize.rs`, `thread_qos.rs`, `process_memory.rs`, `testing.rs`.

## Must-knows

- **`#![deny(missing_docs)]` holds here.** A new `pub` item, field, or enum variant needs a doc comment. Several of
  these types cross IPC through `specta::Type`, so the comment lands in `bindings.ts` too.
- **`specta` is pinned to `=2.0.0-rc.24`, identical to the app's.** Two `specta` crates in one graph and these `Type`
  impls stop satisfying `tauri-specta`, which breaks bindings generation.
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy; the local-FS behavior lives app-side in
  `file_system::listing::caching::patch_listing_after_local_mutation`. `DETAILS.md` § "What the app kept".
- **`thread_qos` no-ops under `cfg(test)` OR the `testing` feature, and the feature half is load-bearing.** `cfg(test)`
  isn't set when a consumer compiles this crate, so without the feature the real Utility QoS would apply inside every
  consumer's parallel test run and starve slow tests past their timeouts. It did.
- **Turn the `testing` feature on through a dev-dependency, never a normal one.** That's what keeps it out of shipped
  builds. It gates `testing::wait_until` / `wait_until_async` and the QoS no-op together.
- **Nothing here produces user-facing prose.** Errors carry typed reasons and structured params; the frontend renders
  every word. `pluralize` and `FileEntry`'s `display_size` are the named exceptions — see `DETAILS.md`.

Composition rationale (why each item is here, what was cut to make it fit, and what deliberately stayed in the app):
`DETAILS.md`. Read it before moving anything else down or pushing anything back up.

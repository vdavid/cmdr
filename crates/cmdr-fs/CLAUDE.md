# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, with **no `tauri` in its dependency tree**,
so the index subsystems reach `Volume` and `FileEntry` without reaching the app. The app re-exports every item from its
original path (`crate::file_system::volume::VolumeError`, `crate::pluralize`, …); prefer that path in app code,
`cmdr_fs::…` from another crate.

## Module map

- `volume/`: the `Volume` trait and the types it exchanges, `InMemoryVolume`, `friendly_error/` (typed, word-free
  classification), and `host/` — what a backend needs from the app, as named traits. Read `src/volume/host/CLAUDE.md`
  before writing a backend or moving one out of the app.
- `entry.rs` + `icons/`: `FileEntry` and the two disk-free classifiers `get_icon_id` calls. What every listing yields.
- `sqlite_util.rs`: the ONE process-wide page-cache slab plus the connection factories all five stores open through.
- `staging.rs`: `StagingTemp`, the ONLY way to name a scratch file. Whether the user SEES one is app-side
  (`file_system::staging`).
- Leaves: `archive_format.rs` (sole source of truth for archive detection), `filesystem_kind.rs` (classification only),
  `firmlinks.rs` (`normalize_path`; the index and the app's watchers have to agree on it), `log_rollup.rs`,
  `tcc_paths.rs`, `ignore_poison.rs`, `pluralize.rs`, `thread_qos.rs`, `thread_cpu.rs`, `process_memory.rs`,
  `testing.rs`.

## Must-knows

- **`#![deny(missing_docs)]` holds here.** New `pub` items, fields, and variants need doc comments; several cross IPC
  via `specta::Type`, so the comment lands in `bindings.ts` too.
- **`specta` stays pinned to `=2.0.0-rc.24`, identical to the app's.** Two copies in one graph break bindings
  generation.
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy. `DETAILS.md` § "What the app kept".
- **❌ Never gate BEHAVIOR on `cfg(test)` here; use `any(test, feature = "testing")`.** `cfg(test)` is off in a
  consumer's test build, so the arm flips and production behavior runs inside their suite. It compiles clean and
  surfaces as someone else's flake. `DETAILS.md` § "Gotcha: `cfg(test)`-conditioned BEHAVIOR".
- **Turn the `testing` feature on through a dev-dependency, never a normal one.** That's what keeps it out of shipped
  builds.
- **`InMemoryVolume` honors the `Volume` contracts data safety LEANS on**, not just the happy path: `delete` refuses a
  non-empty directory, `rename` of a directory carries its subtree. ❌ Never relax a contract to make a test green; the
  double is the oracle. It also LIES on request (`set_stat_failing`, `set_reported_type`, `set_reported_size`,
  `set_modified_at`, `with_delete_failing`, …), so a defense against a hostile backend is testable rather than assumed.
  `DETAILS.md` § "`InMemoryVolume` honors the contracts" and § "The faults `InMemoryVolume` can be told to have".
- **`volume::conformance` holds the promises a backend can't quietly opt out of**, one assertion each, and EVERY
  backend's suite calls the ones it can run (a new backend adds its own calls): `delete` never recurses (one file, or
  one EMPTY directory — the same-volume move keeps a Skipped child's only copy purely by letting the parent's delete
  fail); `rename(force = false)` refuses an existing destination and touches neither node; `create_file` refuses rather
  than truncates; `create_directory_all` reports a pre-existing leaf as `AlreadyExisted`, never `Created`. Each backend
  earns each one by a DIFFERENT mechanism, which is why the promise is asserted rather than assumed. `DETAILS.md` §
  "`InMemoryVolume` honors the contracts".
- **Nothing here produces user-facing prose.** Errors carry typed reasons and structured params; the frontend renders
  every word. `pluralize` and `display_size` are the named exceptions.

Composition rationale, the four cuts that made the closure finite, and what deliberately stayed in the app:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.

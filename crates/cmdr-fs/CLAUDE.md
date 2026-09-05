# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, with **no `tauri` in its dependency tree**,
so the index reaches `Volume` and `FileEntry` without reaching the app. The app re-exports every item at its original
path; prefer that in app code (`crate::file_system::volume::VolumeError`), `cmdr_fs::…` from another crate.

## Module map

- `volume/`: the trait, its types, `InMemoryVolume`, `ids` + `canonical_root` (the ID funnel and double-mount collapse),
  `retirement.rs` (how background work learns it stopped being the live volume), `channel_stream.rs` (the consumer half
  of a network backend's read path), `scan_boundary.rs` + `scan_stop.rs` (the one seam a copy scan touches per entry: it
  reports counts AND answers Cancel and Pause), the four modules a stat-and-listing backend gets its `Volume` bodies
  from (`scan_walk.rs`, `mkdir_all.rs`, `patching.rs`, `secret_store.rs`), `friendly_error/` (typed, word-free
  classification), `usb_speed.rs` (❗ its doc comment reaches `bindings.ts`), and `host/` (what a backend needs from the
  app, as named traits; read `src/volume/host/CLAUDE.md` before writing a backend).
- `entry.rs` + `icons/` (`FileEntry` and the classifiers behind `get_icon_id`), `sqlite_util.rs` (the ONE process-wide
  page-cache slab and the factories all five stores open through), `staging.rs` (`StagingTemp`, the ONLY way to name a
  scratch file).
- Leaves: `archive_format.rs` (sole source of truth for archive detection), `firmlinks.rs` (`normalize_path`; the index
  and the app's watchers have to agree on it), `file_provider.rs` (the cloud-domain marker), `filesystem_kind.rs`,
  `log_rollup`, `tcc_paths`, `ignore_poison`, `pluralize`, `git_meta` (what a git portal row's Size cell states),
  `thread_qos`, `thread_cpu`, `process_memory`, `testing`.

## Must-knows

- **`#![deny(missing_docs)]` holds here**: new `pub` items, fields, and variants need doc comments, and several cross
  IPC via `specta::Type`, so the comment lands in `bindings.ts`.
- **`specta` stays pinned to `=2.0.0-rc.24`, identical to the app's**: two copies in one graph break bindings
  generation.
- **`Volume::capabilities()` is a PURE FOLD of the trait's predicates, published over IPC.** ❌ Never override it: grow
  the surface by adding a predicate (`src/volume/capabilities.rs`).
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy. `DETAILS.md` § "What the app kept".
- **❌ Never gate BEHAVIOR on `cfg(test)` here; use `any(test, feature = "testing")`, switched on through a
  dev-dependency.** `cfg(test)` is off in a consumer's test build, so the arm flips and production behavior runs inside
  their suite: it compiles clean and surfaces as someone else's flake. `DETAILS.md` § "Gotcha: `cfg(test)`-conditioned
  BEHAVIOR".
- **`InMemoryVolume` honors the `Volume` contracts data safety LEANS on**, and LIES on request (`set_stat_failing`,
  `with_delete_failing`, …) so a defense against a hostile backend is testable. ❌ Never relax a contract to make a test
  green: the double is the oracle. Cross-backend promises live in `volume::conformance`, which every backend's suite
  calls, since each earns them differently.
- **❌ Never build a volume ID by hand, or by stripping characters.** `volume::ids` is the one funnel; an ID keys the
  index DB, `lastUsedPaths`, tab state, and routing, so a lossy one hands two disks one identity and sends deletes to
  the wrong disk. Add a constructor there.
- **Nothing here produces user-facing prose**: errors carry typed reasons and structured params, the frontend renders
  every word, and `git_meta.rs` shows the shape for a whole column (`FileEntry.git_meta` states a FACT, ❌ never a
  sentence). `pluralize` is the exception; its callers are all logs.
- **A stat-and-listing backend implements three small traits, ❌ never its own copy of the walk**: `ScanSource`,
  `MakesDirectories`, `PatchSource`. `secret_store.rs` is the only place a backend touches the credential store.
  `DETAILS.md` § "Bodies a backend gets for free".

Composition rationale, the four cuts that made the closure finite, and what deliberately stayed in the app:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.

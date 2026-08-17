# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, with **no `tauri` in its dependency tree**,
so the index subsystems reach `Volume` and `FileEntry` without reaching the app. The app re-exports every item from its
original path (`crate::file_system::volume::VolumeError`, …); prefer that path in app code, `cmdr_fs::…` from another
crate.

## Module map

- `volume/`: the `Volume` trait, its types, `InMemoryVolume`, `ids` + `canonical_root` (the ID funnel, plus double-mount
  collapse), `friendly_error/` (typed, word-free classification), and `host/` — what a backend needs from the app, as
  named traits. Read `src/volume/host/CLAUDE.md` before writing a backend or moving one out of the app.
- `entry.rs` + `icons/`: `FileEntry` and the two disk-free classifiers `get_icon_id` calls. What every listing yields.
- `sqlite_util.rs`: the ONE process-wide page-cache slab plus the connection factories all five stores open through.
- `staging.rs`: `StagingTemp`, the ONLY way to name a scratch file. Whether the user SEES one is app-side
  (`file_system::staging`).
- Leaves: `archive_format.rs` (sole source of truth for archive detection), `filesystem_kind.rs` (classification only),
  `firmlinks.rs` (`normalize_path`; the index and the app's watchers have to agree on it), plus `log_rollup`,
  `tcc_paths`, `ignore_poison`, `pluralize`, `thread_qos`, `thread_cpu`, `process_memory`, `testing`.

## Must-knows

- **`#![deny(missing_docs)]` holds here.** New `pub` items, fields, and variants need doc comments; several cross IPC
  via `specta::Type`, so the comment lands in `bindings.ts` too.
- **`specta` stays pinned to `=2.0.0-rc.24`, identical to the app's.** Two copies in one graph break bindings
  generation.
- **`Volume::capabilities()` is a PURE FOLD of the trait's predicates, published over IPC.** ❌ Never override it: grow
  the surface by adding a predicate. What ships vs. what stays a predicate: `src/volume/capabilities.rs`.
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy. `DETAILS.md` § "What the app kept".
- **❌ Never gate BEHAVIOR on `cfg(test)` here; use `any(test, feature = "testing")`.** `cfg(test)` is off in a
  consumer's test build, so the arm flips and production behavior runs inside their suite: compiles clean, surfaces as
  someone else's flake. `DETAILS.md` § "Gotcha: `cfg(test)`-conditioned BEHAVIOR". Turn `testing` on through a
  dev-dependency, never a normal one; that's what keeps it out of shipped builds.
- **`InMemoryVolume` honors the `Volume` contracts data safety LEANS on**, not just the happy path. ❌ Never relax one
  to make a test green; the double is the oracle. It also LIES on request (`set_stat_failing`, `with_delete_failing`,
  …), so a defense against a hostile backend is testable rather than assumed. `DETAILS.md` § "The faults
  `InMemoryVolume` can be told to have".
- **`volume::conformance` holds the promises a backend can't quietly opt out of** (`delete` never recurses,
  `rename(force = false)` refuses, `create_file` won't truncate, `create_directory_all` reports an existing leaf
  honestly, `is_writable()` matches the mutations offered), and EVERY backend's suite calls the ones it can run. Each
  earns each one by a DIFFERENT mechanism, which is why it's asserted rather than assumed. Why each matters:
  `DETAILS.md` § "`InMemoryVolume` honors the contracts".
- **❌ Never build a volume ID by hand, or by stripping characters.** `volume::ids` is the one funnel; an ID keys the
  index DB, `lastUsedPaths`, tab state, and routing, so a lossy one hands two disks one identity and sends reads (and
  deletes) to the wrong disk. Add a constructor there.
- **Nothing here produces user-facing prose.** Errors carry typed reasons and structured params; the frontend renders
  every word. `pluralize` and `display_size` are the named exceptions.

Composition rationale, the four cuts that made the closure finite, and what deliberately stayed in the app:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.

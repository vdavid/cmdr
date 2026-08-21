# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, with **no `tauri` in its dependency tree**,
so the index subsystems reach `Volume` and `FileEntry` without reaching the app. The app re-exports every item at its
original path; prefer that in app code (`crate::file_system::volume::VolumeError`), `cmdr_fs::…` from another crate.

## Module map

- `volume/`: the trait, its types, `InMemoryVolume`, `ids` + `canonical_root` (the ID funnel and double-mount collapse),
  `retirement.rs` (how background work learns it stopped being the live volume), `friendly_error/` (typed, word-free
  classification), and `host/` (what a backend needs from the app, as named traits; read `src/volume/host/CLAUDE.md`
  before writing a backend).
- `entry.rs` + `icons/` (`FileEntry` and the classifiers behind `get_icon_id`), `sqlite_util.rs` (the ONE process-wide
  page-cache slab and the connection factories all five stores open through), `staging.rs` (`StagingTemp`, the ONLY way
  to name a scratch file).
- Leaves: `archive_format.rs` (sole source of truth for archive detection), `firmlinks.rs` (`normalize_path`; the index
  and the app's watchers have to agree on it), `file_provider.rs` (the cloud-domain marker), `filesystem_kind.rs`,
  `log_rollup`, `tcc_paths`, `ignore_poison`, `pluralize`, `thread_qos`, `thread_cpu`, `process_memory`, `testing`.

## Must-knows

- **`#![deny(missing_docs)]` holds here**: new `pub` items, fields, and variants need doc comments, and several cross
  IPC via `specta::Type`, so the comment lands in `bindings.ts` too.
- **`specta` stays pinned to `=2.0.0-rc.24`, identical to the app's**: two copies in one graph break bindings
  generation.
- **`Volume::capabilities()` is a PURE FOLD of the trait's predicates, published over IPC.** ❌ Never override it: grow
  the surface by adding a predicate (`src/volume/capabilities.rs`).
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy. `DETAILS.md` § "What the app kept".
- **❌ Never gate BEHAVIOR on `cfg(test)` here; use `any(test, feature = "testing")`, switched on through a
  dev-dependency.** `cfg(test)` is off in a consumer's test build, so the arm flips and production behavior runs inside
  their suite: compiles clean, surfaces as someone else's flake. `DETAILS.md` § "Gotcha: `cfg(test)`-conditioned
  BEHAVIOR".
- **`InMemoryVolume` honors the `Volume` contracts data safety LEANS on**, and it LIES on request (`set_stat_failing`,
  `with_delete_failing`, …) so a defense against a hostile backend is testable. ❌ Never relax a contract to make a test
  green: the double is the oracle. The cross-backend promises live as shared assertions in `volume::conformance` that
  EVERY backend's suite calls, since each earns them by a DIFFERENT mechanism.
- **❌ Never build a volume ID by hand, or by stripping characters.** `volume::ids` is the one funnel; an ID keys the
  index DB, `lastUsedPaths`, tab state, and routing, so a lossy one hands two disks one identity and sends reads (and
  deletes) to the wrong disk. Add a constructor there.
- **Nothing here produces user-facing prose**: errors carry typed reasons and structured params, and the frontend
  renders every word (`pluralize` and `display_size` excepted).

Composition rationale, the four cuts that made the closure finite, and what deliberately stayed in the app:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.

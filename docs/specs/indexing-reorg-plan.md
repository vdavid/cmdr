# Indexing subsystem reorganization plan

Status: DESIGN (build-free). Awaiting the lead's benchmark-window go-ahead before any file move or `cargo`/`pnpm check`
compile (a load-sensitive benchmark shares this machine).

## Goal and hard constraint

`apps/desktop/src-tauri/src/indexing/` grew into its own subsystem but is half-modularized: the well-defined stages have
subdirs (`scanner/`, `reconciler/`, `writer/`, `aggregator/`, `store/`, `local_reconcile/`, `volume_scanner/`,
`event_loop/`, `state/`, `churn_monitor/`), while ~45 loose top-level `.rs` files form a flat tail that mixes
transports, pipeline stages, plumbing, and tests. End state: a clean, self-evident module tree a new agent can navigate,
with colocated `CLAUDE.md` + `DETAILS.md` docs on the right boundaries and a thin top-level hub.

**This is a PURE reorganization.** Move and regroup code; do NOT change behavior, logic, or public semantics. No "while
I'm here" rewrites. Every test that passes before passes after, unchanged in intent. A real bug found along the way is
noted for the lead, not fixed inside the reorg (it would hide behind the move noise).

## Organizing principle: pipeline stage, not transport

The subsystem has two cross-cutting axes: transport (local-disk, local-external, SMB, MTP) and pipeline stage (scan,
watch, reconcile, write, aggregate, read, freshness). The EXISTING subdirs are already stage-based and good (`scanner`,
`writer`, `aggregator`, `reconciler`, `event_loop`, `store`). We extend that scheme rather than fight it:
transport-agnostic machinery is organized by stage, and the genuinely transport-specific wiring (per-transport enable +
live watch) collects under one `transports/` area. Organizing top-level-by-transport was rejected: the writer,
aggregator, and store are transport-agnostic, so a transport-first tree would fragment them.

## Target module tree

```
indexing/
  mod.rs                     facade + the public re-export surface (role unchanged)
  metadata.rs                shared metadata-extraction primitive (honest loose leaf; used by scan/reconcile/verify/watch)
  CLAUDE.md / DETAILS.md      THIN hub: what-lives-where routing map + the honest-sizes invariant + links

  lifecycle/                 orchestration: registry, phase machine, coordinator, completion, freshness
    mod.rs
    state.rs  + state/       INDEX_REGISTRY, IndexPhase, IndexVolumeKind, bootstrap (state/ = tests.rs)
    manager.rs               IndexManager coordinator + LOCAL scan dispatch
    network_scan.rs          impl IndexManager: the SMB/MTP Volume-trait scan path (sibling impl block of manager)
    scan_completion.rs       post-scan completion handler
    freshness.rs             Fresh / Stale / Scanning state machine + the seam
    failure.rs               IndexFailureSignal / the Failed state
    lifecycle_bus.rs         neutral lifecycle event bus (single source)
    CLAUDE.md / DETAILS.md

  resources/                 process-wide governance: bounded memory + bounded disk
    mod.rs
    memory_watchdog.rs       the global 16 GB phys_footprint budget
    subsystem_stop.rs        stop-hook registry the watchdog runs alongside stop_all_indexing
    retention.rs             external-index-DB count cap + LRU eviction
    CLAUDE.md / DETAILS.md

  scanner/                   EXISTING local guarded walker (mod.rs, walker/, exclusions.rs, tests.rs). Keep at top level.
    CLAUDE.md / DETAILS.md    NEW pair (its must-knows currently live in the hub)

  network_scanner/           RENAMED from volume_scanner/. The Volume-trait BFS scan shared by SMB + MTP.
    mod.rs (was volume_scanner.rs) + pace_tests.rs + tests.rs
    scan_pace.rs             MOVED IN: how hard a trait scan may hit a share right now (pacing vs navigation)
    system_dirs.rs           MOVED IN: NAS snapshot/system dirs the trait scan must not recurse
    CLAUDE.md / DETAILS.md

  watch/                     local filesystem change detection + processing
    mod.rs
    watcher.rs               drive-level FSEvents (macOS) / inotify (Linux) watcher
    event_loop.rs + event_loop/   live / replay / verification / storm processing
    churn_monitor.rs + churn_monitor/   env-gated per-subtree churn rollup (hooks process_live_batch, which lives here)
    CLAUDE.md / DETAILS.md

  reconcile/                 keep the index matching disk after the initial scan
    mod.rs
    reconciler.rs + reconciler/        event-triggered reconcile (escalation, rescan, throttle, routing)
    local_reconcile.rs + local_reconcile/   full LOCAL rescan-in-place (cost_budget, latency_probe)
    verifier.rs              per-navigation readdir-diff correction
    reconcile_bench.rs       #[cfg(test)] reconcile perf guard (lives with the code it guards)
    reconcile_correctness.rs #[cfg(test)] reconcile regression tests
    CLAUDE.md / DETAILS.md

  writer/                    EXISTING single-writer thread. Keep.
    CLAUDE.md / DETAILS.md    NEW pair

  aggregator/                EXISTING bottom-up dir-stats compute. Keep.
    CLAUDE.md / DETAILS.md    NEW pair

  store/                     EXISTING (already has CLAUDE.md + DETAILS.md — update links only)

  read/                      serve sizes back to the app
    mod.rs
    enrichment.rs            ReadPool + enrich_entries_with_index[_on_volume]
    queries.rs               the IPC read surface (status + dir-stats), no registry mutation
    expected_totals.rs       write-op progress-bar denominators
    pending_sizes.rs         the "size updating" hourglass marked-set
    CLAUDE.md / DETAILS.md

  paths/                     path arithmetic
    mod.rs
    routing.rs               path -> volume resolution + IndexPathSpace + index_read_path
    path_prefix.rs           component-aware absolute-path prefix helpers
    firmlinks.rs             macOS firmlink + /private-symlink normalization
    CLAUDE.md / DETAILS.md

  events/                    the frontend event + scan-progress surface
    mod.rs
    events.rs                Tauri payload structs, ActivityPhase, DebugStats, set_phase_for
    progress_reporter.rs     the 500 ms progress + mid-scan partial-agg tick loop
    partial_agg.rs           pure send-decision + hot-path collection for the loop
    CLAUDE.md / DETAILS.md

  transports/                per-transport enable + live watch (built on network_scanner/ + the local pipeline)
    mod.rs
    smb/   mod.rs, index.rs (was smb_index.rs), watch.rs (was smb_watch.rs),
           integration_test.rs (was smb_scan_integration_test.rs, #[cfg(test)])
    mtp/   mod.rs, index.rs (was mtp_index.rs), watch.rs (was mtp_watch.rs)
    local_external/  mod.rs, index.rs (was local_external_index.rs)
    CLAUDE.md / DETAILS.md

  tests/                     genuinely whole-pipeline integration + stress tests (#[cfg(test)])
    mod.rs
    integration_tests.rs, stress_test_helpers.rs, stress_tests_concurrency.rs,
    stress_tests_lifecycle.rs, stress_tests_partial_aggregation.rs,
    external_drive_fixture.rs (fixture; #[cfg(all(test, target_os = "macos"))] — verify its consumer set on move)
    CLAUDE.md / DETAILS.md    (light C.md; the FSKit-panic guardrail is a real must-know)
```

The two off-by-default observability spikes stay welded to the code they instrument rather than collecting in a thin
`observability/` area: `churn_monitor` hooks `process_live_batch` (in `watch/event_loop/`), so it moves into `watch/`;
the reconcile-latency probe already lives in `reconcile/local_reconcile/` on its `GuardedReader`. A separate area for
one env-gated spike would be over-fragmentation.

Result: two loose files at the top level (`mod.rs` facade, `metadata.rs` shared primitive) plus 13 area dirs and the
existing `store/`. The ~45-file flat tail is gone.

## What moves where, and why (per cluster)

- **lifecycle/** — the coordinator brain. `state.rs` is the registry + phase machine; `manager.rs` + `network_scan.rs`
  are the local and trait scan dispatch (kept together because `network_scan` is a sibling `impl IndexManager` block);
  `scan_completion.rs` finishes a scan; `freshness.rs` is the per-volume Fresh/Stale/Scanning state (lifecycle state, so
  it belongs here, not loose); `failure.rs` is the Failed state; `lifecycle_bus.rs` is the neutral registration/
  dirs-changed bus. One cohesive "how a volume index is born, lives, transitions, and dies."
- **resources/** — the two process-wide caps that are a DIFFERENT concern from per-volume lifecycle: the 16 GB memory
  watchdog, the subsystem stop-hook registry it drives, and the external-DB retention cap. Small, cohesive.
- **scanner/** (local) vs **network_scanner/** (trait). Renaming `volume_scanner/` -> `network_scanner/` removes the
  single most confusing name in the subsystem: today both `scanner` and `volume_scanner` "scan volumes". After the
  rename, `scanner` = local guarded walker, `network_scanner` = SMB/MTP trait BFS, self-evidently. `scan_pace.rs` and
  `system_dirs.rs` are trait-scan-only support, so they move into `network_scanner/`.
- **watch/** — `watcher.rs` (the FSEvents/inotify source) and `event_loop/` (the processing sink) are producer and
  consumer of the same local change stream; grouping them is self-evident. `churn_monitor` moves in here because it
  hooks `process_live_batch`, the live batch that lives in `event_loop/` — it observes exactly this area's work.
- **reconcile/** — the three "make the index match disk again" mechanisms: `reconciler` (event-triggered),
  `local_reconcile` (full local rescan-in-place), `verifier` (per-navigation readdir diff). They already share the
  cost-budget / skip / honest-stale discipline. `reconcile_bench.rs` + `reconcile_correctness.rs` (the reconcile perf
  guard and regression tests) live here with the code they guard, not in `tests/`: the "touches writer + aggregator too"
  criterion is true of nearly every reconcile test and is too fuzzy to separate a file named `reconcile_*` from its
  subject.
- **read/** — the whole read side: `enrichment` (the hot path), `queries` (IPC status + dir-stats), `expected_totals`
  (write-op progress), `pending_sizes` (the hourglass). All read via `ReadPool`, never the lifecycle lock.
- **paths/** — the three path-arithmetic modules: `routing` (path->volume + `IndexPathSpace`), `path_prefix`
  (component-aware prefixes), `firmlinks` (macOS normalization). "Canonical form everywhere" lives here.
- **events/** — `events` (payload structs + phase), `progress_reporter` (the tick loop), `partial_agg` (its pure
  decision half). The frontend-facing event + progress surface.
- **transports/** — the per-transport enable + live-watch pairs. Each transport gets a subdir: `smb/{index,watch}`,
  `mtp/{index,watch}`, `local_external/{index}` (local-external has no bespoke watch; it rides the local `watch/`
  pipeline). `smb/integration_test.rs` (the SMB scan integration test) lives with the SMB transport it exercises. This
  is exactly the lead's "per-transport SMB/MTP/network watch+index" cluster.
- **tests/** — the genuinely whole-pipeline tests: `integration_tests` (scan → aggregate → enrich → watch),
  `stress_tests_*` (concurrency/lifecycle/partial-agg), their shared `stress_test_helpers`, and `external_drive_fixture`
  (the FSKit-panic-safe disk-image fixture). Unit tests stay colocated in each module's `tests.rs` (unchanged); the
  reconcile-specific and SMB-specific tests home with their code (above), so `tests/` holds only what truly spans the
  subsystem.

`metadata.rs` stays a loose top-level leaf: it's a shared primitive used by four different areas (scan, reconcile,
verify, watch), so nesting it under any one of them would create a cross-area dependency that reads worse than an honest
shared leaf. Documented in the hub.

## Import strategy: external call sites should not change

The blast radius includes every importer. Almost all external references go through the `mod.rs` facade re-exports
(`indexing::is_active`, `indexing::IndexVolumeKind`, `indexing::store::…`, `indexing::get_volume_index_status`, …), NOT
deep module paths. The plan keeps the SAME public re-export surface, just pointing at new locations, so external
importers ideally change ZERO lines.

**Why a curated facade is the IDEAL end state here, not the cheap one.** `mod.rs` is deliberately the subsystem's stable
public surface; keeping it stable while the internals reorganize is the textbook clean seam (refactorable internals
behind a curated facade), not a compatibility shortcut. Churning ~20 external call sites to spell out the new deep paths
(`indexing::paths::firmlinks::…`, `indexing::lifecycle::freshness::…`) would do the OPPOSITE of the reorg's goal: it
would leak the new two-level internal structure into every consumer and re-break all of them the next time a module
moves. The facade absorbs structure change so consumers don't have to. To keep this honest (the reviewer's fair worry
that a future agent reads the alias as the module's real home), every module re-export carries a one-line comment naming
the real location, e.g. `// Public API surface; real home is paths/firmlinks.rs.`

- **Function/type re-exports** (`pub use` / `pub(crate) use` in `mod.rs`) already name their source module; repoint them
  (e.g. `pub use state::…` -> `pub use lifecycle::state::…`). The `indexing::` prefix external callers use is unchanged.
- **Module-path re-exports.** Verified against the tree: external sites reference exactly six MOVED submodules by deep
  path (`store::`, `writer::`, `scanner::` are referenced too but don't move). For each moved-and-referenced module, add
  a module re-export in `mod.rs` so the old `indexing::<mod>::` path still resolves:
  - `pub(crate) use paths::{firmlinks, routing};` (firmlinks: 5 external refs, routing: 1)
  - `pub(crate) use read::expected_totals;` (4)
  - `pub(crate) use watch::watcher;` (2)
  - `pub(crate) use lifecycle::{freshness, lifecycle_bus};` (freshness: 6, lifecycle_bus: 12)
  - `store/`, `writer/`, `scanner/` don't move, so their paths are untouched. `manager` is referenced externally only
    through `mod.rs` symbol re-exports, not `indexing::manager::`, so it needs no module re-export (repoint its symbol
    re-exports only).
- **Internal references** (within `indexing/`) DO update to the real new paths (`crate::indexing::state::` ->
  `crate::indexing::lifecycle::state::`, etc.). That's the point of the reorg; we don't lean on compat aliases
  internally. Grep `crate::indexing::` and `super::`/`super::super::` across the crate and fix every reference.

Verification of the "zero external change" claim is part of execution: after the moves,
`rg 'indexing::(state|manager| network_scan|scan_completion|freshness|failure|lifecycle_bus|memory_watchdog|subsystem_stop|retention|volume_scanner| scan_pace|system_dirs|event_loop|watcher|reconciler|local_reconcile|verifier|enrichment|queries|expected_totals| pending_sizes|routing|path_prefix|firmlinks|events|progress_reporter|partial_agg|smb_index|smb_watch|mtp_index| mtp_watch|local_external_index|churn_monitor)::' outside `indexing/`must return only paths that still resolve via a`mod.rs`
re-export; anything else gets updated.

## Doc restructuring: thin hub + spokes

Today: one packed `indexing/CLAUDE.md` (~600 words, ~50 must-know bullets spanning every area) + one 42,803-word
`indexing/DETAILS.md` + the `store/` pair. Target: hub-and-spoke.

- **`indexing/CLAUDE.md` becomes a thin hub**: a one-line "what is this", the routing map (area -> one-line purpose +
  link), the ONE genuinely cross-area invariant (honest sizes = every per-volume rule holds per `volume_id`), the
  `metadata.rs` shared-leaf note, and the closing `DETAILS.md` pointer. Every area-specific must-know moves to that
  area's `CLAUDE.md`.
- **`indexing/DETAILS.md` becomes a thin hub too**: the subsystem-wide architecture map + the data-flow diagram + the
  cross-cutting decisions that don't belong to one area (disposable-cache pattern, the two-axis `IndexVolumeKind`
  capability model), each pointing to the area `DETAILS.md` that owns the depth. The current 42k words are CARVED by
  section into the area `DETAILS.md`s per the § Module structure grouping (which already maps file -> concern).
- **Canonical homes for the cross-cutting narratives (decided up front — this is the make-or-break for single-source).**
  A few load-bearing mechanisms span areas; each gets ONE owner, and every other area (and the hub) points to it by
  path, never restates it:
  - **Honest sizes + the `dir_stats` ledger (four hard rules) + coverage epochs** -> owner `writer/DETAILS.md`. The
    writer owns the `dir_stats` writes, the ledger-unpaid debt (`MarkLedgerUnpaid`/`PayLedgerIfUnpaid`), and delta
    propagation, so the invariants live where the code that must uphold them lives. `aggregator/`, `reconcile/`, and
    `scanner/` reference it. (Its C.md carries the terse guardrails; the mechanism narrative is in its D.md.)
  - **The per-volume registry + phase machine + lock discipline** -> owner `lifecycle/DETAILS.md`.
  - **`IndexPathSpace` + the three-path-spaces discipline + mount-relative strip** -> owner `paths/DETAILS.md`.
  - **The data-flow diagram + the disposable-cache pattern + the capability-axis model** -> stay in the hub
    `indexing/DETAILS.md` (they are genuinely subsystem-wide, not one area's).
  - **The SQLite schema** stays in `store/DETAILS.md` (already there); the honest-sizes epoch columns it documents are
    cross-linked from `writer/DETAILS.md`, not duplicated.
- **Each area gets a `CLAUDE.md` (must-knows only, aim << 600 words) + sibling `DETAILS.md`** (the carved depth).
  Single-source: a mechanism lives in exactly ONE area `DETAILS.md`; the hub and other areas point to it by path.
- **`store/` pair** stays; update any cross-links that move.
- **Wider touchpoints**: `docs/architecture.md` (the subsystem MAP entry for indexing — update the file-level pointers
  it names), `apps/desktop/CLAUDE.md` router (if it names moved paths), and `AGENTS.md` § File structure (it describes
  the desktop layout; check whether the indexing detail changed). Grep all of `docs/` for stale `indexing/<file>.rs`
  paths and fix.
- **Doc-graph checks** stay green. The error-level ones are the ones to babysit: `docs-reachable` (every new area C.md
  must be linked from the hub, and every area D.md reachable by link-walking — one forgotten area link fails CI),
  `docs-dead-links`, and `claude-md-details-sibling` (every area C.md needs a linked D.md sibling). `claude-md-length`
  (≤600 words, warn) is a real pressure point for the big `lifecycle/` C.md (see risk notes). `resident-doc-budget` is
  NOT relevant here: the indexing `CLAUDE.md` is path-scoped (auto-injected on touch), not part of the resident bundle
  (root `CLAUDE.md` + `AGENTS.md` + `.claude/rules/`), so thinning it neither helps nor hurts that check.

The doc rewrite follows the condense-first playbook (`docs/doc-system.md`): verify each must-know against the actual
source while carving (past passes caught real drift), condense wording first, move depth second, describe current state
not history, and strip milestone tags.

## Execution sequence (small, compilable, committed steps)

Each numbered step ends compiling (`cargo check` via `pnpm check --fast` scoped) and is its own commit, so no step is a
giant untested leap. Order is chosen so each cluster's move is independent.

1. **Rename `volume_scanner/` -> `network_scanner/`** and fold in `scan_pace.rs` + `system_dirs.rs`. (Smallest,
   internal-only refs; good warm-up that proves the move+import-fix loop.)
2. **`resources/`**: move `memory_watchdog.rs`, `subsystem_stop.rs`, `retention.rs`.
3. **`paths/`**: move `routing.rs`, `path_prefix.rs`, `firmlinks.rs`; add the `firmlinks` + `routing` module re-exports.
4. **`read/`**: move `enrichment.rs`, `queries.rs`, `expected_totals.rs`, `pending_sizes.rs`; add the `expected_totals`
   module re-export.
5. **`events/`**: move `events.rs`, `progress_reporter.rs`, `partial_agg.rs`. `events/mod.rs` must re-export
   `events/events.rs`'s contents so the facade's `pub use events::*` glob doesn't silently narrow.
6. **`watch/`**: move `watcher.rs`, `event_loop.rs` + `event_loop/`, `churn_monitor.rs` + `churn_monitor/`; add the
   `watcher` module re-export.
7. **`reconcile/`**: move `reconciler.rs` + `reconciler/`, `local_reconcile.rs` + `local_reconcile/`, `verifier.rs`,
   `reconcile_bench.rs`, `reconcile_correctness.rs`.
8. **`transports/`**: move the SMB/MTP/local-external index + watch files into `smb/`, `mtp/`, `local_external/`; move
   `smb_scan_integration_test.rs` -> `transports/smb/integration_test.rs` (fix its `#[path]`).
9. **`lifecycle/`**: move `state.rs` + `state/`, `manager.rs`, `network_scan.rs`, `scan_completion.rs`, `freshness.rs`,
   `failure.rs`, `lifecycle_bus.rs`; add the `freshness` + `lifecycle_bus` module re-exports (`manager` needs none — see
   import strategy); fix `mod.rs`'s `//!` intra-doc link ``[`state`]`` -> the new path (else rustdoc broken-intra-doc
   warns, and `no-ignored-warnings` bites). Done late: highest-fan-in cluster, so fewer churning references per step.
10. **`tests/`**: move `integration_tests.rs`, `stress_test_helpers.rs`, `stress_tests_*.rs`,
    `external_drive_fixture.rs` into `tests/`; convert their `super::` chains to absolute `crate::indexing::…`, preserve
    `#[cfg]`/`#[path]`. Verify `external_drive_fixture`'s consumer set first (a test that stays colocated and imports it
    would need its path updated). Smoke-run 1-2 tests first (`test-infra-smoke-first`) before the full suite.
11. **New docs**: write every area `CLAUDE.md` + `DETAILS.md`, carve the 42k-word D.md to the canonical homes above,
    thin the hub C.md + D.md, update `store/` cross-links.
12. **Wider docs**: `docs/architecture.md` (the indexing MAP entry + file pointers), `apps/desktop/CLAUDE.md` router,
    `AGENTS.md` § File structure; sweep for stale `indexing/<file>.rs` paths across BOTH `docs/**/*.md` AND
    `apps/desktop/src-tauri/src/**/*.md` (colocated `DETAILS.md`s in `media_index/`, `importance/`, `file_system/…`,
    `agent/tools/` cite indexing paths; `agent/tools/DETAILS.md`'s `indexing::queries` goes stale since `queries` has no
    module re-export — fix the doc mention).
13. **CodeGraph** re-sync on the worktree; heed staleness banners.
14. **Strip milestone/plan tags** from touched code + docs
    (`rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b'`).
15. **Full green**: `pnpm check -q --include-slow` at the repo root.

## Risk notes

- **`#[cfg]` attributes on `mod` declarations** must ride along on every move: `mtp_*`, `smb_index`,
  `local_external_index` are `#[cfg(any(target_os = "macos", target_os = "linux"))]`; `external_drive_fixture` is
  `#[cfg(all(test, target_os = "macos"))]`; `smb_scan_integration_test` is `#[cfg(all(test, any(macos, linux)))]` with a
  `#[path]`. Dropping a cfg silently changes what compiles per platform. Check each moved `mod` line.
- **`super::` relative paths in moved files** shift by one level when a file nests deeper (e.g. `reconciler/rescan.rs`
  going from `indexing/reconciler/` to `indexing/reconcile/reconciler/` is the SAME depth relative to its own parent, so
  its `super::` is unaffected — but a top-level file like `verifier.rs` moving into `reconcile/` gains a level, so its
  `super::X` referring to a sibling indexing module becomes `super::super::X` OR, preferably, an absolute
  `crate::indexing::X`). Prefer converting shifted `super::` chains to absolute `crate::indexing::` paths for clarity.
- **Test files and `super::`**: the cross-module tests reference internals heavily. Moving them into `tests/` deepens
  their `super::`. Convert to `crate::indexing::…` absolute paths and smoke-test before the full run.
- **CodeGraph staleness** mid-move: after each move Read moved files directly if the banner flags them; re-sync at the
  end.
- **The `foo.rs` + `foo/` pattern** (module root file beside its children dir) is preserved through moves — Rust
  resolves `reconcile/reconciler.rs` + `reconcile/reconciler/rescan.rs` identically to today's `reconciler.rs` +
  `reconciler/rescan.rs`. No conversion to `mod.rs` is required; keeping the pattern minimizes churn.
- **Behavior-freeze audit**: no function bodies change. The only edits are `mod`/`use` path lines, `mod.rs` re-export
  targets, and file locations. A `git diff --stat` dominated by renames (with tiny per-file `use`-line deltas) is the
  signal we stayed pure; a file with a large content delta is a red flag to re-examine. Caveat: because each move step
  also fixes every crate-wide referrer, a file like `smb_index.rs` gets its imports rewritten in several steps (read ->
  `read/`, then its own move, then `state`/`freshness` -> `lifecycle/`), so the "renames dominate" signal holds in
  AGGREGATE across the branch, not necessarily per-step.
- **The `file-length` allowlist is PATH-KEYED and will need consent (process friction, flagged to David).** 15
  already-allowlisted files live under `indexing/` (`state.rs` 1356, `manager.rs` 1147, `reconciler.rs` 1337 +
  `reconciler/tests.rs` 2062, `verifier.rs` 873, `volume_scanner.rs` 867 + `volume_scanner/tests.rs` 1239,
  `integration_tests.rs` 1111, `stress_tests_concurrency.rs` 1301, `local_reconcile/tests.rs` 817, `writer/mod.rs` 1321
  - `writer/entries/tests.rs` 2138, `aggregator/mod.rs` 810, `scanner/tests.rs` 940, `store/tests.rs` 1800). A pure move
    keeps every line count identical but changes the path key, so the local shrink-wrap drops the old entries and a
    fresh `file-length` WARN appears at each new path. Re-homing those entries at unchanged counts is technically
    "adding a new entry," which the `file-length-allowlist` rule gates on David's consent. It is warn-only (never blocks
    green), but per the rule we surface it rather than silently re-add. Ask for one blanket OK to MIGRATE the 15
    existing entries to their new paths at unchanged numbers (no number raised, no new file allowlisted). This is the
    most likely thing to stall the final green if not settled up front.
- **`lifecycle/` C.md word pressure (the one area doc to watch).** `lifecycle/` is the largest cluster (state + state/ +
  manager + network_scan + scan_completion + freshness + failure + lifecycle_bus, ~205 KB). Its must-knows (registry
  authority, lock-first start, drop-guard-before-drain, typed-kind rescan routing, freshness transitions) are the
  densest set in the subsystem. If its C.md can't land under 600 words after real condensing, that IS the split signal:
  fall back to
  `lifecycle/{registry (state), coordinator (manager+network_scan+scan_completion), status (freshness+failure+bus)}`.
  Decide during doc step 11, not preemptively.

## Resolved design questions (settled after fresh-eyes review)

1. **Rename `volume_scanner/` -> `network_scanner/`**: KEEP. Internal-only, kills the real scanner/volume_scanner
   ambiguity, and consistent with the existing `network_scan.rs` / "network path" vocabulary. Caveat noted in the area
   doc: MTP is USB/PTP, not literally network; the term aligns with committed vocabulary, so a third word
   (`trait_scanner`) would be worse.
2. **`reconcile_bench.rs` / `reconcile_correctness.rs`**: HOME IN `reconcile/`, not `tests/` (they guard reconcile code;
   the "touches writer/aggregator" test is nearly every reconcile test). `smb_scan_integration_test.rs` likewise homes
   under `transports/smb/`. `tests/` keeps only the genuinely whole-pipeline files.
3. **`writer/` + `aggregator/` as two areas**: KEEP SEPARATE. Distinct concerns (write protocol + `dir_stats` mutation
   vs bottom-up compute), each with real depth; merging would blow the C.md budget.
4. **`metadata.rs` loose**: KEEP LOOSE. A true 4-area shared primitive; homing it anywhere creates false ownership and
   inverts a dependency. Documented in the hub.
5. **Area granularity**: 13 areas (down from 14 — `observability/` folded into `watch/`). Right for a ~25k-line
   subsystem with touch-based autoload: editing `read/` shouldn't load `reconcile/`'s must-knows. Fine-grained is a
   feature here.

## Remaining decision for the lead

- **File-length allowlist consent** (see risk notes): OK to migrate the 15 existing `indexing/` allowlist entries to
  their new paths at unchanged line counts?

```

```

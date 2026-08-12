# Untangle the module dependency cycles

**Status: M0–M6 shipped. M7 (the ratcheting check) is the only milestone left.**

Measured result, `cargo-modules` before and after:

- Modules trapped in some cycle, all three crates: **184 → 132**.
- `cmdr` maximum component: **17 → 10** (the survivor is `file_viewer::*`, a cohesive cluster the Non-goals exclude).
- `cmdr-index` maximum component: **23 → 6** (the plan estimated 7).
- `cmdr-fs`: unchanged at 8, never in scope.

Six milestones cut the two large strongly-connected components in the Rust crates down to nothing, plus three small
cross-subsystem cycles, and then install a check so they can't regrow. The two biggest cuts are prerequisites for work
already on the roadmap: per-filesystem backend crates (FTP, S3, SFTP).

## Why

Two motivations, both David's, in priority order. When a decision below is ambiguous, resolve it toward the higher one.

1. **A clear, simple API between parts of the codebase, so an agent working on one doesn't carry the other.** A
   23-module cycle means no module in it can be understood or changed alone. This is the dominant reason.
2. **Faster builds and check runs when a part is unchanged.** Cycles don't cost compile time directly (a crate is one
   unit), but they block the crate extractions that do. `M1` below is a hard prerequisite for the per-backend crates.

Scaling context: the workspace is ~950k lines, FTP(S), S3, and SFTP backends are on the near roadmap, and each new
backend re-welds the `file_system` component the moment it touches the volume manager. The cost of `M1` is lowest now
and rises with every backend added first.

## What we measured, and the two traps in the tooling

`cargo modules dependencies --lib --package <p> --no-fns --no-types --no-traits --no-owns --no-externs --no-sysroot`
emits DOT; nodes are modules and edges are `use` dependencies. Runtime is ~19 s for the 240k-line app crate. Cycle
detection runs on the filtered DOT (Tarjan). Scratch scripts: `scratchpad/{cycles.py,analyze.py,scc2.py}` in the session
tmp dir; recreate them, they aren't tracked.

**Trap 1: `--acyclic` is unusable.** It runs BEFORE the filters, so it always trips on a type and its own method
(`TarCodec` ↔ `TarCodec::fmt`). Do cycle detection yourself on the filtered graph.

**Trap 2: `use super::*` fabricates edges with no symbol basis.** `cargo-modules` resolves a glob to every item the
parent re-exports, so a submodule with a top-level `use super::*` gains a fake edge to every sibling. Proof:
`volumes/nsurl.rs` is fully self-contained (every symbol it uses is defined in the file, `volumes/nsurl.rs:10-105`) yet
shows edges to five siblings, because `volumes/nsurl.rs:5` is `use super::*`.

Non-test glob files that inflate SCCs today: `volumes/{cloud,fs_type,mounts,nsurl,smb}.rs:5-6`,
`file_system/volume/backends/smb/*.rs` (9 files), `indexing/store/{connection,dir_stats,entries,meta}.rs`,
`indexing/reconcile/reconciler/{rescan,rescan_hold,rescan_settle}.rs`, `media_index/scheduler/lifecycle.rs:18`.

**Consequence: four "tangles" are artifacts, not work.** The `smb::*` 10-module, `indexing::store::*` 4-module,
`reconciler::rescan*` 3-module, and most of the `volumes` 8-module groups are glob noise. Neither of the two large
components below contains a non-test glob file, so both are real.

**Half-right, as M0 measured it.** `indexing::store::*` and `reconciler::rescan*` were pure glob noise and vanished
outright. `smb::*` (10 → 5) and `volumes` (8 → 4) shrank but survived, and the survivors are real: in both, the parent
module defines a type (`SmbVolume`; `LocationInfo` / `LocationCategory`) that several children build against via
inherent `impl` blocks while the parent re-exports from those same children. That's the idiomatic parent ↔ child shape
the Non-goals already exclude, merged into one hub because it's one parent with several children rather than isolated
pairs. Not fixable by importing more precisely: `LocationCategory` has no other home to import from.

**Trap 3, smaller: `--no-traits` doesn't filter `From` impls.** `archive_edit::engine ↔ archive_remote_edit` is not a
real edge; `archive_remote_edit.rs` never imports `engine` and defines its own `to_write_error` at line 348. The reverse
edge comes from the two `From` impls at `engine.rs:29,38`.

**Trap 4: an inherent-impl method is attributed to the module where its TYPE is defined, not where the `impl` block
sits.** Found the expensive way during M6: putting a `RescanDrain` struct in `rescan.rs` while
`EventReconciler::rescan_drain() -> RescanDrain` lived in `reconciler.rs` fabricated a `reconciler → rescan` edge, which
closed a loop and left the component at 10 instead of 6. Simulation said 6 and the code said 10 until the attribution
rule turned up. So: **when a cut doesn't produce the simulated number, suspect where a returned type is defined before
suspecting the analysis.**

## Non-goals

- **Not chasing zero cycles.** Of 44 groups across the three crates, roughly 16 are parent ↔ direct child (idiomatic
  Rust: `mod.rs` defines a type, the child implements against it, the parent calls the child). "Fixing" those means
  inventing a shim module to satisfy a graph. Don't.
- **Not touching cohesive clusters.** `file_viewer::*`, `menu::*`, `licensing::*`, `commands::agent::*` are single
  features split across files. Leave them.
- **No behavior change.** Every milestone below is a relocation or an inversion. If a cut appears to require changing an
  invariant rather than moving code, stop and report (see Risks).
- **Not unifying `LocationInfo`.** Real and worth doing (see M6's note), but it's its own effort.

## Design decisions

### 1. Ratchet on maximum SCC size, not cycle count

A 2-module parent/child group is noise; a 23-module group is a design problem. The check (M7) records the current
maximum SCC size per crate and fails when it grows. That makes progress measurable (23 → 7 → 6), keeps the ~16 idiomatic
groups permanently out of scope, and stops a new tangle forming.

### 2. Glob imports get replaced before the check lands, not after

The check is only trustworthy once `use super::*` is gone from the non-test files listed above, otherwise it reports
fabricated cycles. That's ~30 minutes of mechanical work (M0) with no architectural gain on its own, and it must come
first or every later measurement is noise.

### 3. Cut where the symbol is thin, not where the graph score is highest

The highest-scoring single edge in the index engine is `lifecycle::state → lifecycle::manager` (23 → 13 alone), and it
is the one edge we are NOT cutting: `IndexPhase::Running(Box<IndexManager>)` (`lifecycle/state.rs:69`) is an honest
ownership relation. Replacing it with `Box<dyn VolumeCoordinator>` trades a clear ownership story for acyclicity and
turns "what actually runs here" into a grep. Prefer cuts that move a thin thing (a type, a 3-line helper) to its correct
layer.

### 4. Backends never register themselves

SMB already does this correctly: `SmbVolume` is registered from `network/smb_upgrade.rs:241`, an outside wiring module,
never from the session layer. MTP is the anomaly. M5 makes the SMB pattern the house rule, which is the shape three new
backends should copy.

## Milestones

Ordered by return per unit of risk. M0 gates everything. M1 is the highest-value single change in the plan.

**What each one actually did** (M0–M6 are shipped; the milestone bodies below are kept as written, so read them as the
intent, not as remaining work):

- **M0** — 22 files de-globbed. `indexing::store::*` and `reconciler::rescan*` vanished; `smb::*` and `volumes` shrank
  to real parent ↔ child hubs (see above).
- **M1** — `file_system` 17 → 3. `get_volume_manager` is `pub(crate)` at its new home, 75 files repointed, no shim. A
  ninth back-edge turned up in `write_operations/rollback.rs` that the eight-edge count had missed (it wasn't in the
  cycle, so the count was right about the cycle and short about the churn).
- **M2** — the `commands::search` 3-cycle is gone; `cmdr-index` 23 → 20. `bindings.ts` byte-identical, as predicted.
- **M3** — the `events ↔ sink ↔ media_index::events` 3-group is gone and no module under `events/` is in any cycle. **It
  did not move the 20**: the progress pump sits on `manager → reporter → writer → … → manager` whichever parent owns it,
  so moving it to `lifecycle/` renamed a node rather than removing one. The payload enums live in `events/payload.rs`,
  not the `kinds.rs` this plan named — "kind", "type", and "category" are names to avoid.
- **M4** — `media_index` 5 → 0, plus the `writer ↔ upsert` pair, landing exactly the DAG below. Needed one thing the
  plan didn't foresee: `accounted`'s items had to drop their `accounted_` prefix, because while the writer still wrote
  `use super::coverage;` the edge landed on the facade and the cycle survived the split.
- **M5** — `backends::mtp` is out of every cycle; `mtp::connection ↔ file_ops` stays, as intended. The registrar lives
  in `mtp/volume_wiring.rs`, the structural twin of `network/smb_upgrade.rs`, so the two backends read as one pattern.
- **M6** — `cmdr-index` 20 → 6, better than the 7 estimated. Both invariants survive and the DB-delete one got
  _stronger_: withdrawal now actually un-routes a volume, closing a window where a reader could open a fresh connection
  to a database about to be unlinked. `state::volume_cancel_token` is gone entirely (three sites, not the one named).

### M0 — Replace the non-test `use super::*` globs

**Intent:** make the graph trustworthy before anything is measured against it.

Replace the top-level `use super::*` in the non-test files listed above with explicit imports. Purely mechanical; the
compiler names every missing symbol. Leave `use super::*` inside `#[cfg(test)] mod tests` blocks alone, that's the
idiomatic test pattern and it doesn't affect the production graph.

**Verify:** re-run the SCC analysis and confirm the `smb::*` (10), `indexing::store::*` (4), and `reconciler::rescan*`
(3) groups disappear, and that `volumes` drops from 8 to 2. If any survives, it was real, and it belongs on this list.

**Checks:** `pnpm check rust`. **Docs:** none.

### M1 — Move the volume-manager singleton out of the facade (`17 → 3`)

**Intent:** the highest-value cut in the plan, and a hard prerequisite for the per-backend crates. A crate cannot import
the app crate's facade.

`get_volume_manager()` (`file_system/mod.rs:209`, backed by `static VOLUME_MANAGER: LazyLock<VolumeManager>` at
`mod.rs:93`) lives in the facade module that also re-exports `write_operations::*` (`mod.rs:69-90`) and
`volume::MtpVolume` (`mod.rs:50`). Everything below reaches up for the singleton; the facade reaches down to re-export.
`cargo-modules` resolves re-exports to the defining module, so those two habits weld 15 modules together.

1. Move the `static` and the accessor to `file_system/volume/manager.rs`, where `VolumeManager` already lives
   (`manager.rs:18`) and whose entire crate-internal dependency surface is `use super::Volume` — it adds no new edge.
2. Bootstrap (`init_volume_manager`, `register_discovered_volumes`, `os_mounted_smb_shares`, `mod.rs:132-206,320-340`)
   **stays in the facade**, where knowing every backend is correct. Swap `VOLUME_MANAGER.` for `get_volume_manager()` at
   the seven sites.
3. Repoint the 53 `use crate::file_system::get_volume_manager` sites (mostly tests). A `pub use` shim in the facade
   would break the cycle with no call-site churn, since the graph follows the definition. **Do the sed anyway** — the
   shim is what regrows this.

The eight back-edges removed, all carrying only `get_volume_manager`: `archive_edit/compress.rs:27`,
`archive_edit/copy_into.rs:32`, `archive_edit/driver.rs:25`, `archive_edit/engine.rs:15`, `archive_edit/routing.rs:14`,
`write_operations/create.rs:30`, `write_operations/paste_clipboard.rs:20`, `mtp/connection/mod.rs:47`.

**Note the refuted hypothesis, so nobody re-derives it:** `write_operations` does NOT depend on MTP. The only `Mtp`
tokens under `write_operations/` are four doc comments (`scan_preview.rs:68,674`, `transfer/volume/copy.rs:465`,
`transfer/volume/preflight.rs:304`). `transfer/` is transport-agnostic as documented. There is no transport leak.

**Tests:** the existing suite. ~20 real lines changed, so a red test means a real move error. **Docs:**
`file_system/CLAUDE.md` + `DETAILS.md`, `volume/DETAILS.md`. **Checks:** `pnpm check --include-slow`.

### M2 — Three sub-hour cuts

**Intent:** bank the cheap, unambiguous wins independently, so they're mergeable the same day.

1. **`query_builder → commands::search` (kills a 3-cycle).** `search/ai/query_builder.rs:6` imports `TranslateDisplay`
   and `TranslatedQuery`, pure serialization DTOs at `commands/search.rs:164-194` (no methods). Move both to
   `search/ai/types.rs`, `pub use` from `commands/search.rs` so `commands/search.rs:152,153,317,330` and `ipc.rs` are
   untouched. **specta names types by struct identity, not module, so `bindings.ts` is byte-identical** —
   `bindings-fresh` should stay green, and if it doesn't, something else moved. ~45 lines. This is the only edge in the
   app crate pointing INTO the IPC layer from below; cutting it makes "nothing depends on `commands`" a statable
   invariant.
2. **Identity types out of the registry** (index engine cluster 1). `paths/routing.rs:27`,
   `lifecycle/lifecycle_bus.rs:37`, and `lifecycle/network_scan.rs:18` import `lifecycle::state` **only** for `VolumeId`
   (`state.rs:47`), `ROOT_VOLUME_ID` (`state.rs:51`), and `IndexVolumeKind` (`state.rs:589`, whose entire impl is five
   pure `matches!` predicates at `state.rs:606-655`). Move all three to a new leaf `indexing/volume.rs`, next to
   `metadata.rs` — `indexing/CLAUDE.md:53` already documents that file as exactly this pattern. Both public items are
   already re-exported at `lib.rs:96`, so the public API doesn't move. ~50 lines, ~14 import sites.
3. **Event and path helpers to leaves** (index engine cluster 3). `emit_dir_updated` (`reconcile/reconciler.rs:1627`, a
   3-line `events.emit(IndexEvent::DirsUpdated{..})` wrapper) → `events/`. `with_ancestor_closure`
   (`reconciler.rs:1588`), `collect_ancestor_paths`, and `compute_parent_path` (pure path arithmetic) →
   `paths/path_prefix.rs`. Nine call sites. Bonus: shrinks `reconciler.rs` (1,634 lines, allowlisted).

**Docs:** `indexing/CLAUDE.md` module map, `search/DETAILS.md`. **Checks:** `pnpm check` per item, full at the end.

### M3 — The `EventSink` residue (`indexing::events ↔ sink ↔ media_index::events`)

**Intent:** finish a refactor already in flight. This is residue from the recent `EventSink` work and it regrows if
left, because the next media event payload lands in `media_index::events` for the same reason.

1. Move `ActivityPhase`, `MemoryWatchdogAction`, `RescanReason`, and `ScanRunKind`
   (`indexing/events/mod.rs:37,82,107,120`) into a leaf `indexing/events/kinds.rs`; `mod.rs` and `sink.rs` both import
   down. ~95 lines. Cuts `sink.rs:25`.
2. Move `MediaEnrichTerminalReason` (`media_index/events.rs:40`) into `sink.rs` beside the `IndexEvent` variant carrying
   it; re-export from `media_index::events` for the ~20 scheduler call sites. ~40 lines. Cuts `sink.rs:23`.

Both are `pub use`-preserving, so consumer paths and `bindings.ts` stay identical.

**Also here:** `events::progress_reporter` (144 lines) reaches DOWN into `paths::routing`, `scanner`, and `writer`
(`progress_reporter.rs:26-28`). It's a scan-progress pump misfiled in `events/`, and it's why `events` isn't the leaf it
should be. Move it to `scanner/` or `lifecycle/`.

**Docs:** `indexing/events/CLAUDE.md` + `DETAILS.md`. **Checks:** `pnpm check`, `bindings-fresh`.

### M4 — Split media coverage into a read/write pair (`5 → 0`)

**Intent:** the strongest architectural return of the smaller cuts, and the split is pre-argued by the code itself.

`coverage.rs:220-241` already states that eligible (`COUNTS`) and accounted (`ACCOUNTED`) "have DIFFERENT sources and
update models, so they live in separate caches." Only the eligible half needs `scheduler::enrich`; only the accounted
half is called by the writer. The cycle is that they share a file.

1. **Move `parent_dir`** (`scheduler/enrich.rs:320-325`, a 5-line pure string helper used by nine files) to a leaf
   (`media_index/paths.rs`, or `cmdr_fs`). This alone kills `writer → enrich` (`writer/mod.rs:65`).
2. **Split `coverage.rs`** (660 lines; `media_index/coverage/` already exists holding `tests.rs`) along its own section
   divider into `coverage/eligible.rs` (~400), `coverage/accounted.rs` (~220), and `coverage/mod.rs` keeping the joining
   facade `folder_coverage` + `FolderCoverageCounts` + `covered_for_volume` + `StoredPartition`. `coverage/tests.rs`
   splits along the same seam.
3. Optional: move `UpsertAnalysis` (`writer/mod.rs:164`) to `writer/types.rs`, cutting the `writer ↔ upsert` 2-cycle.

Resulting graph: `coverage::eligible → enrich → writer → coverage::accounted → {store, paths}`. A clean DAG.

**Constraint:** keep `folder_coverage` and `covered_for_volume` as the ONLY `pub` surface, with `eligible`/`accounted`
private children, so `index-crate-isolation`'s public-item ceiling doesn't move.

**Docs:** `media_index/CLAUDE.md` (its "Counts stream; polls never build them" and "Coverage = scope + importance"
bullets carry paths), `media_index/DETAILS.md`. **Checks:** `pnpm check --include-slow`.

### M5 — MTP stops registering itself (`3 → 0`)

**Intent:** establish Decision 4's rule in code.

After M1, the residual component is `backends::mtp ↔ mtp::connection ↔ file_ops`. The downward direction is real and
heavy (`backends/mtp.rs:12` imports the connection manager, ~20 call sites). The back-edge is four lines of wiring:
`mtp/connection/mod.rs:441-442` and `:691-692` construct `MtpVolume::new(...)` and `register(...)`; `:508` and `:810`
call `unregister(...)` (which needs only the manager).

Install a `OnceLock` registration hook in `mtp::connection` (attached/detached), implement it where `MtpVolume` and the
manager are both visible, wire it at startup next to the existing `volume_broadcast::init(app)`. ~40 lines.
`MtpStorageRemoved` events already exist nearby, so the shape is familiar.

**Gotcha to preserve explicitly:** the connect path (`mtp/connection/mod.rs:425-450`) registers volumes BEFORE starting
the event loop, and that ordering looks load-bearing. The hook adds an indirection; preserve the ordering deliberately
rather than assuming the hook fires synchronously. Static analysis can't settle this — verify against a real device or
the virtual-MTP feature.

`mtp::connection ↔ file_ops` survives as a parent ↔ child pair (`file_ops.rs:10` takes `MtpConnectionManager` from its
parent). That's ordinary Rust. Leave it.

**Bonus, independent, do it here:** `mtp/connection/{cache.rs:10, bulk_ops.rs:7-8, directory_ops.rs:17}` and
`event_loop.rs:18` import `FileEntry`, `CopyScanResult`, and `VolumeError` via `crate::file_system::`, but all three
already live in `cmdr-fs` (`crates/cmdr-fs/src/entry.rs:107`, `volume/types.rs:176,296`). Repoint at `cmdr_fs::`
directly: find-and-replace, zero behavior change, and it deletes app-crate imports that would block extraction.

**Docs:** `mtp/DETAILS.md` records the "backends don't self-register" rule. **Checks:** `pnpm check --include-slow`
including the MTP suites (`--features cmdr/virtual-mtp`).

### M6 — Push the index read handles instead of pulling them (`21 → 7`)

**Intent:** the decisive cut for the index engine, and the only one in this plan with real design content. Its own
worktree, its own review.

`install_read_pool` (`read/enrichment.rs:125`) **no-ops for non-root**, so `get_read_pool_for` reaches back into
`INDEX_REGISTRY` for every other volume, while `lifecycle/state.rs:143-144` stores `Arc<ReadPool>` / `Arc<PendingSizes>`
in `IndexInstance`. That's a genuine two-way loop, not an import accident.

1. Make `install_read_pool` / `install_pending_sizes` store non-root volumes too (they already take a `volume_id`), and
   delete the `state::get_instance_*` fallbacks (`read/enrichment.rs:118,137`, `read/pending_sizes.rs:195`).
2. Pass the cancellation token through the loop config to remove `state::volume_cancel_token(ROOT_VOLUME_ID)` from
   `watch/event_loop/verification.rs:99`.

~200 lines. Combined with M2.2 this is what takes 23 → 7 and 6; **neither works alone** (21 and 22 respectively). They
are the same rule stated twice: _nothing below `lifecycle` may import `lifecycle::state`_.

**This touches a documented decision.** `lifecycle/CLAUDE.md:20-21` records "Root is special-cased to module globals…
non-root handles live only in the instance" as deliberate. Two invariants must survive: the invalidate-before-DB-delete
ordering (`state.rs:977-982`), and "reads via `ReadPool` never under the lifecycle lock". Update both docs in the same
pass; if either invariant can't survive the change, **stop and report** rather than weakening it.

**Docs:** `indexing/lifecycle/CLAUDE.md` + `DETAILS.md`. **Checks:** `pnpm check --include-slow`; this milestone touches
lifecycle and locking.

### M7 — The ratcheting cycle check

**Intent:** make the result self-defending.

Add `rust-module-cycles` to the Go check runner. It shells out to `cargo modules dependencies` with the filter flags
above, parses the DOT, computes SCCs, and compares the maximum SCC size per crate against a committed allowlist.

- **Warn-only at first**, listing every group, so the remaining ones are visible with numbers.
- **Flip to error** once M0–M6 have landed and the numbers are stable.
- Ratchet down like `file-length`: shrinking always passes, growth fails. Never hand-edit to raise a number.
- It must declare its `Inputs` (Rust sources across all members) and either a workflow reference or a `NotInCI` reason.
  A reason PLUS a reference fails `ci-coverage`.
- ~19 s on the app crate, so it belongs in the slow group, not `--fast`.
- Go unit tests with fixture DOT: a graph that grows its max SCC must fail; one that shrinks must pass.

**Record the four tooling traps** (`--acyclic`, globs, `From` impls, inherent-impl attribution) in
`scripts/check/checks/DETAILS.md`, or the next person re-derives them.

The ratchet numbers to seed it with, measured after M6: `cmdr` 10, `cmdr-index` 6, `cmdr-fs` 8. All three survivors are
declared non-goals (a cohesive feature cluster, `IndexPhase::Running(Box<IndexManager>)` plus the manager ↔ event-loop
pair, and a parent ↔ child hub), so the check can go straight to error rather than warning first.

**Docs:** `scripts/check/checks/DETAILS.md`. **Checks:** `pnpm check go`, `pnpm check`.

## Parallelization

M0 gates everything. After that:

- **M1, M2.1, M3, M4 are independent** — different files, different crates, no shared edits. Safe to run concurrently in
  separate worktrees.
- **M2.2 and M6 are the same effort split in two** and must be sequential, M2.2 first. M2.3 is independent of both.
- **M5 depends on M1** (it operates on M1's residual component).
- **M7 lands last**, when the numbers are stable.

## Risks

- **A cut needs an invariant change, not a move.** The plan assumes every cut is a relocation. M6 is the one with real
  design content and the one touching a documented decision. Mitigation: the stop-and-report gate in M6, and its own
  worktree.
- **MTP registration ordering (M5).** Static analysis can't settle whether the connect-path ordering is load-bearing.
  Mitigation: verify on a real device or the virtual-MTP feature, don't reason about it.
- **The graph lies if the globs come back.** A future `use super::*` silently re-inflates an SCC and the check reports a
  cycle that doesn't exist. Mitigation: M0, plus the traps recorded in `DETAILS.md`.
- **Chasing the artifacts.** Roughly 16 groups are idiomatic parent/child and four more are pure glob noise. Someone
  reading a raw cycle count will want to fix all 44. Mitigation: Decision 1 (ratchet on max SCC size), and the
  Non-goals.

## What we are NOT doing, and why it's tempting

- **`IndexPhase::Running(Box<dyn VolumeCoordinator>)`** — highest single-edge score (23 → 13), buys 6 → 5 at the end,
  and costs a clear ownership story. See Decision 3.
- **The `volumes` trigger/events split.** After M0 discounts the glob artifact, the real cycle is 2 modules: the genuine
  back-edge is `volumes/watcher.rs:25,159,208` calling `volume_broadcast::emit_volumes_changed()`, a deliberate
  fire-and-forget sink with 15+ call sites. Splitting `volume_broadcast.rs` (250 lines) into `{events,trigger,mod}` and
  registering the aggregator via `trigger::set_emitter` would fix it in about half a day. Worth doing eventually,
  smallest payoff of the set, so it's out of scope here.
- **Unifying `LocationInfo` / `LocationCategory`**, currently triplicated across `volumes/mod.rs:43,61`,
  `volumes_linux/mod.rs:85,97`, and `stubs/volumes.rs:11,23`, with a three-way cfg dispatch at
  `volume_broadcast.rs:81-88`. `volumes/CLAUDE.md:47` already documents `append_mtp_volumes` as duplicated across the
  macOS/Linux twins with a "set a new field in BOTH" warning. That's a real footgun and a genuine prize, but it's a
  separate effort and not needed to break any cycle.

## Found along the way, not fixed

Each of these turned up while cutting a cycle and is out of scope for this plan. Listed so they don't get lost.

- **The per-navigation verifier is a silent no-op on every non-root volume.**
  `reconcile/verifier.rs::verify_and_correct` reads the ROOT pool via `get_read_pool()` but writes through the
  _caller's_ writer, which `trigger_verification` takes from whichever volume the app named. `Index::verify_directory`
  runs per navigation for every volume, so on SMB, MTP, and external drives it reads a path that can't resolve in root's
  index, gets nothing, and does nothing. Fixing it means routing through `get_read_pool_for(volume_id)` plus
  `routing::index_read_path` and publishing under the right volume: a behavior change, and the one item here worth real
  attention.
- **`backends/mtp.rs` is 1,194 lines against a 1,095 allowlist entry.** Pre-existing, and inside the growth buffer, so
  it doesn't warn. Splitting it is its own job.
- **A test-isolation gap that only bare `cargo test` sees.**
  `indexing::host::volumes::tests::an_uninstalled_provider_reports_nothing_mounted` doesn't take `handle::test_lock()`
  the way its siblings do, and `handle/tests.rs`'s `.install_for_test()` call doesn't hold it either, despite the
  method's own doc saying to. In-process runs fail it roughly 2 of 3 times on `main` as well. nextest (one process per
  test, and the project's runner) is unaffected, which is why it has stayed hidden.

## Related

- `docs/architecture.md` — the subsystem map.
- `crates/cmdr-index/src/indexing/handle/DETAILS.md` — the public-surface audit; M4's constraint on the public item
  ceiling comes from `index-crate-isolation`.
- `crates/cmdr-index/src/indexing/lifecycle/CLAUDE.md` + `DETAILS.md` — the documented decision M6 changes.
- `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` — why archives aren't a writable `Volume`, which is why
  `archive_edit` legitimately sits inside `write_operations` rather than beside it.

# Folder-importance subsystem — implementation plan

Status: SHIPPED (M1–M4), 2026-07-08. Owner: David. This plan settled the seven design questions David posed, verified
against the live tree (`main`, verified 2026-07-08). It scoped a **general importance API** that any expensive feature
can consume — not an internal of any one consumer. Durable intent now lives in `importance/` and `indexing/` C+D.md; the
open questions below survive the milestone (weight tuning, sampling cost) as follow-ups.

## Why this exists

Several planned features need to know, cheaply and deterministically, **which folders matter**:

- The in-app **agent** (`docs/specs/later/agent-spec.md`) gates LLM summarization on importance (§5.1, §5.2), scores
  event-bundle interest by it (§6.2), and feeds the weight to the LLM as a reasoning input (§5.1). Its decision D8 fixes
  "deterministic importance scorer, cached in the drive index."
- The **media-ML enrichment scheduler** (`docs/specs/later/media-ml-index-plan.md`) wants to enrich important folders
  first and gate expensive passes (its enrichment is opt-in, throttled, and should not spend the ANE on a
  `node_modules`).
- Future expensive features generally (disk-cleanup advice, proactive summaries, prefetch) will want the same signal.

Building a bespoke scorer inside each of these would duplicate the heuristics, the storage, and the recompute wiring
three times, and drift. So importance is **one subsystem with a read API**, computed once per folder, stored per volume,
and queryable by any consumer. The scorer itself is a set of **pure functions over listing metadata** (values in, score
out, no I/O) so the formula can be unit-tested and tuned against real directory trees without a running app.

A headline requirement shapes the storage decision: **importance for an unmounted volume stays queryable.** David wants
his NAS share's importance info available while the NAS is off. Weights are regenerable derived data, so they can live
in a local per-volume file that outlives the mount.

### Product values in play (from `docs/design-principles.md` and `AGENTS.md` § Principles)

- **Respect the user's resources.** The scorer is cheap pure Rust, never a model, never a per-event hot path (agent-spec
  principle 1). Recompute is cost-bounded and rides passes the indexer already runs.
- **Rock solid + everything derived is disposable.** Weights are a cache: purge them and they regenerate on next mount,
  exactly like the drive index. No human work lives here, so there is no data-safety crux (unlike media-ML Decision 4).
- **Elegance above all.** A neutral subsystem with a clean one-way read boundary mirroring `search/`→`indexing/`, not a
  scorer bolted into whichever consumer landed first.
- **Delightful UX + radical transparency.** The API exposes an **explain** call (per-signal contribution breakdown) so a
  consumer — and David, during tuning — can see _why_ a folder scored as it did, not just the number.

## Current state (the map the implementer needs)

Claims below were verified against the code on 2026-07-08 (file refs may drift — confirm with `codegraph_search`). Read
the colocated `CLAUDE.md` + `DETAILS.md` of each subsystem before building on it.

- **`src-tauri/src/indexing/`** — per-volume SQLite index DBs (one writer thread per DB; local + SMB + MTP each get
  their own), recursive size aggregates, `ReadPool` for reads, per-volume registry (`INDEX_REGISTRY`), a freshness
  model, phase events. **Hard invariants we must respect** (from `indexing/CLAUDE.md`, verified): the index is a
  **disposable cache** (schema mismatch / corruption ⇒ delete + recreate via `delete_and_recreate`, no migrations, bump
  `SCHEMA_VERSION`); **one writer thread per DB** (reserve the registry slot lock-first; never hold `INDEX_REGISTRY`
  across a blocking manager call); **`platform_case` collation registered on every connection** (raw `sqlite3` can't
  read the name column — the `index-query` crate exists precisely for ad-hoc reads); reconciler/event loops hold a
  **READ** connection only; **no rayon for macOS-framework calls** (dedicated OS threads +
  `objc2::rc::autoreleasepool`); one global 16 GB memory watchdog (`stop_all_indexing`, indexing-specific). FDA gates
  only `root` auto-start.
  - **Identity model (verified, load-bearing).** `entries` has **no stable cross-rebuild id**: `id` is insert-order over
    a table truncated before each full scan (`store/entries.rs`), and jwalk's parallel order isn't deterministic. The
    real identity is **`(parent_id, name_folded)` UNIQUE** — i.e. the **path**. So importance keys on **path**, exactly
    as the index itself does, and matches what media-ML Decision 3 already established.
  - **The aggregator is the recursive-aggregate pass.** `aggregator/mod.rs::compute_all_aggregates` (and the partial /
    subtree variants) walk the index tree bottom-up computing per-directory size/count stats into `dir_stats`. This is
    the natural sibling computation for importance (Decision 1 weighs it).
  - **`apply_freshness_event_on` (`state.rs:394`) is the neutral scan-completion chokepoint.** Both the LOCAL path
    (`manager.rs`, ~line 929) and the network path (`network_scan.rs`, ~line 345) funnel `FreshnessEvent::ScanCompleted`
    through it, taking the `Arc<...>` freshness handle directly (never re-locking `INDEX_REGISTRY`).
    `IndexAggregationCompleteEvent` fires from both too. **Both are wired to the frontend only** (`.emit(app)` to the
    webview); there is **no in-process backend pub/sub** a Rust subsystem can subscribe to today. This is the exact seam
    media-ML Decision 7 designed its lifecycle bus around, and this plan builds the minimal version of it (Decision 4).
  - Network volumes (SMB/MTP) emit only `Scanning → Live` at the phase layer, but **both kinds fire
    `FreshnessEvent::ScanCompleted`** — drive "ready to score" off that, not off a phase the network path never sends.
- **`src-tauri/src/indexing/enrichment.rs`** — `ReadPool` + `get_read_pool_for(volume_id) -> Option<Arc<ReadPool>>` is
  the sanctioned read boundary: reads route through the per-volume pool, never under the registry mutex; a `None` pool
  means "no index registered, skip." `IndexStore` (`store/`) owns the connection factories and the `platform_case`
  registration. Our read API mirrors this (Decision 5).
- **`src-tauri/src/search/`** — the house precedent for a **read-only, one-way consumer** of `indexing/`: it reaches the
  index only through `ReadPool`/`IndexStore`, keeps a pure `engine.rs` (no I/O), and never takes a raw `rusqlite` dep on
  the index. The importance read API mirrors this boundary exactly, and media-ML Decision 8 mandates the same for
  `media.db` — so all three consumers share one discipline.
- **`Location` shipped** (`src-tauri/src/location.rs`): `pub struct Location { volume_id, path }`, specta-exported, with
  `resolve_location`. So the keying vocabulary is settled: importance keys on **`Location`** (`(volume_id, path)`),
  aligned with navigation. (The `location-type-nav` plan itself is still in progress — the type and resolver landed,
  bare-path navigation isn't killed yet — so its unchecked entry in `docs/specs/index.md` is correct.)
- **`InMemoryVolume`** (`file_system/volume/backends/in_memory.rs`) — a full in-memory `Volume` impl storing entries in
  a `HashMap`, already used across ~169 test sites. This is the base the synthetic-home fixture generator builds on
  (Decision 6), as both the agent spec (§15) and media-ML expect.
- **Navigation signals live in the FRONTEND.** `lastUsedPaths` (per-volume last path) and tab state persist through
  `apps/desktop/src/lib/app-status-store.ts` (the Tauri store / `app-status.json`), keyed by `volumeId`. There is **no
  backend-visible navigation history today.** The backend `lastUsedPaths` mentions in `volume/ids.rs` are about _volume
  id stability affecting_ that frontend store, not a backend store. So "folders the user visits" (agent-spec §5.1) needs
  a small new backend-visible signal — this plan specs it minimally and gates it (Decision 3).
- **Dev/debug surfaces that exist:** the `index-query` crate (`crates/index-query/`, ad-hoc index reads with the
  collation registered) and the debug window / `DEBUG_STATS` timeline. The tuning surface (Decision 6) extends the
  `index-query` pattern rather than inventing a new one.

## Key decisions (with intent — adapt if reality differs, but know the why)

**D1 — A neutral `src-tauri/src/importance/` subsystem, NOT an aggregator column and NOT under `agent/`.** Importance is
a scoring _policy_ (tunable weights, a formula that iterates, an explain breakdown) consumed by three unrelated
features; the aggregator is load-bearing size-math with four easy-to-break partial-aggregation rules. Folding importance
into `compute_all_aggregates` would (a) couple a churny tunable formula to the one place a bug ships wrong directory
sizes, (b) force every formula tweak through a `SCHEMA_VERSION` bump + full rebuild, and (c) hide the policy inside
indexing where no consumer can see or test it in isolation. _Why not `agent/`:_ the agent is only the first consumer;
media-ML and future features are peers. So `importance/` is a sibling of `search/` — a one-way read consumer of
`indexing/` with its own store. _What it reuses from the aggregator:_ it reads the same `dir_stats` and entry-tree the
aggregator produced, via the read pool; it does not recompute sizes.

**D2 — Weights live in a separate per-volume `importance.db`, written by its own single writer.** Options weighed: (a)
columns in the index DB written by the indexing writer — rejected: it extends the indexing writer's command surface for
a foreign concern, couples the formula to `SCHEMA_VERSION`, and means a formula change wipes the whole index; (b) a
separate per-volume store with its own writer — **chosen**; (c) the agent spec's `main.db` — **does not exist yet; this
plan does not create it.** The `media.db` precedent (media-ML Decision 3) is exact: a separate per-volume DB respects
"one writer thread per DB" (no contention with the size-index writer), gives importance its own disposable lifecycle,
and slots SMB/MTP volumes in naturally through the per-volume registry pattern. It carries the index's disposable-cache
discipline verbatim: `platform_case` collation on every connection, delete-and-recreate on schema mismatch (no
migrations), path-keyed rows. Beyond the scalar, each row also persists the **raw signal vector** (the `FolderSignals`
the score was computed from): consumers don't all mean the same thing by importance (the agent's "worth attention" vs
media-ML's "contains meaningful content"), so storing the signals lets a future consumer apply its own weighting profile
over them without a redesign or a rescan. The default scalar stays the common currency. **Offline-unmounted reads fall
out for free:** `importance.db` is a local file that outlives the mount, so a consumer opens it and queries a NAS
share's weights while the NAS is off. **When the OS purges the cache:** the file vanishes, the read API returns "no
weights for this volume," and the next mount + scan regenerates them — weights are disposable, identical to the
index-purge path. **Reconcile with agent-spec D8** ("weights cache in the drive index"): D8 predates this
general-subsystem framing; a separate `importance.db` satisfies D8's real intent (regenerable cache placement, split
from durable data) better than an index column, since it also serves media-ML and survives an index-schema wipe. This is
a confirmed, intentional refinement of agent-spec D8 (David approved the separate-DB placement); the agent spec points
here. **Location-independence:** the agent spec plans to relocate index DBs to `~/Library/Caches/` later;
`importance.db` sits beside whatever the index's cache directory is at the time and neither depends on nor blocks that
move.

**D3 — The scorer is pure functions over listing metadata; navigation signals enter through a small typed backend store,
gated to a named milestone.** The scoring inputs (agent-spec §5.1, verified as the requirements source):
known-unimportant name denylists (`node_modules`, caches, build artifacts, `.git` internals), hidden/system ownership,
extension count and **diversity** (monoculture folders score low), mtime recency, project markers (`.git` and similar
raise the subtree), path-class priors (Downloads/Desktop/Documents/project-roots high; `~/Library`/caches low), Cmdr
navigation signals, and sampled `kMDItemLastUsedDate` on local volumes only. **The scorer core is pure**
(`score(inputs) -> Score` + `explain(inputs) -> Vec<SignalContribution>`, no I/O) — a hard requirement per the agent
spec's testability-seams pattern (§6.3, §15), so unit tests construct inputs directly and the formula iterates without a
running app. **Weights are tunable** (a `Weights` struct with named coefficients, defaulted, overridable for the tuning
loop), because the formula needs iteration against real trees (§18.3) and must not be hardcoded blind. **Per-volume
signal availability differs and degrades explicitly:** SMB/MTP have no Spotlight metadata, so `kMDItemLastUsedDate` is
`None` there and its weight redistributes; ownership/hidden flags come from the listing where available; the scorer
takes an `available: SignalSet` and never fabricates a missing signal. **Navigation signals:** since they live in the
frontend today (see Current state), feeding "folders the user visits" needs a new backend-visible signal. This plan
specs it **small** — a typed `record_visit(Location)` that the frontend already-existing navigation commit calls,
persisted as a compact per-volume visit-count/recency table (privacy-sane: counts and timestamps only, no content,
local-only, in `importance.db`). The agent spec's `user_action_log` is the planned superset of this signal; that effort
is queued right after this subsystem, and when it lands it becomes the visit signal's feeder (`record_visit` folds into
it) — never two parallel recorders. This plan **gates the visit signal to M2** so M1's pure scorer lands without it (the
scorer treats the visit signal as just another optional input, `None` in M1). If wiring the frontend call proves larger
than a thin command, it defers to a named follow-up and the visit signal stays `None`; the scorer is designed so its
absence only removes one term.

**D4 — Build the minimal neutral in-process lifecycle bus in `indexing/`, and sweep the registry at startup.**
Importance must update when listings change, and it needs to know when a volume finished scanning. Media-ML Decision 7
designed this bus but nobody built it; it is shared infrastructure media-ML needs anyway, so this plan builds the
minimal version and carries over that decision's reasoning rather than re-deriving it:

- **Publish from the neutral chokepoint `apply_freshness_event_on`** (where `ScanCompleted` funnels for both local and
  network — verified), alongside the existing Tauri `.emit`. `indexing/` publishes without knowing who listens; the
  clean one-way direction (consumers depend on `indexing/`, never the reverse) is preserved.
- **Per-volume `tokio::sync::watch` (last value retained), not `broadcast`.** `broadcast` does not replay backlog to a
  receiver created after a `send`, so a `ScanCompleted` fired during `setup()` before the importance scheduler
  subscribes is lost; a `watch` retains the last state for a late subscriber.
- **Sweep `INDEX_REGISTRY` at startup, then rely on the bus.** A previously-completed volume loads _ready_ at launch
  from persisted `scan_completed_at` **without re-firing a scan event** — a scheduler that only waits for future events
  would never score an already-indexed volume after a restart (the common case). So on startup, scan the registry for
  already-ready volumes and schedule them, then rely on the bus for subsequent transitions. Guarantee
  subscribe-before-publish ordering.
- **Scheduling is idempotent/coalescing per `volume_id`:** the sweep and a concurrent startup-scan's `ScanCompleted` can
  both target one volume; a pass already running/queued sets a re-run flag instead of enqueuing a second. Covered by a
  coalescing test in M1.

**D5 — Recompute is full-volume on scan completion at first, incremental per-folder on listing-change events later;
cost-bounded either way.** The scorer is cheap (pure arithmetic over already-read `dir_stats` rows), so a full-volume
pass on `ScanCompleted` is affordable and is the initial default (landing in M2 with the scheduler) — walk the index
tree once, score each folder, write to `importance.db`, tag every row with the scan generation (as-of marker).
**Incremental** recompute (rescore only the folders whose listing changed, driven off the reconciler's per-directory
change events) is a later refinement (M2/M3), bounded to the touched subtree and its ancestors (a project marker or
mtime change can raise a parent). The full pass is cost-bounded by walking `dir_stats` (already in memory-friendly
SQLite), not the filesystem; the incremental path is bounded by the changed-folder set. **Sampled
`kMDItemLastUsedDate`** (local only) is the one potentially-slow input: sample rather than sweep (agent-spec §5.1,
§18.4), cap the sample count per pass, and run it off the IPC thread on a dedicated OS thread with an autoreleasepool
(never rayon — macOS framework call).

**D6 — A read API mirroring the `search/`→`indexing/` boundary, with staleness semantics, subscriptions, and an explain
call.** The API (`ImportanceIndex`, mirroring `IndexStore`/`ReadPool`: owns the `importance.db` connection pool,
registers `platform_case`, enforces one-writer) exposes:

- `weight_for(location) -> Option<ScoredWeight>` — the weight for one `(volume_id, rel_path)`, `None` if unscored.
- `top_n(volume_id, n) -> Vec<ScoredWeight>` — the most important folders on a volume (media-ML's "enrich important
  first").
- `above_threshold(volume_id, threshold) -> Vec<ScoredWeight>` — threshold queries (the agent's summary gate).
- `explain(location) -> Option<Explanation>` — per-signal contribution breakdown, for consumer transparency and David's
  tuning loop.
- `signals_for(location) -> Option<FolderSignals>` — the stored raw signal vector, for a consumer that applies its own
  weighting profile instead of the default scalar. Per-consumer profiles are the documented extension path; the scalar
  remains the common currency.
- A **subscription** (a `watch` or a callback the read API exposes) so a consumer that needs change notifications learns
  when a volume's weights were recomputed, rather than polling.
- **Staleness semantics:** every `ScoredWeight` carries the **scan generation / as-of marker** it was computed from, so
  a consumer can tell "this weight is from the May 28 scan" — the same first-class per-volume staleness the agent spec
  makes a feature (D7: answering about unmounted volumes with an as-of caveat). Consumers never assume freshness. The
  keying type is **`Location`** throughout (aligned with navigation, per Current state). `search/` stays a pure read
  consumer of _its_ index; `importance/` is a pure read consumer of _its_ store and of the index read pool — no consumer
  takes a raw `rusqlite` dep on `importance.db`.

**D7 — No IPC surface in v1 beyond the dev/tuning command.** v1 consumers (agent, media-ml) are in-process Rust and
reach importance through the read API directly, not over IPC — so no `tauri-specta` bindings are needed for them. The
one IPC touchpoint is D3's `record_visit(Location)` command (M2) and the dev-tuning surface (M6). If a _frontend_
feature ever needs to display importance (e.g. a "hot folders" panel), it adds typed bindings then, following the
subscribe-don't-poll house rule (a push event on recompute, never a poll). Specced as a clean extension point, deferred
out of v1.

## Architecture

```
indexing/ (existing, +minimal bus)          importance/ (new subsystem)                    consumers (own plans)
  per-volume index DB                          scorer/  (PURE: score + explain, no I/O)       agent/    (agent-spec)
  aggregator: dir_stats  ──── read pool ─────►   inputs assembled from dir_stats + entries       gate summaries on weight
  ReadPool / IndexStore                          Weights (tunable coefficients)                   score event interest
  apply_freshness_event_on  ── publish ──►      scheduler: subscribe to bus + startup sweep,   media_index/ (media-ml plan)
    (neutral per-volume watch bus, NEW)           coalescing per volume_id                        enrich important folders first
                                               store/   importance.db (path-keyed, disposable,   future expensive features
                                                 platform_case, delete+recreate, as-of gen)
                                               visit signal: record_visit(Location) → counts    ALL reach it via the
                                                 (M2; local-only, privacy-sane)                   ImportanceIndex read API,
                                               ImportanceIndex read API  ◄── subscribe            never raw rusqlite.
                                                 weight_for / top_n / above_threshold / explain
                                                 + staleness (as-of scan generation)
                                               dev tuning surface (extends index-query pattern)
```

The scorer sits behind pure functions with the tunable `Weights` injected, so the formula, the explain breakdown, and
every signal's contribution are testable without a running app, a real volume, or Spotlight.

## Milestones

Each milestone is independently shippable and leaves the tree green. Sequential is the default. Agent and media-ML
consumers stay **out of scope** (they wire in via their own plans); this plan states the contract they consume.

### M1 — Pure scorer + fixture generator + unit tests (no storage, no wiring)

The formula and its tests, with zero I/O and zero coupling — so the risky, iterate-heavy logic is proven and tunable
before any storage or scheduler lands.

- New `src-tauri/src/importance/scorer/`: the **pure** `score(inputs: &FolderSignals, weights: &Weights) -> Score` and
  `explain(inputs, weights) -> Explanation` (per-signal `Vec<SignalContribution>`), plus the input types
  (`FolderSignals` carrying the §5.1 signals, `SignalSet` marking which are available, `Weights` with defaulted tunable
  coefficients). Everything is values-in/values-out; no `rusqlite`, no `Volume`, no filesystem.
- Signal coverage in M1: name denylist, hidden/system ownership, extension count + diversity, mtime recency, project
  markers raising a subtree, path-class priors. **Navigation-visit and `kMDItemLastUsedDate` signals are typed into
  `FolderSignals` as optional and left `None` in M1** (they wire in M2), so the formula shape is final but their sources
  land later.
- **Synthetic-home fixture generator** (agent-spec §15, §20.4): a builder over `InMemoryVolume` that constructs
  realistic home-directory trees (a Downloads with mixed junk, a `.git` project, a `node_modules`, a monoculture log
  folder, a Documents/invoices tree) and derives `FolderSignals` from them — the corpus the scorer iterates against.
  Verify `InMemoryVolume`'s shape first (confirmed present, `HashMap`-backed).
- **Docs:** new `importance/CLAUDE.md` + `DETAILS.md` (sibling, enforced by `claude-md-details-sibling`); an
  `importance/` row in `docs/architecture.md`; record the scorer's signal list and the tunable-weights rationale in
  `DETAILS.md`. No user-facing strings yet ⇒ no i18n.
- **Tests (TDD red→green — this is the pure/risky logic the rule targets, `tdd-red-green`):** fail-first then implement,
  for each signal's contribution (a `node_modules` scores near-floor; a `.git` project root scores high; a monoculture
  log folder scores below a mixed folder; recency raises a folder); the **explain breakdown sums to the score**; a
  missing signal (SMB with no Spotlight) redistributes rather than fabricating; the fixture generator produces the
  expected trees. All pure — no FFI, no DB.
- **Checks:** `pnpm check --fast` iterating; full `pnpm check` at end (clippy, rust tests, `claude-md-details-sibling`,
  `docs-reachable`, file-length). Smoke-test 1–2 scorer cases before the full run (`test-infra-smoke-first`).

### M2 — Storage + writer + lifecycle bus + full-volume recompute + visit signal

Give the scorer a home and make it fire on scan completion.

- New `src-tauri/src/importance/store/`: per-volume `importance.db` with the index's disposable-cache discipline
  (`platform_case` on every connection, delete-and-recreate on schema mismatch, `SCHEMA_VERSION`, one writer thread);
  **path-keyed** weight rows tagged with the **as-of scan generation**, each carrying the scalar plus the serialized raw
  signal vector (Decision 2); the `ImportanceWriter` command surface (write a volume's weights, purge a volume).
- **Minimal neutral lifecycle bus in `indexing/`** (Decision 4): a per-volume `watch` published from
  `apply_freshness_event_on`, subscribe-before-publish ordering, documented in `indexing/DETAILS.md`. This is the shared
  infrastructure media-ML will also consume.
- **Scheduler** (`importance/scheduler.rs`): subscribes to the bus + does the startup `INDEX_REGISTRY` sweep; on a
  volume's `ScanCompleted` (or a swept-ready volume at startup), runs the **full-volume recompute** — read `dir_stats` +
  entry-tree through the index read pool, assemble `FolderSignals` per folder, score, write. Idempotent/coalescing per
  `volume_id`. Local only in M2 (SMB out until M4).
- **Visit signal (Decision 3):** a typed `record_visit(Location)` command the frontend's navigation-commit calls,
  persisted as a compact per-volume visit-count/recency table in `importance.db` (local-only, counts+timestamps only).
  The scorer's now-`Some` visit input feeds in on the next recompute. If the frontend wiring proves larger than a thin
  command, defer it to a named follow-up and keep the visit input `None`.
- **`kMDItemLastUsedDate` sampling** (local only): sampled, capped per pass, on a dedicated OS thread with an
  autoreleasepool (never rayon). Feeds the recency-of-use input.
- **Docs:** `importance/DETAILS.md` storage + scheduler + bus sections; `importance/` architecture-map row updated; the
  lifecycle-bus mechanism documented once in `indexing/DETAILS.md` (single-source) and pointed to from
  `importance/DETAILS.md`; the visit-signal privacy posture noted (local-only, no content) for `docs/security.md`.
- **Tests:** _smoke first_ — open/recreate `importance.db` and round-trip one weight before building on it. _TDD
  red→green:_ the **path-keyed staleness / as-of-generation** predicate; the **scheduler coalescing** (sweep +
  concurrent `ScanCompleted` ⇒ one pass, Decision 4); the **bus late-subscriber replay** (a `ScanCompleted` fired before
  subscribe is still seen via the `watch`); the **startup-sweep path** (a volume Fresh-at-launch with no new scan still
  gets scored). _After:_ a full-recompute integration test over a synthetic index (fake signals, no FFI) asserting
  `importance.db` holds the expected ranking; a macOS-gated test that `kMDItemLastUsedDate` sampling runs without
  blocking and stays within its cap.
- **Checks:** full `pnpm check`; `--include-slow` before wrapping (the DB + scheduler paths).

### M3 — Read API + explain + subscriptions + incremental recompute + dev tuning surface

The consumable API and the tuning loop.

- **`ImportanceIndex` read API** (Decision 6, mirroring `IndexStore`/`ReadPool`): `weight_for`, `top_n`,
  `above_threshold`, `explain`, `signals_for`, each returning `ScoredWeight`/`Explanation`/`FolderSignals` carrying the
  as-of generation; a **subscription** (`watch`/callback) firing when a volume's weights are recomputed. Owns the
  `importance.db` read pool and the `platform_case` registration; no consumer takes a raw `rusqlite` dep.
- **Incremental recompute** (Decision 5): rescore only the changed folders (+ affected ancestors) off the reconciler's
  per-directory change events, bounded to the touched subtree. Full-volume recompute stays the scan-completion default.
- **Dev tuning surface** (Decision 6, David's feedback loop): a minimal dev command extending the `index-query` crate
  pattern — run the scorer against David's real home directory (or any path) and print the ranked folders **with their
  explain breakdowns**, so David can eyeball output and tune `Weights` against reality (§18.3). Keep it minimal; it
  reads through the same read API, no separate write path.
- **Docs:** `importance/DETAILS.md` read-API boundary (single-source home for "how consumers reach importance"; the
  agent and media-ML plans point here rather than restating); the incremental-recompute rationale; the
  dev-tuning-command usage.
- **Tests:** _TDD red→green:_ `top_n`/`above_threshold` ordering and threshold-edge correctness; `explain` round-trips
  the per-signal breakdown; the subscription fires exactly once per recompute; incremental recompute rescopes to the
  changed subtree + ancestors and leaves untouched folders' as-of generation intact. _After:_ a read-API integration
  test over a populated `importance.db`.
- **Checks:** full `pnpm check --include-slow`.

### M4 — SMB / multi-volume semantics + offline-unmounted reads

Extend past local, and prove the headline offline capability.

- **SMB scored** (local + SMB from day one per David's requirement): the scheduler handles SMB volumes (driven off
  `FreshnessEvent::ScanCompleted`, which SMB fires even though it never emits the `Aggregating`/`Reconciling` phases —
  verified). Signal degradation is explicit: no Spotlight metadata on SMB, so `kMDItemLastUsedDate` is `None` and its
  weight redistributes; the scorer already handles this via `SignalSet` from M1. **MTP is on-demand only** (per the
  agent spec), not background-scored.
- **Offline-unmounted reads (the headline, Decision 2):** because `importance.db` is a local per-volume file, the read
  API answers `weight_for`/`top_n` for an **unmounted** SMB volume (David's NAS off) from the on-disk store, with the
  as-of generation marking how stale it is. Prove it end-to-end: score a volume, "unmount" it (drop its index
  registration), and assert the read API still returns its weights with the correct as-of caveat.
- **Late-registering volumes on the bus:** a share mounted after startup registers its per-volume `watch` when it
  appears; the scheduler subscribes on registration (the media-ML plan flags this as a latent design point — resolve it
  here now that SMB is in scope).
- **Docs:** `importance/DETAILS.md` multi-volume + offline-read section; note MTP-on-demand-only; `docs/architecture.md`
  updated if the subsystem map gained a network note. Any user-facing string (none expected in v1) goes through the i18n
  catalog with a `@key` description.
- **Tests:** _TDD red→green:_ SMB signal degradation (no Spotlight ⇒ redistributed weights, no fabricated input); the
  **offline read** returns stored weights with the right as-of generation for an unregistered volume; late-volume bus
  subscription. _After:_ a multi-volume scheduler integration test (local + fake-SMB) over synthetic indexes.
- **Checks:** full `pnpm check --include-slow`.
- **Also landed (a memory prerequisite for NAS-sized volumes):** the full-recompute walk was restructured to O(dirs)
  memory — materialize directories only, stream file rows into per-parent accumulators — so a multi-million-entry NAS
  index no longer materializes the whole entries table (was hundreds of MB transient). Had to land before SMB scoring
  went live, since NAS volumes are where it bites. Pinned by a characterization test (`ChildAggregate` matches a
  whole-tree oracle).

**SHIPPED 2026-07-08.** Typed volume kind on the registry (`IndexVolumeKind`), `ScoringPolicy::for_kind` (Local + SMB
scored, MTP excluded), a registration `broadcast` bus for late volumes, and the O(dirs) walk. Details in `importance/`
and `indexing/` C+D.md.

## Cross-cutting

- **Resources + the shared memory watchdog.** The scorer is cheap, but a full-volume recompute plus
  `kMDItemLastUsedDate` sampling is real work on dedicated low-priority OS threads (not rayon — the sampling touches
  macOS frameworks). The existing indexing watchdog measures process-wide resident memory but only stops _indexing_; if
  a recompute ever holds meaningful memory, hook its cancellation into that same watchdog's stop action rather than
  standing up a second ceiling (two ceilings over one resident pool each see headroom). Expected to be light; wire it if
  measurement says otherwise.
- **Cancellation + crash-safety.** Recompute is resumable from the as-of generation (a crash mid-pass leaves a
  partially-written generation; the next pass overwrites it). `importance.db` is disposable; nothing here must survive a
  wipe (contrast media-ML's durable identity store — importance has no durable-human-work analog, which is why this plan
  has no data-safety crux).
- **GC / invalidation.** When a source folder vanishes (index deletion, deletion-driven not absence-during-rescan, per
  the same hazard media-ML Decision 3 documents — a full rescan transiently truncates `entries`), its importance row is
  stale but harmless; the next full recompute drops it. A cheap deletion-driven GC can prune it, but correctness does
  not depend on it (unlike media-ML, where orphan rows cost re-enrichment).
- **Staleness is first-class, never an error** (agent-spec D7). Every weight carries its as-of scan generation;
  consumers caveat their answers ("as of the May 28 scan"). This is what makes offline-unmounted reads honest.
- **No string-matching for classification** (`no-string-matching`): signal availability, volume kind, and scorer state
  cross any boundary as typed enums, never message-substring branches. The name denylist is a set-membership check on
  folded names, not a substring match on a user-facing string.
- **Single-source docs.** The read-API boundary and the lifecycle-bus mechanism each get **one** canonical home
  (`importance/DETAILS.md` and `indexing/DETAILS.md` respectively); the agent and media-ML plans point here rather than
  restating (`docs.md` single-source rule). `docs/architecture.md` gets a map row (what + where + pointer), never the
  mechanism.
- **Dependencies.** No new crate is anticipated (pure Rust arithmetic + `rusqlite` already vendored +
  `tokio::sync::watch` already in the tree). If one becomes necessary, `cargo deny check` + a verified ≥3-day-old
  version per the project `dependencies` rule and `use-latest-dep-versions`.
- **Consumer contract (stated, not built here).** The agent gates summaries via `above_threshold`, scores event interest
  via `weight_for`, and passes the weight to the LLM; media-ML orders enrichment via `top_n` and gates expensive passes
  via `above_threshold`; both subscribe to recompute notifications and read the as-of generation. A consumer whose
  notion of importance differs from the default scalar weights the stored signal vector itself (`signals_for`) rather
  than asking for a new scalar. They wire in through their own plans against the `ImportanceIndex` API.

## Open questions / risks (survive the milestone — follow-ups, not blockers)

- **STILL OPEN — the scoring formula itself is unproven** (agent-spec §18.3). M1 landed a _shape_ and defaults; the real
  weights need iteration against David's home directory and the synthetic corpus (the `importance-tune` bin exists
  precisely for this). Risk: the defaults rank poorly and every consumer inherits a bad signal. Mitigation: tunable
  weights + explain + the tuning loop; do not let a consumer ship on unvalidated defaults. Not done in M1–M4 — a
  post-ship tuning pass against David's real tree.
- **STILL OPEN — `kMDItemLastUsedDate` sampling cost** (agent-spec §18.4) on large folders: per-item MDItem queries are
  slow, so the cap and sample strategy are guesses **still unmeasured** on a real home. Measure and record in
  `docs/notes/`; only local volumes sample (SMB has no Spotlight), so the cost is bounded to the boot disk.
- **The visit signal's frontend wiring** (Decision 3): the navigation-commit call is expected thin, but the frontend
  navigation path is non-trivial. If it balloons, the signal defers and the scorer runs one term short — acceptable,
  flagged.
- **Agent-spec D8 reconciliation** (Decision 2): this plan places weights in a separate `importance.db`, not an index
  column as D8 literally says. Intent-preserving, but the agent-spec author should confirm — flagged in the handoff.
- **Incremental-recompute ancestor fan-out** (Decision 5): a project marker appearing deep in a tree can raise many
  ancestors; bound the fan-out or a pathological change rescopes half the volume. Specify the ancestor-walk cap in M3.

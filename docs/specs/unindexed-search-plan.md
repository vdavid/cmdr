# Search that covers the folder you picked, indexed or not

**Status**: SPECCED, not started. **Owner**: David. **Date**: 2026-08-03.

Indexing stays optional. On a **local** drive, a scoped search that runs to completion returns the same files with or
without an index, only slower. Network volumes, unscoped search, and interrupted walks are deliberate exceptions, all
listed in § Accepted differences. The walk that fills the gap writes what it finds into the drive index, so a drive
converges toward instant through use.

Area docs to read first: `apps/desktop/src-tauri/src/search/CLAUDE.md` and `DETAILS.md`,
`crates/cmdr-index/src/indexing/CLAUDE.md`, `crates/cmdr-index/src/indexing/writer/CLAUDE.md` (coverage epochs,
canonical), `crates/cmdr-index/src/indexing/scanner/CLAUDE.md` (the guarded parallel walker),
`crates/cmdr-index/src/indexing/reconcile/DETAILS.md` (the serial subtree walk, the measurement that argues against it
here, and the caveat under that measurement), `crates/cmdr-index/src/indexing/lifecycle/CLAUDE.md` (the master switch
and the writer reservation), `apps/desktop/src/lib/query-ui/CLAUDE.md`.

## The problem

Search answers only from persisted indexes, and the dialog does not admit it.

- **Scoped to a path on an unindexed drive**: `volumes::ensure_volume` returns `VolumeLoad::NotIndexed`, the scope lands
  in `SearchResult::uncovered_scopes` (`search/execute.rs:154-166`), and nothing renders it. `uncoveredScopes` /
  `unresolvedScopes` sit in `apps/desktop/src/lib/tauri-commands/ipc-types.ts:93-96` with zero frontend readers; the
  only consumer anywhere is MCP (`mcp/executor/search.rs:119`). The user sees a plain "no results".
- **Worse, on a machine with no root index at all** (indexing declined, which is the population this plan exists for),
  the dialog never even asks. `query-runner.svelte.ts:161-167` returns before calling `runQuery()` when
  `!config.isIndexReady`, and `isIndexReady` flips only on `search-index-ready`, emitted only when
  `ensure_volume(ROOT_VOLUME_ID)` returns `Loaded` (`commands/search.rs:70-72`). So search is inert, and any
  coverage-honesty work is unreachable in the running app until that gate becomes per-target. M1 fixes it first.

`search/DETAILS.md:200-201` claims "the dialog and MCP render 'Cmdr hasn't indexed X yet'". The dialog half is false and
needs correcting regardless of this work.

Selection ("Select files…") was investigated alongside and is **not** part of this plan: its whole path reads the pane's
own listing cache, never the index. Its one real coupling is folder `recursiveSize` for size filters
(`SelectionDialog.svelte:254`), which M3a fixes incidentally.

## Accepted differences

David's governing principle: **the drive being indexed or not must not produce a behavioral difference, only a speed
difference.** This is the register of where this plan does not reach that, so the deferrals are visible rather than
buried. It is meant to be complete; anything found later belongs here.

1. **Unscoped search omits unindexed drives**, because `execute.rs:105` builds its target set from
   `all_indexed_volume_ids()`. Not fixed here; Open question 1. **And it degrades once this plan ships**: the first
   scoped live search of a USB drive creates `index-usb.db`, so every later unscoped search loads that arena, searches
   the 2% that was walked, and reports it as covered with no note, because an unscoped target carries
   `from_scope: false` and a non-`Loaded` one is skipped silently by design (`execute.rs:150-158`). M5 must therefore
   report a partially covered volume on an unscoped search rather than letting it pass as covered.
2. **Network volumes (SMB, MTP) get no live walk.** `network_scanner` has no subtree scan, and a live walk over the wire
   is minutes to hours. Open question 2. The walk cuts at the boundary rather than relying on the user's scope choice,
   see M3c.
3. **An interrupted walk is narrower.** Cancel, drive disconnect, app quit, and M9's MCP timeout all end a walk early,
   and each yields a strictly smaller result set than the indexed run. This is the difference users meet most often, so
   M6 labels the result list incomplete rather than letting it read as exhaustive.
4. **Unreadable subtrees are narrower.** The 32-failure give-up prune and M2's `known_unreadable` marker mean a walk
   without Full Disk Access covers less than an index built when it was granted. Honestly signalled, still a difference.
5. **Auto-apply works on indexed drives and not on uncovered ground** (Decision 7). Crossing into a frontier needs
   Enter.
6. **Ranking is not preserved.** Importance weights come from the index, so live-walked results rank by match quality
   and recency only. Results are capped, so at the boundary a different order is a different visible set; the completion
   re-rank (Decision 8) reorders what survived and does not recover what the cap dropped.
7. **Directory size filters behave differently.** A directory's size is overwritten after ranking from
   `dir_stats.recursive_logical_size` (`execute.rs:198`, `:207`, `:234-254`), outside the engine, so M4's factoring does
   not cover it. Over live-walked ground `dir_stats` is absent or a lower bound by construction, so a "folders over 100
   MB" filter returns a different set than the indexed run.
8. **A covered-but-stale subtree is trusted, not re-walked** (Decision 5). After a reconnect, an indexed drive can
   return a deleted file where an unindexed drive would be exact.
9. **The walk indexes what the user will never see in results.** `excludeSystemDirs` is match-time only (Decision 6), so
   a live search of `~/projects` walks and writes every `node_modules` and `.git` under it. That is the multiplier on "a
   search on an unindexed drive can take minutes".
10. **Search-written coverage expires on a 24-hour clock** (Decision 4), so a folder covered by yesterday's search is
    re-walked today.
11. **Media, OCR, and semantic search stay empty.** The walk writes the drive index only, never `media_index`, so photo
    and OCR search on a walked-but-unindexed drive returns nothing. Signalled by the existing
    `search.imageResults.notIndexed` copy, so no new work, but it is a difference.
12. **MCP and agents never trigger a walk** (Decision 10) unless they opt in explicitly.
13. **With the live-walk setting off** (Decision 9), unindexed drives revert to today's behavior wholesale.
14. **A walk that ran to completion can still be short.** The parallel walker abandons a directory that stops producing
    at `LOCAL_LIST_TIMEOUT`, and under rayon contention that left an index about 10% short on a measured run, skipping
    exactly the large directories people care about (`reconcile/DETAILS.md:25-31`,
    `docs/notes/indexing-benchmarks-2026-07-21.md`). Coverage stays honest, since an abandoned directory is never marked
    listed, so the frontier re-offers it next search. But the result list of that run is short without being labelled
    so, which is why M6 labels it alongside the interrupted states.

## Decisions

Each records the intent, so an implementer can adapt without re-litigating. Decisions 4, 9, and 13 need David's
sign-off; the rest record a choice or an existing invariant.

1. **Live walking is strictly the fallback for uncovered ground.** Covered subtrees are served from the index, always.
2. **The walk writes into `index-{volumeId}.db` through the normal writer.** Rejected alternative: an ephemeral
   in-memory `SearchIndex` arena for the dialog session. The index already models partial coverage, so an ephemeral
   structure would be a second, weaker copy of a solved problem, and it would throw away work on every dialog close.
   Durable data is also what Space-to-size needs.
3. **The scan is owned by `indexing/`, matching stays in `search/`, connected by a batched channel.** The
   `search/CLAUDE.md:23` must-know ("search is a one-way, read-only consumer of `indexing/`") is worth keeping, and
   `engine.rs` lives app-side while `cmdr-index` cannot depend on the app (`index-crate-isolation`). So the scoped scan
   emits **batches** of discovered entries over a bounded channel and search matches them on its side: one crossing per
   batch, not per entry, and no matcher inside `cmdr-index`. The scan has two callers, search (this plan) and
   Space-to-size (next).
4. **Search-written coverage expires after 24 hours.** _(Needs David.)_ This closes a hole the plan otherwise creates: a
   walk stamps coverage on a volume that never went through the indexing lifecycle, so no watcher and no reconciler owns
   its freshness, and Decision 5 forbids re-walking stale coverage. Without an expiry, every later search of that folder
   is served from a snapshot that can never update, forever. `listed_epoch` is a monotonic counter, not a clock, and no
   timestamp column exists (`store/mod.rs:571-603`), so this needs a persisted `listed_at`, added in M2 with the rest of
   the schema.
5. **A covered-but-stale subtree is trusted, not re-walked.** `0 < min_subtree_epoch < current_epoch` means exact but
   computed at an older epoch, typically after a reconnect. `reconcile/` owns freshness for volumes that have a
   lifecycle; Decision 4 covers the ones that do not.
6. **The walk indexes everything the normal scanner would; `excludeSystemDirs` is a match-time filter only.** The
   scanner is **not** a consumer of `SYSTEM_DIR_EXCLUDES`: `should_exclude` (`scanner/exclusions.rs:428-490`) uses
   `JUNK_BASENAMES`, `PSEUDO_FS_BASENAMES`, and `EXCLUDED_PREFIXES` only. The real consumers are `search/engine.rs:176`,
   `importance/classify.rs:49`, and the tooltip command `commands/search.rs:145`. The rationale is size and relevance,
   not principle: the `SYSTEM_DIR_EXCLUDES` tier is large and sits under folders people search, so skipping it at walk
   time would stamp coverage on parents whose `dir_stats` are badly short, breaking Space-to-size. The structural tier
   is small and never searched, so the same bounded inaccuracy is accepted there exactly as today's boot scan accepts
   it. The doc comment at `exclusions.rs:21-24` naming "the walk" as the third consumer is wrong and should name the
   tooltip command.
7. **A live walk never auto-applies.** `search.autoApply` is on by default with a 1,000 ms debounce; as a live walk that
   would mean a user typing six characters starts and abandons five multi-minute walks. The dialog already carries this
   precedent for the same reason ("AI mode never auto-applies (cost)", `query-ui/CLAUDE.md`). Index-backed results still
   auto-apply.
8. **Live results append in arrival order and re-rank once on completion**, with the cursor kept on its row by path
   identity, and the re-rank suppressed once the user has moved the cursor.
9. **A live walk is a setting, default on.** _(Needs David.)_ Someone who does not want a search to spin a disk gets to
   turn it off, and it is the kill switch if this changes search's cost profile in a way people dislike.
10. **MCP and agent searches never trigger a walk by default.** They get index-backed results plus the typed coverage
    signal, with an explicit opt-in argument for an agent that wants one.
11. **A superseded query does not cancel the walk, and its in-flight batches are dropped.** Walking is coverage work;
    matching is query work, and Decision 3 separates them. Refining a query keeps the walk running and drops the batches
    already in flight for the old query; the ground the walk had already covered is recovered from the **index**, not
    from a replay buffer, because Decision 12 makes those rows visible to the very next query. So a user who fixes a
    typo after eight minutes loses no walking and no results.
12. **A walk invalidates its volume's arena; the coverage answer is trusted only against a matching arena.** The arena
    `ensure_volume` serves (`volumes.rs:476-518`) is a snapshot, and `search/CLAUDE.md` is explicit that a stale one is
    served rather than rebuilt. So a walk that writes rows marks its volume's arena dirty, and the next query that would
    prune on covered ground reloads first or treats the subtree as uncovered until the reload lands. Rejected: a
    `current_epoch` plus max-`listed_epoch` token, which cannot move at all, because a walk stamps `listed_epoch` with
    the current epoch and never bumps it (`reconcile/reconciler.rs:902`, `writer/mod.rs:1401-1410`) and the arena never
    reads `listed_epoch` anyway (`search/index.rs:74`). Also rejected as the sole signal: `Index::search_generation`,
    which is process-global, fed only by root's writer, stamped `0` for every non-root volume, and hardcoded root-only
    in `is_stale()` (`volumes.rs:283`), while ticking about 5.7 times a second on an idle boot disk
    (`volumes.rs:289-293`).
13. **A user-initiated scan passes through the master switch.** _(Needs David.)_ `lifecycle/master.rs:6` documents that
    with the master switch off, "nothing indexes, anywhere", which is about _background_ work; a scan someone asked for
    by searching is not background work. It genuinely blocks today at three code sites (`handle/mod.rs:163-167`,
    `lifecycle/state.rs:555-558`, `transports/smb/index.rs:220` via `drive_index_should_run`) and is restated as an
    invariant in four docs (`lifecycle/CLAUDE.md`, `lifecycle/DETAILS.md:383`, `transports/CLAUDE.md:23`, `master.rs`).
    The per-drive `user_disabled` veto is a separate consent: Open question 4.
14. **Progress is directories scanned plus the current path. No percentage, no ETA.** The total is unknown by
    definition, and a fabricated ETA violates honest progress (`docs/design-principles.md`).

## The core mechanism: a coverage-pruned walk

- `listed_epoch` is stamped per directory when its direct contents were read. Zero means unknown, and the scanner keeps
  it honest: an abandoned or give-up-pruned directory is never marked listed (`scanner/CLAUDE.md` § "Honest-stale, never
  false-complete").
- `min_subtree_epoch` is the **zero-absorbing min** of a directory's own `listed_epoch` and every child directory's
  `min_subtree_epoch`. Canonical implementation: `store::recompute_min_subtree_epoch`
  (`indexing/store/dir_stats.rs:184-212`), which also `COALESCE`s a missing `dir_stats` row to zero; `writer/delta.rs`
  is the ancestor-walk that calls it.

### The descent rule

The frontier needs **both** epoch fields plus two new columns. Using `min_subtree_epoch` alone degenerates: because the
min absorbs zero upward, one uncovered directory anywhere forces zero on every ancestor including the scope root, so
"the shallowest node at zero" is always the scope root and the frontier becomes "walk everything".

Descending from the scope root:

- `min_subtree_epoch > 0` and not search-written-and-expired → **covered**. Serve from the index, do not descend.
- `min_subtree_epoch > 0`, search-written, `listed_at` older than the TTL → **frontier** (Decision 4).
- `min_subtree_epoch == 0 && listed_epoch > 0` → **partially covered**. This directory was read, something below it was
  not. Descend.
- `listed_epoch == 0 && known_unreadable` → **skip**. A directory the walk has tried and cannot read. Not frontier, and
  reported to the user rather than silently dropped.
- `listed_epoch == 0` → **frontier**. Cut here and hand it to the walk.
- **No `entries` row at all** (a cold volume, or a path the index has never seen; `resolve_path` returns `None`,
  `store/mod.rs:174-176`) → the scope root itself is the whole frontier.

Because `recompute_min_subtree_epoch` coalesces a missing `dir_stats` row to zero, `min_subtree_epoch > 0` implies
`listed_epoch > 0`, so the cases are disjoint and exhaustive. M2's proptest checks that premise before relying on it.

The `known_unreadable` marker and the `listed_at` timestamp are both schema additions and both belong in M2 with the
rest of the data model. Without the marker, a permission-denied subtree stays `listed_epoch = 0` forever and re-enters
the frontier on **every** subsequent search, a permanent repeating slow path with no user signal. Without the timestamp,
M3d would have to change M2's query and its proptest after the fact.

### Exclusions are a live-walk concern only

A policy-excluded child is skipped at `crates/cmdr-index/src/indexing/scanner/insert_visitor.rs:146-147`, so it gets
**no `entries` row at all**, and `recompute_min_subtree_epoch`'s child scan is
`WHERE c.parent_id = ?1 AND c.is_directory = 1` (`store/dir_stats.rs:196-206`), which cannot see a row that does not
exist. The live paths gate identically (`reconcile/reconciler.rs:1159`, `:1338`,
`crates/cmdr-index/src/indexing/watch/event_loop/verification.rs:384`). So excluded directories drive nothing to zero
and the index-side frontier query needs no exclusion logic; the live walk needs all of it, applying the structural tier
exactly as a volume-root scan does, or a scoped search of `/` walks `/private/var` and `/proc`.

Two limits on that, both real:

- **Policy drift.** `EXCLUDED_PREFIXES` and `JUNK_BASENAMES` are compile-time constants, and nothing stamps a policy
  version in `meta` (`store/mod.rs:598-602`). If a release _removes_ an entry, the previously excluded subtrees still
  have no rows, their parents still read as covered, and they become permanently invisible to search with no re-walk
  trigger. Stamp an exclusion-policy version alongside `current_epoch` and treat a mismatch as coverage unknown. M2.
- **E2E.** Under `ExclusionTier::BootDisk`, `should_exclude` returns `true` for everything outside `CMDR_E2E_START_PATH`
  when it is set (`exclusions.rs:434-436`), so the E2E root index contains only the fixture subtree and
  `min_subtree_epoch("/")` reads as fully covered. M6's and M7's live-search E2Es need a fixture rooted so a frontier is
  reachable.

`min_subtree_epoch("/")` can still be zero on a complete boot index, but via honest-stale gaps (abandoned reads,
give-up-pruned subtrees), not exclusions. Those gaps are small, so a root search on an indexed drive walks little.

### Two properties

- **No deduplication as a mechanism.** The tree is partitioned, so each entry is produced by exactly one source. The
  partition is definitional rather than enumerated, which is why M2 returns the frontier only and never a list of
  covered subtrees: the arena already holds exactly the covered rows, so running the engine over the scope unfiltered
  yields the covered half for free. A bounded path hash set stays as insurance against the race where a file is indexed
  between the frontier query and the walk reaching it; bound it at the result cap, not at the walk size.
- **Convergence.** Every search shrinks the uncovered frontier durably, so repeated searching over an area trends toward
  instant, and a refined query walks less than the first one did. **This does not hold today**, which is what M3a
  builds: a cancelled scan currently stamps zero coverage.

### Forward compatibility with content search

Content search will ask this question in a second dimension. Make coverage **per dimension** rather than one flag: when
content indexing lands, give it a `content_epoch` sibling to `listed_epoch`, propagated with the same zero-absorbing
min. The walk stages then fall out without redesign: path-covered and content-covered lands first, path-covered but
content-uncovered needs a read pass only, path-uncovered needs a walk then a read. Not implemented here, but M2's
coverage API must not assume a single dimension.

## Sequencing

- **M1 is the head** and is valuable alone: it converts today's silent wrong answer into an honest one and unblocks
  search on an index-less machine.
- **M2 is independent of M1** and can run in parallel. It is the performance hinge, owns all three schema additions
  (`known_unreadable`, `listed_at`, the exclusion-policy version), and carries a measured exit criterion.
- **M4 is independent of everything before it** (a pure refactor) and can run any time before M5.
- **M3a → M3b → M3c → M3d** are sequential among themselves and all depend on M2's data model. M3d additionally depends
  on M2 having added `listed_at`; it must not need a schema change of its own.
- **M5 depends on M2, M3a-c, and M4.** M6 depends on M5, and inherits M2's `known_unreadable` marker for the unreadable
  signal it ships. **M7 depends on M6. M8 depends on M2's marker and M5's signal. M9 depends on M5. M10 depends on
  everything it measures.**
- **M0 is deliberately last among the user-visible changes** and is blocked on Open question 3. Shipped early it is
  net-negative: it narrows the default scope for indexed users while giving nothing back.

## Milestones

`pnpm check -q --fast` while iterating, the scoped checks named per milestone at each milestone's end,
`pnpm check --include-slow` before wrapping.

**Copy rule for every milestone**: user-facing strings are drafted in the milestone and reviewed by David before merge
(`AGENTS.md` principle 6). Every new key needs its `@key` translator description, and the translation pass follows
`docs/guides/i18n-translation.md`. No milestone is done with untranslated keys shipped.

**Definition of done for the whole effort**: on a local drive with no index, a scoped search that runs to completion
returns the same result set as the same search on the same drive fully indexed, excepting order, Accepted difference 7
(directory size filters), and Accepted difference 14 (directories the walker abandoned, which stay in the frontier and
are labelled), with the first batch painted within two seconds.

### M1. Search asks the question, and answers it honestly

- **Make `isIndexReady` per-target.** `query-runner.svelte.ts:161-167` returns before `runQuery()` when the ROOT arena
  is not loaded, so on an index-less machine search never runs at all and every later milestone is unreachable. The gate
  becomes "is this search's target ready", not "is root loaded". Related: `search-index-ready`
  (`commands/search.rs:70-72`) currently means "root's arena loaded" and needs to say which volume.
- Note that `queryUi.results.indexNotReady` is **not** the first-scan gate: it renders on `!isIndexAvailable`
  (`query-ui/QueryResults.svelte:391-394`), reached only from the `catch` of `prepareSearchIndex()`
  (`SearchDialog.svelte:496-498`), and `prepare_search_index` returns `Ok(ready: false)` during a first scan
  (`commands/search.rs:78-82`). It is a backend-unavailable state. Do not "fix" it here.
- Render `uncoveredScopes` and `unresolvedScopes` with distinct copy. Branch on emptiness, never on message text
  (`.claude/rules/no-string-matching.md`).
- **`unresolvedScopes` copy is provisional and says so.** `search/DETAILS.md:203-204` defines it as "the volume IS
  indexed but the specific path isn't in it", rendered as "couldn't find that path". On a partially indexed volume, a
  real folder someone is standing in lands in exactly that bucket (`execute.rs:172-175`), so M1 would say a folder they
  can see does not exist. Either distinguish "not walked yet" from "genuinely not found" here, or state that M5
  re-classifies it.
- The CTA routes to the **per-drive** flow, not the global toggle. `settings.indexing.askForEachDrive`,
  `settings.indexing.silencedDrives`, and `settings.indexing.reEnableNotifications` already exist for this; the CTA
  respects a silenced drive and offers a dismissal that sticks, or it nags on every search of that drive forever. Two
  variants needed: local-unindexed ("This drive isn't indexed, so this search reads it folder by folder. Index it to
  make searches here instant.") and a network drive, which the product does not want to push toward indexing. Draft for
  David.
- Later states (`searching live`, live count, Cancel) arrive in M6 when they can be reached. Shipping translated copy
  for unreachable states across the locale set costs twice if they change.
- Fix `search/DETAILS.md` § Honesty.

Tests, **test-first**: the note clears on the next run (a real regression anchor), and a search runs on a machine with
no root index (the gate fix, currently impossible). Written after: each field renders its own distinct note; a11y audit;
i18n parity. Docs: `query-ui/CLAUDE.md`, `search/CLAUDE.md`, `search/DETAILS.md`. Checks: `pnpm check svelte`,
`pnpm check i18n`, `pnpm check desktop`.

### M2. Coverage map, and the data model it needs

Read-side plus schema. The most testable unit and the performance hinge.

- The coverage query: given a scope path, return **the frontier only**, plus the arena identity it was computed against
  (Decision 12). By the descent rule above. No exclusion logic.
- **Three schema additions, all here**: `known_unreadable` on directories, `listed_at` for Decision 4's expiry, and an
  exclusion-policy version in `meta`.
- Exposed on the `Index` handle. Adding a `pub` there is a design act (`handle/CLAUDE.md`); record it in
  `handle/DETAILS.md` § "The public surface".
- **Precondition, Open question 5**: `index-crate-isolation` ceilings are set with no headroom by design
  (`RootPromises: 44`, `HandleMethods: 35`, `scripts/check/checks/index-crate-isolation.go:85-86`). There is no spare
  slot: `handle/CLAUDE.md:11` describes the surface as 34 items, and `index-crate-isolation.go:80-82` explains the gap,
  "`HandleMethods` is 35 rather than the audit's headline 34 because this count includes `Index::builder`, the
  constructor". So the count already sits at its ceiling. This plan adds at least three handle methods (the coverage
  query, M3b's scoped scan, and an epoch read: `IndexStore::read_current_epoch` at `store/meta.rs:62` is not on the
  handle), plus the coverage-answer type and any new error enum as `RootPromises` items if re-exported from `lib.rs`. So
  **two ceilings** need David's OK, not one.
- **Exit criterion, measured**: a recorded note in `docs/notes/` over a real 611,699-folder root index, budget under 50
  ms warm for the frontier query. `dir_stats` has `entry_id INTEGER PRIMARY KEY` and no other index
  (`store/mod.rs:588-597`); `idx_parent_name_folded ON entries (parent_id, name_folded)` (`:586`) gives the descent a
  leading-column seek but is not covering (`listed_epoch` and `is_directory` are not in it), so each child costs a
  main-table fetch plus a `dir_stats` PK lookup. If the budget misses, add the index here rather than discovering it in
  M5.

Tests, **test-first**: a proptest that the frontier partitions the subtree (every path produced exactly once by exactly
one of covered, frontier, or known-unreadable). Confirm first that every listed directory gets a `dir_stats` row, or the
partition premise is false. Written after: a single uncovered leaf yields the leaf, not the root; a cold volume yields
the scope root; an honest-stale gap on an otherwise complete boot index yields only that gap; an expired search-written
subtree returns to the frontier; a fully covered scope returns an empty frontier **with a coverage assertion that every
directory was considered**, or it passes on a no-op (`docs/testing.md`). `cargo mutants` over the new module before the
milestone closes. Docs: `indexing/read/DETAILS.md`, `store/DETAILS.md` (the three schema additions),
`handle/DETAILS.md`, the benchmark note. Checks: `pnpm check rust`, `pnpm check rust-tests`.

### M3a. A scoped scan that survives cancellation

The convergence property, which does not exist today. Start by choosing the primitive, with a measurement.

**The two candidates, and why the choice is not obvious.**

- `scanner::scan_subtree` is the guarded **parallel** walker. Two defects: a cancelled scan stamps zero coverage
  (`scanner/mod.rs:547-552` returns `Vec::new()` for `listed_ids` when cancelled, and marks reach the writer only
  through `send_marks`, defined at `mod.rs:76` and called at `:349` and `:402`), and every non-volume-root scan sends
  `DeleteDescendantsById(root_id)` before walking (`scanner/mod.rs:478-483`).
- `reconcile::reconcile_subtree` is **serial** and already has both properties, self-documented: it compares children by
  name and writes only differences, is "safe to interrupt at any point: the DB is never in a partially-deleted state"
  (`reconciler.rs:873-878`), and its `MarkDirsListed` + `PropagateMinSubtreeEpoch` block sits after the cancel `break`
  (break at `reconciler.rs:1038-1039`, block at `:1104-1117`), so an interrupted reconcile does leave durable partial
  coverage.

**The frontier is by definition all-new ground**, so this workload is a bulk add, never an incremental diff. That is the
case the tree has already measured: `reconcile/DETAILS.md:20-21` records 1,309 s for the serial reconcile against 68.1 s
for the parallel scan on the same tree, and `:44-45` records the standing decision that "reconcile's serial per-dir walk
over an add-everything delta is dramatically slower than a parallel bulk rebuild". `reconcile_subtree`'s own comment
calls it "the LIVE small-scope fill path" and says the full-rescan path deliberately does not use it
(`reconciler.rs:880-884`). On a cold volume it does nothing at all: `space.resolve_abs` misses, the parent misses, and
it returns an escalation anchor with zero work (`reconciler.rs:952-960`).

**Read the caveat the source doc puts under that comparison before leaning on it.** `reconcile/DETAILS.md:25-31` opens
with "Before trusting that speed comparison, read `docs/notes/indexing-benchmarks-2026-07-21.md`": on an idle machine
the numbers are 52.7 s against 476.9 s, about 9× rather than 19×, and the parallel scan buys part of its speed by
abandoning directories under rayon contention, leaving that run about 10% short (6,001,637 rows against 6,663,048) in
exactly the large subtrees whose sizes matter most. That is Accepted difference 14, not a reason to reject the parallel
walker, but M3a must decide with its own measurement on a representative frontier rather than either published number.

**So the expected answer is the parallel walker with the two defects fixed**, confirmed by that measurement.

Fixing the parallel walker:

- **Incremental marking, ordered correctly.** Not "mark as directories complete": `writer/mod.rs:314-324` warns that "a
  per-dir emit could update a row still pending in an unflushed batch, leaving it `listed_epoch=0` forever", because a
  directory's own row is created by its parent's `visit_dir` and sits in `InsertVisitor.batch` until `batch_size`
  (`insert_visitor.rs:103-113`, `scan_subtree` passes 2,000 at `mod.rs:388`) and `mark_dirs_listed` is a PK `UPDATE`
  that silently updates zero rows. Drain accumulated `listed_ids` immediately **after** each successful
  `InsertEntriesV2` send in `send_entries`, so the single in-order writer guarantees row-before-mark.
- **The destructive delete may be close to a no-op here, and that needs confirming, not assuming.** A scan root is a
  frontier node, so the descent rule gives it `listed_epoch == 0`. Whether it can carry a _listed_ descendant is a
  separate question resting on a violable invariant (a row exists only because its parent's `visit_dir` succeeded, and a
  successful `visit_dir` eventually stamps `listed_epoch`). The counter-case to construct: a cancelled scan leaves
  unlisted rows under an unlisted frontier node, and the FSEvents verification path
  (`watch/event_loop/verification.rs:382-386` into `scanner::scan_subtree`) then lists one of its descendants, giving a
  frontier node with a listed descendant that `DeleteDescendantsById` would remove. If that case is reachable, the
  delete must go, and note the option that is closed: gating it behind a rebuild flag does not work, because `run_scan`
  inserts with explicit ids (`store/entries.rs:453-455`) against `CREATE UNIQUE INDEX idx_parent_name_folded`
  (`store/mod.rs:586`), so a re-scan without the delete violates uniqueness on every pre-existing child. **Pin the
  answer with a test before building on it.**
- **Batched emit.** Neither primitive emits discovered entries to a channel today, and `ReconcileSummary` has no
  `cancelled` field. Decision 3's channel and M5's terminal states both need additions here.

**Writer cancel semantics**: queued `MarkDirsListed` messages are flushed, not dropped, or convergence is a lie. Esc
returns to the user immediately; the flush completes behind it.

Tests, **test-first**: cancelling mid-scan leaves durable partial coverage with correct lower-bound aggregates. This is
David's case: given `/A/B/C` with 10 files each, a scan that covers A and B but is cancelled before C leaves `≥` sizes
on A, B, and their ancestors, and `<dir>` on C (`recursive_size_complete = false` with a non-zero size renders
`'lower-bound'`, and `complete === false && size === 0` renders the `'dir'` placeholder,
`apps/desktop/src/lib/file-explorer/views/full-list-utils.ts:391`). Second red test: a frontier-rooted scan that is
cancelled never removes a row it did not write. Docs: `scanner/CLAUDE.md` + `DETAILS.md`, `writer/DETAILS.md`,
`reconcile/DETAILS.md` (record why the serial path was not chosen), the measurement note. Checks: `pnpm check rust`,
`pnpm check rust-tests`.

### M3b. Cold-volume bootstrap

The load-bearing gap: today nothing can scan a drive that was never indexed. Both candidate primitives fail on a cold
volume, for different reasons: `scan_subtree` opens a _read_ connection, calls `read_current_epoch` rather than seeding
one, and resolves its root via `resolve_scan_root(conn, path, false)`, which needs the entry and its whole ancestor
chain to exist and otherwise returns `QueryReturnedNoRows` (`store/mod.rs:200`); `reconcile_subtree` returns an
escalation anchor with zero work (`reconciler.rs:951-958`). Both also need an `IndexWriter`, which exists only inside an
`IndexManager`, created only by `start_indexing_for` (`lifecycle/state.rs:545+`) for an active volume. On an unindexed
drive there is no DB, no root sentinel, no epoch, no writer, and no entry to resolve.

Deliver: create the DB, seed the root sentinel and epoch, stand up a writer, and materialize the ancestor chain from the
volume root to the scan root. This is new `lifecycle/` plus `store/` design work.

Tests: bootstrap on a cold `InMemoryVolume` produces a queryable index with the ancestor chain and nothing claiming
coverage it does not have. Written after. Docs: `lifecycle/CLAUDE.md` + `DETAILS.md`, `store/DETAILS.md`,
`handle/DETAILS.md`. Checks: `pnpm check rust`, `pnpm check rust-tests`.

### M3c. Policy: exclusions, the master switch, boundaries, and one writer

- **Exclusion mode.** Structural exclusions on for search-driven scans, layered as an on/off switch over the
  kind-derived `ExclusionScope`, never a second scope source (`scanner/CLAUDE.md`: "`should_exclude` derives scope from
  the volume KIND, never `is_volume_root`"). The existing callers do **not** depend on today's no-exclusions default:
  `reconcile/verifier.rs:412-415` and `watch/event_loop/verification.rs:105-108` both apply `should_exclude` to the scan
  root themselves. What they do not filter is children discovered inside, so enabling descendant exclusions makes them
  consistent with their own root gate.
- **Cut at foreign volumes, by two different probes.** Local-only is not a property of the scope the user picked: a
  scoped walk of `/`, `/Volumes`, or a home folder with a NAS mounted under it crosses into a network volume. Real
  mounts need a device check, and the batched macOS read does not carry one today (it requests
  `ATTR_CMN_RETURNED_ATTRS | NAME | OBJTYPE | MODTIME | FILEID` plus file attrs, no `ATTR_CMN_DEVID`,
  `scanner/walker/bulk_read.rs:126-129`), so this needs a per-directory `stat` or a new attribute. File Provider domains
  need a different probe entirely: "A File Provider domain (Dropbox, Google Drive, iCloud Drive, MacDroid, …) is **NOT**
  a mount point: its root reports the same `st_dev` as `$HOME` and never appears in `mount`, so the usual
  volume-boundary detectors are blind to it" (`scanner/file_provider.rs:7-9`). The answer already exists as
  `file_provider::domain_id_for_dir`, wired as `RootProbes::is_domain_root` (`scanner/exclusions.rs:140`, `:152-155`)
  but used today only to decide whether a `proc`/`sys`/`dev` child sits at a volume root (`exclusions.rs:382-390`). Name
  both probes and what the walk does when it hits either: cut, and report the subtree as uncovered so Accepted
  difference 2 stays true.
- **Master-switch carve-out** (Decision 13) at the three code sites and the four docs that restate the invariant.
- **One writer per DB.** `lifecycle/state.rs:565-571` spells out the hazard: two writers race on id counters and
  accumulator maps, "producing PK collisions and inflated `dir_stats`". The scoped scan reuses the volume's existing
  `IndexWriter` when one is running and takes the lock-first reservation otherwise. A search over a volume mid-full-scan
  does not walk at all, because the scan already covers it. Two live walks with overlapping frontiers coalesce onto one
  walk rather than double-writing.

Tests, **test-first**: a scan runs with the master switch off (Decision 13's carve-out is the invariant a future agent
will "fix" as a bug). Written after: both boundary probes; exclusion mode; writer reuse under a running scan; two
overlapping walks coalesce. Docs: the four master-switch docs, `scanner/CLAUDE.md` + `DETAILS.md`, `exclusions.rs:21-24`
comment fix. Checks: `pnpm check rust`, `pnpm check rust-tests`.

### M3d. Expiry and retention

- **Expiry.** Decision 4's 24-hour TTL on search-written coverage, over M2's `listed_at`: a subtree past it re-enters
  the frontier. The descent rule already carries the case, so this is enforcement, not a query change.
- **Retention, corrected on three counts.** The existing cap is 32 external index DBs (`MAX_EXTERNAL_INDEX_DBS`,
  `resources/retention.rs:42`), evicting **oldest-by-mtime**, and the module states that choice deliberately: "This is
  deliberately not a size budget or an access-time LRU" (`retention.rs:27-30`). Do not silently redefine it. More
  importantly the cap **excludes root** ("Never evict `root`… it's excluded from candidates regardless of mtime",
  `retention.rs:16-18`), and this plan's headline population is a machine with no root index whose walks write
  `index-root.db`. So nothing caps the data this feature mainly produces. Decide and record: either bring search-written
  root coverage under a size budget, or accept it and say so in `resources/DETAILS.md`. Also,
  `enforce_external_index_cap()` is called only from the three transports' index-start paths
  (`transports/local_external/index.rs:139`, `transports/smb/index.rs:198`, `transports/mtp/index.rs:55`), so a walk
  that reserves a writer directly (M3c) never triggers it and must call it.
- Per-drive "Clear index" drops search-written coverage.

Tests: expiry returns a stale subtree to the frontier; a walk-created DB is counted by the cap. Written after. Docs:
`resources/DETAILS.md`, `lib/settings/sections/DriveIndexingSection.svelte` docs. Checks: `pnpm check rust`,
`pnpm check desktop`.

### M4. Compiled query: one matcher, two evaluators

A pure refactor with no behavior change, split out so the "matching must not fork" invariant is independently
verifiable. **It stays app-side in `search/`** per Decision 3; the walk delivers batches, not callbacks.

- Factor the pattern compile plus the size, date, and type predicates out of `engine::search_ranked` into a compiled
  query value that both the arena scan and a batch of walked entries can evaluate. `engine.rs` stays pure and I/O-free.
- **Directory size filters stay outside this**, per Accepted difference 7: a directory's size is overwritten after
  ranking from `dir_stats` (`execute.rs:198`, `:207`, `:234-254`). Say so in the module docs so nobody later assumes one
  matcher covers it.
- Add an unconditional broad-query guard for the live path. The existing one cannot be reused: `engine.rs:258` keys on
  `index.entries.len() > 100_000`, and an unindexed volume's arena has zero entries, so it never fires.

Tests: characterization tests that the refactor is byte-identical on existing queries, with the existing engine tests as
the oracle. Written after, except the guard, which is **test-first**. Checks: `pnpm check rust`,
`pnpm check rust-tests`.

### M5. Coverage-pruned streaming search

- `search/execute.rs` gains the fallback: query the coverage map (M2), run the pure engine over the scope unfiltered
  (which yields exactly the covered half), drive a scoped scan (M3a-c) over the frontier, and match its batches with the
  compiled query (M4).
- **Invalidate and re-check the arena** per Decision 12. A coverage answer is honored only against the arena identity it
  was computed for; a walk that wrote rows marks the arena dirty, and the next query reloads or treats the subtree as
  uncovered until the reload lands. Without this, the second keystroke after a walk returns fewer results than the
  first, with no signal.
- **An unscoped search must not launder a partially walked volume as covered** (Accepted difference 1). Once a walk has
  created `index-{id}.db`, `all_indexed_volume_ids()` picks it up and `from_scope: false` makes the silent-skip path
  inapplicable, so the volume needs a partial-coverage signal of its own rather than passing as complete.
- Streaming events follow `file_system/listing/streaming.rs` (`-progress` / `-complete` / `-error` / `-cancelled`), with
  a run id so batches from a superseded query are dropped while the walk continues (Decision 11). **Batch at 100 rows or
  100 ms, whichever comes first.**
- **The unreadable signal ships here**, stamping M2's `known_unreadable` marker: the walker's give-up budget (32
  consecutive failed reads, sticky per parent, `scanner/walker/mod.rs:214`) prunes a subtree it cannot read, and that
  must surface as a typed "could not read" rather than silence. Without it, a search of `~/Documents` on a machine
  without Full Disk Access returns a confident empty answer and re-walks from scratch every time.
- **Terminal states, all defined**: completed, cancelled, drive disconnected mid-walk (`ScanError::RootUnlistable` is
  volume-root-only, gated at `scanner/mod.rs:543-544`, so a subtree scan needs its own signal, and rows already queued
  for a vanished volume must drain without claiming coverage), and app quit mid-walk through the `resources/`
  stop-hooks. None may leave coverage claiming completeness.
- **Walk lifetime**: a walk outlives the dialog only through "Open in pane" (M7). Otherwise dialog close cancels it, and
  the walk's lifetime is never tied to the arena idle-drop (`volumes.rs:530`).
- Hitting the result cap does **not** stop the walk: convergence is the payoff, and a stopped walk freezes "N so far"
  forever. The progress state says results are capped while coverage continues.
- Every filesystem call goes through the timeout tiers in `commands/CLAUDE.md`.

Tests, **test-first**: a search across a half-covered scope returns the union exactly once; a query refined mid-walk
drops the old batches, keeps the walk, and recovers the already-walked ground from the index (Decisions 11 and 12
together, and the case that silently loses results if either is wrong). Written after: unreadable folders reported
rather than swallowed; a partially walked volume is not laundered as covered in an unscoped search; `excludeSystemDirs`
absent by default and present when off; cancellation stops the walk promptly; the disconnect terminal state. Plus a
contract test for the new IPC event family (`docs/testing.md` § "When you add X, also add Y"). Docs: `search/CLAUDE.md`
(the one-way-consumer must-know needs its nuance), `search/DETAILS.md`, `indexing/CLAUDE.md`. Checks: `pnpm check rust`,
`pnpm check rust-tests`.

### M6. Streaming results in the query UI

- `query-runner.svelte.ts:169-172` awaits one `runQuery()` then calls `setResults` / `setTotalCount` /
  `setCursorIndex(0)`. Add an incremental path there: append, a generation guard, and a cursor held by path identity.
- **The streaming source belongs to `query-runner.svelte.ts`, not `QueryDialog`.** The invariant is real
  (`query-ui/CLAUDE.md:18-20`: consumer callbacks only RETURN data and never write `results` / `totalCount` /
  `cursorIndex`), but QueryDialog is "wiring and layout only" with logic in four tested siblings
  (`query-ui/CLAUDE.md:7-9`), and Selection never streams. So the runner, which already owns those setters, gains an
  optional streaming source supplied through `QueryDialogConfig`.
- Three phases get three honest states: resolving coverage, reading the index, walking. The walking state carries the
  live match count **and** directories scanned plus current path (Decision 14).
- **Cancel's end state is defined**: partial results stay on screen, the list is labeled incomplete, the count resolves
  to what was found before stopping, and the coverage note says the walk was cancelled rather than exhausted. The same
  labelling covers a walk that finished but abandoned directories (Accepted difference 14). Same labelling for the
  disconnect and quit terminal states (Accepted difference 3).
- Cancel is visible with its shortcut shown (`docs/design-principles.md`). **Escape means two things**: the first
  cancels a running walk, the second closes the dialog. QueryDialog already owns Escape via `ownsKeyboard`.
- **Count-only becomes honest here**, not in a docs milestone: `count_only` renders "This search yields N results"
  (`queryUi.results.countOnly.sentence`), which over a partially walked drive is a confident lie. It becomes "N so far"
  while walking, exact on completion.
- **Throttle the `aria-live` announcement** to at most every two seconds plus once on completion. The status bar stays
  mounted for `aria-live` and a count updating every 100 ms is an announcement flood; an axe audit will not catch it.
- Enter starts a live walk; auto-apply does not (Decision 7), and the run button voices it.
- Live results respect the existing cap with the capped state voiced (`search.snapshot.cappedLabel`).

Tests, **test-first**: the generation guard (a stale batch landing in a new run is the silent-corruption case). Written
after: append, cursor stability across batches and across the completion re-rank, the Escape two-step, the cancelled end
state, the aria-live throttle, count-only wording. Plus an E2E for the live-search flow, with a fixture that leaves a
reachable frontier under `CMDR_E2E_START_PATH`. Docs: `query-ui/CLAUDE.md` + `DETAILS.md`. Checks: `pnpm check svelte`,
`pnpm check desktop`, `pnpm check desktop-e2e-playwright`.

### M7. Keep the walk running after "Open in pane"

- "Open in pane" promotes to the `search-results://` snapshot and leaves the walk running. New rows append as found;
  snapshot mutations need the `mutationTick` bump or `SearchResultsView` will not re-render (`search/CLAUDE.md`).
- A toast reports the walk is still running, carrying the match count and directories scanned, with a **Reopen search**
  button. Draft copy for David.
- The dialog preserves its state (`query-ui/CLAUDE.md`: state survives unmount by design, `⌘N` is the only sanctioned
  reset) and reopens in searching mode while the walk is live.
- On completion, swap to an auto-hiding completion toast.

Tests: unit tests for the toast state machine (running → completed → auto-hide, plus reopen). The E2E that a snapshot
pane grows mid-walk needs a deterministic slow walk: add a soft test hook in the style of `CMDR_E2E_COPY_THROTTLE_MS`,
or it lands flaky and needs an allowlist entry David has to approve. Written after. Docs: `lib/search/CLAUDE.md` +
`DETAILS.md`. Checks: `pnpm check desktop`, `pnpm check desktop-e2e-playwright`.

### M8. Full Disk Access route

What is left of unreadable-folder handling once M2 owns the marker and M5 owns the signal: when the cause is missing
Full Disk Access, route into the existing prompt rather than leaving someone with an honest but unhelpful count.

Tests: the FDA route appears when the denial pattern matches and not otherwise. Written after. Docs:
`lib/onboarding/CLAUDE.md`. Checks: `pnpm check desktop`.

### M9. MCP coverage signal and opt-in walk

- Per Decision 10, agent searches do not walk. They return index-backed results plus the typed coverage signal.
- An explicit opt-in argument enables a walk, with a timeout defaulting to 20 seconds and `0` meaning unlimited. On
  expiry, return what was covered plus the coverage signal, never a silent partial.

Tests: executor unit tests for no-walk default, opt-in, and the three timeout values. Written after. Docs: the MCP tool
docs, `search/DETAILS.md`. Checks: `pnpm check rust`.

### M0. Default scope becomes the current folder

Last among the user-visible changes, and blocked on Open question 3.

- Change the default scope from every-indexed-volume to the focused pane's current folder, on all drives.
  `searchable-folder.ts` already walks pane history back to the most recent real folder; when it returns `disabled`
  (`searchable-folder.ts:60`, a snapshot pane with no real-folder history), fall back to today's all-folders behavior.
- **Add a "This drive" scope option.** `ScopeFilterPopover.svelte` offers only the free-text field,
  `queryUi.scope.useCurrentFolder` (`:146`), and `queryUi.scope.allFolders` (`:157`). Without a drive-level option, M0
  makes whole-drive search harder for everyone.
- **Watch the recents side effect**: scope is a free-text expression persisted into every recent search
  (`SearchDialog.svelte:366`, `:398`), so a defaulted scope means every saved recent search carries a machine-specific
  absolute path. Decide whether the default scope is persisted at all.
- Update the onboarding and website copy this contradicts, per Open question 3.

Tests: `searchable-folder.test.ts` and `SearchDialog.svelte.test.ts` scope cases, including the `disabled` fallback, the
new option, and the recents behavior. Written after. Docs: `lib/search/CLAUDE.md` + `DETAILS.md`. Checks:
`pnpm check svelte`, `pnpm check desktop`.

### M10. Analytics, status, and the doc sweep

- **Analytics.** The premise is that indexing becomes optional and nothing today reports whether that worked. Extend
  `search_used` with categorical properties: coverage (covered, live, mixed), walk duration bucket, cancel rate, whether
  the walk was superseded, CTA conversion. Property shapes per `apps/desktop/src-tauri/src/analytics/DETAILS.md`, which
  documents `search_used` today.
- **`feature-status.json`**: the `search` note says "finds files across every indexed drive". Rewrite once true.
- Sweep the area docs named per milestone; confirm `docs-reachable`, `docs-dead-links`, `claude-md-length` are clean.
- Record the `content_epoch` forward-compat note in `writer/DETAILS.md`, where the propagation lives.

Tests: the analytics events fire with the right properties per coverage kind. Written after. Checks: full
`pnpm check --include-slow`.

## Open questions for David

1. **The unscoped case.** "All folders" (⌥V) still omits unindexed drives after this plan, and degrades further per
   Accepted difference 1. Full parity means live-walking every unindexed volume at once, which on a machine with a NAS
   plus two externals is hours. Recommendation: surface them as uncovered with a per-drive "search this one live"
   action.
2. **Network volumes.** SMB and MTP are the drives that are essentially never indexed, and a live walk there is minutes
   to hours over the wire. The product precedent is opt-in per drive with dedicated copy
   (`settings.mediaIndex.networkVolumes.*`). Recommendation: gate the first live walk of a network volume behind a
   confirm, then remember the answer per drive.
3. **M0 narrows the default for indexed users too**, and the onboarding copy sells the opposite ("Instant search of your
   whole drive. Think Spotlight, but even faster.", `onboarding.stepOptional.indexing.benefit1`). Accept the tradeoff
   and rewrite the onboarding and website copy, or keep whole-drive default for indexed volumes and accept the split?
4. **The per-drive veto and the master-switch copy.** `settings.indexing.masterOffNote` says "no drive is indexed and
   folder sizes stay hidden", which stops being true once a search writes coverage. And `user_disabled` is a consent
   given about one specific drive, unlike the global switch. Two calls: does searching a `user_disabled` drive write
   coverage, and does the note get rewritten?
5. **Two `index-crate-isolation` ceiling bumps** (`HandleMethods` and `RootPromises`), needed before M2 starts, per
   `.claude/rules/file-length-allowlist.md`.
6. **Three cost and trust tradeoffs**: Decision 4's 24-hour expiry, Decision 9's live-walk setting, and Decision 13's
   master-switch carve-out.
7. **What caps `index-root.db`** on a machine that declined indexing, given the existing retention cap never evicts root
   (M3d).

## Out of scope

- **Space-to-size on a folder.** It reuses M3a-c directly and lands right after, per David.
- **Branch-scoped watching.** Keeping search-written sizes live belongs to the Space feature; Decision 4's expiry stands
  in for it here, and M3d implements no watch bookkeeping. Design note so it is not relitigated: on macOS, subscribe to
  the drive root's FSEvents and discard events outside the covered branches (already what `watch/watcher.rs` does, and
  filtering is a path-prefix test). On **Linux this is not cheap**: `notify`'s recursive mode registers an inotify watch
  per directory against `max_user_watches`, so register watches only for the covered branches there. Restart persistence
  comes free from `supports_event_replay()` plus the FSEvents `sinceWhen` replay. One correctness detail: when a scan
  adds a new branch, events arriving for it _while the scan runs_ must not be discarded, or the size silently drifts;
  the scan-completion handshake and `BulkReconcileGuard` are the existing hooks.
- **Content search.** Only the forward-compat shape is in scope.
- **The Selection dialog.** Investigated and cleared; its folder-size gap is fixed incidentally by M3a.

## Risks

- **The frontier query is the performance hinge**, which is why M2 carries a measured exit criterion rather than a note.
- **The primitive choice in M3a is a measurement, not a preference.** The two in-tree numbers for the same comparison
  disagree (about 19× on a busy machine, about 9× on an idle one, with the faster path abandoning roughly 10% of rows
  under contention), so M3a measures the frontier profile itself rather than inheriting either.
- **The writer's bounded channel** (20,000, `writer/mod.rs:181`) is sent to with `send_blocking_with_depth`
  (`writer/mod.rs:651-653`) from inside `visit_dir` on the walker's worker threads (`insert_visitor.rs:88-100`, `:130`),
  so a full channel blocks discovery itself. The honest property to design for: the UI never waits on the writer for
  matches it already has, but the discovery rate stays writer-bound. A stronger property needs a drop-oldest UI channel
  plus a decision about letting the walk outrun the writer, which reintroduces the unbounded-memory problem the bound
  exists for.
- **The master-switch carve-out is a promise change.** If David later wants "off means truly nothing", this is the first
  decision to revisit.
- **Exclusion-policy drift** is handled by M2's version stamp, but only for future changes; subtrees excluded by a rule
  removed before the stamp exists stay invisible until a full rescan.

# Search that covers the folder you picked, indexed or not

**Status**: SHIPPED. All eleven milestones (M0–M11) landed and merged to local `main`; nothing here is pending. Keep the
doc for its decisions, its 13-item register of accepted indexed-versus-not differences, and § Execution status's record
of what each milestone actually did. **Owner**: David. **Date**: 2026-08-03.

Indexing stays optional. A search that runs to completion returns the same files with or without an index, only slower,
on every volume kind: local, SMB, MTP, and whatever comes next. The walk that fills the gap writes what it finds into
the drive index and watches the branches it covered, so a drive converges toward instant through use and stays there.

The broadest scope a search can have is **one volume** (Decision 4), which is what makes the guarantee reachable: there
is no fan-out, so there is no case where a search quietly omits a drive.

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
buried. It is meant to be complete; anything found later belongs here — 14 through 16 were added by the closing audit,
which found the register claiming completeness while three live states were missing from it.

1. **An interrupted walk is narrower.** Cancel, drive disconnect, and app quit each end a walk early and yield a
   strictly smaller result set than the indexed run. This is the difference people meet most often, so M6 labels the
   result list incomplete rather than letting it read as exhaustive.
2. **Unreadable subtrees are narrower.** The 32-failure give-up prune and M2's marker (now `entries.unreadable_cause`)
   mean a walk without Full Disk Access covers less than an index built when it was granted. Honestly signalled, and M8
   offers the way out where there is one, but still a difference.
3. **Auto-apply works on indexed drives and not on uncovered ground** (Decision 7). Crossing into a frontier needs
   Enter.
4. **Ranking is not preserved.** Importance weights come from the index, so live-walked results rank by match quality
   and recency only. Results are capped, so at the boundary a different order is a different visible set; the completion
   re-rank (Decision 8) reorders what survived and does not recover what the cap dropped.
5. **Directory size filters behave differently.** A directory's size is overwritten after ranking from
   `dir_stats.recursive_logical_size` (`execute.rs:198`, `:207`, `:234-254`), outside the engine, so M4's factoring does
   not cover it. Over live-walked ground `dir_stats` is absent or a lower bound by construction, so a "folders over 100
   MB" filter returns a different set than the indexed run.
6. **A covered-but-stale subtree is trusted, not re-walked** (Decision 5). A volume that was disconnected while its
   watcher was down can return a deleted file until `reconcile/` catches up. This applies equally to indexed and
   walk-covered volumes now that both are watched (Decision 9), so it is a property of the index rather than a gap
   between the two.
7. **The walk indexes what the user will never see in results.** `excludeSystemDirs` is match-time only (Decision 6), so
   a live search of `~/projects` walks and writes every `node_modules` and `.git` under it. That is the multiplier on "a
   search on an unindexed drive can take minutes".
8. **Media, OCR, and semantic search stay empty.** The walk writes the drive index only, never `media_index`, so photo
   and OCR search on a walked-but-unindexed drive returns nothing. Signalled by the existing
   `search.imageResults.notIndexed` copy, so no new work, but it is a difference.
9. **A walk that ran to completion can still be short**, and M7 labelled it (`SearchRunCoverage::abandoned_ground` →
   `search.coverage.walk.abandoned`), though it happens far less often than an earlier draft of this register claimed.
   The parallel walker abandons a directory that stops producing at `LOCAL_LIST_TIMEOUT`, or gives up after 32
   consecutive failed reads. The measured case where that cost ~10% of rows was a phone's File Provider mount inside a
   whole-`/` scan, **not** general machine load: the walker has never used rayon, and M3a measured zero abandonments
   across four real trees up to 1.2M entries (`reconcile/DETAILS.md`, `docs/notes/cover-walk-primitive-2026-08-05.md`).
   A scoped walk only meets that shape when the scope itself contains an unresponsive mount. Coverage stays honest
   either way, since an abandoned directory is never marked listed, so the frontier re-offers it next search. But the
   result list of that run is short without being labelled so, which is why M6 labels it alongside the interrupted
   states.
10. **A size filter treats hardlinks differently live than indexed.** A walk emits each entry's OWN size, before
    hardlink dedup, because that's what a listing shows; the index stores the deduplicated size, which is `NULL` for the
    2nd+ link to one file. So "files over 1 MB" keeps a hardlinked duplicate in a live result and drops it from an
    indexed one. Found in M4. Bounded (only multiply-linked files, only under a size bound), and the live answer is the
    truthful one, so it's registered rather than fixed.
11. **The master-switch settings note becomes inaccurate, deliberately.** `settings.indexing.masterOffNote` says "no
    drive is indexed and folder sizes stay hidden"; once a search writes coverage, folder sizes appear for walked
    branches. David reviewed this and chose to leave the copy as-is. ❌ Don't "fix" it without asking him.
12. **A live count-only search can count a file twice.** The row path dedupes a walked entry against the rows already
    emitted, bounded by the result cap; a count has no such bound, so a file that is BOTH in the arena and inside a
    frontier subtree is counted by each half. It takes rows under an unlisted directory to happen at all (a verification
    pass, or an interrupted walk), and the row path is unaffected. Found in M5.
13. **A non-virgin frontier root's newly found rows arrive one search late.** The local repair path for a frontier root
    the index already holds children for writes through the serial reconcile, which takes no live consumer, so what it
    ADDS lands in the index without streaming. The next query sees it (the arena mark is set when the walk starts, not
    when it emits). Rare, and the ground is genuinely covered afterwards. Found in M5.
14. **Ground another search is walking answers narrower, and says so** (`still_covering`). Only one walk may cover a
    patch of ground, so a second search over the same folder is told what it left alone; those rows reach the index and
    the next search picks them up, but not that one. The case where it cost the run its WHOLE answer is closed — a run
    with nothing from the index and no ground of its own now waits for the walk that holds it — so what's left is a run
    that has index rows to show and a claimed frontier: it answers with the covered half, labels the rest, and is
    narrower than the same search over an indexed drive. Found by the closing audit.
15. **A volume mid-full-scan isn't walked at all, and the run reports `Interrupted`.** The scan owns the writer and is
    covering that ground anyway (M3c's gate), so the search answers from whatever the index already holds and says the
    walk was interrupted — narrower than the indexed answer, and worded as though something went wrong rather than "your
    drive is busy indexing itself". Verified over MCP in M9. Found by the closing audit.
16. **A broad query answers on a fully indexed scope and fails the whole run on one with any frontier.** The arena
    evaluator allows a query that narrows nothing below 100k rows; the live evaluator refuses outright, and refusing
    takes the RUN with it (M4, deliberately: answering from the index alone over uncovered ground is the
    confident-looking half-answer this plan exists to remove). So the same query is an error on an unindexed drive and a
    result list on an indexed one — the starkest difference in this register, and the one a user is most likely to read
    as a bug. Found by the closing audit.

## Decisions

Each records the intent, so an implementer can adapt without re-litigating. All of them are settled; David signed off on
4, 9, 10, 13, 15, 16, and 17 on 2026-08-04.

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
4. **One volume is the broadest scope a search can have.** The scope chip offers "current folder" (the default) and
   "this volume" (the maximum); today's "All folders" goes away and `⌥V` rebinds to "this volume". This is what makes
   the guarantee reachable rather than aspirational: multi-volume fan-out is the only reason a search could quietly omit
   a drive, or report a 2%-walked drive as covered. It also **deletes** machinery rather than adding it, see M0.
   Accepted cost: searching the boot disk and a NAS in one action stops being possible, and a search of a cold volume
   now waits for that volume's arena (measured at about 10.9 seconds for a 13.5 million-entry NAS index) instead of
   deferring it, which M6's phase states voice honestly.
5. **A covered-but-stale subtree is trusted, not re-walked.** `0 < min_subtree_epoch < current_epoch` means exact but
   computed at an older epoch, typically after a reconnect. `reconcile/` owns freshness, and Decision 9 gives
   walk-covered branches the same watcher that indexed volumes have, so there is no longer a class of coverage nothing
   maintains.
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
9. **A walk watches the branches it covered, and nothing expires.** A rejected earlier draft gave search-written
   coverage a 24-hour TTL, because a walk on a volume with indexing off started no watcher, so its rows were a snapshot
   nothing would ever update. The right fix is the watcher, not a clock: the walk registers a watch on the highest
   branch covering any walk-covered folder and discards events outside those branches. Then a walked branch is genuinely
   equal to an indexed one, with no re-walking and no TTL. See M11 for the mechanism and its Linux caveat.
10. **The MCP tools are a thin wrapper on the same path, and an agent search walks exactly like a person's.** No
    walk-versus-no-walk parameter, no separate policy. `run_blocking` is already the single funnel both callers use
    (`execute.rs` module doc), and Decision 4 removes the one policy that differed between them (`ColdVolumePolicy`,
    which only ever applied to unscoped extra volumes).
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
13. **Both indexing switches govern background work only, never a user-initiated read.** The master switch
    (`lifecycle/master.rs:6`, "nothing indexes, anywhere") and the sticky per-drive `user_disabled` veto both stop
    background scanning and watching; neither stops a scan someone asked for by searching. The master switch genuinely
    blocks today at three code sites (`handle/mod.rs:163-167`, `lifecycle/state.rs:555-558`,
    `transports/smb/index.rs:220` via `drive_index_should_run`) and is restated as an invariant in four docs
    (`lifecycle/CLAUDE.md`, `lifecycle/DETAILS.md:383`, `transports/CLAUDE.md:23`, `master.rs`), so all of those need
    the carve-out or the invariant becomes a lie in four places. The veto keeps real teeth under Decision 9: a vetoed
    drive gets no watcher, so what a search walked there stays covered and served but stops being kept current the
    moment the app does. (M11 correction: it is NOT re-walked — the walk marked those directories listed, so the
    frontier never offers them again and Decision 5 trusts them as covered-but-stale.) The settings copy stays as
    written, see Accepted difference 11.
14. **Progress is directories scanned plus the current path. No percentage, no ETA.** The total is unknown by
    definition, and a fabricated ETA violates honest progress (`docs/design-principles.md`).
15. **There is no way to turn live walking off.** Search is a deliberate action and a walk is what it means; a
    half-answer behind a preference would put the product back where this plan started. ❌ Don't add a setting, a
    per-drive opt-out, or a "search index only" mode.
16. **File Provider domains (Dropbox, iCloud Drive, Google Drive) belong to the boot volume's scope.** They report the
    same device id as `$HOME` and never appear in `mount` (`scanner/file_provider.rs:7-9`), so a device check alone
    cannot see them; that is fine, because they are in scope rather than a boundary to cut at. The guarded walker
    already exists to survive a hung `readdir` on a disconnected provider mount, which is exactly this case.
17. **A full rescan evicts the DB rather than refilling it.** Whenever coverage has to be rebuilt from scratch (a schema
    change, a journal gap too wide to replay, a corrupt store), drop the index DB and let the next search re-walk. This
    matches the crate's standing "rebuild, don't migrate" policy (`indexing/CLAUDE.md`) and keeps a half-migrated store
    from ever claiming coverage it does not have.

## The core mechanism: a coverage-pruned walk

- `listed_epoch` is stamped per directory when its direct contents were read. Zero means unknown, and the scanner keeps
  it honest: an abandoned or give-up-pruned directory is never marked listed (`scanner/CLAUDE.md` § "Honest-stale, never
  false-complete").
- `min_subtree_epoch` is the **zero-absorbing min** of a directory's own `listed_epoch` and every child directory's
  `min_subtree_epoch`. Canonical implementation: `store::recompute_min_subtree_epoch`
  (`indexing/store/dir_stats.rs:184-212`), which also `COALESCE`s a missing `dir_stats` row to zero; `writer/delta.rs`
  is the ancestor-walk that calls it.

### The descent rule

The frontier needs **both** epoch fields plus one new column. Using `min_subtree_epoch` alone degenerates: because the
min absorbs zero upward, one uncovered directory anywhere forces zero on every ancestor including the scope root, so
"the shallowest node at zero" is always the scope root and the frontier becomes "walk everything".

Descending from the scope root:

- `min_subtree_epoch > 0` → **covered**. Serve from the index, do not descend.
- `min_subtree_epoch == 0 && listed_epoch > 0` → **partially covered**. This directory was read, something below it was
  not. Descend.
- `listed_epoch == 0 && known_unreadable` → **skip**. A directory the walk has tried and cannot read. Not frontier, and
  reported to the user rather than silently dropped.
- `listed_epoch == 0` → **frontier**. Cut here and hand it to the walk.
- **No `entries` row at all** (a cold volume, or a path the index has never seen; `resolve_path` returns `None`,
  `store/mod.rs:174-176`) → the scope root itself is the whole frontier.

Because `recompute_min_subtree_epoch` coalesces a missing `dir_stats` row to zero, `min_subtree_epoch > 0` implies
`listed_epoch > 0`, so the cases are disjoint and exhaustive. M2's proptest checks that premise before relying on it.

The `known_unreadable` marker is a schema addition and belongs in M2 with the rest of the data model. Without it, a
permission-denied subtree stays `listed_epoch = 0` forever and re-enters the frontier on **every** subsequent search, a
permanent repeating slow path with no user signal.

### Exclusions are a live-walk concern only

A policy-excluded child is skipped at `crates/cmdr-index/src/indexing/scanner/insert_visitor.rs:146-147`, so it gets
**no `entries` row at all**, and `recompute_min_subtree_epoch`'s child scan is
`WHERE c.parent_id = ?1 AND c.is_directory = 1` (`store/dir_stats.rs:196-206`), which cannot see a row that does not
exist. The live paths gate identically (`reconcile/reconciler.rs:1159`, `:1338`,
`crates/cmdr-index/src/indexing/watch/event_loop/verification.rs:384`). So excluded directories drive nothing to zero
and the index-side frontier query needs no exclusion logic; the live walk needs all of it, applying the structural tier
exactly as a volume-root scan does, or a scoped search of `/` walks `/private/var` and `/proc`.

Two limits on that, both real:

- **Policy drift.** `EXCLUDED_PREFIXES` and `JUNK_BASENAMES` are compile-time constants. If a release _removes_ an
  entry, the previously excluded subtrees still have no rows, their parents still read as covered, and they become
  permanently invisible to search with no re-walk trigger. **Closed in M2**: `meta.exclusion_policy_built_for` carries a
  content fingerprint of the policy, stamped right after every truncating full walk, and a mismatch (or an absent stamp)
  makes the coverage query treat the whole scope as uncovered.
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

## Execution status

All eleven milestones landed and are merged to local `main`. The execution branch and its worktree are gone.

**M10, item by item:**

1. ✅ **The Clear button and the size indicator with drive indexing off.** The section read `get_index_status(root)`,
   which answers off the LIVE registry, so the machine most likely to hold an index it never asked for was the one told
   "No index" with no Clear button — and pressing Clear there would have done nothing anyway (`clear_index` on a volume
   with no instance logged "was not indexed" and returned OK, a pre-existing bug that also made the per-drive "Forget
   this drive" a no-op on an offline drive). Now `Index::disk_footprint` sums every `index-*.db` plus sidecars off the
   FILES, and `Index::forget_all_volumes` clears the union of the registry and the data dir. Scope widened deliberately
   from root to every volume: a search walks whichever drive it's pointed at, so the disk can belong to a share nobody
   ever enabled. `HandleMethods` 38 → 40, no new root promise.
2. ✅ **Decision 17 wired.** Two of its three cases already evicted (a schema change deletes and recreates the file, a
   failed store is forgotten before the retry). The third had nowhere to go: an index that predates this build's
   exclusion policy counts as covering NOTHING, so a walk-built drive re-walked its whole scope on every search forever
   (each root on the slow non-virgin repair path) and never re-stamped, because only a truncating full scan may stamp —
   and no scan is coming for a drive nobody indexes. A writer-only start now drops that database through `clear_index`
   before the store opens. ⚠️ **A `ReadPool` over a recreated database could serve the DELETED one**, found while
   proving that: the thread-local cache is keyed by `(db_path, generation)` and every pool started at `0`, so a
   successor pool inherited connections still open on the unlinked inode. Pre-existing and reachable with no walk
   involved ("Forget this drive", then turn indexing back on). Fixed by issuing each pool a globally unique starting
   generation.

3. ✅ **Analytics.** `search_used` fired at run START with one prop (`mode`), which leaves every question this effort
   raises unanswerable. It now fires ONCE per run, when the run ENDS, with `trigger` (the debounce apart from a run
   somebody asked for), `ending`, `coverage`, `duration_bucket`, `abandoned_ground`, `capped`. `coverage` comes off a
   new typed `SearchRunCoverage::kind` (`covered`/`live`/`mixed`, from the coverage QUESTION rather than how far the run
   got). `superseded` is the frontend's own word, since the backend never reports one. CTA conversion is two events
   (`search_cta_offered` / `search_cta_used`), because the Full Disk Access offer depends on a TCC probe that answers
   after the run does. ⚠️ The run clock starts on the coverage callback's `null`, ❌ not on `searchFilesStreaming`
   resolving: a small folder's whole run can arrive before that promise does, which is what made the first version
   report no duration at all. That retired `LiveSearchSourceDeps.onStarted`.
4. ✅ **`feature-status.json`.** "Finds files on an indexed drive" was the whole limitation this effort removed; the
   note now says an indexed folder answers in well under a second, an unindexed one gets walked live with results
   arriving as they're found, and one search still covers one drive.
5. ✅ **The `content_epoch` note** is in `writer/DETAILS.md` beside the `listed_epoch` rules it will reuse, with the ❌
   that matters: one epoch for both dimensions would make a content pass invalidate listing coverage.
6. ✅ **The locale sweep.** Five more French strings had lost their apostrophes or carried a lone `'` where the catalog
   rule is `''` (M8 fixed four; these are the rest of this effort's ~40 keys). Register needed nothing: all nine locales
   address the reader the way `docs/i18n/formal-informal-decisions.md` decides. ⚠️ Five EN strings from this effort
   carry a lone `'` (`didn't`, `what's`); they render correctly (nothing ICU-significant follows) and the wider catalog
   is mixed at 264 occurrences, so doubling them would restamp 45 `sourceHash` lines for no visible change. Left,
   deliberately.
7. ✅ **The `CLAUDE.md` budget.** `src-tauri/src/search/` 879 → 597 and `src/lib/search/` 684 → 599 (the two named),
   plus `scanner/` 692 → 598, `read/` 601 → 593, `onboarding/` 621 → 600, and `lifecycle/` 803 → 645 (still over, but
   well under the 771 this effort found it at). ⚠️ Three files stay over and are NOT this effort's doing:
   `importance/scheduler` (621), `network_scanner` (685), `store` (709).
8. ✅ **M1's narrowed surface reads coherently**, verified in the running app: an auto-applied run on a drive with no
   index says "Cmdr hasn't indexed Macintosh HD yet, so this search skipped: ~", then "Press Enter and Cmdr will look
   through it now.", then offers [Index this drive] [Don't ask again]. The gap, the one-key way out, and the durable
   offer, in that order. The offer is structurally index-only-run-only (`coverageCtaVolumeId` needs `uncoveredScopes`,
   which `coverageNoteFromRun` always leaves empty).
9. ✅ **M3b's two leftovers split.** `VolumeIndexStatus.enabled` is RIGHT: it says "an index is registered", which is
   what it computes and what the badge needs (a walk-built index carries `freshness: null` and renders gray). Its doc
   now says it is NOT "this drive is indexed". The first-connect suppression was WRONG: it asked `enabled` alone, so a
   drive a search had walked never got offered indexing again that session — the drive that most needs it. It now asks
   for `freshness` too.
10. ✅ **Doc sweep**, with `docs-reachable` / `dead-links` / `link-text` green and `claude-md-length` knowingly warning
    on the three files above.

**Found by driving the app, and not by any test: a search from the home folder walked nothing.** A pane in `~` stores
its path as `~`, and M0 made the current folder the DEFAULT scope, so that string became the include path of an ordinary
search. `realpath` doesn't expand a tilde, so the index resolved a literal `~` (an indexed drive would have said "Cmdr's
index doesn't cover this folder yet") and the walk resolved `/~` and reported "This search stopped early". Cmdr launches
in `~`, so this was the default search on a default machine. Fixed in `canonicalize_scope_path`, the one funnel both
halves of a live search take their scope through.

**Verified in the running app (M10).** With drive indexing OFF and a search having walked, Settings > Indexing > Drive
indexing shows "Index size 121.75 MB" and a live Clear button — where before it said "No index" with no button, and a
Clear would have deleted nothing. Clearing removed all four index databases including three no volume had an instance
for (two NAS addresses and a disk image, 16 MB the old screen never counted), and the row settled to "No index" with no
error. The Enter-run then walked `~` live ("136 matches so far, 24,093 folders scanned"), and Escape said "You stopped
this search, so it shows what Cmdr had found by then."

**M11's design did not survive contact with the watcher's real code.** What changed, and why:

- **The premise "a path-prefix test over the drive-root FSEvents stream the watcher already runs" is wrong for the case
  M11 exists to serve.** A `DriveWatcher` starts in exactly two places, `start_scan` and `start_replay`
  (`lifecycle/manager/start.rs`), both of which belong to `Activation::IndexTheVolume`. A walk-built index is
  `Activation::WriterOnly`: "a database, an epoch, the read handles, a writer — no scan, no watcher". So there is no
  stream to filter; M11 has to START one. Conversely a volume that DOES have the drive-root stream is fully indexed and
  fully watched, and needs nothing from M11.
- **Restart persistence therefore doesn't come free either.** Nothing at launch registers an index for a drive the user
  never enabled, so a persisted branch set has nobody to hand itself to. Resolution: the branch watch resumes when the
  volume's index instance does (the `WriterOnly` arm of `start_indexing_for`), which is the first moment anything can
  read that coverage at all — a search, since an unregistered volume serves neither sizes nor coverage answers. The
  FSEvents `sinceWhen` replay does its half from the persisted `last_event_id`.
- **The mid-walk boundary case is worse than "events get discarded".** Letting the live loop write into a branch a
  parallel walk is covering is M3c's collision one level down: the walker allocates fresh ids, `INSERT OR IGNORE` drops
  the loser, and its subtree is orphaned. So the events must be BUFFERED, not just admitted — which is exactly the shape
  of the scan-completion handshake (`buffer during the scan → replay → switch_to_live`), per branch instead of per
  volume.
- **Discarding an out-of-branch event is not free either.** `process_fs_event` escalates a missing-parent event to a
  subtree rescan (`reconciler/escalation.rs`), so an unfiltered watcher on a walk-built index would walk ground nobody
  asked for. A branch-scoped loop clamps escalation to the branches, and never routes a shallow `MustScanSubDirs` to the
  whole-volume rescan.
- **A coalesced `MustScanSubDirs` above a branch has to be re-anchored, not dropped.** FSEvents reports "something under
  here changed" at a shallower path than the branch; a plain prefix test would discard it and lose every change under
  the covered ground.
- **`BulkReconcileGuard` is not one of the hooks.** M11 named it beside the scan-completion handshake, but it suppresses
  per-entry propagation during a bulk reconcile (`MarkLedgerUnpaid` / `PayLedgerIfUnpaid`); it has nothing to say about
  which events a live loop may act on. The handshake was the right pointer and the only one.
- **The same hazard exists on a SCANNED volume, which no milestone had noticed.** A search walks the holes in an indexed
  drive while that drive's live loop is running, so the loop and the walker write the same names through one writer. The
  buffering therefore applies to every live loop, not only a branch-watched one; a scanned volume keeps no branch
  bookkeeping past the walk (`AfterWalk::Forget`).
- **⚠️ The SMB change-notify translator has the same unclosed race.** It writes through the volume's writer
  (`state::get_writer_and_scanning_for`) with no notion of a cover walk, so a walk on a share races it the way a local
  walk used to race the local loop. Out of M11's scope (this is a local-filesystem watcher) and left standing, recorded
  in `watch/DETAILS.md`.
- **"A vetoed drive's walked branches re-walk" (Decision 13, M11, `master.rs`'s module doc) is not what happens.** The
  walk marked those directories listed, so the frontier query never offers them again; with no watcher they are
  covered-but-stale, trusted per Decision 5. The veto's teeth are real but they're "no watcher, so no freshness", not
  "re-walk".

**Landed.**

- **M11**: the branch watch. `watch/branches.rs` holds `WatchScope` + `BranchWatch`: an event inside a covered branch
  flows, one inside a branch a walk is covering RIGHT NOW waits, one anywhere else is dropped. The middle state is the
  milestone — writing mid-walk lets the parallel walker's fresh ids lose to `INSERT OR IGNORE` and orphan a subtree,
  dropping drifts the branch's sizes with nothing to signal it — and it applies on a SCANNED volume too, since a search
  walking a hole in an indexed drive races that drive's loop identically (nobody had noticed). A sweep above the
  branches is re-anchored onto them AND kept for the rest of its subtree; a branch-confined reconciler never grows
  coverage outward and never routes a shallow anchor to the whole-volume rescan. The set persists as
  `meta.walk_covered_branches` (index-relative, so a remount finds it) and comes back when the volume's index does,
  replaying from the stored `last_event_id` or bumping the epoch when it can't. `master::branch_watch_allowed` is the
  gate (master switch + `user_disabled`, NOT `persisted_scan_completed` — a searcher never opted the drive in). Linux
  watches the branches themselves, macOS the volume root. Proved end to end by a real `DriveWatcher` on a real drive
  (`cover::cold_drive_tests::a_change_inside_a_walked_branch_reaches_the_index_and_one_beside_it_does_not`).

- **M0** (`2d17845cd`, `d711ebc7c`, `0b60a3f05`, `d4d433b1d`): the one-volume ceiling as a typed
  `ScopeError::SpansMultipleVolumes` at the API, the fan-out deleted (k-way merge, `ColdVolumePolicy`,
  `warm_in_background`, `all_indexed_volume_ids`, `RankKey`'s escape from `ranking.rs`), and the two-rung scope chip. A
  defaulted scope is NOT persisted into recent searches, so replaying re-resolves against the pane you are in.
- **M1** (`3e53f3d2e`, `14ff59821`, `f35fb18e2`): `prepare_search_index` gained a `loading` flag so
  `loading: false, ready: false` is the terminal "no index to wait for"; `SearchResult` gained `target_volume_id`; the
  readiness gate is per target (`coverage-note.ts::isTargetIndexReady`); `CoverageNote.svelte` renders both typed fields
  through a new `config.resultsNotice` slot.
- **M2**: the coverage frontier (`crates/cmdr-index/src/indexing/read/coverage.rs`) behind `Index::coverage` and
  `Index::coverage_token`, schema v15's `entries.known_unreadable`, and `meta.exclusion_policy_built_for` stamped after
  every truncating full walk. Measured at 5.4 ms warm on a real 658 188-folder root index against the 50 ms budget, with
  no new database index (`docs/notes/coverage-frontier-query-2026-08-05.md`). `index-crate-isolation` ceilings raised
  once, per David's instruction: root promises 44 → 47, handle methods 35 → 38, the last slot reserved by name for
  `cover`.
- **M3a**: convergence. Marks now ride WITH the rows that make them stampable (one `Pending` lock in `InsertVisitor`,
  rows-then-marks inside the critical section), so a cancelled walk keeps every directory it read instead of stamping
  zero. `Index::cover(volume, frontier, dimension)` walks a frontier into the volume's real index and emits
  `CoveredEntry` batches over a bounded channel while it runs; `CoverOutcome` carries the cancelled/completed split. ❌
  It deletes nothing: `ScanRoot::Virgin` refuses a root that already has children (`ScanError::NotVirgin`) and the
  serial reconcile takes that case. The walk also stamps `known_unreadable` for permission-denied reads, which M2's
  column had no writer for. Primitive chosen by measurement — parallel walker, 3.2–5.8x over the serial reconcile with
  identical row counts on four real trees (`docs/notes/cover-walk-primitive-2026-08-05.md`). Root promises 47 → 50.
- **M3b**: the cold bootstrap, behind `cover` rather than as a method of its own, so **neither ceiling moved**
  (`handle/DETAILS.md` argues why). `start_indexing_for` gained an `Activation`: `IndexTheVolume` or `WriterOnly` (a
  database, an epoch, the read handles, a writer — no scan, no watcher). A writer-only start seeds `current_epoch` and
  stamps `EXCLUSION_POLICY_KEY` **only on a database that has never held an entry**, which is what makes a cold drive
  converge at all. It never claims the Fresh a journal replay earns. Shares, phones, and unmounted drives are refused
  (`NotIndexed`), classified through the enable command's own predicate with the `statfs` probe bounded on its own
  thread. `ensure_walkable` materializes a frontier path's ancestor chain through the writer at `listed_epoch = 0`, and
  declines a chain through a FILE row, a vanished path, or a symlink.

- **M3c**: walk policy. Structural exclusions are on for the search walk, as `ExclusionMode` layered over the
  kind-derived `ExclusionScope`; the walk pins the device its root sits on and cuts where another filesystem is mounted
  (one `symlink_metadata` per discovered directory, 2–3 µs and 3–6% of wall clock, measured); Decision 13's carve-out
  landed as one condition in `start_indexing_for`, with the four docs corrected; and one walk per patch of ground
  (`cover/live.rs`) closes the collision one writer per DB doesn't. Either cut writes NO ROW, because an unlisted row
  would sit in the frontier forever. File Provider domains stay in scope, pinned by a test that claims every directory
  is a domain root and asserts the walk descends anyway.

- **M3d**: the scoped walk on every volume kind. `network_scanner/cover_scan.rs::cover_volume_subtree` is a scoped BFS
  over the `Volume` trait — one frontier node resolved to its own entry id, the same round-trip disciplines and
  `ScanPacer` budget the two whole-volume walks use, `begin_scan_session` / `end_scan_session` bracketing the WHOLE
  frontier once. Its driver (`lifecycle/cover.rs`, `Ground`) is the ONE per-kind branch in the coverage concept: local
  ground reads the disk, everything else asks its `Volume`, and nothing downstream of a discovered entry differs. The
  M3b classifier is open — shares, phones, and network mounts route to the trait walk instead of `NotIndexed`, and only
  an unmounted id is refused. No gate and no confirmation step, per David.

- **M4**: the compiled query. `search/matcher.rs` holds a `CompiledQuery` (the pattern compile plus the type, size, and
  date predicates) and a borrowed `Candidate` both evaluators produce; `engine.rs` builds one per arena row and
  `matches_covered` builds one per `CoveredEntry`, so the walk's entries are judged by the arena's rules and nothing
  else. The refactor is behavior-preserving against the engine's own test suite, unchanged. `CompileError` replaces two
  bare `String`s, with `Display` reproducing both sentences verbatim. The broad-query guard is now per evaluator: the
  arena keeps its row-count ceiling, a live walk refuses outright.

- **M5**: the fallback in `execute.rs` (`run_live_blocking`), and everything a search that arrives over time needs.
  Coverage is asked BEFORE the arena loads, so an arena loaded after it holds every row it calls covered; the reload
  then runs when the walk mark AND the token disagree (`arena_for_coverage`). `live.rs` holds the run registry (a new
  run supersedes the others without cancelling them), `ResultStream` (100 rows or 100 ms, the cap that stops rows and
  never the walk, the bounded dedup set), and `drive_walk` (a forwarder thread for the `!Sync` walk handle, the run's
  own loop waiting on a deadline). Four events, run-id-stamped, in `live/events.rs`; `WalkEnding` types the four
  terminal states. `excludes.rs` extracts the scope exclusions so the live path applies the same ones. The proof is
  `execute/tests/live_e2e.rs`: six searches over a real `Index` and a real walk, including the Decision 12 anchor
  (verified by breaking `arena_for_coverage` and watching it return an empty list).

- **M6**: the streaming UI. `query-ui/query-stream.ts` holds the answers-over-time contract (phases, batches, a typed
  end, a `QueryStreamSource`) and `query-runner.svelte.ts` owns the run: the minted run id, the generation guard,
  appending, the cursor held by path identity, and one re-rank on completion (skipped when nothing walked, and once the
  user has moved the cursor). Search supplies the wire (`live-search-source.ts`) and the order (`live-ranking.ts`, which
  mirrors the backend's bands and is ORDERING, never membership). Three phase states, a status bar that becomes the
  progress strip (count, folders scanned, current path, Stop with its shortcut), a throttled `aria-live` region on an
  inner span, count-only's "N so far", the Escape two-step, and the coverage note's live half. Verified in the running
  app against a real unindexed NAS and an unindexed disk image.

- **M8**: the Full Disk Access route, and the typed cause it needed. `entries.unreadable_cause` (schema v16) splits "a
  walk was refused" from "no walk will read this", all the way to two lists on `SearchRunCoverage` and two sentences in
  `CoverageNote.svelte`. The refusal half offers `search.coverage.setUpFullDiskAccess`, which routes into the onboarding
  wizard's step 1 (the existing prompt, not a second one) through the host's `onGrantFullDiskAccess`. Three conditions
  gate it (`coverage-note.ts::offersFullDiskAccess`): a folder was actually refused, this is macOS, and
  `checkFullDiskAccessQuiet` says Cmdr doesn't already have it. ❌ Never offered over `declined`: no permission opens a
  snapshot tree. It also carried the M7 leftover — rows now reach the dialog 100 ms after they're found rather than when
  a 2 000-entry batch fills (`scanner/live_emit.rs`).

- **M7**: the walk that outlives its dialog. `walk-handoff.svelte.ts` keeps listening after "Open in pane", appends each
  batch to the snapshot (through `appendSnapshotEntries`, which bumps `mutationTick`), drives the toast, and hands the
  run back to a reopened dialog through the new `QueryStreamSource.resume`. `release_search_index` gained a
  `keep_run_id` so the close spares exactly that run. It shipped with two things the milestone didn't name: a walker
  HEARTBEAT (progress off the walk rather than its batches, which M6 recorded as the thing to fix) and the
  abandoned-ground signal that closes Accepted difference 9. `CMDR_E2E_WALK_THROTTLE_MS` is the soft test hook the
  milestone asked for.

- **M9**: MCP takes the live path. `run_live_collected` runs the same live search `start_live` does and folds its events
  into one `LiveAnswer` through `CollectingSink` (`search/live/collect.rs`), because a tool call is one request and one
  reply. `search` and `ai_search` both take it; `run_blocking` stays, serving only the dialog's index-only debounce. The
  wait is a transport budget (`maxWaitSeconds`, default 20 s, max 120 s) and when it runs out the walk KEEPS GOING — the
  reply says so and says to run the search again. ❌ Nothing of `ColdVolumePolicy` survived M0 to delete; the
  milestone's first bullet was already done. `uncovered_scopes` leaves the MCP reply, replaced by the live coverage
  report including M8's two lists. The `search` tool description had to lose 89 characters to fit the registry's
  256-char cap.

**Verified in the running app (M9, over `scripts/mcp-call.sh` against the worktree instance).** An index-served search
returns rows and says nothing; a search over a volume mid-full-scan says the walk was interrupted (which is the M3c
gate, and the first time MCP could report it at all); a `chmod 000` folder beside a fresh tree comes back as
`Note: the OS refused to let Cmdr read <path>.` plus `Cmdr walked 3 folders it hadn't indexed yet` — with NO Full Disk
Access offer, correctly, because this machine has already granted it; a 6 000-directory tree searched with
`maxWaitSeconds: 1` returns the still-walking note, and the same search again counts 5 402 of its files. ⚠️ The FDA
OFFER branch was not re-verified in the app (it needs a relaunch under `CMDR_MOCK_FDA=notgranted`); it rests on the unit
test over `refusal_note`.

**Verified in the running app (M8, on a `chmod 000` folder beside a 200-directory sparse tree).** The refusal gets its
own sentence and path; with real Full Disk Access no offer appears, and under `CMDR_MOCK_FDA=notgranted` the offer and
its line do, and pressing it closes the search dialog and lands on the wizard's step 1. Rows now arrive while the walk
runs (`0 → 34 → 91 → 124 → 169 → 200` over three seconds, sampled inside the webview against
`CMDR_E2E_WALK_THROTTLE_MS=40`). ⚠️ The DECLINED half was NOT re-verified against a real NAS: it rests on
`live_e2e.rs`'s fake-volume `@eaDir` assertion and the network cover tests.

**Decisions taken during execution that the spec did not pre-empt.**

- **Superseding was GLOBAL, so an agent's search would have emptied a person's.** `live::register` marked every other
  run superseded, which is right for one dialog asking one question at a time and wrong the moment a second asker
  exists: an MCP call would have stopped the dialog's events mid-type, and `release_search_index` (the dialog closing)
  would have cancelled an agent's walk out from under its caller. Fixed with `RunOrigin` — `Dialog` supersedes only
  `Dialog`, and `cancel_all_live_runs_except` became `cancel_dialog_runs_except`. Only app quit reaches every origin.
  The plan didn't see it because Decision 10 reads as "MCP needs nothing new", and it doesn't: it needs the registry to
  stop assuming one asker.
- **"Streaming and cancellation where the transport can carry them" resolves to two properties, not zero.** A one-shot
  reply can't stream, but it can carry the rows that had arrived when the wait ran out (rather than nothing), and it can
  decline to cancel: `AnswerEnding::StillWalking` hands back an answer and leaves the walk running, which is Decision
  11's reasoning over a different transport. That's what makes "run the search again" honest advice rather than "start
  over", and it's pinned by `an_agent_that_stops_waiting_does_not_stop_the_walk`.
- **A wait budget is not the walk-versus-don't parameter Decision 10 forbids.** `maxWaitSeconds` cannot turn the walk
  off and cannot make a search index-only; it only says how much of the walk to wait for. Without it an agent had no way
  to finish a big frontier inside one call, and no way to keep a quick lookup quick.
- **`ai_search`'s zero-results retry needed a gate it didn't have.** It re-runs the search with the LLM's scope dropped,
  which over a live path means a second walk over more ground. It now retries only when the first run SETTLED (a run
  still walking hasn't finished answering) and shares ONE deadline with it, so a fallback can't double the wait the
  caller asked for.

- **A `CoverWalk` can't be cancelled from anywhere but the thread reading it, so `Index::cover` takes the token.** The
  handle owns a `Receiver` and is therefore `!Sync`; every party that decides a walk should stop (a closing dialog,
  Escape, a quitting app) is somewhere else. `CoverWalk::cancel` was deleted rather than left as a second way to do it.
  This is also what M7 needs to stop a walk it deliberately outlived.
- **"App quit through the `resources/` stop-hooks" doesn't exist.** Those hooks run from `stop_all_indexing` (the memory
  watchdog, and master-switch-off), not from app exit, and `register_subsystem_stop_hook` isn't exported from
  `cmdr-index` at all, so app code can't register one without a new root promise. Quit cancels every live run at
  `RunEvent::Exit` instead, which is the terminal state the milestone actually needed. Under a watchdog stop the walk's
  writer goes away and its roots fail honestly (`RootOutcome::Failed`, nothing marked); a walk that keeps READING under
  a memory stop is a real (small) gap left standing.
- **The arena mark has to be set when a walk STARTS, not on its first batch.** A walk can write rows it never emits: the
  local repair path for a non-virgin frontier root writes through the serial reconcile, which takes no live consumer. On
  the batch-only mark those rows were pruned as covered by the next query and served from an arena that predated them —
  the exact Decision 12 failure, reached by a route Decision 12 didn't name.
- **The trait walk doesn't re-emit rows the index already holds**, so the two halves genuinely don't overlap in practice
  and the bounded dedup set stays what the plan called it: insurance for the indexed-between-query-and-walk race. Its
  unit test is what proves it, since no end-to-end fixture can produce the race on purpose.
- **`Index::coverage` reads the LIVE registry's pool while a search's arena can come from a DB file on disk.** A volume
  with an index on disk but no registered instance therefore reports its whole scope as frontier while the arena answers
  from the file. Harmless (the dedup set absorbs the overlap, and the first walk registers the volume) but it means the
  first live search of such a volume over-walks.
- **`SearchCoverage` was already taken.** The operation log exports one (how much of a copy's source tree a journal
  search covered), and specta refuses two types of one name, so the live one is `SearchRunCoverage`.
- **A live count-only search can double-count.** The row path dedupes against emitted rows, bounded by the cap; a count
  has no bound to dedupe within, so a file both in the arena and inside a frontier subtree counts twice. Needs rows
  under an unlisted directory to happen at all. Belongs in the accepted-differences register.
- **A broad query on a partly covered scope refuses the whole run** rather than answering from the index and looking
  complete. The arena guard would have allowed it below 100k rows, and the answer would have been a confident-looking
  list that silently skipped the unindexed half — which is what this plan exists to remove.

- **Case folding was derived in three places, not one, and that's a fork the plan didn't name.** M4 was written as "the
  pattern compile plus the size, date, and type predicates". But `prepare_scope_filter` computed the case-sensitivity
  rule a second time and the ranking call took a third copy, so a change to the rule could have made a search exclude
  under one alphabet and match under another. The scope filter now takes it from the compiled query.
- **A walked entry's SIZE can't match an indexed one's, and it shouldn't.** `CoveredEntry` carries pre-hardlink-dedup
  sizes (deliberately, so a listing doesn't show a hardlinked file as 0 bytes) while the index stores the deduplicated
  size, which is `NULL` for a 2nd+ hardlink. So a size bound keeps that file in a live result and drops it from an
  indexed one. It belongs in the accepted-differences register next to difference 5; the live answer is the truthful
  one, so the fix is the register, not the code.
- **The name a walked entry matches under is derived, not carried.** `CoveredEntry` has a path and no name, so the
  matcher takes the path's last component, byte-identically to how `insert_visitor` derives the row name. That holds by
  construction for the local walker (same expression, same input) but only by agreement for the trait walk, whose row
  name is the listing's `name` field while its path is the listing's `path`. A `Volume` backend reporting a path whose
  last component isn't that name would desynchronize live and indexed results silently. Recorded in `search/DETAILS.md`;
  not worth a `name` field on `CoveredEntry` (a `String` per entry per batch) unless a backend ever needs one.

- **The trait walk is add-only PER DIRECTORY, so it needs no virgin-root refusal and no repair path.** M3a's
  `ScanError::NotVirgin` exists because the parallel local walker can't afford a DB lookup per directory across eight
  worker threads reading a `readdir` that costs microseconds. Over the trait that lookup is an indexed query against a
  listing that cost a network round trip, so the walk simply compares each directory's names against the index: an
  existing name keeps its row and its id, an existing child directory is descended into with that id. That closes a gap
  the plan didn't see — there is no scoped serial reconcile over the `Volume` trait, so a `NotVirgin` on a share would
  have had nowhere to go and the node would have stayed frontier forever.
- **MTP's same-name siblings are a data-integrity bug, not a cosmetic one, and the same name check fixes them.** Two
  objects with one name in one folder would otherwise both get ids, both be queued as directories, and both write
  children — after which `INSERT OR IGNORE` drops one row and orphans everything the walk attributed to it.
- **The cold bootstrap needed a trait path too, which M3d's spec text doesn't mention.** `ensure_walkable` materializes
  a frontier path's ancestor chain with `std::fs::symlink_metadata`, which answers nothing for `mtp://…` and needn't
  answer for a direct smb2 session either — every cold trait-volume walk would have declined its own root as "not a
  directory on disk". `Ground::stat_directory` now supplies it, through a `stat_one_directory` whose timeout races the
  task's JOIN handle for exactly the reason listings do.
- **A network volume that is neither a phone nor a plain local mount is classified `Smb`.** The kind names the SCAN
  PATH, not the protocol, and an NFS or WebDAV mount needs precisely what that variant carries (trait-scanned,
  mount-rooted, no journal). Refusing it would make a search of it silently wrong; calling it local would point the
  guarded walker at syscalls that block for minutes. ❌ The walk does NOT upgrade an SMB os-mount to a direct session
  the way the enable command does: that can prompt for credentials, which is not something a search may do.
- **A NAS snapshot directory reads as FRONTIER, so a search of a NAS would have walked the one tree nobody may walk.**
  `network_scanner` indexes `@Recently-Snapshot` / `@eaDir` / … as rows and refuses their subtrees (hardlinked, per
  snapshot, 44 TB reported on a 10 TB volume), which leaves them at `listed_epoch = 0` — exactly the descent rule's
  frontier case. The plan never connected the two, and M2's coverage query has been reporting them since it landed; M3d
  is what would have acted on it. Closed by stamping them `known_unreadable`, whose meaning widens from "a walk tried
  and can't read this" to "nothing is coming for this subtree". No new verdict, no per-kind branch in the coverage
  logic, and an index built earlier heals on the first search that meets one. **M6 inherits it**: those directories now
  appear in `CoverageMap.unreadable` alongside permission-denied ones, and the copy has to cover both ("Cmdr doesn't
  search inside snapshot folders" is a different sentence from "grant Full Disk Access").
- **The empty-root refusal must NOT carry over to the cover walk.** `VolumeScanError::EmptyRoot` exists because a share
  that lists empty is a glitch and a false "complete" strands the index. An empty FOLDER is ordinary, and refusing to
  mark it would hand it back to every later search forever.
- **`lifecycle/network_scan.rs`'s exclusion-policy stamp claimed something the network walks don't do.** Its comment
  said a network scan applies the local junk-basename and pseudo-filesystem tiers; neither network walk calls
  `should_exclude` at all. The stamp is still right to be written (it's conservative — it can only over-report a policy
  change, which costs a re-walk), and the cover walk deliberately matches the full scan here rather than applying
  exclusions the full scan doesn't, so a walk-built share index holds the same rows a scanned one would. Comment fixed.

- **The master-switch carve-out is ONE code site, not three.** The plan named `handle/mod.rs`'s `start_volume` and
  `transports/smb/index.rs`'s reconnect resume alongside `start_indexing_for`, but both of those ARE background work by
  Decision 13's own definition: "Turn on indexing for this drive" and an autonomous reconnect are exactly what the
  switch exists to stop. Carving them out would contradict the decision it's implementing. The walk passes through
  `start_indexing_for` only, so the carve-out is `activation == Activation::IndexTheVolume` there plus deleting M3b's
  `NoCoverContext::MasterSwitchOff`. The four docs were all four right to need it.
- **One writer per DB is not enough on its own.** Two walks through the SAME writer over the same directories collide
  identically to two writers: each allocates a fresh id for the same name, `INSERT OR IGNORE` drops one, and its subtree
  is orphaned. Hence the frontier claims. The plan's "coalesce onto one walk" is implemented as "one walk takes the
  ground, the other reports what it left" rather than a shared-subscriber fan-out — Decisions 11 and 12 already say a
  superseded query recovers its predecessor's ground from the index, and a fan-out needs per-subscriber filtering and
  per-subscriber completion with no second consumer to shape either against.
- **A volume mid-full-scan needed a real gate, not just a comment.** `cover_context_for` handed the writer over whenever
  the phase was `Running`, and a full scan runs in `Running` with `mgr.scanning` set. The plan said "a search over a
  volume mid-full-scan does not walk at all"; it didn't yet.
- **The exclusion switch earns its keep only because `Rebuild` stays off.** The plan's own reasoning ("the existing
  callers do not depend on today's no-exclusions default") is correct, and turning `Rebuild` on too costs exactly one
  test — at which point `ExclusionMode` is dead code and the answer is "always apply". Left off deliberately: it changes
  what `reconcile/verifier.rs` and `watch/event_loop/verification.rs` write, and there IS a real divergence to close
  there (a `Rebuild` of a newly discovered `/Library` indexes `/Library/Caches`, which no boot scan does). That wants
  its own change with those areas' docs, not a ride-along. Recorded in `scanner/DETAILS.md` § "`WalkPolicy`".
- **Two seam tests had been failing on every full `cargo test -p cmdr-index --lib` run**, which blocks `cargo mutants`
  entirely (it refuses a red baseline). The real `build()` installs the config permanently where `install_for_test`
  restores it, and one provider test read a process-wide seam without the lock. Both fixed.

- **The missing-row case is NOT cold-volume-specific, and the plan undersold it.** M3b was written as "a drive that was
  never indexed"; a folder created since its parent was last listed has no `entries` row on a fully indexed drive
  either, and it's exactly the frontier node a coverage answer hands back. Same fix, but it is a warm-volume correctness
  bug, not only a bootstrap one.
- **Active stopped meaning indexed.** A writer-only instance makes `is_active` true on a volume nothing ever scanned, so
  `Index::start_volume` had to stop short-circuiting on it (`awaits_its_first_scan` → force a scan). That also fixed a
  pre-existing case with no walk involved: a first scan someone stopped left the same shape, and "Turn on indexing" was
  a no-op on it. Two consequences left standing for M10: `VolumeIndexStatus.enabled` reads true for a walk-built index
  (the frontend renders `freshness: null` gray, so the badge is honest), and `first-connect-trigger.ts:49` suppresses
  the "index this drive?" toast on a drive a search already walked.
- **The master switch still gates the walk.** Decision 13's carve-out is M3c's, so M3b keeps the invariant the four docs
  state and puts the whole gate in one place (`NoCoverContext::MasterSwitchOff`) for M3c to remove.
- **`Index::list_children` reports an unlisted directory's rows as its contents.** `list_dir_children` never consults
  `listed_epoch`, so a directory with a row but no listing (which the chain materialization creates, and which FSEvents
  verification and `reconcile_subtree` already created) answers with a partial listing rather than "not indexed". The
  agent's `list_dir` tool is the consumer, and its contract says a read that is a lower bound has to say so. Not fixed
  here — it's an honesty gap in a tool result, not in coverage — but it wants an owner.

- **`unresolvedScopes` copy says what Cmdr knows, never that a folder is missing.** Telling "not walked yet" from
  "genuinely not found" needs a filesystem probe inside search routing, which is a network-hang hazard and M5's job.
  Until then the copy is true for a typo and for a real folder on a partly covered volume alike.
- **`VolumeLoad::Failed` rides back as `uncovered_scopes`,** so a DB that will not open reads as "not indexed". Left
  as-is (for the user, search has no usable index either way and re-indexing is the same fix); **M5 splits it**.
- **`category === 'network'` is not a reliable network test** anywhere on the frontend: an SMB share whose direct
  connection was refused stays an OS mount and reports `attached_volume` with `fsType: 'smbfs'`. Use
  `volumeKindOf(...) === 'smb'` (invariant A6). Two sites fixed; **a sweep of the rest is still owed**.
- **A frontier node CAN hold a listed descendant, and it takes no cancellation to get there.** M3a set out to check
  whether the destructive delete was reachable and found a shorter route than the one this plan named: FSEvents
  verification upserts children under a directory without marking that directory listed, then scans each new child,
  which does mark it. So the plan's premise ("a scan root is a frontier node, so it can't carry a listed descendant") is
  false on the live path, not just after a crash.
- **A re-scan without the delete does NOT violate uniqueness** (this plan said it would). `insert_entries_v2_batch` is
  `INSERT OR IGNORE`, so the colliding row is silently skipped and everything the walk then attributes to the id it lost
  is orphaned — the same conclusion (an add-only walk over existing rows is unsafe) by a quieter and worse mechanism.
  M3a resolves it by refusing non-virgin ground rather than by deleting.
- **`known_unreadable` needed a writer, and M3a is where it belongs.** M2 added the column; without something setting
  it, a permission-denied folder re-enters the frontier on every search forever, which is the same convergence failure
  M3a exists to fix. The walk stamps it for `PermissionDenied` only (a timeout is transient), and `mark_dirs_listed`
  clears it.
- **Every walk paid a full watchdog interval of dead time.** Found while measuring the primitive choice: `walk` joins
  its watchdog, which slept a flat interval before checking whether the walk was done. A 368-entry tree took 1.01 s.
  Fixed (condvar, woken by `signal_done`); same tree now 3.92 ms. It affected `scan_subtree` on the verifier path too.

- **Routing Enter through the walk retires the uncovered note from user-triggered runs.** A live run WALKS a drive with
  no index instead of reporting it as a gap, and `SearchRunCoverage` has no `uncovered_scopes` at all, so
  `search.coverage.uncovered.*` and the per-drive "Index this drive" offer are now reachable only from an auto-applied
  (index-only) run. That reads well — the debounce says what it couldn't cover and offers the fuller search for one key
  (`search.coverage.pressEnter`) — but it is a real narrowing of M1's surface that M10's settings/analytics sweep should
  know about.
- **`unreadable`'s two causes had one list and no discriminator, and M8 gave them a typed cause end to end.** The wire
  carried bare paths, so permission-denied and NAS snapshot trees were indistinguishable frontend-side (short of
  matching `@eaDir` by name, which isn't an option) and M6 could only state the fact and name both possibilities. M8
  needed to ACT on one, so `entries.known_unreadable` became `entries.unreadable_cause` (schema v16, free: v15 never
  shipped) with an internal `UnreadableCause::{Denied, Declined}`; the local walker stamps `Denied` on a
  permission-denied read and the trait walk stamps `Declined` on a NAS system dir; `CoverageMap` and `SearchRunCoverage`
  carry `permission_denied` / `declined` as two lists, and the note renders two sentences. ❌ The enum itself does NOT
  cross into `lib.rs`: two `Vec<String>` fields on an existing struct cost no root promise, and the ceiling is David's
  to raise. The consumer partitions by cause anyway, so two lists is also the simpler render.
- **A live walk's progress used to be only as live as its batches**, and the batch size is what made that visible: a
  `CoveredEntry` batch fills at 2 000 entries, so a walk over a sparse tree (one matching file per directory) reports
  `0 folders scanned` and no path for hundreds of directories. M6 saw it on a `~/Library` walk; M7 fixed it with
  `WalkHeartbeat`, stamped as each directory read STARTS. The ROWS had the same problem and M8 closed it:
  `scanner/live_emit.rs`'s `EmitPacer` hands a partial batch over 100 ms after its first row, consulted from the push
  path and — because a walk parked on one directory calls no visitor hook at all — from the local walker's watchdog
  tick, which runs at 100 ms rather than a second while somebody is watching. ❌ The batch itself stays at 2 000: the
  channel is bounded on purpose, and 100 entries per crossing would spend that bound on chatter.
- **A `.svelte.ts` module dynamic-imported by URL is a SECOND instance.** `import('/src/lib/x.svelte.ts')` and
  `import('/src/lib/x.svelte')` give two modules with two copies of the state, and the app resolves to the `.ts` one.
  Only a debugging hazard (nothing in the app imports by URL), but it cost an hour of chasing a phantom duplicate-module
  bug while diagnosing M7's cancel.

- **The dialog close cancelled the run it had just handed to a pane.** Found in the running app, not by any test:
  `release_search_index` correctly spares its `keep_run_id`, but the frontend passed `null`, so the walk stopped the
  instant the pane appeared — the pane held whatever had arrived, the toast said "still searching" over a walk that
  wasn't, and nothing reported a thing. Fixed by asking the module that OWNS the handoff (`handedOffRunId()`) rather
  than reading the state cell from the dialog, and pinned by `SearchDialog.handoff.svelte.test.ts`. ⚠️ The fix is
  covered by that test but was NOT re-verified end to end in the running app.

**The closing pass (after M11): the E2E premise, and what fixing it turned up.**

- **Both live-walk specs rested on a false premise, and the feature's headline had no working end-to-end coverage.**
  "The E2E instance runs against a fresh `CMDR_DATA_DIR`, so the fixture tree is uncovered ground and Enter WALKS it" is
  wrong: `CMDR_E2E_START_PATH` narrows the boot scan to the fixture root (`scanner/exclusions.rs::e2e_allowlist_path`),
  and `Scan: complete (42 entries, 9 dirs)` lands ~100 ms after launch. So `search-live` passed VACUOUSLY (rows appear,
  no Stop button, "results", no note — all equally true of an index-served run) and `search-walk-handoff` failed
  outright, with no walk to outlive the dialog.
- **The fix is a spec-level premise, not a product change**: `test/e2e-playwright/search-walk-ground.ts` takes the index
  away through the two per-drive actions a user has (`indexing disable` + `forget`, then re-reads `cmdr://indexing` to
  prove it's gone) and builds a directory CHAIN as ground. The chain is what makes the timing honest rather than
  guessed: a walk through it is serial by construction, so it lasts at least `depth × CMDR_E2E_WALK_THROTTLE_MS` on any
  machine. That state also pins Decision 13 end to end — neither indexing switch gates a walk somebody ASKED for.
  Measured: 24 levels at a 100 ms throttle ≈ 6 s, versus 44 ms unthrottled.
- **Proved by making each spec fail for the right reason, twice**: with `context_for_walk` stubbed to refuse every walk,
  and with the ground rescanned INTO the index instead of forgotten. All three tests go red both ways, at the
  walk-observing assertions.
- ⚠️ **The throttle was 25 ms and the Linux lane never set it at all**, so the handoff spec had no window there even in
  principle. Now 100 ms in both harnesses (`desktop-svelte-e2e-playwright.go`, `scripts/e2e-linux.sh`).
- **A handed-off pane never grew, and that's M7's headline claim.** Found by the rebuilt spec, at the assertion the old
  one couldn't reach. Rows reached the snapshot and the toast counted to 24 while the pane held the two it opened with:
  `appendSnapshotEntries` wrote into the stored object, so `SearchResultsView`'s `snapshot` derived recomputed to the
  same reference and Svelte stopped propagation before the `entries` derived below it. Both mutators now `store.set` a
  replaced entry. The old spec's `SNAPSHOT_ROWS` also watched the RIGHT pane, where `..` alone satisfied "rows are
  here"; "Show all in main window" routes the ACTIVE pane, which is the left one.
- **A run whose matches total exactly the row cap said "Showing the first 30 of 30 matches."** `capped` means "the cap
  was reached", which is true the moment the last row fits; the sentence now needs rows actually held back.
- **`Rebuild` applies exclusions now**, and `ExclusionMode` is deleted with it (one variant left = always apply). It
  cost exactly the one test M3c predicted. The callers' `should_exclude` gate covers the directory they hand over and
  stops there, which is why a rebuild of a newly discovered `/Library` was indexing `/Library/Caches`.
- **`Index::list_children` consults `listed_epoch` now**: a directory with a row but no listing answers `None` ("not
  indexed") rather than handing back a lower bound shaped like a complete listing. `> 0`, not "at the current epoch",
  matching the descent rule.

**The two defects the closing audit found, both closed.**

- **A scoped search answered with the folder itself when the drive was indexed and not when it wasn't**, then with it
  again once its own walk had been through: three answers to one question, one search apart, which reads as a bug in
  whatever the user did in between. The frontier ROOT was the one entry neither half of a live run emitted — a walk
  reports a directory's CONTENTS, and the root's row is written by `ensure_walkable` where nothing but an index reader
  would ever report it. `ensure_walkable` now says whether it had to create that row, and `cover.rs` emits a root it
  created, once, ahead of that root's listing (a root the index already held stays the covered half's to report, so
  nothing doubles). ❌ Its materialized ancestors do NOT go out: they're above whatever scope asked for the walk. Pinned
  by the three-way test (indexed / unindexed / unindexed-then-repeated) in `live_e2e.rs`.
- **`open_search_dialog autoRun: true` reliably showed an empty dialog on uncovered ground.** Two live runs fired a
  millisecond apart, and the second — the one the dialog renders — found its ground claimed by the first one's walk and
  walked nothing. Traced in the running app (a stack trace at every `startLiveRun` and every `setRunOnMount(true)`), ❌
  not the arena-load race the open item above blamed. Two producers of one one-shot flag: the prefill's `autoRun`, and
  `QueryDialog`'s reopen-with-results path, which arms it in `onMount` from `lastRunQuery` — after the prefill's run has
  already fired and cleared it. A prefill now clears `lastRunQuery` (it REPLACES the session, results and all), so the
  caller's `autoRun` is the whole decision in both directions: `false` no longer runs the prefill anyway.
- **And the run that loses a claim race no longer presents as "no results".** A run that gets NO ground from
  `Index::cover` and had nothing from the index would answer with nothing at all under a note promising the files would
  turn up in a moment; it now waits for the walk that holds the ground and works the whole thing out again (`groundwork`
  groups everything before the first row, so the retry can't say anything twice). ⚠️ The check has to sit after the walk
  REQUEST: a claim is taken inside `cover`, and two searches started 150 ms apart come out of one arena load together —
  an earlier `CoverageMap::being_walked` check read empty in the app every time. Both halves are needed, so a run with
  index rows to show still shows them and reports `still_covering` (register 14). `Index::cover` on a retry reloads the
  arena without consulting the walk mark: a run that watched a walk end knows rows landed, and the mark is a global
  one-shot somebody else may have taken.

**Verified after the closing pass.** The macOS Playwright lane is green (269 tests across 3 shards) and the Linux Docker
lane is green (279 tests) — the first full Linux run of this effort, and the specs the lead reported failing there pass,
including the `search-recent` flake, which turned out to be my restore handing the drive back "fresh" but not yet able
to answer. ⚠️ `rust-tests` reports 56–79 tests killed at the 8 s nextest cap; that is CONTENTION, not this work:
`cargo test -p cmdr-index --lib` is 1,346/1,346 green, each flagged test passes alone in ~1.2 s, and the same four index
tests time out identically on the pre-change tree (controlled by checking the changed files out at `046b9c7a7^` and
re-running). ⚠️ Two duration warns are new and warn-only: the live-walk tests take 6.1 s / 6.3 s on macOS and 2.7 s /
3.6 s on Linux, because a walk somebody can watch is the point. ❌ No allowlist entry added.

**Attribution.** M7's Open-in-pane handoff fix (the `handedOffRunId()` change and `SearchDialog.handoff.svelte.test.ts`)
is INSIDE commit `0253ba91d`, whose message is about analytics: that agent ran `git add -A` over another agent's dirty
tree. Nothing was lost, and the history is deliberately not rewritten — other work is stacked on it. Recorded here so
the next reader isn't misled by the message.

**Open, needing David.**

- The drafted copy from M0 and M1 (11 keys, translated into all nine locales) has not been reviewed. An edit means
  retranslating.
- `SearchDialog.svelte.test.ts` is 1,471 lines against a 1,179 `file-length` allowlist entry, and `lifecycle/state.rs`
  1,672 against 1,356. Both were over before this effort and grew inside it. Warn-only. Raise, split, or leave.
- Whether the network CTA variant should still offer "Index this drive" at all.
- The M7 copy (nine keys: the still-searching toast and its two last words, plus the abandoned-folders note) is drafted
  and translated into all nine locales, unreviewed like M0's and M1's. So is M10's rewritten "Clear index" help text,
  which now says clearing takes EVERY drive's index.
- `index-crate-isolation`'s ceilings were raised twice inside this effort, both with David's standing say-so and both
  argued in the check: root promises 44 → 50 and `Index` methods 35 → 40. There is no headroom left, by design.
- **A prefilled query that arrives while the arena is loading never runs, and nobody is told.** `QueryDialog`'s
  `runOnMount` effect clears its flag and then runs only `if (config.isIndexReady && …)`; when readiness lands a moment
  later, nothing re-fires for the prefill itself (Search's own `search-index-ready` listener re-arms the flag, so the
  run does land when that event comes). The comment calls it deliberate ("the user hits Enter to fire when ready"), and
  for a person looking at the dialog that reads fine. It's also what made `search-recent` flake on the Linux lane behind
  a rescan. Recommendation: keep the flag set until either the run fires or readiness resolves to "no index is coming".
  Not changed here — it's a behavior choice in a shared dialog, not a defect. ⚠️ **This is NOT what made
  `open_search_dialog autoRun: true` show an empty dialog**, which is what this item used to blame. That was a
  double-run (the prefill's own run plus the reopen-with-results path arming the same one-shot flag after it had fired),
  and it's fixed: a prefill now clears `lastRunQuery`, since it replaces the session the reopen path exists to restore.
- **The handoff toast counts far past what the pane can hold.** The dialog asks for `limit: 30`; `ResultStream` stops
  emitting rows at the cap while `match_count` keeps climbing, so a walk handed to a pane can report "35,287 matches so
  far" over 30 rows, and `labelFor` only annotates "(first N of M)" once M passes 10,000. Nothing is lost — the count is
  true and the cap is deliberate (a stopped walk would freeze the count at a number that never becomes true) — but the
  toast and the pane disagree by three orders of magnitude with no words about it. Left alone deliberately: whether the
  handoff should raise its limit, label the gap sooner, or say "showing the first 30" is a product call, not a defect.
- Four `CLAUDE.md` files the closing pass touched sit 3–7 words over the 600-word soft budget after three trimming
  rounds (`lib/search`, `test/e2e-playwright`, `indexing/read`, `indexing/scanner`), each carrying one new guardrail.
  Warn-only, and ❌ no allowlist was touched. Trim further, or leave.

**Verified in the running app (the closing defects).** With the boot drive fully indexed and a fresh tree beside it: a
scoped MCP `search` over a folder nothing had listed returned the folder itself plus its file (`2 of 2`), and the same
search again — now index-served — returned the same two. `open_search_dialog autoRun: true` over uncovered ground now
fires ONE run (one `cover`, one engine pass, no "leaving frontier root(s) to the walk already covering them") and the
dialog renders `2 of 2 results`, where before it rendered none. Two MCP searches fired 150 ms apart over the same
4,081-folder unwalked tree both come back with the same rows: one walks, the other waits for it and answers from the
index — before the fix the second returned "No files found" under the still-covering note.

**When this lands**: the ~120 MB dev data dir at `~/Library/Application Support/com.veszelovszki.cmdr-dev-unindexsearch`
holds a walk-built index from M10's live verification, and drive indexing is back ON there, so it re-scans on its own.
Delete it whenever.

## Sequencing

Run sequentially. Ordering that matters:

- **M0 is the head.** It sets the one-volume ceiling and deletes the fan-out, so nothing after it has to keep
  multi-volume routing working. Doing it later means writing code twice.
- **M1 next**, and valuable alone: it converts today's silent wrong answer into an honest one, and it unblocks search on
  a machine with no root index, without which every later milestone is unreachable in the running app.
- **M2 is independent of M1.** It is the performance hinge, owns both schema additions (`known_unreadable` and the
  exclusion-policy version), and carries a measured exit criterion. Its `index-crate-isolation` ceiling bump is approved
  (David, 2026-08-04), so it is not blocked.
- **M3a → M3b → M3c → M3d** are sequential among themselves and all depend on M2's data model.
- **M4 is independent of everything before it** (a pure refactor) and can land any time before M5.
- **M5 depends on M0, M2, M3a-d, and M4.** M6 depends on M5 and inherits M2's `known_unreadable` marker.
- **M7 depends on M6. M8 depends on M2's marker and M5's signal. M9 depends on M5.**
- **M11 depends on M3a-d** (there is nothing to watch until a walk covers something) and should land before M10, so
  M10's "Clear index" work drops the branch set M11 persists.
- **M10 is last**, because it measures and documents what everything else built.

## Milestones

`pnpm check --fast` while iterating, the scoped checks named per milestone at each milestone's end,
`pnpm check --include-slow` before wrapping.

**Copy rule for every milestone**: write the user-facing strings in the house voice and move on. David waived the review
gate for this effort specifically on 2026-08-04 ("I don't want to review the strings and translations in this effort"),
so ❌ don't stop to escalate copy. His standing rule that human-facing text is his (`AGENTS.md` principle 6) still
applies everywhere else. Every new key still needs its `@key` translator description, and the translation pass still
follows `docs/guides/i18n-translation.md`; no milestone is done with untranslated keys shipped.

**File length**: `file-length` is warn-only and David does not want it acted on in this effort. Leave warnings standing;
❌ don't raise an allowlist entry, and don't split a file just to silence one.

**Definition of done for the whole effort**: on any volume kind with no index, a search that runs to completion returns
the same result set as the same search on the same volume fully indexed, excepting order, Accepted difference 5
(directory size filters), and Accepted difference 9 (directories the walker abandoned, which stay in the frontier and
are labelled), with the first batch painted within two seconds.

### M0. One volume is the ceiling, and the current folder is the default

The head of the whole effort, because it deletes the machinery every later milestone would otherwise have to keep
working around. Its own DoD: `execute.rs` never routes a search to more than one volume.

- **Scope options become "current folder" (default) and "this volume" (maximum).** `ScopeFilterPopover.svelte` today
  offers the free-text field plus `queryUi.scope.useCurrentFolder` (`:146`) and `queryUi.scope.allFolders` (`:157`).
  "All folders" goes away and `⌥V` rebinds to "this volume". Draft the copy for David.
- **Default to the focused pane's current folder, on every drive.** `searchable-folder.ts` already walks pane history
  back to the most recent real folder; when it returns `disabled` (`searchable-folder.ts:60`, a snapshot pane with no
  real-folder history), fall back to "this volume".
- **Delete the fan-out.** With one target the k-way merge across volumes, `ColdVolumePolicy` and its `DeferColdVolumes`
  arm, `RunOutcome::deferred_volumes`, `warm_in_background`, and the re-run-on-`search-index-ready` path all lose their
  reason to exist. Remove them rather than leaving them dormant; `deadcode` and `knip` will not catch
  dormant-but-reachable code, and a future reader cannot tell it is unused. Keep `all_indexed_volume_ids` only if
  something outside search still needs it.
- **Recents side effect**: scope is a free-text expression persisted into every recent search
  (`SearchDialog.svelte:366`, `:398`), so a defaulted scope means every saved recent search carries a machine-specific
  absolute path. Decide whether the default scope is persisted at all; the answer is probably no.
- **Onboarding and website copy** sell "Instant search of your whole drive. Think Spotlight, but even faster."
  (`onboarding.stepOptional.indexing.benefit1`). "This volume" keeps that promise true, but the default is narrower now,
  so re-read those strings and draft any change for David.
- `search-index-ready` currently means "root's arena loaded". With one target it should name its volume, which M1 also
  needs.

Tests, **test-first**: a search with a scope spanning two volumes is impossible to express (the ceiling holds at the
API, not only in the UI). Written after: `searchable-folder.test.ts` and `SearchDialog.svelte.test.ts` scope cases
including the `disabled` fallback, the new chip, the `⌥V` rebind, and the recents behavior; the engine still returns
identical results for a single-volume query after the fan-out removal (the existing multi-volume tests are the oracle
for what is being deleted, so read them before deleting). Docs: `search/CLAUDE.md` (the multi-volume must-know goes
away), `search/DETAILS.md` § Merge, `query-ui/CLAUDE.md` (the scope shortcuts must-know), `lib/search/CLAUDE.md` +
`DETAILS.md`. Checks: `pnpm check rust`, `pnpm check svelte`, `pnpm check desktop`.

### M1. Search asks the question, and answers it honestly

- **Make `isIndexReady` per-target.** `query-runner.svelte.ts:161-167` returns before `runQuery()` when the ROOT arena
  is not loaded, so on an index-less machine search never runs at all and every later milestone is unreachable. The gate
  becomes "is this search's target ready", not "is root loaded". Related: `search-index-ready`
  (`commands/search.rs:70-72`) currently means "root's arena loaded" and needs to say which volume.
- Note that `queryUi.results.indexNotReady` is **not** the first-scan gate: it renders on `!isIndexAvailable`
  (`query-ui/QueryResults.svelte:391-394`), reached only from the `catch` of `prepareSearchIndex()`
  (`SearchDialog.svelte:496-498`), and `prepare_search_index` returns `Ok(ready: false)` during a first scan
  (`commands/search.rs:78-82`). It is a backend-unavailable state. Do not "fix" it here.
- Render `uncoveredScopes` and `unresolvedScopes` with distinct copy. Branch on emptiness, never on message text.
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
- **Two schema additions, both here**: `known_unreadable` on directories, and an exclusion-policy version in `meta`. (An
  earlier draft listed a third, `listed_at` "for Decision 4's expiry". There is no expiry: Decision 9 settled that
  search-written coverage is watched rather than aged out, and Decision 4 is now the one-volume ceiling. Nothing needs
  the column, so it isn't built.)
- Exposed on the `Index` handle. Adding a `pub` there is a design act (`handle/CLAUDE.md`); record it in
  `handle/DETAILS.md` § "The public surface".
- **Precondition, approved by David on 2026-08-04**: `index-crate-isolation` ceilings are set with no headroom by design
  (`RootPromises: 44`, `HandleMethods: 35`, `scripts/check/checks/index-crate-isolation.go:85-86`). There is no spare
  slot: `handle/CLAUDE.md:11` describes the surface as 34 items, and `index-crate-isolation.go:80-82` explains the gap,
  "`HandleMethods` is 35 rather than the audit's headline 34 because this count includes `Index::builder`, the
  constructor". So the count already sits at its ceiling. This plan adds at least three handle methods (the coverage
  query, M3b's scoped scan, and an epoch read: `IndexStore::read_current_epoch` at `store/meta.rs:62` is not on the
  handle), plus the coverage-answer type and any new error enum as `RootPromises` items if re-exported from `lib.rs`, so
  **both ceilings** move. David approved the bumps with one instruction: design the resulting surface to be cohesive,
  then raise the ceilings to match. ❌ Don't bump per method as you go.
- **Exit criterion, measured**: a recorded note in `docs/notes/` over a real 611,699-folder root index, budget under 50
  ms warm for the frontier query. `dir_stats` has `entry_id INTEGER PRIMARY KEY` and no other index
  (`store/mod.rs:588-597`); `idx_parent_name_folded ON entries (parent_id, name_folded)` (`:586`) gives the descent a
  leading-column seek but is not covering (`listed_epoch` and `is_directory` are not in it), so each child costs a
  main-table fetch plus a `dir_stats` PK lookup. If the budget misses, add the index here rather than discovering it in
  M5.

Tests, **test-first**: a proptest that the frontier partitions the subtree (every path produced exactly once by exactly
one of covered, frontier, or known-unreadable). Confirm first that every listed directory gets a `dir_stats` row, or the
partition premise is false. Written after: a single uncovered leaf yields the leaf, not the root; a cold volume yields
the scope root; an honest-stale gap on an otherwise complete boot index yields only that gap; a fully covered scope
returns an empty frontier **with a coverage assertion that every directory was considered**, or it passes on a no-op
(`docs/testing.md`). `cargo mutants` over the new module before the milestone closes. Docs: `indexing/read/DETAILS.md`,
`store/DETAILS.md` (the schema additions), `handle/DETAILS.md`, the benchmark note. Checks: `pnpm check rust`,
`pnpm check rust-tests`.

**Landed 2026-08-05.** One thing the plan got wrong, worth carrying forward: **the partition property alone does not
catch the degenerate rule.** "The scope root is the whole frontier" partitions the subtree perfectly and is exactly the
useless answer the `min_subtree_epoch`-only descent produces, so the proptest as specified passes on it (verified: it
did). What catches it is a second property — every verdict has to match its directory, so a frontier cut must be a
directory nothing has listed. Both proptests are in `read/coverage/tests.rs`.

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
exactly the large subtrees whose sizes matter most. That is Accepted difference 9, not a reason to reject the parallel
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
- **Cut at volume boundaries, and only there.** With Decision 4 a search targets one volume, so a walk that crosses into
  another one has left its scope. Real mounts need a device check, and the batched macOS read does not carry one today
  (it requests `ATTR_CMN_RETURNED_ATTRS | NAME | OBJTYPE | MODTIME | FILEID` plus file attrs, no `ATTR_CMN_DEVID`,
  `scanner/walker/bulk_read.rs:126-129`), so this needs a per-directory `stat` or a new attribute. **File Provider
  domains are NOT a boundary** (Decision 16): Dropbox, iCloud Drive, and Google Drive report the same `st_dev` as
  `$HOME` and never appear in `mount` (`scanner/file_provider.rs:7-9`), and they belong to the boot volume's scope, so
  the walk descends into them. `file_provider::domain_id_for_dir` (wired as `RootProbes::is_domain_root`,
  `scanner/exclusions.rs:140`, `:152-155`) stays used only for what it does today; do not repurpose it as a cut. The
  guarded walker's stall detection is what makes descending into a disconnected provider mount safe.
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

### M3d. The scoped walk on SMB, MTP, and every future volume kind

Local-only was never the intent; it was a deferral this milestone closes. `network_scanner` has no scoped walk today:
its entry points are `network_scanner/full_scan.rs:126 scan_volume_via_trait` (whole volume, maps the scan root to
`ROOT_ID`) and `reconcile_scan.rs`, which diffs against an already-populated index. So this builds a scoped BFS over the
`Volume` trait, which is what makes it work for every future backend for free.

- Reuse what exists: `begin_scan_session` / `end_scan_session` bracket bulk work so SMB's refcounted extra-session pool
  is used rather than re-invented, and `network_scanner` already carries scan pacing and NAS system-dir skips.
- A full walk of a 10 TB NAS measured about 11 minutes on David's hardware, and cancel works throughout, so this needs
  no gate or confirmation step.
- MTP is the slow end and the one with sharp edges (same-name siblings, an easily wedged transport), so the cancel path
  and the IPC deadline rules in `commands/CLAUDE.md` matter more here than anywhere else.
- Every volume kind uses the same coverage epochs and the same writer, so the frontier query and the descent rule need
  no per-kind branches.

Tests, **test-first**: a scoped walk over an `InMemoryVolume` reaching only its subtree, and a cancelled network walk
leaving durable partial coverage exactly as the local one does. Written after: session bracketing is paired even on the
cancel path; pacing is honored. Docs: `network_scanner/CLAUDE.md` + `DETAILS.md`, `scanner/DETAILS.md`. Checks:
`pnpm check rust`, `pnpm check rust-tests`.

### M4. Compiled query: one matcher, two evaluators

A pure refactor with no behavior change, split out so the "matching must not fork" invariant is independently
verifiable. **It stays app-side in `search/`** per Decision 3; the walk delivers batches, not callbacks.

- Factor the pattern compile plus the size, date, and type predicates out of `engine::search_ranked` into a compiled
  query value that both the arena scan and a batch of walked entries can evaluate. `engine.rs` stays pure and I/O-free.
- **Directory size filters stay outside this**, per Accepted difference 5: a directory's size is overwritten after
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
rather than swallowed; `excludeSystemDirs` absent by default and present when off; cancellation stops the walk promptly;
the disconnect terminal state. Plus a contract test for the new IPC event family (`docs/testing.md` § "When you add X,
also add Y"). Docs: `search/CLAUDE.md` (the one-way-consumer must-know needs its nuance), `search/DETAILS.md`,
`indexing/CLAUDE.md`. Checks: `pnpm check rust`, `pnpm check rust-tests`.

### M6. Streaming results in the query UI

**What M5 hands you.** `commands.searchFilesStreaming(query, runId)` returns `{ runId, targetVolumeId }` as soon as
routing has picked a volume (a scope spanning two is the error branch), and `commands.cancelSearch(runId)` stops one.
The caller mints the run id, exactly as it does a `listingId`, so no event can arrive against an id the frontend hasn't
seen. Then:

- **`search-progress`** — `phase` (`resolvingCoverage` | `readingIndex` | `walking`), `entries` (arrival order, ≤100),
  `matchCount` (the run's total so far, past the cap included), `dirsFound` + `currentPath` (Decision 14's progress),
  `capped`.
- **`search-complete`** / **`search-cancelled`** — `matchCount` plus `coverage`: `walk` (`nothingToWalk` | `completed` |
  `interrupted` | `cancelled`), `unreadable`, `stillCovering`, `unresolvedScopes`, `capped`, `targetVolumeId`.
- **`search-error`** — a typed `error` (`query` | `indexUnreadable`) plus the sentence to show.

Five things that will bite otherwise:

- **A superseded run goes silent — including its terminal event.** Starting a run supersedes every other one backend
  side, so ❌ don't wait for a `-complete` on a run you've replaced; drop its id and move on.
- **`unreadable` has two causes and needs two sentences**: a folder Cmdr was refused (grant Full Disk Access) and a NAS
  snapshot tree nobody walks. Same list, different copy.
- **`walk: interrupted` is the drive-went-away state**, not an error, and it means the list is a lower bound.
- **A live row carries no `entryId` and no directory size** (Accepted difference 5), so anything keyed on either has to
  tolerate their absence.
- **Count-only is "N so far"** until the terminal event, and can over-count by the overlap (Accepted difference 12).

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
  labelling covers a walk that finished but abandoned directories (Accepted difference 9). Same labelling for the
  disconnect and quit terminal states (Accepted difference 1).
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

### M9. MCP stays a thin wrapper — LANDED

Per Decision 10 there is no agent-specific policy left to add, so this milestone is mostly deletion and verification.

- Drop the `ColdVolumePolicy::Wait` versus `DeferColdVolumes` split at the MCP boundary. Decision 4 removes the only
  situation it applied to (unscoped extra volumes), so both callers now take the same path with the same arguments.
  **Already done**: M0 deleted the type with the fan-out, and nothing survived at the MCP boundary.
- An agent search walks exactly like a person's, including streaming and cancellation semantics where the transport can
  carry them. **What that means over a one-shot reply**: the rows that had arrived when the wait ran out, and no cancel
  (the walk keeps going). See the execution-status entries.
- Keep the typed coverage signal in the MCP reply, which is the one thing MCP already rendered and the dialog did not.

Tests: an agent search over a partially covered volume walks and returns the union, same as the dialog. Written after.
Docs: the MCP tool docs, `search/DETAILS.md`. Checks: `pnpm check rust`.

### M11. Watch what the walk covered — LANDED

What makes Decision 9 real, and what lets the plan carry no expiry.

- Register a watch on the highest branch covering any walk-covered folder, and discard events outside those branches. On
  macOS this is a path-prefix test over the drive-root FSEvents stream the watcher already runs (`watch/watcher.rs`).
  Restart persistence comes free from `supports_event_replay()` plus the FSEvents `sinceWhen` replay already in place.
- **On Linux, do not copy the macOS shape.** `notify`'s recursive mode registers an inotify watch per directory against
  `max_user_watches`, so watch only the covered branches there rather than the drive root.
- **The boundary case that silently corrupts sizes**: when a walk adds a new branch, events arriving for it _while the
  walk runs_ must not be discarded, or the branch's aggregate drifts with no signal. The scan-completion handshake and
  `BulkReconcileGuard` are the existing hooks.
- A drive with the per-drive veto set gets no watch (Decision 13), so its walked branches re-walk instead of staying
  live.
- The branch set is persisted, so it survives restart, and per-drive "Clear index" drops it along with the coverage.

**What landed differs from the above in five places**, each recorded in the execution status: there was no stream to
filter (a walk-built index has no watcher at all), restart persistence resumes with the volume rather than at launch,
mid-walk events are BUFFERED rather than merely admitted, a sweep above the branches is re-anchored, and the same
buffering had to apply to scanned volumes too. "Clear index" needed no work: it deletes the database the set lives on.

Tests, **test-first**: an event inside a covered branch updates the index and one outside it is discarded; a walk that
adds a branch does not lose events that arrive mid-walk. Written after: restart restores the branch set; a vetoed drive
registers no watch. Docs: `watch/CLAUDE.md` + `DETAILS.md`, `lifecycle/DETAILS.md`. Checks: `pnpm check rust`,
`pnpm check rust-tests`.

### M10. Settings, analytics, status, and the doc sweep

- **The Clear button and the size indicator must work with drive indexing off.** That is the whole of David's answer on
  disk usage: no cap for now, but someone who declined indexing and then searched must be able to see how much the index
  holds and clear it. Check `DriveIndexingSection.svelte`'s disabled states, since today they assume no data exists when
  the master switch is off.
- **A full rescan evicts rather than refills** (Decision 17). Wire it wherever coverage is rebuilt from scratch, and
  state it in `resources/DETAILS.md` alongside the existing retention cap, which is untouched by this plan (32 external
  index DBs, oldest-by-mtime, never evicting root).
- **Analytics.** Extend `search_used` with categorical properties: coverage (covered, live, mixed), walk duration
  bucket, cancel rate, whether the walk was superseded, CTA conversion. Property shapes per
  `apps/desktop/src-tauri/src/analytics/DETAILS.md`, which documents `search_used` today.
- **`feature-status.json`**: the `search` note says "finds files across every indexed drive". Rewrite once true.
- Sweep the area docs named per milestone; confirm `docs-reachable`, `dead-links`, `claude-md-length` are clean.
- Record the `content_epoch` forward-compat note in `writer/DETAILS.md`, where the propagation lives.

Tests: the Clear button and size indicator work with the master switch off; the analytics events fire with the right
properties per coverage kind. Written after. Checks: full `pnpm check --include-slow`.

## Settled with David, 2026-08-04

Recorded so nobody reopens them. Every one of these was an open question in an earlier draft.

- **One volume is the broadest scope**, which removes the unscoped case rather than deferring it (Decision 4).
- **Live walks on SMB and MTP, and every future volume kind.** A full walk of a 10 TB NAS is about 11 minutes and cancel
  works throughout, so no confirm step and no per-drive gate (M3d).
- **The default scope narrows for indexed drives too.** David confirmed he wants this knowing it changes behavior for
  people whose drives are fully indexed (M0).
- **No expiry on search-written coverage; watch the branches instead** (Decision 9, M11).
- **The master-switch settings copy stays as written**, even though folder sizes will appear for walked branches
  (Accepted difference 11).
- **Both indexing switches govern background work only**, so a search walks a `user_disabled` drive but leaves no
  watcher on it (Decision 13).
- **`index-crate-isolation` ceiling bumps are approved**, with the instruction that the resulting API stay cohesive.
  Design the surface, then raise the ceiling to match; don't raise it per method as you go.
- **No off switch for live walking**, in any form (Decision 15).
- **No index size cap for now.** The Clear button and the size indicator must work with drive indexing off, and a full
  rescan evicts rather than refills (Decision 17, M10). If people complain about disk use, a cap can come later.

## Out of scope

- **Space-to-size on a folder.** It reuses M3a-d and M11 directly and lands right after, per David. By then it is mostly
  a trigger: pressing Space on a folder runs the same scoped scan a search runs over a frontier.
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

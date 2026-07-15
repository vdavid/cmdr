# Media-index polish plan

Finish and polish the image-search (media-ML) feature that landed with `docs/specs/media-ml-index-plan.md` (M1 + M1.5 +
M2). A hands-on QA session (David, 2026-07-15, dev app) surfaced three bugs, one privacy gap, and two missing UX pieces.
This plan fixes all of them. The diagnosis below is evidence-based (code reading + the dev app's data dir and logs), not
speculation; executors should still re-verify the load-bearing claims before coding against them.

## Diagnosis (what QA found, and why)

Symptoms observed in the dev app (223,228 qualifying images on `root`):

1. Enabling the master toggle did nothing: "0 of 223,228 images indexed" forever, no CPU activity.
2. The slider preview showed "Working out how much this covers…" indefinitely, at every slider position including max.
3. Moving the slider had no visible effect of any kind.

Root causes, confirmed:

- **The master toggle doesn't start anything — and neither does a restart with the toggle on.** `set_image_index_enabled`
  (`commands/settings.rs`) only flips `gate::set_enabled`; no pass is kicked. Worse, the "startup sweep" doesn't
  actually enqueue passes for Fresh-at-launch volumes: `wire_volume` only spawns a pass when the bus's RETAINED value is
  already `Completed`, and a volume Fresh at launch keeps a `Pending` bus (it never re-fires `ScanCompleted`). So the
  sweep merely wires subscriptions for future rescans — meaning even a user whose toggle is persisted ON gets "0 of N,
  forever" after every restart until some volume happens to rescan. (The scheduler's own module docstring claims the
  sweep "enqueues once here" for Fresh-at-launch volumes — that claim is wrong and must be corrected, not propagated.)
  Evidence: no `media-*.db` existed in the data dir after hours of the toggle being on. The network opt-in command
  (`media_index_set_network_volume_enabled`) already kicks an immediate pass — the master toggle, the threshold setter,
  and scheduler startup all need the same treatment.
- **media_index thinks importance never scored the volume.** Both `MediaScheduler::folder_scores` and
  `coverage::importance_scores` gate on `recompute_generation().unwrap_or(0) == 0` → "never scored" → `None`. But
  `recompute_generation` is only stamped by a FULL importance recompute (`importance/writer.rs::apply_full_pass`, in
  the same transaction as the row replace); the INCREMENTAL path deliberately never bumps or stamps it. The dev
  `importance-root.db` was delete-and-recreated by the importance schema-3 bump and has since seen only incremental
  rescores: 233,196 live weight rows, **no `recompute_generation` meta row**. So media_index reported `pending`
  forever, and the covered count was 0 at every threshold. Search meanwhile used those same 233k weights happily — the
  weights are fine; the "has it scored?" check is wrong for incrementally-maintained volumes.
  - **Latent second bug underneath: why did no full recompute run after the schema recreate?** The importance scheduler
    is documented to drive full recomputes off "bus `ScanCompleted` + sweep" (`importance/CLAUDE.md`), and the app ran
    for hours. Prod's db (schema 2) has generation 2, so full passes used to run. The executor must root-cause this
    (likely candidate: the startup sweep path for a volume that is Fresh at launch — no new `ScanCompleted` edge — with
    a freshly recreated store). This matters beyond dev: **every prod user's importance db gets recreated by the
    schema-3 update**, and if no full recompute follows, image indexing (and anything else keying on the generation)
    sees "never scored" indefinitely.
- **The covered-count preview never re-polls.** `MediaIndexImportanceSlider.svelte` fetches the preview once on mount
  and on slider changes only. The per-volume progress line polls every 3 s; the preview doesn't, so a `pending` result
  ("Working out how much this covers…") sits forever even after the backend could answer.
- **Privacy gap: excluding a folder doesn't delete its rows.** The excluded-folder veto stops FUTURE enrichment only
  (`commands.rs` says so explicitly). GC can never collect those rows either: `enrich_and_gc` keeps every walked image
  in the GC `current` set regardless of gates (deliberately — that's the "slider left never wipes your index" data-
  safety line), so an excluded folder's already-extracted OCR text stays in `media.db` and stays searchable forever.
  For the threshold that behavior is a feature; for the privacy veto it's a bug.
- **No reclaim path.** Rows-persist semantics (correct) mean lowering the slider leaves orphaned coverage. There's no
  way to see or delete it, and the settings progress line ("N of M indexed") uses the full qualifying count as its
  denominator, which reads as "never finishes" whenever the slider is left of max.
- **No live progress surface.** The enrichment pass only logs; the settings line is polled at 3 s granularity; the
  top-right indexing indicator knows nothing about image indexing. "Full counts + ETA are a later milestone" is written
  into `commands.rs` — this plan is that milestone.

## Design intentions (read before coding)

- **The slider stays forward-only.** Moving it never deletes rows; deletion is only ever an explicit user action (the
  reclaim button) or the privacy veto. This preserves the existing GC data-safety line: GC's `current` set stays the
  full walked image set, and the ONLY row-deletion paths are (a) vanished files via GC on a Completed edge, (b) the
  user-explicit prune, (c) the privacy retro-delete. Never derive deletion from scan state or gate state.
- **Honest progress, never fake.** Rates come from a sliding window (reuse the frontend `eta.ts` machinery and its
  design rules); show an ETA only once the rate is stable; never a fabricated percentage. Per-volume ETA is fine; no
  overall cross-volume ETA (consistent with the drive indexer's deliberate per-step-only ETA decision).
- **One progress surface, multiple publishers.** Image indexing joins the EXISTING top-right indicator
  (`lib/indexing/IndexingStatusIndicator` → `IndexingStatusBody`) as a sibling row kind, not a second corner widget.
  The indicator's visibility gate ORs in "any volume enriching".
- **The importance fix belongs at the importance layer first.** Make the missing-full-recompute bug impossible (a
  recreated/fresh store must get a full pass), then ALSO make media_index's read-side check robust (a volume with live
  weights is "scored" even if the meta row is absent) so mid-life edge cases can't regress it. Fix both; don't paper
  over the scheduler bug with only the read-side check.
- **Respect the app's rules**: business logic in Rust, IPC commands thin and async with `spawn_blocking` for DB work,
  typed enums over string matching, i18n keys for every user-facing string, sentence case, active voice, a11y parity
  with the existing indicator rows, `lock-poison`/`unwrap` rules, no rayon for macOS frameworks.

## Milestones

Sequencing: M1 and M2 first (they unblock everything user-visible), then M3 → M4 (prune machinery before reclaim UX),
M5, M6, M7 last. Safe to parallelize across worktree-internal agents ONLY where files don't overlap; see notes per
milestone. Sequential is fine and preferred when in doubt.

### M1 — Acting on settings actually starts work

**Backend.**

- New scheduler entry point (e.g. `scheduler::kick_all_ready_passes(app)` or a method on `MediaScheduler`): iterate
  `crate::indexing::ready_volumes_with_kind()`, spawn a coalesced pass per volume with the existing kind mapping
  (Local → local pass, Smb → network pass which self-checks opt-in, Mtp → never). Reuse `spawn_pass`; the
  `PassCoordinator` already folds concurrent requests.
- **Three call sites**, covering both dead-start mechanisms:
  1. `set_image_index_enabled(true)` (command gains `AppHandle`; mirror `media_index_set_network_volume_enabled`'s
     shape). Also delete that command's stale doc-comment line "(The frontend toggle UI lands in a later slice.)".
  2. The end of `scheduler::start()`, when `gate::is_enabled()` — this fixes the restart case: a Fresh-at-launch
     volume's bus is `Pending` and never re-fires, so without this a persisted-on toggle still never enriches. Fix the
     scheduler module docstring's wrong "the sweep enqueues once here" claim at the same time.
  3. `media_index_set_importance_threshold`, but ONLY when the threshold DECREASES (coverage broadens): the newly
     covered folders should start enriching without waiting for the next scan. A raise only defers future work
     (forward-only semantics — nothing to do now), so kicking on a raise would re-walk the index for nothing.
     Mechanically: the command gains `AppHandle` (like the toggle) and compares the incoming value against
     `gate::importance_threshold()` BEFORE setting it. The slider commits at most a handful of times per drag
     (discrete buckets) and the coordinator coalesces, so the decrease-kick is cheap. Intention: the user's mental
     model is "slider right → more gets indexed, now".
- **Change the local `folder_scores == None` fallback from "enrich all" to "defer, and re-kick when importance
  scores".** Without this, the startup/enable kick races the importance recompute: the pass reads `folder_scores` once
  at start, importance's multi-second full recompute over a big volume hasn't finished, `None` → the pass enriches the
  ENTIRE volume regardless of the slider — and forward-only semantics make that permanent until a manual reclaim. The
  "next pass applies the threshold" the current comment promises is never wired (media subscribes only to the lifecycle
  bus, never to importance). Concretely: (a) a local pass seeing `None` defers the importance-gated remainder while
  still honoring explicit always-index overrides (`config.covers`) — EXACTLY the network `None` fallback's shape, so
  the two paths stay symmetric and a user's explicit directive is never postponed; (b) the scheduler subscribes to
  `importance::read::subscribe(volume_id)` (already exists, currently
  unconsumed by media) and requests a coalesced pass when the volume transitions unscored → scored. **The subscription
  MUST be established synchronously in `wire_volume`, before and independent of the first pass — never lazily after a
  pass observes `None`.** Watch-channel semantics: a receiver is caught up to the current version at subscribe time, so
  `changed()` fires only on the NEXT bump. A lazy "read `None` → then subscribe" flow has a hole: importance completes
  in the gap, the receiver comes up already-caught-up, the volume defers forever. Mirror the existing consumer pattern
  (`search`'s `start_importance_weight_subscriber`: subscribe up front, then initial load, then loop). Scope the
  re-kick to the unscored → scored bridge; don't re-kick on every later bump (the incremental path notifies up to once
  per throttle window under file churn, and a per-bump re-walk is a standing cost for nothing — later threshold
  application rides the natural pass triggers). The bus carries only a generation number and incremental passes re-fire
  it, so detecting the bridge needs a small per-volume "was deferred" flag on the scheduler — set when a pass defers,
  cleared (with a kick) on the next bump. The network fallback (`None` → override-only) stays as is: conservative
  is correct when unscored, and the same bridge re-kicks it too.
- This is a deliberate contract change vs the original media-ml plan's Decision (which chose enrich-all); with M2
  guaranteeing every fresh/recreated store a full recompute trigger, "importance will score every ready local volume"
  becomes an invariant we can lean on. Update the scheduler comment and `media_index/CLAUDE.md`'s "the fallback
  DIFFERS" must-know accordingly. **The residual risk must be VISIBLE, never silent**: M2 guarantees the recompute
  trigger, not its success (a read-pool or write error leaves generation 0 with no notify). Under defer-until-scored
  that failure means image indexing silently never starts — and the settings preview would show the exact "Working out
  how much this covers…" spinner this plan exists to kill. So: expose the deferred state as a typed, distinct field on
  `media_index_volume_state` (e.g. `waitingForImportance`), and have the settings UI voice it honestly ("Working out
  which folders matter — image indexing starts right after") instead of the generic spinner. Composition rule so the
  panel never shows two spinners for one root cause: when `waitingForImportance` is true, this copy REPLACES the
  covered-count preview's generic "Working out how much this covers…" pending text (same underlying wait; one honest
  line). Deliberately NO silent
  fallback to enrich-all on timeout: over-indexing is permanent-until-reclaim, a visible wait is recoverable; a
  persistently failing importance recompute is an importance bug to surface and fix, not to paper over.

**Frontend.**

- `MediaIndexImportanceSlider.svelte`: re-poll the covered-count preview while it's unresolved (`covered === null` or
  `covered.pending`), for example by folding a `refreshPreview(bucket)` call into the existing 3 s state timer under
  that condition. Stop polling once resolved (the pass-completion invalidation already keeps later fetches honest).

**Tests (TDD, red→green — this is a bug fix).**

- Scheduler-level test with `FakeVisionBackend` + a registered fake volume: gate off at "startup", enable + kick →
  the pass runs and enriches (this is the regression test for the dead-start). Verify coalescing still holds (kick
  during a running pass folds into one re-run) — extend `coalescing_tests.rs`.
- Restart-case test: gate already on, volume ready with a `Pending` bus (never `Completed`) → the startup kick still
  enriches it. This is the second dead-start mechanism; without this test the fix can regress silently.
- Threshold-kick test: a decrease triggers a pass request; a raise does not.
- Defer-until-scored test: enable + kick with importance unscored → the local pass defers the importance-gated set
  (an always-index override still enriches); the importance recompute completes → a pass runs and respects the
  threshold. This is the slider-integrity regression test — without it, first-run over-indexing can silently return.
  Setup note: seed a GENUINELY empty importance store (no weights, no generation) — with any weights present, M2's
  `scored_folder_count() > 0` fallback reads the volume as scored and the test never defers.
- Fresh-sweep GC test: the kick-triggered pass GCs only against the Fresh snapshot's complete walked set (this kick
  makes the documented-but-never-fired Fresh-sweep GC path live for the first time).
- Frontend: extract the "should re-poll?" decision into a pure helper and unit-test it (component-mount polling itself
  can stay untested if the helper is covered).

**Docs.** `media_index/CLAUDE.md` must-know list: add the "what starts a pass" triad (bus edge, startup sweep, user
action kicks). `DETAILS.md`: the intention note.

**Checks.** `pnpm check --fast` while iterating; `pnpm check rust svelte` at milestone end.

### M2 — Importance "has scored" detection, and the missing full recompute

**The mechanism is near-certain (verify with the failing test, then fix — don't re-investigate from scratch): this is
the SAME bug as M1's restart case.** `publish_scan_completed` fires only from a LIVE scan finishing
(`apply_freshness_event_on` on a `ScanCompleted` event); a volume loaded Fresh at launch from persisted freshness keeps
the bus's initial `Pending`. BOTH schedulers' `wire_volume` gate their startup spawn on the retained value being
`Completed`, so neither media NOR importance ever enqueues for a Fresh-at-launch volume — both sweeps only wire
subscriptions. Importance "works" in prod purely because its generation persists across sessions from earlier live
scans; the dev store broke because the schema-3 recreate reset it while root stayed Fresh ever since. The existing
indexing test around `ready_volumes_with_kind` checks only that volumes are SURFACED, not that anything enqueues —
which is how this slipped through. Write the failing test that captures enqueue-on-Fresh-with-empty-store, then fix.
The two milestones deliberately fix the shared mechanism with DIFFERENT policies — M1 kicks every ready volume
unconditionally (cheap: staleness makes a redundant pass a fast no-op), while importance must gate on "store has no
generation" (an unconditional kick would re-score every volume on every launch). Also correct the wrong docstrings on
BOTH sides: media's scheduler module doc (M1 covers it), importance's scheduler module doc ("The sweep enqueues those
once at startup"), and the `ready_volumes` comment in `indexing/state.rs`.

**Fix (both layers):**

- Importance layer: the invariant to establish is "a store with no `recompute_generation` meta row (fresh or recreated)
  gets a full recompute as soon as its volume's index is ready". **The trigger must bind to the recreate/store-open
  event itself, NEVER a sweep-time generation read.** Why: the schema delete-and-recreate happens lazily, only inside
  `ImportanceStore::open` on the first WRITE-path open (`writer_for` → first recompute/incremental/visit); the read
  path never recreates. So on the schema-upgrade launch, a sweep-time `recompute_generation()` probe still reads the
  OLD schema's stamped generation ("already scored"), skips the full pass, and THEN the recreate fires on the first
  incremental write — generation gone, trigger already passed, stuck forever. That ordering is exactly the prod-upgrade
  path. Sound shapes: detect the recreate inside `ImportanceStore::open`/`delete_and_recreate` and surface "this store
  was just recreated" to the scheduler (which then enqueues a full pass), or force the write-path open (`writer_for`)
  BEFORE any generation-based trigger decision. The executor picks after root-causing; the constraint is the binding,
  not the mechanism.
- media_index read side (defense in depth): treat a volume as scored when it has live weights, not solely when the
  generation stamp exists. Reuse the EXISTING probe — `ImportanceIndex::scored_folder_count()` already returns the
  weight-row count and short-circuits to 0 for a missing db; fall back to `scored_folder_count() > 0` when the
  generation reads 0, in both `MediaScheduler::folder_scores` and `coverage::importance_scores`. Don't add a new
  method. Intention: media_index's fallback semantics (`None` = enrich-all local / override-only network) should key on
  "importance genuinely has no data", and 233k live rows is data.

**Tests (TDD).** Failing test first: recreated store + incremental-only writes → `folder_scores` returns `Some`
(after the fix) / the scheduler enqueues a full pass (after the fix). The prod-upgrade test MUST drive through the real
lazy-recreate ordering — old-schema db present at "launch", generation readable, recreate firing only on the first
write-path open — otherwise it will pass against a sweep-time-probe fix that fails in production.

**Docs.** `importance/DETAILS.md`: the generation-stamp semantics and the recreate-triggers-full-pass invariant, plus a
one-line guardrail in `importance/CLAUDE.md` if the fix adds one. Note in the changelog draft that this affects all
users on the schema-3 update.

**Overlap warning:** touches `media_index/scheduler/mod.rs` (`folder_scores`) — coordinate with M1 (same file); run
M1 and M2 sequentially or as one agent.

### M3 — Privacy retro-delete + prune machinery

**Backend.**

- Writer capability: batch row deletion by explicit path list and by folder prefix, plus a post-prune `VACUUM` (the
  writer thread owns the connection; add a writer message, run it off the IPC thread; `media.db` is a disposable cache,
  so `VACUUM` is acceptable and is what actually returns disk space).
- `media_index_set_excluded_folder(folder, excluded: true)` retro-deletes existing rows at or under `folder` across
  volumes, non-optionally. Mind the path spaces: exclusion config is OS-path keyed; local rows store index paths that
  equal OS paths, network rows store mount-stripped index paths — reuse the same mapping `should_enrich` uses
  (`os_join` / mount-root strip) rather than inventing a new one. Invalidate the vector cache + coverage cache for
  affected volumes. **Offline network volumes**: the OS-path → index-path mapping needs the mount root, which an
  unmounted volume doesn't have — so the retro-delete is best-effort per REACHABLE volume and must re-fire on
  reconnect (the registration bus is the hook), or an exclusion set while the NAS is unplugged silently never purges.
- **Close the mid-pass race, or the retro-delete is cosmetic**: both passes evaluate exclusion from a start-of-pass
  `network::config::snapshot()`, so a pass already running over the folder keeps `should_enrich == true` and re-inserts
  the rows right after the retro-delete removes them (the single writer thread only ORDERS the two operations; the
  upsert wins). The privacy veto is a hard veto, not a tuning knob, so it must read LIVE state: have the `should_enrich`
  closures call the live `network::config::is_excluded(path)` for the exclusion check specifically (the
  threshold/override parts may stay snapshot-based).
- **And close the in-flight-analyze TOCTOU the live read alone leaves open**: the pass checks `should_enrich`, then
  runs a SLOW `analyze` (OCR can take seconds on a large image), then upserts — an exclusion landing during the analyze
  slips past a check that already passed, and a re-run pass won't collect the row (still-present files stay in the GC
  `current` set forever). Two-part fix, both cheap: (a) re-check live `is_excluded` immediately before sending each
  upsert; (b) sequence the exclusion command as config-set (live state first) → retro-delete → `flush_blocking` the
  writer → retro-delete once more. Deletes are idempotent and prefix-scoped, so the double-tap costs nothing and
  sweeps any straggler upsert that squeezed into the enqueue window. Order matters: the config write MUST precede the
  first delete, or in-flight images re-check against stale state.
- **Give the veto a user-facing trigger** — without one the privacy fix is unreachable outside raw IPC. Add the
  folder context-menu items ("Don't index images in this folder" / "Index images here again", exact copy per the style
  guide) wired to the existing setter + the frontend persist path (`mediaIndex.excludedFolders`), following the app's
  native-menu shape. Keep it minimal: the exclude/un-exclude pair only; the per-folder "always index" trigger stays
  parked (out of scope, tracked in `media_index/DETAILS.md` § What's left for later).
- Justify the new deletion path against the GC safety doctrine in `DETAILS.md`: it's user-explicit and derives from
  settings state only, never from scan/bus/gate state — that's why it doesn't need a Completed edge.

**Tests (TDD — deletion logic is data-safety-critical).** Failing tests first: exclude deletes rows under the folder
and ONLY those; nested overrides don't resurrect them (exclude beats always-index, same precedence as enrichment);
**excluding mid-pass leaves no re-added rows** — cover BOTH races: a pass holding a pre-exclusion snapshot, and an
exclusion landing between an image's `should_enrich` check and its upsert (mid-analyze — use the fake backend's
hooks to land the exclusion inside `analyze`); un-excluding does NOT re-delete or auto-re-enrich (the next pass picks
the folder up again naturally); network-volume path mapping. FTS rows, tag rows, and embeddings all go (one
transaction per batch).

**Docs.** `media_index/CLAUDE.md`: extend the GC must-know with the two explicit deletion paths, and add the
"exclusion reads live config, never the pass snapshot" guardrail. `DETAILS.md`: the precedence + path-mapping detail.

**Not parallel-safe with M1/M2 after all**: the live-exclusion fix edits the `should_enrich` closures in
`media_index/scheduler/mod.rs`, the same file M1 (kick entry point) and M2 (`folder_scores`) touch, and the retro-delete
command lives in `media_index/commands.rs` alongside M1's threshold command. Run M3 after M1+M2.

### M4 — Reclaim-space UX (depends on M2 + M3)

**Backend.** Two thin IPC commands (both `spawn_blocking`, both `MediaIndex`/writer-mediated, no raw SQL in commands):

- `media_index_reclaim_preview(threshold, volume_ids)` → per the CURRENT setting: how many stored rows fall below
  coverage, and an estimated byte size. The doomed-row SELECTION is Rust-side, not SQL: it joins stored `media.db`
  paths against IMPORTANCE folder scores plus the override/exclude config (three different stores), reusing the same
  precedence logic enrichment uses. Only the byte-size SUM over the already-chosen doomed set is a `media.db` query
  (OCR text + tags + embedding blob lengths — honest "about"). Requires M2 so the folder scores are actually readable.
- `media_index_prune_below_threshold(threshold, volume_ids)` → delete those rows (M3 machinery), `VACUUM`, return
  `{ deletedRows, freedBytes }`. Invalidate vector + coverage caches.
- **Single-source the arithmetic, or the numbers won't add up.** Three distinct quantities reach the user, and they
  must come from ONE Rust function both the reclaim commands and `media_index_volume_state` (M5) call:
  `survivingStored` (stored rows inside current coverage), `doomedStored` (stored rows outside it — M4's "delete N" AND
  M5's `keptCount`, the same set), and `coveredQualifying` (drive-index qualifying images in covered folders — the
  slider preview's number, a DIFFERENT thing: it counts what WOULD be indexed, not what IS). Guarantee
  `totalStored = survivingStored + doomedStored` in the copy, and never present `coveredQualifying` as if it were a
  stored-row count (a vanished-but-not-yet-GC'd file or a half-enriched folder makes them disagree). Partition rule for
  the edge bucket: a stored row whose parent folder has NO importance row at all (floored, or scored away since) counts
  as score `0.0` → doomed (consistent with the enrichment gate, which keys on map membership) — spell this out, or the
  partition silently leaks rows into neither bucket. Two independently computed versions of "outside coverage" WILL
  drift and undermine trust in a destructive action. `coveredQualifying` in this function REUSES the `coverage.rs`
  cache path the slider preview already reads — one computation serving both surfaces, never a second derivation.
- Race honesty: the real serialization guarantee is the ONE per-volume writer thread — the prune and any concurrent
  pass both flow through it, so they can't interleave mid-batch. Lean on that: compute the doomed set (with its
  importance scores) up front, pass it into the writer message, and let the writer delete it as its own serialized
  unit. A pass enriching NEW rows during the prune is fine (disjoint sets by definition — newly enriched rows are above
  threshold or override-covered).

**Frontend.** Under the slider in settings: when stored coverage meaningfully exceeds the current setting (suggest:
excess > 100 images AND > 5% of stored — tune freely), show a line + button:
"You have 200,000 images indexed; your current setting covers 150. Delete the extra 199,850 entries and free about
1.9 GB." Button opens a small confirm (deleting is recoverable but costs re-indexing time — say so), then prunes,
toasts the honest result ("Freed 1.9 GB"), and refreshes counts. All copy through i18n keys, style-guide voice, no
"just". Never show it while `pending`. Copy-composition rule: this reclaim line and M5's kept-rows settings line
describe the SAME set from opposite angles ("still searchable" vs "delete to free space") and can render together —
write them as ONE narrative (the kept line frames the value, the reclaim line offers the space-vs-reindex tradeoff),
never two sentences in tension.

**Tests.** Pure doomed-row selection logic unit-tested (TDD); command-level test over a seeded store; component test
for the visibility rule (extract to a pure helper).

**Docs.** `DETAILS.md` § reclaim; settings sections docs if conventions demand.

### M5 — Progress hub: image indexing joins the top-right indicator

**Backend.**

- The enrichment passes (local + network) publish throttled progress events (suggest: at pass start, then at most every
  500 ms or every 100 images, and at completion/pause):
  - `media-enrich-progress { volumeId, done, total, bytesDone, bytesTotal, phase }` — **`total` and `bytesTotal` are
    summed over the ENRICHABLE subset (images passing `should_enrich`), never the full walked set.** The pass walks
    everything and defers non-covered images, so a raw `images.len()` denominator produces a bar stuck at
    "150 of 223,228" — the original never-finishes complaint rebuilt inside the indicator. `done` counts
    enriched-or-already-current within that same subset. One filter over the in-memory vec; per-image sizes ride the
    walked entries (`ImageEntry.size` — an `Option`; treat `None` as 0, so the bytes bar under-counts rather than
    lies), so the bytes double-bar stays honest and free.
  - `media-enrich-complete { volumeId, enriched, gcCount }`, plus typed terminal variants for the network pass's
    idle/disconnect pauses AND the memory-watchdog cancel (the stop hook makes the pass return early — without a
    terminal event there, the indicator row sticks at "enriching" forever, the exact stuck-row bug `index-scan-aborted`
    was added to fix for drive scans). EVERY pass exit path emits a terminal event — including the `?`-error bubbles
    (writer-send failures) in both enrich cores, not just the enumerated pause/cancel paths. Typed discriminants,
    never strings; a `Drop`-guard or a single exit choke point beats per-return-site emission.
- Keep emission out of the per-image hot path except a cheap counter + time check. Events, not polled state, per
  "subscribe, don't poll".
- Extend `media_index_volume_state` with the threshold-aware split: `coveredQualifyingCount` (qualifying images in
  covered folders at the current threshold) alongside the existing full `qualifyingCount`, plus `keptCount` (stored
  rows outside current coverage — computed by the SAME single-source function as M4's doomed set; see M4). Intention: the settings line becomes "N of M in your covered folders" and can honestly
  reach "done" at any slider position; the kept rows get their own quiet line ("200,000 more indexed from broader
  settings — still searchable").

**Frontend.**

- `index-state.svelte.ts` (or a sibling `media-enrich-state.svelte.ts` if cleaner — keep the SINGLE-source rule: one
  reactive store for enrichment activity): listen for the new events, expose per-volume enrichment activity. **Honor
  the module's "listen first, then query" invariant**: with M1, enrichment starts at backend setup, BEFORE the frontend
  mounts, so the pass-start event is lost — register the listeners, THEN query a snapshot (the M5-extended
  `media_index_volume_state`, or a small enrichment-snapshot command) so an in-flight or already-finished pass renders
  correctly at mount. This mirrors `initIndexState` exactly; don't reorder.
- `IndexingStatusBody`: render an "Image indexing" row per actively-enriching volume alongside drive rows — name,
  double bar (images + bytes), images/min, per-volume ETA via the existing `eta.ts` windowed-rate helpers, paused state
  for network volumes ("waiting until you're idle" / "paused — resumes on reconnect"). The corner indicator's
  visibility gate ORs in "any volume enriching". Follow the indicator's existing a11y model (focusable icon,
  `aria-describedby` tooltip, no `role="status"`).
- Settings progress line switches to the threshold-aware counts from the extended state.

**Tests.** Backend: throttle logic unit test (pure); event payload shape via existing binding tests. Frontend: state
store tests mirroring `index-state.svelte.test.ts`; a11y test for the new row mirroring `IndexingDriveRow.a11y.test.ts`;
`indexing-i18n-parity` will enforce the new keys.

**Docs.** `lib/indexing/CLAUDE.md` + `DETAILS.md`: the second-publisher model and the event table addition;
`media_index/CLAUDE.md`: one line on the progress events; `docs/architecture.md` only if a pointer changes.

**Depends on M1** (touches the pass loop where kicks land) — run after it. The frontend half is parallel-safe with M4's
frontend.

### M6 — Settings home: AI > Image search

- Move the image-search settings (master toggle, slider, reclaim, network volumes) out of Behavior > File system
  watching into a new "Image search" SUB-ITEM under AI — the sidebar is a registry-driven tree and nested sections
  already exist (`['Appearance', 'Colors and formats']` is the precedent), so this is `section: ['AI', 'Image search']`
  on the registry entries; `AI` is already in `TOP_LEVEL_ORDER`, so no top-level change.
- **This is a RESTRUCTURING of the AI section, not a pure addition — budget for it.** AI today is a FLAT leaf section
  (`shouldShowTopLevel(['AI'])` renders `AiSection` directly, and AI is NOT in `SettingsContent`'s
  `sectionsWithSubsections`). There is no "flat parent content + subsections" hybrid — a section is summary-grid parent
  OR flat leaf. Giving AI its first subsection therefore forces: the existing AI provider settings move into their own
  named subsection (suggest `['AI', 'Providers']` — copy is David's call, flag it for his visual review), `'AI'` joins
  `sectionsWithSubsections`, BOTH new routing blocks land in `SettingsContent`, and the `settings.spec.ts`
  `expectedOrder` gains TWO subsection lines under AI, not one. Plus the usual: registry `section` updates, new
  `ImageSearchSection.svelte` composing the existing components — per the sections-CLAUDE checklist. A setting's
  `section` is its ONE home; no mirror rows left behind in the old card.
- Copy: one line making the privacy posture explicit — this runs entirely on the user's Mac via Apple's built-in Vision
  framework; nothing leaves the machine, no AI provider or API key involved. (The AI section otherwise implies the
  configured provider.)
- Settings search must find the moved entries (sectionTitle routing is registry-driven — verify, don't assume).
- **Parallel-safe** with everything except M4/M5's settings-line edits (same components). Run before or after, not
  during.

### M7 — Verification, docs sync, wrap

- Full `pnpm check` then `pnpm check --include-slow`.
- Agent-driven live QA in the dev app (MCP harness), covering the original QA script that failed: enable toggle →
  indexing starts within seconds (no restart; David's dev volume is already importance-scored, so no defer); on a
  GENUINELY-unscored fixture volume: enable → the `waitingForImportance` copy shows (not the generic spinner) →
  importance scores → enrichment starts and respects the threshold; preview resolves to a real count; slider drag
  updates preview + delta;
  slider right→left→right loses nothing; reclaim preview + prune round-trip (on a small fixture volume, not David's
  223k root, unless he says otherwise); exclude a folder via the new context-menu item → its hits disappear from
  search, and excluding mid-pass leaves nothing behind; top-right indicator shows
  the enrichment row with sane rate/ETA; NAS opt-in still kicks and idle-gates.
- Docs pass per `docs.md` rule; changelog entry (impact-focused: "Image indexing now starts the moment you enable it",
  the privacy retro-delete, reclaim, live progress).
- Milestone-by-milestone commits per `git-conventions.md`; leave the branch ready for FF-merge; David reviews visuals
  himself (per project memory — don't screenshot-verify UI on his behalf).

## Execution notes

- Recommended agent split: (A) M1+M2 together (shared files, shared TDD context), (B) M3 after A (shared
  `scheduler/mod.rs` + `commands.rs`), (C) M4 after B, (D) M5 after A, (E) M6 anytime alone, (F) M7 last. The only
  safe parallel pair is E alongside anything; everything else is sequential by file overlap.
- Every agent reads `media_index/CLAUDE.md` + `DETAILS.md`, `importance/CLAUDE.md` (A), `lib/indexing/CLAUDE.md` (D),
  settings CLAUDE.md chain (C/E) before editing.
- The dev app may be running while agents work; nothing here touches its data dir except the live QA step.

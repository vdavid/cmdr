# Scan progress + ETA plan

## Problem

During a full drive scan, the indexing status indicator's tooltip shows live counters ("Scanning… 42,000 entries, 1,200
dirs") but no progress percent and no ETA, and the size-column placeholder tooltips say a hardcoded "Sizes are usually
ready after 3 minutes". Three minutes is wrong in both directions (a packed 4 TB disk takes longer; a fresh Air takes
far less), and a count-up with no denominator tells the user nothing about how much is left. Now that partial sizes
appear progressively during the scan, the fixed copy is also half-obsolete: sizes don't "become ready after 3 minutes",
they stream in.

David's idea, refined in discussion: use "bytes processed / used disk space" as a progress ratio, with the known caveat
that this is an apples-to-oranges comparison (APFS clones, hardlinks, exclusions, purgeable space, firmlinks) — and the
insight that resolves most of it: **after the first scan we have exact same-methodology totals to measure against, so
only the first scan needs the rough ratio.**

## Goal

A two-tier scan progress + ETA, surfaced in the indexing status indicator's tooltip (progress bar + percent + ETA, same
presentation the replay and aggregation modes already have):

- **Tier 1 — every scan after the first ("calibrated")**: progress = `entries_scanned / last_scan_total_entries`, where
  the denominator was persisted by the previous completed scan. Apples-to-apples by construction: same exclusions, same
  hardlink dedup, same firmlink handling on both sides. Entry count is the unit (not bytes) because scan cost is per
  `stat()`, not per byte — it's the smoother clock. ETA from the existing `eta.ts` blend (elapsed extrapolation +
  sliding-window rate), seeded by the previous scan's persisted duration so an estimate exists from second one.
- **Tier 2 — the first scan ("rough")**: progress = `bytes_scanned / used_bytes` of the scanned volume, displayed
  clamped (never reaching the high-90s, never an early 100%) and worded as approximate ("Roughly 2m left"). This is the
  onboarding case, where _some_ honest signal beats none.

Also replace the static "Sizes are usually ready after 3 minutes" copy in the size-column/status-bar tooltips with copy
that matches the progressive-sizes reality.

### Non-goals

- No change to replay or aggregation progress (both already have progress + ETA in the indicator).
- No persistent scan-history analytics; we keep exactly one prior scan's totals (the latest completed one).
- No progress in the menu bar / Dock; the indicator tooltip is the single surface.
- No per-directory progress.
- Linux behaves identically (same meta keys, `statvfs`-based used bytes); no Linux-specific work beyond compiling.

## Why two tiers (the apples-to-apples analysis)

The naive single-tier "bytes / statfs-used" ratio fails our own design principles, with measured evidence:

- **APFS clones**: our per-file `st_blocks * 512` sum overcounts true disk usage by 10–20% (measured: ~905 GB summed vs
  ~746 GB `statfs()`, documented in `indexing/CLAUDE.md`). A clone-heavy disk would hit "100%" with minutes left — and a
  progress bar that lies at 100% is explicitly forbidden by `docs/design-principles.md`.
- **Exclusions**: paths `scanner::should_exclude` skips are still counted in the volume's used bytes (undercount
  direction).
- **Purgeable space / local Time Machine snapshots**: in some "used space" definitions but never scanned. (Mitigated:
  see "the denominator" below.)
- **Firmlinks / volume group**: we scan `/` spanning the system+data volumes; a per-volume figure doesn't line up
  exactly with the scanned tree.
- **Bytes are a bumpy clock**: one 50 GB movie costs one `stat()`; 50 GB of `node_modules` costs millions. Byte-based
  progress sprints through media and crawls through dev trees, so a byte-rate ETA swings wildly.

Tier 1 sidesteps every one of these because both sides of the division come from the same instrument: the previous
scan's final `entries_scanned` counter vs this scan's live one. Drift is whatever actually changed on disk between scans
— typically a percent or two. Tier 2 (the only place the rough ratio survives) is wrapped in honesty: clamping,
approximate wording, and a switch to "Almost done" rather than a fake high-precision tail.

## Key discovery: most of the machinery already exists

1. **The tier-1 calibration is ALREADY persisted — almost entirely.** The completion handler in `manager.rs` already
   writes `total_entries` AND `scan_duration_ms` (and `scan_completed_at`) to the `meta` table, which survives
   `TruncateData` (the truncate deletes only `entries` + `dir_stats`). Better: both values are already read back into
   the specta-typed `IndexStatus` struct (`store.rs`) nested inside `IndexStatusResponse` — they reach the FE today. So
   tier 1's denominator and its ETA seed need **zero new persistence**. The only genuinely new persisted value is
   `total_physical_bytes` (tier 2's future self-calibration isn't even needed — see Design — but we persist it anyway
   for symmetry and debugging: one `UpdateMeta` at the same site). Don't add `last_scan_*`-renamed duplicates of the
   existing keys; reuse them.
2. **The scan progress loop already ticks every 500 ms** (`manager.rs::start_scan`'s reporter task) and already emits
   `IndexScanProgressEvent { volume_id, entries_scanned, dirs_found }`. Tier data rides the same events.
3. **The ETA math is already written and tested.** `lib/indexing/eta.ts` has `computeElapsedEta`, `computeWindowEta`
   (sliding-window rate), `blendEtas`, `formatEta`, `pruneSnapshots` — built for replay, unit-tested, and pure. The scan
   tooltip reuses them with entries (tier 1) or bytes (tier 2) as the unit.
4. **The indicator already renders progress bars + ETA** for aggregation and replay (`IndexingStatusIndicator.svelte`
   - `ProgressBar`); the scan branch is the only one still counter-only. Adding it is symmetric, not novel.
5. **The denominator helper exists.** `local_posix.rs::get_space_info_for_path()` returns
   `SpaceInfo { total_bytes, available_bytes, used_bytes }`; on macOS it prefers
   `NSURLVolumeAvailableCapacityForImportantUsageKey`, which treats purgeable space (snapshots, iCloud caches) as
   available — so `used = total − available` approximates what Finder reports as genuinely used, conveniently excluding
   the unscannable purgeable category. Falls back to `statvfs`; Linux uses `statvfs` directly.
6. **The post-dedup size value is the single thing to hook the bytes counter onto.** `scanner.rs::run_scan` resolves
   each entry's `(logical_size, physical_size, …)` through a match whose hardlink arm (`nlink > 1`) nulls the sizes for
   second+ links. Increment `bytes_scanned` by the **resolved** `physical_size.unwrap_or(0)` **after that whole match
   expression, once per entry** — NOT inside the dedup arm: the `nlink > 1` arm fires only for hardlinked files, so a
   literal "increment in the dedup branch" would skip the `nlink == 1` majority and near-zero the counter. Hooked after
   the match, the live numerator follows the exact dedup rules of the stored totals (directories and symlinks contribute
   0 via their `None` sizes) — apples-to-apples with the stored sums for free.
7. **Late-join already has a pattern.** `index-state.svelte.ts` does "listen first, then query `getIndexStatus`" to
   catch scans started before the FE mounted. The new fields ride `IndexStatusResponse` (which IS specta-typed →
   `pnpm bindings:regen` + the `bindings-fresh` check apply; the event payload structs are plain serde and the FE types
   them by hand).

## Design

### Backend: counters and persistence

- **`ScanProgress` gains `bytes_scanned: Arc<AtomicU64>`** (physical bytes, hardlink-deduped — incremented in the same
  branch that decides the size counts, see Key discovery 6; files with `physical_size: None` contribute 0). `snapshot()`
  grows to a 3-tuple (or small struct — implementer's call; struct reads better at call sites).
- **`ScanSummary` gains `total_physical_bytes: u64`** (the counter's final value).
- **Persistence**: the completion handler in `manager.rs` already writes `total_entries` + `scan_duration_ms` +
  `scan_completed_at` to meta. Add ONE key at the same site: `total_physical_bytes` (the counter's final value). Tier 1
  needs nothing new persisted; the physical-bytes total is for tier-2 cap tuning and debugging (cheap: one
  `UpdateMeta`).
- **Latent bug to fix while here (review finding, two halves): the documented "interrupted scan → fresh rescan" contract
  is broken for every rescan.** The `indexing/CLAUDE.md` gotcha says "Scan cancellation leaves partial data …
  `scan_completed_at` not set, so next startup runs fresh" — but the code only delivers that for a first-ever scan:
  1. **Cancelled scans write the completion keys.** The four `UpdateMeta` sends sit in the completion handler's
     `Ok(Ok(summary))` arm with NO `was_cancelled` guard, and `stop_scan` doesn't abort the detached completion task — a
     user-stopped scan persists _partial_ totals AND a fresh `scan_completed_at`.
  2. **`start_scan` never clears the previous `scan_completed_at`.** So even with the guard from (1), a rescan that's
     killed mid-way (power loss, `kill -9`) leaves the PREVIOUS completed scan's timestamp in meta alongside a
     truncated/partial `entries` table — the next startup sees `scan_completed_at.is_some()` and takes the
     journal-replay path on top of a gutted index instead of the `IncompletePreviousScan` fresh rescan. (Empirically
     confirmed: the progressive-sizes verification had to delete the key manually via sqlite3 to exercise the heal
     path.) Fix both: at scan start, clear `scan_completed_at` (send it through the writer channel immediately before
     `TruncateData` — a `DeleteMeta` writer message, or write empty + map empty→`None` in `read_meta_value`; prefer
     `DeleteMeta` for honesty), and gate ALL completion-handler meta writes behind `!summary.was_cancelled`. Leave the
     calibration keys (`total_entries`, `total_physical_bytes`, `scan_duration_ms`) UNcleared at scan start — they must
     keep describing the last _completed_ scan so tier 1 works during the current scan and after a cancel-then-rescan.
     This is in scope: the feature's correctness rests on the contract, and both halves are a few lines.
- **Prior-scan read**: `start_scan` reads the calibration (`total_entries`, `total_physical_bytes`, `scan_duration_ms`)
  from meta BEFORE sending `TruncateData` (they'd survive it anyway — meta is preserved — but reading first keeps the
  data flow obviously correct), plus the volume's `used_bytes` via the space-info helper. All become `Option`s bundled
  in a small `ScanCalibration` struct, stashed as a plain field on the manager (`start_scan` is `&mut self`,
  `get_status` is `&self` — a plain `Option<ScanCalibration>` field works, no interior mutability needed).
  - The space-info call does disk I/O (NSURL XPC round-trip on macOS / statvfs on Linux) and `start_scan` runs in async
    contexts (the auto-start spawn, async Tauri commands) — wrap it in `tokio::task::block_in_place(|| …)`, matching how
    the adjacent `flush_blocking` is already wrapped. Don't make a bare blocking call on a tokio worker (NSURL can stall
    on a wedged mount), and don't put it inside the 500 ms tick loop. Fetch ONCE per scan: a moving denominator makes
    progress jump backwards.
  - Helper visibility: `get_space_info_for_path` is private to `local_posix.rs` today; promote to `pub(crate)`
    (preferred — reuse over duplication) rather than re-implementing statfs in the indexing module. Do NOT reach for
    `crate::volumes::get_volume_space` directly instead: that whole module is macOS-only (objc2-based) and its return
    type lacks `used_bytes`, so using it would break the Linux build — `get_space_info_for_path` is the cross-platform
    wrapper that already handles both.

### Backend: event + status surface

- **`IndexScanStartedEvent` gains the static-per-scan calibration**: `prior_total_entries: Option<u64>`,
  `prior_scan_duration_ms: Option<u64>`, `volume_used_bytes: Option<u64>` (sourced from the existing meta keys + the
  space-info fetch). Static values ride the started event once; the 500 ms progress event carries only the moving
  counters. (Intent: don't re-send constants every 500 ms; and the FE tier decision — calibrated vs rough — is then a
  pure function of one event.) These event structs are plain serde (FE hand-types the payloads), so no bindings impact
  from the events themselves.
- **`IndexScanProgressEvent` gains `bytes_scanned: u64`.**
- **`IndexScanCompleteEvent`**: unchanged (it already carries the true totals).
- **Late-join surface**: `IndexStatusResponse.index_status` (specta-typed `IndexStatus`) ALREADY carries
  `total_entries` + `scan_duration_ms` from meta. Add `bytes_scanned: u64` and `volume_used_bytes: Option<u64>` as
  top-level response fields (bytes ride the same `scan_handle.snapshot()` `get_status` already reads; used-bytes comes
  from the stashed `ScanCalibration`), and `total_physical_bytes` to `IndexStatus`'s meta read for symmetry.
  `IndexStatus`/`IndexStatusResponse` are specta-typed → **`pnpm bindings:regen`** and commit the regenerated
  `bindings.ts` (CI `bindings-fresh` gate).
  - One subtlety the FE milestone must handle: during a scan, `index_status.total_entries` read from meta is the
    _previous_ scan's total (the completion handler is the only writer) — exactly the denominator tier 1 wants. The FE
    must not confuse it with the live `entries_scanned` counter.

### Frontend: tier selection and math

All additions colocated in `lib/indexing/`:

- **`index-state.svelte.ts`**: store the calibration from `index-scan-started` (and the status IPC backfill), plus
  `bytesScanned` from progress events, plus a **`scanStartedAt` timestamp** (`Date.now()` on `index-scan-started`,
  mirroring the existing `replayStartedAt`/`aggregationStartedAt`) — both tier-1 ETA paths need elapsed time and no
  scan-start timestamp exists today. New getters. Reset on scan start. Late-join caveat: `IndexStatusResponse` carries
  no scan-start wall-clock, so after a mid-scan window reload the percent works (elapsed-free) but the ETA
  seed/elapsed-extrapolation have nothing until the sliding window accumulates — acceptable graceful degradation,
  expected during M4.3 verification, not a regression. NOTE: today's late-join backfill block reads ONLY
  `entriesScanned`/`dirsFound` and ignores the nested `index_status` object entirely — extend it to also pull
  `bytesScanned`, `volumeUsedBytes`, and the calibration from `index_status.totalEntries` /
  `index_status.scanDurationMs` (don't assume the nested object already flows through; it doesn't).
- **`eta.ts`** additions (pure, unit-tested):
  - `computeScanProgress(...)`: returns `{ fraction, rough } | null`. Tier 1 when `lastScanTotalEntries` is present:
    `entriesScanned / lastTotal`, clamped to ≤ 0.99 (the previous scan's total is approximate for _this_ disk state;
    99% + "Almost done" is honest, 100% mid-scan is a lie). Tier 2 when absent but `volumeUsedBytes` is present:
    `bytesScanned / usedBytes`, clamped to ≤ 0.95, `rough: true`. Neither → null (no denominator: fall back to today's
    counter-only tooltip).
  - Clamp constants named and commented (`SCAN_PROGRESS_CALIBRATED_MAX = 0.99`, `SCAN_PROGRESS_ROUGH_MAX = 0.95`): the
    rough tier clamps lower because its error band is wider (clones overshoot up to ~20%).
  - ETA: reuse `computeElapsedEta` + `computeWindowEta` + `blendEtas` with **entries** as the unit for tier 1 and
    **bytes** for tier 2 (each tier's rate and remaining-work must use the same unit). Tier 1 additionally seeds an
    estimate before the window has samples: `priorScanDurationMs − elapsedMs`, **converted to seconds** before
    `formatEta` (which takes seconds and floors negatives/sub-2s to "Almost done", so a scan outrunning the prior
    duration degrades honestly). The seed is the sole early signal (`computeElapsedEta`/`computeWindowEta` both return
    null early) and hands over entirely once the blend is available — don't mix the ms-based seed into `blendEtas`
    without unit alignment. Tier 2's ETA gets the same "Roughly" wording as its percent.
- **`IndexingStatusIndicator.svelte`**: the scan branch of the tooltip gains `ProgressBar` + percent + ETA, mirroring
  the replay branch's structure (window-snapshot glue in the component, math in `eta.ts`). Copy per style guide
  (sentence case, no trivializing):
  - Tier 1: "Scanning your drive... 42% · 1m 20s left" + the existing entries/dirs counters.
  - Tier 2: "Scanning your drive (first scan)... roughly 2m left" + counters. When clamped at the cap or the ETA goes
    sub-threshold: "Almost done" (existing `formatEta` threshold behavior).
  - Match the component's existing three-ASCII-dots convention ("Scanning...") rather than introducing a true ellipsis
    glyph into the same tooltip.
  - When `computeScanProgress` returns null: today's counter-only content (graceful degradation).
- **Static placeholder copy**: replace "Sizes are usually ready after 3 minutes" (three sites: `views/FullList.svelte`
  row tooltip, `views/full-list-utils.ts::buildDirSizeTooltip`-adjacent helper, `selection/SelectionInfo.svelte`) with
  progressive-sizes-accurate copy: "Sizes appear as the scan progresses". These remain static strings — wiring live ETA
  into per-row tooltips is complexity the corner indicator already covers. Update the one test that actually pins the
  string (`dir-size-display.test.ts` — `SelectionInfo.dir-size-state.test.ts` asserts indicator presence, not tooltip
  text, so it needs no edit) and the two CLAUDE.md mentions (`views/CLAUDE.md`, `selection/CLAUDE.md`).

### Edge cases and intent notes

- **First scan, space-info fails** (sandbox, weird mounts): tier 2 has no denominator → counter-only tooltip. Never
  block or delay the scan for the denominator.
- **Disk changed a lot since last scan** (tier 1 denominator stale): fraction clamps at 0.99 and the ETA window-rate
  still converges on truth; a scan that finishes "early" at 80% just completes — the bar is replaced by the aggregation
  phase's own progress (design principle: new state, new indicator — already the case).
- **Schema-version bump or `clear_index`** drops the whole DB including meta → next scan is tier 2 again. Correct: the
  calibration died with the DB.
- **`CMDR_E2E_START_PATH` restricted scans**: tiny fixture trees; persisted totals are fixture-sized and would be
  nonsense for a real volume — but they're also only ever compared against the same fixture scans inside the same test
  session. No special-casing.
- **Don't bump `WRITER_GENERATION`** for the new meta write OR the new `DeleteMeta` handler — `UpdateMeta` already
  doesn't, and search staleness only cares about entry/dir_stats mutations. Don't reflexively `bump_generation` in the
  `DeleteMeta` arm. (`DeleteMeta` records in `WriterStats` by default like `UpdateMeta` — fine, leave it.)
- **Same-session post-cancel state is intentionally "no `scan_completed_at` while live"**: after a `stop_scan` with no
  restart, the key stays cleared (cleared at start, gate blocks the rewrite) while live mode runs on the partial index —
  exactly today's live-on-partial behavior minus the lying completion marker. The restart heals it; don't mistake the
  missing key mid-session for a regression, and don't gate the reconcile/live transition on `was_cancelled` (that'd be a
  behavior change out of scope).
- **FE parse detail**: `IndexStatus.total_entries` reaches the FE as `string | null` (meta values are TEXT) —
  `Number()`-parse before using it as the tier-1 denominator (the debug panel already does this dance).
- **Replay-then-verification startups don't write the calibration keys** (no full scan ran) — the previous full scan's
  totals stay, which is exactly right.
- **Progress can exceed the previous total** (disk grew): the 0.99 clamp absorbs it; never show >100% or negative ETA
  (`formatEta` already floors at "Almost done").

## TDD milestones

Strict red-green; Rust enum/struct field additions compile-stub first where needed, then the failing test, then the
implementation.

### M1 — Backend counters + persistence (Rust, TDD)

1. Test: `bytes_scanned` counts physical bytes with hardlink dedup parity — temp tree containing BOTH plain single-link
   files AND a hardlink pair (nlink > 1): counter total equals the stored physical-size sum (sum over `entries` rows),
   NOT 2× the linked file. The fixture MUST include the single-link files: the counter increment sits after the
   size-resolution match, applied to every file, and a fixture with only hardlinks wouldn't catch the "increment placed
   inside the dedup arm" bug (which zeroes the count for the `nlink == 1` majority). Red against a stub counter, then
   green.
2. Test: `ScanSummary.total_physical_bytes` equals the counter's final value after `scan_volume` on a temp tree. Note
   `run_scan` constructs `ScanSummary` at TWO sites — the cancellation early-return and the normal return — populate the
   new field at both (the compiler enforces it; flagging so the cancel path isn't an afterthought: harmless at runtime
   since the `was_cancelled` gate skips persistence, but it must compile and stay honest).
3. Persistence of `total_physical_bytes` + the `was_cancelled` guard: the `UpdateMeta` sends sit in the completion
   handler inside `start_scan`'s spawned task, which needs a full `IndexManager` + `AppHandle` — NOT unit-testable per
   the module's testing bar (the existing meta writes have no unit test either). Cover what IS unit-testable: (a) the
   writer-level `UpdateMeta`/`get_meta` round-trip for the new key (one-liner next to the existing writer meta tests),
   and (b) if the guard is extracted as a small pure decision (`should_persist_scan_meta(&summary)` or simply an inline
   `if !summary.was_cancelled`), prefer the inline `if` — a one-line guard doesn't warrant a helper; the cancel behavior
   lands under M4's manual verification (stop a scan → restart the app → expect the `IncompletePreviousScan` fresh
   rescan, which the guard newly restores). Update the `indexing/CLAUDE.md` gotcha text in M4.4 if its wording needs to
   reflect the now-actually-true contract. 3b. The contract-restoring fixes (see Design § "Latent bug"): the
   `DeleteMeta` writer message (or empty-maps-to-None read) gets a writer-level round-trip test (set key → delete → read
   back `None`); the clear-at-start + `was_cancelled` gate land in `start_scan`/the completion handler
   (integration/manual coverage, M4.3b).
4. Test: calibration read — seed meta keys (`total_entries`, `total_physical_bytes`, `scan_duration_ms`), call the
   extracted read helper (takes a connection), get the `ScanCalibration` back; missing keys → `None`s. (Extract the read
   into a testable function; `start_scan` itself stays under integration/manual coverage per the module's testing bar.)
5. Implement the `get_space_info_for_path` visibility promotion (`pub(crate)`) — its behavior is already covered by
   `local_posix_test.rs`; no new test, just confirm the existing one still passes.

### M2 — Event + status surface (Rust + bindings)

1. Wire the calibration into `start_scan` (read before truncate, fetch used bytes once, stash on the manager for
   `get_status`), extend the three structs (`IndexScanStartedEvent`, `IndexScanProgressEvent`, `IndexStatusResponse`),
   emit `bytes_scanned` from the existing 500 ms reporter (it already snapshots progress; the snapshot grows).
2. Test (Rust): `get_status` reflects the calibration + live bytes during a simulated scan (the existing status-response
   test patterns in `mod.rs`/`state.rs` tests).
3. `cd apps/desktop && pnpm bindings:regen`; commit the regenerated `bindings.ts` (the `bindings-fresh` check pins it).

### M3 — Frontend math + indicator + copy (TDD via Vitest)

1. `eta.ts`: `computeScanProgress` truth table (tier selection, both clamps, null fallbacks, zero denominators) + tier-1
   seeding behavior — red first, then implement. Extend `eta.test.ts`. Also harden `formatEta` with a `Number.isFinite`
   guard (→ "Almost done" or empty) + pinning test: unreachable through the planned null-gated paths, but the scan
   branch is a new caller and "Infinitym left" is the failure mode if a future edit drops the null gate. One line of
   insurance.
2. `index-state.svelte.ts`: new fields/getters wired to the started/progress events and the status backfill (this file
   is coverage-allowlisted event glue; the logic lives in the pure helpers, keep it that way).
3. `IndexingStatusIndicator.svelte`: scan branch gains bar + percent + ETA per the Design section; a11y test gains a
   scanning-with-progress case (`IndexingStatusIndicator.a11y.test.ts` has the pattern). The a11y test `vi.mock`s
   `./index-state.svelte` with an explicit getter object — EVERY new getter the indicator imports must be added to that
   mock, or the pre-existing scanning test crashes on `undefined` before the new case even runs.
4. The three static-copy sites + their pinning tests + the two CLAUDE.md mentions.
5. `cd apps/desktop && pnpm vitest run` for the touched suites;
   `./scripts/check.sh --check svelte-check --check eslint`.

### M4 — Real-volume verification + docs + checks

1. Fresh-DB first scan (delete the dev instance's index DB): verify tier 2 — rough percent + "roughly" wording + clamp
   behavior near the end + never 100% mid-scan. Note the observed final unclamped fraction (measures how far the
   clone/exclusion errors actually net out on this machine — informs whether 0.95 is the right cap).
2. Immediately rescan (force scan): verify tier 1 — percent from entry calibration, ETA accuracy over the scan (record
   predicted-at-30s vs actual), "Almost done" tail, aggregation phase takes over at 100%-equivalent.
3. Reload the window mid-scan: late-join shows the same progress (status IPC backfill). 3b. Interrupted-scan
   verification (the restored contract), both halves: (a) stop a scan mid-way via `stop_scan`, restart the app → expect
   the `IncompletePreviousScan` notification + fresh rescan, with tier-1 progress using the _prior completed_ scan's
   totals, not the cancelled scan's partials; (b) `kill -9` mid-rescan (when a previous scan HAD completed), restart →
   same heal, WITHOUT manually deleting `scan_completed_at` (the clear-at-start makes this work; before this fix it
   didn't).
4. Docs: `indexing/CLAUDE.md` (meta keys table + the calibration flow in the data-flow diagram + a Key decision
   "Two-tier scan progress" capturing the apples-to-apples reasoning and the clamp intent), `lib/indexing/CLAUDE.md`
   (new state fields, eta additions, indicator scan branch, copy change).
5. `./scripts/check.sh` full; then `--include-slow`. The usual file-length warnings stay warnings.

## Parallelization

Sequential. M3 depends on M2's bindings; M2 on M1's types. The copy change (M3.4) is independent and could go first, but
it's minutes of work — not worth splitting.

## Resolved during review

- **R1 (review round 1, important)**: the bytes counter increments once per entry with the resolved post-dedup
  `physical_size`, AFTER the size-resolution match — not inside the hardlink arm (which fires only for `nlink > 1` and
  would near-zero the counter). M1.1's fixture must include single-link files to catch that bug class.
- **R1 (important)**: `total_entries` + `scan_duration_ms` are already persisted in meta AND already reach the FE via
  the specta-typed `IndexStatus` — the original `last_scan_*` keys duplicated them. Only `total_physical_bytes` is new
  persistence; tier 1 needs zero new meta keys.
- **R1 (important)**: the FE late-join backfill currently ignores the nested `index_status` object — the plan now spells
  out extending it; specta `bindings:regen` applies to `IndexStatus`/`IndexStatusResponse` (events stay hand-typed
  serde).
- **R1 (minor)**: persistence-write testing reframed honestly (completion handler isn't unit-testable; cover the writer
  round-trip + leave the rest to manual verification, matching the existing keys' coverage); the "3 minutes" string is
  pinned by one test, not two; the tier-1 ETA seed needs ms→seconds conversion before `formatEta` and must not be
  blended without unit alignment; `crate::volumes::get_volume_space` ruled out as the space-info source (macOS-only, no
  `used_bytes`); calibration stash works as a plain manager field (`start_scan` is `&mut self`).
- **R2 (review round 2, important — latent bug found)**: the "cancelled scans don't write the meta keys" property does
  NOT exist in today's code — the completion handler's `Ok(Ok(summary))` arm has no `was_cancelled` guard and
  `stop_scan` doesn't abort the detached completion task, so a stopped scan persists partial totals AND
  `scan_completed_at` (contradicting the documented `indexing/CLAUDE.md` gotcha; the code drifted from the doc, and the
  earlier real-app verification of the progressive-sizes feature observed exactly this empirically). Lead follow-up dug
  deeper: `start_scan` also never CLEARS the previous `scan_completed_at`, so the heal contract is broken for killed
  rescans too, guard or no guard. The plan now fixes both halves (clear-at-start via the writer channel +
  `was_cancelled` gate on the completion writes) while keeping the calibration keys uncleared so tier 1 still has the
  last completed scan's totals.
- **R2 (minor)**: the space-info fetch must be wrapped in `block_in_place` (matching the adjacent `flush_blocking`
  pattern), not called bare on a tokio worker; `formatEta` gets a `Number.isFinite` guard + test (insurance — the
  null-gated paths can't reach it today). Round 2 also verified and cleared: the partial-aggregation passes emit no
  `index-aggregation-progress` (so they can't steal the indicator tooltip mid-scan), `index_status` is always `Some`
  mid-scan (late-join denominator available), every scan path funnels through `start_scan`'s single `index-scan-started`
  emit site, and Linux behaves identically through `statvfs`.

- **R3 (review round 3, all minor)**: verified the latent-bug fix design end-to-end (branch walk of `resume_or_scan`
  confirms a deleted `scan_completed_at` + intact `last_event_id` lands on the `IncompletePreviousScan` heal, not
  replay; `DeleteMeta` beats empty-string mapping because the latter would change read semantics for every meta key; no
  other consumer breaks). Added: populate `total_physical_bytes` at BOTH `ScanSummary` construction sites; extend the
  a11y test's `vi.mock` with every new getter; `DeleteMeta` doesn't bump the writer generation; the same-session
  post-cancel "no `scan_completed_at` while live" state is intentional; FE must `Number()`-parse the TEXT meta values.

- **R4 (review round 4, all low)**: named the missing `scanStartedAt` FE field explicitly (both tier-1 ETA paths need
  elapsed; mirror `replayStartedAt`); documented the late-join ETA-seed degradation as intentional; pinned the tooltip
  copy to the component's existing "..." convention. Round 4 verified everything else clean: no internal contradictions
  after three rounds of edits, all file:line references accurate, milestone cadence matches repo conventions, the
  latent-bug fix verifies end-to-end.

## Open questions (defaults stated)

- Whether tier 2 should ALSO show a percent (clamped) or only the rough ETA. Default: show both, with "roughly" wording
  on the whole line — a bar with no number reads as broken, and the clamp keeps it honest.
- Whether `ScanProgress::snapshot()` becomes a struct or a wider tuple. Default: small struct (`ScanProgressSnapshot`) —
  three positional u64s at call sites is the kind of thing the typed-IPC rules exist to prevent.
- Whether the tier-1 seed should blend with the window estimate once available or hand over entirely. Default: hand over
  entirely (the existing `blendEtas(elapsed, window)` pair is already a blend; a third input adds complexity for
  marginal smoothing).

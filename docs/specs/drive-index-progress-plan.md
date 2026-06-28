# Drive indexing: clearer, unified progress reporting

## Why this exists

Today the app shows indexing progress in **two independent places** that reinvent the same information differently, and
neither tells the full story. On a real NAS first-scan (David's `naspi on naspolya`, ~1.4M files), the top-right
hourglass tooltip sat at "0%" for a long time while the breadcrumb badge tooltip showed a steadily-climbing file count
that was far more meaningful. The user can't tell which drive is scanning, can't trust the percentage, and has no idea
what the steps are or how many remain.

This plan fixes four things, smallest-blast-radius first:

1. **Name the drive** in the top-right tooltip (it's currently anonymous for a single drive).
2. **Lead with the most meaningful info** (the file count), and only show a percentage/ETA bar when the denominator is
   trustworthy — never a misleading stuck "0%".
3. **Unify the two surfaces** onto one per-volume status model + one shared row component, so the breadcrumb badge
   tooltip and the top-right tooltip render the _same_ representation (the badge shows just its own volume; the corner
   shows all active volumes).
4. **Make the process legible**: a per-volume **step checklist** (unchecked → spinner → check) with the live detail
   (counters, bar, ETA) under the current step, so the user understands what indexing does, where it is, and roughly how
   long it'll take. Applies to both full (re)scans and event-log roll-ons.

### Product values in play

- **Honest progress and ETA** (principle 3, "Rock solid"): never show a number that lies. A byte-ratio that sits at 0%
  while 200k files have been found is dishonest. The count is honest; show it.
- **Delightful UX** (principle 1): a clear checklist with real-time state is more reassuring than a mystery bar.
- **Elegance above all** (principle 2): one status model, one row component, not two reimplementations. This is the
  through-line — points 1 and 2 are partly _consequences_ of the duplication in point 3.
- **Smart backend / thin frontend**: phase/step truth comes from Rust as typed events; the FE renders. No
  string-matching on phase labels (`.claude/rules/no-string-matching.md`) — branch on typed discriminators only.

## Current state (so the implementer has the map)

Two surfaces, two data paths:

- **Top-right hourglass**: `src/lib/indexing/IndexingStatusIndicator.svelte` (visibility + one row per active volume) +
  `IndexingDriveRow.svelte` (the rich row: two-tier scan label + counters + `ProgressBar` + percent + ETA, or
  aggregation phase, or replay). Data: `index-state.svelte.ts` (`VolumeIndexActivity` per `volumeId`, plus a per-volume
  `AggregationActivity`). This is the **fuller** model.
- **Breadcrumb badge**: `src/lib/file-explorer/navigation/DriveIndexBadge.svelte` (the colored dot + a **plain-text**
  tooltip "Indexing… N files · M:SS" — count + elapsed, no bar/percent/ETA) + its state/copy mapping in
  `drive-index-status.ts`. Data: `drive-index-manager.svelte.ts`, which owns `statusMap` (freshness color + last-scan
  facts for the dot/menu/footer) **and** a redundant `scanProgressMap` (live scan count off `index-scan-progress`).

Backend (`src-tauri/src/indexing/`):

- Per-volume scan/replay/aggregation events all carry `volumeId` (`events.rs`, plus the aggregation-progress event in
  `writer/`). Aggregation sub-phases are already per-volume: `saving_entries → loading → sorting → computing → writing`.
- The **top-level** pipeline phase (`Scanning → Aggregating → Reconciling → Live`) is tracked only via the **global**
  `DEBUG_STATS.set_phase(ActivityPhase::…)` singleton (`events.rs`), consumed by the debug window's "Phase timeline"
  (`routes/debug/DebugDriveIndexPanel.svelte`). It is **not** emitted as a per-volume event today. Call sites where the
  `volumeId` is in scope: `manager.rs:314/596/681/761/858` (local: Scanning→Aggregating→Reconciling→Live),
  `network_scan.rs:222/343` (SMB/MTP: **Scanning→Live directly** — no distinct Aggregating/Reconciling phase emitted,
  though the writer still runs aggregation and emits its sub-phase events), `event_loop.rs:842` (post-replay → Live).

The `index-state.svelte.ts` "listen first, then query" ordering and the two-tier `computeScanProgress` are load-bearing;
read `src/lib/indexing/CLAUDE.md` + `DETAILS.md` before touching them.

## Decision: the count is the spine, the bar is a garnish

The core reframing behind M1+M2, and the thing the whole feature rotates around:

- **The file count is always meaningful and monotonic.** It's the primary signal. Show it prominently, always, on both
  surfaces, for every scan tier.
- **The percentage/ETA bar is a secondary estimate, shown only when its denominator is trustworthy.** Trustworthy =
  tier-1 (a prior scan calibrated `priorTotalEntries`). For a first scan (tier-2, byte-ratio against `volumeUsedBytes`),
  the bar is unreliable early (sits near 0 on big volumes), so we **don't** show a precise percent there. Options, in
  preference order: (a) show count + elapsed only (matches the badge the user preferred), or (b) an **indeterminate**
  bar (motion, no number) if we want to signal "working". Decision: **(a) count + elapsed for first scans**, no bar, no
  fabricated percent. It's the honest choice and it unifies the two surfaces' behavior. A calibrated rescan keeps the
  real percent + ETA bar.
  - **Why not (b):** an indeterminate bar adds chrome without information; the elapsed clock already signals liveness,
    and the badge precedent (which the user explicitly preferred) is count + elapsed. Keep it quiet.
  - **Be honest about what this gives up:** count + elapsed gives _liveness_ but no sense of _how far along_ a first
    scan is (a 30+ min NAS first scan shows a climbing count and a running clock, nothing else). That's unavoidable —
    there's no calibration to estimate against on a first scan. The deliberate partial answer to "how far / how long?"
    for first scans is **M4's step checklist** (you can see you're on "Scan files" of N steps), plus keeping the
    **"first scan"** context visible (it sets the "this takes a while" expectation, which is itself reassurance). David
    preferring count+elapsed was choosing the lesser of two bad options over a stuck 0% bar — not an endorsement of "no
    completion signal ever," so lean on the checklist to carry that weight.

## Milestones

Sequential. M1 and M2 are independently shippable quick wins. M3 is the unification (and is what makes M1/M2 land on
_both_ surfaces from one place). M4 is the checklist and depends on M3's shared row + a new backend event.

---

### M1 — Name the drive in the top-right tooltip

**Intent:** the user must instantly see _which_ drive is indexing, as a clear title — even when only one drive is
active.

**Change:** in `IndexingStatusIndicator.svelte`, always render the per-drive heading (drop the
`showHeadings = rows.length > 1` gate; pass `showHeading={true}` always). In `IndexingDriveRow.svelte`, promote
`.drive-heading` to read as a real title (it's already `font-weight: 600`, `--color-text-secondary`; bump to
`--color-text-primary` and verify it reads as a heading above the status line). The name already resolves via the volume
store (`driveName(volumeId)`); the `indexing.drive.heading` key is a `{name}` passthrough — keep it.

**Edge:** the synthetic aggregation-only row and the id-fallback (drive vanished mid-scan) still work — heading shows
the resolved name or the id. No copy change needed beyond possibly relaxing the `@key` description (it currently says
"shown when more than one drive is indexing"); update that description to match the new always-on behavior.

**Docs:** update `src/lib/indexing/DETAILS.md` "Status indicator tooltip content" (the "shows only when more than one
drive is active" line is now false) and the `CLAUDE.md` if it references it.

**Tests (after):** extend `IndexingStatusIndicator.a11y.test.ts` — the single-drive scanning case must now render the
heading. Update the existing assertion that no heading shows for a single drive.

**Checks:** `pnpm check desktop` (svelte + a11y + i18n parity).

---

### M2 — Count-first progress; bar/percent only when calibrated

**Intent:** never show a misleading stuck percentage. Lead with the honest count; reserve the bar+percent+ETA for
calibrated rescans.

**Change (scan mode in `IndexingDriveRow.svelte`):**

- Always show the counters line (`indexing.scan.counters`, "171,607 entries, 16,101 dirs") — it already exists but is
  appended to the label; consider splitting it onto its own line under the label for prominence (a detail line like
  replay's). Keep the label ("Scanning your drive…" / "(first scan)…").
- Add an **elapsed clock** to the scan detail for the first-scan (tier-2/`rough`) case, mirroring the badge's "· M:SS".
  - **Move `formatElapsedClock` to a shared util NOW (in M2), not M3.** It's currently private to
    `drive-index-status.ts` (navigation/), but M2 needs it in `IndexingDriveRow` (indexing/). Extract it to a small
    shared module (e.g. `src/lib/indexing/elapsed.ts` or an existing util) in M2 so M2 doesn't reach across modules or
    duplicate. The badge keeps using it; M3 then has it already shared.
  - **The clock needs a ticking source.** `IndexingDriveRow` has no 1 Hz timer (unlike `DriveIndexBadge`, which runs a
    `$state` `now` updated each second). `Date.now()` inside a `$derived` is NOT reactive — the clock would advance only
    when a ~500 ms progress event lands and **freeze entirely if progress stalls**, which is exactly the reported NAS
    scenario. Give the row (or the M3 shared body) its own 1 Hz `$state` tick gated on "scanning" (idle rows run no
    timer), or pass `now` in. Mirror `DriveIndexBadge.svelte`'s pattern.
- Gate the `ProgressBar` + percent on `!scanRough` (tier-1 calibrated) **only**. When `scanRough`, render **no bar and
  no percent** — count + elapsed instead. `computeScanProgress` already flags `rough`; this is a render-gate, not a math
  change. Don't change the tier math or the ETA-window sampling.
- **Guard the empty initial state:** before the first progress event (count 0, elapsed under 1 s), fall back to the bare
  "Scanning your drive…" label without a "0 entries, 0:00" line — mirror the badge's existing static fallback. Don't
  flash zeros.

**Why keep tier-2 math at all:** `scanProgressInfo.rough` is still needed to pick the "(first scan)" label and to decide
the gate. We just stop _rendering_ its fraction.

**Risk:** the a11y test asserts a "calibrated-with-bar" and "counter-only" case already — good, that maps cleanly. Make
sure the `aria-label` on the (now sometimes-absent) progress bar and the `aria-describedby` live label still describe
the count for screen readers when the bar is gone.

**Docs:** `DETAILS.md` "Two-tier scan progress" + "Status indicator tooltip content" — document that tier-2 renders
count+elapsed, no bar.

**Tests (after; this is a render-policy change, low risk):** a11y test case "first scan renders count + elapsed, no
progressbar"; assert no `progressbar` role in that DOM. Keep the calibrated case asserting the bar is present.

**Checks:** `pnpm check desktop`.

---

### M3 — One status model, one row component (the unification)

**Intent:** the breadcrumb badge tooltip and the top-right tooltip render the **same** per-volume status. Kill the
duplicate data path and the duplicate copy. This is the elegance win and what makes M1/M2 apply everywhere from one
place.

**Design:**

- **One source of per-volume live activity: `index-state.svelte.ts`.** It already keys `VolumeIndexActivity` (+
  `AggregationActivity`) by `volumeId` for scan/replay/aggregation. Retire `drive-index-manager.svelte.ts`'s
  `scanProgressMap` (the redundant live-count path). The manager keeps owning `statusMap` (freshness color + last-scan
  facts) — that's the dot color and the menu/footer, which `index-state` doesn't model. So: **manager = freshness/menu;
  index-state = live activity.** Two clear responsibilities, no overlap.
- **One shared row component.** Extract the per-volume status body from `IndexingDriveRow.svelte` into a reusable
  component (e.g. `IndexingStatusBody.svelte` under `src/lib/indexing/`) that takes a `VolumeIndexActivity` +
  `AggregationActivity | undefined` and renders the label/counters/detail/bar (the M2 policy). `IndexingDriveRow`
  becomes a thin wrapper (heading + body + its own ETA window). The breadcrumb badge's scanning tooltip renders the same
  body for its one volume.
- **Badge tooltip goes rich (DOM), not string.** `DriveIndexBadge.svelte` currently uses `use:tooltip={tooltipText}`
  (string). For the _scanning_ state, switch to the `contentEl` DOM-tooltip variant (the indicator already uses it) so
  it can host the shared body. Non-scanning states (disabled/fresh/stale) stay as the existing text tooltip — they're
  fine and don't need the body. Keep the menu, dot, and footer exactly as they are.
  - **Gotcha to preserve:** the indicator's `contentEl` pattern renders the body inside a `<div hidden>` host and passes
    the _inner_ div (not the hidden host) so `hidden` isn't adopted into the tooltip. Mirror that in the badge.
  - **Preserve the "scanning, no activity yet" fallback.** The badge dot color comes from the _backend freshness_
    (`manager.statusMap`, `scanning`), but the rich body comes from `index-state` activity — which for a non-root
    (SMB/MTP) volume only hydrates on the next 500 ms progress tick (the root-only `getIndexStatus` backfill in
    `index-state.svelte.ts` doesn't cover SMB/MTP). So there's a real window — mid-scan reload, or before the first tick
    — where `freshness === 'scanning'` but there's no `VolumeIndexActivity` entry, and the body would render blank. The
    badge MUST keep a static fallback ("Scanning your drive…") for that window, exactly as `DriveIndexBadge.svelte` does
    today via `tString('…tooltipScanning')` when `!scanProgress`. Don't let the tooltip go empty.
  - **Add a per-volume activity getter to `index-state`'s public API.** Today the barrel exposes
    `getActiveIndexVolumes()` (all) but no "this one volume's activity" read. The badge needs
    `getVolumeActivity(volumeId)` (returns `VolumeIndexActivity | undefined`) to render only its own volume. Add it.
- **ETA windows must not collide.** Each consumer that renders the body owns its own ETA sliding-window state (today
  that lives in `IndexingDriveRow`). When two surfaces render the same volume simultaneously (corner + that volume's
  open badge tooltip), they each keep an independent window — that's fine and already the per-row model. Keep the window
  state in the wrapper, not the shared body, OR have the body accept the window as a prop. Decision: **window stays in
  each wrapper**; the shared body is presentational (takes computed `label`/`detail`/`progress`/`percentDisplay` props,
  or takes the activity and an injected eta). Pick the split that keeps the body free of stateful `$effect` glue — lean
  presentational.
- **Copy unification.** The badge's scanning copy (`fileExplorer.navigation.driveIndex.tooltipScanning*`) and the
  indicator's (`indexing.scan.*`) overlap. After M3 the scanning tooltip uses the `indexing.scan.*` family on both
  surfaces. Leave the badge's non-scanning copy (disabled/fresh/stale/menu/footer) untouched. Remove the now-dead
  `tooltipScanningCount*` keys (and the `driveIndexScanProgress` helper) only if nothing else references them — grep
  first; if the elapsed-clock helper (`formatElapsedClock`) is reused by the shared body, move it to a shared util
  rather than deleting.

**Risk / safety:** this touches the breadcrumb (high-traffic UI). The badge dot/menu/freshness must not regress. Keep
`drive-index-status.ts` (state→color/menu) as-is; only the _scanning tooltip body_ changes. The
`DriveIndexBadge.svelte.test.ts` and `.a11y.test.ts` must stay green; extend them for the new DOM tooltip body.

**Docs:** rewrite `src/lib/indexing/CLAUDE.md` + `DETAILS.md` and `navigation/CLAUDE.md`'s badge bullet to describe the
single shared body + the manager-vs-index-state responsibility split. This is a structural change → update colocated
docs in the same pass (`.claude/rules/docs.md`).

**Tests:**

- Unit: the shared body renders each mode (scan tier-1 with bar, scan tier-2 count+elapsed, aggregation, replay) from a
  fixture activity. Pure-ish; mount-test it.
- a11y: badge scanning tooltip (new DOM body) passes axe; indicator unchanged.
- Keep `index-state.svelte.test.ts` green; if `scanProgressMap` removal changes the manager's surface, update
  `DriveIndexBadge.svelte.test.ts` accordingly.

**Checks:** `pnpm check desktop`. Manual: run the app, open the badge tooltip mid-scan and confirm it matches the
corner.

---

### M4 — Per-volume step checklist

**Intent:** the user understands the _process_: what the steps are, which one is active (unchecked box → animated
spinner → checked box), live detail + ETA under the active step, and a rough overall sense of remaining time. Works for
full (re)scans and event-log roll-ons. Big tooltip is fine.

**M4a — Backend: emit a per-volume phase event.**

- Add `index-phase-changed { volumeId, phase: ActivityPhase }` (typed event, `events.rs`, `tauri-specta`). Emit it
  alongside the existing `DEBUG_STATS.set_phase(...)` at the sites where `volumeId` is in scope. **Verified in review:**
  `volumeId` IS in scope at every site — `manager.rs:314` (`self.volume_id`), `:596`, `:681/:761/:858` (the spawned
  completion task captures `let volume_id = self.volume_id.clone()` at `manager.rs:640`), the Idle sites `:898/:1022`,
  `network_scan.rs:222` (`self.volume_id`), `:343/:382` (captured clone at `:256`), and `event_loop.rs:842` (inside
  `run_replay_event_loop(volume_id: String)`). Emit at the Idle sites too, so the FE can clear/complete the checklist.
  The global debug timeline stays; this just _also_ tells the FE, per volume. **No string-matching** — `ActivityPhase`
  is already a typed serde enum exported to `bindings.ts`; the FE maps the variant to a step.
- **Register the event in `collect_events!`** (see `indexing/DETAILS.md`). Without this registration `bindings:regen`
  silently emits nothing for the new event. Then regenerate bindings (`pnpm bindings:regen`) and add the FE event
  wrapper in `tauri-commands/indexing.ts` (`onIndexPhaseChanged`).
- **Reload backfill (decide here, not "during execution"):** `index-phase-changed` fires only on _transitions_, so after
  a mid-scan window reload the FE can't learn the _current_ phase (the existing `getIndexStatus` is root-scan-only and
  carries no phase; `IndexDebugStatusResponse.activityPhase` is the **global** singleton — wrong under concurrent
  volumes and debug-only). The reconcile step is worst-hit (no progress events, only the phase event marks it).
  **Decision:** derive checklist step-state primarily from the _presence_ of `index-state` activity/aggregation entries
  (which DO backfill scan counts and fire aggregation events for every volume), and treat the phase event as the
  authoritative driver for the steps that have no other signal (reconcile). Accept that the **reconcile step is
  unobservable after a reload that lands mid-reconcile** — it's a brief, rare, low-stakes window; the step simply shows
  as not-yet-active until the next phase event or completion. If cheap, also add an optional per-volume current
  `ActivityPhase` to `VolumeIndexStatus` (already fetched per-volume by the manager) so the checklist can hydrate the
  current step on reload — prefer this if it's ~15 LoC, else accept the gap and document it.
- **Network-scan honesty (two steps differ for SMB/MTP — verified in review):**
  - SMB/MTP emit `Scanning → Live` with no distinct `Aggregating`/`Reconciling` phase, yet the per-volume aggregation
    sub-phase events (`loading → sorting → computing → writing`) _do_ fire (the writer is spawned per-volume and the
    network path sends `ComputeAllAggregates`). So drive the "Compute folder sizes" step off the **aggregation events**,
    not off a top-level `Aggregating` phase that network never emits.
  - **The `saving_entries` sub-phase also never fires for SMB/MTP.** `set_expected_total_entries()` is local-only
    (`manager.rs`); network volumes insert entries inline _during_ the BFS walk, so there is no post-scan "save entries"
    drain. **Do NOT fake it** by calling `set_expected_total_entries` on the network path — that fabricates a phase that
    doesn't semantically exist. Instead, the step model is **event-driven and composed**: a step appears/activates only
    when its driving events fire. For network, the "Find files" and "Save the file list" steps effectively collapse into
    one (insert is part of the scan), so "Save the file list" simply doesn't appear. Document this in
    `indexing/DETAILS.md`.
  - So the universal rule: **the checklist is built from the events that actually fire for THIS volume** (phase event +
    scan/aggregation-sub-phase/replay events), never a hardcoded "every scan has these 4 steps" list. See M4b's step
    model.

**M4b — Frontend: the checklist UI.**

- A per-volume checklist replaces/augments the single status line in the shared body (M3). **Steps are composed from the
  events that fire for this volume** (not a fixed list — see M4a network honesty). User-facing labels (sentence case,
  friendly, no jargon — these are final copy, put them through the style guide but these are the intended meanings):
  1. **Find files** — the scan. Detail: count + elapsed (+ bar+ETA if calibrated, per M2). On a first scan, keep the
     "first scan" context here (e.g. the label or a sub-line conveys "first scan — this takes a while").
  2. **Save the file list** — the `saving_entries` aggregation sub-phase (local only; for network it's part of "Find
     files", so this step simply doesn't appear). ❌ Not "Save entries" ("entries" is a DB term) and ❌ not "Build the
     index" (collides with the feature name — the whole operation is "indexing", so a _step_ called that reads as if the
     others don't build the index). "Save the file list" is concrete, distinct, and stays in the user's mental model.
  3. **Compute folder sizes** — `loading → sorting → computing → writing` (computing/writing have progress; loading/
     sorting are indeterminate — shown as the active-step spinner + a text sub-line, NOT a second nested spinner). Use
     "folders" not "directories" in this step AND in the sub-phase detail copy underneath, for one consistent word on
     one surface (the existing `indexing.aggregation.*` sub-phase strings say "directories" — reword them to "folders"
     here, or render folder-worded variants).
  4. **Catch up on recent changes** — the post-scan reconcile. ❌ Not "Reconcile" (accounting/dev jargon).
     Indeterminate; local only — for network it doesn't apply, so the step doesn't appear (don't show a
     permanently-unchecked step).

  For an **event-log roll-on** (replay), the checklist collapses to a single **Update index** step (replay, with its
  blended ETA). "index" is established product vocabulary here (badge, menu, "rescan"), so it's fine. Don't show
  scan/build/compute steps that won't run.

- **Step state** per item: `pending` (unchecked box) / `active` (spinner) / `done` (check). (No `canceled` visual — see
  the stop/cancel note below; a stopped scan unmounts the row.) Derive from the phase event + which events have fired.
  Use `<Icon>` glyphs (checkbox, check) and `<Spinner>` (per `src/CLAUDE.md` — no hand-rolled spinner). Active step
  shows the live detail underneath (the M3 body's per-mode content). **No nested spinners** — the active step's spinner
  is the only one; sub-phases are text, not a second ring.
- **Stopped / canceled / failed (principle 3: everything cancelable) — handled by the row DISAPPEARING, not an
  in-checklist "stopped" visual.** Don't build a fourth `canceled` step-visual inside the checklist — there's no host
  for it: on a clean OR canceled stop the activity entry is removed from `index-state` (local `index-scan-complete`
  fires even when canceled), so the corner row unmounts and the badge falls back to its freshness color (gray/yellow).
  That vanishing + the badge color IS the honest, calm feedback (no frozen spinner, no "error"/"failed" copy). So the
  step states are just `pending` / `active` / `done`.
  - **But fix the real latent bug this exposes (proactive):** the **network (SMB/MTP) abort paths don't clear the
    activity row.** `network_scan.rs` disconnect (`:363`) and cancel/fail (`:384`) arms emit no `index-scan-complete`,
    and `index-state` clears a volume's activity ONLY on scan-complete / replay-complete (it doesn't subscribe to
    freshness). So a canceled or disconnected network scan leaves a **stuck "scanning" corner row** — and M3/M4 lean
    harder on `index-state` as the single source, making it more visible. Emit a terminal clear (a scan-complete, or a
    dedicated `index-scan-aborted { volumeId }` that `index-state` treats as "remove this volume's activity") on those
    abort arms so the row clears. This is a small, correct bug fix; do it in M3 (where the single-source reliance lands)
    and add a regression test.
- **Phase→label map is NEW, not the existing one.** ❌ Don't "extend `phaseToLabelKey`" — that map (in
  `IndexingDriveRow.svelte`) keys aggregation **sub-phase** strings (`saving_entries`/`loading`/…), not `ActivityPhase`.
  Add a separate `ActivityPhase`→step-label-key map for the checklist steps.
- **Tooltip height stability.** The `contentEl` tooltip "measures once on show and can't see later content growth"
  (`IndexingStatusIndicator.svelte` comment, the reason for its fixed `min-width`). A checklist whose steps tick and
  whose active-step detail appears/disappears changes height materially on a right-anchored, viewport-clamped,
  measured-once box → clip/mis-position risk. Either confirm the tooltip re-measures/re-positions on content change, or
  reserve a stable height for the checklist (render all steps up-front; only the per-step icon + the single active
  detail line change). Validate this with a real mid-scan QA pass.
- **Overall ETA — DEFERRED as one coherent unit with its calibration (honest-spine call).** An overall "~Xm left" is
  only honest if the not-yet-started steps have real estimates. Those require persisted **per-phase** priors — and the
  per-phase `duration_ms` the backend records today lives only in the in-memory `DEBUG_STATS` ring (capped at 20, reset
  on restart, not per-volume-persisted). So a "rough overall ETA" built without them collapses to _just the active
  step's ETA wearing an "overall" label_ — which trips this plan's own honest-ETA spine. Rather than ship that, **defer
  overall ETA together with the backend per-phase calibration** (a new per-volume meta-write of last scan's per-phase
  durations + a read to seed estimates). Capture it as the named follow-up in `docs/specs/`.
  - **What v1 ships for "how long?" instead, honestly:** (1) the **step-of-N structure** itself answers complaint #4's
    "step 3 of 3 or 3 of 10?" directly — you see every step and which is active; (2) the **active step shows its own
    ETA** where it has a trustworthy denominator (calibrated scan, computing/writing), which on a rescan is most of the
    wall-clock. That's a large, honest leap over today's single mystery bar without fabricating a total.
- **Multi-volume stacking.** The corner tooltip shows _all_ active volumes; a full multi-step checklist per volume means
  N stacked checklists. "Big tooltip is fine" doesn't cover four drives. **Collapse secondary volumes to a one-line
  summary** (name + current step + count/percent), and expand the full checklist only for the primary/active volume (or
  the first). The badge tooltip always shows just its own volume's full checklist. Keep it legible, not a wall.
- The debug window's "Phase timeline" (`DebugDriveIndexPanel.svelte`) is a reference for rendering a phase list with a
  current highlight — borrow its structure, not its debug styling.

**Frontend design:** the checklist is new UI meeting human eyes → the implementing FE agent MUST load
`docs/style-guide.md`, `docs/design-principles.md`, and the `frontend-design` skill
(`/Users/veszelovszki/.claude/plugins/cache/claude-code-plugins/frontend-design/1.1.0/skills/frontend-design/SKILL.md`).
Sentence case, active voice, friendly, no "error/failed". Respect `prefers-reduced-motion` (the spinner + any step
transitions). AA+ contrast. The whole thing lives in a tooltip; keep it calm and legible, not flashy.

**i18n:** new step labels go in `messages/en/indexing.json` with translator `@key` descriptions. Map the typed
`ActivityPhase` to step-label KEYS via a **new, separate** `ActivityPhase`→key map (NOT the existing `phaseToLabelKey`,
which keys aggregation sub-phase strings — see the M4b note); keep the existing sub-phase map for the under-step detail
copy. Never branch on wording. Keep `indexing-i18n-parity.test.ts` green.

**Tests:**

- TDD (real red→green) for the step-state derivation (pure function: given phase + fired-events → per-step state). This
  is the risky logic; write it test-first.
- a11y: the checklist tooltip passes axe; steps have accessible names/states (a checklist needs proper roles — verify
  with the a11y agent).
- Backend: a Rust test that the per-volume phase event fires at each transition (mirror the existing `events.rs`
  `set_phase` transition tests, but for the new emit).
- i18n parity stays green.

**Docs:** `indexing/CLAUDE.md` (the per-volume phase event is a new must-know: it's per-volume, unlike the global
`ActivityPhase`), `DETAILS.md` (the step model, network-scan composition, calibration), and the backend
`indexing/CLAUDE.md`/`DETAILS.md` for the new event + emit sites.

**Checks:** `pnpm check` (full, after M4), then `--include-slow` at the very end. Relevant E2E only.

---

## Cross-cutting

- **No string-matching** on any phase/reason (`.claude/rules/no-string-matching.md`) — typed enums → catalog keys
  throughout.
- **All user-facing strings via `t()`/catalog** — `cmdr/no-raw-user-facing-string` is enforced on `lib/indexing/`.
- **Icons/spinner via `<Icon>`/`<Spinner>`** — no raw lucide imports, no hand-rolled rings.
- **CSS tokens only** — no raw px where a `--spacing-*`/`--font-size-*` token exists.
- **Keep `index-state` "listen first, then query" ordering** intact (CLAUDE.md must-know).

## Parallelization

Run **sequentially**. M2 depends on M1's file being settled (same component), M3 restructures what M1/M2 touched, M4
builds on M3's shared body + the new event. The only safe parallel split is M4a (backend event) vs the M4b FE
scaffolding, but even there the FE needs the regenerated binding — so just do M4a → `bindings:regen` → M4b.

## Definition of done

- All four points visibly fixed: drive named, count-first honest progress, one shared representation on both surfaces, a
  legible per-volume step checklist with current-step detail and per-step ETA. The step-of-N structure + active-step ETA
  carry "how long?"; a true **overall** ETA (with its backend per-phase calibration) is an explicit, named follow-up in
  `docs/specs/`, deliberately deferred to keep the ETA honest — not a blocker.
- Friendly, jargon-free step labels (Find files / Save the file list / Compute folder sizes / Catch up on recent changes
  / Update index), one word for folders, a stopped scan unmounts the row honestly (plus the network-abort stuck-row bug
  fixed), multi-volume collapsed sensibly.
- `pnpm check --include-slow` green (surface any unrelated failures).
- Milestone tags stripped from touched code/docs.
- Colocated docs updated in the same pass.
- App launched for David to QA at the end.

# Search details

Pull-tier docs for `lib/search/`: architecture, flows, and decision rationale. Must-know invariants and gotchas live in
`CLAUDE.md`.

## i18n

User-facing copy born in this area lives in the `search.*` catalog (`$lib/intl/messages/en/search.json`), resolved via
`tString()` (the `SearchDialogConfig` title/actions/tooltips, the recent-search aria label, the system-dir-exclude
tooltip, snapshot labels). `cmdr/no-raw-user-facing-string` is enforced on `/lib/search/`. Most of the dialog's
on-screen copy is owned by the shared `lib/query-ui/` primitives (the `queryUi.*` catalog), not here.
`SEARCH_RESULTS_NOT_A_FOLDER_TOAST` stays a string const (resolved from the catalog at module load) because out-of-scope
consumers (`command-dispatch.ts`, `transfer-entry.ts`) import it by name. Parity net: `search-i18n-parity.test.ts`.

Backend: `src-tauri/src/search/` (index, engine, query, AI pipeline), `src-tauri/src/commands/search.rs` (thin IPC
wrappers).

This dialog is the first consumer of the shared Query UI primitives in `../query-ui/CLAUDE.md`: unified query bar, mode
chips, AI prompt strip, filter chips strip, virtualized results table, the query field's recent-items dropdown, the
`createQueryFilterState()` factory that owns cross-consumer fields, and the in-dialog keyboard contract. Search-specific
concerns (snapshot store, virtual volume, MCP open path, "Open in pane", index lifecycle, scope smart fallback) stay
here. Selection (see `lib/selection-dialog/`) is the second consumer; both wrap `QueryDialog` and share the same
primitives.

Dialog dimensions: `max-width: min(1080px, 80vw)`, `max-height: 80vh`. The dialog grows up to 1080 px wide but shrinks
to 80vw on smaller windows, and the results region absorbs whatever vertical room is left.

## Files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md`. What
each piece DOES is in the sections below: the wrapper in § "Search wrapper", the two state factories and the façade in §
"Where the state actually lives", the snapshot store and its refcounting in § "Snapshot store", `searchable-folder` in §
"'Use current folder' smart fallback", and the capability flags in § "Capability flags". Everything shared with the
Selection dialog (the query bar, mode chips, AI strip, filter chips, path pills, row menu, results table, empty state,
and the `recent-items/` family) lives in `../query-ui/CLAUDE.md`, over the app-wide `$lib/ui/Chip` / `Popover` /
`FilterPopover` primitives. Only the layout facts that none of those carry live here:

- **`SearchResultsView.svelte` does NOT live in this directory.** It sits in `lib/file-explorer/pane/` with the other
  pane views even though it's conceptually a Search consumer, because it renders as a pane for `search-results://`
  snapshot panes.
- **`capabilities.ts` owns only the `SEARCH_RESULTS_NOT_A_FOLDER_TOAST` string.** The capabilities themselves come from
  the per-kind table in `lib/file-explorer/pane/volume-capabilities.ts`; there is deliberately no Search-specific
  capability shim, so don't add one.
- **`snapshot-store.svelte.ts` carries the `.svelte.ts` extension for ONE `$state` cell**, `mutationTick`. The snapshot
  map itself is deliberately plain module state, because consumers read snapshots imperatively at render time and
  nothing should re-render when the map changes; the tick exists so a rendered snapshot can subscribe to a change under
  it (a cross-snapshot delete, or a still-running walk appending rows to a pane it was handed). Keep new reactivity out
  of the map: wrap `getSnapshot` results in a `$derived` at the call site instead.
- **The tick alone doesn't get a change onto the screen, and that gap shipped once.** `SearchResultsView`'s
  `snapshot = $derived(void getMutationTick(), getSnapshot(id))` re-runs on the bump, but its VALUE is the stored
  object; a mutator that wrote into that object left the derived recomputing to the same reference, and Svelte stops
  propagation there, so `entries = $derived(snapshot.entries.map(...))` never re-ran. A handed-off pane held the two
  rows it opened with while its toast counted to 24. Every mutator therefore `store.set`s a REPLACED entry
  (`appendSnapshotEntries`, `removeEntryFromAllSnapshots`), and `SearchResultsView.svelte.test.ts` pins it by appending
  to a mounted view and counting rows.
- **The mounted-dialog tests share one fixture, `test-search-dialog-harness.ts`.** Mounting SearchDialog means standing
  up its whole IPC surface plus the settings and stores it reads, so the spies, the live-run fake, and the
  mount/teardown helpers live there and each `SearchDialog.<concern>.svelte.test.ts` reaches them with
  `vi.mock('$lib/…', async () => (await import('./test-search-dialog-harness')).xMock())`. The dynamic import inside the
  factory is what keeps import order and hoisting out of it. ❌ Nothing in the harness may import a mocked module (or
  `SearchDialog.svelte` / `search-state.svelte`) at module scope: the factories load the harness, so such an import
  would ask for the module whose factory is still running. `SearchDialog.coverage.*` and `SearchDialog.handoff.*` keep
  their own fakes deliberately, since each drives different live-run behavior.

## Search wrapper

`SearchDialog.svelte` carries neither the dialog orchestration nor the glue. The overlay, keyboard contract, IME guard,
auto-apply gates, `lastDialogEvent` writes, the `⏎` ownership swap, the title bar, the chip strip, the AI prompt strip,
the results table, the recent-items dropdown, and the empty state all live in `../query-ui/QueryDialog.svelte`. The
Search wrapper builds a [`QueryDialogConfig`](../query-ui/query-dialog-config.ts) for Search and mounts QueryDialog with
it.

Everything Search-specific sits in a module beside it, one per job, each unit-tested on its own:

- **`search-lifecycle.svelte.ts`** — `prepareSearchIndex` on mount, `releaseSearchIndex` (naming the handed-off run) on
  destroy, the `search-index-ready` listener and the auto-run-after-index-ready hook (which fires only when the volume
  that landed is the one being searched), the per-target readiness gate below, and the system-dir exclude tooltip
  (`getSystemDirExcludes`, escaped into HTML by the pure `system-dir-tooltip.ts`).
- **`search-runners.ts`** — both run paths and the `buildRunQuery` they share (bar + filters + AI pattern + the scope,
  whose parse is async): `runQuery` is the one-shot index answer the debounce takes, `streamingSource` is every run the
  user asked for. It also writes the coverage note on both paths, cleared before the ask and filled from the answer.
- **`search-run-tracking.ts`** — the run clock and the one `search_used` event per run, plus the two CTA events. The
  vocabulary is `search-analytics.ts`; this is the wiring.
- **`ai-translate.ts`** — calls `translate_search_query` and applies the AI's filter writes (`size`, `date`, scope,
  `caseSensitive`, `excludeSystemDirs`, Pattern chip + label). Returns `{ caveat, highlightedFields }` to QueryDialog.
- **`coverage-cta.svelte.ts`** — reads the note and decides what may be OFFERED over it: the per-drive indexing offer,
  the Full Disk Access route (with its quiet TCC probe), and the `search_cta_offered` reporting.
- **`snapshot-promotion.ts`** — "Show all in main window" (⌥⏎): builds the `SearchSnapshot`, mints an id, pins via
  `setLastAttemptId`, hands a still-running walk to `walk-handoff.svelte.ts`, and persists the recent-search entry. It
  and "Go to file" are the only two call sites that add to recent searches.
- **`recent-search-adapter.ts`** — the row adapter + key (the only seam where Search-specific fields like `scope` /
  `excludeSystemDirs` leak into the chip's tooltip), plus pick (LOADS, never runs) and remove.

What's left in the wrapper: the props, the `QueryDialogConfig`, the two snippets (coverage note, image grid), the "Go to
file" / path-pill / row-menu exits through `onNavigate`, `onClearState` wiring ⌘N to `clearSearchState()`, and the live
AI-provider subscription so the AI chip appears / disappears with the setting. It does not own the overlay element, the
keyboard handler, the IME guard, the auto-apply debounce, the popover toggle, or any other orchestration concern.

The wrapper holds the two locals a run outlives: `liveRun` (the run in flight, plain and not `$state` — it changes per
batch and nothing renders it) and `handedOffRun` (the run a pane is now fed by, so the close can name it).

The route (`+page.svelte`) mounts SearchDialog with its props: `onNavigate`, `onClose`, `scopePresets`, `searchVolume`,
`onShowAllInMainWindow`.

### Which volumes each search covers

Both searches now cover ONE volume, and each picks it differently.

**Filename search covers the scope's volume**, at most one (`src-tauri/src/search/CLAUDE.md`). The frontend's job is to
make sure a scope always exists: an empty box means the focused pane's current folder, resolved at run time in
`buildRunQuery()` (`search-runners.ts`) from `defaultScope`, so it follows the pane rather than freezing at dialog-open
time.

**One volume answers every "which drive?" question the dialog has**, so there's one prop for it: `searchVolume`, the
focused pane's current volume. `+page.svelte` passes `searchVolume={getFocusedPaneSearchTargetVolume()}` (in
`focused-pane-reads.ts`), which reads the focused pane's volume id and resolves it against the live volume store via the
pure `resolveSearchTargetVolume` (`search-target-volume.ts`). It feeds three consumers:

- the **readiness gate** (below), which waits only for that volume's arena;
- the **image-OCR grid**, whose media-index volume id IS the pane's volume id (`root` for the local disk, `smb-…` for an
  SMB share), so browsing the NAS surfaces the NAS's photos and browsing local surfaces local;
- the **mount root** that volume's `VolumeInfo.path` gives (`/` for root, `/Volumes/<share>` for SMB), which
  `ImageSearchResults` prepends to index-relative OCR hits via `resolveMediaHitPath`, plus `isNetwork` for the network
  coverage voice.

`isNetwork` comes from the typed `volumeKindOf(...) === 'smb'` (`file-explorer/pane/volume-capabilities.ts`), NOT from
`category === 'network'`. Verified live on 2026-08-04: an SMB share Cmdr couldn't upgrade to a direct connection stays
an OS mount and the volume list reports it as `attached_volume` with `fsType: 'smbfs'`, so the category test alone told
a NAS user their boot disk wasn't indexed. `volumeKindOf` is the single frontend classifier (invariant A6) and sees both
shapes.

A focused pane whose volume isn't a real filesystem volume in the list (a `search-results://` snapshot) falls back to
the local root, as does the prop when unset — the same fallback `resolveDefaultScope` makes for the scope.

### The readiness gate is per target

`config.isIndexReady` gates `executeQuery`, and its question is **"is the volume this run will land on worth waiting
for?"**, not "is root loaded". The pure `isTargetIndexReady` (`coverage-note.ts`) answers it from three inputs:

- **`targetVolumeId`** — `searchVolume.volumeId` when the scope box is empty (the default, so the run lands on the
  pane's volume), `null` when the user typed or clicked a scope. `null` means "only the backend can route this path to a
  volume", and reads as run-it: an SMB id keys on the address and cloud drives route to `root`, so a frontend guess
  would fork routing, and waiting on a wrong guess is how a search stops happening.
- **which arenas have landed** — recorded per volume from the `search-index-ready` event, which names its volume.
- **`pendingVolumeId`** — the one volume a pre-load is in flight for. `prepare_search_index` pre-loads root only, and
  its `loading` field is the backend's promise that an event is coming. `loading: false` with `ready: false` is the
  terminal "root has no index to load", which is what makes search work on a machine that declined indexing.

Waiting when an event IS coming is still worth it: each ungated keystroke would otherwise issue an IPC that blocks on
the same arena load, burning a blocking-pool thread per keystroke for no earlier answer.

**Known gap, still open**: if root's pre-load starts and then fails to read its DB (corruption), no event follows and
the dialog stays on "Loading index…" for the session. The spawned load in `commands/search.rs::prepare_search_index`
logs `VolumeLoad::Failed` / `NotIndexed` and emits nothing, so the gate never resolves. Reopening the dialog re-asks
`prepare_search_index` and recovers. The proper fix is a terminal "no index is coming" signal on that path, which the
live-search work did NOT deliver: its terminal states cover a RUN, and this failure happens before a run starts.

**⌘N doesn't touch readiness.** `clearExtras()` resets what the user typed and leaves what the machine reported alone.
Wiping the readiness flag there meant the gate went back to "waiting" with no second event ever coming, so every later
search in that session silently did nothing.

### The coverage note

The one-shot path (`search-runners.ts`) clears the note before the IPC and writes the answer's `uncoveredScopes` /
`unresolvedScopes` / `targetVolumeId` into it after (`coverageNoteFrom`), so the note always belongs to the run on
screen and a run that throws can't leave a stale caveat under a fresh answer. A LIVE run does the same through its
source's `onCoverage` (`null` on start, the terminal answer at the end); what it fills in is § The live search.
`CoverageNote.svelte` renders it through QueryDialog's `resultsNotice` slot, directly above the results it qualifies.
Both lists are checked independently rather than as an either/or: they're mutually exclusive today by construction, and
a reader that assumed so would go silent if that changed.

The per-drive offer ("Index this drive") shows only for an **uncovered** gap (an unresolved path is on a drive that's
already indexed, so there's nothing to turn on), only for a drive the live volume list can name, and never for a drive
the user silenced (`indexing.silencedDrives`, the same persisted choice the first-connect prompt writes; "Don't ask
again" writes it here). The note itself always renders: silencing the offer doesn't make the gap untrue. The offer acts
on `SearchResult.targetVolumeId` — the volume the BACKEND routed to — because a typed scope can point at a drive the
pane isn't on, and offering to index the wrong one would be worse than saying nothing.

## The live search (`live-search-source.ts`, `live-ranking.ts`)

A search of ground the index doesn't cover walks it, so the answer arrives over seconds or minutes. The shared dialog
carries the streaming machinery (`query-ui/DETAILS.md` § Streaming); everything Search-flavoured stops here.

**Two run paths, one query builder.** `buildRunQuery()` builds the payload (bar + filters + AI pattern + the scope,
whose parse is async) and both paths call it, or an auto-applied answer and an Enter-run one could differ for reasons
nobody could see. `runQuery` (`runSearch`) is the one-shot index answer the DEBOUNCE takes; `streamingSource` is every
run the user asked for. That's Decision 7: a live walk never auto-applies, because six keystrokes would start and
abandon five multi-minute walks.

**What that costs, and what it buys.** The one-shot path is now the only one that can report `uncoveredScopes` — a live
run WALKS a drive with no index rather than reporting it as a gap, which is the whole point of the effort. So the
uncovered copy and the per-drive indexing offer belong to auto-applied runs, and that note ends with "Press Enter and
Cmdr will look through it now" so the gap it reports costs one key to close. The run button says the same thing standing
still (`runTitleOverride`).

**The coverage note gained a `live` half** (`coverageNoteFromRun`): how the walk ended, folders nothing will read, and
ground another walk already holds. Three notes on the copy:

- **Ground nothing will read arrives as TWO lists, and stays two.** `permissionDenied` is a folder somebody refused
  Cmdr; `declined` is a NAS snapshot tree Cmdr won't read on purpose. Two sentences, and only the first has a way out:
  when the run met a refusal, this is macOS, and Cmdr doesn't have Full Disk Access yet
  (`coverage-note.ts::offersFullDiskAccess`), the note offers the setup and `SearchDialog` routes into the onboarding
  wizard's FDA step — the same page first launch shows, never a second one. ❌ Never offer it over `declined`: no
  permission opens a snapshot tree, so it would send someone to System Settings to fix nothing. ❌ Don't infer the cause
  from a path's basename either; that's what the typed cause on the wire is for.
- **The probe is `checkFullDiskAccessQuiet`, and only when a refusal is on screen.** The loud `checkFullDiskAccess`
  fires a TCC-registration storm per denial, and this runs per search. `hasFullDiskAccess` starts at `true`, so nothing
  is offered before the probe answers: an offer that appears and then vanishes is worse than one a moment late.
- **`interrupted` is not an error.** The drive went away, or a root couldn't be read. It means the list is a lower
  bound, and the status bar says that much while the note says which kind of short it was.
- **`stillCovering` is "these arrive a bit later", never "these are lost".** Another walk on the same volume holds that
  ground; its rows reach the same index.

**`rankLiveResults` is ORDERING, never membership.** A live run appends the index's ranked half and then the walk's rows
in arrival order, which is right while the list grows and wrong once it stops, so the run's last act is one re-rank over
the whole set by match quality then recency. It mirrors the backend's `ranking::stem_for` / `classify_match` so the two
can't disagree about which row leads. Importance weights are the index's and a walked row has none, which is exactly the
plan's accepted difference 4. Because nothing is added or dropped, the fork that would make an unindexed drive ANSWER
differently isn't possible here.

**The abandoned-ground signal is the quiet third way a run comes back short.** `walk: completed` no longer means the
list is exhaustive: `abandonedGround` is true when ground was given up on — a folder that stopped responding, one that
failed with an errno Cmdr can't act on, or a subtree pruned after too many failed reads. It covers both what THIS run's
walk gave up on and what an earlier walk recorded, since the index remembers those and the frontier stops offering them
(`crates/cmdr-index`'s `UnreadableCause::Abandoned`), so a run over a wedged mount never goes near them and would
otherwise look exhaustive. Cmdr retries that ground on a backoff of its own. The flag rides ALONGSIDE the ending rather
than inside it — folding it into `interrupted` would tell the user the drive went away, which isn't what happened.
`isIncomplete` and the note both consult it (Accepted difference 9).

⚠️ **It's still only a boolean, and the paths exist.** The wire carries `permissionDenied` and `declined` as lists but
folds abandoned ground into this flag, so the note can say a run is short and can't say which folders. Naming them is a
copy decision (David reviews every user-facing string), and the honest sentence is different from both existing lists:
nothing for the user to do, and Cmdr will try again.

**Count-only tells the truth about a rising number.** "N so far" while the walk runs and after a run that ended short (a
lower bound either way); the exact sentence only when the ground was covered. Even then the total inherits accepted
difference 12 (a live count-only run can double-count a file that is both in the arena and inside a frontier subtree),
which is registered rather than fixed.

## What a search reports (`search-analytics.ts`)

A run reports to analytics ONCE, when it ENDS, because the questions worth asking about a search that can walk aren't
answerable before then: did it need to walk at all, how long did that take, did the person stay for it. The vocabulary
(triggers, endings, coverage kinds, duration buckets) is a pure module that can't see a query, a pattern, or a path; the
dialog owns the clock and the IPC.

Two things about the wiring are load-bearing:

- **The clock starts on the coverage callback's `null`**, not on `searchFilesStreaming` resolving. A small folder's
  whole run can arrive before that promise does, so a start hook downstream of it fires AFTER the run already ended.
- **A run whose successor arrives while it's still going is `superseded`, and only the frontend can say so.** Its walk
  keeps running (Decision 11) and no terminal event for it is coming, so the next run starting is the one moment it can
  be counted.

CTA conversion is two events (`search_cta_offered` / `search_cta_used`) rather than a prop, because the Full Disk Access
offer depends on a TCC probe that answers after the run does. The prop list, the values, and why each exists:
`apps/desktop/src-tauri/src/analytics/DETAILS.md` § "The search events, in detail".

## State shape

The user's typed text and the active mode are one model:

```ts
let query = $state('') // The text in the bar
let mode = $state<SearchMode>('filename') // 'ai' | 'filename' | 'regex'
let lastAiPrompt = $state<string | null>(null) // The natural-language prompt before AI overwrites `query`
let lastAiCaveat = $state<string | null>(null) // The AI translator's caveat (or null)
```

`buildSearchQuery()` reads `query` + `mode`: `mode === 'regex'` produces `patternType: 'regex'`, anything else produces
`patternType: 'glob'`. AI mode is only ever invoked via `executeAiSearch()`, which (1) captures the user's prompt into
`lastAiPrompt`, (2) calls `translateSearchQuery`, (3) overwrites `query` + `mode` with the AI's result so the user can
see and iterate on the translated pattern, and (4) sets `lastAiCaveat` from the result. The `AiPromptStrip` is visible
whenever `lastAiPrompt` is non-null; it clears on `⌘N` (via `clearSearchState`) and on any successful non-AI search
(`executeSearch(fromAiTranslation = false)`).

There is **no `aiPrompt` state and no `namePattern` state**. Read `query` instead. Anywhere `patternType` is needed,
derive from `mode` (`regex => regex`, everything else => glob).

### Where the state actually lives

The state is split into two factories so Search and Selection can each own an instance:

- **Cross-consumer core**: `../query-ui/query-filter-state.svelte.ts` — factory `createQueryFilterState()`. Owns
  `query`, `mode`, size + date filters, `caseSensitive`, `lastAiPrompt`, `lastAiCaveat`, per-mode `handTyped` buffers,
  `results`, `totalCount`, `cursorIndex`, `isSearching`, `lastDialogEvent`, `runOnMount`, `lastRunQuery`. See
  `../query-ui/CLAUDE.md`.
- **Search-only extras**: `search-extras-state.svelte.ts` — factory `createSearchExtrasState()`. Owns `scope`,
  `excludeSystemDirs`, `countOnly`, per-volume index readiness (which arenas landed + their entry counts, plus the one
  volume a pre-load is pending for), `isIndexAvailable`, the `coverageNote`, `lastAiLabel`, `lastAiPattern`,
  `lastAiPatternKind`. Selection doesn't carry these (no whole-drive index, no Search-style scope row, no snapshot
  breadcrumb, no Pattern chip).
- **`buildSearchQuery()`** lives in `build-search-query.ts` and layers `excludeSystemDirs` onto
  `core.buildBaseSearchQuery()`.
- **`recordAiTranslation` is split**: the core writes ONLY to `handTyped[mode]`; the extras' `recordAiPatternAndLabel`
  writes the Pattern chip + label slots. The Search façade calls both in sequence. See `../query-ui/DETAILS.md` §
  "`recordAiTranslation` is split".

`lib/search/search-state.svelte.ts` is a transparent façade re-exporting the legacy named functions that the Search
dialog imports. It also exports `searchQueryState` (the core instance) so prop-driven components like `FilterChips` can
be wired to Search's instance without going through the per-setter façade.

## Search-specific UI behavior

Search-only contracts (cross-consumer ones live in `../query-ui/CLAUDE.md`):

- The Search façade's `recordAiTranslation` (composed call) overwrites the matching hand-typed buffer
  (`handTyped.filename` for a glob, `handTyped.regex` for a regex) so a fresh AI run clobbers the user's earlier
  hand-typed pattern in the same kind.
- `filter-chip-state.ts::deriveSizeChip` accepts a `FileSizeFormat` argument; the chip follows the popover's `kB`/`KB`
  mapping instead of printing the raw enum value.
- `path-pills-layout.ts::scheduleStableWidthMeasure` runs a follow-up re-measure on the next animation frame and again
  ~80ms later. Catches the CSS grid race where `el.clientWidth` reads stale before the parent track settles, which would
  otherwise cause "render full path, then collapse to ellipsis".
- `VolumeBreadcrumb.svelte` reports the static "Search results" name for the `search-results` volume;
  `FilePane.svelte::breadcrumbDisplayPath` renders the snapshot label (`*.svelte`, the AI title, etc.) as the path.
  Don't invert these (label in the volume slot, empty path).
- "Hide boring folders" (label, with that wording specifically — not "Hide system folders"). The tooltip lists ALL
  excluded directory names (one per line, mono font), no "+30 more" truncation.
- `+page.svelte::handleOpenSearchInPane` calls `explorerRef.refocus()` after opening the snapshot so the user can
  immediately navigate/select in the pane without an extra click.
- `pane/has-parent.ts` owns the `hasParent` derivation; `pane/has-parent.test.ts` pins that `selectAll` in a snapshot
  pane covers index 0, not 1.

## Scope: a two-rung ladder (Search-only)

The scope box takes comma-separated folder paths with a `!` prefix for exclusions, parsed via the `parseSearchScope()`
IPC call in `buildRunQuery()` (async, so not part of `buildSearchQuery()`). Selection has no scope row (a selection runs
against a single in-memory folder), so `FilterChips.svelte` accepts a `scopeChipVisible` prop that Selection passes as
`false`; the `⌥I` (open scope popover) and `⌥C` / `⌥V` (inside the popover) shortcuts are suppressed in that case.

A search covers at most one volume, so the popover offers exactly two rungs, both writing a literal path into the box:

- **"Use current folder" (⌥C)** — the focused pane's folder.
- **"This volume" (⌥V)** — the mount root of the volume that folder is on. The widest a search can go; there is no "all
  folders" rung any more.

### The empty box means the current folder

An empty box is NOT "everywhere": `buildRunQuery()` sends `defaultScope.path` as the sole include path, which is the
current folder, or the volume root when the pane has no real folder behind it. Three consequences worth knowing:

- The Search-in chip renders the default's NAME ("Current folder" / "This volume") with `configured: false`, so it shows
  where the search goes without offering an × to clear something the user never set. `Chip` renders `value` whenever
  it's set, independently of `configured`, precisely for this.
- The scope textarea's placeholder is the resolved default PATH, so the popover says exactly what will be searched.
- **A defaulted scope is never persisted.** `scope` state stays `''`, so recent searches and snapshots record "wherever
  I was" instead of baking in a machine-specific absolute path nobody chose; replaying one re-resolves against the pane
  you're standing in then. It also keeps the history dedupe key meaningful — one "report" entry, not one per folder
  visited. A scope the user typed or clicked in IS persisted, because that was a choice.

### Where the presets come from

- `getFocusedPaneSearchScope()` in `lib/file-explorer/pane/focused-pane-reads.ts` reads the focused pane's path,
  history, and the live volume roots from the stores, and delegates to the pure `resolveSearchScope`.
- `+page.svelte` passes the result as the `scopePresets` prop; `SearchDialog` derives `defaultScope` from it.
- When the focused pane's path starts with `search-results://`, reusing it would produce an unsearchable
  `search-results://sr-N` scope, so `searchable-folder.ts` walks the pane's history backward for the most recent
  non-snapshot path. If none is reachable, `currentFolder` is `null`: the ⌥C button renders disabled with the canonical
  tooltip ("Current folder is search results, which isn't searchable. Open a real folder first.") and the default drops
  a rung to the volume, so a search still runs.
- `volumeRootsFrom` filters the switcher's volume list down to the entries that actually OWN an index (`main_volume`,
  `attached_volume`, `network`, `mobile_device`). **Favorites and cloud drives must stay out.** A favorite is a plain
  folder wearing a volume's clothes, so counting `~/Downloads` as a root made "This volume" resolve to `~/Downloads`
  while searching inside it: the maximum rung collapsed onto the default one, and asking for the whole drive silently
  returned one folder (caught by running the app, not by a test). Cloud drives route to `root` in the backend
  (`paths/routing.rs`, plan Decision 16), so treating one as its own volume would disagree with the routing that answers
  the search.
- `volumeRootFor` picks the LONGEST volume root containing the folder, matching whole segments — every path is under
  `/`, so a first-match scan would call a NAS folder a boot-disk one, and a `/Volumes/nas` root must not swallow
  `/Volumes/nas-backup`.

The pure helpers are unit-tested in `searchable-folder.test.ts`.

### Scope shortcuts inside the popover

`⌥F` is the Filename mode chip globally. The scope actions live as `⌥C` and `⌥V`, active ONLY while the Search-in
popover is open. They're wired via a top-level `<svelte:window>` in `FilterChips.svelte` that gates on
`openChip === 'scope'`. Don't promote them back to global shortcuts — that collides with the mode chips.

## Data flow

```
User presses ⌘F
  -> +page.svelte sets showSearchDialog = true
  -> SearchDialog mounts, calls prepareSearchIndex() IPC
  -> Backend pre-loads root's arena (2-3s) and says whether one is coming; emits "search-index-ready {volumeId}" when it lands
  -> User types in the bar -> 1s debounce -> searchFiles(query) IPC (filename/regex modes only)
  -> User presses Enter in AI mode -> translateSearchQuery -> populates filters -> searchFiles
  -> Results displayed, keyboard nav with ↑/↓, Enter navigates to file
  -> Dialog close -> releaseSearchIndex() IPC -> 5 min idle timer -> index dropped
```

The shared parts of this flow (debounce / IME guard / cursor model / Press-Enter hint / `runOnMount` / `lastDialogEvent`
/ `deriveEnterAction`) live in `../query-ui/CLAUDE.md` — Search just sets up the lifecycle around them.

## Search-specific patterns

**Index not available state**: when the backend can't answer at all, `prepareSearchIndex()` throws and the dialog shows
"Drive index not ready…" with scan progress if available, inputs and filters disabled. It is NOT the "this drive has no
index" state: a `prepare` that returns `{ ready: false, loading: false }` is a fine, answerable session — the search
runs and comes back with its coverage gap named.

**AI single-pass flow**: `executeAiSearch()` calls `translateSearchQuery()` once (LLM classifies intent into enums +
extracts keywords, Rust builds the query deterministically), then runs `executeSearch()`. No preflight, no refinement
pass. The previous two-pass system caused ~15% regressions; deterministic structure means there's nothing to refine.

**AI mode keeps the prompt in the bar; pattern lives in its own slot** (post-fixup, clarification 2): After AI
translates, the bar in AI mode STILL shows the user's natural-language prompt — the user can press Enter to
re-translate. The AI's produced pattern (glob or regex) is stored separately on `lastAiPattern` + `lastAiPatternKind`
and surfaced via the Pattern chip in the filter strip. Switching to filename or regex mode (⌘2 / ⌘3) is what hands the
pattern to the matching input; the other mode keeps whatever the user last typed by hand. Per-mode hand-typed buffers
live in `handTyped` inside the core state factory; `switchMode()` swaps `query` between them.

Lifecycle:

- `executeAiSearch(trimmed)` sets `lastAiPrompt = trimmed` BEFORE calling `translateSearchQuery`. The capture is
  unconditional: even if the IPC fails, the user still sees what they asked.
- After the translation succeeds, the façade's `recordAiTranslation({ pattern, kind, label })` populates the core's
  hand-typed buffer AND the extras' `lastAiPattern`, `lastAiPatternKind`, `lastAiLabel` (the LLM-produced short title
  used for the snapshot breadcrumb).
- `lastAiCaveat = translateResult.caveat ?? null`.
- `executeSearch(fromAiTranslation: boolean)` clears `lastAiPrompt` / `lastAiCaveat` when `fromAiTranslation` is false.
  In AI mode it also pulls `lastAiPattern` / `lastAiPatternKind` into the outgoing search query, so the bar's
  natural-language prompt isn't sent to the engine.
- `clearSearchState()` (called by `⌘N`) clears prompt + pattern + label + caveat + the per-mode hand- typed buffers.

**AI transparency strip lifecycle** (clarification 6): the strip stays visible until the user starts a new search OR
presses ⌘N. Switching modes (⌘1 / ⌘2 / ⌘3) does NOT hide it; the strip belongs to the most-recent AI run.

The disabled "Refine…" button on the strip is the placeholder for the chat-back UX.

**Auto mode fallback when AI gets disabled mid-session**: If the AI provider is switched off while the dialog is open
and the active mode is `ai`, the dialog quietly flips to `filename`. The user wouldn't be able to run a search
otherwise.

**MCP `open_search_dialog`**: External openers (the MCP tool) write to the same module-level `$state` and flip
`runOnMount` via `applySearchPrefill()`. The route's `mcp-listeners.ts` handles the `mcp-open-search-dialog` Tauri
event: it sanitizes the payload, defaults `mode` to `'ai'` when AI is enabled (else `'filename'`), calls
`applySearchPrefill`, then flips `showSearchDialog = true` on the route. The dialog's `$effect` consumer for
`runOnMount` fires for both cold-open and hot-prefill paths (one source of truth, two arrival modes), then dispatches to
`executeAiSearch` or `executeSearch` based on mode. The flag is cleared before the search call so the downstream state
writes can't re-trigger the effect. AI mode honors the explicit-trigger contract because the MCP caller's
`autoRun: true` counts as the explicit trigger.

A prefill also clears `lastRunQuery`, because it REPLACES the session: it drops the previous run's results, and that
field names the query those results came from. ❌ Don't restore it. The dialog's reopen-with-results path is a second
producer of `runOnMount` and arms it from exactly that field, in `onMount` — after the prefill's own run has already
fired and cleared the flag. That ran the same query twice a millisecond apart; the second run found its ground claimed
by the first one's walk (`cover/live.rs`), walked nothing, and is the one the dialog renders, so
`open_search_dialog autoRun: true` on unindexed ground showed an empty dialog. Clearing the field also makes
`autoRun: false` mean what it says, instead of the reopen path running the prefill anyway.

**Footer right-edge actions**: the shared `QueryDialog` renders the two footer buttons at the right of the dialog
footer, opposite the recent-searches strip, from Search's `config.secondaryAction` / `config.primaryAction`. They're
standard `$lib/ui/Button`s (`size="regular"`) with the keyboard shortcut on a `ShortcutChip`, disabled (not hidden) when
there are no results:

- **"Go to file"** (⏎): closes the dialog and navigates the active pane to the cursor row's parent folder, focusing the
  file. Routes through the dialog's existing `onNavigate(path)` callback.
- **"Show all in main window"** (⌥A): the primary action. `snapshot-promotion.ts::promoteResultsToPane` builds a
  `SearchSnapshot`, pins it via `setLastAttemptId`, and adds the query to recent searches; the wrapper hands the
  snapshot id to the host and closes the dialog. The host routes the active pane to `search-results://<id>`. State is
  preserved across close + reopen, so `⌘F` reopens to the same results.

Both buttons are hidden (not just disabled) on empty/idle state. Empty + idle inputs disable both (index not ready).

## "Open in pane"

Click on the footer's "Open in pane" button promotes the current result set into a real pane view via the
`search-results://<id>` virtual volume. `snapshot-promotion.ts::promoteResultsToPane`:

1. Builds a `SearchSnapshot` from live state (`getResults()` / `getMode()` / `getQuery()` / filters / scope / flags).
2. Mints a fresh id via `nextSnapshotId()` and stores via `getOrCreate(id, snapshot)`.
3. Pins the snapshot via `setLastAttemptId(id)` so refcount stays ≥1 even before history pushes.
4. Calls `addRecentSearch(historyEntry)`. **This is the one and only call site that adds to recent searches** (per plan
   §3.5: auto-applies and Enter-runs don't pollute the history). For AI mode, the entry's `query` carries the original
   natural-language prompt (via `getLastAiPrompt()`), not the AI's translated pattern.
5. Returns the id, and the wrapper hands it to the host via `onShowAllInMainWindow?.(id)` (`+page.svelte` →
   `DualPaneExplorer.openSearchSnapshotInPane`), which routes through the
   `navigate({ to: { snapshot }, source: 'user' })` transaction so pinned-tab fork / focus / history-push all apply
   uniformly.
6. The wrapper closes the dialog. State is preserved (module-level `$state` survives unmount); ⌘F reopens to the same
   place.

The label shown in the pane breadcrumb (and the snapshot's `label` field) is built by
`snapshot-label.ts::buildSnapshotLabel`:

- AI mode: the LLM-produced label wins when present (a short human-friendly title; max ~40 chars). Falls back to the
  original prompt when the model omits the field.
- Filename mode: the pattern as-is (`*.pdf`).
- Regex mode: the pattern wrapped in slashes (`/pattern/`).

## The walk that outlives the dialog (`walk-handoff.svelte.ts`)

"Open in pane" is the ONE case where a search keeps running with its dialog gone. Everything else about closing the
dialog stops the walk, because nobody is waiting for it; here the results are on screen in a pane, so the rows keep
arriving there.

Four parts, and they're split across three files for one reason each:

- **`walk-handoff.svelte.ts`** owns the run after the handoff: it keeps listening (`observeSearchRun`), appends every
  batch to the snapshot, drives the toast, and hands the run back to a reopened dialog.
- **`walk-handoff-state.svelte.ts`** holds the state cell and the two actions a person can take on it (reopen, stop).
  Split out because the module above owns the toast COMPONENT and the component has to read the live counters; both in
  one file is an import cycle.
- **`live-run-events.ts`** holds the four Tauri listeners. Split out because the handoff and the dialog's transport both
  subscribe and can't import each other.

**The close must NAME the run it spares.** `teardownSearchLifecycle` calls `releaseSearchIndex(handedOffRunId())`, and
`release_search_index` cancels every run BUT that one. A `null` there kills the walk the instant the pane appears: the
pane fills with whatever had arrived, the toast says "still searching" over a walk that isn't, and nothing reports a
problem. Pinned by `SearchDialog.handoff.svelte.test.ts`.

**Four ways it settles, and only one stops the walk.** The terminal event, a new search superseding the run
(`supersedeHandedOffWalk`, called from `createLiveSearchSource.start` — no terminal event is coming for a superseded
run, so without this the toast waits forever), the run failing, and the pane going away. Only the last cancels: it's the
one where the work has lost its last consumer. Superseding deliberately doesn't (Decision 11 — the walk carries on
filling the index).

**The toast is prop-free on purpose.** A toast replaced in place keeps the props it was created with
(`ui/toast/toast-store.svelte.ts`), so counters passed in would freeze at the values they had when the pane opened. The
content component reads the module instead.

**Reopening ADOPTS, it doesn't re-run.** `QueryStreamSource.resume` wins over QueryDialog's reopen-with-results path: a
fresh run would supersede the live one and the pane would quietly stop growing. The resumption carries the rows found
while nobody was listening, or the dialog would show a count the list can't account for.

**Appends need the mutation tick.** `appendSnapshotEntries` bumps it; snapshots aren't `$state`, so without it the rows
land in the store and never reach the screen.

## Snapshot store

`snapshot-store.svelte.ts` holds `SearchSnapshot` records (query, mode, filters, scope, capped 10,000 entries,
totalCount, createdAt, friendly label) under monotonic `sr-N` ids, plus a per-record refcount. The store has no hard cap
on its own — **refcount is the only authority**. Refs come from two sources:

- **Pane history entries** whose `path` starts with `search-results://<id>` hold +1 per occurrence. The tab-state
  manager (`pushHistoryEntry` and the closed-tab lifecycle) drives inc/dec — `navigation-history.ts` itself stays pure
  (no snapshot-store import). Pushing past `MAX_HISTORY_PER_TAB = 100` evicts the oldest entry, and truncating forward
  on a new push after `back()` evicts the discarded tail. Both kinds of eviction surface via `push()`'s `droppedEntries`
  return field, and `pushHistoryEntry` releases the matching refs in one step.
- **The "last dialog attempt" slot** (`setLastAttemptId`) holds +1 for the most-recent dialog search regardless of
  whether any pane references it. Swaps decrement the old id and increment the new one atomically. The dialog calls this
  on each new search.

### Closed-tab lifecycle and refs

Tab close via `closeTabRecording` does NOT release refs; ownership transfers to the `ClosedTab` entry on the pane's
closed-tab stack (cap default 10). Reopen (`⌘⇧T`) just pops the entry back — no double- count. The refs only release
when the closed-tab stack evicts the entry (cap overflow in `pushClosed`, or manual `trimClosedStack`). Implemented via
the `transferSnapshotRefs(closedTab, 'transfer' | 'release')` helper in `tab-state-manager.svelte.ts`. The non-recording
`closeTab` / `closeOtherTabs` (used in tests and programmatic flows) release refs immediately, since nothing else holds
them.

**`{#key activeTabId}` recreation is safe**: history lives on `TabState`, not on the pane. The dual- pane explorer
destroys and recreates `FilePane` on tab switch (cold load), but `TabManager` survives, and the per-tab `history` field
is untouched. Snapshot refs therefore persist across pane recreation.

## Capability flags

The `search-results` row of the per-kind `VolumeCapabilities` table (`lib/file-explorer/pane/volume-capabilities.ts`) is
`{ canPasteInto: false, canCreateChild: false, canRenameInPlace: false, canBeSource: true, … }`.
`SearchResultsView.svelte` reads it directly via `capabilitiesForKind('search-results')` (the row context menu's
`restrict` flag reads `!caps.canRenameInPlace`). Every capability-GUARD consumer reads the table via `capabilitiesFor`
(the A6 conversion is complete): the F-bar + keyboard dispatch (destination-op guards), clipboard (snapshot-clip
`pathScheme`, MTP refusal `kind === 'mtp'`), transfer/delete (`!hasBackendListing` source routing + the
`search-results`-kind-scoped dest block), `pane-commands` (`isSnapshotPane` off `!hasBackendListing`), MCP sync
(`!syncsToMcp`), and `has-parent` (`hasParentRow`). See `lib/file-explorer/pane/DETAILS.md` § "Volume capabilities" for
the per-site breakdown. Consumers:

- **F-key bar** (`lib/file-explorer/pane/FunctionKeyBar.svelte` mounted in `routes/(main)/+page.svelte`): derives its
  `canMkdir` / `canMkfile` (= `caps.canCreateChild`), `canRename` (= `caps.canRenameInPlace`), and `canSourceOps` (=
  `caps.canBeSource`) off `capabilitiesFor(focusedVolumeId)`. On a `search-results` pane, F2 (Rename), F7 (New folder),
  and Shift+F4 (New file) render visibly disabled; F5 / F6 / F8 (Copy / Move / Delete) stay enabled because the snapshot
  row is source-OK.
- **Right-click context menu**: `showFileContextMenu` IPC takes a `restrictDestinationActions` flag. When `true`, the
  Rust menu builder omits Rename and New folder. Source-side items (Open, Copy, Move, Delete, Show in Finder, Copy
  filename, Copy path) stay. The flag is set when `!canRename && !canMkdir`.
- **Keyboard shortcut dispatch** (`routes/(main)/command-dispatch.ts::blockedByCapabilities`): catches `⌘V`, `⌘⌥V`,
  `F7`, Shift+F4, `F2` / `file.rename` when the focused pane's capabilities can't satisfy the destination op
  (`!canPasteInto` / `!canCreateChild` / `!canRenameInPlace`). Surfaces the friendly toast
  `"Search results aren't a folder. Paste into a real folder instead."` (canonical string
  `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) — for the `search-results` kind only; a `network` pane keeps its prior silence.

### Cross-snapshot purge

A stored snapshot outlives the pane that opened it and the operation that emptied it: the user can reopen
`search-results://sr-3` in any window, hours later, and its rows must not name files that were moved, deleted, trashed,
or renamed away meanwhile. `snapshot-purge.ts` keeps it honest, subscribed once per window from `(main)/+page.svelte`.

**Its input is the `write-source-item-done` stream, which is outcome and not intent.** A top-level source item emits
once it is fully processed, carrying `sourceRemoved`; the purge acts on that flag and ignores everything else. So:

- A **skipped** item emits nothing, and its row stays (the file is still on disk).
- An item a **cancel** never reached emits nothing, and the ones the operation did get through are still purged. There
  is no "on completion" about it.
- A **cross-FS move** reports `sourceRemoved: false` from its staging pass and `true` from its source-delete phase, so a
  Skip decided in between can't purge a file that stays.
- A **copy** reports `false` throughout. A **bulk rename** reports `true`: nothing answers to the old path any more.

**Why not the operation's `sourcePaths`.** They are what the user ASKED for, so they miss all four cases above; the
dialog that holds them is also the wrong place, since a window watching an operation it never started has none
(`file-explorer/pane/DETAILS.md` § "Birth context") and a snapshot is not a pane's property anyway. Putting the vanished
paths on the completion event instead was the other candidate and is not available: a 500k-file move would ship 500k
strings to every webview. **Why not `directory-diff`**, which also reports vanished rows: it is emitted per WATCHED
listing, and a delete from a search-results pane targets a real file whose parent folder is usually open in no pane at
all, so the flow this feature exists for would purge nothing.

The purge is per top-level source path, so a snapshot row for a file INSIDE a moved directory outlives its file. That
was true of the old shape too; the honest fix is a prefix sweep, and it needs care around a directory merge whose
children were partly skipped.

`removeEntryFromAllSnapshots(path)` is the store-side half:

1. Walks every stored snapshot and replaces its `entries` array with one that excludes the deleted path (preserves
   reference identity on the unchanged entries; only the array changes).
2. Bumps a module-level `mutationTick` `$state` whenever at least one snapshot was mutated.
3. Leaves `totalCount` alone — the existing `entries.length` vs `totalCount` mismatch is the truncation signal.

`SearchResultsView.svelte`'s snapshot lookup reads `getMutationTick()` inside its `$derived` so the view re-renders
after a purge. Without the tick, the `Map` mutation would be invisible to Svelte reactivity (snapshots aren't `$state`
themselves, by design — see the store's header).

### Source-side ops from the snapshot pane

With `isSourceOK: true`, Cmd+C / Cmd+X / F5 / F6 / drag-out run against the cursor + selection in the snapshot pane. The
snapshot pane shares `FilePane.selection` state with normal panes. Wire path:

- **Cmd+C / Cmd+X** route through `DualPaneExplorer.copyToClipboard` / `cutToClipboard`, which detect the snapshot pane
  via `getSnapshotClipboardPaths` and call `copy_paths_to_clipboard` / `cut_paths_to_clipboard` (paths-by-value sibling
  IPCs of the listing-id-keyed `copy_files_to_clipboard` family). The Rust commands reuse
  `clipboard::write_file_urls_to_clipboard` and `set_cut_state` / `clear_cut_state`, so the system clipboard contract
  (file URLs + newline-separated text) is identical.
- **F5 / F6** route through `openUnifiedTransferDialog`, which detects `volumeId === 'search-results'` and calls
  `transfer-operations::buildTransferPropsFromSnapshot` instead of the listing-id-driven builders. The snapshot's
  selected (or cursor) entries are resolved to paths via `snapshot-store::resolveSnapshotPaths`, fed into the same
  `TransferDialogPropsData` shape every transfer uses, and the existing `copy_files` / `move_files` IPCs run with
  `sources: Vec<String>`.
- **Drag-out** uses the `'paths'` drag context in `lib/file-explorer/drag/drag-drop.ts`: when `FullList` is rendered
  with `staticEntries` and the user drags a selection, the FE builds a paths array from `getEntryAt(idx)` and routes
  through `start_drag_paths`.
- **Post-move snapshot cleanup**: covered by the cross-snapshot purge above. After F6 from the snapshot pane, the rows
  the move actually took disappear from every snapshot that referenced them; a skipped one stays.

Destination-side write ops are still blocked: pasting INTO a search-results pane shows the canonical
`SEARCH_RESULTS_NOT_A_FOLDER_TOAST` (via the F-bar disablement, the menu item omission, and the dispatcher's
`blockedByCapabilities` guard). `openTransferDialog` also blocks F5/F6 when the OPPOSITE pane is a snapshot, so the
shortcut path can't accidentally route a copy/move INTO a snapshot.

## Search-specific decisions

**Decision**: Recent-search entries are added only on "Open in pane", not on Enter / auto-apply. **Why**: David's
explicit design call. The 1,000-entry budget stays signal-rich (results worth acting on) instead of polluted with every
keystroke-debounced auto-apply. Auto-apply fires on a 1 s debounce — adding every fire would turn the history into a
high-frequency log of false starts. The Rust IPC accepts any entry; the gate is the frontend's single `addRecentSearch`
call site in `snapshot-promotion.ts::persistRecentSearch`, reached only from the two actions above.

**Decision**: "Open in pane" promotes to the `search-results` virtual volume, not a special FilePane mode. **Why**: We
already had the precedent: the `network` browser is a `volumeId` the FilePane special- cases, not a forked pane
component. Following that pattern lets us reuse the entire file-explorer toolkit (selection, keyboard nav, copy / move
source, history, Quick Look, drag-out) for free, and gives the user a real navigable pane with history-aware `⌘[` /
`⌘]`. A "special mode" branch would have leaked into every pane-aware module forever; the virtual-volume namespace
concentrates the special-casing into a small number of well-documented sites (FilePane gates, `navigate.ts`'s
`commitPathFromListing` drop-foreign-listings policy, the breadcrumb label resolver).

**Decision**: Dialog, not a panel or sidebar. **Why**: Search is a focused, transient task. A command- palette-style
overlay matches this usage pattern and doesn't consume permanent screen real estate.

**Decision**: Structured filters always visible (not hidden behind "advanced"). **Why**: The filter row is compact (one
line) and makes the query model transparent. Users see exactly what's being searched.

## Search-specific gotchas

**Gotcha**: `prepareSearchIndex()` failure means index unavailable. **Why**: The backend returns an error when
`get_read_pool()` returns `None` (indexing disabled or not started). The dialog catches this and enters the disabled
state.

## References

- [AI search eval history](../../../../../docs/notes/ai-search-eval-history.md) -- Four rounds of prompt tuning for the
  AI natural language to structured query translation, with a 30-query test catalog and lessons learned.
- `../query-ui/CLAUDE.md` -- Cross-consumer query UI primitives (the bar, mode chips, filter chips, results list,
  recent-items, the filter-state factory, the shared keyboard contract, gotchas, and decisions).

## Dependencies

- `$lib/tauri-commands` -- `prepareSearchIndex`, `searchFiles`, `releaseSearchIndex`, `translateSearchQuery`,
  `parseSearchScope`, `getRecentSearches`, `addRecentSearch`, `removeRecentSearch`, `clearRecentSearches`,
  `applyRecentSearchesMaxCount`, `showFileContextMenu`, `showInFinder`
- `$lib/shortcuts/key-capture` -- `isMacOS()` for the footer action's macOS/Linux label fork
- `$lib/indexing` -- `isVolumeScanning(ROOT_VOLUME_ID)`, `getEntriesScanned` (LOCAL index-build progress for the
  unavailable state; keyed to `root` so a network scan doesn't flip the label while root's count stays 0)
- `$lib/settings` -- `getSetting('ai.provider')` (AI chip visibility, ⌘ shortcut numbering)
- Shared primitives from `../query-ui/CLAUDE.md`
- CSS variables from `app.css`

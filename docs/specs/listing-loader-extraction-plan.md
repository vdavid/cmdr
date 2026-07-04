# FilePane listing-loader extraction plan

Status: reviewed twice (fresh-eyes Opus rounds 1-2 folded in; round 2 verdict: sound and ready to execute). Worktree:
`.claude/worktrees/extract-listing-loader`, branch `extract-listing-loader` off local `main` (978e0ffb).

## Goal

Drain the last deliberately-deferred cluster out of `FilePane.svelte` (~2815 lines): the **listing loader** — the
`loadDirectory` / `handleListingComplete` / `resetLoadingState` orchestration, the six streaming-event listeners, the
`pendingLoad` promise machinery, and the **generation + listingId token model** that drops foreign (stale) listings so a
late listing can never land in the wrong pane or overwrite a newer navigation.

This was held back as the highest-risk extraction because the cluster owns ~15 reactive state write-backs and is the
crown-jewel staleness guard. The extraction is **behavior-preserving; no redesign**. The `FilePaneAPI` export set
(`pane/types.ts`) stays byte-identical. Streaming-listener wiring, `pendingLoad` resolution order, and reset semantics
stay identical.

## Why this is safe to do as a pure extraction

The cluster is a self-contained lifecycle machine. Its inbound edges (all preserved as thin delegates / injected deps):

- **`FilePaneAPI` methods that wrap it** (external callers reach it ONLY through `paneRef.*`, so keeping `FilePane`'s
  exports as thin delegates means zero change at any external call site): `navigateToPath`, `navigateToParent`,
  `handleCancelLoading`, `whenLoadSettles`, `isLoading`, `getListingId`, `getSwapState`, `adoptListing`. Callers:
  `swap-panes.ts`, `edge-flow-handlers.ts`, `dialog-state`, `file-operation-commands`, `pane-mcp-sync`, `rename-flow`,
  `listing-diff-sync`, `navigate.ts`.
- **`loadDirectory` and `navigateToFallback` are also called intra-component and by sibling factories**, so they must be
  PUBLIC loader methods (not internal-only): `loadDirectory` is called by the retry-on-reachable `$effect` (~2083), the
  initial-path/MTP-connect `$effect` (~2107/2127), `onMount` (~2325), `navigateToParent`, `navigateToFallback`, the
  open/navigate handlers (~1701/1717), `handleVolumeChange` (~1775), and the `createSmbViewState` dep (~486).
  `navigateToFallback` is passed as a dep to BOTH `createSmbViewState` (~487) and `initListingDiffSync` (~2252). After
  the move, `FilePane` passes `loader.loadDirectory` / `loader.navigateToFallback` into those factory dep objects.
- **Three `$effect`s / `onMount`** stay in `FilePane` and call `loader.loadDirectory(...)`. They also currently write
  loader-owned lifecycle state directly (`onMount` sets `loading = false`; see the co-ownership note below) — those
  writes route through loader setters after the move.

## The crown jewel: the token / drop-foreign-listings model (must be provably unchanged)

Today, `loadDirectory`:

1. Rejects any pending load (`rejectPendingLoad('Superseded by new navigation')`).
2. Bumps a per-pane counter: `const thisGeneration = ++loadGeneration` — this captured value is the load's identity.
3. Cancels the previous backend listing, clears the six `unlisten*` handles, sets loading state, `await tick()`.
4. Generates `newListingId = crypto.randomUUID()`, sets `listingId = newListingId`.
5. Registers the six listeners. **Every listener body is guarded by the SAME predicate:**
   `payload.listingId === newListingId && thisGeneration === loadGeneration`.
6. `await listDirectoryStart(...)`; then re-checks `if (thisGeneration !== loadGeneration) { cancelListing; return }`
   (and the same guard in the `catch`).

The predicate is what makes a foreign listing inert: once a newer `loadDirectory` runs, `loadGeneration` has advanced,
so the older load's captured `thisGeneration` no longer matches and the SYNCHRONOUS body of each still-registered
listener no-ops — even before its `unlisten*` fires. The `payload.listingId` half is defense-in-depth against a backend
event tagged with a different listing id.

**Important nuance — the guard protects only the synchronous listener entry, NOT the async tails.** Two handlers do work
after an `await` that is NOT re-guarded on generation:

- `onListingError` → `pathExistsChecked(loadPath).then(...)` (~1391-1411) calls `resetLoadingState` /
  `navigateToFallback` / `onPathChange?.(loadPath)` after its await, with no `thisGeneration === loadGeneration`
  re-check.
- `handleListingComplete` writes `totalCount` synchronously (~1461) then writes `cursorIndex` after `await findFileIndex`
  (~1466-1471), again without a re-check.

This is CURRENT behavior. The extraction preserves it byte-for-byte: **do NOT add re-guards to these async tails** —
that would be a redesign, not a behavior-preserving move. M3 includes a test that pins the current (unguarded) async-tail
behavior so a later "tidy-up" can't silently change it.

**Extraction rule:** the synchronous entry predicate moves verbatim. We factor it into a named pure helper
`isEventForCurrentLoad(payloadListingId, captured, liveGeneration)` so it can be unit-tested in true red→green isolation,
then call it from every listener's entry. No behavioral change: same two comparisons, same order, same
guard-the-entry-only semantics.

Note there is a SECOND, unrelated drop-foreign policy at the coordinator level (`navigate.ts::commitPathFromListing`,
DETAILS § navigate transaction). That one is out of scope and untouched. This plan concerns only the pane-local
generation guard.

## State the cluster owns / touches

Loader-owned state. Some of these are written ONLY by the loader; several are **co-owned** — written by external
collaborators too, so the loader must expose SETTERS for them (this is the correction from review round 1; treating them
as write-exclusive "outputs" would silently break the external writers):

- `loadGeneration` (plain), `isDestroyed` (plain — also read by the tag-sweep `isStale`).
- `listingId` (`$state`) — loader-write-exclusive.
- `loadedPath` (plain), `lastSequence` (plain). **`lastSequence` is CO-OWNED:** `initListingDiffSync` writes it via a
  `setLastSequence` dep (~2237). Loader exposes `setLastSequence`; `FilePane` rewires the diff-sync dep to
  `loader.setLastSequence`.
- `loading` (`$state`) — **CO-OWNED:** `onMount` (~2322/2338) and `injectError` (~2024) also set it. Loader exposes a
  setter (or `setLoadingIdle()` / a small `injectError()` method). Initialize `loading` to `true` (matches today's
  `$state(true)` so the spinner shows on first paint before onMount).
- `openingFolder`, `loadingCount`, `finalizingCount` (`$state`) — loader-write-exclusive.
- `totalCount` (`$state`) — **CO-OWNED:** `initListingDiffSync` writes it via `setTotalCount` (~2242) and the
  `includeHidden` `$effect` writes it after a `getTotalCount` refetch (~2052). Loader exposes `setTotalCount`; both sites
  rewire to `loader.setTotalCount` (the includeHidden effect stays in `FilePane`, reading `loader.listingId` /
  `loader.loading` and writing `loader.setTotalCount`).
- `error`, `friendlyError` (`$state`) — **CO-OWNED:** `injectError` (~2022-2023) sets both. Fold into the loader's
  `injectError()` method (or setters).
- `volumeRootFromEvent` (`$state`) — loader-write-exclusive (read by `effectiveVolumeRoot`→`hasParent`).
- The six `unlisten*` handles (plain).
- `pendingLoadResolve` / `pendingLoadReject` (plain) + `resolvePendingLoad` / `rejectPendingLoad`.

Shared `FilePane` state it must poke (NOT owned — injected). **Cursor + entry writes inside the cluster are RAW `$state`
assignments, so inject RAW setters, NOT the `FilePaneAPI` methods** (this is the review-round-1 trap): `setCursorIndex`
(the export, ~627) branches on network/search-results, scrolls, `await tick()`s, and fires `debouncedSyncMcp` — the
cluster's `cursorIndex = …` sites (`handleListingComplete` ~1468/1470, `adoptListing` ~1011) do their OWN scroll and must
NOT go through it. Use a raw setter mirroring the existing `applyCursorIndex` dep (~2230):

- `cursorIndex` (RAW setter, mirrors `applyCursorIndex`), `currentPath` (get/set), `cacheGeneration` bump (adoptListing),
  `entryUnderCursor` (RAW setter to null; refresh via the injected `fetchEntryUnderCursor`), `syncStatusMap` (cleared),
  `syncRetryTimer` (cleared), `selection` (clearSelection / get/setSelectedIndices).

Collaborators it calls (injected): `rename.cancel`, `cancelClickToRename`, `dismissTransientToasts`, `jump.clear`,
`benchmark`, `debouncedSyncMcp.call`, `fetchEntryUnderCursor`, `fetchListingStats`, `sweepListingTags`,
`evictPerPathIconsForDir`, list refs' `scrollToIndex`.

Reactive reads it needs (injected getters): `volumeId`, `includeHidden`, `sortBy`, `sortOrder`, `directorySortMode`,
`caps`, `hasParent`, `isMtpView`, `volumePath`, `viewMode`, `canonicalPath`, `volumeName`, `briefListRef`/`fullListRef`.

Callbacks (injected): `onPathChange`, `onMtpFatalError`, `onCancelLoading`, `onVolumeChange`.

IPC (imported directly in the factory, via `$lib/tauri-commands`): `cancelListing`, `listDirectoryEnd`,
`listDirectoryStart`, `findFileIndex`, `pathExistsChecked`, `resolveValidPath`, `renderListingError`, and the six
`onListing*` subscribers.

## Design decision — the boundary shape (REVIEW THIS FIRST)

Two idiomatic shapes exist in `pane/`; the reviewer should rule on which:

- **Option A — state-owning factory (recommended, matches `git-browser-sync`/`volume-space`/`dialog-state`).** The
  factory owns the lifecycle `$state` (`loading`, `openingFolder`, `loadingCount`, `finalizingCount`, `totalCount`,
  `error`, `friendlyError`, `listingId`, `volumeRootFromEvent`) and exposes them as getters PLUS the co-ownership setters
  above (`setTotalCount`, `setLastSequence`, `injectError`/loading setter). `FilePane` reads `loader.loading`,
  `loader.listingId`, `loader.totalCount`, etc. Read-site churn is the cost: ~40–60 references across markup, `hasParent`
  (via `effectiveVolumeRoot`←`volumeRootFromEvent`), `effectiveTotalCount`←`totalCount`, the `mcpSync`/`jump` deps
  getters, and the alt-view `{#if}` chain switch to `loader.*`. Getters preserve reactivity (proven by
  `git-browser-sync`'s `gitRepoInfo`). NOTE (review round 1): these fields are co-owned, not pure outputs — so the
  churn also includes non-markup WRITE sites that are easier to miss than markup (the `includeHidden` effect, the
  diff-sync deps, `onMount`, `injectError`). The implementation checklist below enumerates them so none is dropped.

- **Option B — surgical factory (matches `type-to-jump-controller`).** `FilePane` keeps the `$state` locals; the factory
  takes a getter + setter per field. Minimal read-site churn (markup untouched), but a ~15-accessor deps object and the
  lifecycle state stays visually in `FilePane` even though the loader is its primary writer.

**DECISION (during M2 implementation): Option B.** The read-site enumeration proved decisive: `listingId` / `totalCount`
/ `loading` / `error` are read by ~60 sites, and the majority sit in NON-loader concerns — `fetchEntryUnderCursor`,
`fetchListingStats`, `selectAllFiles`, `updateMenuContext`, `refreshView`, the cursor/selection `$effect`s, the
markup, and FIVE sub-factory dep getters (`pane-mcp-sync`, `type-to-jump-controller`, `listing-diff-sync`,
`rename-flow`, `dialog-state`). That distribution means these are genuinely pane-shared state (many readers) with the
loader as their primary WRITER — the same relationship `cursorIndex` has with `type-to-jump-controller` (stays in the
pane, injected). Option A would prefix ~60 unrelated reads with `loader.`, coupling every reader to the loader and making
`FilePane` less readable, not more — and each rewrite is a reactivity-bug risk for zero behavioral gain. So the ideal end
state here (`ideal-over-cheap`, honestly applied) is B: the loader owns the ORCHESTRATION + the generation/pendingLoad
machinery (the actual risky cluster); the lifecycle `$state` stays in `FilePane`, and the loader reads/writes it through
a focused injected accessor set. This keeps the crown-jewel token model, the six listeners, `pendingLoad`, and the
generation counter fully encapsulated in the factory while leaving the ~60 read sites untouched.

Under B: `loadGeneration` (the ONLY two bump sites — `loadDirectory` and `adoptListing` — both move into the loader, so
the counter is loader-private), `isDestroyed`, `loadedPath`, the six `unlisten*`, and `pendingLoadResolve/Reject` are
loader-owned. `getSwapState`/`adoptListing` move into the loader (paired with the counter; they read pane state via
injected getters, bump `cacheGeneration` + set the RAW cursor + selection via injected setters). The trivial accessors
that only read a `$state` slot stay in `FilePane`: `isLoading`, `getListingId`, and `injectError` (a 3-line debug method
touching only pane `$state`, no loader-owned state — no reason to move it).

**Read/write-site checklist for Option A (drop none):** markup `{loading}` / `{error}` / `{friendlyError}` / count
displays / the alt-view `{#if}` chain; deriveds `effectiveVolumeRoot` (`volumeRootFromEvent`), `effectiveTotalCount`
(`totalCount`), `hasParent` (indirect); `createPaneMcpSync` getters (`getListingId`, `getTotalCount`); `jump` deps
(`getListingId`, `getLoading`); the `includeHidden` `$effect` (reads `listingId`/`loading`, writes `totalCount`); the
`initListingDiffSync` deps (`getListingId`, `getLastSequence`/`setLastSequence`, `setTotalCount`, `navigateToFallback`);
`createSmbViewState` deps (`loadDirectory`, `navigateToFallback` — keep the exact arrow wrapper `(path) => void
loader.loadDirectory(path)` that DROPS `selectName`, ~486, or the SMB fallback signature shifts); `onMount` (`loading`
write, `loadDirectory` call); the dirExistsPoll (`navigateToFallback`, ~2392/2401); `injectError`;
`isInErrorState`/Quick-Look hooks (read `friendlyError`); `getSwapState`/`adoptListing`. Three reactivity-sensitive
`$effect`s gate on `listingId && !loading` and must become `loader.listingId && !loader.loading`: the includeHidden
refetch (~2042), the cursorIndex→entry/MCP-sync effect (~2151), and the selection→stats effect (~2201).

## Milestones

### M0 — Characterize (before any move)

- Read the six listener bodies, `handleListingComplete`, `resetLoadingState`, `navigateToFallback`, `navigateToPath`,
  `navigateToParent`, `handleCancelLoading`, `getSwapState`, `adoptListing`, `whenLoadSettles` once more against the live
  file so the move is a faithful cut/paste, not a paraphrase.
- Confirm `FilePaneAPI` shape in `pane/types.ts` and snapshot it (copy the interface text) to diff after.

### M1 — Extract the pure token predicate (TRUE red→green TDD)

- New `pane/listing-token.ts` (pure, no runes): `isEventForCurrentLoad(payloadListingId, captured: { listingId,
  generation }, liveGeneration) => boolean`.
- `pane/listing-token.test.ts` written FIRST, with the function stubbed to `return true` so the "drop a foreign event"
  case is genuinely RED, then implement to green. Cases: same listingId + same generation → accept; same listingId +
  advanced generation → drop; different listingId + same generation → drop; both stale → drop.
- Docs: none yet. Checks: `pnpm check eslint-typecheck-ts vitest-desktop -q` scoped, plus the fast lint lane.

### M2 — Extract the loader factory

- New `pane/listing-loader.svelte.ts`: `createListingLoader(deps): ListingLoader`. Module doc comment states it owns the
  listing lifecycle + generation guard, mirrors the `git-browser-sync` header style, and links the token model.
- Move the cluster verbatim (Option A: bring the `$state` in; Option B: wire getters/setters). Every listener calls
  `isEventForCurrentLoad(...)` at its synchronous entry (async tails stay unguarded — see crown-jewel § "do not
  re-guard"). Expose the co-ownership setters (`setTotalCount`, `setLastSequence`, `injectError` + loading setter) and
  the RAW cursor/entry setters used by `handleListingComplete`/`adoptListing`.
- `cleanup()` performs the FULL listing teardown, not just unlisten: it reads loader-owned `listingId`/`loadedPath`, so
  `onDestroy`'s `if (listingId) { cancelListing; listDirectoryEnd; evictPerPathIconsForDir(loadedPath) }` block (~2412)
  MOVES into `cleanup()` alongside the six `unlisten*` fires and `isDestroyed = true`. `FilePane`'s `onDestroy` keeps its
  other teardown (timers, debounces, `jump.dispose`, `diskSpace.cleanup`, `gitBrowser.cleanup`) and calls
  `loader.cleanup()`.
- `FilePane.svelte`: replace the inline cluster with `const loader = createListingLoader({...})` and thin export
  delegates (`navigateToPath`, `navigateToParent`, `handleCancelLoading`, `whenLoadSettles`, `isLoading`,
  `getListingId`, `getSwapState`, `adoptListing`, and `injectError` — the last folds into `loader.injectError` but stays
  an exported `FilePaneAPI` method) that call `loader.*`. Byte-identical signatures. `onDestroy` calls `loader.cleanup()`
  (placement relative to the other teardown is behavior-neutral — they're independent concerns).
- Docs: update `pane/CLAUDE.md` module map (add `listing-loader`) and `pane/DETAILS.md` (a "listing loader / generation
  guard" subsection; move/point the token-model description here as the single source). `pane/CLAUDE.md` is already ~596
  words against the hard 600 cap, so this milestone MUST condense to make room for the new module-map entry (per
  `docs/doc-system.md` condense-first), not just append.
- Checks: `pnpm check eslint-typecheck-ts svelte -q` + scoped vitest.

### M3 — Factory integration tests (foreign-listing dropped, proven red by mutation)

- `pane/listing-loader.svelte.test.ts` (harness like `git-browser-sync.svelte.test.ts` / `smb-view-state.svelte.test.ts`
  — `$effect.root` + mock deps; mock the `onListing*` subscribers to capture callbacks and return unlisten spies; mock
  `listDirectoryStart` to resolve).
- Pinned behaviors:
  - **Foreign complete dropped:** start load A (capture its complete cb + listingId), start load B (supersedes), fire
    A's complete → `totalCount`/cursor unchanged, `onPathChange` NOT called with A's path; fire B's complete → accepted.
  - Same for A's `error`, `cancelled`, `progress`, `opening`, `read-complete` callbacks after B starts → all no-op (the
    synchronous entry guard drops them).
  - **Async-tail preservation (pins CURRENT unguarded behavior, so a later tidy-up can't change it):** fire A's `error`
    entry (accepted — A is still current), THEN start B, THEN resolve A's `pathExistsChecked` promise → assert the tail
    still runs as it does today (this is deliberately NOT re-guarded). Likewise `handleListingComplete`'s post-`await
    findFileIndex` cursor write. Comment these tests clearly as behavior-lock, not correctness assertions.
  - **Post-await supersession:** if `loadGeneration` advances during the `await listDirectoryStart` (and in the `catch`),
    the abandoned listing is cancelled and no state is committed.
  - **pendingLoad ordering:** `navigateToPath` rejects the prior pending load, resolves on complete; `whenLoadSettles`
    chains onto the existing resolver without disturbing a waiting `navigateToPath`.
  - **reset semantics:** `resetLoadingState(msg)` rejects pending with the message; cancel path rejects with
    `'Loading cancelled'`; `preserveTotalCount` respected.
  - **branch coverage for verbatim-moved branches** (cheap, and exactly what a paraphrase can silently drop):
    `navigateToFallback` outside-volume branch (`onVolumeChange('root','/',target)`) vs the in-volume `currentPath +
    loadDirectory` branch (~1259-1266); `handleCancelLoading`'s `!loading || !listingId` early return (~1522);
    `navigateToParent`'s two early returns (at-root, unresolved `canonicalPath`, ~1068/1072).
- Prove RED: temporarily delete the generation half of the predicate (or the post-await guard), run — the foreign-drop
  tests MUST fail; restore — green. Record this in the commit body.
- Checks: scoped vitest, then the full `eslint-typecheck-ts` lane.

### M4 — Verify, gate, wrap

- `FilePaneAPI` diff: confirm `pane/types.ts` unchanged and the export signatures in `FilePane.svelte` byte-identical to
  the M0 snapshot.
- E2E gate: `pnpm check desktop-e2e-playwright` (216/216). ANY failure → re-run once; if it recurs even intermittently,
  STOP and discriminate against `main` (do not write off as flake — the suite was just de-flaked to 7 clean runs).
- `file-length`: run `pnpm check file-length` to ratchet FilePane's 2815 floor DOWN to its new count (allowed: draining,
  not loosening). Confirm `listing-loader.svelte.ts` is under the 800 warn line (expected ~450–550); if a test file
  exceeds 800, flag to David rather than allowlisting silently.
- Full `pnpm check` (acceptable red: cargo-audit quick-xml only).
- Docs final pass: `pane/CLAUDE.md` + `DETAILS.md` current, single-sourced (token model lives in ONE place now).

## Checks cadence

- Iterating: `pnpm check eslint-typecheck-ts svelte vitest-desktop -q` scoped by name where possible.
- Per milestone: E2E where a milestone changes runtime wiring (M2, M4).
- Before wrapping: full `pnpm check` + the type-aware `eslint-typecheck-ts` lane (a sibling-agent lesson: the fast lint
  lane alone misses type-aware rules).

## Risks & mitigations

- **Dropped co-ownership setter** (the top risk from review round 1): if the implementer treats `totalCount` /
  `lastSequence` / `loading` / `error` as write-exclusive outputs and omits the setters, the external writers
  (file-watcher diff-sync count/sequence updates, the includeHidden refetch, onMount spinner clear, injectError) silently
  break. Mitigation: the Option A read/write-site checklist above enumerates every writer; a scoped vitest of diff-sync
  plus the hidden-files-toggle and swap E2E exercise these. Verify each `set*` dep resolves to a `loader.*` setter after
  the move.
- **RAW vs `FilePaneAPI` cursor setter** (review round 1 trap): using the exported `setCursorIndex` inside
  `handleListingComplete`/`adoptListing` would double-scroll, mis-branch on view kind, and fire extra MCP syncs. Use the
  raw setter mirroring `applyCursorIndex`. Caught by cursor-position E2E + swap E2E.
- **Reactivity break under Option A** (a getter not tracked, a derived reading a stale value): caught by E2E (loading
  spinner, breadcrumb, count displays) + the type lane. Mitigation: move deriveds that read lifecycle state (`hasParent`
  via `effectiveVolumeRoot`←`volumeRootFromEvent`, `effectiveTotalCount`←`totalCount`) carefully; keep them in `FilePane`
  reading `loader.*` getters rather than duplicating them in the factory.
- **`adoptListing`/`getSwapState` share `loadGeneration`** with the loader — they MUST move with it (swap-panes would
  otherwise bump a different counter than the loader reads). Pinned by `swap-panes.test.ts` + the swap E2E.
- **`whenLoadSettles` chains onto `pendingLoadResolve`/`Reject`** — the chaining semantics (both prev and new fire) must
  survive. Pinned by an M3 test.
- **Tag-sweep `isStale`** closes over `isDestroyed` + `loadGeneration` + `listingId` — all loader-owned after the move,
  so the closure reads them from the factory. Pinned indirectly by not regressing tag E2E.

## Parallelism

None. Sequential, single-agent. A sibling agent works only in `test/e2e-playwright/conflict-*` — do not touch those.

## Definition of done

Cluster extracted; token model pinned by a pure unit test (red→green) AND factory integration tests (foreign listing
proven dropped, red-by-mutation); `FilePaneAPI` byte-identical; E2E 216/216 per milestone; full `pnpm check` green (bar
sanctioned red); `CLAUDE.md`/`DETAILS.md` current and single-sourced; self-reviewed solid AND elegant.

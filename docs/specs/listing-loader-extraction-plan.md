# FilePane listing-loader extraction plan

Status: DRAFT (pre-review). Worktree: `.claude/worktrees/extract-listing-loader`, branch `extract-listing-loader` off
local `main` (978e0ffb).

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

The cluster is a self-contained lifecycle machine. Its only inbound edges are:

- The `FilePaneAPI` methods that wrap it: `loadDirectory` (internal), `navigateToPath`, `navigateToParent`,
  `handleCancelLoading`, `whenLoadSettles`, `isLoading`, `getListingId`, `getSwapState`, `adoptListing`. External callers
  (`swap-panes.ts`, `edge-flow-handlers.ts`, `dialog-state`, `file-operation-commands`, `pane-mcp-sync`, `rename-flow`,
  `listing-diff-sync`, `navigate.ts`) reach it ONLY through these `paneRef.*` methods. Keeping `FilePane`'s exports as
  thin delegates to the factory means zero change at any call site outside `FilePane.svelte`.
- Three `$effect`s / `onMount` inside `FilePane` that call `loadDirectory` (retry-on-reachable, the initial-path/MTP-
  connect effect, the onMount initial load). These stay in `FilePane` and call `loader.loadDirectory(...)`.

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
so the older load's captured `thisGeneration` no longer matches and ALL of its still-registered listeners no-op — even
before their `unlisten*` fires. The `payload.listingId` half is defense-in-depth against a backend event tagged with a
different listing id.

**Extraction rule:** this predicate moves verbatim. We factor it into a named pure helper
`isEventForCurrentLoad(payloadListingId, captured, liveGeneration)` so it can be unit-tested in true red→green isolation,
then call it from every listener. No behavioral change: same two comparisons, same order.

Note there is a SECOND, unrelated drop-foreign policy at the coordinator level (`navigate.ts::commitPathFromListing`,
DETAILS § navigate transaction). That one is out of scope and untouched. This plan concerns only the pane-local
generation guard.

## State the cluster owns / touches

Owned lifecycle state (only the loader writes these):

- `loadGeneration` (plain), `isDestroyed` (plain — also read by the tag-sweep `isStale`).
- `listingId` (`$state`), `loadedPath` (plain), `lastSequence` (plain).
- `loading`, `openingFolder`, `loadingCount`, `finalizingCount`, `totalCount` (`$state`).
- `error`, `friendlyError` (`$state`).
- `volumeRootFromEvent` (`$state`).
- The six `unlisten*` handles (plain).
- `pendingLoadResolve` / `pendingLoadReject` (plain) + `resolvePendingLoad` / `rejectPendingLoad`.

Shared `FilePane` state it must poke (NOT owned — injected):

- `cursorIndex` (write via `setCursorIndex`), `currentPath` (get/set), `cacheGeneration` bump (adoptListing),
  `entryUnderCursor` (cleared to null; refreshed via `fetchEntryUnderCursor`), `syncStatusMap` (cleared),
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
  `error`, `friendlyError`, `listingId`, `volumeRootFromEvent`) and exposes them as getters. `FilePane` reads
  `loader.loading`, `loader.listingId`, `loader.totalCount`, etc. in its markup and deriveds. Most elegant end state
  (these ARE the loader's outputs), but the biggest read-site churn: ~40–60 references across markup, `hasParent`,
  `effectiveTotalCount`, the `mcpSync`/`jump` deps getters, and the alt-view `{#if}` chain must switch to `loader.*`.
  Getters preserve reactivity (proven by `git-browser-sync`'s `gitRepoInfo`).

- **Option B — surgical factory (matches `type-to-jump-controller`).** `FilePane` keeps the `$state` locals; the factory
  takes a getter + setter per field. Minimal read-site churn (markup untouched), but a ~15-setter deps object and the
  lifecycle state stays visually in `FilePane` even though only the loader writes it.

**Recommendation: Option A.** It's the ideal end state (David's `ideal-over-cheap`), matches the dominant idiom, and the
churn is mechanical and fully covered by the type-aware lint lane + E2E. The reviewer should confirm, or push to B if the
churn is judged too risky for a behavior-preserving pass. Whichever we pick, `getSwapState`/`adoptListing` live with the
state they read/write (so they move into the factory under A; they need `cacheGeneration` bump + `setCursorIndex` +
selection injected).

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
  `isEventForCurrentLoad(...)`. `cleanup()` fires the six `unlisten*` and sets `isDestroyed` (called from `FilePane`'s
  `onDestroy`, which keeps its other teardown).
- `FilePane.svelte`: replace the inline cluster with `const loader = createListingLoader({...})` and thin export
  delegates (`navigateToPath`, `navigateToParent`, `handleCancelLoading`, `whenLoadSettles`, `isLoading`,
  `getListingId`, `getSwapState`, `adoptListing`) that call `loader.*`. Byte-identical signatures.
- Docs: update `pane/CLAUDE.md` module map (add `listing-loader`) and `pane/DETAILS.md` (a "listing loader / generation
  guard" subsection; move/point the token-model description here as the single source). Keep `CLAUDE.md` under 600 words.
- Checks: `pnpm check eslint-typecheck-ts svelte -q` + scoped vitest.

### M3 — Factory integration tests (foreign-listing dropped, proven red by mutation)

- `pane/listing-loader.svelte.test.ts` (harness like `git-browser-sync.svelte.test.ts` / `smb-view-state.svelte.test.ts`
  — `$effect.root` + mock deps; mock the `onListing*` subscribers to capture callbacks and return unlisten spies; mock
  `listDirectoryStart` to resolve).
- Pinned behaviors:
  - **Foreign complete dropped:** start load A (capture its complete cb + listingId), start load B (supersedes), fire
    A's complete → `totalCount`/cursor unchanged, `onPathChange` NOT called with A's path; fire B's complete → accepted.
  - Same for A's `error`, `cancelled`, `progress`, `opening`, `read-complete` callbacks after B starts → all no-op.
  - **Post-await supersession:** if `loadGeneration` advances during the `await listDirectoryStart`, the abandoned
    listing is cancelled and no state is committed.
  - **pendingLoad ordering:** `navigateToPath` rejects the prior pending load, resolves on complete; `whenLoadSettles`
    chains onto the existing resolver without disturbing a waiting `navigateToPath`.
  - **reset semantics:** `resetLoadingState(msg)` rejects pending with the message; cancel path rejects with
    `'Loading cancelled'`; `preserveTotalCount` respected.
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

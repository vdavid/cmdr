# Navigation transaction – Phase 3 execution plan

Just-in-time execution plan for Phase 3 of the [explorer architecture refactor](explorer-architecture-plan.md). Read
that master spec first (§ Target architecture 3 "Transactional navigation", § Invariants register P4 / A4 / A5 /
PR1–PR5, § Landmine register L1 / L2 / L5 / L6 / L7 / L8 / L12, § Open questions Q3, § Verification strategy). This is
the hardest phase: it dissolves the four-function coordinator-level navigation braid into one `navigate(intent)`
transaction, retires both ad-hoc staleness counters, and collapses the scattered persistence sites into a single
subscriber – all behavior-preserving.

## Loud rules (read before touching anything)

- **P4 – navigation stays AS optimistic as today, PER ARM (pinned by the M1 regression tests – reality is
  arm-specific).** The volume-switch arm commits volumeId + path + history synchronously before any listing (truly
  optimistic). The in-place path-nav arm commits ONLY at `listing-complete` (`applyPathChange` via `onPathChange`) – it
  is NOT optimistic today, and `navigate()` must NOT "upgrade" it to an immediate commit (that would change when the
  path/breadcrumb updates relative to the listing – a PR3 violation). The master's blanket "state commits immediately"
  is imprecise; preserve the split the M1 scenario-8 tests pin. What P4 forbids in both arms: a NEW synchronous
  validate-then-commit gate ("resolve the real path, THEN set state") – the spinner must show immediately. The only
  synchronous work before each arm's existing commit point is the capability/refusal checks that today are already
  synchronous (the `network` refusal, the MTP refusal). Cross-volume snapshot resolution (`resolvePathVolume`) stays
  async-then-`handleVolumeChange`, exactly as today.
- **A4 – the phase isn't done while the old entries or the counters live.** `handleVolumeChange`, `handlePathChange`,
  `applyPathChange`, the coordinator-level `navigateToPath`,
  `handleNavigationAction`/`updatePaneAfterHistoryNavigation`, `applyVolumePathCorrection`, AND both
  `volumeChangeGeneration` (DPE:245) and `quickLookFollowGeneration` (DPE:1197) are DELETED by phase end, not wrapped.
  If two writers exist, the refactor made things worse. The `applyPathChange` string-prefix forensics (DPE:447–491, the
  FE twin of the banned `error-string-match`) die with them, replaced by one token compare.
- **The FilePane primitives STAY pane-owned.** `FilePane.navigateToPath` (FilePane:1522), `FilePane.navigateToParent`
  (1078), `loadDirectory` (1274), `handleListingComplete` (1458), the `loadGeneration` counter, the listing-event
  listeners, `handleNavigate` (1664), `switchVolumeForRealPath` (1715) – all stay. `navigate()` sits ON TOP of them; it
  decides intent + state + persistence, then calls the FilePane primitive to drive the actual listing. Listing mechanics
  do not move (master scoping note). The `onPathChange` / `onVolumeChange` / `onCancelLoading` / `onMtpFatalError` /
  `onRetryUnreachable` / `onOpenHome` callbacks from FilePane → DPE stay the inbound channel; what changes is that their
  DPE-side handlers become thin shims over `navigate()` (or `navigate()` internals), not parallel braids.
- **A5 – persistence fires from exactly one module.** After this phase there's one debounced+diffed subscriber that
  watches the store and writes `app-status` + per-pane tabs. The 24 `saveAppStatus` + 24 `saveTabsForPaneSide` + 4
  `saveLastUsedPathForVolume` calls in DPE collapse into it. See § Milestone 4 for the precise absorb/keep split – not
  everything collapses (settings saves, the MCP backend mirror, and tab-CRUD-in-`tab-operations` are NOT pane-state
  persistence).
- **L12 / refusal strings are CONTRACT.** `navigate()` returns a typed `NavigateResult` union; the refusal `message`
  holds the EXACT current strings the MCP adapter forwards verbatim as the `mcp-response` error. A test pins the texts
  byte-for-byte. See § Milestone 2.
- **L5 – `computeHasParent` and `isCrossVolumeNavigation` stay coupled.** `navigate()`'s snapshot branch routes through
  the same `isCrossVolumeNavigation` gate FilePane uses; don't fork the snapshot-exit logic. `has-parent.ts` is
  untouched (it's FilePane-local), but the cross-volume routing `navigate()` performs MUST match what
  `FilePane.handleNavigate` does, or the pane poisons (`volumeId === 'search-results'` + real path).
- **Per-keystroke stays off `navigate()`.** Arrow/Page/Home/End navigation is `sendKeyToFocusedPane` → FilePane, NOT
  `navigate()`. `navigate()` is for pane-destination changes (volume/path/history/snapshot), not cursor motion (P2/P3).

## Open question 3 – resolved (FE-side request map keyed by listing id)

**Decision: the transaction token is an FE-side value, NOT a Rust event-payload change. No IPC contract change, no
bindings regen.** Evidence from the actual listing-event flow:

- `loadDirectory` (FilePane:1274) mints a fresh `listingId = crypto.randomUUID()` (FilePane:1346) per call and bumps a
  per-pane `loadGeneration` (FilePane:1296). Every listing event (`listing-complete`, `listing-error`,
  `listing-cancelled`, `-opening`, `-progress`, `-read-complete`) already carries `listingId` in its payload
  (`ListingCompleteEvent` etc., `types.ts:113`), and every listener gates on
  `event.payload.listingId === newListingId && thisGeneration === loadGeneration` (FilePane:1376). So the **FilePane
  already disambiguates concurrent loads on one pane by listingId today** – two loads get distinct ids, the older
  generation's listeners are torn down (`unlistenComplete?.()`, FilePane:1315), and a stale completion is dropped at the
  source.
- The coordinator-level staleness the token replaces is COARSER: `applyPathChange`'s volume-prefix forensics
  (DPE:456–477) and `volumeChangeGeneration` (DPE:675–688) guard against a stale `onPathChange` landing on a pane whose
  volume flipped, and against a stale background `determineNavigationPath` correction. Both operate at human frequency
  (a volume switch is a user action), well within what an FE-side `Map<pane, txToken>` can model.

**Mechanism:** `navigate()` mints a monotonic `txToken` per call, stores it as the pane's "current transaction" in the
store (or a `navigate()`-module-local `Map<'left'|'right', number>`). The state commit, the `onPathChange` re-entry, and
the background correction each capture their token and bail on `token !== current[pane]`. This subsumes ALL THREE
mechanisms (the prefix forensics, `volumeChangeGeneration`, `quickLookFollowGeneration`) into one compare. The
FilePane's own `loadGeneration` + `listingId` gate stays – it's the pane-local listing mechanism (P3), a different
layer; `navigate`'s token is the coordinator-level intent layer.

**Tripwire that would force the event-payload alternative:** if a single pane can have two _coordinator-accepted_
navigations in flight that the FE can't distinguish by their `navigate()` call order alone – i.e. if a stale
`onPathChange` could carry a token that the FE map has already advanced past but the listing genuinely belongs to the
_newer_ transaction. With the current design (one active transaction per pane, newer always wins, FilePane tears down
old listeners), this can't happen. If a future feature introduces _parallel_ same-pane loads (e.g. a background prefetch
that shares the pane's token namespace), the FE map becomes ambiguous and the token must travel WITH the listing event
(`listingId → txToken` correlation in the Rust payload, bindings regen). Until then, FE-side. **Decide this together
with the `NavigateResult` shape (same milestone, M2) – David reviews both as the seam.**

## navigate(intent) + NavigateResult – seam shapes (David reviews these personally)

The concrete shapes the implementing agent builds in M2. Marked as the seam-defining commit per master § Verification
("David personally reviews … the `navigate()` intent type").

```ts
type NavigateSource = 'user' | 'mcp' | 'history' | 'correction' | 'cancel' | 'fallback' | 'mirror'

type NavigateIntent = {
  pane: 'left' | 'right'
  to:
    | { volumeId?: string; path: string } // volume change (volumeId set) OR in-place path nav (volumeId omitted ⇒ same volume)
    | { history: 'back' | 'forward' | 'parent' }
    | { snapshot: string } // search-results snapshot id; routes through the volume-change machinery
  source: NavigateSource
  selectName?: string // land cursor on this entry after the listing settles (the FilePane selectName channel)
}

type NavigateRefusal = {
  kind: 'on-network-volume' | 'mtp-unconnected' | 'pane-unavailable' | 'no-volume-resolved'
  message: string // EXACT current refusal string – contract, forwarded as mcp-response error (L12)
}

type NavigateResult =
  | { status: 'started'; settled: Promise<void> } // settled resolves when the listing completes / rejects on listing error
  | { status: 'refused'; reason: NavigateRefusal }
```

Notes binding the shape to reality:

- **`{ volumeId?, path }`** unifies today's `handlePathChange` (in-place, same volume → `volumeId` omitted) and
  `handleVolumeChange` (volume switch → `volumeId` present). The pinned-tab fork (L7) lives INSIDE `navigate()`,
  branching on `getActiveTab(mgr).pinned` exactly as DPE:413 / DPE:618 do today – the two near-identical new-tab
  branches unify here, with tests, and ONLY here.
- **`{ snapshot: id }`** is `openSearchSnapshotInPane`'s entry (DPE:600): builds `search-results://<id>` and routes
  through the volume-change path so `pushHistoryEntry` increments the snapshot refcount (the snapshot-store
  integration).
- **`{ history }`** is `handleNavigationAction` (DPE:1047): `parent` delegates to `FilePane.navigateToParent`; `back` /
  `forward` walk the history stack and commit via the same single-commit path. `updatePaneAfterHistoryNavigation`
  (DPE:1023) folds in.
- **`settled: Promise<void>`** is the typed replacement for today's `Promise<void>` arm of `navigateToPath`'s
  `string | Promise<void>`. It's what `navigate-and-select.ts`, `go-to-path`, `go-to-latest`, and `handleSearchNavigate`
  await before `moveCursor`. **`status: 'refused'`** replaces the `string` arm; callers branch on
  `result.status === 'refused'` instead of `typeof result === 'string'`.
- **The exact refusal strings to pin** (from DPE:1556–1599 + `validateMtpNavigation`):
  - `on-network-volume`:
    `` `Pane is on the ${volumeName ?? volumeId} volume. Use select_volume to switch to a local volume first.` ``
    (DPE:1561)
  - `pane-unavailable`: `'Pane not available'` (DPE:1592)
  - `mtp-unconnected`: whatever `paneCommands.validateMtpNavigation` returns (DPE:1588) – find and pin it verbatim.
  - `no-volume-resolved`: the cross-volume `resolvePathVolume` returned no volume – today this LOGS and returns
    `undefined` from the async arm (DPE:1578), so the MCP round-trip sees `ok: true` with no nav. Preserve that exact
    behavior (it's not a `string` refusal today); represent it as a `started` result whose `settled` resolves to a
    no-op, NOT a refusal, unless the implementing agent confirms the current MCP ack is `ok: false`. **Pin current
    behavior in the M1 regression test BEFORE deciding** – this is a corruption-adjacent edge the master didn't
    enumerate.

## Bus interplay – does `nav.toPath` become a registry command now?

Phase 2 deferred this to Phase 3 (master § Phase map Phase 2 sequencing note; the Phase-2 plan M4 explicitly parks
`mcp-nav-to-path` off the bus). **Decision: the adapter calls `navigate()` DIRECTLY for `mcp-nav-to-path`; `nav.toPath`
does NOT become a bus command.**

Rationale, grounded in the dispatch contract:

- `handleCommandExecute` returns `Promise<void>` (command-dispatch.ts:182). The bus is fire-and-forget by design; a
  command handler can't surface a `NavigateResult` to the adapter. The round-trip pattern (`mcp-open-under-cursor`,
  `mcp-move-cursor`) works only because the adapter awaits the dispatch promise and infers `ok`/`error` from
  resolve-vs-reject – there's no _value_ channel. `mcp-nav-to-path` needs the refusal _value_ (the exact string)
  forwarded as `mcp-response.error`, which a `void`-returning dispatch can't carry.
- The Phase-2 round-trip events keep their adapter-owned `requestId` + `emit('mcp-response', …)` (master Open Q2 –
  `mcp-response` stays in the adapter). `mcp-nav-to-path` is the same shape: the adapter owns transport. So the adapter
  calls `const result = getExplorer()?.navigate({ pane, to: { path }, source: 'mcp' })`, then forwards
  `result.status === 'refused' ? { ok: false, error: result.reason.message }` / awaits `result.settled` for
  `{ ok: true }` / `{ ok: false, error }` on reject – byte-identical to today's `typeof result === 'string'` branch
  (mcp-listeners.ts:231–249), just typed.
- Adding a `nav.toPath` registry entry whose handler returns `void` and then having the adapter ALSO call `navigate()`
  to read the result would be a dead parallel path (PR1/A4 violation). One direct call is cleaner. The `ExplorerAPI`
  surface keeps a `navigate(intent): NavigateResult` method (replacing `navigateToPath`); the adapter and the four
  external write-callers use it directly.

**`nav.back` / `nav.forward` / `nav.parent` STAY bus commands** (Phase 2 wired them). Their dispatch cases
(command-dispatch.ts:415–424) swap `explorerRef?.navigate('back')` for
`explorerRef?.navigate({ pane: focusedPane, to: { history: 'back' }, source: 'user' })` – they route through the bus
(routing) AND now call the new mechanism. The `mcp-key` GoBack/GoForward path (mcp-listeners.ts:179) rides through
unchanged. **Naming caution:** today `ExplorerAPI.navigate(action)` and `ExplorerAPI.navigateToPath(pane, path)` are two
methods; this phase merges them into one `navigate(intent)`. Keep the public method name `navigate` (it already exists,
the bus already calls it) – the intent arg shape is what changes. Rename per AGENTS.md "name internals after the UI":
the user-facing concept is "navigate", so `navigate(intent)` it is.

## Fresh grep (run 2026-06-05, this worktree)

`DualPaneExplorer.svelte` is **2103 lines** (master § Goal cites 3318 pre-Phase-0; Phase-1 plan cited 2140 – it shrank
further). `FilePane.svelte` is **2914**. Line numbers below are indicative; re-grep at each milestone (PR4).

### The coordinator-level braid (retires this phase)

| Function                           | Site (DPE) | Role                                                                               |
| ---------------------------------- | ---------- | ---------------------------------------------------------------------------------- |
| `handlePathChange`                 | 408        | In-place vs pinned-new-tab fork (L7) → `applyPathChange`                           |
| `applyPathChange`                  | 447        | Stale-path volume-prefix forensics (L6), commit, persist, cursor restore           |
| `handleVolumeChange`               | 606        | Volume switch: pinned fork (L7), single commit, focus, `applyVolumePathCorrection` |
| `applyVolumePathCorrection`        | 669        | Background `determineNavigationPath` gated by `volumeChangeGeneration`             |
| `handleNavigationAction`           | 1047       | back/forward/parent                                                                |
| `updatePaneAfterHistoryNavigation` | 1023       | History-walk commit                                                                |
| `navigateToPath` (coordinator)     | 1556       | MCP entry: `network` refusal, cross-volume snapshot resolve, MTP refusal, sentinel |
| `openSearchSnapshotInPane`         | 600        | `{ snapshot }` entry → `handleVolumeChange`                                        |
| `volumeChangeGeneration`           | 245        | Staleness counter #1 (retires)                                                     |
| `quickLookFollowGeneration`        | 1197       | Staleness counter #2 (retires – folds into the token; see § M3)                    |

### Callers of `handleVolumeChange` (all internal, all fold into `navigate()`)

`openSearchSnapshotInPane` (603), `selectVolumeByIndex` (1517/1520/1524), `navigateToPath` cross-volume arm (1581),
`selectVolumeByName` (1642), `mirrorLocalStateToPane` (1707), `mirrorNetworkStateToPane` (1729). The
`copyPathBetweenPanes` mirrors (DPE:1663) are a `source: 'mirror'` use of `navigate()` – they call `handleVolumeChange`

- `setPanePath`/`pushPath` + `navigateToPath` today (DPE:1702–1742); they must keep the no-focus-shift `restoreFocus`
  semantics (L1-adjacent). Migrate them onto `navigate({ source: 'mirror' })` but preserve `restoreFocus`.

### FilePane primitives (STAY)

`navigateToPath` (1522), `navigateToParent` (1078), `loadDirectory` (1274), `handleListingComplete` (1458),
`handleCancelLoading` (1506, the pane-side ESC handler), `handleNavigate` (1664), `switchVolumeForRealPath` (1715),
`handleVolumeChangeFromBreadcrumb` (1750), `loadGeneration`, the six listing-event listeners (1360–1422). The
`onPathChange` push fires from TWO branches – `handleListingComplete` (success, 1483) AND the `listing-error` handler
when the path still exists (1412) – both must keep landing in `navigate()`'s commit-or-token path (navigation/CLAUDE.md
§ "history pushed on both success AND failure").

### External write-callers retiring onto `navigate()`

| Caller                                  | Site          | Today                                                             |
| --------------------------------------- | ------------- | ----------------------------------------------------------------- |
| `mcp-listeners.ts` `mcp-nav-to-path`    | 218–250       | `explorerRef.navigateToPath` + `typeof result === 'string'` (L12) |
| `navigate-and-select.ts`                | 27 / 49       | `navigateToDirInPane` / `navigateToFileInPane` (string sentinel)  |
| `go-to-path.ts`                         | 68 / 72 / 76  | via `navigate-and-select`                                         |
| `go-to-latest.ts`                       | 47 / 86 / 103 | via `navigate-and-select` + a direct `navigateToPath` (103)       |
| `+page.svelte` `handleSearchNavigate`   | 808–819       | `navigateToPath` + `result instanceof Promise` → `moveCursor`     |
| `+page.svelte` `handleOpenSearchInPane` | 829           | `openSearchSnapshotInPane`                                        |

### Edge-flow handlers (M5)

`handleCancelLoading` (DPE:708 – the coordinator one, distinct from FilePane:1506), `handleMtpFatalError` (757),
`handleRetryUnreachable` (770), `handleOpenHome` (799), and `handleVolumeUnmount` (1001 – the `volume-unmounted`
redirect; per-pane, NO history push, a fifth direct-commit writer the master didn't enumerate). Each does its own
commit + persist sequence today; they fold onto the transaction's single-commit + token path while preserving their
exact fallback behavior (incl. the history-push asymmetry – see M5).

### Persistence sites (fresh count, verified 2026-06-05 – master § Caller map's ~25/~24/~6/~12 was approximate)

| Call                                 | Count | Where   | Disposition                                                                                             |
| ------------------------------------ | ----- | ------- | ------------------------------------------------------------------------------------------------------- |
| `saveAppStatus`                      | 24    | DPE     | Absorb into subscriber (it ALREADY debounces 200ms + diffs, see § M4)                                   |
| `saveTabsForPaneSide`                | 24    | DPE     | Absorb (per-pane tab persistence triggered by nav/sort/tab mutations)                                   |
| `saveLastUsedPathForVolume`          | 4     | DPE     | Absorb. The four sites: 439, 481, 1040 (nav commits) + 615 (the volume-change pre-save of the OLD path) |
| `saveTabsForPane`                    | 11    | tab-ops | **KEEP in tab-operations** – tab CRUD, not nav-state; see § M4 scope                                    |
| `updatePaneTabs`/`updateFocusedPane` | 6     | DPE     | **KEEP** – MCP backend mirror (L8), NOT disk persistence                                                |

### Deviations from master spec

1. **`DualPaneExplorer.svelte` is 2103, not 3318.** Phase 0+1 shrank it. Re-grep per milestone.
2. **`saveAppStatus` is ALREADY debounced (200ms) and effectively diffed** (`pendingSave` field-merge +
   `status.x !== undefined` per-field writes, `app-status-store.ts:130–184`). The A5 "single debounced+diffed
   subscriber" is therefore mostly a _consolidation of trigger sites_, not new debounce machinery – the subscriber
   derives the `AppStatus` snapshot from the store and calls the existing `saveAppStatus` once per store change, letting
   its existing debounce coalesce. The win is "one trigger site" (grep-able A5), not "add a debounce." Frame M4
   accordingly.
3. **There are TWO `handleCancelLoading`** – the coordinator one (DPE:708, the ESC fallback logic) and the FilePane one
   (FilePane:1506, which fires `onCancelLoading` up to the coordinator). M5 migrates the coordinator one; the FilePane
   one stays (pane-owned primitive).
4. **`quickLookFollowGeneration` (DPE:1197) is a THIRD staleness counter** the master groups as "stretch." It guards the
   Quick-Look cursor-follow debounce, NOT navigation listings – it's a different concern (cursor-follow IPC dedupe). It
   does NOT obviously fold into the navigation token. **Decision: leave `quickLookFollowGeneration` in place; it is not
   a navigation-staleness mechanism** (it dedupes `quickLookSetPath` IPCs on cursor move, DPE:1219–1238). Folding it
   into the nav token would couple two unrelated debounces. The master's "(stretch) quickLookFollowGeneration" is
   retracted: A4's "all three staleness mechanisms" means the prefix forensics + `volumeChangeGeneration` + the
   background-correction guard (which IS `volumeChangeGeneration`), i.e. the navigation ones. Flag for David in the M3
   seam review.
5. **The cross-volume `no-volume-resolved` arm returns `undefined` (not a refusal string) today** (DPE:1578) – an MCP
   `nav_to_path` to a snapshot-pane real path with no resolvable volume acks `ok: true` with no navigation. Pin this
   exact behavior in M1 before designing `NavigateResult` (see § shape notes). The master's refusal enumeration didn't
   cover it.

## Milestones

Each milestone is atomic (add + migrate + delete old path; PR1). Gates per milestone: `--fast` continuously during work;
full `pnpm check` + `--check desktop-e2e-linux` before the milestone commit. Phase-end (after M5): `--include-slow`
(adds macOS Playwright + `rust-tests-linux`), then watch CI to green before merging to `main`. PR3 (byte-identical
behavior) gets EXTRA scrutiny this phase – this is the riskiest braid; the M1 regression tests are the guard.
Import-cycle rule (master § Verification): `navigate()` lives in a module the store and FilePane can import;
`navigate()` imports the store + FilePane primitives via the `PaneAccess`/`FilePaneAPI` handles it's handed, never
`routes/`. PR5: the whole phase reverts as one merge range – design M2–M5 so reverting the merge commit is clean (M1's
tests survive a revert harmlessly).

### M1 – Regression tests FIRST (pin current behavior, red-green-ready)

**Scope:** new headless tests pinning the corruption scenarios + the refusal strings against the CURRENT braid. NO
production code change. These tests are written so they can be re-pointed at `navigate()` in M2–M5 (red against a broken
implementation, green against the faithful one). The corruption scenarios come from `file-explorer/CLAUDE.md` § Gotchas
and `pane/CLAUDE.md` (master § Verification: "corruption scenarios … become the tests").

**The injection seam – design it explicitly, the milestone agent must NOT discover this gap mid-flight.** The existing
`DualPaneExplorer.test.ts` harness mocks `listen` as a no-op (`DualPaneExplorer.test.ts:62`,
`listen: vi.fn(() => Promise.resolve(() => {}))`), so a mounted braid CANNOT today receive a synthetic
`listing-complete`/`listing-error` into FilePane's listeners – the callbacks are registered but never fired. AND the
coordinator handlers (`applyPathChange`/`handlePathChange`/`handleVolumeChange`) are internal `function` closures, not
exported, so they're unreachable from a test that holds the `ExplorerAPI` handle. **Three of the eight scenarios (1,
2, 8) need a real seam.** The mandated design – pick (a), it's the smallest honest fix:

- **(a) Capture-and-replay `listen` mock (preferred).** Replace the no-op `listen` mock with a helper that records every
  registered callback keyed by event name into a `Map<string, Array<(payload) => void>>`, and exposes a
  `fireListingEvent(eventName, payload)` to invoke them. Put the helper in `pane/integration-test-utils.ts` (the
  existing shared pane-test scaffolding) so M2–M5 reuse it. The test then: mounts `DualPaneExplorer`, drives a
  navigation through the `ExplorerAPI` handle (or a render-prop callback it can reach), then
  `fireListingEvent('listing-complete', { listingId, totalCount, volumeRoot })` with the listingId the pane minted
  (capture it from the recorded `listDirectoryStart` mock call). This is what makes the stale-`onPathChange` and
  optimistic-ordering scenarios expressible at the braid layer.
- **(b) Direct-callback fallback** for scenarios where the braid layer is overkill: `handlePathChange`/`applyPathChange`
  are reachable indirectly because FilePane's `onPathChange` render prop is bound to them; a test that mounts the pane
  and invokes the bound prop hits the coordinator drop-logic without needing a real listing event. Pure helpers
  (`isCrossVolumeNavigation`, `computeHasParent`, `isPathOnVolume`) are already unit-tested directly and need no seam.

**Per-scenario layer** (so the agent knows which lever each scenario pulls):

1. **Stale `onPathChange` after a volume flip** (L6, the central gotcha) — _braid layer via seam (a)_: pane on volume A
   loading `/A/deep`, flip to volume B via the volume-change path, then `fireListingEvent('listing-complete', …)` for
   the stale `/A/deep` listingId → assert `onPathChange('/A/deep')` is dropped (no `pushPath`, no
   `saveLastUsedPathForVolume('B', '/A/deep')`). Both the real-volume `isPathOnVolume` branch AND the `network` /
   `search-results` prefix branches. _Without seam (a) this scenario cannot be written – it's the whole reason the seam
   exists._
2. **Snapshot-pane poisoning** (L5, R4) — _braid layer via seam (a)_, OR the `onVolumeChange` render-prop path (b): pane
   on `search-results://sr-1`, navigate to a real `/Library/x` → `isCrossVolumeNavigation` MUST route through
   volume-change so the pane ends `volumeId !== 'search-results'`. Assert no "Dropping stale onPathChange on
   search-results pane" warning and `SearchResultsView` not stuck on "no longer available". The
   `isCrossVolumeNavigation` _trigger_ is already unit-tested (`snapshot-pane-navigation.test.ts`); this scenario pins
   the _braid integration_ (resolve → `handleVolumeChange` → commit), which needs the mount + a faked
   `resolvePathVolume`.
3. **Pinned-tab fork** (L7) — _braid layer, no listing event needed_: pinned active tab, path change OR volume change →
   a NEW unpinned tab opens with the target, active tab unchanged; at `MAX_TABS_PER_PANE` → in-place + "Tab limit
   reached" toast. Reachable by driving the `onPathChange`/`onVolumeChange` render props (b). Pin both branches (DPE:413
   / DPE:618).
4. **Unreachable fallback** (`handleRetryUnreachable` DPE:770 / `handleOpenHome` DPE:799) — _braid layer via the
   `onRetryUnreachable`/`onOpenHome` render props (b)_: retry resolves volume via faked `resolvePathVolume`, clears
   `tab.unreachable`, commits; open-home goes to `~` on default volume. Pin the commit shape (tab state + persisted
   snapshot).
5. **Cancel-during-load** (`handleCancelLoading` DPE:708) — _braid layer via the `onCancelLoading` render prop (b)_: the
   three branches – network entry restore, history-back when the cancelled path completed, walk-up-to-parent when it
   didn't (needs a faked `resolveValidPath`), and the `navigateToPath(entry.path)` re-drive.
6. **MTP fatal fallback** (`handleMtpFatalError` DPE:757) — _braid layer via the `onMtpFatalError` render prop (b)_:
   commits default volume + path + history (needs a faked `getDefaultVolumeId`).
7. **Refusal strings** (L12) — _`ExplorerAPI` handle, no seam needed_: `navigateToPath` on a `network`-volume pane
   returns the exact `` `Pane is on the … volume. Use select_volume …` `` string; pane-unavailable returns
   `'Pane not available'`; MTP returns BOTH its exact strings (the `mtp://`-mismatch
   `` `Pane is not on this MTP volume — call select_volume first.` `` AND the on-MTP-volume
   `` `Pane is on the … MTP volume. …` ``, `pane-commands.ts::validateMtpNavigation` – pin both, note the em dash in the
   first); the `no-volume-resolved` cross-volume arm returns `undefined` (deviation 5). These become the
   `NavigateResult.reason.message` assertions in M2. Called directly through the handle, returns synchronously.
8. **Optimistic-commit ordering** (P4) — _braid layer via seam (a)_: drive a navigation, assert `getPanePath` reflects
   the target synchronously after the call returns, BEFORE `fireListingEvent('listing-complete', …)`. The seam is what
   lets the test prove the commit precedes the listing settle (no listing event = no commit, in a broken
   validate-then-commit rewrite). The guard against an accidental synchronous-gate rewrite in M2.

**Test plan:** new file `pane/navigation-transaction.test.ts` (or split per concern). **First task of M1: build the
capture-and-replay `listen` helper in `integration-test-utils.ts`** – scenarios 1, 2, 8 block on it; write one tiny
smoke test for the helper itself before the scenarios. Scenarios 3–6 use the render-prop (b) path; scenario 7 uses the
handle directly. These land in M1 and stay green through M2–M5 (re-pointed at `navigate()` internals as the handlers
fold in). New file → covered by the 70% `src/lib/**` gate.

**DONE:** all eight scenario groups green against the CURRENT braid; the refusal strings pinned byte-for-byte;
`--fast` + full suite + `desktop-e2e-linux` green; zero production change.

### M2 – `navigate(intent)` core + transaction token + `NavigateResult` (the seam)

**Scope:** new `pane/navigate.ts` (or `pane/navigation-transaction.ts`) exporting `navigate(intent, deps)` + the
`NavigateIntent` / `NavigateResult` / `NavigateRefusal` types. Built + tested in isolation against a fake resolver and a
fake `PaneAccess`/`FilePaneAPI` – NOT yet wired into DPE (the old braid still runs; M3 swaps callers). This is the
seam-defining commit – **flag for David's review** (the intent type + the token mechanism + the Q3 resolution).

**Intentions:**

- `navigate(intent, deps)` where `deps` carries the store handle (read pane volumeId/path/history/tab-mgr + the named
  mutators), the `FilePaneAPI` getter (to call `navigateToPath`/`navigateToParent`/`setNetworkHost`), a `resolveVolume`
  fn (`resolvePathVolume`, injectable for tests), and the persistence-trigger (in M2 a no-op spy; M4 wires the
  subscriber). Mirror the Phase-0 factory pattern (`createPaneCommands(access, dialogs)`).
- **Single commit point:** volumeId + path + history written together (today scattered across `setPaneVolumeId` /
  `setPanePath` / `setPaneHistory` per branch). One internal `commit(pane, { volumeId, path, historyEntry })` that the
  pinned-tab fork, the in-place path, the volume switch, the history walk, and the edge flows all call.
- **Transaction token:** monotonic per `navigate()` call, stored per-pane. The commit captures it; the background
  `determineNavigationPath` correction (folding `applyVolumePathCorrection`, DPE:669) bails on stale; the `onPathChange`
  re-entry path bails on stale (replacing the prefix forensics – the _policy_ "drop foreign listings" is identical, the
  _mechanism_ is the token, L6). Pin with a fake-resolver test: a slow correction whose token was superseded is dropped.
- **`swapPanes` token invariant (L4):** the per-pane token map keyed by side ('left'/'right') is safe across a pane swap
  ONLY because `canSwapPanes()` (DPE:1287) refuses while either pane `isLoading()` – so no live transaction can exist at
  swap time, and `swapPanes` never has to migrate or invalidate a token between sides. `swapPanes` stays zero-IPC (L4)
  and touches no token. The token model relies on that `isLoading()` gate; don't relax it. (If a future change allowed
  swapping mid-load, an in-flight correction's captured token would land on the wrong pane after the swap – the gate is
  what makes side-keyed tokens correct.)
- **`NavigateResult`:** synchronous refusals return `{ status: 'refused', reason }` with the exact strings (M1 pins
  them). Success returns `{ status: 'started', settled }`. **Define `settled`'s resolve point PER INTENT ARM – today's
  semantics differ by arm and callers depend on the difference (PR3, caller-observable timing):**
  - **In-place path nav + volume switch on a real volume** (the `paneRef.navigateToPath(path)` arm, DPE:1599): `settled`
    is the FilePane `navigateToPath` promise – it resolves on `listing-complete` via `resolvePendingLoad`
    (FilePane:1492). This is the only arm where "resolves when the listing completes" is literally true.
  - **Cross-volume snapshot arm** (DPE:1572): today the async IIFE resolves when fire-and-forget `handleVolumeChange`
    RETURNS – BEFORE the listing loads (DPE:1573–1585). **Preserve this: `settled` here resolves when the volume-change
    commit is done, NOT when the new listing completes.** Do NOT "fix" it to await the listing – that changes the timing
    `handleSearchNavigate` and `navigate-and-select` observe, and they already bridge the gap themselves (see next
    bullet). Changing it is a PR3 violation.
  - **Network / device-only-MTP-view branches** (no `loadDirectory` fires): `settled` resolves immediately (a resolved
    no-op), matching today's behavior where those branches return without a load.
  - **History (`back`/`forward`)** and **edge flows** (`{ source: 'cancel'|'fallback' }`): match whichever underlying
    primitive they drive (history-walk that re-drives via `navigateToParent`/`navigateToPath` resolves on its listing; a
    state-restore-only branch resolves immediately).
- **Preserve the `whenLoadSettles` cursor-after-nav bridge (L2-adjacent).** Because the cross-volume `settled` resolves
  before the listing loads, callers that move the cursor after navigating (`handleSearchNavigate` → `moveCursor`,
  `navigate-and-select::navigateToFileInPane` → `moveCursor`) rely on `moveCursor`'s internal
  `await paneRef.whenLoadSettles()` (DPE:1616) to avoid racing an empty cache. M3 must NOT collapse that await away when
  it rewrites these callers onto `await result.settled; moveCursor(...)` – the `whenLoadSettles` inside `moveCursor` is
  the real gate, `result.settled` is the navigation-started gate. Keep both.
- The cross-volume snapshot arm and the no-volume-resolved arm preserve M1's pinned behavior.
- **L7 unification:** the two near-identical pinned-new-tab branches (DPE:413, DPE:618) become ONE branch in
  `navigate()`, parameterized by `{ volumeId?, path }`. This is the only place L7 is allowed to unify (master L7: "only
  there, in the nav phase, with tests").
- **P4:** commit is synchronous and immediate; the load + correction are background. No `await` before the commit except
  the genuinely-async cross-volume `resolvePathVolume` (which today is already async-then-`handleVolumeChange`,
  DPE:1573).

**Test plan (red-green TDD against the fake resolver):** the cross-volume snapshot branch, the pinned-tab fork, the
token drop (stale correction), the refusal union (each `kind` → exact `message`), the optimistic-commit ordering. These
are the M1 scenarios re-expressed against `navigate()` directly (headless, no mount needed once the deps are
injectable). New file → 70% gate covers it.

**DONE:** `navigate()` + types land with their tests; fake-resolver tests green incl. token-drop + refusal strings; the
old braid still runs (dead-code-free because `navigate()` is consumed in M3 – land M2+M3 together if `knip` can't
tolerate the gap, the Phase-1 M1/M2 precedent); `--fast` + full suite + `desktop-e2e-linux` green; **David reviews the
seam.**

### M3 – Migrate callers; DELETE the old braid + both nav staleness counters (A4)

**Scope:** swap every coordinator handler + external write-caller onto `navigate()`, then DELETE `handlePathChange`,
`applyPathChange`, `handleVolumeChange`, `applyVolumePathCorrection`, `handleNavigationAction`,
`updatePaneAfterHistoryNavigation`, the coordinator `navigateToPath`, `volumeChangeGeneration` (DPE:245), and the
`applyPathChange` prefix forensics. The FilePane→DPE callbacks (`onPathChange`/`onVolumeChange`/etc., DPE:1954–1973)
re-point at `navigate()` internals (a thin `onPathChange` shim that calls the commit-or-drop path with
`source: 'fallback'`/`'user'`).

**Migration checklist (PR4 – this IS the checklist, not the master's table):**

- DPE render callbacks (1954–1973): `onPathChange` → `navigate({ to: { path }, source })` token-gated drop;
  `onVolumeChange` → `navigate({ to: { volumeId, path } })`;
  `onCancelLoading`/`onMtpFatalError`/`onRetryUnreachable`/`onOpenHome` → M5.
- Bus nav cases (command-dispatch.ts:415–424): `navigate('back'|'forward'|'parent')` → `navigate({ to: { history } })`.
- `nav.open` (command-dispatch.ts:411) stays `sendKeyToFocusedPane('Enter')` – it's a keystroke forward, not a
  destination change. Don't route it through `navigate()`.
- `mcp-nav-to-path` (mcp-listeners.ts:218–250): adapter calls `navigate()` directly, forwards `NavigateResult` →
  `mcp-response` (see § Bus interplay). The `requestId` round-trip + `emit` stay adapter-local.
- `navigate-and-select.ts` (27/49): branch on `result.status === 'refused'` instead of `typeof === 'string'`;
  `await result.settled` before `moveCursor`. Update the module doc-comment (it narrates the `string | Promise<void>`
  contract).
- `go-to-path.ts`, `go-to-latest.ts`: via `navigate-and-select` (free); `go-to-latest.ts:103`'s direct `navigateToPath`
  → `navigate()`.
- `+page.svelte` `handleSearchNavigate` (808): `navigate({ to: { path }, source: 'user' })`, `await settled` →
  `moveCursor`. `handleOpenSearchInPane` (829): `navigate({ to: { snapshot }, source: 'user' })`.
- `selectVolumeByIndex`/`selectVolumeByName` (DPE:1503/1639): their `handleVolumeChange` calls →
  `navigate({ to: { volumeId, path } })`. **L1 caution:** `selectVolumeByIndex` deliberately does NOT re-anchor focus
  (DPE:1527, mtp.spec.ts:414); `navigate()` must NOT add a `containerElement.focus()` for the `'user'`/volume-select
  source.
- `copyPathBetweenPanes` mirrors (DPE:1663–1742): `navigate({ source: 'mirror' })`, preserve `restoreFocus` (no focus
  shift).
- `ExplorerAPI` (explorer-api.ts): `navigateToPath: (pane, path) => string | Promise<void>` and
  `navigate: (action) => void` collapse into `navigate: (intent) => NavigateResult`. Update all `.d.ts`-style consumers.

**Parent-nav re-entrancy – the token must tolerate self-re-entry.** `navigate({ to: { history: 'parent' } })` delegates
to `FilePane.navigateToParent` (FilePane:1078), whose `loadDirectory` completion fires `onPathChange(parentPath)`
(FilePane:1483) → which M3 routes back into `navigate()` (or its commit-or-token path) as a SECOND, source-`'fallback'`
re-entry of the SAME logical navigation. The token design must NOT let the parent transaction invalidate its own
follow-up commit: the `onPathChange` re-entry for a path the active transaction is ALREADY navigating to must be treated
as the expected completion (commit/persist the path), not as a stale foreign listing to drop. Concretely – the token
compare drops listings whose token is OLDER than `current[pane]`; a self-re-entry carries the SAME token (no new
`navigate()` call minted one), so it passes the compare and commits. State this explicitly in the M2 token contract so
the implementing agent doesn't bump the token on the `onPathChange` re-entry (which would make the parent-nav's own
completion look stale and drop the path push – Back-depth regression).

**Second same-token self-re-entry case: `navigateToFallback` (FilePane:1261, the deleted-folder walk-up).** When a
listing fails because the path is gone, FilePane resolves the nearest valid parent and calls `navigateToFallback`, which
re-enters the coordinator either via `onPathChange` (on the subsequent successful load) or via
`onVolumeChange('root', '/', target)` when the volume root was unreachable (DPE-side `handleVolumeChange`,
FilePane:1264). Both re-entries belong to the SAME logical navigation the user/transaction started – same class as the
parent-nav rule: they carry the SAME transaction token (no new `navigate()` call minted one), so the token must let them
through, not drop them as stale. The `onVolumeChange('root', …)` arm is a token-preserving volume switch within the
active transaction, not a fresh one. Pin this with the M1 cancel/walk-up scenario (#5's walk-up arm) so a token bump on
fallback re-entry can't silently regress the deleted-folder recovery.

**Landmines:** L1 (focus re-anchor: `moveCursor` refocuses; volume-select + path-nav do NOT – mtp.spec.ts:414). L2
(`whenLoadSettles` lives in `moveCursor`, untouched). L5 (snapshot coupling – `navigate()`'s `{ snapshot }` +
cross-volume arm match `FilePane.handleNavigate`). L6 (token replaces prefix forensics, policy identical). L7 (pinned
fork unified in M2). L8 (the MCP tab mirror `syncTabsToBackend` + `updatePaneTabs`/`updateFocusedPane` are NOT touched –
they're the backend mirror, not persistence; M4 leaves them alone).

**Test plan:** the M1 regression suite re-points at `navigate()` and stays green (the whole point). The MCP E2E
(`mtp.spec.ts` incl. the L1 guard at :414, the `mcp-nav-to-path` round-trip + refusal-forward specs) stays green – it's
the L12 contract. The cross-volume R4 E2E + `snapshot-pane-navigation.test.ts` stay green. NO coverage backstop on the
routes-side files (`+page.svelte`, `mcp-listeners.ts`, `command-dispatch.ts`, `explorer-api.ts`,
`navigate-and-select.ts`, `go-to-*`) – the A4 "no parallel paths" review is the only guard: re-grep
`handleVolumeChange|applyPathChange| handlePathChange|volumeChangeGeneration` at milestone end → ZERO hits. **Also grep
for residual direct commit writers OUTSIDE the `navigate()` implementation** – `setPaneVolumeId(` / `setPanePath(` /
`setPaneHistory(` call sites: after M3 (and M5 for the edge flows) the ONLY caller of these per-pane mutators is
`navigate()`'s internal `commit`. Any survivor in DPE (e.g. an unmigrated `handleVolumeUnmount`, or a missed mirror
helper) is an A4-violating parallel writer the narrower grep misses – this is the gate that catches the
master-unenumerated fifth writer. Re-grep `typeof result === 'string'` → zero.

**DONE:** old braid + `volumeChangeGeneration` + prefix forensics DELETED; all callers on `navigate()`; refusal-forward
byte-identical (L12); focus timing identical (L1); `--fast` + full suite + `desktop-e2e-linux` + MCP E2E green.

### M4 – Single persistence subscriber (A5)

**Scope:** one debounced+diffed subscriber (a `$effect` in DPE reacting to the store, OR a store-colocated subscriber
module – decide by import-cycle topology) that derives the `AppStatus` snapshot + per-pane tab state from the store and
fires persistence. Absorb the 24 `saveAppStatus` + 24 `saveTabsForPaneSide` + 4 `saveLastUsedPathForVolume` _trigger
sites_ now scattered through `navigate()` (post-M3) and the surviving DPE handlers.

**Intentions:**

- **What the subscriber covers (absorbs):** `saveAppStatus` (left/right path + volumeId + viewMode + sortBy,
  focusedPane, leftPaneWidthPercent – the exact `AppStatus` shape, app-status-store.ts:100–123) and
  `saveTabsForPaneSide` (per-pane persisted tabs) triggered by _navigation/pane-state_ changes. The subscriber reads the
  store's per-pane active-tab state + focus + layout, diffs against the last-persisted snapshot, and calls the existing
  (already-debounced) persistence fns. Because `saveAppStatus` ALREADY debounces+merges (deviation 2), the subscriber's
  job is "one trigger site," not "new debounce" – it calls `saveAppStatus(snapshot)` on store change and lets the 200ms
  debounce coalesce. Keep the per-pane slicing (P1): two subscriptions or a per-pane diff, never one `$effect` reading
  both panes' tab arrays.
- **`saveLastUsedPathForVolume`:** this writes a `volumeId → path` map keyed off navigation events
  (DPE:439/481/615/1040). It's navigation-state persistence → absorb into the subscriber's per-pane path-change reaction
  (when a pane's `{volumeId, path}` commits, record the last-used-path). The pre-save of the OLD path on volume change
  (DPE:615) is a navigate()-internal concern – keep it inside `navigate()`'s commit (it needs the old value before the
  swap), OR have the subscriber diff the previous snapshot. Decide in M4; the constraint is "exactly one module fires
  it" (A5).
- **What STAYS (does NOT collapse into the subscriber) – enumerate precisely:**
  - `saveTabsForPane` in `tab-operations.ts` (11 sites): tab CRUD (open/close/reorder/pin/reopen). These persist tab
    STRUCTURE on tab-bar actions, not navigation. They stay in `tab-operations` – the A2/A1-vs-tab-manager scope
    boundary (Phase-1) keeps tab mechanics in `tab-operations`. **A5 nuance:** A5 says "navigation/pane state
    persistence fires from one module." Tab-CRUD persistence is a SEPARATE concern (tab structure, not pane navigation).
    Document the split: the subscriber owns _active-tab nav-state + focus + layout_; `tab-operations` owns _tab-set
    structure_. Both are "persistence" but different surfaces; A5's "one module" is per-surface. **Flag for David** –
    this is an interpretation of A5 the master didn't spell out.
  - `updatePaneTabs` / `updateFocusedPane` / `syncTabsToBackend` (DPE:189/204/699/1281): the MCP BACKEND MIRROR (L8),
    not disk persistence. Untouched – different debounce (100ms), different target (Rust state store for MCP).
  - `saveSettings` (toggleHiddenFiles, DPE:1371): a SETTINGS save (`showHiddenFiles` is a setting), not pane-state.
    Stays. `showHiddenFiles` is store-owned (Phase 1) but its persistence is the settings store, not `app-status`.
  - View-mode `saveAppStatus` from `setViewMode`/`setViewModeFromMenu` (DPE:1382/1398) and sort `saveAppStatus`
    (DPE:538/1476): these ARE `AppStatus` fields (viewMode, sortBy) → absorb into the subscriber (they're pane-chrome
    state the store owns post-Phase-1). The subscriber reacts to the active tab's viewMode/sortBy change.
  - `leftPaneWidthPercent` saves (DPE:1093/1098): layout, store-owned → absorb (debounced; note the resize-end-only save
    semantics today, DPE:1092 – the subscriber must not over-persist on every drag tick; preserve the drag-end
    coalescing, which the 200ms debounce already gives).

**Landmines:** L8 (MCP mirror untouched). The `saveAppStatus` debounce semantics (200ms) must not change – reuse, don't
replace. Don't persist on every `leftPaneWidthPercent` drag frame (resize uses `setLeftPaneWidthPercent` per frame but
`saveAppStatus` only at drag-end today; the debounce + a drag-end flush preserves this).

**Test plan:** a subscriber unit test: a store mutation → exactly one `saveAppStatus` call with the diffed snapshot;
no-op when nothing changed (diff); per-pane isolation (a left-pane change doesn't re-persist right). Mock the
persistence fns. The existing `DualPaneExplorer.test.ts` `saveAppStatus`/`savePaneTabs` mock assertions stay green. New
subscriber module → 70% gate covers it (if it's a `.svelte.ts` module; a DPE-inline `$effect` has no backstop → A4
review).

**DONE:** one subscriber fires nav-state persistence; the absorbed trigger sites deleted from `navigate()` + handlers;
the KEEP set documented in `pane/CLAUDE.md`; grep "where does pane nav-state persist?" → one module; `--fast` + full
suite + `desktop-e2e-linux` green; behavior identical (paths/volume/sort/view/focus/layout all still persist).

### M5 – Edge flows onto the transaction

**Scope:** fold `handleCancelLoading` (DPE:708), `handleMtpFatalError` (DPE:757), `handleRetryUnreachable` (DPE:770),
`handleOpenHome` (DPE:799), AND `handleVolumeUnmount` (DPE:1001) onto `navigate()`'s commit + token + (M4) subscriber
path, deleting their bespoke commit/persist sequences.

**Intentions:**

- `handleCancelLoading`: the three branches (network restore, history-back, walk-up-to-parent) each end in a commit –
  route through `navigate({ source: 'cancel' })` for the history-back + walk-up arms; the network-restore arm is a
  state-restore (commit without a load) – express as a `navigate()` variant or keep the `setNetworkHost` + commit inside
  a `source: 'cancel'` path. Preserve `containerElement?.focus()` exactly (it focuses after cancel, DPE:719/746/754 –
  distinct from the L1 no-focus volume-select).
- `handleMtpFatalError` / `handleRetryUnreachable` / `handleOpenHome`: each commits a `{ volumeId, path, historyEntry }`
  → `navigate({ source: 'fallback' })`. `handleRetryUnreachable` clears `tab.unreachable` + `requestVolumeRefresh` first
  (keep that ordering); `handleOpenHome` clears `tab.unreachable` → `~`. The `unreachable` tab-state flag is tab-manager
  state (Phase-1 scope) – `navigate()` reads/clears it via the tab-mgr API, doesn't promote it. These three DO push
  history (e.g. `handleMtpFatalError` pushes `{ volumeId, path }`, DPE:765).
- `handleVolumeUnmount` (DPE:1001, the `volume-unmounted` redirect): for EACH pane whose `volumeId === unmountedId`,
  switches to the default volume at `~` (or `/` if `~` is gone). **Per-pane** (it independently redirects left and
  right, DPE:1008/1016 – not just the focused pane), and **NO history push** today (it does `setPaneVolumeId` +
  `setPanePath` + `saveAppStatus` + `saveTabsForPaneSide` only, DPE:1008–1019 – no `pushHistoryEntry`/`pushPath`). Route
  each affected pane through `navigate({ source: 'fallback' })`, but the intent/source must be able to express
  **suppress history push** so the redirect doesn't add a history entry the current code doesn't. **Pin today's
  no-history-push behavior in an M1 scenario (extend #6's fallback group) BEFORE folding it** – a naive
  `navigate({ source: 'fallback' })` that always pushes history would be a PR3 regression (an unmount would inject a
  spurious Back target). Decide the mechanism: either a `fallback`-source that never pushes history (but then
  `handleMtpFatalError` – which DOES push – can't share the source), OR an explicit `pushHistory: false` on the intent
  for the unmount case. Verify which fallbacks push and which don't, and encode the split – don't assume all `fallback`
  sources behave identically.

**Landmines:** L1 (cancel/fallback DO focus the container, unlike volume-select – preserve per-source focus behavior).
The unreachable-retry `resolvePathVolume` timeout fallback to `getDefaultVolumeId` (DPE:780) must survive.
**History-push asymmetry:** MTP-fatal / retry / open-home push history; the unmount redirect does NOT – the intent must
distinguish them (see above), or the unmount silently grows a Back target (PR3).

**Test plan:** the M1 cancel/MTP-fatal/unreachable scenarios (#4, #5, #6) plus the new unmount-redirect scenario
(per-pane redirect, no history push) re-pointed at the folded handlers, green. The `VolumeUnreachableBanner` E2E
(retry + open-home) + the cancel-during-load E2E + the volume-unmount-redirect E2E (if one exists; else add a headless
test) stay green. Re-grep
`handleCancelLoading|handleMtpFatalError|handleRetryUnreachable|handleOpenHome|handleVolumeUnmount` → the coordinator
bodies are now thin `navigate()` shims (or deleted, the logic living in `navigate()` source branches).

**DONE:** all FIVE edge flows on the transaction; per-source focus behavior identical (L1); the history-push asymmetry
(unmount suppresses, others push) preserved byte-identically; unreachable/MTP/unmount fallback behavior byte-identical;
`--fast` + full suite + `desktop-e2e-linux` green. **Phase-end:** `--include-slow` green (macOS Playwright +
`rust-tests-linux`) + watch CI to green before the phase merge to `main`. Update `file-explorer/CLAUDE.md` § Gotchas
(rewrite the stale-path gotcha as the token contract, master § Docs updates), `pane/CLAUDE.md` (the `navigate()`
transaction + the persistence-subscriber split), `docs/architecture.md` frontend section.

## Invariants this phase must honor

- **P4** (loud) – optimistic: immediate commit, background correction gated by token, no synchronous
  validate-then-commit IPC. Pinned by M1 scenario #8.
- **P2 / P3** – arrow/Page/Home/End cursor nav stays `sendKeyToFocusedPane` → FilePane, never `navigate()`;
  `cursorIndex`/selection stay FilePane-local.
- **A4** – `navigate()` ships WITH the deletion of the old braid + `volumeChangeGeneration` + the prefix forensics, same
  phase, no parallel paths. M3's grep-to-zero is the gate.
- **A5** – nav-state persistence fires from one subscriber; the tab-CRUD + MCP-mirror + settings split is documented
  (M4). One grep answer to "where does pane nav-state persist?"
- **L1** – per-source focus: `moveCursor` refocuses; volume-select + path-nav + mirror do NOT; cancel/fallback DO.
  mtp.spec.ts:414 is the guard.
- **L2** – `whenLoadSettles` ordering inside `moveCursor` untouched.
- **L5** – `computeHasParent` ⟷ `isCrossVolumeNavigation` stay coupled; `navigate()`'s snapshot routing matches
  `FilePane.handleNavigate`.
- **L6** – the token replaces the _mechanism_; the drop-foreign-listings _policy_ is identical.
- **L7** – the two pinned-new-tab branches unify ONLY inside `navigate()` (M2), with tests.
- **L8** – the MCP tab/focus mirror (`syncTabsToBackend`, `updatePaneTabs`, `updateFocusedPane`, 100ms debounce) is NOT
  persistence; untouched by M4.
- **L12** – the `NavigateResult` refusal union carries the EXACT current strings; the `mcp-response` forward is
  byte-identical; pinned by M1 scenario #7.
- **PR1/PR3** – each milestone add+migrate+delete atomic; byte-identical user-visible behavior (toast copy, focus
  timing, persisted state, history depth). EXTRA PR3 scrutiny – riskiest braid.
- **PR5** – the phase reverts as one merge range; M2–M5 are cumulative, not independently revertable.

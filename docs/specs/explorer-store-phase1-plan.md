# Explorer store — Phase 1 execution plan

Just-in-time execution plan for Phase 1 of the [explorer architecture refactor](explorer-architecture-plan.md). Read
that master spec first (§ Target architecture 1, § Invariants register, § Landmine register). This plan adapts its five
milestones to the code as it stands after Phase 0.

**Goal of the phase:** un-trap `DualPaneExplorer`'s navigation + UI-chrome state into one module store
(`explorer-state.svelte.ts`), so consumers read state directly instead of through `explorerRef` getters. Phase 0 already
built `PaneAccess` as the future store's read API; this phase moves the object construction from component closures into
store re-exports without changing any factory signature.

## Store file layout decision

One file: `lib/file-explorer/pane/explorer-state.svelte.ts`, exporting `createExplorerState()` plus a module-level
default instance (`explorerState`). Factory-first for testability (vitest instantiates fresh); the singleton is what the
component binds. Where the singleton leaks across tests, ship `_resetForTesting()` (the `snapshot-store` precedent) and
reset in `beforeEach`. `SvelteSet`/`SvelteMap` reset via `.clear()`, never reassignment. Colocated `CLAUDE.md`
enumerates the writers (A2).

**What moves into the store (store's own fields):** `focusedPane`, `showHiddenFiles`, `leftPaneWidthPercent`, and the
**holder references** `leftTabMgr`/`rightTabMgr` (each a `$state<TabManager>`). The tab managers stay values the store
holds, mutated via their existing `tab-state-manager` / `tab-operations` free functions — A1/A2 govern the store's own
fields, NOT the tab-manager internals (resolves the tab-manager tension; see master § 1).

**What stays in the component:** `paneRefs`, `containerElement`, `paneWrapperEls`, `initialized`, all the `unlisten*`
handles, the tab-sync debounce timer, dialog/drag/index wiring, `volumeChangeGeneration`, and every navigation handler
(`handleVolumeChange`, `handlePathChange`, `applyPathChange`, `navigateToPath`, `navigate`, `moveCursor`, the `set*`
mutators). Those retire in Phase 3, not here. **`cursorIndex`/selection/listing UI state stay in `FilePane` (P3).**

## Fresh grep (run 2026-06-05, this worktree)

`explorerRef` readers outside `DualPaneExplorer.svelte` — matches the spec's caller map exactly, **zero new readers**:

| File                                                                                           | Surface used                                                            | Migrates in |
| ---------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- | ----------- |
| `routes/(main)/+page.svelte`                                                                   | F-bar flag (`onFocusedVolumeChange`), dialog data path, dialog togglers | M3, M4      |
| `routes/(main)/command-dispatch.ts`                                                            | ~50 write/command calls + `getFocusedPanePath`/`getFocusedPaneVolumeId` | reads in M4 |
| `routes/(main)/mcp-listeners.ts`                                                               | `navigateToPath`, `openItemUnderCursor`                                 | Phase 3     |
| `lib/file-explorer/quick-look/…`                                                               | `routePanelKey` (doc-comment ref only; via ExplorerAPI)                 | Phase 2     |
| `lib/file-explorer/navigation/navigate-and-select.ts`                                          | `navigateToPath`, `moveCursor`                                          | Phase 3     |
| `lib/go-to-path/go-to-path.ts`                                                                 | `getFocusedPanePath`, `getFocusedPane` (reads) + nav helpers (write)    | reads in M4 |
| `lib/downloads/go-to-latest.ts`                                                                | `getFocusedPane` (read) + `navigateToPath` (write)                      | reads in M4 |
| `lib/downloads/{event-bridge,global-shortcut-bridge}.svelte.ts`, `DownloadToastContent.svelte` | hold `ExplorerAPI` handle                                               | M4          |

`app-state.ts` hit is a doc comment only — not a reader. **`getFocusedPane`/`getFocusedPanePath`/`getShowHiddenFiles`**
are the read-only getters this phase retargets at the store; `navigateToPath`/`moveCursor` are WRITE entries that stay
on `explorerRef` until Phase 3 (the helpers keep their `ExplorerAPI` param this phase).

Component `$derived`s today (DualPaneExplorer ~line 139–150, 396–408): `left/rightPath`, `left/rightVolumeId`,
`left/rightViewMode`, `left/rightSortBy`, `left/rightSortOrder`, `left/rightHistory` (12 per-pane, P1-safe), plus
`leftVolumePath/Name`, `rightVolumePath/Name`, `volumes` (from `getStoreVolumes()`). **None read both panes** — keep it
that way.

**Deviation from spec:** master § Goal cites `DualPaneExplorer.svelte` at 3318 lines; it is **2140** post-Phase-0. The
state block and accessor helpers are the migration surface; line numbers below are indicative.

## Milestones

Each milestone is atomic (add + migrate + delete old path; PR1). Gates per milestone: `--fast` continuously during work;
full `pnpm check` + `--check desktop-e2e-linux` before the milestone commit (macOS Playwright allowed overnight — rAF
fix landed, host idle). Import-cycle rule: the store imports **nothing** from `routes/` or command handlers.

### M1 — `createExplorerState()` factory + tests (TDD)

**Scope:** new `explorer-state.svelte.ts` (factory + default instance) + `explorer-state.test.ts`. No component changes
yet — the store is built and tested in isolation.

**Intentions:** A1 (module-private `$state`; export getters + intent functions, never the writable). A2 (one named
mutator per field: `setFocusedPane`, `toggleHiddenFiles`/`setShowHiddenFiles`, `setLeftPaneWidthPercent`, plus tab-mgr
holder getters `getTabMgr(pane)` returning the **live** `$state<TabManager>` reference — never a copy/snapshot, so
`$derived` callers keep tracking after the move). Factory + `_resetForTesting()` testability.

**Test plan (red-green TDD):** factory isolation (two instances don't share state); getter/mutator round-trips;
`_resetForTesting` clears to defaults; `getTabMgr` returns a live reference whose mutations are observed by a
`$derived`. New file → covered by the 70% `src/lib/**` gate, so tests land in this milestone (PR2).

**DONE:** new file + tests green; `--fast` + full suite + `desktop-e2e-linux` green; no component wired yet (store is
dead code this one milestone — acceptable mid-phase, consumed in M2). `knip` may flag unused exports; wire enough in M2
to clear it, or land M1+M2 as one commit if knip can't tolerate the gap.

### M2 — Move holders; `PaneAccess` construction becomes store-backed

**Scope:** `DualPaneExplorer.svelte` state block + the `paneAccess` object (~line 337) + the 12 per-pane `$derived`s.

**Intentions:** delete the component's `let focusedPane`, `let showHiddenFiles`, `let leftPaneWidthPercent`,
`let left/rightTabMgr` and read them from `explorerState` instead. `PaneAccess` getters now close over the store
(`getFocusedPane: () => explorerState.getFocusedPane()`, etc.) — **factory signatures unchanged**, so the four Phase-0
factories (`clipboard-operations`, `file-operation-commands`, `pane-commands`, `drag-drop-controller`) are NOT touched.
The 12 `$derived`s re-read `getActiveTab(getTabMgr(pane))` through the store holder; they stay per-pane (P1). Component
writers (`setPanePath`, `setFocusedPane` callers, etc.) call the store's named mutators. `$effect` creation timing
unchanged (L3). Tab managers keep their setter API (master § 1 A1/A2 scope).

**Test plan:** existing `DualPaneExplorer.test.ts`, `selection-consistency.test.ts`, `listing-diff-sync.test.ts`,
`file-pane-keyboard.test.ts` must keep passing unchanged — they're the behavior contract. Add `_resetForTesting()` to
the test harness `beforeEach` if the singleton bleeds.

**DONE:** focus/hidden/layout/tab-holders live in the store; PaneAccess store-backed; all four factories untouched; full
suite + `desktop-e2e-linux` green. Behavior byte-identical (PR3: focus timing, menu check states, toast copy).

### M3 — F-bar reads the store; delete `onFocusedVolumeChange`

**Scope:** `FunctionKeyBar.svelte` (mounted in `+page.svelte`), `+page.svelte` (~line 100, 937),
`DualPaneExplorer.svelte` (`onFocusedVolumeChange` prop ~line 161/170/590 + its `$effect`).

**Open question 1 — resolved YES (direct store read).** `FunctionKeyBar` reads the focused pane's volumeId from the
store directly via a `$derived`, replacing the `onFocusedVolumeChange` callback → `+page.svelte` `focusedPaneVolumeId`
$state →
prop chain. **Rationale:** the callback exists *only* because plain `explorerRef` method calls aren't reactive (see the
prop's own doc comment); a store getter inside a `$derived`IS reactive, so the entire plumbing — prop declaration,`$props` destructure, the `$effect`that fires it, the`+page.svelte`mirror state — deletes. This is exactly the consumer the store exists for (master Open Q1 lean), and A9 blesses component`$effect`s reacting to module stores (`ai-toast-sync`/`quickLookState`
precedent) — don't re-wrap in props.

**Known-transitional A6 exception (carry forward, do NOT fix here):** the `=== 'search-results'` derivation behind the
F-bar capability flags (`isFocusedPaneSearchResults` in `+page.svelte`) stays as a volume-id string compare. Phase 4
owns replacing it with capabilities. Keep the string compare; just move its _input_ (the volumeId) to a store read.

**Test plan:** F-bar capability E2E (search-results pane disables F2/F7/Shift+F4) must stay green — that's the contract
the deletion can't regress. Existing Vitest for `+page.svelte` F-bar wiring kept passing.

**DONE:** `onFocusedVolumeChange` prop + `$effect` + `+page.svelte` mirror state deleted; F-bar flags driven by store
`$derived`; full suite + `desktop-e2e-linux` green; F-bar disablement behavior identical.

### M4 — Migrate remaining external readers off `explorerRef` getters

**Scope (read-only getters only):** `+page.svelte` dialog data path (`getFocusedPaneEntries`,
`applyIndicesToFocusedPane`, `getFocusedPaneSearchableFolder`, `getFocusedPanePath`); `go-to-path.ts`
(`getFocusedPanePath`, `getFocusedPane`); `go-to-latest.ts` / downloads bridges (`getFocusedPane` read);
`command-dispatch.ts` read calls (`getFocusedPanePath`, `getFocusedPaneVolumeId`). Retarget these reads at
`explorerState` getters.

**Intentions:** `getFocusedPaneEntries` / `applyIndicesToFocusedPane` touch `FilePane` (cursor/selection, P3) — they
stay on `explorerRef` for now; only the _focused-pane-selection_ read (`getFocusedPane`, `getFocusedPanePath`) moves to
the store. Anything that WRITES navigation (`navigateToPath`, `moveCursor` in `navigate-and-select`, `mcp-listeners`,
`go-to-latest`, `handleSearchNavigate`) **stays on `explorerRef`** — retires in Phase 3. The downloads bridges /
`DownloadToastContent` keep their `ExplorerAPI` param this phase (their nav calls are Phase 3); only swap their
`getFocusedPane` read if it's cleanly store-backed without dragging the write surface along — else defer the whole file
to Phase 3 and note it.

**Test plan:** no coverage backstop on routes-side files (PR2) — the A4 "no parallel paths" review is the only guard.
Existing `go-to-latest.test.ts`, `DownloadToastContent.test.ts`, `go-to-path.test.ts` must pass unchanged. Re-grep
`explorerRef` read getters at milestone end; the remaining hits must all be WRITE surfaces deferred to Phase 3.

**DONE:** read-only `getFocusedPane*` consumers read the store; write surfaces still on `explorerRef` (documented as
Phase-3 work); full suite + `desktop-e2e-linux` green.

### M5 — Store `CLAUDE.md` + `cmdr/no-explorer-state-writes` ESLint rule + docs sweep

**Scope:** new `lib/file-explorer/pane/CLAUDE.md` section (or colocated note) for the store: writers list (A2), the
A1/A2-vs-tab-manager scope boundary, the live-reference-getter rule. New ESLint plugin
`apps/desktop/eslint-plugins/no-explorer-state-writes.js` + registration in `eslint.config.js`. Update `pane/CLAUDE.md`
file map, `file-explorer/CLAUDE.md`, and `docs/architecture.md` frontend section.

**ESLint rule (model on `no-raw-tauri-invoke.js`):** ban importing the store module's **writable surface** (the raw
`$state` / direct field assignment) outside `explorer-state.svelte.ts` itself. Allow the getter/intent exports
everywhere. Allowed-path fragments: the store file, `.test.`, `/test/`. `meta.type: 'problem'`, a `messageId` pointing
at the store's named mutators. Register under the existing `cmdr` plugin block (~line 226) alongside the other two
rules. This is the durable anti-rot guardrail (A2) — discipline that isn't enforced decays once the component wall is
down.

**Test plan:** the rule itself needs a fixture (a file that assigns the store field → reported; a file that calls a
mutator → clean). Mirror how the repo tests its other custom rules. `--fast` runs the rule across `src/`.

**DONE:** store `CLAUDE.md` written; ESLint rule landed + registered + green across `src/`; docs swept; full suite +
`desktop-e2e-linux` green. **Phase-end:** `--include-slow` green + watch CI to green before the phase merge to `main`.

## Invariants this phase must honor

P1 (per-pane slices, no cross-pane `$derived`), P3 (cursor/selection stay in `FilePane`), A1/A2 (private state + one
mutator + lint), A8 (no new components), A9 (component `$effect`s on module stores are blessed). The A6 volume-id string
compare behind F-bar flags is a **known-transitional exception** owned by Phase 4 — carried, not fixed, here.

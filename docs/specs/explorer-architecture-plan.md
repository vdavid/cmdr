# Explorer architecture plan

**Status: shipped (2026-06-05).** All phases (0–4) landed on `main`; the four per-phase plans in this folder are the
execution records. Phase 5 (backend-authoritative pane state) is deliberately deferred – revisit when a feature needs
two writers of pane state (multi-window, agent-driven panes) or transactional MCP writes. The Phase-2 plan's optional M6
(flat handler record) shipped separately via [command-handler-record-plan.md](command-handler-record-plan.md). This is a
historical document; the current architecture is described in `docs/architecture.md` and the colocated `CLAUDE.md`
files.

The master spec for refactoring the dual-pane explorer frontend from component-trapped state + imperative instance APIs
to a module store + typed command dispatch + transactional navigation + capability-driven virtual volumes.

This document is the durable context for every agent working on any phase. It carries the **intentions** behind each
decision so implementers can adapt tactically without drifting strategically. Per-phase execution plans are written
just-in-time before each phase (see § Phase map) and link back here.

## Goal

Pay down the four root-cause debts of the explorer area:

1. **State trapped in component instances.** `DualPaneExplorer.svelte` (3318 lines) owns tabs/focus/layout;
   `FilePane.svelte` (2936) owns cursor/selection/listing UI state. This is why the two imperative APIs exist (the
   `ExplorerAPI` interface has ~60 members backed by 62 component `export function`s; `FilePaneAPI` has ~55 methods):
   they're accessors for state behind `bind:this`. Every consumer threads through
   `+page.svelte → explorerRef → paneRef`.
2. **Navigation is a braid, not a transaction.** Volume change + path change + history + tab mutation + persistence +
   virtual-volume guards interleave across four functions, defended by two ad-hoc generation counters and string-prefix
   forensics. Every navigation bug to date (stale `onPathChange` corruption, snapshot-pane poisoning, SMB mis-load) is
   this debt expressing itself.
3. **Commands have ~7 entry paths, several stringly-typed.** Keyboard, native menu (two listeners in two components),
   palette, F-key bar (bypasses dispatch entirely!), MCP (17 inbound events plus the `mcp-response` reply channel,
   payloads cast from `unknown`), Quick Look key forwarding, debug panel.
4. **Virtual volumes are string compares.** `volumeId === 'search-results'` / `'network'` branches are sprinkled across
   navigation, clipboard, transfer, delete, breadcrumb, and MCP sync. Each new virtual volume costs a codebase sweep.

## Non-goals

- **No behavior changes.** Every phase is behavior-preserving; the E2E suites are the contract.
- **No touching `FileDataStore`.** The non-reactive listing store (O(visible) reactivity, see
  [docs/notes/non-reactive-file-store.md](../notes/non-reactive-file-store.md)) is the load-bearing perf design.
  Untouchable.
- **No framework.** No event sourcing, no action log, no state library. Plain runes modules + one dispatch function: the
  idiom the codebase already speaks.
- **No backend-authoritative pane state (yet).** The backend already mirrors focus/tabs for MCP. Making it authoritative
  is a possible future phase; this plan keeps shapes compatible with it (intent-based `navigate()`, command dispatch)
  but doesn't do it. Decide after the navigation phase ships.
- **No warm tab cache, no `{#key}` removal, no listing-layer changes.**

## Why past attempts failed (and what that teaches)

Transcript + git archaeology of all prior attempts found: **no split was ever landed-and-reverted.** The one genuine
attempt (a `<DialogCoordinator>` child component) was correctly killed at planning: a reviewer judged it "a boundary
without a real responsibility seam" because dialogs read and write pane state heavily, and a child-component boundary
severs that. Meanwhile every closure-based factory extraction (`dialog-state`, `tab-operations`, `initialization`,
`index-events`, `listing-diff-sync`, `pane-mcp-sync`) landed and stuck.

**The rule this teaches:** in this subsystem, _component boundaries fail; closure/factory/module boundaries succeed._
This plan introduces zero new components. The seam is not between features: it's between **state ownership** (one store
module) and **command logic** (factories/handlers reading the store through a typed interface).

## Target architecture

Four structures. Each carries its intention; tactical shapes may adapt, intentions may not.

### 1. Explorer store (`explorer-state.svelte.ts`)

Module-level runes store owning what `DualPaneExplorer` holds today: per-pane tab managers, `focusedPane`,
`showHiddenFiles`, layout (`leftPaneWidthPercent`). Components become projections.

**Intention:** dissolve the instance APIs by un-trapping the state, so callers read state directly and dispatch commands
directly, instead of relocating the 46 exports, we remove their reason to exist.

Binding constraints (from discovery):

- **Per-pane slices, never a monolith.** The store exposes `left`/`right` sub-state so Svelte's fine-grained tracking
  keeps today's invalidation granularity (12 per-pane `$derived`s). No single `$derived` may read both panes' tab
  arrays. _(Perf invariant P1.)_
- **State is module-private.** Export getters (`getFocusedPane()`) and intent functions (`switchFocus()`, `navigate()`),
  never the raw writable `$state`. This is the existing `volume-store` pattern promoted to a hard rule. The quick-look
  "exported-mutable + documented writers" shortcut does NOT generalize to a 10+-field store.
- **One named mutator per state field, all inside the store module.** Writers enumerated in the store's colocated
  `CLAUDE.md`.
- **Enforced by lint:** a new ESLint rule (modeled on `cmdr/no-raw-tauri-invoke`) banning imports of the store's
  writable surface outside the store module. Discipline that isn't enforced decays; this rule is what keeps "one bounded
  writer" true after the component wall comes down.
- **Factory-first for testability:** `createExplorerState()` factory + a module-level default instance. Tests
  instantiate fresh; the singleton is for the app. Where a true singleton is unavoidable, ship `_resetForTesting()` (the
  `snapshot-store` precedent) and reset in `beforeEach`. `SvelteSet`/`SvelteMap` reset via `.clear()`, never
  reassignment.
- **What does NOT move:** `cursorIndex`, selection, listing UI state stay local to `FilePane` (hot path; perf invariant
  P3). The store is navigation + UI-chrome state only.
- **Scope of A1/A2 (resolves the tab-manager tension):** the private-state + one-mutator rules and the lint rule govern
  the **store's own fields** (`focusedPane`, layout, `showHiddenFiles`, and the holder references). The tab managers are
  _values the store holds_, not store fields: they keep their existing setter-based API (`createTabManager`) mutated via
  their existing free functions. Wrapping every tab-manager setter behind store intents would be exactly the churn the
  "move the holder, not the signals" design avoids. Tab-manager discipline stays where it lives today: in
  `tab-state-manager.svelte.ts` and `tab-operations.ts`.

Note: the tab managers are _already_ module-factory `$state` objects (`tab-state-manager.svelte.ts`); the component
merely holds the references. Phase 1 moves the holder, not the signals, which is why zero perf cost is a structural
fact, not a hope.

### 2. Typed command dispatch (promote `command-registry.ts`)

One dispatch spine for every entry path that resolves to a user-intent command: keyboard combos, native menu (both
listeners), palette, F-key bar, MCP, Quick Look forwarded commands, debug panel.

**Intention:** one place where "what can the user do, and when is it allowed" lives: typed end to end, with capability
gating declared once instead of three times (F-bar flags, dispatch guards, MCP error strings today).

Binding constraints:

- **Typed ids and args, derived from the registry.** `const commands = [...] as const satisfies readonly Command[]`;
  `type CommandId = (typeof commands)[number]['id']`; a `CommandArgs` map;
  `dispatch<K extends CommandId>(id: K, args: CommandArgs[K])`. Handlers as `Record<CommandId, Handler>` so a missing
  handler is a compile error and cmd-click traverses dispatch → handler in one hop. Today's registry is `Command[]` with
  `id: string`. This typing work is the first milestone of the phase.
- **New ESLint rule `cmdr/no-raw-command-dispatch`:** no string literals as dispatch ids outside the registry; callers
  must pass `CommandId`-typed values. The durable anti-rot guardrail, same role as `no-raw-tauri-invoke`.
- **No string-action sub-dispatchers survive.** `handleSelectionAction(action: string)`,
  `handleMcpSelect(mode: string)`, MCP tab `action: string`, `confirmDialog(dialogType: string)` get promoted to
  registry commands or literal-union params. The existing `navigate(action: 'back' | 'forward' | 'parent')` literal
  union is the model.
- **Per-keystroke keys stay OUT of the bus.** Arrow keys, type-to-jump chars, and type-to-jump reset keys keep the
  direct `DualPaneExplorer.handleKeyDown → FilePane` path. The bus carries resolved commands only, never raw keystrokes.
  Routing arrows through dispatch would add a registry lookup + `log.info` + `record_breadcrumb` IPC per keypress.
  _(Perf invariant P2.)_
- **Pre-dispatch guards survive verbatim.** The text-region intercepts (⌘C-with-text-selection, ⌘←/→-in-input,
  `.error-pane` copy zone) stay _in front of_ dispatch in `handleGlobalKeyDown`: they're browser-native behavior, not
  app commands. Moving them into commands re-introduces the error-pane copy and rename-cursor regressions.
- **MCP becomes MORE typed than today.** The 17 inbound `mcp-*` events (14 of them with `as {...}` casts) map onto
  registry commands with typed args; payloads get validating parses (the `mcp-open-search-dialog` whitelist is the
  in-repo precedent). `mcp-response` is the outbound reply channel: it stays in the adapter (Open Q2), it does not map
  to a command. Stretch goal: typed events via tauri-specta instead of hand-built `json!` payloads on the Rust side.
- **Known duplications to reconcile (from the caller map):** the `handleFn*` handlers in `+page.svelte` (which
  `FunctionKeyBar` invokes via its `onView`/`onCopy`/… props) duplicate the `file.*` commands and bypass dispatch;
  `view-mode-changed` (native menu event) duplicates `view.brief`/`view.full`; `menu-action` is listened inside
  `FilePane`. All three converge on the bus in this phase.

### 3. Transactional navigation (`navigate(intent)`)

One entry point for every pane navigation, replacing the current four-function **coordinator-level** braid
(`DualPaneExplorer.navigateToPath`, `handleVolumeChange`, `handlePathChange`, `applyPathChange`) and both ad-hoc
generation counters. Scoping note: there are TWO `navigateToPath`s: the coordinator-level `ExplorerAPI` method (retired
by this phase) and the per-pane `FilePane.navigateToPath` primitive that actually drives listing loads. `navigate()`
sits ON TOP of the FilePane primitive (listing-load mechanics stay pane-owned); only the coordinator-level entries are
retired.

**Intention:** make navigation-state corruption _inexpressible_ rather than guarded-against. One commit point for state,
one for persistence, one staleness mechanism.

Shape:

```ts
navigate({
  pane,
  to: { volumeId?, path } | { history: 'back' | 'forward' | 'parent' } | { snapshot: id },
  source, // 'user' | 'mcp' | 'history' | 'correction' | ...
})
```

Inside, in order: capability/validity checks (MTP, virtual volumes, read-only) → tab decision (in-place vs
new-tab-on-pinned) → **single state commit** (volumeId + path + history together, never separately) → **single
persistence call** → listing load triggered as a consequence, carrying a transaction token.

Binding constraints:

- **The old entries are retired, not wrapped.** If `handleVolumeChange` or `applyPathChange` survive alongside
  `navigate()`, the refactor added a fourth writer and made things worse. The phase isn't done until they're deleted.
- **The sync-refusal contract gets a typed replacement.** Today `navigateToPath` returns `string | Promise<void>`, where
  the `string` is a refusal sentinel that three external callers branch on via `typeof result === 'string'`
  (`mcp-listeners.ts`, `navigation/navigate-and-select.ts`, `go-to-path`). `navigate()` returns a `NavigateResult`
  discriminated union (`{ status: 'started', settled: Promise<…> } | { status: 'refused', reason: NavigateRefusal }`) so
  the refusal path survives byte-identically without re-introducing the stringly-typed channel. _(Landmine L12.)_ **The
  refusal strings are themselves contract:** the MCP adapter forwards them verbatim as the `mcp-response` error payload,
  and agents/users read them ("Pane is on the Network volume. Use `select_volume`…"). `NavigateRefusal` carries a
  `message` field holding the exact current strings (or the adapter maps reason → string); a regression test pins the
  texts. This is also _why_ `mcp-response` stays in the adapter (Open Q2).
- **One transaction token subsumes all three staleness mechanisms:** the `listing-complete` path-prefix forensics in
  `applyPathChange` (string-classifying state, the FE twin of the banned `error-string-match` pattern), the
  `volumeChangeGeneration` counter, and (stretch) `quickLookFollowGeneration`. Stale results are dropped by token
  compare, immune to new volume schemes.
- **Navigation stays optimistic.** State commits immediately; `determineNavigationPath` corrections apply in the
  background gated by the token, exactly today's semantics. No synchronous "validate then commit" gate (that would be a
  UX regression, not just perf). _(Perf invariant P4.)_
- **Persistence becomes one subscriber.** Today: ~25 `saveAppStatus` + ~24 `saveTabsForPaneSide` + ~6
  `saveLastUsedPathForVolume` call sites in `DualPaneExplorer`, plus ~12 `saveTabsForPane` calls in `tab-operations.ts`.
  After: one debounced, diffed persistence module subscribing to store changes. Grep-able answer to "where does pane
  state get persisted?": one file.
- **Testable headless:** `navigate(intent, { resolveVolume, state })` against a fake volume resolver, including the
  cross-volume snapshot branch and the pinned-tab fork. The corruption regression tests are written FIRST (see §
  Verification).

### 4. Virtual-volume capabilities

A FE-side capability interface mirroring the spirit of the Rust `Volume` trait.

**Intention:** adding virtual volume #3 becomes "implement the interface," not "sweep the codebase."

Binding constraints:

- A real typed interface
  (`interface VolumeCapabilities { canWrite; canCreateChild; supportsTrash; hasBackendListing; pathScheme; … }`) plus a
  `kind` discriminated union, NOT a `Record<string, boolean>` bag.
- Command `canExecute` and all guard logic branch on capabilities, never on `volumeId === '…'`. This is the FE analogue
  of the repo-wide "no string-matching state classification" rule.
- `FilePane`'s `{#if}` alt-view chain resolves through a per-kind content descriptor.
- The existing `searchResultsVolumeCapabilities()` in `lib/search/capabilities` is the seed, generalized.

## Discovery verdicts (2026-06-04)

Three questions were investigated against the actual code before this spec was written. Full agent reports informed the
constraints above; the verdicts:

**Performance: zero degradation is achievable, structurally.** The only path that ever caused real perf pain (50k
listing reactivity) is quarantined in `FileDataStore` and untouched. Everything the refactor touches runs at human
frequency or O(visible). The tab managers are already module-factory `$state`, so the store move changes the holder, not
the signals. Five perf invariants (P1–P5 below) neutralize the identified risks; one new micro-benchmark (cursor
latency) guards the hottest path.

**Reasoning: genuinely easier, conditional on conventions.** Five representative flows traced before/after: three
clearly clearer (stale-listing token, Selection-dialog dispatch, MCP nav), two conditionally clearer (only if
`navigate()` truly retires the old entries and persistence gets one commit point), none worse at trace level. The honest
costs: jump-to-definition gains one hop through the dispatch table (mitigated by typed ids + flat
`Record<CommandId, Handler>`), and module state invites N writers (neutralized by the private-state + lint-rule
constraints, without those, this refactor IS the "differently complicated" failure mode, so they're non-negotiable).
Bonus finding: two component `$effect`s already react to a module store today (`quickLookState`): the pattern is
production-proven (`ai-toast-sync`), and the spec blesses it explicitly so future agents don't "clean it up."

**Type safety & testability: at least equal, with upside, IF the registry typing lands first.** A naive
`dispatch(id: string, args: unknown)` would be a strict regression vs today's fully-typed `explorerRef` methods: the
`CommandId`/`CommandArgs` derivation plus the `no-raw-command-dispatch` lint rule is what makes it instead a net gain
(today command ids are bare `string` everywhere, and the MCP boundary is `unknown`-cast). Testability is a clear win:
seven currently mount-only logic clusters become headless-testable (transfer guard chains, clipboard branches, the
navigation transaction, `copyPathBetweenPanes` refinement, selection dispatch, sort toggle, MCP handlers). The one trap
(module-store state bleeding between vitest tests) has an in-repo answer: factory stores per test
(`createSelectionState` precedent) or `_resetForTesting()` (`snapshot-store` precedent). Component shells stay thin
(markup + store binding) so per-file coverage stays above the 70% gate without allowlisting.

## Invariants register

The numbered, non-negotiable list. Reviewer agents check every milestone against these.

**Perf:**

- **P1**: No `$derived` reads both panes' tab arrays/state. Store is per-pane sliced.
- **P2**: Arrow keys, type-to-jump chars, and type-to-jump reset keys never route through command dispatch. No
  per-keystroke registry lookup, logging, or breadcrumb IPC.
- **P3**: `cursorIndex`, selection, and listing UI state stay local to `FilePane`. Not promoted to the store.
- **P4**: Navigation stays optimistic: immediate state commit, background correction gated by token. No new synchronous
  pre-navigation IPC.
- **P5**: `FileDataStore` stays non-reactive; `{#key activeTabId}` cold recreation and the 50 ms tab-cycle debounce are
  preserved.

**Architecture:**

- **A1**: Store `$state` is module-private; only getters and intent functions are exported.
- **A2**: Every store field has exactly one named mutator, inside the store module; writers enumerated in the store's
  `CLAUDE.md`; enforced by the new store-write ESLint rule.
- **A3**: Dispatch ids are `CommandId`-typed end to end; no string literals outside the registry (enforced by
  `cmdr/no-raw-command-dispatch`).
- **A4**: When `navigate()` ships, the old navigation entries and all three ad-hoc staleness mechanisms are deleted in
  the same phase. No parallel paths.
- **A5**: Persistence fires from exactly one module.
- **A6**: Guard logic branches on capabilities, never on volume-id strings.
- **A7**: Pre-dispatch text-region guards stay in front of the bus, verbatim.
- **A8**: No new components. Factories, stores, and pure helpers only.
- **A9**: Component `$effect`s reacting to module stores are a blessed pattern (`ai-toast-sync` precedent); don't
  re-wrap them in props. **Why:** props can't carry a store that outlives the component and is written from outside the
  component tree; the prop version re-creates the plumbing this plan deletes.

**Process:**

- **PR1**: Every milestone ends green on `pnpm check` and `--include-slow`, with no dead code: a milestone = add +
  migrate callers + delete old path, atomically. `knip` (dead exports) and the 70% coverage gate enforce this
  structurally; duplication is a reviewer gate (the repo's `jscpd` check scans Rust only, so "the old copy still exists"
  must be caught by the A4 review, not tooling).
- **PR2**: New modules land with their tests in the same milestone (TDD for new seams; characterization tests before
  moves). Coverage reality check: the 70% per-file gate covers `src/lib/**` only: every new orchestrator
  (`explorer-state`, dispatch, `navigate()`, capabilities) needs its headless test plan in the SAME milestone (factory
  - fake-resolver tests are the lever), while routes-side migrations (`explorer-api.ts`, `command-dispatch.ts`,
    `mcp-listeners.ts`, `+page.svelte`) have NO coverage backstop: the A4 "no parallel paths" review is the only thing
    catching a half-migrated routes file. Reviewers weight it accordingly.
- **PR3**: Behavior-preserving means byte-identical user-visible behavior, including toast copy, focus timing, and menu
  check states.
- **PR4**: Phases assume their migrated surface is frozen for their duration. Features landing on `main` mid-phase that
  add ExplorerAPI consumers, persistence call sites, or volume-id branches must be caught by a re-grep before the
  phase's final milestone and added to its checklist. The per-phase plan's first milestone always starts with the fresh
  grep, never this spec's tables.
- **PR5**: Rollback granularity is the phase merge. Milestones within a phase are not independently revertable
  (add+migrate+delete is atomic per milestone but cumulative across them); design each phase so reverting its whole
  merge commit is clean, and forward-fix only for issues found within a phase's own worktree.

## Landmine register

Hard-won behaviors that must survive byte-identically. Each implementing agent gets this list; each reviewer verifies
the ones their milestone touches.

- **L1: Focus re-anchoring after MCP cursor moves.** Where: `moveCursor` (refocuses, after awaiting),
  `selectVolumeByIndex`/`navigateToPath` (deliberately do NOT). Rule: regression guard `mtp.spec.ts:414` ("drops a Space
  press"). Move byte-identically; don't normalize.
- **L2: `await paneRef.whenLoadSettles()` before cursor ops.** Where: `moveCursor`. Rule: the
  `move_cursor`-races-pane-load fix. Ordering is load-bearing.
- **L3: `$effect` creation timing.** Where: everywhere. Rule: effects in factories are created synchronously during
  component init (`initListingDiffSync` pattern), never lazily, never in `onMount`. (The effect-poisoning incident.)
- **L4: `swapPanes` is zero-IPC.** Where: `swapPanes` + `getSwapState`/`adoptListing`. Rule: listing ownership swaps on
  the FE only.
- **L5: Snapshot-pane coupling.** Where: `computeHasParent` + `isCrossVolumeNavigation`. Rule: the two MUST stay coupled
  (selection off-by-one / pane poisoning otherwise). Pinned in `pane/CLAUDE.md`.
- **L6: Stale-path guard semantics.** Where: `applyPathChange` → token. Rule: the token replaces the _mechanism_; the
  _policy_ (drop foreign listings) is identical. The corruption scenarios in `file-explorer/CLAUDE.md` § Gotchas become
  the regression tests.
- **L7: Pinned-tab duplication.** Where: `handlePathChange` + `handleVolumeChange`. Rule: the two near-identical new-tab
  branches unify inside `navigate()`, but only there, in the nav phase, with tests. Not as a drive-by.
- **L8: Tab-sync debounce + MCP mirror.** Where: `syncTabsToBackend` (100 ms), `pane-mcp-sync`. Rule: keep debounce
  semantics; MCP mirror reads the store after phase 1 (simplification, not removal).
- **L9: Quick Look key forwarding bypasses DOM.** Where: `routePanelKey`. Rule: synthesized KeyboardEvent path with its
  own type-to-jump intercept mirror. Keep mirrored with the main intercept.
- **L10: Read-only / search-results guards produce specific UI.** Where: transfer/delete/rename openers. Rule: alert
  titles and toast strings are user-facing contract (E2E asserts them).
- **L11: Five externally-unused `ExplorerAPI` members.** Where: `selectVolumeByIndex`, `closeActiveTab`, `switchToTab`,
  `getTabsForPane`, `getVolumes`. Rule: phase 0 removes all five from the `ExplorerAPI` _interface_. But only delete
  function bodies that are truly unreferenced (`closeActiveTab`, `getTabsForPane`, and `getVolumes` after verifying no
  internal callers): **`selectVolumeByIndex` and `switchToTab` have internal callers** (`selectVolumeByName`, tab
  handlers) and keep their bodies (demote to non-exported). L1 depends on `selectVolumeByIndex` surviving. `knip` can't
  see Svelte-component internal calls; verify by grep, not tooling.
- **L12: `navigateToPath` sync-refusal sentinel.** Where: returns `string \| Promise<void>`; three callers branch on
  `typeof result === 'string'`. Rule: replace with the typed `NavigateResult` union (see § Transactional navigation);
  refusal behavior byte-identical.

## Caller map (migration tables)

Full inventory as of 2026-06-04. The phase plans consume these as checklists.

- **`ExplorerAPI` consumers (~8 files, not 3):** `+page.svelte`, `command-dispatch.ts`, `mcp-listeners.ts`,
  `quick-look-state.svelte.ts` (`routePanelKey`), `navigation/navigate-and-select.ts`, `go-to-path/go-to-path.ts`,
  downloads bridges (`event-bridge.svelte.ts`, `global-shortcut-bridge.svelte.ts`, `go-to-latest.ts`,
  `DownloadToastContent.svelte`).
- **`FilePaneAPI`:** held ONLY by `DualPaneExplorer`; helper modules receive it as a parameter. Fully encapsulated
  sub-surface, its dissolution is gated on the store phases, not urgent.
- **Entry paths:** global keydown (`+page.svelte`), pane keydown (`DualPaneExplorer` → `FilePane`), native menu
  `execute-command` (`+page.svelte`) + `menu-action` (inside `FilePane`!) + `view-mode-changed` (inside
  `DualPaneExplorer`), palette (99 registry commands), F-key bar (**bypasses dispatch, the `handleFn*` handlers in
  `+page.svelte` call `explorerRef` directly**), 17 inbound MCP events (+ `mcp-response` outbound), Quick Look
  forwarding, debug panel events.
- **Persistence sites:** ~25× `saveAppStatus`, ~24× `saveTabsForPaneSide`, ~6× `saveLastUsedPathForVolume` (mostly in
  `DualPaneExplorer`), ~12× `saveTabsForPane` (in `tab-operations.ts`), 2×+2× `updateFocusedPane`/`updatePaneTabs`.
  These counts drift: **regenerate them at phase-3 planning time**; the migration checklist is the fresh grep, not this
  table.

## Phase map

Five phases, each its own worktree + execute-style session (sequential agents, one milestone each), merged to `main`
only after `--include-slow` green + CI green. Estimated ~23–25 work agents + ~10 dedicated review/check agents
(end-of-phase full reviews and check runs); the per-milestone architecture-conformance reviews are lighter passes run by
the orchestrator plus a short-lived reviewer agent within the session, not separate full agents.

### Phase 0: Factoring (the map-maker)

Extract command bodies into factories behind a `PaneAccess` interface; delete the 5 dead exports (L11).

**Key decision:** `PaneAccess` is designed as **the future store's read API**: same getter names and shapes the store
will export in phase 1. The factories never change signature when the store lands; only the object construction moves
from component closures to store re-exports. This makes phase 0 scaffolding-free. **Reactivity-transparency
requirement:** `PaneAccess` getters must return live references (the `createTabManager` getter pattern), never copies or
`$state.snapshot`s: signature stability is not enough; call sites inside `$derived`/`$effect` must keep tracking when
the backing source moves from component closures to module `$state` in phase 1.

Milestones: (1) `PaneAccess` + `clipboard-operations.ts` (+ characterization tests for the untested branches); (2)
`file-operation-commands.ts` (~640 lines: transfer/delete/mkdir/mkfile/viewer openers); (3)
`drag-drop-controller.svelte.ts` (state + handlers + 3 listeners + highlight effect; manual drag checklist at the end);
(4) `pane-commands.ts` (MCP/palette surface bodies; exports become one-line delegates).

### Phase 1: Explorer store

Milestones: (1) `createExplorerState()` factory + tests (TDD); (2) move tabs/focus/layout holders; `PaneAccess`
construction becomes store-backed; component deriveds become store projections; (3) migrate read-only consumers off
`explorerRef` getters (F-bar capability flags read the store, note: the `=== 'search-results'` derivation behind those
flags stays put in `+page.svelte` as a known-transitional A6 exception that phase 4 owns and removes; the
`onFocusedVolumeChange` prop plumbing deletes); (4) migrate remaining external readers: downloads bridges, go-to-path,
navigate-and-select, AND the `+page.svelte` dialog data path (`getFocusedPaneEntries`, `applyIndicesToFocusedPane`,
`getFocusedPaneSearchableFolder`, `getFocusedPanePath`; `handleSearchNavigate`'s `navigateToPath`/`moveCursor` calls
retire later, in phase 3); (5) store `CLAUDE.md` + the store-write ESLint rule + docs sweep.

### Phase 2: Command bus

Sequencing note: navigation-related commands (`nav.back/forward/parent`, `mcp-nav-to-path`) route through the bus in
this phase but keep calling the OLD navigation entries until phase 3 swaps the mechanism underneath. The double touch is
intentional (bus = routing, `navigate()` = mechanism), and a phase-2 agent must NOT reach into navigation mechanics to
avoid it.

Milestones: (1) registry typing (`as const satisfies`, `CommandId`, `CommandArgs`, typed `dispatch`, flat handler
record) + `cmdr/no-raw-command-dispatch`; (2) F-key bar onto the bus (deletes the `handleFn*` duplication); (3) native
menu paths (`execute-command`, `menu-action`, `view-mode-changed`) onto the bus; (4) MCP events onto the bus with
validating parses (round-trip `mcp-response` semantics preserved); (5) Quick Look + debug + Selection-dialog `onCommand`
prop deletion; string-action sub-dispatchers promoted.

### Phase 3: Navigation transaction (the hard one)

Milestones: (1) **regression tests first**: the corruption scenarios (stale listing after volume flip, snapshot-pane
poisoning, pinned-tab fork, unreachable fallback, cancel-during-load, MTP fatal fallback) written as headless tests
against the current behavior, red-green against the new `navigate()`; (2) `navigate()` core + transaction token +
fake-resolver tests; (3) migrate `handleVolumeChange`/`handlePathChange`/`navigateToPath` callers; delete old entries

- both generation counters; (4) persistence subscriber (single commit point; absorb the ~55 + ~12 scattered sites); (5)
  `handleCancelLoading`/`handleMtpFatalError`/`handleRetryUnreachable`/`handleOpenHome` onto the transaction.

### Phase 4: Virtual-volume capabilities

Milestones: (1) capability interface + per-kind table + tests; (2) command `canExecute` consolidation (F-bar flags +
dispatch guards + MCP errors read one source); (3) sweep `=== 'network'` / `=== 'search-results'` branches; (4)
`FilePane` alt-view chain through content descriptors.

### Phase 5 (deferred, separate decision): Backend-authoritative pane state

Re-point `navigate()` intents and the MCP mirror at Rust. Decide after phase 3 ships; out of scope here.

## Verification strategy

- **TDD policy:** new seams (store, dispatch typing, `navigate()`, capabilities) are red-green TDD per the repo's TDD
  rule. Pure code motion (phase 0) gets characterization tests _before_ the move for currently-untested logic.
- **Per milestone:** `--fast` continuously; full `pnpm check` + `--include-slow` before the milestone commit. Watch CI
  to green after each phase merge (local-green ≠ CI-green for timing-sensitive specs).
- **Perf gates:** before phase 1 starts, capture baselines: (a) the `VITE_BENCHMARK=1` 50k-listing timeline ("Svelte
  reactivity" segment, expect O(visible) <5 ms), (b) a NEW cursor-move latency probe (Playwright spec: N ArrowDowns in a
  ≥10k dir, p95 keydown→scroll-settled vs a recorded baseline). Re-run both at the end of every phase. For post-merge,
  in-the-wild observability: a dev/opt-in **sampled** keydown-latency breadcrumb (recorded off the hot path, included in
  error-report bundles) so a real-device regression the bench didn't provoke is detectable: the bench gates are
  necessary but bench-only.
- **Import-cycle topology:** the `import-cycles` fast-lane check scans all of `src/` and WILL fire if the store or the
  bus imports its consumers. Rule: the store imports nothing from `routes/` or from command handlers; the bus imports
  the store, never the reverse; handlers import both. Hub modules never reach back.
- **Manual gates:** drag-drop checklist (Finder→pane, pane→pane self-drag, ⌥ flip mid-drag with OS badge, leave +
  re-enter, large-preview suppression) at the end of phase 0 milestone 3 and after phase 2; Quick Look key-forwarding
  smoke after phase 2.
- **Review cadence (execute.md + additions):** every milestone gets an architecture-conformance review against this
  spec's invariants register before the next agent starts; each phase ends with the standard full review + checks
  agents. David personally reviews the seam-defining commit of each phase (the `PaneAccess` shape, the store API, the
  dispatch signature, the `navigate()` intent type, the capability interface).

## Docs updates (per phase)

- `pane/CLAUDE.md`: replace the implicit "DualPaneExplorer resists splitting" with the explicit rule (component
  boundaries no; store/factory seams yes), update the file map per phase.
- New `CLAUDE.md`s colocated with the store and the command bus (writers list, dispatch rules, the two ESLint rules).
- `file-explorer/CLAUDE.md` § Gotchas: rewrite the stale-path gotcha as the token contract once phase 3 lands.
- `docs/architecture.md`: update the frontend section after phases 1–2.

## Open questions (to resolve in per-phase planning, not now)

1. Phase 1: does `FunctionKeyBar` read the store directly or receive props from `+page.svelte`? (Lean: direct store
   read; it's exactly the consumer the store exists for.)
2. Phase 2: do MCP round-trip semantics (`mcp-response` with request ids) live in the dispatcher or stay in
   `mcp-listeners.ts` as a thin adapter? (Lean: thin adapter, the bus shouldn't know about transport.)
3. Phase 3: does the transaction token ride the existing `listing-complete` event payload (Rust change + bindings regen)
   or an FE-side request map keyed by listing id? (Lean: FE-side first, no IPC contract change. Tripwire for promoting
   it to the event payload: if two concurrent loads on the same pane can't be disambiguated by listing id, the FE map is
   insufficient and the token must travel with the event. Decide together with the `NavigateResult` shape, same
   milestone.)
4. Phase 4: do capabilities live per `VolumeInfo` (data, from Rust) or per volume _kind_ (FE table)? (Lean: FE table
   keyed by kind, seeded from `searchResultsVolumeCapabilities`; revisit if Rust grows a capabilities surface.)

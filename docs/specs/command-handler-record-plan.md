# Command handler record – execution plan

Just-in-time execution plan for the deferred, optional **M6** of the
[command bus phase](explorer-command-bus-phase2-plan.md) (§ "M6 (optional) – Flat handler record conversion"), itself a
milestone of the [explorer architecture refactor](explorer-architecture-plan.md). Read the master spec's § Target
architecture 2 ("Typed command dispatch") and § Invariants register first, plus the Phase-2 plan's § M1 "flat record"
decision (the conversion was deferred so a regression would bisect to the conversion alone, not tangle with an
entry-path migration). This plan does that conversion: it turns `handleCommandExecute`'s ~815-line `switch` into a typed
flat handler record over `Exclude<CommandId, DispatchExemptId>`, done characterize-then-convert (M1 pins behavior, M2
converts, M3 sweeps docs).

The win is one-hop cmd-click traversal (dispatch → handler) plus a compile-time guarantee that every dispatchable id has
a handler (missing handler = type error). The switch is already exhaustive over `CommandId` after Phase-2 M1, so this is
NOT a type-safety upgrade for the ids themselves; it's an ergonomics + completeness-guarantee upgrade, and it dissolves
the `file-length` warning on `command-dispatch.ts` (815 lines, warn threshold 800) by splitting handlers into
family-grouped modules, WITHOUT allowlisting.

## Loud rules (read before touching anything)

- **P2 – the per-keystroke ids NEVER enter dispatch, and this refactor must not "finish the migration."** Six ids
  (`nav.up`, `nav.down`, `nav.left`, `nav.right`, `nav.firstInFull`, `nav.lastInFull`) are in `COMMAND_IDS` only for the
  shortcuts-rebinding UI; the live keystroke path is `DualPaneExplorer.handleKeyDown → FilePane`, NEVER the bus (master
  P2: routing an arrow through dispatch would add a registry lookup + `log.info` + `record_breadcrumb` IPC per
  keypress). They are members of the `DispatchExemptId` union BY DESIGN. The exemption union must document WHY per
  family so a future agent doesn't see "20 ids with no handler" and add handlers to `nav.up` "to be complete." Adding a
  handler for any P2 id is a perf regression, not a completion.
- **PR3 – byte-identical user-visible behavior.** Toast copy verbatim (the zoom toasts,
  `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`, the tab-limit / no-recently-closed warns, the cloud error toasts), the silent
  no-op for exempt ids dispatched programmatically, the per-command `log.info` + `record_breadcrumb`, and crucially the
  **await semantics per arm** (see next rule). This is a pure restructure: zero behavior delta. The M1 characterization
  suite is the proof.
- **Await semantics are CONTRACT, pinned per id (the MCP ack-timing rule).** Today the dispatch promise resolves at
  different points per arm. Two ids resolve only AFTER their async work completes because an MCP round-trip awaits the
  dispatch promise before acking (`mcp-listeners.ts`): `nav.openUnderCursor` (`await openItemUnderCursor()`,
  command-dispatch.ts:461) and `cursor.moveTo` (`await moveCursor(pane, to)`, :468). The adapter's
  `await dispatch(...); emit('mcp-response', { ok: true })` only fires `ok: true` once that await resolves; an exception
  propagates to its `try/catch` → `ok: false`. **The handler signature MUST preserve each arm's await-vs-`void` decision
  EXACTLY.** A handler that `void`s a promise the switch `await`ed (or vice versa) silently breaks the ack timing with
  no compile error. M1 pins the resolve point for every id; M2's handlers must reproduce it.
- **Shared per-dispatch context resolved ONCE, evaluation semantics identical.** The switch reads
  `explorerRef = ctx.getExplorer()` exactly once per dispatch (command-dispatch.ts:200), then every arm uses that one
  reference. The handler record must preserve "one `getExplorer()` per dispatch," NOT "one per handler that needs it."
  Design the handler to receive a pre-resolved per-dispatch context object (`explorerRef`, `ctx`, `dispatchArgs`, and
  the shared local helpers) so a helper called twice doesn't re-read the explorer. Grouped arms
  (`view.zoom.set75|set100|set125|set150`, `sort.byName|byExtension|...`) keep ONE shared body referenced from each id's
  record entry, never copy-pasted.
- **The preamble stays IN ORDER, OUTSIDE the record.** The dispatch core runs, before any handler lookup: (1)
  `handleTextRegionShortcut(id)` text-region intercept → return (A7, command-dispatch.ts:206); (2) `log.info(id)`
  (:213); (3) `record_breadcrumb` invoke (:215); (4) `ctx.dialogs.showCommandPalette(false)` (:219); (5)
  `blockedByCapabilities(id, explorerRef)` → return (:224). Only THEN the record lookup. This order is pinned by M1 and
  must not move. The text-region intercept and capability guard are NOT handlers; they're pre-dispatch guards (A7/A6).
- **Exempt ∪ handled = CommandId, disjoint.** The `DispatchExemptId` union is the ONLY maintained list. A type-level
  check proves `Exclude<CommandId, DispatchExemptId>` is exactly the record's key set (no missing handler, no handler
  for an exempt id) AND that `DispatchExemptId ⊆ CommandId`. Adding a command then forces either a record entry (compile
  error until added) or an explicit exemption (a deliberate, documented choice). No silent fall-through.

## The exempt-id families (the `DispatchExemptId` union)

Three families, 20 ids, each documented with WHY. Verified by grep (2026-06-05, this worktree): these are exactly the
ids in `COMMAND_IDS` with no `case` in the switch today. An unmatched dispatch falls off the switch end and silently
no-ops; the record preserves that silence (see § PR3 decision).

```ts
/**
 * Ids that are registered (for the shortcuts-rebinding UI) but deliberately have
 * NO dispatch handler. Three families; see each member's comment for WHY. This is
 * the ONLY maintained exemption list — a type check proves
 * Exclude<CommandId, DispatchExemptId> equals the handler record's keys.
 */
type DispatchExemptId =
  // Family 1 — Native-menu-owned. macOS PredefinedMenuItems (terminate:, hide:,
  // hideOtherApplications:, unhideAllApplications:) run these via native selectors.
  // A JS handler would double-fire alongside the native one (registry § Decision).
  | 'app.quit'
  | 'app.hide'
  | 'app.hideOthers'
  | 'app.showAll'
  // Family 2 — Per-keystroke, P2-protected. Live path is handleKeyDown → FilePane;
  // these NEVER ride the bus (per-keypress lookup + log + breadcrumb IPC = perf
  // regression). Registered only so the rebinding UI can show/edit their shortcuts.
  // ❌ DO NOT add handlers — that is a P2 violation, not a completion.
  | 'nav.up'
  | 'nav.down'
  | 'nav.left'
  | 'nav.right'
  | 'nav.firstInFull'
  | 'nav.lastInFull'
  // Family 3 — Component-scoped. Handled inside the component that owns the modal /
  // sub-view (CommandPalette, VolumeChooser, NetworkBrowser, ShareBrowser, the
  // context menu), via its own keydown handler — not the global dispatch spine.
  // Registered for the rebinding UI.
  | 'palette.up'
  | 'palette.down'
  | 'palette.execute'
  | 'palette.close'
  | 'volume.select'
  | 'volume.close'
  | 'network.selectHost'
  | 'share.back'
  | 'share.selectShare'
  | 'file.contextMenu'
```

## Handler signature + record type – seam shapes (David reviews these personally)

The seam-defining shapes the M2 agent builds. Marked for David's review (the master § Verification cadence: David
reviews each phase's seam — here, the handler signature + the exempt-union type check).

```ts
/**
 * Per-dispatch context, resolved ONCE in the dispatch core before the record
 * lookup. `explorerRef` is read once per dispatch (NOT per handler), preserving
 * the switch's evaluation semantics. Handlers read args off `dispatchArgs` with
 * the same single cast the switch uses today (the public generic already
 * type-checked the payload at the call site).
 */
interface CommandHandlerContext {
  explorerRef: ExplorerAPI | undefined
  ctx: CommandDispatchContext
  dispatchArgs: CommandArgs[CommandId] | undefined
}

/**
 * A handler may be sync or async. The dispatch core does `await handler(hctx)`
 * uniformly — which is byte-identical to the switch for BOTH arms: an arm that
 * `return`s synchronously (sync handler) and an arm that `await`s before
 * returning (async handler resolving after its work). The await-vs-void decision
 * lives INSIDE each handler body, exactly as the switch case had it — `void`-ing
 * a fire-and-forget promise stays `void`, `await`-ing a round-trip stays `await`.
 */
type CommandHandler = (hctx: CommandHandlerContext) => void | Promise<void>

type CommandHandlerRecord = Record<Exclude<CommandId, DispatchExemptId>, CommandHandler>
```

Notes binding the shape to reality:

- **`await handler(hctx)` in the core is uniform and correct.** Awaiting a `void` return is a no-op (resolves the same
  tick), so the sync arms keep resolving synchronously-equivalent; the two round-trip arms keep resolving after their
  inner await because the handler body itself `await`s. The contract is: **the handler body, not the core, decides
  resolve timing** — it carries over each case's exact `await`/`void`. M1 pins which ids resolve late
  (`nav.openUnderCursor`, `cursor.moveTo`) so M2 can't accidentally normalize them.
- **Why `await handler(hctx)` is tick-safe (don't second-guess it).** `handleCommandExecute` is ALREADY `async`
  (command-dispatch.ts:191), and `await handler(hctx)` still INVOKES the handler synchronously — a JS async function
  body runs to its first `await` before suspending — so a sync handler's side effects all run in the SAME tick, before
  any suspension, byte-identical to today's sync `switch` arms. This is what keeps the ordering-sensitive caller
  correct: `mcp-listeners.ts:172` does `applySearchPrefill(prefill)` then `void dispatch(searchOpenCommand)`, relying on
  the `search.open` handler flipping `showSearchDialog(true)` SYNCHRONOUSLY (before any `await`) so the dialog reads the
  just-applied prefill on open. The synchronous `ctx.dialogs.show*` flips in the dialog-opener handlers must stay in the
  handler body's synchronous prefix, exactly as the switch arms had them.
- **`dispatchArgs` typing.** The record value type is the loose `CommandHandler`; per-handler the arg payload is read
  with the same `dispatchArgs as CommandArgs['view.setMode']` cast the switch uses today (command-dispatch.ts:301). The
  public `handleCommandExecute<K>` signature is UNCHANGED — it still arg-checks per command at the call site; only the
  internal dispatch mechanism (switch → record) changes. **Do not widen or re-type the public entry point** —
  `mcp-listeners.ts`'s `CommandDispatch` type and every test depend on it verbatim.
- **The exempt no-op path.** After the preamble, the core looks up `handlerRecord[id]`. For an exempt id, `id` is not a
  key of the record. The lookup is typed as `Exclude<CommandId, DispatchExemptId>`, so the core must narrow: `id` widens
  to `CommandId`; index with a guarded read
  (`const handler = (handlerRecord as Partial<Record<CommandId, CommandHandler>>)[id]`) and `if (!handler) return` to
  reproduce the silent no-op. The type check (below) guarantees the only ids hitting that `return` are the 20 exempt
  ones — so the runtime guard is belt-and-suspenders, not a real fall-through risk.
- **The type-level exempt-union check (the durable guard).** A compile-time assertion in a test file (the
  `command-registry.test.ts` `@ts-expect-error` precedent) that (a) `DispatchExemptId` extends `CommandId`, and (b) the
  record's keys are exactly `Exclude<CommandId, DispatchExemptId>` — the `Record<Exclude<…>, …>` type already forces (b)
  at the record's definition site (a missing key fails to compile, an extra key fails to compile). Add a runtime
  set-equality test too (mirroring `command-registry.test.ts`'s tuple↔registry test):
  `keys(record) ∪ DISPATCH_EXEMPT_IDS` equals `COMMAND_IDS`, disjoint. The runtime list `DISPATCH_EXEMPT_IDS` (an
  `as const` tuple) backs both the type and the test.

## Handler grouping (decided from the actual cases)

The 89 dispatchable ids split into cohesive family modules under a new `routes/(main)/command-handlers/` directory. Each
module exports a `Partial<CommandHandlerRecord>` (or a plain object of `CommandHandler`s keyed by its ids); the dispatch
core spreads them into one `CommandHandlerRecord`. Grouping chosen by reading every case; counts are id-counts (grouped
arms count once per id):

| Module                | Ids | Members                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------- | --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `app-dialog-handlers` | 12  | `app.commandPalette`, `search.open`, `nav.goToPath`, `app.settings`, `app.about`, `app.licenseKey`, `help.sendErrorReport`, `app.checkForUpdates`, `cmdr.openOnboarding`, `about.openWebsite`, `about.openUpgrade`, `about.close` (the selection-dialog openers `selection.selectFiles`/`selection.deselectFiles` live in `selection-handlers`, not here)                                                                                                                                                     |
| `view-handlers`       | 10  | `view.showHidden`, `view.briefMode`, `view.fullMode`, `view.setMode`, and the 6 zoom ids (`view.zoom.set75/100/125/150` as one shared body, `view.zoom.in`, `view.zoom.out`) — `showZoomToast` moves here as a module-local helper                                                                                                                                                                                                                                                                            |
| `pane-handlers`       | 7   | `pane.switch`, `pane.swap`, `pane.leftVolumeChooser`, `pane.rightVolumeChooser`, `pane.copyPathLeftToRight`, `pane.copyPathRightToLeft`, `pane.refresh`                                                                                                                                                                                                                                                                                                                                                       |
| `tab-handlers`        | 8   | `tab.new`, `tab.close`, `tab.reopen`, `tab.next`, `tab.prev`, `tab.togglePin`, `tab.closeOthers`, `tab.mcpAction`                                                                                                                                                                                                                                                                                                                                                                                             |
| `nav-handlers`        | 11  | `nav.open`, `nav.parent`, `nav.back`, `nav.forward`, `nav.home`, `nav.end`, `nav.pageUp`, `nav.pageDown`, `nav.openUnderCursor` (await), `cursor.moveTo` (await), `cursor.scrollTo`                                                                                                                                                                                                                                                                                                                           |
| `sort-handlers`       | 9   | `sort.byName/byExtension/bySize/byModified/byCreated`, `sort.ascending`, `sort.descending`, `sort.toggleOrder`, `sort.set`                                                                                                                                                                                                                                                                                                                                                                                    |
| `file-handlers`       | 18  | `file.view`, `file.rename`, `file.edit`, `file.copy`, `file.move`, `file.newFolder`, `file.newFile`, `file.delete`, `file.deletePermanently`, `dialog.confirm`, `file.showInFinder`, `file.copyPath`, `file.copyCurrentDirectoryPath`, `file.copyFilename`, `file.quickLook`, `file.getInfo`, `cloud.makeOffline`, `cloud.removeDownload` (the cloud pair are `getFileAndPathUnderCursor`-then-act arms, identical shape to the file copy\*/getInfo arms — they share the `withEntryUnderCursor` helper here) |
| `clipboard-handlers`  | 4   | `edit.copy`, `edit.cut`, `edit.paste`, `edit.pasteAsMove` (the `activeElement` input-vs-file branches stay verbatim)                                                                                                                                                                                                                                                                                                                                                                                          |
| `selection-handlers`  | 7   | `selection.toggle`, `selection.toggleAndDown`, `selection.selectAll`, `selection.deselectAll`, `selection.mcpSelect`, `selection.selectFiles`, `selection.deselectFiles`                                                                                                                                                                                                                                                                                                                                      |
| `misc-handlers`       | 3   | `downloads.goToLatest`, `network.refresh`, `volume.selectByName` (small singletons; fold into nav/pane if a module feels thin — decide at M2)                                                                                                                                                                                                                                                                                                                                                                 |

That's 89 across nine modules (the exact module count flexes if `misc` folds in — the M2 agent decides the final
boundary; the constraint is "no module is a junk drawer and the dispatch core ends small"). The dispatch core
(`command-dispatch.ts`: preamble + record assembly + lookup + the three pre-dispatch helpers `activeTextRegion` /
`handleTextRegionShortcut` / `blockedByCapabilities`) lands well under 400 lines, dissolving the `file-length` warning
without an allowlist entry.

**Cross-module shared helpers.** `showZoomToast` is view-only → moves into `view-handlers`. `activeTextRegion`,
`handleTextRegionShortcut`, `blockedByCapabilities` stay in the dispatch core (they're pre-dispatch guards, not
handlers). The `getFileAndPathUnderCursor`-then-act pattern (file.edit / showInFinder / copyPath / copyFilename /
getInfo / cloud.makeOffline / cloud.removeDownload) repeats — extract one small `withEntryUnderCursor(hctx, fn)` helper
in `file-handlers` rather than re-reading `explorerRef?.getFileAndPathUnderCursor()` in each (keeps the "one read"
discipline within the module). The cloud pair lives in `file-handlers` for this reason (same arm shape), not in
`selection-handlers`.

## Fresh grep (run 2026-06-05, this worktree)

- `command-dispatch.ts` is **815 lines** (warn threshold 800; NO allowlist entry — it's a pure warning today). The
  `switch (id)` spans lines 227–814 with **89 `case` labels** over **109 `COMMAND_IDS`**, NO `default`. An unmatched
  dispatch falls off the switch end → silent no-op.
- **20 ids have no case** (grep-verified): exactly the three families above (`comm -23 ids cases` → the 20-line list;
  `comm -13` → empty, so every case maps to a real id).
- **`// eslint-disable-next-line complexity`** sits at command-dispatch.ts:190 above `handleCommandExecute` — the switch
  trips the cognitive-complexity lint. The record conversion REMOVES the need for it: the core becomes a small lookup,
  and each handler is below the threshold. Drop the disable in M2 (don't carry it forward "just in case").
- **Await-before-return arms (the ack-timing contract), grep-confirmed:** `tab.close` (await close + await window close,
  :380/:383), `nav.openUnderCursor` (:461), `cursor.moveTo` (:468), `downloads.goToLatest` (:480), `file.edit` (:551),
  `file.showInFinder` (:601), `file.copyPath` (:609), `file.copyCurrentDirectoryPath` (:617), `file.copyFilename`
  (:625), `file.quickLook` (await close :647 / await open :658), `file.getInfo` (:665), `cloud.makeOffline` (:674),
  `cloud.removeDownload` (:686), `edit.paste` (await readClipboardText :786), `about.openWebsite` (:804),
  `about.openUpgrade` (:808). Every OTHER explorer-driving arm `void`s its promise (fire-and-forget). The TWO that the
  MCP round-trip depends on are `nav.openUnderCursor` and `cursor.moveTo` — these MUST stay `await` so the adapter acks
  on real completion. The rest's await is local (it gates a follow-up line, e.g. `tab.close`'s window close, or just
  surfaces a rejection) — preserve each verbatim regardless.
- **Existing test scaffolding to build on:** `command-dispatch.test.ts` already mocks `getAppLogger`, `invoke`
  (breadcrumb), `getFocusedPaneVolumeId` / `getFocusedPanePath`, `volume-store`, and `addToast`, and builds a partial
  `ExplorerAPI` via `makeCtx(explorer: Partial<ExplorerAPI>)`. It pins `view.setMode` arg routing and the full
  `blockedByCapabilities` matrix (search-results blocks + toasts, local allows, network silent-falls-through). The M1
  suite EXTENDS this file's harness — same mocks, same `makeCtx` — to cover every dispatchable id.

### Deviations / discoveries

1. **815, not "~520."** The Phase-2 plan's M1 cited `handleCommandExecute` at ~520 lines and the file at line 170 for
   the signature; the file grew (more MCP per-pane commands, the typed-args refactor landed) to 815, signature at
   line 191. Re-grep at M2 start (PR4).
2. **No `default` arm, confirmed.** The switch ends at `about.close` (:811–813) and closes at :814. The "silent no-op
   for unmatched id" is structural (fall-off-the-end), not an explicit `default: return`. The record reproduces it via
   the `if (!handler) return` guard. M1 must pin this: dispatching an exempt id today is a no-op with NO toast, NO
   throw, NO log-beyond-the-preamble — and the preamble (`log.info` + breadcrumb) STILL fires for exempt ids (they reach
   the preamble before the switch). The record preserves this: preamble runs, then the no-op guard returns. Pin "exempt
   id → preamble fires, then silent return" explicitly.
3. **The preamble fires for exempt ids too.** Because `log.info(id)` and `record_breadcrumb` run BEFORE the switch
   (:213/:215), dispatching e.g. `palette.up` through `handleCommandExecute` logs it and breadcrumbs it, then no-ops.
   This is current behavior (an exempt id reaching dispatch is already unusual — they're component-handled — but the
   path exists). PR3 keeps it byte-identical: don't move the preamble after the lookup to "skip logging no-ops."
4. **`view.showHidden` early-returns on no explorer BEFORE the toast/side-effect** (`if (!explorerRef) return`, :277).
   Several arms have arm-local guards like this; the handler bodies carry them verbatim. Not all arms use
   `explorerRef?.` — some read it unconditionally after an early guard. Preserve each arm's exact guard shape.
5. **`tab.close` does a dynamic `import('@tauri-apps/api/window')`** inside the arm (:382). The handler keeps the
   dynamic import inside its body (don't hoist to module top — it's lazy by design, only on the last-tab branch).
6. **`selection.selectAll` and `edit.copy` have input-focus branches**
   (`document.activeElement instanceof HTMLInputElement`, :707/:738) that run INSIDE the arm, AFTER the text-region
   intercept already let them through (the intercept only fires for `.error-pane`/`[data-text-region]` selections,
   :135–156). The two guards are distinct layers: `handleTextRegionShortcut` (pre-dispatch, A7) and the in-arm
   `activeElement` check (handler-local). Both must survive — they cover different focus contexts (a selectable error
   region vs a focused `<input>`). Don't merge them.

## Milestones

Three milestones. Each is atomic (PR1): a complete, green state on `main`. Gates per milestone: `--fast` continuously
during work; full `pnpm check` + `--check desktop-e2e-linux` before the milestone commit. End of M3: `--include-slow`
(macOS Playwright + `rust-tests-linux`), then watch CI to green before merging. Import-cycle rule: the new
`command-handlers/` modules import `ExplorerAPI` (the interface), `$lib/commands` types, and the leaf helpers the arms
call (e.g. `tauri-commands`, `settings`, `toast`) — exactly what `command-dispatch.ts` imports today; they must NOT
import `+page.svelte` or each other circularly. The dispatch core imports the handler modules; the handler modules never
import the core. `import-cycles` (fast lane) fires if this inverts.

### M1 – Characterization suite FIRST (pin every id, red-green-ready)

**Scope:** new headless tests pinning, for EVERY dispatchable id, the call pattern against a fully-mocked
`CommandDispatchContext` / `ExplorerAPI` — which method(s) fire, with what args, AND the await semantics. NO production
code change. Built on the existing `command-dispatch.test.ts` harness (extend it or add a sibling
`command-dispatch.characterization.test.ts`). These tests are written so they survive the M2 switch→record conversion
unchanged (they drive the public `handleCommandExecute`, which doesn't change signature) — green against the switch
today, green against the record tomorrow.

**Intentions:**

- **Table-driven where the arm is a simple delegate.** Most arms are `explorerRef?.someMethod(args)` → one mock call.
  Build a table of `{ id, args?, expect: (explorer, dialogs, toast) => void }` and loop. The `view.setMode` test
  (already present) and the `blockedByCapabilities` matrix are the existing model. The table covers the ~70 simple
  delegate arms in one structure.
- **Bespoke tests for the arms with branches / toasts / dynamic imports:** `view.showHidden` (toggle +
  `syncMenuShowHidden`, no-explorer early return), the four zoom-preset ids + zoom in/out (`getSetting`/`setSetting` +
  `showZoomToast` message text per direction — pin the exact strings), `tab.new`/`tab.close`/`tab.reopen` (the
  toast/last-tab branches), `selection.selectAll` + `edit.copy`/`edit.cut`/`edit.paste` (the `activeElement` input
  branches — mock `document.activeElement`), `file.quickLook` (the dispatch-guard + open/close toggle),
  `cloud.makeOffline`/`removeDownload` (the try/catch error toast), `about.openWebsite`/`openUpgrade` (the exact URLs).
- **Pin the await semantics explicitly for the two round-trip ids.** A test that asserts the dispatch promise for
  `nav.openUnderCursor` does NOT resolve until the mocked `openItemUnderCursor` resolves, and the same for
  `cursor.moveTo` / `moveCursor`. Use a deferred promise the test controls:
  `handleCommandExecute('nav.openUnderCursor', ctx)` returns a promise that's still pending while the mock is pending,
  and resolves once the mock resolves. This is the guard against an M2 handler that `void`s where the switch `await`ed
  (which would break the MCP ack with no compile error). Add a weaker "resolves" assertion for the other await arms
  (they don't gate an MCP ack, but pinning catches an accidental semantics flip).
- **Pin the preamble order + the exempt no-op.** A test that mocks `log.info`, `invoke` (breadcrumb), and
  `showCommandPalette`, dispatches a normal id, and asserts the call ORDER: text-region check → `log.info(id)` →
  `record_breadcrumb` → `showCommandPalette(false)` → `blockedByCapabilities` → handler. A second test: dispatch each of
  the 20 exempt ids → assert `log.info` + breadcrumb fired (preamble runs) AND no `ExplorerAPI` method, no toast, no
  throw (silent no-op). A third: a text-region-active `edit.copy`/`selection.selectAll` returns BEFORE `log.info` (the
  A7 early bail) — extend the existing intercept coverage if not already pinned.
- **Pin `blockedByCapabilities` is unchanged** — the existing matrix tests already do this; keep them.

**Test plan:** extend `command-dispatch.test.ts` (or a sibling characterization file) with the id table + the bespoke
arms + the await-timing + preamble-order + exempt-no-op tests. Mock `document.activeElement` via the jsdom env for the
input-branch arms. New/extended test file → covered by the 70% `src/**` gate; the routes file itself has no coverage
backstop, so this suite IS the behavioral guard for M2 (master § PR2).

- **New `vi.mock` hoists M1 needs (beyond the existing harness).** The current `command-dispatch.test.ts` mocks only
  `$lib/logging/logger`, `@tauri-apps/api/core` (invoke), `$lib/file-explorer/pane/focused-pane-reads`,
  `$lib/stores/volume-store.svelte`, and `$lib/ui/toast`. The characterization suite covers arms that import more
  module-level deps, so add hoisted `vi.mock`s (all are static imports in `command-dispatch.ts`, so hoist cleanly) for:
  `$lib/settings` (`getSetting`/`setSetting` — zoom arms), `$lib/shortcuts` (`getEffectiveShortcuts` — used inside
  `showZoomToast`), `$lib/settings/settings-window` (`openSettingsWindow` — `app.settings`), `$lib/tauri-commands` (the
  whole barrel: `openInEditor`, `showInFinder`, `copyToClipboard`, `quickLookOpen`/`quickLookClose`, `getInfo`,
  `openExternalUrl`, `syncMenuShowHidden`, `readClipboardText`, `cloudMakeAvailableOffline`/`cloudRemoveDownload`),
  `$lib/updates/updater.svelte` (`runMenuTriggeredCheck` — `app.checkForUpdates`),
  `$lib/error-reporter/error-report-flow.svelte` (`openErrorReportDialog` — `help.sendErrorReport`), and
  `$lib/file-explorer/quick-look/quick-look-state.svelte` (`quickLookState` + the two dispatch-guard fns —
  `file.quickLook`). `$lib/downloads/go-to-latest` (`goToLatestDownload`) is also static-imported; mock it for the
  `downloads.goToLatest` arm.

**DONE:** every dispatchable id has a characterization assertion (call pattern + args); the two round-trip ids' await
timing pinned with deferred-promise tests; the preamble order pinned; the 20 exempt ids pinned as
preamble-then-silent-no-op; all green against the CURRENT switch; `--fast` + full suite + `desktop-e2e-linux` green;
ZERO production change.

### M2 – Convert: handler record + `DispatchExemptId` union + file split (the seam)

**Scope:** introduce `DispatchExemptId` + the `DISPATCH_EXEMPT_IDS` tuple + the type-level/runtime exempt-union checks;
build the `command-handlers/` modules (one per family, § grouping); rewrite the `command-dispatch.ts` core to assemble
the record and look up by id; DELETE the 89-case switch and the `// eslint-disable complexity`. This is the
seam-defining commit — **flag for David's review** (the handler signature, the per-dispatch context, the exempt-union
type check). Lands as ONE commit: the record can't half-exist (a `Partial` record during transition would leave some ids
on the switch and some on the record — two parallel dispatch paths, a PR1/A4-style violation, and the
`Record<Exclude<…>>` type wouldn't compile until ALL non-exempt ids have handlers). So M2 is atomic by construction; say
so. (Investigated: an incremental `Partial<CommandHandlerRecord>`-with-switch-fallback would technically stay green
per-step, but it means the switch and the record BOTH dispatch live ids simultaneously — exactly the parallel-path the
master forbids; the bisectability win the Phase-2 plan wanted from "isolated milestone" is satisfied by M2 being one
clean commit, not by salami-slicing it.)

**Intentions:**

- **Build the per-dispatch context once.** The core, after the preamble, constructs
  `const hctx = { explorerRef, ctx, dispatchArgs }` (explorerRef already read once at :200) and calls
  `await handler(hctx)`. No handler re-reads `getExplorer()`.
- **Each handler body is the OLD case body verbatim**, reading `explorerRef` / `ctx` / `dispatchArgs` from `hctx`.
  Grouped arms become ONE shared body referenced by each id's key (e.g. the zoom-preset handler is one function the four
  preset ids point at; or four thin entries calling one `applyZoomPreset(hctx, preset)` — either way, one body). The
  `sort.by*` arms each call `setSortColumn('name'|'extension'|...)` — keep them as five tiny distinct handlers (the arg
  differs) or one parameterized helper; the M2 agent picks, the constraint is no copy-paste of logic.
- **Preserve every arm's await/void EXACTLY** (the grep list above is the checklist). The two round-trip handlers
  (`nav.openUnderCursor`, `cursor.moveTo`) are `async` and `await` their inner call; M1's deferred-promise tests are the
  guard.
- **The exempt no-op.** The core's lookup `const handler = (record as Partial<Record<CommandId, CommandHandler>>)[id]`;
  `if (!handler) { return }`. The type guarantees only the 20 exempt ids hit it. Add a short comment pointing at
  `DispatchExemptId`'s doc so the silent return reads as intentional.
- **The type + runtime checks land WITH the record** (PR2 — new seam, tests in the same milestone): the
  `Record<Exclude<CommandId, DispatchExemptId>, CommandHandler>` type forces completeness at compile time; a
  `command-handler-record.test.ts` asserts (runtime) `new Set([...keys(record), ...DISPATCH_EXEMPT_IDS])` equals
  `new Set(COMMAND_IDS)` and that the two sets are disjoint, plus a `@ts-expect-error` that a bogus exempt id (one not
  in `CommandId`) fails. This is the "add-a-command forces a decision" guard.
- **Keep the public `handleCommandExecute<K extends CommandId>(commandId, ctx, ...args)` signature byte-identical.**
  Only the internal mechanism changes. `mcp-listeners.ts`, `+page.svelte`, the palette `onExecute`, and every test call
  it unchanged.
- **`file-length`:** after the split, the core is <400 lines (no allowlist needed); each handler module is small. If any
  module somehow lands over 800, that's a sign the grouping is wrong — re-split, don't allowlist (file-length-allowlist
  rule: never loosen without consent).

**Landmines:**

- **Await flip (the silent one).** The ONLY way M2 breaks the MCP ack with no compile error is `void`-ing where the
  switch `await`ed. M1's deferred-promise tests for `nav.openUnderCursor` + `cursor.moveTo` are the gate; run them red
  first (temporarily `void` the handler, confirm the test fails) to prove they bite.
- **Double `getExplorer()`.** A handler calling `ctx.getExplorer()` itself (instead of reading `hctx.explorerRef`) would
  re-read per handler — semantically different if the explorer ref could change mid-dispatch (HMR). Forbid it: handlers
  read `hctx.explorerRef`. Grep the new modules for `getExplorer(` → should be zero (only the core calls it).
- **Preamble drift.** The text-region intercept, `log.info`, breadcrumb, `showCommandPalette(false)`, and
  `blockedByCapabilities` must stay in the core in that order, before the lookup. M1's order test guards it.
- **Grouped-arm copy-paste.** `view.zoom.set75|100|125|150` and the `sort.by*` set must share bodies, not duplicate. The
  Rust `jscpd` check scans Rust only, so duplication here is a reviewer gate — the A4-style "no copy-paste" review.

**Test plan:** the M1 characterization suite re-runs UNCHANGED against the record and stays green (the whole point — it
drives the public entry, which didn't change). The new `command-handler-record.test.ts` (completeness + disjointness +
the `@ts-expect-error`). Run the two round-trip await tests red-first (flip to `void`, see them fail) to prove the
guard. The MCP E2E (`mtp.spec.ts` round-trip specs for `mcp-open-under-cursor` / `mcp-move-cursor`) stays green — the
L1/L2 + ack-timing contract. Re-grep `case '` in `command-dispatch.ts` → ZERO (the switch is gone). Re-grep
`getExplorer(` in `command-handlers/` → zero.

**DONE:** the switch DELETED; the flat record over `Exclude<CommandId, DispatchExemptId>` live; `DispatchExemptId` + the
two completeness checks land; handlers split into family modules; the `complexity` disable removed;
`command-dispatch.ts` under the file-length warn threshold with NO allowlist entry; await semantics byte-identical
(round-trip tests green); `--fast` + full suite + `desktop-e2e-linux` green; **David reviews the seam.**

### M3 – Docs + sweep

**Scope:** docs updates reflecting the new dispatch shape + the add-a-command flow; a final re-grep sweep;
`--include-slow`

- CI-green before merge.

**Intentions:**

- **`routes/(main)/CLAUDE.md`:** rewrite the dispatch description — `handleCommandExecute` is now a small core (preamble
  - record lookup) over a flat `CommandHandlerRecord`; handlers live in `command-handlers/` grouped by family; document
    the three exempt families + WHY each (native-menu-owned, P2 per-keystroke, component-scoped) and the
    `DispatchExemptId` union as the single maintained list. Update the "Adding a user-facing action" section: step 4
    becomes "add a handler to the family module's record (the compiler holds you to it via
    `Record<Exclude<CommandId, DispatchExemptId>, …>`) — OR add the id to `DISPATCH_EXEMPT_IDS` with a documented WHY."
    Update the file-length gotcha (it currently says ">670 lines, extract helpers" — the core is now small; the gotcha
    becomes "handlers go in `command-handlers/`, not the core").
- **`lib/commands/CLAUDE.md`:** § "Adding a command" gains the handler-record step: "Add the handler to the family
  module's record in `routes/(main)/command-handlers/`. A missing handler is a COMPILE error (the record is keyed by
  `Exclude<CommandId, DispatchExemptId>`); an intentionally-handlerless command goes in `DISPATCH_EXEMPT_IDS` with a
  reason." Replace the line that says "the `case` then reads the payload from `dispatchArgs`" → "the handler reads the
  payload from `hctx.dispatchArgs`." Update the § "Typed ids and the dispatch boundary" paragraph that currently says
  "the flat `Record<CommandId, Handler>` conversion is a deferred, optional follow-up" — it's DONE now; describe the
  record + the exempt union.
- **AGENTS.md:** the add-an-action line (AGENTS.md:227-228, "...the entry in `command-registry.ts`, and the handler case
  in `handleCommandExecute` in `routes/(main)/command-dispatch.ts`") → "...and the handler in `command-handlers/` (the
  record is keyed by `Exclude<CommandId, DispatchExemptId>`, so a missing one is a compile error; an intentionally
  handlerless command goes in `DISPATCH_EXEMPT_IDS`)." Replace the "handler case in `handleCommandExecute`" phrasing —
  it describes the deleted switch.
- **`docs/architecture.md`:** the `commands/` row (architecture.md:34) mentions "the one typed `handleCommandExecute`
  spine" — append that handlers are a flat family-grouped record with a compile-time completeness guarantee, AND correct
  its stale "~115 ids" to 109 (actual `COMMAND_IDS` count, grep-verified).
- **`docs/specs/explorer-command-bus-phase2-plan.md`:** mark § "M6 (optional)" as DONE, linking this plan (the M6 it
  deferred is now executed here).

**Test plan:** docs-only changes covered by `--fast` (`oxfmt`, `claude-md-reminder`, `changelog-links`). The full
suite + `desktop-e2e-linux` re-run (no code change, but confirm nothing regressed from M2's commit). Phase-end
`--include-slow` (macOS Playwright + `rust-tests-linux`) + watch CI to green before merging to `main`.

**DONE:** all four docs (+ the Phase-2 plan's M6 marker) updated; re-grep `case '` in the routes dir → zero; the
add-a-command flow documents the record + the exempt union; `--include-slow` green; CI green.

## PR3 decision — keep the silent no-op for exempt ids (no dev-warn)

Today an exempt id dispatched programmatically falls off the switch and silently no-ops (after the preamble's
log+breadcrumb). **Decision: keep it byte-identically silent.** A dev-mode `console.warn` ("dispatched an exempt id")
was considered and rejected as scope creep + a PR3 risk: (1) PR3 mandates byte-identical user-visible behavior, and a
warn — even dev-only — is a new side effect the characterization tests would have to encode and could leak into E2E log
assertions; (2) the type system ALREADY makes the real failure mode (adding a command and forgetting its handler) a
compile error, so a runtime warn guards nothing the compiler doesn't; (3) the exempt ids reaching dispatch at all is a
non-event (they're component-handled or native-handled) — warning on a deliberate, documented exemption is noise. The
`if (!handler) return` guard stays silent. If a future debugging need arises, a warn can be added behind
`import.meta.env.DEV` as its own change with its own test — not folded into this refactor.

## Invariants this refactor must honor

- **P2** (loud) — the six per-keystroke `nav.*` ids stay OFF the bus, members of `DispatchExemptId` BY DESIGN; no
  handler is ever added for them. Documented per family so nobody "completes the migration."
- **A3** — dispatch ids stay `CommandId`-typed end to end; the record is keyed by
  `Exclude<CommandId, DispatchExemptId>`, a closed subset; no string-literal ids (`cmdr/no-raw-command-dispatch`
  unchanged).
- **A6** — `blockedByCapabilities` (capability guard, not a volume-id string compare) stays a pre-dispatch guard in the
  core, unchanged.
- **A7** — the text-region intercept (`handleTextRegionShortcut`) stays in front of the record lookup, before
  `log.info`, verbatim.
- **A8** — no new components; the `command-handlers/` modules are plain `.ts` handler objects + pure helpers.
- **PR1** — M1 (tests) and M3 (docs) are independently green; M2 is ONE atomic commit (the record can't half-exist — the
  `Record<Exclude<…>>` type forbids a partial transition, and a switch+record dual path would be a parallel-dispatch
  violation).
- **PR3** — byte-identical user-visible behavior: toast copy, the silent exempt no-op, the preamble log+breadcrumb, and
  the per-arm await/void semantics (the two MCP round-trip ids resolve after their inner await, pinned by M1).
- **Ack-timing contract** — `nav.openUnderCursor` + `cursor.moveTo` handlers stay `async`+`await`; the adapter's
  `await dispatch(...)` → `mcp-response { ok }` only fires after the action settles. M1's deferred-promise tests guard
  it; the `mtp.spec.ts` round-trip E2E is the integration backstop.

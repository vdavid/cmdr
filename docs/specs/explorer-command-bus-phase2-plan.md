# Command bus – Phase 2 execution plan

Just-in-time execution plan for Phase 2 of the [explorer architecture refactor](explorer-architecture-plan.md). Read
that master spec first (§ Target architecture 2 "Typed command dispatch", § Invariants register, § Landmine register, §
Phase map → Phase 2). This plan adapts its five-milestone sketch to the code as it stands after Phase 1.

**Goal of the phase:** make `handleCommandExecute` the one typed dispatch spine for every entry path that resolves to a
user-intent command. Today seven entry paths reach the explorer: the dispatch switch already serves keyboard / palette /
native-menu `execute-command`, but the F-key bar (`handleFn*`), three native-menu side events (`menu-action`,
`view-mode-changed`, `menu-sort`), the 18 inbound MCP events, the Quick Look forwarding path, the Selection-dialog
`onCommand` prop chain, and the debug-panel events all bypass it or duplicate its cases. This phase routes them onto the
bus and types the registry end to end (`CommandId`, `CommandArgs`, typed `dispatch`, flat handler record) behind a new
`cmdr/no-raw-command-dispatch` ESLint rule.

## Loud sequencing rule (read before touching anything)

**Navigation-related commands route through the bus this phase but keep calling the OLD navigation entries until
Phase 3.** `nav.back` / `nav.forward` / `nav.parent` still call `explorerRef.navigate('back'|'forward'|'parent')` – they
route through the bus (routing), but the mechanism underneath is unchanged. The double touch (bus = routing,
`navigate()` = mechanism) is **intentional**. **One exception: `mcp-nav-to-path` stays entirely OFF the bus in Phase 2**
– its `string`-sentinel return (`navigateToPath` returns `string | Promise<void>`) can't pass through fire-and-forget
`dispatch`, so the adapter keeps calling `explorerRef.navigateToPath(pane, path)` directly and forwards the sentinel
verbatim. It only joins the bus in Phase 3 when `NavigateResult` types the refusal (see M4). A Phase-2 agent must NOT
reach into `DualPaneExplorer.navigateToPath` / `handleVolumeChange` / `applyPathChange`, the generation counters, or the
`string`-refusal sentinel. Those retire in Phase 3 (master § A4, L12). The Phase-2 contract is: the same `ExplorerAPI`
call the old path made still gets made, just reached through a typed handler instead of a `handleFn*` closure, a string
switch, or an `as {...}` cast.

**P2 is non-negotiable.** Arrow keys, type-to-jump chars, and type-to-jump reset keys NEVER route through the bus.
`DualPaneExplorer.handleKeyDown → FilePane` stays the direct path; `routePanelKey`'s in-pane type-to-jump intercept (L9)
stays a synthesized-KeyboardEvent path. The bus carries resolved commands only – routing a keystroke through it would
add a registry lookup + `log.info` + `record_breadcrumb` IPC per keypress. The Quick Look milestone touches
`routePanelKey`'s command-resolution seam only, never its per-keystroke jump mirror.

## Open question 2 – resolved (mcp-response stays in the adapter)

The `mcp-response` outbound reply channel (request-id round-trips for `mcp-nav-to-path`, `mcp-open-under-cursor`,
`mcp-move-cursor`) stays in `mcp-listeners.ts` as a thin transport adapter. The bus dispatches the intent; the adapter
owns the `requestId` correlation and the `emit('mcp-response', …)` reply. **Rationale:** the bus shouldn't know about
transport (master Open Q2 lean). `mcp-response` is not a command and gets no registry entry. The refusal-string forward
(`emit('mcp-response', { ok: false, error: result })` where `result` is the sync `string` from `navigateToPath`) is
preserved byte-identically – it's the Phase-3 `NavigateResult` work that types it, not this phase (L12).

## Fresh grep (run 2026-06-05, this worktree)

The registry and its consumers:

- `lib/commands/command-registry.ts:14` – `export const commands: Command[]` with `id: string` (the `Command` interface
  at `lib/commands/types.ts:20` has `id: string`). Currently 99 entries. Consumers: `getPaletteCommands()` (palette +
  fuzzy search), `updateLicenseCommandName()` (mutates `app.licenseKey` name in place – note: the typing must not freeze
  the array so this `.find().name =` write survives), and `lib/shortcuts/shortcut-dispatch.ts:45`
  `lookupCommand(shortcutString): string | undefined` (keyboard → command-id reverse map).
- `routes/(main)/command-dispatch.ts:170` – `handleCommandExecute(commandId: string, ctx)`: a ~520-line
  `switch (commandId)` with the `// eslint-disable complexity` at line 169. This is the future dispatch core.
  Pre-dispatch guards that stay in front, verbatim (A7): `handleTextRegionShortcut` (line 121, ⌘C/⌘A text-region
  intercept, runs BEFORE `log.info`), and `blockedBySearchResultsPane` (line 96, L10 toast guard, runs AFTER
  `log.info` + `record_breadcrumb`). Both branch on `commandId` already.

Entry paths that bypass or duplicate the dispatch:

| Entry path                    | Site                                                                                                   | Current shape                                                                                          |
| ----------------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| Global keydown                | `+page.svelte:368`                                                                                     | `lookupCommand(string)` → `handleCommandExecute(commandId)`. Already on the bus. A7 guards at 359/365. |
| Native menu `execute-command` | `+page.svelte:225`                                                                                     | `handleCommandExecute(commandId)`. Already on the bus (most menu items route here from Rust).          |
| Palette                       | `+page.svelte:881` `onExecute={handleCommandExecute}`                                                  | Already on the bus.                                                                                    |
| Selection-dialog key prop     | `FilePane.svelte:1874` → `DualPaneExplorer.svelte:1979` → `+page.svelte:927`                           | `onCommand?.(string)` chain → `handleCommandExecute`. **String-typed prop**, reaches the bus.          |
| F-key bar                     | `+page.svelte:774-811` `handleFn*` → `FunctionKeyBar.svelte` `on*` props                               | **Bypasses dispatch.** 9 `handleFn*` closures call `explorerRef` directly, duplicating `file.*`.       |
| `menu-action` (Rust)          | `FilePane.svelte:2401` `listen('menu-action')`                                                         | **In-pane listener.** Only `action === 'open'` → `handleNavigate(cursorEntry)`. Duplicates `nav.open`. |
| `view-mode-changed` (Rust)    | `DualPaneExplorer.svelte:966` `listen('view-mode-changed')`                                            | **In-component listener.** Per-pane `setPaneViewMode` + persistence. Duplicates `view.brief/fullMode`. |
| `menu-sort` (Rust)            | `mcp-listeners.ts:61` `listen('menu-sort')`                                                            | Native-menu event mis-filed among MCP listeners. `setSortColumn/setSortOrder`. Duplicates `sort.*`.    |
| Quick Look forwarding         | `DualPaneExplorer.svelte:1496` `routePanelKey` → `paneCommands.routePanelKey` (`pane-commands.ts:128`) | Synthesized KeyboardEvent; `file.quickLook` toggle lives in dispatch (`command-dispatch.ts:513`).      |
| Debug panel (dev only)        | `+page.svelte:304/308/312` `debug-inject-error` / `debug-reset-error` / `debug-trigger-transfer-error` | Direct `explorerRef.injectError/resetError/triggerTransferError`. Dev-only, not commands.              |
| MCP events                    | `mcp-listeners.ts` (17 mcp-\*) + `DualPaneExplorer.svelte:1012` `mcp-tab` (1)                          | 18 inbound events, ~15 `as {...}` casts in `mcp-listeners.ts`. See the MCP table below.                |

String-action sub-dispatchers to promote (master § Target arch 2, "No string-action sub-dispatchers survive"):

- `handleSelectionAction(action: string)` – `pane-commands.ts:171`, switch over
  `clear|deselectAll|selectAll|toggleAtCursor|toggleAtCursorAndMoveDown|selectRange`. Called from dispatch
  (`command-dispatch.ts:579/583/595/599`) and from `mcp-select` indirectly. Exposed on `DualPaneExplorer` at line 1543.
- `handleMcpSelect(pane, start, count, mode: string)` – `pane-commands.ts:329`, `mode` is `replace|add|subtract`. Called
  from `mcp-select` (`mcp-listeners.ts:90`). Exposed at `DualPaneExplorer.svelte:1786`.
- `handleMcpTabAction(pane, action: string, tabId?, pinned?)` – driven by `mcp-tab` (`DualPaneExplorer.svelte:1012`),
  `action` is `new|close|close_others|activate|reopen|set_pinned`.
- `confirmDialog(dialogType: string, onConflict?)` – `pane-commands.ts:21` → `dialog-state.svelte.ts:528`
  `confirmOpenDialog`, `dialogType` is `transfer-confirmation|delete-confirmation`. Driven by `mcp-confirm-dialog`
  (`mcp-listeners.ts:202`). Exposed at `DualPaneExplorer.svelte:1285`.
- `menu-sort` / `mcp-sort` `action`/`by`/`order` string params (`mcp-listeners.ts:61/72`).

The ESLint-rule model: `eslint-plugins/no-raw-tauri-invoke.js` (allowed-path fragments, `CallExpression` literal-string
match, `meta.type: 'problem'`). Registered under the `cmdr` plugin block in `eslint.config.js` (~line 232) alongside
`no-explorer-state-writes` and `no-error-string-match`. Mirror that wiring for `no-raw-command-dispatch`.

### Deviations from master spec

1. **18 inbound MCP events, not 17.** The master spec and § Caller map say "17 inbound `mcp-*` events." There are 17 in
   `mcp-listeners.ts`, **plus** `mcp-tab` handled inside `DualPaneExplorer.svelte:1012` (the spec's "MCP tab
   `action: string`" sub-dispatcher is this event's handler). Count the full 18; M4 must reach into `DualPaneExplorer`
   for `mcp-tab`, not just `mcp-listeners.ts`.
2. **`menu-sort` is a native-menu event, not MCP.** It lives in `mcp-listeners.ts:61` but is emitted by Rust
   `menu_handlers.rs:440` on Sort-menu clicks (the four shortcut-bound sort columns), not by the MCP server. It belongs
   conceptually with M3 (native menu) even though it physically sits in the MCP file. Decide its milestone by behavior:
   route it through the bus in M3, alongside `view-mode-changed`.
3. **Native menu is already mostly on the bus.** `menu.rs` routes most clicks through `execute-command` →
   `handleCommandExecute` already (the unified-dispatch decision predates this refactor). M3's real surface is the THREE
   exception events (`view-mode-changed`, `menu-sort`, `menu-action`) that emit directly instead of through
   `execute-command`, not the whole menu. Smaller than the sketch implies.
4. **The registry array is mutated at runtime, so `as const satisfies` on it is wrong.** `updateLicenseCommandName`
   (`command-registry.ts:638-643`) does `commands.find(...).name = …`. `as const satisfies readonly Command[]` makes
   every element property `readonly` at the **type** level (not just a runtime-freeze concern), so that write becomes a
   compile error. `getPaletteCommands(): Command[]` (line 633) and the shortcuts conflict-detector's mutable `Command[]`
   params would also fight a `readonly` registry. **Primary M1 approach: keep the registry as a mutable `Command[]` and
   derive `CommandId` from a SEPARATE `as const` id tuple**, with a guard (a `satisfies` check or a unit test asserting
   set-equality) that the tuple and the registry ids stay in sync. The earlier "it's only a type-level assertion so the
   write survives" framing conflated runtime vs type mutability and is retracted.
5. **`DualPaneExplorer.svelte` is ~2140 lines** (master § Goal cites 3318 pre-Phase-0). Line numbers above are
   indicative; re-grep at each milestone.

## MCP event inventory (fresh)

For M4. **18 inbound MCP events** (17 handled in `mcp-listeners.ts` + `mcp-tab` in `DualPaneExplorer.svelte:1012`), plus
`menu-sort` (native-menu, M3's scope, not MCP) for a total of 19 rows below. Note: `mcp-listeners.ts` holds 18
`listenTauri` calls (17 mcp-\* events + `menu-sort`). Classified by: maps to an existing registry command, needs a new
command/arg shape, or stays adapter-local.

| Event                    | Payload (current cast)                                    | Maps to                                                                                                                                                                                                                                                |
| ------------------------ | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mcp-open-search-dialog` | validated whitelist (no cast) → `applySearchPrefill`      | Adapter-local: prefill + `search.open`. Already the validating-parse precedent.                                                                                                                                                                        |
| `mcp-key`                | `{ key: string }`                                         | `nav.back`/`nav.forward` (GoBack/GoForward) else `sendKeyToFocusedPane`. Keep the key passthrough; route the two nav cases through the bus, keep them on OLD nav entries (sequencing rule).                                                            |
| `menu-sort`              | `{ action, value: string }`                               | Native menu → `sort.*` commands (M3, not M4).                                                                                                                                                                                                          |
| `mcp-sort`               | `{ pane, by, order: string }`                             | Needs a `sort.set` command with `{ column, order, pane }` args (per-pane, no current registry equivalent – the `sort.*` commands act on the focused pane).                                                                                             |
| `mcp-volume-select`      | `{ pane, name: string }`                                  | New command `volume.selectByName` `{ pane, name }`. Navigation-adjacent: keep calling `selectVolumeByName` (Phase 3 owns volume mechanics).                                                                                                            |
| `mcp-select`             | `{ pane, start, count, mode: string }`                    | New command `selection.mcpSelect` `{ pane, start, count, mode }`; promote `handleMcpSelect`'s `mode` to a literal union.                                                                                                                               |
| `mcp-nav-to-path`        | `{ pane, path, requestId? }`                              | **Stays OFF the bus in Phase 2** – the `string`-sentinel return can't pass through fire-and-forget `dispatch`. Adapter keeps calling `navigateToPath` directly (L12). `nav.toPath` registry entry + bus wiring lands in Phase 3 with `NavigateResult`. |
| `mcp-open-under-cursor`  | `{ requestId }`                                           | `nav.openUnderCursor` (or reuse `nav.open` with a focused-pane variant). Round-trip stays in adapter.                                                                                                                                                  |
| `mcp-move-cursor`        | `{ pane, to, requestId }`                                 | New command `cursor.moveTo` `{ pane, to }`. Round-trip stays in adapter. **L1/L2** (focus re-anchor + `whenLoadSettles`) live inside `moveCursor` – don't touch.                                                                                       |
| `mcp-scroll-to`          | `{ pane, index }`                                         | New command `cursor.scrollTo` `{ pane, index }`.                                                                                                                                                                                                       |
| `mcp-set-view-mode`      | `{ pane, mode: string }`                                  | `view.briefMode`/`view.fullMode` but per-pane → new `view.setMode` `{ pane, mode }` arg variant (the existing commands act on the focused pane).                                                                                                       |
| `mcp-refresh`            | `()`                                                      | New command `pane.refresh`.                                                                                                                                                                                                                            |
| `mcp-copy`               | `{ autoConfirm?, onConflict? }`                           | `file.copy` with optional `{ autoConfirm, onConflict }` args.                                                                                                                                                                                          |
| `mcp-move`               | `{ autoConfirm?, onConflict? }`                           | `file.move` with optional args.                                                                                                                                                                                                                        |
| `mcp-mkdir`              | `()`                                                      | `file.newFolder`.                                                                                                                                                                                                                                      |
| `mcp-mkfile`             | `()`                                                      | `file.newFile`.                                                                                                                                                                                                                                        |
| `mcp-delete`             | `{ autoConfirm? }`                                        | `file.delete` with optional `{ autoConfirm }` arg.                                                                                                                                                                                                     |
| `mcp-confirm-dialog`     | `{ type, onConflict? }`                                   | New command `dialog.confirm` `{ type, onConflict }`; promote `confirmOpenDialog`'s `dialogType` to a literal union.                                                                                                                                    |
| `mcp-tab`                | `{ action, pane, tabId?, pinned? }` (in DualPaneExplorer) | `tab.*` commands but with `{ pane, tabId?, pinned? }` args – the existing `tab.new`/`tab.close`/etc. act on the focused pane. Either add per-pane args or a `tab.mcpAction` command.                                                                   |

**Validating parses (M4):** every cast above gets a defensive parse per the `mcp-open-search-dialog` precedent
(`mcp-listeners.ts:27-45`): validate the discriminant strings against a whitelist, collapse unknowns to a safe default
or a silent skip, never `as`-cast a payload into a typed arg. The adapter parses the raw event payload into
`CommandArgs[K]` before `dispatch`; a malformed payload must not reach a handler.

## Milestones

Each milestone is atomic (add + migrate + delete the old path; PR1). Gates per milestone: `--fast` continuously during
work; full `./scripts/check.sh` + `--check desktop-e2e-linux` before the milestone commit. Phase-end (after M5):
`--include-slow` (adds macOS Playwright + `rust-tests-linux`), the manual Quick Look key-forwarding smoke, and the
drag-drop manual checklist (master § Verification, manual gates after Phase 2); watch CI to green before merging to
`main`. Import-cycle rule (master § Verification): the bus imports the store, never the reverse; handlers import both;
the registry imports neither `routes/` nor handlers.

### M1 – Registry typing + `cmdr/no-raw-command-dispatch`

**Scope:** `lib/commands/command-registry.ts`, `lib/commands/types.ts`, a new `CommandArgs` map, the `dispatch` typing
seam, `eslint-plugins/no-raw-command-dispatch.js` + registration. NO behavior change, NO entry-path migration yet – this
is the type foundation the later milestones stand on. The naive `dispatch(id: string, args: unknown)` would be a strict
regression vs today's typed `explorerRef` calls (master § Discovery verdicts); this milestone is what makes the rest a
net gain.

**Intentions:**

- **Derive `CommandId` from a separate `as const` id tuple, keep `commands` a mutable `Command[]`** (deviation 4): a
  `const COMMAND_IDS = ['app.quit', 'file.rename', …] as const`, `type CommandId = (typeof COMMAND_IDS)[number]`. Do NOT
  put `as const satisfies readonly Command[]` on the `commands` array – it would make `Command` properties `readonly`
  and break `updateLicenseCommandName`'s `cmd.name = …` write plus the mutable `Command[]` consumers
  (`getPaletteCommands`, the conflict detector). A1-of-the-bus: ids become a closed union, not `string`. Guard the
  tuple↔registry sync with a `satisfies` check or a unit test asserting `new Set(COMMAND_IDS)` equals
  `new Set(commands.map(c => c.id))` (both directions) so a new registry entry without a tuple entry – or vice versa –
  fails at build/test time.
- `CommandArgs`: a `Record<CommandId, …>` map. **Most commands are arg-less** – model those as `void` (or `undefined`)
  so `dispatch('file.rename')` needs no second argument. The arg-carrying ones (the MCP per-pane variants from the
  inventory: `sort.set`, `selection.mcpSelect`, `cursor.moveTo`, `cursor.scrollTo`, `view.setMode`, `dialog.confirm`,
  `file.copy`/`move`/`delete` options, `tab.mcpAction`, `volume.selectByName`) carry typed payloads. (`nav.toPath` is
  NOT a Phase-2 command – it lands in Phase 3 with `NavigateResult`; see M4.) Define `CommandArgs` so arg-less ids
  resolve to `void` and `dispatch<K>(id: K, ...args: CommandArgs[K] extends void ? [] : [CommandArgs[K]])` – the
  implementing agent picks the exact conditional-tuple shape; the intent is "arg-less callers stay terse, arg-carrying
  callers are type-checked."
- **`handleCommandExecute`'s giant switch: map to a flat `Record<CommandId, Handler>` incrementally, NOT a big-bang
  rewrite.** The master sketch wants a flat handler record (one-hop cmd-click traversal). The honest tradeoff: the
  switch is 520 lines with shared context (`ctx`, `explorerRef`, the A7 guards, the per-command `log.info` +
  `record_breadcrumb`). Rewriting all 99 cases into a record in M1 is a large, risky, behavior-must-be-identical diff
  with no coverage backstop on the routes file (master § PR2). **Decision: M1 introduces the typed `dispatch` signature
  and the `CommandId`/`CommandArgs` types, and keeps `handleCommandExecute` as the switch behind a typed
  `dispatch(id, args)` entry point that the switch reads.** The flat-record conversion is deferred – if it lands at all,
  it lands as its own isolated milestone (M6, optional) after the entry paths are migrated, so a regression is
  bisectable to the conversion and not tangled with a migration. Justify in the commit: the switch already gives
  missing-case-is-dead-code via the registry-id union once `commandId: CommandId`; the record's only marginal win is
  jump-to-definition, not type safety, and it's not worth a 520-line rewrite mid-phase. (Reviewer note: this is a
  deliberate adaptation of the master sketch – flag it for David's seam review.)
- `cmdr/no-raw-command-dispatch` (model `no-raw-tauri-invoke.js`): no string literals as `dispatch` /
  `handleCommandExecute` ids outside the registry; callers pass `CommandId`-typed values. Allowed-path fragments: the
  registry file, `.test.`, `/test/`. The durable A3 anti-rot guardrail.
- **Guard Rust↔FE command-id drift (the `CommandId` union can't reach across the IPC boundary).** Rust `menu/mod.rs:197`
  `menu_id_to_command` hardcodes ~37 command-id string literals emitted via `execute-command`, and cross-window emits
  exist too (`LicenseSection.svelte:37` emits `{ commandId: 'app.licenseKey' }` to `main`). A stale Rust id silently
  hits the switch `default` and no-ops – the FE union can't catch it. **Mandate a characterization test asserting every
  Rust-emitted command id ∈ the registry ids.** The repo already has the precedent: `mod.rs:587`
  `test_command_id_to_menu_id_roundtrip` round-trips the menu↔command mapping. Either (a) extract the Rust id list (a
  generated fixture / parse of `menu_id_to_command`) and assert ⊆ `COMMAND_IDS` from a test, or (b) at minimum a
  hand-maintained id list pinned by a test, with a comment in BOTH `menu/mod.rs` and the registry pointing at each
  other. **State explicitly:** the `execute-command` emit payloads themselves stay un-typed across IPC (Rust emits a
  `json!({ "commandId": "…" })`; tauri-specta typing the menu-emit surface is a stretch goal, master § Target arch 2);
  the drift test is the accepted backstop for that un-typed seam. Decide whether this test lands in M1 (with the typing)
  or M3 (with the native-menu migration) – either is fine; flag it so it doesn't fall between them.

**Test plan (TDD for the rule + types):** rule fixture (a file dispatching a literal `'file.rename'` → reported; a file
dispatching a `CommandId`-typed local → clean), mirroring the repo's other custom-rule RuleTester tests. A type-level
test that `CommandId` is the closed union (a bogus id fails to compile – use `@ts-expect-error`). A runtime test pinning
`updateLicenseCommandName` still mutates the array. Existing `fuzzy-search.test.ts`, `shortcut-dispatch.test.ts`,
`command-dispatch` tests keep passing unchanged.

**DONE:** registry typed; `CommandId`/`CommandArgs`/`dispatch` land; `handleCommandExecute` signature is
`(commandId: CommandId, ctx)`; the ESLint rule is registered + green across `src/`; full suite + `desktop-e2e-linux`
green; zero behavior change.

### M2 – F-key bar onto the bus (delete `handleFn*`)

**Scope:** `+page.svelte:774-811` (the 9 `handleFn*` closures) + `+page.svelte:936-948` (the `FunctionKeyBar` `on*`-prop
wiring), `FunctionKeyBar.svelte` (props → command dispatch).

**Intentions:** replace each `handleFn*` + its `on*` prop with a `dispatch(commandId)` call. The mapping is direct:
`onView`→`file.view`, `onEdit`→`file.edit`, `onCopy`→`file.copy`, `onMove`→`file.move`, `onRename`→`file.rename`,
`onNewFile`→`file.newFile`, `onNewFolder`→`file.newFolder`, `onDelete`→`file.delete`,
`onDeletePermanently`→`file.deletePermanently`. The 9 closures duplicate the `file.*` dispatch cases exactly (compare
`handleFnView`'s `openViewerForCursor()` to `command-dispatch.ts:442` – identical), so deleting them and routing the
buttons through `dispatch` removes pure duplication. **Decide where dispatch is reached from `FunctionKeyBar`:** the
component already reads `explorerState` directly for its `canMkdir/canMkfile/canRename` flags (the A9 pattern from Phase
1, `FunctionKeyBar.svelte:36-57`), so a single `onCommand?: (id: CommandId) => void` prop wired to
`handleCommandExecute` is the minimal seam – keep the capability-flag reads as-is. Keep `canSourceOps` a prop (Phase-1
note: it's a genuine source-op concept, always `true` for now, not derived from the snapshot-pane volumeId; Phase 4 owns
it).

**Accepted telemetry delta (not a PR3 violation).** Today's `handleFn*` closures call `explorerRef` directly and skip
the dispatch preamble (`log.info(commandId)` + the `record_breadcrumb` IPC + the `blockedBySearchResultsPane` check,
`command-dispatch.ts:184-195`). Routing F-keys through the bus adds all three. This is a **deliberate, accepted** delta
– uniform telemetry + capability gating for every entry path is the whole point of the bus, and an F-key action is
exactly the kind of user intent that should log + breadcrumb. PR3's "byte-identical" applies to user-visible behavior
(toast copy, disabled states, focus timing), not to internal logging/breadcrumb volume. Call it out so a reviewer
doesn't flag the new log lines as a regression.

**Landmines:** L10 – the F-bar buttons' `disabled` flags are user-facing contract (search-results pane disables
F2/F7/Shift+F4); routing the click through `dispatch` must keep the buttons disabled exactly as today. The dispatch path
ALSO runs the `blockedBySearchResultsPane` toast guard (`command-dispatch.ts:96`) now that F-keys flow through it – the
search-results block must AGREE with the F-bar's own disabled flags so no new toast fires where the button was
clickable. The button's visible `disabled` must win first (a disabled button can't be clicked, so it never reaches the
toast, which stays the "last resort" shortcut-path guard); verify the two capability sources don't disagree (both gate
the same `search-results` set: paste / mkdir / mkfile / rename).

**Test plan:** the F-bar capability E2E (search-results pane disables F2/F7/Shift+F4) stays green – the contract the
deletion can't regress. Existing `+page.svelte` F-bar-wiring Vitest kept passing; if the wiring moves into
`FunctionKeyBar`, move its test there. No coverage backstop on `+page.svelte` (routes file, master § PR2) – the A4 "no
parallel paths" review confirms all 9 closures are deleted, none left dangling.

**DONE:** all 9 `handleFn*` closures + their `on*` props deleted; F-bar buttons dispatch `file.*` commands;
byte-identical behavior incl. disabled states and toast copy (PR3); full suite + `desktop-e2e-linux` green.

### M3 – Native-menu side events onto the bus (`view-mode-changed`, `menu-sort`, `menu-action`)

**Scope:** `DualPaneExplorer.svelte:966` (`view-mode-changed`), `mcp-listeners.ts:61` (`menu-sort`),
`FilePane.svelte:2401` (`menu-action`), plus the `onCommand` string-prop chain (`FilePane.svelte:1874` →
`DualPaneExplorer.svelte:1979` → `+page.svelte:927`) which gets `CommandId`-typed here since it's a native-menu-adjacent
command channel.

**Intentions:** `execute-command` already carries most menu clicks to the bus (deviation 3); this milestone is the THREE
direct-emit exceptions plus the selection-dialog command prop.

- `view-mode-changed` (`DualPaneExplorer.svelte:966`): payload `{ mode, pane? }`. This is a **per-pane** view change
  with focus-preserving semantics (an inactive-pane menu click changes that pane without stealing focus) plus its own
  persistence (`saveAppStatus` + `saveTabsForPaneSide`). The existing `view.briefMode`/`view.fullMode` commands act on
  the focused pane only – they can't express "set the right pane while left is focused." Route through a new
  `view.setMode` `{ pane, mode }` command (shared with `mcp-set-view-mode` in M4). **Do NOT route it to the ExplorerAPI
  `setViewMode` (`DualPaneExplorer.svelte:1404`) – that path is NOT byte-identical (PR3).** `setViewMode` calls
  `pushViewMenuState()` (line 1409) in addition to `setPaneViewMode` + persistence; the `view-mode-changed` listener
  (line 969) deliberately OMITS `pushViewMenuState`, because a menu click is the menu telling the FE its own new state –
  pushing it back would double-sync against Rust's `sync_view_mode_check_states` (the menu already toggled the
  CheckMenuItem on click, `menu_handlers.rs:383`). The `view.setMode` handler must do exactly what the listener does:
  `setPaneViewMode(targetPane, newMode)` + `saveAppStatus({ [paneKey(targetPane,'viewMode')]: newMode })` +
  `saveTabsForPaneSide(targetPane)`, and **no `pushViewMenuState`**. Keep the focus-preserving semantics byte-identical.
- `menu-sort` (`mcp-listeners.ts:61`): native sort-menu clicks → `setSortColumn`/`setSortOrder` on the focused pane.
  Maps to the existing `sort.byName`/`sort.ascending`/etc. registry commands. Route through `dispatch` and delete the
  bespoke listener. (It physically lives in the MCP file but is a menu event – deviation 2.)
- `menu-action` (`FilePane.svelte:2401`): only `action === 'open'` → `handleNavigate(cursorEntry)`. This duplicates
  `nav.open`. **Sequencing caution:** `handleNavigate` is navigation mechanism – routing `menu-action`'s `'open'`
  through `nav.open` (which today calls `sendKeyToFocusedPane('Enter')`, `command-dispatch.ts:366`) must reproduce the
  exact same behavior the in-pane listener produces. Verify the cursor-entry `handleNavigate` path and the `Enter`-key
  path land on the same place; if they diverge, keep the in-pane listener calling its current primitive and only
  re-route the trigger, not the mechanism (Phase 3 owns nav mechanism).
- `onCommand` prop chain: type it `(id: CommandId) => void` end to end. The only current producer is the
  selection-dialog-key classifier (`FilePane.svelte:1874`, `selection.selectFiles`/`selection.deselectFiles`). The
  string literals there become `CommandId`-typed; the `no-raw-command-dispatch` rule covers them.

**Landmines:** L8 (tab-sync debounce – `view-mode-changed` persistence must not change the debounce semantics). The
per-pane menu check-state sync (`view-mode-changed` is paired with Rust `sync_view_mode_check_states`) is behavior the
handler must not disturb.

**Test plan:** existing per-pane view-mode E2E (inactive-pane click changes that pane without focus change) and
sort-menu E2E stay green. Selection-dialog `+`/`-` E2E stays green. Re-grep
`listen('view-mode-changed'|'menu-sort'|'menu-action')` at milestone end – zero hits outside the bus.

**DONE:** the three direct-emit listeners deleted; their behavior reached through typed `dispatch`; `onCommand` chain
`CommandId`-typed; per-pane + focus-preserving + persistence semantics byte-identical; full suite + `desktop-e2e-linux`
green.

### M4 – MCP events onto the bus with validating parses

**Scope:** all 18 MCP events – 17 in `mcp-listeners.ts` + `mcp-tab` in `DualPaneExplorer.svelte:1012`. New registry
commands + `CommandArgs` shapes per the inventory table. The string-action sub-dispatchers (`handleSelectionAction`,
`handleMcpSelect`, `handleMcpTabAction`, `confirmDialog`) promoted to literal-union params or registry commands.

**Intentions:**

- `mcp-listeners.ts` becomes a thin **transport adapter**: for each event, validate-parse the raw payload into the typed
  `CommandArgs[K]` (per the `mcp-open-search-dialog` whitelist precedent), then `dispatch(id, args)`. No `as {...}`
  casts survive – every discriminant string is whitelist-checked, unknowns collapse to a safe default or silent skip.
- **`mcp-response` round-trips stay in the adapter** (Open Q2). `mcp-open-under-cursor` and `mcp-move-cursor` dispatch
  their intent through the bus and keep their `requestId` correlation + `emit('mcp-response', …)` in `mcp-listeners.ts`
  (the bus stays transport-unaware: it dispatches a `void`-returning command, the adapter awaits a settle signal and
  replies).
- **`mcp-nav-to-path` BYPASSES dispatch entirely in Phase 2 – do NOT add a `nav.toPath` handler.** The whole reason it
  exists is the sync-refusal sentinel: `explorerRef.navigateToPath` returns `string | Promise<void>`, and the adapter
  branches on `typeof result === 'string'` to forward the refusal text as the `mcp-response` error
  (`mcp-listeners.ts:102-119`). `dispatch` is fire-and-forget (`Promise<void>`); it can't surface that return value, so
  there's no way to route `mcp-nav-to-path` through the bus AND keep the round-trip in Phase 2. **Resolution: the
  adapter keeps calling `explorerRef.navigateToPath` directly for `mcp-nav-to-path`** (the one event that stays off the
  bus this phase), byte-identical to today (L12). Adding a `nav.toPath` registry entry + handler now would be a dead
  parallel path (PR1/A4 violation) – the registry entry, the typed args, and the bus wiring for it land in **Phase 3**
  together with `NavigateResult` (the typed refusal union that finally lets it return through dispatch). Same rule
  applies to any other sentinel-dependent event if one surfaces. (So `nav.toPath` is listed in the M4 arg-carrying
  examples as a Phase-3 target, not a Phase-2 deliverable.)
- **Sequencing for the bus-routed nav-adjacent events:** `mcp-key`'s GoBack/GoForward → `nav.back`/`nav.forward` route
  through the bus but keep calling the OLD nav entry (`navigate`). `mcp-volume-select` → `volume.selectByName` keeps
  calling `selectVolumeByName`. Do NOT touch nav/volume mechanics (Phase 3).
- Promote the sub-dispatchers: `handleMcpSelect`'s `mode: string` → `'replace'|'add'|'subtract'` union;
  `handleSelectionAction`'s `action: string` → the existing action union (it already has a closed set);
  `handleMcpTabAction`'s `action: string` → `'new'|'close'|'close_others'|'activate'|'reopen'|'set_pinned'` union;
  `confirmDialog`'s `dialogType: string` → `'transfer-confirmation'|'delete-confirmation'` union. These become typed
  `CommandArgs` or literal-union params (the existing `navigate(action: 'back'|'forward'|'parent')` union is the model,
  master § Target arch 2).
- `mcp-tab` lives in `DualPaneExplorer` – its handler writes tab state (navigation-adjacent). Route the `mcp-tab` event
  through `dispatch` to a `tab.*` command but keep `handleMcpTabAction` calling its current tab primitives (Phase 3 owns
  tab/nav mechanism; tab managers keep their setter API per Phase 1 A1/A2 scope).

**Landmines:** L1 (focus re-anchoring – `moveCursor` refocuses after awaiting; `navigateToPath`/`selectVolumeByName`
deliberately do NOT; regression guard `mtp.spec.ts:414`). L2 (`await whenLoadSettles()` before cursor ops in
`moveCursor`). Both live inside the primitives the handlers call – the bus routing must not reorder or normalize them.
L12 (the `navigateToPath` sync-refusal `string` forwarded as `mcp-response` error – byte-identical).

**Test plan:** the MCP E2E suite (`mtp.spec.ts` incl. the L1 guard at :414, the `mcp-*` round-trip specs) stays green –
it's the contract for the whole MCP surface. New `CommandArgs` parse helpers get unit tests (malformed payload →
default/skip, well-formed → typed args), landing in the same milestone (master § PR2 – these are testable pure parsers,
unlike the routes-side migration). Re-grep `as {` in `mcp-listeners.ts` at milestone end – zero (except where a parse
genuinely needs a narrowing cast after validation).

**DONE:** all 18 events dispatch through the bus with validating parses; `mcp-response` round-trips preserved in the
adapter; the four string-action sub-dispatchers typed; nav/volume/tab mechanics untouched (Phase 3); full suite +
`desktop-e2e-linux` green; MCP E2E green.

### M5 – Quick Look + debug + Selection-dialog cleanup; docs + ESLint sweep

**Scope:** `routePanelKey` command-resolution seam (`DualPaneExplorer.svelte:1496`, `pane-commands.ts:128`), the
debug-panel events (`+page.svelte:304/308/312`), final removal of any surviving `onCommand`-as-string surface,
`lib/commands/CLAUDE.md` + `routes/(main)/CLAUDE.md` + `pane/CLAUDE.md` doc updates, `docs/architecture.md` frontend
section.

**Intentions:**

- **Quick Look forwarding (L9 – handle with care):** `routePanelKey` synthesizes a KeyboardEvent and runs an in-pane
  type-to-jump intercept BEFORE delegating to `handleKeyDown`. P2 says per-keystroke keys stay off the bus, so
  **`routePanelKey`'s jump-char + reset-key mirror stays exactly as-is** (`pane-commands.ts:152-161`). The only seam
  this milestone may touch: if Quick Look forwards a resolved COMMAND (e.g. a Shift+Space toggle that today re-enters
  `handleCommandExecute('file.quickLook')` via the dispatch-guard path, `command-dispatch.ts:524`), that command path is
  already on the bus – confirm it's `CommandId`-typed and the dispatch guard (`quickLookDispatchGuardJustFired`) is
  preserved. If `routePanelKey` carries no command (only keystrokes), this milestone is a no-op for Quick Look beyond
  confirming P2/L9 are intact. Do NOT route keystrokes through the bus to "clean it up."
- **Debug panel:** `debug-inject-error` / `debug-reset-error` / `debug-trigger-transfer-error` are dev-only, gated by
  `import.meta.env.DEV`, and are NOT user commands (no registry entry, no palette, no shortcut). The master sketch lists
  "debug panel deletion" but these aren't duplications of bus commands – they're a distinct dev channel. **Decision:
  leave them as direct `explorerRef` calls.** Promoting them to registry commands would pollute the command union with
  dev-only ids and gain nothing (master § Target arch 2 lists debug panel as an entry path "that resolves to a
  user-intent command" – these don't; they inject test state). Document the carve-out in `routes/(main)/CLAUDE.md` so a
  future agent doesn't "finish the migration." (Reviewer note: adaptation of the master sketch – flag for review.)
- **Selection-dialog `onCommand` prop:** already typed `CommandId` in M3. If anything string-typed survives, finish it
  here.
- **Docs + lint sweep:** update `lib/commands/CLAUDE.md` (the "Adding a command" steps now reference the typed
  `dispatch` + `CommandArgs`, and the `as const` id tuple + sync guard that every new command must also update – the
  registry itself stays a mutable `Command[]` per deviation 4), `routes/(main)/CLAUDE.md` (the dispatch is
  `CommandId`-typed; the debug carve-out), `pane/CLAUDE.md` (the F-bar + selection-dialog now dispatch),
  `docs/architecture.md` frontend section. Confirm `no-raw-command-dispatch` is green across all of `src/` with zero
  opt-outs (or each opt-out justified inline).

**Test plan:** the manual Quick Look key-forwarding smoke (master § Verification) – type-to-jump in the Quick Look
panel, Shift+Space toggle, arrow nav. The `pane-commands.test.ts` `routePanelKey` jump-intercept tests stay green
unchanged (L9 contract). Debug-error-preview dev flow still works (manual). Docs-only changes covered by `--fast`
(oxfmt, claude-md-reminder).

**DONE:** Quick Look command seam confirmed `CommandId`-typed with L9/P2 intact; debug carve-out documented;
`no-raw-command-dispatch` green with no unjustified opt-outs; all docs swept; **phase-end:** `--include-slow` green +
manual Quick Look + drag-drop checklists + watch CI to green before the phase merge to `main`.

### M6 (optional) – Flat handler record conversion — DONE

Executed as its own plan: [command-handler-record-plan.md](command-handler-record-plan.md). The `handleCommandExecute`
switch is now a small dispatch core over a flat handler record keyed by `Exclude<CommandId, DispatchExemptId>` (the win:
one-hop cmd-click traversal + a compile-time completeness guarantee), with handlers split into family modules under
`routes/(main)/command-handlers/`. It landed behavior-identical, isolated from the entry-path migration, exactly as
deferred out of M1.

## Invariants this phase must honor

- **P2** (loud) – arrow keys / type-to-jump chars / reset keys NEVER route through the bus; no per-keystroke registry
  lookup, `log.info`, or `record_breadcrumb`. `routePanelKey`'s jump mirror (L9) stays a synthesized-event path.
- **A3** – dispatch ids are `CommandId`-typed end to end; no string literals outside the registry, enforced by
  `cmdr/no-raw-command-dispatch`.
- **A7** – pre-dispatch text-region guards (`handleTextRegionShortcut`, the ⌘C/⌘←/→ bails in `+page.svelte`) stay in
  front of the bus, verbatim.
- **A8** – no new components. Registry typing, an ESLint rule, validating parsers, and handler routing only.
- **A9** – `FunctionKeyBar` reading `explorerState` in a `$derived` (Phase-1 pattern) stays; don't re-wrap in props.
- **PR1/PR3** – each milestone is add + migrate + delete, atomic, byte-identical user-visible behavior (toast copy,
  disabled states, focus timing, menu check states, `mcp-response` round-trips).
- **Sequencing (loud)** – navigation/volume/tab commands route through the bus but keep calling the OLD
  `navigate`/`navigateToPath`/`selectVolumeByName`/tab-mutation entries; the refusal-string forward and the generation
  counters are untouched (Phase 3, A4/L12). **L10** – read-only / search-results guard alert + toast strings are
  user-facing contract (E2E asserts them). **L1/L2/L8/L9** – focus re-anchoring, `whenLoadSettles` ordering, tab-sync
  debounce, and Quick Look jump-mirror survive byte-identically inside the primitives the handlers call.

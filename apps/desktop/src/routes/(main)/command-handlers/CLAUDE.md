# Command handlers

The family-grouped handler modules behind the dispatch core (`../command-dispatch.ts`). Each command id maps to one
`CommandHandler`; the core runs the preamble, then looks the id up in the assembled record and `await`s the handler.

## Shape

- `types.ts` — the seam: `CommandHandlerContext` (the per-dispatch `{ explorerRef, ctx, dispatchArgs }`, resolved ONCE
  in the core), `CommandHandler` (`(hctx) => void | Promise<void>`), `CommandHandlerRecord`
  (`Record<Exclude<CommandId, DispatchExemptId>, CommandHandler>`), and the `DispatchExemptId` union + its runtime
  `DISPATCH_EXEMPT_IDS` tuple.
- One module per family (`app-dialog`, `view`, `pane`, `tab`, `nav`, `sort`, `file`, `clipboard`, `selection`, `misc`),
  each a `satisfies Partial<CommandHandlerRecord>` object.
- `index.ts` — spreads the family modules into one `commandHandlers: CommandHandlerRecord`. The annotation is the
  completeness guard: a missing handler or an exempt-id handler fails to compile.

## Rules

- **Handlers read `hctx.explorerRef`, never `ctx.getExplorer()`.** The core reads the explorer once per dispatch; a
  handler re-reading it would re-evaluate mid-dispatch (HMR-fragile). Grep `getExplorer(` here → must be zero.
- **Preserve each arm's `await` vs `void` exactly.** The MCP round-trip ids (`nav.openUnderCursor`, `cursor.moveTo`,
  `selection.mcpSelect`, `selection.mcpSelectByNames`, `pane.refresh`) are `async` + `await` so the adapter acks on real
  completion (the ack-timing contract); the `command-dispatch.characterization.test.ts` deferred-promise pins guard
  this. Every other explorer-driving arm `void`s its promise. `void`-ing a round-trip (or awaiting a fire-and-forget) is
  a silent behavior break with no compile error.
- **Grouped ids share ONE body, no copy-paste.** The four `view.zoom.setNN` presets call one `applyZoomPreset`; the
  get-entry-then-act file/cloud arms call one `withEntryUnderCursor`.
- **No imports of the core or `+page.svelte`.** Modules import `../command-dispatch-context`, `../explorer-api`,
  `$lib/commands` types, and the leaf helpers the arms call. The core imports them; they never import the core
  (`import-cycles` fires if this inverts).

## The exempt families (`DispatchExemptId`)

20 ids registered (for the rebinding UI) with NO handler, in three families documented in `types.ts`: native-menu-owned
(`app.quit` etc., run by macOS PredefinedMenuItems), per-keystroke P2 (`nav.up/down/left/right/firstInFull/lastInFull`,
which ride `handleKeyDown → FilePane`, NEVER the bus), and component-scoped (palette / volume / network / share /
context menu, handled inside their own components). The core silently no-ops these after the preamble.

Family 1's four ids are spread into `DISPATCH_EXEMPT_IDS` from `NATIVE_SHORTCUT_COMMAND_IDS` in
`$lib/commands/command-registry` — the same list the registry's `nativeShortcut` flag keys off and the shortcuts editor
uses to render those rows read-only — so the "AppKit owns this" fact lives in exactly one place. The `DispatchExemptId`
union still lists the four literals (the type can't spread a runtime tuple); `command-registry.test.ts` pins the two in
sync.

**❌ Do NOT add a handler for a per-keystroke `nav.*` id** — that routes an arrow key through a registry lookup + log +
breadcrumb IPC per keypress (a P2 perf regression), not a completion.

## Adding a command

Add the handler to the right family module. A missing one is a COMPILE error (the record is keyed by
`Exclude<CommandId, DispatchExemptId>`). An intentionally handlerless command goes in `DISPATCH_EXEMPT_IDS` with a
documented reason. The `command-handler-record.test.ts` set-equality test fails if the id is in neither.

Full details: [DETAILS.md](DETAILS.md).

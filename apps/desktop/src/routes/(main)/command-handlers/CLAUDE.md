# Command handlers

The family-grouped handler modules behind the dispatch core (`../command-dispatch.ts`). Each `CommandId` maps to one
`CommandHandler`; the core runs the preamble, then looks the id up in the assembled record and `await`s the handler.

## Shape

- `types.ts`: the seam (`CommandHandlerContext`, `CommandHandler`, `CommandHandlerRecord`, the `DispatchExemptId` union
  - its runtime `DISPATCH_EXEMPT_IDS` tuple). Self-documenting; read it before touching exemptions.
- One module per family (`app-dialog`, `view`, `pane`, `tab`, `nav`, `sort`, `file`, `clipboard`, `selection`, `misc`),
  each a `satisfies Partial<CommandHandlerRecord>` object.
- `index.ts`: spreads the families into one `commandHandlers: CommandHandlerRecord`. The annotation is the completeness
  guard: a missing handler or an exempt-id handler fails to compile.

## Rules

- **Handlers read `hctx.explorerRef`, never `ctx.getExplorer()`.** The core reads the explorer once per dispatch;
  re-reading would re-evaluate mid-dispatch (HMR-fragile). Grep `getExplorer(` here must stay zero.
- **Preserve each arm's `await` vs `void` exactly.** The MCP round-trip ids (`nav.openUnderCursor`, `cursor.moveTo`,
  `selection.mcpSelect`, `selection.mcpSelectByNames`, `pane.refresh`) are `async` + `await` so the adapter acks on real
  completion (the ack-timing contract; `command-dispatch.characterization.test.ts` deferred-promise pins guard it).
  Every other explorer-driving arm `void`s its promise. `void`-ing a round-trip (or awaiting a fire-and-forget) is a
  silent behavior break with no compile error.
- **Grouped ids share ONE body, no copy-paste.** The four `view.zoom.setNN` presets call one `applyZoomPreset`; the
  get-entry-then-act file/cloud arms call one `withEntryUnderCursor`.
- **No imports of the core or `+page.svelte`.** Modules import `../command-dispatch-context`, `../explorer-api`,
  `$lib/commands` types, and the leaf helpers the arms call; never the core (`import-cycles` fires if this inverts).
- **❌ Don't add a handler for a per-keystroke `nav.*` id** (`nav.up/down/left/right/firstInFull/lastInFull`). They ride
  `handleKeyDown → FilePane`, never the bus; a registry lookup + log + breadcrumb IPC per keypress is a P2 regression,
  not a completion.

## Adding a command

Add the handler to the right family module. A missing one is a COMPILE error (the record is keyed by
`Exclude<CommandId, DispatchExemptId>`). An intentionally handlerless command goes in `DISPATCH_EXEMPT_IDS` with a
documented reason. `command-handler-record.test.ts` set-equality fails if the id is in neither.

Full details: [DETAILS.md](DETAILS.md).

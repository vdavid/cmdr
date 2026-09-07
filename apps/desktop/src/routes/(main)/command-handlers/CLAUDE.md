# Command handlers

The family-grouped handler modules behind the dispatch core (`../command-dispatch.ts`). Each `CommandId` maps to one
`CommandHandler`; the core runs the preamble, then looks the id up in the assembled record and `await`s the handler.

## Shape

- `types.ts`: the seam, including `DispatchExemptId` and its runtime `DISPATCH_EXEMPT_IDS` tuple. Read it before
  touching exemptions.
- One module per family (`app-dialog`, `view`, `pane`, `tab`, `nav`, `sort`, `file`, `clipboard`, `selection`, `tag`,
  `servers`, `misc`), spread by `index.ts` into `commandHandlers: CommandHandlerRecord`. That annotation is the
  completeness guard: a missing or exempt-id handler fails to compile.

## Rules

- **Handlers read `hctx.explorerRef`, never `ctx.getExplorer()`.** The core reads the explorer once per dispatch;
  re-reading would re-evaluate mid-dispatch (HMR-fragile). Grep `getExplorer(` here must stay zero.
- **Preserve each arm's `await` vs `void` exactly.** The five MCP round-trip ids (`nav.openUnderCursor`,
  `cursor.moveTo`, `selection.mcpSelect`, `selection.mcpSelectByNames`, `pane.refresh`) are `async` + `await` so the
  adapter acks on real completion; every other explorer-driving arm `void`s its promise. Swapping one breaks behavior
  silently, with no compile error (`command-dispatch.characterization.test.ts` pins it).
- **Grouped ids share ONE body, no copy-paste** (`applyZoomPreset`, `withEntryUnderCursor`, `copyPathAndAnnounce`).
- **The `servers.*` row actions (pin, disconnect, forget, edit) go through `runServerRowAction`**, the native row
  menu's own path, so menu and palette can't drift on a confirmation or a toast. Which server they act on is
  `$lib/servers/server-command-target.ts`'s call, ❌ never `getFocusedPaneVolumeId()` alone: the hub IS a pane, so that
  reading answers the synthetic hub row (`$lib/servers/DETAILS.md` § Which server a command acts on). Finding no server
  says NOTHING: the palette lists every command whatever the pane is on. ❗ `servers.edit` and `servers.connect` open
  their sheet and return; awaiting it holds the pipeline open.
- **`file.copyPath` skips `withEntryUnderCursor`** for `getPathToCopyUnderCursor()`, which resolves `..` to the pane's
  own directory. Every other under-cursor arm keeps treating `..` as "no entry"
  (`file-explorer/pane/DETAILS.md` § Copy-path).
- **The clipboard arms branch on `isTextInputFocused()` (`$lib/utils/text-input-focus`) before touching the explorer**:
  a native menu accelerator reaches them even with focus in a dialog's text field. Don't re-roll the `activeElement`
  check: the keydown resolver and the capability guard read that same predicate.
- **No imports of the core or `+page.svelte`**; `import-cycles` fires if this inverts.

A new command's handler goes in the right family module (a missing one is a COMPILE error); a deliberately handlerless
one goes in `DISPATCH_EXEMPT_IDS` with a reason. `command-handler-record.test.ts` fails if it is in neither. The
per-keystroke `nav.*` ids are exempt on purpose: `../CLAUDE.md` and `types.ts` say why.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.

# Transfer (copy and move)

Frontend for copy (F5), move (F6), and compress (⌥F5): destination picker, dry-run conflict scan, dual-bar progress
dialog, error rendering. One set serves all via `operationType`; delete/trash reuse the progress dialog. Backend
counterpart: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

## Module map

- `TransferDialog.svelte` is the setup shell, `TransferProgressDialog.svelte` the execution shell; each sits over its
  own `transfer-*-state.svelte.ts`.
- `transfer-dispatch.ts` is birth: which backend command a confirmed copy/move/compress/delete/trash routes to. The
  rest: DETAILS § File map.

## Must-knows

- **The dialog is a VIEW of its operation, ❌ never its owner.** Phase, counts, rates, ETA, clash, outcome, and every
  command come from its session (`../operation-session/CLAUDE.md`), shared with the queue rows and the corner chip. The
  view keeps only UI concerns. ❌ Never a second smoother, listener, or event buffer.
- **A close is a DETACH, ❌ never a cancel.** `onclose` goes to `detach()`, handing a still-running operation to the
  queue window; only the Cancel button cancels, and unmounting stops nothing. With no session yet it leaves the
  operation ALONE (guessing would report a cancel over a live transfer). ❌ While a clash is up there's no `onclose` at
  all: every way out of a clash decides something about the user's files.
- **Queue and the dialog-scoped F2 are FRONTEND-ONLY** (set `backgrounded`, open the queue window, unmount). ❌
  `backgrounded` and `destroyed` stay plain `let`s: teardown reads them during reactive-scope disposal, where a rune
  returns a stale value, which is how a just-queued transfer once got cancelled.
- **One transfer entry seam**: F5/F6, drag-and-drop, and paste all prepare through `pane/transfer-entry.ts`. The
  destination-guard copy is E2E-asserted, and the paste path's MTP refusal stays SEPARATE and BEFORE the shared guard.
- **Batch IPC for selection lookups** (`get_paths_at_indices` / `get_files_at_indices`), ❌ never a per-index loop: 50k
  files is 5-10 s vs ~1 ms.
- **Speed, ETA, and bars are backend-owned and SHARED with the queue window** (`../TransferProgressReadout.svelte`): ❌
  no second instantaneous rate here, and its fixed-width columns are why the dialog is 580 px wide.
- **A stall drops the ETA and says why** (`transfer-stall.ts`): the BACKEND classifies, this side owns the threshold. ❌
  Never infer a stall from event timing, since a wedge emits no events at all. ❌ Don't hand-pick a yellow for the
  notice; the tone token owns both themes. `DETAILS.md` § "The stalled-transfer notice".
- **"Couldn't find out" is its own state in BOTH pre-confirm checks, ❌ never silence and ❌ never an empty answer.**
  Rendering nothing is what a CLEAN destination renders, and this feeds an overwrite decision. `DETAILS.md` § "When the
  dialog can't find out".
- **Rollback / Cancel disable during the settle window**, and a cancel close waits for both `write-cancelled` AND
  `write-settled` — ❌ but never as the ONLY exit: `progress.dismiss()` backs a Close button that leaves at once.
- **The progress dialog does NOT wait for the scan; the BACKEND does.** It dispatches on mount, so a still-counting
  transfer has an `operationId`, a queue row, and Background from frame one. ❌ Never cancel the preview on teardown:
  the operation owns it. Confirm ALWAYS awaits `scan.scanStarted`. DETAILS § Scan.
- **Compress swaps the conflict-policy UI for a dest-exists overwrite check**; its auto-confirm (MCP) path must ❌ never
  silently overwrite.
- **ONE map from an MCP `onConflict` name to a policy** (`conflict-policy.ts`), shared with the `dialog confirm` path.
  ❌ Never a second copy: an unmapped name silently becomes `skip`, turning "ask me about each file" into "skip every
  file".

The file map, rollback's limits, the password interception, the E2E markers, the phase catalog, flows, and decisions:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.

# Transfer (copy and move)

Frontend for copy (F5), move (F6), and compress (⌥F5): destination picker, dry-run conflict scan, dual-bar progress
dialog, error rendering. One set serves all via `operationType`; delete/trash reuse the progress dialog. Backend:
`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

## Module map

- `TransferDialog.svelte` is the setup shell, `TransferProgressDialog.svelte` the execution shell; each sits over its
  own `transfer-*-state.svelte.ts`.
- `transfer-dispatch.ts` is birth: which backend command a confirmed copy/move/compress/delete/trash routes to. The
  rest: DETAILS § File map.

## Must-knows

- **The dialog is a VIEW of its operation, ❌ never its owner.** Everything (phase, counts, rates, ETA, clash, outcome,
  commands) comes from its session (`../operation-session/CLAUDE.md`), shared with the queue rows and the corner chip.
  ❌ Never a second smoother, listener, or event buffer.
- **A close is a DETACH, ❌ never a cancel.** `onclose` calls `detach()`; only the Cancel button cancels, and unmounting
  stops nothing. With no session yet it leaves the operation ALONE. ❌ While a clash is up there's no `onclose` at all:
  every way out of a clash decides something about the user's files.
- **Queue and the dialog-scoped F2 are FRONTEND-ONLY** (set `backgrounded`, open the queue window, unmount). ❌
  `backgrounded` and `destroyed` stay plain `let`s: teardown reads them during reactive-scope disposal, where a rune
  goes stale, which is how a just-queued transfer once got cancelled.
- **One transfer entry seam**: F5/F6, drag-and-drop, and paste all prepare through `pane/transfer-entry.ts`. The paste
  path's MTP refusal stays SEPARATE and BEFORE the shared guard.
- **Batch IPC for selection lookups** (`get_paths_at_indices` / `get_files_at_indices`), ❌ never a per-index loop: 50k
  files costs 5-10 s vs ~1 ms.
- **Speed, ETA, and bars are backend-owned and SHARED with the queue window** (`../TransferProgressReadout.svelte`): ❌
  no second instantaneous rate here.
- **A stall drops the ETA and says why** (`transfer-stall.ts`): the BACKEND classifies, this side owns the threshold. ❌
  Never infer a stall from event timing: a wedge emits no events at all. DETAILS § "The stalled-transfer notice".
- **A cancel's reversal drains its bar to zero, so the TOAST says what actually stayed** (`cancel-rollback-toast.ts`,
  off `event.rollback`). ❌ Never read the verb off the view's config: an ADOPTED dialog's is inert, and a move's
  reversal worded as a delete is a data-safety lie. ❌ Never colour a deliberate skip as a warning. DETAILS § "What a
  cancelled transfer's reversal says afterwards".
- **"Couldn't find out" is its own state in BOTH pre-confirm checks, ❌ never silence.** Nothing rendered is what a
  CLEAN destination looks like, and this feeds an overwrite decision. DETAILS § "When the dialog can't find out".
- **Rollback / Cancel disable during the settle window; an unavailable Rollback is `aria-disabled` + a why, ❌ never
  `disabled`**, which hides it from a keyboard. A cancel close waits for both `write-cancelled` AND `write-settled` — ❌
  but never as the ONLY exit: `progress.dismiss()` leaves at once.
- **The progress dialog does NOT wait for the scan; the BACKEND does.** It dispatches on mount, so a still-counting
  transfer has an `operationId`, a queue row, and Background from frame one. ❌ Never cancel the preview on teardown;
  confirm ALWAYS awaits `scan.scanStarted`. DETAILS § Scan.
- **Compress swaps the conflict-policy UI for a dest-exists overwrite check**; its auto-confirm (MCP) path ❌ never
  silently overwrites.
- **ONE map from an MCP `onConflict` name to a policy** (`conflict-policy.ts`), shared with `dialog confirm`. ❌ Never a
  second copy: an unmapped name silently becomes `skip`, turning "ask about each file" into "skip every file".

The file map, rollback's limits, the password interception, the E2E markers, the phase catalog, flows, and decisions:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.

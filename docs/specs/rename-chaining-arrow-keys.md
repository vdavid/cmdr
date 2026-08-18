# Chained rename with ArrowUp / ArrowDown

**Status**: shipped, all four milestones. The toast wording awaits David's copy review. **Owner**: David. **Date**:
2026-07-28.

What shipped lives in `apps/desktop/src/lib/file-explorer/rename/CLAUDE.md` and its `DETAILS.md` §§ "Rename sessions"
and "Chaining the rename with the arrow keys"; this file is the plan it was built from and can be wiped with the rest of
the folder.

While the inline rename editor is open, ArrowDown commits the current edit and immediately starts renaming the file
below; ArrowUp does the same upwards. Renaming a run of files becomes one keyboard flow instead of F2, Enter, Down, F2.

Only ArrowDown and ArrowUp. Not PageDown, PageUp, Home, End, or any modified arrow.

Read `apps/desktop/src/lib/file-explorer/rename/CLAUDE.md` and its `DETAILS.md` § "Ending a rename session" before
starting: the click-away commit path this builds on, and the guards that keep the click and blur paths apart, are
described there.

## Settled decisions

These are decided. Don't relitigate them while implementing.

1. **Fire and forget, not await.** The hop happens immediately; the save runs in the background, tagged with a session
   id. Chaining stays fast on slow volumes (SMB, MTP), where awaiting each save would stall every step.
2. **The neighbour is captured BEFORE the rename lands**, by path. The rename may re-sort the listing and move the
   renamed file far away; the file we hop to is the one that was visually below/above at keypress time.
3. **An unusable name is discarded, and the hop still happens.** The user keeps moving; the edit is dropped.
4. **A conflict is discarded**, no dialog. The chain must not stop to ask.
5. **An extension change commits**, no dialog, when the extension-change policy is "ask". The operation log plus the
   coming rollback/undo feature is the safety net for a fumbled extension.
6. **Policy "no" still discards.** With `fileOperations.allowFileExtensionChanges: 'no'` an extension change is a
   validation error, not a dialog. Skipping the dialog is not the same as overriding the user's setting.
7. **At a boundary, the key is a no-op.** ArrowDown on the last row, ArrowUp on the first real row (or onto `..`):
   nothing happens, the editor stays open with the edit intact. No commit, no discard.
8. **No rate limiting on key repeat.** Holding the arrow rips through the directory. Session ids are what keep that
   safe.

## Data safety is the point of this design

The rename subsystem has already produced one bug of exactly this shape: the editor bound to a stale row, writing a new
name onto the WRONG file (see `rename/DETAILS.md` § Decisions, "mounts BY PATH, not by index", and the paste-rename
latch in `b0de3824f`). Chaining multiplies the opportunities, because a save for file N is in flight while the editor is
already on file N+1.

The invariant that prevents it: **every save is bound to the `{path, originalName}` captured when its session
activated**, never to "the current cursor", "the current editor", or an index. `executeRenameSave(target, ...)` already
takes the captured target, so this holds as long as nothing re-reads live state on the way to the backend.

## The step sequence

On ArrowDown (ArrowUp mirrors it) inside the editor:

1. **Find the neighbour index**: `cursorIndex ± 1`, clamped, skipping the `..` row. If there is no real file there, the
   key is a no-op (decision 7). Stop here.
2. **Capture the neighbour entry** via `listRef.getEntryAt(neighbourIndex)` (`pane/types.ts:153`), which reads the
   loaded window synchronously. The cursor's neighbour is essentially always loaded; if it returns `undefined` (window
   edge), fall back to an async `getFileAt(listingId, backendIndex, includeHidden)`. Keep the entry's `path` and `name`.
3. **Decide the current edit's fate**:
   - `rename.severity === 'error'` (unusable name, or a policy-"no" extension change): discard, fire no save.
   - Unchanged name: no save, no toast.
   - Otherwise: fire `executeFlow(skipExtensionCheck: true)` without awaiting, tagged with the current session id.
4. **Hop**: move the cursor to the neighbour via `applyNavigation(neighbourIndex, listRef)`
   (`pane/cursor-nav-keys.ts:45`, which also scrolls it into view) and activate a new rename session **on the captured
   entry**.

Step 4 must NOT be "move the cursor, then rename whatever is under it". `entryUnderCursor` is filled asynchronously by
an IPC `getFileAt` in an effect on `cursorIndex` (`pane/FilePane.svelte:1345`), so immediately after the cursor moves it
still holds the PREVIOUS file. That race is the paste-latch bug. Either activate directly on the captured entry (add a
path-taking entry point next to `startRename`), or go through `startRename({ expectedName })`, whose poll refuses to
activate on a mismatched entry (`pane/rename-flow.svelte.ts:247`). Direct activation on the captured entry is preferred:
no poll, and the target is path-bound by construction.

## Invariants

**I1. Session identity.** Every activation gets a monotonically increasing session id. Both the save side and the editor
side carry it.

**I2. A superseded session may produce toasts and background refreshes ONLY.** It must never touch rename state, focus,
the cursor, or open a dialog. This one rule covers success, error, timeout, conflict, and extension-ask at once:

- A stale success must not `rename.cancel()` (that kills the live session), must not `restoreFocus()`, and must not set
  `pendingCursorName` (see I3).
- A stale error must not `triggerShake()`: it would shake the NEXT file's editor, blaming the wrong file.
- A stale completion must never open the conflict or extension dialog: it would ask about a file the user has moved
  past. Conflicts discard with a toast (decision 4); extension-ask can't occur (we pass `skipExtensionCheck`).

`finalizeRename` (`pane/rename-flow.svelte.ts:208`) is where most of this concentrates today; it currently does all of
the forbidden things unconditionally.

**I3. A superseded session must not set `pendingCursorName`.** On success `finalizeRename` sets it
(`rename-flow.svelte.ts:214`), and when the watcher diff arrives `listing-diff-sync` moves the cursor to the RENAMED
file and **returns early**, skipping `reconcileCursorAndSelection` entirely
(`pane/listing-diff-sync.svelte.ts:173-181`). Mid-chain that both yanks the cursor back to the previous file and drops
the index reconciliation the live session needs. Only the last session in a chain (the one ended by Enter, Escape, a
click, or a boundary) gets the normal post-rename cursor behavior.

**I4. A cancel from a superseded editor is ignored.** When session N+1 activates, session N's input unmounts (a
different row), which fires `onblur` → `handleRenameCancel` → cancels the session that is now live. This is a guaranteed
bug, not a race. Fix by threading the session id into `InlineRenameEditor` (captured at mount, handed back with
`onCancel`) and dropping cancels whose id isn't current. Do NOT reach for a "suppress the next cancel" one-shot:
`suppressBlurCancel` already showed that a one-shot waiting for a blur that never comes eats the user's next Escape.

**I5. Conflict detection stays backend-authoritative.** Don't discard on the frontend's conflict signal. It's a
`severity: 'warning'` computed against `renameSiblingNames`, snapshotted when the session started
(`rename-flow.svelte.ts:122`); mid-chain it is stale by construction, because the chain's own renames are changing the
directory. Discarding on a stale false positive would silently throw away what the user typed. Let it reach
`checkRenameValidity` and discard on the authoritative `{ type: 'conflict' }`. At keypress time we therefore check only
`severity === 'error'`.

**I6. The re-sort takes care of itself, once I3 holds.** The editor mounts by path (`rename/rename-mount.ts`), so a diff
that moves rows makes the editor follow its file; `reconcileCursorAndSelection` (`pane/listing-diff-sync.svelte.ts:42`)
shifts the cursor by add/remove index arithmetic. `pendingCursorName` is the only thing that bypasses that machinery.

## Two things that bite in practice

**Sibling names reload every step.** `loadSiblingNames` (`rename-flow.svelte.ts:141`) pages the WHOLE listing in
500-item batches on each activation. Chaining 20 files in a 100k-file directory is 20 full re-reads. Cache the list for
the life of a chain and patch it locally as renames land (drop the old name, add the new one). Note this interacts with
I5: the cache is for the red-border hint, never for the discard decision.

**Failure toasts get wiped by the next keystroke.** `handleRenameInput` calls `dismissTransientToastsForPane` on every
keystroke (`rename-flow.svelte.ts:299`), so typing the next name erases the toast saying the previous rename didn't
apply. Silently discarding a conflicted rename is only honest if the user sees it. Use `dismissal: 'persistent'` for
chain failures, or aggregate them into one end-of-chain summary. Copy is user-facing: draft it, David reviews.

## UX details

- Up/Down currently move the caret to the start/end of the input. That's given up; Home/End still do it.
- The new editor focuses and selects name-minus-extension on mount, same as any rename. Chaining therefore lets the user
  type a fresh name immediately.
- The editor must stay visible: `applyNavigation` scrolls the neighbour into view, which matters because an editor
  scrolled out of the virtual window unmounts and its blur cancels (`rename/CLAUDE.md` § Cancel triggers).

## Test matrix

Unit (`pane/rename-flow.test.ts` and a new editor test), unless noted:

- **DATA SAFETY**: chain three renames against a slow (manually resolved) backend, assert each `renameFile` call carries
  its own correct `(from, to)` pair and none crossed.
- A stale success does not cancel the live session (I2).
- A stale success does not set `pendingCursorName` (I3).
- A stale error toasts but does not shake (I2).
- The old editor's unmount blur does not cancel the new session (I4) — component-level.
- An unusable name discards, fires no save, and still hops (decision 3).
- A conflict result discards with a persistent toast, no dialog (decisions 4, I5).
- An extension change commits with no dialog under policy "ask" (decision 5), and discards under policy "no" (decision
  6).
- ArrowDown on the last row and ArrowUp on the first real row leave the session untouched, with the edit intact
  (decision 7).
- Key repeat: ten steps fired synchronously produce ten correctly-paired saves and one live session at the end (decision
  8).
- E2E (`test/e2e-playwright/file-operations.spec.ts`): chain three files where the first rename re-sorts the listing
  (name sort, `a.txt` → `z.txt`), assert all three land on disk with the right names and the editor tracked the right
  rows.

## Milestones

1. Session ids: add to `rename-state.svelte.ts`, thread through the flow's terminal paths (I1, I2, I3) and into
   `InlineRenameEditor` for cancels (I4). Ships alone, with tests, changing no behavior.
2. The step itself: editor keydown branch, prop through both views, neighbour capture, hop, path-bound activation
   (decisions 1, 2, 7, 8).
3. The fate rules: unusable-name discard, conflict discard, extension commit, policy "no" discard (decisions 3-6, I5).
4. Polish: sibling-name caching, toast persistence, docs (`rename/CLAUDE.md` must-know + `DETAILS.md` section).

Milestone 1 is worth landing on its own: it's the part that makes the rest safe, and it's testable without any of the
chaining UI.

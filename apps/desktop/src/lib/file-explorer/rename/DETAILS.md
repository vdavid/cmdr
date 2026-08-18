# Rename details

Depth and rationale for inline rename. `CLAUDE.md` holds the must-knows.

## Components

- **InlineRenameEditor.svelte**: input field that replaces the static name cell. Green/yellow/red border by validation
  state, 300 ms glow/zoom on activation, pre-selects the filename excluding the extension.
- **RenameConflictDialog.svelte**: side-by-side file comparison (size, modified) when a conflict is detected. Options:
  "Overwrite and trash old file" (`NSFileManager.trashItem`), "Overwrite and delete old file" (permanent), "Cancel",
  "Continue renaming".
- **ExtensionChangeDialog.svelte**: confirmation when the extension changed and policy is "ask". Buttons: "Keep .{old}",
  "Use .{new}". Checkbox: "Always allow extension changes" (sets policy to "yes").
- **rename-state.svelte.ts**: reactive `$state` for active/target/currentName/validation/shaking/focusTrigger. Must be
  `.svelte.ts` for Svelte 5 reactivity.
- **rename-operations.ts**: pure save-flow logic returning a `RenameResult` discriminated union instead of side effects.
- **rename-activation.ts**: click-to-rename timer logic (800 ms hold, 10 px threshold, cancel on double-click).
- **rename-step.ts**: the two pure halves of a chained rename: which keypress chains, and which row it lands on.

## Three-stage save flow (`rename-operations.ts::executeRenameSave()`)

`RenameResult` variants: `noop`, `error`, `timeout`, `extension-ask`, `conflict`, `success`.

1. **Extension check**: if `extensionPolicy === 'ask'` and extensions differ meaningfully
   (`extensionsDifferMeaningfully()` from `filename-validation.ts`), return `{ type: 'extension-ask' }`; the caller
   shows ExtensionChangeDialog. "Keep" retries with `skipExtensionCheck=true`. Case-only changes (`photo.JPG` →
   `photo.jpg`) and known-equivalent changes (`.jpeg` → `.jpg`, `.md` → `.txt`) skip the dialog entirely.
2. **Backend validity check**: `checkRenameValidity(parentPath, originalName, trimmedName)`. `valid: false` →
   `{ type: 'error' }`. `hasConflict: true, isCaseOnlyRename: false` → `{ type: 'conflict', validity }`.
   `hasConflict: true, isCaseOnlyRename: true` → proceed (same inode, just case). `hasConflict: false` → proceed.
3. **Perform rename**: `renameFile(from, to, force)`. Success → `{ type: 'success', newName }`. Timeout →
   `{ type: 'timeout', message }`; the caller shows a persistent warning toast (the rename may have succeeded) and
   auto-refreshes the listing.

Conflict resolution calls `performRename(target, newName, force: true)` after "Overwrite and trash/delete". The
`moveToTrash` call in the overwrite-trash path also has timeout detection (persistent toast + refresh).

## Permission check on activation (`checkRenamePermission(path)`)

Verifies: parent dir writable (Unix `access(W_OK)`), file not immutable (`UF_IMMUTABLE`), file not SIP-protected
(`SF_IMMUTABLE`). On failure, auto-cancel and notify. On read-only volumes, show modal alert "This is a read-only
volume. Renaming isn't possible here." Skipped for MTP volumes (Unix `access()` doesn't work on MTP virtual paths).

## Validation

- **Frontend (instant)** `filename-validation.ts`: disallowed chars (slash, null on macOS), empty/whitespace-only, byte
  limits (255 name, 1024 path), extension change vs setting. The red border while editing follows the policy: "no" draws
  it on an extension change, "ask" doesn't (the dialog waits for save), "yes" never validates the extension.
- **Backend (authoritative)** `validation.rs` + `check_rename_validity` command, accepts an optional `volumeId`: local
  FS (`None` or `"root"`) uses `symlink_metadata` + inode comparison for case-only detection; non-local volumes (MTP)
  use `Volume::get_metadata()` for conflict detection, `is_case_only_rename` always `false` (MTP is case-sensitive).

## Post-rename cursor tracking

File watcher emits `directory-diff` → `findFileIndex(listingId, newName)` → frontend index → `setCursorIndex()`. If
renamed to a dot-prefixed name while hidden files are off, show "Your file disappeared from view because hidden files
aren't shown." The `moveCursorToNewFolder()` pattern: subscribe to `directory-diff`, wait 50 ms after the event for the
listing cache to update, query `findFileIndex()`, clean up the listener after a 3 s timeout.

## Rename sessions

One session is one activation of the editor, from the moment it opens until the save it sent comes back.
`rename-state.svelte.ts` numbers them: `activate()` takes the next `sessionId`, and `isSuperseded(id)` answers whether a
NEWER activation has happened since. A plain `cancel()` keeps the id, so ending the editing session (Escape, a blur)
doesn't make the save that's already on its way a stranger to its own result.

Everything that finishes asynchronously carries the id it started with: the save (`executeFlow` captures it beside the
target), the permission check and sibling-name load (`activateRename`), the conflict dialog's follow-up rename, and the
editor's own cancel.

**A superseded session may speak, never steer.** `handleRenameResult` routes a late result to `reportSupersededResult`,
which can only toast and refresh the listing in the background. It cannot cancel (that would close the editor the user
is typing in), restore focus, shake (that would blame the file now on screen for a problem with a different one), set
`pendingCursorName` (which makes `listing-diff-sync` jump the cursor to the renamed file and skip index reconciliation
entirely), or open the conflict / extension dialog (nobody can answer a question about a file they've moved past). Those
moves aren't guarded inside the superseded branch, they're absent from it.

`InlineRenameEditor` takes its `sessionId` as a prop and reads it ONCE, at mount (`openedForSession`). An editor blurs
on its way out when another takes over, and that blur cancels; reporting the session live by then would discard an edit
that has already started. Reading it live would reintroduce exactly that. A one-shot "ignore the next cancel" flag is
the wrong shape here: `suppressBlurCancel` shows how a one-shot waiting for a blur that never comes eats the user's next
Escape instead.

## Chaining the rename with the arrow keys

A bare ArrowDown inside the editor settles the name being typed (usually by saving it, see below) and reopens the editor
on the row below; ArrowUp does the same upwards. Renaming a run of files becomes one keyboard flow. `rename-step.ts` holds both pure halves,
`rename-flow.handleRenameStep` performs the step, and `InlineRenameEditor` raises it.

The step, in the order it must happen:

1. **Resolve the neighbour row** (`resolveStepIndex`): `cursorIndex ± 1`, bounded by the listing and by the `..` row,
   which is nothing to rename. No row there → the key does NOTHING: no commit, no discard, the editor stays open with
   the edit intact. Running off the end of a directory is the user finding the edge, not a decision about the name
   they're typing.
2. **Capture the entry** from the loaded window (`getEntryAt`), or read it with `getFileAt` when it has scrolled out.
   Capturing BEFORE the save goes out is what makes the hop land where the user was looking: the rename may re-sort the
   listing and carry the renamed file far away, and the row they meant is the one that sat beside the editor when they
   pressed the key.
3. **Decide the edit's fate** (`decideStepFate`), and fire the save unawaited when it's a save. Chaining stays fast on
   slow volumes (SMB, MTP), where awaiting each save would stall every step.
4. **Hop**: move the cursor (`applyNavigation`, which also scrolls the row into view) and activate on the captured
   entry.

Two orderings inside that are load-bearing, and both fail silently:

- **The save is fired BEFORE the next activation.** `executeFlow` reads the target, the typed name, and the session id
  synchronously before its first `await`, so firing it first is what tags it with the session that typed it. Activating
  first would hand the in-flight save the NEW session's id, and every supersession guard above would go blind at once.
- **The new editor opens on the captured entry**, through `startRenameOnEntry`, not through `startRename`. `startRename`
  activates on `getEntryUnderCursor()`, which an async IPC read fills from the cursor index: right after the cursor
  moves it still names the file the chain just left, so activating through it would write the next name onto that file.
  That is the paste-rename latch (`b0de3824f`) in a new place. Taking the entry directly makes the target path-bound by
  construction, with no poll to get right.

Nothing rate-limits key repeat: holding the arrow rips through the directory, and session ids are what keep a burst of
steps from crossing each other's results. The scroll in step 4 matters for more than looks: an editor scrolled out of
the virtual window unmounts and its blur discards.

Only a bare ArrowUp / ArrowDown chains, matched on the whole combo through the file list's own `nav.up` / `nav.down`
commands. The caret-to-start/end that a bare arrow used to do inside the input is given up for this; Home and End still
do it.

### What becomes of the edit being stepped away from

Whatever the answer, the hop happens: the user is moving, and only the name they typed is at stake. Nothing actively
discards either, because the activation that follows resets the editor; an edit that isn't sent is simply gone.

- **Unchanged name**: nothing at all. No save, no toast.
- **`rename.severity === 'error'`**: dropped without a round trip, with a `chainKeptOriginalName` toast naming the file
  that kept its own name. This is also where the extension-change policy is honored. A chain has to pass
  `skipExtensionCheck`, since a dialog would ask about a file the user has moved past; under policy "no" the changed
  extension is already a validation ERROR, so it lands here and never reaches the backend. Skipping the dialog therefore
  never becomes overriding the setting, and no second policy read is needed to get that. Under "ask" it grades as fine
  and commits, with the operation log as the way back from a fumbled extension.
- **Anything else**: `executeFlow(skipExtensionCheck: true)`, unawaited.

**A conflict is decided by the BACKEND, never at keypress time.** The frontend's conflict signal is a
`severity: 'warning'` computed against `renameSiblingNames`, snapshotted when the session opened, and a chain rewrites
the directory as it runs: mid-chain that snapshot is stale by construction. Dropping the edit on it would silently throw
away a name that is perfectly free. So the keypress checks only `severity === 'error'`, the edit reaches
`checkRenameValidity`, and the authoritative `{ type: 'conflict' }` is handled in `reportSupersededResult` (a chained
save is always superseded by the time it returns): dropped with the same `chainKeptOriginalName` toast, never a dialog.

Both toasts are `dismissal: 'persistent'`. `handleRenameInput` clears the pane's transient toasts on every keystroke,
which is exactly when the user is typing the next name, so a transient one would be gone before it was read. The toast
stack holds five and silently drops new ones once they're all persistent (`ui/DETAILS.md` § Toast system), so a chain
with more than five failures still loses the tail; aggregating them into one running summary is the fix.

## Ending a rename session

Four ways in, each with a different meaning:

- **Enter** (`onSubmit` → `handleRenameSubmit`): save. An invalid name shakes, toasts, and KEEPS the editor open and
  focused, because the user is still there to fix it.
- **Escape / Tab** (`onCancel` → `handleRenameCancel`): discard.
- **A mouse press outside the editor** (`onClickAway` → `handleRenameClickAway`): save, the way Finder does.
- **Losing focus with no click behind it** (`onblur` → `handleRenameCancel`): discard. This is the structural case: the
  row scrolls out of the virtual window and the input unmounts.

The click-away path keys off a document-level `mousedown` (capture phase, registered by `InlineRenameEditor` for its own
lifetime), NOT off blur. Blur can't tell "the user clicked elsewhere" from "the list scrolled", and committing on the
second would silently rename a file because the user scrolled. Capture phase so it lands before the row and pane
handlers move focus.

Three guards keep the two paths from colliding, all in `rename-flow.svelte.ts`:

- `pendingCommit`: the click's own blur arrives right after the save was sent, and must not cancel the session the save
  still owns (`handleRenameResult` still needs `rename.target` to open a dialog).
- The dialog-state check in `handleRenameClickAway`: the editor stays mounted under the extension/conflict dialogs, so
  pressing a dialog button reaches the click-away handler too.
- `suppressBlurCancel = !commitFromClickAway`: a dialog opening normally blurs the editor, and that blur must not
  cancel. After a click-away the blur was already spent, so arming the one-shot flag would eat the user's next Escape
  instead.

`commitFromClickAway` also gates `restoreFocus()`: the click already decided where focus goes (another row, the other
pane, the breadcrumb), so a save finishing afterwards must not yank it back. Same reason a backend problem reported
after a click-away ends the session instead of shaking: there's no focused field left to fix the name in.

An invalid name plus a click away discards the edit and toasts `fileExplorer.rename.keptOriginalName` (the validation
message plus "Kept the original name."). Trapping the click until the name is valid is the other defensible option, and
what Finder does with a modal alert; Cmdr doesn't trap, and says why instead of failing silently.

## Decisions

- **The inline editor mounts BY PATH, not by index** (`shouldMountRenameEditor(target, row)` in `rename-mount.ts`,
  shared by `FullList` and `BriefList`): it returns `target?.path === row.path`. `RenameTarget` carries no `index`. Why:
  nothing reconciled a stored `target.index` after activation (`listing-diff-sync` reconciles cursor and selection
  indices, never the rename target), so a watcher diff that inserted or removed a row ABOVE the renamed file shifted
  every row's index while the editor stayed pinned to the stale one — rendering the editor, and on save writing the new
  name, onto the WRONG file. That is a data-safety bug of the same class as the paste-rename latch fixed in `b0de3824f`,
  latent only because diffs rarely shift rows mid-rename. Path is the natural key: the row `{#each}` is already keyed by
  `file.path` and Svelte 5 throws on duplicate keys, so path uniqueness within a listing is an enforced invariant. A
  diff that shifts OTHER rows now makes the editor FOLLOW its file; a diff that changes the TARGET's own path (external
  rename/delete) is a removal → the existing `listing-diff-sync` cancel (which path-compares `c.entry.path`), not a
  follow. The compiler enforces the switch: deleting `index` from `RenameTarget` turns any surviving index comparison
  into a type error.
- **Separate components in `file-explorer/rename/`**: rename is tightly coupled to FilePane rendering (replaces the name
  cell inline) and uses `$state()` (requires `.svelte.ts`). Transfer operations are self-contained dialogs that don't
  touch FilePane internals. The separation reflects the architectural boundary.
- **Inode comparison for conflict detection**: on case-insensitive APFS, `readme.txt` → `README.txt` is valid (same
  file). A naive `exists()` check flags a false positive; comparing `dev+ino` via `symlink_metadata()` detects case-only
  renames correctly.
- **Three separate dialogs (conflict, extension, permission)**: each triggers at a different stage (permission on
  activation; extension and conflict mid-flow, can continue editing). Combining them would need complex multi-state
  logic; separate dialogs keep each concern isolated.
- **Trim silently instead of erroring**: leading/trailing whitespace is almost always unintentional. The input preserves
  what the user typed (transparency) while save logic uses the trimmed value.

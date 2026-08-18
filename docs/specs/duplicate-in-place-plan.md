# Duplicate in place

Copying an item into the folder it already lives in produces a copy under a new name, instead of the
"Can't copy to the same location" refusal Cmdr shows today.

From [#50](https://github.com/vdavid/cmdr/issues/50) ("Should allow copy / pasting to same location") and a separate
in-app report about dragging onto the folder an item is already in. Duplicating a file is a common thing to want, and
every mainstream file manager offers it.

## The product decision

Both mainstream conventions agree that a same-folder paste is an intent, never an error. They disagree about *when* the
name is chosen:

- **Finder**: naming happens after. ⌘V or ⌘D creates `photo copy.jpg` instantly, no prompt. Rename it later if you care.
- **Total Commander**: naming happens before. F5 opens the copy dialog with an editable target field, so duplicating is
  "F5, edit the name, Enter".

Issue #50 asks for a modal "enter a new name" prompt. We're not doing that as the default: it taxes every paste for the
minority of cases where the generated name isn't fine, and a blocking modal is the wrong shape in a keyboard-first app.

What we do instead serves both camps:

1. **A copy into the same folder auto-renames, silently.** No conflict dialog, no prompt, no error.
2. **Paste and F5 additionally open inline rename** on the new item when exactly one resulted, stem selected, the way
   pasted clipboard content already does. Esc keeps the generated name. This is the answer to #50: whoever wants to name
   the copy names it immediately with no modal, and whoever doesn't is already done.
3. **A move into the same folder does nothing, silently.** It's already where it was asked to go.
4. **An explicit Duplicate command**, so the intent is first-class rather than a side effect of paste.
5. **Drag and drop** duplicates too, with one guard against sloppy drags (M7).

### Which gestures open the rename editor, and why not all of them

**Paste and F5 do. Duplicate (⌘D) and drag don't.** This is a deliberate split, not an oversight, and getting it backwards
would undercut the whole feature:

- ⌘D *is* Finder's Duplicate, and the reason M6 wants that binding is precisely that Finder's ⌘D asks nothing. Popping an
  editor on it would ship the opposite of the behavior whose familiarity justifies the key. It would also break stamping
  out several copies in a row: after the first ⌘D, focus sits in an editor and the second ⌘D does nothing until Esc.
- A drag ends with the mouse, not the keyboard. Stealing focus into a text field on mouse-release is the wrong shape.
- Paste and F5 are the gestures where the user has just *directed* a copy somewhere, and they're the ones #50 is actually
  about ("all file managers I used so far would ask for a new name **when pasting**").

**No setting gates this.** The same reasoning that rejects a modal rejects a toggle: two gestures that ask and two that
don't already covers both preferences, so a preference to configure would be a preference nobody needs to find.

### Why not one prompt per item

A same-folder paste of 30 files must not raise 30 conflict prompts. **A self-collision is not a conflict**: a conflict is
"something else is in the way, what do you want", and this is "the thing in the way is the file itself, which is exactly
what duplicating means". It gets resolved before the conflict machinery is consulted, and never reaches the user.

### The generated name: `photo (1).jpg`

Cmdr already generates ` (1)`, ` (2)` for transfer conflicts (`conflict.rs::numbered_name`, documented as "the ONE
` (N)` formatter"), so a duplicate reusing that scheme keeps one naming vocabulary for what is structurally one
operation. It's also language-neutral, which matters: Cmdr is translated, so a `copy` suffix would owe every language a
reviewed word, and then owe the sequence parser every one of those words when continuing a series.

**The tension worth naming**: `docs/design-principles.md` says platform-native over generic, and macOS Finder says
`photo copy.jpg` (localized: Hungarian Finder writes `photo másolata`). That's a real argument for `copy`, and the reason
to take it seriously is that Cmdr is macOS-only today. It loses to the consistency argument here only because switching
just the duplicate path would leave Cmdr generating `photo copy.jpg` on a duplicate and `photo (1).jpg` on a conflict,
which is worse than either scheme alone; and switching *both* means re-reviewing the suffix in every shipped language.

So the decision is `(1)`, and the hedge is that **M1 collapses the three current numbering implementations into one**.
Changing the scheme later becomes a one-place edit plus its tests, not a hunt.

## The design

### The core idea: self-collision is a per-item identity question

Today `validate_not_same_location` (`validation.rs:58`) rejects the whole operation up front when any source's parent
equals the destination. That's the wrong altitude in three ways:

- It's an **operation-level** verdict on a **per-item** question, all-or-nothing: a clipboard holding paths from two
  folders, pasted into one of them, is refused wholesale today even though only some items are self-collisions.
- It's a **lexical** comparison (`source.parent() == destination`), so it misses a symlinked parent, a case-differing
  path on APFS, and NFC/NFD normalization differences (which macOS produces routinely, and which the clipboard can
  introduce). Those already slip past it today and land in the conflict machinery.
- It answers **before** the destination path for an item is even known, so it can't be the place that decides what to do
  instead.

The replacement asks the question where the answer is actionable: at the moment an item's destination path is final,
**is this destination the same file as the source?** Answered by comparing `dev+ino` rather than paths, which settles
case-insensitivity, normalization, symlinked parents, and hardlinks in one shot.

`validation.rs:308` already has exactly this as `is_same_file`, written for this hazard ("prevents data loss when copying
a file over itself via a symlink"). **Reuse it as-is, and note that it uses `fs::metadata`, which follows symlinks** —
that is the correct choice here and it's what makes the symlinked-parent case work. Don't "fix" it to `symlink_metadata`;
that would break the very case the plan tests for. (The rename module answers a narrower question with `symlink_metadata`
because it must treat a symlink as its own entry; the two are deliberately different.)

On a self-collision:

- **copy** redirects to a unique name and proceeds. No prompt, no policy consulted.
- **move** counts the item done and writes nothing.

Non-local volumes (SMB, MTP) have no meaningful `dev+ino`, so there the comparison is same-volume-id plus a folded-path
one, reusing `dest_name_index.rs::fold` (NFC + lowercase), already the project's answer to "would this backend treat
these two names as the same".

### This is a data-safety change, not only a UX one

The guard is currently the only thing standing between a self-collision and the general conflict machinery, and **every
answer that machinery can give is wrong for this shape**:

- `Overwrite` (reachable today via an apply-to-all latch from an earlier real conflict in the same batch, or a
  configured default) sends the item through `stage_and_land_file`, whose replace path renames the existing destination
  aside and deletes it afterwards. When destination *is* the source, that deletes the original: same name, same bytes,
  **new inode**. Hard links to the original break, birth-time resets, inode-keyed metadata can drop, for zero benefit.
- `Overwrite` on a *directory* is worse: `safe_overwrite_dir` sets the source directory aside and recreates it empty,
  then the copy walks a file list whose paths point at the vanished original and fails mid-operation. Nothing tests this
  today.
- `Stop` shows a nonsensical "this file conflicts with itself" prompt, with identical size and mtime on both sides.
- `Skip` silently does nothing, which is the opposite of what the user asked for.

**Consequence for sequencing: removing the guard and adding the self-collision rule must be the same commit.** Never land
the removal on its own, not even briefly.

### What stays untouched

`validate_destination_not_inside_source` is a different guard for a different hazard (copying a folder into its own
subtree, which recurses until the disk fills) and stays exactly as it is. Duplicating `docs/` produces the sibling
`docs (1)/`, never a child, so the two never interact.

Rollback is already correct and needs no change: `copy/rollback.rs` deletes only `transaction.created_files` /
`created_dirs`, which are paths the operation itself created, so rolling back a duplicate removes `photo (1).jpg` and can
never touch the original. Pin it with a test anyway (M2), because it's exactly the kind of claim this plan otherwise
insists on anchoring.

### Explicitly out of scope

**Duplicating inside a zip.** Archive edits are a third, independent pipeline (`archive_edit/conflicts.rs`) with its own
`find_unique_inner`, and no same-location guard, so a source pasted into its own folder inside a zip already behaves
tolerably by accident. Fixing its self-conflict prompt is a follow-up; note it in `archive_edit/DETAILS.md` rather than
widening this effort.

## Milestones

Sequential. Each ends green with the checks named, and is its own commit.

### M1. One numbering implementation, and it continues a sequence

**Why first**: it's pure logic, it's self-contained, and the feature feels broken without the sequence rule. Duplicating
twice must give `photo (1).jpg` then `photo (2).jpg`, not `photo (1) (1).jpg`.

**(a) Collapse the duplicated numbering.** `conflict.rs::numbered_name:356` is documented as the one ` (N)` formatter, but
`transfer/volume/conflict.rs::find_unique_volume_name:632` hand-rolls the same format string inline instead of calling
it. Point it at `numbered_name`. Without this, part (b) fixes the local path and silently leaves the volume path
generating `photo (1) (1)`. It's a clean drop-in: `numbered_name` is `pub(super)` on `write_operations::conflict` and the
volume module is a descendant, it's sync and called from non-async code inside the loop, and the volume loop starts at
counter 1 so `numbered_name`'s `counter == 0` branch is simply never taken.

(`archive_edit/conflicts.rs::find_unique_inner` is the third copy; it works on zip inner-path strings and is out of scope
per above, but leave a comment there so the next reader knows it's deliberate.)

**(b) Continue a trailing sequence.** Teach the ` (N)` search to split a trailing ` (<digits>)` off the stem and continue
from there.

The shared piece is a pure `split_sequence(stem) -> (base, next_counter)`, **not** a shared search loop:
`find_unique_name` has to keep advancing when its `O_CREAT|O_EXCL` create loses a race, so its loop and a non-reserving
one can't be layered as "search, then reserve". Add `next_available_name(path)` alongside it — same `split_sequence`,
same `numbered_name`, but an `!exists()` probe and no create. M2 needs that non-reserving half; adding it here keeps M2
free of naming changes.

The sequence rules:

- Non-numeric parentheticals are not a sequence: `Report (final).pdf` → `Report (final) (1).pdf`.
- A number that doesn't fit `u32` is not a sequence either; treat it as literal text.
- No zero-padding preservation: `photo (007).jpg` → `photo (8).jpg`.
- `photo (0).jpg` → `photo (1).jpg`.

This changes the existing conflict-rename path too, which is intended: copying `photo (1).jpg` into a folder that already
holds one gives `photo (2).jpg`, which is the better answer there as well.

**Tests (TDD, real red first)**: unit tests beside the function in `conflict.rs` (`mod find_unique_name_tests:1174`
already exists), covering each bullet plus the existing no-sequence cases unchanged; and the volume-side naming, to pin
that (a) actually took.

⚠️ `validation_integration_test.rs:68-88` defines a **private duplicate `find_unique_name`** that reimplements the old
non-atomic `!exists()` algorithm. Its tests exercise that copy, not production code, so they will happily stay green
through a real regression. Delete the local helper and point those tests at the real function.

**Checks**: `pnpm check rust`.

### M2. Self-collision in the local engine

**One commit**: add the identity check and its two outcomes, and in the same change delete `validate_not_same_location`,
its call sites (`write_operations/mod.rs:474`, `:523`), and the now-unreachable `WriteOperationError::SameLocation`
variant (`types.rs:550`). See "This is a data-safety change" above for why these can't be separate commits.

This one milestone covers the whole local story, including everything the frontend sends: the frontend always dispatches
copy through `copyBetweenVolumes`, and `transfer/volume/copy.rs:140-173` delegates a both-local copy straight to
`copy_files_start` (`move.rs:118` likewise). Paste, F5, drag, and MCP all land here. MCP is not a bypass: its copy/move
executors drive the same frontend dialog and the same Tauri commands a person does.

**Ask it per top-level source, once, before the copy loop — never per file.** The copy loop iterates the scan's *leaf*
files (`docs/a.txt`, `docs/sub/b.txt`), and for a same-folder duplicate every one of those leaves is its own
self-collision. Checking per leaf would scatter `a (1).txt` through the original `docs/` instead of producing `docs (1)/`.
The question belongs to the top-level source.

The reason a top-level rename doesn't propagate on its own: `FileInfo::dest_path()` (`scan_cache.rs:629`) recomputes
`destination.join(relative)` from the original relative path on every call, with no memory of any rename. The mechanism
that does propagate is `dir_remap` (`transfer/copy/mod.rs::apply_dir_remap:65`), which rewrites a destination by its
longest mapped ancestor and is already applied at every per-file dest-path use: `single_item.rs:153`, `:257`, and
`scanned_dirs.rs:50`. Today it's populated in exactly one place (`single_item.rs:255`, the folder-over-file rename).

So in `copy_files_with_progress_inner`, right after `transaction` / `created_dirs` / `dir_remap` are declared
(`copy/mod.rs:192-195`) and before the copy loop: for each top-level source, compute `destination.join(file_name)`; if
that's the same file as the source, reserve a unique name and `dir_remap.insert(original_dest, unique_dest)`. Everything
downstream then follows for free, the file loop and the later scanned-dirs pass alike. A plain file collapses into the
same rule (its top level is itself, and `starts_with` matches a path against itself).

**Three things to get right:**

- **Don't pre-seed with `find_unique_name`.** It's tempting, since it's the existing name picker, but it reserves the
  name by creating an `O_CREAT|O_EXCL` 0-byte **file**. Pre-seeding with it means the copy then finds its own placeholder
  sitting at `dest_path`, `path_exists_or_is_symlink` says occupied, and `copy_single_item` raises a conflict prompt
  against the placeholder — the exact prompt this feature exists to remove, and under the default `Stop` policy the user
  sees it every time.

  Use the **non-reserving** `next_available_name(path)` from M1 for the remap instead. Then:
  - a **file** source needs nothing further: `dest_path` doesn't exist, the normal write path runs, and the non-overwrite
    landing already refuses an occupied destination via `RENAME_EXCL` (`transfer/CLAUDE.md` § Streaming). A concurrent
    writer that steals the name between the pick and the landing produces a loud, safe failure rather than a clobber.
  - a **directory** source claims its name eagerly with `create_dir` (not `create_dir_all`), advancing to the next
    candidate on `AlreadyExists` — that loop *is* the atomic reservation, and it's the directory analogue of what
    `find_unique_name` does for files. Record it on the transaction and in `created_dirs`.

  The alternative (keep `find_unique_name` and thread a reserved-paths set into `copy_single_item` so it skips the
  conflict check on its own placeholder) also works, but it puts a new condition inside the riskiest branch in the file.
  Prefer the version that leaves that branch untouched.

- **Empty directories.** An empty folder produces no `FileInfo` at all, and `create_scanned_dirs_at_destination`
  (`scanned_dirs.rs:51`) treats an existing destination as nothing to do. It only works because the pre-seed happens
  before the loop and `scanned_dirs.rs:50` already applies the remap. Pin it with a test; this is invisible today because
  the up-front guard rejects it first.

- **Move must skip before it recurses.** `dest_path` is computed at `move_op.rs:189` and the dir/dir merge branch is at
  `:216`. `merge_move_directory` (`:393-483`) threads `dest_dir` down through recursion, so a self-collision would
  self-merge: every leaf either renames to itself (a POSIX no-op) or gets shuffled aside to `name (1)`, depending on
  policy. No data loss, but nonsense. Drop a self-colliding top-level source right after `:189`, before `:216` is
  reached.

`resolve_conflict` itself stays about the user's conflict policy and learns nothing about duplication.

**Tests (TDD, real red first — this is the risky logic)**:

- A file duplicated in place lands as `photo (1).jpg`, original untouched and byte-identical, **same inode** (the
  regression anchor for the aside-and-delete hazard).
- Duplicating it again gives `photo (2).jpg`; both earlier copies survive.
- A folder duplicated in place lands as `docs (1)/` with the full subtree inside, `docs/` unchanged. Pin the failure this
  design exists to prevent: no `a (1).txt` anywhere inside the original `docs/`, and no conflict event.
- An **empty** folder duplicated in place produces an empty `docs (1)/`.
- A move into the same folder leaves the item alone and reports it done, with no conflict event and no `name (1)`
  anywhere.
- A **mixed multi-source** copy (one source already in the destination, one from elsewhere) duplicates the first and
  copies the second, with no prompt for either. This is the all-or-nothing behavior the old guard got wrong.
- A source reached through a symlinked parent resolving to the destination is a self-collision (the case the lexical
  guard missed).
- Duplicating under a non-default policy (`Overwrite`, and an apply-to-all `Overwrite` latch) still duplicates and still
  never touches the original. This is the data-safety pin.
- Rolling back a duplicate removes `photo (1).jpg` and leaves the original intact.
- Regression anchor: a genuine conflict (different file, same name) still raises the normal conflict flow, untouched.

**Docs**: `write_operations/DETAILS.md` gets a "Self-collision" section (the decision, the `dev+ino` rationale, the
per-top-level-source placement, the rollback property, and why the guard removal is inseparable from it);
`write_operations/CLAUDE.md` gets one guardrail line. Drop every `SameLocation` mention. `docs/architecture.md` needs no
change (no new subsystem, no moved file), which is worth confirming rather than assuming.

**Checks**: `pnpm check rust`.

### M3. The cross-volume engine stops asking a nonsensical question

Smaller than it looks. There is **no same-location guard on the true cross-volume path at all**, so an MTP→MTP or
SMB→SMB duplicate inside one folder isn't blocked today: under `Rename` it already produces a correct duplicate by
accident of the general machinery, and under the default `Stop` it raises a "this file conflicts with itself" prompt
showing identical size and mtime on both sides. This milestone replaces that prompt with the same silent auto-rename.

Add the identity check at the top of `resolve_volume_conflict` (`transfer/volume/conflict.rs:50`), before the dir/dir
merge short-circuit at `:103`. **No `dir_remap` equivalent is needed here**: the volume engine threads `dest_path` down
through `merge_level` (`volume/merge.rs:363`, with `child_dest = dest_path.join(&entry.name)` at `:443`; note it's
`merge_level` that recurses, not its caller `copy_directory_streaming:305`), so a top-level rename propagates through the
subtree on its own. That asymmetry with the local path is worth a line in the docs.

If the folded-path comparison ends up needing `dest_name_index.rs::fold`, that function is currently a private `fn` and
its visibility has to widen.

**Tests**: unit tests at the volume conflict seam. The identity rule is what's being pinned, not the transport, so unit
coverage is enough unless the SMB fixture suite already has a same-share copy to extend.

**Docs**: `transfer/volume/DETAILS.md`, including the propagation asymmetry.

**Checks**: `pnpm check rust`.

### M4. The pre-flight stops inventing conflicts

**This is the milestone most likely to be skipped and most likely to break the feature silently.** It's a backend change
despite sounding like UI.

Before confirm, `TransferDialog` runs a top-level conflict check (`transfer-conflict-check.svelte.ts` →
`scanVolumeForConflicts`), a single destination listing matched **by name**. In a same-folder copy every source name is
present at the destination, so without a fix the dialog announces "12 conflicts", shows the overwrite/skip/rename policy
radios, and hands the backend a `preKnownConflicts` list naming every source. Under `Skip all` the backend then
bulk-skips all of them and **the duplicate silently does nothing**.

This affects **F5/F6 and drag and drop**, both of which go through `TransferDialog`
(`drag-drop-controller.svelte.ts:186`, `:213`). Paste is genuinely unaffected: `pasteFromClipboard`
(`clipboard-operations.ts:288`) calls `dialogs.startTransferProgress(...)` directly and never runs this precheck.

**Where to fix it.** The per-backend `scan_for_conflicts` implementations (`local_posix.rs:714`, `smb/scan.rs:445`,
`mtp/scan.rs:208`) can't answer this: they receive `SourceItemInfo`, which carries a *name* and no source path
(`cmdr-fs/src/volume/types.rs:303`). Rather than widen that DTO across three backends and their test doubles, filter one
level up in `scan_volume_for_conflicts_within` (`commands/file_system/volume_copy.rs:361`), which already receives
`source_paths: Option<Vec<String>>`: drop any returned `ScanConflict` whose `dest_path` is the same file as one of the
sources. One place, one definition of self-collision, no trait churn.

Confirm the dialog actually passes `source_paths` (it has `getSourcePaths` in its deps); the parameter is optional, and
the filter is inert without it.

**Also in this milestone, the frontend refusals go:**

- Remove the "already there" rejection from `transfer-dialog-logic.ts:56-63` (`normDest === sourceParent` →
  `pathErrorAlreadyThere`) **for copy**. Keep the subfolder rejection at `:52`, which is the real hazard, and keep
  "already there" for move.
- Add the paste fast path: a move-paste whose sources are all already in the destination does nothing at all — no dialog,
  no toast, no "moved 0 files". A partial set still runs, and the backend skips the ones already there.

**Retire the `sameLocation` strings from every locale, not just English.** `errors.write.sameLocation.*` exists in
`en/errors.json:1660-1677` **and** in the de, es, fr, hu, nl, pt, sv, vi, and zh catalogs. Nothing automates this:
`desktop-message-keys-unused` only scans `en` against source usage, and `pnpm intl:keys` only regenerates `keys.gen.ts`
from `en`. Deleting only the English keys passes every check and leaves nine orphaned translations shipping forever.
Delete all ten by hand, plus `transfer-error-messages.ts` and the parity tests pinning them, then run `pnpm intl:keys`.

**The completion toast needs no change**, and that's worth stating rather than discovering: a self-collision never enters
the skip or conflict counters, so `transfer-complete-toast.ts` composes an ordinary "Copied 1 file" / "Copied 12 files".
Assert it once so a future counter change can't quietly turn a duplicate into "Copied 0 files".

**Cursor and selection after a multi-item duplicate are unchanged** — the same behavior as any multi-item paste today.
Only the single-item case (M5) moves the cursor.

**Tests**: a backend unit test that the conflict scan returns zero for a same-folder source set; a frontend test that the
dialog shows no conflict count and sends an empty `preKnownConflicts`; `clipboard-operations.test.ts` (move-all-already-
there is a no-op; copy is not; a partial move still dispatches); `transfer-dialog-logic.test.ts` (copy into the source's
own parent validates clean; move and the subfolder case still reject); the toast assertion above. E2E in
`file-operations.spec.ts`: ⌘C then ⌘V in one pane produces the numbered copy.

**Checks**: `pnpm check`, then the E2E spec.

### M5. Inline rename on a single-item duplicate, from paste and F5 only

After a paste-or-F5 duplicate settles and produced exactly one item, open the inline rename editor on it with the stem
selected. **Not from ⌘D, not from drag** — see "Which gestures open the rename editor" above; the trigger has to be
plumbed explicitly rather than inferred from "this was a same-folder copy".

**The frontend must never recompute the generated name.** Nothing in the event stream carries it: `WriteCompleteEvent`
and `WriteSettledEvent` are counts and ids only, and `WriteProgressEvent.currentFile` is a filename mid-flight with no
promise of being the last one. Read it from the operation journal instead: `getOperationLogDetail(operationId, 1, 0)`
returns `items[0].destPath` (`tauri-commands/operation-log.ts:41-49`). One extra IPC round-trip after settle, and it's
the real resolved name rather than a frontend reimplementation of `numbered_name` that would rot the moment M1's sequence
rule changes.

Journal capture is synchronous per item during the write, so the row is durable by the time the terminal event fires.
That ordering isn't written down as a contract anywhere, so **treat an empty or missing `destPath` as "skip the
rename"**, never as an error, and never as a reason to retry in a loop.

**The activation pattern is fully precedented** — copy `pane/paste-clipboard-as-file.ts:67-88`:

1. `moveCursorToNewFolder(...)` (`file-operations/mkdir/new-folder-operations.ts:32-77`) arms `setPendingCursorName(name)`,
   which `listing-diff-sync.svelte.ts:173-176` consumes on the next directory diff, with an immediate `findFileIndex`
   attempt and a bounded retry for the diff that already fired.
2. `paneRef.startRename({ suppressExtensionWarning: true, expectedName })`. `expectedName` refuses to activate unless the
   entry under the cursor is exactly that name, retries briefly while the diff lands, then gives up silently
   (`pane/types.ts:20-30`) — exactly the "don't rename the wrong file" guarantee this needs.
   `suppressExtensionWarning` matters: editing an auto-numbered stem must not raise the extension-change confirm.

Don't fire when more than one item resulted, when the operation was cancelled, or when the user navigated away. The
`expectedName` guard covers navigation on its own, but check the first two explicitly rather than leaning on it.

**Tests**: unit tests on the activation decision (single item from paste opens the editor; multiple don't; cancelled
doesn't; missing `destPath` doesn't; **a ⌘D duplicate doesn't**). E2E: ⌘V in the same folder leaves the editor open on the
new file, and Esc keeps the generated name.

**Docs**: `file-explorer/rename/DETAILS.md` (programmatic activation entry point), `file-operations/DETAILS.md` (the
settle tail, and which triggers opt in).

**Checks**: `pnpm check desktop`, then the E2E spec.

### M6. The Duplicate command

A first-class command in the palette, the context menu, and the menu bar, operating on the selection (or the cursor item
when nothing is selected). No rename editor; see M5.

**Files a new command has to touch** (`file.rename` / `file.copy` are the templates; this list is the result of tracing
`file.rename` end to end):

- `commands/command-ids.ts` (add `file.duplicate` to `COMMAND_IDS`), `commands/sources/file-list.ts` (the `CommandSource`
  entry), `routes/(main)/command-handlers/file-handlers.ts` (the handler).
- `shortcuts/shortcuts-store.ts`'s `menuCommands` array, which its own docs require to stay in sync with the Rust menu
  items.
- `routes/(main)/mcp-listeners.ts`, which carries a typed const and dispatch per command.
- `intl/messages/en/commands.json` (label + its `@` description), then `pnpm intl:keys`.
- The native menus are **hand-built per platform in Rust and are not derived from the TS registry**:
  `src-tauri/src/menu/command_map.rs` (the id const plus both directions of the map), `menu/menu_structure.rs:150-158`
  (the right-click Copy/Move/Rename group), and `menu/macos.rs` + `menu/linux.rs` for the menu bar.
  `commands/rust-command-id-drift.test.ts` parses `menu_id_to_command` and fails if the Rust side is missing.
- Pinned test inventories: `command-registry.parity.test.ts` (name and description parity) and
  `command-registry.test.ts` (`EXPECTED_PALETTE_IDS`).

Check whether `function-key-commands.ts` (the F-key bar) wants an entry. It probably doesn't — the bar is full and
Duplicate isn't an F-key idiom in either Finder or Total Commander — but rule it out deliberately.

**On ⌘D**: it's the Finder-native binding and it's free wherever the file list is actually showing, but
`errorPane.toggleTechnicalDetails` claims it as a `fixedKey` command via a capture-phase listener
(`commands/sources/file-list.ts:103-108`, `command-registry.ts:69` "Family 4 — deliberate override"). The two never
overlap in practice: the error screen shows *instead of* the file list, and there's nothing to duplicate there. The catch
is that `fixedKey` commands are exempt from conflict detection, so a second claim on the chord wouldn't be flagged by
anything.

Bind ⌘D and run `registry-conflicts.test.ts`. **If it objects, ship the command unbound** (palette and context menu only)
and leave the binding to David: a shortcut collision is his call, not one to resolve unilaterally.

🧑 **New user-facing copy needs David's sign-off**: the command label ("Duplicate") and its palette description are the
only new strings this effort ships. Per AGENTS.md principle 4 they're a draft until he's looked at them. Flag it in the
final report rather than treating the milestone as closed.

**Tests**: the pinned inventories above, plus a unit test that Duplicate dispatches a same-folder copy of the selection,
and of the cursor item when the selection is empty.

**Docs**: `commands/CLAUDE.md` and `commands/DETAILS.md` document the registry, tuple, and handler contract this
milestone touches; update them if the new command changes anything they state. If it doesn't (command data lives in
`sources/*.ts`, already covered by the module map), say so in the commit rather than leaving it ambiguous.

**Checks**: `pnpm check desktop`.

### M7. Drag and drop

A drop that **crosses into the other pane** duplicates, even when both panes show the same folder: that's a deliberate
act. A plain drag that **starts and ends inside its own pane** stays a no-op, silently, because an aborted drag is common
and Finder deliberately does nothing there. An explicit copy-drag (⌥ held) duplicates either way. No rename editor; see
M5.

The change is one condition: `pane/drag-drop-controller.svelte.ts:449-450` suppresses every same-pane background drop
unconditionally today. It becomes conditional on the resolved operation being `copy`, which `pickDropOperation`
(`drag/drop-operation.ts:55-68`) already derives from `getModifierState()` (⌥ held → copy). The narrower
`isInvalidSelfDescendantDrop` guard (`drag/drop-target-validation.ts:5-7`) stays as it is: dropping an item *onto itself*
is still nothing.

**Tests**: unit tests on the drop decision for each of the three cases.

E2E can't reach this one, and the reason is specific rather than general: `drag-drop-entry.spec.ts`'s `triggerFileDrop` /
`triggerSelfFileDrop` helpers call `dragDrop.handleFileDrop(...)` directly, which is *downstream* of the suppression gate
this milestone modifies. The adjacent infrastructure looks like free coverage and isn't. Say that in the commit so the
next reader doesn't re-derive it.

**Docs**: `file-explorer/drag/DETAILS.md`.

**Checks**: `pnpm check desktop`.

### M8. Wrap

- Full `pnpm check`, then `--include-slow`.
- Add two lines to `docs/guides/testing/manual-checklist.md`, whose stated purpose is exactly the two categories this
  effort touches: "Duplicate appears in the context menu and menu bar", and "a same-pane drag stays a no-op while a
  cross-pane drag duplicates".
- Update `docs/specs/index.md`.
- Manual QA in the running app (drag excepted).
- Consider whether the MCP `copy` tool description needs a word: it currently says "Copy the selection… **to the other
  pane**" (`tool_registry/mod.rs:293`), which no longer describes everything copy can do. `file.duplicate` gets no MCP
  tool of its own in this effort; note that as a deliberate omission.

## Notes for whoever executes this

- **Nothing here runs in parallel.** M2 depends on M1's naming, M4-M7 depend on the backend accepting the operation, and
  M5 depends on M4. The work is small enough that sequential costs little.
- **M1, M2, and M4 are load-bearing.** M4 is easy to mistake for polish; without it F5 and drag duplicates still show the
  conflict radios and can silently no-op. M5 is the one most likely to be dropped, and dropping it leaves a coherent
  feature.
- **Don't let the F5 dialog grow a target-name field.** It looks deceptively close: the field is a free-text `editedPath`
  passed straight to `onConfirm`, and compress already pre-fills a full path ending in a new `.zip` leaf
  (`transfer-dialog-utils.ts:181-198`). But copy's backend treats the destination as a *folder*: `ensure_destination_dir`
  creates it and requires it to be a directory, so a leaf-named target would produce a **folder** called `photo (1).jpg`.
  Making it work needs per-item target names across every transfer engine. Inline rename gives the Total Commander user
  the same outcome with one keystroke sequence and no new API. If it ever becomes worth it, the extension is a per-source
  name map on the copy command, not a special case for the single-source path.
- **Duplicating on APFS is free and instant.** Same-volume copies already go through `copyfile(3)` with `COPYFILE_CLONE`
  (`transfer/copy_strategy.rs`), so duplicating a 10 GB video costs no space and no wait. Worth not breaking, and worth a
  line in the release notes.
- **Watch the `❌` budget.** `invariant-density` only goes down. If a rule here wants writing, prefer encoding it in a
  type: an outcome enum the caller must match on beats a comment saying don't forget the remap. Don't hand-edit any
  allowlist; run the check and commit its rewrite.

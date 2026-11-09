# Transfer tree (frontend): details

Must-knows: `CLAUDE.md`. The view command's contract: `crates/cmdr-operations/src/transfer_tree/DETAILS.md` § "The
view". Issue: [#382](https://github.com/getcmdr/cmdr/issues/382).

## What it promises

One bar for a six-folder copy can't say which folder is untouched, which file is in flight, or whether the first three
landed. These are the product calls (David, 2026-10-11) the code holds:

- **Rows are the selection, as a tree** rooted at the sources' common parent, every label relative to it. A single-child
  folder chain is one row (`a/b`, like VS Code); a file never compacts into its folder.
- **The tree follows the work**: folders holding it open, a folder shuts once the work left it, and a folder the user
  toggled stays as they left it. A user's scroll or keyboard move pauses following until Follow (or F).
- **Everything stays reachable**: 5,000 top-level files is a 5,000-row virtual list, and a finished row keeps its
  checkmark, ❌ never folded into "N done".
- **Rows below the work really are next**: every driver starts files in tree order
  (`crates/cmdr-operations/src/transfer/DETAILS.md` § "Processing order"). Parallel drivers show several active rows.
- **One axis for every folder bar**: the operation's bottleneck (files or bytes), the same one its ETA follows.
- **Out of scope**, each its own follow-up: skipping the rest of a folder, keeping a FAILED operation's tree on its
  error dialog (#261), trees for delete, trash, and edits inside an existing archive, and remembering a resized dialog's
  height.

## Using it

```svelte
<TransferTree operationId={id} tick={session.progress} operation="copy" defaultRows={8} />
```

- `operationId` (a running operation) or `source` (a fixture), fixed for the tree's life: re-key the tree to switch.
- `tick`: anything that changes when the operation advanced; the session's `progress` is the natural one.
- `operation`: `'copy' | 'move' | 'compress'`, which words the active state ("Copying").
- `defaultRows`: the most rows tall it gets before the host gives it more (never below three). It takes only the rows it
  has (a single file is one row) and grows as rows arrive, never shrinking back: `--spacing-tree-rows` is the high-water
  mark of `min(totalRows, defaultRows)`, so a folder shutting as the work moves on doesn't bounce the host's layout. The
  root is a `flex: 1 1 auto` column with that height as its basis and `min(rows, 3)` rows as its floor, so a resizable
  dialog's growing child stretches it and a squeezed one keeps three rows.

- `memory` (optional, `createTransferTreeMemory()`): where the host keeps the tree while something else takes its place.
  The tree leaves its hand toggles, following, scroll position, and grown height there when it unmounts, and the next
  tree over the same operation (or fixture) picks them up; a different operation starts fresh. The dialog passes one,
  since its conflict prompt and reversals replace the body.
- `tabbable` (default true): whether Tab reaches the tree's tab-stop row. The queue clears it on every row but its
  grid's cursor row, so the grid stays one Tab stop plus that row's tree (`../queue/DETAILS.md` § "The inline tree").
  Click and code still focus a row.

Which operations and phases show a tree is `tree-operation.ts` (`treeOperationFor`): a copy, move, or compress past its
scan, never during a reversal. The dialog and the queue rows both ask it, so they can't disagree.

It renders nothing for `not_a_transfer` (delete, trash), "Listing the files…" while `ready: false` (queued, scanning, or
a tree a bug turned off), and keeps its last rows once the operation has finished.

## The pull

`transfer-tree-view.svelte.ts` builds each request when it's SENT (`createCoalesced`: one in flight, the newest
waiting), so a toggle or scroll during a pending request rides along. Following asks `Follow { lead: 10 }`, so the rows
the pinned headers cover and a short scroll up are loaded; paused, it asks `At { offset: top − 10 }`. The limit is the
viewport's rows plus 10 above and below. Scroll-driven pulls wait for a frame. Hand toggles are capped at the backend's
2,000 each way; past that the oldest drops (`withNewest`).

Ticks pull through a 200 ms throttle (`TICK_PULL_INTERVAL_MS`, `createThrottle`): the first at once, then at most one
per interval, and the last tick of a burst always gets its trailing pull, so the final state lands. **Why**: the local
engine ticks once per file, so a run of tiny files pulled back to back, past the five pulls a second per view the budget
allows. **Why a clock**: the E2E app gets no animation frames, so a frame-gated pull never came there. Toggles, Follow,
and scrolls skip the throttle: they're a person's, and rare.

**The settle is a tick too** (`transfer-progress-state.svelte.ts`'s `progressTick` changes on `write-settled`), and the
backend keeps a settled operation's tree until the progress dialog's teardown calls `releaseTransferTree`
(`crates/cmdr-operations/src/transfer_tree/DETAILS.md` § "The view command"). **Why**: a transfer faster than the
throttle (a server-side copy of 36 files, #702) settled before the trailing pull came back, and the done dialog kept
rows that said every file was still waiting.

**Decision: no tree event; each view pulls on the operation's own `write-progress`.** **Why**: a broadcast tick can't
know what each view expanded or scrolled to, so it would either flood every window with rows nobody shows or need
per-view backend registrations that leak when a window closes. Pulling keeps the backend stateless per view, bounds
every answer by the viewport (≤ 200 rows), and makes attaching mid-copy the same code path as an update.

`followTop` is where the viewport's top belongs while following: the work sits `max(2, min(3, its depth))` rows down
(one row per header its folders will pin, so the stack never covers it), clamped so the viewport stays full. The
component sets `scrollTop` to it after each answer. Pausing records that position as the top, so the first paused pull
windows the rows on screen even if no scroll event ever said where that was.

**Decision: intent pauses following, a scroll event never does.** **Why**: a programmatic follow scroll, the browser
clamping `scrollTop` when a folder above shut, and a user's drag all fire the same `scroll` event, and tagging the
programmatic ones still loses the clamp. A wheel, a press on the scroller itself (its scrollbar), and a keyboard move
are unambiguous. A wheel counts only when the rows overflow: over a tree that fits, it scrolls the page around it (the
queue window), and pausing there would pop up Follow for nothing. The Follow button (a mini `Button` floating
bottom-right, out of the Tab order (`tabindex="-1"`: F is the way to it, and a stop that comes and goes would shift
every Tab after it), its word plus an F `keyHint` chip and no glyph; opaque and stacked above the rows and the pinned
headers, since a secondary button is see-through and the row under it showed through) and the F key resume (§ "The F
key").

## Rendering and focus

- The probe element carries the row-height token (`--spacing-tree-row`: one line of small text plus padding), so the
  virtual window divides by the real pixel height and follows the text size. Before there's a layout (first frame,
  jsdom) the tree draws the loaded window instead (`range`).
- **The viewport is measured in a task after a resize, ❌ never inside the `ResizeObserver` callback.** Measuring
  re-windows the rows, which mounts rows (each label with its own observer) while WebKit is still delivering the round:
  a dialog drag logged "ResizeObserver loop completed with undelivered notifications" on almost every frame (112 in 120
  frames, release build, a real WKWebView probe, 2026-10-13; 0 after). A task, not an animation frame: a window behind
  another gets no frames. `TransferTree.test.ts` pins that the callback renders nothing.
- Rows sit in absolutely placed slots keyed by flat index (`role="none"`, so the treeitems stay the tree's children). A
  slot's row changes in place as the tree changes; ❌ keying by id would MOVE the focused element, which blurs it.
- The keyboard cursor is a flat index plus the row id it holds. When an answer puts that id at another index (a folder
  above opened or shut), the cursor moves with it and, if the tree has focus, focus follows. A keyboard target that
  isn't loaded yet (End on a long list) waits as `pendingFocus` and lands when its row arrives. The cursor's row is
  cached, so it stays rendered after the window scrolled away from it.
- Roving `tabindex`: the cursor's row, else the row at work, else the first drawn row. The tree element itself is a tab
  stop only when no row is drawn.
- **The cursor is drawn like the file list's and the queue's** (`is-under-cursor`: `--color-cursor-inactive` plus the
  accent hairline, `--color-cursor-active` while the tree has focus, so an unfocused tree dims it like a pane). It IS
  the focus indicator, so a row has no outline. **Why**: a `:focus-visible` ring never shows for a click, so a clicked
  row (in the queue window above all) gave no sign of where the cursor was, and keyboard work couldn't pick up from
  there. It shows only once a click or a key placed the cursor; the row at work is not the cursor.

## Pinned headers

The open folders whose rows scrolled away stick to the top, stacked by depth (VS Code's sticky scroll): each is a real
row, with its icon, counts, and bar while at work. `pinned-headers.ts` picks them from the viewport's top row: header
`k` is the level-`k` ancestor of the row that would show right under the stack, if that ancestor's row has scrolled
under it. Headers that no longer enclose that row drop (a folder's last rows passing under). At most three; the third
compacts every deeper scrolled-past folder into one header (`a/b/c`), so the row under the stack always has its parent
in sight. `ancestorsOf` answers a row's chain from the loaded rows, then the view's `ancestors` (whole rows from the
backend, counts included) for whatever sits above the window.

The top row comes from `scrollTop` with a pixel of slack (`rowAtTop`). **Why**: the row height is fractional and WebKit
keeps `scrollTop` in whole pixels, so a scroll to row 1 read back a hair short; flooring that said row 0, and a flat
folder vanished (neither pinned nor drawn) whenever Up brought the cursor to its second child. `TransferTree.test.ts`
pins it with a whole-pixel scroller.

The stack is a zero-height `position: sticky` layer at the scroller's top, so the rows start under it and its headers
float over the rows they cover. A click on a header moves the cursor to its folder's row (revealed just below the
headers its own folders make); a click on its chevron shuts the folder.

**Decision: the headers are `aria-hidden` mirrors, not treeitems.** **Why**: a pinned header is a second copy of a row
the tree already has; as a treeitem it would read twice and break `aria-posinset` order. A screen reader walks the real
tree, where Left reaches every parent; the pointer gets the header click. They carry no tab stop for the same reason.

## Keyboard

Up / Down, PageUp / PageDown, Home / End move the cursor (and pause following). A target under the pinned headers is out
of sight too: `reveal` scrolls it to just below the headers its own folders will make. Right expands a shut folder, or
moves to its first child when open. Left shuts an open folder, or moves to the parent: an earlier row in the window one
level up, or, when the parent is above the window, the view's `ancestors` entry for that depth (`ancestorsOf`). F
resumes following while paused. A click on the chevron toggles; a double-click on a folder row toggles. A toggle never
pauses following. A click anywhere else on a row puts the cursor there and pauses following, so the row it picked stays
in sight.

### The F key

F resumes following while the tree is paused (Follow showing), read by `decisionKeyOf` like the decision prompts'
letters: the typed letter, the physical `KeyF` only on a non-Latin layout, never with a modifier or a repeat, never in a
text field, and never translated. `handleFollowKey` is the one entry; the tree calls it from its rows, and each host
forwards its own keydowns to widen the scope:

- **The progress dialog: anywhere in it.** Its other keys are F2, Tab, Escape, and Enter on a focused button; the clash
  letters only exist while a clash replaces the tree. So a bare F has no other meaning there.
- **The queue window: anywhere in the tree's own row** (the row, its buttons, its tree). The grid has no F of its own,
  and the row scopes it to the one tree the user is looking at, where a window-wide F would have to pick among several.

**Why**: F listened on the tree's rows alone and matched only `event.code`. A wheel is what usually pauses a tree, and
it leaves focus where it was (a dialog button, the queue row), so in the real app F did nothing. Matching `code` alone
also missed any keystroke without one (the MCP bridge sends none) and a layout that types F elsewhere.

## What a row says (`row-format.ts`)

- Count line, joined by middle dots: a folder at work "312 / 1,000 files · 1.2 GB / 4.0 GB", a waiting or finished one
  "1,000 files · 4.0 GB", plus "3 skipped" and a red "1 couldn’t finish" (the house voice never says "failed"; the
  queue's word). A file: its size, "1.0 MB / 4.0 MB" at work, "Skipped", or its failure's typed title in red. A failed
  folder keeps the progress form, so "999 / 1,000" says what's missing.
- Accessible name: `formatUnitList([label, …the count line's segments, state])`, "Photos 2024, 312 / 1,000 files, 1.2 GB
  / 4.0 GB, Copying": the visible text first, verbatim and in order, then the state (left out when a segment already
  says it, as a skipped file's "Skipped" does). **Why**: WCAG 2.5.3 (label in name). Speech control finds a row by the
  words on screen, so an "of" form of a visible "/" missed it; axe can only mark the rule incomplete in jsdom, so
  `row-format.test.ts` pins it. Each item is a standalone catalog phrase, so no key is a sentence fragment. No
  `aria-live`: the header readout speaks; a 5 Hz tree would talk over everything.
- The bar: folders along the view's `axis` (`Files` or `Size` progress as its name), files by bytes, drawn only on rows
  at work, without the shimmer (eight shimmering bars in a parallel SMB copy were busy).
- The label middle-truncates (`useShortenMiddle`) and hovers its full relative path, rebuilt by `rowPaths` from the
  window's rows plus `ancestors`.

## Fixtures

`fixtures/catalog.ts` holds seven shapes (single file; 5,000 top-level files on the files axis; 5 folders × 1,000; deep
`a/b/c` plus `Photos/2024` compact chains; six levels of non-compacting folders, past the pinned-header cap; parallel
SMB with 8 in flight across 3 folders; failed and skipped rows). `scripted-transfer.ts` simulates the transfer and
answers views with the backend's semantics in plain O(rows) code. `fixture-player.svelte.ts` drives one into a tree:
`step()` for tests, `play()` at the backend's 200 ms cadence (looping) for Debug → Components → Transfer tree.
`one-copying-file.ts` is the smallest static view, for tests of a surface that embeds the tree (the dialog, a queue
row).

The real thing is in Debug → Soft dialogs → Transfer progress: a paced local copy over a fixture tree for the local
shapes (`apps/desktop/src/lib/dialog-gallery/DETAILS.md` § "`transfer-progress` is real-operation"). The parallel and
failed shapes need a remote volume or a failing write, so they live only in the catalog.

## Tests

`row-format.test.ts`, `tree-operation.test.ts`, `pinned-headers.test.ts` (which folders pin, the cap's compaction,
dropping a passed folder), `transfer-tree-view.svelte.test.ts` (coalescer, follow and its lead under a deep stack,
toggles, ancestors), `TransferTree.test.ts` (fixtures: posinset / setsize / level, keyboard, Follow, the tab stop,
failure rows, the content-sized height), and `transfer-tree.a11y.test.ts` (axe over every fixture: preparing,
mid-transfer, paused, finished). Axe's contrast rule is off in jsdom; `pnpm check a11y-contrast` checks the rows' color
pairs in both themes.

End to end: `apps/desktop/test/e2e-playwright/transfer-tree.spec.ts` pauses a paced copy of five folders and checks the
tree against tree order (done folders shut above the work, the work's folder open, Follow, Left / Right), reading each
row's `data-state`; `operation-queue.spec.ts` checks a backgrounded copy's queue row shows its tree;
`dialog-resize.spec.ts` drags the dialog taller and checks the tree takes the height. The i18n capture photographs the
tree on the queue window's running row.

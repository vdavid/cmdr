# Viewer text selection: granularity drag, keyboard extension, and the optional text cursor

Fixes three user-reported gaps in the F3 viewer's selection and adds the four extensions and the one new setting agreed
with David on 2026-09-05. Everything here lives in `apps/desktop/src/routes/viewer/` plus the eight settings-plumbing
sites the new toggle needs.

Read before executing: `apps/desktop/src/routes/viewer/CLAUDE.md` and its `DETAILS.md` (§ "Selection model", § "Pointer
→ caret", § "Click cycle"), `docs/design-principles.md` (principle 2 "rock solid", 3 "delightful UX", 6 "elegance"),
`docs/style-guide.md` before writing the two new user-facing strings, and `apps/desktop/src/lib/settings/DETAILS.md`
§ "Restricted-window mode" before touching the setting.

## The problem

A user reported three things, all confirmed by reading the code:

1. **Double-click then drag stays stuck on the first word.** `handlePointerDown` (`viewer-pointer-drag.svelte.ts`)
   returns early for a press counted 2 or 3, before `dragPointerId` is set, so every later `pointermove` bails on its
   first line. Deliberate and documented: arming the drag with the character-granularity move handler would let a hand
   twitch call `setFocus(caret)` and collapse the fresh word back to a caret.
2. **Shift+Left/Right does nothing.** `viewer-keyboard.ts` has no Left/Right branch at all. Shift alone isn't
   meta/ctrl/alt, so the press falls to `handleBareKey` → `handleNavigationKey`, which knows only Up/Down/Page/Home/End.
3. **Option+Shift / Ctrl+Shift + Left/Right does nothing.** `e.altKey` routes the press to `handleModifiedKey`, which
   tries the two search chords and then bails at `if (!e.altKey && !e.shiftKey)`.

Plus the extensions David approved: Shift+Up/Down extends (today it scrolls, so this **changes** working behavior),
Shift+Home/End extends to line edges, ⌘+Shift+Up/Down extends to file edges, shift-click respects the active
granularity the way native does, unmodified Left/Right scroll horizontally, and an optional text cursor everything above
has to work with.

## The shape of the fix

One idea carries items 1 and 3: **selection granularity**. The model already tracks `{ anchor, focus }`; what's missing
is that both endpoints can be *ranges* snapped to word or line boundaries. Give the pointer drag a granularity and the
"twitch collapses the word" problem disappears by construction rather than being defended against: a pointer that hasn't
left the original word yields the same union, so there is nothing to collapse. The old early-return existed only because
`pointermove` had one granularity.

Items 2 and 3 need no caret in the model either: keyboard extension is "keep `anchor`, move `focus`", which is what a
caret would drive anyway. The text cursor is therefore a **pure render layer** over state that already exists, which is
why it can be a setting rather than an architecture.

### New modules (all pure, all unit-testable without a layout engine)

- **`viewer-selection-granularity.ts`**: the `SelectionGranularity` type (`'character' | 'word' | 'line'`) and
  `extendRangeToGranularity({ anchorRange, focus, granularity, getLineText })` → `Selection`. Used by the pointer drag
  AND shift-click, so the two can't drift.
- **`viewer-caret-motion.ts`**: `moveFocus({ from, motion, getLineText, getTotalLines, desiredColumn })` →
  `{ focus: LineOffset | null, targetLine: number, desiredColumn: number | null }`. `motion` is a discriminated union
  (`char` / `word` / `line` / `lineEdge` / `docEdge`, each with `direction: -1 | 1`), which makes the whole key map an
  exhaustive switch a test can enumerate. **`focus: null` with a real `targetLine`** is the "target line isn't cached
  yet" answer (decision 5), and the caller scrolls to `targetLine` regardless.

### Extended modules

- **`viewer-word.ts`** gains `findWordStartBefore(lineText, offset)` and `findWordEndAfter(lineText, offset)` beside the
  existing `findWordBoundsAt`, sharing the private `isWordSegment`. **Intent**: the JavaScriptCore `isWordLike` bug
  (documented in that file and in `DETAILS.md` § "Selection model") would bite word *motion* exactly as it bit
  double-click. One workaround, one place. ❌ Never call `Intl.Segmenter` word granularity from anywhere else.
- **`selection.svelte.ts`** gains `setRange({ anchor, focus })` on `createViewerSelection`. One object param, per the
  `cmdr/no-confusable-callback-params` rule: two `LineOffset`s are exactly the confusable pair that rule exists for.
- **`viewer-search-scroll.ts`** gains `ensureVisibleOffset({ lineTop, lineHeight, scrollTop, viewportHeight, margin })`
  → `number | null` (the new scrollTop, or `null` when already visible). It sits beside `recenterOffset` because the
  file's purpose is exactly "pure per-axis scroll math"; the composable wraps it as `scroll.ensureLineVisible(line)`.
  **Feed it the viewer's real geometry, not a `line × lineHeight` estimate**: `lineTop` comes from
  `scroll.getLineTop(n)`, which already applies `scrollScale` (files over `MAX_SCROLL_HEIGHT` are compressed), and the
  height comes from the height map × `scrollScale` when it's ready, `effectiveLineHeight` otherwise. Note in the wrapper
  that `recenterOffset` next door speaks *viewport-relative rendered-rect* coordinates while this one speaks
  *content-relative scaled* ones, so the two are not interchangeable; a wrapped line is one tall row and an estimate
  mislands it.
- **`viewer-pointer.ts`** gains two exports over the `charBoxes` machinery it already owns:
  `caretRectFor(content, lineOffset)` (for painting the text cursor) and `measureColumnWidth(content)` (for the
  horizontal scroll step). **Intent**: this module is the one place allowed to measure line text geometry; adding a
  third caller there beats a second measurement path elsewhere.

## Vocabulary decision: "caret" vs "text cursor"

The route already uses **caret** to mean *a resolved text position* (`viewer-caret-geometry.ts`, `caretFromPoint`,
`LineOffset`). The new rendered thing is a different concept, so it gets a different word: **text cursor**, in the UI
copy (`Show text cursor`), the setting id (`viewer.showTextCursor`), the CSS class (`.text-cursor`), and the module
(`viewer-text-cursor.svelte.ts`).

**Why this matters enough to write down**: `caretRectFor` returning the box the text cursor paints is the exact place a
future agent would start calling the rendered element "the caret" and then wonder why `viewer-caret-geometry.ts` doesn't
render anything. Record the boundary in `DETAILS.md` § "Selection model" as part of M5.

## Decisions taken up front (and why)

1. **Unmodified arrows always scroll, even with the text cursor on.** The obvious alternative (with a visible cursor,
   Right moves the cursor) makes the key map mode-dependent, and the viewer's primary job is looking at a file, not
   editing one. A mode-dependent map is worse than a slightly non-editor-like one, and this is reversible in one branch
   later. Left/Right gain horizontal scroll, which is what a two-pane-file-manager user expects from a no-wrap view.
2. **Keyboard extension with no selection at all is a no-op.** A plain click already sets a collapsed selection at the
   click point, so click-then-Shift+Arrow works and is the discoverable path. Seeding from the top of the viewport would
   start a selection somewhere the user can't see; with the text cursor off, they'd have no idea it happened.
3. **Vertical motion moves one LOGICAL line, not one visual row.** In word-wrap mode native moves by visual row. Every
   other navigation in the viewer (`scrollByLines`, the height map, the search jump) is logical-line-based, so a visual
   row here would be the only place with a second coordinate system. Note the upgrade path in `DETAILS.md`; don't build
   it now.
4. **Vertical motion keeps a desired column.** Walking down through a short line and back must return to the original
   column, as it does everywhere else in the OS. The column is logical (UTF-16 offset), matching decision 3. It resets
   on any horizontal motion and on any pointer gesture.
5. **A motion whose target line isn't in the line cache yields no offset for that press, but still scrolls.**
   `moveFocus` returns `{ focus: null, targetLine }` rather than a bare `null`: the keyboard consumes the key (so the
   page doesn't scroll out from under the user), leaves the selection alone, and **still** calls
   `ensureLineVisible(targetLine)`. **That last part is load-bearing**: scrolling is what puts the line in the render
   window and triggers the fetch, so the next press succeeds. Return a bare `null` and the press is a permanent no-op,
   because nothing ever fetches the line it wanted. **Intent**: the honest alternative would be to guess the unknown
   line's length, and a guessed offset crosses the IPC boundary into `viewer_read_range`. Never guess an offset. This is
   only reachable by out-running the 100 ms fetch debounce with key repeat.
6. **`docEdge` down uses decision 5's shape rather than minting a sentinel.** Three branches:
   - **Last line cached** → `{ line: totalLines - 1, offset: lastLineLength }`. Exact, one press.
   - **Last line not cached** (the common case: `lineCache` only holds fetched windows, so the last line of a large file
     essentially never is) → `{ focus: null, targetLine: totalLines - 1 }`. The key is consumed,
     `ensureLineVisible(totalLines - 1)` scrolls to the end, the fetch lands, and a **second** ⌘+Shift+Down completes
     the selection exactly. Two presses, always correct.
   - **`totalLines === null`** (ByteSeek-no-index) → `setFocus({ line: EOF_LINE, offset: 0 })`, reusing M0's constant,
     which maps to `RangeEnd::Eof` at the IPC boundary. ❌ Don't invent a second end-of-file representation, and
     ❌ don't call `selectToEof()` here: that sets BOTH endpoints (anchor back to `{0, 0}`) and would silently destroy
     the user's anchor. `docEdge` moves the focus only, like every other motion.

   ❌ **Don't reach for the sentinel just because the last line isn't cached.** It looks like the tidier answer, and it
   makes the sentinel a *live, movable* focus, which decision 7 explains is a wedge.

   **Consequence to state rather than discover**: ⌘+Shift+Down from mid-file then copy shows the "unknown size" confirm
   dialog on a large file instead of copying silently. `isWholeFileSelection` bails on `start.line !== 0` before its
   `end.line >= totalLines - 1` branch, so `estimateSelectionBytes` walks lines and returns `null` at the first one with
   an unknown byte length, which `runCopy` maps to a confirm. It terminates safely and it's defensible; it's only a bug
   report waiting to happen if we don't write it down.
7. **The keyboard resolves a sentinel focus to a real line BEFORE calling `moveFocus`; the pure module never sees one.**
   This guard is **not optional and not tied to the `docEdge` design**: ⌘A in ByteSeek-no-index mode already parks the
   focus on the EOF sentinel line today, so the moment keyboard extension ships, one Shift+Up from there asks to step
   onto a line that can never be cached, `ensureLineVisible` slams the view to the bottom, and every further press does
   it again forever with no way to shrink the selection.

   It belongs in the keyboard layer, not in `moveFocus`. The pure module holds `getLineText` and `getTotalLines` but no
   idea what's on screen, so a refusal there could only hand back the sentinel as its `targetLine` and the caller would
   scroll to *that* — the same slam. So: before calling `moveFocus`, replace a sentinel `from` with the last line the
   scroll composable is currently rendering (the last entry of `scroll.visibleLines`; there is no `visibleTo` getter) at that
   line's cached length; if nothing is rendered, the press is a no-op. This works because reaching the sentinel always
   scrolled the view to the bottom, so the last rendered line is the practical end of the file.

   State it as a precondition with a test of its own: **`moveFocus` never receives a sentinel `from`.** Keeping the pure
   module total is what makes its exhaustive-switch test worth anything.
8. **The text cursor defaults to OFF.** Cmdr's viewer is keyboard-first and David explicitly doesn't want one by
   default; the setting exists for people who expect an editor.
9. **The text cursor never blinks under `prefers-reduced-motion`,** and its blink phase restarts on every focus change.
   Design principle 3. Without the phase restart, a keypress landing during the "off" half looks like the cursor
   vanished, which is exactly the kind of small wrongness the principle is about.

## Milestones

Each milestone is independently committable and leaves the tree green.

### M0: the EOF sentinel is off by one and has never fired (adjacent bug, found while planning)

**Not part of the reported feature, but the feature's `docEdge` branch rides this exact path, so it gets fixed first.**

`handleSelectAllShortcut` mints the ByteSeek-no-index ⌘A sentinel as
`selectAll({ totalLines: Number.MAX_SAFE_INTEGER, lastLineLength: 0 })`, and `makeSelectAll` returns
`focus: { line: totalLines - 1, ... }`. So the focus line is `MAX_SAFE_INTEGER - 1`, while all three consumers compare
against `MAX_SAFE_INTEGER` literally. Consequences today:

- `getRangeEndsForCurrentSelection`'s `usesEof` is always `false`, so ⌘A on a ByteSeek-no-index file ships
  `{ kind: 'line', line: 9007199254740990, offset: 0 }` across `viewer_read_range` instead of `{ kind: 'eof' }`. The
  `eof` variant has apparently never been emitted in production.
- `isWholeFileSelection` returns `false` for the exact case its own doc comment says it exists for, so the copy-size
  short-circuit never engages and ⌘A on a huge file falls through to the per-line estimator.

Nothing caught it because every test hand-writes `focus: { line: Number.MAX_SAFE_INTEGER, offset: 0 }` instead of
driving `handleSelectAllShortcut`, and three doc comments plus `DETAILS.md` all assert the wrong value.

**How bad is it?** Not a data bug: `read_range` validates only the *start* against `total_lines` and streams until the
chunks run out, so `Line { line: 9007199254740990 }` reads the same bytes `Eof` would. `RangeEnd::Eof` itself is fully
implemented and has five backend tests; it's only the frontend that has never emitted it. The user-visible cost is the
copy-size path: `isWholeFileSelection` returning `false` means ⌘A on a huge ByteSeek file falls through to the per-line
estimator instead of using the known file size. So this is a correctness-and-clarity fix, and the reason to do it now is
that `docEdge` is about to add a second producer on the same path.

**Fix the class, not the comparison**: export one `EOF_LINE` constant from `selection.svelte.ts` and a
`makeSelectToEof()` that sets `focus.line = EOF_LINE` directly, so the value is never reconstructed by arithmetic in one
place and compared literally in three. The seam is wider than the pure helper, and every one of these moves together:

- `createViewerSelection` gains `selectToEof()` beside `selectAll`.
- The `KeyboardDeps.selection` interface, the page's `selection: { selectAll: ... }` wiring, and the
  `viewer-keyboard.test.ts` fixture (`selection: { selectAll: noop }`) — the fixture is a compile error until updated,
  which is convenient, because that file is where the red test belongs.
- The context menu needs no second path: it already routes through `handleSelectAllShortcut`.
- A doc line on `makeSelectAll` saying its `totalLines` is only ever a real line count and end-of-file is
  `makeSelectToEof`. A runtime guard would be disproportionate; the type can't express the distinction cheaply.
- Four stale comments and two docs that currently assert the wrong value: `selection.svelte.ts` (three places),
  `routes/viewer/DETAILS.md`, and `src-tauri/src/file_viewer/DETAILS.md`, whose claim that `Eof` is "used by ⌘A in
  ByteSeek-no-index mode" has been false since it was written and becomes true with this fix.

**What M0 does NOT fix**: `estimateSelectionBytes` has no sentinel awareness, so the `docEdge`-from-mid-file case still
walks lines and lands on the unknown-size confirm (decision 6's stated consequence). Leave it.

**Tests** (TDD, real red first — the whole point here, since the existing tests pass against behavior production never
produces): both current sentinel tests hand-write `focus: { line: Number.MAX_SAFE_INTEGER, offset: 0 }`, so rewrite them
against the imported `EOF_LINE` and make at least one drive `handleSelectAllShortcut` end to end
(`getTotalLines: () => null`, `getTotalBytes: () => 1`, then assert `isWholeFileSelection(sel, null) === true` and that
the range ends yield `{ kind: 'eof' }`). That fails today, for the right reason.

**Checks**: `pnpm check --fast`, then `pnpm check svelte`.

### M1: pointer granularity (the reported bug) and shift-click granularity

**Files**: `viewer-selection-granularity.ts` (new), `viewer-pointer-drag.svelte.ts`, `selection.svelte.ts`,
`+page.svelte` (the pointer-drag deps object gains `setRange`; M1 can't land without that edit).

**Character-granularity drags keep calling `setFocus`**, unchanged. Only granularity `word` / `line` uses `setRange`.
This is deliberate: the existing unit tests assert against the `setFocus` mock, and a plain drag genuinely only moves
one endpoint, so switching it to `setRange` would churn tests to express the same thing less directly.

**Behavior**: a press counted 2 or 3 now arms the drag with granularity `word` / `line` and remembers the pressed
word's (or line's) `[start, end)` as the **anchor range**. Every `pointermove` computes the caret's range at that
granularity and sets the selection to the union, direction preserved: dragging left of the anchor word gives
`anchor = anchorRange.end`, `focus = focusRange.start`, so a reversed drag reads correctly and `normaliseSelection`
still orders it. Autoscroll's `reAimAfterAutoscroll` routes through the same extend call, so a word drag past the
viewport edge keeps its granularity.

**Two pieces of state, with different lifetimes. Don't collapse them into one:**

- `dragGranularity`, reset by `endDrag` (it describes the drag in progress).
- `gestureGranularity` **plus the remembered `anchorRange`**, reset only by a plain (`count === 1`, non-shift) press.
  These are what a later shift-click reads. `endDrag` fires on the double-press's own `pointerup`, so a shift-click that
  read `dragGranularity` would always see `character` and the feature would silently do nothing. The anchor range has to
  survive too, or a backwards shift-click can't produce `anchor = anchorRange.end`, which is the whole point of the
  model.

A shift-click therefore extends at `gestureGranularity` from the remembered anchor range (native behavior), and still
doesn't advance the click cycle (the existing `lastPress = { ...press, count: 1 }` line stays).

**Edge case to keep honest**: during a fast autoscroll into unfetched lines, `getLineText` returns `undefined` and the
word bounds collapse on that line. The character path has the same hole today and the row renders empty anyway, so this
is consistent rather than new. One line in `DETAILS.md`.

**Tests** (TDD, real red first: this is a bug fix):

- `viewer-pointer-drag.svelte.test.ts` (unit, RED first): double-press then move right over the next word selects both
  words whole; move left past the anchor word selects backwards whole-word; a move that stays inside the anchor word
  leaves the selection exactly as the double-press left it (the twitch case the old early-return defended); triple-press
  then move down selects both lines whole; shift-click after a double-press extends by whole words.
- `viewer.spec.ts` § "File viewer multi-click selection" (E2E, written after): extend the existing spec with a
  double-press + `pointermove` + `pointerup` sequence over `alpha beta gamma` and assert `selectedTextOnLineZero()` is
  `beta gamma`. The `pressAt` / `betaPoint` helpers already there do most of it.

**Checks**: `pnpm check --fast`, then `pnpm check svelte`.

**No existing test presses an arrow key in any viewer spec or unit test** (verified 2026-09-05: the only keys the viewer
specs press are `Control+f`, `Escape`, `Enter`, and the copy chord). So Shift+Up/Down no longer scrolling and Left/Right
starting to scroll horizontally break nothing that exists; the new behavior needs new coverage, not repaired coverage.

### M2: word-boundary walkers and the pure motion model

**Files**: `viewer-word.ts`, `viewer-caret-motion.ts` (new). No wiring yet, so this milestone can run in parallel with
M1 if there's ever a reason to (disjoint files); sequential is fine and is the default.

**Behavior**: `findWordStartBefore` / `findWordEndAfter` walk the `Intl.Segmenter` segments in a direction and stop at
the first word segment's far edge. macOS semantics: Option+Shift+Right lands on the **end** of the next word,
Option+Shift+Left on the **start** of the previous one. `moveFocus` handles the five motions, grapheme-aware for `char`
(so an emoji, a ZWJ sequence, or a combining mark is one step). Note the two are different guarantees: the geometry path
in `viewer-pointer.ts` keeps offsets on **codepoint** boundaries, which motion by **grapheme** strictly refines, so the
existing invariant still holds and this is the stronger promise, and crosses line boundaries: right at end-of-line → `{ line + 1, offset: 0 }`,
left at offset 0 → `{ line - 1, offset: lineLength }`, both bounded by the file.

**Tests** (TDD, real red first: this is the risky logic):

- `viewer-word.test.ts`: forward and backward from inside a word, from whitespace, from punctuation, at line start, at
  line end, on an empty line, and **through the JSC-shaped segmenter stub already in that file** (a word ending in a
  digit must not be skipped, the exact bug documented there).
- `viewer-caret-motion.test.ts` (new): every motion × both directions; a grapheme cluster (👨‍👩‍👧 and `e` + U+0301)
  crossed in one step; line crossing at both ends; file-edge clamping; `null` on an uncached line; desired-column
  preservation across a short line; the `EOF_LINE` `docEdge` case with `getTotalLines() === null`.

**Checks**: `pnpm check --fast`.

### M3: keyboard wiring, ensure-visible, and horizontal scroll

**Files**: `viewer-keyboard.ts`, `viewer-search-scroll.ts`, `viewer-scroll.svelte.ts`, `viewer-pointer.ts`,
`+page.svelte` (deps wiring only).

**The complete key map this milestone lands.** Twelve extend chords plus two scroll keys; nothing else changes:

| Keys | Motion | Notes |
| --- | --- | --- |
| Shift+Left / Right | `char`, ∓1 | Grapheme step, crosses line boundaries |
| Shift+Up / Down | `line`, ∓1 | **Replaces today's scroll.** Logical line, keeps `desiredColumn` |
| Option+Shift+Left / Right | `word`, ∓1 | macOS: right lands on the END of the next word, left on the START of the previous |
| Ctrl+Shift+Left / Right | `word`, ∓1 | Same motion, for Linux and Windows; macOS usually eats it at the system level |
| Shift+Home / End | `lineEdge`, ∓1 | Extends to the **line** edge |
| ⌘+Shift+Up / Down | `docEdge`, ∓1 | Extends to the **file** edge; down is the two-press case in decision 6 |
| Left / Right (no modifier) | none | Horizontal scroll by one column, no-op under word wrap |
| Home / End (no modifier) | none | Unchanged: scroll to file start / end |

**Why Shift+Home means the line edge while bare Home means the file edge**: unmodified Home/End are scroll-view
navigation, which is what macOS does in a document view and what the viewer already does. Shift+Home/End are *selection*
gestures, and in every editor a selection gesture works on the line. The modifier changes the target on purpose; ⌘ then
promotes it back to the whole file. Don't "harmonize" these into one target.

**Behavior and the routing traps**:

- ❌ **Never write `e.shiftKey && e.key === 'ArrowLeft'`.** `cmdr/no-raw-key-match` is an **error** in this repo
  (`apps/desktop/eslint.config.js`), and it fires on exactly that shape: a required modifier read sharing a boolean
  expression with a literal key comparison while leaving at least one of the four modifiers unconstrained. The naive
  form is what an agent writes first, and it fails the lint gate, so M3 wouldn't land green. Use the **guard-then-branch**
  shape the rule deliberately doesn't catch and that the existing Shift+Enter branch already uses: test `e.key` first
  (a `switch` over the arrow / Home / End set), then read the modifier flags in a **separate statement** inside that
  branch. Pinning all four flags explicitly is the other sanctioned shape. The rule's own header names this window's
  case: "in a window with no command registry (the viewer), match locally but split on 'carries ⌘/⌃/⌥' up front".
- Plain Shift+Arrow arrives on the **unmodified** path in `handleKeyDown`. The extend branch goes **after the
  `if (searchInputFocused) return` early return and immediately before `handleBareKey`**. Before that guard it would
  steal the search input's own Shift+Arrow; after `handleBareKey` Shift+Up would keep scrolling. It must be
  **arrow-and-Home/End-only**, or Shift+Enter stops meaning findPrev. The comment there ("Shift stays free") needs
  updating in the same edit.
- Option/Ctrl+Shift+Arrow arrives on the **modified** path in `handleModifiedKey`, which runs **regardless of focus**.
  The extend branch sits after the search-chord check (so ⌘⌥R / ⌘⌥C still win) and before the
  `if (!e.altKey && !e.shiftKey)` bail, and **must be gated on `!searchInputFocused`** — without that gate it steals
  ⌥⇧← / ⌥⇧→ from a focused search input, and the M3 test below fails.
- ⌘+Shift+Up/Down is the `docEdge` motion; on macOS Ctrl+Shift+Arrow is usually eaten by Mission Control before it
  reaches us, which is fine and costs nothing to support for Linux and Windows.
- Every extend press calls `preventDefault()` and then `scroll.ensureLineVisible(...)` — using `focus.line` on success
  and the returned `targetLine` when the offset couldn't be resolved, per decision 5. The scroll happens on **both**
  paths; that's what makes the uncached-line case self-healing instead of permanently stuck.
- Unmodified Left/Right join `handleNavigationKey` as `scrollByColumns(-1 / +1)`, one column measured once by
  `measureColumnWidth(content)` (cached, invalidated on the debounced text-scale change the scroll composable already
  subscribes to, and when nothing is rendered there is nothing to scroll, so skip the step entirely rather than inventing a
  ratio). In word-wrap mode there's no
  horizontal overflow, so it's a natural no-op.
- **`KeyboardDeps` grows the wiring this milestone actually turns on**, and the plan names it so nobody invents it:
  `selection` gains `get selection()` (so `moveFocus` has a `from`) and `setFocus`; `NavigationActions` gains
  `ensureLineVisible(line)` and `scrollByColumns(n)`. Today `KeyboardDeps.selection` is `{ selectAll }` alone, which
  is why the extend branch has nothing to read or write without this.
- The keyboard composable owns `desiredColumn`, reset by any horizontal motion and by `setAnchor` / `setFocus` from a
  pointer gesture (wire the reset through the page's pointer deps, not a second listener).
- **Horizontal ensure-visible too.** On a long unwrapped line, repeated Shift+Right walks the focus past the right edge
  and nothing scrolls: `handleScroll` tracks only `scrollTop` / `viewportHeight`, and the viewer's only horizontal
  scroll today is search's `contentRef.scrollLeft = left`. Reuse that exact shape: measure the focus character's rect
  with `caretRectFor`, run it through `recenterOffset` against the content box, and set `scrollLeft`, the way
  `viewer-search.svelte.ts` already does for a match. Skip it in word-wrap mode, as the search path does.

**Tests**:

- `viewer-keyboard.test.ts` (unit, RED first for the routing): each of the twelve chords in the table above reaches the right motion; Shift+Enter
  still calls `findPrev`; ⌘⌥R still toggles regex; a chord with the search input focused doesn't steal the input's own
  Shift+Arrow (the input owns its text selection).
- `viewer-search-scroll.test.ts` (unit): `ensureVisibleOffset` returns `null` when visible, top-aligns when above,
  bottom-aligns when below, and respects the margin.
- `viewer.spec.ts` (E2E, after): click into line 0, press Shift+Right five times, assert the painted selection is the
  first five characters; then copy and assert the clipboard matches. One E2E for Option+Shift+Right asserting a whole
  word. **Three constraints from `test/e2e-playwright/CLAUDE.md` and the existing specs**: the copy modifier comes from
  `CTRL_OR_META`, ❌ never a hardcoded ⌘ (these specs also run on Linux Docker); dispatch the key sequence through
  `viewer.evaluate(...)` rather than OS-level `keyboard.press`, because `viewer.spec.ts` already records that pressing
  into a *scoped viewer window* flakes under Xvfb (the keystroke lands on the main window and the viewer webview never
  sees it); and the existing `pressAt` helper is the model for that.

**Checks**: `pnpm check`. Note the E2E lane is `desktop-e2e-playwright` (slow, macOS-only, `NotInCI`) and **has no
per-spec filter** — there is no `pnpm check e2e` selector. For single-spec iteration use the manual launch recipe in
`test/e2e-playwright/DETAILS.md` § "Running on macOS".

### M4: the optional text cursor

**Files**: `viewer-text-cursor.svelte.ts` (new), `+page.svelte`, `viewer-pointer.ts`, and the eight settings-plumbing
sites (all eight are compile- or check-blocking; there is no partial version of this milestone):

1. `src-tauri/src/settings/loader.rs`: `RestrictedWindowSettings.viewer_show_text_cursor` + its line in
   `parse_restricted_window_settings`.
2. `src-tauri/src/commands/settings.rs`: `RestrictedWindowPersistableSetting::ViewerShowTextCursor` + its `setting_id()`
   arm.
3. `src/lib/settings/settings-store.ts`: the `RESTRICTED_PERSISTABLE_SETTINGS` entry and the
   `initializeSettingsRestricted` snapshot mapping.
4. `src/lib/settings/restricted-settings-bridge.ts`: the `PERSIST_ALLOWLIST` entry.
5. `src/lib/settings/definitions/viewer.ts` + `sections/ViewerSection.svelte`: the registry entry and the row.
6. `src/lib/intl/messages/en/settings.json`: label, description, and an `@`-metadata block carrying **`description`
   only**. ❌ Don't hand-write `screenshot` / `screenshotNote`: `scripts/couple-screenshots.ts` generates those from the
   capture report (and `screenshotNote` only for representative stand-ins; no key in `settings.json` has one). The
   coupler adds `screenshot` on the next `pnpm i18n:shots`, and `desktop-message-screenshots-fresh` is warn-only, so
   nothing blocks.
7. `src/lib/settings/types.ts`: the `'viewer.showTextCursor': boolean` entry in the settings value map, one line below
   `'viewer.wordWrap'`. Without it every `getSetting` / `setSetting` call for the new id is a type error.
8. `src/lib/intl/keys.gen.ts`: regenerate with `pnpm intl:keys` from `apps/desktop/`, or the
   `desktop-message-keys-fresh` check fails on the stale file. Commit the regenerated file, same as `bindings.ts`.

**Why all eight**: the viewer window has no `store:default` capability by security design. ❌ Never re-grant store
access to get a setting in; extend this allowlist. (`src-tauri/capabilities/CLAUDE.md` § viewer,
`lib/settings/DETAILS.md` § "Restricted-window mode".) `bindings.ts` regenerates from the Rust changes; commit the
regenerated file. Sites 7 and 8 are the two an agent skips and then debugs for twenty minutes.

**Rendering**: a `ViewerTextCursor.svelte` component holding one absolutely-positioned `<div class="text-cursor">`,
mounted in **`.scroll-spacer`** (already `position: relative`).

❌ **Don't apply `translateY(linesOffset)` to it.** The cursor is positioned from a *measured* rect, not from logical
coordinates: `caretRectFor` bottoms out in `Range.getClientRects()`, so it returns a **viewport** rect read live off the
rendered row, which already includes `.lines-container`'s own `translateY` and the current scroll position. Subtracting
the spacer's `getBoundingClientRect()` is the whole conversion. Adding the transform on top double-counts `linesOffset`,
which on a large file is 10⁵-10⁷ px off screen. Concretely: `top = rect.top - spacerRect.top`,
`left = rect.left - spacerRect.left`, both read in the same `$effect` after `tick()`.

❌ **Not inside `.lines-container`**, however tempting it is to let the existing transform carry it:
`runWrappedLineHeightEffect` computes `avgWrappedLineHeight` as `linesContainerRef.getBoundingClientRect().height /
linesContainerRef.children.length`, so one extra child shrinks the average and corrupts `scrollLineHeight`,
`visibleFrom` / `visibleTo`, and `spacerHeight` in word-wrap mode until the height map is ready. A cursor that quietly
breaks virtual scrolling is the worst possible bug to ship here.

Its box comes from `caretRectFor(content, focus)` converted to spacer-relative coordinates. Recomputed in an
`$effect` after `tick()`, keyed on the selection, the scroll position, the rendered line set, and the wrap flag. Hidden
when: media mode, the setting is off, the selection is `null`, the focus line isn't rendered, or the measurement fails.
Blink is a CSS `step-end` animation, dropped entirely under `@media (prefers-reduced-motion: reduce)`, and re-keyed on
focus change with `{#key}` so a keypress always leaves the cursor solid.

**`caretRectFor` picks an EDGE of a measured box; it is not a fallback ladder.** This distinction is the whole
correctness of the thing. `measureChar` clamps its probe to `length - 1`, so at `offset === length` it returns a
perfectly good box — the **last character's**. Treat that as "measurement failed" and you don't get a hidden cursor
that signals the bug, you get a cursor painted one glyph too far left at every single line end, which is exactly where
Shift+End and every rightward walk park it. So:

- Probe `min(offset, length - 1)`, then return `box.left` when `offset <= box.start` and `box.right` when
  `offset >= box.end`. That one rule covers line start, line end, and everything between.
- The only true fallbacks: `length === 0` (empty line → the row's `.line-text` rect `left`) and a null `rangeRect`.
- Take the cursor's height from the **measured char box**, ❌ never from the row. Under word wrap `.line` has
  `height: auto`, so a wrapped logical line's row is several visual rows tall and a row-height cursor would paint a
  tall bar down the whole paragraph. The char box is one visual row by construction.

**Wrap-point behavior, written down so nobody "fixes" it later**: for an interior offset landing exactly on a wrap
boundary, `measureChar` returns the first glyph of the next visual row, so the cursor paints at the start of row N+1
rather than the end of row N. That follows `rangeRect`'s documented preference for the rect with width, and it matches
native downstream affinity. It's correct; leave it.

**Reactivity**: read the setting through the reactive settings layer, not a one-shot `getSetting` at mount, so flipping
it in Settings shows up in an already-open viewer. Restricted windows already receive live updates over the cross-window
`settings:changed` event, so this costs nothing extra; `viewer.wordWrap` reads once at mount only because `W` is its
primary control.

**Copy** (draft, David reviews before it ships per his "humans to humans" rule):

- Label: `Show text cursor`
- Description: `Show a blinking text cursor in the file viewer, marking where a keyboard selection grows from.`

**Tests**:

- `restricted-settings.test.ts`: the new id round-trips through the persist path and is refused when spoofed under a
  different id.
- A unit test for the spacer-relative rect math and for `caretRectFor`'s edge rule plus its two true fallbacks (the pure part, which lives in
  `viewer-text-cursor.svelte.ts` beside the component).
- `viewer.spec.ts` (E2E): with the setting on, a click paints a `.text-cursor` at the click point and Shift+Right moves
  it; with it off, no `.text-cursor` exists. A persistence spec mirroring `viewer-wordwrap-persistence.spec.ts`.
- `viewer.a11y.test.ts`: the cursor is `aria-hidden` (it's a visual echo of the selection the live region already
  announces, so a second announcement would be noise). **This is why the cursor is its own component**: that file mounts
  individual components one per test by design and never mounts `+page.svelte`, so a cursor living inline in the page
  would be untestable there.

**Checks**: `pnpm check` (includes the Rust lanes for the two backend files and the i18n parity test).

### M5: docs and the full gate

- `routes/viewer/CLAUDE.md`: one must-know line for the granularity model (the "a press counted 2 or 3 doesn't arm the
  drag" guardrail is now WRONG and must be replaced, not appended to), one for the keyboard extension entry points and
  their two routing traps, one for the caret/text-cursor vocabulary split. Watch the 300-400 word target; this file is
  already long, so **cut** something stale in the same pass rather than only adding (`claude-md-length` warns past 600,
  and `.claude/rules/file-length-allowlist.md` forbids silently raising a number).
- `routes/viewer/DETAILS.md`: rewrite § "Click cycle"'s last paragraph (the early-return rationale is now historical, so
  per `describe-current-not-history` it goes away rather than being narrated), extend § "Selection model" with
  granularity, the motion model, the four decisions above, and the vocabulary boundary; add a short § "Text cursor".
- `lib/settings/DETAILS.md` § "Restricted-window mode", plus the other places that ENUMERATE the restricted settings
  rather than just describing them: the doc comments on `RestrictedWindowSettings` and
  `RestrictedWindowPersistableSetting` (both say "these two booleans"), `src-tauri/capabilities/CLAUDE.md` § viewer, and
  the header comment in `restricted-settings-bridge.ts` ("the allowlist is enforced twice … the two permitted
  settings"). Each of those counts is wrong the moment a third setting lands.
- Full `pnpm check --include-slow`.

## Execution notes

- Sequential, one commit per milestone, conventional-commit titles leading with impact.
- ❌ Don't touch `+page.svelte` beyond deps wiring and the cursor element. It sits at 1491 lines against a 1442
  allowlist entry; the allowlist grants a 10% growth buffer (so ~1586 before it warns), which leaves room for the cursor
  element and its effect but not for new logic. Logic goes in the composables. Raising the recorded number needs David's
  consent (`.claude/rules/file-length-allowlist.md`).
- The E2E viewer specs are the slow lane and there is no per-spec filter in the check runner. Iterate on a single spec
  through `test/e2e-playwright/DETAILS.md` § "Running on macOS" during M1-M4, and take the full
  `pnpm check --include-slow` pass once, at M5.
- If any warn-only allowlist wants to grow, stop and report it rather than bumping it.

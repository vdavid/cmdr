# File viewer select and copy

Spec for adding a working "select text, copy it" flow to the read-only file viewer.

## Why

The viewer at `apps/desktop/src/routes/viewer/` virtualises everything. Only the visible window (~100 lines around the
viewport) ever lives in the DOM, regardless of file size, so the native browser primitives that users reach for don't
work the way they expect:

- **⌘A** isn't bound. The browser default takes over and selects only what's in the DOM (the visible window). It looks
  like "everything's selected", but the clipboard gets ~100 lines. Users won't notice until they paste somewhere and a
  10 MB log turns into a screenful. This is the trap, and the main reason David flagged this.
- **Mouse-drag selection** starts fine, but the moment the anchor or focus scrolls out of the visible buffer, its DOM
  node is recycled. The Selection API's anchor becomes a detached node and the selection silently collapses or jumps.
- **Right-click Copy** has the same buffer dependency.

The user need is mundane and constant: open a log file in Cmdr, find an interesting chunk, copy it to paste into a chat
or another editor. Right now that flow either fails silently (⌘A) or breaks on any selection larger than the viewport.

We need a selection model that's independent of the DOM and a copy path that reads from the backend (which has the
authoritative bytes), with safety rails for very large selections.

## Goals

- ⌘A selects the whole file (conceptually), regardless of size.
- Mouse-drag selection works across virtual-scrolled lines, including ranges that span far beyond the visible buffer.
- Copying writes the actual selected bytes to the macOS pasteboard.
- Large copies are gated by clear, helpful prompts. The user is never silently handed a multi-gigabyte clipboard.
- Status bar advertises the new ⌘A shortcut.
- Selection state and rendering are unified for ⌘A, mouse-drag, shift-click, and future programmatic callers.

## Non-goals

- Bringing back the browser's native Selection API. It doesn't survive virtualisation (see Design summary § Why not
  Selection API). We replace it deliberately.
- Services menu integration ("Look Up in Dictionary", third-party text Services). Read-only text viewer is the wrong
  surface for that, and the macOS Services pipeline requires the native Selection API which we're stepping away from.
- Multi-cursor / multi-range selection. One contiguous range is enough for "copy a chunk".
- Editing. The viewer stays read-only.
- Saving selection bytes as a file. Listed as a polish item in M5 because the >100 MB refuse dialog wants the option,
  not as a v1 deliverable.

## Design summary

### Selection model

Track selection as two endpoints in a logical coordinate space, not as DOM nodes:

```ts
type LineOffset = { line: number; offset: number } // offset is a UTF-16 code-unit index inside the line text
type Selection = { anchor: LineOffset; focus: LineOffset } | null
```

`anchor` is where the gesture started, `focus` is where it currently is. The "selected range" runs between them, in
either direction. Empty selection = `null` (clearer than `anchor.equals(focus)`, see open questions for the empty-line
caveat).

We use UTF-16 code units for `offset` to match the JS string model the rest of the viewer already uses (search columns
are already UTF-16, see `src-tauri/src/file_viewer/CLAUDE.md` § Key decisions). One mental model for the whole frontend.

A small `selection.svelte.ts` composable owns the state and the math: normalise (return start/end in document order),
test "is line N inside the range", compute the start/end offsets inside a given line, set/clear/extend.

### Why not Selection API + delegate-to-browser

The browser-native flow would be: let the DOM render selection backgrounds, intercept the `copy` event, fill it with our
own text. That breaks at virtualisation: once a line scrolls out, its DOM node is gone, the Selection's anchor or focus
becomes a detached node, and the visual selection collapses. Patching that with custom mousemove handlers ends up
duplicating most of what we're building anyway, plus fighting the browser's own selection state. Cleaner to suppress the
native selection entirely (`user-select: none` on `.file-content`) and own the model end to end.

### Range semantics

Selection is half-open: `[start, end)`. The line at `start` is included from `start.offset` to its end; intermediate
lines are included in full; the line at `end` is included from offset 0 up to (but not including) `end.offset`. For ⌘A
on an N-line file we set `focus = { line: N - 1, offset: lastLineLength }` so the last character is included.

Edge cases the unit tests must cover:

- Empty file (0 lines): selection stays `null`. ⌘A is a no-op.
- File of only newlines (`"\n\n\n"`, three empty lines): ⌘A produces `anchor = {0, 0}`, `focus = {2, 0}`. Lines 0 and 1
  contribute their (empty) content plus the implicit newline; the half-open boundary excludes any content on line 2.
  Backend read returns `"\n\n"`.
- File ending without a trailing `\n`: `totalLines` already counts the last line; ⌘A sets `focus.offset` to its real
  length.
- Single-line file: ⌘A produces `anchor = {0, 0}`, `focus = {0, lastLineLength}`.

### Rendering

Three line states per visible line:

- **Outside the range**: render normally.
- **Strictly between anchor and focus** (full lines): full-row background tint via a `.selected` class on the line.
- **Edge lines** (the anchor line and focus line, when partial): render via the same `getHighlightedSegments` pattern
  the search system uses (`+page.svelte` lines 603 to 605). The function gets extended to also emit `.selected` spans in
  addition to `<mark>`s for search hits. Search hit + selection on the same span is allowed: search wins on the
  background colour, selection wins on the text colour, or whatever the visual call lands on (see M2 design step). The
  point is the segmenter is the existing pattern; we don't invent a new one.

Lines outside the visible buffer aren't rendered. They're still conceptually selected; copy reads from the backend.

### `user-select` strategy

Today's CSS has a deliberate `user-select: text` on `.file-content` (`+page.svelte` lines 829 to 830), opting into
native selection on top of the global `html, body { user-select: none }` reset. M1 flips that, but it can't flip naively
because other parts of the viewer relied on the global default:

| Surface               | M1 value                  | Why                                                                                 |
| --------------------- | ------------------------- | ----------------------------------------------------------------------------------- |
| `.file-content`       | `user-select: none`       | Our custom selection is the only one; browser can't render a competing broken one.  |
| Search `<input>`      | (default)                 | Inputs get user selection from the browser regardless. Don't touch.                 |
| `.status-bar`         | `user-select: text` (new) | Users may want to copy the file name or line count. Add explicitly.                 |
| `.line-number` gutter | `user-select: none`       | Aria-hidden chrome, not content. Already inherits from the global; assert in tests. |

Note: `caretPositionFromPoint` reportedly still works on `user-select: none` text in modern WebKit, but the project's
minimum macOS target should be verified in an M3a spike (see Risk register).

### ⌘A

In FullLoad / LineIndex mode: `anchor = { line: 0, offset: 0 }`,
`focus = { line: totalLines - 1, offset: lastLineLength }`. Same code path as everything else.

In ByteSeek-no-index mode: `totalLines` is an estimate, and we don't know the offset of the last line. Selection model
gets a `RangeEnd` discriminator (see Backend IPC) so the focus can be expressed as "end of file" rather than a fake line
number. Frontend stores `focus.line = Infinity` for this case and the IPC translates to the typed enum at the boundary.

Empty file (0 lines): ⌘A is a no-op. Selection stays `null`. The "Copy" path simply has nothing to do; no toast.

### Mouse-drag

We use **pointer events** (`pointerdown` / `pointermove` / `pointerup` / `pointercancel`) rather than mouse events,
because pointer events support `setPointerCapture`. Without capture, the drag would lose its `mouseup` whenever the
cursor exits the Tauri webview (a separate macOS window, a different app, the desktop) and the autoscroll RAF loop would
run forever.

- `pointerdown` on `.file-content`:
  1. `document.caretPositionFromPoint(x, y)` to locate the precise character. Fallback to the deprecated
     `document.caretRangeFromPoint` for older WebKit if needed (verified in the M3a spike).
  2. Walk up the returned node to its `[data-line]` ancestor; bail if none (clicked the gutter or scrollbar).
  3. Sum offsets across sibling spans inside `.line-text` because search highlighting nests `<mark>` inside the line
     text. The flat character offset is the sum of preceding sibling text lengths plus the offset within the current
     node.
  4. Set `anchor = focus = { line, offset }`.
  5. Call `event.currentTarget.setPointerCapture(event.pointerId)` so subsequent pointer events fire even outside the
     webview.
- `pointermove` while captured: recompute `focus`.
- Autoscroll near viewport edges: when the cursor is within ~30px of the top or bottom of `.file-content`, run a
  `requestAnimationFrame` loop that scrolls `scroll.contentRef` proportionally to distance from the edge. The viewer's
  existing fetch-on-scroll pipeline (`viewer-scroll.svelte.ts`) brings in newly-needed lines automatically.
- `pointerup`, `pointercancel`, or window `blur`: stop the RAF loop, release pointer capture, freeze the selection. The
  `blur` handler is the safety net for the case where macOS hands focus to another app mid-drag without firing pointer
  events.

### Copy with size tiers

Three bands, sized from the selected range's byte length (UTF-8 encoded).

**Thresholds are fixed binary bytes**, independent of the user's binary/decimal display setting. Sizes shown in dialogs
and toasts go through `formatBytes()`, which respects `appearance.fileSizeFormat` for display only.

```ts
const COPY_CONFIRM_BYTES = 10 * 1024 * 1024 // 10 MiB
const COPY_REFUSE_BYTES = 100 * 1024 * 1024 // 100 MiB

type CopyAction = 'silent' | 'confirm' | 'refuse'

function selectCopyAction(bytes: number): CopyAction {
  if (bytes >= COPY_REFUSE_BYTES) return 'refuse'
  if (bytes >= COPY_CONFIRM_BYTES) return 'confirm'
  return 'silent'
}
```

Boundary Vitest cases: `10 MiB` → `confirm`, `10 MiB - 1` → `silent`, `100 MiB` → `refuse`, `100 MiB - 1` → `confirm`.

| Band              | M2 behaviour                                                                                                                                                                                                                                                                                                                                                           | M5 addition                              |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| **`silent`**      | Copy silently. Show an `info`-level transient toast on success: "{formatBytes(N)} on your clipboard". 4-second auto-dismiss.                                                                                                                                                                                                                                           | Unchanged.                               |
| **`confirm`**     | Confirm. Title: "Copy {formatBytes(N)} to clipboard?" Body: "Large pastes can slow down other apps. Try search (⌘F) to narrow it down." Buttons: **Cancel**, **Copy**. Default action: Cancel.                                                                                                                                                                         | Add a third button: **Save as file…**.   |
| **`refuse`** (M5) | Title: "Copy {formatBytes(N)} to clipboard?" Body: "That's larger than the 100 MB clipboard limit. Try search (⌘F) to find what you need, or save the selection as a file." Buttons: **Cancel**, **Save as file…**. Default: Cancel. Direct copy is not offered. This is the long-term shape; the same dialog ships in M5 once Save as file… lands.                    | (Same dialog, shipped in M5.)            |
| **`refuse`** (M2) | Same title and body as the M5 refuse dialog above, but without the **Save as file…** button (the action lands in M5). To avoid a placeholder button that does nothing, M2 ships a temporary single-button variant: **Got it**. The body keeps the "or save the selection as a file" sentence so the user still sees the workaround (manual: ⌘C in their editor, save). | Promoted to the full Cancel · Save form. |

The success toast text "{formatBytes(N)} on your clipboard" is one chosen phrasing over "Copied {formatBytes(N)}"; talk
about the user, not the action (style-guide.md § Success messages).

All dialogs use `ModalDialog` and follow the style-guide format: verb-noun title question, body explains "why it
matters", buttons are outcome verbs (no Yes / No, no bare "OK").

For the `confirm` band, the actual byte-reading work happens after the user picks Copy. Reads expected to take >300 ms
surface a progress toast (radical-transparency principle, see design-principles.md). Reads are cancelable via Escape;
cancelling also stops the backend stream (design principle: long ops cancelable, stopping background work too).

While a read is in flight, the copy action is disabled (a busy flag on the selection store). A second ⌘C during a slow
read is a no-op rather than starting a parallel read.

No "error" or "failed" anywhere in the copy. (See style-guide.md § Wording.)

### Backend IPC

New Tauri commands (M2 lands the first two, M5 lands the third):

```rust
#[derive(serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum RangeEnd {
    Line { line: u64, offset: u32 },  // UTF-16 code units inside the line
    Eof,                              // end of file, for ByteSeek-no-index ⌘A
}

#[derive(serde::Serialize, specta::Type)]
struct ReadRangeStart {
    read_id: u64,
}

/// Starts a cancelable range read. Emits `viewer-read-progress` events for slow reads
/// and a `viewer-read-complete` event with the full text on completion.
#[tauri::command]
async fn viewer_read_range(
    session_id: String,
    anchor: RangeEnd,
    focus: RangeEnd,
) -> Result<String, ViewerError>;

/// Flips the cancel flag for an in-flight read. The reader checks the flag every chunk
/// (64 KB) and returns `ViewerError::Cancelled` on hit.
#[tauri::command]
fn viewer_cancel_read(session_id: String, read_id: u64) -> Result<(), ViewerError>;

/// (M5) Streams the selection bytes to a chosen file path. Atomic temp+rename.
#[tauri::command]
async fn viewer_write_range_to_file(
    session_id: String,
    anchor: RangeEnd,
    focus: RangeEnd,
    dest_path: String,
) -> Result<(), ViewerError>;
```

`ViewerError` is a typed enum with variants like `Cancelled`, `OutOfRange`, `BackendNotReady`, `WriteFailed`, etc.
matched on by the frontend per the no-string-classification rule.

#### Offset encoding: UTF-16, with clamp-to-codepoint at the backend

The selection store uses UTF-16 code units for `offset` (matches search column units; one mental model for the
frontend). The backend operates on UTF-8 strings, where UTF-16 offsets aren't a native unit. The conversion happens at
the backend boundary:

- For each edge line, decode the line text, walk it with `char_indices()`, and accumulate UTF-16 code-unit lengths via
  `c.len_utf16()` until the running UTF-16 count reaches the requested offset.
- If the requested offset lands between the high and low surrogate of an astral codepoint (for example, offset 1 inside
  `"👋"` which is two UTF-16 units), **clamp to the nearest codepoint boundary** (round down). Lone surrogates on the
  wire are clamped, never returned as illegal UTF-8.

Rust test required for M2: a fixture line containing `"👋"`, with anchor offset 1 (mid-surrogate). The clamp rounds down
to offset 0, and the returned range excludes the wave emoji.

Alternative considered: switch the wire format to UTF-8 bytes (matches Rust idioms). Rejected because the frontend
already speaks UTF-16 for search highlight columns, and using UTF-16 for selection keeps both at one unit. A
`TextEncoder` conversion at every IPC call site is more error-prone than one well-tested conversion at the backend.

#### Cancellation plumbing

Each read gets its own cancel flag, **not** a session-wide one. Same lesson as `search_cancel` (see the
file_viewer/CLAUDE.md gotcha around `session.search` and `SearchStatus::Cancelled`): a session-wide flag races against
concurrent reads and against reads that complete just as the user starts a new one.

Layout:

```rust
struct ViewerSession {
    // ... existing fields ...
    active_reads: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    next_read_id: AtomicU64,
}
```

`viewer_read_range` allocates a `read_id`, inserts an `Arc<AtomicBool>` into `active_reads`, spawns the read on the
blocking thread, and removes the entry when the read returns (cancelled or not). `viewer_cancel_read(read_id)` flips the
bool. The reader loop checks the bool every 64 KB chunk and exits with `ViewerError::Cancelled`. If the read finishes
before the cancel arrives, the cancel is a no-op (the entry's already gone from `active_reads`).

For the one-shot < 10 MB common case, the cancel plumbing is unused but free; the read finishes fast enough that the
user can't press Escape in time.

#### Other backend notes

- Returns the selected text as a single UTF-8 string, decoding the line range from whichever backend the session uses
  (FullLoad / ByteSeek / LineIndex). All three backends already read lines; the new command stitches a range.
- The typed `RangeEnd` enum keeps the API honest: no `u64::MAX` sentinel, no string-matching on a special value (per the
  no-string-classification rule in AGENTS.md).
- The streaming variant for the `confirm` tier is deferred (see Open Questions). For the < 10 MB common case the
  one-shot path is fine: 10 MB across IPC is well within Tauri's limits and finishes in well under 100 ms on local
  files.
- `viewer_read_range` and `viewer_write_range_to_file` are `async` and wrapped in `blocking_with_timeout` per the
  platform-constraints rule in `architecture.md` § Tauri IPC threading. Reads under 2s use the default timeout; for
  larger selections the timeout is computed from estimated size: 1s per 100 MB plus a generous safety multiplier, capped
  at 60s.

#### Capability file additions

Today's `src-tauri/capabilities/viewer.json` only allows `core:window:*`, `core:event:default`, and `store:default`. The
Tauri commands the viewer already calls (`viewer_open`, `viewer_get_lines`, etc.) work because Tauri 2's auto-registered
commands don't need an allowlist entry; the security boundary is the per-window capability set, not a per-command
allowlist. Verify this assumption with a 1 KB write spike at the start of M2 (see Step 1 below).

The new dependencies M2 introduces that **do** need capability entries:

- `clipboard-manager:allow-write-text` if the viewer ever calls `navigator.clipboard.writeText` through the Tauri plugin
  (the main window has `clipboard-manager:default`, but the viewer window doesn't). Plain
  `navigator.clipboard.writeText` from JavaScript runs against the browser API and may not need the plugin permission in
  Tauri 2; the spike confirms which path applies.

**M2 step 1 (sequenced first)**: write a small 1 KB string from a viewer window via `navigator.clipboard.writeText`,
confirm it lands on the macOS pasteboard, and check the dev-tools console for permission errors. If it works, no
capability change is needed for the JS path. If it rejects, add `clipboard-manager:allow-write-text` (and possibly
`clipboard-manager:default`) to `viewer.json` and retry. Document the final decision in the M2 milestone scope and in
`viewer.json`'s adjacent comment.

If the spike shows the JS path doesn't work for large writes (Risk register: 50–100 MB), the fallback is a new
backend-side Tauri command writing via NSPasteboard. That command would need its own capability entry.

### Status bar

Extend the shortcut hint at `+page.svelte` line 650:

```
W wrap · ⌘A select all · ⌘C copy · ⌘F search · Esc close
```

The app is macOS-only, so hardcode `⌘` rather than picking by platform. The existing hint reads "Ctrl+F search"; M1
fixes that to `⌘F search` for consistency with the new ⌘A and ⌘C entries (also more platform-native, per
design-principles.md § Platform-native, not generic).

Sentence case throughout. The middle dot `·` is the existing separator.

**Width fallback**: on narrow viewer windows the full hint may overflow. If it does, drop the leading `W wrap ·` chunk
first (the wrap badge already has a tooltip with the same shortcut). The status bar applies `text-overflow: ellipsis` as
a last resort, never line-wraps. Confirm visually on a 600px-wide viewer window during M1.

### Right-click and copy event

- `contextmenu` on `.file-content`: `event.preventDefault()` first thing, otherwise macOS opens its native context menu
  on top of ours. Then open a small custom menu (`Copy`, optionally `Select all`). Reuses our existing context-menu
  pattern from elsewhere in the app (TBD which one: investigate in M4; the file explorer's pane context menu is the
  leading candidate to copy from).
- Intercept the `copy` event (⌘C from anywhere inside `.viewer-container`) and reuse the same code path as the menu
  Copy. This way ⌘C works regardless of focus.

## Open questions and decisions

### "Bytes of the selection" for size-tier classification

The selection lives in UTF-16 line offsets but pasteboards see UTF-8 bytes. The tier thresholds are in bytes, so we need
a byte estimate _before_ we read the selection.

**Decision**: Estimate cheaply via the line-byte-offset map the backend already has.

- **FullLoad**: cached line text in memory; we can sum exact UTF-8 lengths in O(lines) trivially.
- **LineIndex**: sparse checkpoints every 256 lines. Sum checkpoint deltas for the full-line range, plus exact length
  for the two edge lines (read those into memory; they're at most one line each, bounded by the longest line). This is
  O(checkpoints in range) and cheap.
- **ByteSeek (no index)**: we know byte offsets at the cursor's current scroll position. For ⌘A we know `totalBytes`.
  For partial selections we don't have exact byte positions out of the box. Approach: store the byte offset of each line
  we've ever fetched (the backend already returns it via `LineChunk`), cache in the scroll composable, and use the
  bracket lines we _have_ seen to estimate. If we genuinely don't know (rare: drag spans a region we never scrolled
  through), short-circuit to the > 100 MB band's refuse dialog with a "selection size unknown, save as file" path.
  Cheaper than reading the whole range just to measure.

The function `estimateSelectionBytes(anchor, focus)` lives in `selection.svelte.ts`. **Unit-testable**: proptest-style
fuzz over (lines, line lengths, anchor, focus).

### Should ⌘A on a 4 GB file compute exact size or short-circuit?

For ⌘A specifically, `totalBytes` is known (it's the file size minus a small per-line newline accounting if the file
doesn't end with one, close enough). Use that directly. No iteration, no estimate. So ⌘A on a 4 GB file lands in the

> 100 MB refuse band immediately with the exact file size displayed, which is exactly what we want.

### Clipboard plugin vs `navigator.clipboard.writeText`

The codebase uses both: `CommandBox.svelte` uses `navigator.clipboard.writeText` for short strings, the file clipboard
(Cmd+C on files) uses the native NSPasteboard via `src-tauri/src/clipboard/`. For viewer text copy, **use
`navigator.clipboard.writeText`**: it goes to the system pasteboard on macOS via WebKit, accepts large payloads (the
WebKit limit isn't documented but tests show it handles 100+ MB on macOS Sonoma+ without complaint), and avoids adding a
new Tauri command for plain text copy. The native file-clipboard path is for file references, not text. Decision
revisitable if 50–100 MB writes turn out to choke `navigator.clipboard` in practice (test as part of M2).

### Selection on lines with zero text (just `\n`)

`{ line: N, offset: 0 }` for both anchor and focus on an empty line means "the empty line is selected if anchor.line <
focus.line", and "no selection on the empty line if anchor and focus are both there". The empty-line edge case is
handled by the segmenter: when start.line == end.line == N and start.offset == end.offset, the segmenter emits no
`.selected` span. Caret-style "I clicked but didn't drag" → `null` selection.

### Bare anchor (pointerdown then pointerup, no drag)

After a `pointerdown` + `pointerup` with the focus equal to anchor: we set `selection = null`. No visible selection. The
caret position is lost (the viewer has no caret today, and we're not adding one). Consistent with read-only text viewers
users expect.

### Reversed drag (anchor below focus)

Selection is symmetric: render uses `start = min(anchor, focus)`, `end = max(...)`, where comparison is by
`(line, offset)` lexicographically. Tested explicitly in M1 unit tests.

### Why **Save as file…** ships in M5, not M2

M2 ships the dialogs with the minimum useful buttons: Cancel · Copy in the 10–100 MB band, single OK in the > 100 MB
band. M5 adds **Save as file…** to both bands once `viewer_write_range_to_file` lands. Justified by gradient: get the
core copy flow right first (confirm + size-tier logic + cancellation), then add the save-as branch on top with its own
isolated test surface (Tauri dialog plugin, atomic temp+rename writing).

A "disabled placeholder button" was considered and rejected: a greyed-out button advertising a feature that doesn't work
yet is worse than a clear single-button refusal with text that points to a workaround.

### Why no streaming IPC variant in M2?

100 MB is our hard ceiling for direct copy. 100 MB over a Tauri IPC string serialise + parse round-trip on Apple Silicon
measures around 300–500 ms in informal testing. That's tolerable for a one-shot copy with a progress toast. Streaming
would matter at the 500 MB+ scale, but we refuse there. Decision: skip streaming until evidence shows the one-shot is
too slow at 50–100 MB.

## Milestones

Generally sequential in a single worktree, with M5 doc updates running alongside M3 / M4 code work.

### M1: Selection model + ⌘A (no copy yet)

**Scope**: just the model and rendering. No clipboard yet.

- New file `apps/desktop/src/routes/viewer/selection.svelte.ts`: state, normalise, line-in-range, offset-in-line, set,
  clear, `extendToFullFile(totalLines, lastLineLength)`, `estimateSelectionBytes(...)`.
- Extend `getHighlightedSegments` in `viewer-search.svelte.ts` (or, if that's the wrong home, factor a shared
  segmenter): emit `.selected` segments in addition to `<mark>`. Search hit + selected on the same range stacks visually
  (define exact CSS in M1: search background + selected background mixed; selection text colour wins).
- `+page.svelte`:
  - Add ⌘A handler in `handleKeyDown` (before the existing checks). Empty file: no-op.
  - Render full-line `.selected` class on lines strictly between anchor and focus.
  - Render partial highlight on edge lines via the extended segmenter.
  - Flip the existing `.file-content` rule from `user-select: text` (lines 829 to 830 today) to `user-select: none`. Add
    an explicit `.status-bar { user-select: text }` rule so users can still copy the file name and line count.
    `.line-number` keeps the global default (`none`); assert via a Vitest snapshot of computed styles.
  - `.selected` background uses `var(--color-accent-subtle)`, the same token the file-list cursor highlight uses
    (design-system.md § File list, "Cursor highlight"). Selection text colour follows the file list's "selected = gold"
    language: `var(--color-selection-fg)` on the selected text. Both work in light and dark.
- Status bar: replace `Ctrl+F search` with `⌘F search`, and extend the hint to include "⌘A select all" and "⌘C copy"
  (sentence case). Confirm the hint fits on a 600 px-wide viewer window; if not, drop the leading `W wrap ·` chunk.

**Tests** (TDD-friendly):

- Vitest `selection.svelte.test.ts`: normalise, in-range, segmenter math, reversed drag, empty file (0 lines), single
  line, file with only `\n`, multi-byte UTF-8 inside the offset math (verify offset semantics on combining characters,
  surrogate pairs).
- A11y test for the viewer status bar's keyboard hint visible to AT.

**Not tested at this milestone**: Playwright e2e for ⌘A's visual effect (covered in M3 alongside copy).

### M2: Copy with size thresholds

**Step 1 (sequenced first, blocks everything else in M2)**: clipboard write spike. Add a one-shot test button to a debug
surface (or run via the dev console) that calls `navigator.clipboard.writeText('hello world')` from a viewer window.
Confirm:

1. The string lands on the macOS pasteboard (paste-test in another app).
2. No permission rejection in the dev-tools console.
3. The behaviour is the same with the viewer window backgrounded and focused.

If permission is rejected, add `clipboard-manager:default` and / or `clipboard-manager:allow-write-text` to
`src-tauri/capabilities/viewer.json` and retry. Document the final capability set in the milestone scope notes.

**Step 2: backend.**

- New Rust commands `viewer_read_range` and `viewer_cancel_read` (signatures in § Backend IPC above).
- New `ViewerError` typed enum with `Cancelled`, `OutOfRange`, `BackendNotReady`, etc. Frontend matches on variants (no
  string-matching, per AGENTS.md).
- `ViewerSession` grows `active_reads: Mutex<HashMap<u64, Arc<AtomicBool>>>` and `next_read_id: AtomicU64`.
- Backend UTF-16 → UTF-8 offset conversion with surrogate clamp (see § Backend IPC § Offset encoding).
- Unit tests in `apps/desktop/src-tauri/src/file_viewer/session_test.rs` per backend:
  - **FullLoad**: anchor == focus (returns `""`), single line, multi-line, reversed inputs (backend normalises),
    out-of-range (returns `OutOfRange`).
  - **ByteSeek (no index)**: same matrix, plus `RangeEnd::Eof` for ⌘A.
  - **LineIndex**: same matrix, range spanning checkpoint boundaries.
  - **UTF-16 surrogate clamp**: fixture line with `"👋"`; anchor at offset 1 clamps to offset 0; output excludes the
    emoji.
  - **Cancellation**: spawn a read on a fixture that returns after a delay (via test-only feature flag or a slow
    in-memory backend), call `viewer_cancel_read`, assert the read returns `ViewerError::Cancelled` and the entry is
    removed from `active_reads`.
- Proptest: stitching adjacent ranges equals one big range.

**Step 3: frontend.**

- `tauri-commands/viewer.ts`: typed wrappers `viewerReadRange` and `viewerCancelRead` (regenerated via
  `bindings:regen`).
- Frontend copy flow in `+page.svelte`:
  - Listen for `copy` event on `.viewer-container`.
  - Compute estimated bytes via `estimateSelectionBytes`.
  - Pass through `selectCopyAction(bytes)`. Branch on the returned action:
    - `'silent'`: read range, write clipboard, show `info` toast. No dialog.
    - `'confirm'`: show confirm dialog. On Copy: read range, write clipboard, show success toast. On Cancel: do nothing.
    - `'refuse'` (M2): show the temporary single-button Got it dialog.
  - Use `addToast` (`info` level) for the silent-band success path. Use `ModalDialog` for the two larger bands. Buttons
    via the `Button` component (`secondary` for Cancel, `primary` for Copy / Got it).
  - Show a progress toast (`info` level) for reads expected to take >300 ms based on estimated size; cancel via Escape,
    which calls `viewer_cancel_read(sessionId, readId)`. The read returns `ViewerError::Cancelled`, the progress toast
    is dismissed, no error toast.
  - Disable the copy action while a read is in flight (busy flag on the selection store). A second ⌘C is a no-op.
  - Write to clipboard via `navigator.clipboard.writeText` after the read completes, in the same click-handler tick
    (preserves the user-gesture context that some browsers require for clipboard writes).
- Update `src-tauri/capabilities/viewer.json` per the spike's findings.

**Tests**:

- Rust unit tests as above.
- Vitest IPC contract test in `apps/desktop/src/lib/ipc/viewer.test.ts` for `viewer_read_range` (it's destructive in the
  "destructive, cross-window, or has > 2 positional args" sense: three structured args (session, anchor, focus) with one
  of them being a tagged enum, so the contract test is mandated by testing.md § "When you add X, also add Y").
- Vitest test for the size-tier branching logic (a pure function `selectCopyAction(bytes)` returning
  `'silent' | 'confirm' | 'refuse'`). Threshold-boundary tests.
- A11y tier-3 test for the two new dialog components.

**TDD posture**: high. The IPC, the threshold function, the size estimator, the backend range read are all naturally
TDD-friendly. Write the Rust read-range tests _before_ implementing the read paths.

### M3a: Caret math + drag-within-viewport (no autoscroll)

The "drag" half splits into two milestones because the autoscroll plumbing (pointer capture, blur fallback, RAF loop) is
large enough to deserve its own scope and tests.

- Pointer event handlers (`pointerdown` / `pointermove` / `pointerup` / `pointercancel`) on `.file-content`. No
  autoscroll yet; drags past the viewport edge just stop tracking when the pointer leaves the visible buffer.
- `caretFromPoint(x, y)` helper in a new `viewer-pointer.ts` (pure function, takes a Document for testability): resolves
  `(x, y)` to `{ line, offset }`. Returns `null` for points outside any `[data-line]`.
- Sibling-offset summation that handles nested `<mark>` from search highlighting.
- **Spike subtask (first thing in M3a)**: confirm `caretPositionFromPoint` returns a non-null caret on
  `user-select: none` text in the project's minimum macOS target. If not, implement the `elementsFromPoint` +
  per-character width measurement fallback. Document the result.

**Tests**:

- Vitest for the sibling-offset summation (pure function, mocked DOM nodes including nested `<mark>` from search +
  `.selected` from M1).
- Vitest covering emoji boundaries inside the offset math (the click lands mid-surrogate; the function clamps).
- Playwright e2e: drag entirely within the viewport, ⌘C, assert clipboard text matches the dragged range.

**TDD posture**: high. Caret math is pure and TDD-friendly.

**Dependency**: the e2e spec needs M2's copy flow live (writes to clipboard).

### M3b: Autoscroll + drag-past-edge

- `setPointerCapture(pointerId)` on `pointerdown` so the drag survives the cursor leaving the webview.
- RAF-driven autoscroll loop when the pointer is within `EDGE_AUTOSCROLL_PX = 30` of the viewport's top or bottom. Speed
  proportional to distance from edge, capped at ~30 lines per frame at 60 fps. Stops on `pointerup`, `pointercancel`,
  window `blur`, or when the pointer re-enters the safe band.
- Window `blur` handler as a safety net: macOS may hand focus to another app mid-drag without firing pointer events.
- The viewer's existing fetch-on-scroll pipeline brings in newly-needed lines automatically as the autoscroll advances.

**Tests**:

- Vitest for the autoscroll speed curve (pure function: `(distanceFromEdge, edge) → linesPerFrame`).
- Vitest: a `blur` event stops the autoscroll loop (mock the RAF loop's cancel hook).
- Vitest: `pointercancel` stops the loop and freezes selection.
- Playwright e2e: a drag-past-edge spec that pulls the cursor out of the viewport, expects autoscroll to pull more lines
  into view, releases the pointer outside the webview (synthesised mouseup-outside via a Playwright trick or a scoped
  move past the window edge), ⌘C, asserts the clipboard contains content from beyond the original buffer. Uses
  `pollUntil` for the autoscroll reach.

**TDD posture**: medium. The pure functions are TDD-friendly; the integrated drag-past-edge gesture needs a real
browser.

### M4: Right-click Copy menu, copy event interception, shift-click extend

- `contextmenu` handler on `.file-content`: opens a minimal context menu (Copy; Select all when nothing is currently
  selected). Investigate which existing menu pattern to copy from (file explorer pane context menu is the candidate); if
  none of them is a clean fit for a viewer-only menu, create a small dedicated component.
- `copy` event interception unifies ⌘C from the menu, the keyboard, and the future toolbar (none today).
- Shift-click: extends the selection from the existing anchor to the clicked position. If no current selection,
  shift-click is treated like a plain click (set anchor = focus to clicked position).

**Tests**:

- Playwright e2e: open viewer, right-click in the content, click Copy, assert clipboard.
- Vitest for shift-click extension (pure function on selection).
- A11y: confirm the context menu meets the existing context-menu accessibility patterns in the codebase.

### M5: Polish

- Double-click selects the word under the pointer; triple-click selects the line. Word boundaries follow standard
  Unicode word boundary rules from `Intl.Segmenter('en', { granularity: 'word' })` (no extra dependency, Safari 14.1+).
- **Save as file…** action wired up in the 10–100 MB confirm and the > 100 MB refuse dialogs. Opens the native macOS
  save panel via Tauri's dialog plugin (`tauri-plugin-dialog`); on confirm, streams the selection to the chosen path via
  a new `viewer_write_range_to_file` command. Cancelable. Reuses the redact-and-write safety patterns from elsewhere (no
  need to redact here: it's user content, but the atomic temp+rename pattern from `file_system/write_operations/` is the
  model).
- Final accessibility pass: VoiceOver announces the selection ("Selected lines 12 to 47, 1,240 characters" or similar).
  ARIA live region. RTL text rendering check.
- Docs sweep:
  - Update `apps/desktop/src/routes/viewer/CLAUDE.md` with the Selection model section (state shape, segmenter
    integration, autoscroll, copy bands).
  - Update `apps/desktop/src-tauri/src/file_viewer/CLAUDE.md` with the `viewer_read_range` command and any new gotchas
    surfaced during M2.
  - Update `docs/architecture.md` only if a surface (new command-palette entry, etc.) crosses a docs boundary.

**TDD posture**: high for the double / triple-click word boundary logic (pure, fuzz with Intl.Segmenter); low for the
save panel path (Tauri dialog integration is hard to unit-test, manual + Playwright).

## TDD posture per milestone

| Milestone | TDD-friendly                                                            | Test-after                                         |
| --------- | ----------------------------------------------------------------------- | -------------------------------------------------- |
| M1        | All of it: selection model, segmenter math, ⌘A range                    | Visual rendering is manual-verify                  |
| M2        | Threshold function, byte estimator, `viewer_read_range`, IPC contract   | Toast / dialog visuals are manual + a11y           |
| M3a       | Sibling-offset summation, caret-from-point clamp on emoji boundaries    | Drag-within-viewport gesture (Playwright)          |
| M3b       | Autoscroll speed curve, blur and pointercancel stop the loop            | Drag-past-edge gesture with mouseup-outside (E2E)  |
| M4        | Shift-click extension                                                   | Context-menu invocation (E2E)                      |
| M5        | Word-boundary segmentation, save-stream chunking, byte count formatting | macOS save-panel integration (manual + Playwright) |

The honest answer is "TDD all the pure parts, Playwright the gestures, manual-verify the visuals." That's what's
realistic.

## Latitude to fix latent bugs

Agents working on these milestones are explicitly invited to fix small (~10–15 LoC) latent bugs they find in viewer
code, even outside the strict scope of the milestone they're on. Examples that might surface:

- Off-by-one in `getLineHeight()` callers.
- Wrong handling of files without a trailing newline.
- Search highlighting that doesn't re-apply when the user scrolls to a previously-uncached line.

Correctness and bug-free code over crystal-clean commits. If a bug is larger than ~15 LoC or touches behaviour outside
the viewer, surface it and ask.

## Existing coverage baseline

Before writing new tests, here's what already exists for the viewer:

### Rust (`apps/desktop/src-tauri/src/file_viewer/`)

- `byte_seek_test.rs` (512 lines): UTF-8 boundary handling, backward-scan limit, search match offsets in ByteSeek mode,
  backend search highlighting.
- `full_load_test.rs` (380 lines): basic line iteration, fraction-seek, search column UTF-16 semantics.
- `line_index_test.rs` (404 lines): sparse checkpoint construction, line-to-byte mapping at and across checkpoint
  boundaries.
- `session_test.rs` (387 lines): backend upgrade ByteSeek → LineIndex, the `search_cancel` ↔ `SearchStatus::Cancelled`
  flow (the gotcha around not nulling `session.search` first).

**Coverage gap**: no range-extraction tests today. The three backends know how to iterate lines and search, but nothing
reads "from `(line, offset)` to `(line, offset)`". This is new territory for M2.

### Frontend (`apps/desktop/src/routes/viewer/`)

No Vitest tests today. The supporting modules (`viewer-scroll.svelte.ts`, `viewer-search.svelte.ts`,
`viewer-line-heights.svelte.ts`, `viewer-keyboard.ts`) are uncovered by colocated `*.test.ts` files.

Adjacent coverage exists outside the route folder:

- `apps/desktop/src/lib/ipc/viewer.test.ts` (173 lines): IPC contract tests for the existing viewer commands.
- `apps/desktop/src/lib/file-viewer/binary-warning.test.ts` (98 lines): the binary-file warning banner.

**This means M1's `selection.svelte.test.ts` will be the first Vitest test in `routes/viewer/`.** Treat that as an
opportunity to set the bar: tight, fast, deterministic.

### Playwright (`apps/desktop/test/e2e-playwright/viewer.spec.ts`)

277 lines. Covers: open viewer via `open-file-viewer` event, content renders for a small fixture, the binary-warning
banner appears for binary files, the search bar opens, the Escape and W shortcuts. The existing fixture is a 1 KB
`file-a.txt`.

**Plan**: extend this file, don't fork. Add new fixtures (a 20 MB synthesised file for the confirm band; a 120 MB file
for the refuse band) into `apps/desktop/test/e2e-playwright/fixtures.ts` or the shared fixture root.

### Implied posture

The viewer is **decently** tested on the Rust side (over 1,500 lines of test for the three backends and the session) and
**thin** on the frontend side (one IPC contract test, one tiny component test, one e2e spec). David's intuition is
right: under-tested overall on the frontend, well-tested on the backend.

The new code (selection model, copy flow, drag) is mostly frontend-and-IPC. Pure-function-heavy. TDD-friendly. The
selection feature is a chance to seed Vitest coverage for the viewer's frontend modules.

## Tests strategy

### Vitest (frontend unit)

Colocated `*.test.ts` files next to each new module:

- `selection.svelte.test.ts`: model, segmenter math, byte estimator, edge cases (empty file, single line, only-`\n`,
  multi-byte UTF-8, surrogate pairs, anchor == focus, reversed drag, line beyond totalLines, half-open boundary
  semantics).
- `viewer-pointer.test.ts`: caret-from-point sibling-offset summation, nested-mark handling, emoji-boundary clamp.
- `viewer-copy.test.ts`: `selectCopyAction(bytes)` threshold function with the four boundary cases (10 MiB - 1, 10 MiB,
  100 MiB - 1, 100 MiB), IPC call shape (mocked).
- `viewer-autoscroll.test.ts`: speed curve pure function, blur stops the RAF loop, pointercancel stops the loop.
- A11y tier-3 tests for the new dialog components (`confirm` band, `refuse` band).

### Rust nextest (backend unit)

In `apps/desktop/src-tauri/src/file_viewer/session_test.rs`:

- `viewer_read_range_full_load_*`: anchor == focus (returns `""`), single-line, multi-line, reversed, out-of-bounds
  clamp, `RangeEnd::Eof`.
- `viewer_read_range_byte_seek_*`: same matrix, with `RangeEnd::Eof` exercising the no-index ⌘A path.
- `viewer_read_range_line_index_*`: same matrix, with range spanning checkpoint boundaries.
- `viewer_read_range_utf16_surrogate_*`: fixture with `"👋"`, offset 1 clamps to offset 0.
- `viewer_cancel_read_*`: spawn a slow read on a test backend, cancel mid-flight, assert `ViewerError::Cancelled`,
  assert `active_reads` no longer holds the entry.
- Proptest: round-trip property "stitching adjacent ranges equals one big range".

### Playwright e2e

In `apps/desktop/test/e2e-playwright/viewer.spec.ts` (extend, don't fork):

- **⌘A then ⌘C copies the whole file** (small fixture, `silent` band).
- **Drag within viewport, ⌘C, assert clipboard** (M3a).
- **Drag with autoscroll past the initial buffer, ⌘C, assert clipboard contains content from beyond the buffer** (M3b).
- **Right-click Copy** (M4).
- **`confirm`-band dialog appears for a 20 MB selection, Cancel cancels, Copy copies** (synthesised fixture).
- **`refuse`-band dialog appears for a 120 MB selection, Got it dismisses (M2), Save as file… writes (M5)**.

Use `dispatchMenuCommand` for any menu-bound copies once the menu lands. Synthesised keyboard events only for tests
explicitly about the keyboard pathway.

### What gets tested where

| Property                            | Where                       |
| ----------------------------------- | --------------------------- |
| Selection model correctness         | Vitest unit                 |
| Byte estimator correctness          | Vitest unit                 |
| Threshold band selection            | Vitest unit                 |
| Backend range read across backends  | Rust nextest                |
| IPC contract drift                  | Vitest IPC contract test    |
| Dialog component ARIA               | Tier-3 a11y                 |
| Keyboard ⌘A pathway                 | Playwright (one test only)  |
| Mouse-drag gesture                  | Playwright                  |
| Autoscroll behaviour                | Playwright                  |
| Visual rendering of selected ranges | Manual verify + tier-3 a11y |

## Docs updates

- `apps/desktop/src/routes/viewer/CLAUDE.md` (M5 sweep, but draft updates in each milestone). Commit to these specific
  additions:
  - New "Selection model" section: state shape (`{ anchor, focus } | null`), half-open `[start, end)` semantics, why not
    the Selection API, segmenter integration, byte estimator.
  - Gotcha: "Selection offsets are UTF-16 code units. Backend clamps lone surrogates to the nearest codepoint boundary."
  - Gotcha: "`user-select: none` on `.file-content` is intentional; do not revert. Status bar opts back in with
    `user-select: text` so users can copy the file name."
  - Gotcha: "Sibling spans inside `.line-text` are added by search highlighting; caret-offset math must sum across
    siblings."
  - Gotcha: "Drag autoscroll uses `setPointerCapture` + window `blur` fallback because the Tauri webview can lose
    mouseup events to other windows."
- `apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`:
  - Add `viewer_read_range`, `viewer_cancel_read`, and (M5) `viewer_write_range_to_file` to the Tauri commands table.
  - Document the `RangeEnd` enum and the UTF-16 → UTF-8 offset conversion with the surrogate-clamp rule.
  - Gotcha: "`viewer_read_range` cancellation flag lives per-read in `session.active_reads`, not on the session, same
    lesson as `search_cancel`. A session-wide flag races against concurrent reads."
- `docs/architecture.md`: only if M5 adds something cross-cutting (the `viewer_write_range_to_file` save path). The
  Frontend / Backend tables don't need rewording for the selection feature itself.
- AGENTS.md doesn't need updating.

## Checks

Before declaring each milestone done:

- `./scripts/check.sh` (full default suite).
- `./scripts/check.sh --check oxfmt` after any final-touch edits (oxfmt is in `AppOther`, not in `--rust` / `--svelte`,
  per AGENTS.md).

Before declaring the whole feature done (after M5):

- `./scripts/check.sh --include-slow`. Allow ~20 min. This catches the e2e drift, the Playwright cascade failures, the
  Rust-tests-linux build, the eslint-typecheck.
- One full `desktop-e2e-playwright` warm-then-cold run per the M3 / M4 process notes in `docs/testing.md` § Process.
- Manual exercise via the `cmdr` MCP server: open the viewer on a real file, drive ⌘A, copy, drag, autoscroll, and the
  three size-band dialogs through `mcp__cmdr-dev__*` tools. Per AGENTS.md, MCP is the primary way to test the running
  app, not the browser. The viewer's MCP surface lives in `src-tauri/src/mcp/`; extend it if needed (for example a
  `viewer_get_selection` tool for the e2e and MCP-driven flows).

## Risk register

| Risk                                                                                                     | Mitigation                                                                                                                                                                                                                                               |
| -------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `navigator.clipboard.writeText` chokes at 50–100 MB on macOS                                             | Test during M2 with a real 80 MB write. If it fails, fall back to a Tauri command that calls NSPasteboard directly (the file clipboard module has the plumbing).                                                                                         |
| `navigator.clipboard.writeText` requires a user-gesture context that gets lost across the confirm dialog | The dialog's Copy button click _is_ a user gesture, so the call from inside the click handler is in-context. Verified manually as part of M2. If the awaited backend read interleaves and breaks the gesture, fall back to the Tauri NSPasteboard route. |
| `document.caretPositionFromPoint` is unavailable in older WebKit                                         | Tauri 2 ships WebKit recent enough for this API; verify on the minimum supported macOS (the project's tauri config has the target). Fallback: `caretRangeFromPoint`.                                                                                     |
| IME / composition in the viewer                                                                          | Non-issue: read-only viewer, no IME composition possible.                                                                                                                                                                                                |
| VoiceOver announcement of selection                                                                      | ARIA live region for "selected N lines, ~M bytes" updates. Confirmed in M5's a11y pass. Test with VoiceOver on a real Mac.                                                                                                                               |
| RTL text rendering of partial selection                                                                  | The segmenter outputs spans in logical order; CSS handles BiDi reordering automatically. Verify with one Arabic test line in the Vitest segmenter tests (visual is manual).                                                                              |
| Byte-size estimation cost for huge ranges                                                                | O(checkpoints) for LineIndex, O(lines) for FullLoad (capped by file size which capped FullLoad at 1 MB anyway). For ByteSeek-no-index, the fallback path returns "unknown" and refuses. Acceptable.                                                      |
| Backend range read blocks for long on a huge slow-disk file                                              | `blocking_with_timeout` with computed timeout, cancellation via Escape sets an AtomicBool the read loop checks per chunk. Same pattern as the existing search cancellation.                                                                              |
| Native macOS pasteboard size limit                                                                       | NSPasteboard accepts large payloads. The 100 MB ceiling is for downstream pastes, not for the pasteboard write itself.                                                                                                                                   |
| Anchor on a recycled DOM node                                                                            | Selection state lives outside the DOM. The DOM is purely a renderer of the state. No risk by construction.                                                                                                                                               |
| Search-highlight mark + selection-highlight on the same span (visual collision)                          | Design call during M1: selection wins on background colour, search wins on text colour (or whichever the visual test prefers). Document the final call in CLAUDE.md.                                                                                     |
| `caretPositionFromPoint` returns `null` over `user-select: none` text in some WebKit versions            | M3a spike verifies on the current macOS target. Fallback: `elementsFromPoint` + per-character pretext-measured width to derive the offset. Document the result in the M3a milestone notes.                                                               |
| Status-bar hint width on narrow viewer windows                                                           | M1 confirms fit on a 600 px-wide window. If it doesn't fit, drop the leading `W wrap ·` chunk (the wrap badge tooltip carries the same shortcut). Last resort: `useShortenMiddle` action or `text-overflow: ellipsis`.                                   |
| Concurrent ⌘C while a previous read is still running                                                     | Disable the copy action while a read is in flight (busy flag on the selection store). A second ⌘C is a no-op.                                                                                                                                            |
| `navigator.clipboard.writeText` rejects in non-secure contexts                                           | The Tauri webview runs `tauri://` which is treated as a secure context, but verify with the M2 step 1 spike (1 KB write). If it rejects, the fallback is a Tauri-side NSPasteboard command.                                                              |
| macOS native context menu opens on right-click                                                           | M4 calls `event.preventDefault()` at the start of the `contextmenu` handler before opening our custom menu. Without this, both menus would stack.                                                                                                        |

## Parallelism notes

Same-worktree, generally sequential. The split into M3a / M3b is itself a sequencing call (autoscroll plumbing alone is
meaty), but they can land in one chunk if the implementation lines up cleanly.

Seams where work can overlap:

- M5's docs sweep can be drafted in parallel with M3 / M4 code work. Different files.
- The Rust `viewer_read_range` unit tests (M2 backend Step 2) can be written before the M2 frontend (TDD posture).
- The clipboard write spike (M2 Step 1) is gating; everything else in M2 waits on it.

## Intent capture

Every decision in this plan has its rationale spelled out in the section that introduces it. The short version:

- **Why custom selection model**: native Selection API can't survive DOM recycling. We considered the route of letting
  the browser select and intercepting `copy`; it loses to virtualisation every time.
- **Why UTF-16 offsets**: matches the search engine's existing choice and the JS string model. Off-by-one bugs at the
  IPC boundary are a known time-sink we're avoiding.
- **Why three size bands**: silent for the common case (< 10 MB respects the user's flow), confirm for the danger zone
  (10–100 MB respects the design principle of "the user is always in control"), refuse for the unsafe zone (> 100 MB
  respects design principle 5 about resources).
- **Why `viewer_read_range` instead of returning lines individually**: one IPC round-trip per copy. Streaming is
  premature until evidence shows it's needed.
- **Why suppress native selection**: stops the browser from rendering a competing-and-broken selection on top of ours.
- **Why the segmenter pattern from search**: zero new render mechanism. The existing pattern handles nested highlights
  and is already tested.

If you're working in a milestone and you find yourself fighting the design, stop and re-read the open question that
matches your situation. If your situation isn't there, surface it as a new open question rather than improvise.

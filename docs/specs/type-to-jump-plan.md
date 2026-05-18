# Type-to-jump plan

Quick navigation inside the current directory by typing. User types characters in a focused file pane → the cursor jumps
to the best-matching file. A small "Jump: tes" indicator sits at the bottom-right of the pane while it's active. Idle
for 1 s and the buffer resets; idle for 5 s and the indicator vanishes.

## Why

The primary goal is **fast in-directory navigation without leaving the keyboard**. macOS Finder, Total Commander, and
most file managers have this. Cmdr currently forces the user to arrow-key or grab the mouse to reach a file by name,
which is unacceptable for a keyboard-first app (see `design-principles.md`).

This is not whole-drive search (that's `command-palette/` and `search/`). It's strictly a cursor jumper inside the
currently displayed listing.

**Indicator label**: the original spec used "Search: " but that collides with the actual Search feature in
`command-palette/` and `search/`. "Jump: " is unambiguous and shorter. (Final wording is David's call, flagged below in
§ Open questions.)

## Key decisions (and why)

### 1. Match algorithm: fuzzy, top-scoring wins

David picked fuzzy over prefix or substring. Fuzzy is more forgiving: typing "tjs" lands on `tests.js`, "rdme" lands on
`README.md`. The top-scoring match wins; sort order is the tiebreaker (lower index wins on a tie).

**Why this matters for UX**: Fuzzy is less predictable than prefix. If the user types "te" and the highest-scored match
is `Templates/` even though `tests/` exists, the cursor jumps to `Templates/`. That's OK. They keep typing ("tes") and
the match converges on `tests/`. The 1 s reset means a typo is forgotten quickly.

**Why scoring respects sort only as tiebreaker (not primary)**: If sort were primary, fuzzy would degrade to "first
entry that fuzzily matches in this sort order," which is barely different from substring and ignores the user's intent
(they used a non-prefix character sequence on purpose).

### 2. Backend-driven

The frontend pane only holds a ~500-entry prefetch buffer around the visible scroll window (verified:
`apps/desktop/src/lib/file-explorer/views/CLAUDE.md` § "Data flow": "Data lives in Rust `LISTING_CACHE`. Frontend
fetches visible ranges on-demand via `getFileRange`"). The full sorted listing lives in the backend's `LISTING_CACHE`
(`apps/desktop/src-tauri/src/file_system/listing/caching.rs`).

The alternative (ship all filenames to the frontend on listing complete) was rejected: 100k filenames is ~3 MB
serialized, churns IPC, and re-introduces the very Svelte-reactivity bloat that the non-reactive store explicitly
avoids.

Note: this is "display logic on data only the backend has," not the usual "business logic in Rust" call. The
command-palette is fully frontend-side because it operates on the ~60 in-memory commands; it would be the wrong model to
copy here.

A single IPC call per keystroke, returning one `usize` (the matched index), is the right shape. Latency: IPC round-trip
~1–5 ms + match time. For ≤10k entries, total <10 ms. For 100k entries, target <30 ms with rayon-parallelized scoring
(see § Performance).

### 3. Same-letter → just appends to buffer

No Finder-style "press 't' three times to cycle through t-files" smart-cycling. Buffer is buffer; arrow keys are for
cycling. Predictable and matches the original spec.

### 4. Trigger keys: letters and digits only, no modifiers, when a file list is focused

- **Accept**: `a-z`, `A-Z`, `0-9`. Lowercased before appending to buffer (fuzzy is case-insensitive anyway).
- **Skip**: Cmd/Ctrl/Alt-modified keys (so `Cmd+T`, `Cmd+F`, etc. keep working). Skip when any input/textarea has focus
  (rename editor, search dialog, breadcrumb edit, settings, command palette). Skip space, dot, dash, underscore, as they
  often collide with other behaviors (space toggles selection) and the fuzzy matcher tolerates their absence.
- **Active pane only**: type-to-jump fires for whichever pane has focus. Switching panes clears the other pane's buffer
  - indicator.

### 5. Timeouts: only buffer-reset is user-configurable

- **Buffer reset**: default 1000 ms, adjustable in **Settings > Advanced** as `fileExplorer.typeToJump.resetDelay`.
  Range: 300–3000 ms, step 100 ms. Slider component.
- **Indicator hide**: hardcoded 5000 ms from last keystroke. Cosmetic detail: making it adjustable adds setting bloat
  for no real-world payoff.

**Indicator lives longer than the buffer, with a visual cue for the gap.** After the 1 s buffer reset, the indicator
shifts into a "stale" visual state (italic + reduced opacity, label prefixed with a subtle "·") so the user can see at a
glance that the next keystroke will start fresh, not append to the displayed buffer. Without this cue, the spec's
asymmetric timeouts cause a confusing one-frame contradiction: indicator says `Jump: tes`, user types `f`, indicator
flips to `Jump: f`. The stale style makes the reset visible and the transition unambiguous. The 5 s indicator hide
applies to the stale state too. (Radical transparency principle.)

### 6. Clears the buffer + indicator

- ESC
- Arrow keys, Page Up/Down, Home/End, Enter, Tab, Backspace
- Entering rename mode (F2 or click-to-rename): clears immediately so the indicator doesn't linger over the inline
  rename editor
- Opening a context menu (file or breadcrumb)
- Starting a drag
- Focus moving to the FunctionKeyBar, tab bar, breadcrumb edit, or any other interactive control inside the pane
- Switching active pane
- Switching tab within the same pane (each tab has its own listing)
- Directory change (navigation)
- Re-sort (sort column / order change): the listing's index space has changed, the buffer's previous match is stale
- 5 s after last keystroke (indicator only, buffer is already dead by then)

**Not reset on**: toggling hidden files. The buffer survives; the next keystroke evaluates against the new visibility
set. Wiping a 3-char buffer the user is mid-typing because they pressed Cmd+Shift+. would be hostile.

**Backspace caveat**: Backspace is listed in the reset triggers because it already maps to "navigate to parent" in the
file explorer (see `file-explorer/CLAUDE.md` § "Back/forward navigation"). Users mid-type may expect Backspace to delete
the last char from the buffer. Choosing to keep navigation behavior because (a) reassigning Backspace is a bigger keymap
discussion, (b) the buffer is short-lived anyway, (c) ESC is the discoverable "abort" key. If this surfaces as a real
user complaint post-launch, revisit.

### 7. State per pane

Two panes can each have their own buffer/indicator/timers. The component-level state lives in `FilePane.svelte` (not
global), keyed implicitly by the pane instance.

### 8. Indicator placement: bottom-right, inside the pane

Per the spec. Not the toast system (toasts are global, not pane-local). Not a tooltip (tooltips are anchored to
elements; this is a free-floating ephemeral). A small absolutely-positioned `<div>` inside the pane root, styled like a
tooltip (soft shadow, rounded, low z-index). `pointer-events: none`.

Content: `Jump: tes` with the buffer rendered verbatim (no escaping needed, letters and digits only). Sentence case
label per the style guide.

**Placement gotcha (verify visually)**: `FunctionKeyBar.svelte` sits at the bottom of the window, and each pane has a
status bar/usage bar below the listing. The indicator must sit above both, anchored to the pane's content area (not the
window). Concretely: absolutely position inside the pane's file-list container, with bottom offset equal to the status
bar's height + small gap. Mock this in M2 and adjust before locking in.

### 9. Accessibility

- The indicator carries `role="status"` and `aria-live="polite"` so screen readers announce buffer changes and the
  matched filename (e.g., "Jump to AGENTS.md"). Pattern: see `apps/desktop/src/lib/ui/toast/` and `RepoChip.svelte`'s
  `aria-label`.
- The visual fade between active and stale states respects `prefers-reduced-motion`: no animated opacity transition when
  the user prefers reduced motion (replace with an instant flip). Top-5 design principle.
- The indicator does not steal focus (`pointer-events: none`, no `tabindex`). Keyboard focus stays on the file list.
- Visual styling uses existing design tokens (`--color-overlay-bg`, `--radius-sm`, `--shadow-elevated`, `--z-overlay`).
  No raw px for `font-size`, `border-radius`, `font-family`, or `z-index ≥ 10`. Stylelint enforces this.

## Architecture

### Backend

**New IPC command** in `apps/desktop/src-tauri/src/commands/file_system/listing.rs`:

```rust
#[tauri::command]
#[specta::specta]
pub async fn find_first_fuzzy_match(
    listing_id: String,
    query: String,
    include_hidden: bool,
) -> Result<Option<usize>, IpcError> { … }
```

Returns the backend index of the highest-scored fuzzy match, or `None`. The frontend translates this to a frontend index
by accounting for the `..` parent offset (see § Gotchas).

**Logic** lives in a new module `apps/desktop/src-tauri/src/file_system/listing/fuzzy_jump.rs`:

```rust
pub fn find_first_match(entries: &[FileEntry], query: &str, include_hidden: bool) -> Option<usize>
```

Pure function, easy to unit-test. Reads `LISTING_CACHE` via the existing helper, locks for read, scans entries.

**Fuzzy crate**: `nucleo-matcher` (Helix's matcher, used by Zellij and others; see § Open questions for the version
pin). Pros: ~µs per match, smart-case (lowercase query = case-insensitive), good scoring, MIT licensed. Cons: needs
verification on the latest version and license compatibility. Defer to `cargo deny check`.

**Skip rules during scoring**:

- Skip hidden files when `include_hidden == false`.
- Match the filename only (not extension separately, not the full path). Whole filename including extension goes into
  the matcher, since fuzzy already weighs prefix matches highly.
- (No special-casing for `..`: the synthetic parent entry is not in `LISTING_CACHE` at all; it's prepended by the
  frontend only. The parent-offset adjustment on the frontend is the single source of truth here. See
  `apps/desktop/src/lib/file-explorer/views/CLAUDE.md` Gotcha: "Index 0 is `..` parent entry (not in backend cache).")

**Performance**: For ≤10k entries, single-threaded scan is fine. For larger listings, use rayon's `par_iter` to score in
parallel and reduce to argmax. Benchmark to confirm; document the threshold in `notes/`.

**No new state**: the `LISTING_CACHE` already holds sorted entries. The matcher is rebuilt per call (it's stateless and
cheap). No per-listing fuzzy index needed.

**Logging**: every call logs a single `log::debug!(target: "type_to_jump", ...)` line with `listing_id`, query length,
result index, and elapsed micros. Useful for "why didn't my keystroke land?" diagnosis. The target string follows the
snake_case convention used elsewhere (`open_with`, `fda_gate`, etc.) and is filterable via
`RUST_LOG="type_to_jump=debug"` directly, or via the module path
`RUST_LOG="cmdr_lib::file_system::listing::fuzzy_jump=debug"` for the conventional module-path style. Per the logging
rule, never `println!`/`eprintln!` (see `apps/desktop/src-tauri/src/logging/CLAUDE.md`).

**Threading**: the command is `async` like the rest of the listing commands. The match itself is CPU-bound but small.
Wrap in `tokio::task::spawn_blocking` only if benchmarks show it stalling the runtime on large dirs. Default to inline
first, instrument with the debug log, decide.

### Frontend

**State in `FilePane.svelte`** (per pane):

```ts
let jumpBuffer = $state('')
let jumpIndicatorVisible = $state(false)
let jumpBufferResetTimer: ReturnType<typeof setTimeout> | null = null
let jumpIndicatorHideTimer: ReturnType<typeof setTimeout> | null = null
```

**Keydown interception**: Hook into `DualPaneExplorer.svelte`'s existing unified key handler. Before falling through to
existing shortcuts (`handleNavigationShortcut`, command dispatch, etc.), check if the keystroke is a printable
letter/digit with no modifiers and no input focus. If yes, intercept:

1. Append the lowercased char to `jumpBuffer`.
2. Show the indicator. Restart the 5 s hide timer.
3. Restart the 1 s buffer-reset timer.
4. Call `findFirstFuzzyMatch(listingId, jumpBuffer, includeHidden)`.
5. On result: convert backend index → frontend index (parent offset), set `cursorIndex`, scroll into view via the
   existing virtual-scroll API.

**Reset triggers** call a single `clearJumpState()` helper that nulls the buffer, hides the indicator, and clears both
timers.

**Settings integration**: Add `fileExplorer.typeToJump.resetDelay` to `settings-registry.ts` following the
`appearance.textSize` slider pattern. Default 1000 (ms), range 300–3000, step 100. Reactive getter
`getTypeToJumpResetDelay()` in `reactive-settings.svelte.ts`. Live-apply: the next debounce uses the new value naturally
No special applier code needed. Final section path in § Open questions.

**New component**: `apps/desktop/src/lib/file-explorer/pane/TypeToJumpIndicator.svelte`. Pure presentational, receives
`buffer: string` and `visible: boolean` as props. Rendered inside `FilePane.svelte`'s root.

**Typed IPC binding**: After adding the Rust command, regenerate bindings via `pnpm bindings:regen`. Call as
`commands.findFirstFuzzyMatch(listingId, query, includeHidden)`.

## Gotchas

- **Parent offset**: Backend listing indices are 0-based. The frontend prepends `..` when `hasParent`, making frontend
  index = backend index + 1. The IPC returns a backend index; the frontend must add 1 when `hasParent` before setting
  `cursorIndex`.
- **Streaming listings (MTP, SMB, large local dirs)**: A pane may receive a listing that's still being read in the
  background (`list_directory_start_streaming`). Calling `find_first_fuzzy_match` during streaming matches whatever's in
  `LISTING_CACHE` at that moment. A 60k-file MTP folder can arrive over several seconds, and the user typing "AGE" early
  can land on a partial match that gets superseded later when `AGENTS.md` arrives. **Decision: do NOT auto-jump on
  streaming progress.** The cursor moving under the user without a keystroke violates "the user is always in control"
  (top-5 principle 3). Behavior instead: a match lands once per keystroke, full stop. If the user types again after more
  entries have streamed in, the next IPC call evaluates against the fuller cache. Document this clearly in the
  indicator's `aria-label` (or a tooltip on the indicator) so power users understand: "Jump uses entries loaded so far.
  Type again to refresh as more arrive."
- **Sort changes mid-search**: If the user re-sorts while a buffer is active, the cursor's new position may diverge from
  the matched entry (since sort changes indices). Acceptable: re-sort is a rare interrupting action; we clear the buffer
  on re-sort as a safety net (treat re-sort like navigation).
- **Empty listings**: Return `Ok(None)`, frontend does nothing.
- **Frontend racing the backend**: User types fast. Keystrokes 1, 2, 3 fire three IPC calls. Response order may be 1,
  3, 2. Tag each call with a monotonic counter (a generation number); apply the response only if its generation is the
  highest seen. Same pattern as the diff-generation counter in
  `apps/desktop/src/lib/file-explorer/operations/adjust-selection-indices.ts` (see `file-explorer/CLAUDE.md` §
  "Operation lifecycle: Diff-driven adjustment").
- **Listing destroyed before response**: If the user navigates away mid-search, the response comes back referencing a
  dead listing_id. Frontend ignores it (the generation counter already protects this: the new listing has its own
  generation).
- **Hidden files toggle**: The pane already tracks `includeHidden`. Pass it as-is to the IPC. No reset of the buffer
  when the user toggles hidden. The next match re-evaluates against the new set. (Confirmed above in § 6.)
- **No interaction with the `--filter` command bar** (if any future feature adds inline filtering). This buffer is
  ephemeral and self-clearing; a future filter UI would be separate.

## Testing

### Unit (Rust)

`apps/desktop/src-tauri/src/file_system/listing/fuzzy_jump.rs` `#[cfg(test)] mod tests`:

- Empty listing → `None`.
- No matches → `None`.
- Single match → that index.
- Multiple matches → highest-scored.
- Ties → lower index wins.
- Hidden file excluded when `include_hidden == false`.
- Case-insensitive: "tes" matches `TESTS.txt`.
- (No "skip `..`" test: `..` is never in `LISTING_CACHE`, only prepended by the frontend.)
- Unicode filename: "résumé" matches `Résumé.pdf` reasonably (delegate to matcher's behavior; document what we observe).

### Unit (Frontend, Vitest)

`apps/desktop/test/lib/file-explorer/pane/type-to-jump-state.svelte.test.ts` (the factory lives in
`type-to-jump-state.svelte.ts` because it holds `$state` runes for buffer + indicator visibility). Factory shape:
`createTypeToJumpState({ getResetMs, onMatch, onIndicatorHide })`:

- Initial state: buffer empty, indicator hidden.
- `appendChar('t')` → buffer 'T'→'t', indicator visible.
- `clear()` → empty + hidden, timers cleared.
- 1 s elapsed (fake timers) → buffer empty, indicator still visible.
- 5 s elapsed (fake timers) → indicator hidden.
- Race protection: out-of-order match responses applied only if newer generation.

### A11y (Vitest with jsdom)

`apps/desktop/test/lib/file-explorer/pane/TypeToJumpIndicator.a11y.test.ts` (follow the pattern from
`FullList.a11y.test.ts`, `ErrorPane.a11y.test.ts`):

- Renders with `role="status"` and `aria-live="polite"`.
- Visible buffer text appears in the accessible name.
- Hidden state has `aria-hidden="true"` or is removed from the DOM.
- Stale state still announces (doesn't disable the live region mid-cycle).
- `prefers-reduced-motion: reduce` matches → no opacity transition class.

### Integration (Playwright E2E)

`apps/desktop/test/e2e-playwright/tests/type-to-jump.spec.ts`:

- Open a known fixture directory (e.g., the test repo's `apps/desktop/src/lib/`), type "fil" → cursor lands on
  `file-explorer/` (or first fuzzy hit).
- Indicator appears, shows "Jump: fil".
- Press ESC → indicator gone, cursor unchanged.
- Type "co", wait 1.2 s → indicator shifts to stale state. Type "se" → buffer is "se", not "cose"; indicator returns to
  active.
- Press Cmd+T → no buffer added (modifier skip), command palette opens.
- Type when search dialog is open → no jump fires (input focus guard).
- Type "abc" in pane A, click pane B → indicator gone in pane A.
- Switch tab within the same pane while indicator is up → indicator gone.
- Verify indicator doesn't overlap the FunctionKeyBar at the minimum supported window height.

### Manual

David runs the dev app, opens `~/projects-git/vdavid/cmdr` in a pane, types "AGE" → cursor lands on `AGENTS.md`. Tests
the indicator placement visually on macOS dark/light. Verifies the indicator doesn't overlap the bottom status bar or
the function-key bar. Adjusts the timeout setting in Settings > File explorer > Advanced, retypes, confirms the new
timeout takes effect immediately.

## Docs updates

- `apps/desktop/src/lib/file-explorer/CLAUDE.md`: Add a "Type-to-jump" subsection under the "Pane (`pane/`)" section
  describing the state model, reset triggers, and parent-offset gotcha.
- `apps/desktop/src-tauri/src/file_system/listing/CLAUDE.md` (create if missing, or add to the existing one): Note
  `fuzzy_jump.rs` and the IPC entry point.
- `apps/desktop/src-tauri/src/commands/CLAUDE.md`: Add `find_first_fuzzy_match` to the listing command list.
- `docs/architecture.md`: No change needed; "Type-to-jump" is a sub-feature of the file explorer.
- Style guide doesn't need updating; the indicator label "Jump: …" already follows sentence case + colon-list rules.

## Milestones

Sequential. Each ends with `./scripts/check.sh` (or at least `--rust`/`--svelte` + `--check oxfmt`).

### M1: Backend: fuzzy match + IPC

Files: `file_system/listing/fuzzy_jump.rs` (new), `commands/file_system/listing.rs` (add command), bindings regen.

- Add `nucleo-matcher` to `Cargo.toml` (verify latest version via crates.io; pin to a version ~1 month old per
  `~/.claude/rules/use-latest-dep-versions.md`).
- Run `cargo deny check licenses` for the new dep.
- Write `fuzzy_jump::find_first_match`. Unit-test it.
- Add the Tauri command wrapping it. Pattern: thin pass-through, timeout-protected (this hits in-memory state, not the
  filesystem, so 1 s timeout is plenty).
- `pnpm bindings:regen` (from `apps/desktop/`) to surface `commands.findFirstFuzzyMatch`.
- Checks: `./scripts/check.sh --rust && ./scripts/check.sh --check oxfmt`.

**Done when**: Rust unit tests pass, bindings file is fresh, no clippy warnings.

### M2: Frontend: state + keyboard plumbing + indicator

Files: `file-explorer/pane/type-to-jump-state.svelte.ts` (new factory), `TypeToJumpIndicator.svelte` (new),
`FilePane.svelte` (wire state + indicator), `DualPaneExplorer.svelte` (keyboard intercept).

- Extract a pure factory `createTypeToJumpState({ getResetMs, onMatch, log })` returning
  `{ buffer, indicatorVisible, appendChar, clear }`. State + timers internal. This is the testable unit.
- Add `TypeToJumpIndicator.svelte` with bottom-right absolute positioning, tooltip-like styling using existing CSS
  tokens (`--color-overlay-bg`, `--radius-sm`, `--shadow-elevated`, `--z-overlay`). No raw px for `font-size`,
  `border-radius`, etc. Design-token check applies.
- Wire into `FilePane.svelte`: instantiate the state, render the indicator, expose a `handleJumpKeystroke(char)`
  callback to the parent dispatcher.
- In `DualPaneExplorer.svelte`'s keydown handler, before falling through to command dispatch / nav shortcuts, check the
  keystroke: printable letter/digit, no modifiers, no input focus, file list scope active. If yes, route to the active
  pane's `handleJumpKeystroke`. If a nav/edit key (arrow, enter, esc, etc.), call `clearJumpState()` on the active pane.
- Wire reset triggers: pane focus change, directory change, sort change, listing replace.
- Add the race-protection generation counter.
- Checks: `./scripts/check.sh --svelte && ./scripts/check.sh --check oxfmt`.

**Done when**: Manual test in `pnpm dev` shows the indicator appearing, cursor jumping, ESC/timeouts working.

### M3: Settings integration

Files: `settings/settings-registry.ts`, `settings/reactive-settings.svelte.ts`, `settings/settings-applier.ts` (if
needed), the **Advanced** section component.

- Add `fileExplorer.typeToJump.resetDelay` to the registry under section path **Advanced**. Default 1000 (ms), min 300,
  max 3000, step 100. Slider component.
- Add reactive getter `getTypeToJumpResetDelay()`.
- Verify the slider renders in the Advanced settings section. Read `settings/sections/` first to confirm the exact
  filename and the section path string used by other Advanced entries.
- Update the type-to-jump state factory to read the setting via the getter on each timer reset (so live changes apply on
  the next keystroke).
- Checks: `./scripts/check.sh --svelte && ./scripts/check.sh --check oxfmt`.

**Done when**: Manually changing the slider in Settings live-applies without restart.

### M4: Tests + MCP + docs + final check

- Write the Vitest unit tests for the state factory.
- Write the a11y Vitest test for `TypeToJumpIndicator.a11y.test.ts`.
- Write the Playwright E2E spec covering golden paths.
- **MCP surface**: `DualPaneExplorer.svelte` already exposes pane state to the cmdr MCP server (port 19224 prod / 19225 dev). Add the
  type-to-jump buffer + indicator visibility + last matched filename to that surface so agents can drive and assert this
  feature in tests. See `src-tauri/src/mcp/CLAUDE.md` for the resource conventions.
- Update CLAUDE.md files noted in § Docs updates.
- Run `./scripts/check.sh` (full suite). Fix any warnings (file-length allowlist: leave warnings as warnings per
  `~/projects-git/vdavid/cmdr/.claude/rules/file-length-allowlist.md`).
- Verify coverage holds at ≥70% for the new files. If something's untestable, extract a pure helper rather than
  allowlisting.

**Done when**: All checks green. Manual verification on macOS dark and light themes.

## Decisions confirmed by David

1. **Indicator label**: "Jump: tes" (not "Search: tes").
2. **Settings location**: **Settings > Advanced** (the top-level Advanced section, NOT File explorer > Advanced). The
   Advanced section exists today (see `settings/sections/AdvancedSection.svelte` or similar; verify the exact filename
   in M3).

## Decisions deferred to the implementer (pick during M1/M2 and document)

1. **`nucleo-matcher` vs alternatives**: confirm via crates.io and a quick benchmark that nucleo-matcher is the right
   pick (vs `sublime_fuzzy`, `fuzzy-matcher`). Pin to a ~1-month-old stable version before opening M1. Run
   `cargo deny check licenses` and resolve up front. Don't merge with the question open.
2. **rayon parallelization threshold**: measure single-threaded scoring on 10k, 50k, 100k entry directories. If
   single-threaded stays under ~20 ms even at 100k, skip rayon entirely (simpler code).

## Out of scope (for now)

- **Cycling matches by re-typing the same letter** (Finder-style). The user picked buffer-only semantics.
- **Highlighting matched characters in the file list** (like the command palette does). Nice-to-have; defer.
- **Number-key acceleration** (e.g., typing "5g" to jump to file 5 with prefix "g"). Defer.
- **Backspace to delete the last char from the buffer**. Spec says backspace clears + navigates up; not changing that.
- **Search across panes simultaneously**. Per-pane only.
- **Persisting the buffer across navigation**. Buffer dies on navigation by design.
- **Wraparound at end of list**. Top-scoring fuzzy doesn't need wraparound, there's a single best match.

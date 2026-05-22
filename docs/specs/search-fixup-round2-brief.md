# Search dialog fix-up: round 2 brief

Round 1 landed as commit `c2be4b2f`. This is round 2: David reviewed the running app and has another tight list. TDD
this round explicitly: write the failing tests first to repro the bugs, run the suite (red), implement, run again
(green).

## David's verbatim feedback

> Much better! Commit the progress. But still some issues. This is what it looks like now: [screenshot]
>
> - Dialog
>   - Replace the little glowing dot with our normal spinner for the "Searching" state, with the label "Searching..."
>     underneath
>   - When a search is currently running (after the debounce, so when the search is actually running), the result list
>     should be replaced the single spinner, with the label "Searching..." underneath.
>   - Now both the content of the result list AND its status bar says searching. The status bar should just be empty so
>     we don't duplicate that info.
>   - Similarly, for there are no results, the content says "No files found", and the status bar "No results". The
>     content should be: "No files match these criteria:\n{bulleted list of the criteria}", and the status bar should be
>     empty.
>   - At the bottom of the window, the recent searches bar should have the label "Recent searches:" so it's clearer what
>     these are.
>   - The two Open buttons at the bottom-right should be always visible, just disabled when inactive
>   - The "Go to file" button says "⏎" on it, which would be cool if it worked, but it doesn't currently work. Please
>     wire it up so that it opens the current search result when active. The logic should be that: IF we have search
>     results on display (and thus, the "Go to file" button is enabled) AND the last event was either "Search results
>     arrived" or "Cursor moved", then ⏎ does "Go to file". ELSE (if there are no search results on display OR the last
>     action was edits to the search text or the filters, then ⏎ runs the search.
>   - The `⏎` button at the top-right should have the text `Search ⏎`, and the `⏎` shortcut on both buttons should come
>     and go dynamically, based on the current state of what the ⏎ button actually does (see above), so it's always only
>     displayed in one location, depending on what it actually does in the current state.
>   - The shortcuts ⌥F and and ⌥D for "Search in → Use current folder / All folders" don't actually work, they are not
>     wired up.
>   - Size filter selector should have no dropdowns but the options listed in lists. Like, it'd be three to five
>     columns, in the first column `none`, `any`, `>=`, `<=`, and `between`. Second column the numbers 0, 1, 5, 10, 20,
>     50, 100, 200, 500, Custom..., third column `bytes` (or byte, depending on whether the number selected before it is
>     1 or not), `KB` (or `kB`, depending on the current setting for SI/binary), `MB`, `GB`), and if the first column
>     selection is "between" then two more columns like this. If first col setting is "none" then three columns are
>     displayed, but the second and third columns are disabled. This layout saves the user a few clicks. Same change
>     needed to the "Modified" filter. And both need some shortcut like ⌥S and ⌥M.
>   - In filters, when I say "Search in" → Use current folder, and current folder is search results, I get
>     `search-results://sr-1` or similar. It should go back in history at dialog load and see if we have a last opened
>     folder in the navi list; if yes, use that here; if no, then this should be disabled, with the tooltip describing
>     that the current folder is not searchable.
>   - AI/filename/regex modes should have some shortcut, like ⌥A (already taken — "Show all in main window should
>     probably be ⌥S then"), ⌥F, and ⌥R
> - Result list:
>   - The headers are all over the place, not aligned with the actual columns at all. They should align (left-aligned).
>   - Path width is measured wrong: there is plenty of space available, but I still see the "...", and then a lot of
>     empty space
>   - "Show all in main window" button has ⌥A written on it, but ⌥A is not wired up. It should be wired up.
> - Results pane in main window
>   - These shortcuts don't work on PgUp/PgDown, (probably also Home/End), Left/Right, selection with Space (probably
>     Insert fails too, I don't have Insert on my kb - but weirdly, "Toggle selection (Space)" in the context menu
>     works), selection with ⇧Up/⇧Down.
>   - ⌘A doesn't select the first item, only the rest.
>   - F3, F4, Show in Finder, Open, Open with don't work. Weirdly, Get info and Quick look do work.
>   - `Copy path` doesn't work, and `Copy {filename}` is effectively `Copy {fullpath}` here because we display the full
>     path for the name. Logic for the `Copy {filename}` action should be extended, both for the display and for what it
>     actually copies, and both should be just the filename. Plus this action doesn't currently work either.
>
> Fix these in TDD please whereever reasonable. I want to make sure that the agent finds the actual problems and can
> repro them and then really fixes them, plus helps us avoid regressions later. (Building is expensive, so best to write
> a bundle of failing tests first, then fix them.)

## Resolved clarifications

**Final shortcut allocation** (use these everywhere, including the inline shortcut hints in the UI):

| Shortcut | Action                                                                                 |
| -------- | -------------------------------------------------------------------------------------- |
| `⌥A`     | Mode chip: AI                                                                          |
| `⌥F`     | Mode chip: Filename                                                                    |
| `⌥R`     | Mode chip: Regex                                                                       |
| `⌥S`     | Open Size filter popover                                                               |
| `⌥M`     | Open Modified filter popover                                                           |
| `⌥I`     | Open Search in filter popover                                                          |
| `⌥⏎`     | Show all in main window (was `⌥A` in round 1; rename the wiring and the surfaced hint) |
| `⌥C`     | Inside Search in popover only: Use current folder                                      |
| `⌥V`     | Inside Search in popover only: All folders                                             |
| `⌘N`     | Clear dialog state                                                                     |
| `⌘H`     | All searches popover                                                                   |

Notes:

- Mode-chip shortcuts work globally inside the dialog (focus need not be on the chip).
- Filter-chip shortcuts open the popover focused on the chip's first control.
- `⌥C` / `⌥V` only work while the Search in popover is open.

**Size filter columns**: drop the `none` row — it was a typo. Just `any`, `>=`, `<=`, `between` in column 1.

## Process constraints

- **TDD where reasonable.** Write the failing tests first; run the suite to confirm they fail (red); implement; rerun
  (green). David emphasized this because building is expensive and regressions must not be possible.
- **Do NOT commit.** Leave changes uncommitted in the worktree; David reviews and commits.
- **No em-dashes anywhere.** Sentence case. No hardcoded colors. Tokens for radius/font/z-index. Same high bar as the
  rest.
- Update colocated CLAUDE.mds as part of the work.
- Test via MCP (`localhost:19225` for cmdr, `localhost:9223` for tauri). Frontend hot-reloads on save; backend changes
  require a Rust rebuild (avoid where possible; reserve for the LLM prompt changes if any).
- The dev app may be stopped at the start of this round. Restart with `pnpm dev` in the background, poll
  `curl http://localhost:19225/mcp/health` until ready.

## Issue catalog (use as a test checklist)

Each issue must (a) get a failing test that reproduces it, (b) get a fix, (c) end with the test passing. Group tests by
feature in dedicated files (e.g. `search-loading-state.svelte.test.ts`, `search-no-results.svelte.test.ts`) to keep them
readable.

### Dialog

- D1. "Glowing dot" search indicator replaced by the project's normal spinner (`LoadingIcon`) plus "Searching..." label
  underneath.
- D2. When `isSearching` is true post-debounce, the result list area is REPLACED by the spinner + label. (No rows
  visible during the active fetch.)
- D3. Status bar empty when content area shows "Searching..." (no duplication).
- D4. No-results state: content area renders `No files match these criteria:` followed by a bulleted list of the active
  criteria (mode, query, size, modified, search in, etc.). Status bar empty.
- D5. Recent-searches bar gets a `Recent searches:` label prefix.
- D6. The two right-edge footer buttons (`Go to file`, `Show all in main window`) are ALWAYS visible, just disabled when
  their preconditions aren't met. (Round 1 hid them on empty/idle.)
- D7. Wire `Go to file` to actually open the cursor result via the active pane (navigate to parent, focus the file).
  Currently the button is rendered but the handler is a no-op.
- D8. **`⏎` ownership swap**: introduce a `lastDialogEvent` discriminator (or compute a derived `enterAction`) in
  `search-state.svelte.ts`:
  - `enterAction === 'go-to-file'` when `results.length > 0` AND the last event was `results-arrived` or `cursor-moved`.
  - `enterAction === 'run-search'` otherwise (empty results, dialog freshly opened, query/filter just edited).
  - The Search button on the bar reads `Search ⏎` only when `enterAction === 'run-search'`. The Go-to-file footer button
    reads `Go to file ⏎` only when `enterAction === 'go-to-file'`. The other reads without the shortcut hint. Exactly
    one of them shows the hint at any time.
  - `Enter` keypress dispatches accordingly.
- D9. Wire `⌥F` and `⌥D` properly. NOTE: ⌥F is now Filename mode (global). The Search-in scope keyboard shortcuts move
  inside the popover (⌥C / ⌥V). Remove any stale global ⌥F/⌥D scope handlers.
- D10. **Size filter list selector**: replace the dropdowns with a grid:
  - Col 1: `any`, `>=`, `<=`, `between` (radio-like; one selected).
  - Col 2: `0`, `1`, `5`, `10`, `20`, `50`, `100`, `200`, `500`, `Custom...` (selecting Custom keeps a text input).
    Disabled when Col 1 = `any`.
  - Col 3: unit. The label varies: when Col 2 value is exactly `1`, render `byte`; else `bytes`. For kB/KB, read the
    user's `appearance.fileSizeFormat` setting (binary vs SI) and label accordingly. `MB`, `GB`. Disabled when Col 1 =
    `any`.
  - When Col 1 = `between`: append Col 4 + Col 5 mirroring Col 2/Col 3 for the upper bound.
  - Shortcut `⌥S` opens the popover focused on Col 1.
- D11. **Modified filter list selector**: same pattern as Size. Cols: `any`, `after`, `before`, `between`; col 2 a few
  preset dates (`today`, `yesterday`, `this week`, `last week`, `this month`, `last month`, `this year`, `Custom...`);
  for `between`, append two more cols. Shortcut `⌥M`.
- D12. **"Use current folder" smart fallback**: when the active pane's volumeId is `search-results`, walk back through
  the pane's nav history for the most recent non-search-results history entry's path. If found, use that. If none found,
  the "Use current folder" button is disabled with a tooltip explaining the current folder isn't searchable.
- D13. **Mode-chip shortcuts**: `⌥A` AI, `⌥F` Filename, `⌥R` Regex. Wired globally inside the dialog (focus need not be
  on the chip). Surface the shortcut as a small inline hint on each chip (consistent with David's "discoverable
  shortcuts" rule). Do NOT wire a shortcut to the disabled Content chip.

### Result list

- R1. Column headers are MISALIGNED with content cells. The grid template applied to the header row must match the row
  template exactly. After D10/D11 we have new column logic too; verify headers (left-aligned) line up under their
  content.
- R2. **Path pill width measurement is wrong**: there's clearly free space yet `...` collapse fires. The pretext-based
  fitting in `PathPills.svelte` is overly conservative or doesn't react to row width. Debug with a real width via
  `ResizeObserver`, ensure full path is shown when it fits, and the collapse triggers only when it actually doesn't fit.
  Add tests with mocked widths.
- R3. The `Show all in main window` button currently has `⌥A` text but the shortcut is `⌥⏎` per round 2. Update the
  text. Wire `⌥⏎` to actually trigger the button.

### Results pane in main window (the `search-results://...` snapshot pane)

- P1. PgUp / PgDown don't move the cursor in the snapshot pane. Wire them.
- P2. Home / End don't move the cursor. Wire them.
- P3. Left / Right don't work (probably no-op semantics make sense in a flat snapshot — Left could go-to-parent of
  cursor file? Or no-op? Pick the simplest: no-op for now, but suppress any propagation that does the wrong thing).
  Match `FullList` defaults if possible.
- P4. Space doesn't toggle selection in the snapshot pane. The right-click `Toggle selection (Space)` menu item works —
  so the action exists, just the key isn't wired. Wire `Space` (toggle selection at cursor, same as the existing file
  pane).
- P5. ⇧Up / ⇧Down don't extend selection. Wire them.
- P6. `⌘A` selects all but misses the first item. Off-by-one in the snapshot pane's select-all path. Find and fix.
- P7. `F3` (open in viewer?) and `F4` (edit?) don't work. Verify in `FunctionKeyBar` what each F-key dispatches; route
  through the snapshot pane's keyboard handler. Get info (`F9`?) and Quick Look (`Shift+Space`) DO work — use them as
  the reference for what's already wired correctly.
- P8. "Show in Finder", "Open", "Open with" menu items don't work from the snapshot pane. Trace through the context menu
  in `apps/desktop/src-tauri/src/menu/` and `commands/ui.rs::show_file_context_menu`. The `restrictDestinationActions`
  flag from M8c is probably the culprit — it may be suppressing too much.
- P9. `Copy path` menu item doesn't work in the snapshot pane.
- P10. `Copy {filename}` menu item: both the DISPLAY label and the ACTION are wrong:
  - Display: today the row's `name` field IS the full path (we changed this in round 1 to render the full path in the
    Name column). The menu uses the row's `name` for the `Copy {name}` label, so the label reads
    `Copy ~/Foo/Bar/file.txt`. Should read `Copy file.txt`.
  - Action: it copies `~/Foo/Bar/file.txt` (the full path) when the user wants just the basename.
  - Fix: extract the basename for both the menu label AND the clipboard payload. The Rust side that builds the context
    menu (`commands/ui.rs`) receives the full row, so it can compute the basename. Or pass the basename explicitly from
    the frontend.

## TDD playbook (the approach the agent must follow)

For each issue:

1. **Locate** the code path that owns the behavior. State in the test's comment WHY the test should fail.
2. **Write a Vitest test** that asserts the desired behavior. Run it to confirm it fails.
   (`pnpm vitest run path/to/test --reporter=verbose 2>&1 | tail -40`.)
3. **Implement the fix**.
4. **Run the test again**, confirm green. Run the surrounding suite to catch regressions.
5. **Frontend hot-reload should suffice for visual sanity checks**; back-end changes (Rust context menu, IPC) require a
   rebuild — batch those at the end.

For P-series items (snapshot pane), prefer integration-style Vitest tests that mount `FilePane` (or `SearchResultsView`)
with a snapshot and dispatch keydown / context-menu events. Mirror the existing `FilePane.test.ts` patterns; if none
exist for the snapshot case, add them.

## Report format

```
## Issue catalog summary
[D1: red -> green | passing test name]
[D2: ...]
...

## TDD evidence
Briefly: how many tests were written, how many initially failed, all green at end.

## MCP visual verification
Screenshots at /tmp/r2-*.png.

## check.sh
Pass count, failures categorized.

## Self-review
"Solid AND elegant? Proud and confident?" address every D and P item explicitly.

## Uncommitted diff summary
`git diff HEAD --stat` output.
```

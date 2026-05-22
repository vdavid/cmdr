# Search dialog fix-up: David's brief

This file captures David's verbatim request and the resolved clarifications. Used to scope a single-agent fix-up pass on
top of the search redesign that just shipped. Wiped after the work lands.

## David's verbatim prompt

> Overall, very nice! Get an agent to do these fixes without committing:
>
> - Search dialog
>   - The `dialog-footer` should have the darker background like `footer-actions` does so that the entire bottom part of
>     the window is the same color. It looks broken now.
>   - Result list
>     - The "More actions" button (`...`) should always be visible. Currently, it only shows up where the keyboard
>       cursor is, and on mouse hover.
>     - The headers don't align with the content columns; the `...` column has no header and because of that, it's just
>       misaligned. It should probably have the title "Actions"
>     - The content should fill the width available, with the path column having dynamic size
>     - Size and Modified date columns clash. They should never clash, there should be a nice space between them
>     - Path should never break to two lines. There should be an item `...` in the middle of the path instead of the
>       path elements that don't fit (use pretext for very fast text measurement), and when hovering that item with the
>       mouse, a tooltip should come up with those items, in the same design as the rest, and clickable all the same
>     - Mouse hover should NOT just bring up the gray background thing, but move the actual accent-colored cursor. So
>       both the keyboard AND the mouse should move the same cursor. Exactly like we do in the volume switcher. And it
>       should loop between the top<->bottom of the list
>     - If the filename is too long to fit, put the ellipses to its middle, not the end (I think we have a utility
>       function for this that uses pretext)
>   - Responsivity: the dialog has a fixed width. Instead, it should have a max width which is
>     `min({the current width we have, this size looks good}, 80% of the window width)` and it should remain
>     horizontally centered like it does now
>   - Height: similarly; the result list should responsively shrink so that the search window is never higher than 80%
>     of the window
>   - Instead of "Open in Finder" button, say "Go to file" and do not open in Finder (there should be no such feature)
>     but show in the active pane in-app. (Rename the internals of the button/feature too)
>   - Instead of "Open in pane", use the term "Show all in main window ⌘A" (Rename the internals here too)
>   - When having asked the AI about something, the "search mode" switches to "Filename". This is not good. It should
>     stay in AI mode, and display the current search pattern in the row where the rest of the filters are. So the
>     search bar should remain available to modify the AI search.
>   - Up/Down buttons should move the cursor regardless of where focus is within the pane.
>   - Keyboard shortcuts are not discoverable. Wherever there is a shortcut available on the dialog, include it on the
>     UI with tertiary text or something postfixed, like `All searches... ⌘H` on the "All searches" button.
> - Pane
>   - When opened in the pane, there should be no extra "Path" column, just the normal Name column, but with a full path
>     like `~/Library/Dropbox/test.md`, so the full path. And if too long then the mid-path ellipses should kick in like
>     normal, mouse hover hsould give the full path as tooltip. Strandard stuff for the FullPane, just needs to handle
>     the full path string, maybe it does out of the box. In any case, much less customization needed than what you did
>   - The volume selector and path is ugly. The plan was to set the volume selector to "Search results" and the path to
>     a friendly description of what the search is about. Include this friendly name in the LLM output and save it with
>     the serch results, display it when opened

## Resolved clarifications

1. **"Show all in main window" shortcut**: `⌥A` (not `⌘A`, to leave room for future Select-all on results).
2. **AI mode after a run**: bar still holds the original AI prompt; Enter re-translates via LLM. Switching to
   Filename/Regex mode loads the AI-produced pattern into the appropriate input ONLY if its kind matches (AI produced
   glob -> goes to filename input; AI produced regex -> goes to regex input). The other mode's input is empty OR
   restored from the user's last hand-typed value of that kind (preserve it; don't trash hand-typed history).
3. **"Go to file"**: close the dialog, navigate the active pane to the file's parent folder, focus the file (push new
   history). Replace whatever was there; no special-casing prior state.
4. **Snapshot label from LLM**: AI mode only. Filename / regex modes keep current labels (`*.pdf`, `/pattern/`).
5. **Pattern chip placement**: always rendered in the filter chip row alongside Size / Modified / Search in. Reads
   `Pattern: <value> x` (where value is the glob, the regex, or the AI-produced pattern). Clicking x clears the pattern
   (does NOT clear the AI strip).
6. **AI transparency strip on mode switch**: stays visible until the user starts a new search OR presses ⌘N. Mode
   switching does not hide it.
7. **`showInFinder` IPC**: leave the IPC and other call sites alone; only rip it out of the search dialog footer.

## Process constraints

- New worktree (already created: `worktree-search-fixup`).
- Single agent does the whole fix-up pass.
- TDD where reasonable. Update colocated CLAUDE.mds as part of the work.
- Test via MCP (the cmdr-dev and tauri MCPs at `localhost:19225` and `localhost:9223`). The dev app may need to be
  restarted with `pnpm dev` in the background.
- **Do NOT commit.** Leave all changes uncommitted in the worktree; David will review and commit (or amend in batches)
  himself.
- Adhere to the same high bar as the rest of the redesign (no em-dashes anywhere; no hard-coded colors; tokens for
  radius/font/z-index; sentence case; no premature abstractions; no half-finished implementations).

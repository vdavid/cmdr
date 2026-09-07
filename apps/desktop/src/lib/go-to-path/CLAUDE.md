# Go to path (frontend)

The ⌘G modal that jumps the focused pane to a typed, pasted, or recent path. A thin presenter: the backend's
`resolve_go_to_path` (`apps/desktop/src-tauri/src/go_to_path/CLAUDE.md`) owns all path reasoning.

## Module map

- `go-to-path.ts`: the `goToPath(explorer, input)` handler plus the pure `digitToRecentIndex` and
  `shouldPrefillClipboard`. `scheme-intercept.ts`: what a `<scheme>://` input means.
- `GoToPathDialog.svelte` (textbox, recent rows, inline ancestor warning, `RESOLVE_DEBOUNCE_MS`),
  `GoToPathAncestorToastContent.svelte`, `recent-paths-state.svelte.ts` (`$state` mirror of the backend recents store),
  `go-to-path-ids.ts`.
- Navigation primitives live one level up in `../file-explorer/navigation/navigate-and-select.ts`.

## Must-knows

- **A `<scheme>://` input never reaches the backend resolver.** `resolve_go_to_path` walks `std::fs` over a path joined
  onto the pane's directory, so it answers `invalid` for an address that is about to work. All three resolving sites
  (the jump, the debounced preview, the clipboard prefill) call ONE classifier. ❗ Reading and ACTING are separate:
  `readSchemeInput` is safe from the preview; `actOnSchemeInput` opens the sheet and is the jump's alone, because a
  preview that opened a modal would put a sheet on screen mid-keystroke. ❗ An SMB address hands off to the SERVERS HUB,
  the same destination ⌘K's own hand-off uses: an SMB connect is a share MOUNT, so there is no volume to navigate to,
  and a navigation command that leaves the pane where it was has not navigated. DETAILS § The scheme intercept.
- **Switch on the typed `kind`** (`directory` / `file` / `nearestAncestor` / `invalid`), never on `reason` or toast
  copy: that wording is user-facing only.
- **`file` selects, never opens**: navigate to the parent, then move the cursor onto the file.
- **Recents hold the RESOLVED target, written only by manual jumps in this dialog** (not `nav_to_path` MCP calls, not
  app-wide navigation). The backend owns dedup, order, and the cap of 10; the `$state` mirror re-reads the authoritative
  list after each write rather than guessing the new order.
- **The digit→recent jump is guarded by the EMPTY box, not a modifier.** No valid path starts with a digit, so digits
  are ordinary input once anything is typed. Confirmed with David; don't switch it to a modifier.
- **Keep the `if (show && showGoToPathDialog) return` guard in `routes/(main)/+page.svelte`.** The native `Go to path…`
  menu item carries ⌘G as an accelerator AND `command-registry` binds ⌘G, so both fire on macOS.
- **The ancestor toast's back-shortcut is snapshotted at toast creation** and rendered as a literal-mode `ShortcutChip`.
  A `commandId`-mode chip re-renders live, and a later rebind shouldn't rewrite a visible toast.
- **Anything that isn't a scheme input is local**, so a relative input on a non-local pane falls back to
  nearest-ancestor (often `/`).

Architecture, navigation semantics, decisions, and the manual smoke checklist: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.

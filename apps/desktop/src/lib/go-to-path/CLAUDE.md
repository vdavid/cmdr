# Go to path (frontend)

Frontend half of the "Go to path" action (⌘G, command palette): a small modal that jumps the focused pane to a typed,
pasted, or recent path. Local filesystem only for v1.

Backend counterpart: `apps/desktop/src-tauri/src/go_to_path/CLAUDE.md`. The backend's `resolve_go_to_path` owns all path
reasoning; this frontend is a thin presenter (smart backend, thin frontend).

## Module map

- `go-to-path.ts`: `goToPath(explorer, input)` handler (resolve → switch on typed `kind` → navigate → record recents) +
  the pure helpers `digitToRecentIndex` and `shouldPrefillClipboard`.
- `GoToPathDialog.svelte`: the modal (auto-focused textbox, up to 10 recent rows, live inline ancestor warning).
  `RESOLVE_DEBOUNCE_MS` lives here.
- `GoToPathAncestorToastContent.svelte`: INFO toast for the nearest-ancestor outcome.
- `recent-paths-state.svelte.ts`: `$state` mirror of the backend recents store.
- `go-to-path-ids.ts`: stable dedup id for the ancestor INFO toast.

Navigation primitives are shared one level up in `../file-explorer/navigation/navigate-and-select.ts`
(`navigateToDirInPane`, `navigateToFileInPane`), reused by "Go to latest download".

## Must-knows

- **Switch on the typed `kind`, never on a message string.** The backend returns one `GoToPathResolution` with a `kind`
  discriminator (`directory` / `file` / `nearestAncestor` / `invalid`); `reason` and toast copy are user-facing only.
  This is the no-string-matching rule and the smart-backend principle.
- **`file` selects, never opens.** `file` → navigate to the parent and move the cursor onto the file. Don't open it.
- **Recents store the RESOLVED target, populated ONLY by manual jumps in this dialog.** Not by `nav_to_path` MCP calls,
  not by ordinary app-wide navigation (matches the search-history precedent). Deduped by path, move-to-top, cap 10. The
  backend owns dedup/order/cap; the `$state` mirror re-reads the authoritative list after each write rather than
  guessing the new order.
- **The digit→recent jump is guarded by the EMPTY box, not a modifier.** Empty box + `'1'..'9'` → recents 0..8, `'0'` →
  10th. No valid path starts with a digit, so once any character is typed, digits are ordinary input. The guard is
  stated in a code comment at the keydown site; confirmed with David. Don't switch it to a modifier.
- **Menu double-dispatch idempotency.** The native `Go to path…` menu item carries ⌘G as an accelerator AND
  `command-registry` binds ⌘G, so both fire on macOS. The `showGoToPathDialog` callback in `+page.svelte` guards with
  `if (show && showGoToPathDialog) return` so a double-fire opens the dialog once. Don't drop the guard.
- **The ancestor toast's back-shortcut is snapshotted at toast-creation** (`getEffectiveShortcuts('nav.back')[0]`) and
  rendered as a literal-mode `ShortcutChip`, never hardcoded and never live-subscribed: a later rebind shouldn't rewrite
  a visible toast. A `commandId`-mode chip re-renders live, so keep it literal.

## v1 limitations

- **Local filesystem only.** Typed SMB/MTP paths are out of scope; a relative input on a non-local pane falls back to
  nearest-ancestor (often `/`). Absolute and `~` paths always work.
- **Case-insensitivity.** Dedupe is a raw path-string compare, so on case-insensitive APFS `/Users/x/Foo` and
  `/Users/x/foo` can show as two recents. Accepted for v1.

Architecture, navigation semantics, decisions, and the manual smoke checklist: `DETAILS.md`.

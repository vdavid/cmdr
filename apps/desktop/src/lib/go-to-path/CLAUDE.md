# Go to path (frontend)

Frontend half of the "Go to path" action (⌘G, command palette). A small modal that jumps the focused pane to a typed,
pasted, or recent path. Local filesystem only for v1 (absolute and `~` paths always work; relative paths resolve against
the focused pane's current dir).

Backend counterpart: [`src-tauri/src/go_to_path/CLAUDE.md`](../../../src-tauri/src/go_to_path/CLAUDE.md). The backend's
`resolve_go_to_path` owns all path reasoning; this frontend is a thin presenter (smart backend, thin frontend).

## Architecture

| File                                  | Purpose                                                                                                                                                                                            |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `go-to-path.ts`                       | `goToPath(explorer, input)` handler: resolve → switch on the typed `kind` → navigate via the shared primitives → record the resolved target into recents. Plus the pure helpers (see below).       |
| `GoToPathDialog.svelte`               | The modal: auto-focused textbox (clipboard-prefilled when the clipboard resolves to a real path), up to 10 recent rows (digit chip + middle-truncated path + `[x]`), live inline ancestor warning. |
| `GoToPathAncestorToastContent.svelte` | INFO toast for the nearest-ancestor outcome. Props `requested`, `landed`, `backShortcut` (snapshotted). Code-formats the paths and the kbd.                                                        |
| `recent-paths-state.svelte.ts`        | `$state` mirror of the backend recents store. `loadRecentPaths` / `addRecentPath` / `removeRecentPath` write the backend via IPC, then re-read the authoritative list so the UI stays in sync.     |
| `go-to-path-ids.ts`                   | Stable dedup id for the ancestor INFO toast.                                                                                                                                                       |

The navigation primitives live one level up in
[`lib/file-explorer/navigation/navigate-and-select.ts`](../file-explorer/navigation/navigate-and-select.ts) because
they're shared with "Go to latest download" (`lib/downloads/go-to-latest.ts`): `navigateToDirInPane` (navigate, no
cursor move) and `navigateToFileInPane` (navigate to parent, then `moveCursor` onto the file). Both handle
`navigateToPath`'s `string | Promise<void>` return — report-and-bail on the sync-error string, else await the listing.

## Navigation semantics

The backend resolves the typed string into exactly one `GoToPathResolution`; the handler acts per `kind` (always in the
focused pane):

- `directory` → `navigateToDirInPane`. Cursor lands on row 0 (`..`) via normal navigation.
- `file` → `navigateToFileInPane`: navigate to the parent, select the file. We do NOT open it.
- `nearestAncestor` → `navigateToDirInPane(ancestorDir)` + an INFO toast.
- `invalid` → no-op (empty/unresolvable input; the dialog gates this anyway by disabling "Go to path" on an empty box).

On any successful jump (directory/file/ancestor) the RESOLVED target is recorded into recents, never the raw input.

While typing, a debounced (`RESOLVE_DEBOUNCE_MS`, wrapped in `withTimeout`) resolve drives the live inline warning below
the box for the `nearestAncestor` case only. The same `resolve_go_to_path` command serves both the live preview and the
actual jump — one source of truth, no drift.

## Pure helpers (unit-tested)

- `digitToRecentIndex(inputValue, key, recentsCount, modifierHeld?)`: empty box + `'1'..'9'` → 0..8, `'0'` → 9;
  out-of-range / non-empty box / non-digit / modifier → `null`.
- `shouldPrefillClipboard(resolution)`: `directory` / `file` → `true`, else `false`.

## Recents

- Populated **only by manual jumps in this dialog** — not by `nav_to_path` MCP calls, not by ordinary app-wide
  navigation (matches the search-history precedent).
- Stores the **resolved target** (dir / file path / ancestor), deduped by path, move-to-top, cap 10. The backend owns
  all three; the `$state` mirror re-reads the authoritative list after each write rather than guessing the new order.
- Rows are middle-truncated for display (`useShortenMiddle`, `preferBreakAt: '/'`) with the full path in a `title`
  tooltip (`tooltipWhenTruncated`).

## Key decisions

**Decision**: The dialog lives in `routes/(main)/+page.svelte` (a `showGoToPathDialog` boolean + the
`CommandDispatchDialogs.showGoToPathDialog` callback), beside Search and Selection — not in `DialogManager`. **Why**:
`DialogManager` hosts pane-scoped file-operation dialogs; Go-to-path is a window-level modal that reads the focused pane
and acts on it, exactly like Search/Selection. Following that pattern keeps it beside its true siblings.

**Decision**: The backend owns resolution; the frontend switches on the typed `kind` and never on a message string.
**Why**: Smart-backend / thin-frontend (AGENTS.md principle 3) and the no-string-matching rule. The `GoToPathResolution`
discriminator is the contract; the wording (`reason`, the toast copy) is for the user.

**Decision**: The digit→recent jump is guarded by the **empty box**, not by a modifier. **Why**: No valid path starts
with a digit (paths start with `/`, `~`, or `.`), so once any character is in the box, digits are ordinary input. The
empty-box guard is therefore unambiguous and modifier-free. Confirmed with David. The guard is stated in a code comment
at the keydown site.

**Decision**: The ancestor toast's back-shortcut is snapshotted at toast-creation
(`getEffectiveShortcuts('nav.back')[0]`), never hardcoded and never live-subscribed. **Why**: A later rebind shouldn't
rewrite a visible toast — that would no longer match what the user could press. The next toast picks up the new binding.
Matches the downloads toast snapshot rule.

## Menu double-dispatch (idempotency)

A command with a native menu accelerator AND a `command-registry` shortcut fires both paths on macOS. The
`showGoToPathDialog` callback in `+page.svelte` guards with `if (show && showGoToPathDialog) return`, so a double-fire
opens the dialog exactly once. (The native Go-menu item lands in M3; this guard is already in place so it'll be correct
when it does.)

## v1 limitations

- **Local filesystem only.** Typed SMB/MTP paths are out of scope. A relative input on a non-local focused pane resolves
  against a non-local base, so the local-fs existence walk falls back to nearest-ancestor (often `/`). Acceptable
  degraded behavior; absolute and `~` paths always work.
- **Case-insensitivity.** Dedupe is a raw resolved-path string compare, so on case-insensitive APFS `/Users/x/Foo` and
  `/Users/x/foo` can show as two recents. Accepted for v1 (worst case: a duplicate-looking entry).

## Manual smoke checklist

Run through this after any change to the dialog, the handler, or the recents mirror. Each step is independent.

1. Start dev: `pnpm dev` at repo root.
2. Press `⌘G` → the dialog opens with the textbox focused. Press `Esc` → it closes and the pane refocuses.
3. Copy an existing directory path to the clipboard (for example `pbcopy <<< ~/Documents`), press `⌘G` → the box is
   prefilled with that path and selected. Copy a non-existent path, press `⌘G` → the box opens empty.
4. Type an existing directory, press `Enter` → the focused pane navigates into it; the dialog closes.
5. Type an existing file (for example `~/Downloads/foo.txt`), press `Enter` → the pane goes to the parent and the cursor
   lands on the file (not opened).
6. Type a non-existent path (for example `/tmp/nope/a.txt`) → the inline warning shows the closest existing ancestor.
   Press `Enter` → the pane jumps to that ancestor and an INFO toast appears with the `nav.back` shortcut.
7. With the box empty, press `1` → jumps to the most recent path; `0` → jumps to the tenth. With any character typed,
   digits are ordinary input (no jump).
8. Click a recent row → jumps immediately. Click a row's `[x]` → removes the entry without jumping.
9. Rebind `nav.back` in Settings > Keyboard shortcuts, trigger the ancestor toast again → the toast shows the new combo
   (snapshotted per toast). An already-visible toast keeps its old combo.

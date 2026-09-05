# Open terminal here: details

Depth behind `CLAUDE.md`. The spec that decided all of this is `docs/specs/open-terminal-here.md`; the launch table,
the per-app recipes, and why there's no window-vs-tab control are `src-tauri/src/file_system/DETAILS.md`.

## What "here" means

`resolveTerminalFolder` takes the pane's own path, its volume KIND, and the cursor row, and answers one folder or
`null`. In order:

1. A kind with no OS-visible paths (`mtp`, `adb`, `network`, `search-results`) ⇒ `null`. Nothing else is consulted.
2. A pane inside an archive ⇒ the folder holding the archive FILE (`folderContainingArchive`). Nothing inside an
   archive is on disk, so the containing folder is the nearest place a shell can stand.
3. A folder under the cursor ⇒ that folder. It's what the user is pointing at, so it wins over the pane's own.
4. Everything else ⇒ the pane's own folder: a file under the cursor, the `..` row, and an empty listing with no cursor
   at all.

Two edges worth naming:

- **`..` resolves to the pane's own folder, not the parent.** Standing on `..` means "I'm looking at this folder",
  which is the reading `getPathToCopyUnderCursor` already takes. The parent would be a surprise.
- **An archive FILE under the cursor falls to rule 4.** A shell can't `cd` into a zip. The check is
  `pathInsideArchive(cursor.path)`, an extension-only test, so a real directory literally named `foo.zip` also lands on
  the pane's folder — consistent with how the pane treats Enter on that path, and the only wrong answer available is a
  folder one level up.

## Why enablement is a push, not a read

The native menu bar is built once in Rust and lives in the AppKit process-wide menu, so nothing there can ask the
frontend what the focused pane is sitting on. `menu-gate.svelte.ts` runs an `$effect.root` over
`getFocusedPaneVolumeId()` and pushes a boolean through `set_open_terminal_here_enabled`, the same shape as
`routes/(main)/menu-operation-gate.svelte.ts`. It dedupes on the last value sent, so switching tabs inside one volume
costs nothing.

Rust stores the verdict in `MenuState.open_terminal_here_enabled` rather than only applying it, because
`set_menu_context` enables every explorer item on a focus round-trip. The id is skipped in that loop (like
`REOPEN_CLOSED_TAB_ID`) and `apply_open_terminal_here_state` re-applies the stored verdict last. That is also what
restores it after a menu-bar rebuild: a language change throws every item away, and the frontend's
`menu-bar-rebuilt` handler calls `activate_window_menu('main')`, which runs `set_menu_context`.

The pane context menu needs no channel: it's built per right-click, so `show_file_context_menu` carries the answer in
`PaneContextMenuFacts.canOpenTerminalHere`. The search-results snapshot pane and the Search dialog pass `false` — a
result set is not a folder, so "here" has nothing to mean there. Enabling it for a search result would be defensible
(each row has a real containing folder), but that's a different command shape than the pane-scoped one the spec took.

## The first-use picker

`decideFirstUsePick` (pure) is the whole decision:

| Situation | Launches | Persists | Hint | Flag spent |
| --- | --- | --- | --- | --- |
| Hint already spent | the stored choice | no | no | — |
| App list came back empty (query timed out) | the stored choice | no | no | no |
| Terminal.app is the only terminal installed | Terminal | no | no | **no** |
| Exactly one terminal running, user hasn't chosen | that one | **yes** | yes | yes |
| Two or more running, or none, or user already chose | the stored choice | no | yes | yes |

The unspent flag in row three is the one that looks like a bug and isn't: the hint exists to advertise a setting that
only matters once a second terminal exists, so it waits for one.

Adoption is skipped when `storedChoice` isn't Terminal.app: the user has already answered this question in Settings,
and a running app is a weaker signal than a choice.

## The toasts

Both are persistent and share the one-slot `open-terminal-here` toast group, so a second one retires the first rather
than stacking. Both deep-link to the terminal-app row through `openSettingsToTerminalApp()` under the
`'open-terminal-toast'` settings surface.

`launchRefused` and `timedOut` become plain error toasts. `not_a_local_path` coming back from Rust reuses the same
`noPath` wording as the frontend gate's refusal: from the user's side it's the same sentence, and the only difference
is that Rust noticed the mount was gone and the gate hadn't.

## Testing

- `terminal-target.test.ts` and `first-use-pick.test.ts` are pure: no DOM, no IPC.
- `open-terminal-here.test.ts` mocks the IPC, the settings store, and the toast surface, and pins the side effects
  (what gets written, what gets said) rather than the launch itself.
- `test/e2e-playwright/open-terminal-here.spec.ts` pins the folder resolution end to end. The `playwright-e2e` build
  records the folder into `crate::open_mock` instead of launching, so a suite run leaves no terminal windows behind.
  It spends the hint flag in `beforeEach`: whether the hint would fire depends on which terminals happen to be
  installed on the machine running the suite.

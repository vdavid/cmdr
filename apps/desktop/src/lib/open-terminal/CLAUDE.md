# Open terminal here (frontend)

The frontend half of `file.openTerminalHere`: which folder "here" means, whether the command is offered at all, and the
two toasts around the launch. Backend counterpart: `src-tauri/src/file_system/terminal.rs`.

## Module map

- **`terminal-target.ts`**: pure. `canOpenTerminalIn(kind)` (the enablement rule) and `resolveTerminalFolder(pane)` (the
  cursor rules). Both surfaces and the handler read these, so "here" has one definition.
- **`first-use-pick.ts`**: pure. `decideFirstUsePick` returns what the first run launches, persists, and says.
- **`open-terminal-here.ts`**: `openTerminalHereForFolder({ folder, volumeId })`, the one entry point. Never throws.
- **`terminal-app-setting.ts`**: the two settings plus the Settings deep-link. **`menu-gate.svelte.ts`**: the native
  File-menu enabled push. The two `*ToastContent.svelte` bodies are the hint and the app-is-gone toast.

## Must-knows

- **Gate on the pane's VOLUME kind, ❌ never on the path string.** `capabilitiesFor(volumeId).kind`, ❌ never
  `capabilitiesForPane`: an archive pane's kind-from-path would hide the drive the archive lives on, and that drive is
  what decides whether the containing folder is reachable. Rust re-reads `paths_are_os_visible()` at launch time and
  answers `not_a_local_path`, which is what catches a share whose mount went away after the gate said yes.
- **Four surfaces, and only two of them can grey out.** The File menu item and the pane context menu carry an enabled
  flag; the palette has no disabled state and an accelerator fires whatever the menu looks like. So the handler is the
  real refusal and it words the hint (`commands.handler.openTerminalHere.noPath`); the greying is chrome.
- **The hint flag is spent only when the hint actually shows.** Terminal.app alone on the Mac ⇒ open it, say nothing,
  and ❗ leave `behavior.openTerminalHereToastSeen` FALSE: someone who installs Ghostty next month is still owed the
  hint, and a flag spent today would eat it. `first-use-pick.ts` owns this.
- **`listTerminalApps` runs only while the hint is unspent.** After that the stored choice is the whole answer, so the
  ordinary path costs one IPC, not two.
- **The cursor row is RE-READ before acting on it, ❌ never the displayed one.** `getCursorRowForTerminal()` awaits
  `refreshCursorEntry()`, because the displayed cursor entry is one IPC behind a move: arrow-down then ⌥⌘T would
  otherwise open the folder the cursor just left. `file-explorer/pane/DETAILS.md`.
- **The missing-app name is asked for BEFORE the setting is reset**, while the setting still names the app that's gone.
  `terminal_app_display_name` is a table lookup, which is all that's left once the bundle is uninstalled; `null` means
  Cmdr has no name and the toast uses its nameless wording. ❌ Never show the bundle id.

The cursor rules, the surface wiring, and why the enablement push exists at all: `DETAILS.md`.

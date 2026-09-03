# Open terminal here

**The problem**: A user asked for a shortcut or menu item that opens a terminal at the active pane's folder. Cmdr has
"Show in Finder" and "Open in editor" but nothing that hands a folder to a shell, and a keyboard-first file manager
without it sends every developer back to Finder's right-click menu. One user asked; every Total Commander, Marta, and
ForkLift user expects it.

**What macOS gives us**: nothing. There is no system-wide "default terminal" setting. Finder's built-in "New Terminal at
Folder" service is hardcoded to Terminal.app; iTerm2 works around that by installing its own service. Every app that
offers this (VS Code, Marta, Raycast, the "New Terminal Here" App Store app) ships its own list of known terminals plus
a setting. That's the normal shape, and we do the same. (Per-app launch behavior verified 2026-09-03 against the vendor
docs and discussions linked in the M1 recipe table.)

❗ **Don't build window-vs-tab control in v1.** There is no universal mechanism: Terminal.app follows the system "Prefer
tabs when opening documents" setting, Warp has a URI scheme with both actions, Ghostty opens a tab when handed a folder
and a window only via `--args`, iTerm2 needs AppleScript (an Automation permission prompt) for a tab, kitty and WezTerm
need their own CLI or remote control, and Alacritty has no tabs at all. Launch each app the way it natively takes a
folder and let the terminal's own preferences decide. That's what the user already configured in their terminal. A
"window / tab" toggle shown only for apps that honor it (Warp, Ghostty, WezTerm) is a later milestone if anyone asks.

## What already exists

- **App discovery and launching**: `apps/desktop/src-tauri/src/file_system/open_with.rs` wraps `NSWorkspace`:
  `read_app_display_name`, `read_bundle_identifier`, `load_app_icon`, `open_paths_with`, `pick_app_via_open_panel` (an
  `NSOpenPanel` filtered to `.app` bundles), and `start_invalidation_observer`, which fires on app launch and quit.
  Asking "is bundle ID X installed?" is a millisecond `NSWorkspace` call, so ❌ no `/Applications` scan and ❌ no
  "Refresh" button: query the known list whenever the settings row renders and again at action time.
- **The sibling actions**: `apps/desktop/src-tauri/src/commands/file_actions.rs` holds `show_in_finder`,
  `open_in_editor`, and `open_path`, each with a `playwright-e2e` variant that records into `open_mock` instead of
  launching. The new command follows that pattern exactly.
- **Command plumbing**: `file.showInFinder` is the model. Its command id lives in `shortcuts-store.ts`, its menu id and
  scope in `apps/desktop/src-tauri/src/menu/command_map.rs`, and its menu item in `menu_structure.rs`. The command
  palette and the pane context menu pick commands up from the same registry.
- **Settings**: definitions are data in `apps/desktop/src/lib/settings/definitions/behavior.ts` under
  `['Behavior', 'Navigation & file ops']`, rendered by `NavigationAndFileOpsSection.svelte` with `SettingRow`.

## Design decisions, taken

1. **Which folder**: the active pane's current folder. Not the folder under the cursor. This matches Finder's service
   and the request as worded.
2. **Where it's enabled**: only when the pane sits on a plain POSIX path. Inside an archive, on MTP, on ADB, and on a
   direct-SMB pane there is no path a shell can `cd` into, so the menu item, palette entry, and shortcut are disabled
   with a hint ("This folder isn't on a local disk"). A Finder-mounted SMB volume under `/Volumes` works fine. Gate on
   the pane's volume kind / `LocationInfo` capabilities, ❌ never on the path string.
3. **Default**: Terminal.app. It's always present, so a user with no other terminal never sees a setting.
4. **First-use picker**: the first time the action runs, if at least one other known terminal is installed, show a small
   dialog: "Which terminal should Cmdr open?" listing the installed ones with icons, pre-selecting the one that is
   currently running if exactly one is (via `NSWorkspace.runningApplications`), with a "Change it anytime in Settings"
   note. The choice persists; the dialog never returns. A Warp user is done in one click and never opens Settings. If
   only Terminal is installed, skip the dialog and open Terminal.
5. **Settings row, not a section**: one dropdown in Navigation & file ops: "Open terminal here uses: [Terminal ▾]". It
   lists only the known terminals that are installed, plus "Choose an app…", which reuses `pick_app_via_open_panel` and
   launches via `open -a <app> <dir>`. That works for any terminal that registers as a folder handler, which nearly all
   do. A dedicated Terminal section only earns its place when a second terminal feature exists.
6. **No "path to a binary" custom option in v1**: a bare binary path doesn't say how to pass the directory. If someone
   asks, the right shape is a command template with a `{path}` placeholder, split into argv without a shell. Deferred.
7. **Graceful failure**: if the chosen app was uninstalled, open Terminal instead and toast "Warp isn't installed
   anymore, so this opened in Terminal", with a link to the settings row. Reset the setting to Terminal so the toast
   doesn't repeat.
8. **Shortcut**: Cmd+Shift+T is taken. Pick a free default (Cmd+Alt+T is the candidate) and check it in
   `conflict-detector.ts`; the shortcut is rebindable like every other command.
9. **Surfaces**: File menu next to Show in Finder, command palette, pane context menu, and the shortcut.

## Milestones

### M1. The launch module

`apps/desktop/src-tauri/src/file_system/terminal.rs` (or a sibling under `open_with.rs`'s home) with:

- A **known-terminals table**: bundle id, display name, and a launch recipe. Start with Terminal (`com.apple.Terminal`),
  iTerm2 (`com.googlecode.iterm2`), Warp (`dev.warp.Warp-Stable`), Ghostty (`com.mitchellh.ghostty`), kitty
  (`net.kovidgoyal.kitty`), Alacritty (`org.alacritty`), WezTerm (`com.github.wez.wezterm`), and Hyper
  (`co.zeit.hyper`). ❗ Verify every bundle id against the installed app or the vendor's repo before shipping; ❌ don't
  trust the list above blindly.
- **Recipes**: `open -a <bundle> <dir>` is the default and covers Terminal, iTerm2, Ghostty, kitty, WezTerm, and Hyper.
  Warp gets `open warp://action/new_window?path=<percent-encoded dir>` (its documented URI scheme:
  `https://docs.warp.dev/terminal/more-features/uri-scheme/`). Alacritty gets
  `open -na Alacritty --args --working-directory <dir>` because it doesn't accept a folder as a document. Ghostty's
  folder-open lands in a tab of the running instance (`https://github.com/ghostty-org/ghostty/discussions/5910`); that's
  fine per the window-vs-tab decision above.
- **`list_terminal_apps`** IPC: the known table filtered to installed apps, each with name, bundle id, icon, and an
  `is_running` flag. Plus the currently chosen one, resolved.
- **`open_terminal_here(path)`** IPC: resolves the setting, runs the recipe, returns a typed outcome
  (`opened | app_missing_opened_terminal_instead | not_a_local_path`). ❌ Never a message to parse. `playwright-e2e`
  variant records into `open_mock` like `open_in_editor`.
- **Tests**: the recipe builder is pure (bundle id + path → argv), so unit-test it per app, including a path with
  spaces, a quote, and a non-ASCII character. Percent-encoding for Warp is the one place that bites.

### M2. The setting

- `behavior.openTerminalHereApp` in `behavior.ts`: a string holding a bundle id, or a custom `.app` path for the "Choose
  an app…" case; default `com.apple.Terminal`. Plus a hidden `behavior.openTerminalHerePickerSeen` boolean for the
  first-use dialog, the way `doubleClickOnPaneNotificationSeen` does it.
- The dropdown row in `NavigationAndFileOpsSection.svelte`, fed by `list_terminal_apps` each time the section renders.
  Icons in the options. "Choose an app…" at the bottom.
- i18n keys for label, description, options, and the toast, in every locale the repo carries (the parity test will tell
  you).

### M3. The command and its surfaces

- `file.openTerminalHere` in `shortcuts-store.ts`, `command_map.rs` (pane-scoped, not file-scoped), `menu_structure.rs`
  next to Show in Finder, the palette, and the pane context menu.
- Enablement wired to the pane's volume kind. Disabled state carries the hint.
- The first-use picker dialog, built with the house `ModalDialog` primitive (`docs/guides/building-ui.md`).
- The uninstalled-app fallback toast.

### M4. Docs and checks

- `file_system/CLAUDE.md` and `DETAILS.md`: the known-terminals table's home and the "no scan, query `NSWorkspace`" why.
- `settings/sections/DETAILS.md`: the row and the first-use picker.
- `menu/DETAILS.md`: the new command id.
- Move the vendor links above into `docs/notes/README.md` and point to them from `file_system/DETAILS.md`.
- `pnpm check` per milestone; `--include-slow` once before merge.

## Out of scope, deliberately

- Window-vs-tab control (see the top). Revisit only on request.
- A command-template custom option. Revisit only on request.
- Linux: no default terminal there either (`x-terminal-emulator` on Debian, the emerging `xdg-terminal-exec`). The same
  design carries over with a different table when Linux ships.
- An integrated terminal pane. Different feature, different spec.

## Size

About one agent-day. One Rust module with a table and two commands, one setting, one settings row, one small dialog,
menu and palette wiring, and pure unit tests for the recipe builder. Clear win, no tradeoff, as long as v1 stays out of
the tab-vs-window business.

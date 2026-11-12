# Keymap survey: Windows and Linux, and Total Commander parity

Research for task 26 of `docs/specs/windows-port-plan.md`. Read-only survey at commit `82a84788f6`, 2026-11-11.
Machine-readable proposal: `docs/notes/windows-port/keymap-proposal.json`.

## Status

Superseded as a key table: `docs/notes/windows-port/keymap-decisions.md` is the table that shipped, and the registry
follows it. This survey describes the code at `82a84788f6`, before the keymap work: the `toPlatformShortcut` converter,
the ⌃→Shift hack, the one non-mac menu bar, and the `metaKey` reads it names are gone (`lib/commands/DETAILS.md` §
"Default keys per platform" has today's shape). It stays for its reserved-key research and its Total Commander mapping.
Corrections from David's #1102 comment (2026-11-12), which win over the text below:

- GNOME stopped taking `Alt+F1` in GNOME 41; KDE, Xfce, and Cinnamon still take it. `Alt+F2` is taken on all five
  desktops.
- Xfce takes `Ctrl+F1`–`F12` and most `Alt+F` keys. Cinnamon takes `Ctrl+Alt+L`; KDE dropped it in 2022.
- AltGr: Chromium may clear `ctrlKey` / `altKey` and set `AltGraph` when the combo types a character, which makes the
  binding unreachable instead of misfiring. Untested in WebView2; either way the binding is broken.
- WebView2 also takes `Shift+F5`, `Ctrl+F5`, `Ctrl+J`, `Ctrl+G`, `Ctrl+Shift+G`, `Alt+←/→`, and `F7` (until #1115 turns
  its browser keys off). WebKitGTK reserves nothing.
- TC 11.57 has six keys the table lacks (`Alt+Shift+F7`, `Alt+Shift+F12`, `Alt+Shift+↓`, `Shift+F7`, `Ctrl+Shift+A`
  open-tabs list, `Ctrl+Shift+Q` separate preview window); none maps to a Cmdr command. `Ctrl+←/→` is a match.
- Explorer's `Ctrl+D` deletes; TC's favorites meaning wins.
- `keymap-proposal.json`'s status slips are fixed there: `pane.moveDividerLeft/Right` are `changed-convention` on
  Windows (the key changes), and `favorites.open` is `same`.

Legend: (a) fine, (b) internal clash, (c) OS-, desktop-, or webview-reserved, (d) convention mismatch or off-mac bug.
Every OS/desktop/webview claim says verified (with source) or unverified.

## How the non-mac keymap is made today

- Registry defaults are macOS strings in `apps/desktop/src/lib/commands/sources/*.ts`. Off macOS, `getDefaultShortcuts`
  / `getEffectiveShortcuts` (`apps/desktop/src/lib/shortcuts/shortcuts-store.ts:449-469`) map each through
  `toPlatformShortcut` (`apps/desktop/src/lib/shortcuts/key-capture.ts:378`): ⌘ and ⌃ both become Ctrl, ⌥ Alt, ⇧ Shift,
  and when ⌘ and ⌃ appear together ⌃ becomes Shift (the hack). Windows takes exactly the Linux path (no Windows branch
  anywhere).
- Five hand-written per-platform overrides exist: `pane.focusControls`, `app.focusNotifications`,
  `pane.moveDividerLeft/Right` (`main-window.ts:322,334,393,401`) via `isMacOS()`, and three capability gates
  (`file.openTerminalHere`, `file.getInfo`, `file.quickLook`, `file-list.ts:393,460,471`), which are all `false` on
  Windows and Linux (`apps/desktop/src-tauri/src/platform/windows.rs:10-12`, `linux/mod.rs:12-14`), so those commands
  have no key there.
- Native menu: one non-mac bar for Linux AND Windows (`crates/cmdr-menu/src/spec.rs:45`, `Platform::current()` is
  `Linux` for every non-mac target). Its accelerators (snapshot `crates/cmdr-menu/src/bar_test.rs` `LINUX_MENU_BAR`)
  leave out F-keys, Tab, Space, and zoom because GTK intercepts them; the Ctrl chords are registered natively, including
  `favorites_open [Ctrl+D]`. GTK `&` mnemonics are allocated per language on both non-mac platforms.
- The only global (system-wide) shortcut is the opt-in "go to latest download", default `⌃⌥⌘J`
  (`apps/desktop/src/lib/settings/definitions/behavior.ts:492`), which becomes `Control+Alt+Super+J` off macOS;
  Super+anything is OS territory on Windows and GNOME.
- Tauri/WebView: `zoom_hotkeys_enabled` defaults to false (verified, `tauri-utils-2.10.2/src/config.rs:2450`). WebView2
  browser accelerator keys stay ON: wry defaults `browser_accelerator_keys: true` (verified,
  `wry-0.57.0/src/lib.rs:1768`), and Tauri 2.12.5 exposes no setter for it (verified: no match for
  `browser_accelerator_keys` in `tauri-2.12.5/src`). The app suppresses browser defaults only for `⌘A` and `⌥⌘I`
  (`routes/(main)/global-keydown.ts:71-75`), both tested with `metaKey`, so off macOS nothing is suppressed beyond what
  a claimed command `preventDefault`s. Whether `preventDefault` stops a WebView2 browser accelerator is unverified.

## 1. Complete default table (228 commands, 162 with a default on at least one platform)

Mapped by re-implementing `toPlatformShortcut` over the parsed registry (script outputs in the JSON). Fixed = hardcoded
key (not rebindable); Native = AppKit-owned.

- **`app.quit` (Native)**
  - Name: Quit Cmdr
  - Scope: App
  - macOS: `⌘Q`
  - Today on Linux and Windows: `Ctrl+Q`
  - Windows: (d) No Quit off macOS; Windows expects Alt+F4 (OS does it), GNOME expects Ctrl+Q.
  - Linux (GNOME, KDE): (d) No Quit off macOS; Windows expects Alt+F4 (OS does it), GNOME expects Ctrl+Q.
- **`app.hide` (Native)**
  - Name: Hide Cmdr
  - Scope: App
  - macOS: `⌘H`
  - Today on Linux and Windows: `Ctrl+H`
  - Windows: (d) Mac-only concept; the mapped Ctrl+H does nothing.
  - Linux (GNOME, KDE): (d) Mac-only concept; the mapped Ctrl+H does nothing.
- **`app.hideOthers` (Native)**
  - Name: Hide others
  - Scope: App
  - macOS: `⌥⌘H`
  - Today on Linux and Windows: `Alt+Ctrl+H`
  - Windows: (d) Mac-only; maps to non-canonical "Alt+Ctrl+H".
  - Linux (GNOME, KDE): (d) Mac-only; maps to non-canonical "Alt+Ctrl+H".
- **`app.commandPalette`**
  - Name: Open command palette
  - Scope: App
  - macOS: `⌘⇧P`
  - Today on Linux and Windows: `Ctrl+Shift+P`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`app.settings`**
  - Name: Open settings
  - Scope: App
  - macOS: `⌘,`
  - Today on Linux and Windows: `Ctrl+,`
  - Windows: (d) Ctrl+, is not a Windows/GNOME convention (harmless).
  - Linux (GNOME, KDE): (d) Ctrl+, is not a Windows/GNOME convention (harmless).
- **`queue.show`**
  - Name: Operation queue
  - Scope: App
  - macOS: `⌘⌥Q`
  - Today on Linux and Windows: `Ctrl+Alt+Q`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`log.operationLog`**
  - Name: Operation log
  - Scope: App
  - macOS: `⌘⌥L`
  - Today on Linux and Windows: `Ctrl+Alt+L`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Ctrl+Alt+L: kde Lock screen (unverified)
- **`askCmdr.toggle`**
  - Name: Ask Cmdr
  - Scope: App
  - macOS: `⌘⌥A`
  - Today on Linux and Windows: `Ctrl+Alt+A`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`search.open`**
  - Name: Search files
  - Scope: Main window
  - macOS: `⌘F`, `⌥F7`
  - Today on Linux and Windows: `Ctrl+F`, `Alt+F7`
  - Windows: (c) Ctrl+F: windows WebView2 find on page (verified)
  - Linux (GNOME, KDE): (c) Alt+F7: gnome Move window (verified)
- **`nav.goToPath`**
  - Name: Go to folder…
  - Scope: Main window
  - macOS: `⌘G`, `⌘⇧G`
  - Today on Linux and Windows: `Ctrl+G`, `Ctrl+Shift+G`
  - Windows: (c) Ctrl+G: windows Find next / previous (unverified); (c) Ctrl+Shift+G: windows Find next / previous
    (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`favorites.open`**
  - Name: Show favorites
  - Scope: Main window
  - macOS: `⌃D`
  - Today on Linux and Windows: `Ctrl+D`
  - Windows: (b) clash on Ctrl+D
  - Linux (GNOME, KDE): (b) clash on Ctrl+D
- **`downloads.goToLatest`**
  - Name: Go to latest download
  - Scope: Main window
  - macOS: `⌘J`
  - Today on Linux and Windows: `Ctrl+J`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`view.showHidden`**
  - Name: Toggle hidden files
  - Scope: Main window
  - macOS: `⌘⇧.`
  - Today on Linux and Windows: `Ctrl+Shift+.`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`view.calculateFolderSizes`**
  - Name: Calculate folder sizes
  - Scope: Main window
  - macOS: `⌥⇧Enter`
  - Today on Linux and Windows: `Alt+Shift+Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`view.briefMode`**
  - Name: Switch to Brief view
  - Scope: Main window
  - macOS: `⌘2`
  - Today on Linux and Windows: `Ctrl+2`
  - Windows: (d) TC users expect Ctrl+F1.
  - Linux (GNOME, KDE): (d) TC users expect Ctrl+F1.
- **`view.fullMode`**
  - Name: Switch to Full view
  - Scope: Main window
  - macOS: `⌘1`
  - Today on Linux and Windows: `Ctrl+1`
  - Windows: (d) TC users expect Ctrl+F2.
  - Linux (GNOME, KDE): (d) TC users expect Ctrl+F2.
- **`view.zoom.set100`**
  - Name: Zoom to 100%
  - Scope: Main window
  - macOS: `⌘0`
  - Today on Linux and Windows: `Ctrl+0`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`view.zoom.in`**
  - Name: Zoom in
  - Scope: Main window
  - macOS: `⌘+`, `⌘=`
  - Today on Linux and Windows: `Ctrl++`, `Ctrl+=`
  - Windows: (c) Ctrl++: windows WebView2 zoom (verified)
  - Linux (GNOME, KDE): (a) fine
- **`view.zoom.out`**
  - Name: Zoom out
  - Scope: Main window
  - macOS: `⌘-`
  - Today on Linux and Windows: `Ctrl+-`
  - Windows: (c) Ctrl+-: windows WebView2 zoom (verified)
  - Linux (GNOME, KDE): (a) fine
- **`sort.byName`**
  - Name: Sort by name
  - Scope: Main window
  - macOS: `⌘3`, `⌘F3`
  - Today on Linux and Windows: `Ctrl+3`, `Ctrl+F3`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Ctrl+F3: kde Switch to desktop 1–4 (verified)
- **`sort.byExtension`**
  - Name: Sort by extension
  - Scope: Main window
  - macOS: `⌘4`, `⌘F4`
  - Today on Linux and Windows: `Ctrl+4`, `Ctrl+F4`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Ctrl+F4: kde Switch to desktop 1–4 (verified)
- **`sort.byModified`**
  - Name: Sort by date modified
  - Scope: Main window
  - macOS: `⌘5`, `⌘F5`
  - Today on Linux and Windows: `Ctrl+5`, `Ctrl+F5`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`sort.bySize`**
  - Name: Sort by size
  - Scope: Main window
  - macOS: `⌘6`, `⌘F6`
  - Today on Linux and Windows: `Ctrl+6`, `Ctrl+F6`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`pane.switch`**
  - Name: Switch pane
  - Scope: Main window
  - macOS: `Tab`
  - Today on Linux and Windows: `Tab`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`pane.swap`**
  - Name: Swap panes
  - Scope: Main window
  - macOS: `⌘U`
  - Today on Linux and Windows: `Ctrl+U`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`pane.clone`**
  - Name: Clone pane
  - Scope: Main window
  - macOS: `⌘⇧C`
  - Today on Linux and Windows: `Ctrl+Shift+C`
  - Windows: (c) Ctrl+Shift+C: windows WebView2 DevTools (verified)
  - Linux (GNOME, KDE): (a) fine
- **`pane.focusControls`**
  - Name: Move focus to the pane controls
  - Scope: Main window
  - macOS: `⌃F5`
  - Today on Linux and Windows: `Ctrl+Shift+F5`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`app.focusNotifications`**
  - Name: Move focus to the notifications
  - Scope: Main window
  - macOS: `⌃F6`
  - Today on Linux and Windows: `Ctrl+Shift+F6`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`pane.leftVolumeChooser`**
  - Name: Open left volume switcher
  - Scope: Main window
  - macOS: `⌥F1`
  - Today on Linux and Windows: `Alt+F1`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Alt+F1: kde Application launcher (unverified)
- **`pane.rightVolumeChooser`**
  - Name: Open right volume switcher
  - Scope: Main window
  - macOS: `⌥F2`
  - Today on Linux and Windows: `Alt+F2`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Alt+F2: gnome Run command prompt (verified); (c) Alt+F2: kde KRunner (unverified)
- **`pane.copyPathLeftToRight`**
  - Name: Copy path from left to right pane
  - Scope: Main window
  - macOS: `⌘→`
  - Today on Linux and Windows: `Ctrl+→`
  - Windows: (d) Bug: text-input exemption only knows "⌘→" (global-keydown.ts:88), so Ctrl+→ word-jump in a text field
    runs this.
  - Linux (GNOME, KDE): (d) Bug: text-input exemption only knows "⌘→" (global-keydown.ts:88), so Ctrl+→ word-jump in a
    text field runs this.
- **`pane.copyPathRightToLeft`**
  - Name: Copy path from right to left pane
  - Scope: Main window
  - macOS: `⌘←`
  - Today on Linux and Windows: `Ctrl+←`
  - Windows: (d) Same bug with Ctrl+←.
  - Linux (GNOME, KDE): (d) Same bug with Ctrl+←.
- **`pane.refresh`**
  - Name: Refresh pane
  - Scope: Main window
  - macOS: `⌘R`
  - Today on Linux and Windows: `Ctrl+R`
  - Windows: (c) Ctrl+R: windows WebView2 reload (verified)
  - Linux (GNOME, KDE): (a) fine
- **`pane.moveDividerLeft`**
  - Name: Move pane divider left
  - Scope: Main window
  - macOS: `⌃⌥←`
  - Today on Linux and Windows: `Ctrl+Alt+Shift+←`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Ctrl+Alt+Shift+←: gnome Move window to workspace (verified)
- **`pane.moveDividerRight`**
  - Name: Move pane divider right
  - Scope: Main window
  - macOS: `⌃⌥→`
  - Today on Linux and Windows: `Ctrl+Alt+Shift+→`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Ctrl+Alt+Shift+→: gnome Move window to workspace (verified)
- **`tab.new`**
  - Name: New tab
  - Scope: Main window
  - macOS: `⌘T`
  - Today on Linux and Windows: `Ctrl+T`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`tab.close`**
  - Name: Close tab
  - Scope: Main window
  - macOS: `⌘W`
  - Today on Linux and Windows: `Ctrl+W`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`tab.reopen`**
  - Name: Reopen closed tab
  - Scope: Main window
  - macOS: `⌘⇧T`
  - Today on Linux and Windows: `Ctrl+Shift+T`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`tab.next`**
  - Name: Next tab
  - Scope: Main window
  - macOS: `⌃Tab`, `⌘⌥→`
  - Today on Linux and Windows: `Ctrl+Tab`, `Ctrl+Alt+→`
  - Windows: (c) Ctrl+Alt+→: windows Screen rotation on some Intel graphics drivers (unverified)
  - Linux (GNOME, KDE): (c) Ctrl+Alt+→: gnome Switch workspace (verified)
- **`tab.prev`**
  - Name: Previous tab
  - Scope: Main window
  - macOS: `⌃⇧Tab`, `⌘⌥←`
  - Today on Linux and Windows: `Ctrl+Shift+Tab`, `Ctrl+Alt+←`
  - Windows: (c) Ctrl+Alt+←: windows Screen rotation on some Intel graphics drivers (unverified)
  - Linux (GNOME, KDE): (c) Ctrl+Alt+←: gnome Switch workspace (verified)
- **`tab.moveLeft`**
  - Name: Move tab left
  - Scope: Main window
  - macOS: `⌘⌃⇧←`
  - Today on Linux and Windows: `Ctrl+Shift+←`
  - Windows: (d) ⌃→Shift hack output Ctrl+Shift+← is "select word left" in text fields.
  - Linux (GNOME, KDE): (d) ⌃→Shift hack output Ctrl+Shift+← is "select word left" in text fields.
- **`tab.moveRight`**
  - Name: Move tab right
  - Scope: Main window
  - macOS: `⌘⌃⇧→`
  - Today on Linux and Windows: `Ctrl+Shift+→`
  - Windows: (d) Ctrl+Shift+→ is "select word right".
  - Linux (GNOME, KDE): (d) Ctrl+Shift+→ is "select word right".
- **`askCmdr.newChat`**
  - Name: New Ask Cmdr chat
  - Scope: Main window
  - macOS: `⌘N`
  - Today on Linux and Windows: `Ctrl+N`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`askCmdr.find` (Fixed)**
  - Name: Find in chat
  - Scope: Main window/Ask Cmdr
  - macOS: `⌘F`
  - Today on Linux and Windows: `Ctrl+F`
  - Windows: (c) Ctrl+F: windows WebView2 find on page (verified)
  - Linux (GNOME, KDE): (a) fine
- **`askCmdr.findNext` (Fixed)**
  - Name: Next match in chat
  - Scope: Main window/Ask Cmdr
  - macOS: `⌘G`, `Enter`
  - Today on Linux and Windows: `Ctrl+G`, `Enter`
  - Windows: (c) Ctrl+G: windows Find next / previous (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`askCmdr.findPrevious` (Fixed)**
  - Name: Previous match in chat
  - Scope: Main window/Ask Cmdr
  - macOS: `⌘⇧G`, `⇧Enter`
  - Today on Linux and Windows: `Ctrl+Shift+G`, `Shift+Enter`
  - Windows: (c) Ctrl+Shift+G: windows Find next / previous (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`nav.up` (Fixed)**
  - Name: Move cursor to previous item
  - Scope: Main window/File list
  - macOS: `↑`
  - Today on Linux and Windows: `↑`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.down` (Fixed)**
  - Name: Move cursor to next item
  - Scope: Main window/File list
  - macOS: `↓`
  - Today on Linux and Windows: `↓`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.open`**
  - Name: Open file or folder
  - Scope: Main window/File list
  - macOS: `Enter`, `⌘↓`
  - Today on Linux and Windows: `Enter`, `Ctrl+↓`
  - Windows: (d) TC also uses Ctrl+PageDown; Ctrl+↓ is mechanical.
  - Linux (GNOME, KDE): (d) TC also uses Ctrl+PageDown; Ctrl+↓ is mechanical.
- **`nav.parent`**
  - Name: Go to parent folder
  - Scope: Main window/File list
  - macOS: `Backspace`, `⌘↑`
  - Today on Linux and Windows: `Backspace`, `Ctrl+↑`
  - Windows: (d) Windows/GTK users expect Alt+↑ (Explorer, Nautilus, Dolphin); Ctrl+↑ is TC's "open in new tab".
  - Linux (GNOME, KDE): (d) Windows/GTK users expect Alt+↑ (Explorer, Nautilus, Dolphin); Ctrl+↑ is TC's "open in new
    tab".
- **`nav.home`**
  - Name: Go to first file
  - Scope: Main window/File list
  - macOS: `⌥↑`, `Home`
  - Today on Linux and Windows: `Alt+↑`, `Home`
  - Windows: (d) Alt+↑ here blocks Explorer's Alt+↑ = up.
  - Linux (GNOME, KDE): (d) Alt+↑ here blocks Explorer's Alt+↑ = up.
- **`nav.end`**
  - Name: Go to last file
  - Scope: Main window/File list
  - macOS: `⌥↓`, `End`
  - Today on Linux and Windows: `Alt+↓`, `End`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.pageUp`**
  - Name: Page up
  - Scope: Main window/File list
  - macOS: `PageUp`
  - Today on Linux and Windows: `PageUp`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.pageDown`**
  - Name: Page down
  - Scope: Main window/File list
  - macOS: `PageDown`
  - Today on Linux and Windows: `PageDown`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.back`**
  - Name: Go back
  - Scope: Main window/File list
  - macOS: `⌘[`
  - Today on Linux and Windows: `Ctrl+[`
  - Windows: (d) Windows/Linux expect Alt+← (Explorer, browsers, TC); Ctrl+[ needs AltGr on German/Nordic layouts.
  - Linux (GNOME, KDE): (d) Windows/Linux expect Alt+← (Explorer, browsers, TC); Ctrl+[ needs AltGr on German/Nordic
    layouts.
- **`nav.forward`**
  - Name: Go forward
  - Scope: Main window/File list
  - macOS: `⌘]`
  - Today on Linux and Windows: `Ctrl+]`
  - Windows: (d) Expect Alt+→.
  - Linux (GNOME, KDE): (d) Expect Alt+→.
- **`nav.goToRoot`**
  - Name: Go to root folder
  - Scope: Main window/File list
  - macOS: `⌘/`
  - Today on Linux and Windows: `Ctrl+/`
  - Windows: (d) TC users expect Ctrl+\.
  - Linux (GNOME, KDE): (d) TC users expect Ctrl+\.
- **`nav.goHome`**
  - Name: Go to home folder
  - Scope: Main window/File list
  - macOS: `⌘⇧H`
  - Today on Linux and Windows: `Ctrl+Shift+H`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`errorPane.toggleTechnicalDetails` (Fixed)**
  - Name: Show technical details
  - Scope: Main window/Error screen
  - macOS: `⌘D`
  - Today on Linux and Windows: `Ctrl+D`
  - Windows: (b) clash on Ctrl+D
  - Linux (GNOME, KDE): (b) clash on Ctrl+D
- **`nav.left` (Fixed)**
  - Name: Move to left column
  - Scope: Main window/Brief mode
  - macOS: `←`
  - Today on Linux and Windows: `←`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.right` (Fixed)**
  - Name: Move to right column
  - Scope: Main window/Brief mode
  - macOS: `→`
  - Today on Linux and Windows: `→`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.firstInFull` (Fixed)**
  - Name: Jump to first file
  - Scope: Main window/Full mode
  - macOS: `←`
  - Today on Linux and Windows: `←`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`nav.lastInFull` (Fixed)**
  - Name: Jump to last file
  - Scope: Main window/Full mode
  - macOS: `→`
  - Today on Linux and Windows: `→`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.rename`**
  - Name: Rename
  - Scope: Main window/File list
  - macOS: `F2`, `⇧F6`
  - Today on Linux and Windows: `F2`, `Shift+F6`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`pane.quickFilter`**
  - Name: Quick filter
  - Scope: Main window/File list
  - macOS: `⌃S`
  - Today on Linux and Windows: `Ctrl+S`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.multiRename`**
  - Name: Multi-rename
  - Scope: Main window/File list
  - macOS: `⌃M`
  - Today on Linux and Windows: `Ctrl+M`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.view`**
  - Name: View
  - Scope: Main window/File list
  - macOS: `F3`
  - Today on Linux and Windows: `F3`
  - Windows: (c) F3: windows WebView2 find on page (verified)
  - Linux (GNOME, KDE): (a) fine
- **`file.edit`**
  - Name: Edit in default editor
  - Scope: Main window/File list
  - macOS: `F4`
  - Today on Linux and Windows: `F4`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.copy`**
  - Name: Copy
  - Scope: Main window/File list
  - macOS: `F5`
  - Today on Linux and Windows: `F5`
  - Windows: (c) F5: windows WebView2 reload (verified)
  - Linux (GNOME, KDE): (a) fine
- **`file.move`**
  - Name: Move
  - Scope: Main window/File list
  - macOS: `F6`
  - Today on Linux and Windows: `F6`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.duplicate`**
  - Name: Duplicate
  - Scope: Main window/File list
  - macOS: `⌘D`
  - Today on Linux and Windows: `Ctrl+D`
  - Windows: (b) clash on Ctrl+D; (d) TC users expect Shift+F5 (Ctrl+D is the hotlist).
  - Linux (GNOME, KDE): (b) clash on Ctrl+D; (d) TC users expect Shift+F5 (Ctrl+D is the hotlist).
- **`file.compress`**
  - Name: Compress
  - Scope: Main window/File list
  - macOS: `⌥F5`
  - Today on Linux and Windows: `Alt+F5`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (c) Alt+F5: gnome Restore (verified)
- **`edit.copy`**
  - Name: Copy to clipboard
  - Scope: Main window/File list
  - macOS: `⌘C`
  - Today on Linux and Windows: `Ctrl+C`
  - Windows: (d) Bug: text-selection guard only knows "⌘C" (global-keydown.ts:85), so Ctrl+C with text selected runs
    file copy.
  - Linux (GNOME, KDE): (d) Bug: text-selection guard only knows "⌘C" (global-keydown.ts:85), so Ctrl+C with text
    selected runs file copy.
- **`edit.cut`**
  - Name: Cut to clipboard
  - Scope: Main window/File list
  - macOS: `⌘X`
  - Today on Linux and Windows: `Ctrl+X`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`edit.paste`**
  - Name: Paste
  - Scope: Main window/File list
  - macOS: `⌘V`
  - Today on Linux and Windows: `Ctrl+V`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`edit.pasteAsMove`**
  - Name: Move here
  - Scope: Main window/File list
  - macOS: `⌘⌥V`
  - Today on Linux and Windows: `Ctrl+Alt+V`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.newFolder`**
  - Name: New folder
  - Scope: Main window/File list
  - macOS: `F7`
  - Today on Linux and Windows: `F7`
  - Windows: (c) F7: windows Chromium caret browsing prompt (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`file.newFile`**
  - Name: Create new file
  - Scope: Main window/File list
  - macOS: `⇧F4`
  - Today on Linux and Windows: `Shift+F4`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.delete`**
  - Name: Delete
  - Scope: Main window/File list
  - macOS: `F8`, `⌘Backspace`, `Delete`
  - Today on Linux and Windows: `F8`, `Ctrl+Backspace`, `Delete`
  - Windows: (d) Ctrl+Backspace is "delete word" in Windows/GTK text fields; F8/Delete already cover it.
  - Linux (GNOME, KDE): (d) Ctrl+Backspace is "delete word" in Windows/GTK text fields; F8/Delete already cover it.
- **`file.deletePermanently`**
  - Name: Delete permanently
  - Scope: Main window/File list
  - macOS: `⇧F8`, `⇧Delete`, `⌘⌥Backspace`
  - Today on Linux and Windows: `Shift+F8`, `Shift+Delete`, `Ctrl+Alt+Backspace`
  - Windows: (d) Ctrl+Alt+Backspace is unconventional (and the X11 zap combo where enabled).
  - Linux (GNOME, KDE): (d) Ctrl+Alt+Backspace is unconventional (and the X11 zap combo where enabled).
- **`file.showInFinder`**
  - Name: Show in file manager
  - Scope: Main window/File list
  - macOS: `⌘⌥O`
  - Today on Linux and Windows: `Ctrl+Alt+O`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.openTerminalHere`**
  - Name: Open terminal here
  - Scope: Main window/File list
  - macOS: `F9`, `⌘⌥T`
  - Today on Linux and Windows: –
  - Windows: -
  - Linux (GNOME, KDE): -
- **`file.copyPath`**
  - Name: Copy path to clipboard
  - Scope: Main window/File list
  - macOS: `⌘⌥C`
  - Today on Linux and Windows: `Ctrl+Alt+C`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`file.getInfo`**
  - Name: File properties
  - Scope: Main window/File list
  - macOS: `⌘I`
  - Today on Linux and Windows: –
  - Windows: -
  - Linux (GNOME, KDE): -
- **`file.quickLook`**
  - Name: Preview
  - Scope: Main window/File list
  - macOS: `⇧Space`
  - Today on Linux and Windows: –
  - Windows: -
  - Linux (GNOME, KDE): -
- **`file.contextMenu`**
  - Name: Open context menu
  - Scope: Main window/File list
  - macOS: `⌃Enter`
  - Today on Linux and Windows: `Ctrl+Enter`
  - Windows: (d) Windows/GTK expect Shift+F10 or the Menu key.
  - Linux (GNOME, KDE): (d) Windows/GTK expect Shift+F10 or the Menu key.
- **`selection.toggle`**
  - Name: Toggle selection
  - Scope: Main window/File list
  - macOS: `Space`
  - Today on Linux and Windows: `Space`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.toggleAndDown`**
  - Name: Toggle selection and move down
  - Scope: Main window/File list
  - macOS: `Insert`
  - Today on Linux and Windows: `Insert`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.selectAll`**
  - Name: Select all
  - Scope: Main window/File list
  - macOS: `⌘A`
  - Today on Linux and Windows: `Ctrl+A`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.deselectAll`**
  - Name: Deselect all
  - Scope: Main window/File list
  - macOS: `⌘⇧A`
  - Today on Linux and Windows: `Ctrl+Shift+A`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.invert`**
  - Name: Invert selection
  - Scope: Main window/File list
  - macOS: `*`
  - Today on Linux and Windows: `*`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.selectSameKind`**
  - Name: Select all of the same kind
  - Scope: Main window/File list
  - macOS: `⌥⇧=`, `⌥+`
  - Today on Linux and Windows: `Alt+Shift+=`, `Alt++`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.compareDirectories`**
  - Name: Compare folders
  - Scope: Main window/File list
  - macOS: `⇧F2`
  - Today on Linux and Windows: `Shift+F2`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.selectFiles`**
  - Name: Select files…
  - Scope: Main window/File list
  - macOS: `+`
  - Today on Linux and Windows: `+`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`selection.deselectFiles`**
  - Name: Deselect files…
  - Scope: Main window/File list
  - macOS: `-`
  - Today on Linux and Windows: `-`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.openPresets` (Fixed)**
  - Name: Open the presets menu
  - Scope: Main window/Multi-rename
  - macOS: `F2`
  - Today on Linux and Windows: `F2`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.savePreset` (Fixed)**
  - Name: Save the current settings as a preset
  - Scope: Main window/Multi-rename
  - macOS: `⌘S`
  - Today on Linux and Windows: `Ctrl+S`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.letterCase` (Fixed)**
  - Name: Open the letter case menu
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥U`
  - Today on Linux and Windows: `Ctrl+Alt+U`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.removeDiacritics` (Fixed)**
  - Name: Turn Remove diacritics on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥N`
  - Today on Linux and Windows: `Ctrl+Alt+N`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.greekToLatin` (Fixed)**
  - Name: Turn Greek to Latin on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥G`
  - Today on Linux and Windows: `Ctrl+Alt+G`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.normalizeUnicode` (Fixed)**
  - Name: Turn Normalize Unicode on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥P`
  - Today on Linux and Windows: `Ctrl+Alt+P`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.matchCase` (Fixed)**
  - Name: Turn Match case on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥I`
  - Today on Linux and Windows: `Ctrl+Alt+I`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.firstMatchOnly` (Fixed)**
  - Name: Turn First match only on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥F`
  - Today on Linux and Windows: `Ctrl+Alt+F`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.includeExtension` (Fixed)**
  - Name: Turn Include extension on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥E`
  - Today on Linux and Windows: `Ctrl+Alt+E`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.regex` (Fixed)**
  - Name: Turn Regular expression on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥R`
  - Today on Linux and Windows: `Ctrl+Alt+R`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.replaceWholeName` (Fixed)**
  - Name: Turn Replace whole name on or off
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥W`
  - Today on Linux and Windows: `Ctrl+Alt+W`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.insertField` (Fixed)**
  - Name: Insert a photo, video, or music field
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥X`
  - Today on Linux and Windows: `Ctrl+Alt+X`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.undoRename` (Fixed)**
  - Name: Undo the last multi-rename
  - Scope: Main window/Multi-rename
  - macOS: `⌘⌥Z`
  - Today on Linux and Windows: `Ctrl+Alt+Z`
  - Windows: (c) AltGr shadow while typing in the sheet (unverified)
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.results` (Fixed)**
  - Name: Edit the new names in your text editor
  - Scope: Main window/Multi-rename
  - macOS: `⌥Enter`
  - Today on Linux and Windows: `Alt+Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.fieldHistory` (Fixed)**
  - Name: Show the field’s earlier values
  - Scope: Main window/Multi-rename
  - macOS: `↓`
  - Today on Linux and Windows: `↓`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.moveRowUp` (Fixed)**
  - Name: Move the file up in the rename order
  - Scope: Main window/Multi-rename
  - macOS: `⇧↑`, `⌥↑`
  - Today on Linux and Windows: `Shift+↑`, `Alt+↑`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.moveRowDown` (Fixed)**
  - Name: Move the file down in the rename order
  - Scope: Main window/Multi-rename
  - macOS: `⇧↓`, `⌥↓`
  - Today on Linux and Windows: `Shift+↓`, `Alt+↓`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`multiRename.rename` (Fixed)**
  - Name: Rename the files
  - Scope: Main window/Multi-rename
  - macOS: `⌘Enter`
  - Today on Linux and Windows: `Ctrl+Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`operationLog.goToItems` (Fixed)**
  - Name: Go to the operation’s items
  - Scope: Main window/Operation log
  - macOS: `→`, `Enter`
  - Today on Linux and Windows: `→`, `Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`operationLog.toggleItem` (Fixed)**
  - Name: Select or deselect the item
  - Scope: Main window/Operation log
  - macOS: `Space`
  - Today on Linux and Windows: `Space`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`operationLog.toggleItemAndDown` (Fixed)**
  - Name: Select or deselect the item, then move down
  - Scope: Main window/Operation log
  - macOS: `Insert`
  - Today on Linux and Windows: `Insert`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`operationLog.selectAll` (Fixed)**
  - Name: Select every item
  - Scope: Main window/Operation log
  - macOS: `⌘A`
  - Today on Linux and Windows: `Ctrl+A`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`operationLog.deselectAll` (Fixed)**
  - Name: Deselect every item
  - Scope: Main window/Operation log
  - macOS: `⌘⇧A`
  - Today on Linux and Windows: `Ctrl+Shift+A`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`operationLog.rollBack` (Fixed)**
  - Name: Roll back the selected items
  - Scope: Main window/Operation log
  - macOS: `⌘Enter`
  - Today on Linux and Windows: `Ctrl+Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`network.selectHost` (Fixed)**
  - Name: Select network host
  - Scope: Main window/Servers
  - macOS: `Enter`
  - Today on Linux and Windows: `Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`share.back` (Fixed)**
  - Name: Back to host list
  - Scope: Main window/Places
  - macOS: `Backspace`, `Escape`, `⌘↑`
  - Today on Linux and Windows: `Backspace`, `Escape`, `Ctrl+↑`
  - Windows: (d) Ctrl+↑ is mechanical; Alt+↑ is the convention.
  - Linux (GNOME, KDE): (d) Ctrl+↑ is mechanical; Alt+↑ is the convention.
- **`share.selectShare` (Fixed)**
  - Name: Connect to share
  - Scope: Main window/Places
  - macOS: `Enter`
  - Today on Linux and Windows: `Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`servers.connect`**
  - Name: Connect to server…
  - Scope: Main window
  - macOS: `⌘K`
  - Today on Linux and Windows: `Ctrl+K`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`servers.edit`**
  - Name: Edit server…
  - Scope: Main window/Servers
  - macOS: `⌘E`
  - Today on Linux and Windows: `Ctrl+E`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`volume.select` (Fixed)**
  - Name: Select volume
  - Scope: Main window/Volume chooser
  - macOS: `Enter`
  - Today on Linux and Windows: `Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`volume.close` (Fixed)**
  - Name: Close volume switcher
  - Scope: Main window/Volume chooser
  - macOS: `Escape`
  - Today on Linux and Windows: `Escape`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`favorites.openByNumber` (Fixed)**
  - Name: Open the favorite with that number
  - Scope: Main window/Favorites menu
  - macOS: `1`, `2`, `3`, `4`, `5`, `6`, `7`, `8`, `9`
  - Today on Linux and Windows: `1`, `2`, `3`, `4`, `5`, `6`, `7`, `8`, `9`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`favorites.addFromMenu` (Fixed)**
  - Name: fileExplorer.navigation.favoritesAddCurrent
  - Scope: Main window/Favorites menu
  - macOS: `0`
  - Today on Linux and Windows: `0`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`about.close`**
  - Name: Close About window
  - Scope: About window
  - macOS: `Escape`
  - Today on Linux and Windows: `Escape`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.cursorUp` (Fixed)**
  - Name: Previous operation
  - Scope: Operation queue window
  - macOS: `↑`
  - Today on Linux and Windows: `↑`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.cursorDown` (Fixed)**
  - Name: Next operation
  - Scope: Operation queue window
  - macOS: `↓`
  - Today on Linux and Windows: `↓`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.cursorFirst` (Fixed)**
  - Name: First operation
  - Scope: Operation queue window
  - macOS: `Home`
  - Today on Linux and Windows: `Home`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.cursorLast` (Fixed)**
  - Name: Last operation
  - Scope: Operation queue window
  - macOS: `End`
  - Today on Linux and Windows: `End`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.extendUp` (Fixed)**
  - Name: Extend the selection up
  - Scope: Operation queue window
  - macOS: `⇧↑`
  - Today on Linux and Windows: `Shift+↑`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.extendDown` (Fixed)**
  - Name: Extend the selection down
  - Scope: Operation queue window
  - macOS: `⇧↓`
  - Today on Linux and Windows: `Shift+↓`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.toggleSelected` (Fixed)**
  - Name: Add the operation to the selection, or remove it
  - Scope: Operation queue window
  - macOS: `Space`
  - Today on Linux and Windows: `Space`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.selectAll` (Fixed)**
  - Name: Select every operation
  - Scope: Operation queue window
  - macOS: `⌘A`
  - Today on Linux and Windows: `Ctrl+A`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.showInMainWindow` (Fixed)**
  - Name: Show the operation in the main window
  - Scope: Operation queue window
  - macOS: `Enter`, `S`
  - Today on Linux and Windows: `Enter`, `S`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.pauseOrResume` (Fixed)**
  - Name: Pause or resume the operation
  - Scope: Operation queue window
  - macOS: `P`
  - Today on Linux and Windows: `P`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.cancelOrDismiss` (Fixed)**
  - Name: Cancel the operation, or dismiss one that couldn’t finish
  - Scope: Operation queue window
  - macOS: `Backspace`, `Delete`
  - Today on Linux and Windows: `Backspace`, `Delete`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.cancel` (Fixed)**
  - Name: Cancel the operation
  - Scope: Operation queue window
  - macOS: `C`
  - Today on Linux and Windows: `C`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.rollback` (Fixed)**
  - Name: Roll back the operation
  - Scope: Operation queue window
  - macOS: `B`
  - Today on Linux and Windows: `B`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.moveEarlier` (Fixed)**
  - Name: Move the waiting operation up in line
  - Scope: Operation queue window
  - macOS: `⌥↑`
  - Today on Linux and Windows: `Alt+↑`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.moveLater` (Fixed)**
  - Name: Move the waiting operation down in line
  - Scope: Operation queue window
  - macOS: `⌥↓`
  - Today on Linux and Windows: `Alt+↓`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.nextControl` (Fixed)**
  - Name: Next button in the row
  - Scope: Operation queue window
  - macOS: `→`
  - Today on Linux and Windows: `→`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.previousControl` (Fixed)**
  - Name: Previous button in the row
  - Scope: Operation queue window
  - macOS: `←`
  - Today on Linux and Windows: `←`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`queue.close` (Fixed)**
  - Name: Close the operation queue
  - Scope: Operation queue window
  - macOS: `Escape`
  - Today on Linux and Windows: `Escape`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.toggleWordWrap`**
  - Name: Turn word wrap on or off
  - Scope: Viewer window
  - macOS: `W`
  - Today on Linux and Windows: `W`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.toggleTail`**
  - Name: Turn tail mode on or off
  - Scope: Viewer window
  - macOS: `F`
  - Today on Linux and Windows: `F`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.viewAsMedia`**
  - Name: View as image or PDF
  - Scope: Viewer window
  - macOS: `0`
  - Today on Linux and Windows: `0`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.viewAsText`**
  - Name: View as text
  - Scope: Viewer window
  - macOS: `1`
  - Today on Linux and Windows: `1`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.viewAsBinary`**
  - Name: View as binary
  - Scope: Viewer window
  - macOS: `2`
  - Today on Linux and Windows: `2`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.viewAsHex`**
  - Name: View as hex
  - Scope: Viewer window
  - macOS: `3`
  - Today on Linux and Windows: `3`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.zoomIn`**
  - Name: Zoom in on the image
  - Scope: Viewer window
  - macOS: `+`, `=`
  - Today on Linux and Windows: `+`, `=`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.zoomOut`**
  - Name: Zoom out of the image
  - Scope: Viewer window
  - macOS: `-`
  - Today on Linux and Windows: `-`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.toggleImageZoom`**
  - Name: Switch the image between fit and actual size
  - Scope: Viewer window
  - macOS: `Enter`, `Space`
  - Today on Linux and Windows: `Enter`, `Space`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`viewer.goBack`**
  - Name: Go back to the previous document
  - Scope: Viewer window
  - macOS: `⌘[`
  - Today on Linux and Windows: `Ctrl+[`
  - Windows: (d) Expect Alt+←.
  - Linux (GNOME, KDE): (d) Expect Alt+←.
- **`viewer.goForward`**
  - Name: Go forward to the next document
  - Scope: Viewer window
  - macOS: `⌘]`
  - Today on Linux and Windows: `Ctrl+]`
  - Windows: (d) Expect Alt+→.
  - Linux (GNOME, KDE): (d) Expect Alt+→.
- **`palette.up` (Fixed)**
  - Name: Previous result
  - Scope: Command palette
  - macOS: `↑`
  - Today on Linux and Windows: `↑`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`palette.down` (Fixed)**
  - Name: Next result
  - Scope: Command palette
  - macOS: `↓`
  - Today on Linux and Windows: `↓`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`palette.execute` (Fixed)**
  - Name: Execute command
  - Scope: Command palette
  - macOS: `Enter`
  - Today on Linux and Windows: `Enter`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine
- **`palette.close` (Fixed)**
  - Name: Close palette
  - Scope: Command palette
  - macOS: `Escape`
  - Today on Linux and Windows: `Escape`
  - Windows: (a) fine
  - Linux (GNOME, KDE): (a) fine

No default on any platform (66): `app.showAll`, `app.about`, `app.acknowledgements`, `app.checkForUpdates`,
`cmdr.openOnboarding`, `app.toggleCliCommand`, `help.openShortcuts`, `help.accessibility`, `help.viewDebugLog`,
`help.sendErrorReport`, `help.whatsNew`, `feedback.send`, `suggestedOps.show`, `favorites.add`, `view.setMode`,
`view.zoom.set75`, `view.zoom.set125`, `view.zoom.set150`, `sort.byCreated`, `sort.byAdded`, `sort.ascending`,
`sort.descending`, `sort.toggleOrder`, `sort.set`, `pane.resetDivider`, `tab.togglePin`, `tab.closeOthers`,
`tab.moveToOtherPane`, `edit.undoTrash`, `file.goToTrash`, `file.copyCurrentDirectoryPath`, `file.copyFilename`,
`file.copyShareLink`, `file.copyShareLinkOneDay`, `file.copyShareLinkOneHour`, `cloud.makeOffline`,
`cloud.removeDownload`, `cloud.openInGoogleDrive`, `cloud.copyGoogleDriveLink`, `cloud.askGemini`,
`restic.forgetPassword`, `tags.toggleGrey`, `tags.toggleGreen`, `tags.togglePurple`, `tags.toggleBlue`,
`tags.toggleYellow`, `tags.toggleRed`, `tags.toggleOrange`, `selection.compareDirectoriesMissing`,
`selection.compareDirectoriesSize`, `network.refresh`, `servers.show`, `servers.togglePin`, `servers.disconnect`,
`servers.forgetSecret`, `volume.selectByName`, `selection.mcpSelect`, `selection.mcpSelectByNames`, `cursor.moveTo`,
`cursor.scrollTo`, `nav.openUnderCursor`, `tab.mcpAction`, `dialog.confirm`, `about.openWebsite`, `about.openUpgrade`,
`app.licenseKey`.

## 2. Internal clashes off macOS (computed)

Computed over every scope pair the real `scopeHierarchy` (`apps/desktop/src/lib/shortcuts/scope-hierarchy.ts`) says
overlaps, plus same-combo Tier 1 commands in sibling scopes (one dispatch winner). macOS: zero. Linux and Windows today:

- **`Ctrl+D`**
  - Commands: `file.duplicate` (⌘D) vs `favorites.open` (⌃D)
  - Scopes: File list vs Main window
  - Effect: Dispatch keeps the more specific scope, so in the file list Duplicate wins and Favorites is dead. Known,
    parked in `apps/desktop/src/lib/shortcuts/conflict-detector-registry.test.ts:17-25`. Extra twist: the Linux/Windows
    native menu registers `Ctrl+D` on Favorites (`bar_test.rs` LINUX_MENU_BAR, Go menu), so if GTK/Win32 handle the
    accelerator before the webview (unverified), Favorites opens and Duplicate dies instead, or both fire.
- **`Ctrl+D`**
  - Commands: `errorPane.toggleTechnicalDetails` (⌘D, fixed) vs `favorites.open`
  - Scopes: Error screen vs Main window
  - Effect: Not mentioned anywhere today; the registry test subtracts the whole `Ctrl+D` combo, which hides this second
    pair.

No other overlapping-scope clash exists after mapping (all 228 commands checked). The proposal in the JSON has zero
clashes on both platforms (same check). Near-misses the mapping avoids only because of the hand overrides: `⌃F5`/`⌃F6`
(would hit `⌘F5`/`⌘F6` sort keys), `⌃⌥←/→` (would hit `⌘⌥←/→` tab keys). The ⌃→Shift hack currently decides only
`tab.moveLeft/Right` (`⌘⌃⇧←/→` → `Ctrl+Shift+←/→`) and the native `app.hideOthers`.

## 3. OS-, desktop-, and webview-reserved combos

### Windows

- **`Alt+F4`**
  - What it does: Close the active window
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Alt+Escape`**
  - What it does: Cycle windows
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Alt+Space`**
  - What it does: Window menu
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Alt+Tab`**
  - What it does: Switch windows
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Alt+Shift+Tab`**
  - What it does: Switch windows
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Ctrl+Alt+Delete`**
  - What it does: Security screen
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Ctrl+Alt+Tab`**
  - What it does: App thumbnails
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Ctrl+Escape`**
  - What it does: Start menu
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Ctrl+Shift+Escape`**
  - What it does: Task Manager
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`F10`**
  - What it does: Activate the menu bar
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Alt+PrintScreen`**
  - What it does: Window screenshot
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`Super+*`**
  - What it does: Windows-logo-key shortcuts (about 110 listed)
  - Kind: os
  - Verified: yes
  - Source:
    https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- **`PrintScreen`**
  - What it does: Snipping Tool (Windows 11 default)
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Alt (alone)`**
  - What it does: Focus the menu bar / open a menu by its & mnemonic; Cmdr's off-mac bar allocates & mnemonics
    (apps/desktop/src-tauri/src/menu/DETAILS.md "Linux mnemonics are allocated") and Windows uses that same bar
    (crates/cmdr-menu/src/spec.rs:45 Platform::current() is Linux for every non-mac OS)
  - Kind: os
  - Verified: no
  - Source: from memory (Win32 menu behavior)
- **`Alt+<menu letter>`**
  - What it does: Focus the menu bar / open a menu by its & mnemonic; Cmdr's off-mac bar allocates & mnemonics
    (apps/desktop/src-tauri/src/menu/DETAILS.md "Linux mnemonics are allocated") and Windows uses that same bar
    (crates/cmdr-menu/src/spec.rs:45 Platform::current() is Linux for every non-mac OS)
  - Kind: os
  - Verified: no
  - Source: from memory (Win32 menu behavior)
- **`Left Alt+Shift`**
  - What it does: Switch input language (only when several layouts are installed; fires on release with no other key)
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Shift`**
  - What it does: Switch input language (only when several layouts are installed; fires on release with no other key)
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+<letter>`**
  - What it does: AltGr is reported as Ctrl+Alt on Windows, so a Ctrl+Alt+letter binding shadows an AltGr character
    (German AltGr+E = €, AltGr+Q = @; Polish AltGr+A/C/E/L/N/O/S/X/Z = ą ć ę ł ń ó ś ź ż)
  - Kind: layout
  - Verified: no
  - Source: from memory (Chromium/WebView2 KeyboardEvent on Windows)
- **`Ctrl+Alt+←`**
  - What it does: Screen rotation on some Intel graphics drivers
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+→`**
  - What it does: Screen rotation on some Intel graphics drivers
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+↑`**
  - What it does: Screen rotation on some Intel graphics drivers
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+↓`**
  - What it does: Screen rotation on some Intel graphics drivers
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+F`**
  - What it does: WebView2 find on page
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`F3`**
  - What it does: WebView2 find on page
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`Ctrl+P`**
  - What it does: WebView2 print
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`Ctrl+R`**
  - What it does: WebView2 reload
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`F5`**
  - What it does: WebView2 reload
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`Ctrl++`**
  - What it does: WebView2 zoom (Tauri turns zoom hotkeys off by default: tauri-utils-2.10.2 config.rs:2450)
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`Ctrl+-`**
  - What it does: WebView2 zoom (Tauri turns zoom hotkeys off by default: tauri-utils-2.10.2 config.rs:2450)
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`Ctrl+Shift+C`**
  - What it does: WebView2 DevTools (only where DevTools are enabled)
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`F12`**
  - What it does: WebView2 DevTools (only where DevTools are enabled)
  - Kind: webview
  - Verified: yes
  - Source:
    https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
- **`Alt+←`**
  - What it does: WebView2 history back / forward ("special keys for browser functions" is all the doc says)
  - Kind: webview
  - Verified: no
  - Source: from memory
- **`Alt+→`**
  - What it does: WebView2 history back / forward ("special keys for browser functions" is all the doc says)
  - Kind: webview
  - Verified: no
  - Source: from memory
- **`F7`**
  - What it does: Chromium caret browsing prompt
  - Kind: webview
  - Verified: no
  - Source: from memory
- **`Ctrl+Shift+I`**
  - What it does: DevTools
  - Kind: webview
  - Verified: no
  - Source: from memory
- **`Ctrl+Shift+J`**
  - What it does: DevTools
  - Kind: webview
  - Verified: no
  - Source: from memory
- **`Ctrl+G`**
  - What it does: Find next / previous
  - Kind: webview
  - Verified: no
  - Source: from memory
- **`Shift+F3`**
  - What it does: Find next / previous
  - Kind: webview
  - Verified: no
  - Source: from memory
- **`Ctrl+Shift+G`**
  - What it does: Find next / previous
  - Kind: webview
  - Verified: no
  - Source: from memory

### GNOME

- **`Alt+F2`**
  - What it does: Run command prompt
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+F4`**
  - What it does: Close window
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+F5`**
  - What it does: Restore (unmaximize) window
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+F6`**
  - What it does: Switch windows of an app directly
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+Shift+F6`**
  - What it does: Switch windows of an app directly
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+F7`**
  - What it does: Move window
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+F8`**
  - What it does: Resize window
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+F10`**
  - What it does: Toggle maximization
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+Space`**
  - What it does: Window menu
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+Tab`**
  - What it does: Switch applications / windows of an app
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+Shift+Tab`**
  - What it does: Switch applications / windows of an app
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+``**
  - What it does: Switch applications / windows of an app
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+Shift+``**
  - What it does: Switch applications / windows of an app
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+Escape`**
  - What it does: Cycle windows / system controls
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Alt+Shift+Escape`**
  - What it does: Cycle windows / system controls
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Escape`**
  - What it does: Cycle windows / system controls
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Shift+Escape`**
  - What it does: Cycle windows / system controls
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Tab`**
  - What it does: Switch system controls
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Shift+Tab`**
  - What it does: Switch system controls
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+←`**
  - What it does: Switch workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+→`**
  - What it does: Switch workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+↑`**
  - What it does: Switch workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+↓`**
  - What it does: Switch workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Shift+←`**
  - What it does: Move window to workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Shift+→`**
  - What it does: Move window to workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Shift+↑`**
  - What it does: Move window to workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+Shift+↓`**
  - What it does: Move window to workspace
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Super (alone)`**
  - What it does: Activities overlay and Super shortcuts
    (Super+A/V/M/S/N/1-9/H/Up/Down/Left/Right/Space/Tab/Home/End/PageUp/PageDown, Super+Ctrl+1-9, …)
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
    ; https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/data/org.gnome.shell.gschema.xml.in ;
    https://gitlab.gnome.org/GNOME/mutter/-/raw/main/data/org.gnome.mutter.gschema.xml.in
- **`Super+*`**
  - What it does: Activities overlay and Super shortcuts
    (Super+A/V/M/S/N/1-9/H/Up/Down/Left/Right/Space/Tab/Home/End/PageUp/PageDown, Super+Ctrl+1-9, …)
  - Kind: os
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
    ; https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/data/org.gnome.shell.gschema.xml.in ;
    https://gitlab.gnome.org/GNOME/mutter/-/raw/main/data/org.gnome.mutter.gschema.xml.in
- **`PrintScreen`**
  - What it does: Screenshots
  - Kind: os
  - Verified: yes
  - Source: https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/data/org.gnome.shell.gschema.xml.in
- **`Shift+PrintScreen`**
  - What it does: Screenshots
  - Kind: os
  - Verified: yes
  - Source: https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/data/org.gnome.shell.gschema.xml.in
- **`Alt+PrintScreen`**
  - What it does: Screenshots
  - Kind: os
  - Verified: yes
  - Source: https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/data/org.gnome.shell.gschema.xml.in
- **`Ctrl+Alt+Shift+R`**
  - What it does: Screen recording
  - Kind: os
  - Verified: yes
  - Source: https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/data/org.gnome.shell.gschema.xml.in
- **`Alt+F1`**
  - What it does: Nothing by default: panel-main-menu is deprecated with an empty default (Ubuntu or extensions may
    differ)
  - Kind: free
  - Verified: yes
  - Source:
    https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
- **`Ctrl+Alt+T`**
  - What it does: Open terminal (Ubuntu default custom binding)
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+Delete`**
  - What it does: Log out
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+F1…F12`**
  - What it does: Switch virtual terminal / session
  - Kind: os
  - Verified: no
  - Source: from memory
- **`F10`**
  - What it does: GTK activates the menu bar (gtk-menu-bar-accel default)
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Alt+<menu letter>`**
  - What it does: GTK menu-bar mnemonics, allocated per language (apps/desktop/src-tauri/src/menu/DETAILS.md)
  - Kind: os
  - Verified: yes
  - Source: apps/desktop/src-tauri/src/menu/DETAILS.md:159

### KDE Plasma

- **`Alt+F3`**
  - What it does: Window operations menu
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/useractions.cpp (line 827)
- **`Alt+F4`**
  - What it does: Close window
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/useractions.cpp (line 829)
- **`Ctrl+F1`**
  - What it does: Switch to desktop 1–4 (alongside Meta+F1–F4)
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/virtualdesktops.cpp (lines 884–887)
- **`Ctrl+F2`**
  - What it does: Switch to desktop 1–4 (alongside Meta+F1–F4)
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/virtualdesktops.cpp (lines 884–887)
- **`Ctrl+F3`**
  - What it does: Switch to desktop 1–4 (alongside Meta+F1–F4)
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/virtualdesktops.cpp (lines 884–887)
- **`Ctrl+F4`**
  - What it does: Switch to desktop 1–4 (alongside Meta+F1–F4)
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/virtualdesktops.cpp (lines 884–887)
- **`Ctrl+F7`**
  - What it does: Present windows of the current app
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/plugins/windowview/windowvieweffect.cpp (line 68)
- **`Ctrl+F9`**
  - What it does: Present windows (current desktop)
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/plugins/windowview/windowvieweffect.cpp (line 50)
- **`Ctrl+F10`**
  - What it does: Present windows (all desktops)
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/plugins/windowview/windowvieweffect.cpp (line 59)
- **`Meta+*`**
  - What it does: Tiling, maximize, desktops, overview (Meta+W), grid (Meta+G), show desktop (Meta+D), Meta+Ctrl+Escape
    kill window, …
  - Kind: os
  - Verified: yes
  - Source: https://raw.githubusercontent.com/KDE/kwin/master/src/useractions.cpp ;
    https://raw.githubusercontent.com/KDE/kwin/master/src/virtualdesktops.cpp
- **`Alt+F1`**
  - What it does: Application launcher (Kickoff)
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Alt+F2`**
  - What it does: KRunner
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Alt+Space`**
  - What it does: KRunner
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Alt+Tab`**
  - What it does: Task switcher
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Alt+Shift+Tab`**
  - What it does: Task switcher
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Alt+``**
  - What it does: Task switcher
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Escape`**
  - What it does: System monitor
  - Kind: os
  - Verified: no
  - Source: from memory
- **`PrintScreen`**
  - What it does: Spectacle
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+L`**
  - What it does: Lock screen
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Meta+L`**
  - What it does: Lock screen
  - Kind: os
  - Verified: no
  - Source: from memory
- **`Ctrl+Alt+Delete`**
  - What it does: Log out dialog
  - Kind: os
  - Verified: no
  - Source: from memory

### Today's Cmdr defaults that hit them

| Command                   | Combo              | Windows                                                     | GNOME                               | KDE                               |
| ------------------------- | ------------------ | ----------------------------------------------------------- | ----------------------------------- | --------------------------------- |
| `log.operationLog`        | `Ctrl+Alt+L`       | –                                                           | –                                   | Lock screen (unverified)          |
| `search.open`             | `Ctrl+F`           | WebView2 find on page (verified)                            | –                                   | –                                 |
| `search.open`             | `Alt+F7`           | –                                                           | Move window (verified)              | –                                 |
| `nav.goToPath`            | `Ctrl+G`           | Find next / previous (unverified)                           | –                                   | –                                 |
| `nav.goToPath`            | `Ctrl+Shift+G`     | Find next / previous (unverified)                           | –                                   | –                                 |
| `view.zoom.in`            | `Ctrl++`           | WebView2 zoom (verified)                                    | –                                   | –                                 |
| `view.zoom.out`           | `Ctrl+-`           | WebView2 zoom (verified)                                    | –                                   | –                                 |
| `sort.byName`             | `Ctrl+F3`          | –                                                           | –                                   | Switch to desktop 1–4 (verified)  |
| `sort.byExtension`        | `Ctrl+F4`          | –                                                           | –                                   | Switch to desktop 1–4 (verified)  |
| `pane.clone`              | `Ctrl+Shift+C`     | WebView2 DevTools (verified)                                | –                                   | –                                 |
| `pane.leftVolumeChooser`  | `Alt+F1`           | –                                                           | –                                   | Application launcher (unverified) |
| `pane.rightVolumeChooser` | `Alt+F2`           | –                                                           | Run command prompt (verified)       | KRunner (unverified)              |
| `pane.refresh`            | `Ctrl+R`           | WebView2 reload (verified)                                  | –                                   | –                                 |
| `pane.moveDividerLeft`    | `Ctrl+Alt+Shift+←` | –                                                           | Move window to workspace (verified) | –                                 |
| `pane.moveDividerRight`   | `Ctrl+Alt+Shift+→` | –                                                           | Move window to workspace (verified) | –                                 |
| `tab.next`                | `Ctrl+Alt+→`       | Screen rotation on some Intel graphics drivers (unverified) | Switch workspace (verified)         | –                                 |
| `tab.prev`                | `Ctrl+Alt+←`       | Screen rotation on some Intel graphics drivers (unverified) | Switch workspace (verified)         | –                                 |
| `askCmdr.find`            | `Ctrl+F`           | WebView2 find on page (verified)                            | –                                   | –                                 |
| `askCmdr.findNext`        | `Ctrl+G`           | Find next / previous (unverified)                           | –                                   | –                                 |
| `askCmdr.findPrevious`    | `Ctrl+Shift+G`     | Find next / previous (unverified)                           | –                                   | –                                 |
| `file.view`               | `F3`               | WebView2 find on page (verified)                            | –                                   | –                                 |
| `file.copy`               | `F5`               | WebView2 reload (verified)                                  | –                                   | –                                 |
| `file.compress`           | `Alt+F5`           | –                                                           | Restore (verified)                  | –                                 |
| `file.newFolder`          | `F7`               | Chromium caret browsing prompt (unverified)                 | –                                   | –                                 |

Plus the AltGr class on Windows: every `Ctrl+Alt+<letter>` default (6 in the main window: `queue.show` Q,
`log.operationLog` L, `askCmdr.toggle` A, `edit.pasteAsMove` V, `file.copyPath` C, `file.showInFinder` O; 11 in the
Multi-rename sheet: U N G P I F E R W X Z) reads an AltGr character as its command through the physical-key fallback. In
main-window text fields typing still wins (exact combo only, `global-keydown.ts` `commandForEvent`), but the
Multi-rename option keys resolve the fallback inside text fields on purpose
(`apps/desktop/src/lib/multi-rename/option-keys.ts:1-7`), so typing € (German AltGr+E) or ę ń ź ż (Polish) in a mask
field would flip an option. That AltGr arrives as Ctrl+Alt on Windows is from memory (unverified); on Linux AltGr is a
Level3 shift and not Ctrl+Alt (unverified).

### Off-mac keyboard bugs found on the way (not keymap choices)

- `routes/(main)/global-keydown.ts:85` and `:88` compare against the macOS strings `⌘C`, `⌘←`, `⌘→`, so off macOS:
  Ctrl+C with text selected runs file copy, and Ctrl+←/→ (word jump) in a text field runs "copy path between panes".
- `routes/(main)/global-keydown.ts:71-75` suppresses `⌘A` / `⌥⌘I` by `metaKey`: Super+A / Super+Alt+I off macOS, so
  nothing is suppressed there.
- The search dialog's `⌘N`, `⌘H`, `⌘1`–`⌘9` (`apps/desktop/src/lib/query-ui/query-shortcuts.ts:22-27,105,119,126`)
  require `metaKey` and reject `ctrlKey`: off macOS they are Win+N / Win+H / Win+1–9 (Windows notification centre,
  dictation, taskbar apps) and Super+N / Super+1–9 on GNOME (focus notification, switch to app; verified in
  `org.gnome.shell.gschema.xml.in`), so they are unreachable. Its `⌥A/⌥F/⌥R` become Alt+A/F/R, which may meet menu
  mnemonics (unverified).
- `pane-key-router.ts:153` (`⌘↓` swallow) and `routes/viewer/viewer-keyboard.ts:91,99` (`⌘⇧↑/↓` to document edge) test
  `metaKey` only.
- Click modifiers: ⌘-click toggles a row (`pane-pointer.ts:125`, `BriefList.svelte`, `FullList.svelte` pass `metaKey`);
  off macOS that is Super+click, which many window managers take for moving windows. Windows and Linux users expect
  Ctrl+click.

### WebView2 and WebKitGTK accelerators

- Verified (Microsoft doc,
  https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled):
  with `AreBrowserAcceleratorKeysEnabled` true (the default), WebView2 handles Ctrl+F and F3 (find), Ctrl+P (print),
  Ctrl+R and F5 (reload), Ctrl+Plus/Minus (zoom), Ctrl+Shift+C and F12 (DevTools), and browser Back/Forward/Search keys.
  Editing keys (Ctrl+X/C/V/A/Z, Home/End/PgUp/PgDn) are never affected. The doc does not say whether page
  `preventDefault` cancels them: unverified.
- Consequence: in the main window the claimed commands (Ctrl+F, F3, F5, Ctrl+R) probably cancel the browser action, but
  every OTHER window (Settings, Viewer, Help, About, Queue, Debug) claims none of them, so F5 or Ctrl+R there would
  reload the page, and Ctrl+P would open a print dialog everywhere (unverified, follows from the default). The robust
  fix is turning the setting off per webview; Tauri 2.12.5 has no API, so it needs the Windows `with_webview` controller
  handle (`ICoreWebView2Settings3::put_AreBrowserAcceleratorKeysEnabled(false)`, what wry does at
  `webview2/mod.rs:649-651`) or a Tauri upgrade that exposes it (unverified whether one exists).
- F7 caret browsing, Alt+←/→ history, Ctrl+G find-next, Ctrl+Shift+I: from memory, unverified for WebView2.
- WebKitGTK: no source checked. From memory it has no reload/print/find accelerators of its own outside a browser shell;
  developer tools need `enable-developer-extras`. Unverified; check on a real GNOME box.

## 4. Total Commander default keymap vs Cmdr

Source: Total Commander's `KEYBOARD.TXT` as transcribed (TC 8 era) at
https://softpanorama.org/OFM/Total_commander/keyboard_map.shtml and
https://www.scribd.com/document/351942143/112-Shortcuts-for-Total-Commander-8 (the two agree; secondary sources). The
installer and official pages could not be fetched: the sandbox egress proxy refused `www.ghisler.com`, `ghisler.ch`,
`www.totalcommander.ch`, and `totalcommander.ch` (CONNECT 403), WebFetch could not resolve `www.ghisler.ch`, the TC wiki
has no keyboard page (`https://www.ghisler.ch/wiki/index.php/Keyboard_shortcuts` is 404), and the forum is behind an
Anubis bot wall. So no `KEYBOARD.TXT` from a current TC 11 installer was read; newer additions are missing and marked
unverified. Ctrl+D = directory hotlist is also confirmed on the TC wiki
(https://www.ghisler.ch/wiki/index.php/Directory_hotlist, per search snippet).

107 TC rows (some rows hold two keys); 58 map to an existing Cmdr command (some loosely, noted). Columns: does Cmdr's
default match TC today off macOS, and under the proposal on Windows and on Linux.

- **`F1`**
  - TC action: Help
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: Cmdr has Help > Keyboard shortcuts (help.openShortcuts) but no F1 help.
- **`F2`**
  - TC action: Reread source window
  - Cmdr command: `pane.refresh`
  - Today off mac: differs
  - Proposed Windows: differs
  - Proposed Linux: differs
  - Reserved: –
  - Note: Cmdr F2 = Rename (File Explorer convention); refresh stays Ctrl+R (TC's own second key).
- **`F3`**
  - TC action: List files (Lister)
  - Cmdr command: `file.view`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: F3: windows WebView2 find on page (verified)
  - Note:
- **`F4`**
  - TC action: Edit files
  - Cmdr command: `file.edit`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`F5`**
  - TC action: Copy files
  - Cmdr command: `file.copy`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: F5: windows WebView2 reload (verified)
  - Note:
- **`F6`**
  - TC action: Rename or move files
  - Cmdr command: `file.move`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`F7`**
  - TC action: Create directory
  - Cmdr command: `file.newFolder`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: F7: windows Chromium caret browsing prompt (unverified)
  - Note:
- **`F8 / Delete`**
  - TC action: Delete files
  - Cmdr command: `file.delete`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`F9`**
  - TC action: Activate menu above source window
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: No per-pane menu.
- **`F10`**
  - TC action: Activate left menu / deactivate menu
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: The OS does this for the native bar (Windows verified, GTK unverified).
- **`Alt+F1`**
  - TC action: Change left drive
  - Cmdr command: `pane.leftVolumeChooser`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Alt+F1: kde Application launcher (unverified)
  - Note:
- **`Alt+F2`**
  - TC action: Change right drive
  - Cmdr command: `pane.rightVolumeChooser`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Alt+F2: gnome Run command prompt (verified); Alt+F2: kde KRunner (unverified)
  - Note:
- **`Alt+F3`**
  - TC action: Use alternate viewer
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+Shift+F3`**
  - TC action: Lister with internal viewer only
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+F4`**
  - TC action: Exit
  - Cmdr command: `app.quit`
  - Today off mac: differs
  - Proposed Windows: unbound
  - Proposed Linux: unbound
  - Reserved: Alt+F4: windows Close the active window (verified); Alt+F4: gnome Close window (verified); Alt+F4: kde
    Close window (verified)
  - Note: OS window close; Cmdr has no Quit command off macOS.
- **`Alt+F5`**
  - TC action: Pack files
  - Cmdr command: `file.compress`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Alt+F5: gnome Restore (verified)
  - Note:
- **`Alt+Shift+F5`**
  - TC action: Move to archive
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+F6`**
  - TC action: Unpack from archive
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: No extract command (archives browse as folders).
- **`Alt+F7`**
  - TC action: Find
  - Cmdr command: `search.open`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Alt+F7: gnome Move window (verified)
  - Note:
- **`Alt+F8`**
  - TC action: Command-line history
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: No command line.
- **`Alt+F9`**
  - TC action: Unpack (same as Alt+F6)
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+Shift+F9`**
  - TC action: Test archives
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+F10`**
  - TC action: Directory tree dialog
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+F11`**
  - TC action: Left breadcrumb bar
  - Cmdr command: `pane.focusControls`
  - Today off mac: differs
  - Proposed Windows: differs
  - Proposed Linux: differs
  - Reserved: –
  - Note: Closest: pane chrome focus (path segments).
- **`Alt+F12`**
  - TC action: Right breadcrumb bar
  - Cmdr command: `pane.focusControls`
  - Today off mac: differs
  - Proposed Windows: differs
  - Proposed Linux: differs
  - Reserved: –
  - Note: Closest, as above.
- **`Alt+Shift+F11`**
  - TC action: Focus the button bar
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Shift+F1`**
  - TC action: Custom columns view menu
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Shift+F2`**
  - TC action: Compare file lists
  - Cmdr command: `selection.compareDirectories`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Shift+F3`**
  - TC action: List only file under cursor
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Shift+F4`**
  - TC action: New text file, open in editor
  - Cmdr command: `file.newFile`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Shift+F5`**
  - TC action: Copy with rename in the same directory
  - Cmdr command: `file.duplicate`
  - Today off mac: differs
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Shift+F5`**
  - TC action: Create shortcuts
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Shift+F6`**
  - TC action: Rename in the same directory
  - Cmdr command: `file.rename`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Shift+F8 / Shift+Delete`**
  - TC action: Delete directly
  - Cmdr command: `file.deletePermanently`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Shift+F10`**
  - TC action: Context menu
  - Cmdr command: `file.contextMenu`
  - Today off mac: differs
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Shift+Esc`**
  - TC action: Minimize
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: OS minimize.
- **`Alt+← / Alt+→`**
  - TC action: Previous / next visited folder
  - Cmdr command: `nav.back / nav.forward`
  - Today off mac: differs
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Alt+←: windows WebView2 history back / forward (unverified); Alt+→: windows WebView2 history back /
    forward (unverified)
  - Note:
- **`Alt+↓`**
  - TC action: Folder history list
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Num +`**
  - TC action: Expand selection
  - Cmdr command: `selection.selectFiles`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Num -`**
  - TC action: Shrink selection
  - Cmdr command: `selection.deselectFiles`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Num *`**
  - TC action: Invert selection
  - Cmdr command: `selection.invert`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Num /`**
  - TC action: Restore selection
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Shift+Num + / - / *`**
  - TC action: Files-and-folders variants
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+Num +`**
  - TC action: Select all
  - Cmdr command: `selection.selectAll`
  - Today off mac: differs
  - Proposed Windows: differs
  - Proposed Linux: differs
  - Reserved: Ctrl++: windows WebView2 zoom (verified)
  - Note: Numpad + is "+" in the vocabulary, so this is Ctrl++ = zoom in.
- **`Ctrl+Shift+Num +`**
  - TC action: Select all (other mode)
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+Num -`**
  - TC action: Deselect all
  - Cmdr command: `selection.deselectAll`
  - Today off mac: differs
  - Proposed Windows: differs
  - Proposed Linux: differs
  - Reserved: Ctrl+-: windows WebView2 zoom (verified)
  - Note: Same numpad collision with zoom out.
- **`Ctrl+Shift+Num -`**
  - TC action: Deselect all files only
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+Num +`**
  - TC action: Select same extension
  - Cmdr command: `selection.selectSameKind`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Alt+Num -`**
  - TC action: Deselect same extension
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+PgUp / Backspace`**
  - TC action: Parent directory
  - Cmdr command: `nav.parent`
  - Today off mac: partial
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+\ / Ctrl+<`**
  - TC action: Root directory
  - Cmdr command: `nav.goToRoot`
  - Today off mac: differs
  - Proposed Windows: partial
  - Proposed Linux: partial
  - Reserved: –
  - Note:
- **`Ctrl+PgDn`**
  - TC action: Open directory or archive
  - Cmdr command: `nav.open`
  - Today off mac: differs
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+← / Ctrl+→`**
  - TC action: Open folder in target pane
  - Cmdr command: `pane.copyPathRightToLeft / pane.copyPathLeftToRight`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+F1`**
  - TC action: Brief view
  - Cmdr command: `view.briefMode`
  - Today off mac: differs
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Ctrl+F1: kde Switch to desktop 1–4 (verified)
  - Note:
- **`Ctrl+Shift+F1`**
  - TC action: Thumbnails view
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+F2`**
  - TC action: Full view
  - Cmdr command: `view.fullMode`
  - Today off mac: differs
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Ctrl+F2: kde Switch to desktop 1–4 (verified)
  - Note:
- **`Ctrl+Shift+F2`**
  - TC action: Comments view
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+F3`**
  - TC action: Sort by name
  - Cmdr command: `sort.byName`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Ctrl+F3: kde Switch to desktop 1–4 (verified)
  - Note:
- **`Ctrl+F4`**
  - TC action: Sort by extension
  - Cmdr command: `sort.byExtension`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Ctrl+F4: kde Switch to desktop 1–4 (verified)
  - Note:
- **`Ctrl+F5`**
  - TC action: Sort by date
  - Cmdr command: `sort.byModified`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+F6`**
  - TC action: Sort by size
  - Cmdr command: `sort.bySize`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+F7`**
  - TC action: Unsorted
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+F8`**
  - TC action: Directory tree
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+Shift+F8`**
  - TC action: Cycle tree states
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+F9`**
  - TC action: Print file
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+F10`**
  - TC action: Show all files
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: Not view.showHidden (that is hidden files).
- **`Ctrl+F11`**
  - TC action: Show only programs
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+F12`**
  - TC action: Show user-defined files
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Tab`**
  - TC action: Switch panes
  - Cmdr command: `pane.switch`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Shift+Tab`**
  - TC action: Switch to separate tree
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Insert`**
  - TC action: Select file
  - Cmdr command: `selection.toggleAndDown`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Space`**
  - TC action: Select file (counts a folder's size)
  - Cmdr command: `selection.toggle`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Enter`**
  - TC action: Open / run
  - Cmdr command: `nav.open`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Shift+Enter`**
  - TC action: Run with /c, alt archive handling
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Alt+Shift+Enter`**
  - TC action: Count all folder sizes
  - Cmdr command: `view.calculateFolderSizes`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Alt+Enter`**
  - TC action: Properties
  - Cmdr command: `file.getInfo`
  - Today off mac: unbound
  - Proposed Windows: unbound
  - Proposed Linux: unbound
  - Reserved: –
  - Note: No properties capability off macOS yet.
- **`Ctrl+A`**
  - TC action: Select all
  - Cmdr command: `selection.selectAll`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+B`**
  - TC action: Branch view
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+Shift+B`**
  - TC action: Selected branch view
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+C`**
  - TC action: Copy to clipboard
  - Cmdr command: `edit.copy`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+X`**
  - TC action: Cut to clipboard
  - Cmdr command: `edit.cut`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+V`**
  - TC action: Paste
  - Cmdr command: `edit.paste`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+D`**
  - TC action: Directory hotlist
  - Cmdr command: `favorites.open`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note: Today it matches on paper but loses to file.duplicate in the file list (the clash); the proposal fixes it.
- **`Ctrl+F`**
  - TC action: Connect to FTP
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: Cmdr keeps Ctrl+F for search; FTP connect is servers.connect (Ctrl+K).
- **`Ctrl+Shift+F`**
  - TC action: Disconnect FTP
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+I`**
  - TC action: Switch to target directory
  - Cmdr command: `pane.switch`
  - Today off mac: differs
  - Proposed Windows: differs
  - Proposed Linux: differs
  - Reserved: –
  - Note: TC's second key for the same act; Cmdr leaves Ctrl+I free.
- **`Ctrl+L`**
  - TC action: Occupied space of selection
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+M`**
  - TC action: Multi-rename tool
  - Cmdr command: `file.multiRename`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Shift+M`**
  - TC action: FTP transfer mode
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+N`**
  - TC action: New FTP connection
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: Cmdr Ctrl+N = Ask Cmdr new chat.
- **`Ctrl+P`**
  - TC action: Copy path to command line
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: No command line; Ctrl+P is also WebView2 print.
- **`Ctrl+Q`**
  - TC action: Quick view panel
  - Cmdr command: `file.quickLook`
  - Today off mac: unbound
  - Proposed Windows: unbound
  - Proposed Linux: unbound
  - Reserved: –
  - Note: No preview capability off macOS.
- **`Ctrl+R`**
  - TC action: Reread
  - Cmdr command: `pane.refresh`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: Ctrl+R: windows WebView2 reload (verified)
  - Note:
- **`Ctrl+S`**
  - TC action: Quick filter
  - Cmdr command: `pane.quickFilter`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Shift+S`**
  - TC action: Quick filter, last filter
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+T`**
  - TC action: New tab
  - Cmdr command: `tab.new`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Shift+T`**
  - TC action: New tab in background
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: Cmdr Ctrl+Shift+T = reopen closed tab (browser convention).
- **`Ctrl+U`**
  - TC action: Exchange directories
  - Cmdr command: `pane.swap`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Shift+U`**
  - TC action: Exchange directories and tabs
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+W`**
  - TC action: Close tab
  - Cmdr command: `tab.close`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Shift+W`**
  - TC action: Close all tabs
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+Z`**
  - TC action: Edit comment
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+↑`**
  - TC action: Open folder in new tab
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: Today Cmdr's Ctrl+↑ is Go to parent (mechanical ⌘↑).
- **`Ctrl+Shift+↑`**
  - TC action: Open folder in other pane, new tab
  - Cmdr command: no equivalent
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note:
- **`Ctrl+Tab`**
  - TC action: Next tab
  - Cmdr command: `tab.next`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Shift+Tab`**
  - TC action: Previous tab
  - Cmdr command: `tab.prev`
  - Today off mac: match
  - Proposed Windows: match
  - Proposed Linux: match
  - Reserved: –
  - Note:
- **`Ctrl+Alt+letters`**
  - TC action: Quick search
  - Cmdr command: `(type-to-jump, bare letters)`
  - Today off mac: -
  - Proposed Windows: -
  - Proposed Linux: -
  - Reserved: –
  - Note: Cmdr jumps on bare letters (TC's optional "Letters" mode); Ctrl+Alt is AltGr on Windows.

From memory, not in the transcription (unverified): `Ctrl+Enter` copies the file name to the command line,
`Ctrl+Shift+Enter` the path and name (no command line in Cmdr either way). TC 9 to 11 added more defaults that this
survey could not read.

### Cmdr commands with a default but no TC key (37)

`app.commandPalette`, `app.settings`, `queue.show`, `log.operationLog`, `askCmdr.toggle`, `nav.goToPath`,
`downloads.goToLatest`, `view.showHidden`, `view.zoom.set100`, `view.zoom.in`, `view.zoom.out`, `pane.clone`,
`app.focusNotifications`, `pane.moveDividerLeft`, `pane.moveDividerRight`, `tab.reopen`, `tab.moveLeft`,
`tab.moveRight`, `askCmdr.newChat`, `nav.home`, `nav.end`, `nav.pageUp`, `nav.pageDown`, `nav.goHome`,
`edit.pasteAsMove`, `file.showInFinder`, `file.openTerminalHere`, `file.copyPath`, `servers.edit`, `about.close`,
`viewer.toggleWordWrap`, `viewer.toggleTail`, `viewer.zoomIn`, `viewer.zoomOut`, `viewer.toggleImageZoom`,
`viewer.goBack`, `viewer.goForward`. Plus every fixed-key sheet, palette, queue, and viewer key, which TC has no direct
counterpart for (TC's Lister uses 1/2/3 for text/binary/hex too, which Cmdr's viewer already copies).

### The counts

- **107 TC rows; 58 map to a Cmdr command; 49 have no Cmdr feature** (FTP, archives as actions, command line, trees,
  thumbnails, comments, branch view, print, file filters).
- **Today off macOS** (Linux, and Windows identically): 41 of the 58 match, but `Ctrl+D` is dead in the file list, so 40
  really work; 1 partial (`Ctrl+PgUp/Backspace`: Backspace only), 14 differ, 2 unbound.
- **Proposed Windows default: 48 match, 1 partial (`Ctrl+<` for European keyboards not added), 6 differ, 3 unbound.**
  The 8 rows that change are `Shift+F5`, `Shift+F10`, `Alt+←/→`, `Ctrl+PgUp`, `Ctrl+PgDn`, `Ctrl+F1`, `Ctrl+F2`,
  `Ctrl+\`, plus `Ctrl+D` actually working. Nothing Windows itself reserves stands in the way (verified list), but five
  matches (`F3`, `F5`, `Ctrl+R`, `Alt+←/→`) are WebView2 browser accelerators that must be proven suppressed.
- **The 9 that stay different by design or for lack of a feature:** `F2` (Cmdr keeps Explorer's Rename; TC's reread is
  `Ctrl+R`, which matches), `Ctrl+Num +`/`Ctrl+Num -` (the vocabulary can't tell numpad + from Ctrl++ zoom), `Alt+F4`
  (the OS closes the window, which is what TC does too), `Alt+Enter` and `Ctrl+Q` (no properties or preview capability
  off macOS yet), `Ctrl+I` (TC's second pane-switch key, free in Cmdr, could be added), `Alt+F11`/`Alt+F12` (breadcrumb
  bars, only loosely `pane.focusControls`).
- **Proposed Linux default: the same 48 on paper.** Effective on stock GNOME: 45 (Alt+F2 run dialog, Alt+F5 restore
  window, Alt+F7 move window are swallowed, verified). Effective on stock KDE Plasma: 44 verified (Ctrl+F1–F4 switch
  desktops, verified in KWin), likely 42 (Alt+F1 Kickoff and Alt+F2 KRunner, unverified). The proposal gives the volume
  choosers a second key (`Ctrl+Shift+F1/F2`) and keeps `Ctrl+1/2/3/4` first for view and sort, so no feature loses its
  only key; Alt+F5 compress is the one that does on GNOME (menu and palette remain).
- **Windows can be fully TC-faithful for every TC key Cmdr has a feature for; Linux cannot**, because GNOME and KDE grab
  Alt+F2/F5/F7 and Ctrl+F1–F4 at the compositor, before any app sees them.

### What TC-likes on Linux do (verified for Double Commander only)

- Double Commander keeps TC's keys on Linux and lets the desktop win: its documented defaults are Alt+F1, Alt+F2,
  Alt+F5, Alt+F7, Alt+F9, Ctrl+F1–F6 exactly as TC (https://doublecmd.github.io/doc/en/shortcuts.html). It adds
  Ctrl+Shift+Home for home, Ctrl+. for hidden files, Alt+Z for target = source, Ctrl+K for network drive.
- Krusader ships a separate "Total Commander" keymap file rather than making TC its default
  (`total_commander.keymap.info` in its sources, per search result
  https://lxr.kde.org/source/utilities/krusader/app/total_commander.keymap.info; not opened). Its own defaults:
  unverified.
- Midnight Commander lives in a terminal, where Alt+F-keys usually reach it only if the terminal does not pass them to
  the desktop; it offers Esc-digit as the F-key fallback (from memory, unverified).

## 5. Proposed per-platform default keymap (changes only)

Full row-per-command data with notes: the JSON. Status words: `same`, `changed-tc` (match Total Commander),
`changed-reserved` (avoid an OS or desktop reservation), `changed-clash` (fix an internal clash), `changed-convention`
(Windows/Linux convention or off-mac bug), `unbound` (no default, with the reason).

| Command                            | macOS (unchanged)               | Today off mac                                    | Windows                             | Status             | Linux                               | Status             | TC                     |
| ---------------------------------- | ------------------------------- | ------------------------------------------------ | ----------------------------------- | ------------------ | ----------------------------------- | ------------------ | ---------------------- |
| `app.quit`                         | `⌘Q`                            | `Ctrl+Q`                                         | –                                   | unbound            | –                                   | unbound            | Alt+F4                 |
| `app.hide`                         | `⌘H`                            | `Ctrl+H`                                         | –                                   | unbound            | –                                   | unbound            | –                      |
| `app.hideOthers`                   | `⌥⌘H`                           | `Alt+Ctrl+H`                                     | –                                   | unbound            | –                                   | unbound            | –                      |
| `favorites.open`                   | `⌃D`                            | `Ctrl+D`                                         | `Ctrl+D`                            | changed-clash      | `Ctrl+D`                            | changed-clash      | Ctrl+D                 |
| `view.briefMode`                   | `⌘2`                            | `Ctrl+2`                                         | `Ctrl+F1`, `Ctrl+2`                 | changed-tc         | `Ctrl+2`, `Ctrl+F1`                 | changed-tc         | Ctrl+F1                |
| `view.fullMode`                    | `⌘1`                            | `Ctrl+1`                                         | `Ctrl+F2`, `Ctrl+1`                 | changed-tc         | `Ctrl+1`, `Ctrl+F2`                 | changed-tc         | Ctrl+F2                |
| `pane.leftVolumeChooser`           | `⌥F1`                           | `Alt+F1`                                         | `Alt+F1`                            | same               | `Alt+F1`, `Ctrl+Shift+F1`           | changed-reserved   | Alt+F1                 |
| `pane.rightVolumeChooser`          | `⌥F2`                           | `Alt+F2`                                         | `Alt+F2`                            | same               | `Alt+F2`, `Ctrl+Shift+F2`           | changed-reserved   | Alt+F2                 |
| `pane.moveDividerLeft`             | `⌃⌥←`                           | `Ctrl+Alt+Shift+←`                               | `Alt+Shift+←`                       | same               | `Alt+Shift+←`                       | changed-reserved   | –                      |
| `pane.moveDividerRight`            | `⌃⌥→`                           | `Ctrl+Alt+Shift+→`                               | `Alt+Shift+→`                       | same               | `Alt+Shift+→`                       | changed-reserved   | –                      |
| `tab.next`                         | `⌃Tab`, `⌘⌥→`                   | `Ctrl+Tab`, `Ctrl+Alt+→`                         | `Ctrl+Tab`, `Ctrl+Alt+→`            | same               | `Ctrl+Tab`                          | changed-reserved   | Ctrl+Tab               |
| `tab.prev`                         | `⌃⇧Tab`, `⌘⌥←`                  | `Ctrl+Shift+Tab`, `Ctrl+Alt+←`                   | `Ctrl+Shift+Tab`, `Ctrl+Alt+←`      | same               | `Ctrl+Shift+Tab`                    | changed-reserved   | Ctrl+Shift+Tab         |
| `tab.moveLeft`                     | `⌘⌃⇧←`                          | `Ctrl+Shift+←`                                   | `Ctrl+Shift+PageUp`                 | changed-clash      | `Ctrl+Shift+PageUp`                 | changed-clash      | –                      |
| `tab.moveRight`                    | `⌘⌃⇧→`                          | `Ctrl+Shift+→`                                   | `Ctrl+Shift+PageDown`               | changed-clash      | `Ctrl+Shift+PageDown`               | changed-clash      | –                      |
| `nav.open`                         | `Enter`, `⌘↓`                   | `Enter`, `Ctrl+↓`                                | `Enter`, `Ctrl+PageDown`            | changed-tc         | `Enter`, `Ctrl+PageDown`            | changed-tc         | Enter, Ctrl+PgDn       |
| `nav.parent`                       | `Backspace`, `⌘↑`               | `Backspace`, `Ctrl+↑`                            | `Backspace`, `Alt+↑`, `Ctrl+PageUp` | changed-tc         | `Backspace`, `Alt+↑`, `Ctrl+PageUp` | changed-tc         | Backspace, Ctrl+PgUp   |
| `nav.home`                         | `⌥↑`, `Home`                    | `Alt+↑`, `Home`                                  | `Home`                              | changed-clash      | `Home`                              | changed-clash      | –                      |
| `nav.end`                          | `⌥↓`, `End`                     | `Alt+↓`, `End`                                   | `End`                               | changed-clash      | `End`                               | changed-clash      | –                      |
| `nav.back`                         | `⌘[`                            | `Ctrl+[`                                         | `Alt+←`                             | changed-tc         | `Alt+←`                             | changed-tc         | Alt+←                  |
| `nav.forward`                      | `⌘]`                            | `Ctrl+]`                                         | `Alt+→`                             | changed-tc         | `Alt+→`                             | changed-tc         | Alt+→                  |
| `nav.goToRoot`                     | `⌘/`                            | `Ctrl+/`                                         | `Ctrl+\`, `Ctrl+/`                  | changed-tc         | `Ctrl+\`, `Ctrl+/`                  | changed-tc         | Ctrl+\                 |
| `errorPane.toggleTechnicalDetails` | `⌘D`                            | `Ctrl+D`                                         | `Ctrl+Shift+D`                      | changed-clash      | `Ctrl+Shift+D`                      | changed-clash      | –                      |
| `file.duplicate`                   | `⌘D`                            | `Ctrl+D`                                         | `Shift+F5`                          | changed-clash      | `Shift+F5`                          | changed-clash      | Shift+F5               |
| `file.delete`                      | `F8`, `⌘Backspace`, `Delete`    | `F8`, `Ctrl+Backspace`, `Delete`                 | `F8`, `Delete`                      | changed-convention | `F8`, `Delete`                      | changed-convention | F8, Delete             |
| `file.deletePermanently`           | `⇧F8`, `⇧Delete`, `⌘⌥Backspace` | `Shift+F8`, `Shift+Delete`, `Ctrl+Alt+Backspace` | `Shift+F8`, `Shift+Delete`          | changed-convention | `Shift+F8`, `Shift+Delete`          | changed-reserved   | Shift+F8, Shift+Delete |
| `file.contextMenu`                 | `⌃Enter`                        | `Ctrl+Enter`                                     | `Shift+F10`, `Ctrl+Enter`           | changed-convention | `Shift+F10`, `Ctrl+Enter`           | changed-convention | Shift+F10              |
| `share.back`                       | `Backspace`, `Escape`, `⌘↑`     | `Backspace`, `Escape`, `Ctrl+↑`                  | `Backspace`, `Escape`, `Alt+↑`      | changed-convention | `Backspace`, `Escape`, `Alt+↑`      | changed-convention | –                      |
| `viewer.goBack`                    | `⌘[`                            | `Ctrl+[`                                         | `Alt+←`                             | changed-convention | `Alt+←`                             | changed-convention | –                      |
| `viewer.goForward`                 | `⌘]`                            | `Ctrl+]`                                         | `Alt+→`                             | changed-convention | `Alt+→`                             | changed-convention | –                      |

The proposal retires the ⌃→Shift hack: its only live outputs (`tab.moveLeft/Right`) get explicit keys, and
`app.hideOthers` is macOS-native. With explicit per-platform arrays, `toPlatformShortcut` reduces to glyph-to-word
renaming.

Rust must move with it: the non-mac menu bar hardcodes the old chords (`crates/cmdr-menu/src/bar.rs`, snapshot
`bar_test.rs`: `go_parent [Ctrl+Up]`, `go_back [Ctrl+[]`, `go_forward [Ctrl+]]`, `favorites_open [Ctrl+D]`,
`next_tab`…), and `menu/accelerators.rs` applies the same ⌃→Shift rule (`apps/desktop/src-tauri/src/menu/DETAILS.md`
"Accelerators"). Windows currently gets the Linux bar, so a Windows-only default needs a third `Platform` arm or a
startup push of the defaults.

## 6. Is a Total Commander preset easy?

- Today there are no presets, no import or export, and no keymap setting. Customizations are deltas in `shortcuts.json`
  against the registry defaults (`apps/desktop/src/lib/shortcuts/CLAUDE.md`).
- Defaults are resolved in exactly two functions (`getDefaultShortcuts`, `getEffectiveShortcuts`,
  `shortcuts-store.ts:449-469`) plus the Rust menu defaults and `get_restricted_window_shortcuts`
  (`apps/desktop/src-tauri/src/settings/restricted_shortcuts.rs`, which sends only customizations). A preset is "a
  second defaults table keyed by command id, chosen by a setting", read at those two seams.
- Costs: (1) deltas are relative to the base, so switching presets under existing customizations needs a rule (keep
  deltas; a delta equal to the new base gets cleaned by `cleanupIfMatchesDefaults`); (2) `resyncMenuAccelerators` pushes
  only customized ids, so it would have to push every id whose preset default differs from the Rust bar default; (3) the
  registry conflict and vocabulary tests must run per preset × platform. Estimate: S for the data, M with menus, tests,
  and a Settings picker.
- The shortcut editor already has a "system shortcut" soft-warning table for macOS
  (`apps/desktop/src/lib/settings/sections/keyboard-shortcuts-banner.ts:101-116`, `classifySystemShortcut`); the
  Windows, GNOME, and KDE tables in §3 can go in beside it, keyed per platform, and the same data can drive the
  OS-reserved-combo test task 26 asks for.

## 7. Options for David

1. **Mechanical mapping plus fixes** (smallest): fix Ctrl+D, retire the hack, fix the bugs in §3, drop the
   GNOME-reserved Ctrl+Alt arrows. Leaves 40 of 58 TC keys working.
2. **TC-faithful default off macOS** (this proposal): explicit `windows` and `linux` arrays per command; 48 of 58 on
   Windows, 45 on GNOME, about 42–44 on KDE, with Cmdr's own keys kept as second bindings so nothing loses its only key.
   Matches the product story (README: "a Total Commander-like experience") and what Double Commander does.
3. **Option 1 as the default plus an opt-in "Total Commander" preset**: most flexible, most moving parts (§6), and the
   default still disappoints the TC migrants who are most of the Windows audience.

## 8. Open questions

- TC-faithful by default on Windows and Linux (option 2), or a preset (option 3)?
- Drop the mechanical Ctrl+↑ = Go to parent (TC uses Ctrl+↑ for "open folder in new tab"), and give Alt+↑ to Go to
  parent by taking it off Go to first item (Home stays)?
- Add a Quit command off macOS? Linux convention is Ctrl+Q; Windows relies on Alt+F4. (Ctrl+Q is also TC's quick view.)
- Linux: accept that Alt+F5 (compress), Alt+F2 (right volume), Alt+F7 (search) are dead on stock GNOME, with second keys
  only where proposed (Ctrl+Shift+F1/F2 for the volume choosers)? Or pick a second key for compress too?
- AltGr on Windows: fix in the matcher (skip the physical-key fallback when `getModifierState("AltGraph")`), or move the
  Multi-rename option keys off Ctrl+Alt+letter?
- Turn WebView2 browser accelerator keys off for every window (needs the Windows `with_webview` controller call, since
  Tauri 2.12.5 has no setter)? This also decides whether F5/Ctrl+R/Ctrl+P are safe in Settings and the viewer.
- Keep F2 = Rename (Explorer and current Cmdr) rather than TC's F2 = reread? The proposal keeps it.
- Add TC's Ctrl+I (switch to target pane) and Ctrl+< (root on European keyboards)?
- Numpad vocabulary: name numpad keys distinctly (for example `Num+`) so Ctrl+Num + / Ctrl+Num - can be select all /
  deselect all without colliding with zoom?
- Error screen's details toggle off macOS: Ctrl+Shift+D OK?
- Windows `tab.next/prev`: keep Ctrl+Alt+→/← (works except on some Intel drivers, unverified) or drop it like Linux for
  one shared keymap?
- Search dialog: its ⌘N / ⌘H / ⌘1–9 are unreachable off macOS today (Super combos); move them into the registry so they
  map like everything else?

## 9. What could not be verified

- A current TC 11 `KEYBOARD.TXT` or help file (all TC hosts refused by the sandbox proxy; secondary TC 8 transcriptions
  used).
- Whether `preventDefault` in the page cancels WebView2 browser accelerators; F7 caret browsing, Alt+←/→, Ctrl+G
  behavior in WebView2.
- WebKitGTK built-in accelerators; GTK F10 menu activation; whether GTK or Win32 menu accelerators run before the
  webview for Ctrl chords (decides who wins `Ctrl+D` today).
- KDE: Alt+F1 (Kickoff), Alt+F2/Alt+Space (KRunner), Alt+Tab, Ctrl+Esc, Print (sources not fetched); whether KWin grabs
  Ctrl+F1–F4 when only one desktop exists.
- GNOME/Ubuntu extras: Ctrl+Alt+T, Ctrl+Alt+Delete, Ctrl+Alt+F-keys VT switch, Ubuntu changes to Alt+F1.
- Windows: AltGr = Ctrl+Alt in WebView2 key events; Alt+Shift / Ctrl+Shift layout switching; PrintScreen;
  Ctrl+Alt+arrows on Intel drivers; Alt+letter mnemonics on the muda Win32 menu.

## Sources read

- https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in
  (fetched 2026-11-11)
- https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/data/org.gnome.shell.gschema.xml.in (fetched 2026-11-11)
- https://gitlab.gnome.org/GNOME/mutter/-/raw/main/data/org.gnome.mutter.gschema.xml.in (fetched 2026-11-11)
- https://raw.githubusercontent.com/KDE/kwin/master/src/useractions.cpp (fetched 2026-11-11)
- https://raw.githubusercontent.com/KDE/kwin/master/src/virtualdesktops.cpp (fetched 2026-11-11)
- https://raw.githubusercontent.com/KDE/kwin/master/src/plugins/windowview/windowvieweffect.cpp (fetched 2026-11-11)
- https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
  (fetched 2026-11-11, page dated 2026-09-02)
- https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.arebrowseracceleratorkeysenabled
  (fetched 2026-11-11)
- https://softpanorama.org/OFM/Total_commander/keyboard_map.shtml (TC KEYBOARD.TXT transcription, page last modified
  2019-03-12)
- https://www.scribd.com/document/351942143/112-Shortcuts-for-Total-Commander-8 (same, TC 8)
- https://doublecmd.github.io/doc/en/shortcuts.html (Double Commander defaults)
- Local crate sources: ~/.cargo/registry tauri-2.12.5, tauri-utils-2.10.2, wry-0.57.0, muda-0.20.0 (versions from
  Cargo.lock)

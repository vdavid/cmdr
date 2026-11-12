# Per-platform default keymap: decisions (task 26, #1102)

David's decisions from the #1102 comment (2026-11-12) and his later additions, checked against primary sources on
2026-11-12. This is the table that shipped: the registry's Windows and Linux arms (`apps/desktop/src/lib/commands/`,
pinned by `__snapshots__/default-keymap.{macos,windows,linux}.txt`) follow it, and where it and `keymap-proposal.json`
or `keymap-survey.md` disagree, this file wins. Registry line references are from `5135d6d238`, the commit the work
started from.

Total Commander's own key list (11.57's `KEYBOARD.TXT`, the source behind every "TC" below) can't be redistributed, so
it isn't in the repo: it's David's vault note `projects/Cmdr/research/Total Commander keyboard reference.md`, local
only. The research reports behind this file (`code-report.md`, `os-report.md`, `tc-report.md`) are in the main clone's
`_ignored/keymap-research/`, also local only.

## Settled with David

1. **The viewer's search toggles** were hardcoded `Ctrl+Alt+R` and `Ctrl+Alt+C` off macOS, which breaks the "no
   `Ctrl+Alt+<letter>` off macOS" rule: AltGr+C types `&` on Hungarian and Czech, AltGr+R types `®` on US-International.
   Shipped: `Alt+R` and `Alt+C` on Windows and Linux, macOS `⌘⌥R` / `⌘⌥C`, as the fixed registry commands
   `viewer.toggleSearchRegex` / `viewer.toggleSearchCase`. VS Code's find widget uses exactly this split
   (`Alt+R`/`Alt+C`, macOS `⌥⌘R`/`⌥⌘C`; verified, `microsoft/vscode` `src/vs/editor/contrib/find/browser/findModel.ts`
   at `527b5f320f`, 2026-11-12). Same reasoning backs the Multi-rename `Alt+<letter>` keys.
2. **The search dialog's commands (decision 12)** would clash in the scope model on every OS: `search.newQuery` meets
   `askCmdr.newChat`, and `search.modeByNumber` meets `view.fullMode`, `view.briefMode`, and `sort.byName`, if the
   scope's chain includes `Main window`. Shipped: the keys as decided, in a `Main window/Search dialog` scope whose
   chain skips `Main window`, the way `Main window/Ask Cmdr` does (its shadowing is deliberate and the keys are fixed).
3. **Multi-rename `Alt+F`, `Alt+E`, `Alt+G` vs future Windows menu access keys.** Open, with task 63. On Linux the GTK
   mnemonics are masked while the sheet is up (§ Reservations behind the Linux keys, "Menu bar"); Windows has no access
   keys today. If task 63 gives the Windows bar first-letter access keys (File, Edit, Select, View, Go, Tab, Help; the
   Window menu is macOS-only), three option keys collide while the sheet is open. Keys stay as decided; task 63 picks
   access letters around them, or the sheet claims Alt+letter first (whether WebView2 lets a page's `preventDefault`
   stop a Win32 menu access key is unverified).

No decided key had to change. Two keys here are new picks: Compress's Linux second key (`Alt+Shift+P`, § Reservations
behind the Linux keys) and the viewer toggles above.

## The rule

- Windows and Linux follow Total Commander. A native key (File Explorer on Windows; Nautilus, Dolphin, Nemo, Thunar on
  Linux) joins where it doesn't clash with a TC key. No Finder-style keys off macOS.
- No `Ctrl+Alt+<letter>` defaults off macOS. Only `H` is free of AltGr characters across Hungarian, German, Polish
  Programmers, Czech, US-International, Swedish, and French (verified, Microsoft's KLC tables via kbdlayout.info,
  2026-11-12, local `os-report.md`). Ctrl+Alt with arrows or F-keys is fine: those type nothing. One decided exception
  outside the registry: the global go-to-latest hotkey defaults to `Ctrl+Alt+Super+J` (`Ctrl+Alt+Win+J` on Windows),
  macOS's `⌃⌥⌘J` spelled natively. The Windows / Super key in it makes it a chord no AltGr layout types (David,
  2026-11-12).
- macOS stays as it is, plus the additions below.
- "Mechanical" below means what the old one-way converter made of the macOS keys: ⌘ and ⌃ became Ctrl, ⌥ Alt, ⇧ Shift
  (and ⌃ became Shift when ⌘ was also held). The converter is gone; every command not listed in § Changed commands holds
  those same keys, written out per OS in its source (`cmdOrCtrl`, `sameOnAll`, or a literal), including `app.settings`
  (`Ctrl+,` only), `pane.quickFilter` (`Ctrl+S`, TC's key too), `favorites.open` (`Ctrl+D`, TC's hotlist),
  `file.multiRename` (`Ctrl+M`), and `search.open` (`Ctrl+F`, `Alt+F7`).
- Combos are written in the registry's canonical off-macOS form (`Ctrl+Alt+Shift+Key`, `PageUp`, arrows as glyphs). The
  Windows / Super key is stored `Super` on both; Windows shows it as `Win` (`docs/style-guide.md` § Platform
  vocabulary).

## Changed commands

Registry order, primary key first. W = Windows, L = Linux. Only commands that exist off macOS.

- `app.quit` (Quit Cmdr): macOS `⌘Q`; W none; L none. Needs a real handler and a palette and menu row off macOS (#1110),
  but no key Cmdr binds: `Alt+F4` already closes the window through the quit gate on Windows and on all five Linux
  desktops (each binds `Alt+F4` to close, verified below). The File menu row shows no accelerator, not even an `Alt+F4`
  hint (the Windows menu model is task 63's). `Ctrl+Q` stays free for a future quick view (TC).
- `queue.show` (Operation queue): macOS `⌘⌥Q`; W `Ctrl+Shift+Q`; L `Ctrl+Shift+Q`. No Ctrl+Alt letter. Note: TC 11.57
  uses `Ctrl+Shift+Q` for its separate quick-view window, so Preview off macOS can't take it later.
- `log.operationLog` (Operation log): macOS `⌘⌥L`; W `Ctrl+Shift+L`; L `Ctrl+Shift+L`. No Ctrl+Alt letter (also
  Cinnamon's lock key).
- `askCmdr.toggle` (Ask Cmdr): macOS `⌘⌥A`; W `Ctrl+Shift+K`; L `Ctrl+Shift+K`. No Ctrl+Alt letter.
- `view.briefMode` (Switch to Brief view): macOS `⌘2`; W `Ctrl+F1`, `Ctrl+2`; L `Ctrl+2`, `Ctrl+F1`. TC's key; Linux
  keeps `Ctrl+2` first because KDE and Xfce take `Ctrl+F1`.
- `view.fullMode` (Switch to Full view): macOS `⌘1`; W `Ctrl+F2`, `Ctrl+1`; L `Ctrl+1`, `Ctrl+F2`. Same.
- `pane.switch` (Switch pane): macOS `Tab`; W `Tab`, `Ctrl+I`; L `Tab`, `Ctrl+I`. TC's "switch to target directory".
  `Ctrl+I` is free only because `file.getInfo` doesn't exist off macOS; a future Properties command takes `Alt+Enter`
  (Explorer, all four Linux file managers), never `Ctrl+I`.
- `pane.clone` (Clone pane): macOS `⌘⇧C`; W `Ctrl+Shift+M`; L `Ctrl+Shift+M`. Frees `Ctrl+Shift+C` for Copy path.
- `pane.focusControls` (Move focus to the pane controls): macOS `⌃F5`; W `Ctrl+Shift+F5`; L `Ctrl+Shift+F5`. Mechanical
  `Ctrl+F5` is sort by modified. TC uses `Ctrl+Shift+F5` for "create shortcuts"; no Cmdr equivalent.
- `app.focusNotifications` (Move focus to the notifications): macOS `⌃F6`; W `Ctrl+Shift+F6`; L `Ctrl+Shift+F6`.
  Mechanical `Ctrl+F6` is sort by size.
- `pane.leftVolumeChooser` (Open left volume switcher): macOS `⌥F1`; W `Alt+F1`; L `Alt+F1`, `Ctrl+Shift+F1`, `Alt+1`.
  `Alt+F1` is dead on KDE, Xfce, and Cinnamon.
- `pane.rightVolumeChooser` (Open right volume switcher): macOS `⌥F2`; W `Alt+F2`; L `Alt+F2`, `Ctrl+Shift+F2`, `Alt+2`.
  `Alt+F2` is dead on all five desktops. (TC's `Ctrl+Shift+F1`/`F2` are thumbnails and comments views; Cmdr has
  neither.)
- `pane.moveDividerLeft` (Move pane divider left): macOS `⌃⌥←`; W `Alt+Shift+←`; L `Alt+Shift+←`. Mechanical
  `Ctrl+Alt+←` is the tab key on Windows and a workspace switch on GNOME, Xfce, and Cinnamon. Midnight Commander uses
  the same keys for its split.
- `pane.moveDividerRight` (Move pane divider right): macOS `⌃⌥→`; W `Alt+Shift+→`; L `Alt+Shift+→`. Same.
- `tab.next` (Next tab): macOS `⌃Tab`, `⌘⌥→`; W `Ctrl+Tab`, `Ctrl+Alt+→` (mechanical, kept by decision 11); L
  `Ctrl+Tab`. GNOME, Xfce, and Cinnamon take `Ctrl+Alt+→`.
- `tab.prev` (Previous tab): macOS `⌃⇧Tab`, `⌘⌥←`; W `Ctrl+Shift+Tab`, `Ctrl+Alt+←`; L `Ctrl+Shift+Tab`. Same.
- `tab.moveLeft` (Move tab left): macOS `⌘⌃⇧←`; W `Ctrl+Shift+PageUp`; L `Ctrl+Shift+PageUp`. Mechanical `Ctrl+Shift+←`
  is "select word" in text fields. Nautilus, Nemo, and Krusader use these.
- `tab.moveRight` (Move tab right): macOS `⌘⌃⇧→`; W `Ctrl+Shift+PageDown`; L `Ctrl+Shift+PageDown`. Same.
- `nav.open` (Open file or folder): macOS `Enter`, `⌘↓`; W `Enter`, `Ctrl+PageDown`; L same. TC's key; drops `Ctrl+↓`.
- `nav.parent` (Go to parent folder): macOS `Backspace`, `⌘↑`; W `Backspace`, `Alt+↑`, `Ctrl+PageUp`; L same. `Alt+↑` is
  Explorer's and all four Linux file managers' key (TC has none); `Ctrl+PageUp` is TC's. `Ctrl+↑` stays unbound (TC:
  open folder in a new tab).
- `nav.home` (Go to first file): macOS `⌥↑`, `Home`; W `Home`; L `Home`. Gives `Alt+↑` to parent.
- `nav.end` (Go to last file): macOS `⌥↓`, `End`; W `End`; L `End`. Symmetry.
- `nav.back` (Go back): macOS `⌘[`, plus `⌥←` (addition below); W `Alt+←`; L `Alt+←`. TC, Explorer, all four Linux file
  managers. `Ctrl+[` needs AltGr on German and Nordic layouts.
- `nav.forward` (Go forward): macOS `⌘]`, plus `⌥→`; W `Alt+→`; L `Alt+→`. Same.
- `nav.goToRoot` (Go to root folder): macOS `⌘/`; W `Ctrl+\`, `Ctrl+<`, `Ctrl+/`; L same. TC's US and European root
  keys. `Ctrl+<` is the `IntlBackslash` key by position (it types `<` on German and Nordic, `í` on Hungarian, `\` on UK;
  US ANSI keyboards don't have it).
- `errorPane.toggleTechnicalDetails` (Show technical details): macOS `⌘D`; W `Ctrl+Shift+D`; L `Ctrl+Shift+D`.
  Mechanical `Ctrl+D` is Favorites. Free in TC 11.57 and on all five desktops.
- `file.duplicate` (Duplicate): macOS `⌘D`; W `Shift+F5`; L `Shift+F5`. TC's "copy in the same directory"; frees
  `Ctrl+D` for Favorites.
- `file.compress` (Compress): macOS `⌥F5`; W `Alt+F5`; L `Alt+F5`, `Alt+Shift+P`. `Alt+F5` is dead on GNOME and
  Cinnamon; `Alt+Shift+P` is Krusader's pack key and free on all five.
- `edit.pasteAsMove` (Move here): macOS `⌘⌥V`; W `Ctrl+Shift+V`; L `Ctrl+Shift+V`. No Ctrl+Alt letter (AltGr+V is `@` on
  Hungarian and Czech). In a text field it's "paste as plain text" on Windows (Microsoft's list) and in Chromium and
  WebKitGTK, so it's in the native text keys below, the bar registers no accelerator for it there, and
  `edit.pasteAsMove`'s handler returns while a text field has focus, on every OS and from every road.
- `file.delete` (Delete): macOS `F8`, `⌘Backspace`, `Delete`; W `F8`, `Delete`; L same. `Ctrl+Backspace` is "delete
  word" in text fields.
- `file.deletePermanently` (Delete permanently): macOS `⇧F8`, `⇧Delete`, `⌘⌥Backspace`; W `Shift+F8`, `Shift+Delete`; L
  same. Drops the `Ctrl+Alt+Backspace` third key with its `Ctrl+Backspace` sibling.
- `file.showInFinder` (Show in file manager): macOS `⌘⌥O`; W `Ctrl+Shift+E`; L `Ctrl+Shift+E`. No Ctrl+Alt letter.
  Explorer's own `Ctrl+Shift+E` expands its navigation tree, which Cmdr doesn't have. IBus took `Ctrl+Shift+E` for emoji
  in 1.5.20–1.5.25 only, older than every distro Cmdr supports (below).
- `file.copyPath` (Copy path to clipboard): macOS `⌘⌥C`; W `Ctrl+Shift+C`; L `Ctrl+Shift+C`. Explorer's "Copy as path"
  (verified, Windows Insider build 22543 notes, 2022-01-27:
  https://blogs.windows.com/windows-insider/2022/01/27/announcing-windows-11-insider-preview-build-22543/ ; not in
  Microsoft's current shortcut list). WebView2's DevTools inspect key until #1115 lands.
- `file.contextMenu` (Open context menu): macOS `⌃Enter`; W `Shift+F10`, `Ctrl+Enter`; L same. TC, Explorer, GTK, Qt.
- `multiRename.letterCase` (Open the letter case menu): macOS `⌘⌥U`; W `Alt+U`; L `Alt+U`.
- `multiRename.removeDiacritics` (Turn Remove diacritics on or off): macOS `⌘⌥N`; W `Alt+N`; L `Alt+N`.
- `multiRename.greekToLatin` (Turn Greek to Latin on or off): macOS `⌘⌥G`; W `Alt+G`; L `Alt+G`.
- `multiRename.normalizeUnicode` (Turn Normalize Unicode on or off): macOS `⌘⌥P`; W `Alt+P`; L `Alt+P`.
- `multiRename.matchCase` (Turn Match case on or off): macOS `⌘⌥I`; W `Alt+I`; L `Alt+I`.
- `multiRename.firstMatchOnly` (Turn First match only on or off): macOS `⌘⌥F`; W `Alt+F`; L `Alt+F`.
- `multiRename.includeExtension` (Turn Include extension on or off): macOS `⌘⌥E`; W `Alt+E`; L `Alt+E`.
- `multiRename.regex` (Turn Regular expression on or off): macOS `⌘⌥R`; W `Alt+R`; L `Alt+R`.
- `multiRename.replaceWholeName` (Turn Replace whole name on or off): macOS `⌘⌥W`; W `Alt+W`; L `Alt+W`.
- `multiRename.insertField` (Insert a photo, video, or music field): macOS `⌘⌥X`; W `Alt+X`; L `Alt+X`.
- `multiRename.undoRename` (Undo the last multi-rename): macOS `⌘⌥Z`; W `Alt+Z`; L `Alt+Z`. All 11: the Windows
  access-key style, and AltGr+letter (masks need `[` and `]`, AltGr+F/G on Hungarian and Czech) stops toggling options.
  The 11 letters are distinct and no Main window or App command binds an Alt+letter off macOS.
- `share.back` (Back to host list): macOS `Backspace`, `Escape`, `⌘↑`; W `Backspace`, `Escape`, `Alt+↑`; L same. Follows
  Go to parent.
- `viewer.goBack` (Go back to the previous document): macOS `⌘[`, plus `⌥←`; W `Alt+←`; L `Alt+←`. Same as the file
  list.
- `viewer.goForward` (Go forward to the next document): macOS `⌘]`, plus `⌥→`; W `Alt+→`; L `Alt+→`. Same.

New commands:

- `view.toggleFullScreen` (#1114): macOS none; W `F11`; L `F11`. Explorer and every browser; GNOME, KDE, Cinnamon, and
  Ubuntu bind no bare `F11`, Xfce binds only `Alt+F11`. WebView2 keeps its own `F11` off in Tauri's windows (it's on
  only in windows opened when `NewWindowRequested` isn't handled). The command exists on all three OSes behind the
  `full_screen` capability, flipped on for Windows and Linux; on macOS it's a palette row with no key and no Cmdr menu
  row, since AppKit's own View menu item keeps its key and its place.
- `search.newQuery`, `search.toggleRecent`, `search.modeByNumber` (decision 12, fixed): macOS `⌘N`, `⌘Y`, `⌘1`–`⌘3`; W
  and L `Ctrl+N`, `Ctrl+H`, `Ctrl+1`–`Ctrl+3`. Their scope: § Settled with David, item 2. Recent searches take each OS's
  browser history key (Chrome's `⌘Y` on macOS, `Ctrl+H` elsewhere): on macOS `⌘H` is AppKit's Hide, which shares the
  dialog's `App` ancestor, so the dialog's old `⌘H` clashed with it (David's call, 2026-11-12).
- `viewer.search`, `viewer.toggleSearchRegex`, `viewer.toggleSearchCase` (fixed): macOS `⌘F`, `⌘⌥R`, `⌘⌥C`; W and L
  `Ctrl+F`, `Alt+R`, `Alt+C` (§ Settled with David, item 1).

## macOS additions

- `nav.back` gains `⌥←` and `nav.forward` gains `⌥→`, after `⌘[`/`⌘]`. Free in every scope that overlaps
  `Main window/File list` (its chain plus Brief and Full mode): the only ⌥-arrow bindings in the registry are
  `pane.moveDividerLeft/Right` (`⌃⌥←/→`), `tab.next/prev` (`⌘⌥→/←`), and the ⌥↑/↓ family, all other combos. macOS
  reserves neither.
- `viewer.goBack`/`viewer.goForward` gain them too (my reading of "in the viewer too"). Free: the viewer reads ⌥-arrow
  only with Shift (`horizontalMotion`), and skips registry keys while its search input has focus.
- Condition: in a main-window text field (rename editor, path bar) `⌥←/→` is word motion, and `global-keydown.ts` keeps
  ⌥ combos live there, so they'd navigate back mid-edit. They go into macOS's native text keys below. The search dialog
  already avoids them for this reason (`query-ui/query-shortcuts.ts` header).

## Native text keys

Combos the OS's text editing owns in a focused text input; dispatch leaves them to the field whatever command they're
bound to (`nativeTextKeys(platform)` in `apps/desktop/src/routes/(main)/native-text-keys.ts`, which a test walks set for
set against this list). The menu bar never registers one either, past a row's own built key (Edit > Copy): the frontend
writes the table to `crates/cmdr-menu/src/page_owned_keys.tsv`, where `rebound_row_key` keeps a pushed one display-only
and `bar_parity_test.rs` holds every built accelerator to it.

- macOS: `⌘C` with a selection, `⌘←`/`⌘→`, `⌥←`/`⌥→`, `⌘↑`/`⌘↓` (the field's start and end), and their `⇧` forms; `⌘⌫`
  (delete to the line start), `⌥⌫` (delete the word before), `⌘⌥⌫`, `⌘⌦`, and `⌥⌦`. The bound ones: `⌘↑` (Go to parent,
  which the macOS bar now only shows), `⌘⌫` (Move to trash), and `⌘⌥⌫` (Delete permanently), which deleted files from
  inside a field until 2026-11-12.
- Windows and Linux: `Ctrl+C` with a selection, `Ctrl+Insert`, `Ctrl+←`/`Ctrl+→`, `Ctrl+Shift+←`/`Ctrl+Shift+→`,
  `Home`/`End` with their Shift and Ctrl forms, `Ctrl+Backspace`/`Ctrl+Delete` (word delete), `Shift+Delete` (cut),
  `Ctrl+Shift+V` (paste as plain text), and `Shift+F10` (the field's own context menu). The bound ones today: `Ctrl+←/→`
  (copy path between panes), `Shift+Delete` (Delete permanently), `Ctrl+Shift+V` (Move here), `Shift+F10` (Open context
  menu), `Home`/`End` (first and last file).
- Not native, but worth a bail: `Ctrl+I` (Switch pane) fires from a rename editor and blurs it.
- Beyond the table, by command: a command that moves or removes files (`FILE_MOVING_COMMAND_IDS` in
  `lib/commands/command-registry.ts`: Move to trash, Delete permanently, Move, Move here, Undo trash, Remove download)
  never runs from a key or the menu bar while a text field has focus, whatever key it's on, F-keys included
  (`routes/(main)/text-field-gate.ts`). The menu event can't say whether a key or a click sent it, so a click on File >
  Delete with a field focused does nothing too; the palette and the F-key bar still run them. `text-field-gate.test.ts`
  walks every default key of those commands on all three OSes.

## Commands on one OS only

- macOS only: `app.hide`, `app.hideOthers`, `app.showAll` (#1110; behind the `appMenu` capability, so off macOS they
  reserve nothing and `Ctrl+H` claims no macOS reservation). Behind capabilities that are false on Windows and Linux
  today (`platform/{windows,linux/mod}.rs`): `app.toggleCliCommand`, `file.getInfo`, `file.quickLook`,
  `file.openTerminalHere` (when it gets a Windows or Linux body: `F9` only, no `Ctrl+Alt+T`, which Ubuntu, Xfce, KDE,
  and Cinnamon take for a terminal), the five `cloud.*`, and the seven `tags.*`.
- Windows and Linux only: none. (`view.toggleFullScreen` exists everywhere and holds a key only on Windows and Linux.)
- Different on Windows and Linux: `view.briefMode`/`fullMode` key order, the volume choosers' Linux second keys,
  Compress's Linux second key, and `tab.next`/`prev`'s `Ctrl+Alt+→/←` (Windows only).

## Internal clashes

Checked both full tables (every registry command, mechanical keys plus the changes above, the commands that don't exist
on that OS removed) pair by pair for the same combo in overlapping scopes, using `scope-hierarchy.ts`.

- Windows: 201 bindings; Linux: 204 (counted before the search dialog's and viewer's fixed keys joined the registry).
  Both are clean, and `registry-conflicts.test.ts` keeps them so with an empty `allowedConflicts` on all three OSes;
  `keymap-invariants.test.ts` also walks every raw arm, capability ignored, so a gated command's keys can't clash
  either.
- The two `Ctrl+D` clashes of the mechanical keymap are gone (Duplicate moved to `Shift+F5`, the error screen to
  `Ctrl+Shift+D`).
- Not a clash by scope, worth knowing: `Alt+↑` is Go to parent (File list), Move row up (Multi-rename), and Move earlier
  (operation queue window). The scopes never meet, but on Windows and Linux the queue window wears the main bar, so Go
  to parent only SHOWS `Alt+↑` there (keydown dispatch runs it in the main window), and no bar row may register a key
  the queue's commands use (`page_owned_keys.tsv`, `bar_parity_test.rs`). A real-X check in `menu-mnemonic-mask.spec.ts`
  presses `Alt+↑` and `Alt+↓` in the queue window.

## Reservations behind the Linux keys

Verified by reading each desktop's default bindings in source on 2026-11-12:

- GNOME (`gsettings-desktop-schemas` `77fa51b13f`, `gnome-shell` `7fcbec8013`, `mutter` `efad8e9f30`,
  `gnome-settings-daemon` `d3e717002a`): `Alt+F2` run, `Alt+F4` close, `Alt+F5` unmaximize, `Alt+F6`, `Alt+F7` move,
  `Alt+F8` resize, `Alt+F10`, `Alt+Space`, `Alt+Tab`, `Alt+Escape`, `Ctrl+Alt+←/→/↑/↓` workspaces,
  `Ctrl+Alt+Shift+arrows`, `Ctrl+Alt+F1`–`F12` (Wayland VT switch), `Ctrl+Alt+Tab`, `Ctrl+Alt+Escape`,
  `Ctrl+Alt+Delete`, `Ctrl+Alt+Shift+R`, and Super combos. Free: `Alt+F1` (empty since GNOME 41), every `Ctrl+F`-key.
- GNOME, read again on 2026-11-12 for the window-manager keys: `Alt+Shift+Tab`, ``Alt+` `` and `Alt+Shift+~` (the key
  above Tab, by position; spelled as a US layout records it), `Ctrl+Alt+Shift+Tab`, `Alt+Shift+F6`, `Alt+Shift+Escape`,
  `Ctrl+Alt+Shift+Escape` (`gsettings-desktop-schemas` `schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in` at
  `258dfe9871`); `Print`, `Alt+Print`, `Shift+Print` screenshots, and `Ctrl+Alt+Shift+R` recording (`gnome-shell`
  `data/org.gnome.shell.gschema.xml.in` at `6b8a634282`). `Ctrl+Alt+F1`–`F12` is in `mutter`
  `data/org.gnome.mutter.wayland.gschema.xml.in` (`c36bb37a79`), so Wayland only. Free: `Ctrl+Print`.
- Ubuntu on top (`ubuntu-settings` gsettings override, gnome-settings-daemon terminal patch): `Ctrl+Alt+T` terminal,
  `Ctrl+Alt+D` show desktop, `Ctrl+Alt+keypad` window placement. Unverified for the window-manager keys above: the
  override files sit on Launchpad, which refused an anonymous read on 2026-11-12, so the table assumes Ubuntu keeps
  every GNOME default.
- KDE Plasma 6 (`kwin` `d9573792e2`, `plasma-workspace` `2d24c1c29e`, `kscreenlocker` `44b77eadbc`): `Alt+F1` launcher,
  `Alt+F2`/`Alt+Space` KRunner, `Alt+Shift+F2`, `Alt+F3` window menu, `Alt+F4`, `Ctrl+F1`–`F4` desktops, `Ctrl+F7`,
  `Ctrl+F9`, `Ctrl+F10`, `Ctrl+F12`, `Ctrl+Alt+Delete`, `Ctrl+Alt+T` (Konsole), and Meta combos. No `Ctrl+Alt+arrows`,
  no `Ctrl+Alt+L`. Also `Alt+Tab`, `Alt+Shift+Tab`, ``Alt+` ``, and `Alt+~` (`Alt+Shift+~` on a US layout), set in
  `TabBox::key` (`kwin` `src/tabbox/tabbox.cpp` at `9e63d97ede`, lines 333–338), and `Print` / `Shift+Print` (Spectacle,
  `spectacle` `src/SpectacleCore.cpp` at `354595addf`, lines 701–708; its `Alt+Print` and `Ctrl+Print` need Meta). Not
  KDE defaults, all Meta-based: `Ctrl+Escape` (`plasma-systemmonitor` `67c3872603`), `Ctrl+Alt+Escape` (Kill window is
  `Meta+Ctrl+Esc`, `kwin` `src/useractions.cpp` at `15575a5c35`), `Ctrl+Alt+arrows` (`src/virtualdesktops.cpp` at
  `e381db7900`). Unverified: `Ctrl+Alt+F1`–`F12` on Wayland (KWin switches on `XF86Switch_VT_*`, `src/input.cpp` at
  `a2a7855509`, but the XKB map behind it wasn't readable).
- Xfce (`libxfce4ui` mirror `73db502864`, `xfce4-keyboard-shortcuts.xml`): `Alt+F1`, `Alt+F2`, `Alt+F3`, `Alt+F4`,
  `Alt+F6`–`Alt+F12`, `Ctrl+F1`–`Ctrl+F12` workspaces, `Ctrl+Alt+arrows`, `Ctrl+Alt+Shift+←/→/↑`, `Ctrl+Alt+Home/End`,
  `Ctrl+Alt+keypad`, `Ctrl+Alt+T`, `Ctrl+Alt+F` (Thunar), `Ctrl+Alt+Delete`, `Ctrl+Alt+Escape`, `Ctrl+Escape`,
  `Ctrl+Shift+Escape`, `Alt+Insert`, `Alt+Delete`, `Alt+Shift+PageUp/PageDown`, `Alt+Space`. Free: `Alt+F5`. Also (the
  same file at `0320f03b8f`): `Alt+Tab` / `Alt+Shift+Tab` window cycling, `Print`, `Alt+Print`, `Shift+Print`
  (screenshooter), and, in the 4.20 releases (the file at `c6640844d3`), `Ctrl+Alt+L` lock and `Ctrl+Alt+D` show
  desktop, which `0320f03b8f` (2026-05-02, the 4.21 development line) moves to Super. The bare arrows and `Escape` there
  act only inside xfwm4's move, resize, and cycle, so they aren't listed.
- Cinnamon (`cinnamon-desktop` `4f797bcfc9`): `Alt+F1` expo, `Alt+F2`, `Alt+F4`, `Alt+F5`, `Alt+F7`, `Alt+F8`,
  `Alt+F10`, `Alt+Space`, `Ctrl+Alt+arrows`, `Ctrl+Alt+Shift+arrows`, `Ctrl+Alt+L` lock, `Ctrl+Alt+T`, `Ctrl+Alt+End`,
  `Ctrl+Alt+Escape`, `Ctrl+Alt+Delete`, `Ctrl+Alt+Tab`, `Ctrl+Alt+Shift+R`. Also `Alt+Tab`, `Alt+Shift+Tab`, ``Alt+` ``,
  `Alt+Shift+~`, `Ctrl+Alt+Shift+Tab` (`schemas/org.cinnamon.desktop.keybindings.wm.gschema.xml.in` at `110609f9c1`);
  `Print`, `Alt+Print`, `Shift+Print`, `Ctrl+Print`, `Ctrl+Alt+Print`, `Ctrl+Shift+Print` screenshots
  (`schemas/org.cinnamon.desktop.keybindings.media-keys.gschema.xml.in` at `096d728f89`, which re-confirms the
  `Ctrl+Alt+` keys above); and `Ctrl+Alt+F1`–`F12` in a Wayland session (`linuxmint/muffin`
  `data/org.cinnamon.muffin.wayland.gschema.xml.in` at `e603d4afa3`).
- The table spells `Print` as `PrintScreen`, the `event.key` a keypress carries. On X11 the X server itself likely takes
  `Ctrl+Alt+F1`–`F12` on every desktop; that's unread, so only the desktops whose own schema binds them list them.
- Input methods: IBus (`ibus` `7d228eafa9`) takes `Ctrl+Shift+U` (Unicode entry) in every release and its emoji key
  moved from `Ctrl+Shift+E` (1.5.20–1.5.25) to `Ctrl+.`/`Ctrl+;` (1.5.26–1.5.27) to `Super+.`/`Super+;` (1.5.28 on).
  GTK's own input context also starts Unicode entry on `Ctrl+Shift+U`. Cmdr's floor is Ubuntu 22.04 (IBus 1.5.26),
  Debian 12 (1.5.27), Fedora 36 (`docs/notes/system-requirements-and-es2025.md`; versions from Repology, 2026-11-12), so
  `Ctrl+Shift+E` is safe and Cmdr binds none of `Ctrl+Shift+U`, `Ctrl+.`, or `Ctrl+;`.
- WebKitGTK reserves no browser keys; its editing bindings (`Ctrl+B`/`Ctrl+I` in rich text, `Ctrl+Shift+V`) act only in
  editable content (local `os-report.md`, WebKit `KeyBindingTranslator.cpp`).

Every new or moved Linux key against those five plus IBus and GTK:

- Free on all five: `Ctrl+Shift+L`, `Q`, `V`, `E`, `K`, `M`, `C`, `D`; `Ctrl+Shift+PageUp/PageDown`;
  `Ctrl+PageUp/PageDown`; `Alt+Shift+←/→`; `Alt+↑`, `Alt+←`, `Alt+→`; `Alt+1`, `Alt+2` (no desktop binds `Alt+<digit>`;
  GTK notebooks do, but WebKitGTK isn't one); `Ctrl+I`; `Ctrl+\`; `Ctrl+<`; `Shift+F5`; `Shift+F10`; `F11`;
  `Ctrl+Shift+F1`, `Ctrl+Shift+F2`, `Ctrl+Shift+F5`, `Ctrl+Shift+F6`; `Alt+Shift+P`; and all 11 Multi-rename
  `Alt+<letter>` keys.
- Dead somewhere, with the working key named first or beside it: `Alt+F1` (KDE, Xfce, Cinnamon), `Alt+F2` (all five),
  `Alt+F5` (GNOME, Cinnamon), `Alt+F7` (GNOME, Xfce, Cinnamon; `Ctrl+F` first), `Ctrl+F1`/`F2` (KDE, Xfce),
  `Ctrl+F3`–`F6` sorts (KDE for F3 and F4, Xfce for all; `Ctrl+3`–`Ctrl+6` first), `Ctrl+Alt+→/←` (GNOME, Xfce,
  Cinnamon; Linux drops it).
- Menu bar: Linux HAS access keys. `src-tauri/src/menu/mnemonics.rs` gives every GTK bar title and item a mnemonic from
  the translated label at build time, and GTK takes a title's `Alt+<letter>` before WebKit sees it, so `preventDefault`
  can't help; English titles take F, E, S, V, G, T, H, other languages differ. Item mnemonics are inert while the menu
  is closed. So without help the search dialog's `Alt+F`, `Alt+S`, and `Alt+V` and the 11 Multi-rename keys would open a
  menu. What shipped: while a main-window dialog, overlay, or the palette is up, the main window's `GtkWindow` mnemonic
  modifier is an unreachable mask, and Alt comes back after the last one closes, like a native modal
  (`apps/desktop/src-tauri/src/menu/DETAILS.md` § "A modal surface takes the mnemonics away"). Settings, the queue, the
  Keyboard shortcuts window, and Debug wear the main bar on Linux too; Settings' shortcut recorders hold the mask while
  they listen. `menu-mnemonic-mask.spec.ts` proves it in the Linux E2E lane with real X key events through `xdotool`.
  The viewer's bar has no mnemonics, so its `Alt+R` / `Alt+C` are safe.

## Windows checks

- Windows itself (Microsoft Support, "Keyboard shortcuts in Windows", page dated 2026-09-02): reserves `Alt+Tab`,
  `Alt+Esc`, `Alt+Space`, `Alt+F4`, `Ctrl+Esc`, `Ctrl+Shift+Esc`, `Ctrl+Alt+Del`, `Ctrl+Alt+Tab`, `Ctrl+Space` (Chinese
  IME), the bare `Ctrl+Shift` chord (layout switch with several layouts), and `Alt+underlined letter` for menus. None of
  the new keys. `Alt+Shift+arrows` and `Ctrl+Shift+arrows` act only on a focused Start-menu tile. The same page confirms
  Explorer's `Alt+↑`, `Alt+←/→`, `Shift+F10`, `F2`, `F11`, and `Ctrl+Shift+V` = paste as plain text.
- WebView2 (Microsoft Learn, "Differences between Microsoft Edge and WebView2", updated 2025-12-04):
  - Off for good, so the page gets them: `Ctrl+D`, `Ctrl+Shift+D`, `Ctrl+Shift+L`, `Ctrl+Shift+E`, `Ctrl+Shift+K`,
    `Ctrl+S`, `Ctrl+1`–`9`, `Ctrl+Tab`, `Ctrl+Shift+Tab`, `Ctrl+N`, `Ctrl+T`, `Ctrl+Shift+T`, `Ctrl+E`, `Ctrl+K`,
    `Ctrl+H`, `Alt+E`, `Alt+F`.
  - Feature off, key still listed (asterisked): `Ctrl+M` (Multi-rename), `Ctrl+Shift+M` (Clone pane), `F6` and
    `Shift+F6` (Move, Rename) as "focus next/previous pane, supported in windowed hosting mode", which is the mode Tauri
    uses. See the unverified list.
  - On until #1115 turns them off: `Shift+F5` (Duplicate), `Ctrl+F5` (sort by modified), `F5` (Copy), `Ctrl+R`
    (Refresh), `F3` (View), `F7` (New folder), `Ctrl+F`, `Ctrl+G`, `Ctrl+Shift+G`, `Ctrl+J`, `Alt+←/→`, `Ctrl+0`,
    `Ctrl++`, `Ctrl+-`, `Ctrl+Shift+C` (Copy path, DevTools inspect).
  - Not listed at all: `Ctrl+Shift+Q`, `Ctrl+Shift+V`, `Ctrl+I`, `Ctrl+\`, `Ctrl+<`, `Alt+↑`, `Alt+Shift+←/→`,
    `Ctrl+PageUp/PageDown`, `Ctrl+Shift+PageUp/PageDown`, `Shift+F10`, Alt+letters.
- TC 11.57 (`_ignored/keymap-research/tc/KEYBOARD.TXT`, local only): none of `Ctrl+Shift+L`, `V`, `E`, `K`, `C`, `D` is
  bound; `Ctrl+Shift+Q` and `Ctrl+Shift+M` are (quick-view window, FTP transfer mode), neither a Cmdr feature.

## Outside the registry

- The global go-to-latest-download hotkey (`behavior.fileSystemWatching.globalGoToLatestShortcut.binding`) defaults per
  OS: `⌃⌥⌘J` on macOS, `Ctrl+Alt+Super+J` on Windows and Linux (shown `Ctrl+Alt+Win+J` on Windows), the same physical
  chord in each OS's spelling. It's the decided exception to "no `Ctrl+Alt+<letter>`" (§ The rule): the Super key keeps
  it clear of AltGr. Rust mirrors the default in `downloads::runtime::DEFAULT_BINDINGS`. Unverified against Windows' own
  Win+Ctrl+Alt combos; `Win+Alt+J` is a simpler candidate to try there.
- The search dialog's mode chips `⌥A`/`⌥F`/`⌥R` are `Alt+A`/`Alt+F`/`Alt+R` off macOS, and its popover and scope keys
  (`⌥S`/`⌥M`/`⌥I`, `⌥C`/`⌥V`) likewise: one per-OS table, `lib/query-ui/query-keys.ts`, which the chips render and the
  routers match. On Linux the mnemonic mask keeps them (§ Reservations behind the Linux keys, "Menu bar"); on Windows
  `Alt+F` is the File menu access key if task 63 adds them.
- The viewer's search toggles: § Settled with David, item 1.
- What each OS takes before Cmdr sees it, which Settings warns about when a user records one: one table per OS in
  `apps/desktop/src/lib/settings/sections/system-reserved-keys.ts`, built from § Windows checks and § Reservations
  behind the Linux keys (only verified entries). `keymap-invariants.test.ts` checks the defaults against it.
- The handlers that still read `metaKey` as the primary modifier, outside the keymap work's surfaces:
  `docs/notes/windows-port/keymap-metakey-leftovers.md`.

## Still unverified (needs a real Windows or Linux box)

- WebView2: whether `F6`, `Shift+F6`, `Ctrl+M`, and `Ctrl+Shift+M` reach the page in windowed hosting. If `F6` moves
  focus out, #1115 needs a per-key `IsBrowserAcceleratorKeyEnabled(FALSE)` for it: the global switch covers only the
  "turned off if AcceleratorEnabled is false" table.
- WebView2: whether `Ctrl+PageUp/PageDown` and `Ctrl+Shift+PageUp/PageDown` (Chromium's tab keys) reach the page.
- WebView2: whether a page's `preventDefault` on `Alt+letter` stops a Win32 menu access key, once task 63 adds them.
- WebView2: the AltGr event shape for AltGr+E and LeftCtrl+LeftAlt+E on German, and right Alt as `AltGraph` with empty
  modifiers on AltGr layouts (Multi-rename's Alt keys then need the left Alt there).
- Windows: whether the bare `Ctrl+Shift` layout switch fires during `Ctrl+Shift+<key>`, and whether `Alt+Shift` (default
  input-language switch) fires during `Alt+Shift+←/→`, on a machine with two layouts.
- Windows: `Ctrl+Alt+←/→` screen rotation on legacy Intel control panels.
- Linux: whether GTK's `Shift+F10` popup-menu binding reaches WebKitGTK's page before GTK acts, and whether the GTK menu
  bar's accelerators fire before the page (they decide whether the native text keys work for menu-bound keys).
- Linux: an XKB `grp:alt_shift_toggle` layout switch (not a default on the five desktops) would eat `Alt+Shift+←/→` and
  `Alt+Shift+P`. Fcitx5's defaults weren't checked.

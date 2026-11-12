# Platform-variant copy: decisions (task 27)

David's answers to the open questions in `copy-survey.md` (2026-11-11). Where this file and `copy-proposal.json`
disagree, this file wins. The governing principle is `docs/design-principles.md` § "Platform-native, not generic":
Windows, Linux, and macOS are each first-class, and a few extra lines of per-OS text beat one watered-down string.

## Glossary

- **This Mac**: "this PC" / "your PC" on Windows, "this computer" / "your computer" on Linux.
- **Trash on Windows**: two registers.
  - Labels (the F8 button, menu rows, palette commands, dialog buttons): "Delete", and "Delete permanently" for the
    Shift+F8 action.
  - Sentences (confirmations, progress, toasts, errors, settings descriptions): always name the place, like "Move 3
    files to the Recycle Bin?" and "Moved to the Recycle Bin". Always "the Recycle Bin", capitalized.
  - Why: File Explorer and Total Commander both label the action "Delete" and name the Recycle Bin only in the
    confirmation (Total Commander: "Do you really want to move the selected file to the recycle bin?", pile `sv` catalog
    ids 2036–2039 and 5335). Naming it in every sentence keeps the transparency principle.
- **Quit on Windows**: two registers.
  - The action (commands, menu rows, buttons, dialog titles): "Exit", like "Exit Cmdr", "Delete all and exit", "Exit
    now", "Exit Cmdr?", "Exit while an operation is running?".
  - Narration where Cmdr is the subject: "close", like "Cmdr closed unexpectedly last time", "then closes", "Cmdr closed
    before this finished", "Asked Cmdr to close".
  - Flagged for David's English review: Microsoft's Writing Style Guide says to use "close" for apps and keeps "Exit"
    for the File menu item (`copy-glossary-evidence.md`). The decision stands as David made it ("Exit" in the palette
    and menus "definitely"); the buttons and dialog titles ("Exit now", "Exit Cmdr?") are the part the guide would word
    as "Close".
  - Linux says "Quit" / "quit" in both registers, including `crashReporter.dialog.body.ended` (the proposal's "closed"
    there is a slip).
- **macOS as the actor, on Linux**: rephrase to drop the actor where the sentence allows it; otherwise "the system" /
  "your system". "This computer" for the machine. "Linux" only when contrasting operating systems.
  - Why: Nautilus and Dolphin never name an actor; Thunar says "your system" and "the computer"; "Linux" appears in the
    pile's file-manager catalogs only as a platform name in a compatibility note.
- **Keychain**: as proposed. Windows checkboxes say "Remember password" and name Credential Manager only where the user
  has to go there; Linux "Remember in keyring"; KDE "KWallet" once the desktop value exists.
- **Home folder on Windows**: "user folder" everywhere, including the command ("Go to user folder"), with "home" kept as
  a palette search alias.
  - Why: Windows 11 File Explorer's "Home" is the quick-access landing page, and Microsoft's "home folder" term means an
    admin-assigned folder on a file server.

## Beyond wording

- **Gate, don't translate**: the Quick Look hint toast and its setting, the Linux menu bar's "Get info" and "Quick look"
  rows, the macOS Local Network block in Settings, `listing.showTags`, and the Local AI option are hidden where the
  feature doesn't exist. `showSafeSaveFiles`, the bundle row, and the full-screen rows are gated too when the code shows
  they mean nothing off macOS. Windows and Linux users get a simpler UI.
- **A macOS cause on a surface every OS reaches is worded per OS, and its macOS-only button is gated** (the conformance
  review's finds, each read from the code):
  - The MTP device-in-use dialog (`mtp.ptpcameradDialog.*`): Linux reaches it when the desktop's own MTP mount holds the
    phone, so it says to unmount the phone in the file manager; Windows gets a plain "another app holds it". Only macOS
    reads about `ptpcamerad` and gets a command.
  - The downloads watcher being down (`downloads.fda.message`, `common.downloadsFdaHint`): off macOS there's no Full
    Disk Access, and the watcher is down only when it couldn't attach to the folder, so the text says to check the
    folder. "Open System Settings" shows under `fullDiskAccess`.
  - A direct SMB connection the machine itself blocked (`directConnectionBlockedByThisMacToast`): off macOS the cause
    left is a firewall. The Local Network button shows under `localNetworkPermission`, and the after-update variant is
    raised only where that permission exists.
  - Hidden bookkeeping settings of macOS-only features (the nudge stamps, the Full Disk Access choice) carry
    `needsCapability`, so `cmdr://settings` doesn't list their labels off macOS.
  - The seeded favorite for the user's folder is named "User folder" on Windows
    (`apps/desktop/src-tauri/src/favorites/platform/`).
- **Errors only macOS produces today**: write the Windows and Linux variants where we know what they should say; skip
  the rest (they stay macOS text until their platform path exists).
- **Mechanism for the non-ICU families** (`errors.*`, `menu.*`): one top-level platform block.
- **Linux desktop terms**: ship the generic fallback now; a `desktop` value comes later with the plan's task 59.
- **Translations**: in a platform select, each branch uses that OS's own localized term, and `@key` descriptions of keys
  shown off macOS are reworded to say so. This is a rule in the translator guide; translating is task 28.
- **The two Mac text changes** are accepted: `ai.secretError.genericBody` ("keyring" → "Keychain") and "e.g." in the Go
  to path placeholder.
- **Rows that asked for a judgment call** (the survey's § "Rows that need David's decision"): take the proposal's
  recommendation. Where the answer depends on behavior nobody has verified (the Windows firewall prompt, MTP exclusivity
  on Windows, the WebDAV trust store, Wayland global shortcuts, the Windows hidden-file rule, `~` in Go to path on
  Windows), read the code; if the code doesn't settle it, keep the macOS text for that platform and list the row as open
  here.
- **Key glyphs in prose** (the eight values with literal ⌘/⌥/⌃/⇧) wait for the keymap work (#1102).

## Open rows

Rows where the code doesn't settle a platform yet, so that platform keeps the macOS text (`openBranches`, or status
`open` for the eject rows, in `copy-final.json`). Each reopens when the named Windows code lands.

- `fileOperations.transferDialog.namedFolderHint`, Windows: "end the path with “/”" stays until the Windows destination
  field is built (plan D1 makes `/` valid everywhere, but what the field shows and accepts isn't decided).
- `transfer.metadataClass.extendedAttributes`, Windows: "Finder tags" stays until the Windows metadata bodies (`cmdr-fs`
  `metadata/platform/windows.rs`, a stub) say which class a Windows copy reports there.
- `settings.listing.showHiddenFiles.description`, Windows: "dotfiles (like .zshrc)" stays until the Windows listing
  backend decides its hidden-file rule (the hidden attribute, dotfiles, or both).
- `ai.cloudConsent.askCmdr.protectedContents`, Windows: the macOS list ("your keychains", "Mail, Messages, Notes") stays
  until `apps/desktop/src-tauri/src/tools/sensitive.rs:RULES` gains Windows entries (browser profiles under `AppData`,
  Credential Manager's files). Today nothing on Windows is protected, so no honest Windows list exists yet; this blocks
  shipping Ask Cmdr on Windows. Linux names only what `RULES` covers (no mail client).
- `settings.advanced.oldMacosNoticeShown.label`, Windows and Linux: the hidden one-shot flag's label ("Old macOS notice
  shown") still lists in the MCP `cmdr://settings` resource. No capability states "this OS has a supported-version floor
  notice" (`startup-gates.ts` asks `isMacOS()`, which is right for the notice itself), and one hidden label doesn't earn
  a new one. It closes when Windows gets its own old-version notice and the flag is renamed for both.
- `main.oldWebkit.body`, Windows: no `windows` branch, so Windows would read the Linux text ("a newer WebKitGTK").
  Unreachable today: the guard blocks an old WebKit, and Windows runs on the evergreen WebView2. Write the branch if the
  boot guard ever gains a WebView2 floor.
- `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description`, Windows and Linux (X11):
  "Requires Full Disk Access." stays with the key glyphs until #1102 passes the combo as a placeholder (the row's note).

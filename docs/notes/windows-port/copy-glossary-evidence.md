# Platform-variant copy: glossary evidence (task 27)

Evidence check of the glossary in `copy-survey.md` and the `windows`, `linux`, and `linuxDesktops` texts in
`copy-proposal.json`, against the localization reference pile and the web. `copy-decisions.md` still wins where it
speaks; this file adds evidence and lists the proposal rows that need new wording. Checked 2026-11-11.

Sources and how they were read:

- **Pile** (main clone only, `_ignored/i18n/`): Microsoft terminology `hu/microsoft-terminology/HUNGARIAN.tbx` (English
  side and its `definition` field), the en-GB Microsoft voice guide `en-GB/microsoft-style-guides/StyleGuide.pdf` (via
  `pdftotext`), and the English `msgid`s of `en-GB/gnome-nautilus/nautilus.po` (PO-Revision-Date 2026-02-16),
  `en-GB/kde-dolphin/dolphin.po` (2024-11-01), and `en-GB/xfce-thunar/thunar.po`. Total Commander ids from `de/`, `sv/`,
  and `hu/total-commander/WCMD.LNG.utf8`.
- **Web**: Microsoft Writing Style Guide A–Z pages (fetched as markdown from learn.microsoft.com), Microsoft Support,
  the GNOME HIG, apps.kde.org, and third-party how-tos where nothing first-party turned up (marked as such).
- Pile gaps worth knowing: the TBX has no Windows shell UI labels ("This PC", "Eject", "Open file location", "Show more
  options" are all absent), and Dolphin's Places panel, Quit, Configure, and eject actions live in KIO/KXmlGui catalogs
  that the pile doesn't carry. TBX "error checking" is the Excel sense (mining trap 4).

## Per-term verdicts

The survey glossary marks 19 terms "Verified: no" (this-mac, macos-actor, keychain, get-info, terminal,
activity-monitor, spotlight, disk-utility, dock, startup-disk, home-folder, eject, homebrew, system-notifications,
settings-ellipsis, disk-image, full-screen, right-click, install-uninstall). They come first, then the 16 it marked
verified.

### Terms marked unverified

- **`this-mac`**: confirmed with a caveat, high.
  - Windows' own UI and the Microsoft voice guide say "PC": en-GB voice guide word list: "PC: Use for personal computing
    devices. Use computer for situations about PCs and Macs" (verified in pile,
    `en-GB/microsoft-style-guides/StyleGuide.pdf` p. 8, 2026-11-11). File Explorer's "This PC" node (verified on web,
    third-party, petri.com "How to map a network drive on Windows", 2026-11-11).
  - Caveat: the Writing Style Guide's computer-and-device collection prefers "computer" and says "It's OK to use PC when
    space is a constraint, but don't switch between PC and computer" (verified on web,
    https://learn.microsoft.com/en-us/style-guide/a-z-word-list-term-collections/term-collections/computer-device-terms,
    2026-11-11). The decision ("this PC / your PC") matches Windows UI and the voice guide, so keep it, but never mix
    "PC" and "computer" in one string. One row does: see corrections.
  - Linux "this computer": Thunar says "Browse all local and remote disks and folders accessible from this computer";
    Dolphin "media attached to the computer" (verified in pile, `en-GB/xfce-thunar/thunar.po`,
    `en-GB/kde-dolphin/dolphin.po`, 2026-11-11). High.
- **`macos-actor`**: confirmed, high. "Windows" as the actor is normal Microsoft voice. Linux: Thunar "presents a
  security risk to your system", "You may harm your system"; Dolphin "change or replace any file or folder on this
  system" (verified in pile, same files, 2026-11-11). Matches the decision.
- **`keychain`**: Windows confirmed, KDE corrected.
  - Windows: TBX "Credential Manager": "A common credential store and interface that is used to save and manage user
    credentials" (verified in pile, `hu/microsoft-terminology/HUNGARIAN.tbx` entry 21301, 2026-11-11). The "Windows
    Credentials" tab name is from memory (unverified). High.
  - GNOME: "Passwords and Keys" (Seahorse) stays web-summary-only (the survey's flathub source); not in the pile.
    Tentative.
  - KDE: the manager app is "KWalletManager" on apps.kde.org and "KDE Wallet Manager" in its launcher and the Arch wiki;
    KDE's user-facing name for the store is "KDE Wallet" (verified on web, https://apps.kde.org/kwalletmanager5/ and
    https://wiki.archlinux.org/title/KDE_Wallet, search summaries, 2026-11-11). The proposal's "KWallet Manager" (with a
    space) matches neither. Medium.
- **`get-info`**: confirmed, high. Nautilus `_Properties` and `Properties`, Dolphin `Properties`, Thunar
  `_Properties...`; tab names Nautilus `Permissions`, Dolphin `Permissions`, Thunar `Permissions` (verified in pile, the
  three `en-GB` catalogs, 2026-11-11). GNOME HIG lists "Properties, Alt+Return" (verified on web,
  https://developer.gnome.org/hig/reference/keyboard.html, 2026-11-11). Windows TBX "Properties": "A UI element that
  users can click to display information about a selected object" (verified in pile, TBX entry 35259, 2026-11-11).
  Windows' "Security" tab is from memory (unverified, high confidence). Total Commander's own Properties id is 1754
  (`de` "Eigenschaften", `hu` "Tulajdonságok"; verified in pile, `*/total-commander/WCMD.LNG.utf8`, 2026-11-11).
- **`terminal`**: confirmed, high, with one command bug (see corrections).
  - Windows: TBX "Terminal": "The latest generation of the Windows Console…" (verified in pile, TBX, 2026-11-11). That
    Windows Terminal opens PowerShell by default is from memory (unverified, high confidence); it matters because
    cmd-only switches break there.
  - KDE: Dolphin names "a standalone terminal application like Konsole" (verified in pile,
    `en-GB/kde-dolphin/dolphin.po`, 2026-11-11).
  - GNOME: "a terminal" stays right because the default app differs by distro (GNOME Console, Ptyxis, GNOME Terminal);
    from memory, unverified.
- **`activity-monitor`**: Windows confirmed, Linux unverified. TBX "Task Manager": "A tool that provides information
  about programs and processes running on the computer…" (verified in pile, TBX entry 10351, 2026-11-11).
  Ctrl+Shift+Esc: "Launch Task Manager… Ctrl+Shift+Esc" (verified in pile, `en-GB/microsoft-style-guides/StyleGuide.pdf`
  p. 40, 2026-11-11). TBX also has "Resource Monitor", which one proposal row uses (verified in pile, TBX, 2026-11-11).
  GNOME and KDE "System Monitor": memory only.
- **`spotlight`**: Windows confirmed, Linux unverified. TBX "Windows Search": "A PC (desktop and beyond) search
  platform…"; "Start menu" and "search box" exist too (verified in pile, TBX, 2026-11-11). The proposal's "search for it
  in Start" is fine; Microsoft's own phrasing is "Go to Search, enter…" (verified on web, Describing interactions with
  the UI, link below, 2026-11-11). GNOME Activities and KRunner: memory only; no proposal row uses them.
- **`disk-utility`**: Windows plausible, unverified. Drive Properties > Tools > Error checking > Check is from memory.
  The TBX "error checking" entry is the Excel sense and doesn't count. GNOME Disks and KDE Partition Manager: memory
  only.
- **`dock`**: Windows confirmed, high. TBX "taskbar" (lowercase): "The bar that contains the 'Start' button…" (verified
  in pile, TBX entry 7785, 2026-11-11). GNOME dash and KDE panel: memory only; no row ships them yet.
- **`startup-disk`**: Windows confirmed, high. TBX "system drive": "The drive letter of the mass-storage device (usually
  a hard disk) where Windows is installed" (verified in pile, TBX entry 14759, 2026-11-11). The Writing Style Guide says
  use "drive", not "disk", for the drive on a PC (computer-device-terms page above). That drives the "disk → drive"
  correction below. Linux "system disk": judgment.
- **`home-folder`**: decision's Why confirmed.
  - TBX "home folder": "A folder (usually on a file server) that administrators can assign to individual users or
    groups" (verified in pile, TBX entry 15807, 2026-11-11). So "home folder" would mislead on Windows, as the decision
    says.
  - TBX also has "personal folder" ("labeled with the name you use to log on… located at the top of the Start menu", a
    Vista-era term) and "user profile" (the settings file), neither a better fit (verified in pile, TBX entries 20540
    and 17268, 2026-11-11). "User folder" isn't a TBX term; it's a plain description, which is fine.
  - Linux "home folder": Thunar "Go to the home folder", "Open the home folder"; Dolphin "The location for the home
    folder…"; Nautilus sidebar "Home" (verified in pile, the three catalogs, 2026-11-11). High.
- **`eject`**: confirmed, with a Linux nuance.
  - Windows: "Eject" for removable drives: Support's "select Eject <device>" and the "Safely Remove Hardware and Eject
    Media" tray menu (verified on web,
    https://support.microsoft.com/en-us/windows/hardware/safely-remove-hardware-in-windows, search summary, 2026-11-11).
    "Disconnect" for mapped network drives (verified on web, third-party, petri.com, 2026-11-11; TBX "disconnect: To
    break a connection", verified in pile). High.
  - GNOME: `_Eject`, `_Unmount`, `_Safely Remove Drive`; Thunar `_Eject`, `_Unmount`, `Safely _Remove` (verified in
    pile, `en-GB/gnome-nautilus/nautilus.po` and `en-GB/xfce-thunar/thunar.po`, 2026-11-11). So on Linux a network mount
    is "unmounted", never "disconnected". KDE's eject and unmount labels aren't in `dolphin.po` (they live in
    KIO/Solid); unverified.
- **`homebrew`**: partly confirmed. TBX "Windows Package Manager" and "WinGet", and "Microsoft Store" (verified in pile,
  TBX entries 50751, 50752, 33812, 2026-11-11). Linux Flathub: memory; no proposal row names it.
- **`system-notifications`**: unverified, judgment. Nothing in the pile names a notification channel this way. "Windows"
  as a radio label is fine; Linux "System" stays judgment.
- **`settings-ellipsis`**: corrected for GNOME.
  - GNOME: Nautilus's window menu item is `_Preferences`, no ellipsis (verified in pile,
    `en-GB/gnome-nautilus/nautilus.po`, `nautilus-window.blp:37`, 2026-11-11). The GNOME HIG lists "Preferences" among
    the standard primary menu items (verified on web, https://developer.gnome.org/hig/patterns/controls/menus.html,
    2026-11-11). Thunar, outside GNOME, writes `Pr_eferences...` (verified in pile).
  - KDE: Dolphin's own help text says "Configure Dolphin > View > General", which confirms the "Configure <App>" naming
    (verified in pile, `en-GB/kde-dolphin/dolphin.po`, 2026-11-11). The trailing ellipsis comes from KStandardAction
    (memory, unverified).
  - Windows: "Settings" with no ellipsis, as in Notepad and Terminal: memory, unverified.
- **`disk-image`**: Windows confirmed. TBX "ISO image" and "virtual hard disk (VHD)" (verified in pile, TBX,
  2026-11-11). "Mount" and "Eject" on an ISO in File Explorer: memory.
- **`full-screen`**: confirmed. TBX "full screen" and "full-screen mode" (verified in pile, TBX, 2026-11-11). F11:
  memory.
- **`right-click`**: confirmed with a nuance. TBX "right-click" (verified in pile). The Writing Style Guide wants
  "select and hold (or right-click)" when an instruction isn't touch-specific (verified on web, Describing interactions
  with the UI, link below, 2026-11-11). For a keyboard-and-mouse desktop app, plain "right-click" is fine.
- **`install-uninstall`**: Windows confirmed, high. "Select Start > Settings > Apps > Installed apps" (verified on web,
  https://support.microsoft.com/en-gb/windows/apps/uninstall-or-remove-apps-and-programs-in-windows, search summary,
  2026-11-11). Linux "package manager": judgment.

### Terms marked verified

- **`file-manager`**: confirmed. TBX "File Explorer": "A utility that enables the user to locate and open files and
  folders" (verified in pile, TBX entry 11280, 2026-11-11). GNOME's app is "Files" (Nautilus `_About Files`, verified in
  pile). Reveal verbs in the pile: Nautilus `_Open Item Location`, Thunar `Open Item _Location`, Dolphin `Show Target`
  and `Open Path` (verified in pile, 2026-11-11). None says "Show in …", so "Show in Files" and "Show in Dolphin" are
  Cmdr's own phrasing (fine for an outside app; tentative).
- **`trash`**: confirmed, with two additions.
  - Microsoft: "Capitalize both words when referring to the Recycle Bin on the Windows desktop… Lowercase when referring
    to the recycle bin in OneDrive… In UI, it's OK to use Recycle bin for labels and headings… Always use the definite
    article the" (verified on web,
    https://learn.microsoft.com/en-us/style-guide/a-z-word-list-term-collections/r/recycle-bin, 2026-11-11). TBX
    "Recycle Bin" entry 8780 agrees (verified in pile).
  - Total Commander's F8 setting: id 5335 (`de` "F8/Entf löscht in den Papierkorb (Umschalt=direkt)") and confirmation
    ids 2036–2039 (verified in pile, `de/total-commander/WCMD.LNG.utf8`, 2026-11-11). Matches the decision.
  - GNOME: `Move to Trash` (menu), `Delete Permanently`, `_Delete Permanently…`; sentences lowercase ("Move “%s” to
    trash", "%d file moved to trash") with "the Trash" as the place name ("All items in the Trash…"). KDE:
    `Move to Trash`, `Permanently Delete %2`, `Delete (using shortcut for Trash)`. Thunar: `Mo_ve to Trash`, "Delete all
    files and folders in the Trash" (verified in pile, the three catalogs, 2026-11-11).
  - The en-GB overlay: KDE en-GB says "wastebin" (confirmed), but GNOME and Thunar en-GB say "Wastebasket" ("All items
    in the Wastebasket will be permanently deleted"; Thunar "Display the Wastebasket") (verified in pile,
    `en-GB/gnome-nautilus/nautilus.po`, `en-GB/xfce-thunar/thunar.po`, 2026-11-11). The survey listed only the KDE word.
- **`system-settings`**: Windows paths confirmed; one KDE path corrected.
  - Windows: "Start > Settings > Accessibility… Select Text size" (verified on web,
    https://support.microsoft.com/en-us/accessibility/windows/make-text-and-apps-bigger, search summary, 2026-11-11);
    Apps > Installed apps (above). TBX "Settings: The app where people can find Windows and device settings" (verified
    in pile, entry 37873). Network & internet, System > Storage, Personalization > Colors, and Time & language > Date &
    time: memory plus the survey's Windows Central source; high.
  - KDE Plasma 6: shortcuts sit under "Keyboard > Shortcuts" (verified on web, third-party, bbs.archlinux.org "Global
    keyboard shortcuts not working in KDE Plasma 6" and tunaos.org, search summaries, 2026-11-11). Dolphin names "System
    Settings" and "System Settings|Workspace|Search" (verified in pile).
  - GNOME panel names (Network, Wi-Fi, Appearance, Keyboard): memory, unverified.
- **`quit`**: confirmed for Linux; Microsoft evidence pushes against part of the Windows decision.
  - GNOME HIG: "Quit, Ctrl+Q: Closes the app, including all its windows" (verified on web,
    https://developer.gnome.org/hig/reference/keyboard.html, 2026-11-11). Nautilus `Quit`, Thunar "Quit a running Thunar
    instance" (verified in pile). The HIG also says "Primary menus shouldn't include menu items for Close or Quit"
    (menus page, above), which bears on the open "Quit row at the end of File on Linux" question only weakly, since
    Cmdr's Linux UI is a menu bar.
  - Microsoft "exit": "Don't use to describe closing an app or program" (verified on web,
    https://learn.microsoft.com/en-us/style-guide/a-z-word-list-term-collections/e/exit, 2026-11-11). Microsoft "quit":
    "Don't use quit to refer to… Closing an app or a program. Use close instead… The action a program takes to close
    itself when it has encountered a problem… Use close instead" (verified on web,
    https://learn.microsoft.com/en-us/style-guide/a-z-word-list-term-collections/q/quit, 2026-11-11). The UI verbs page
    lists "Close" for apps and for "the action that a program or app takes when it encounters a problem" (verified on
    web, https://learn.microsoft.com/en-us/style-guide/procedures-instructions/describing-interactions-with-ui,
    2026-11-11).
  - "Exit" survives as a File menu label: "File Exit Alt+F4" (verified in pile,
    `en-GB/microsoft-style-guides/StyleGuide.pdf` p. 40, 2026-11-11), and Windows 11 Notepad's File menu ends in "Exit"
    (verified on web, eclasses.in search summary, 2026-11-11). TBX "exit: To close software, such as a program or
    application" (verified in pile).
  - So: "Exit Cmdr" as a command or menu label is native. "Exit Cmdr?", "Exit now", and "Delete all and exit" are labels
    Microsoft's own guidance would write with "close". Flagged under "Contradicts a decision" below.
- **`quick-look`**: confirmed. TBX "Peek": "The system-wide utility for Windows that allows users to preview file
  content…" and "PowerToys" (verified in pile, TBX, 2026-11-11). Hiding stays right.
- **`modifier-keys`**: confirmed for Ctrl, Alt, and Shift (TBX "CTRL", "ALT key", "SHIFT key", verified in pile). "Win"
  matches PowerToys' display (memory); in prose, Microsoft writes "Windows logo key" ("press the Windows logo key+U",
  verified on web, the Microsoft Support "Make text and apps bigger" search summary linked under `system-settings`,
  2026-11-11). Key glyphs wait for #1102 anyway.
- **`locked`**: confirmed. TBX "read-only": "Capable of being retrieved (read), printed or copied but not changed"
  (verified in pile, TBX entry 14967, 2026-11-11). "General tab > Attributes > Read-only": memory.
- **`full-disk-access`, `local-network`, `os-version`, `applications-folder`, `finder-tags`, `paths`, `services-menu`,
  `icloud`, `menu-bar`**: code-sourced in the survey; nothing in the pile or on the web bears on them. Left as is.

## Corrections to apply to proposal rows

Each item: pattern → affected keys → corrected wording. Items marked "(decision)" only apply `copy-decisions.md` to rows
the proposal wrote before it; the rest are new.

- **Windows quit register (decision)** → corrected Windows text:
  - `settings.dataWipe.description`: "…Cmdr closes afterwards." (narration).
  - `settings.dataWipe.confirm`: "Delete all and exit".
  - `settings.dataWipe.quit`: "Exit Cmdr".
  - `main.quit.title`: "Exit while an operation is running?" / "Exit while {countText} operations are running?".
  - `main.quit.quitNow`: "Exit now".
  - `ui.toolAsk.quit.title`: "Exit Cmdr?".
  - Rows already right: `commands.appQuit.label`, `askCmdr.tool.quit.*`, `askCmdr.decision.interrupted`,
    `crashReporter.dialog.body.ended`, `operationLog.rollback.interruptedNotice`, `settings.dataWipe.intro*`.
- **Linux crash narration (decision)** → `crashReporter.dialog.body.ended` (linux) → "Cmdr quit unexpectedly last time.
  Here’s a crash report with details that can help fix this."
- **Windows Recycle Bin labels (decision: labels say "Delete")** → corrected Windows text:
  - `suggestedOps.action.trash`, `fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmMoveToTrash`, and
    the `other` branch of `fileOperations.transferDialog.titleVerbOnly` / `titleWithCounts`: "Delete".
  - `commands.editUndoTrash.label`, `menu.edit.undoTrash`: "Undo delete"; `.one`, `menu.edit.undoTrashOne`: "Undo delete
    of “{name}”"; `.many`, `menu.edit.undoTrashMany`: "Undo delete of {countText} items" (with the plural select where
    the source has one). File Explorer's own "Undo Delete" is memory, unverified.
  - `fileExplorer.renameConflict.overwriteTrash` / `overwriteTrashFolder`: "Overwrite and delete old file" / "…old
    folder". Judgment call: these are buttons, so the label register applies; if David wants the place named here,
    "Overwrite (old file goes to the Recycle Bin)".
  - `commands.fileGoToTrash.label`, `fileOperations.trash.goToTrashAction`: keep "Go to Recycle Bin" (it names the
    place, and Microsoft allows the article to drop in labels, like File Explorer's "Empty Recycle Bin").
- **"Recycle Bin" without "the" in a heading that reads as a sentence** →
  `fileOperations.transferProgress.canceledAlertTitle` (trash branch) → "Move to the Recycle Bin canceled", matching
  `errors.write.cancelled.title.trash`.
- **"Disk" for the PC's drive (Windows)** → Microsoft: "Use hard drive, not disk… to refer to the drive on a PC":
  - `errors.write.landingNotRecorded.suggestion`: "…check that your PC has free space, then restart Cmdr."
  - `fileExplorer.openWith.copyRefused.notEnoughSpace`: "Your PC doesn’t have room for a {size} copy of {fileName}…".
  - `errors.listing.diskReadProblem.suggestion`, `errors.listing.couldntReadUnknown.suggestion`,
    `errors.listing.ioSerious.suggestion`: "Check that the drive or device is still (properly) connected".
  - Leave "disk image" and the "Low disk space warning" setting name alone.
- **PC and computer in one string (Windows)** → `servers.refusal.exportRefused` → "{host} doesn’t let this PC open that
  export. Check which devices the export allows in the server’s NFS settings." (Microsoft: don't switch between PC and
  computer; "device" is its general term.)
- **"Click" in Windows instructions** → Microsoft: "Select", not "Click":
  - `settings.fileViewer.suppressBinaryWarning.description` (windows): "Turn this on (or select “Never show this warning
    again” in the banner) to stop showing it." This also fixes a wrong button name: the banner's real label is
    `viewer.binaryWarning.suppressForever` = "Never show this warning again"
    (`apps/desktop/src/lib/intl/messages/en/viewer.json:151`), not “Don’t show again”.
  - `fileExplorer.network.osMountFallback.message`: "Select the button below to try again."
  - `fileOperations.delete.cloudOnlineOnlyMixedWarning`, `cloudOnlineOnlyAllWarning`: "Select “Delete” if deletion is
    OK…".
  - The 10 "right-click" rows are fine.
- **"Enable" for a toggle (Windows)** → Microsoft: "turn on" → `settings.network.directSmbConnection.description` ("When
  this is on, Cmdr connects directly…") and `onboarding.stepOptional.mtp.desc` ("If you turn this on, Cmdr can…"). Low
  priority.
- **cmd-only syntax under "In Terminal" (Windows)** → `errors.listing.symlinkLoopErrno.suggestion` → "In Terminal, run
  `cmd /c dir /AL` in this folder to see where the links point". Plain `dir /AL` fails in PowerShell, Windows Terminal's
  default shell, because `Get-ChildItem` reads `/AL` as a path (memory, unverified; high confidence). The other Windows
  commands (`ping`, `icacls`, `net use`, bare `dir`) work in both shells.
- **Credential Manager name drift (Windows)** → `ai.secretError.keychainTitle` (windows) → "Couldn’t save your API key:
  Credential Manager denied access", matching every other row and the TBX term.
- **macOS and Quick Look left in Linux text** → Linux variants:
  - `fileExplorer.extensionChange.descriptionAdd` (linux): drop "macOS may show this folder as a single item instead."
    from the folder branch, so it matches the Windows text.
  - `viewer.binaryWarning.body` (linux): "…To do that, close this window and press <openKey></openKey> (or double-click
    the file) to open it in the associated app." (no Quick Look on Linux).
  - `settings.fileViewer.suppressBinaryWarning.description` (linux): drop "⇧Space (Quick Look) or", and say "…explaining
    that Enter (open in the associated app) is probably what you wanted."
- **"Disconnect" for a Linux network mount** → Nautilus and Thunar say "Unmount" →
  `errors.listing.authRequiredEneedauth.suggestion` (linux, gnome, kde, fallback): "Unmount the volume in {Files |
  Dolphin | your file manager} and mount it again". This matches the proposal's own
  `errors.listing.staleConnection.suggestion` Linux rows.
- **GNOME menu ellipsis** → `menu.app.settings` (gnome) → "Preferences". The KDE "Configure Cmdr…" and the generic Linux
  "Preferences…" can stay.
- **KDE shortcuts path** → `shortcuts.conflict.systemShortcut` (kde) → "…You can free it up in System Settings >
  Keyboard > Shortcuts."
- **KDE wallet manager name** → `fileExplorer.restic.forgetPassword.keychainKeptIt` (kde) and
  `ai.secretError.keychainBody` (kde) → "KDE Wallet Manager" for "KWallet Manager".
- **Linux trash capitalization (low priority)** → the Linux texts inherit the Mac mix ("to Trash", "to trash", "the
  Trash"). Nautilus writes lowercase "trash" in sentences and capitalizes only the place name ("the Trash"). Pick one
  rule for the Linux branch when the rows are edited; it doesn't block shipping.

## Contradicts a decision

- **Windows "Exit" in dialogs and buttons**: Microsoft's Writing Style Guide says don't use "exit" (or "quit") for
  closing an app and use "close" (links under `quit` above). "Exit" stays native only as the File menu or command label
  ("Exit Cmdr", File > Exit, Alt+F4). So the decision's "Exit Cmdr?", "Exit now", "Delete all and exit", and "Exit while
  an operation is running?" are the decision's call, not Microsoft's usage. Microsoft would write "Close Cmdr?" and
  "Close now", which is what the proposal had.
- **KDE "KWallet"**: KDE's user-facing name is "KDE Wallet", and the manager is "KDE Wallet Manager" (web summaries
  only, medium). The decision says "KWallet" once the desktop value exists; worth a second look then.
- **"PC" vs "computer" on Windows**: the Writing Style Guide prefers "computer", but Windows UI ("This PC") and the
  en-GB Microsoft voice guide say "PC". The evidence leans toward the decision, so this is only a caveat: never mix the
  two in one string.

## Still unverified

- Windows: "This PC" and "Disconnect" from first-party Microsoft pages (only third-party, since Microsoft's map-a-drive
  page 404s); the "Security" and "General > Attributes > Read-only" tab names; drive Properties > Tools > Check;
  "Windows Credentials" tab; "Undo Delete" in File Explorer; Terminal's default PowerShell profile; "Settings" with no
  ellipsis in inbox apps; `certmgr` in `servers.refusal.certificateUntrusted` (the UI name is likely "Manage user
  certificates", and the trust-store question is open in `copy-decisions.md` anyway).
- GNOME: "Passwords and Keys" still being the shipped name; Settings panel names (Network, Wi-Fi, Appearance, Keyboard,
  System > Date & Time); System Monitor (some distros now ship other apps); Disks' "Check Filesystem"; the default
  terminal per distro.
- KDE: Dolphin's eject, unmount, Quit, and "Configure Cmdr…" ellipsis (KIO/KXmlGui catalogs aren't in the pile); System
  Monitor; KDE Partition Manager.
- Total Commander's exit wording: id 5914 is a false friend (`sv` "Avbryt" = Cancel, `hu` "Bezárás" = Close, `de`
  "&Beenden"), and TC ships no English `.LNG`, so TC can't settle exit vs close.

# Platform-variant copy survey (task 27)

Generated from `009bdb32fb` on 2026-11-11. Machine-readable rows: `copy-proposal.json` (same folder). Nothing in the
repo was changed; every text below is a proposal for David.

## Counts

- Rows in the main list: 478 (492 catalog keys; 14 existing `.mac`/`.other`-style pairs collapsed into one row each).
- Reviewed and left as is (false positives, or genuinely the same word on every OS): 109.
- `@key` descriptions that steer translators to macOS terms on otherwise neutral values: 349.
- macOS-only today by capability or backend (`none`): 205. Shown on Linux today: 273.
- Need David’s call: 56. Raw-family rows (errors._, menu._): 121. Rows with GNOME/KDE variants: 66. Rows that also
  change the Mac text: 2.

- **select**: 269
- **none**: 174
- **split-feature**: 18
- **neutral**: 10
- **code**: 7

- **os-name**: 104
- **trash**: 72
- **file-manager**: 63
- **settings-app**: 54
- **menu-bar-and-dock**: 42
- **volumes-and-paths**: 26
- **terminal-and-tools**: 25
- **window-chrome**: 21
- **keychain**: 20
- **install-and-update**: 15
- **existing-pair**: 14
- **mtp-and-devices**: 11
- **modifier-keys**: 11

- **trash**: 72
- **macos-actor**: 62
- **this-mac**: 61
- **file-manager**: 58
- **terminal**: 36
- **system-settings**: 31
- **full-disk-access**: 31
- **keychain**: 23
- **quit**: 20
- **get-info**: 13
- **local-network**: 13
- **dock**: 12
- **locked**: 12
- **modifier-keys**: 12
- **finder-tags**: 12
- **startup-disk**: 10
- **applications-folder**: 9
- **eject**: 8
- **quick-look**: 8
- **icloud**: 7
- **spotlight**: 5
- **homebrew**: 5
- **disk-utility**: 4
- **home-folder**: 4
- **os-version**: 4
- **paths**: 4
- **activity-monitor**: 2
- **disk-image**: 2
- **system-notifications**: 2
- **services-menu**: 2
- **full-screen**: 2
- **settings-ellipsis**: 2
- **menu-bar**: 2
- **install-uninstall**: 1
- **right-click**: 1

## Glossary

- **`this-mac`**
  - macOS: this Mac / your Mac
  - Windows: this PC / your PC
  - Linux (GNOME / KDE): this computer / your computer (GNOME: this computer; KDE: this computer)
  - Source: Windows File Explorer “This PC” node (memory); Microsoft Writing Style Guide uses “PC” (not fetched:
    learn.microsoft.com is blocked in this session)
  - Verified: no
- **`macos-actor`**
  - macOS: macOS (as the thing that did something)
  - Windows: Windows
  - Linux (GNOME / KDE): the system / your system (GNOME: the system; KDE: the system)
  - Source: judgment
  - Verified: no
- **`file-manager`**
  - macOS: Finder
  - Windows: File Explorer
  - Linux (GNOME / KDE): file manager (GNOME: Files, KDE: Dolphin) (GNOME: Files; KDE: Dolphin)
  - Source: Windows 11 context menu “Open file location” (elevenforum.com, windowslatest.com); VS Code label from user
    reports
  - Verified: yes
- **`trash`**
  - macOS: Trash (place), move to trash (verb)
  - Windows: the Recycle Bin (always with “the”, capitalized as the desktop feature)
  - Linux (GNOME / KDE): Trash (GNOME, KDE en-US); en-GB KDE: Wastebin (GNOME: Trash; KDE: Trash)
  - Source: https://learn.microsoft.com/en-us/style-guide/a-z-word-list-term-collections/r/recycle-bin (via search
    summary); KDE en_GB “Wastebin” (bugs.kde.org 379654, Launchpad dolphin en_GB)
  - Verified: yes
- **`system-settings`**
  - macOS: System Settings (+ pane paths, OS-localized via system_strings.rs)
  - Windows: Settings (paths like Settings > System > Storage)
  - Linux (GNOME / KDE): GNOME: Settings; KDE: System Settings; fallback “your system settings” (GNOME: Settings; KDE:
    System Settings)
  - Source: Windows Central guide for Settings > Time & language > Date & time
  - Verified: yes
- **`keychain`**
  - macOS: Keychain / Keychain Access
  - Windows: Credential Manager (but checkboxes say “Remember password”)
  - Linux (GNOME / KDE): keyring (GNOME: Passwords and Keys app; KDE: KWallet) (GNOME: your keyring (Passwords and
    Keys); KDE: KWallet)
  - Source: GNOME “Passwords and Keys” (Seahorse): en.opensuse.org, flathub.org; Windows from memory
  - Verified: no
- **`get-info`**
  - macOS: Get Info
  - Windows: Properties
  - Linux (GNOME / KDE): Properties (GNOME: Properties; KDE: Properties)
  - Source: memory
  - Verified: no
- **`quick-look`**
  - macOS: Quick Look
  - Windows: (none built in; PowerToys Peek)
  - Linux (GNOME / KDE): (none built in; GNOME Sushi) (GNOME: (Sushi, optional); KDE: (none))
  - Source: capabilities.rs
  - Verified: yes
- **`quit`**
  - macOS: Quit
  - Windows: Exit (menus) / Close (sentences)
  - Linux (GNOME / KDE): Quit (GNOME HIG, Ctrl+Q) (GNOME: Quit; KDE: Quit)
  - Source: Microsoft Manual of Style 4th ed. (oreilly.com summary); developer.gnome.org/hig/reference/keyboard.html
  - Verified: yes
- **`terminal`**
  - macOS: Terminal (the app)
  - Windows: Terminal (Windows Terminal is the Windows 11 default) with PowerShell syntax
  - Linux (GNOME / KDE): a terminal (GNOME: Console/Terminal, KDE: Konsole) (GNOME: a terminal; KDE: Konsole)
  - Source: memory
  - Verified: no
- **`activity-monitor`**
  - macOS: Activity Monitor
  - Windows: Task Manager (Ctrl+Shift+Esc)
  - Linux (GNOME / KDE): system monitor (GNOME: System Monitor, KDE: System Monitor) (GNOME: System Monitor; KDE: System
    Monitor)
  - Source: memory
  - Verified: no
- **`spotlight`**
  - macOS: Spotlight
  - Windows: Start menu search / Windows Search
  - Linux (GNOME / KDE): (desktop search: GNOME Activities, KDE KRunner) (GNOME: the Activities search; KDE: KRunner)
  - Source: memory
  - Verified: no
- **`disk-utility`**
  - macOS: Disk Utility > First Aid
  - Windows: drive Properties > Tools > Check
  - Linux (GNOME / KDE): GNOME: Disks (Check Filesystem); KDE: KDE Partition Manager (GNOME: Disks; KDE: KDE Partition
    Manager)
  - Source: memory
  - Verified: no
- **`dock`**
  - macOS: Dock
  - Windows: taskbar (pin to taskbar)
  - Linux (GNOME / KDE): dash / panel (desktop-specific) (GNOME: dash; KDE: panel)
  - Source: memory
  - Verified: no
- **`startup-disk`**
  - macOS: startup disk
  - Windows: system drive
  - Linux (GNOME / KDE): system disk (GNOME: system disk; KDE: system disk)
  - Source: memory
  - Verified: no
- **`home-folder`**
  - macOS: home folder
  - Windows: user folder (C:\Users\name)
  - Linux (GNOME / KDE): home folder (GNOME: Home) (GNOME: Home; KDE: Home)
  - Source: memory
  - Verified: no
- **`full-disk-access`**
  - macOS: Full Disk Access
  - Windows: (no equivalent)
  - Linux (GNOME / KDE): (no equivalent)
  - Source: capabilities.rs
  - Verified: yes
- **`local-network`**
  - macOS: Local Network permission
  - Windows: (firewall prompt, if any)
  - Linux (GNOME / KDE): (none)
  - Source: NetworkSection.svelte
  - Verified: yes
- **`modifier-keys`**
  - macOS: ⌘ ⌥ ⌃ ⇧ glyphs
  - Windows: Ctrl, Alt, Shift, Win
  - Linux (GNOME / KDE): Ctrl, Alt, Shift, Super (GNOME: Super; KDE: Meta)
  - Source: key-capture.ts
  - Verified: yes
- **`eject`**
  - macOS: Eject
  - Windows: Eject (drives, mounted images); Disconnect (network drives)
  - Linux (GNOME / KDE): Eject (GNOME/KDE, removable); Unmount (network) (GNOME: Eject; KDE: Eject)
  - Source: memory
  - Verified: no
- **`os-version`**
  - macOS: macOS version
  - Windows: Windows version
  - Linux (GNOME / KDE): Linux distribution and version
  - Source: platform/linux/mod.rs os_version (PRETTY_NAME)
  - Verified: yes
- **`applications-folder`**
  - macOS: Applications folder
  - Windows: (n/a: per-user NSIS install)
  - Linux (GNOME / KDE): (n/a)
  - Source: plan decisions
  - Verified: yes
- **`homebrew`**
  - macOS: Homebrew
  - Windows: winget / Microsoft Store
  - Linux (GNOME / KDE): distro package / Flathub
  - Source: plan decisions
  - Verified: no
- **`finder-tags`**
  - macOS: Finder tags
  - Windows: (none)
  - Linux (GNOME / KDE): (none)
  - Source: capabilities.rs
  - Verified: yes
- **`locked`**
  - macOS: Locked (Finder flag)
  - Windows: Read-only attribute
  - Linux (GNOME / KDE): immutable flag (chattr +i)
  - Source: crates/cmdr-fs/src/metadata/platform/macos.rs
  - Verified: yes
- **`paths`**
  - macOS: ~/Documents, /…
  - Windows: C:\Users\…, \\server\share (plan D1 display)
  - Linux (GNOME / KDE): ~/Documents, /…
  - Source: windows-port-plan.md D1
  - Verified: yes
- **`system-notifications`**
  - macOS: macOS (as a notification channel)
  - Windows: Windows
  - Linux (GNOME / KDE): System
  - Source: judgment
  - Verified: no
- **`settings-ellipsis`**
  - macOS: Settings…
  - Windows: Settings
  - Linux (GNOME / KDE): GNOME: Preferences; KDE: Configure Cmdr… (GNOME: Preferences; KDE: Configure Cmdr…)
  - Source: memory
  - Verified: no
- **`services-menu`**
  - macOS: Services menu
  - Windows: (none)
  - Linux (GNOME / KDE): (none)
  - Source: capabilities.rs
  - Verified: yes
- **`icloud`**
  - macOS: iCloud Drive
  - Windows: (iCloud for Windows exists; not detected)
  - Linux (GNOME / KDE): (none)
  - Source: provider.rs
  - Verified: yes
- **`disk-image`**
  - macOS: disk image
  - Windows: disk image (ISO, VHD)
  - Linux (GNOME / KDE): disk image (GNOME: disk image; KDE: disk image)
  - Source: memory
  - Verified: no
- **`full-screen`**
  - macOS: full screen
  - Windows: full screen (F11)
  - Linux (GNOME / KDE): full screen
  - Source: memory
  - Verified: no
- **`right-click`**
  - macOS: right-click
  - Windows: right-click
  - Linux (GNOME / KDE): right-click
  - Source: memory
  - Verified: no
- **`menu-bar`**
  - macOS: app menu (Cmdr > …)
  - Windows: (no app menu)
  - Linux (GNOME / KDE): (no app menu)
  - Source: crates/cmdr-menu/src/bar.rs
  - Verified: yes
- **`install-uninstall`**
  - macOS: drag to Trash
  - Windows: Settings > Apps > Installed apps > Uninstall
  - Linux (GNOME / KDE): package manager
  - Source: memory
  - Verified: no

Notes per term:

- **this-mac**: Windows’ own UI says “This PC” and Microsoft copy says “your PC”. Linux has no brand noun, so
  “computer”. The biggest single decision: about 60 rows. Native convention: Microsoft copy says “PC”, never “computer”,
  for the user’s Windows machine.
- **macos-actor**: “macOS blocked…”, “macOS wouldn’t move…”. Linux could say “Linux”, but the refusing layer is often
  the desktop or a security module, so “the system” is more honest.
- **file-manager**: Reveal verb: macOS “Show in Finder”, Windows “Show in File Explorer” (VS Code says “Reveal in File
  Explorer”; Explorer’s own item is “Open file location”), Linux “Show in file manager”. Desktop-specific Linux names
  need a desktop value or a runtime lookup (`xdg-mime query default inode/directory`). Native convention: Windows users
  expect “Open file location” / “Show in folder” phrasing as much as the app name; “Show in File Explorer” keeps parity
  with the Mac label.
- **trash**: Windows’ own delete flow says “Delete” (to the Recycle Bin) and “Permanently delete” (Shift+Delete). Cmdr’s
  “Move to trash” could become “Delete” on Windows with “Delete permanently” for Shift+F8, which is more native than
  “Move to Recycle Bin”. The en-GB overlay handles “Bin”/“Wastebin” for Linux if wanted. Native convention: Native
  Windows phrasing is “Delete” (implies the Recycle Bin) and “Move to the Recycle Bin” only in confirmations; Microsoft
  capitalizes Recycle Bin for the Windows desktop feature and always writes “the Recycle Bin”.
- **system-settings**: Windows paths should be written as Windows writes them; a Windows `system_strings` arm could
  localize them like macOS’s (ms-settings pages have localized titles). Native convention: Microsoft writes paths as
  “Settings > Time & language > Date & time” and uses “turn on” for toggles.
- **keychain**: Windows apps almost never name Credential Manager in a checkbox; they say “Remember password”/“Save
  password”. Name Credential Manager only where the user must go there. Native convention: Windows: “Remember my
  credentials” / “Remember password” in sign-in dialogs; Credential Manager > Windows Credentials when a user must
  manage an entry.
- **get-info**: All three Windows/GNOME/KDE say Properties; the permissions tab is “Security” on Windows, “Permissions”
  on GNOME/KDE. Native convention: Windows: “right-click it and select Properties, then the Security tab” (Microsoft
  style: “select”, not “choose” or “click”).
- **quick-look**: Capability-gated; the right answer is hiding, not renaming.
- **quit**: Microsoft Manual of Style: use “exit”, not “quit”, for closing a program; newer guidance prefers “close”.
  GNOME HIG lists Quit (Ctrl+Q). Native convention: Windows sentences say “Cmdr closed unexpectedly”, “close Cmdr”; the
  menu item is “Exit”.
- **terminal**: Commands differ more than the noun: `ls -la`/`df -h`/`lsof` don’t exist in PowerShell; `ping` does. Each
  Windows row uses a Windows command or a GUI route. Native convention: Windows users rarely get command-line advice; a
  GUI route (Properties > Security, Task Manager) is the native first step.
- **disk-utility**: Native convention: Windows calls it error checking; the button is “Check”.
- **dock**: Dock strings are capability-gated; this matters only for the desktop-wide “empty the Trash in the Dock”
  advice.
- **startup-disk**: Microsoft docs say “the drive where Windows is installed (usually C:)” and “system drive”.
- **home-folder**: Recommended: Windows “user folder” in prose, but keep Cmdr’s “Go to home folder” command name stable?
  David decides.
- **full-disk-access**: Capability-gated.
- **local-network**: Needs a capability; the Settings info block is ungated today.
- **modifier-keys**: Prefer passing the bound combo as a placeholder over writing glyphs in copy. Native convention:
  Windows writes combos as Ctrl+Shift+T.
- **eject**: Removable-drive Eject is genuinely shared. For network shares Windows says “Disconnect”, which matters for
  the servers.* eject strings.
- **applications-folder**: macOS install flow only.
- **finder-tags**: Capability `colorTags`.
- **locked**: Lock flags are read only on macOS today.
- **settings-ellipsis**: GNOME HIG’s primary menu says Preferences; KDE apps say “Configure <App>…”. Cmdr’s window is
  titled Settings, so renaming it per desktop is a product call.
- **services-menu**: Capability `servicesMenu`.
- **disk-image**: Genuinely shared.
- **full-screen**: Genuinely shared.
- **right-click**: Genuinely shared.

## Mechanism

### Inject `platform` into every ICU format call (recommended)

In `apps/desktop/src/lib/intl/messages.svelte.ts`, `t()` (line ~441) and `tStringIn()` (~482) call
`getCompiled(locale, key).format(params)`; pass `{ platform: getPlatform(), ...params }` instead, and `Trans.svelte`
builds its handler object the same way. `getPlatform()` from `#lib/platform` reads `window.__CMDR_PLATFORM__` per call,
so the compiled-message cache (keyed by locale and key) stays valid and `withPlatform()` in tests flips the branch
without cache busting. Messages then write `{platform, select, macos {…} windows {…} other {…}}`; `other` is Linux and
every fallback context (Vitest, plain browser), matching `FALLBACK_PLATFORM = 'linux'`.

Trade-offs: Pros: one key per string, translators see all variants together, no call-site changes, the existing
`.mac`/`.other` pairs collapse. Cons: `platform` becomes a reserved param name (a caller passing its own `platform` must
be a type error: make `TranslationParams` exclude it); `#lib/platform` is a new import of the runtime (allowed: it reads
a global, no logger/settings); messages with nested `plural`/`select` get longer.

What the checks need to learn:

- i18n-parity: `platform` counts as a placeholder; every locale must keep the SAME branch set English has (`macos`,
  `windows`, `other`), not just the arg. New rule, since parity today compares only placeholder names.
- i18n-coverage: today’s SOURCE-TEXT-ONLY logic flags a value whose branches differ from English only by category; teach
  it that a `platform` select copied with English text in one branch is still untranslated (it already warns per branch;
  make the windows/other branch count).
- i18n-plural: unaffected, but must recurse into each platform branch (it already walks nested ICU).
- New `i18n-platform-select`: `platform` may only appear as a top-level select (no `{platform}` interpolation, so no OS
  name leaks as raw text), branches limited to `macos | windows | linux | other`, `other` required.
- New ESLint `cmdr/no-platform-copy-branch`: forbid `isMacOS() ? tString(a) : tString(b)` and `{#if isMacOS()}` around
  copy; the ~10 remaining copy call sites go away.
- desktop-message-keys-unused: the collapsed `.mac`/`.other` keys disappear, so their allowlist entries (if any) go too.
- i18n-dont-translate / quoted-labels: branch-aware, so “Finder” is expected only in the macos branch and a quoted label
  (“Remember password”) is matched per branch.

### Raw families (`errors.*`, Rust `menu.*`): a top-level platform block, resolved before the raw pipeline (recommended)

Both raw paths can’t use ICU (`getMessage()` + `expandSystemStrings` + snarkdown; Rust `menu_t` is lookup + `.replace`).
Allow ONE construct: a value that is entirely `{platform, select, macos {…} windows {…} other {…}}`, whose branch bodies
stay raw. A ~40-line splitter (shared by `getMessage()`, the native-strings generator, and the checks) picks the branch
first; everything after is unchanged. For Rust, `pnpm intl:native-strings` resolves the block at generation time into
per-target tables selected by `cfg(target_os)`, so `menu_t` stays a lookup and `crates/cmdr-menu`’s `PerPlatform<&str>`
(macos/linux only today) can retire its two-key labels.

Trade-offs: Pros: translators see one string with variants in every family; no new keys; Rust needs no ICU. Cons: a
second, tiny grammar to keep in sync with ICU’s (mitigated by sharing one splitter and making `i18n-icu` validate the
block shape for raw keys). Alternative M2b: platform-suffixed sibling keys (`errors.x.suggestion.windows`) with a
`key.<platform>` → `key` fallback; simpler parser-wise, but doubles keys, needs the unused-key and parity checks to
learn sibling groups, and translators see variants apart. Alternative M2c: move `errors.*` to ICU; rejected by the
runtime’s own reasons (snarkdown/esc/`{system_settings}` collisions).

### OS-supplied names (`system_strings.rs`) per platform

`{system_settings}`, `{privacy_and_security}`, `{localNetwork}`, `{appearance}` resolve to macOS pane names from
`.loctable` files; off macOS they fall back to the English MAC defaults (so Linux shows “System Settings” today). Give
`LocalizedSystemStrings` per-OS arms: Windows could read localized Settings page titles (or ship English “Settings”
defaults), Linux returns desktop names. Most token users are capability-gated (FDA, Local Network); for the few that
aren’t, the platform select carries a Windows path and the tokens stay in the macos branch.

Trade-offs: Keeps OS-localized pane names on macOS; avoids pretending Windows/Linux have the same panes. Low priority
once the ungated Local Network block is gated.

### Linux desktop value (later)

Rows carry `linuxDesktops` (GNOME/KDE/fallback). Shipping them needs a `desktop` value beside `platform` (from
`XDG_CURRENT_DESKTOP`, injected by the same init script) and a nested select in the `other` branch:
`other {{desktop, select, gnome {Show in Files} kde {Show in Dolphin} other {Show in file manager}}}`. For the file
manager specifically, `code` is better: read the default `inode/directory` handler’s display name at runtime and pass it
as `{fileManager}`.

Trade-offs: Native names on the two big desktops at the cost of 2 more branches per string for translators. Recommend
shipping the fallback first and adding `desktop` only for the ~15 highest-traffic rows (reveal, trash, settings paths,
keyring).

### Shortcut glyphs in prose → placeholders

The 8 values with literal ⌘/⌥/⌃/⇧ become `{combo}` placeholders (or self-closing `<key></key>` tags) filled from
`getEffectiveShortcuts(id)` + `formatKeyCombo`, as `fileExplorer.quickLookHint` already does. One English string for all
platforms, and user rebinds show correctly.

Trade-offs: Needs task 26’s per-platform keymap to be right first; each locale re-translates once.

### Browser demo, tests, golden fixtures

The demo defines `__CMDR_PLATFORM__` from the visitor’s UA (`lib/demo/platform.ts`), so a Windows visitor gets the
windows branch automatically. Vitest defaults to `'linux'` → the `other` branch: every test asserting a Mac string must
add `withPlatform('macos')`, which is the point. The frozen
`#lib/error-messages/__fixtures__/friendly_error_golden.json` (“never regenerate”) must be asserted under
`withPlatform('macos')`, and gains sibling goldens for windows/linux. The i18n screenshot run (`pnpm i18n:shots`, macOS)
can’t photograph windows/linux branches; the `@key.screenshot` field needs a per-platform form or an explicit “no
screenshot for this branch” marker.

Trade-offs: None real; just budget for the test sweep (expect a few hundred assertions to touch).

## Open questions

- Glossary `this-mac`: “this PC” on Windows and “this computer” on Linux? Settles ~60 rows.
- Glossary `trash` on Windows: “Move to Recycle Bin”, or the more native “Delete” (Recycle Bin implied) with “Delete
  permanently” for Shift+F8? Settles ~65 rows and the F8 labels.
- Glossary `quit` on Windows: “Exit” in the palette/menu and “close” in sentences? Settles ~15 rows.
- Glossary `macos-actor` on Linux: “the system” or name “Linux”? Settles ~30 rows.
- Glossary `keychain`: Windows checkboxes say “Remember password” (not naming Credential Manager); Linux “Remember in
  keyring”, KDE “KWallet”? Settles ~20 rows.
- Glossary `home-folder`: Windows “user folder” in prose, while the command stays “Go to home folder”?
- Gate instead of translate (code changes, no copy): the Quick Look hint toast (fires on Space on Linux today), the
  Linux menu bar’s Get info and Quick look rows, the Local Network info block in Settings > SMB/Network shares,
  `listing.showTags`, the Local AI option, and maybe `showSafeSaveFiles` and the bundle row.
- Errors catalog: write Windows/Linux variants now for the ~25 errno reasons only macOS produces today (they reach
  Linux/Windows once the plan’s typed OS-error classes land), or wait?
- Mechanism for the raw families: one platform block (M2, recommended) or sibling keys (M2b)?
- Linux desktop terms: ship the generic fallback first, and add a `desktop` value later for the top ~15 rows?
- Translations already carry macOS vocabulary where English is neutral (de `menu.window.minimize` “Im Dock ablegen”, ru
  “Убрать в Dock”); 370 `@key` descriptions tell translators to use “the word your language’s macOS uses”. Task 28 needs
  a translator-brief rule: in a platform select, each branch uses that OS’s own localized term, and descriptions of keys
  shown off macOS get reworded.
- Unverified behavior that decides wording: Windows firewall prompt for SMB discovery, MTP exclusivity on Windows (WPD
  vs libmtp), the WebDAV trust store off macOS, whether the global download shortcut works on Wayland/Windows, Windows
  hidden-file rule (attribute vs dot), whether Go to path accepts `~` on Windows.

## Rows that need David’s decision

- `errors.listing.resourceBusy.suggestion`: Is pointing Windows users at Resource Monitor’s handle search worth it, or
  should Windows just say “close apps that might use this folder”?
- `errors.listing.connectionReset.suggestion`: Accept “Terminal” for Windows 11 (the app’s real name) or write
  “PowerShell” for users on Windows 10?
- `errors.listing.symlinkLoopErrno.suggestion`: `dir /AL` is a Command Prompt built-in, not PowerShell (Windows
  Terminal’s default); is a shell-specific command right here, or should Windows drop the command line?
- `errors.listing.nameTooLongErrno.explanation`: Cmdr plans `\\?\` long paths in its Windows backend (plan D1); if it
  always uses them, should the 260-character sentence go?
- `errors.provider.appBased.serious`: When Windows provider detection lands, should “Quit and reopen” become “Close and
  reopen” there?
- `errors.write.trashRefused.message.notPermitted`: Should the Linux branch name the OS (“Linux wouldn’t let…”) or stay
  “The system”? The glossary row `macos-actor` settles all 30-odd rows like this.
- `errors.write.trashRefused.suggestion.ownedByAnotherUser`: Is suggesting `sudo` acceptable in Cmdr’s voice, or should
  Linux just say “Ask the owner or an administrator to move them”?
- `errors.eject.unmountRefusedBySystem`: Write the variants now, or leave it macOS-only until the Linux/Windows eject
  paths name holders?
- `errors.write.homeFolderMove.message.isHome`: Windows calls `C:\Users\name` the user folder and says “sign in”; adopt
  that, or keep “home folder” (Cmdr’s own Go to home folder command) for consistency?
- `errors.write.homeFolderRename.suggestion.isHome`: Is the Linux `usermod` hint too technical for this playful dialog?
  An alternative is the Windows wording for both.
- `settings.appearance.textSize.description`: Does the webview actually follow the Windows text-size setting (WebView2
  does for system UI fonts, not always for CSS px)? If not, Windows should drop the “compounds” clause.
- `settings.listing.showTags.description`: Gate `listing.showTags` on `needsCapability: 'colorTags'` (then no copy
  change), or keep it everywhere for tags read off a Mac-formatted disk?
- `settings.behavior.fileSystemWatching.downloadsNotifications.opt.macos`: “Windows” as a button label next to “In-app”
  reads well; for Linux, “System” or “Desktop”? (A neutral “System” everywhere would change the Mac label.)
- `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description`: Does the global shortcut work at
  all on Linux (Wayland forbids global grabs) and Windows? If not, gate the row instead.
- `settings.network.enabled.description`: Does Cmdr’s Windows build trigger a Windows Defender Firewall prompt (mDNS
  listen, or the direct SMB client)? If not, drop that clause on Windows.
- `settings.network.localNetworkAccessLabel`: Gate the block on a new `localNetworkPermission` capability (macOS only),
  or write a Windows firewall version? Recommended: a capability; then these six keys stay macOS-only with no copy
  change.
- `settings.fileOperations.adbBinaryPath.description`: Does the adb locator on Windows actually search winget/Scoop
  locations? The text should name only what it searches.
- `settings.fileExplorer.suppressQuickLookHint.label`: Gate `maybeShowQuickLookHint()` and this setting on
  `hasCapability('quickLook')`? Then all five Quick Look hint strings are macOS-only and need no variant.
- `settings.advanced.showSafeSaveFiles.description`: Does the `.sb-` filter mean anything off macOS? If not, gate the
  row to macOS; if it also hides `~$` (Office) or `.goutputstream-` (GNOME), rewrite per OS.
- `settings.servers.trustedCertificates.description`: Which trust store does the WebDAV client use off macOS (native-tls
  vs rustls with webpki roots)? If it’s Cmdr’s own list, “Cmdr doesn’t trust” is the honest wording everywhere.
- `settings.dataWipe.doneMessage`: How will Linux ship (AppImage, .deb, Flatpak)? The Linux sentence should name that
  one way.
- `settings.ai.tooltipLocal`: Is Local AI (llama.cpp on Metal) planned off macOS? If never, hide the option via a
  capability and keep this text; if later, add a Windows/Linux requirement line then.
- `settings.ai.tooltipLocalDisabled`: “yet” is a promise: keep it?
- `settings.archives.bundle.description`: Gate the bundle row to macOS?
- `settings.advanced.exitFullScreenOnEscape.label`: Does Cmdr offer full screen off macOS (no green button, no ⌃⌘F in
  the Linux menu)? If not, gate the four full-screen rows.
- `commands.appQuit.label`: Windows: “Exit Cmdr” (classic File > Exit, Microsoft style) or “Close Cmdr” (what modern
  Windows apps and the title-bar X say)? The glossary row `quit` settles about 15 rows.
- `menu.app.quit`: Should the Linux and Windows menu bars get a Quit/Exit row at the end of File (GNOME HIG: Quit,
  Ctrl+Q; Windows: Exit)? That’s a new `linux_only`/`windows_only` row with its own key, not a variant of this one.
- `menu.app.settings`: Rename the Linux menu item per desktop (Preferences / Configure Cmdr…) while the window stays
  “Settings”, or keep “Settings…” for one name everywhere? Windows: drop the ellipsis?
- `menu.file.getInfo`: Make the row `macos_only` (or capability-driven) in `bar.rs`, so Linux and Windows don’t show it
  at all? Recommended.
- `menu.file.quickLook`: Make it `macos_only` in the bar too?
- `main.dockPinNudge.title`: Windows has a near twin: “Pin to taskbar”. Worth a Windows nudge later (new feature, new
  keys)?
- `main.oldWebkit.title`: Could the guard fire on Linux with an old WebKitGTK? Then Linux needs “Cmdr needs a newer
  WebKitGTK; update your system” wording.
- `onboarding.stepAi.localTooltip`: Same “yet” question as settings.ai.tooltipLocalDisabled.
- `onboarding.stepBeta.checklist.starNote`: Show this checklist line on macOS only, or write per-OS distribution
  milestones (winget, Flathub)?
- `onboarding.stepOptional.networking.summary`: Is there any permission prompt on Windows for SMB discovery? Until
  verified, the honest Windows text may also be “No permission needed”.
- `onboarding.stepOptional.indexing.benefit1`: The Spotlight comparison is a marketing line: worth a Windows twin
  (“Think Everything…” would be cheeky), or drop the comparison off macOS?
- `onboarding.stepOptional.mtp.desc`: What does Cmdr’s MTP do on Windows: its own libmtp over WinUSB, or WPD? The cost
  sentence depends on it.
- `fileExplorer.network.osMountFallback.message`: The “4x slower (sometimes 100x)” figure was measured on macOS; keep a
  number only where it was measured?
- `fileExplorer.extensionChange.descriptionRemove`: Should the folder case even ask off macOS (no packages there)?
  Skipping the dialog for folders on Linux/Windows is a code change.
- `fileExplorer.quickLookHint.finderComparison`: Gate the hint on `quickLook` (recommended)?
- `fileExplorer.navigation.driveIndex.tooltipCoalesced`: Will the Linux/Windows watchers report overflow into this same
  status? If yes, write the variants; if not, leave as macOS-only.
- `fileExplorer.quickFilter.intro.openSettings`: Follow the `settings-ellipsis` glossary decision.
- `ai.cloudConsent.askCmdr.protectedContents`: Does the AI sensitive-path deny-list have Windows and Linux entries yet?
  The consent text must match it exactly; that’s a privacy promise.
- `ai.local.notInstalled`: Same as settings.ai.tooltipLocal: hide the Local card off macOS?
- `ai.secretError.genericBody`: This changes the macOS text (keyring → Keychain); OK?
- `downloads.shortcutRow.addModifier`: Can global shortcuts use the Windows key (many Win+ combos are OS-reserved)? If
  not, Windows lists only Ctrl, Alt, or Shift.
- `fileOperations.transferDialog.macOnlyFiles.toggle`: Show the checkbox on Windows/Linux only when the selection
  contains such files?
- `goToPath.dialog.inputPlaceholder`: Does Go to path accept `~` on Windows? If yes, `~\Documents` is a friendlier
  example; the parser decides. Also: change “e.g.” on macOS too?
- `servers.refusal.certificateUntrusted`: Which store does the client consult off macOS? If Cmdr’s own
  trusted-certificates list (Settings > Servers), point there on every OS.
- `servers.sheet.nfsIdentityThisMac`: What does the Windows NFS backend default the IDs to?
- `shortcuts.conflict.reservedByMacOs`: Does NATIVE_SHORTCUT_COMMAND_IDS mean anything off macOS? Task 26 (keymap)
  decides; this row follows it.
- `transfer.metadataClass.extendedAttributes`: Which metadata classes can a Linux/Windows copy report? Windows has no
  xattrs (NTFS uses alternate data streams); the class may never fire there.
- `ui.toolAsk.openBundle.title`: Does the Windows build need an “Run this program?” ask for .exe/.bat? New keys, not a
  variant.
- `mtp.connectedToast.bodyMac + mtp.connectedToast.bodyOther`: On Windows, does Cmdr hold the device exclusively? If
  not, the Windows branch might say nothing about other apps.
- `fileOperations.transferDialog.namedFolderHint`: Will the Windows destination field accept both `/` and `\`? Then name
  `\`, the one Windows users type.
- `settings.listing.showHiddenFiles.description`: On Windows, does Cmdr treat dotfiles as hidden (like Git Bash) or only
  the Hidden attribute (like File Explorer)? And show protected system files?

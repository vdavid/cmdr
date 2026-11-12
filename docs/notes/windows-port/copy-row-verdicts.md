# Platform-variant copy: row verdicts from the code (task 27)

Settles, from the code at `f0e0d8639b` (2026-11-11), every row and gate that `copy-decisions.md` left to "read the
code". Inputs: `copy-decisions.md` (wins over everything), `copy-survey.md`, `copy-proposal.json`. Where a verdict says
"proposal", the row's `windows` / `linux` text in `copy-proposal.json` ships as written. "Open" means the code doesn't
settle it yet: keep the macOS text on that platform for now. Kept as the code evidence `copy-final.json` rows cite; the
shipped text is `copy-final.json`'s, which departs from a verdict in a few rows (each says why in its `sources`), and
the rows still open are `copy-decisions.md` § Open rows.

## Gates

Each gate hides a macOS-only surface off macOS. The catalog keys listed stay macOS-only with no copy change.

- **Quick Look hint toast and its setting**: existing capability `quickLook`.
  - Gate the call in `apps/desktop/src/lib/file-explorer/quick-look/quick-look-hint.ts:maybeShowQuickLookHint` (return
    early unless `hasCapability('quickLook')`); its only caller is `pane-key-router.ts` on `selection.toggle`, so today
    every Linux Space press shows the toast.
  - Add `needsCapability: 'quickLook'` to the `fileExplorer.suppressQuickLookHint` entry in
    `apps/desktop/src/lib/settings/definitions/advanced.ts`.
  - Keys: `fileExplorer.quickLookHint.*`, `settings.fileExplorer.suppressQuickLookHint.label`, `.description`,
    `.keywords`.
- **Linux menu bar's "Get info" and "Quick look"**: wrap both rows in `macos_only(...)` in
  `crates/cmdr-menu/src/bar.rs:MENU_BAR` (File menu, `GET_INFO_ID` and `QUICK_LOOK_ID`). `cmdr-menu` can't read
  `PlatformCapabilities` (no `tauri`), so the static platform filter is the tool; capabilities `getInfo` and `quickLook`
  are `false` on Linux and Windows (`platform/linux/mod.rs:capabilities`, `platform/windows.rs:capabilities`), and a
  test pinning bar ⇔ capability would keep them in step. Keys: `menu.file.getInfo`, `menu.file.quickLook`.
- **Local Network block in Settings**: new capability `local_network_permission` (macOS `true`, Linux `false`, Windows
  `false`). Gate the `<div class="local-network-info">` inside the `network.enabled` row of
  `apps/desktop/src/lib/settings/sections/NetworkSection.svelte`. Keys: `settings.network.localNetworkAccessLabel`,
  `.permissionIntroPrefix`, `.permissionIntroConnectLink`, `.permissionIntroSuffix`, `.manageAnytimePrefix`,
  `.permissionPath`, `.permissionWithout`.
- **`listing.showTags`**: existing capability `colorTags`. Add `needsCapability: 'colorTags'` to `listing.showTags` in
  `apps/desktop/src/lib/settings/definitions/appearance.ts`, and have the two readers (`pane/listing-loader.ts` near the
  `getSetting('listing.showTags')` check, `views/file-list-utils.ts`) ask `hasCapability('colorTags')` too, so the
  hidden setting's default can't keep a tag fetch alive. Keys: `settings.listing.showTags.label`, `.description`,
  `.keywords`.
- **Local AI** (settings option, onboarding option, the Local card): new capability `local_ai` (macOS `true`, Linux
  `false`, Windows `false`), meaning "this OS build ships a local model runtime". It's coarser than
  `ai/mod.rs:is_local_ai_supported`, which keeps narrowing on macOS (Apple silicon, macOS 14), so an Intel Mac still
  sees the option disabled with its reason.
  - Sites: the `local` option and `providerTooltips` in `apps/desktop/src/lib/settings/sections/AiSection.svelte`; the
    `local` choice in `apps/desktop/src/lib/onboarding/StepAi.svelte`;
    `apps/desktop/src/lib/settings/sections/AiLocalSection.svelte` and its card-menu entry.
  - Keys: `settings.ai.tooltipLocal`, `settings.ai.tooltipLocalDisabled`, `onboarding.stepAi.localTooltip`,
    `ai.local.notInstalled`, plus the rest of `ai.local.*`. Check `ai.managed.localOnlyUnsupported` (an MDM "local only"
    policy off macOS) still reads right once the option is gone.
- **`showSafeSaveFiles`**: new capability `safe_save_files` (macOS `true`, Linux `false`, Windows `false`). The filter
  is `crates/cmdr-fs/src/staging.rs:is_safe_save_name`, which matches only NSDocument's `.sb-` marker; nothing else
  (Office `~$`, GNOME `.goutputstream-`) is covered, so off macOS the row describes a file nobody's apps write. Add
  `needsCapability` to the `advanced.showSafeSaveFiles` registry entry. The filter itself can keep running everywhere (a
  Mac saving onto a share). Keys: `settings.advanced.showSafeSaveFiles.*`.
- **Bundle row**: new capability `app_bundles` (macOS `true`, Linux `false`, Windows `false`).
  `apps/desktop/src/lib/file-explorer/pane/archive-enter-policy.ts:ARCHIVE_ENTER_FORMATS` matches `bundle` by folder
  extension on every OS, and "Open" launches through LaunchServices, so off macOS the row governs Enter on any folder
  named `X.app`. Gate the `BUNDLE_ID` row in `ArchivesSection.svelte` (and its `card.bundles` card) with
  `needsCapability: 'appBundles'` on `behavior.archiveEnter.bundle` in `definitions/behavior.ts`, and skip the `bundle`
  format in `classify` off macOS. Keys: `settings.archives.card.bundles`, `settings.archives.bundle.label`,
  `.description`, `settings.behavior.archiveEnter.bundle.keywords`.
- **Full-screen rows**: new capability `full_screen` (macOS `true`, Linux `false`, Windows `false` until a native F11
  entry exists). Off macOS Cmdr offers no way into full screen: no menu row in `crates/cmdr-menu/src/bar.rs`, no
  command, no F11 handler; only `window_state/mod.rs` restores a saved full-screen state, which a window manager
  shortcut could have set. Gate `advanced.exitFullScreenOnEscape` and `advanced.exitFullScreenOnEscapeHintShown` in
  `definitions/advanced.ts`, and make `routes/(main)/escape-key.ts:exitFullScreenOnEscape` return early without the
  capability, so the toast can't fire either. Keys: `settings.advanced.exitFullScreenOnEscape.*`,
  `settings.advanced.exitFullScreenOnEscapeHintShown.*`, `main.escapeFullScreenHint.*`. Flip the capability on per OS
  when the native-feel phase adds F11.
- **Homebrew star note**: new capability `homebrew_cask` (macOS `true`, others `false`), on the checklist line in the
  onboarding beta step. Key: `onboarding.stepBeta.checklist.starNote`. (Per-OS distribution milestones are a later
  content task.)
- **`NATIVE_SHORTCUT_COMMAND_IDS`**: no capability; make the set macOS-only in
  `apps/desktop/src/lib/commands/command-registry.ts` (task 26, #1102). The four ids are AppKit `PredefinedMenuItem`s,
  which exist only in the macOS app menu (`bar.rs`, `macos_menu(APP_MENU_ID, …)`). Key:
  `shortcuts.conflict.reservedByMacOs`.
- **Extension-change folder ask**: a code change, no capability. Folders get an extension only as a package
  (`apps/desktop/src/lib/utils/file-extension.ts:fileExtension`), and packages come from
  `crates/cmdr-fs/src/file_extension.rs:PACKAGE_EXTENSIONS` by name on every OS (`crates/cmdr-fs/src/entry.rs` sets
  `is_package` from it). Restrict the name-based package rule to macOS; then the `folder` branch of
  `fileExplorer.extensionChange.descriptionRemove` / `descriptionAdd` is unreachable off macOS and stays macOS text.
- **Also macOS-only already, no new gate**: `main.dockPinNudge.*` (`dockPin`), `ui.toolAsk.openBundle.*` (macOS launch
  path), `fileExplorer.navigation.driveIndex.tooltipCoalesced*` (unreachable off macOS, see the watcher finding below).

New capabilities needed: `local_network_permission`, `local_ai`, `safe_save_files`, `app_bundles`, `full_screen`,
`homebrew_cask`, and `global_shortcuts` (below).

## Behavior findings

- **Windows firewall and SMB discovery**: a prompt is expected.
  `apps/desktop/src-tauri/src/network/mdns_discovery.rs:start_browse` creates an `mdns_sd::ServiceDaemon`, which binds
  UDP 5353 on all interfaces to receive multicast replies, only while a `DiscoveryLease` is held. `mdns-sd` is an
  ungated dependency (`apps/desktop/src-tauri/Cargo.toml`), and the NSIS installer (`tauri.windows.conf.json`,
  `installMode: currentUser`) adds no firewall rule. Windows Defender Firewall asks the first time an app without a rule
  listens for inbound traffic. The direct SMB client only dials out (TCP 445), which never prompts. Confirm on the task
  56/QA Windows pass.
- **MTP on Windows**: shared, not exclusive. Plan D2 (`docs/specs/windows-port-plan.md`) ships MTP through `mtp-rs`'s
  WPD backend, Windows' own MTP stack, which File Explorer uses at the same time (the `mtp-rs` repo's README § Windows).
  No ptpcamerad-style suppression exists or is needed. `crates/cmdr-mtp/Cargo.toml` still lists
  `platforms = ["linux", "macos"]`; task 56 builds the Windows arm.
- **WebDAV (and S3) trust store**: the OS's, on every OS. `crates/cmdr-webdav/src/tls_trust.rs:PinningVerifier` wraps
  `rustls-platform-verifier` (Security.framework on macOS, the Windows certificate store via CryptoAPI, the system CA
  bundle on Linux), and a person's pin only rescues an OS refusal. `cmdr-s3` uses reqwest's `rustls` feature, the same
  platform verifier.
  - `CertificateUntrusted` (`crates/cmdr-webdav/src/errors.rs:classify_connect_error`) is the NON-approvable refusal
    (revoked, malformed, badly signed); only `CertificateNeedsApproval` is pinnable.
- **Global "go to latest download" shortcut**: works on Windows, not on Wayland. `downloads/global_shortcut.rs` wraps
  `tauri-plugin-global-shortcut` 2.4.1 over `global-hotkey` 0.8.0, whose platform impls are `macos`, `windows`
  (`RegisterHotKey`), and `x11` only (`global-hotkey-0.8.0/src/platform_impl/`). On a Wayland session the X11 grab lands
  on XWayland and fires only while an XWayland window has focus, so the feature is dead there. Nothing in the app checks
  the session type.
  - New capability `global_shortcuts`: macOS `true`, Windows `true`, Linux `true` only on an X11 session
    (`XDG_SESSION_TYPE`, or `WAYLAND_DISPLAY` unset). Gate the `globalGoToLatestShortcut` rows in
    `NotificationsSection.svelte` / `KeyboardShortcutsSection.svelte` and skip `refresh_global_go_to_latest_shortcut`
    where `false`.
  - The Win key: `RegisterHotKey` takes `MOD_WIN`; an OS-reserved combo fails to register and already reads as
    unavailable (`global_shortcut.rs` test `the_os_refusing_a_combo_reads_as_unavailable_and_nothing_else_does`).
- **Hidden-file rule**:
  - Linux: a leading dot (`crates/cmdr-fs/src/entry.rs:is_hidden_by_name`), plus, at a volume root only, names in that
    root's `/.hidden` file (`crates/cmdr-local/src/reading.rs`, `read_dot_hidden_names`, ungated). `has_uf_hidden` is
    `false` on Linux.
  - Windows: undecided. No Windows local backend exists (`crates/cmdr-local/Cargo.toml`
    `platforms = ["linux", "macos"]`), and the plan doesn't pick dot vs `FILE_ATTRIBUTE_HIDDEN` vs system files.
- **`~` in Go to path on Windows**: `crates/cmdr-fs/src/tilde.rs:expand_tilde` expands only `~` and `~/…` (never `~\…`),
  via `dirs::home_dir()`. On Windows that yields `C:\Users\ada/Documents`, a backslash root that breaks plan D1's
  `C:/Users/ada` form (see bugs). Windows path parsing in the go-to-path resolver
  (`apps/desktop/src-tauri/src/go_to_path/mod.rs:resolve`) isn't built yet.
- **adb locator**: searches nothing Windows-specific. `crates/cmdr-adb/src/server.rs:locate_adb_binary` tries the
  override, `$ADB`, `$PATH` entries joined with `adb`, `$ANDROID_HOME` / `$ANDROID_SDK_ROOT` `platform-tools/adb`,
  `~/Library/Android/sdk/platform-tools/adb`, `/opt/homebrew/bin/adb`, `/usr/local/bin/adb`. Every candidate lacks
  `.exe`, so on Windows only the override and `$ADB` can ever match (plan task 46 owns `adb.exe` discovery). On Linux it
  finds `$PATH` and the SDK env vars, but not Android Studio's Linux default `~/Android/Sdk/platform-tools`.
- **Full screen off macOS**: no entry point (see gates).
- **`showSafeSaveFiles` off macOS**: `.sb-` only (see gates).
- **Bundle row off macOS**: name match plus LaunchServices (see gates).
- **Extension-change folder ask off macOS**: fires for folders named like a macOS package (see gates).
- **`NATIVE_SHORTCUT_COMMAND_IDS` off macOS**: meaningless. The ids are dispatch-exempt (`DISPATCH_EXEMPT_IDS` sources
  from it) and their only implementation is the macOS app menu. On Linux, `app.quit` has no handler and no menu row, so
  Ctrl+Q does nothing, and the shortcuts store refuses to rebind the four (`shortcuts-store.ts`, `nativeShortcutIds`).
- **Old-WebKit guard on Linux**: it can fire. The guard is the inline script in `apps/desktop/src/app.html`
  (`belowFloor`: `crypto.randomUUID`, `Object.hasOwn`, `Array.prototype.findLast`, `:has()`), ungated by platform. A
  WebKitGTK older than the Safari 15.4 feature set trips it; Ubuntu 22.04's original WebKitGTK sits at that edge.
  WebView2 is evergreen Chromium, so it never fires on Windows.
  - Mechanism: the guard's strings are resolved at BUILD time (`vite.config.js` +
    `apps/desktop/scripts/gen-boot-guard-lib.ts`, keys in `src/lib/utils/boot-guard-keys.ts`), so a `platform` select
    there must be resolved from the build target (`TAURI_ENV_PLATFORM`), not the runtime global.
- **AI sensitive-path deny-list**: one folded rule list, no `cfg`, in
  `apps/desktop/src-tauri/src/tools/sensitive.rs:RULES`, matched on every OS.
  - Linux entries present: `.local/share/keyrings`, `.mozilla`, `.config/google-chrome`, `.config/chromium`,
    `.config/bravesoftware`, `.config/microsoft-edge`, plus the OS-neutral `.ssh`, `.gnupg`, `.aws`, … and
    `CREDENTIAL_FILE_NAMES`. No Linux mail client (`.thunderbird`, `.local/share/evolution`).
  - Windows entries: none. Firefox and Chrome profiles under `AppData/Roaming/Mozilla`, `AppData/Local/Google/Chrome`,
    and Credential Manager's `AppData/*/Microsoft/Credentials` / `Vault` aren't matched.
- **Metadata classes a copy can report** (`crates/cmdr-operations/src/metadata_losses.rs:CLASSES`, counted from the
  destination's `MetadataCaps` against the source's):
  - Linux local disk reports `CREATED`, `MODE`, `XATTRS` (`user.*` only) and stores `MODE` and `XATTRS`
    (`crates/cmdr-fs/src/metadata/platform/linux.rs:LOCAL_DISK_CAPS`). So a Linux copy can report creation dates,
    permissions, extended attributes, and folder dates; Locked and Hidden only when another source (a Mac share) carries
    flags.
  - Windows: no bodies yet (`crates/cmdr-fs/src/metadata/platform/windows.rs` is a stub; its header plans ADS, the DACL,
    and file attributes).
- **System text size**: `apps/desktop/src-tauri/src/system_look/platform/linux/mod.rs:text_size_multiplier` and
  `platform/windows.rs:text_size_multiplier` both return `1.0`, so nothing compounds off macOS today.
- **Linux packaging**: `apps/desktop/src-tauri/tauri.linux.conf.json` targets `deb`, `rpm`, and `appimage`, three ways
  in, so "the way you installed it" is the honest sentence.
- **NFS identity on Windows**: `crates/cmdr-nfs/src/dial/platform/windows.rs:process_ids` returns `(65534, 65534)`
  (`nobody`), machine name from `COMPUTERNAME`.
- **Drive-index overflow off macOS**: never reported. `tooltipCoalesced` counts shallow `MustScanSubDirs` anchors
  (`crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/route.rs:decide_shallow_anchor_in`), which only FSEvents
  produces; the `notify` backend's `overflow_flag` is constant `false`.
- **Windows build is D4's Windows 10 22H2 and 11**: Windows Terminal is the default only on 11, while PowerShell exists
  on both. Every Windows branch that proposed "In Terminal, run …" says "In PowerShell, run …" instead (the commands it
  names, `ping`, `dir`, `icacls`, run there).

## Verdicts per open row

Rows from the survey's § "Rows that need David's decision", plus `split-feature` / `code` rows.

- `errors.listing.resourceBusy.suggestion` → proposal (Resource Monitor).
- `errors.listing.connectionReset.suggestion` → Windows rewritten for Windows 10; Linux proposal.
  - Windows: "Here’s what to try:\n- Check that the server is running\n- In PowerShell, try `ping <hostname>` to test if
    the server is reachable\n- Navigate here again to retry"
- `errors.listing.symlinkLoopErrno.suggestion` → Windows rewritten (`dir /AL` is a Command Prompt built-in and fails in
  PowerShell); Linux proposal.
  - Windows: "Here’s what to try:\n- In PowerShell, run `Get-ChildItem -Force | Select-Object Name, LinkType, Target` in
    this folder to see where the links point\n- Find and fix the link that creates the loop\n- If you’re not sure which
    link is the problem, follow them one by one"
- `errors.listing.nameTooLongErrno.explanation` → Windows drops the 260-character clause (plan D1 puts every local path
  through `\\?\` in the Windows backend); Linux proposal.
  - Windows: "The file or folder name exceeds the system’s limit (255 characters for one name on NTFS). This can also
    happen when the full path (all folders combined) exceeds the system’s maximum path length."
- `errors.provider.appBased.serious` → write now (OneDrive and Dropbox detection is coming): Windows "This folder is
  managed by **{name}**. Here’s what to try:\n- Close and reopen {app}\n- Sign out and back in to {app}\n- Check the
  {name} status page for outages"; Linux keeps the macOS text ("Quit and reopen").
- `errors.write.trashRefused.message.notPermitted` → Windows proposal; Linux drops the actor per the glossary: "Cmdr
  wasn’t allowed to touch {count} of the items you picked."
- `errors.write.trashRefused.suggestion.ownedByAnotherUser` → proposal.
- `errors.eject.unmountRefusedBySystem` → write now: Windows proposal; Linux "Something on this computer is still using
  it. Wait a minute, then try again."
- `errors.write.homeFolderMove.message.isHome` → proposal (decided: "user folder", "sign in").
- `errors.write.homeFolderRename.suggestion.isHome` → proposal.
- `settings.appearance.textSize.description` → both drop the compounds clause (multiplier is `1.0`): Windows and Linux
  "Scales text and UI throughout the app." Restore a per-OS clause when an arm reads the system setting.
- `settings.listing.showTags.description` → macOS-only, gated (`colorTags`).
- `settings.behavior.fileSystemWatching.downloadsNotifications.opt.macos` → proposal (Windows "Windows", Linux
  "System").
- `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description` → Windows proposal; Linux proposal
  where `global_shortcuts` holds (X11), row gated on Wayland. Glyph part waits for #1102.
- `settings.network.enabled.description` → proposal (firewall clause stays on Windows).
- `settings.network.localNetworkAccessLabel` and the five sibling block keys → macOS-only, gated
  (`local_network_permission`).
- `settings.fileOperations.adbBinaryPath.description` → Linux proposal; Windows open (the locator searches nothing
  Windows has).
- `settings.fileExplorer.suppressQuickLookHint.label` / `.description` → macOS-only, gated (`quickLook`).
- `settings.fileOperations.pasteClipboardAsFile.description`,
  `settings.fileExplorer.tabs.closedTabHistorySize.description`, `fileExplorer.clipboard.empty`, `queryUi.empty.tip`,
  `queryUi.empty.tipAi`, `viewer.copyDialog.confirmBody`, `viewer.copyDialog.refuseBody` → wait for #1102 (key
  placeholders).
- `settings.advanced.showSafeSaveFiles.description` → macOS-only, gated (`safe_save_files`).
- `settings.servers.trustedCertificates.description` → proposal (the OS's trust store is what refuses first, on every
  OS).
- `settings.dataWipe.doneMessage` → proposal.
- `settings.ai.tooltipLocal` → macOS-only, gated (`local_ai`).
- `settings.ai.tooltipLocalDisabled` → macOS-only, gated (`local_ai`); the "yet" question disappears with the option.
- `settings.archives.bundle.description` → macOS-only, gated (`app_bundles`).
- `settings.advanced.exitFullScreenOnEscape.label` (and the other full-screen rows) → macOS-only, gated (`full_screen`).
- `commands.appQuit.label` → glossary: Windows "Exit Cmdr", Linux "Quit Cmdr". Only useful once `app.quit` is
  dispatchable off macOS (bugs).
- `menu.app.quit` → stays macOS-only (it's in the macOS app menu). The Linux "Quit" and Windows "Exit" rows are a new
  `linux_only` / `windows_only` File-menu row with its own key; recommended, since Linux has no keyboard way to quit
  today.
- `menu.app.settings` → proposal (Windows "Settings", Linux "Preferences"). The Linux toast button
  `fileExplorer.quickFilter.intro.openSettings` (proposal "Settings…") then names a different label than the menu row;
  see bugs.
- `menu.file.getInfo`, `menu.file.quickLook` → macOS-only, gated (`macos_only` in `bar.rs`).
- `main.dockPinNudge.title` → macOS-only, already gated (`dockPin`); a taskbar nudge is a new feature.
- `main.oldWebkit.title` / `.body` / `.quit` → write a Linux branch (Windows can't reach it, so it shares `other`):
  - title: "Cmdr needs a newer WebKitGTK"
  - body: "Cmdr’s interface needs a newer WebKitGTK than this computer has. Install your system’s pending updates. That
    should sort it out."
  - quit: "Quit"
- `onboarding.stepAi.localTooltip` → macOS-only, gated (`local_ai`).
- `onboarding.stepBeta.checklist.starNote` → macOS-only, gated (`homebrew_cask`).
- `onboarding.stepOptional.networking.summary` → proposal (Windows "Windows may ask to let Cmdr through its firewall",
  Linux "No permission needed").
- `onboarding.stepOptional.indexing.benefit1` → proposal.
- `onboarding.stepOptional.mtp.desc` → proposal (Windows: the first sentence alone, since WPD shares the device; Linux:
  the desktop-may-hold-it sentence).
- `fileExplorer.network.osMountFallback.message` → proposal (a number only on macOS, where it was measured).
- `fileExplorer.extensionChange.descriptionRemove` → `other` (file) branches per proposal; the `folder` branch stays
  macOS text, unreachable off macOS after the package-name fix.
- `fileExplorer.quickLookHint.finderComparison`, `.quickView` (and the other hint keys) → macOS-only, gated
  (`quickLook`).
- `fileExplorer.navigation.driveIndex.tooltipCoalesced` (and `…CheckRunning`, `…NoNextCheck`) → macOS-only; no variants
  until a non-FSEvents watcher reports overflow.
- `fileExplorer.quickFilter.intro.openSettings` → proposal (Windows "Settings", Linux "Settings…").
- `ai.cloudConsent.askCmdr.protectedContents` → Linux rewritten to match `sensitive.rs:RULES` exactly (no mail rule on
  Linux); Windows open, and a blocker (bugs).
  - Linux: "Some contents stay home whatever anyone asks: SSH keys, cloud and tool credentials, your keyrings, browser
    data, Cmdr’s own data, and any file holding a private key. Their names can still reach your provider."
  - A consent-copy change: check whether `CLOUD_AI_CONSENT_VERSION` must bump
    (`apps/desktop/src-tauri/src/tools/CLAUDE.md`).
- `ai.local.notInstalled` → macOS-only, gated (`local_ai`).
- `ai.secretError.genericBody` → proposal (decided, macOS text changes too).
- `downloads.shortcutRow.addModifier` → proposal (Windows lists Win; Linux lists Super). Glyph source may move to
  `key-capture.ts` with #1102.
- `fileOperations.transferDialog.macOnlyFiles.toggle` → neutral, keep everywhere. A Mac-formatted stick on Linux or
  Windows carries `.DS_Store` and `._*` too; showing it only when the selection holds such files would need a scan
  before the dialog, a separate feature.
- `goToPath.dialog.inputPlaceholder` → macOS and Linux "Type or paste a path, for example ~/Documents" (decided);
  Windows open (keep the macOS text until Windows path parsing lands).
- `servers.refusal.certificateUntrusted` → proposal for both branches (the OS store is the one to add it to). See bugs
  for the revoked-certificate case.
- `servers.sheet.nfsIdentityThisMac` → proposal (Windows defaults to 65534, so "the IDs Cmdr sends" is honest).
- `shortcuts.conflict.reservedByMacOs` → macOS-only, gated (native set macOS-only, #1102).
- `transfer.metadataClass.extendedAttributes` → Linux proposal ("Extended attributes"); Windows open (no Windows
  metadata bodies).
- `ui.toolAsk.openBundle.title` → macOS-only; a Windows "Run this program?" ask is new keys, out of scope.
- `mtp.connectedToast.bodyMac` + `.bodyOther` → Linux proposal; Windows has no body: WPD isn't exclusive, so
  `MtpConnectedToastContent.svelte` should render neither body line (and arguably no "Disable MTP…" button) on Windows.
  A code change, not a variant.
- `fileOperations.transferDialog.namedFolderHint` → Windows open (keep "/"; D1 makes `/` valid everywhere, but what the
  Windows destination field shows and accepts isn't built).
- `settings.listing.showHiddenFiles.description` → Linux proposal; Windows open.

Counts (by row; a row with an open Windows branch counts once under open):

- Proposal as is: 20.
- Proposal with a code-settled edit: 9 (`connectionReset`, `symlinkLoopErrno`, `nameTooLongErrno`, `appBased.serious`,
  `trashRefused.notPermitted`, `unmountRefusedBySystem`, `textSize`, `oldWebkit`, `mtp.connectedToast` Windows).
- macOS-only, gated or unreachable: 19.
- Wait for #1102: 7.
- Open on Windows (Linux settled): 6 (`adbBinaryPath`, `protectedContents`, `inputPlaceholder`, `namedFolderHint`,
  `showHiddenFiles`, `metadataClass.extendedAttributes`).
- Neutral, keep: 1 (`macOnlyFiles.toggle`).
- Code changes riding on verdicts: the package-name rule (`extensionChange`), the MTP toast body on Windows, and new
  Quit/Exit menu rows.

## Errors only macOS produces today

Write (meaning known on Windows and Linux; proposal text, with "In Terminal" → "In PowerShell" on Windows):

- `errors.listing.hostDown.suggestion`
- `errors.listing.lockUnavailable.suggestion`
- `errors.listing.notPermitted.explanation`: Linux drops the actor: "Cmdr wasn’t allowed to access `{path}`. This
  usually means the folder’s permissions don’t allow it, or a security policy (like SELinux or AppArmor) protects it."
- `errors.listing.notPermitted.suggestion`
- `errors.listing.pathNotFoundErrno.suggestion`
- `errors.listing.noPermissionErrno.explanation`
- `errors.listing.noPermissionErrno.suggestion`
- `errors.listing.diskFullErrno.suggestion`
- `errors.listing.notSupportedErrno.suggestion`
- `errors.listing.networkUnreachable.suggestion`
- `errors.listing.connectionRefused.explanation`
- `errors.listing.connectionRefused.suggestion`
- `errors.listing.symlinkLoopErrno.suggestion` (Windows text above)
- `errors.listing.nameTooLongErrno.explanation` (Windows text above)
- `errors.listing.hostUnreachable.suggestion`
- `errors.listing.quotaExceeded.suggestion`
- `errors.listing.diskReadProblem.suggestion`
- `errors.listing.unexpectedSystemResponse.suggestion`
- `errors.listing.remotePermissionDenied.explanation`
- `errors.listing.remotePermissionDenied.suggestion`
- `errors.provider.appBased.serious` (text above)
- `errors.eject.unmountRefusedBySystem` (text above)
- Partial: `errors.listing.authRequiredEneedauth.suggestion` Windows only (ENEEDAUTH is BSD; Linux has no such errno, so
  skip Linux). `errors.listing.attributeNotFound.suggestion` Linux only (ENODATA is Linux's ENOATTR; Windows has no
  xattrs, skip Windows).
- `errors.listing.readOnlyVolumeErrno.explanation` is neutral already.

Skip (no Windows/Linux path, or a macOS-only mechanism):

- `errors.listing.tccRestricted.*` (TCC), `errors.listing.emptyRootICloud.*` (iCloud volume)
- `errors.provider.macDroid.*`, `errors.provider.iCloud.*`, `errors.provider.macFuse.*`, `errors.provider.pCloudFuse.*`
  (macOS-only detection)
- `errors.write.trashRefused.message.locked`, `.suggestion.locked`, `errors.mutation.fileLocked` (lock flags read only
  on macOS; `linux.rs:flags_of` is empty, and Windows' Read-only attribute is a different thing)
- `errors.write.trashRefused.suggestion.noFullDiskAccess` (already behind `fullDiskAccess`)
- `errors.mutation.sipProtected` (SIP), `errors.mount.signInHelperStuck` (NetAuthSysAgent)

## Bugs spotted on the way

- **Linux index silently drifts on an inotify overflow**:
  `crates/cmdr-index/src/indexing/watch/watcher/platform/notify_backend.rs:translate_notify_event` drops
  `EventKind::Other`, which is how `notify` 8.2.2 reports `IN_Q_OVERFLOW`
  (`Event::new(EventKind::Other).set_flag(Flag::Rescan)`, `notify-8.2.2/src/inotify.rs`), and `overflow_flag` is a
  constant `false` whose comment ("`notify` never reports an overflow here") is wrong. A queue overflow loses events
  with no rescan. Fix: read `Flag::Rescan` into the overflow flag.
- **Local AI claims support on Linux ARM64**: `apps/desktop/src-tauri/src/ai/mod.rs:is_local_ai_supported` is
  `cfg!(target_arch = "aarch64") && (!cfg!(target_os = "macos") || …)`, so it's `true` on Linux aarch64 (and would be on
  Windows ARM64), while the bundled `llama-server` is the macOS ARM64 build. The `local_ai` capability above fixes the
  UI; the predicate should also require macOS.
- **Windows browser data isn't protected from the AI**, while the consent copy promises "browser data":
  `apps/desktop/src-tauri/src/tools/sensitive.rs:RULES` has no `AppData` entries (Firefox, Chrome, Edge, Brave profiles,
  Credential Manager files). Must land before the Windows build ships Ask Cmdr. Linux has no mail-client rule, so the
  Linux text above drops "email data".
- **adb never found on Windows**: `crates/cmdr-adb/src/server.rs:locate_adb_binary` joins `adb` without `.exe`
  everywhere, so `$PATH` and the SDK env vars can't match on Windows (task 46). Linux misses
  `~/Android/Sdk/platform-tools/adb`, Android Studio's default.
- **`expand_tilde` breaks D1 on Windows**: `crates/cmdr-fs/src/tilde.rs:expand_tilde` splices `dirs::home_dir()`
  verbatim, giving `C:\Users\ada/Documents`; D1 wants `C:/Users/ada/Documents`. Latent until the Windows backend lands.
- **No way to quit with the keyboard on Linux**: `app.quit` is a `NATIVE_SHORTCUT_COMMAND_IDS` entry (dispatch-exempt,
  rebind refused) whose only implementation is the macOS app menu's `Predefined::Quit`; Linux has neither a handler nor
  a menu row, so Ctrl+Q does nothing and can't be rebound. Same mechanism makes binding Ctrl+H on Linux report "reserved
  by macOS (Hide Cmdr)".
- **macOS package rules leak to Linux**: `PACKAGE_EXTENSIONS` makes any folder named `X.app`, `X.bundle`, `X.xcodeproj`,
  … a package on Linux (pkg icon, extension-change ask, bundle Enter policy).
- **Copy: a revoked certificate gets "add it to your trust store"**: `servers.refusal.certificateUntrusted` covers every
  non-approvable OS refusal, revoked and malformed ones included, where adding the certificate won't help. Affects the
  macOS text too; worth its own wording pass.
- **Inconsistent Linux settings label**: the proposal names the Linux menu row "Preferences" (`menu.app.settings`) but
  the toast button that mirrors it "Settings…" (`fileExplorer.quickFilter.intro.openSettings`). Pick one; "Settings…"
  matches the window title.
- **Log says "macOS kernel mount" on Linux**:
  `apps/desktop/src-tauri/src/network/smb_connect_failure.rs:log_direct_connect_failure` writes "Staying on the macOS
  kernel mount" and "reachable from this Mac" on every OS. Log-only.
- **`packages.rs` uses a negated cfg**: `apps/desktop/src-tauri/src/file_system/packages.rs` has
  `#[cfg(not(target_os = "macos"))] async fn look_up`, the shape `docs/guides/platform-seams.md` replaces with named
  arms (probably allowlisted by `desktop-rust-platform-cfg-placement`).

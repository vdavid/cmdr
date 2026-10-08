# Tauri commands: details

Depth. `CLAUDE.md` holds the must-knows; this file holds the per-file command inventory, the routing map for new
commands, and notable non-obvious placements.

## Per-file inventory

- **`ipc-types.ts`**: `TimedOut<T>` and `throwIpcError()`. ❌ No generic IPC error type: each command family ships its
  own typed enum, and a refusal a person reads crosses the throw as a `TypedFailure` (`$lib/ipc/typed-failure.ts`).
- **`index.ts`**: barrel re-export of everything below.
- **`file-listing.ts`**: virtual-scroll listing API, batch accessors (`getPathsAtIndices`, `getFilesAtIndices`),
  revision-checked `getSelectionSnapshot` (atomic selected paths and counts; pane protocol in
  `../file-explorer/pane/DETAILS.md` § Compare directories), `getFileBeside` (the row next to a named one, resolved and
  read under one backend lock), drag-and-drop, `pathExists`, `createDirectory`, `createFile`, sync status, font metrics,
  `getBriefColumnTextWidths` (Brief-view column measurement), `setListingIncludeHidden` (the pane's hidden-files
  setting, which picks its `directory-diff` rows).
- **`file-viewer.ts`**: viewer session only: open, seek, search (with `useRegex` / `caseSensitive` modes), close, word
  wrap menu, `viewerSetSearchInputFocused` (the search box's claim on the viewer bar's Edit > Cut / Paste), encoding
  pickers (`viewerSetEncoding` / `viewerGetEncodingOptions`), tail mode (`viewerSetTailMode`), `viewerReload`, and
  `showViewerContextMenu` (the native right-click menu).
- **`file-actions.ts`**: open file/URL, Finder reveal, Quick Look, Get Info, context menu (file / breadcrumb /
  volume-selector-row / parent-row), clipboard, the text editor pair, cloud actions (`cloudMakeAvailableOffline` /
  `cloudRemoveDownload`, iCloud Drive only), `googleDriveLinks` (a Drive item's `viewUrl` plus its `geminiUrl`, or
  `null` when the path isn't one we can identify, backing the three Drive context-menu actions from one resolution), and
  the "Open terminal here" trio. That trio: `listTerminalApps` (which terminal apps are installed, for the settings row
  and the first-use picker), `openTerminalHere` (answers with an OUTCOME, and throws `OpenTerminalFailure` only when the
  launch couldn't be attempted), and `terminalAppDisplayName` (a table lookup, for naming an app that has just been
  uninstalled). All three take the stored choice as an argument, since the frontend owns the settings store. The text
  editor pair takes it too: `openInEditor(path, appChoice, askAboutOtherEditors)` answers an `EditorOpenReport` (the
  outcome, the app that got the file, and whether other editors exist when asked) and throws `OpenInEditorFailure` only
  when the launch couldn't be attempted; `listTextEditors(appChoice)` (macOS) lists what LaunchServices calls a text
  editor, with `chosenId` as the form to store for a stored or freshly picked choice. `showFileContextMenu` closes with
  two fact objects: `PaneContextMenuFacts` (what the SURFACE contributes) then `ContextMenuTarget`, the TEXT the menu's
  header line shows about the right-clicked rows (`countText`, `sizeText`). Both are rendered on this side because both
  need locale-aware number formatting, which Rust's `menu_t` deliberately lacks; build them with
  `$lib/file-explorer/selection/context-menu-target`, ❌ never by hand. Rust still derives the target COUNT from `paths`
  and uses it to pick the header's shape. Omit `sizeText` for "no honest size"; ❌ never a zero. Full rationale:
  `$lib/file-explorer/selection/DETAILS.md` § Context-menu header. It also sends a `shortcuts` map the caller never
  passes: the private `boundShortcuts()` reads every bound combo out of the registry per popup, so the native menu
  labels its items from what the user has actually bound rather than from literals that go stale on a rebind.
  `PaneContextMenuFacts.canShowInFolder` is the explicit search-results-only menu fact; it is not inferred from
  `restrictDestinationActions`, because one says which action to add and the other says which destination actions to
  remove. The matching typed event carries the popup's primary `path` back to the main window.
  `showBreadcrumbContextMenu` rides the same map (its only argument is the eject target now), and both send the
  CANONICAL spelling — ❌ never `toDisplayShortcut`, whose glyphs Rust's converter turns into garbage.
  `src-tauri/src/menu/DETAILS.md` § "Where a CONTEXT menu's accelerator comes from".
- **`favorites.ts`**: user-editable switcher favorites: `addFavorite`, `removeFavorite`, `renameFavorite`,
  `reorderFavorites`, plus `stripFavoritePrefix` (recover the bare id from a `fav-…` switcher id). Listing rides
  `listVolumes` / `volumes-changed`; there's no `listFavorites`.
- **`icons.ts`**: icon fetching (`getIcons`, `getCustomFolderIconIds`, `refreshDirectoryIcons`) and cache invalidation.
- **`app-state.ts`**: MCP pane state, dialog open/close tracking, menu context, view settings, `showMainWindow`,
  child-window rect persistence (`get/setChildWindowRect`), `updateMenuAccelerator`.
- **`archive.ts`**: the encrypted-archive password: `setArchivePassword` / `clearArchivePassword` (the secret's only
  path out of the frontend), plus `notifyArchivePasswordPrompt` / `notifyArchivePasswordDismissed`, which mirror what
  the prompt is ASKING so `cmdr://state` can name the archive and the MCP `unlock_archive` tool can answer it. ❌ The
  password is never part of that mirror.
- **`write-operations.ts`**: copy/move/delete, conflict resolution, scan preview. (Size/duration formatting moved to
  `$lib/units`.)
- **`rename.ts`**: `checkRenamePermission`, `checkRenameValidity`, `renameFile`, `moveToTrash`.
- **`mutation-reply.ts`**: `awaitMutation` (behind `renameFile`, `createDirectory`, `createFile`) and
  `awaitClipboardPaste` (behind `pasteClipboardAsFile`, which also carries the created file). Those commands answer
  `stillRunning` past their deadline, and this waits for the matching `mutation-settled` event, so the promise resolves
  or throws with the real end, and `onStillRunning` tells the caller when to say the volume is slow. It listens BEFORE
  invoking, since the event can overtake the reply. Backend: `write_operations/DETAILS.md` § "A slow instant mutation
  says it is still running".
- **`operations.ts`**: the operation manager (queue window): `listOperations`, `cancelOperation(s)`, `pauseOperation` /
  `resumeOperation`, `pauseAll` / `resumeAll`, `dismissFailedOperation` / `dismissAllFailedOperations`, and the
  `onOperationsChanged` membership/status event.
- **`operation-log.ts`**: the operation journal's read API (`getRecentOperationLogEntries`, `getOperationLogDetail`)
  plus its two write entries. `rollbackOperation` reverses ONE operation and resolves as soon as the inverse is queued,
  throwing a typed `RollbackRefusalFailure` on a refusal; `undoOperations` reverses SEVERAL and resolves only with the
  whole tally, which is what Ask Cmdr's rename undo reports. ❌ Don't reorder the ids handed to `undoOperations`: the
  backend reverses them newest-first and the apply order breaks a same-second tie.
- **`quit.ts`**: the quit gate's two answers (`quitConfirm` / `quitCancel`, each returning the gate's typed
  `QuitAnswer`) and the two subscriptions, `onQuitRequested` and `onQuitCalledOff`. There is deliberately no
  cancel-every-operation wrapper anywhere in this directory: that command belongs to the gate, which calls it in Rust
  (`src-tauri/src/quit/`), and a frontend wrapper is how a window teardown once came to kill backgrounded transfers.
- **`storage.ts`**: `listVolumes`, `getVolumeSpace`, `watchVolumeSpace` / `unwatchVolumeSpace`, `ejectVolume`,
  `getBusyVolumeIds` (bootstrap for the eject-busy gate), `onVolumeContextAction`, `onVolumeConnectionChanged` (session
  health of any connecting volume, not just SMB), `checkFullDiskAccess`, `checkFullDiskAccessQuiet`,
  `getMacosMajorVersion`, `openPrivacySettings`, `openSystemSettingsUrl`.
- **`networking.ts`**: SMB host discovery, share listing, Keychain credential ops, mounting, direct-connection upgrade,
  in-place `reconnectVolume` and per-volume `disconnectSmbVolume`, plus `getVolumeSignInState(volumeId)` — what a
  sign-in on any volume would ask for (`'nothing'` / `'password'` / `'key_passphrase'`), backend-neutral and ❗ asked
  when the affordance renders, ❌ never kept from an earlier answer. `mountNetworkShare` throws a `MountFailure`
  (`asMountError` gets the typed refusal back), while the three `upgradeToSmbVolume*` wrappers return the backend's
  `UpgradeResult` as it comes: every outcome, a volume that's gone included, is an answer, never a throw.
- **`mtp.ts`**: Android MTP: device listing, connect/disconnect, file ops, transfer progress, volume copy.
- **`sftp.ts`**: SFTP servers minus connecting (that's `servers.ts`'s `connectServer` / `connectSavedPlace`): cancel,
  disconnect, the two-phase host-key approval, the saved-server list, and the password store. The whole frontend
  contract, including the approval sequence and the per-rung banner table: `crates/cmdr-sftp/DETAILS.md` § "Connecting
  from the frontend". ❗ Reconnecting an SFTP volume, and asking what a sign-in would want, use `networking.ts`'s
  `reconnectVolume` / `reconnectVolumeWithCredentials` / `getVolumeSignInState`, all three backend-neutral (two despite
  the name).
- **`webdav.ts`**: WebDAV servers minus connecting: cancel, disconnect, the saved-server list, the password store, and
  the unattended-reconnect query. No host-key step. The contract: `crates/cmdr-webdav/DETAILS.md` § "Connecting from the
  frontend". Reconnect and sign-in use the same three backend-neutral `networking.ts` commands SFTP does.
- **`s3.ts`**: S3 accounts minus everything the servers family already speaks (connect, cancel, disconnect, pin, forget
  all take S3 places): the ACCOUNT's secret trio, keyed on the provider choice plus the access key id, so every bucket
  under one key shares it, and the unattended-reconnect query. The model and every connect outcome:
  `crates/cmdr-s3/DETAILS.md`. Also `estimateOperationCost`, the list-price estimate the Copy, Move, and Delete dialogs
  ask once their scan preview settles (`apps/desktop/src-tauri/src/s3_costs/DETAILS.md`).
- **`licensing.ts`**: license status, activation, expiry, server validation.
- **`settings.ts`**: port checking, file watcher debounce, indexing toggle, MCP server control, AI subsystem commands.
  `cloudAiHostVerdicts(baseUrls)` answers each URL with the policy's own `ManagedAiRefusal | null` (local, no request).
  `startAiServer` and `startAiDownload` are raw invokes (generic commands specta skips), so their typed `LocalAiError`
  rejection arrives as `unknown`: `$lib/ai/local-ai-error.ts` restores it.
- **`logging.ts`**: `getDebugLogPath`, the log file this session writes (Help > View debug log).
- **`tab.ts`**: tab context menu: `showTabContextMenu`, `onTabContextAction`.
- **`function-key-bar.ts`**: the function key bar's one-item context menu: `showFunctionKeyBarContextMenu`,
  `onFunctionKeyBarHideRequested` (payload-less; the frontend owns the setting write and the toast).
- **`clipboard-files.ts`**: clipboard file operations: copy/cut files to system clipboard, read/paste, clear cut state.
- **`indexing.ts`**: drive-indexing commands (status reads `getIndexStatus` / `getVolumeIndexStatusById`, lifecycle
  `enable/disable/forget/rescan/clearDriveIndex`) plus the event listeners: typed `on*` wrappers over the `tauri-specta`
  `events.index*` helpers (scan/replay/aggregation progress + complete, rescan notification, dir-updated, memory
  warning, the coverage-branch pair, `onIndexCoveragePhaseStarted` — which phase of a first index is running, as the
  backend classified it — and `onIndexNeedsFreshScan`, a drive whose index was marked for a rebuild because the drive
  went away mid-write).
- **`ai.ts`**: AI lifecycle event listeners
  (`onAi{DownloadProgress,Starting,ServerReady,Verifying,Installing,InstallComplete,Extracting}`) over the `events.ai*`
  helpers.
- **`appearance.ts`**: one-shot OS environment reads (`getAccentColor`, `getShouldReduceTransparency`,
  `getSystemTextSizeMultiplier`, `getLocalizedSystemStrings`, `getOsLocales`) plus `onAccentColorChanged` /
  `onReduceTransparencyChanged` / `onSystemTextSizeChanged` / `onOsLocalesChanged` over the OS appearance, text-size,
  and live language/region-change events.
- **`menu-events.ts`**: `onViewModeChanged` / `onMenuSort` / `onMediaIndexFolderExclusion` / `onMediaIndexFolderChoice`
  over the direct (non-`execute-command`) native-menu events. The two media-index ones carry the right-clicked folder
  plus its target state; `listener-setup.ts` routes each into the ONE FE helper that also backs the Settings list.
  `onOpenWithCopyRefused` carries an "Open with" launch that couldn’t copy its file out of an archive or a repo’s
  history, for `../file-explorer/open-with-refused-bridge.ts`.
- **`directory-watcher.ts`**: `onDirectoryDiff` / `onDirectoryDeleted` over the file-watcher events (`onDirectoryDiff`
  casts the generated payload to the FE `DirectoryDiff` whose `entry` is the FE `FileEntry`).
- **`native-drag.ts`**: `onDragImageSize` / `onDragModifiers` (macOS drag overlay) + `onDragOutSessionStarted` /
  `onDragOutSessionComplete` (drag-out-to-Finder toasts).
- **`quick-look.ts`**: `onQuickLookKey` / `onQuickLookClosed` over the Quick Look panel events.
- **`downloads.ts`**: downloads-watcher commands (`downloadsWatcherStatus`, `goToLatestDownload`,
  `setGlobalGoToLatestShortcut`, `recheckDownloadsWatcherGate`) plus `onDownloadDetected` / `onGlobalShortcutFired` over
  the downloads-watcher + global-hotkey events.
- **`reveal.ts`**: "Reveal in Cmdr" (macOS). `drainPendingReveals`, the Settings row's `getRevealHandlerState` /
  `setRevealHandlerEnabled`, and `onRevealDelivered` over the payloadless `reveal-delivered` event (a reveal that
  actually moved a pane, which is what the first-landing notice listens for). Every command wrapper swallows the
  missing-command rejection other platforms give; the two handler wrappers answer `null` then, while a dev or E2E build
  gets a real answer carrying `blockedBy: 'notProductionBuild'`.
- **`restricted-paths.ts`**: `onRestrictedPathsChanged` over the TCC-restricted-path-set event.
- **`dialog-events.ts`**: window-management events: `onExecuteCommand` + `emitExecuteCommand` (the unified
  menu/cross-window relay), the MCP `dialog` lifecycle (`on{Open,Focus,Close}Settings` / `…FileViewer` / `…About` /
  `…Confirmation`, `onCloseAllFileViewers`, `onMcpSettingsClose`), `requestOpenSettings` (emit `open-settings` so the
  main window opens Settings on behalf of a window without window-creation perms), `onViewerWordWrapToggled`,
  `onViewerEditAction` (the viewer bar's Edit > Copy / Select all, which the viewer runs itself),
  `onViewerContextMenuAction` (the same pair from its right-click menu, always over the file),
  `onPersistRestrictedSetting`, and `requestForegroundOperation` / `onForegroundOperationRequested` (the queue window
  asking the main window to show one operation in its progress dialog; the payload is the id alone, because the registry
  snapshot both windows receive is the truth about everything else), and `onMouseNav` (macOS reads the mouse's back /
  forward navigation in AppKit, because a Logi Options+ mouse posts a swipe rather than a button;
  `routes/(main)/DETAILS.md` § Mouse back / forward buttons).
- **`git.ts`**: git-browser commands (`getGitRepoInfo`, `subscribeGitState` / `unsubscribeGitState`,
  `getGitStatusForPaths`) plus `onGitStateChanged` over the per-repo `git-state-changed` event.
- **`go-to-path.ts`**: ⌘G path resolution (`resolveGoToPath`) and the persisted recent-paths list (`getRecentPaths`,
  `addRecentPath`, `removeRecentPath`).
- **`tags.ts`**: macOS Finder color tags: `toggleTags` (toggle a color across paths) and `enrichTags` (patch fresh tag
  data into a cached listing).
- **`updates.ts`**: macOS custom updater: `checkForUpdate(trigger)` (a typed `UpdateCheckOutcome`, managed policy
  applied) / `updateWriteBlocker` / `downloadUpdate()` (no URL: the backend fetches what it offered) / `installUpdate`
  (throws `UpdateInstallFailure`) (see `$lib/updates/updater.svelte.ts` for the full flow and the non-macOS Tauri-plugin
  fallback).
- **`debug.ts`**: dev/benchmark IPC: `benchmarkLog` (join a frontend timing into the Rust benchmark timeline).
- **`usage.ts`**: `getLaunchDayCount`, the gate for usage-gated hints ("you've used Cmdr for a few days now"). Reads the
  on-device launch-day ledger Rust appends at startup; answers 0 when it can't, so a hint stays silent rather than
  firing on a guess. The ledger never leaves the Mac and is deliberately not a setting:
  `../../../src-tauri/src/usage/CLAUDE.md`.
- **`dock.ts`**: macOS Dock: `getDockPinState` (may we offer a tile, and is Cmdr already down there) and
  `addCmdrToDock`. Both turn an unreachable backend into a typed answer rather than a throw — `preferencesUnreadable`
  and `timedOut` — because their callers are a startup gate and a toast button, neither of which can hold an exception.
  `../../../src-tauri/src/dock/CLAUDE.md`.
- **`notifications.ts`**: `getNotificationPermission` (whether macOS will show Cmdr's banners; an unreachable backend
  reads as `unknown`) and `showNotification` (passes the typed `Result` through). Feature code calls
  `sendMacosNotification` in `$lib/notifications/` instead, which combines the two and never throws. Why the permission
  goes to `UNUserNotificationCenter`: the module doc of `../../../src-tauri/src/notifications.rs`.
- **`crash-reporter.ts`**: next-launch crash preview, dismiss, and send. Send crosses IPC with the preview's report id
  and separately consented optional email only; the backend-owned pending file remains the payload authority.
- **`error-reporter.ts`**: the error-report preview, send, and `saveErrorReportToDisk` (a command in every build: it's
  the only action when the organization turned reports off, `../error-reporter/DETAILS.md`).
- **`managed-policy.ts`**: the organization's MDM policy for the UI: `getManagedPolicy` and `onManagedPolicyChanged`,
  both carrying `ManagedPolicyView`. Its one caller loads the barrel lazily (`../managed-policy/CLAUDE.md`).

## Where to put new commands

- Viewer session (anything prefixed `viewer_*`) → `file-viewer.ts`.
- File listing display (listing API, sync status, font metrics) → `file-listing.ts`.
- Single-file actions (open, reveal, preview, context menu) → `file-actions.ts`.
- Icons (fetch, refresh, cache clear) → `icons.ts`.
- MCP pane/dialog state, menu sync, window lifecycle → `app-state.ts`.
- Copy/move/delete operations → `write-operations.ts`.
- Rename/trash → `rename.ts`.
- Volumes/disk access → `storage.ts`.
- Network/SMB → `networking.ts`.
- MTP/Android → `mtp.ts`.
- SFTP servers → `sftp.ts`.
- WebDAV servers → `webdav.ts`.
- S3 accounts → `s3.ts`.
- Licensing → `licensing.ts`.
- Settings/AI → `settings.ts`.
- Clipboard file operations (copy/cut/paste files via system clipboard) → `clipboard-files.ts`.
- Drive indexing (status, enable/disable/rescan) → `indexing.ts`.
- Git browser (repo info, live state subscription, per-path status) → `git.ts`.
- Downloads watcher (status, go-to-latest, global hotkey) → `downloads.ts`.
- Native system notifications (send, macOS permission) → `notifications.ts`.
- "Reveal in Cmdr" (`NSFileViewer` registration, the cold-start drain) → `reveal.ts`.
- ⌘G path resolution and recent paths → `go-to-path.ts`.
- macOS Finder color tags → `tags.ts`.
- App updater → `updates.ts`.
- OS appearance/environment reads → `appearance.ts`.
- Dev/benchmark IPC → `debug.ts`.

## Unused wrappers and commands

Every exported function or const in a sub-file needs a production caller outside this folder, and every `commands.*`
entry in `$lib/ipc/bindings` needs a live caller; otherwise it's deleted, wired up, or allowlisted with a reason in
`scripts/check/checks/desktop-ipc-unused-allowlist.json`. The `desktop-ipc-unused` check (fast lane) enforces both,
because nothing else can: knip counts the barrel's re-exports as uses, and rustc never calls a registered command
unused. Before it existed, 37 dead commands and 28 dead wrappers piled up (`docs/notes/ipc-dead-code-audit.md`).

- **Counts as a caller**: an import from `$lib/tauri-commands` (static, `await import(…)` with destructuring, or a
  re-export) in a non-test file; `commands.<name>` outside this folder or inside a live wrapper; a raw `invoke('name')`
  in production code or E2E (`test/e2e-*`). An export a sibling sub-file imports (`throwIpcError`) is folder plumbing
  and counts too.
- **Doesn't count**: test files, `test-*` harnesses, `vi.fn()` mocks, comments, and a wrapper calling a command when
  that wrapper is itself unused (both get reported).
- **Allowlisting**: an allowlisted wrapper covers the commands it calls. A surface shipped ahead of its UI, or a feature
  missing its UI, gets an entry naming why; the check drops entries that gained a caller. Mechanics:
  `scripts/check/checks/DETAILS.md` § "IPC dead code".

## Notable non-obvious placements

`ask-cmdr.ts` holds `onAskCmdrTurn`, the subscription every turn's progress arrives on (rail sends and the agent's own
wakes alike, keyed by conversation). `proposalReady` carries names for display only; the rename review's
`preflightBulkRename` and `applyBulkRename` calls send opaque proposal and row ids, never reconstructed paths,
destinations, or fingerprints. Each row also carries `RenameEvidence` (`RenameEvidenceSource` + a `detail` string): the
backend-verified reason for the name, mirroring Rust `RenameEvidence`. `detail` is model-authored, so render it as plain
text only (`../ask-cmdr/DETAILS.md` § The "Why this name" column).

- Size, rate, and duration formatting lives in `$lib/units`, not here: this module is IPC wrappers only.
- `listen` and `UnlistenFn` from `@tauri-apps/api/event` are re-exported through `write-operations.ts`.
- `getSyncStatus` and font metrics (`storeFontMetrics`, `hasFontMetrics`) live in `file-listing.ts` because they
  directly support file list rendering.
- `analytics.ts` is the one exception to "IPC wrappers only": alongside `trackEvent` it holds `itemCountBucket`, a
  deliberate MIRROR of the backend's `analytics::item_count_bucket`. A frontend event never crosses the Rust helper on
  its way out, and one product with two ideas of what "a lot" means makes the dashboard unreadable, so the copy lives
  next to the IPC every frontend event goes through. `analytics.test.ts` pins its boundaries against the Rust twin's own
  test; change one side and it tells you about the other. ❌ Don't invent a per-feature bucketing next to a call to it —
  the one documented exception is a count with a hard low cap of its own (open tabs cap at ten, where this ladder has
  two values across the whole range), and those say so at the call site.

`VolumeCopyConfig.destinationName` preserves the named local-copy request across IPC. Contract:
`../file-operations/transfer/DETAILS.md` § "Single-item destinations". `SourceItemInput.isDirectory` can supply the
known selection kind when a conflict probe checks an alternate name.

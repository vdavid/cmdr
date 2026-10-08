# IPC dead-code audit (2026-10-03)

Nothing checks the frontend↔backend IPC layer for dead code: `apps/desktop/knip.json` ignores
`src/lib/tauri-commands/**`, knip counts the barrel re-exports as uses, and a registered `#[tauri::command]` never looks
unused to rustc. So this audit read every exported wrapper in `src/lib/tauri-commands/*.ts` and every `commands.*` in
`src/lib/ipc/bindings.ts` against production callers, then asked **why** each unused one was unused: a dead leftover, or
a feature that's missing its UI.

## Method

- **Wrappers (A)**: an exported function with no import from a production file outside `tauri-commands/` (test files and
  `vi.fn()` harnesses excluded), dynamic `import('$lib/tauri-commands')` included.
- **Commands (B)**: a `commands.*` entry whose only frontend users are dead wrappers, or none. Each was then searched by
  its snake_case name for raw `invoke('…')` strings, E2E (`__TAURI_INTERNALS__.invoke`), the MCP executor, and Rust
  callers.
- **Why**: `git log -S` / `-G` for when the last caller vanished, the UI and settings for whether a person should be
  able to reach it, and `gh issue list --state all` for a matching issue.
- Classes: **DEAD** (superseded; the feature works another way), **GAP** (a user-visible capability is missing or
  broken), **PENDING** (groundwork for planned work), **UNSURE** (a product call).

Result: 37 commands and 28 wrappers deleted in commit `128e9f10b`; three gaps and three open questions below. No PENDING
items: the S3 wrappers flagged (`hasS3Credentials`, `deleteS3Credentials`, `getKnownS3Places`) turned out to be
superseded too, not groundwork.

## GAP

All three are fixed: the AI key in `0397ad6a2`, focus-settings in `df944874e`, and the memory-stop toast in `a18c3d84e`
(translations in `1eabde815`). Kept below as the record of what was missing.

- **`deleteAiApiKey` / `delete_ai_api_key`**: there's no way to remove a saved cloud AI key. Settings › AI › Provider
  saves on typing and the field starts empty behind a "your key is saved" placeholder, so a person can replace a key but
  never take one out of the Keychain. The command exists since `42bc5eaf7` (2026-05-14, "AI API keys: move to OS secret
  store") and never had a caller. No issue. **Fix**: a "Remove key" action beside the field in
  `ai-provider-setup/ProviderSetupSteps.svelte`, calling `deleteAiApiKey(providerId)` and re-reading
  `getAiApiKeyStatus`, with its `AiSecretFailure` worded like the save path's.
- **`onFocusSettings` / the `focus-settings` event**: MCP `dialog focus settings` emits `FocusSettings` to the main
  window and nothing listens, so the tool answers "OK: Focused settings" while the settings window stays where it was.
  The emit arrived with the MCP rewrite (`1061fad78`, 2026-02-05); no listener ever existed, before or after the
  typed-events migration (`5cb844a94`). Its siblings (`onFocusAbout`, `onFocusFileViewer`) are wired in
  `routes/(main)/listener-setup.ts`. No issue. **Fix**: focus the `settings` webview window directly in
  `mcp/executor/dialogs.rs` (it's a separate window, so no frontend hop is needed), then drop the event and wrapper.
- **`onIndexMemoryWarning` / the `index-memory-warning` event**: when the memory watchdog crosses 16 GB it stops EVERY
  volume's index (`MemoryWatchdogAction::StoppedIndexing`), and the person is never told; the index just goes stale
  until restart. The event is emitted with byte-precise figures, but no frontend listener has existed since the watchdog
  landed (`f1501ece7`, 2026-03-10) or after its typed migration (`5649f676c`). Related but not the same: closed #230.
  **Fix**: subscribe in the indexing state module and show a toast (or a status on the drive's badge) saying indexing
  paused to protect memory, with a restart hint; `StillGrowingAfterStop` stays log-only.

## UNSURE

- **`clearRecentSearches`, `clearRecentSelections`, and `clear_recent_paths` (no wrapper)**: "clear all" for the three
  recents stores (Search ⌘H, Selection, Go to folder). Built with the stores (`1f03ff49e` 2026-05-22, `7ce90bb35`
  2026-05-23) and never wired; each list removes entries one at a time. A "Clear history" row in each popover would use
  them; otherwise delete all three. Tracked in #359.
- **`media_index_dedup_clusters` and `media_index_search_tag` (no wrappers)**: the M2 media-index backend (`3a43ff32e`,
  2026-07-13) grew near-duplicate grouping and a tag-score filter, and neither ever got a surface. Tag search is
  reachable through MCP `search_photos` (it calls the crate's `images_with_tag` directly); duplicate finding isn't
  reachable at all. The open media issues (#223–#226) cover faces and captions, not these. Decide whether a "Find
  duplicate photos" view is planned. Duplicates are tracked in #360, the tag filter in #361.
- **`greet`**: the Tauri template's command, kept alive only by `lib/ipc/test-helpers.test.ts`, the mock harness's own
  smoke test. Deleted in `7265cacc5`; that test now drives `has_font_metrics`.

## DEAD (deleted in `128e9f10b`)

- **`cancelSftpConnect`, `cancelWebdavConnect`** (+ commands): superseded by `cancelServerConnect`; neither wrapper ever
  had a caller (added `b911c8004` / `60def8113`, the servers facade arrived alongside).
- **`disconnectSftpVolume`, `disconnectWebdavVolume`** (+ commands): superseded by `disconnectPlace`.
- **`forgetKnownSftpServer`, `forgetKnownWebdavServer`** (+ commands): superseded by `forgetServer` (which also drops
  the session). The Rust tests now call the stores' `forget` directly.
- **`hasSftpCredentials`, `deleteSftpCredentials`, `hasWebdavCredentials`, `deleteWebdavCredentials`,
  `hasS3Credentials`, `deleteS3Credentials`**: superseded by `hasServerSecret` / `forgetServerSecret`. The Rust
  functions stay, crate-internal, because that facade dispatches to them.
- **`getKnownS3Places`**: only `knownS3PlaceOf` used it; inlined there. The command stays.
- **`removeManualServer`** (+ command and stub): superseded by `forgetSavedSmbHost` (`2648a44da`, 2026-09-24). The MCP
  `remove_manual_server` tool calls `network::manual_servers::remove_manual_server` directly and still works.
- **`activateLicense`** (+ `activate_license` command, `licensing::activate_license(_async)`): superseded by
  `verifyLicense` + `commitLicense` (`0abc7049d`, 2026-03-06), which keeps an unvalidated key off disk.
- **`getFolderSuggestions`** (+ command): superseded by `streamFolderSuggestions` (`d681c8ded`, 2026-05-09). Its
  sanitizer tests now run through the streaming sanitizer.
- **`copyFiles`** (+ `copy_files`): every copy goes through `copyBetweenVolumes` since `4e1efab7e` (2026-04-03). The IPC
  contract tests moved to `move_files`, which the same-volume move still uses.
- **`scanVolumeForCopy`** (+ `scan_volume_for_copy`, the core `scan_for_volume_copy`, `VolumeCopyScanResult`, and their
  preview-only tests): never had a frontend caller; the transfer dialog previews with `startScanPreview`,
  `scanVolumeForConflicts`, and `destinationWriteAccess`. The transfer docs already said "nothing in the UI calls
  `scan_for_volume_copy` today". The ADB "undialed phone" test now drives `scan_volume_for_conflicts`.
- **The pre-volume MTP set**: `listMtpDevices`, `getMtpDeviceInfo`, `getMtpStorages`, `isMtpConnectionError`,
  `listMtpDirectory`, `deleteMtpObject`, `createMtpFolder`, `renameMtpObject`, `moveMtpObject`, `scanMtpForCopy`, plus
  `disconnect_mtp_device` (no wrapper), their commands, and their Windows stubs. Phones browse and mutate through
  `MtpVolume`, list as volumes through the device provider, and disconnect through Eject. Callers went in `e7f41d749`
  (2026-02-03, "MTP: Delete dead code") and `8e5cd8220` (2026-10-02, #159).
- **`listAdbDevices`** (+ `list_adb_devices`): never had a caller; ADB phones reach the switcher through the device
  provider (`733a25b6f`).
- **`list_directory_start`** (no wrapper; the live `listDirectoryStart` wrapper calls the streaming command): the
  synchronous listing command. Its core, `list_directory_start_with_volume`, stays as a `#[cfg(test)]` seam.
- **`cancel_all_write_operations`** (no wrapper): the quit gate calls the Rust function; the wrapper went on purpose in
  `6819a0661` (2026-08-10), and now the IPC door is closed too.
- **`list_active_operations`, `get_operation_status`** (no wrappers): callers went in `2c805eff5` (2026-02-14); the
  queue window reads `list_operations` and the progress events. `list_active_operations` and `OperationSummary` are
  gone; the Rust `get_operation_status` query stays (the journal and `transfer_sides.rs` read it).
- **`stop_drive_index`** (no wrapper): no frontend caller since the typed bindings; `disableDriveIndex` stops a drive.
- **`get_volume_index_status`** (no wrapper): the badges read `getVolumeIndexStatusById`.
- **`get_host_auth_mode`, `get_known_shares`** (no wrappers, + stubs): callers went in `2c805eff5` (2026-02-14); the
  servers hub reads `listSavedServers`. `smb_cache::get_cached_shares_auth_mode` went with them.
- **`has_smb_credentials`** (no wrapper, + stub): deliberately never probed, since every read can raise a Keychain
  prompt (`a0566ad6f`, 2026-09-25); the sheet reads `hasServerSecret`.

## Not dead (flagged by the scan, kept)

- `record_breadcrumb` (raw `invoke` in `error-reporter/breadcrumbs.ts`), `preview_friendly_error` (raw `invoke` in the
  debug window's error preview), `get_mcp_token` and `get_dir_stats` (E2E), `get_memory_diagnostics` (agent tooling,
  `docs/tooling/memory-debugging.md`; the MCP `memory_diagnostics` tool calls the Rust function).
- The four `TypedFailure` classes (`AdbConnectFailure`, `OpenInEditorFailure`, `OpenTerminalFailure`,
  `SavedPlaceFailure`): thrown and unwrapped inside their own modules.

## Follow-ups the deletion exposed

- `MtpDeleteScope::Tree` in `cmdr-mtp` has no caller left (the removed `delete_mtp_object` was the only one). Keep it as
  crate capability, or drop it with `tree_scope_still_removes_a_whole_subtree`.
- `list_directory_start_with_volume` is a test-only twin of the streaming listing pipeline; folding its three tests onto
  the streaming path (with a collector sink) would let it go.
- The Windows stubs (`stubs/mtp.rs`, `stubs/network.rs`) compile on no lane; they were edited by reading alone.
- **Done**: `VolumeScanError::SourceVolumeNotFound` / `SourceVolumeNotConnected` are gone (nothing, the stubs included,
  built them after the removed command).
- **Done**: the `desktop-ipc-unused` check now fails on an uncalled `tauri-commands` export or `commands.*` entry, with
  the UNSURE items above (plus `get_memory_diagnostics`, agent tooling) in its allowlist until they're decided. The GAP
  items are being wired up instead, so they get no entry. knip also sees `tauri-commands/` now. Rule:
  `apps/desktop/src/lib/tauri-commands/DETAILS.md` § "Unused wrappers and commands".

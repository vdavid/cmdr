# MTP frontend integration

UI and state for Android device browsing via MTP. The frontend is a passive consumer: the backend (`src-tauri/src/mtp/`)
auto-connects devices on USB hotplug and owns all connection orchestration.

## Module map

- `mtp-store.svelte.ts`: reactive device list + connection state; sets up the event listeners in `initialize()`
- `mtp-path-utils.ts`: parse/construct MTP paths
- `PtpcameradDialog.svelte` (macOS) and `MtpPermissionDialog.svelte` (Linux): manual-fix dialogs with a copyable command
- `MtpConnectedToastContent.svelte`: sticky toast shown on connect

## Gotchas

- **Copy lives in the `mtp.*` catalog**, resolved via `t()`/`tString()`/`<Trans>`; don't hardcode user-facing strings
  (`cmdr/no-raw-user-facing-string` is enforced here). `<Trans>` snippets for the dialogs go at markup top level, NOT
  inside `<ModalDialog>` (Svelte would treat them as the dialog's named props). See [DETAILS.md](DETAILS.md) § i18n.

- **Path format is `mtp://{deviceId}/{storageId}/{path}`, all slashes.** `deviceId` looks like `0-5`, `storageId` is a
  decimal number (for example `65537`). No colon separator, no hex, no vendor:product encoding. Each storage (Internal,
  SD card) is a separate volume with its own ID.
- **Frontend listens to exactly four events**, all in `initialize()`: `onMtpDeviceConnected`, `onMtpDeviceDisconnected`,
  `onMtpExclusiveAccessError`, `onMtpPermissionError`. There's no directory-changed listener. Don't reintroduce one
  without the backend emitting it.
- **The connect toast reads module-level `$state` (in `mtp-connected-toast-state.svelte.ts`), not props**: the toast
  system renders components with zero props, so the caller sets it via `setLastConnectedDeviceName()` before
  `addToast()` and the toast reads `getLastConnectedDeviceName()`. The state lives in a `.svelte.ts` module (not the
  toast's `<script module>`) so its exports type across imports. Gated by the `fileOperations.mtpConnectionWarning`
  setting (default `true`).
- **MTP can be disabled entirely** via the `fileOperations.mtpEnabled` setting (Settings > General > MTP). When off,
  devices disconnect and hotplug is ignored; the frontend just reacts to `volumes-changed` as usual.
- **`resetForTesting()` must clear every module-level field.** Tests call it instead of `vi.resetModules()` to skip the
  ~8s module re-parse penalty; new module state that it misses leaks across tests.
- **Clipboard (Cmd+C/X/V) is blocked for MTP** because the system clipboard needs local file paths. Copy/move route
  through the `Volume` trait (F5/F6); the UI suggests those instead.

Full details (storage-ID hex conversion at the IPC boundary, ptpcamerad auto-suppression flow, Linux udev rules at
`src-tauri/resources/99-cmdr-mtp.rules`, coarse cache invalidation): [DETAILS.md](DETAILS.md).

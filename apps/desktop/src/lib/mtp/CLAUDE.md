# MTP frontend integration

UI and state for Android device browsing via MTP. The frontend is a passive consumer: the backend (`src-tauri/src/mtp/`)
auto-connects devices on USB hotplug and owns all connection orchestration.

## Module map

- `mtp-path-utils.ts`: parse/construct MTP paths
- `PtpcameradDialog.svelte` (macOS) and `MtpPermissionDialog.svelte` (Linux): manual-fix dialogs with a copyable command
- `MtpConnectedToastContent.svelte`: sticky toast shown on connect

## Gotchas

- **Copy lives in the `mtp.*` catalog**, resolved via `t()`/`tString()`/`<Trans>`; don't hardcode user-facing strings
  (`cmdr/no-raw-user-facing-string` is enforced here). `<Trans>` snippets for the dialogs go at markup top level, NOT
  inside `<ModalDialog>` (Svelte would treat them as the dialog's named props). See `DETAILS.md` § i18n.

- **Path format is `mtp://{deviceId}/{storageId}/{path}`, all slashes.** `deviceId` looks like `0-5`, `storageId` is a
  decimal number (for example `65537`). No colon separator, no hex, no vendor:product encoding. Each storage (Internal,
  SD card) is a separate volume with its own ID.
- **There's no frontend device store.** The volume list is the one source of which storages exist (each is a
  `{device}:{storage}` volume); the root layout listens for connect (toast) and the two access errors (fix dialogs), and
  `pane/mtp-disconnect-watch.svelte.ts` for disconnect. There's no directory-changed listener. Don't reintroduce one
  without the backend emitting it.
- **The connect toast is gated by `fileOperations.mtpConnectionWarning`** (default `true`) and takes the device name as
  a prop, falling back to the translated `mtp.deviceFallbackName` when the backend sends none.
- **MTP can be disabled entirely** via the `fileOperations.mtpEnabled` setting (Settings > General > MTP). When off,
  devices disconnect and hotplug is ignored; the frontend just reacts to `volumes-changed` as usual.
- **Clipboard (Cmd+C/X/V) is blocked for MTP** because the system clipboard needs local file paths. Copy/move route
  through the `Volume` trait (F5/F6); the UI suggests those instead.

Full details (storage-ID hex conversion at the IPC boundary, ptpcamerad auto-suppression flow, Linux udev rules at
`src-tauri/resources/99-cmdr-mtp.rules`, coarse cache invalidation): `DETAILS.md`.

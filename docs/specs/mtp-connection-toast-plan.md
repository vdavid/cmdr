# MTP connection toast

Show a sticky toast when an MTP device connects, explaining what happened (ptpcamerad was suppressed) and giving users
control. Also add a "don't warn" setting and move MTP settings to their own section.

## Why

We auto-suppress `ptpcamerad` on macOS — a system daemon the user didn't ask to be killed. Users deserve to know what
happened, and should have an easy way to disable MTP support or suppress the notification. Commander One does this with
a modal alert, but we'll use a sticky toast that doesn't interrupt workflow.

## Changes overview

1. Move MTP settings from `General > File operations` to `General > MTP` (new subsection)
2. Add `fileOperations.mtpConnectionWarning` setting (checkbox, not switch — less prominent)
3. Create `MtpConnectedToastContent.svelte` — custom toast component shown on device connect
4. Add `SettingCheckbox.svelte` — new settings component (doesn't exist yet)
5. Add device name to `mtp-device-connected` event payload (backend)

## Design decisions

### Toast content and tone

The toast should be informative but not alarming. Content:

- Title: **"Connected to {device name}"** (or "Connected to MTP device" if name unknown)
- Body on macOS: "Cmdr paused the macOS camera daemon (ptpcamerad) to access this device. If you want to use this device
  in another app, disable MTP support in settings."
- Body on Linux: "If you want to use this device in another app, disable MTP support in settings."
- Controls: "Don't show again" checkbox, "OK" button, "Disable MTP..." link button

### Passing device name to a prop-less toast component

The toast system renders custom components with zero props (`<ContentComponent />`). To pass the device name, we use a
**module-level `$state` variable** in the toast component file. The `+layout.svelte` listener sets this variable before
calling `addToast`. The component reads it reactively. This follows the same self-contained pattern as
`CrashReportToastContent` (which knows its own toast ID by convention) — the component just also knows where to find its
data.

```ts
// In MtpConnectedToastContent.svelte (module context)
export let lastConnectedDeviceName = $state('MTP device')

// In +layout.svelte (before addToast)
import { lastConnectedDeviceName } from '$lib/mtp/MtpConnectedToastContent.svelte'
// ... inside the event handler:
lastConnectedDeviceName = event.deviceName ?? 'MTP device'
addToast(MtpConnectedToastContent, { id: 'mtp-connected', dismissal: 'persistent', level: 'info' })
```

Actually, Svelte 5 module-level `$state` uses `<script context="module">`. This is cleaner than a separate store file
for a single value.

### Toast behavior

- **Persistent** (sticky) — doesn't auto-dismiss. User must click OK or the X button.
- **Dedup via `id: 'mtp-connected'`** — if multiple devices connect, the toast updates in place (shows latest device
  name). We don't stack one toast per device.
- Dismissed by: OK button, X close button, or "Disable MTP..." link
- "Don't show again" checkbox: when checked, the next dismiss action (OK, X, or Disable MTP) also sets
  `fileOperations.mtpConnectionWarning` to `false`

### Removing old ptpcamerad toasts

The connection toast subsumes both old string toasts:

- `ptpcameradSuppressedUnlistenPromise` ("Paused macOS camera daemon for MTP access") — REMOVE
- `ptpcameradRestoredUnlistenPromise` ("Restored macOS camera daemon") — REMOVE

The "restored" toast is redundant because: (a) users don't care about the daemon being re-enabled, (b) it fires on
disconnect which is already visible (device disappears from volume picker). If MTP is disabled via the toast's "Disable
MTP..." button, the device disconnects and MTP is turned off — no need for a separate "daemon restored" toast.

### Setting: `fileOperations.mtpConnectionWarning`

- Key: `fileOperations.mtpConnectionWarning`
- Section: `['General', 'MTP']`
- Label: "Warn when a device connects"
- Default: `true` (show the toast by default)
- Component: `checkbox` — must add `'checkbox'` to the `SettingDefinition.component` type union in `types.ts`
- Placed right under the main MTP enabled toggle
- Positive polarity: `true` = warn (avoids double-negative confusion of "Don't warn" = unchecked)

### "Disable MTP..." button in toast

Must call `setSetting('fileOperations.mtpEnabled', false)` — NOT `setMtpEnabled()` directly. The settings-applier
already listens for `fileOperations.mtpEnabled` changes and calls the Tauri command. Calling the command directly would
desync the setting value from the backend state.

### SettingCheckbox component

Needs to be created. Uses `@ark-ui/svelte/checkbox` (Ark UI headless checkbox). Same pattern as `SettingSwitch`: reads
from settings store, subscribes to changes, calls `setSetting` on toggle. Visually smaller and less prominent than a
switch — just a standard checkbox with a label.

### Moving to `General > MTP` section

Both MTP settings move from `General > File operations` to `General > MTP`. This means:

- New `MtpSection.svelte` in `sections/`
- Remove MTP rows from `FileOperationsSection.svelte`
- Add to `SettingsContent.svelte`
- Registry entries get `section: ['General', 'MTP']`
- Sidebar auto-populates from registry (General is already in `sectionsWithSubsections`)

### Device name in event payload

Add `deviceName` to the `mtp-device-connected` event. The backend has `connected_info.device.product` (`Option<String>`)
at emit time. Use `connected_info.device.product.clone().unwrap_or_default()` — empty string on the frontend means
"unknown", fallback to "MTP device" in the toast display.

### No schema migration needed

New settings with defaults work via `getSetting()` fallback to the registry default. No `SCHEMA_VERSION` bump needed.

## Implementation

### Milestone 1: Settings restructure + SettingCheckbox

**Files:**

- `src/lib/settings/components/SettingCheckbox.svelte` — New component. Uses `@ark-ui/svelte/checkbox`. Same reactive
  pattern as `SettingSwitch` (read from store, subscribe, write on change). Minimal visual footprint.
- `src/lib/settings/types.ts` — Add `'fileOperations.mtpConnectionWarning': boolean` to `SettingsValues`. Add
  `'checkbox'` to the `SettingDefinition.component` type union.
- `src/lib/settings/settings-registry.ts` — Move `fileOperations.mtpEnabled` to section `['General', 'MTP']` (label and
  description already updated by David). Add `fileOperations.mtpConnectionWarning` with section `['General', 'MTP']`,
  label "Warn when a device connects", default `true`, component `checkbox`.
- `src/lib/settings/sections/MtpSection.svelte` — New section component with both MTP settings. The enabled toggle uses
  `SettingSwitch` (no `split`). The warning setting uses `SettingCheckbox` (no `split`), styled less prominently.
- `src/lib/settings/sections/FileOperationsSection.svelte` — Remove the MTP setting row (the `mtpEnabledDef` lookup, the
  `shouldShow` block, and the `SettingSwitch` import if no longer needed)
- `src/lib/settings/components/SettingsContent.svelte` — Import `MtpSection`, add
  `shouldShowSection(['General', 'MTP'])` block in the General sections group

**Testing:** Run `svelte-check`. Visually verify in Settings.

### Milestone 2: Backend — device name in event

**Files:**

- `src-tauri/src/mtp/connection/mod.rs` — Add `"deviceName"` to the `mtp-device-connected` event JSON:
  `"deviceName": connected_info.device.product.clone().unwrap_or_default()`
- `src/lib/tauri-commands/mtp.ts` — Add `deviceName?: string` to `MtpDeviceConnectedEvent`

**Testing:** Run clippy.

### Milestone 3: Toast component + wiring

**Files:**

- `src/lib/mtp/MtpConnectedToastContent.svelte` — Self-contained toast component with module-level `$state` for device
  name. Shows: title with device name, explanation about ptpcamerad (macOS) or generic text (Linux), "Don't show again"
  checkbox, OK button, "Disable MTP..." link button. Actions:
  - OK / X close: dismiss toast. If "Don't show again" checked, also
    `setSetting('fileOperations.mtpConnectionWarning', false)`
  - "Disable MTP...": `setSetting('fileOperations.mtpEnabled', false)`, dismiss toast, if "Don't show again" checked
    also write that setting
- `src/routes/(main)/+layout.svelte`:
  - Add `onMtpDeviceConnected` listener that checks `getSetting('fileOperations.mtpConnectionWarning')`, sets
    `lastConnectedDeviceName`, and shows the toast
  - Remove `ptpcameradSuppressedUnlistenPromise` and `ptpcameradRestoredUnlistenPromise` listeners and cleanup
  - Remove `onPtpcameradSuppressed` and `onPtpcameradRestored` imports (if no longer used elsewhere)
- `src/lib/mtp/index.ts` — Check if barrel exists; export `MtpConnectedToastContent` if it does

**Testing:** Run all checks. Verify toast appears on device connect, controls work.

### Milestone 4: Docs

**Files:**

- `src-tauri/src/mtp/CLAUDE.md` — Document `deviceName` in event payload
- `src/lib/mtp/CLAUDE.md` — Document the connection toast and warning setting
- `src/lib/settings/CLAUDE.md` — Update section list (MTP section added, checkbox component added)

## Parallel execution notes

Milestones 1 and 2 are independent and could run in parallel. Milestone 3 depends on both.

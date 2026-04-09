# MTP settings toggle

Add a setting to enable/disable MTP (Android device) support. When disabled, no USB hotplug watching, no device
connections, no ptpcamerad suppression. When toggled at runtime, takes effect instantly — no restart needed.

## Why

We now auto-suppress `ptpcamerad` on macOS, which is system-level behavior (killing a LaunchAgent). Users who don't
connect Android devices shouldn't have this happening. The toggle lets users opt out entirely, and gives a clear signal
that MTP support exists as a feature.

## Design decisions

### Where in settings: `General > File operations`

MTP is a file operations feature (transferring files to/from Android devices). It doesn't warrant its own section — it's
a single toggle. The `File operations` section already has 3 settings and can accommodate one more.

Not `Network` — MTP is USB, not network. Not `Advanced` — it's a meaningful user-facing feature toggle, not a technical
knob.

### Setting key: `fileOperations.mtpEnabled`

Follows existing dot-notation convention. Default: `true` (MTP support on by default — most users want it to just work
when they plug in a phone).

### Instant apply: disconnect all devices + stop watcher (or start watcher)

When toggled off: disconnect all connected MTP devices, restore ptpcamerad, and set a flag that prevents the watcher
from processing new events. When toggled on: clear the flag so the watcher resumes processing hotplug events, then
immediately check for already-connected devices.

**Why not stop/restart the watcher task?** The watcher uses `OnceLock` statics and an infinite async loop with no
shutdown channel. Refactoring it to support full stop/restart is significant complexity for a toggle that will rarely be
used. Instead, we add an `AtomicBool` gate: the watcher loop keeps running but `check_for_device_changes()` returns
early when MTP is disabled. This is the simplest correct approach.

### Backend reads setting at startup

The setting must be in `loader.rs` so the backend can configure `MTP_ENABLED` before `start_mtp_watcher` runs. The
watcher always starts (it's `OnceLock`-based, designed for single init) and initializes `KNOWN_DEVICES` + `APP_HANDLE`
regardless of the flag. But the `AtomicBool` gate prevents the initial auto-connect and hotplug processing when MTP is
disabled. This ensures the `OnceLock` statics are ready for when the user re-enables MTP later.

### In-progress transfers on disable

When MTP is disabled, active transfers will fail with connection errors. This is acceptable for v1 — the toggle is
rarely used, and the user is explicitly choosing to disable MTP. Documenting this behavior is sufficient.

### Platform visibility

MTP is macOS + Linux only. The setting should always be visible in the UI (it's harmless on platforms without MTP — the
backend command is a no-op). This avoids platform-conditional UI complexity for a single toggle.

## Implementation

### Milestone 1: Backend — gating MTP with an AtomicBool

**Files:**

- `src-tauri/src/mtp/watcher.rs` — Add `MTP_ENABLED: AtomicBool` (default `true`), `set_mtp_enabled()`. Gate
  `check_for_device_changes()` and the initial auto-connect loop on this flag.
- `src-tauri/src/commands/mtp.rs` — Add `set_mtp_enabled` Tauri command (thin async pass-through).
- `src-tauri/src/lib.rs` — Register the new command. Read `mtp_enabled` from settings at startup and call
  `set_mtp_enabled(value)` before `start_mtp_watcher`.
- `src-tauri/src/settings/loader.rs` — Add `mtp_enabled: Option<bool>` field, parse from `fileOperations.mtpEnabled`.

**Why the `AtomicBool` lives in `watcher.rs`:** The watcher is the orchestration layer that decides whether to
auto-connect. Connection manager doesn't care about the toggle — it connects/disconnects what it's told to.

**`set_mtp_enabled(enabled, app)` is async** (because `disconnect()` is async). The Tauri command calls it directly.

**When `enabled = false`:**

1. Set `MTP_ENABLED` to `false`
2. Get all connected device IDs from `connection_manager().get_all_connected_devices()`
3. For each, spawn `auto_disconnect_device(device_id)` (reuses existing pattern)
4. Clear `KNOWN_DEVICES` so re-enable detects all plugged-in devices as new
5. On macOS: call `restore_ptpcamerad()`

**When `enabled = true`:**

1. Set `MTP_ENABLED` to `true`
2. Call `check_for_device_changes()` to pick up any already-plugged-in devices (which will auto-suppress ptpcamerad if
   devices are found)

**Race condition note:** A hotplug event in flight when the flag is set may briefly auto-connect a device that then gets
disconnected. The worst outcome is a brief flash in the UI — harmless.

**Startup flow (explicit):**

1. `ensure_ptpcamerad_enabled()` (crash recovery — always runs, fine regardless of MTP setting)
2. Read `mtp_enabled` from settings, call `mtp::watcher::set_mtp_enabled_flag(value)` (just sets the AtomicBool, no
   async work)
3. `start_mtp_watcher(app)` — inits `OnceLock` statics, but initial auto-connect is gated on `MTP_ENABLED`

**Important:** In `lib.rs`, `load_settings()` currently runs at line ~359, _after_ `start_mtp_watcher()` at line ~335.
We need to move `load_settings()` earlier — before the MTP watcher starts — so the flag is set before any auto-connect
happens. `load_settings()` just reads a JSON file, no dependencies on anything initialized between lines 335-359.

**Platform gating:** All ptpcamerad calls in `set_mtp_enabled` must be wrapped in `#[cfg(target_os = "macos")]`, same as
the existing helpers in `watcher.rs`.

**Testing:** Run clippy. Run existing MTP unit tests (`cargo nextest run mtp`). Add a unit test for the `MTP_ENABLED`
flag gating.

### Milestone 2: Frontend — setting definition + UI + wiring

**Files:**

- `src/lib/settings/types.ts` — Add `'fileOperations.mtpEnabled': boolean` to `SettingsValues`.
- `src/lib/settings/settings-registry.ts` — Add setting definition with section `['General', 'File operations']`,
  `type: 'boolean'`, `default: true`, `component: 'switch'`.
- `src/lib/settings/sections/FileOperationsSection.svelte` — Add a `SettingRow` + `SettingSwitch` for the new toggle.
  Place it first in the section (it's the most impactful setting there).
- `src/lib/tauri-commands/mtp.ts` — Add `setMtpEnabled(enabled: boolean)` wrapper (it's an MTP command, not settings).
- `src/lib/tauri-commands/index.ts` — Export `setMtpEnabled`.
- `src/lib/settings/settings-applier.ts` — Add `case 'fileOperations.mtpEnabled':` calling `setMtpEnabled()`.

**Testing:** Run svelte-check. Run all checks. Verify the setting appears in Settings > General > File operations.

### Milestone 3: Docs + verification

**Files:**

- `src-tauri/src/mtp/CLAUDE.md` — Document the `MTP_ENABLED` gate and `set_mtp_enabled` command.
- `src/lib/mtp/CLAUDE.md` — Mention the settings toggle.
- `src-tauri/src/settings/CLAUDE.md` — Add `mtp_enabled` to Settings struct docs.

**Verification with MCP servers:**

1. Open Settings, navigate to File operations, verify the MTP toggle appears
2. With an MTP device connected: turn off the toggle → verify the device disappears from the volume picker
3. Turn the toggle back on → verify the device reappears (may take a moment for auto-connect)
4. With MTP disabled: plug in a device → verify it does NOT appear
5. Re-enable MTP → verify the already-plugged device gets detected

## Parallel execution notes

Milestones 1 and 2 touch different file sets and could technically run in parallel, but since milestone 2 needs the
Tauri command name from milestone 1, sequential is safer and simpler.

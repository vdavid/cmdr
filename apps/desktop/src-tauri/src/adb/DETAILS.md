# ADB (app side): details

The app side of `crates/cmdr-adb/`: how a device reaches the sidebar, when a volume is dialed, what an eject does, and
what the frontend calls. Read this before any non-trivial work here. The wire contract, the `Volume` answers, and the
error policy are the crate's (`crates/cmdr-adb/DETAILS.md`); what the backend still owes is
`docs/specs/android-adb-backend-follow-ups.md`. The seam both device backends register through is `device_volumes.rs`,
whose module doc is canonical for the trait.

## Where each thing lives

- **The crate** owns the protocol, the `Volume`, and the `track_devices` subscription with its reconnect backoff. It
  never names the registry, the listing, or `tauri`.
- **This module** owns the cached device state, the provider, the connect wiring, eject, and the commands. It is the
  only place that knows both the crate and the app.
- **`device_volumes.rs`** owns the provider registry, `append_device_volumes` (what `volume_listing::complete` folds
  over), `provider_for_volume_id` (what `eject.rs` asks before answering `EjectAction::DeviceDisconnect`),
  `device_volume_for_path` (what path resolution asks), and `notify_devices_changed`, the one push channel (it emits
  `volumes-changed`).
- **`commands/volumes.rs::resolve_path_to_volume`** is where an `adb://` path turns into a dial: it calls
  `volume_wiring::volume_id_for_path` before the generic device-provider lookup.

## Flows

**Startup** (`lib.rs` setup): `install_device_provider` registers `AdbDeviceProvider` beside `MtpDeviceProvider`,
then `start_adb_tracker` starts the `host:track-devices` subscription (a second call while one is running is a
no-op). The tracker talks only to the local server socket, never to USB. With no `adb` binary the tracker STOPS
itself and says so at `debug`: there is nothing to reconnect to, and retrying would warn every 15 s for the whole
session on the many machines that carry no Android tooling. A machine without the platform tools therefore sees
nothing and pays nothing after startup.

**Settings** (`fileOperations.adbEnabled`, default on; `fileOperations.adbBinaryPath`, empty for the platform
search): both are live-applied, and they travel TOGETHER through one `set_adb_settings` command, because the tracker
restarts under whichever binary the path names and pushing one alone would restart it under a stale one. The frontend's
`adb-settings.ts` re-reads both and pushes; `settings-applier.ts` wires either change to it. Startup seeds the path
BEFORE `start_adb_tracker`, or the first subscription runs against whatever the environment offered. ❗ Turning ADB off
stops the subscription AND empties the cached device list (`apply_device_list(Vec::new())`), which is what retires the
connected volumes and takes the rows off the switcher: a stopped tracker alone leaves the last list frozen on screen and
its volumes registered, so a phone looks browsable and answers nothing. The path reaches `cmdr_adb` through
`set_adb_binary_override`, consulted by `locate_adb_binary` ahead of `$ADB` and the rest of the search; ❗ it only wins
while it is RUNNABLE, so a path that went stale (an SDK moved, a typo saved) falls through to the search rather than
making every Android device vanish.

**The settings screen** (`apps/desktop/src/lib/settings/sections/AdbSection.svelte`, `File systems > Android (ADB)`)
renders both settings plus three things that are not settings: the status (`get_adb_install_status`), a Re-check button,
and, while no binary was found, a copyable `brew install android-platform-tools`. ❗ It calls `recheck_adb_install` once
per CLICK and `get_adb_install_status` once on mount, which is the frontend half of the no-polling rule below.

**Re-check** (`recheck_adb_install`): it stands for a person saying "I installed it now", so it clears the crate's
start-attempt memory (`cmdr_adb::forget_start_attempt`), starts a fresh tracker, and answers an `AdbInstallStatus`
(`binaryPath`, `tracking`) for the settings screen to render. `get_adb_install_status` is the same answer without
looking again. `apply_settings_at` clears the same memory when ADB is turned on or the binary path changes, for the
same reason: a newly named binary deserves the one attempt an existing one already spent. ❌ Nothing may poll any of
them: one `adb start-server` attempt per human action is what keeps the "never a retry loop that spawns processes" rule
true.

**Hotplug**: every push from `cmdr_adb::track_devices` (the full `host:devices-l` list, refetched by the crate on each
short-format push) lands in `device_provider::apply_device_list`, synchronously on the runtime. It stores the list,
and for every serial that left and had a volume, calls the volume's `note_device_gone` (the crate emits the
`Disconnected` transition once) and `VolumeManager::unregister` (which retires it). Then
`notify_devices_changed("adb")` → `volumes-changed` → the frontend refetches the list. When the server goes away the
crate reconnects with backoff (1 s doubling to 15 s) and redelivers the list, so a change missed while it was down is
caught up. That backoff is for a server that exists and went away; a MISSING `adb` binary ends the loop instead.

**Connect**: `connect_adb_device(serial, attempt_id)` answers an already-dialed volume's id without a second dial;
otherwise it files `attempt_id` in this module's own `AttemptTable` (`network/connect_wiring.rs`, ADB's table is its
own so a stray cancel from another backend's sign-in can't reach in) and
`cmdr_adb::connect_adb_volume(params, host, cancel)` runs the crate's four phases, the volume goes in through
`VolumeManager::register_if_absent` (never `register`: no OS mount can pre-register the id, and a repeated connect
must not retire a volume a pane is using), is remembered by serial, and `notify_devices_changed("adb")` lets
`volume_listing::complete` enrich the entry with its capabilities. Errors cross IPC as `AdbConnectOutcomeError`, a
typed mirror of `AdbConnectError` (`AdbNotInstalled`, `ServerUnreachable`, `DeviceGone`, `Unauthorized`,
`DeviceTooOld`, `TimedOut`, `Cancelled`, `Transport`); the frontend words each one in `adb-connect-errors.ts`.

**Cancel**: the id is the CALLER's, minted before the call, because a phone can sit on its "Allow USB debugging?"
prompt for as long as nobody picks it up and the pane has to arm its cancel button while that is happening.
`cancel_adb_connect(attempt_id)` answers whether a dial was running; a `false` is ordinary (a cancel racing a dial
that just finished finds nothing filed). A called-off dial leaves nothing behind: no volume registered, nothing
remembered, no `volumes-changed`. The one dial nobody mints an id for is a pane walking onto an `adb://` path, which
files under `adb-navigation:<serial>` so a repeat navigation replaces only its own entry.

**Eject**: `eject.rs` asks `provider_for_volume_id`, gets this provider, and answers
`EjectAction::DeviceDisconnect { provider: "adb", volume_id }`. `AdbDeviceProvider::eject` forgets the volume and
unregisters it; nothing is sent to the phone (`adb` has no per-client detach). The device stays in the cached list, so
it is listed again on the next `volumes-changed` and re-dialed on the next navigation. A device the user wants gone
for good is unplugged, or revoked on the phone.

## The provider's answers

`AdbDeviceProvider` answers from the cache, never the wire:

- `id`: `"adb"`.
- `entries()`: one entry per device that has a filesystem to offer, dialed or not: id `adb:<serial>`, path
  `adb://<serial>`, `fs_type: "adb"`, name = `AdbDevice::display_name()` (the model, falling back to the serial),
  `mount_is_read_only: false`, `usb_speed: None`, and a `device_readiness` from `readiness_of`:
  - `device` → `ready`.
  - `unauthorized`, `authorizing`, `connecting` → `waiting_for_authorization`. All three end at the same place the
    user is looking, the phone's own prompt, and hiding the row is what made that moment silent.
  - `offline` → `unavailable { offline }`; `no permissions` → `unavailable { no_permissions }`. The row is there so
    the reason can be its tooltip.
  - `recovery`, `bootloader`, `sideload`, and a state word the crate can't read are NOT listed: a phone that isn't
    running Android has no filesystem, and a row that can never open is worse than no row. `list_adb_devices` still
    returns them with their typed state.

  ❗ `device_readiness` is PRESENCE, never session health: `connection_state` stays `None` on a device row, so nothing
  enrolls a phone waiting for its Allow tap in the reconnect backoff (`cmdr_fs::volume::connection` carries the split).
  ❗ A `waiting_for_authorization` row resolving through `commands/volumes.rs::resolve_path_to_volume` still dials, and
  the dial answers `Unauthorized`. The frontend avoids that round-trip: it holds the pane's listing and renders the
  waiting state without dialing, and dials only once a broadcast says the row turned ready
  (`apps/desktop/src/lib/adb/DETAILS.md`, `src/lib/file-explorer/pane/DETAILS.md` § "A pane on a phone").
- `owns_volume_id`: any cached serial's id matches.
- `space_for_path`: the connected volume's `get_space_info` (`df -k` on the device), `None` until it is dialed.
- `eject`: above.

## IPC and frontend

- `list_adb_devices() -> Vec<AdbDevice>`: the cached list, typed states included.
- `connect_adb_device(serial, attempt_id) -> Result<volume_id, AdbConnectOutcomeError>`, and
  `cancel_adb_connect(attempt_id) -> bool`.
- `set_adb_settings(enabled, binary_path)`: the live apply above.
- Frontend: `src/lib/adb/` (`adb-path-utils.ts` for the `adb://` scheme beside `mtp://`, plus `adb-volume-label.ts`,
  `adb-connect-errors.ts`, `device-readiness.ts`, `adb-settings.ts`, and the `AdbHint` pair) and
  `tauri-commands/adb.ts`. The frontend is a passive consumer of `volumes-changed`, the posture `src/lib/mtp/CLAUDE.md`
  describes for MTP; its one active step is the connect a navigation triggers.

## Testing

Suites here drive `cmdr_adb::testing::FakeAdbServer` (the crate's `testing` feature is on for the app's dev targets):
the tracker's diff and inline retirement, the provider's listing answers, eject, `resolve_path_to_volume` on an
`adb://` path, and the transfer engine through the registry. A cell asserting on the protocol belongs in the crate:
`crates/cmdr-adb/DETAILS.md` § "Which side a test lives on".

## Deliberate non-goals

Two things read like gaps and are not. They live here because the spec that decided them is wiped, and because both are
the kind of "oversight" someone will otherwise fix.

- **An ADB volume is never indexed.** `cmdr-index` does not route `adb://`, and that is the intended end state. A phone
  is transient, its filesystem is large, and walking it over USB to fill an index would thrash the device and the cable
  for data that is stale the moment it is unplugged. Search inside an ADB pane is live filename search over the current
  listing.
- **Wireless pairing stays the `adb` server's job.** `adb pair` and its six-digit code have no surface in Cmdr and
  won't get one: pairing is a one-time terminal step with its own flow, and a device paired there arrives through
  `track-devices` exactly like a cabled one, so this module already serves it.

## Not wired yet

- An " (ADB)" name suffix when the same phone is also listed over MTP (`entries()` names the model alone). The frontend
  applies one from the volume list (`src/lib/adb/adb-volume-label.ts`); the merged one-row-per-phone listing that
  retires it is `docs/specs/later/adb-merged-phone-row.md`.
- The MCP `select_volume` tool can't reach an ADB device: `mcp/executor/nav.rs` validates a name against
  `volumes::list_locations` plus MTP, and a device row comes from the provider seam instead. (Go to path DOES answer
  for an `adb://` path, through `src/lib/go-to-path/scheme-intercept.ts`.)
- The real-device pass and the crate's own deferrals: `crates/cmdr-adb/DETAILS.md` § "Known gaps and follow-ups".

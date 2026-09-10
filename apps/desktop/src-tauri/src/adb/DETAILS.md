# ADB (app side): details

The app side of `crates/cmdr-adb/`: how a device reaches the sidebar, when a volume is dialed, what an eject does, and
what the frontend calls. Read this before any non-trivial work here. The wire contract, the `Volume` answers, and the
error policy are the crate's (`crates/cmdr-adb/DETAILS.md`); what the backend still owes is
`docs/specs/later/adb-backend-follow-ups.md`. The seam both device backends register through is `device_volumes.rs`,
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
- **`commands/volumes.rs::resolve_path_to_volume`** answers an `adb://` path with the cached row
  (`device_volume_for_path`) and ❗ never dials: resolution runs for a restored tab, Go to path, a drag, and every lap
  of a frontend retry, so a dial there turns a loop on a failing listing into hundreds of failed dials a second. The
  one dialer is the pane standing on the phone (`src/lib/file-explorer/pane/device-connect.svelte.ts` →
  `connect_adb_device`).

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
own so a stray cancel from another backend's sign-in can't reach in) and gets in line for the serial's dial.

❗ **At most one wire dial per serial** (`volume_wiring.rs`'s `IN_FLIGHT`). The Allow tap is exactly when several
callers reach for one phone at once (a second pane, a retry), and dialing each on its own would register the FIRST
volume but remember the LAST, so eject and `note_device_gone` would reach a volume no pane uses. A caller
that finds a dial running JOINS it and gets its answer; otherwise it starts one, a spawned task running
`cmdr_adb::connect_adb_volume(params, host, wire)`. The volume goes in through `VolumeManager::register_if_absent`
(never `register`: no OS mount can pre-register the id, and a connect must not retire a volume a pane is using), the
SAME `Arc` is remembered by serial, and `notify_devices_changed("adb")` lets `volume_listing::complete` enrich the entry
with its capabilities. One lock covers joining, withdrawing, and the dial's register-and-publish, so a caller that looks
again under it finds a volume that just landed rather than dialing a second time. ❗ The install happens only while the
cached list still carries the phone, and the look, the registration, and the remembering run under the state lock
`apply_device_list` stores a push under (`device_provider::install_if_listed`): a phone unplugged mid-dial answers
`DeviceGone` with nothing registered or remembered, never a dead volume the next plug-in is handed without a dial.
Errors cross IPC as
`AdbConnectOutcomeError`, a
typed mirror of `AdbConnectError` (`AdbNotInstalled`, `ServerUnreachable`, `DeviceGone`, `Unauthorized`,
`DeviceTooOld`, `TimedOut`, `Cancelled`, `Transport`); the frontend words each one in `adb-connect-errors.ts`.

**Cancel**: the id is the CALLER's, minted before the call, because a phone can sit on its "Allow USB debugging?"
prompt for as long as nobody picks it up and the pane has to arm its cancel button while that is happening.
`cancel_adb_connect(attempt_id)` answers whether a dial was running; a `false` is ordinary (a cancel racing a dial
that just finished finds nothing filed). ❗ A cancel is per ATTEMPT, never per dial: the called-off attempt answers
`Cancelled` at once, and the wire dial is called off only when its last joined attempt is gone (two panes on one phone
is the realistic case). A claim is withdrawn however its attempt ends, a dropped future included, so the last one
leaving always calls the dial off. A called-off dial leaves nothing behind: no volume registered, nothing remembered,
no `volumes-changed`. An attempt cancelled in the same instant its dial published keeps the dial's real answer,
because the volume is really there.

**A phone nobody has dialed** (listed, no registered volume: before the pane's connect lands, or after an eject)
answers every read without dialing. `read_directory_with_progress` refuses with `VolumeError::DeviceDisconnected`
(`streaming.rs::missing_volume_error`), and `path_exists` answers "couldn't tell" (`timed_out: true`). ❌ Never
`NotFound`: the frontend reads it as "this folder was deleted" and walks the pane up and off the phone. Both ask
`device_volumes::provider_for_volume_id`, so any device id a provider lists answers the same way; an id nobody owns
stays `NotFound`, as an unmount race is.

**Eject**: `eject.rs` asks `provider_for_volume_id`, gets this provider, and answers
`EjectAction::DeviceDisconnect { provider: "adb", volume_id }`. `AdbDeviceProvider::eject` forgets the volume and
unregisters whatever the registry holds under the id, remembered or not; nothing is sent to the phone (`adb` has no
per-client detach). The device stays in the cached list, so
it is listed again on the next `volumes-changed`, now without `capabilities` (enrichment fills them only for a
registered volume), and a pane standing on it holds its listing and dials again (`src/lib/file-explorer/pane/DETAILS.md`
§ "A pane on a phone"). A device the user wants gone for good is unplugged, or revoked on the phone.

## The provider's answers

`AdbDeviceProvider` answers from the cache, never the wire:

- `id`: `"adb"`.
- `entries()`: one entry per device that has a filesystem to offer, dialed or not: id `adb_volume_id(serial)`, path
  `adb_app_root(serial)` (`adb://<serial>`, the dialed volume's `root()`), `fs_type: "adb"`, name = `AdbDevice::display_name()` (the model, falling back to the serial),
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
  ❗ A dial against a `waiting_for_authorization` row would answer `Unauthorized`, so the pane skips that round-trip:
  it holds its listing, renders the waiting state, and dials only once a broadcast says the row turned ready
  (`apps/desktop/src/lib/adb/DETAILS.md`, `src/lib/file-explorer/pane/DETAILS.md` § "A pane on a phone").
- `owns_volume_id`: any cached serial's id matches.
- `space_for_path`: the connected volume's `get_space_info` (the phone's shared storage, whatever path on it is
  asked), `None` until it is dialed. It feeds the pane's indicator, which the poller keys by volume, so it stays one
  figure per phone; the copy pre-flight asks the volume per folder instead. Crate `DETAILS.md` § "The `Volume`
  answers, and why" has the `df` side.
- `eject`: above.

## IPC and frontend

- `list_adb_devices() -> Vec<AdbDevice>`: the cached list, typed states included.
- `connect_adb_device(serial, attempt_id) -> Result<volume_id, AdbConnectOutcomeError>`, and
  `cancel_adb_connect(attempt_id) -> bool`.
- `set_adb_settings(enabled, binary_path)`: the live apply above.
- Frontend: `src/lib/adb/` (`adb-path-utils.ts` for the `adb://` scheme beside `mtp://`, plus `adb-volume-label.ts`,
  `adb-connect-errors.ts`, `device-readiness.ts`, `adb-settings.ts`, and the `AdbHint` pair) and
  `tauri-commands/adb.ts`. The frontend is a passive consumer of `volumes-changed`, the posture `src/lib/mtp/CLAUDE.md`
  describes for MTP; its one active step is the dial a pane standing on a phone makes
  (`src/lib/file-explorer/pane/device-connect.svelte.ts`).

## Testing

Suites here drive `cmdr_adb::testing::FakeAdbServer` (the crate's `testing` feature is on for the app's dev targets).
`volume_wiring_test.rs` holds calling a dial off, one dial per phone (three concurrent callers, a joined attempt
cancelled while the other waits, every joined attempt cancelled; each holds the fake's answers so every caller is
provably in line first), a dial whose phone left the cached list while it was held (answers `DeviceGone`, leaves
nothing), the settings' live apply, the binary-path fallback, a pane listing a dialed phone through
`read_directory_with_progress` on an `adb://<serial>/sdcard` path (the cell that holds the prefixed spelling end to
end), and a listing and a `path_exists` on a listed, undialed phone (never `NotFound`); `device_provider.rs` holds the
provider's row answers, `serial_of_path`, and an eject of a volume the registry holds but the provider never
remembered; `commands/volumes.rs` holds resolving an `adb://` path without dialing
(the cell points `ANDROID_ADB_SERVER_PORT` at the fake, so a dial would be seen). Not covered here yet: the tracker's
diff and inline retirement, eject's round trip through `eject.rs`, and the transfer engine through the registry. A cell asserting on the protocol belongs in the crate: `crates/cmdr-adb/DETAILS.md` §
"Which side a test lives on".

## Deliberate non-goals

Two things read like gaps and are not. They live here because the spec that decided them is wiped, and because both are
the kind of "oversight" someone will otherwise fix.

- **An ADB volume is never indexed.** `BackendKind::Adb` answers `can_be_indexed: false`, so the switcher offers a
  phone no index badge and no first-connect prompt (the frontend's per-kind default answers before it's dialed), and
  `Index::start_volume` refuses one. That is the intended end state. A phone
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

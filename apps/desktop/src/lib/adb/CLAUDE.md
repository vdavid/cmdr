# ADB (frontend)

Everything the app shows about an Android phone reached over ADB: the `adb://` path spelling, the switcher row's
readiness, the words for a connect that stopped, the two settings, and the line offering USB debugging. The app-side
backend is `src-tauri/src/adb/`; the wire is `crates/cmdr-adb/`.

## Module map

- `adb-path-utils.ts` reads and writes `adb://<serial>[/device path]`, plus the predicates that treat MTP and ADB alike
  (`isDeviceVolumeId`, `isDeviceScheme`). `adb-volume-label.ts`: the "(ADB)" suffix.
- `adb-connect-errors.ts` turns every `AdbConnectOutcomeError` into a sentence plus what the pane may offer next;
  `device-readiness.ts` turns a `DeviceReadiness` into a switcher row's state.
- `adb-settings.ts`: the two settings and the one push that applies them. `AdbHint.svelte` + `should-show-adb-hint.ts`:
  the MTP pane's one line offering the fuller way in. The pane's side of a dial is
  `../file-explorer/pane/device-connect.svelte.ts`, ❌ not here.

## Must-knows

- **❗ A `waiting_for_authorization` row is OPENABLE, and that is the answer everything here turns on.** Opening it puts
  the pane on the state that walks in by itself once the user taps Allow; a disabled row is the silence people read as
  "Cmdr can't see my phone". Only `unavailable` rows are greyed.
- **❗ Readiness is PRESENCE, `connectionState` is session health.** A phone on its prompt has no session, so nothing
  enrolls it in a reconnect backoff. Two modules, two questions: `device-readiness.ts` here,
  `../file-explorer/navigation/connection-state.ts` there.
- **❗ `unauthorized` is a WAIT, not a refusal.** A dial that comes back unauthorized lands on the same waiting state a
  `waiting_for_authorization` row does, so ❌ never word it as a failure: nothing went wrong and the user is mid-tap.
- **❌ No inert affordance.** `deviceGone` and `deviceTooOld` carry NO recovery, so the pane renders the sentence with
  no button. A "Try again" that can only fail again is what this rule exists to prevent.
- **❌ Nothing a person reads may carry a serial, "adb", "transport", "daemon", or "socket".** The backend's diagnostic
  goes to the log; `$lib/error-messages/friendly-error-style.test.ts` enforces the vocabulary over every variant.
- **❗ Spell an `adb://` path only through `adb-path-utils.ts`**, which mirrors `cmdr_fs::volume::adb_volume_id`. The
  volume id (`adb-<slug>-<digest>`) does NOT carry the serial: read it from the volume's PATH.

The three outcome shapes, what each readiness makes of a row, why the tooltips are Android's words, the `/sdcard`
landing rule, and the hint's four gates: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.

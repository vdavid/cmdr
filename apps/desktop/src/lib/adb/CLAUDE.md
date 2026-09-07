# ADB (frontend)

Everything the app shows about an Android phone reached over ADB: the `adb://` path spelling, the switcher row's
readiness, the words for a connect that stopped, the two settings, and the line offering USB debugging. Up:
`../../CLAUDE.md`. The app-side backend is `src-tauri/src/adb/`; the wire is `crates/cmdr-adb/`.

## Module map

- `adb-path-utils.ts`: reads and writes `adb://<serial>[/device path]`, plus the predicates that treat MTP and ADB alike
  (`isDeviceVolumeId`, `isDeviceScheme`).
- `adb-connect-errors.ts`: every `AdbConnectOutcomeError` into a sentence plus what the pane may offer next.
  `device-readiness.ts`: `DeviceReadiness` into a switcher row's state.
- `adb-settings.ts`: the two settings and the one push that applies them. `adb-volume-label.ts`: the "(ADB)" suffix.
- `AdbHint.svelte` + `should-show-adb-hint.ts`: the MTP pane's one line offering the fuller way in.
- The pane's side of a dial is a sibling, ❌ not here: `../file-explorer/pane/device-connect.svelte.ts`.

## Must-knows

- **❗ A `waiting_for_authorization` row is OPENABLE, and that is the answer everything here turns on.** Opening it is
  what puts the pane on the state that walks in by itself once the user taps Allow; a disabled row is the silence people
  read as "Cmdr can't see my phone". Only `unavailable` rows are greyed.
- **❗ Readiness is PRESENCE, `connectionState` is session health.** A phone on its prompt has no session, so nothing
  enrolls it in a reconnect backoff. Two modules, two questions: `device-readiness.ts` here,
  `../file-explorer/navigation/connection-state.ts` there.
- **❗ `unauthorized` is a WAIT, not a refusal.** `readAdbConnectOutcome` answers three shapes, and a dial that comes
  back unauthorized lands on the same waiting state a `waiting_for_authorization` row does. ❌ Never word it as a
  failure: nothing went wrong and the user is mid-tap.
- **❌ No inert affordance.** `deviceGone` and `deviceTooOld` carry NO recovery, so the pane renders the sentence with
  no button. A "Try again" that can only fail again is what this rule exists to prevent.
- **❌ Nothing a person reads may carry a serial, "adb", "transport", "daemon", or "socket".** The backend's diagnostic
  goes to the log; `friendly-error-style.test.ts` enforces the vocabulary over every variant.
- **The tooltips are ANDROID's words, in a generic type.** `DeviceReadiness` is provider-agnostic and ADB is its only
  producer today, so `adb.readiness.*` is where the wording lives. A second provider answering readiness moves them
  behind a per-provider lookup rather than bending Android's onto it.
- **❗ Spell an `adb://` path only through `adb-path-utils.ts`**, which mirrors `cmdr_fs::volume::adb_volume_id`. The
  volume id (`adb-<slug>-<digest>`) does NOT carry the serial: read it from the volume's PATH.
- **The hint is silent when the phone already has an ADB row.** Telling someone to turn on what they turned on is what
  makes a hint a nag; `should-show-adb-hint.ts` is where that is one readable line.

`DETAILS.md` holds the three outcome shapes, the readiness table, the `/sdcard` landing rule, and the hint's gates.

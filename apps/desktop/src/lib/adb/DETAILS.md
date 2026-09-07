# ADB (frontend): details

The frontend half of Android-over-ADB: what a phone's row says, what the pane shows while one opens, where a first
navigation lands, and the one line that makes the feature findable. Read this before any non-trivial work here. The
app-side backend contract (the provider, the dial, eject, the settings) is `apps/desktop/src-tauri/src/adb/DETAILS.md`;
the wire is `crates/cmdr-adb/DETAILS.md`. Neither is restated here, and neither is why a phone's dial skips
`connect-flow.ts`: that is `$lib/servers/DETAILS.md` § The device dial.

## Two fields, two questions

`VolumeInfo` carries both `connectionState` and `deviceReadiness`, and conflating them is the mistake this split exists
to prevent.

- **`connectionState`** answers "how live is this session". Its consumers are the reconnect manager, the switcher's dot,
  and the Disconnect predicate (`../file-explorer/navigation/connection-state.ts`).
- **`deviceReadiness`** answers "is the device reachable at all". Only the device providers set it, and an ADB row's
  `connectionState` stays `null` forever.

A phone sitting on its "Allow USB debugging?" prompt is present and answering the daemon. Under one field it would look
like a dropped session and start a backoff loop that dials nothing until it gives up. `deviceRowState` here reads the
second field and answers only two things: whether the row opens, and what its tooltip says.

### What each readiness makes of a row

- Absent (`null`), the case every disk and every server lands on, and `ready`: the row opens, with no tooltip.
- `waiting_for_authorization`: the row **opens**, tooltipped "Waiting for you to allow USB debugging".
- `unavailable { offline }`: greyed, tooltipped to wake the screen or reseat the cable.
- `unavailable { no_permissions }`: greyed, tooltipped that this Mac can't reach the phone over USB.

The greying is `.volume-item.is-unavailable` plus `aria-disabled`, and `handleVolumeSelect` returns early on a row that
doesn't open, so the keyboard path and the pointer path refuse together.

**Decision / a `waiting_for_authorization` row opens.** The row is visible at all because hiding it left the one moment
a user needs feedback silent, and a row you can see but cannot open reproduces that silence one step later. Opening it
is what resolves it: the pane lands on the waiting state, and the tap on Allow walks it in.

**Decision / the tooltips are ANDROID's words, in a generic type.** `DeviceReadiness` is provider-agnostic and ADB is
its only producer, so `adb.readiness.*` is where the wording lives. A second provider answering readiness moves them
behind a per-provider lookup rather than bending Android's phrasing onto it.

## The three outcome shapes

`readAdbConnectOutcome` is total over `AdbConnectOutcomeError` (a `Record` keyed on the variant names, so a new backend
arm can't compile wordless) and answers one of three shapes rather than one sentence, because two of the eight variants
are not refusals:

- **`silent`**, for `cancelled`. The user pressed the button; saying anything about it is noise.
- **`waiting`**, for `unauthorized`. The phone is mid-prompt, carrying the same two sentences a readiness-driven wait
  carries, from `waitingForTheAllowTap()`, so the two producers can't drift into two vocabularies.
- **`refused`**, the other six, each with a `recovery`:
  - `open_settings`: `adbNotInstalled`. Only Settings can fix it (point Cmdr at an `adb`, or re-check after installing
    the platform tools), so the button deep-links to `File systems > Android (ADB)`.
  - `retry`: `serverUnreachable`, `timedOut`, `transport`. A second attempt can clear all three with nothing else
    changing.
  - `none`: `deviceGone`, `deviceTooOld`. ❗ The pane renders NO action row. An unplugged phone is fixed by the cable
    and an Android 6 phone by nothing, so a "Try again" here would be an affordance that is guaranteed to fail.

**Why the sentences live on the frontend.** Same rule as every other error surface in Cmdr: Rust classifies, the
frontend words. The backend's `AdbConnectError` carries a diagnostic string; that string goes to the log and never to a
person. `friendly-error-style.test.ts` runs the whole table through the writing rules plus a vocabulary ban of its own
(no serial, no "adb", no "transport", no "daemon", no "socket").

## Where a phone's first navigation lands

The rule is in `../file-explorer/navigation/path-navigation.ts::firstLandingOn`, applied in `determineNavigationPath`'s
default arm: the one reached only when there is no favorite target, no matching other-pane path, and no remembered
path for the volume.

An `adb://<serial>` volume path becomes `adb://<serial>/sdcard`. An Android device root is a kernel filesystem: `acct`,
`apex`, `bin`, `proc`, forty entries a person mostly cannot read, and the user's own files are one level in.

- The volume ROOT is unchanged. Backspace goes there and the breadcrumb shows it, so nothing is hidden.
- ❌ `/data` is not hidden or special-cased anywhere. It answers `PermissionDenied` on a locked phone and is exactly
  what someone came for on a rooted or debuggable one.
- MTP is deliberately not folded in: an MTP volume is already rooted at one STORAGE, so its root is the media tree
  rather than a kernel filesystem.

## The line offering USB debugging

`AdbHint.svelte` sits between a pane's breadcrumb and its listing. Without it the feature is undiscoverable: a phone
with USB debugging off is a plain MTP row and says nothing about ADB, so a user who would want the full filesystem never
learns it exists.

`should-show-adb-hint.ts` is the whole decision, pure and four-armed:

- Only on an MTP pane.
- Silent once `behavior.adbHintDismissed` is set (a hidden FE-owned setting, the shape `behavior.serversPinHintSeen`
  takes).
- ❗ Silent when the same phone already has an ADB row (two `mobile_device` rows under one name). USB debugging is
  already on; telling someone to turn on what they turned on is what makes a hint a nag.
- ❗ Silent when `fileOperations.adbEnabled` is off, because turning USB debugging on would then produce no row at all
  and the advice would lead nowhere.

**Decision / the "How" link points at Android's own instructions**
(`https://developer.android.com/studio/debug/dev-options`), not at a page under `getcmdr.com/help/`. Turning on
developer options is a vendor procedure that changes with each Android release, and Google ships it translated into more
languages than Cmdr has. A page of ours would be a second thing to keep true. If a Cmdr help page is ever written, this
is one constant to repoint.

The link goes through `openExternalUrl`, ❌ never as a plain `<a>` navigation: Tauri blocks that, and the link would
silently do nothing.

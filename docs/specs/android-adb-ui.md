# Android over ADB: making it land

**Problem.** The backend works and nobody can reach it. `crates/cmdr-adb` connects, lists, streams, and writes, but a
person with a phone on the desk has no way to say "open it": there is no connect flow, no device picker, no settings,
and no words for the six ways a connect can refuse. This spec is the UI half, and it takes positions rather than listing
options.

The backend contract it builds on is `android-adb-backend.md`; the wire is `crates/cmdr-adb/DETAILS.md`. Neither is
restated here.

## The eight decisions

### 1. One row per phone, not one per protocol

A Pixel plugged into a Mac with platform-tools installed is visible to Cmdr twice: MTP sees it, ADB sees it. Shipping
two switcher rows for one object on the desk is the easy path and it is wrong; it makes the user pick a protocol before
they have a question, and "Pixel 9" vs "Pixel 9 (ADB)" is not a choice anyone outside this repo can make.

**Decision:** the volume switcher shows **one row per physical device**. MTP is the default face, because it needs no
developer mode and covers what most people want (photos, music, documents). ADB is a mode you switch that row into, from
its context menu and from a control in the pane header: **"Show the full filesystem"**.

The match key is the serial, which both sides already have (`mtp_ids::device_id_for` prefers it; ADB's volume id is
`adb:<serial>`). Where only one protocol sees the device, the row is simply that one, unlabelled: a phone with USB
debugging off is an MTP row and says nothing about ADB.

This retires `adb.volumeLabelWithSuffix`, the "(ADB)" suffix key the backend shipped. Keep the key until the merged row
lands; delete it in the same commit.

**Cost:** a cross-provider identity pass in `device_volumes.rs` (fold entries by serial before handing the listing out),
and a per-row "active protocol" the pane remembers. It is the largest single item here and it is worth it: two rows for
one phone is a wart every future device backend would copy.

### 2. Non-ready devices are visible, disabled, and explained

`AdbDeviceProvider::entries()` lists only `Ready` devices. So a phone that is plugged in but not yet authorized is
invisible, and the user concludes Cmdr cannot see their phone. That is the worst possible failure: silence in the exact
moment the user is looking for feedback.

**Decision:** `unauthorized`, `offline`, `noPermissions`, `connecting`, and `authorizing` devices appear in the switcher
as **disabled rows carrying their reason** ("Waiting for you to allow USB debugging"). They are not navigable.
`recovery`, `bootloader`, and `sideload` stay hidden: those are states you booted into deliberately, and a file manager
has nothing to offer there.

### 3. Authorizing resolves itself

The "Allow USB debugging?" prompt is the one moment every Android developer knows. Today the user would tap Allow and
then have to go back to Cmdr and try again.

**Decision:** when the user opens an unauthorized device, the pane shows the guidance in place ("Check your phone and
tap Allow") and **proceeds on its own the moment the state changes**. `host:track-devices` already pushes the
`unauthorized` → `device` transition; the pane subscribes and navigates. No button, no retry, no modal. This is the
detail that will make the feature feel finished, and it is nearly free.

### 4. Connecting happens in the pane, never in a dialog

**Decision:** the first navigation into `adb://<serial>` renders a connecting state **in the pane**, with the standard
cancel affordance. Every refusal renders in the same place. ❌ No modal: a modal for a connect that usually takes under
a second is a flash of chrome, and it steals focus from a keyboard-first app.

The six refusals need their words back (the `adb.connect.*` catalog was removed when it had no call site):
`adbNotInstalled` routes to Settings, `unauthorized` is decision 3, `deviceTooOld` and `deviceGone` are terminal,
`serverUnreachable` and `timedOut` offer a retry. `transport` is the unclassified case and says the least.

### 5. Settings > File systems > Android (ADB)

**Decision:** one section, four controls, in this order.

1. **Enable Android debugging (ADB)** — a switch, default **on**. With no `adb` present the tracker stops itself, so on
   the many machines without Android tooling "on" costs one refused loopback connect at startup and nothing after.
   Mirrors `MTP_ENABLED`.
2. **Status** — "Found at `/opt/homebrew/bin/adb`" or "Not found", plus whether the device list is live. This is
   `getAdbInstallStatus()`.
3. **Re-check** — a button calling `recheckAdbInstall()`. ❗ One call per click; ❌ never on mount, never polled. It is
   the only path allowed to retry `adb start-server`.
4. **`adb` location** — a path override, persisted, feeding the endpoint's `binary`.

Control 4 is not optional polish. A GUI app on macOS does not inherit the user's shell `PATH`, so every developer whose
toolchain comes from mise, asdf, nix, or a custom SDK root has `adb` on their terminal `PATH` and invisible to Cmdr.
Without an override, those users conclude the feature is broken and we get the bug report. The backend already reads
`$ADB` first, so this is a settings row wired to that same lookup.

### 6. Panes open at `/sdcard`, not `/`

A device root is a kernel filesystem: `acct`, `apex`, `bin`, `proc`, forty entries a person does not want and mostly
cannot read.

**Decision:** the first navigation into a device lands on **`/sdcard`**, where the user's own files are. Root is one
Backspace away and the breadcrumb shows it, so nothing is hidden. `/data` stays listed and simply fails with
`PermissionDenied` on a non-rooted phone; ❌ do not special-case or hide it, because on a rooted or debuggable device it
is exactly what the user came for.

### 7. ADB volumes are not indexed, deliberately

**Decision:** `cmdr-index` does **not** route `adb://`, and that is the intended end state, not a gap. A phone is
transient, its filesystem is large, and walking it over USB to fill an index would thrash the device and the cable for
data that is stale as soon as it is unplugged. Search inside an ADB pane is live filename search over the current
listing.

Written down here because "the index doesn't cover ADB" reads like an oversight, and someone will otherwise fix it.

### 8. "Disconnect", not "Eject"

`adb` has no per-client detach. `AdbDeviceProvider::eject` forgets the volume and unregisters it; the device stays on
the cable and the next navigation re-dials it.

**Decision:** device rows that cannot truly be ejected say **"Disconnect"**. "Eject" carries a promise about
safe-to-unplug that we would not be keeping. MTP keeps "Eject", which it earns by closing the device session.

## Copy

All new strings are drafts for a human pass, per principle 4. The tone rules are `docs/style-guide.md`: no "error", no
"failed", conversational and actionable. Nothing may render a backend diagnostic; every refusal is worded from the typed
`AdbConnectOutcomeError` variant.

The section name is **"Android (ADB)"** rather than "ADB": the parenthetical is what a developer searches for, the word
before it is what everyone else recognizes. ❌ Never expose "adb server", "sync service", or "transport" in UI copy.

## What this does not cover

- **Wireless ADB pairing** (`adb pair`, mDNS). Deliberately left to the `adb` CLI: pairing is a one-time setup with its
  own six-digit-code flow, and a device paired in the terminal shows up here like any other.
- **Staging leftovers.** A crash mid-transfer leaves `<name>.cmdr-tmp-<pid>-<n>` on the phone. Backend follow-up, not
  UI: a sweep of stale staging names in the destination on connect.
- **A real-device pass**, which still gates all of this: the authorize prompt, `unauthorized` → `device` mid-session, a
  2 GB transfer, and a `/data` listing on a non-rooted phone.

## Order to build

1. Settings section (decision 5) and the connect surfaces (4). Together these make the feature reachable and
   diagnosable, and they need no new backend work.
2. Non-ready rows (2) and auto-proceed on authorize (3). The two that decide whether it feels finished.
3. `/sdcard` default (6) and "Disconnect" (8). Small, independent.
4. The merged device row (1). The largest, and the one to do last so the rest is not blocked behind it.

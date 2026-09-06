# What the ADB backend still owes

The backend is done. `crates/cmdr-adb` talks to the ADB server on `127.0.0.1:5037`, lists, streams, and writes over the
sync service, runs its verbs over `shell,v2`, and answers as a device-anchored `Volume` beside MTP rather than replacing
it. The development also added the seam MTP never had: `device_volumes.rs`, a provider registry the volume list folds
over, with `host:track-devices` as the first push-channel hotplug. The wire, the `Volume` answers, and the error policy
are canonical in `crates/cmdr-adb/DETAILS.md`; the app-side half is in `apps/desktop/src-tauri/src/adb/DETAILS.md`. This
file exists so what is left stays schedulable.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

Two things that look like gaps and are not. **Indexing an ADB volume is a settled non-goal**, decided in
`android-adb-ui.md` § "ADB volumes are not indexed, deliberately": a phone is transient and walking it over USB would
thrash the cable for data that goes stale on unplug. **The wire-level gaps** (no ranged `RECV`, so a resumed read
re-reads from zero) live in `crates/cmdr-adb/DETAILS.md` § "Known gaps and follow-ups", and the smaller app-side ones,
including a connect that isn't cancelable from the pane, in `adb/DETAILS.md` § "Not wired yet".

## 1. No real phone has ever run this

- **Problem**: every test is against the in-repo fake ADB server. Nothing has been observed on hardware: the authorize
  prompt, an `unauthorized` → `device` transition mid-session, a 2 GB `RECV` and `SEND`, or a `/data` listing on a
  non-rooted phone (which should answer `PermissionDenied` carrying the path).
- **Impact**: this gates everything else, including the whole UI spec, which says so itself. A fake server agrees with
  whatever the crate believes about framing and state transitions, so the first real device is where a wrong belief
  surfaces. Until it runs, the honest status is "works against our own mock".
- **Solution**: an Android phone with USB debugging on, plugged into the Mac, and the four cases walked by hand. Record
  what comes back in `crates/cmdr-adb/DETAILS.md` with the usual evidence anchor (device, Android version, date).
- **Size**: an afternoon, once a phone is on the desk.

## 2. Nobody can reach it from the UI

- **Problem**: a `Ready` device already on the volume list can be browsed, and that is the only way in. There is no
  connect flow, no device picker, no words for the six ways a connect refuses, and no settings. `isAdbPath` is exported
  and nothing outside its own test calls it; `connectAdbDevice` is the same.
- **Impact**: a phone that is plugged in but not yet authorized is invisible, so the user concludes Cmdr cannot see it.
  On top of that, a macOS GUI app never inherits the shell `PATH`, so every developer whose `adb` comes from mise, asdf,
  nix, or a custom SDK root has a working toolchain that Cmdr cannot find, with no override to point it at one.
- **Solution**: `android-adb-ui.md` owns this end to end, in eight decisions and a four-step build order. It covers the
  connect surfaces, non-ready rows, the Settings section with the `adb` path override and the
  `fileOperations.adbEnabled` twin of the MTP toggle, panes opening at `/sdcard`, and "Disconnect" rather than "Eject".
  Nothing on the backend side blocks it.
- **Size**: read the order in that spec. The merged one-row-per-phone item is the large one and is deliberately last.

## 3. ⌘G can't take an `adb://` path

- **Problem**: `go_to_path::resolve` has no scheme branch. An `adb://<serial>/sdcard` input isn't absolute to `Path`, so
  it joins against the focused pane's directory, misses on disk, and comes back as `NearestAncestor` pointing at
  nonsense. Typing a device path into the path bar cannot work.
- **Impact**: small in reach and sharp when hit, because the path bar is the keyboard-first way into a known location,
  and a user who copies a path out of a breadcrumb and pastes it back gets silently redirected. `mtp://` has the same
  hole, so this is one fix for both device schemes rather than an ADB-specific one. MCP is fine: `is_virtual_path` is
  scheme-generic, so `nav_to_path` passes an `adb://` path through, and `select_volume` sees the volume because
  `volume_listing::complete` folds the provider in.
- **Solution**: short-circuit a device scheme at the top of `resolve`, before the tilde expansion and the join, and
  classify it as a directory without touching the disk. The pane's own `adb://` navigation arm
  (`file-explorer/pane/navigate.ts`) is the authority on whether it's reachable, and it refuses honestly.
- **Size**: an hour or two, most of it tests for both schemes.

## 4. `sendrecv_v2` compression is off until it's measured

- **Problem**: the sync service's v2 packets can carry brotli, lz4, or zstd, and the crate sends none of them.
- **Impact**: unknown, which is the point. The device does the compressing, so a phone with a busy CPU may transfer a
  compressible tree slower with a flag on than off, and a USB 2 cable with an idle CPU may go much faster. Guessing
  either way is how a backend gets a throughput regression nobody can attribute.
- **Solution**: measure first, on real hardware, with the three algorithms against a compressible tree and an already
  compressed one (photos), then enable whatever wins and write the numbers into `docs/notes/`.
- **Size**: half a day of measuring; the flag itself is a few lines. Blocked behind item 1.

## 5. Wireless pairing stays the `adb` server's job

- **Problem**: `adb pair` and its six-digit code have no surface in Cmdr, and won't get one.
- **Impact**: none we intend to carry. Pairing is a one-time terminal step with its own flow, and a device paired there
  shows up in `track-devices` exactly like a cabled one, so the backend already serves it.
- **Solution**: leave it. Recorded here and in `android-adb-ui.md` § "What this does not cover" so it doesn't get
  rediscovered as a gap.
- **Size**: zero. It's a decision, not work.

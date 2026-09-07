# What the ADB backend still owes

The backend is done. `crates/cmdr-adb` talks to the ADB server on `127.0.0.1:5037`, lists, streams, and writes over the
sync service, runs its verbs over `shell,v2`, and answers as a device-anchored `Volume` beside MTP rather than replacing
it. The development also added the seam MTP never had: `device_volumes.rs`, a provider registry the volume list folds
over, with `host:track-devices` as the first push-channel hotplug. The wire, the `Volume` answers, and the error policy
are canonical in `crates/cmdr-adb/DETAILS.md`; the app-side half is in `apps/desktop/src-tauri/src/adb/DETAILS.md`. This
file exists so what is left stays schedulable.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

Two things that look like gaps and are not. **Indexing an ADB volume is a settled non-goal**, and so is wireless
pairing; both are written down with their reasons in `apps/desktop/src-tauri/src/adb/DETAILS.md` § "Deliberate
non-goals". **The wire-level gaps** (no ranged `RECV`, so a resumed read re-reads from zero) live in
`crates/cmdr-adb/DETAILS.md` § "Known gaps and follow-ups", and the smaller app-side ones in `adb/DETAILS.md` § "Not
wired yet".

## 1. No real phone has ever run this

- **Problem**: every test is against the in-repo fake ADB server. Nothing has been observed on hardware: the authorize
  prompt, an `unauthorized` → `device` transition mid-session, a 2 GB `RECV` and `SEND`, or a `/data` listing on a
  non-rooted phone (which should answer `PermissionDenied` carrying the path).
- **Impact**: this gates everything else, the shipped UI included. A fake server agrees with whatever the crate believes
  about framing and state transitions, so the first real device is where a wrong belief surfaces. Until it runs, the
  honest status is "works against our own mock".
- **Solution**: an Android phone with USB debugging on, plugged into the Mac, and the four cases walked by hand. Record
  what comes back in `crates/cmdr-adb/DETAILS.md` with the usual evidence anchor (device, Android version, date).
- **Size**: an afternoon, once a phone is on the desk.

## 2. Nobody can reach it from the UI ✅ SHIPPED, except the merged row

- **What it was**: a `Ready` device already on the volume list could be browsed, and that was the only way in. A phone
  plugged in but not yet authorized was invisible, so the user concluded Cmdr could not see it; and because a macOS GUI
  app never inherits the shell `PATH`, a developer whose `adb` comes from mise, asdf, nix, or a custom SDK root had a
  working toolchain Cmdr could not find, with no override.
- **Where it landed**: `servers-hub-plan.md` M4 and M5. Every device state is a switcher row with its readiness
  (`apps/desktop/src/lib/adb/DETAILS.md` § "The readiness table"); a phone waiting for its Allow tap opens into a pane
  state that walks in by itself; every refusal has words and its own recovery (§ "The three outcome shapes"); a first
  navigation lands on `/sdcard`; the eject slot says Disconnect; the Settings section carries the `adb` path override
  and the `fileOperations.adbEnabled` toggle (`src-tauri/src/adb/DETAILS.md` § Settings).
- **What is left**: one switcher row per phone rather than one per protocol, deferred as
  `later/adb-merged-phone-row.md`. Nothing waits on it, and the "(ADB)" name suffix is the stopgap until it lands.

## 3. ⌘G takes an `adb://` path ✅ SHIPPED

- **What it was**: `go_to_path::resolve` had no scheme branch, so an `adb://<serial>/sdcard` input joined against the
  focused pane's directory, missed on disk, and came back as `NearestAncestor` pointing at nonsense. `mtp://` had the
  same hole, so copying a path out of a breadcrumb and pasting it back was silently redirected.
- **Where it landed**: `servers-hub-plan.md` § D13, as a FRONTEND intercept rather than the Rust fix this file
  sketched. `apps/desktop/src/lib/go-to-path/scheme-intercept.ts` classifies a scheme input before the local resolver
  is asked, and its `DEVICE_SCHEMES` covers both device schemes in one move. The Rust resolver stays a local
  `std::fs::metadata` walk on purpose: teaching it schemes would mean a resolver that consults the saved-server stores
  it has no business reading.
- **Why the frontend, and how the three call sites share one function**:
  `apps/desktop/src/lib/go-to-path/DETAILS.md`.

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
- **Solution**: leave it. Recorded here and in `apps/desktop/src-tauri/src/adb/DETAILS.md` § "Deliberate non-goals" so
  it doesn't get rediscovered as a gap.
- **Size**: zero. It's a decision, not work.

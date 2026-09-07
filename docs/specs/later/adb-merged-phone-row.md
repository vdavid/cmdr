# One switcher row per phone, not one per protocol

The last item of the shipped Android-over-ADB work, deferred because it is the largest and nothing else waits on it.
Everything else that spec decided is now beside the code: the frontend half in `apps/desktop/src/lib/adb/DETAILS.md` and
`apps/desktop/src/lib/file-explorer/pane/DETAILS.md`, the backend half in `apps/desktop/src-tauri/src/adb/DETAILS.md`,
the wire in `crates/cmdr-adb/DETAILS.md`.

## The problem

A Pixel plugged into a Mac with platform-tools installed is visible to Cmdr **twice**: MTP sees it, ADB sees it. Two
switcher rows for one object on the desk is the easy path and it is wrong. It makes the user pick a protocol before they
have a question, and "Pixel 9" versus "Pixel 9 (ADB)" is not a choice anyone outside this repo can make.

The shipped stopgap is `apps/desktop/src/lib/adb/adb-volume-label.ts`: the ADB entry gets an "(ADB)" suffix **only**
when an MTP row shares its name, so a phone reachable one way keeps its plain name. That is a label, not a fix.

## The decision

The switcher shows **one row per physical device**. MTP is the default face, because it needs no developer mode and
covers what most people want (photos, music, documents). ADB is a mode you switch that row into, from its context menu
and from a control in the pane header: **"Show the full filesystem"**.

The match key is the serial, which both sides already have: `mtp_ids::device_id_for` prefers it, and an ADB volume id is
minted from it (`cmdr_fs::volume::adb_volume_id`). Where only one protocol sees the device, the row is simply that one,
unlabelled — a phone with USB debugging off is an MTP row and says nothing about ADB.

## What it costs

- A cross-provider identity pass in `apps/desktop/src-tauri/src/device_volumes.rs`: fold entries by serial before
  handing the listing out. The provider registry is already the right seam for it; what it lacks is a notion of two
  providers answering for one device.
- A per-row **active protocol** the pane remembers, and the header control that switches it.
- `adb.volumeLabelWithSuffix` and `adb-volume-label.ts` die in the same commit, along with the
  `deviceVolumeLabel(volume, volumes)` call in `VolumeBreadcrumb.svelte`.
- The `@key` for that message names the suffix as its whole reason to exist, so its ten translations go with it.

It is the largest single item of that effort and it is worth it: two rows for one phone is a wart every future device
backend would copy.

## What it does NOT change

- **Readiness stays per protocol.** A phone can be `ready` over MTP and `waiting_for_authorization` over ADB at the same
  moment, and the merged row has to say which face it is describing. `apps/desktop/src/lib/adb/DETAILS.md` § "The
  readiness table" is the rule the merged row inherits, ❌ not one to re-derive.
- **The pane's connect seam.** `device-connect.svelte.ts` keys on the volume's own id and path; a merged row still hands
  the pane one of the two ids when it opens.

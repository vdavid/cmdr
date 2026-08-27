# What archive browsing still owes

Browsing and editing archives shipped: `crates/cmdr-archive/` reads zip, the tar family, and 7z (encrypted ones
included, prompting for the password at extract time, or at browse time for a header-encrypted 7z), and
`apps/desktop/src-tauri/src/file_system/write_operations/archive_edit/` mutates a zip through a staged temp+rename,
local or remote-hosted. Those two `CLAUDE.md`/`DETAILS.md` pairs are the canonical account of all of it.

Three things are open. Each says what it costs and what would trigger it.

❌ Nothing here restates a mechanism that already has a home. Every item points at the doc that owns it.

## 1. A zip edit rewrites the whole archive, even to add one small file

**The gap**: every zip edit is an O(archive) temp+rename rewrite. Adding a 1 MB file to a 2 GB zip rewrites 2 GB, and
for a NAS-hosted zip it round-trips the whole archive over the wire twice.

**The design is settled, and it is not append-past-EOF.** `docs/notes/m-append-spike.md` is the canonical account:
append-past-EOF is a no-go (`ditto -x -k` forward-scans local file headers, stops at the old central directory, and
silently drops every appended entry, which crash-safety makes unfixable within the layout), and CD-rewrite deletes leave
deleted bytes recoverable. What to build instead is "clone + tail-rewrite": clone the archive to a sibling temp,
truncate at the old central-directory offset, append the new entries plus a fresh contiguous CD and EOCD, then atomic
rename. The result is a normal-structure zip with zero dead bytes, and the note carries the measurements and the
reader-compatibility matrix.

**Scope**: only tail-ADDs get the fast path. Delete, rename, and mid-archive edits stay on the O(archive) mutator, which
physically removes bytes and so has no remanence problem.

**The remote half waits on a crate API.** The server-side-copy analog (`FSCTL_SRV_COPYCHUNK` the retained bytes on the
share, upload only the new entry) needs a client-level server-side-copy API that `smb2` 0.20.0 doesn't expose; the
message layer is there, the request and the resume-key round trip aren't, and the write path hardcodes `offset: 0`. It
is the same missing API that leaves `Volume::copy_within` at `NotSupported` for SMB (`crates/cmdr-fs/DETAILS.md` §
"`Volume::copy_within`: letting a server copy for itself"), so one piece of `smb2` work unblocks both. Old Samba and NAS
firmware may lack copychunk, so this wants a capability probe and a fallback to today's pull-round-trip either way.

**Before shipping**: a manual Quick Look and Spotlight check on a machine with a foreground (the spike's headless Quick
Look daemon hung on every zip, layout-independent), plus property tests for the zip64 and data-descriptor paths.

**Trigger**: real archives feeling slow in practice, or the NAS case starting to matter. temp+rename is correct today.

## 2. Enter on a file inside an archive can't open it in an external app

**The gap**: Enter on a file inside an archive opens the built-in viewer (temp-extract, byte-capped, per-instance
reaper). "Open with <external app>" is not offered. `crates/cmdr-archive/DETAILS.md` § "Left for the follow-up
milestones" owns why: a detached launched app holds the file for an unknown lifetime and has no close event to hook, so
it can't reuse the viewer's session-scoped extract.

**The shape to build**, spiked and settled: clone the viewer's persist-extract module (`file_viewer/archive_extract.rs`,
described in `apps/desktop/src-tauri/src/file_viewer/DETAILS.md` § "Per-instance extract dir + startup reaper") into a
sibling open-with module with a startup-ONLY reaper. Its own per-instance dir under the app data dir with its own subdir
prefix, so neither reaper can touch the other's live temps; a uuid subdir per open, no dedup cache; and the launch path
swaps each archive-inner path for its freshly-extracted temp in `menu_handlers.rs`'s `open-with:` and
`OPEN_WITH_OTHER_ID` branches before calling `open_paths_with`, which is otherwise unchanged. The app data dir isn't
TCC-protected, so the startup reaper needs no full-disk-access guard.

**Rejected, so nobody re-derives them**: a session or refcount-scoped temp (no close event exists for a detached app,
which is the whole reason this was deferred); a dedup cache keyed by inner path (buys nothing, costs archive-edit
invalidation); an age or TTL background reaper (redundant, since the process boundary already marks every prior-run temp
abandoned); a shared extract dir (a second instance's startup reap would delete the first's live temps); and write-back
to the archive when the external app saves (inner files are read-only preview, so those edits are lost by design).

**The one unknown**: candidate listing happens at menu-build time (`commands/menu.rs` →
`file_system::open_with::compute_open_with_choices` → `URLsForApplicationsToOpenURL:`) against the inner path, which is
not a real file. LaunchServices generally maps extension to UTType without a stat, so it probably answers; if it
doesn't, list against a path carrying the right extension instead.

**Cost**: small. One module cloned, one startup init line, and the click-handler path swap. Everything but that seam is
direct reuse.

## 3. Editing a zip on an MTP device pulls and pushes the whole archive

**The gap**: a remote zip edit is pull-edit-upload-swap, which for MTP is O(archive) over USB in both directions.
In-place editing would touch `mtp-rs` (first-party, so the change is available to us). The remote-edit contract itself
is in `write_operations/archive_edit/DETAILS.md`.

**Trigger**: MTP zip editing seeing real use. It is a stretch item, and the correct-but-slow path works today.

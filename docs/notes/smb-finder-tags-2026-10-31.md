# Finder tags on a share: what a sweep costs, before and after smb2

Why a pane reads tags over `smb2` rather than through the macOS mount. The design:
`apps/desktop/src-tauri/src/file_system/DETAILS.md` § "Finder tags". What each server does, and what the engine under
this costs: `~/projects-git/vdavid/smb2/docs/notes/named-streams-and-xattrs.md`.

## Rig

- QNAP TS-464, QuTS hero, ZFS, Samba 4.20, from a Mac on Wi-Fi, over Tailscale (the route the mount takes). `smb2`
  0.39.0, macOS 27.0.1, 2026-10-31.
- 2,000 one-byte files, every 20th tagged (`crates/cmdr-smb/examples/tag_sweep_bench.rs`, release build), in a scratch
  folder removed afterward.
- The real sweep's shape: 500-path chunks, one after another (`pane/tag-sweep.ts`). "Mount" is what Cmdr did before: a
  serial `getxattr` through the macOS mount.

## Numbers

Files a second, three runs each, in the order they ran:

- **Mount, serial**: 102, 193, 181 (median 181). 10–20 s for the folder, slowest chunk 2.7–4.9 s.
- **`SmbVolume::read_xattr_many`, `Verified`**: 1,235, 1,186, 1,152 (median 1,186). 1.7 s for the folder, slowest chunk
  415–494 ms.
- **`SmbVolume::read_xattr_many`, `AssumePathsExist`** (what `enrich_tags` sends): 1,058, 1,111, 1,005 (median 1,058).
  1.9 s for the folder, slowest chunk 463–569 ms.

So the sweep got **5.8× faster** in the mode it uses, and a chunk now answers well inside the 2 s reply tier where
through the mount it outran it. The 102 was the mount's first pass, and the median sets it aside.

## Reading it

- `Verified` wins by about 10% here, because 95% of these files have no tag, and listing streams answers those in one
  round trip. `enrich_tags` still sends `AssumePathsExist`: smb2's own measurements have it twice as fast when every
  file is tagged (1,150 against 620 files a second) and steadier under load (p99 20–24 ms against 22–100 ms), and it
  keeps the same cost whatever the folder holds.
- The ceiling is `smbd` handling one connection on one thread, about 0.8 ms a file, so a scan-pool session would raise
  it. It isn't worth a login per folder for a sweep that already beats the screen.

## The proof, end to end

On the same share, 2026-10-31, with `diskutil unmount /Volumes/naspi` first (no entry in `mount`, no `/Volumes/naspi`):

- `tags_io_integration_test.rs::nas_a_seeded_folder_sweeps_and_toggles_with_no_mount`, through the app's own route (a
  real `SmbVolume` in the registry, a real listing): the 2,000-file sweep put the seed's red tags in the listing, and
  two toggles landed.
- Then a fresh mount: `NSURLTagNamesKey`, the API Finder's tags go through, read `["Blue"]` on the file tagged and `[]`
  on the one cleared, whose `xattr -px` was macOS's own 42-byte empty array. A file never tagged had no attribute at
  all.
- The live app, direct (`connectionState: direct`) on the same folder, with the mount still up: rows showed their tags,
  the `tag` tool's toggle, set, and clear landed, and a tag written from the kernel mount (another session, so another
  client) reached the pane through the watcher.

**A plain unmount ends the direct session too.** `volumes/lifecycle.rs::handle_volume_unmounted` unregisters a share's
volume when its last mount leaves, and `on_unmount` drops the smb2 session, so the pane goes home. The state this work
protects is the one `Volume::note_root_mount_gone` names (a mount proven dead by an errno, with the session still up),
which is what the no-mount test sets up. Keeping a direct pane alive across a deliberate unmount is a separate product
question.

**Copies drop tags, both ways.** Verified in the live app on the share: a red file copied from the share to disk and one
from disk to the share both landed with no `_kMDItemUserTags`. That's #405, whose named-streams blocker is now gone.

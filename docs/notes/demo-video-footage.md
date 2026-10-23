# Demo video footage (2026-10-23)

The raw footage for the 90-second demo video (#693), recorded from the real app by `pnpm marketing:clips --video` on the
night of 2026-10-22. It's input for the video edit, not a deliverable. How the pipeline works:
`docs/guides/product-clips.md`.

## Where it is

- **Masters**: `brand/clip-masters/video/{beat}-dark.mov`, gitignored, so they exist only on the machine that recorded
  them (David's laptop). ProRes 4444, variable frame rate up to 60 fps (frames arrive only when the window changes, and
  the last frame holds until the stop), about 2.3 GB for all seven.
- **Manifest**: `brand/clip-masters/video/video-beats.json`, one entry per beat: `file`, `duration`, `width` / `height`,
  `usable` (start and end in seconds from the master's start; outside it is about 1.5 s of slack for cuts), `keyFrame`,
  and timestamped `notes` naming each key moment. An editor should read the timings from there, not from this note.
- **Look**: dark theme, main window 1280x800 at 125% zoom, so the masters are 2560x1600 (the `care` beat is the Settings
  window, 1704x1500). Traffic lights and title bar are real; no mouse pointer is shown.
- **Content**: everything on screen is the staged demo tree at `/Users/Shared/Studio` (a small studio's shared drive
  with made-up names). Nothing personal is in frame. The status bar shows the real disk's free space, and the search
  empty state shows the real index's entry count.

## The beats

- `speed` (14 s usable): Enter on a folder of 50,000 files, the listing appears after about 1.5 s, PageDown ten times,
  End to the last file, Home, then a search for `sensor-41873` finds the one file. The ~1.5 s blank while the folder
  loads is honest; trim it if it drags.
- `trust` (14 s): F5 on 64 photos (284 MB), the copy dialog, the live progress bar and file tree, Pause, Resume,
  Rollback with its confirmation, the bar walking back to zero, and the "Removed all N items" toast.
- `rename` (9 s): 30 photos selected, Multi-rename opens, `Iceland [Y-M-D] [C:3]` typed at human speed (the field shows
  "a placeholder is missing its closing ]" mid-typing, as it really does), the preview filling, Rename, the new names.
- `reach` (21.5 s): Enter on `Brand assets.zip` and Cmdr's first-time "Browse like a folder / Open with default app"
  menu, inside the zip, into its folder, back out, into the `website` Git repo, into `.git` where the history is
  folders, `commits` (one folder per commit, named by hash and message), back, `branches` (`main`, `redesign`).
- `ai` (12 s): a real natural-language search on Cmdr's built-in local model (Ministral 3B, downloaded into the clips
  instance through `start_ai_download`; no API key). "tax report from last year" typed in "Ask anything" mode, Enter,
  and the dialog shows what the model chose (`(?is)tax.*\.(pdf|doc|…)`, modified 2025-01-01 – 2026-01-01) and the two
  2025 tax files, out of three years of them plus a non-tax decoy from 2025.
- `hero` (10 s): a calm two-pane window, the cursor walking down the photos, Tab to the other pane, a few rows, Tab
  back.
- `care` (11 s): the Settings window, the Language picker open, ArrowDown through seven languages (each re-renders the
  window live), English picked back, then Theme mode set to Light and the window turning light.

## What's missing or rough

- **No network share**: the SMB fixtures aren't trivially available on this machine, so `reach` has no NAS. Phones were
  out of scope.
- **`reach` sometimes needs its retry**: on the first take after a launch the zip's open-with menu was slow to appear
  once. The scene now answers the menu, and the last run passed first time.
- **The masters are big**: ProRes 4444 at 2x is roughly 25–60 MB a second. Transcode before sharing them anywhere.

## App behavior the recording surfaced (possible bugs, not investigated)

- **The language picker's preview can stick.** In Settings > Language, arrowing through languages and then picking
  English again left the SAVED language on the last previewed one (Vietnamese): windows opened later in the same session
  (the viewer) came up in Vietnamese until the next launch. The clips work around it by recording that beat last.
- **A dispatched `nav.parent` goes up two levels.** Emitting `execute-command` with `nav.parent` logged the action twice
  and navigated to the grandparent. Pressing ⌘↑ goes up one level as expected. The menu's Go > Parent may take the same
  path; worth checking by hand.
- **A folder created under the running app can show `<dir>` for minutes.** A demo tree deleted and recreated while the
  app ran read `<dir>` / `≥` folder sizes, with "Parent path not found in index" in the log, until the next launch. The
  pipeline now stages the tree before launching.

## Re-recording

```bash
pnpm marketing:clips --video                 # all seven beats, about 5 minutes
pnpm marketing:clips --video --only reach,ai  # some of them; the manifest keeps the others
```

Leave the machine alone while it runs, and keep the display awake (`caffeinate -dimsu`).

## Phase 3: the second video, "safe file operations, to and from anywhere"

Recorded by `pnpm marketing:clips --video2` on 2026-10-23, into `brand/clip-masters/video2/` (gitignored, about 3.2 GB)
with its own `video-beats.json` in the same shape. Same look: dark, 1280x800 at 125% zoom (2560x1600 masters), slack
around a usable span, named moments in `notes`. The editor-facing caveats are in each beat's `remark`.

### Setup it dresses up

- **NAS**: the repo's Docker SMB fixture with a password (`smb-consumer-auth`, port 11481), refilled with `Photos/2024`
  (5,000 photos, sparse files), `Photos/2025/Lisbon trip` (36 real-sized photos), `Documents`, `Family videos`, and
  `Backup`. Added as "Home NAS" and mounted through the production path (`connect_to_server`, `mount_network_share`),
  which upgrades to a direct smb2 connection.
- **Bucket**: the VersityGW S3 fixture (port 14480) with a `family-photos` bucket of `2024/Christmas`, `2025/Easter`,
  and a README.
- **Phone**: the virtual MTP device, launched as "Pixel 8" (`CMDR_VIRTUAL_MTP_MODEL`, a small addition to `cmdr-mtp`'s
  virtual device), its `DCIM/Camera` filled with 40 `PXL_…` photos after launch.
- **Restic**: a `Backups` repo in the demo tree (password in `marketing-video2-fixtures.ts`), two snapshots of the
  documents and camera roll.
- ❗ The SMB fixture's own seed files are replaced in the running container; restarting it brings them back for the E2E
  suites. Both Docker stacks were left running under the `manual` lease (`stop.sh` in each releases it).

### The beats

- `smb` (16.5 s usable): the NAS root, into `Photos/2024` (5,000 files listed at once), End to the last, then F5 on
  `Lisbon trip` to `Backup` on the same share: a server-side copy, done in well under a second.
- `s3` (116 s, trim it): 1,300 tiny thumbnails selected beside a `family-photos` folder in a real Cloudflare R2 bucket,
  F5, the copy dialog's "About $0.01 at Cloudflare R2 list prices" (the manifest's `keyFrame`), Start, and the upload
  running to "Copied 1,300 files." The upload takes about 110 s.
- `phone` (11.5 s): `Pixel 8 - Internal Storage`, into `DCIM/Camera`, all selected, copied to the Mac with a progress
  dialog.
- `archives` (25 s, trim it): a file copied out of `Brand assets.zip` (including the "Browse like a folder" prompt),
  then the `Backups` restic repo, its password typed, the snapshots as folders, into `latest`, and a file restored.
- `transfers` (21 s): with the Operations window over the main window, a copy sent to the background, a move, and a
  compress queued; the compress paused and resumed; the queue empties.
- `undo` (14 s, at 100% zoom, see below): a copy finishes, Undo in its dialog rolls it back, then the operation log
  rolls back an earlier copy of `Raw imports`.
- `hero2` (10 s): local photos on the left, the NAS's `Lisbon trip` on the right, the cursor walking and Tab-ing across.

### What's missing or different from the ask

- **The S3 picker reads `cmdr-s3-test`**, the real test bucket's name: the volume picker names a bucket place by its
  bucket. The demo folder is `cmdr-clips-demo/family-photos`.
- **No ADB moment.** There's no browsable fake ADB device; a real phone or emulator would be needed.
- **The NAS shows as "private on 127.0.0.1:11481"**, not "Home NAS": the volume picker names a share by its address.
  Fixing it needs either a hostname for the fixture (an `/etc/hosts` entry) or the picker to use the server's name.
- **The `phone` copy dialog is skipped**: its From field spells the virtual device's internal `cmdr-e2e-virtual` id, so
  that copy starts through MCP straight into the progress dialog. (A download from S3 has the same problem, with the
  access key id in the path, which is why `s3` uploads.)
- **In `transfers` the Operations window covers part of the main window's dialogs.** It sits over the main window's
  lower right so both are in frame.

### App behavior this phase surfaced (possible bugs, not investigated)

- **An S3 upload into the VersityGW fixture never progressed** (admitted, then nothing for two minutes). The log also
  warned `couldn't rewrite …/backend-state/s3/unfinished-uploads: No such file or directory` on a fresh data dir.
- **A server-side SMB copy reports "Copied 1 file … 1 / 36"** with every file in the tree at "0 bytes" while the byte
  bar reads 100%: the per-file accounting doesn't follow a copy-on-server.
- **The operation log's "You" wraps to "Yo u"** in its narrow column at 125% zoom. That's why `undo` is recorded at
  100%.
- **A queue row's title overlaps its "Running" badge** ("Copying 1 folder" over "Running") at the Operations window's
  default width.
- **An S3 bucket connected without remembering its secret asks to sign in again** on the next session's first use.
- **Settings set right before the app quits may not persist** (`keepProgressOpenWhenDone` and the language both came
  back at their earlier values on the next launch). Possibly the same root cause as #698.
- **Cmdr logs S3 app paths, access key id included, at INFO**: one R2 run put the key id (not the secret) into the clips
  instance's `cmdr.log` 2,641 times (`s3://<key id>@…` in listing and stall-probe lines). Error reports carry logs, so
  this reaches the reports board too.
- **The copy dialog's title reads "Copy 1300 files"** while its tally under it reads "1,300 files": the title skips the
  thousands separator.
- **The copy dialog's "To" volume picker reads "Select…"** with an S3 bucket as the destination, though the path box is
  right.
- **A local AI search may flash "Nothing to filter on yet. Try rephrasing?"** above "Searching…" before its answer lands
  (seen in one recorded frame; fast-changing UI, so worth a look by hand before calling it a bug).
- **Stopping the app with SIGTERM leaves its `llama-server` running**; the next launch stops it
  (`kill_stale_llama_servers`).

### Re-recording

```bash
apps/desktop/test/smb-servers/start.sh minimal && apps/desktop/test/s3-servers/start.sh minimal
pnpm marketing:clips --video2                      # all seven, about 8 minutes
pnpm marketing:clips --video2 --only s3,transfers  # some; the manifest keeps the others
```

## Overnight follow-ups (2026-10-23)

- **`multi-rename` website clip**: the take now undoes its own rename off camera, so the dark and light takes open the
  same sheet (neither shows an "Undo rename" the other lacks). Re-recorded and verified on frames from both.
- **`ai` (video 1) is a real AI search**, on the local model, as described above. The model samples (`--temp 0.6`): one
  take answered with the date range alone and no "tax" in the pattern, so the demo tree (v5) gained a non-tax decoy from
  last year, and a take whose results aren't all tax files now fails and retries. "Last year" is the staging day's: in a
  new year, restage (`--remove-demo`) before re-recording.
- **`s3` (video 2) shows a real cost estimate**, on Cloudflare R2. Why R2: B2, Wasabi, Hetzner, and Spaces price every
  request at $0 and an upload has no egress, so their estimate rounds to zero and hides; R2 reaches a cent at ~1,112
  Class A requests, hence 1,300 tiny files. Why an upload: a download's From field spells the bucket path with the key
  id. The secret never crossed Playwright, MCP, or IPC (the orchestrator wrote it into the instance's file secret store
  before launch); a scan of the instance's data dir, the test results, and the masters found it 0 times, and the frames
  show neither the key id nor the account id. Bucket `cmdr-s3-test`, prefix `cmdr-clips-demo/`: the run deleted the
  upload and a listing of the prefix found 0 objects; the secret was taken back out of the store.
- **`copy-rollback` poster: skipped.** Its `≥24.03 MB` is the destination folder being written mid-copy, where the index
  can only give a lower bound; no wait before the take changes it, and every frame with the progress dialog shows it.
- **NAS name ("Home NAS" in `smb` / `hero2`): skipped.** A Bonjour proxy
  (`dns-sd -P "Home NAS" _smb._tcp local 11481 Home-NAS.local 127.0.0.1`) resolves and reaches the fixture, but the
  label stayed "private on 127.0.0.1:11481": Cmdr swaps an IP for a Bonjour name only from its own mDNS browse, which
  runs under a discovery lease the take doesn't hold, and it appends `:port` off 445 regardless. Port 445 on 127.0.0.1
  needs root (only the wildcard address is open to an unprivileged bind, which would put the share on the LAN). So the
  best reachable label is "private on Home NAS:11481". A clean one needs `/etc/hosts` plus a root-bound 445, or the
  picker using the saved server's name. `smb` was re-recorded during the attempt; same content as before.

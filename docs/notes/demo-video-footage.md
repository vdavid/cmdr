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
- `ai` (11 s): ❗ **not an AI search.** A regular filename search (`invoice`, then `budget`) over the whole studio. A
  cloud provider needs a key entered through Settings > AI, and typing David's OpenAI key into the recording instance
  through the automation layer risked it landing in a log, so I didn't script it. The manifest carries a `remark` saying
  so. To get the real beat: set up a provider in the clips instance by hand (its data dir is
  `~/Library/Application Support/com.veszelovszki.cmdr-clips`), then rewrite the `ai` scene in
  `apps/desktop/test/e2e-playwright/marketing-video-scenes.ts` to type a sentence in AI mode and press Enter.
- `hero` (10 s): a calm two-pane window, the cursor walking down the photos, Tab to the other pane, a few rows, Tab
  back.
- `care` (11 s): the Settings window, the Language picker open, ArrowDown through seven languages (each re-renders the
  window live), English picked back, then Theme mode set to Light and the window turning light.

## What's missing or rough

- **No network share**: the SMB fixtures aren't trivially available on this machine, so `reach` has no NAS. Phones were
  out of scope.
- **`ai` is a placeholder**, see above.
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

# Two Cmdr icons in the Dock after an update (#1023)

Why some people see a second Cmdr icon in the Dock after updating, measured on real releases. Short version: the in-app
updater's **Restart** button starts the new Cmdr as a SECOND instance (`open -n`) while the old one is still running,
and the Dock gives every instance of an app its own tile. The pinned tile belongs to the old instance, which then exits.
Nothing about the bundle, LaunchServices, or the tile Cmdr writes is involved.

![The Dock with two Cmdr icons: the pinned one right after Finder, and a second one with the running dot at the end of
the apps](assets/dock-duplicate-icon.png)

## Symptom

- The pinned Cmdr tile stays where it was; a second Cmdr tile appears at the end of the apps (or in the recent-apps
  section, when "Show suggested and recent apps in Dock" is on).
- The running dot is always under the new tile. Clicking the old pinned tile activates the running app (LaunchServices
  finds it by bundle id; the Dock just doesn't draw it as that tile's process).

## Setup

- M1 MacBook Pro, macOS 27.0.1 (26A434), GUI session logged in, driven over SSH, 2026-11-07.
- Releases tried: 0.50.0, 0.51.0, 0.55.0, 0.59.0, and 0.60.0. The updater always installs the newest release, so every
  update hop landed on 0.60.0.
- Real signed releases from GitHub Releases (`Cmdr_<v>_aarch64.app.tar.gz`), extracted straight into
  `/Applications/Cmdr.app`, user-owned like a drag install. The M1's root-owned 0.55.0 `.pkg` install was moved aside
  (root, through Ansible's become) and put back afterwards; the Dock plist was exported first and imported back at the
  end.
- **Pin**: a tile written exactly like `dock::pin_cmdr` (six keys, `_CFURLStringType` 0, no `book`, prepended to
  `persistent-apps` through `CFPreferences`, then the Dock terminated), from a small Swift tool.
- **Update**: `update-check.json` reset so the update is due, then a plain `open`; the old version's own updater
  downloaded, verified, and synced 0.60.0 into the running bundle. Then the update toast's **Restart** button was
  pressed through the Accessibility API (`AXPress`), so the relaunch ran the old version's real code.
- **Observation**: the Dock's own Accessibility tree (`AXApplicationDockItem` children of the Dock process, with
  `AXIsApplicationRunning` and `AXURL`), which is exactly what the Dock draws, read by a Swift tool; plus
  `defaults read com.apple.dock persistent-apps` / `recent-apps`, `lsappinfo`, and `pgrep`. Screenshots weren't possible
  (lid closed, no display).

## What happened

Each line is the Dock's AX tree for the Cmdr tiles. "Pinned" is the `persistent-apps` tile at index 1 (right after
Finder); "extra" is the other Cmdr tile (the M1 has recents on, so it sits in that section).

1. **Cmdr's own pin works and survives a launch.** After the write and Dock restart, the Dock rewrote the entry into its
   full form: a 568-byte `book`, `_CFURLStringType` 15 (`file:///Applications/Cmdr.app/`), `dock-extra`, `is-beta`, and
   `file-mod-date` / `parent-mod-date` set to 0. `open /Applications/Cmdr.app`: pinned tile RUNNING. This settles the
   "manual check" `src-tauri/src/dock/DETAILS.md` still owed: no bookmark needed, the Dock fills it in.
2. **Real update, Restart button**, pinned tile RUNNING before each one:
   - 0.59.0 → 0.60.0, three runs: pinned `-`, extra RUNNING, every time.
   - 0.55.0 → 0.60.0: pinned `-`, extra RUNNING.
   - 0.51.0 → 0.60.0: pinned RUNNING, no extra tile in use.
   - 0.50.0 → 0.60.0: pinned RUNNING, no extra tile in use.
   - While the synced-but-not-restarted old process was running, the pinned tile stayed RUNNING: the sync itself changes
     nothing the Dock cares about.
3. **No update at all, same effect.** With 0.60.0 running from the pinned tile: `open -n /Applications/Cmdr.app` gave
   two processes and BOTH tiles RUNNING; terminating the old one left pinned `-`, extra RUNNING. Exactly the post-update
   picture, with no file touched.
4. **With "Show suggested and recent apps" off**, which is the screenshot's layout: the same `open -n` plus old-exits
   put the extra RUNNING tile at the end of the apps, right before the separator to Downloads and Trash. After a quit it
   disappeared, and a plain reopen lit the pinned tile again.
5. **A plain quit and reopen fixes it.** Quitting the second-instance Cmdr and launching it normally put the dot back on
   the pinned tile. The stored `persistent-apps` entry never changed in any run (same GUID, same `book`).

## The mechanism

- Tauri's `process::restart` (behind `relaunch()` from `@tauri-apps/plugin-process`, which the update toast calls)
  changed in **tauri 2.12.2**: it now relaunches through LaunchServices with `/usr/bin/open -n <bundle>`, waits for
  `open` to return, then `exit(0)`. Before that (2.12.1 and older) it spawned `Contents/MacOS/Cmdr` directly as a child
  process. Cmdr shipped tauri 2.12.1 up to 0.51.0 and 2.12.2+ from 0.52.0 (`Cargo.lock` at each tag).
- `-n` means "open a new instance even if one is running". For a moment two Cmdr processes are registered with
  LaunchServices, and the Dock gives the second instance its own tile, as it does for any app opened with `open -n`.
  Then the first instance, the one bound to the pinned tile, exits. The Dock doesn't move the survivor onto the pinned
  tile; it keeps it on its own "running, not pinned" tile until it quits.
- The relaunching code is the OLD version's, so the behavior follows the version you update FROM: every Restart-button
  update from 0.52.0 or later does it (four out of four measured), updates from 0.51.0 or earlier don't. That fits "it
  started on one update": the first update a person took from 0.52.0+ (0.53.0 shipped 2026-10-15).
- Why it looks permanent: the duplicate lasts until Cmdr is quit and reopened, and most people restart Cmdr mainly
  through the update toast, every few days, which re-creates it each time. With recents on, the extra tile also stays in
  the recents section after a quit, as any recent app does.
- Not the cause, ruled out by the runs above: the in-place sync (pinned tile kept its dot while the synced old process
  ran), the Local Network refresh's `LSRegisterURL` + touch (0.60.0 does it on its first launch, and run 3 shows the
  same split without any update), Cmdr's book-less tile (the Dock normalizes it, and it behaves like a native one), and
  the staging copy's LaunchServices registration.

## Open question

- David reports that his own 0.59.0 → 0.60.0 update didn't re-create the duplicate after he had re-pinned. Every
  Restart-button run here did. Worth asking how that restart happened: a Quit and a click on the Dock tile (or
  Spotlight) doesn't go through `open -n` and leaves the pinned tile with its dot.

## What would fix it

- **Relaunch only after the old process is gone, without `-n`.** A detached helper that waits for the old pid to exit
  and then runs `open <bundle>` (no `-n`), or an equivalent `NSWorkspace.openApplication` call made from that helper.
  That's the shape Sparkle's relauncher has. The new process then is the only instance, and the Dock binds it to the
  pinned tile. It also removes the two-process overlap the instance lock's five-second `ACQUIRE_WINDOW` exists for
  (`apps/desktop/src-tauri/src/instance_lock.rs`).
- Replacing the whole bundle (Sparkle-style swap) would NOT fix this on its own: the duplicate comes from the relaunch,
  not from how the files land.
- Nothing to repair for people already affected: their stored tile is fine, and the first relaunch that doesn't use
  `open -n` (or any quit and reopen) puts the dot back.

## Reproducing

- `open -n /Applications/Cmdr.app` while Cmdr runs, then quit the first instance. That's the whole bug, no update
  needed.
- Reading what the Dock draws over SSH: walk the Dock process's `AXChildren` → first `AXList` → items, and read
  `AXTitle`, `AXIsApplicationRunning`, and `AXURL` (System Events can't coerce `AXURL`, so use the AX C API from a Swift
  tool). Pressing a button in Cmdr's webview over AX needs `AXEnhancedUserInterface` set on the app element first, or
  WebKit exposes no web content.

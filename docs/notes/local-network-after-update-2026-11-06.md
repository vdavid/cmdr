# Local Network blocked after an in-place update (#1019)

Why a user's Local Network permission stopped applying after each Cmdr update until they switched Cmdr off and on, what
we measured, and why the fix lives on the first launch after an update. The code that does it:
`apps/desktop/src-tauri/src/updater/local_network_refresh.rs` and `apps/desktop/src-tauri/src/after_update.rs`.

## Setup

- M1 MacBook Pro, macOS 27.0.1 (26A434), GUI session logged in, driven over SSH, 2026-11-06.
- Real signed releases: `Cmdr_0.58.0_aarch64.app.tar.gz` and `Cmdr_0.59.0_aarch64.app.tar.gz` from GitHub Releases.
  Binary UUIDs (`dwarfdump --uuid`): 0.58.0 `55D8A271-A914-3F49-988C-9A837B2AE9CA`, 0.59.0
  `2DB67854-9561-3810-A08D-F6C813188EEF`.
- 0.58.0 placed in `/Applications/Cmdr.app` (user-owned, as a drag-install leaves it), the M1's own root-owned 0.55.0
  `.pkg` install moved aside so only one `com.veszelovszki.cmdr` was registered with LaunchServices, then put back.
- The NAS share (`//david@192.168.1.111/naspi`) kernel-mounted, so Cmdr's launch sweep dials `192.168.1.111:445`
  directly, which is a Local Network operation.
- The update made due (`update-check.json` reset), so 0.58.0's own updater downloaded, verified, and synced 0.59.0 into
  the running bundle about 4 s after launch: the real code path, not a simulation.
- Observed with `log show` over `nehelper`, `nesessionmanager`, `neagent`, `UserEventAgent`, and `lsd`, plus
  `/Library/Preferences/com.apple.networkextension.uuidcache.plist` (world-readable, binary; grepping its hex for a UUID
  says whether `nehelper` cached it).
- Limit: Cmdr's Local Network switch on that Mac was OFF (`DenyMulticast = true` in its rule in
  `com.apple.networkextension.plist`, the field the Local Network toggle writes), and nothing could click System
  Settings over SSH (no Accessibility for `osascript`). So the test watched the RULE's contents, not the allowed →
  blocked flip a user sees. That flip still deserves one GUI check by David (below).

## How macOS matches a process to its Local Network rule

- The rule is per bundle id (`SigningIdentifier`), but the kernel policy matches processes by main-executable UUID.
  `nehelper` fills the bundle id → UUIDs list from LaunchServices ("Populating the cache with 1 UUID(s) for
  com.veszelovszki.cmdr"), and `UserEventAgent` logs a matched process as "found bundle id com.veszelovszki.cmdr by UUID
  …". This matches TN3179 § Build-time considerations ("local network privacy uses your main executable UUID").
- `nehelper` rebuilds the list on an "apps installed" notification from the SYSTEM `lsd`, which fires when files under
  the bundle change. A user-side `lsregister -f` (what `LSRegisterURL(url, true)` does) updates the user's
  LaunchServices record but does NOT notify `nehelper`.
- With several copies registered under one bundle id, `nehelper` took the UUIDs of one copy only (the root-owned 0.55.0,
  not the running 0.58.0 in `~/Applications`). TN3179 warns about this too (FB15568200).

## What happened on a real update

Three real updates (13:31, 13:35, 13:37 local time), the same each time:

1. The updater's sync and `touch_bundle` fire the system `lsd`, and `nehelper` repopulates within about 2 s.
2. It reads the user's LaunchServices record BEFORE the user `lsd` re-registers the bundle (log order at 13:35: system
   `lsd` 21.573, `nehelper` starts populating 22.141, user `lsd` re-registers 22.228–22.338, `nehelper` caches
   `55D8A271…` at 22.340). So the rule keeps the OLD binary's UUID.
3. Relaunching the updated app: LaunchServices re-registers it, but no "apps installed" notification reaches `nehelper`,
   so the cache keeps the old UUID. `UserEventAgent` now finds the process only "by PID", never "by UUID". For an app
   whose switch is on, that's a process outside its allow rule: blocked, until the switch is toggled (which rebuilds the
   rule from the now-current record).
4. A shell-driven copy of the same swap (same order, temp file + rename) didn't lose the race (user `lsd` happened to go
   first), so the race is timing-dependent, which fits "after each update" for some users and not others.

## What fixes it

- `lsregister -f /Applications/Cmdr.app` alone, after the update and relaunch: cache unchanged (0.58 still in, 0.59
  not). So option (b) from the issue, `LSRegisterURL` after install, does NOT fix it by itself.
- `touch /Applications/Cmdr.app` after the relaunch: `nehelper` repopulated and cached 0.59.0's UUID within about 1.5 s.
- `lsregister -f` then `touch`, right after relaunching an updated app: 0.59.0 cached and 0.58.0 gone within about 1.5
  s, 2 runs out of 2 (13:36:52, 13:37:41).

So the fix: on the first launch of a new version, re-register (makes the user record current, deterministically), then
touch the bundle (makes `nehelper` re-read it). Doing it at launch rather than right after the sync matters: the
still-running OLD process keeps its LAN access until the person restarts, and the release shipping the fix repairs its
own update.

## Also seen

- Right after the background install, the still-running 0.58.0 process kept matching "by UUID" (the stale cache helps
  it). In the run where the race went the other way, the old process would lose LAN access until restart. Nothing can
  keep both UUIDs in one rule.
- A fresh install's first dial was blocked in the same 1–3 ms as a stuck permission, so "blocked by this Mac" right
  after a FIRST install can be the unanswered prompt. That's why the notice only speaks up after an update.

## Still worth checking by hand

On a Mac with Cmdr's Local Network switch ON: install 0.59.0 (or any build before the fix), let it update to the first
release carrying this fix, relaunch, and open a LAN share. It should connect directly with no toggle. One GUI session is
enough; the rule-level evidence above is what the fix rests on.

## Repro hazard

Never `rm -rf` a scratch dir that has a share mounted under it: an `umount` that answers "Resource busy" leaves the
mount in place, and `rm` walks into the share. This repro did exactly that and deleted 201 files on the NAS, all
recovered from QNAP's network recycle bin. Check `mount | grep <dir>` first, and stop if an unmount fails.

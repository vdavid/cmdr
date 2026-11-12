# DiskArbitration and unmount holders: measured evidence

The evidence the eject and drive-safety work (the unmount approver, the per-disk eject, the holder scan and its facts)
rests on, kept for whoever picks up the deferred DiskArbitration work (per-disk DA sessions, the DA teardown swap) or
re-questions a budget. The designs themselves live beside the code: `apps/desktop/src-tauri/src/volumes/DETAILS.md` §
"The unmount approver" and `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Eject". Open work: GitHub
issues [#246](https://github.com/vdavid/cmdr/issues/246)–[#250](https://github.com/vdavid/cmdr/issues/250).

Unless marked otherwise: verified on macOS 26.6.2 (25G83), unsandboxed uid 501, with throwaway probes against 50–60 MB
APFS DMGs attached `-nobrowse`, 2026-09-12. Apple sources: DiskArbitration-535.0.10, xnu-12377.1.9. Where this section
and § "The approval-hook spike" differ, the spike wins.

## DiskArbitration: statuses, timing, and the unmount flow

- **Busy is `unix_err(EBUSY)`, `0x0000C010`**. The daemon rewrites any `unmount(2)` failure to EBUSY
  (`diskarbitrationd/DARequest.c:1525`). `kDAReturnBusy` (`0xF8DA0002`) covers `/`, the Data volume, or an approval
  dissent.
- **A refusal is slow because of the daemon's holder scan.** After EBUSY, `diskarbitrationd` runs a root
  `proc_listpidspath(PROC_ALL_PIDS, PATH_IS_VOLUME)` before it answers (`DARequest.c:1534→1707`), for `diskutil` (via
  `storagekitd`) and the DA API alike. At load 2.2–2.5: `diskutil eject` of a held APFS image 20.4, 1.8, 0.5, 27.8, and
  0.6 s; DA container Whole 5.1, 1.9, 0.45, 0.42 s; HFS+ `diskutil` 1.15 and 0.50 s. At load about 2 on 2026-09-14
  refusals took 0.13–0.2 s.
- **A container Whole unmount reaches every volume in the container, and partial unmounts are real**: two-volume APFS
  image, file held on volume B, A unmounted, B stayed mounted.
- **Whole links subrequests by BSD unit** (`DAQueue.c:974`), so `DADiskUnmount(physical, Whole)` doesn't reach the APFS
  volumes of its synthesized container.
- **`DADiskCreateFromVolumePath` runs `statfs` then `realpath`** (`DiskArbitration/DADisk.c:413-421`), so it hangs on a
  DMG whose backing file lives on a hung SMB share. It returns NULL for a path that's no longer a mount.
- **DA doesn't track SMB**: `diskutil info /Volumes/testshare` prints "Could not find disk".
- **IOKit ancestry of an APFS volume** (`ioreg -p IOService -t`): `AppleAPFSVolume` → `AppleAPFSContainer` (the
  synthesized whole disk) → `AppleAPFSContainerScheme` → `IOMedia` (the physical store partition) → the physical whole.

The following were verified by reading DiskArbitration-535.0.10 on 2026-09-14:

- **Every approval callback has its own 10 s timer, started when the daemon queues it to the session**
  (`diskarbitrationd/DAQueue.c:627-629`, checked at `:178-180`), not when the client runs it. The daemon queues a second
  callback to a session whose first is unanswered (`DASession.c:312-314`), and the client copies its queue and runs it
  one callback at a time (`DiskArbitration/DASession.c:242,257-277`), so a queued ask either sits in the same copied
  batch or arrives as the next mach message, runnable as the previous ask returns; its timer runs meanwhile.
- **A timed-out answer counts as approval** (`DAQueue.c:115-121`): DA unmounts.
- **Requests on different disks are in the approval stage at once**: serialization is per disk
  (`kDADiskStateCommandActive`, `DARequest.c:1396,1410,1914`), and a Whole request's per-volume subrequests go out in
  one pass (`DAQueue.c:1000,1025`). Writable media first run `__DARequestUnmountTickle` on a background thread
  (`DARequest.c:1400-1404`), so sibling asks can arrive a few hundred ms apart with their own timers (the 190 ms in §
  "Approval coverage").
- **A timed-out session is skipped for unmount, eject, and mount approvals, peeks, and claim releases**
  (`DAQueue.c:548-550,604-609`), which DA treats as approval, until the client copies its callback queue
  (`DAServer.c:2147`). Appeared, disappeared, description-changed, and idle callbacks are still queued
  (`DAQueue.c:470, 488-499,663-715,727-734`), and their delivery is what clears the flag.
- **A disk from a callback carries a frozen description** (`DiskArbitration/DiskArbitration.c:477`,
  `DADisk.c:184, 241-245`; nothing ever refreshes it). A fresh `DADiskCreateFromBSDName` has no cached description, so
  `DADiskCopyDescription` on it is a MIG fetch of the daemon's current one (`DADisk.c:100,253`), and NULL once the disk
  left the daemon's list (`DAServer.c:139-158`).
- **Idle never fires between an approval answer and that unmount's completion** (the request stays listed through the
  approval, `DAStage.c:341-343,439-445`; `CommandActive` covers the unmount, `DARequest.c:1442,1592`,
  `DAStage.c:211,338`), **but it does fire between two separate requests**: once one request completes and the stage
  pass is quiet, idle is queued before the tool's next request exists (`DARequest.c:1587-1594`, `DAStage.c:487-500`). So
  `diskutil eject`'s unmount, eject-container, eject-physical sequence and `unmountDisk`'s per-volume requests have
  idles in between.
- **Idle is queued on the same per-session queue, in order**, at most once per burst of callbacks to that session
  (`DAQueue.c:63-75,727-734`, `DASession.c:310`), and once at registration if the daemon is already idle
  (`DAServer.c:2733-2747`).
- **A disk that disappears while mounted**: the daemon settles its pending answers as approvals, fails its queued
  requests (`DAServer.c:1423`, `DAQueue.c:785-813`), sends disappeared (`DAServer.c:1427`), then issues its own
  `DADiskUnmount(disk, Force)` (`:1432-1434`), which skips approval because the disk is a zombie by then
  (`DARequest.c:1388-1391`). No eject approval and no description change is sent for it (`DAQueue.c:665`). An approval
  already queued before the pull can still reach the client; its answer is dropped.
- **Bindings**: `objc2-disk-arbitration` 0.3.2 (published 2025-10-04; `madsmtm/objc2` not archived, 1,032 stars, pushed
  2026-09-12) binds `DASessionSetDispatchQueue`, `DARegisterDiskUnmountApprovalCallback` (callback returns
  `*const DADissenter`), `DARegisterDiskEjectApprovalCallback`, `DARegisterDiskAppearedCallback`,
  `DARegisterDiskDisappearedCallback`, `DARegisterDiskDescriptionChangedCallback`, `DADissenterCreate`,
  `DADiskCreateFromBSDName`, and `DADiskCopyDescription` (verified by reading the published crate, 2026-09-14). Not
  bound: the private `void DARegisterIdleCallback(DASessionRef, void (*)(void *context), void *context)`
  (`DiskArbitrationPrivate.h:329,331`). `dispatch2` 0.3.1 (2026-02-26) and `objc2-io-kit` 0.3.2 are in `Cargo.lock`
  transitively.

## Holders

- **`proc_listpidspath` is public (`libproc.h:85`) but sees same-uid processes only** (`CHECK_SAME_USER`,
  `xnu/bsd/kern/proc_info.c:2196-2211`). It catches open files, cwd, and per-thread cwd, misses memory-mapped-only
  holders, and `stat`s its path first (`proc_listpidspath.c:83`). With `PATH_IS_VOLUME`: 142 ms to 9.4 s under load; a
  healthy SMB share took 67 ms.
- **A Security query can make the prober the holder.** With a binary running FROM the volume,
  `SecCodeCopyGuestWithAttributes` put the probe in `proc_listpidspath`, `SecCodeCopySigningInformation` took 4.5 s, and
  a Whole unmount in that window named the probe as dissenter.
- **Typed classification signals** (`NSRunningApplication`, `SecCodeCopyGuestWithAttributes` +
  `SecCodeCopySigningInformation`, ppid from `PROC_PIDT_SHORTBSDINFO`):
  - `/usr/libexec/lsd` (uid 0 and uid 501): parent launchd, Apple platform binary (platform identifier 26), no
    `NSRunningApplication`. `mds_stores` (uid 308): same shape.
  - Finder: platform binary AND a regular activation policy.
  - `/bin/zsh` in a terminal: platform binary, no app; the parent chain reaches Warp (regular).
  - `/bin/sleep` and Calculator are platform binaries too, so "platform binary" alone never means "macOS".
  - Responsible-PID attribution works, but responsible apps are often accessory apps, and Google Drive's has no
    `NSRunningApplication` (§ "Holder facts checked in the spike").
  - ❗ **Responsibility is INHERITED, and launchd adopting a process doesn't clear it** (verified on macOS 27.0,
    2026-09-16, by a holder started through a shell that exited: `PPID` 1, still attributed to the terminal app). So the
    responsible-app rule names the app a tool was STARTED from, and the executable-on-the-drive rule answers only for a
    holder no app ever launched.
- **ERR-TT2FH**: the dissenter was `/usr/libexec/lsd`, registering an unseen `.app` after a pane fetched its icon, for
  about 0.3–0.9 s. The retry exists for it.
- **Holds "wait a minute" doesn't fit**: `diskimagesiod` holds a volume while an image stored on it stays attached;
  `backupd` holds it for a whole Time Machine backup.

## The approval-hook spike

Verified on macOS 26.6.2 (25G83), unsandboxed uid 501, load about 2, 2026-09-14, with a throwaway Swift probe (not
committed): DA sessions on serial dispatch queues whose callbacks MATCH only the spike's own volume names, against fresh
APFS and HFS+ DMGs attached `-nobrowse`, with every mutating call gated on `hdiutil info -plist` plus
`diskutil info -plist` identity. Timings come from the probe's clock and `log stream` on `diskarbitrationd`. Apple
sources: DiskArbitration-535.0.10.

### Design consequences

- **An unmount-approval session is the reliable pre-unmount hook for DA-mediated unmounts.** DA asks it before
  `diskutil unmount`, `unmountDisk`, and `eject`, `NSWorkspace unmountAndEjectDevice(at:)`, `hdiutil detach`, and
  `hdiutil detach -force`, once per mounted volume. It never sees a raw `unmount(2)`: `/sbin/umount` by the mount's
  owner needs no sudo and only shows up afterwards (`DidUnmount`, a `DAVolumePath` description change).
- **It holds an unmount for less than 10 s per ask, never longer.** Past that, DA logs the session as not responding and
  unmounts without the answer. An index stop inside the ask needs a budget well under 10 s, which `INDEX_STOP_DEADLINE`
  (15 s) isn't.
- **A timed-out session that registered ONLY approval callbacks is never asked again.** Register another callback kind
  (idle or description-changed) on the same session, so its next delivery clears the timeout, or recreate the session.
- **Asks run one after another on the session's queue.** A Whole request on a two-volume APFS container delivers both
  asks back to back, so a slow first ask spends the second's 10 s too: stop every sibling in the first ask.
- **Force can't be refused.** DA still asks, then ignores the dissent, so the hook does its stop work on every ask and
  never relies on dissenting.
- **An ask, or `WillUnmount`, doesn't mean the unmount happens.** Both fire for a request the kernel then refuses
  (EBUSY), and for every volume of a Whole request before any unmount. Whatever the hook stops must resume after a
  refusal; the trigger is DA's idle callback while the volume is still mounted.
- **`WillUnmount` is posted synchronously from AppKit's own DA approval callback, on the main thread.** A handler that
  spawns a thread and returns races the unmount. Blocking there would hold the unmount, but it freezes Cmdr's main
  thread and meets the same 10 s limit, so a dedicated DA session on its own queue is the better home.
- **Dissent works on non-force requests, and each caller reports it differently.** `diskutil` names the dissenting PID
  and its parent; the DA API answers `0xF8DA0002` with the dissenter's PID; NSWorkspace throws a bare `OSStatus` -47 and
  can leave a multi-partition disk partly unmounted.
- **A vanished disk is distinguishable from an eject**: a disappeared callback for the whole disk with NO eject approval
  before it. A description change alone can't tell. Measured only with the volume already unmounted.
- **`NSWorkspace unmountAndEjectDevice(at:)` on an APFS DMG ejects the synthesized container**: success, both volumes
  unmounted, image still attached. It confirms "never eject the synthesized container", and warns that API-driven ejects
  of APFS media may not power down.
- **Responsible-PID attribution works, but responsible apps are often accessory (menu bar) apps, and Google Drive's has
  no `NSRunningApplication`.** The app and responsible-app holder rules can't require a regular activation policy.
- **Index databases live on the Mac, never on the drive**, so a pulled drive can't corrupt them; the pre-unmount stop
  exists for the FSEvents stream and open handles.

### Approval coverage

- **`diskutil unmount <path>`**: asked, 7–12 ms after the solicitation. On macOS 26, `diskutil` requests go through
  `storagekitd`: DA's log names `storagekitd [47890]` as the requester.
- **`diskutil unmountDisk disk7`** (HFS+, two partitions): asked once per volume, one after the other (s2, then s1 190
  ms later).
- **`diskutil eject <path>`** (APFS DMG): asked. The sequence is `DADiskUnmount(disk6s1)` (not Whole), then
  `DADiskEject(disk6)` (the container), then `DADiskEject(disk5)` (physical, which detached); each eject asked the
  eject-approval callback.
- **`NSWorkspace unmountAndEjectDevice(at:)`**: asked; the requester is the calling process.
  - APFS: `DADiskUnmount(container, Whole)`, both volumes asked, then `DADiskEject(container)`. It answered OK in 122
    ms, and `hdiutil info` still listed the image.
  - HFS+ two partitions: `WillUnmount` for both.
- **`hdiutil detach disk8`** (APFS): asked. `DADiskUnmount(disk9 container, Whole)`, `DADiskUnmount(disk8, Whole)`, then
  `DADiskEject(disk8)`, in 132 ms.
- **`hdiutil detach -force`**: asked (options `0x00080001`, Force and Whole); it ignored a dissent and detached in 279
  ms.
- **`/sbin/umount /Volumes/X`, no sudo**: succeeded in 74 ms with no ask and no `WillUnmount`, only `DidUnmount`, a
  `DAVolumePath` change, and idle. sudo isn't available, and root wasn't needed for a DA mount the user owns.

### Per volume on a whole-disk eject

- **Yes, one ask per mounted volume**: HFS+ partitions (`unmountDisk`, `hdiutil detach`) and APFS container volumes
  (NSWorkspace eject, container Whole). The APFS pair arrived in the same millisecond.
- **Building a two-volume APFS image**: `diskutil apfs addVolume` answered -69493 on the SYNTHESIZED container of a 128
  MB image too, and succeeded on a 1.1 GB sparse image (`hdiutil create -size 1100m -type SPARSE -fs APFS`, then
  `addVolume <container> APFS <name> -nomount`). So the earlier -69493 came from the container's size (APFS scales its
  volume cap with container size), not the node. The exact formula is unverified.
- **A two-GPT-partition image without FAT**: `hdiutil create -layout GPTSPUD -fs HFS+`, attach, then
  `diskutil partitionDisk <disk> GPT JHFS+ A 60M JHFS+ B R`. `partitionDisk` remounts both browsable, so remount them
  with `diskutil mount -mountOptions nobrowse`. Real-image tests can cover the sibling-partition case.

### Blocking answer

- **2 s block**: `diskutil unmount` took 2.256 s (unblocked baseline 0.279 s). **8 s**: 8.368 s.
- **12 s**: DA logged `daprobe [<pid>]:<id> not responding.` 10.66 s after the solicitation (`__kDAResponseTimerLimit`
  10 plus the 1 s grace, `diskarbitrationd/DAQueue.c:45-46,174-199`), unmounted without the answer (`diskutil` 10.87 s),
  and ignored the late approval. Reproduced twice.
- **Recovery**: the timeout flag clears only when the client copies its callback queue (`DAServer.c:2147`), and approval
  dispatch skips a flagged session (`DAQueue.c:609`).
  - A session that also registered appeared, disappeared, description-changed, and idle callbacks was asked again on the
    next request (and timed out again).
  - An approval-only session was NOT asked on the next request (0.212 s, no ask): it stays silent for its lifetime.

### Dissent

`DADissenterCreate(kDAReturnBusy, "spike dissent")` from the approval callback:

- **`diskutil unmount`**: exit 1 in 0.135 s; stderr named the dissent string, the approver's PID, and its parent's PID
  and path.
- **DA API `DADiskUnmount`** (not Whole): 9 ms, status `0xF8DA0002`, and `DADissenterGetProcessID` (through `dlsym`)
  answered the approver's PID.
- **`hdiutil detach`** (Whole, two partitions): both asked, exit 2, "Resource busy", nothing unmounted.
- **NSWorkspace eject** (two HFS+ partitions, dissent on H1 only): threw `NSOSStatusErrorDomain` -47 with an empty
  `userInfo` and no process named, in 82 ms, and left H2 unmounted with H1 still mounted.
- **Force** (`diskutil unmount force`, `hdiutil detach -force`): asked, dissent ignored, unmounted (`DARequest.c:1610`).

### Disappearance without a request

- **Simulated** by SIGKILLing the image's own `diskimages-helper`: a legacy DiskImages image whose `hdid-pid` served
  only that image, with its volume already unmounted. DA logged `removed disk` for all four nodes, and the watcher's
  disappeared callbacks for `disk5`, `disk6`, and `disk6s1` came 12 ms later, with no eject solicitation, eject
  approval, or unmount approval.
- **Every DA-mediated eject or detach** in this spike delivered the whole disk's eject approval 14–18 ms before its
  disappeared callbacks.
- **`hdiutil detach -force` is not a simulation**: it asks (§ "Approval coverage").
- **Raw `umount` is the unmount-without-request shape**: `DidUnmount` with no `WillUnmount` and no ask, and the media
  stays.
- **Unverified by measurement: a MOUNTED volume vanishing** (a real pulled cable). The source reading in §
  "DiskArbitration: statuses, timing, and the unmount flow" says the daemon's own force unmount skips approval and no
  eject approval is sent. Not run: killing the helper under a mounted filesystem is a new kernel-level risk.

### Refused-unmount settle signal

- **A holder process with the H1 volume root open**, so the kernel answers EBUSY:
  - `diskutil unmount`: the idle callback came 53 ms after DA logged the failure, and `diskutil` exited 81 ms after
    that; its stderr named the holder PID.
  - DA API unmount: status `0x0000C010` with the holder as dissenter PID, in 132 ms; idle 1 ms after the requester's
    callback.
  - `unmountDisk` with H1 held: H2 unmounted, H1 refused (partial), idle after each.
- **A refusal brings no description change and no `DidUnmount`**, but `WillUnmount` was posted for it.
- **Idle means "DA's queue is quiet"** and also fires after successes and between separate requests, so pair it with the
  volume's current mount state. It's private (`DiskArbitrationPrivate.h:331`, `DARegisterIdleCallback` through `dlsym`);
  no public callback marks a refusal to an observer.
- **Refusals took 0.13–0.2 s at load about 2**, far below the 20–28 s outliers under load in § "DiskArbitration:
  statuses, timing, and the unmount flow".

### `NSWorkspaceWillUnmountNotification`

- **Posted from AppKit's own DA approval callback, synchronously, on the main thread.** A watcher with NO approval
  callback of its own, sleeping 8 s in its observer: `diskutil unmount` took 8.290 s, and DA logged "unmounted disk,
  ongoing" in the same millisecond the observer returned.
- **Sleeping 12 s**: DA logged the WATCHER process as not responding at 10.66 s (AppKit's is the only DA session in that
  process) and unmounted; `DidUnmount` arrived after the observer returned. AppKit's session recovered: the next unmount
  posted `WillUnmount` again.
- **With an approval session present**, AppKit's `WillUnmount` and the probe's ask arrived 1–6 ms apart: DA asks every
  session at once and waits for all of them.
- **Unverified**: whether AppKit's session filters by disk, so whether a slow observer also delays OTHER disks' unmounts
  (needs two concurrent disks).

### Holder facts checked in the spike

- **`dlsym(RTLD_DEFAULT, "responsibility_get_pid_responsible_for_pid")`** resolves in an unsandboxed, non-root, unsigned
  Swift binary, at 2–8 µs per call:
  - `com.apple.WebKit.WebContent` (parent launchd) → BetterDisplay (accessory), Google Drive (`NSRunningApplication` nil
    for the responsible PID), CleanShot X (accessory), and Cmdr.app (regular).
  - `com.apple.WebKit.GPU` → iStat Menus Helper (accessory); a Chrome renderer → Google Chrome (regular).
  - Standalone agents (iStat Menus Helper, `ViewBridgeAuxiliary`, `DiskUnmountWatcher`) → themselves.
- **`DADissenterGetProcessID` through `dlsym`** resolves in a binary linking DA.
- **`lstat` devices**: on the APFS image, `st_dev`, the mount point's `f_fsid.val[0]`, and its `getfsstat` entry all
  read 16777241. On the boot volume, `/`'s `st_dev` (16777234) differs from its `f_fsid.val[0]` (16777235, the sealed
  system snapshot), so compare against the mount point's own `stat().st_dev`, not `f_fsid`.
- **Security calls off the volume** (`SecCodeCopyGuestWithAttributes`, then `SecCodeCopySigningInformation`): Finder 4
  plus 3 ms; both `lsd` processes and a WebContent helper 1 ms or less each (platform identifier 26); iStat Menus Helper
  2 plus 43 ms (no platform identifier).
- **`hdiutil info -plist`** carries the typed per-image keys `hdid-pid` and `diskimages2` besides `image-path`.
- **BSD units repeat at once**: a detached image's `disk8` and `disk9` went to the next attach, and image A came back as
  `disk5` and `disk6` both times it was re-attached.
- **The nested-image signal** (run during M14, macOS 27.0, 2026-09-16): an HFS+ image attached from a file on another
  attached image's volume reads `hdid-pid` 12984 in `hdiutil info -plist`, and `lsof` shows that same pid holding the
  outer volume's `inner.dmg` — uid 501, so a same-uid walk sees it. The tool answers in 18 ms. IOKit carries the same
  image under `IOHDIXController` → `IOHDIXHDDriveOutKernel`, whose `image-path` property names the backing file but
  carries no pid; its user client's `IOUserClientCreator` does, as a sentence, which is why the plist won.
- **The DiskImages2 server is a different process, under a different uid** (macOS 27.0.1, 2026-10-30; first seen on a
  real USB stick, #246). `open -g` of a `.dmg` on another image's volume, and `diskutil image attach` of one, each start
  a `/usr/libexec/diskimagesiod` of its own: uid 271, parent launchd, and one process per image (pids 53397 and 55913
  side by side). `hdiutil attach` keeps its uid 501 `diskimages-helper`. `hdiutil info -plist` reads `diskimages2` true
  and `hdid-pid` the daemon's pid, and `diskutil unmount` of the outer volume says
  `dissented by PID 53397 (/usr/libexec/diskimagesiod)`, parent `/sbin/launchd`. A same-uid walk can't see it, so only
  the image list names it.
- **Not run** (new risk): the self-holder reproduction.

## What the landed approver confirmed

Verified on macOS 27.0 (2026-09-16) by `volumes::platform::imp::unmount_approver::real_image`: a real approval session
on its own serial queue, against synthetic HFS+ images. The spike's design consequences hold on this macOS.

- DA asks Cmdr's session before `diskutil unmount` and `diskutil unmountDisk`, and waits for the answer: every stop in
  these runs ran while its volume was still in the mount table.
- A whole-disk request's per-volume asks really do arrive back to back, so the first ask's group stop is what protects
  the sibling: after the first ask, both partitions had been let go of, and the next request met no live index.
- A dissent refuses a non-force `diskutil unmount`; `hdiutil detach -force` asks, ignores it, and detaches anyway.
- The idle callback (`DARegisterIdleCallback` through `dlsym`) resolves on this macOS and fires after a refused unmount,
  which is what hands the index back. After an unmount that succeeded, the volume is unlisted and its resume starts
  nothing.
- ❗ A DA lookup from INSIDE an approval callback (`DADiskCreateFromBSDName` plus `DADiskCopyDescription`, how an ask
  computes its group) doesn't deadlock against the daemon waiting for that same ask's answer.
- Timing: 4–24 s per test, dominated by building a two-partition image (`partitionDisk` plus two remounts), not by DA.

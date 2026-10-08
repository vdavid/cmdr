# Volumes details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision detail lives here.

## Location categories

`LocationCategory` variants:

- **Favorite**: user-editable, from the `favorites/` store.
- **MainVolume**: root volume at `/`.
- **AttachedVolume**: a mount with a switcher row that no cloud provider serves (see "Which mounts get a row").
- **CloudDrive**: iCloud at `~/Library/Mobile Documents/…`, providers at `~/Library/CloudStorage/`, and a mount a cloud
  provider's own filesystem serves (`is_cloud_mount`).
- **Network**: SFTP and WebDAV places, from the servers arm below. (The synthetic `Servers` hub row carries it too, but
  that row is minted in `commands/volumes.rs`, not by this module.)
- **MobileDevice**: MTP and ADB storages, appended by `device_volumes.rs`.

Provider identity lives in `file_system/cloud_provider.rs`, not here: `CloudProvider::from_cloud_storage_dir` maps
`~/Library/CloudStorage/` dir prefixes to a typed provider (Dropbox, GoogleDrive→Google Drive, OneDrive/Business, Box,
pCloud, else `Other` carrying the first `-`-segment), and `locate` resolves a path to its drive plus that drive's root.
`parse_cloud_provider_name` and `match_cloud_drive_root` here are thin adapters over it, so the switcher and the file
context menu can't drift on who owns a path. The file context menu needs the same answer plus what each provider can
DO (`supports_eviction`), which is why the enum sits in `file_system/` rather than in `crate::volumes`.

## Which mounts get a row

**Decision**: `is_user_facing_mount` admits a mount when the OS doesn't mark it `MNT_DONTBROWSE`, or when
`provider_for_mount` recognizes who serves it (path pattern or fs type: `pcloudfs`, `macfuse` / `osxfuse`,
`/Volumes/pCloudDrive`, `/Volumes/veracrypt*`, `~/.CMVolumes`), wherever it's mounted. It still drops the boot volume (which has its own row), a dot-prefixed mount, anything
under `~/Library/CloudStorage` (the cloud arm publishes those, and a second row would be a duplicate), and another
account's own mount, recognized provider or not (§ "Another account's mounts").

**Why the flag**: `MNT_DONTBROWSE` is the same bit Finder reads to decide what belongs in its sidebar, so it separates
drives from plumbing without this module naming a single system path. Measured on a stock macOS 27 machine: `/dev`,
`/System/Volumes/{VM,Preboot,Update,xarts,iSCPreboot,Hardware,Data}`, the autofs `home` trigger, and `/Volumes/Recovery`
all carry it; `/` and a mounted SMB share don't. The `/Volumes/` prefix test it replaces got this wrong in both
directions: it let `/Volumes/Recovery` through (only a name check saved it) and hid every drive mounted anywhere else.

**Why the provider clause**: a cloud client or FUSE layer may mark its mount unbrowsable and add its own Finder sidebar
shortcut instead, and the flag alone would hide that drive. It's an allowlist of mounts we positively recognize, so it
fails closed: a new system mount stays out without anyone naming it. Nobody has confirmed a real client that does this
(pCloud 4.3.1 was installed on the test Mac with its mount never approved, so there was nothing to inspect, 2026-09-21);
the clause is a precaution, and costs nothing if no provider ever needs it.

**Why not "anything inside `$HOME`"**: that was the rule, on the premise that no system mount lives there. Xcode breaks
it: CoreDevice's `DeviceFS` is an FSKit mount at `~/Library/Developer/CoreDevice/DeviceFS`, fs type `devicefs`, flagged
`nobrowse`, holding only a `.VolumeIcon.icns` and reporting a synthetic 1.10 TB with no BSD disk behind it, so it
showed as a phantom "Devices" drive (verified on macOS 27.0 26A428 with Xcode 27.0, via `mount`, `df`, and
`diskutil info`, 2026-09-21). Unknown: whether it changes shape once an iPhone is attached; it's excluded either way.

**Two questions, one detection**: admission asks `provider.is_some()`; the CLOUD-or-VOLUMES group and the index
affordances ask `is_cloud_storage()` (`is_cloud_mount`). A VeraCrypt or plain macFUSE mount is recognized but isn't
cloud storage, so it gets a row in VOLUMES. Gating admission on `is_cloud_storage()` would drop it; a test pins the
split. The one family where this arm could overlap the cloud arm is iCloud Drive's folder: a mount exactly there shares
the cloud arm's path, and `list_locations` dedupes on path.

**What this does NOT decide**: whether a path can be OPENED. The volume registry sweeps every mount this account can
reach regardless (`file_system/volume/DETAILS.md` § "Registration covers the whole mount table"); a mount dropped here
is still navigable, it just has no row of its own. The one mount the registry skips too is another account's own, next.

## Another account's mounts

**Decision**: a mount is another account's own, and hidden, when its mount-table row says someone else mounted it
(`f_owner` is neither this process's uid nor 0), it isn't a disk (`f_mntfromname` doesn't start with `/dev/`), and it
isn't a network filesystem (`is_network_fs_type`). `MountEntry::is_private_to_another_user` in `mounts.rs` is the one
rule; `MountedBy` is the owner as a type.

**Why**: on a Mac with two accounts that both run pCloud, the kernel table lists both `pCloud Drive` mounts, one per
home folder, and Cmdr gave each a row. The other account's drive can't be opened from this one: its icon lookups
answered `No such file or directory`, the row showed as unavailable, and picking it dropped the pane in the home
folder (reported against 0.48.0). Nothing in discovery looked at who owned a mount.

**What each consumer of the mount table does with such a mount**:

- **The switcher** (`get_attached_volumes`, through `is_user_facing_mount`): no row, so also no local enrichment (NSURL,
  icon, DiskArbitration) against it, and no space poll, since the poller only watches what a pane shows.
- **The registry sweep** (`registrable_mount_roots`): not registered.
- **The mount watcher** (`handle_volume_mounted`, asking `is_private_to_another_user`): no registration, no
  `volume-mounted`, no refresh. An unmount can't be asked about (the row is gone), and finds nothing to remove.
- **The index** (`mount_roots`): STILL listed. The boot scan cuts at every mount inside the boot tree, and it has to
  stop at one it can't enter as well. ❌ Never point the index at `registrable_mount_roots`.
- **Table facts** (`is_mount_point`, `mount_identity_at`, `has_mount_identity`, `mount_sources`, `smb_mounts`):
  unfiltered. They answer what the kernel lists, and the rule excludes disks and network mounts anyway.
- **Adoption** (`mount_registration::adopt_mount_serving`): untouched, so a path inside such a mount that `statfs` does
  answer for still gets its volume on first listing, and the pane shows the real refusal.

**Why the disk exception**: a drive plugged in, or an image attached, while someone else was logged in is "mounted by"
them, and every account can still use it. `/Volumes/Amp`, mounted from `/dev/disk5s1`, carries the mounting user's uid
with a `drwxr-xr-x` root (verified on macOS 27.0 26A428, a C probe printing `f_owner` per `getmntinfo` row, 2026-09-30).

**Why root's mounts stay**: `f_owner` 0 is every system mount, and a FUSE daemon running as root mounts for everyone.

**Why the row and no access probe**: a probe is a syscall per mount that can hang on a dead share (§ "Hung mounts"),
and the drive this rule exists for answered an ambiguous "not there" instead of a refusal. `f_owner` rides in the same
`getfsstat(MNT_NOWAIT)` snapshot as everything else, at no syscall. It's what `mount` prints as "mounted by <name>",
for exactly the rows whose owner isn't 0 (verified with the same probe beside `mount`'s output, 2026-09-30).

**Why network mounts are left alone**: whether another account's SMB, NFS, WebDAV, or AFP mount is usable from this one
is unverified, and hiding a share someone can reach is worse than showing one they can't. The test Mac has one account,
so nothing could be tried from a second. What's known: an SMB mount's root is `drwx------` and owned by the mounting
user (`/Volumes/naspi`, macOS 27.0, `stat`, 2026-09-30), which suggests another account can't enter it. To widen the
rule, mount a share as one account, fast-user-switch, and list it from the other; if it refuses, drop the
`is_network_fs_type` clause and its test. ❗ `is_network_fs_type` also picks the volume-ID derivation and which mounts
skip blocking enrichment: this rule only reads it.

Linux has the same rule for FUSE, read off the mount options: `volumes_linux/DETAILS.md` § "Another account's FUSE
mounts".

## Location IDs (two cross-file sync points)

`DEFAULT_VOLUME_ID = "root"`; `ICLOUD_VOLUME_ID = "cloud-icloud"` is the only hardcoded cloud-drive ID (the others
derive from the `~/Library/CloudStorage/<provider>` dir name). Both the ID and the provider mapping are mirrored in
`friendly_error.rs`, which `crate::volumes` can't reach, being macOS-only: it matches the `ICLOUD_VOLUME_ID` literal
under a sync-point comment, and `CloudProvider`'s provider list must stay in sync with
`friendly_error::enrich_with_provider`'s separate one. A test pins `CloudProvider::ICloudDrive.volume_id()` against
`ICLOUD_VOLUME_ID` so at least that half can't drift silently.

Every other ID is minted by `cmdr_fs::volume::ids` (below).

## `list_locations()`

Aggregates all `LocationCategory` entries in order (favorites first) and deduplicates through
`cmdr_fs::volume::published_locations::dedupe_locations`, shared with `volumes_linux/`: the first row per ID, and the
first volume row per path.

**Decision: a favorite never claims its path.** A favorite is a `fav-<uuid>` shortcut into a volume, not a volume, so
it dedupes on ID only. **Why**: favorites are gathered first, so a favorite at `/` used to take the path slot and drop
the "Macintosh HD" row. The pane's chip then read "Volume", and every favorite on the boot disk resolved to `root`, a
volume missing from the list, so none of them opened (#349). The same held for a favorite at a drive's mount root or a
cloud drive's folder. The consequence: a favorite and a volume can share a path, so a frontend lookup by path must skip
favorites (`pane-volume.ts::volumeMountedAt`).

Inside this listing, `LocationCategory::Network` comes only from the servers arm below: an OS-mounted SMB share is an
`AttachedVolume` under `/Volumes/`, and the OS-level `/Network` browseable location has no sidebar entry.

### The servers arm

`server_volumes::append_server_volumes` folds into `volume_listing::complete` between the device providers and
enrichment: one row per SFTP or WebDAV place the app knows, registered or merely saved (state `Saved`, the greyed row
with the hollow dot). `category: Network`, `fs_type: "sftp"` / `"webdav"`, `is_ejectable: false` (a server has nothing
to unplug; its control says Disconnect), `supports_trash: false`, `pinned: Some(_)`, and `landing_path`: the saved start
folder as an app path, or `None` when the place lands at its root (no start folder, or one the root no longer holds). A
pane picking a `saved` place goes straight to that landing: `apps/desktop/src/lib/file-explorer/navigation/DETAILS.md` §
`path-navigation.ts`.

❗ **A `saved` row and the volume it becomes share ONE id**, minted by `cmdr_fs::volume::sftp_volume_id` /
`webdav_volume_id`, the same ids the registry keys on. That identity is what makes a tab restorable: a tab stores
`(volumeId, path)`, the switcher shows the same id greyed, and activating either dials the same saved entry.

❗ **The pin filters the SWITCHER, not this list.** A volume id with no row is one the app denies exists, and Enter on
a hub row, a restored tab, a favorite inside a server, and the pane's own lookup all reach for a row by id. So every
place gets a row carrying its own `pinned`, and `pinned` is `None` on every row the cap was never about (a local disk,
a favorite, a mounted SMB share). Where the cap is applied and what the switcher does with it:
`apps/desktop/src/lib/file-explorer/navigation/DETAILS.md` § "The three-things rule, and which side enforces it".

❗ **A registered volume nothing has saved still gets a row.** `forget_server` drops the saved entry without dropping
the session, and a live volume with no row is one a pane can sit on while the switcher denies it exists; the row then
takes its name and root from the volume itself.

❗ **Cached state only, ❌ never the wire**: the listing runs on every `volumes-changed`, so a probe here would turn a
refresh into a round of network traffic. The device-provider seam is deliberately NOT reused;
`crates/cmdr-sftp/DETAILS.md` says why: it lists things that appear and leave on their own.

### Enrichment has two twins

`smb.rs::enrich_from_volume_registry` is the only place a `LocationInfo` learns anything from the `VolumeManager`, and
`volumes_linux/smb.rs` carries the twin. Both copy `capabilities()` and `connection_state()`; only the macOS one adds
the `OsMount` fallback, because that fallback is a `mount_smbfs` the Linux build never performs. ❗ A field filled on one
twin and not the other is a pane whose buttons differ by OS, which is exactly what the Docker E2E lane would have hit
with a registered SFTP volume greying out under Linux while it stayed live on a Mac. Each twin has a cell for that.

### One volume ID publishes one mount root

**Decision**: `get_attached_volumes` collapses mounts that share a volume ID
(`cmdr_fs::volume::canonical_root::collapse_by_volume_id`), keeping the SHORTEST path and breaking ties
lexicographically. `list_locations` then dedupes on ID as well as path (favorites on ID only; see above).

The collapse lives in `cmdr-fs` rather than here because it's a pure list transform over `(volume id, mount root)`
pairs, and Linux needs the identical rule (`volumes_linux/DETAILS.md`): bind mounts and container mounts make "one ID,
several roots" routine there. Each platform implements `MountRootCandidate` on its own `LocationInfo` and keeps its own
platform-specific ID derivation. A second copy would rot independently, which is the whole reason the two enumerations
share this one.

**Why**: a volume ID is identity, and one filesystem can legitimately be mounted twice: macOS mounts the same SMB share
at `/Volumes/naspi` and `/Volumes/naspi-1`, and both derive the same ID (a share keys on `(server, port, share)`; a
local disk on its filesystem UUID). Deduplicating on path alone let both survive under one ID, and everything
downstream keys on the ID:

- `file_system::register_discovered_volumes` registered both, so the registry ended up rooted at whichever mount
  registered last. A pane restoring a saved `/Volumes/naspi/…` path then hit a backend rooted at `/Volumes/naspi-1` and
  the listing failed.
- The frontend builds keyed lists from the volume list, and duplicate keys throw during render (Svelte
  `each_key_duplicate`), which took the transfer dialog down with it.

The shortest path wins because macOS suffixes the LATER mount, so the shortest is the original: the root every saved
path, favorite, and index row already refers to. The choice is pure and order-independent, so discovery order can't
decide identity. Registration keeps the incumbent on a conflict too (`file_system/volume/DETAILS.md` § "Key
decisions"), so no single source has to get this right alone.

**Publishing one location is not the same as forgetting the others.** The registry keeps every mount root that carries
an ID and promotes a survivor when the active one dies, so the shortest-path rule here is the tie-break among live
roots rather than a permanent binding: `file_system/volume/DETAILS.md` § "A volume ID owns a set of mount roots". This
collapse stays purely about what the switcher SHOWS, and it still runs without touching any mount.

## Hung mounts

**The problem.** A network mount (SMB, NFS, …) can wedge so that every metadata syscall on it blocks in the kernel for
30s–forever (uninterruptible: even `SIGKILL` won't land until the mount is force-unmounted). Volume discovery is riddled
with such syscalls, and a single dead mount used to take the whole app down at launch: `init_volume_manager` ran
`get_attached_volumes` synchronously on the main thread (inside the Tauri `setup` closure), and NSFileManager's
`mountedVolumeURLsIncludingResourceValuesForKeys` `getattrlist`s every mount to build the URL array. On a wedged
`/Volumes/naspi` the main thread stuck in `__getattrlist` for 90s+ and the webview never recovered (its startup IPC piled
up behind the frozen process). The MCP `cmdr://state` resource hit the same wall through `list_locations`: reads took a
flat ~30s (one smbfs kernel timeout). (Incident: live NAS QA, 2026-07-13.)

**The fix, in three layers.**

1. **Non-blocking enumeration.** `get_attached_volumes` enumerates via `getfsstat(MNT_NOWAIT)` (`enumerate_mounts`), not
   NSFileManager. `MNT_NOWAIT` returns the kernel's cached mount table (mount point, fs type, `MNT_RDONLY` flag, and the
   `f_mntfromname` SMB source) without ever round-tripping to a filesystem, so a wedged mount can't stall it: this is
   the difference between `df -n` and plain `df`. `getfsstat` was verified non-blocking on the exact wedged NAS state
   from the incident. Because fs type and read-only come straight from the snapshot, three former per-volume `statfs`
   calls (`get_fs_type`, `read_only_from_statfs`, `get_smb_mount_info`) are gone from this path. The provider check
   (`is_cloud_provider_mount`) takes the snapshot's fs type for the same reason: it once re-read it with a `statfs` on
   every mount no path pattern matched, SMB and NFS shares included. That hang was reasoned from the code, not
   reproduced against a wedged mount.
2. **Skip blocking enrichment for network mounts.** `build_attached_location` runs the blocking NSURL / NSWorkspace /
   DiskArbitration enrichment (`resolve_local`) ONLY for local mounts. Network mounts (`is_network_fs_type`) derive
   everything from the getfsstat snapshot: id/name from `f_mntfromname` (SMB → "share on server"), `is_ejectable = false`
   (cosmetically moot: the eject affordance keys on `connectionState` and `volume/eject/` forces it true for SMB), no icon,
   never a disk image. So a dead network mount contributes its entry and never blocks discovery of the healthy volumes
   beside it.
3. **Off-main + timeout-guarded callers.** `init_volume_manager` registers root synchronously (cheap, `/` never hangs)
   and spawns attached/cloud discovery on the `volume-init` helper thread, then re-emits `volumes-changed`. Every caller
   of `list_locations` is wrapped in a ~2s `spawn_blocking` timeout: `volume_listing::discover_local` is the only door
   for the `volumes-changed` push and the `list_volumes` IPC, and it has no unbounded path; the MCP
   `snapshot_volumes` guards its own. So the remaining unguarded blocking
   paths inside `list_locations` (`get_favorites` and `get_cloud_drives`, which still `statfs`/icon per item and would
   hang on a favorite or cloud folder that lives on a wedged mount) degrade to a bounded 2s partial result instead of an
   infinite stall. `get_main_volume` builds root directly from `/`, never enumerating.
4. **A timed-out listing publishes the LAST GOOD one.** `volume_broadcast` keeps the most recent successful
   `list_locations` and re-emits it (still flagged `timed_out`) when a later one misses the deadline. Publishing the
   empty list beside that flag told the frontend "you have no volumes", and since the picker's refresh button re-ran the
   same listing into the same timeout, nothing the user could do brought them back. Rationale and the staleness bound:
   `apps/desktop/src-tauri/src/volume_broadcast/round.rs` § `LocalSnapshot`. A discovery that STARTED before the one already applied can't roll
   the snapshot back, since overlapping rounds are the norm while a mount hangs.
5. **Server rows never wait on local discovery.** Every `volumes-changed` round races discovery against
   `PROVISIONAL_AFTER` (100 ms). A healthy listing wins and the round emits once. A slow one makes the round emit the
   cached local part beside fresh server, device, and registry rows with `discovery_pending: true`, then emit again
   when discovery lands or times out. Before this, one hung mount (a Tailscale SMB share) held the whole list, server
   rows included, for the full 2 s, so a server re-added with a new folder kept its old folder in the switcher (same
   id: host + port + user) and opening it said "Path not found". A mutex orders the events, so one composed from an
   older snapshot never lands after a newer one. Consumers treat `discovery_pending` like `timed_out` for
   retire-by-absence, and the volume store settles a retry only on a non-pending event. `apps/desktop/src-tauri/src/volume_broadcast/round.rs` §
   `Broadcaster::round`.

A pane LISTING a folder on a hung mount is the other half: it says the volume isn't answering after 8 s, keeps waiting
and retrying, and caps the threads the mount can pin. `apps/desktop/src-tauri/src/file_system/listing/DETAILS.md` § "Stalled listings".

Note that the 2s deadline fires for reasons other than a hung mount: `list_locations` runs on the shared blocking pool,
so a subsystem that saturates the pool starves it just as effectively (`commands/CLAUDE.md` § `BlockingBudget`).

**Follow-up.** `get_favorites` and `get_cloud_drives` still do unguarded per-item `statfs`/icon; a favorite pointing at a
hung mount makes `list_locations` time out (2s), so that listing carries no fresh volumes at all and the broadcast falls
back to the last good set. Fully fixing "one dead mount never hides the others" here needs per-item timeouts for those
two, tracked separately.

## Global state in `watcher.rs`

- `APP_HANDLE: OnceLock<AppHandle>`: app handle for emitting events.
- `OBSERVER_INSTALLED: OnceLock<()>`: idempotency gate.

The observer `RcBlock` closures aren't kept in our own static; `addObserverForName:object:queue:usingBlock:` retains the
block for the lifetime of the registration, and we never remove the observer. Same pattern as
`file_system/open_with.rs`. `DualPaneExplorer` uses the `volume-unmounted` event (carrying the volume path) to redirect
panes off ejected volumes.

## Volume space

`get_volume_space(path)` uses `NSURLVolumeTotalCapacityKey` and `NSURLVolumeAvailableCapacityForImportantUsageKey`
(falls back to `NSURLVolumeAvailableCapacityKey`). Returns `None` for non-existent paths.

## Live volume space

`live_space.rs::live_volume_space(path)` is what the space poller reads every 2 s (through
`Volume::get_live_space_info` and the poller's unregistered-path fallback). It returns the same important-usage figure
as `get_volume_space`, mostly at `statfs` cost:

- **Why**: on the boot volume the important-usage query costs ~6.5 ms of CPU (5.9 of it system time, plus work in the
  `deleted` daemon), against ~0.01 ms for `statfs`. The poller asks for the boot volume permanently (the low-space
  check), so that alone was ~0.33% of a core. On external and SMB volumes the query is ~0.02 ms; the cost is
  CacheDelete's purgeable accounting on the boot container.
- **How**: important-usage free = `statfs` free + purgeable, and ordinary writes and deletes move `statfs` and the
  important-usage figure by the same bytes while purgeable holds still. So each filesystem (keyed by mount point) keeps
  an anchor: the last important-usage reading and the `statfs` taken beside it. A live reading is the anchor plus how far
  `statfs` moved since, clamped to the drive.
- **When it asks again** (`plan`): no anchor yet; the anchor is 60 s old; `statfs` drifted by 1/1,000 of the drive (one
  readout step: past that it may be macOS purging, which raises `statfs` free while the important-usage figure holds);
  the total changed; or a write operation settled since (`expect_space_change`, from `TauriEventSink::emit_settled`),
  because deleting a file a Time Machine local snapshot still holds frees purgeable space `statfs` never sees.
- **Evidence** (verified on macOS 27.0, a Swift probe sampling all three figures every 2 s for 5 min, 2026-09-27): `statfs`
  free equals `NSURLVolumeAvailableCapacityKey` to the byte, purgeable held at 10.692 GB through 3.3 GB of writes,
  and the anchor-plus-delta figure stayed within 0.4 MB of the real one (one sample 10 MB off: a write inside the call). Numbers and before/after CPU:
  `docs/notes/performance/space-poll-cost-2026-09-27.md`.
- ❌ **Only for readouts that poll.** Copy validation and the transfer pre-flight act on the figure once, so they keep
  calling `get_volume_space`, which always asks.

## The unmount approver

`unmount_approver/` answers DiskArbitration's unmount approval, so a drive is let go BEFORE any DA-mediated unmount,
whoever started it: Finder, `diskutil`, `hdiutil`, `NSWorkspace`, another app, or Cmdr's own eject. It takes over from
the `NSWorkspaceWillUnmountNotification` handler, which stays only as the fallback for a Mac where the approver can't
install: that handler is racy by construction (it spawns a thread and returns, so the unmount runs alongside the stop)
and leaves a refused unmount's index stopped for good.

**The session.** One `DASession` on its own serial queue at user-initiated QoS (a lower one lets Cmdr's own indexing
load stall an ask), carrying unmount-approval, appeared, disappeared, description-changed (watching
`kDADiskDescriptionVolumePathKey`), and idle callbacks. It gets its dispatch queue LAST, after every registration, so no
callback can run against a half-registered session. Registering the non-approval kinds is also what keeps it
recoverable: DA stops asking a session that timed out until that session copies its callback queue again, and it's the
other callback kinds' delivery that clears the flag (`DAQueue.c:548-550`, `DAServer.c:2147`, read from
DiskArbitration-535.0.10, 2026-09-14).

**What an ask does**, on that queue, in `callbacks.rs`:

1. Reads the disk's description: BSD node, volume UUID, whole-disk BSD unit, and volume path. No path, no registered
   volume whose ACTIVE root is that path, or a disk this session doesn't act on → approve at once.
2. Moves that whole disk's ask generation, which voids every resume candidate offered before it.
3. Computes the group: every registered volume mounted on the same BSD unit (`disk_units.rs`). **Why the unit**: a
   whole-disk request's per-volume asks are linked by BSD unit and arrive back to back, so the first ask has to let go
   of the group or the second spends its own window waiting. A physical disk's other container is a different request.
   `disk_units.rs` is shared with Cmdr's own eject, which passes a whole PHYSICAL disk's units (its own plus every
   synthesized container on it) rather than one, and reads `bsd_name_at` to name the node under a mount. ❗ **An
   `AskGroup` has TWO answers, ❌ never one list**, the same rule `DiskMounts` and `HolderScan` carry: a mount table
   that wouldn't answer sets `unit_unreadable`, and step 7 then dissents whatever the volumes it COULD see said. Reading
   it as an empty unit would let the ask stop only the volume that was clicked and approve an unmount that takes a
   sibling down under its live FSEvents watcher.
4. Marks the group unmount-pending at the drive-release gate, so no start lands on it meanwhile.
5. `drive_release::release(group, deadline)`: the one wait a DA queue may make.
6. Records every volume it stopped while indexing, and carries an earlier ask's record forward to this epoch and
   generation. A volume Cmdr's own eject is taking down belongs to that flight, so the approver records none of those.
7. Answers: dissent (`kDAReturnBusy`) if anything is still letting go, a write op is busy on the disk, or the unit read
   came back unreadable; otherwise approve.

**The shared deadline** (`ask.rs`). DA times each callback from when it QUEUED it (10 s, `DAQueue.c:178-180`), not from
when the client runs it, and a session runs its callbacks one at a time. So an ask starting while an earlier ask's
window is still open may have been queued inside it, and joins that chain: its deadline is `chain_start +
APPROVAL_STOP_BUDGET` (7 s), leaving 3 s for the client's wake-up, the asks queued behind the stop, and scheduling under
load, while still covering an index shutdown's 5 s live-loop drain. **Time-based, ❌ never gap-based**: a queue that
stalls between two asks would otherwise start a new chain and hand a queued ask a budget its DA timer has already spent.
An ask with no time left answers without waiting, approving only a disk with no index, no ticket in flight, and no busy
write op, and leaving its stop to a detached thread.

**Handing the drive back** (`records.rs`). An ask, or a whole-disk request, doesn't mean the unmount happens: the kernel
can still answer EBUSY. The signal that a refusal settled is DA's private idle callback (`DARegisterIdleCallback`,
through `dlsym`); no public callback marks a refusal to an observer. At idle, every unmount-pending flag clears and
every record goes to `drive_release::resume`, which settles, re-checks, and starts what's still owed. The owner stands
behind a record until a newer ask on its disk, and presence comes from the mount table, ❌ never from the callback
disk's description, which is a frozen copy nothing refreshes (`DiskArbitration/DADisk.c:184`). A record is consumed by
its resume, so a later idle can't start the same volume twice. `Appeared` and `Disappeared` drop a disk's records,
because DA hands a freed BSD unit to the next disk at once.

**Why a mount ended** (`causes.rs`). An unmount Cmdr was asked about has already let go of the drive. One nobody asked
about hasn't, and leaves an index holding a filesystem that isn't there. The machine is pure and fed by the same
callbacks, in DiskArbitration's delivery order, ❌ never by timing:

- a volume path clearing is `Asked` when an ask marked that BSD node, otherwise `Unasked` (a raw `/sbin/umount`, or an
  unmount DA made while this session was skipped);
- a whole disk disappearing is `Ejected` when an eject approval came, otherwise `Pulled` — or `Unknown` when an ask on
  that disk had overrun `DA_RESPONSE_WINDOW`, because DA skips a timed-out session for approvals and the eject approval
  that would have said "ejected" may never have arrived. `Unknown` acts like `Pulled`: stopping an index twice is cheap,
  leaving one on a gone drive isn't.

`Unasked`, `Pulled`, and `Unknown` release through the gate with owner `Vanish` under `VANISH_STOP_WAIT` (15 s), on a
thread off the DA queue, with one `warn` naming the cause. ❌ No resume: a drive that vanished is owed nothing back, and
the crate's own stop writes the rebuild marker when deletes were in flight
(`crates/cmdr-index/src/indexing/lifecycle/DETAILS.md` § "The rebuild marker").

❗ **A pull can only stop what was recorded while the drive was still mounted**, since a disk that's gone can't be looked
up in the mount table. So a Cmdr volume is noted from every callback that carries a volume path — a disk appearing, and
each ask's own group — and an `Appeared` voids a disk's DECISIONS, never its volume map. Residual: a drive mounted
before the session installs is known only from DA's appeared burst at registration, which races volume discovery; and
"overran the window" is measured as the ask's OWN runtime, so an ask that joined a chain and answered inside 10 s can
still have overrun DA's timer, which started when DA queued it. The eject approval itself is always answered at once.

**What it can't do.** Under force (`diskutil unmount force`, `hdiutil detach -force`), DA asks, ignores the dissent, and
unmounts anyway (`DARequest.c:1610`), so the stop work happens on every ask and the dissent is only the non-force
fallback. When several indexed drives are ejected together and the chain's budget runs out, the later asks dissent with
their stops detached; under force that leaves the FSKit wedge exposure for that drive. Per-disk DA sessions are the only
thing that would beat it, and they're deferred (GitHub [#248](https://github.com/vdavid/cmdr/issues/248)). A raw
`/sbin/umount` bypasses DA entirely: no ask, no `WillUnmount`, only the aftermath.

**Evidence.** The DA behavior this section rests on (which requests ask, the queue-time timer, the timed-out session
going silent, idle's meaning, a pulled disk's callback order) is measured and cited in
`docs/notes/diskarbitration-unmount-evidence-2026-09.md`.

**Install failure** keeps the old hook: `install_or_fall_back` installs the `WillUnmount` observer only when the
approver couldn't install, and logs an `error`. ❌ Never both: two pre-unmount hooks stop the same index twice.

**Tests.** `callbacks.rs` and `records.rs` run on injected seams (`test_seams.rs`) and a fake clock. The real-image lane
(`real_image.rs`, `pnpm check disk-images`) installs a session that acts only on its own image's BSD nodes and drives
real `diskutil` unmounts: verified on macOS 27.0 (2026-09-16) that an idle volume's unmount is asked about while it's
still mounted, a held file's refusal hands the index back after the idle, the first ask of a two-partition disk lets go
of both partitions, a dissent refuses a non-force unmount, `hdiutil detach -force` ignores that dissent, and a person's
enable issued during an ask never lands an index on the drive that's leaving.

## Key decisions

**Decision**: DiskArbitration owns the pre-unmount hook; `NSWorkspaceWillUnmountNotification` is only its fallback.
**Why**: the AppKit notification is posted from AppKit's own DA approval callback on the MAIN thread, so the only way to
hold an unmount from there is to block the main thread, which freezes Cmdr and still meets DA's 10 s limit. A dedicated
session on its own queue holds the unmount without touching the main thread, hears about the refusal (idle) so it can
hand the index back, and is asked for every DA-mediated unmount, `hdiutil` and force included. The fallback keeps the
old best-effort behavior for a Mac where the session can't be created. Measured with a throwaway probe on macOS 26.6.2,
2026-09-14, and against the real session on macOS 27.0, 2026-09-16 (§ "The unmount approver").

**Decision**: Detect mounted disk images (`.dmg`) via DiskArbitration's `DADeviceModel`, set on
`LocationInfo::is_disk_image` (see `disk_image.rs`).
**Why**: Disk images are transient install-style mounts, so the UI suppresses their index affordances and free-space
bars (the frontend reads `isDiskImage`). The reliable signal is DiskArbitration: `DADeviceModel == "Disk Image"` for any
`hdiutil`-attached image (verified on macOS 15.5, 2026-06-27). Read-only is NOT a usable proxy: a writable APFS `.dmg`
reports `mount_is_read_only == false`, and conversely a locked SD card is read-only but not an image, so the two flags
are independent. `fs_type`/`f_mntfromname` don't disambiguate either (a `.dmg` can be APFS/HFS and present a normal
`/dev/diskNsM` source). The DA call is synchronous (no run loop) and cheap next to the per-volume NSURL/icon work, but it
resolves the volume path, so callers gate it to local (non-SMB) mounts to keep a hung network mount from stalling it.
Both `get_attached_volumes` (the switcher list) and `resolve_path_volume_fast` (highlight + transfer-source) set the flag
so they can't drift.

**Decision**: A local volume is ejectable when its media is ejectable (`NSURLVolumeIsEjectableKey`) OR macOS places its
disk on an external bus (`NSURLVolumeIsInternalKey == false`). One rule, `nsurl::offers_eject`, reached by both
`get_attached_volumes` and `resolve_path_volume_fast` through `nsurl::is_volume_ejectable`.
**Why**: the media flag alone means "this media ejects from its drive under software control", which a disk image and
most USB drives answer yes and an external disk macOS sees as FIXED answers no: a user's Thunderbolt/USB SSD (a SanDisk
PRO-G40) showed no eject button in v0.48.0 while a mounted `.dmg` beside it did (reported 2026-09-29; the per-key
values for that drive are inferred, not read off it). The two callers must agree because the switcher's button reads
the first and the eject command re-checks through the second, so a drift shows a button the command then refuses.
Three limits are deliberate:

- ❗ Only an explicit "not internal" counts. A disk macOS gives no answer for stays without a button.
- An internal volume with fixed media (a second APFS volume, an internal data disk) offers none, although Finder does:
  Cmdr can't mount it back, and its eject takes the whole physical disk, which there is the boot disk.
- The boot volume never ejects, even on a Mac booted from an external disk. Network mounts are never asked: they end
  through their session, and the lookup can hang on a dead one (§ "Hung mounts").

**Decision**: Populate `mount_is_read_only` for attached volumes from the `statfs` `MNT_RDONLY` flag (`read_only_from_statfs`).
**Why**: It powers the 🔒 indicator and the copy/move write guard for ANY read-only mount (a read-only `.dmg`, a locked
SD card, an optical disc), not just MTP locked storage. The frontend guard machinery (`file-operation-commands.ts`,
`transfer-entry.ts`) already keys on `mountIsReadOnly`, so populating the flag activates it with no frontend change; backend
`validate_destination_writable` (via `libc::access`) is the second line of defense.

**Decision**: A volume ID is derived from the volume's IDENTITY, never from the shape of its mount path, and every ID is
built by one funnel (`cmdr_fs::volume::ids`; `ids.rs` here picks the constructor).
**Why**: A volume ID keys the index DB, `lastUsedPaths`, tab `volumeId` fields, the `VolumeManager` registry, and
therefore operation routing. Deriving it by DELETING characters (strip everything outside `[a-z0-9-]`, then lowercase)
is a many-to-one map, so it hands two volumes one identity: `/Volumes/My Disk` and `/Volumes/My_Disk` both became
`volumesmydisk`, and a NAS's `Public` share and a Docker container's `public` share both became `volumespublic`. The
collision cross-contaminates every per-volume store, and the user-visible bug was a wrong-case path leaking from a
stale `lastUsedPaths` entry into `SmbVolume::list_directory`, where the case-sensitive `strip_prefix` against
`mount_path` failed and the smb2 path was built as `Volumes\Public` (relative under the share root), producing
`STATUS_OBJECT_PATH_NOT_FOUND` from Samba. Reads and destructive operations landing on the wrong disk is the same bug
with worse consequences.

Three properties make that unreachable rather than unlikely:

1. **Injective encoding.** Every derived ID is `{scheme}-{slug}-{digest}`, where the digest is 64 bits of BLAKE3 over
   the length-prefixed, scheme-separated canonical tuple. The slug is lossy and cosmetic (so a data dir stays
   eyeballable); nothing may key off it. Length-prefixing is what stops `("nas", "polyashare")` and `("naspolya",
   "share")` hashing the same bytes. Cryptographic rather than a fast hash because volume names are user-controlled, so
   a *chosen* collision has to be out of reach too. The bounded length matters: these are filename components
   (`index-{id}.db`).
2. **The best available identity source, per kind.** SMB keys on `(server, port, share)` with server and share
   lowercased (DNS hostnames and SMB share names are both case-insensitive, so that's canonicalization, not loss); MTP
   on the device serial; a local volume on its filesystem UUID; anything else on its mount path. `volume_id_for` is the
   single rule, shared by `get_attached_volumes` and `resolve_path_volume_fast` so they can't disagree about one
   volume.
3. **A loud registry.** `VolumeManager::register` logs an error when one ID would cover two different `root()`s. Not
   reachable from a derived ID, but a byte-for-byte volume clone reports the same UUID as its original, and so does a
   filesystem mounted twice; both are genuinely ambiguous, and neither may be silent.

**Decision**: A local volume's ID keys on its filesystem UUID (`NSURLVolumeUUIDStringKey`), not on its mount point.
**Why**: The mount point isn't stable. Plug a disk in while `/Volumes/Backup` is taken and macOS mounts it at
`/Volumes/Backup 1`; rename a volume and it moves too. A path-keyed ID therefore orphaned that volume's index and saved
paths and forced a full rescan, for the same physical disk. The UUID is read through `LocalVolumeMeta`, the existing
"blocking, local mounts only" seam, so the NSURL round-trip never happens for a network mount (it hangs on a dead one,
which is the whole point of § "Hung mounts"). A volume without a UUID (tmpfs, most FUSE mounts) falls back to its path.
`NSURLVolumeUUIDStringKey` is stringly-typed, and a typo would silently return `None` forever while every volume
quietly fell back to a path ID, so `nsurl::tests::the_boot_volume_reports_a_uuid` pins that the key still resolves
(verified on macOS 26.5.2, `getResourceValue:forKey:`, 2026-08-10).

The unmount path can't use any of this: it goes through `VolumeManager::remove_root` (root-keyed, like
`find_by_root`), because neither statfs nor NSURL recovers a gone mount's identity. When that unregisters the volume, the
watcher hands its id to `network::os_mount_notice::forget_unmounted_volume` (the Linux watcher does the same), so a
server whose slow-connection notice named this share can speak again after a remount (`network/DETAILS.md` § "Telling
the user about a kernel-mount fallback").

**Decision**: Index databases keyed by an ID from the retired scheme are deleted at launch (the reclaim half of
`Index::start_root_at_launch`, driven by `is_legacy_volume_id`), rather than migrated.
**Why**: They're disposable caches, so the cost of dropping one is a rescan, while the cost of a mis-targeted rename is
a corrupt index. Nothing can mint a legacy ID any more, so nothing will ever open these files again; left alone they'd
sit in the data dir until the LRU cap happened to reach them, which for a user under the cap is never. Persisted tab
IDs need no migration at all: `pane/initialization.ts` already re-resolves every tab's volume from its path at startup
precisely because a stored ID can go stale.

**Decision**: Gate launch-time icon fetches on the FDA decision (`crate::fda_gate::is_fda_pending_runtime()`).
**Why**: `NSWorkspace.iconForFile:` resolution touches LaunchServices and several adjacent TCC services beyond the input
path. On a fresh prod install with FDA off, calling it for `/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads`,
the iCloud root, and per-provider cloud-storage paths stacked 5-10 macOS native permission popups (MediaLibrary,
AppData, Desktop, Documents, Downloads, …) on top of the in-app FDA modal, exactly the onboarding flood the modal is
meant to replace. Returning `icon: None` from `get_icon_for_path()` while the gate is pending eliminates the class; the
frontend falls back to a generic folder icon, so the sidebar still shows favorite/volume entries (just generic for the
few seconds before the user decides). See `commands/indexing.rs::start_indexing_after_fda_decision` for the gate-clear +
re-emit on the deny path; the allow path requires a restart, so re-entering `setup()` sets the gate to `false` via the
OS probe. The gate's frontend half is `apps/desktop/src/lib/onboarding/CLAUDE.md`.

**Decision**: Use `NSWorkspace` notifications, not an FSEvents watcher on `/Volumes`.
**Why**: FSEvents fires when the kernel writes a directory entry under `/Volumes`, which races the mount: `statfs` on the
new mount point still returns the root filesystem's `fsid` until the OS finishes mounting. Polling `fsid` to settle
times out on slow drives behind USB-C/Thunderbolt docks, and a timeout would filter the volume out until an app
restart. `NSWorkspace` notifications are posted by `diskarbitrationd` after the mount is fully settled and
`NSFileManager` metadata is ready, so there's no race, and they carry the volume URL directly in
`userInfo[NSWorkspaceVolumeURLKey]` (no diffing or polling). DiskArbitration would work too but needs a CFRunLoop
scheduled separately from Tokio; `NSWorkspace` rides on the AppKit runloop Tauri already runs.

**Decision**: Use `OnceLock` for `APP_HANDLE` and `OBSERVER_INSTALLED`.
**Why**: `start_volume_watcher` must be idempotent; `OnceLock::set` failing on the second call is the gate. `LazyLock`
would initialize eagerly, which doesn't work because the `AppHandle` isn't available at static-init time.

**Decision**: Use `NSURLVolumeAvailableCapacityForImportantUsageKey` with fallback to `NSURLVolumeAvailableCapacityKey`.
**Why**: The "ForImportantUsage" key accounts for purgeable space (iCloud, APFS snapshots), matching what Finder shows.
The plain key reports only physically free blocks, misleadingly low on APFS volumes with purgeable data. The fallback
handles older macOS versions lacking the key.

**Decision**: `supports_trash` defaults to `true` for unknown filesystem types.
**Why**: Optimistic default. Most local filesystems support trash; the exceptions (network mounts, FAT-family) are
explicitly listed. If an unknown fs type doesn't support trash, the op fails gracefully at trash time, better than
pessimistically disabling trash for a filesystem that supports it.

**Decision**: Use `libc::statfs` for filesystem type detection, not `NSURLVolumeLocalizedFormatDescriptionKey`.
**Why**: The NSURL key returns a locale-dependent human string ("APFS (Case-sensitive)"). `statfs.f_fstypename` returns
a stable machine identifier ("apfs", "smbfs", "nfs") that matches against the known network/non-trash list.

**Decision**: Every `statfs` name field is read through `fs_type::statfs_string` (UTF-8, lossy), never
`c as u8 as char`.
**Why**: `f_mntonname` and `f_mntfromname` hold real paths, so the latin-1 read turned a mount at `/Volumes/公開` into
`/Volumes/å¬é`. The mangled path then failed its own `statfs`, `get_smb_mount_info` returned `None`, and the share
published as an unknown volume under a `path-volumes-å-é-…` id instead of its `smb-…` one (verified against
`smb-consumer-unicode` on macOS 15.5, 2026-08-22). Lossy rather than strict, because a mount label is whatever bytes
the volume carries and a replacement character beats losing the mount.

## SMB mount sources are percent-escaped

macOS's SMB stack only speaks URLs: `mount_smbfs //guest@localhost/café` is rejected outright ("URL parsing failed"),
and a mount of that share records `//guest:@localhost:11484/caf%C3%A9` in `f_mntfromname`. `parse_smb_mount_source`
percent-DECODES server, share, and username on the way out (`decode_mount_field`), so everything downstream compares
and hashes the name the server actually advertises.

Left escaped, the mounted `café` never equals the advertised `café`: `mount.rs::find_mount_path_for_share` reports a
live mount as missing (so the app re-mounts a share it already has), and `smb_volume_id` derives one id from the share
list and a different one from `statfs` for the same mount. Both halves are caught by
`smb_integration_mount_non_ascii_share`, which mounts `café` for real and then asserts BOTH that
`find_mount_path_for_share` finds it and that `resolve_path_volume_fast` derives the `(server, port, share)` id from
that live mount. A `%` that isn't a valid escape is a character in a name, so a decode failure keeps the source text
rather than dropping the mount.

The authority half (`user:password@host:port`, with a bracketed IPv6 host) goes through
`cmdr_fs::volume::smb_mount_source::split_authority`, shared with the Linux twin: a guest mount records its empty
password (`//guest:@…`), which must never ride along in the username, and an IPv6 host comes back unbracketed with its
port (`//guest:@[::1]:18445/public` → `::1`, 18445). The rules: `crates/cmdr-fs/DETAILS.md`.

The escaping half lives with the mount: `network/mount.rs::build_smb_mount_url`.

## A mount can sit inside its share

A share name is ONE path segment: it's what goes to TreeConnect, so it can never carry a separator. A mount source with
more segments than that (`//dana@example.com/SYSVOL/example.com`) names a DIRECTORY inside the share, and
`parse_smb_mount_source` splits it accordingly into `share` plus `SmbMountInfo::subpath` (`None` at the share root,
never `Some("")`, which would make the downstream join prepend a `/`).

macOS produces this shape on its own: it follows a DFS referral by making a SECOND mount underneath the namespace root,
so `smb://example.com/SYSVOL` leaves `/Volumes/SYSVOL/example.com` live beside `/Volumes/SYSVOL`. A subdirectory mount
(`mount_smbfs //server/share/sub`) is the same shape asked for deliberately, and Linux's `mount -t cifs` records it the
same way, which is why `volumes_linux/smb.rs` parses identically.

Swallowing the whole tail into `share` sent `SYSVOL/example.com` to TreeConnect, which no server has a share for:
`STATUS_BAD_NETWORK_NAME`, so the share stayed on the slow kernel mount and the user was warned about a share already
connected directly one level up (reported as ERR-48RZX).

**Decoding happens per SEGMENT, after the split.** Each segment is escaped on its own
(§ "SMB mount sources are percent-escaped"), so decoding the tail as one string would turn a `%2F` inside a directory
name into a separator that was never in the path.

**A mount anchored inside a share is the SAME volume as the share.** `volume_id_for` keys on `(server, port, share)`
and ignores the subpath, so the nested mount and the namespace root collapse to one ID, one session, one index, and one
set of saved paths, and the redundant upgrade attempt short-circuits as already-direct. What the backend then does with
the anchor (both path directions, and how a promotion between mount roots resolves one): `crates/cmdr-smb/DETAILS.md`
§ "A mount anchored inside the share".

## Gotchas

**Gotcha**: `VolumeInfo` is a type alias for `LocationInfo`, not a separate type.
**Why**: The frontend sends/receives `VolumeInfo`, but locations also cover favorites and cloud drives. The alias keeps
IPC compatibility without a frontend migration.

**Gotcha**: The watcher registers/unregisters volumes with `VolumeManager` directly (tight coupling to
`file_system::volume::manager::get_volume_manager()`).
**Why**: A mounting volume must be immediately available for file operations. Emitting only a Tauri event and letting the
frontend trigger registration would open a race window where ops fail because the volume isn't registered yet. Direct
registration ensures that by the time the frontend gets `volume-mounted`, the volume is usable.

**Gotcha**: `get_main_volume`, `get_attached_volumes`, and `get_volume_space` wrap their bodies in
`objc2::rc::autoreleasepool`.
**Why**: Called from `spawn_blocking` threads. Without a pool, the per-call `NSFileManager`/`NSURL`/`NSString`/`NSNumber`
objects accumulate in a default pool that's never drained, leaking memory over hours.

**Gotcha**: The observer block in `watcher.rs::install_observers` runs on the main thread.
**Why**: With `queue: nil`, AppKit dispatches the block on the thread that posted the notification, and
`diskarbitrationd` posts on the main thread. Keep the body cheap: `register_volume_with_manager` is microseconds,
`try_upgrade_smb_mount` and `emit_volumes_changed` both `tauri::async_runtime::spawn`, and `app.emit` is non-blocking.
Don't add blocking I/O here without moving it onto a background task. The rename block does exactly that: following a
rename derives an id (an NSURL read) and restarts the drive's index (a drain of up to seconds), so it hands
`handle_volume_renamed` a thread of its own (`file_system/volume/DETAILS.md` § "A renamed drive").

**Gotcha**: `userInfo` is downcast with `Retained::cast_unchecked` to `NSDictionary<NSString, NSURL>`.
**Why**: AppKit documents the values under `NSWorkspaceVolumeURLKey` and, on a rename, `NSWorkspaceVolumeOldURLKey` as
`NSURL`s, and those are the only keys read through the cast (the dictionary also carries localized-name `NSString`s). The unchecked cast trades a runtime
type check for a hard contract on Apple's side. A safer alternative (`cast::<NSDictionary>` plus a per-value
`downcast::<NSURL>`) costs an `isKindOfClass:` call per notification. We lean on the documented contract; revisit if a
future macOS version breaks it.

## Dependencies

- External: `dirs`, `objc2`, `objc2_foundation`, `objc2_app_kit` (`NSWorkspace`), `block2` (`RcBlock`).
- Internal: `crate::file_system::volume::{manager::get_volume_manager, LocalPosixVolume}`, `crate::icons::get_icon_for_path`.

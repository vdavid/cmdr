# Volume backends details

Pull-tier docs for `file_system/volume/backends/`: per-backend architecture, lifecycle flows, and decision rationale.
Must-know invariants and gotchas live in `CLAUDE.md`. The trait shape, capability matrix, streaming patterns, and
"Building a new volume" checklist live in the parent `../DETAILS.md`. When you're modifying `LocalPosixVolume` or
`InMemoryVolume`, read here; for a crate backend, read its own `DETAILS.md` (`crates/cmdr-smb/`, `crates/cmdr-mtp/`,
and so on).

## Key files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` §
Module map. `local_posix.rs` splits into itself plus `local_posix/scan.rs` and `local_posix/streams.rs`. What each piece DOES is in the sections below (§ "SMB
auto-upgrade lifecycle", § "Per-backend decisions", § Testing), or in `crates/cmdr-archive/DETAILS.md` for
`ArchiveVolume` and `crates/cmdr-smb/DETAILS.md` for `SmbVolume`. Only the layout facts that none of those carry live
here:

- **The SMB backend owns no `AppHandle` and names no `tauri` type.** Every reach into the app goes through the
  `VolumeHost` it takes in `connect_smb_volume` and stores on `SmbVolumeInner`: pane listings, the secret store, the
  index, the frontend event channel, the concurrency knob, the foreground signal, analytics, and the runtime background
  work spawns onto. The seam set and what each one replaces: `crates/cmdr-fs/src/volume/host/DETAILS.md`.
- **The `smb-fell-back-to-os-mount` notice is app-side, both halves.** `network/os_mount_notice.rs` decides whether to
  speak (once per server per run, and only for a caller whose `FallbackNotice` says someone is watching) and emits the event, holding the only `AppHandle` this corner of the app needs. The
  typed `tauri_specta::Event` structs and the wire `VolumeConnection` enum stay in the always-compiled `network/events.rs`,
  so `collect_events!` in `ipc.rs` can reference them on EVERY platform; the `smb` module is `#[cfg]`-gated to macOS
  and Linux (as is `mtp/`), and moving a struct in there breaks the Windows build of the event collector.
- **`volume-connection-changed` is backend-neutral, and SMB is only its first emitter.** Any backend that holds a
  session (FTP, S3, SFTP) emits the same event and inherits the frontend's unreachable banner, per-volume backoff, and
  "Sign in" prompt for free. ❌ Don't add a second, backend-named connection event: widen `VolumeConnection` and reuse
  this one. The `From<ConnectionState>` impl in `crates/cmdr-smb/src/volume/state.rs` shows the shape a backend supplies, mapping its own
  internal state machine onto the wire enum.
- **`in_memory.rs`'s `with_file_count` builder is what makes `InMemoryVolume` usable for stress tests**, not just CRUD
  unit tests.

## SMB auto-upgrade lifecycle

SMB mounts are automatically upgraded to `SmbVolume` (direct smb2 connection) in three scenarios:

1. **Startup** (`file_system::upgrade_existing_smb_mounts()`): Reads the kernel's mount table
   (`volumes::smb_mounts`, the non-blocking `getfsstat` snapshot) for SMB shares no `SmbVolume` serves. ❌ Not the
   volume registry: that fills on a background thread that `statfs`es every mount, so at launch it lags the kernel by
   seconds, and a gate that asked it read "No SMB mounts to upgrade" with four shares up, on every launch (issue #123).
   If any are found, holds the mDNS browse for the pass (creds are keyed by hostname, not IP), then waits for mDNS to
   reach `Active` state (polls every 500ms, up to 15s). Uses `tauri::async_runtime::spawn` (not
   `tokio::spawn`; runs during `setup()` before Tokio is fully available). Emits `volumes-changed` after upgrades so
   the frontend refreshes indicators. **No `firstTriggerDone` gate**: the function is a no-op when no SMB mounts are
   present (no network activity, no macOS Local Network prompt). When mounts are present AND `network.directSmbConnection`
   is on (default `true`), it starts the browse — that's when the macOS prompt fires, once per app per data dir. Without
   this, dev profiles with auto-reconnected SMB shares would stay on the slow OS-mount path forever.

2. **Mount detection** (`volumes/watcher.rs::try_upgrade_smb_mount`): When FSEvents detects a new volume in `/Volumes/`
   and it's `smbfs`, spawns a background upgrade attempt, which holds the mDNS browse through `discover_server`.

3. **Pane open** (`network::smb_pane_upgrade::upgrade_on_pane_open`, from `list_directory_start_streaming`): a pane
   landing on an OS-mounted share Cmdr hasn't upgraded tries that one share, behind a 60 s per-share cooldown.
   `network/DETAILS.md` § "A pane on an OS-mounted share tries the direct connection".

All three paths check the `network.directSmbConnection` setting (global `AtomicBool`). Both are best-effort. Failures log a
warning and the volume stays as `LocalPosixVolume`. The "Connect directly" UI action (`upgrade_to_smb_volume` command)
and the MCP `upgrade_smb_to_direct` tool provide manual upgrade paths.

### Every upgrade decides at ACT time, never at trigger time

Each path waits before it connects: 1.5 s for the mDNS host cache on every path, and up to 15 s more for
`wait_for_mdns_ready` on the startup pass. Any other path can finish the job inside that window, so a decision made
before the wait is stale by the time it's used. Two rules keep that from turning into redundant swaps:

- **The startup pass scans after its wait, not before.** `upgrade_existing_smb_mounts` still does a pre-scan, but
  purely as a gate: nothing to do ⇒ return without touching mDNS, so a machine with no SMB mounts never sees the macOS
  Local Network prompt. The list it acts on comes from a second `os_mounted_smb_shares()` call once mDNS has settled,
  which also picks up shares mounted during the wait. Both read the mount table, so neither depends on how far the
  registry sweep has got; a share the sweep hasn't reached yet is upgraded anyway, and the sweep's `register_if_absent`
  then leaves the `SmbVolume` in place. Two mounts of one share (a DFS referral) get one attempt
  (`smb_upgrade::shares_to_adopt`).
- **Every connect site re-checks first.** `register_smb_volume` and `try_smb_upgrade` both bail via
  `smb_upgrade::is_already_direct` when the id already resolves to a healthy direct volume. `Disconnected` deliberately
  does not count: that's the manual "Connect directly" recovery path, and short-circuiting it would dead-end the user.

### A first connect that never reached the server gets one more try

`connect_with_retry` (in `network/smb_upgrade.rs`) wraps `connect_smb_volume` on both upgrade paths. The first direct
connect to a private LAN address shortly after launch routinely comes back `EHOSTUNREACH` while the route and the macOS
Local Network permission settle, and the identical attempt moments later succeeds (three times in one session on
2026-08-01, each followed by a clean connect). Without a retry the user has to notice and click "Connect directly"
again, which is exactly what produced the double upgrade pass above.

Two bounds keep a genuinely-down server failing promptly, because someone is watching a "Connecting directly…" toast:

- **Count**: `CONNECT_RETRY_BACKOFF` (300 ms, 1200 ms) ⇒ three attempts at most.
- **Cost**: `CONNECT_RETRY_BUDGET` (2 s) measured across the ATTEMPTS. An `EHOSTUNREACH` returns instantly so a real
  blip gets its retries; an attempt that ate the 10 s connect timeout already answered the question, and stacking
  another would triple the wait.

Only `UpgradeFailure::Unreachable` retries. An auth rejection is final (retrying risks locking the account; the "Sign
in" flow owns that recovery), and so is anything the server itself answered with.

**The reason crosses IPC typed, never as a sentence.** `UpgradeFailure` (variants and the `blockedByThisMac` evidence
rule: `network/DETAILS.md` § "This Mac refusing the route") is
classified in Rust by io kind and smb2 error kind — never by message text — and the frontend
writes the copy from the catalog (`src/lib/file-explorer/network/upgrade-messages.ts`). The raw error stays in the log
where it's a diagnostic. Before this, `try_smb_upgrade` built an English sentence in Rust, the toast wrapped it in
"Direct connection failed: " (the style guide forbids "failed" outright), and the two catch-block call sites pasted a
raw `String(e)` in the same slot.

**Only one pass runs at a time** (`smb_upgrade::UpgradePass`, an RAII guard over a process-global flag).
`note_network_action` calls `upgrade_existing_smb_mounts` on every user networking action, so without the
guard N actions stack N passes that each sleep 15 s and then fire. Dropping the extra triggers is safe precisely
because the running pass re-scans at act time.

The failure this prevents: two "Connect directly" clicks nine seconds apart replaced one healthy volume three times in
15 seconds, and the third replacement landed in the middle of a 3 GB copy to the NAS.

## The SMB backend

`SmbVolume` itself — the reconnect lifecycle, the scan-connection pool, re-rooting, the archive push-refresh, and its
decisions — is `crates/cmdr-smb/DETAILS.md`. What stays on this side is the auto-upgrade lifecycle above (it is
`network/`'s, not the backend's). The app-side suites sit with the app code they assert on; the map is
`crates/cmdr-smb/DETAILS.md` § "Which side a test lives on", and what they pin is
`file_system/write_operations/DETAILS.md` § "The SMB app-side suites".


## Per-backend decisions

**Decision**: `SmbVolume` and `MtpVolume` store `volume_id: String` for listing cache lookups
**Why**: `notify_mutation` needs to call `host.listings().directory_changed(volume_id, ...)` to find the right cached listings. The volume_id is computed at creation time (`smb_volume_id(server, port, share)` for SMB so two same-named shares on different servers don't collide — see `volumes/CLAUDE.md` § "Volume IDs"; `"{device_id}:{storage_id}"` for MTP) and stored on the struct rather than recomputed on every mutation.

**Decision**: `LocalPosixVolume::write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before it returns
**Why**: Every cross-volume copy/move that lands on a local disk (MTP → Local, SMB → Local, USB import) flows through this one method. A bare `file.flush()` finish is a userspace no-op on a raw `std::fs::File`, so the bytes would sit only in the OS page cache when the op reports "complete" — letting the user eject / sleep and lose data (on a move, from both sides, since the source delete runs after the copy reports Ok). The `sync_data` (fdatasync) gives the "durable as each file completes" property the local-FS chunked copy already has (`transfer/chunked_copy.rs`), so a crash mid-batch leaves earlier files safe. The parent-dir fsync makes the file's directory entry durable too. Both are best-effort on error: a failure logs under `target: "write_durability"` and continues rather than failing a completed multi-GB transfer at the final fsync (matching `durability::flush_created_destinations`). Non-local backends (MTP/SMB/InMemory) need no equivalent — durability there is the device/server's concern. Pinned by `local_posix_test::test_write_from_stream_multichunk_is_durable_and_correct` (content-correctness regression guard; the fdatasync itself isn't observable from a unit test).

**Decision**: `LocalPosixVolume` overrides `is_directory` and `entry_kind` with a bare `lstat`
**Why**: `get_metadata` also stats a link's target and looks up owner names, none of which answers "what is at this path". Both answers see a link as the link, which a move engine depends on (`../../write_operations/transfer/DETAILS.md` § "Symlinks are opaque to a move"); ❌ don't route either through `get_metadata`, whose `is_directory` is true for a link to a folder.

**Decision**: `local_posix` stays in the app crate permanently; it is NOT a candidate for a backend crate
**Why**: it looks like the smallest backend (1,018 lines across `local_posix.rs` + `local_posix/`, measured 2026-09-05) and is the one backend whose couplings are to the app's LOCAL machinery rather than to a protocol. It is the only caller of the real-FS reader in `listing/reading.rs`, which serves the non-volume listing path too, and it's the FSEvents watcher's peer. It's the sole caller of `find_listings_for_path_on_volume` and `patch_listing_after_local_mutation`, and the latter is *definitionally* local — it `std::fs`-stats the changed entry, which no backend on a protocol can do. **Those two reasons are the whole argument, and they are the only ones.** The git portal is ❌ not a third: it reaches a repo's virtual trees through a route plus a listing overlay, and `local_posix.rs` names git nowhere (`file_system/git/DETAILS.md` § "Two seams, no hooks"). ❌ Don't propose this as "completing the set" once FTP and S3 are crates: the set is deliberately incomplete. Seam rationale: `crates/cmdr-fs/src/volume/host/DETAILS.md`.

**Decision**: MTP is `crates/cmdr-mtp`, and the three things that once read as permanent refusals each got an answer
**Why**: a backend has two faces (`../DETAILS.md` § "Architecture"), and MTP's split along them: the file-ops face was already the `Volume` trait, so all the work was on the lifecycle face, done as an in-place retrofit onto the host seams with the whole suite watching, which left the move itself a `git mv`. The three refusals: the 13 `specta::Type` + `tauri_specta::Event` derives inside the transport layer became one crate-local `MtpDeviceEvents` trait carrying a typed `MtpDeviceEvent`, with the payload structs and their derives in the app-side `apps/desktop/src-tauri/src/mtp/events.rs` — the same "backend says WHAT, host says what the user sees" split every other backend lives under. The nine inline `#[cfg(test)]` gates on real behavior became `any(test, feature = "testing")`, the crate rule that exists precisely because `cfg(test)` is set only for a crate's own test target. And the test hooks became a gated `pub` under `cmdr_mtp::volume::testing`, the argued exception SMB already granted `detach_session_for_test` (`crates/cmdr-fs/src/volume/host/DETAILS.md` § "Visibility that has no cross-crate equivalent"), because the app's scan-oracle cell asserts on the APP's fresh-listing oracle and belongs app-side. What stays app-side: the hotplug watcher (ADB's tracker twin), the macOS workaround, the registrar wiring, the tauri event payloads, and the IPC commands. Where the boundary runs in full: `crates/cmdr-mtp/DETAILS.md`.

**Decision**: a backend never registers itself; an outside wiring module does
**Why**: registration needs to know both the concrete volume type and the manager, and a backend that reaches the registry to insert itself draws a dependency edge back up into the layer that knows every backend — which is exactly what welds a subsystem into one cycle and what a backend crate cannot do at all. `network/smb_upgrade.rs` and `mtp/volume_wiring.rs` are the two structural twins to copy: the backend exposes a constructor and, where it needs to trigger registration from deep inside (MTP's attach/detach), a `OnceLock` hook the wiring module fills at startup. **Preserve the ORDERING deliberately when you wire one**: MTP's connect path registers volumes before starting its event loop, and a hook adds an indirection that can quietly change when that happens. This is not settleable by static analysis; verify against a real device or the `virtual-mtp` feature.

The SMB backend's own decisions moved with its code: `crates/cmdr-smb/DETAILS.md` § "Decisions".


## Supersede vs. unmount

`Volume` has two retirement hooks, and confusing them breaks live operations.

- **`on_unmount`**: the device is gone (ejected, network mount torn down, FSEvents unmount). Tear everything down.
  `SmbVolume` flips `unmounted`, forces state to `Disconnected`, cancels the watcher, closes the scan pool, and drops
  the smb2 `Tree` + `SmbClient`. Callers: `volumes::watcher::handle_volume_unmounted` and `volume::eject`.
- **`on_superseded`**: a NEWER instance took this volume's id in the `VolumeManager`, but the device is still there.
  Sole caller: `network::smb_upgrade::register_replacing_predecessor`.

Both record the same fact through the same flag, `SmbVolumeInner::retirement` (a `cmdr_fs::volume::Retirement`): this
share no longer owns its volume id. The registry sets it too, for the third way out that neither hook covers — a volume
REMOVED without being replaced or unmounted (an eject, the last mount root of a share going away). Everything that reads
it treats the three alike, because to the watcher, the scan pool, and the connection events they are the same thing. Why
the registry is the writer, and why a re-root deliberately isn't a retirement:
`crates/cmdr-fs/src/volume/host/DETAILS.md` § "The two registry reach-backs".

**The invariant: a superseded volume keeps serving its holders.** The `VolumeManager` is not the only owner of a
`Volume`. Anything that resolved the id earlier holds an `Arc` for the whole duration of its work:

- a running transfer clones `src_vol` / `dst_vol` into every per-file task (`write_operations::transfer::volume::copy`),
- the file viewer holds an open `VolumeReadStream`,
- an in-flight listing, a conflict scan, and a preflight walk each hold one,
- the indexer holds one across a scan session.

None of those can switch to the successor mid-flight, and the busy-volumes set doesn't track most of them. So
`SmbVolume::on_superseded` leaves `state`, `tree`, and `client` untouched. The session is released when the last `Arc`
drops (smb2 aborts its receiver task with the last `Arc<Inner>`), which makes the lifetime structurally correct rather
than a race to be timed. ❌ Never reinstate a teardown here: it killed a live NAS copy with `DeviceDisconnected` on a
connection that was still healthy (a redundant upgrade pass replaced the volume mid-transfer). Pinned by
`smb_integration_superseded_volume_still_serves_its_holders` and
`smb_upgrade::tests::a_held_volume_reference_keeps_working_across_a_replace`.

**What DOES retire is everything scoped to the volume ID**, because the successor owns that now:

- The **watcher** is cancelled. It runs on its own dedicated session (so cancelling it can't disturb a transfer), and
  two watchers on one id double-feed the listing cache and the index.
- The **scan pool** opens no new connections (an already-open one drains with its scan).
- **`volume-connection-changed` events** are suppressed (`emit_state_change_for_id`, `update_state_on_smb_error`). A
  retired instance announcing a disconnect would tell the frontend a healthy volume just dropped.
- The **index-resume hook** is skipped in `do_attempt_reconnect`; the successor ran it when it registered.

A superseded volume still **reconnects** for the holders on it (their only recovery path, since they can't move to the
successor) — silently, without respawning a watcher.

**Watcher identity is a pointer, not an id.** `spawn_watcher_death_reconnect` takes the dying watcher's
`SelfHandle<SmbVolumeInner>` and re-upgrades it on every backoff step, so it acts only for the share it was spawned for.
Resolving the id instead would answer with the SUCCESSOR after a swap and mark a perfectly healthy volume
`Disconnected`, and would keep answering after an eject for as long as any in-flight holder kept the share allocated.
`cmdr-smb`'s `retirement_test.rs` pins all three answers, and `manager::tests::unregistering_a_volume_retires_it` pins the registry's side of them.

## Testing

- `in_memory_test.rs`: unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `local_posix_test.rs`: real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- **No SMB, archive, or MTP cell lives here.** Which side of the crate boundary one belongs on is decided by what it asserts,
  not by what it connects to (`crates/cmdr-smb/DETAILS.md` § "Which side a test lives on"), and the app-side ones then
  sit beside the app code they assert on. What they pin, the Docker fixture ports, and the soak and wedge harnesses:
  `file_system/write_operations/DETAILS.md` § "The SMB app-side suites".

## Local renames are atomic-exclusive

`LocalPosixVolume` routes every non-forced rename through the shared atomic-exclusive primitive,
`rename_local_exclusive`. This applies equally to `/`, attached disks, Dropbox, iCloud, and other local POSIX roots
registered with non-root volume IDs. Forced renames retain normal POSIX replacement semantics because the caller
explicitly authorized replacement.

**There is exactly one implementation of it**, and `write_operations::overwrite::rename_no_replace` is a one-line
delegation to it. Both names have to stay one function: while they were two, only the copy side degraded on a
filesystem lacking the kernel flag, so the pane's own renames failed on every mounted SMB share while a copy landing in
the same folder succeeded (ERR-8RFN4, app 0.45.1).

**The kernel flag is not universal.** `renamex_np(RENAME_EXCL)` and `renameat2(RENAME_NOREPLACE)` are APFS, HFS+, and
ext4; macOS's smbfs answers `ENOTSUP` (errno 45 — verified twice against Synology SMB mounts: macOS 26.6.2 from the
ERR-8RFN4 bundle's log, and macOS 27.0 by a direct `renamex_np` probe where APFS took the flag, the mount returned
errno 45, and the check-then-rename fallback landed, 2026-09-17), and other non-native mounts answer `EINVAL` or
`ENOSYS`. Those three degrade to a
`symlink_metadata` check then a plain `rename(2)`; ❌ every other errno reaches the caller unchanged, or a refusal the
filesystem meant becomes a second, quieter way to fail. The degradation is racy and deliberately so: a lost race
refuses, where a bare `rename(2)` would clobber silently and unrecoverably.

**FAT32 and exFAT are NOT on the degraded path**, which is worth knowing before sizing any work that calls itself
"the USB-stick case": both take the flag and answer `EEXIST` (verified on macOS 27.0 by a direct `renamex_np` probe
against freshly created FAT32 and exFAT disk images alongside HFS+ and APFS controls, all four identical, 2026-09-19).
What actually reaches the fallback is macOS-mounted SMB and FUSE mounts.

**The check is three-valued, and ❗ that is the whole point.** `name_state` folds the `symlink_metadata` result into
`Occupied` / `Free` / `Unknown(err)`, and `rename_if_free` renames on `Free` alone. A stat that failed for any reason
other than the name being free has proved nothing about the name, so `Unknown` refuses and carries the filesystem's
own errno out. Collapsing it into "free" (which `.is_ok()` did) turns every transient stat failure into a silent,
unrecoverable clobber, on exactly the flaky mounts that reach this path in the first place. The enum exists so the
fold can't be written as a bool again.

The `ENOTSUP` path can't be reached on a filesystem a test can mount, and `testing::disk_images` is not for minting a
FAT image, so `rename_exclusive_with` takes the attempt as a parameter and `local_posix_test.rs` injects the errno.
The `Unknown` arm needs no seam at all: a stat and the rename beside it resolve the same path through the same
permissions, so nothing a test can set up fails one without failing the other, and only a transient failure separates
them. `local_posix_test.rs` hands `rename_if_free` the verdict directly instead.

## Where the shared conformance assertions live

`cmdr_fs::volume::conformance` holds the promises no backend may quietly opt out of, and each backend runs the ones it
can: `local_posix_conformance_test.rs` here, and every crate backend's own `volume/conformance_test.rs`. MTP's
`volume/delete_test.rs` stays separate from its conformance file because the non-recursion contract is the one MTP has
to IMPLEMENT rather than inherit (`MtpDeleteScope`), with enough scaffolding to earn its own file. The roster and what
each one defends: `crates/cmdr-fs/DETAILS.md` § "The shared assertions in `volume::conformance`".

Where a crate backend's own `Volume` impl deviates from the shared shape is that crate's business, not this doc's:
MTP's five deviations (the grouped copy scan, the expensive `get_metadata`, the safely-droppable read stream, the
conflict-scan destination probe, and the check-then-act no-clobber rename) are `crates/cmdr-mtp/DETAILS.md` § "What
`MtpVolume` does differently".

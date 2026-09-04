# Volume backends details

Pull-tier docs for `file_system/volume/backends/`: per-backend architecture, lifecycle flows, and decision rationale.
Must-know invariants and gotchas live in `CLAUDE.md`. The trait shape, capability matrix, streaming patterns, and
"Building a new volume" checklist live in the parent `../DETAILS.md`. When you're modifying `MtpVolume`,
`LocalPosixVolume`, or `InMemoryVolume`, read here; for `SmbVolume` and its watcher, `crates/cmdr-smb/DETAILS.md`.

## Key files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` §
Module map. `mtp/` splits into `mod.rs`, `volume_impl.rs`, `streams.rs`, `mapping.rs`, `cancel.rs`, and `scan.rs`;
`local_posix.rs` into itself plus `local_posix/scan.rs` and `local_posix/streams.rs`. What each piece DOES is in the sections below (§ "SMB
auto-upgrade lifecycle", § "Per-backend decisions", § Testing), or in `crates/cmdr-archive/DETAILS.md` for
`ArchiveVolume` and `crates/cmdr-smb/DETAILS.md` for `SmbVolume`. Only the layout facts that none of those carry live
here:

- **The SMB backend owns no `AppHandle` and names no `tauri` type.** Every reach into the app goes through the
  `VolumeHost` it takes in `connect_smb_volume` and stores on `SmbVolumeInner`: pane listings, the secret store, the
  index, the frontend event channel, the concurrency knob, the foreground signal, analytics, and the runtime background
  work spawns onto. The seam set and what each one replaces: `crates/cmdr-fs/src/volume/host/DETAILS.md`.
- **The `smb-fell-back-to-os-mount` notice is app-side, both halves.** `network/os_mount_notice.rs` decides whether to
  speak (once per server per run) and emits the event, holding the only `AppHandle` this corner of the app needs. The
  typed `tauri_specta::Event` structs and the wire `VolumeConnection` enum stay in the always-compiled `network/mod.rs`,
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

SMB mounts are automatically upgraded to `SmbVolume` (direct smb2 connection) in two scenarios:

1. **Startup** (`file_system::upgrade_existing_smb_mounts(app_handle)`): Scans registered volumes for `smbfs` type. If
   any are found, calls `network::ensure_mdns_started` to kick off mDNS itself (creds are keyed by hostname, not IP),
   then waits for mDNS to reach `Active` state (polls every 500ms, up to 15s). Uses `tauri::async_runtime::spawn` (not
   `tokio::spawn`; runs during `setup()` before Tokio is fully available). Emits `volumes-changed` after upgrades so
   the frontend refreshes indicators. **No `firstTriggerDone` gate**: the function is a no-op when no SMB mounts are
   present (no network activity, no macOS Local Network prompt). When mounts are present AND `network.directSmbConnection`
   is on (default `true`), it kicks off mDNS — that's when the macOS prompt fires, once per app per data dir. Without
   this, dev profiles with auto-reconnected SMB shares would stay on the slow OS-mount path forever.

2. **Mount detection** (`volumes/watcher.rs::try_upgrade_smb_mount`): When FSEvents detects a new volume in `/Volumes/`
   and it's `smbfs`, spawns a background upgrade attempt. Calls `ensure_mdns_started` to kick off mDNS too.

Both paths check the `network.directSmbConnection` setting (global `AtomicBool`). Both are best-effort. Failures log a
warning and the volume stays as `LocalPosixVolume`. The "Connect directly" UI action (`upgrade_to_smb_volume` command)
and the MCP `upgrade_smb_to_direct` tool provide manual upgrade paths.

### Every upgrade decides at ACT time, never at trigger time

Each path waits before it connects: 1.5 s for the mDNS host cache on every path, and up to 15 s more for
`wait_for_mdns_ready` on the startup pass. Any other path can finish the job inside that window, so a decision made
before the wait is stale by the time it's used. Two rules keep that from turning into redundant swaps:

- **The startup pass scans after its wait, not before.** `upgrade_existing_smb_mounts` still does a pre-scan, but
  purely as a gate: nothing to do ⇒ return without touching mDNS, so a machine with no SMB mounts never sees the macOS
  Local Network prompt. The list it acts on comes from a second `os_mounted_smb_shares()` call once mDNS has settled,
  which also picks up shares mounted during the wait.
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

**The reason crosses IPC typed, never as a sentence.** `UpgradeFailure` (`unreachable` / `tooSlow` / `unexpected`) is
classified in Rust by io kind and smb2 error kind — never by message text — and the frontend
writes the copy from the catalog (`src/lib/file-explorer/network/upgrade-messages.ts`). The raw error stays in the log
where it's a diagnostic. Before this, `try_smb_upgrade` built an English sentence in Rust, the toast wrapped it in
"Direct connection failed: " (the style guide forbids "failed" outright), and the two catch-block call sites pasted a
raw `String(e)` in the same slot.

**Only one pass runs at a time** (`smb_upgrade::UpgradePass`, an RAII guard over a process-global flag).
`ensure_network_discovery_started` calls `upgrade_existing_smb_mounts` on every user networking action, so without the
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

**Decision**: `MtpVolume` overrides `scan_for_copy_batch_with_boundary` to group selected paths by parent and list each parent once
**Why**: MTP has no single-file stat call: `get_metadata(path)` lists the parent directory and searches by name. A naive scan that called `get_metadata` per path would re-list `/DCIM/Camera` (15k entries, ~17 s over USB) for every selected photo. The override groups the input paths by parent, calls `list_directory(parent, on_progress)` once per unique parent, and indexes the entries by name for O(1) lookups. **Oracle layered on top**: before listing a parent, the override consults `try_get_authoritative_listing(volume_id, parent)`; on hit, the cached entries replace the listing call entirely (no USB I/O for that parent). On miss the single-listing-per-parent path runs, so cold-cache perf is preserved. Decision is per-parent; one batch can mix watcher-fresh and cold parents.

**Decision**: `LocalPosixVolume::write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before it returns
**Why**: Every cross-volume copy/move that lands on a local disk (MTP → Local, SMB → Local, USB import) flows through this one method. A bare `file.flush()` finish is a userspace no-op on a raw `std::fs::File`, so the bytes would sit only in the OS page cache when the op reports "complete" — letting the user eject / sleep and lose data (on a move, from both sides, since the source delete runs after the copy reports Ok). The `sync_data` (fdatasync) gives the "durable as each file completes" property the local-FS chunked copy already has (`transfer/chunked_copy.rs`), so a crash mid-batch leaves earlier files safe. The parent-dir fsync makes the file's directory entry durable too. Both are best-effort on error: a failure logs under `target: "write_durability"` and continues rather than failing a completed multi-GB transfer at the final fsync (matching `durability::flush_created_destinations`). Non-local backends (MTP/SMB/InMemory) need no equivalent — durability there is the device/server's concern. Pinned by `local_posix_test::test_write_from_stream_multichunk_is_durable_and_correct` (content-correctness regression guard; the fdatasync itself isn't observable from a unit test).

**Decision**: `local_posix` stays in the app crate permanently; it is NOT a candidate for a backend crate
**Why**: it looks like the smallest backend (1,056 lines across `local_posix.rs` + `local_posix/`, measured 2026-09-04) and is the hardest extraction of the four. It calls `crate::file_system::git` at ten sites (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`, and seven `is_virtual` guards), and `file_system/git/` is 6,327 lines including a `gix`-backed repo walker and a `.git` watcher: the git portal is *implemented as* `LocalPosixVolume` hooks, so extracting the backend means extracting git or inventing a git seam with exactly one implementor forever. It is also the only caller of the real-FS reader in `listing/reading.rs`, which serves the non-volume listing path too, and it's the FSEvents watcher's peer. It's the sole caller of `find_listings_for_path_on_volume` and `patch_listing_after_local_mutation`, and the latter is *definitionally* local — it `std::fs`-stats the changed entry, which no backend on a protocol can do. ❌ Don't propose this as "completing the set" once FTP and S3 are crates: the set is deliberately incomplete. Seam rationale: `crates/cmdr-fs/src/volume/host/DETAILS.md`.

**Decision**: MTP becomes `crates/cmdr-mtp`, and the three things that once made it a redesign are each answered
**Why**: the sizing insight is that a backend has two faces (`../DETAILS.md` § "Architecture"). MTP's file-ops face (`backends/mtp/`) already is the `Volume` trait and moves untouched; all the work is on its lifecycle face (`src/mtp/`), and that work is a retrofit onto the host seams done IN PLACE, with the whole suite watching, before the move itself becomes a `git mv`. Each earlier blocker has an answer: the 13 `specta::Type` + `tauri_specta::Event` derives inside the transport layer become one crate-local `MtpDeviceEvents` trait carrying a typed `MtpDeviceEvent`, with the payload structs and their derives in the app-side `apps/desktop/src-tauri/src/mtp/events.rs`, which is the same "backend says WHAT, host says what the user sees" split every other backend already lives under. The nine inline `#[cfg(test)]` gates on real behavior become `any(test, feature = "testing")`, the crate rule that exists precisely because `cfg(test)` is set only for a crate's own test target. `test_hooks`' `pub(in crate::file_system::volume)` becomes a gated `pub` under `cmdr_mtp::volume::testing`, the argued exception SMB already granted `detach_session_for_test` (`crates/cmdr-fs/src/volume/host/DETAILS.md` § "Visibility that has no cross-crate equivalent"), because the app's scan-oracle cell asserts on the APP's fresh-listing oracle and belongs app-side. And "`backends/mtp/` is a veneer over `src/mtp/`" stops being an objection once both move together. What stays app-side: the hotplug watcher (ADB's tracker twin), the macOS workaround, the registrar wiring, the tauri event payloads, and the IPC commands. The plan, its ten decisions, and its milestones: `docs/specs/mtp-crate-extraction.md`.

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


## Gotchas

**Gotcha**: `MtpReadStream` holds nothing scarce between windows, so dropping it mid-read is safe and needs no `Drop` impl
**Why**: It reads in bounded `GetPartialObject64(offset, MTP_READ_WINDOW)` windows (the windowing + offset accounting live in `mtp/connection`; see that module's DETAILS § "Bounded-window reads"). Between windows nothing is in flight — no held `FileDownload`, no pinned PTP session — so a cancel/pause/drop has nothing to abort or drain (`cancel_and_release` is the trait default no-op). If the stream is dropped WHILE a window read is in flight, mtp-rs's `TransactionScope` flags the pipe and the next op drains it under the operation lock (one ~300 ms self-heal), so an aborted window never desyncs the session. ❌ Don't re-add a `Drop`/cancel here: there's no held `FileDownload`, so mtp-rs's `ReceiveStream` unconsumed-drop panic (the reason a `Drop` cancel was once needed) can't apply.

**Gotcha**: `MtpVolume::get_metadata` is expensive: it lists the entire parent directory
**Why**: MTP has no single-file stat call. `get_metadata` lists the parent directory and searches for the entry by name. This is used by `notify_mutation` after each self-mutation (create, delete, rename) and is acceptable because those are infrequent, but avoid calling it in hot paths.

## Testing

- `in_memory_test.rs`: unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `local_posix_test.rs`: real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `mtp/` inline tests: path conversion and capability flags (no device needed)
- **No SMB or archive cell lives here.** Which side of the crate boundary one belongs on is decided by what it asserts,
  not by what it connects to (`crates/cmdr-smb/DETAILS.md` § "Which side a test lives on"), and the app-side ones then
  sit beside the app code they assert on. What they pin, the Docker fixture ports, and the soak and wedge harnesses:
  `file_system/write_operations/DETAILS.md` § "The SMB app-side suites".
`LocalPosixVolume` routes every non-forced rename through the shared atomic-exclusive primitive. This applies equally
to `/`, attached disks, Dropbox, iCloud, and other local POSIX roots registered with non-root volume IDs. Forced
renames retain normal POSIX replacement semantics because the caller explicitly authorized replacement.

## Where the shared conformance assertions live

`cmdr_fs::volume::conformance` holds the promises no backend may quietly opt out of, and each backend runs the ones it
can: `mtp_conformance_test.rs`, `local_posix_conformance_test.rs`, and each remote crate's own `conformance_test.rs`
collect them per backend, and `mtp_delete_test.rs` stays separate because the non-recursion contract is the one MTP has
to IMPLEMENT rather than inherit (`MtpDeleteScope`), with enough scaffolding to earn its own file. The roster and what
each one defends: `crates/cmdr-fs/DETAILS.md` § "The shared assertions in `volume::conformance`".

**Decision**: MTP settles a conflict scan's missing destination through `get_metadata`, not through a `NotFound` arm.
**Why**: every other backend reads a `VolumeError::NotFound` from the destination listing as "nothing clashes" and
answers an empty list. MTP can't: `resolve_path_to_handle` is cache-only, so a path nobody has browsed to fails as a
generic `IoError` ("path not in cache"), which is honest, because it means UNKNOWN rather than absent. Reading every
listing failure as absence would let a disconnected device pass for an empty folder and clear the copy to run.
`get_metadata` settles it by listing the PARENT, so only a confirmed-absent destination reads as empty and every other
failure stays the caller's to see. It costs one extra parent listing, on the error path only.

## MTP's no-clobber rename is check-then-act

`MtpVolume::rename` earns the `force == false` refusal by asking `exists(to)` and then moving. Every other backend
claims the name with a primitive the other end refuses (`renamex_np(RENAME_EXCL)`, an SFTP `create_new` placeholder or
plain `SSH_FXP_RENAME`, WebDAV's `Overwrite: F`, SMB's `ReplaceIfExists == false`), so MTP is the only one whose refusal
has a window in it. The conformance cell is `mtp_conformance_test.rs`'s
`rename_honors_the_shared_no_clobber_contract`.

**Decision**: leave the window open and say so, rather than build machinery around it. **Why**: MTP offers nothing
tighter to build on (verified on `mtp-rs` 0.32.0, source read, 2026-09-02). A same-directory rename is
`SetObjectPropValue(0x9804)` on `ObjectFileName(0xDC07)`, and a cross-directory move is `MoveObject(0x1019)` with
params `[handle, storage_id, parent]`; neither operation takes an overwrite or exclusive flag, and PTP's response-code
enum has no collision code to read one out of (`StoreFull`, `AccessDenied`, `InvalidParameter`, and no
`ObjectAlreadyExists`). The protocol also permits two siblings with the same name, so a device asked to collide doesn't
refuse: it complies, and the user ends up with a duplicate. ❌ Don't reach for a lock or a retry loop here. Cmdr isn't the only writer — the phone's own apps and MTP's
other clients mutate the same storage — so a lock this side would buy nothing and read like a guarantee.

**Gotcha: a virtual-MTP test must UNREGISTER its device, not just disconnect.** `setup_virtual_mtp_device()` registers a
device over a fresh `TempDir`; leaving it registered means the next test in the same binary connects to a stale storage
handle over a directory that's gone, and fails on its first write with a bare `GeneralError` that says nothing about the
cause. Pair every setup with `unregister_virtual_mtp_device(fixture.location_id)`.

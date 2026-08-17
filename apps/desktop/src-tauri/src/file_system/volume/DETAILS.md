# Volume abstraction details

Pull-tier docs for `file_system/volume/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in `CLAUDE.md`.

This module defines the `Volume` trait (the core abstraction for all storage backends in Cmdr) and the `VolumeManager`
registry. Per-backend implementations live in `backends/CLAUDE.md`. The friendly-error system (used by
every backend to turn raw OS errors into warm user-facing copy) lives in `friendly_error/CLAUDE.md`.

## Purpose

Every file system operation (listing, copy, rename, delete, indexing, watching) goes through a `Volume`. The trait hides
the differences between a local POSIX path, an MTP device, an in-memory test fixture, and future backends (SMB, S3,
FTP). Callers never touch the filesystem directly; they call `Volume` methods with **paths relative to the volume root**.

## Key files

- **`mod.rs`**: `Volume` trait (async: most methods return `Pin<Box<dyn Future>>`; sync: `name`, `root`, `supports_*`, `local_path`, `space_poll_interval`) plus the `VolumeReadStream` and `SequentialExtract` sub-traits. Re-exports `types::*` and `ids::*`
- **`types.rs`**: the data types the trait exchanges (`VolumeError` + its `Display`/`Error`/`From<io::Error>` impls, `SpaceInfo`, `CopyScanResult`, `BatchScanResult`, `ScanConflict`, `SourceItemInfo`, `LaneKey`, `ListingProgress`, `MutationEvent`, `SmbConnectionState`)
- **`ids.rs`** (in `cmdr-fs`): the funnel every volume ID is built through (`local_volume_id`, `path_volume_id`,
  `smb_volume_id`, `mtp_device_id`, `is_legacy_volume_id`). Which constructor a macOS mount goes through is
  `crate::volumes::ids`; the Linux twin is `volumes_linux::volume_id_for_mount`
- **`manager.rs`** (+ `manager/roots.rs`): `VolumeManager`: thread-safe `RwLock<HashMap>` registry; supports a default
  volume. Also holds the process-wide instance and its `get_volume_manager()` accessor. `roots.rs` holds the mount-root
  set each entry owns and the promotion rules over it
- **`backends/`**: Per-backend `Volume` impls (`LocalPosixVolume`, `MtpVolume`, `SmbVolume` + watcher, `InMemoryVolume`). See `backends/CLAUDE.md`.
- **`friendly_error/`**: User-facing error messages + provider detection. See `friendly_error/CLAUDE.md`.

## Architecture

```
VolumeManager (registry)
  └─ Arc<dyn Volume>  (async trait: most methods return Pin<Box<dyn Future>>)
        ├─ LocalPosixVolume   → real FS (spawn_blocking for I/O)
        ├─ MtpVolume          → direct async MTP ops
        ├─ SmbVolume          → direct async smb2 ops (direct protocol, not OS mount)
        └─ InMemoryVolume     → HashMap, test/stress use only
```

**Drive indexing does NOT go through `Volume`.** The local scanner and the FSEvents watcher are called directly by the
indexing lifecycle (`indexing::scanner::{scan_volume, scan_subtree}`, `DriveWatcher::start`), dispatched on
`VolumeKind::uses_local_scanner()`; SMB and MTP index through `network_scanner::scan_volume_via_trait`, a BFS over
`Volume::list_directory`. So `list_directory` is the only volume abstraction the indexer uses, and a new backend gets
indexed by implementing it, not by implementing an indexing hook.

Every non-forced `LocalPosixVolume::rename` is atomic-no-overwrite. macOS uses `renamex_np(RENAME_EXCL)` and Linux uses
`renameat2(RENAME_NOREPLACE)`, covering the boot volume, attached local volumes, and cloud folders registered under
their own volume IDs. A separate metadata check followed by plain `rename` is not an acceptable substitute.

## Trait capability model

Optional methods default to `Err(VolumeError::NotSupported)` or `false`, so new volume types can be added incrementally. Key capability flags:

- `can_watch_listings()`: enables the `notify`-based *listing* file watcher in `operations.rs` (separate from the drive-index watcher, which the indexing lifecycle owns). `MtpVolume` returns `false` (it has its own USB event loop).
- `supports_export()`: "this volume can stream its bytes via `open_read_stream`" (so it can act as a source in a cross-volume copy). Gates the copy dialog's "copy from this volume" UI. Local, MTP, SMB, and InMemory return `true`.
- `is_writable()`: whether the backend accepts mutations at all (create, rename, delete). Default `false`, matching the `NotSupported` default of every mutation method, so a backend opts in when it implements them. `true` for `LocalPosixVolume`, `SmbVolume`, `MtpVolume`, and `InMemoryVolume`; `ArchiveVolume` restates `false` explicitly, because writing INTO a zip is the app's managed archive-edit rewrite and never mutates through the volume. It is a claim about the BACKEND, so a read-only MOUNT of a writable backend still answers `true` — that mount's own read-only flag travels separately as the location's `isReadOnly`. This is the one capability predicate whose answer reaches the user as UI state (New folder / New file / Rename / Paste render enabled off it), so `conformance::assert_writability_matches_the_mutations_offered` pins it against real behavior in both directions.
- `capabilities()`: the published fold of the predicates above into `VolumeCapabilities` (`is_writable`, `can_export`), the struct that travels over IPC so the frontend receives capability as DATA. ❌ Never override it and never compute an answer inside it: growing the surface means adding a predicate and folding it there. Only what a consumer OUTSIDE the backend acts on belongs in the struct; the predicates that steer the operations engine stay predicates. Published onto each `LocationInfo` by `volumes::enrich_from_volume_registry` (and its Linux twin), which is also why a location with no registered volume carries `capabilities: null` and lets the frontend fall back to its per-kind defaults.
- `supports_streaming()`: enables cross-volume transfers via `open_read_stream` / `write_from_stream`. `LocalPosixVolume`, `MtpVolume`, `SmbVolume`, and `InMemoryVolume` all return `true`. This is the universal byte path for every non-APFS-clone copy. New backends just implement the two streaming methods to get cross-volume copy for free.
- `max_concurrent_ops()`: how many streaming copies the copy engine can drive in parallel against this volume. The batch copy path resolves a pair through `transfer_concurrency` (`write_operations/transfer/volume/copy.rs`), clamped to 32, and spawns that many `FuturesUnordered` tasks. It is NOT a plain `min()`: a volume answering `operations_are_local() == true` reports a CPU guard-rail rather than a transport limit, so its cap doesn't bound a remote peer. Defaults to `1` (safe for any new backend). Current values: `LocalPosixVolume` returns `available_parallelism()/2` clamped to 4..=16 (local); `SmbVolume` returns the `network.smbConcurrency` setting, default 10, range 1..=32; `MtpVolume` returns 1 (USB bulk transport is serial, and that 1 is what routes a phone to the serial driver); `InMemoryVolume` returns 32 (local).
- `operations_are_local()`: whether one operation here is a local syscall rather than a transport round trip. A claim about COST, so it is a different question from `supports_local_fs_access` (an OS-mounted SMB share is `true` there, `false` here). Default `false`, the conservative answer in both directions. `true` for `LocalPosixVolume` and `InMemoryVolume` only.
- `create_directory_all()`: reports `DirectoryCreation::{Created, AlreadyExisted}` for the LEAF. The copy driver skips its destination conflict pre-check entirely on `Created` (`transfer/DETAILS.md` § "Answering the pre-check from one listing"), so an overriding backend must answer honestly and answer `AlreadyExisted` when unsure — including when it lost a create race.
- `local_path()`: returns `Some` only for local volumes; allows `copyfile(2)` fast-path in copy operations. `SmbVolume` returns `None` so copies go through smb2 instead of the slow OS mount.
- `supports_local_fs_access()`: whether `std::fs` operations (stat, read_dir) work on this volume's paths. Default `true`. `MtpVolume` and `SmbVolume` return `false`. Used to skip the legacy synthetic entry diff path (now superseded by `notify_mutation`).
- `paths_are_os_visible()`: whether ANOTHER app can open a `file://` URL built from a path this volume hands out. Defaults to whatever `supports_local_fs_access()` says, which is right wherever the two coincide. `SmbVolume` is the one backend that splits them: it answers `false` above (its own I/O rides smb2, never `std::fs`) and `true` here, because the sneaky mount keeps the share OS-mounted and every path it yields is an ordinary `/Volumes/…` path. Consumed by the macOS drag-out path (`commands/file_system/drag.rs::locality_for_volume`) to pick the pasteboard layout: `false` means promise-only items, which only Finder accepts, so a backend that answers it wrong makes drags into browsers and mail clients silently do nothing while Finder keeps working. It is a claim about the MOUNT, not the backend kind, so it has to track the mount going away — see `note_root_mount_gone` below.
- `note_root_mount_gone()`: the registry telling a volume that its active mount root is gone and there was no live sibling to promote it to (§ "A volume ID owns a set of mount roots"). Default no-op; only `SmbVolume` overrides, latching `paths_are_os_visible()` to `false` while its smb2 session keeps browsing. A volume can't work this out for itself — nothing may probe a mount — and the failure it prevents is silent: paths that still list fine in Cmdr, and a drag out of them that does nothing.
- `notify_mutation(volume_id, parent_path, mutation)`: called after a successful mutation (create, delete, rename, and `write_from_stream`) to update the listing cache immediately. Fire-and-forget, no error propagation. See "Mutation notification" below.
- `smb_connection_state()`: returns `Some(SmbConnectionState)` for SMB volumes (green/yellow indicator in volume picker). Default `None`. Only `SmbVolume` implements it.
- `attempt_reconnect()`: tries to rebuild the volume's underlying session in place after a transient connection loss. Default `Err(NotSupported)`. Only `SmbVolume` overrides today; the Tauri command `reconnect_smb_volume` and the FE reconnect manager call this on each backoff tick. Idempotent and single-flight: concurrent callers wait on the same in-flight attempt instead of dog-piling the server.
- `reconnect_with_credentials(username, password)`: reconnect with freshly-entered credentials, replacing whatever was cached. Default `Err(NotSupported)`; `SmbVolume` persists the new password (so the next reconnect is silent) then runs `attempt_reconnect`. Invoked by the Tauri command `reconnect_smb_volume_with_credentials` behind the "Sign in" prompt shown after an auth-failure reconnect give-up.
- `on_unmount()`: lifecycle hook called before unregistration. `SmbVolume` uses it to disconnect its smb2 session. Default is no-op.
- `on_superseded()`: lifecycle hook for "a newer instance took my id, but the device is still here". Defaults to `on_unmount()`; `SmbVolume` overrides it to keep serving the holders that already have it. Contract: `backends/DETAILS.md` § "Supersede vs. unmount".
- `begin_scan_session()` / `end_scan_session()`: default-no-op async hooks the indexing lifecycle
  (`indexing/lifecycle/network_scan.rs`) calls right before and after a background `Volume`-trait scan/reconcile walk.
  Let a backend open scan-scoped resources for the duration of a walk. `SmbVolume` overrides them to open/close a pool of
  extra TCP sessions the cold walk lists across (canonical: `backends/DETAILS.md` § "SMB scan-connection pool"); the pool
  is invisible to the scanner, which keeps calling `list_directory_for_scan`. MTP keeps the default (its single USB pipe
  can't parallelize), and local volumes never reach this path.
- `connection_liveness()`: has this volume's connection been PROVEN dead, as opposed to merely slow to answer? Three-valued (`None` = no evidence either way, which is what **every backend answers today**). It exists to gate the one aggressive thing the transfer watchdog does — ending the wait on a task that has stopped moving — and a wrong `Dead` kills healthy slow transfers, so the bar is proof rather than suspicion. ❌ Never answer it from elapsed silence: a large write to a loaded spinning-disk NAS is legitimately slow and looks identical on the wire to a dead server. Telling the two apart needs a keepalive (an ECHO answered inside a window), and the pinned `smb2` has one but deliberately never reads a missed probe as death — a busy NAS drops probes — while its one sound verdict (`Error::ServerUnresponsive`) reaches the caller only after tearing the connection down, which the per-file retry already covers. What `smb2` would have to expose for `SmbVolume` to answer, and everything the answer gates: `write_operations/transfer/DETAILS.md` § "The watchdog ACTS".
- `rerooted(new_root)`: build an equivalent volume rooted somewhere else, or `None` (the default) for "leave me where
  I am". This is how the registry carries out a promotion when a volume's active mount root dies and another mount
  reaches the same filesystem; see § "A volume ID owns a set of mount roots" below. Implement it wherever the root is
  pure addressing: `LocalPosixVolume` does it in one line, and `SmbVolume` hands out another instance over its shared
  session (`backends/DETAILS.md` § "Re-rooting a share"). Declining is not a failure mode: a backend whose transport is
  anchored to the old root keeps its registration instead of being handed a root it can't serve.
- `space_poll_interval()`: recommended interval for the live disk-space poller (`space_poller.rs`). Default 2 s (local volumes). `SmbVolume` and `MtpVolume` override to 5 s. `InMemoryVolume` returns `None` (no polling). The poller uses this to tick each volume at its own cadence.
- `create_directory_errors_on_existing_dir()`: whether `create_directory` reliably returns `VolumeError::AlreadyExists` for an existing same-name dir. Default `true` (LocalPosix, SMB, InMemory all do). `MtpVolume` overrides to `false` — the MTP protocol allows same-name sibling objects and `create_folder` silently makes a duplicate, so the folder-merge walker (`write_operations/transfer/volume/strategy.rs`) pre-checks existence on MTP instead of trusting the create to error. A blindly-created duplicate would make a merge target the wrong directory.
- `listing_watch_coverage(path)`: what a live watch on this volume's cached listing for `path` actually observes, as a three-state `WatchCoverage` (`None` / `ThisMachineOnly` / `EveryWriter`). Three consumers today:
    1. `file_system::listing::caching::try_get_authoritative_listing` (the "fresh-listing oracle") — write-op pre-flight scans reuse a cached listing instead of re-reading.
    2. `write_operations::delete::scan_volume_recursive` (the oracle-aware delete walker) — same idea, per-recursion-level.
    3. The `refresh_listing` Tauri command (`commands/file_system/listing.rs`) — short-circuits the post-transfer redundant `list_directory` re-read entirely when the volume is keeping the cache fresh via `notify_mutation`. Without this, a 1k-entry MTP folder paid ~17 s + USB session collision after every transfer outcome, wedging the next user op.
  Only `EveryWriter` authorizes any of them to skip a read. Default `None`, so a new backend without a real watcher can't accidentally claim freshness.

  **Why three states and not a bool.** An OS-mounted network share (SMB, NFS, AFP, WebDAV) is served by `LocalPosixVolume`, and an FSEvents watch on it really does deliver this machine's writes through the mount — the pane updates correctly after your own copy. It delivers NOTHING for a change another client makes to the share, because FSEvents is a local-VFS notifier rather than a share notifier (verified on macOS 26.5.2 against a live `smbfs` mount, 2026-08-08; `docs/notes/silent-inertness-hunt-2026-08-08.md`). A boolean forces that case to lie in one direction or the other: `true` hands a delete walker entries that may already be wrong, `false` throws away a watch that's genuinely keeping the pane current. `ThisMachineOnly` says exactly what's true, and an exhaustive match makes every future caller pick a side.

  **Freshness contract**: `EveryWriter` is a claim about WHICH WRITERS reach us, never about latency. Every backend has a debounce or settling window between a real change and the cache reflecting it: local FS ≈ 10 ms (FSEvents coalesce), SMB 200 ms (watcher debounce; > 50 events/dir triggers a `FullRefresh`), MTP 500 ms (event debouncer plus per-device polling; many cameras emit no events at all, so on those `EveryWriter` means only "the device is reachable, and it's the only writer"). Callers must treat the result as "fresh as our most recent observation" — the same guarantee a `list_directory` call gives. The MTP and SMB answers are volume-level, not path-level: once covered, every path on that volume is oracle-eligible.

## Conflict classification fields

`scan_for_conflicts` is what powers the upfront Transfer dialog's "N folders will merge / N conflicts" classification, so each `ScanConflict` carries the type of both sides:

- `ScanConflict.source_is_directory` / `dest_is_directory`: let the FE tell a dir-vs-dir collision (a silent merge, never a conflict) from a file clash or a cross-type clash (a real conflict). The FE counts only the latter toward `totalConflictCount` and the bulk-skip set; dir-vs-dir surfaces as the "will merge" info line. The source flag comes from the caller-supplied `SourceItemInfo`; the dest flag from the dest listing entry the scan already lists.
- `SourceItemInfo.is_directory`: the caller (the conflict-scan command) knows each source's type from the `FileEntry` it already holds and passes it in, so backends copy it straight onto `ScanConflict.source_is_directory` without a per-source `is_directory` round-trip.

The sibling per-file conflict event (`write_operations::types::WriteConflictEvent`, emitted mid-operation when a deep clash needs a human) carries `source_size: Option<u64>` for the same reason: a cross-type clash can now surface on the same-volume fast path, where no pre-flight scan ran, so a folder source's size is genuinely unknown. The FE renders `(unknown)` for a `None` source size, and `size_difference` collapses to `None` when either side is unknown. (`ScanConflict.source_size` itself stays a plain `u64` — the upfront scan always has a size for the items it lists.)

Folders always merge (see `write_operations/transfer/CLAUDE.md` § "Dir-vs-dir is NEVER a conflict"), so these flags exist purely to classify, never to gate a folder behind a prompt.

## Cancel-aware variants

`list_directory_with_cancel(path, on_progress, cancel)` and
`delete_with_cancel(path, cancel)` accept an
`Option<&CancellationToken>` — the one cancellation primitive every layer of
Cmdr speaks — that backends interpret as a cooperative stop signal. Default
impls delegate to the non-cancel `list_directory` / `delete`, dropping the token
— so adding a new backend doesn't have to implement them unless its operations
are interruptible at a meaningful boundary. `list_directory_for_scan` is the
third: the index scanner's entry point, whose default is
`list_directory_with_cancel`.

- `MtpVolume` overrides all three. mtp-rs polls its own `Arc<AtomicBool>`-backed
  `CancelToken`, so `MtpCancelBridge` mirrors the token into one for the
  duration of a call, through a task parked on `cancelled()` that retires when
  the bridge drops. That bails the per-handle `GetObjectInfo` loop within one
  USB roundtrip's latency.
- `LocalPosixVolume` and `InMemoryVolume` inherit the default (ignore the
  token); local listings are effectively atomic. `SmbVolume` overrides
  `list_directory_for_scan` only, to draw from its per-scan connection pool, and
  ignores the token there — SMB cancel propagation is a follow-up.

The write-op layer hands `Some(&state.backend_cancel)` (the same token
`cancel_write_operation` cancels when intent leaves `Running`). Volumes that
ignore it are unaffected; volumes that consume it stop their wire activity, not
just the loop above.

See `apps/desktop/src-tauri/src/mtp/CLAUDE.md` § "Cancel propagation" for the
MTP-specific wiring and the rationale for "between-roundtrip" cancel vs PTP
`CancelTransaction`.

## Building a new volume

Adding a new backend (say, FTP, WebDAV, S3, or a new device protocol) is a matter of implementing the `Volume` trait and opting into the capability flags that make sense for your backend. The checklist below walks the path in the order you'd hit each concern.

Work through it top-to-bottom. Each tier depends on the previous being solid. Ship to users only after tier 3.

### Tier 1: make it listable (mandatory)

Without these, the volume can't even appear in the UI:

- [ ] Implement `name()` and `root()` (return the display name and the path everything is relative to).
- [ ] Implement `list_directory(path, on_progress)`: the core read. **Feed `on_progress` as you enumerate**, and don't rename the parameter to `_on_progress` to quiet the compiler. It drives the pane's "Loaded N files..." readout, which is all the user sees while a big folder reads; dropping it leaves them on "Opening folder..." for the whole wait, and nothing fails to say so. If your enumeration happens on a thread the callback can't reach (it's `Sync` but not `Send`, so `spawn_blocking` is out), publish counts into a shared tally and sample it from the async side: `LocalPosixVolume` is the worked example, described in `listing/DETAILS.md` § "Local listing progress".
- [ ] Implement `get_metadata(path)`: per-entry stat.
- [ ] Implement `exists(path)` and `is_directory(path)`. On backends where these would issue two round-trips, implement them in terms of `get_metadata` to share the cost.
- [ ] Implement `get_space_info()`: for the volume usage bar and pre-copy space checks. Return zeros if the backend doesn't report it.
- [ ] Register the volume via `VolumeManager::register_if_absent` (not `register`; see "Key decisions" below).
- [ ] Add unit tests using a fake/in-memory harness or real fixtures.

### Tier 2: make it writable (recommended for real-world use)

Everything below is optional per the trait (methods default to `Err(NotSupported)` or `false`), but a read-only volume is rarely useful:

- [ ] Implement `create_directory`, `create_file`, `delete`, `rename`.
- [ ] After each successful mutation, call `self.notify_mutation(&volume_id, parent_path, MutationEvent::...)` so the listing cache updates immediately. Override `notify_mutation` on the trait if your backend can answer `get_metadata` faster than `std::fs::metadata` would (MTP and SMB do this).
- [ ] Return `supports_streaming() = true` and implement `open_read_stream` + `write_from_stream`. These are the byte path for every cross-volume copy. The Copy dialog uses them for "this volume ↔ anywhere" transfers.
- [ ] Return `supports_export() = true` if the volume should appear as a copy source in the UI.
- [ ] Implement `scan_for_copy` (count + bytes) and `scan_for_conflicts` (destination collision detection). These feed the Copy dialog's pre-flight. `scan_for_conflicts` takes a `SourceItemInfo` per source and emits a `ScanConflict` per collision; see "Conflict classification fields" above for the `is_directory` flags it must populate.
- [ ] Map your backend's errors through a `map_*_error` function that returns `VolumeError`. Connection-loss errors should trigger a state transition (see `SmbVolume::handle_smb_result` as a reference) so subsequent calls fail fast.
- [ ] **No full-file buffering in per-file transfer paths.** Don't drain the incoming `VolumeReadStream` into a `Vec<u8>` before writing, and don't collect the remote file into a `Vec<u8>` before yielding. An 8 GB copy would allocate 8 GB of RAM. See the "Streaming requirement" section on each trait method's doc comment: `open_read_stream`, `write_from_stream`.

### Tier 3: integrate with the wider app (optional, but mostly expected)

- [ ] If the backend's paths are real local-filesystem paths a `notify` watch can be pointed at, set `can_watch_listings() = true`. If your change-notification channel is your OWN (SMB's CHANGE_NOTIFY, MTP's USB event loop, the archive content watch), leave it `false` and drive `notify_directory_changed` from that channel instead. ❌ Don't answer `true` because the volume happens to sit under an OS mount: an FSEvents watch on a network mount can't see other clients (see `listing_watch_coverage` above), so that trades a real channel for a blind one.
- [ ] Implement `listing_watch_coverage(path)` if your backend can cheaply answer "what does a live watch on this listing see?". Answering `EveryWriter` opts the volume into the fresh-listing oracle: write-op pre-flight scans (copy/move scan preview) reuse cached entries from `LISTING_CACHE` instead of paying a `list_directory` round trip. Default `None` is the safe choice — without a real watcher, the cache may be arbitrarily stale. Pick the variant matching what your notification channel is WIRED to, not what you wish it covered: under-claiming costs a re-read, over-claiming hands stale entries to a delete walker. Path-level (LocalPosixVolume) is the most accurate signal; volume-level (MTP "device connected", SMB "Direct + watcher running") is fine when the channel is volume-wide. Be honest about the per-backend debounce window in the doc comment; see `try_get_authoritative_listing` for the freshness contract.
- [ ] If `std::fs` operations work on the volume's paths (you're a local FS with extra flavor), leave `supports_local_fs_access()` at the default `true`. Otherwise override to `false` so the legacy synthetic-diff path is skipped.
- [ ] Answer `paths_are_os_visible()` separately if your backend keeps an OS mount alive alongside its own transport (as SMB does). It inherits `supports_local_fs_access()` otherwise, so a backend whose paths ARE openable by other apps but which reads them over its own protocol has to say so, or every drag out of its pane degrades to Finder-only file promises. Then handle `note_root_mount_gone()` too: your own transport survives the mount going away, and that's exactly when a hardcoded `true` starts publishing URLs that open nowhere.
- [ ] If `std::fs::copy` can target this volume's paths directly, return `Some(root)` from `local_path()`. The copy path will prefer `copyfile(3)` / `copy_file_range(2)` for same-device copies. Otherwise return `None` (the default).
- [ ] Override `lane_key()` to a STABLE identifier for the shared physical resource a transfer contends on (MTP device serial, SMB server+share, local mount root). The operation manager serializes write ops that share a lane (budget 1) and parallelizes disjoint ones; the default returns the volume root, which is right for an independent mount but would over-serialize if multiple `Volume` instances actually share one device/pipe. See `../write_operations/DETAILS.md` § "Operation manager".
- [ ] Override `space_poll_interval()` to whatever polling cadence your backend can afford (local 2 s, network 5 s, none = don't poll).
- [ ] If your backend holds a session that can drop (FTP, S3, SFTP, anything network-bound), emit `volume-connection-changed` (`network::VolumeConnectionChanged` + the `VolumeConnection` enum) on every transition and you inherit the whole frontend recovery story for free: the unreachable banner, the per-volume backoff cycle, and the "Sign in" prompt when saved credentials go stale. ❌ Don't add a backend-named connection event alongside it; the channel is backend-neutral on purpose. Map your internal state machine onto the wire enum the way `From<ConnectionState> for VolumeConnection` does in `backends/smb/state.rs`, and emit `NeedsCredentials` straight from your reconnect give-up path (no backend rests in it). Flow: `backends/DETAILS.md` § "SMB live-reconnect lifecycle".
- [ ] If the volume needs async teardown (session close, handle drop), implement `on_unmount`. The default is a no-op.
- [ ] If tearing that state down mid-flight would break a caller still holding an `Arc`, also implement `on_superseded` (it defaults to `on_unmount`).
- [ ] Add a branch to `detect_provider` / `provider_suggestion` in `friendly_error/provider.rs` (see `friendly_error/CLAUDE.md`) if there's a recognizable path shape or fs type worth calling out in friendly errors.
- [ ] Add a capability-matrix row below and update the `docs/architecture.md` volume line if the shape changes meaningfully.

### Tier 4: E2E and friendly-error polish

- [ ] **Call every `cmdr_fs::volume::conformance` assertion your backend can run.** These are the promises that only a comment would otherwise hold, each one load-bearing for data safety: `delete` never recurses, `rename(force = false)` refuses an existing destination, `create_file` refuses rather than truncates, `create_directory_all` reports a pre-existing leaf as `AlreadyExisted`. Every existing backend calls the ones it implements, and skipping yours is how a backend claims a contract by implementing the trait and breaks it where nobody looks (MTP's `delete` did exactly that, for years). What each one is for: `crates/cmdr-fs/DETAILS.md` § "The shared assertions in `volume::conformance`".
- [ ] Add integration tests (real fixtures if possible; see the Docker SMB containers for inspiration).
- [ ] Verify your backend's common failure modes classify well: each one should reach a `ListingErrorReason` that words up usefully, not the generic I/O fallback. The Rust side ships no prose, so check the reason in `friendly_error/tests.rs` and the rendered copy through the debug window's error-pane preview. `docs/guides/error-handling.md`.
- [ ] Stress-test concurrent reads and writes (the `stress_tests_*` modules in indexing are the reference pattern).

## Capability matrix

At-a-glance view of which capabilities each current volume opts into. Use this when picking a reference implementation for your new volume.

| Capability                  | Local                | MTP                     | SMB                       | InMemory           | Archive                  |
| --------------------------- | -------------------- | ----------------------- | ------------------------- | ------------------ | ------------------------ |
| `list_directory` / metadata | ✅                   | ✅                      | ✅                        | ✅                 | ✅                       |
| Mutations (create/delete/rename) | ✅              | ✅                      | ✅                        | ✅                 | ❌ read-only (mutation planned) |
| `supports_export`           | ✅                   | ✅                      | ✅                        | ✅                 | ✅                       |
| `supports_streaming`        | ✅                   | ✅                      | ✅                        | ✅                 | ✅                       |
| `open_read_stream`          | ✅ spawn_blocking    | ✅ owned download       | ✅ channel-backed         | ✅ in-memory       | ✅ core `ArchiveEntryReader` |
| `write_from_stream`         | ✅ spawn_blocking    | ✅ streaming            | ✅ streaming              | ✅ in-memory       | ❌ (mutation planned)    |
| `can_watch_listings`         | ✅ FSEvents/inotify  | ❌ (own USB watcher)    | ❌ (own smb2 CHANGE_NOTIFY watcher) | ❌       | ❌ (own content watch on the `.zip`) |
| `listing_watch_coverage`        | path-level (WATCHER_MANAGER); `ThisMachineOnly` on a network mount | volume-level `EveryWriter` (device connected) | volume-level `EveryWriter` (watcher + Direct) | `None` (default) | `EveryWriter` while the content watch lives |
| `supports_local_fs_access`  | ✅ (default)         | ❌                      | ❌                        | ❌                 | ❌ (inner paths)         |
| `paths_are_os_visible`      | ✅ (inherited)       | ❌ (inherited)          | ✅ OVERRIDE while its mount lives | ❌ (inherited)    | ❌ (inherited)           |
| `local_path`                | ✅ `Some(root)`      | `None`                  | `None`                    | `None`             | `None`                   |
| `notify_mutation`           | default (std::fs)    | ✅ MTP `get_metadata`   | ✅ smb2 `get_metadata`    | ✅ in-memory       | n/a (read-only)          |
| `create_directory_errors_on_existing_dir` | ✅ (default) | ❌ (protocol allows dup names) | ✅ (default) | ✅ (default) | n/a (read-only)  |
| `scanner` / `watcher` (indexing) | ✅ / ✅          | ❌                      | ❌                        | ❌                 | ❌                       |
| `rerooted`                  | ✅ new instance      | `None` (device-anchored) | ✅ new instance, shared session | `None` (default) | `None` (inner paths)     |
| `on_unmount`                | default              | default                 | ✅ drops smb2 session     | default            | default                  |
| `on_superseded`             | default              | default                 | ✅ retires id, keeps session | default         | default                  |
| `smb_connection_state`      | `None`               | `None`                  | ✅                        | `None`             | `None`                   |
| `space_poll_interval`       | 2 s (default)        | 5 s                     | 5 s                       | `None`             | `None`                   |
| `lane_key` / `get_space_info` | mount root / statvfs+NSURL | device serial / device | server+share / smb2 | root or override / configured | **parent's** / **parent's** |
| `max_concurrent_ops`        | 4..=16 (core-based)  | 1 (USB bulk serial)     | `network.smbConcurrency`  | 32                 | 1 (initial cap)          |
| `operations_are_local`      | ✅ `true`            | `false`                 | `false`                   | ✅ `true`          | `false`                  |

Legend: ✅ = implemented, ❌ = opted out (default or explicitly), ⚠️ = implemented but suboptimal (memory-heavy or otherwise worth revisiting).

`ArchiveVolume` is the read-only zip backend (`crates/cmdr-archive/CLAUDE.md`); its `lane_key` and
`get_space_info` uniquely delegate to a **parent** volume (the volume storing the `.zip`), so archive work shares the
device's lane and the space check sees the parent drive's real free space.

When adding a new volume, add a column for it and fill in each row. The matrix doubles as a self-review: gaps will stare back at you.

## Streaming patterns

Reads and writes have different shapes because the consumer relationship is different:

- **Reads** return a `VolumeReadStream` that an external caller polls. The download handle has to live past the function call and cross async contexts. That's where the lifetime/ownership gymnastics below come from.
- **Writes** consume a stream (or a local file) inside the method itself. The chunk loop is the consumer, so there's nothing to hand off. For backends with a `'static` writer (smb2 0.9's owned `FileWriter`, mtp-rs's `upload_stream`), drive the writer directly on a cloned session handle — no lock held across I/O. For backends whose writer borrows from the session, hold the session lock for the chunk loop's duration. `SmbVolume::write_from_stream` is the reference implementation: clone the session once, open the smb2 `FileWriter` on the clone, loop `write_chunk`, call `finish()` on success or `abort()` on cancel. No task spawn, no channel, no self-referential struct, no client mutex held while WRITEs are in flight.

The rest of this section is about **read-side** lifetime handling. Which pattern to pick depends on whether your protocol SDK's download handle is `'static` or borrowed.

### Pattern A: cached session + bounded windows (use when the SDK exposes a stateless partial-read primitive)

If the SDK can read an arbitrary byte range on demand (no held streaming handle), cache the resolved session in your stream struct and issue one bounded read per `next_chunk`. Nothing is held between reads, so there's no lifetime gymnastics, no task, no channel, and no `Drop` to write. **Example: `MtpReadStream`** (`backends/mtp/mod.rs`), which loops mtp-rs's `WindowedDownload::next_window` (one `GetPartialObject64` each).

```rust
struct MtpReadStream {
    session: MtpReadSession,  // caches the mtp-rs WindowedDownload (owns size/offset/EOF)
    device_id: String,
}
```

`next_chunk()` delegates to the connection layer's `read_next_window`, which takes the per-device lock for one window. The window bookkeeping (size, offset, clamp-to-remaining, EOF, advance-by-returned-length, the 0-byte-before-EOF stall guard) lives in mtp-rs's `WindowedDownload`, not here. `cancel_and_release` is the trait default no-op (nothing held); a mid-window drop self-heals via mtp-rs's `TransactionScope`. The lock invariant (every `next_window` runs under Cmdr's device lock) lives in `mtp/connection` (DETAILS § "Bounded-window reads").

### Pattern B: channel-backed stream (use when the SDK's download type borrows `&mut Connection`)

If the SDK's download handle holds a borrow against the session (like `smb2::FileDownload<'a>` borrowing `&'a mut Connection`), you can't stuff it into a `'static` struct. Use a background producer task that holds an `OwnedMutexGuard` over the session, drives the download, and feeds chunks through a bounded mpsc channel. **Example: `SmbReadStream`** (`backends/smb/streams.rs` → `open_smb_download_stream`).

Key building blocks:
- `Arc<tokio::sync::Mutex<Session>>` so the task can call `lock_owned()` and own the guard until done.
- Bounded mpsc channel (capacity ~4) for backpressure. Peak memory is `capacity × chunk_size`, a few MB regardless of file size.
- Oneshot channel for the total size (reported before the first chunk so the consumer sees the correct `total_size()` synchronously).
- Oneshot channel for cancellation. `Drop` on the stream sends the signal, producer breaks its loop and releases the guard.
- If the session state (connection health) can transition on protocol errors, wrap the state atomic in `Arc<AtomicU8>` so the task can update it from outside `&self` context.

### Anti-pattern: pre-buffering the whole file

Don't slurp the whole file into a `Vec<u8>` before yielding chunks. For an 8 GB file that means an 8 GB allocation. If the consumer API is stream-shaped, the producer should stream too.

The same rule applies to write paths: `write_from_stream` must drive the backend's chunk-by-chunk writer (for example, smb2's `FileWriter`) rather than slurping the source into a `Vec<u8>` first. See the "Streaming requirement" section on each Volume trait method's doc comment.

## Path handling gotchas

### The two dialects, and who reconciles them

The UI speaks two path dialects, and a leading `/` doesn't tell them apart:

- A **pane** sends the absolute path it displays (`/Volumes/naspi/photos`).
- The **transfer dialog's destination box** is VOLUME-RELATIVE (`/photos`), because the volume is a separate dropdown
  beside it. Same for the conflict scan and the exists-probe that feed that box.

`cmdr_fs::volume::root_anchored(root, path)` folds both into the absolute, root-anchored form: every spelling of the
root (empty, `.`, `/`) is the root itself; a path already under the root (matched by whole COMPONENTS, so
`/Volumes/naspi-1` is not under `/Volumes/naspi`) passes through; anything else is volume-relative and hangs off the
root. It's idempotent, so a call site anchors without knowing which dialect it holds, and a scheme-shaped root
(`mtp://device/storage`) works the same.

**The CALLER anchors; the backend stays strict.** `SmbVolume::to_smb_path` answers `NotFound` for an absolute path
outside its mount rather than guessing at the dialect, because guessing addressed real files at the wrong place. That
strictness is why the anchoring has to happen: an unanchored `/photos` reached the SMB backend as an out-of-mount
absolute path and failed a move into a share subfolder in 2 ms, before any I/O, reporting the DESTINATION as a missing
source (`SourceNotFound`, since `map_volume_error` maps every `VolumeError::NotFound` to it). Anchoring at the IPC
boundary is `commands/file_system/volume_copy.rs::resolve_dest_path`, which every copy / move / compress / scan
destination goes through, plus `path_exists`.

- **`LocalPosixVolume::resolve`** is `root_anchored` (that shared rule is what makes an O_EXCL reservation in
  `transfer/volume/conflict.rs` land where `write_from_stream` will later write, and a `note_pending_write_for_cmdr`
  registration match the path the writer hits).
- **`MtpVolume::to_mtp_path`**: strips the `mtp://{device}/{storage}/` URL prefix and leading slashes, returning the
  bare relative path the MTP library expects. Lenient about a bare `/DCIM`, which is why MTP never showed the failure
  SMB did.
- **`InMemoryVolume::normalize`**: always resolves to an absolute path anchored at `/`.

## Integration status

`LocalPosixVolume` is wired into the indexing subsystem. `VolumeManager` is actively used.

## Git delegation hooks

`LocalPosixVolume` delegates three read-side methods to the git module after `resolve()`:

- `list_directory` calls `git::try_route_listing(resolved_path)`. Returns the virtual listing for `.git/`, `.git/branches/...`, `.git/tags/...`, `.git/commits/...`, `.git/stash/...`, `.git/worktrees/...`, or `.git/submodules/...`. Real `.git/*` entries (HEAD, config, hooks/, objects/, refs/, etc.) get `None` from the hook and fall through to real-FS listing. The portal root (`.git/`) returns a mixed listing: real entries plus the six virtual categories.
- `get_metadata` calls `git::try_route_metadata(resolved_path)`.
- `open_read_stream` calls `git::try_open_blob_stream(resolved_path)`. Returns a `GitBlobReadStream` for blobs inside refs; real `.git/*` files fall through to the LocalPosixVolume real-FS reader.

All mutation methods (`create_file`, `create_directory`, `delete`, `rename`, `write_from_stream`) detect virtual paths via `git::is_virtual(path)` and return `VolumeError::NotSupported` immediately. `notify_mutation` early-returns for virtual paths since git mutations happen out-of-band (the user runs `git` in a terminal); state changes flow through the `.git`-watcher pipeline (`file_system/git/watcher.rs`) instead.

The hook order is fixed: `resolve()` first (normalizes the path), then `try_route_*`. This lets the user open `.git` from any volume-rooted path and get the portal regardless of whether the frontend sent an absolute or relative path.

## Eject

`eject.rs` (macOS+Linux) owns volume teardown across every kind, so it lives next to the `VolumeManager` and `Volume`
trait it dispatches over. `commands::eject::eject_volume` is a thin delegate; the pipeline is:

1. **Busy gate**: refuse (`EjectError::Busy`) if a write op is touching the volume (`file_system::busy_volume_ids`), so
   a transfer can't be truncated. The picker already disables Eject for busy volumes; this defends against a race or an
   MCP/automation caller.
2. **Classify**: MTP (id shaped `{device_id}:{storage_id}`, confirmed against the live device list) → disconnect the
   session; a registered `SmbVolume` (`smb_connection_state().is_some()`) → `diskutil unmount` (FSEvents drives smb2
   teardown via `on_unmount`); otherwise NSURL/`/sys/block` ejectability → `diskutil eject` (powers down USB, detaches
   DMGs). The pure `decide_eject_action` makes this choice and is unit-tested without touching the FS.
3. **Execute**: MTP disconnect, or a `diskutil`/`umount` subprocess under a 15 s timeout.

The MCP `eject` tool wraps `eject::eject` directly (not the command), surfacing `Busy` / non-ejectable as honest tool errors; see `mcp/DETAILS.md`.

Errors are the typed `EjectError` (`Busy`, `VolumeNotFound`, `Decision`, `Failed`, `TimedOut`); the command maps
`TimedOut` to `IpcError::timeout()` and the rest to `IpcError::from_err`, so the wire error keeps the timeout flag
without string-matching. Returns once teardown is *initiated* — `volume-unmounted` / `mtp-device-disconnected` fire
shortly after and panes rooted at the volume redirect to root. `disconnect_smb_volume` (in `commands::network`) is the
same `diskutil unmount` pattern for the explicit SMB-disconnect path.

**Why the drive indexer stops before `diskutil unmount` runs.** Unmounting a local volume — especially FAT/exFAT via
macOS's FSKit `msdos` service — while a process still holds it open (an FSEvents watcher or open file handle) can wedge
the FSKit service mid-unmount, which on macOS 26 escalated to a WindowServer watchdog kernel panic (observed
2026-07-15). So `stop_index_then_unmount` above stops a `LocalExternal` volume's index — dropping its FSEvents watcher
and closing its SQLite handles — BEFORE the `diskutil` subprocess runs. The post-unmount `NSWorkspaceDidUnmountNotification`
hook is only cleanup (the volume's already gone), not wedge-prevention. See `indexing/DETAILS.md` § "Unmount/eject
lifecycle for a LocalExternal index (the wedge-safe ordering)" for the full incident writeup and the ordering guarantee.

## Key decisions

**Decision**: Trait with optional methods defaulting to `NotSupported`/`false`
**Why**: New volume types (SMB, S3, FTP) will have vastly different capability sets. Forcing every implementor to stub out every method would be noisy and error-prone. Defaults let new backends start with just `list_directory` + `get_metadata` and opt in to capabilities incrementally. The alternative (a capabilities bitfield) would require runtime checks everywhere and couldn't express return-type differences.

**Decision**: drive indexing has no hook on `Volume`
**Why**: the indexer reaches a volume in exactly two ways, and neither wants a per-volume plugin object. The local disk is scanned and watched by concrete calls from the lifecycle layer, chosen by volume KIND; every other transport is walked through `Volume::list_directory`. A `scanner()` / `watcher()` pair on the trait was written for "future backends", and the backends that arrived (SMB, MTP) chose the BFS shape instead, so it sat uncalled. Don't re-add it: an abstraction with one implementor and no caller costs a real dependency (`file_system` → `indexing`) to buy nothing.

**Decision**: the singleton lives in `manager.rs`, its bootstrap in the `file_system` facade
**Why**: `get_volume_manager()` is what nearly every subsystem reaches for, so putting it in `file_system/mod.rs` (which re-exports `write_operations::*` and the backends downward) welded 17 modules into one cycle: everything below reached up for the accessor while the facade reached down to re-export. Beside the type it adds no edge at all, `manager.rs`'s only crate-internal import being `super::Volume`, and a per-backend crate (FTP, S3, SFTP) can reach the registry without importing a module that knows every backend. `init_volume_manager` / `register_discovered_volumes` stay in the facade, where knowing every backend is the point. ❌ Don't add a `pub use` shim in `file_system/mod.rs`: it re-welds the cycle the moment someone imports through it.

**Decision**: `VolumeManager` uses `RwLock<HashMap>` (not `DashMap` or `Mutex`)
**Why**: Volume registration/unregistration is rare (mount/unmount events); reads are frequent (every file operation resolves a volume). `RwLock` gives concurrent read access without pulling in an extra dependency. `DashMap` would work but is heavier than needed for a registry that rarely exceeds ~10 entries.

**Decision**: `VolumeManager::register_if_absent` for watcher registrations
**Why**: When the mount flow pre-registers an `SmbVolume`, the FSEvents watcher would overwrite it with a `LocalPosixVolume` via `register`. `register_if_absent` is a no-op if a volume is already registered, preserving the `SmbVolume`. The existing `register` (overwrite) is kept for explicit replacement (like SmbVolume replacing itself on reconnect).

### A volume ID owns a set of mount roots

**Decision**: a registry entry (`manager/roots.rs::Registration`) is the volume plus the SET of mount roots known to
carry its ID, exactly one of them ACTIVE (the one `volume.root()` returns). `remove_root` and `mark_root_stale` move the
ID between them; `unregister` drops the whole entry.

**Why**: one filesystem can be reached through several mount points and they all derive one volume ID (an SMB share keys
on `(server, port, share)`, a local disk on its filesystem UUID). Binding the ID to one root chosen purely by path shape
meant nothing re-resolved when that root went away: ejecting `/Volumes/naspi` while the same share was still mounted at
`/Volumes/naspi-1` unregistered the volume outright (the unmount path looks a gone mount up by root), so the share was
gone from Cmdr until a restart — discovery only runs at launch. The nastier shape is a NAS dropping off the network:
macOS leaves the original mount wedged and lands the reconnect at the suffixed path, both enumerate, and the
shortest-path rule picks the corpse on every launch while Finder works fine.

The rules over the set:

- **Ranking** (`MountRoot::rank`): liveness first, then shortest path, then lexicographic. The path half is the original
  dedupe rule and still decides between equally-live roots, which is what keeps a saved `/Volumes/naspi/…` path
  restoring correctly. What changed is its RANK: path shape is a guess about identity, an errno is evidence about
  health, so a proven-stale short root loses to a live long one.
- **Recording**: a mount event for an already-registered ID at a new root keeps the incumbent ACTIVE (see the next
  decision) and records the new root as a fallback. `find_by_root` therefore matches ANY known root, not just the
  active one — a sibling the lookup can't see is a sibling nothing can promote to. Callers that need "is this the
  active root?" compare `volume.root()`; `handle_volume_will_unmount` does, so losing a spare mount doesn't stop a
  healthy volume's index.
- **Promotion**: carried out through `Volume::rerooted`. On a backend that declines, the entry stays where it is
  (`RootRemoval::ActiveRootStranded`) rather than being unregistered, because a backend can decline precisely when its
  transport doesn't ride the OS mount, and then it keeps serving. The two backends that can be doubly mounted
  (`LocalPosixVolume`, `SmbVolume`) both re-root, so nothing takes that arm today.
- **Two triggers, no probe.** The unmount watcher calls `remove_root`; a failed operation calls
  `volume::note_root_failure`, which marks the root stale on a mount-is-gone errno (`ENOTCONN`, `ETIMEDOUT`,
  `EHOSTDOWN`, `EHOSTUNREACH`, `ENETDOWN`, `ENETUNREACH`, `ESTALE`; typed errno, never message text) and promotes. ❌
  Nothing may PROBE a root for liveness: an NSURL/`statfs` round trip on a wedged network mount blocks 30–120 s and
  froze the app at launch (`volumes/DETAILS.md` § "Hung mounts"). Evidence arrives as a failure, so a sibling that turns
  out to be dead too simply proves it on its own next failure. Promotion emits `volumes-changed` so the switcher and the
  panes stop pointing at a root that's no longer active.

**What a promotion does NOT do**: it never calls `on_unmount` and never stops an index — the filesystem is still there,
just addressed differently. An index instance keeps the mount root it captured at start, which is correct for the case
this exists for (double mounts are network shares, and their indexes are `IndexVolumeKind::Smb`, torn down through
their own path) and would need re-pointing if a `LocalExternal` disk ever showed up at two mount points.

**Decision**: `register` replaces only at the SAME root; an identity conflict keeps the incumbent
**Why**: replacing the volume at one root is routine (that's the SMB upgrade: an OS-mounted `LocalPosixVolume` becomes a direct `SmbVolume` at `/Volumes/naspi`, and a live transfer holding an `Arc` keeps working through it). Two DIFFERENT roots claiming one ID is not routine, and letting the last writer win made registration ORDER decide where the volume was rooted. A share mounted at both `/Volumes/naspi` and `/Volumes/naspi-1` derives one ID from both mounts, so the registry ended up rooted at `/Volumes/naspi-1` and a pane restoring a saved `/Volumes/naspi/…` path failed its listing. Keeping the incumbent makes the outcome deterministic without pretending the ambiguity is resolved: `report_identity_conflict` still logs it, because the honest answers (a cloned volume, a double mount) both deserve a human's attention. Discovery collapses double mounts before they reach here (`volumes/DETAILS.md` § "One volume ID publishes one mount root"); this is defense in depth, not the only guard. `is_identity_conflict` (root inequality) is what tells the two cases apart. Restoring a remembered registration in a test goes through `force_register`, which skips the guard, since putting back the previous value has to be unconditional.

**Decision**: `Volume` trait is async (methods return `Pin<Box<dyn Future>>`)
**Why**: MTP and SMB operations are inherently async (USB bulk transfers, network I/O). The previous sync trait required `block_on` bridges that risked nested-runtime panics in cross-volume streaming. The async trait lets MTP and SMB call their async backends directly. `LocalPosixVolume` wraps its blocking I/O in `spawn_blocking`. Sync-only methods (`name()`, `root()`, `supports_*()`, capability flags) remain non-async.

**Decision**: `VolumeError` stores `String` messages, not the original `std::io::Error`
**Why**: `std::io::Error` is not `Clone`, but `VolumeError` needs to be `Clone` for ergonomic error propagation across thread boundaries and for serialization to the frontend. Storing the formatted message loses the original error type but keeps the information that matters for user-facing error messages. The `IoError` variant also carries `raw_os_error: Option<i32>` so the friendly error mapper can match on platform-specific errno codes.

**Decision**: `LocalPosixVolume` uses `symlink_metadata` for `exists()` instead of `Path::exists()`
**Why**: `Path::exists()` follows symlinks. A dangling symlink returns `false`, which would make the volume claim a file doesn't exist when it visibly does in a directory listing. `symlink_metadata` detects the symlink itself, matching what the user sees.

**Decision**: `notify_mutation` lives on the Volume trait, not in Tauri commands
**Why**: Every mutation method (`create_file`, `create_directory`, `delete`, `rename`) knows what changed. Adding the notification call at the end of each method keeps it colocated with the mutation. The alternative (notification calls in every Tauri command) is fragile, easy to miss a call site.

### Recursive destination create

**Decision**: `Volume::create_directory_all` is a trait DEFAULT (mkdir -p), not a per-backend method
**Why**: The volume-aware transfer pipelines (`copy_volumes_with_progress` / `move_volumes_with_progress` / `move_within_same_volume_with_progress`) need to auto-create a missing destination folder on EVERY backend, matching the local-FS `ensure_destination_dir`. A default method built on the existing `exists()` + `create_directory()` primitives gives every backend (local, SMB, MTP, in-memory) the behavior for free, with no `smb2`/`mtp-rs` changes. The default walks `dest`'s ancestors leaf→root until one already `exists()`, then creates the missing ones shallowest-first. Probing existence per component before creating is what makes it safe on backends whose `create_directory` can't signal a collision (`MtpVolume`, `create_directory_errors_on_existing_dir() == false`, which would otherwise make a duplicate same-name sibling): the helper never calls `create_directory` on a level it already saw exist. An `AlreadyExists` from `create_directory` (a concurrent op won the race) is also treated as success, so re-creating an existing ancestor is a no-op. The leaf-first walk keeps the network/IPC round-trips minimal — when only the leaf is new, it's one `exists()` plus one `create_directory`. Backends override only if they gain a cheaper native recursive mkdir; SMB and MTP don't, so the per-component loop is correct there. Wired into the transfer gate AFTER the dest-inside-source guard (same order as local), so a copy can't create a folder inside its own source. Covered by `inmemory_test.rs` (the trait default, idempotency, partial-tree, typed-failure, and MTP-semantics no-duplicate cases), the cross/same-volume in-memory transfer tests, and the Docker SMB `smb_integration_copy_creates_missing_nested_dest`. MTP recursive-create rides the shared default + the `errors_on_existing` pre-check path; it lacks a device test (no mockable MTP harness).

**Decision**: `Volume::scan_for_copy_batch` returns `BatchScanResult { aggregate, per_path }`
**Why**: The copy engine needs per-source type+size hints (`is_directory`, `total_bytes`) for its `source_hints` map, which seeds conflict detection and feeds the SMB compound fast-path's size hint. Returning both at once (one trait call, one round-trip per backend) avoids the N separate `scan_for_copy` calls that an aggregate-only batch API would force. Scan-preview callers that only want the aggregate just read `.aggregate`. `LocalPosixVolume` and `InMemoryVolume` inherit the default (serial per-path loop, cheap); `MtpVolume` preserves its "group by parent dir" batch; `SmbVolume` overrides with the pipelined stat path. See `backends/CLAUDE.md` for the per-backend overrides.

**Decision**: All cross-volume copy flows through `open_read_stream` / `write_from_stream`
**Why**: The three plausible copy paths (local↔local, local↔volume, volume↔volume) all reduce to "open a reader, pipe to a writer." The APFS clonefile fast path is the only one with a real capability difference. Routing the other two through a single streaming path means new backends (S3, WebDAV, FTP) implement two methods instead of four, concurrency lives in one place (`volume/copy.rs`), and features like resume / checksum / progress benefit every direction at once. Don't reintroduce `export_to_local` / `import_from_local`. See `docs/notes/phase4-volume-copy-unification.md`.

**Decision**: `Volume::list_directory` / `scan_for_copy_batch_with_progress` callbacks take a `ListingProgress { files, dirs, bytes }` struct (not `Fn(usize)` — files-only).
**Why**: A files-only count makes MTP and Direct SMB scan previews show "0 bytes / N files / 0 dirs" climbing through the scan, because `run_volume_scan_preview` has nothing else to forward to the mid-stream `scan-preview-progress` event. The struct lets each backend track running file count, dir count, and byte total as it enumerates entries (MTP per-handle in `mtp/connection/directory_ops.rs`, SMB in a single tally pass after `list_directory_impl`, the default trait impl in `scan_for_copy_batch_with_progress`). Self-documenting field semantics; room to grow (symlinks, special files). Streaming-listing UI callers (`commands/file_system/listing.rs`) read `progress.entries()` (= `files + dirs`) which preserves their "Loaded N entries…" display. The baseline-shift logic in `run_oracle_aware_batch_scan` shifts files / dirs / bytes together so cross-group accumulation stays cumulative. Pinned by `scan_preview_listing_progress_tests`.

**Decision**: Progress callbacks use `&dyn Fn(u64, u64) -> ControlFlow<()>`, not `FnMut`
**Why**: The Volume trait is object-safe (`dyn Volume`), so callbacks must be `Fn` (not `FnMut`). Callers use `AtomicU64` for byte counters and `Cell<Instant>` for timestamps to mutate state inside a `Fn` closure. This avoids needing `RefCell` or `Mutex` in the hot path.

**Decision**: `on_unmount()` trait method instead of `Any` downcasting
**Why**: Avoids runtime type checking, extensible for future volume types (S3, FTP might also need cleanup), consistent with the trait's design of optional methods with default no-ops.

## Gotchas

**Gotcha**: `LocalPosixVolume::resolve` has a three-way branch for absolute paths
**Why**: The frontend sometimes sends full absolute paths (like `/Users/alice/Documents`), not paths relative to the volume root. If the volume root is `/Users/alice/Dropbox`, the resolve logic must detect whether the absolute path is already inside the root (pass through), whether the root is `/` (pass through), or neither (strip leading `/` and join). Getting this wrong silently serves the wrong directory.

**Gotcha**: `write_from_stream` is a mutation; call `notify_mutation` on success on backends with unreliable out-of-band notifications
**Why**: `write_from_stream` originally relied on the SMB CHANGE_NOTIFY watcher / MTP USB event loop to patch `LISTING_CACHE` after a cross-volume copy. Both are lossy under load: the smb2 watcher keeps one outstanding `CHANGE_NOTIFY` request at a time, and Samba drops events that arrive between consecutive responses (real reproduction: 9 files copied, 4 events delivered, destination pane showed 4 files until the user navigated away and back — files written fine, only the cache was stale). Many MTP devices emit no self-mutation events at all. The other mutation methods (`create_file`, `create_directory`, `delete`, `rename`) already call `self.notify_mutation(...)` after success; `write_from_stream` must too. `LocalPosixVolume` is the exception: FSEvents is reliable, so local mutations don't need the extra patch. The "After each successful mutation, call `self.notify_mutation(...)`" rule in the Tier 2 checklist includes `write_from_stream`.

**Gotcha**: On macOS, never use `statvfs` alone for disk space. Use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks and ignores purgeable space (APFS snapshots, iCloud caches), which can be tens of GB. This causes inconsistent numbers between the status bar (NSURL API) and copy validation (`statvfs`), and prematurely blocks copies that would succeed. `get_space_info_for_path` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

## Testing

- **E2E error injection**: The `Volume` trait has an `inject_error(&self, errno: i32)` method behind the `playwright-e2e` feature flag. `LocalPosixVolume` and `InMemoryVolume` implement it. The next `list_directory` call returns the injected errno, then clears it (single-shot, so retry tests work). Default is no-op.
- `inmemory_test.rs`: integration tests combining `InMemoryVolume` + `VolumeManager`, streaming state, sort helpers
- `manager.rs` inline tests: concurrent registration/read/write-mix scenarios
- `mtp_scan_oracle_tests.rs`, `smb_scan_oracle_tests.rs`: oracle-aware batch-scan integration tests for MTP and SMB

Per-backend tests live colocated with their backend in `backends/`. See `backends/DETAILS.md` §
"Testing".

### Test isolation for the global `VolumeManager`

Prefer a **private** `VolumeManager::new()` (most `manager.rs` and `archive_routing.rs` tests do), or a **unique**
volume id when the code under test reaches for `get_volume_manager()`. Neither works for the handful of tests whose
subject IS a hardcoded id: `create_*_core(None, …)`, `write_payload_to_dir(None, …)`, and `scan_preview_source_volume`
resolve `None` to `"root"`, so the volume has to be registered under exactly that.

Under plain `cargo test` a crate's tests share one process, so those tests are all writing to one `"root"` slot:
- Installing an **equivalent** volume idempotently is safe. `ensure_root_volume()` (duplicated in `create/tests.rs`,
  `write_operations/paste_clipboard_tests.rs`, `file_viewer/archive_extract_test.rs`, and `commands/rename.rs`)
  `register_if_absent`s a local-FS `"root"`, so whoever runs first wins and the value is the same either way.
- Installing a **different** volume needs `manager::test_support::TestVolumeRegistration`, which restores the previous
  registration from `Drop` (unwind included). Without it, `commands/file_system/write_ops.rs`'s `InMemoryVolume` `"root"`
  outlives its test, `ensure_root_volume`'s `register_if_absent` then silently no-ops, and every later real-FS
  create/paste assertion fails against an in-memory volume with no hint of who swapped it.

Same guard family as `listing::caching_test_support::TestListingGuard` (over `LISTING_CACHE`),
`write_operations::test_support::TestOperationGuard` (over `WRITE_OPERATION_STATE`), and
`indexing::tests::stress_test_helpers::TestInstanceGuard` (over `INDEX_REGISTRY`).

## Mutation notification

`Volume::notify_mutation`'s trait default is a **no-op**, and that's deliberate: the trait lives in `cmdr-fs`, which
knows nothing about `LISTING_CACHE`. Every backend that can be mutated overrides it.

- `LocalPosixVolume` calls `file_system::listing::mutation::patch_listing_after_local_mutation`, which stats the affected
  entry through `std::fs` and turns it into the right `DirectoryChange`. It early-returns for virtual git paths, whose
  invalidations come through the `.git`-watcher pipeline instead.
- `SmbVolume` and `MtpVolume` build the entry from their own protocol's `get_metadata` (faster than `std::fs` would be,
  and on MTP `std::fs` isn't an option at all) and call `notify_directory_changed` directly.
- `ArchiveVolume` never calls it, because it implements no mutation: `create_file`, `delete`, `rename`, and
  `write_from_stream` inherit the trait's `NotSupported` default, and `create_directory_all` overrides it only to
  return the same (pinned by `volume_test.rs::every_mutation_is_unsupported`). Zip edits are real, but they go around
  this backend: `write_operations::archive_edit` drives `ArchiveMutator` against the containing filesystem, so the
  `.zip` file's OWN volume is what notifies. `write_operations/rename.rs` takes a plain `get` rather than `resolve`
  for exactly that reason: a rename must never route to the `ArchiveVolume`.

**Why it's a no-op rather than a required method**: making it required would force ~45 `impl Volume for` sites — mostly
test doubles — to each write `Box::pin(async {})` for no gain. The no-op is also more correct than the local-FS default
it replaced, which had `InMemoryVolume` and every test double statting the real filesystem.

**The cost, stated plainly**: a new mutable backend that forgets to override this gets a silently stale destination
pane rather than a free correct one. That's what the `CLAUDE.md` guardrail and the Tier 2 checklist above are for.
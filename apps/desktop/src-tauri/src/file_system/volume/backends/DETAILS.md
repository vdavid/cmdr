# Volume backends details

Pull-tier docs for `file_system/volume/backends/`: per-backend architecture, lifecycle flows, and decision rationale.
Must-know invariants and gotchas live in `CLAUDE.md`. The trait shape, capability matrix, streaming
patterns, and "Building a new volume" checklist live in the parent `../DETAILS.md`. When you're
modifying `SmbVolume`, `MtpVolume`, `LocalPosixVolume`, the SMB watcher, or `InMemoryVolume`, read here.

## Key files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape, including the `smb/`
submodule split: `CLAUDE.md` § Module map. What each piece DOES is in the sections below (§ "SMB auto-upgrade
lifecycle", § "SMB live-reconnect lifecycle", § "SMB scan-connection pool", § "Per-backend decisions" for the session
split / watcher session / `write_from_stream` shape, § Testing for the SMB suites and their `#[path = "../smb_*.rs"]`
wiring), or in `crates/cmdr-archive/DETAILS.md` for `ArchiveVolume`. Only the layout facts that none of those carry live here:

- **`smb/events.rs` deliberately does NOT own `VolumeConnectionChanged`.** It holds the global `AppHandle`
  (`set_app_handle` from `lib.rs::setup`) and `emit_state_change`, but the typed `tauri_specta::Event` struct and its
  `VolumeConnection` state enum stay in the always-compiled `network/mod.rs`, so `collect_events!` in `ipc.rs` can
  reference them on EVERY platform. The `smb/` module is `#[cfg]`-gated to macOS and Linux (as is `mtp/`); moving the
  struct in here breaks the Windows build of the event collector.
- **`volume-connection-changed` is backend-neutral, and SMB is only its first emitter.** Any backend that holds a
  session (FTP, S3, SFTP) emits the same event and inherits the frontend's unreachable banner, per-volume backoff, and
  "Sign in" prompt for free. ❌ Don't add a second, backend-named connection event: widen `VolumeConnection` and reuse
  this one. The `From<ConnectionState>` impl in `smb/state.rs` shows the shape a backend supplies, mapping its own
  internal state machine onto the wire enum.
- **`smb/volume_impl.rs` holds the ENTIRE `impl Volume for SmbVolume`** because a trait impl can't be split across
  files. The heavy bodies live as inherent `*_impl` methods in `scan.rs` / `streams.rs`, with `volume_impl.rs` reduced
  to one-line delegators. A new trait method goes here and delegates; don't try to move a trait method out.
- **`smb/foreground_yield.rs` answers "should a background transfer stand aside?" WITHOUT a per-device gate.** MTP has
  an explicit holder for its single scarce USB pipe; SMB frames just interleave over one connection, so the signal here
  is time-based instead: the share counts as busy for `TRANSFER_FOREGROUND_IDLE_THRESHOLD` after the last navigation on
  it. Scope is PER VOLUME on purpose, so browsing a local folder never slows a NAS copy. `CheckpointStream`'s auto-yield
  parks on these two functions and `SmbVolume`'s `Volume` foreground-yield methods delegate to them.
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
classified in Rust by io kind and smb2 error kind — never by message text (`no-string-matching`) — and the frontend
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

## SMB live-reconnect lifecycle

When a hot-path op hits `ConnectionLost` / `SessionExpired`, `handle_smb_result` flips state to `Disconnected` and
`transition_to_disconnected` emits `volume-connection-changed { volumeId, state: "disconnected" }`. The frontend reconnect
manager listens for this event and runs a per-volume backoff cycle (timer-driven, calling the
`reconnect_smb_volume(volumeId)` Tauri command on each tick).

`SmbVolume::do_attempt_reconnect` is the single source of truth for re-establishing the session:

1. Acquires `reconnect_lock` (single-flight: concurrent FE-cycle and lazy-nav callers wait here).
2. If state is already `Direct`, returns Ok cheaply.
3. Tries `build_session()` with the cached `SmbConnectionParams` (the credentials that worked at original connect).
4. If that fails with an auth error, calls `refresh_credentials_from_store` (which re-reads from `keychain::get_credentials`) and retries once with the fresh creds. On success, the new credentials replace the cached ones via `params.write()`.
5. On success: installs the new client + tree, restarts the watcher with `spawn_watcher` (the prior watcher is cancelled via `stop_watcher` first), then `transition_to_direct` flips state and emits `volume-connection-changed { state: "connected" }`. Doing the state flip last means observers wake up to a fully-installed session.
6. On failure: state stays `Disconnected`. The FE backoff cycle decides whether to retry. **Auth give-up is special**: when the failure is an auth error and the refreshed store creds also fail (or there are none), `do_attempt_reconnect` emits `volume-connection-changed { state: "needs_credentials" }` before returning Err. `NeedsCredentials` is a transient signal for the frontend, not a `ConnectionState` variant: the backend state machine stays binary Direct/Disconnected, which is why `From<ConnectionState> for VolumeConnection` (in `smb/state.rs`) only ever produces the other two and the give-up path names the variant directly. The reconnect manager flips to `needs-auth`, stops the futile backoff, and FilePane shows a "Sign in" prompt (`SmbReauthView`) instead of the generic "unreachable" banner. The user signs in via `Volume::reconnect_with_credentials` (Tauri `reconnect_smb_volume_with_credentials`), which persists the new password server-level (so the next reconnect is silent), updates the in-memory params, and runs `do_attempt_reconnect`. If the new creds are also wrong, it re-emits `needs_credentials` — a bad retry re-prompts rather than dead-ending.

Credentials are kept in memory for the lifetime of the `SmbVolume` (no security concern: they're already in the
process's address space for every smb2 call). Only re-pulled from the secret store on auth failure, in case the user
updated them.

### Backend-autonomous reconnect and index resume

The FE reconnect manager only runs its backoff while a `FilePane` subscribes to the volume, so before this a background
disconnect (no pane open, or a restart) left an enabled NAS index dark until the user manually re-enabled. Two backend
hooks close that, both funneling through the ONE reconnect path (`do_attempt_reconnect`):

- **`spawn_watcher_death_reconnect(volume_id)`** (in `smb/reconnect.rs`, kicked from the watcher's fatal-error exit). The watcher
  runs on its own dedicated smb2 session; that session erroring proves the server connection broke. A background
  disconnect may not have touched the MAIN session yet, so it can still read `Direct` — meaning `do_attempt_reconnect`
  would no-op. So the kick FIRST marks the volume `Disconnected`, then drives `do_attempt_reconnect` on a bounded, growing
  backoff (`WATCHER_DEATH_RECONNECT_BACKOFF`: ~6 tries over ~4 min, then gives up quietly — never hammering a truly-down
  server). It re-resolves the volume from the manager each iteration (an unmount/replace swaps the instance) and stops
  early on unmount, on a race back to `Direct` (an FE reconnect won), or on an auth failure (`PermissionDenied` — the FE
  "Sign in" flow owns that; retrying risks locking the account). Single-flight `reconnect_lock` coalesces it with any
  concurrent FE reconnect.
- **`indexing::resume_smb_index_if_enabled(volume_id)`** fires at every session-install success — `do_attempt_reconnect`
  (in-place reconnect), `register_smb_volume` (launch/auto-upgrade), and `try_smb_upgrade` (manual "Connect directly").
  It's fire-and-forget (spawns, so it never starts the async indexer under `reconnect_lock` / a registry lock), a no-op
  if the index is already active, and gated on the PERSISTED per-volume state — resume ONLY when a completed scan is
  recorded AND the user hasn't turned indexing off (the sticky `user_disabled` marker; `disable_drive_index` keeps the DB
  for fast re-enable but records intent). Registering flows through the indexing lifecycle registration bus, so the media
  scheduler resumes enrichment with no scheduler changes. The resumed index loads Stale (we weren't watching while
  disconnected); a rescan restores Fresh. Canonical detail lives in `indexing/DETAILS.md` § "SMB indexing and the
  freshness model"; this bullet is the volume-side trigger map.

## SMB scan-connection pool

Canonical home for the per-scan connection pool (`smb/scan_pool.rs`).

A cold NAS index scan is metadata-read-bound, but the ceiling is **per-connection serialization in the server's ksmbd**,
not the disks: one SMB connection can't drive the server's read queue deep enough regardless of the SMB in-flight
window. NAS-side measurement (2026-07-22) held total in-flight depth constant and varied only the TCP connection count;
4 connections raised read IOPS ~1.75× at flat disk latency and lifted cold client throughput ~3.8×. Evidence:
`~/projects-git/vdavid/smb2/docs/benchmark-findings.md` §§ "Directory-listing throughput probe" and "NAS-side ground truth" — link, don't
restate.

So background bulk work opens `SCAN_POOL_SIZE` (4) EXTRA smb2 sessions (separate TCP connections) for its duration and
spreads across them; the pane's own session keeps serving browsing. Two users today: the index scan's directory
listings, and media enrichment's parallel prefetch reads.

- **Lifecycle, refcounted.** Opened LAZILY on `Volume::begin_scan_session` (`SmbVolume::open_scan_pool`, idempotent),
  closed when the LAST concurrent scan session ends (`scan_session_refs`, a saturating counter — an index rescan and an
  enrichment pass can overlap, and either one's `end_scan_session` must not tear the pool out from under the other);
  `on_unmount` tears it down synchronously regardless (`close_scan_pool_sync` flips the pool's `closed` flag so
  reconnect loops bail — a member must not keep walking an unmounted volume). `on_superseded` does NOT close it (an
  in-flight scan is still drawing from it); it only stops a retired volume opening a NEW one. Steady-state footprint between scans is
  unchanged (`scan_pool: RwLock<Option<Arc<ScanPool>>>` is `None`). The index-scan lifecycle brackets the spawned walk
  task (`indexing/lifecycle/network_scan.rs`); the media pass brackets via a drop-guard in its scheduler — both run
  `end` on every outcome.
- **Invisible to the scanner.** The `network_scanner` walk is unchanged and transport-agnostic; it keeps calling
  `list_directory_for_scan`, which draws from the pool (round-robin) when one is active and falls back to the main
  session otherwise. **Pacing stays in the scanner** (`network_scanner/scan_pace.rs`): the global in-flight budget caps
  the pool's total concurrency, so "drop to 1 while the user browses" survives for free. The pool never owns pacing.
- **A pool member is a full `SmbClient` + `Tree`** from the same `build_session` the main path uses; `Connection::clone`
  only multiplexes over ONE session, so separate connections mean separate `SmbClient`s. Each member has its own async
  `Mutex` (cloning the `Connection` needs `&mut`), so different members list truly in parallel; the lock is held only to
  clone (microseconds), never across a `build_session`.
- **Selection is a pure, unit-tested `PoolSlots`** (round-robin `next_alive`, `mark_dead`/`mark_alive`, single-flight
  `try_begin_reconnect`), decoupled from the real sessions so the handout/replacement logic is testable server-free.
- **Failure handling.** A listing failing with a typed `ConnectionLost`/`SessionExpired` is retried on a sibling member,
  the dead member is dropped, and a single-flight background task reconnects it (`build_session`, bounded growing backoff
  `POOL_MEMBER_RECONNECT_BACKOFF`; gives up on auth — the MAIN session owns the credential-refresh / `needs_credentials` flow).
  A dead member NEVER transitions the main volume's connection state. A per-directory error (permission, not-found) is
  the same on any connection, so it's surfaced immediately, not retried. If every member is momentarily dead, the
  listing falls back to the main session, which keeps the scan progressing and, if it too is dead, yields the
  `DeviceDisconnected` the scanner's terminal-disconnect path expects. Members open STAGGERED at pool open; a rejected
  Nth session (server session cap) just means the pool runs with fewer.
- **Params are a snapshot.** If the main session refreshes credentials mid-scan (password change), members failing auth
  give up and listings fall back to the main session (documented degradation, not a correctness issue).
- **Reads: compound-only on members** (`open_read_stream_for_scan_impl`). Media enrichment's prefetch reads small
  HINTED files from pool members via the 1-RTT `read_file_compound` (dead member ⇒ sibling retry, exactly like a
  listing; size drift or a too-large file ⇒ main-session streaming). Members deliberately never serve STREAMING reads:
  a member dying mid-stream would surface as a transport error the pool can't transparently retry for the consumer —
  the main session, with its reconnect machinery and connection-state signaling, owns streaming.

## Per-backend decisions

**Decision**: `SmbVolume::to_smb_path` returns `Result<String, VolumeError>` and refuses a path outside the mount root
**Why**: it turns a path the frontend sent into the share-relative string that goes on the wire, and every way of GUESSING an answer for an out-of-root path put a real request at a real, wrong place. It compared the root as a raw STRING, so with root `/Volumes/naspi` a path under the sibling mount `/Volumes/naspi-1/x` stripped to `-1/x` — a legal file name on the share, which the server would happily create or delete. Anything that matched neither fell through to "strip the leading slash", so `/Users/me/notes.txt` went out as the share-relative `Users/me/notes.txt`. Matching whole path COMPONENTS (`Path::strip_prefix`) kills the first, and `VolumeError::NotFound` for the rest kills the second: a path that isn't on this volume genuinely isn't found there, and the caller surfaces that instead of acting elsewhere. `exists` maps the error to `false` (the honest answer to the question it was asked), and the post-mutation listing-cache patches go through `display_path_for`, which returns an `Option` so a write that already succeeded is never reported as failed because its parent path didn't convert.

**Decision**: `SmbVolume` and `MtpVolume` store `volume_id: String` for listing cache lookups
**Why**: `notify_mutation` needs to call `notify_directory_changed(volume_id, ...)` to find the right cached listings. The volume_id is computed at creation time (`smb_volume_id(server, port, share)` for SMB so two same-named shares on different servers don't collide — see `volumes/CLAUDE.md` § "Volume IDs"; `"{device_id}:{storage_id}"` for MTP) and stored on the struct rather than recomputed on every mutation.

**Decision**: `map_smb_error` maps `ErrorKind::InvalidName` to its own `VolumeError::InvalidName`, never the `IoError` catch-all
**Why**: `STATUS_OBJECT_NAME_INVALID` means the server refused the NAME, so it never looked for the file and the identical request can only fail the identical way. As an `IoError` it inherited the wrong behavior twice over: `retry.rs::is_retryable` would have burned the full backoff re-sending a hopeless write, and the dialog would have offered "couldn't copy the file" plus a Retry button instead of the one thing that works (rename it). The typed variant carries end to end: `friendly_error::kinds::invalid_name` on the listing path (`NeedsAction`, ❌ no retry hint) and `WriteOperationError::InvalidName` on the write path, which names the failing file so a 5,000-item transfer says WHICH one to rename. smb2 ≥ 0.18 maps the characters SMB2 forbids outright (`"`, `*`, `:`, `<`, `>`, `?`, `\`, `|`, control characters, trailing space or period) into the Unicode private-use area, so those copy through fine; what still reaches this arm is a reserved Windows device name (`CON`, `NUL`, `LPT1`), a name past the server's own length limit, or a character its filesystem can't store. The status is also in smb2's table now, so the technical-details line reads `STATUS_OBJECT_NAME_INVALID` rather than bare `0xC0000033`.

**Decision**: `SmbVolume::supports_local_fs_access()` returns `false`, but `paths_are_os_visible()` returns `true`
**Why**: `SmbVolume` handles listing updates via `notify_mutation` using its own smb2 `get_metadata`. A `std::fs`-based synthetic diff path (`emit_synthetic_entry_diff`) would be redundant and would go through the slow OS mount. Returning `false` skips it. But "Cmdr shouldn't use `std::fs` here" is a different claim from "no other app can open these paths": the sneaky mount keeps the share at `mount_path` and every path this volume hands out is an absolute path under it. The macOS drag-out path needs the second answer, so it reads `paths_are_os_visible()`. While it read the first one, a drag out of an SMB pane published `NSFilePromiseProvider` items with an empty pasteboard, which Finder accepts and every other drop target (browser upload widget, mail composer, editor) rejects — so dragging NAS files into an email did nothing while the same drag from Finder's mount worked. ❌ Don't collapse the two flags: five write/caching call sites read `supports_local_fs_access()` as "is this remote?", where `false` stays the honest answer.

**Decision**: `SmbVolume` splits session storage: `Arc<Mutex<Option<SmbClient>>>` + `Arc<RwLock<Option<Arc<Tree>>>>`
**Why**: Keeping the session in one `Mutex<Option<(SmbClient, Tree)>>` would force the streaming-read producer and the compound read/write fast-paths to hold the mutex for the entire transfer, serializing every concurrent copy through it. `smb2::Connection` is `Clone` (cheap `Arc::clone`, all clones multiplex frames over one SMB session), so splitting the Tree out lets us briefly lock the client, clone its `Connection`, and release the lock, then drive `Tree::download` / `Tree::read_file_compound` / `Tree::write_file_compound` on the cloned `Connection` with no lock held. N concurrent copies on one `SmbVolume` pipeline N operations over the single session instead of queuing on the mutex. Tree lives in a `RwLock` because we only take read locks in the hot path (cloning an `Arc<Tree>`) and only write on disconnect. The streaming-write path uses the same clone-and-release shape (see the `write_from_stream` Decision below), so the client mutex is never held across I/O.

**Decision**: `SmbVolume::local_path()` returns `None`
**Why**: `local_path()` is checked in `volume/copy.rs` to decide whether to use native OS copy APIs. If SmbVolume returned `Some(mount_path)`, copies would go through the slow OS mount, which is exactly what we're trying to avoid. `root()` still returns the mount path for frontend path resolution.

**Decision**: SmbVolume background watcher runs on a dedicated smb2 session, not a clone of the volume's main connection
**Why**: smb2 0.10 made `Watcher` `'static` (owns a `Connection` clone), so technically the watcher could share the volume's session via `clone_session`. Empirically it can't: stacking the watcher's CHANGE_NOTIFY long-polls on the same TCP session as heavy concurrent writes wedges Samba — `smb_integration_concurrent_streaming_writes_no_deadlock` hangs against `smb-consumer-maxreadsize` (64 KB max read/write, 8 concurrent writers, 200 × 1 MB files). The dedicated session keeps the watcher's traffic out of the writers' way at the cost of a separate TCP+auth. What we *do* keep from the new API: the watcher is `'static` (no borrow on the watcher task's `client`), and the pipelining (one CHANGE_NOTIFY pre-issued so events during consumer processing don't fall in a re-arm gap). Stat calls for new/modified files still go through `VolumeManager::get(volume_id).get_metadata(...)` (the main session), so the cmdr-side `notify_mutation` cache patch from our own writes lands first regardless.

**Decision**: Watcher task is not stored on `SmbVolume`, only the cancel sender is
**Why**: The spawned task owns its own `Watcher` and `SmbClient`. Storing them on the struct alongside the cancel sender would just duplicate ownership without buying anything — `watcher.next_events()` is `&mut self`, so the task is the only thing that can drive it anyway. The `watcher_cancel: Mutex<Option<oneshot::Sender<()>>>` on the struct provides clean shutdown.

**Decision**: Watcher doesn't reconnect itself; on death it KICKS the one reconnect path
**Why**: When `next_events` errors with anything but `NOTIFY_ENUM_DIR`, the watcher's task returns. It must NOT run its own reconnect-with-backoff loop: two state machines tracking the same "is the session alive" question diverge — the watcher's internal retries would swallow real disconnections the FE reconnect manager surfaces. So the watcher still owns no reconnect logic; it just calls `spawn_watcher_death_reconnect(volume_id)`, which drives `do_attempt_reconnect` (the single source of truth) on a bounded backoff. One reconnect path, one source of truth — now triggered on watcher death too, not only by the next hot-path op / FE backoff tick. See § "Backend-autonomous reconnect and index resume" for why the kick marks the volume `Disconnected` first.

**Decision**: we run `smb2`'s deadline and keepalive defaults unchanged, and read none of them as a liveness verdict
**Why**: every wait a request can make is bounded by the crate, so Cmdr needs no timeout layer of its own. A frame gets 20 s to reach the socket (`Error::SendTimeout`); once out, the server gets 30 s of SILENCE (not elapsed time — every interim `STATUS_PENDING` restarts the clock, so a multi-minute write to a loaded NAS is never cut off), stretched to 6× that on a connection an ECHO probe has just proven alive. A breach tears the connection down, which is why `retry.rs` sees a typed `DeviceDisconnected` / `ConnectionTimeout` instead of a hang. The ECHO keepalive (5 s, on by default) only probes when the wire has gone quiet with work outstanding, so a busy transfer pays nothing for it. ❌ **A missed probe is NOT evidence of death** and nothing here may treat it as such: a QNAP TS-464 drops probes precisely while it writes (measured 2026-08-02: 1 of 3 dropped under write load, 0 of 3 idle). The crate agrees — its only death verdict, `Error::ServerUnresponsive`, needs a request to burn its whole deadline AND the connection to have put nothing at all on the wire meanwhile. That is also why `SmbVolume::connection_liveness()` stays unimplemented; the full argument and what `smb2` would have to expose to change it: `write_operations/transfer/DETAILS.md` § "The watchdog ACTS".

**Silence measured across a frozen Cmdr is discounted, not counted** (`smb2` 0.18.1+). Every clock in the crate measures wall time, so a stretch where this process was not scheduled at all — a laptop sleep, an App Nap, a machine starved by a parallel build — used to read as the server going quiet, and the reconnect that followed was against a NAS that had been answering the whole time (2026-08-08: three freezes of 62 s, 175 s, and 355 s in twelve minutes). The crate now recognizes the gap from its own loop cadence and shifts every liveness clock forward by it. Two consequences for Cmdr: an `SmbVolume` reconnect after a wake is now evidence about the *network*, not about the sleep, and `MetricsSnapshot::scheduling_stalls` (surfaced through `commands/smb_diagnostics.rs`) is the counter that says whether the app stopped running. ❌ Don't add a Cmdr-side sleep/wake hook for this — the crate handles any stall from any cause, and a hook would only cover the one macOS reports.

**Decision**: the watcher's dedicated session is probed like any other, and a watcher death stays cheap
**Why**: CHANGE_NOTIFY is exempt from the request deadline by design (it waits for an event that may never come), so that connection is bounded by connection-wide silence instead — which is the only thing that lets a watcher on a dead session ever find out, and it's what feeds `spawn_watcher_death_reconnect`. The cost of a false one is small by construction: the kick marks the volume `Disconnected` and rebuilds the session, while an in-flight transfer holds its own `Arc<Tree>` + `Connection` clone and runs on. ⚠️ Unverified: whether a NAS busy enough to drop 6 consecutive probes (30 s of total silence on that session) can trigger this during a large copy. It has not been observed; if watcher deaths ever cluster with heavy transfers, that is the mechanism to suspect, and the fix is `Connection::set_response_timeout` on the watcher's session alone.

**Decision**: Watcher debounces 200ms per batch, `FullRefresh` above 50 events per directory
**Why**: Prevents 1000 individual stat calls when 1000 files are copied. The 200ms window collects events that arrive in rapid succession. The 50-event threshold for `FullRefresh` avoids O(n) stat calls for bulk operations.

**Decision**: `write_from_stream` uses a cloned `Connection` + `Arc<Tree>` (owned `FileWriter`)
**Why**: `FileWriter` owns its `Connection` (cheap `Arc::clone`) and `Arc<Tree>` rather than borrowing `&'a mut Connection`. `write_from_stream` calls `clone_session` once up front and drives both the compound fast-path AND the streaming fallback on the same owned `Connection` clone. The client mutex is held only for the few microseconds of `clone_session()`, never across I/O. **Don't switch back to a borrowed `FileWriter<'a>` that holds the client mutex across the upload**: that shape deadlocks under sustained concurrent pressure (the two-phase brief-clone-then-long-hold pattern is the QNAP deadlock reproducer). The regression is pinned by `smb_integration_concurrent_streaming_writes_no_deadlock`. The architectural property we get from owned `FileWriter`: N concurrent streaming writes on one `SmbVolume` pipeline N WRITE chains over a single SMB session, multiplexed by `MessageId` in smb2's receiver task. No external locking, no mutex contention on the hot copy path.

**Decision**: `write_from_stream` ERROR paths delete the partial file, mirroring the cancel branch
**Why**: Once the streaming `FileWriter` is open and bytes have streamed into it, an early error (mid-stream source-read error, `write_chunk` failure, `finish` failure, the compound-fallback writer's `write_chunk`/`finish`) would otherwise leave a half-written file at the user's intended destination name — corrupt bytes presented as a real file (violates AGENTS.md principle #4). The cancel branch already cleaned up (`writer.abort()` + best-effort `delete_file` on a fresh cloned session); every owned-writer error site now does the same. **`abort()` before delete is load-bearing**: dropping a `FileWriter` without `finish()`/`abort()` leaks the SMB handle (smb2's `FileWriter::Drop` only logs, never sends CLOSE), so a fresh-session `delete_file` (CREATE-with-delete-on-close) hits a sharing violation against the still-open handle and the partial lingers. So: `write_chunk`/source-read errors `writer.abort().await` first (writer still owned), then `delete_partial()`. `finish()` consumes the writer, so on its failure the handle is already gone — best-effort `delete_partial()` only. The compound FAST-path (`write_file_compound`) is atomic CREATE+WRITE+FLUSH+CLOSE and the compound DRAIN loop buffers in memory before any handle opens, so neither leaves a streamed partial — those propagate their error unchanged. The original error always propagates (never `Cancelled`); cleanup is best-effort and never masks it. Pinned by `smb_integration_write_from_stream_source_error_deletes_partial` (source errors after the first chunk; asserts the propagated `IoError` and that no file remains at the destination). Don't refactor the owned-writer error sites into a post-block catch-all that loses the writer — you'd lose the `abort()` and the delete would no-op against the leaked handle.

**Decision**: `SmbVolume` overrides `scan_for_copy_batch` to pipeline per-path stats over a single SMB session
**Why**: A naive scan phase that loops `scan_for_copy` per top-level source costs N sequential RTTs before the copy phase can start. For a 100-file copy over a ~60 ms Tailscale link that's ~5 s of serial stats. The override clones `smb2::Connection` per path under a brief client-mutex acquire (cheap `Arc::clone`, all clones multiplex over the same SMB session), releases the lock, then drives `tree.stat(&mut conn, path)` on each clone inside a `FuturesUnordered`. Empty root paths skip the stat. Single-path batches fall through to `scan_recursive` so one-file drag-drops don't pay the batch machinery cost. Directories found during the stat phase recurse sequentially afterward; parallel directory recursion is a future enhancement. Measured 6.5× wall-clock win at 100 × 10 KB: 6.11 s → 947 ms. See `docs/notes/phase4-rtt-investigation.md` for the wire trace. **Oracle layered on top**: before the pipelined-stat block runs, every input path's parent is checked against the fresh-listing oracle (`try_get_authoritative_listing(volume_id, parent)`). Oracle-served paths get their size + `is_directory` from the cached `FileEntry` and are removed from the leftover set; only the leftover paths go through the pipelined stat. Decision is per-parent: one batch can mix oracle-served and pipelined-stat paths, and if every path resolves via the oracle the stat pipeline is skipped entirely.

**Decision**: `MtpVolume` overrides `scan_for_copy_batch_with_progress` to group selected paths by parent and list each parent once
**Why**: MTP has no single-file stat call: `get_metadata(path)` lists the parent directory and searches by name. A naive scan that called `get_metadata` per path would re-list `/DCIM/Camera` (15k entries, ~17 s over USB) for every selected photo. The override groups the input paths by parent, calls `list_directory(parent, on_progress)` once per unique parent, and indexes the entries by name for O(1) lookups. **Oracle layered on top**: before listing a parent, the override consults `try_get_authoritative_listing(volume_id, parent)`; on hit, the cached entries replace the listing call entirely (no USB I/O for that parent). On miss the single-listing-per-parent path runs, so cold-cache perf is preserved. Decision is per-parent; one batch can mix watcher-fresh and cold parents.

**Decision**: `SmbVolume` has a compound fast-path in `open_read_stream_with_hint` and `write_from_stream` for files ≤ `max_read_size` / `max_write_size`
**Why**: The streaming open+read+close sequence costs 3 RTTs per file. For small files (typical 10 KB copies on a NAS) that dominates wall-clock at high-latency links (~60 ms RTT → ~180 ms/file just for protocol overhead, not data). `smb2` already exposes `Tree::read_file_compound` (CREATE+READ+CLOSE in a single compound frame = 1 RTT) and `Tree::write_file_compound` (CREATE+WRITE+FLUSH+CLOSE = 1 RTT). The copy pipeline feeds per-file size hints from the pre-copy scan; when the size is known and fits in one READ/WRITE, we take the compound path. Falls back cleanly to the streaming reader/writer when the hint is missing or the file is too big. Small compound reads return a `Vec<u8>` wrapped as a single-chunk `InlineReadStream` so the consumer API stays shaped the same. See `docs/notes/phase4-rtt-investigation.md` for the measurement. The WRITE side's condition is also a DATA-SAFETY contract: `write_is_single_shot` answers with the same `fits_one_compound_write` the fast path branches on, and the transfer layer skips its `.cmdr-tmp-*` staging on the strength of that answer. What the backend owes in return (short sources stay on the compound path, a post-CREATE failure cleans up after itself): `write_operations/transfer/DETAILS.md` § "The single-shot exemption".

**Decision**: `LocalPosixVolume::write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before it returns
**Why**: Every cross-volume copy/move that lands on a local disk (MTP → Local, SMB → Local, USB import) flows through this one method. A bare `file.flush()` finish is a userspace no-op on a raw `std::fs::File`, so the bytes would sit only in the OS page cache when the op reports "complete" — letting the user eject / sleep and lose data (on a move, from both sides, since the source delete runs after the copy reports Ok). The `sync_data` (fdatasync) gives the "durable as each file completes" property the local-FS chunked copy already has (`transfer/chunked_copy.rs`), so a crash mid-batch leaves earlier files safe. The parent-dir fsync makes the file's directory entry durable too. Both are best-effort on error: a failure logs under `target: "write_durability"` and continues rather than failing a completed multi-GB transfer at the final fsync (matching `durability::flush_created_destinations`). Non-local backends (MTP/SMB/InMemory) need no equivalent — durability there is the device/server's concern. Pinned by `local_posix_test::test_write_from_stream_multichunk_is_durable_and_correct` (content-correctness regression guard; the fdatasync itself isn't observable from a unit test).

## SMB archive push-refresh

The recursive share watcher already refreshes the DIRECTORY listing showing a changed `.zip` (its new size/mtime). On top of that, `process_event_batch`'s Modified and RenamedNewName handlers call `maybe_refresh_archive_listings(volume_id, entry_path)`: when `entry_path`'s name is a supported archive (`archive::has_supported_archive_extension`, the single-source predicate `format_for_name` backs), it fires the same `caching::refresh_archive_listings` the local `archive::watch` fires, pushing an out-of-band edit of the `.zip` to any open archive-INNER listing.

Why this is the whole fix, cheaply:

- **Same consumer, same key.** `refresh_archive_listings` scans `LISTING_CACHE` for keys at/inside the archive path and re-reads them; `volume_id` here is the parent DRIVE id, which is exactly what archive listings key on, so no rekeying. It's a no-op when the path isn't an archive or no inner listing is open, and the watcher already runs for the whole volume lifetime — so the only added cost is a re-parse when a `.zip` actually changes AND an inner pane is open.
- **`entry_path` is already normalized.** It's the `to_nfd_display_path` result, so it went through the same backslash→slash + NFC→NFD normalization every other cache-facing path in `smb_watcher.rs` uses. Passing the raw event filename would miss the cache.
- **Fires independent of the stat.** The refresh runs even when the pre-refresh `get_metadata` fails (a mid-write, truncated `.zip`): `refresh_archive_listings` keeps the previous inner listing on an unreadable parse rather than blanking the pane, and the next change event retries.
- **NOT a freshness claim.** This is a visible-listing UX nicety, a SEPARATE consumer from the write-op fresh-listing oracle. `ArchiveVolume::listing_watch_coverage` stays `None` for a remote parent regardless (the SMB watcher is lossy under load, so the oracle must keep re-reading pre-flight scans honestly). The remote-archive freshness decision and the guardrail test are in `crates/cmdr-archive/src/watch/DETAILS.md` § "remote archives have NO live watch". MTP keeps manual refresh (F5) as its contract.

Tests: `smb_watcher/archive_refresh_test.rs` (a Modified `.zip` event refreshes the inner listing; a non-archive change doesn't — the extension gate).

## Supersede vs. unmount

`Volume` has two retirement hooks, and confusing them breaks live operations.

- **`on_unmount`**: the device is gone (ejected, network mount torn down, FSEvents unmount). Tear everything down.
  `SmbVolume` flips `unmounted`, forces state to `Disconnected`, cancels the watcher, closes the scan pool, and drops
  the smb2 `Tree` + `SmbClient`. Callers: `volumes::watcher::handle_volume_unmounted` and `volume::eject`.
- **`on_superseded`**: a NEWER instance took this volume's id in the `VolumeManager`, but the device is still there.
  Sole caller: `network::smb_upgrade::register_replacing_predecessor`.

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

**Watcher identity, not just id.** `spawn_watcher_death_reconnect` takes the dying watcher's `SmbVolume::instance_id`
and re-resolves the manager entry against it on every backoff step. A watcher dying in the window around a swap would
otherwise resolve the id to the SUCCESSOR and mark a perfectly healthy volume `Disconnected`.

## Gotchas

**Gotcha**: `MtpReadStream` holds nothing scarce between windows, so dropping it mid-read is safe and needs no `Drop` impl
**Why**: It reads in bounded `GetPartialObject64(offset, MTP_READ_WINDOW)` windows (the windowing + offset accounting live in `mtp/connection`; see that module's DETAILS § "Bounded-window reads"). Between windows nothing is in flight — no held `FileDownload`, no pinned PTP session — so a cancel/pause/drop has nothing to abort or drain (`cancel_and_release` is the trait default no-op). If the stream is dropped WHILE a window read is in flight, mtp-rs's `TransactionScope` flags the pipe and the next op drains it under the operation lock (one ~300 ms self-heal), so an aborted window never desyncs the session. ❌ Don't re-add a `Drop`/cancel here: there's no held `FileDownload`, so mtp-rs's `ReceiveStream` unconsumed-drop panic (the reason a `Drop` cancel was once needed) can't apply.

**Gotcha**: `MtpVolume::get_metadata` is expensive: it lists the entire parent directory
**Why**: MTP has no single-file stat call. `get_metadata` lists the parent directory and searches for the entry by name. This is used by `notify_mutation` after each self-mutation (create, delete, rename) and is acceptable because those are infrequent, but avoid calling it in hot paths.

**Gotcha**: Watcher filenames from SMB use backslashes; must normalize to forward slashes
**Why**: SMB servers send paths like `papers\new-file.txt`. The watcher normalizes these to `papers/new-file.txt` before extracting parent directories and constructing display paths.

**Gotcha**: Watcher filenames are NFC (from server) but macOS mount paths are NFD
**Why**: SMB servers return NFC-normalized filenames. macOS filesystem paths use NFD. The watcher NFD-normalizes filenames before constructing display paths used for cache lookups.

## Testing

- `in_memory_test.rs`: unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `local_posix_test.rs`: real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `mtp/` inline tests: path conversion and capability flags (no device needed)
- `smb_test.rs`: SMB unit tests (no server needed): type mapping (DirectoryEntry→FileEntry, FsInfo→SpaceInfo,
  Error→VolumeError), connection state transitions, path conversion, capability flags, and the channel-backed
  `SmbReadStream` consumer. These run by default.
- The SMB test suites live in files under `backends/` wired as `#[cfg(test)] #[path = "../smb_*.rs"] mod`s of `smb`
  from `smb/mod.rs` (so `super::*` still reaches the backend's private items; the `../` hops up out of the `smb/`
  directory), split by theme: `smb_test.rs` (unit, above), `smb_integration_test.rs`
  (connection management, core CRUD, basic streaming smoke, scan/conflict preview), `smb_streaming_integration_test.rs`
  (the full read/write streaming surface: progress, cancel, large multi-chunk files, plus the error/cleanup paths with
  the `ErroringReadStream` double), `smb_transfer_semantics_test.rs` (high-level merge/move contracts driven through
  the transfer pipelines), `smb_stress_test.rs` (concurrency: the no-deadlock guard with its `MutexCaptureLogger`
  machinery, and the 100-file content-integrity test), `smb_full_concurrency_test.rs` (below), and `smb_soak_test.rs`
  (below). Cross-suite helpers
  (`make_docker_volume`, `test_dir_name`, `ensure_clean`, `hash_bytes`, `hash_volume_file`, `TEST_PREFIX_ROOT`,
  `cleanup_test_prefix`) live in `smb_test_support.rs` as `pub(super)` items.
- **Docker SMB integration tests** (the themed `smb_*_test.rs` Docker suites above): `#[ignore]` tests that require Docker SMB containers
  (start with `apps/desktop/test/smb-servers/start.sh`). Run with `cargo nextest run smb_integration --run-ignored all`.
  Connect via `smb2::testing::guest_port()` (10480, guest/no-auth), `auth_port()` (10481, `testuser`/`testpass`),
  `readonly_port()` (10488), `slow_port()` (10493, 200ms latency). Use these for testing real SMB protocol behavior
  (streaming, error paths, network edge cases). See `apps/desktop/test/smb-servers/README.md` for the full container
  list and env var overrides.
- **Full-concurrency copy** (`smb_full_concurrency_test.rs`): the automated net under the 2026-07-31 transfer wedge
  (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). 400 local sources onto the share through
  `copy_volumes_with_progress` at the driver's own concurrency, with sizes on BOTH SMB write paths: the large ones are
  sized off the session's negotiated `max_write` at runtime, not hardcoded, so they always land on the staged streaming
  writer. Beyond byte-exactness it asserts three things a content check alone would miss: the concurrency window really
  filled (peak `TransferActivity::in_flight` off the progress events, against a floor rather than the driver's own
  formula, so a change to it can't fail this suite for the wrong reason), a `.cmdr-tmp-*` really appeared during the
  copy (else the
  "no leftovers" check passes vacuously), and none survived it.

  Both tests here bound their own wait and, on expiry, panic with `transfer_probe`'s LIVE in-flight table via
  `write_operations::render_live_transfer_dump` — a `#[cfg(test)]` accessor over the probe registry. They time out on
  the copy's `JoinHandle`, never on the copy future: timing out the future DROPS it, which drops the probe guard and
  empties the registry before there is anything to dump. `smb_integration_a_wedged_copy_is_caught_and_names_its_phase`
  is the test of that mechanism — it parks a copy on the pause gate and asserts the bound fires with a dump naming the
  operation, the driver phase, the window fill, and `parked(pause)`. Without it the deadline and the dump are untested
  scaffolding, and a suite meant to catch a hang becomes one. The wedge is staged through the pause gate rather than a
  silenced server because what is under test is the harness at expiry, and a pause reaches that state deterministically
  without holding the shared Docker stack hostage. `.config/nextest.toml` grants the big test a 75 s cap so its own 45 s
  deadline stays authoritative; a cap kill would leave no diagnostic, which is the exact outcome the milestone ends.
- **SMB soak test** (`smb_soak_copy_loop` in `smb_soak_test.rs`): Repeats the SMB→Local copy pipeline for hundreds to
  thousands of iterations and watches RSS, open FDs, SMB credits, and per-iteration wall-clock drift. Catches accumulating bugs
  the single-shot integration tests can't see (credit leak, FD leak, memory growth, slowdown). Default mode:
  `CMDR_SOAK_ITERATIONS=100` (≈5 s against Docker). Long mode: `CMDR_SOAK_DURATION_SECS=1800` (30 min, via
  `./scripts/soak-smb.sh`). CI has a `workflow_dispatch`-only job in `slow-checks.yml`.
`LocalPosixVolume` routes every non-forced rename through the shared atomic-exclusive primitive. This applies equally
to `/`, attached disks, Dropbox, iCloud, and other local POSIX roots registered with non-root volume IDs. Forced
renames retain normal POSIX replacement semantics because the caller explicitly authorized replacement.

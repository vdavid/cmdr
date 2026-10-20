# `cmdr-smb` details

## Where the boundary runs, and why

Two boundaries run through this crate, and they answer different questions.

**The backend / app boundary** is the `Volume` trait plus the `VolumeHost` seams: `SmbVolume` implements one and asks
everything else through the other, so nothing here names the app. What stayed up there is what genuinely needs the app:
finding a share and mounting it (`apps/desktop/src-tauri/src/network/`), minting an `SmbVolume` and registering it
(`network/smb_upgrade.rs`), and driving every copy, move, and delete with the real event sink and pause gate
(`crates/cmdr-operations/src/`). App call sites import this crate by name, the way they do `cmdr-sftp` and
`cmdr-webdav`.

**The protocol / app boundary** is older and sits inside `network/`, which grew as one pile: mDNS discovery, share
listing, mounting, the keychain, the auto-upgrade passes, and the Tauri events, plus a handful of pure functions over
`smb2`'s own types that ended up there for no reason beyond proximity. `src/{types,errors,connection}.rs` is those pure
functions, plus the vocabulary they speak.

The test for the second one is a single question: **can the protocol and its own types answer this?**

Here, because the answer is yes:

- `build_smb_addr` — an `smb2` address string. Strips a `.local` suffix, because `smb2` puts the addr's host component
  into UNC paths (`\\server\IPC$`) and some servers reject `.local` there. An IPv6 literal goes in bare (`fe80::1:445`)
  and still dials: an address that doesn't parse as a socket address reaches the resolver split on its LAST colon. Bare,
  bracketed, and expanded spellings all opened a share (verified on smb2 0.21.0 / tokio 1.53.1, `try_open_share` against
  the `guest` fixture through a `[::1]` forwarder, 2026-09-10), so brackets would change nothing. In every spelling smb2
  takes the UNC server name as what comes before the FIRST colon, and Samba ignores it.
- `is_auth_error` / `classify_error` / `classify_authenticated_error` — reading an `smb2::Error`. Every retry, fallback,
  and sign-in path branches on the first one, and it's the reason the `UpgradeFailure` log can name a rejected password
  even though its own enum has no auth variant. A named login answered with a guest session (`GuestSessionRefused`, smb2
  0.31) counts as auth: it's how a Samba server set to `map to guest = bad user` answers a wrong password. A reply
  refused for its signature (`SignatureInvalid`) is ❌ not auth, and becomes `ShareListError::SignatureInvalid` /
  `VolumeError::SignatureInvalid`, never the `ProtocolError` that sends a listing down the CLI fallback.
- Every smb2 error on the main session passes ONE funnel, `SmbVolumeInner::note_smb_error` (called by
  `handle_smb_result`, the streaming-read producer, the random reader, and the copy scan's stat pipeline), which calls
  `session::report_security_refusal` to tell the host's `security_refusal` seam about a refused signature. A refused
  signature also drops the main session, like a lost connection (`state::ends_the_session`): nothing can vouch for a
  signed session's state after a refused reply, so the next dial (the pane's reconnect, the notice's Reconnect) builds a
  fresh, freshly signed one (#457).
- A reconnect the saved credentials can't bring back emits `VolumeConnection::GuestSessionRefused` when the server
  answered the named login with a guest session, else `NeedsCredentials` (`reconnect::credentials_refusal_state`), so
  the pane names what Cmdr refused.
- `try_list_shares_as_guest` / `try_list_shares_authenticated` — one connect and one `list_shares`, nothing else.
- `ShareInfo` / `AuthMode` / `ShareListResult` / `ShareListError` — what a listing attempt produced.

In the app, because the answer is no:

- **Discovery** (`network/mdns_discovery.rs`, `manual_servers.rs`, `virtual_smb_hosts.rs`) — mDNS is a different
  protocol, and the discovered-host registry is app state the frontend subscribes to.
- **Mounts** (`network/mount.rs`, `mount_linux.rs`) — `NetFSMountURLSync` and `gio mount` are OS APIs, not SMB.
- **The keychain** (`network/keychain.rs`, `credential_store.rs`) — the backend asks for credentials through the
  `CredentialStore` seam; where they're stored is the host's business.
- **The CLI fallback** (`network/smb_smbutil.rs`, `smb_smbclient.rs`) — subprocesses under the app's deadline wrapper,
  reading app settings.
- **The upgrade passes** (`network/smb_upgrade.rs`) — they decide when to replace a kernel mount with a direct session,
  which needs the volume registry, the index, and analytics.
- **Every event and every word** (`network/events.rs`'s `VolumeConnectionChanged`, `SmbFellBackToOsMount`,
  `os_mount_notice.rs`) — `tauri_specta` payloads and the once-per-server ledger behind them.

The full app-side story stays in `apps/desktop/src-tauri/src/network/DETAILS.md`; this document doesn't restate it.

## Why `convert_shares` sits in `types.rs` rather than beside the classifiers

It builds `ShareInfo` out of `smb2::ShareInfo`, so it belongs with the type it produces. `errors.rs` answers one
question — what went wrong, and would credentials help — and keeping a successful-path mapper there blurred it.

`smb2::list_shares()` already filters to disk shares and strips `$` shares, which is why `is_disk` is unconditionally
true: the field exists for the frontend's benefit, not as a filter this crate applies.

## What the `testing` feature does, and the cargo rule behind it

Two things: it publishes `volume::testing` (the shared Docker fixtures, plus the `blake3` they hash with), and it
forwards `smb2/testing`. The app's `smb-e2e` feature turns it on for the second reason, reaching `smb2` through
`cmdr-smb` rather than naming `smb2/testing` directly.

That works because cargo unifies features per package across the whole resolved graph: there is one `smb2` node, and a
feature any dependent turns on is on for every dependent. So `network/virtual_smb_hosts.rs`, which calls
`smb2::testing::guest_port()` through the app's OWN direct `smb2` dependency, keeps compiling with no second forward.
(Verified with `cargo tree -p cmdr -e features -i smb2 --features smb-e2e` and a real
`cargo check -p cmdr --lib --examples --features smb-e2e` on cargo 1.9x, 2026-08-21: `smb2 feature "testing"` appears
only under the `smb-e2e` resolution.)

A self dev-dependency (`cmdr-smb = { path = ".", features = ["testing"] }`) turns the feature on for every dev target
and leaves it off for the lib, so `volume::testing` and its `blake3` exist in tests and in no shipped build. Without
that self dependency a `smb2/testing`-gated item would be unreachable from a plain `cargo test -p cmdr-smb`, because a
feature is off by default for the crate that declares it.

One consequence worth knowing: **a test that needs the Docker fixture ports can live on either side of the boundary.**
Whichever crate's test target enables the feature, the one `smb2` gets it.

## The public surface is capped

`index-crate-isolation` holds this crate to 16 root promises, 4 public modules, and 22 public items inside them. The
numbers were set to exactly what the crate exposed the day the extraction finished, with no headroom, so an addition has
to be argued for, and each raise since carries its argument below.

**A backend's API is the `Volume` trait it implements**, which is `cmdr-fs`'s promise rather than this crate's, so none
of its methods are counted here. Everything that IS counted exists because something outside has to build a share or ask
after one, and a new item should name which of three audiences it serves:

- **The protocol layer**, for `network/`'s discovery, share-listing, and mount passes: `build_smb_addr`, the two
  `try_list_shares_*` calls, `try_open_share`, the three `classify_*` / `is_auth_error` readers, `convert_shares`, and
  the four vocabulary types that cross IPC.
- **Constructing and asking after a share**, for `network/smb_upgrade.rs` and the debug window's diagnostics dashboard:
  `connect_smb_volume`, `SmbVolume` with `volume_id` / `session_state` / `diagnostics`, `SmbConnectionParams` with its
  five fields, and `ConnectionState`. `SmbVolume::new` is `pub(crate)`: outside, a share is dialed, which is the only
  way to get the connected client and tree it takes.
- **Placing a mount inside its share**, for the same upgrade path: `MountAnchor` with `at_share_root` and `new`, and
  `SmbVolume::exchange_mount_roots_with`. § "A mount anchored inside the share" has what they're for.

**The item bucket was raised 18 → 21 on 2026-09-09**, with David's say-so, for that third audience: four items in, one
back. The anchor concept is worth a bump because the app is exactly the caller that got it wrong. `smb_upgrade.rs` read
a mount path off `statfs` and passed it alone, which keyed a DFS sub-mount as one place and addressed it as another
(ERR-48RZX); a pair of adjacent parameters would let the same thing happen again at the same seam. `MountAnchor` makes
that unrepresentable, so it earns its place at the boundary the app crosses rather than only inside the crate. The
constructors come with it: `at_share_root` names the ordinary mount, which is nearly every call site, and `new` folds
the anchor to NFC, which no caller may skip. `exchange_mount_roots_with` replaced three narrower methods, so the
bookkeeping the registry needs crosses the boundary once.

**`try_open_share` is one root promise and one item past the extraction's numbers**, for the protocol-layer audience.
When a kernel mount can only say "not found", the app needs the server's own answer to "may this identity open this
share" (`apps/desktop/src-tauri/src/network/DETAILS.md` § "A share that says not found", ERR-SHUSC). The four
dispositions each lose: a facade would be the app building an `smb2::SmbClient` itself, which no app code does and which
is what this crate exists to stop; folding it into a listing call would TreeConnect `IPC$` rather than the share; gating
is no answer for production code; and the probe is the fix, so it can't be deleted. It takes `SmbConnectionParams`, so
the NFC fold and the `Guest` default come with it rather than being rebuilt by the caller.

Four public modules is the whole tree a host can name a path into: `connection`, `errors`, `types`, `volume`. Everything
under `volume` except the four items above is private, `SmbVolumeInner` included. `volume::testing` and
`detach_session_for_test` are `testing`-gated, and the counter skips a gated module and a gated item outright, so they
sit outside all three numbers — which is what keeps the app's SMB suites from becoming a reason to widen this, and also
means nothing measures the fixture module's own growth. Keep it a fixture module by reading § "Which side a test lives
on", not by watching a number.

## Layout

- **`volume/volume_impl.rs` holds the ENTIRE `impl Volume for SmbVolume`** because a trait impl can't be split across
  files, and NOTHING else. Every method body that runs to more than a few lines lives as an inherent `*_impl` method in
  the concern module that owns it, with `volume_impl.rs` left holding one-line delegators plus the capability flags,
  whose whole content is the reasoning in their doc comments. A new trait method goes here and delegates; don't try to
  move a trait method out.
- **The concerns are named modules, and each one answers a single sentence.** `paths` translates between the app's
  volume-relative paths and the share-relative ones smb2 speaks; `query` reads the share without changing it (listings,
  metadata, existence, space); `mutation` changes it and patches the listings that showed it; then `session`,
  `reconnect`, `state`, `read_stream` (a file read as chunks), `write_stream` (a stream written into a file),
  `random_reader` (one held-open `FileReader` read by offset, which the file viewer keeps for a whole session, #303),
  `scan`, `scan_pool`, `watcher`, `foreground_yield`, `slow_calls` (the rolled-up `warn` for a metadata call the server
  held over a second), `attributes` (Hidden and Locked by path), and the stateless `mapping`. Splitting further needs a
  responsibility you can name the same way; a line count is not one. `e5ea10d02` reverted four splits invented to
  satisfy the counter, and every one of them had widened a visibility or torn a struct from its trait impl to do it.
  These carry the same `pub(super)` the crate already used and leave no module reaching into another's internals.
- **`volume/foreground_yield.rs` answers "should a background transfer stand aside?" from the WORK, not the transport.**
  MTP has an explicit holder for its single scarce USB pipe; SMB frames just interleave over one connection, so there is
  nothing on the transport to count. A foreground LISTING is a scoped operation, though, so it holds a per-volume lease
  for its real duration, and the share stays busy for `TRANSFER_FOREGROUND_IDLE_THRESHOLD` after the last one ends. The
  two halves are composed once, in `cmdr_fs::volume::host::activity::volume_busy_for_user`. Scope is PER VOLUME on
  purpose, so browsing a local folder never slows a NAS copy. `CheckpointStream`'s auto-yield parks on these two
  functions and `SmbVolume`'s `Volume` foreground-yield methods delegate to them.
- **`SmbVolumeInner` is private to `volume/`**, and nothing outside it may name the type. What the app gets is
  `SmbVolume`, `SmbConnectionParams`, `ConnectionState`, and `connect_smb_volume`.

## Which side a test lives on

The suites split by what a cell ASSERTS, never by what it connects to. Both sides connect to the same containers through
`volume::testing`.

- **Here**, if the assertion is about this backend: the `Volume` contract against a real server, the byte path, the
  shared conformance promises, the retirement wiring, the watcher's archive-refresh routing, and every session-free unit
  case. § "The suites" below has the file-by-file map.
- **In the app**, if the assertion is about what the APP does with a share, and the cell sits beside the app code it
  asserts on rather than beside the backend. `apps/desktop/src-tauri/src/operations_host/backend_suites/` holds the
  transfer suites (`smb_transfer_safety_test.rs`, `smb_transfer_semantics_test.rs`, `smb_full_concurrency_test.rs`,
  `smb_stress_test.rs`, `smb_soak_test.rs`), the remote-archive cells (`smb_archive_integration_test.rs`), the
  local-source stream write (`smb_stream_write_integration_test.rs`), and the fixture wiring they share
  (`smb_test_support.rs`). `listing_host/tests/` holds the pane-close watcher regression
  (`smb_pane_close_watch_integration_test.rs`), and `file_system/volume/` holds the two cells where the index crate
  meets a real share (`smb_index_scan_test.rs`, `smb_media_fetch_integration_test.rs`).

**These are WHITE-BOX tests, and that is why they're here.** They build an `SmbVolumeInner` by struct literal, drive
`do_attempt_reconnect` directly, and read the client, tree, and scan pool out of the session. ❌ Don't widen the
backend's surface to keep one app-side: measured 2026-08-21, anything satisfying `.inner.client` / `.inner.tree` /
`.inner.scan_pool` is the whole struct. `volume::testing` is the opposite shape on purpose — it hands out fixtures and
three numbers, never state.

The app reaches `volume::testing` through its own `smb_test_support.rs`, which shadows `make_docker_volume` with one
that passes the app's real `VolumeHost`. That difference is the whole reason the wrapper exists: a backend test wants a
host that answers nothing, and an app test wants the listing cache and the activity tracker to see what the share
reports.

## SMB live-reconnect lifecycle

When a hot-path op hits an error that ends the session (§ "Which session a loss is about"), the funnel flips state to
`Disconnected` and `transition_to_disconnected` emits `volume-connection-changed { volumeId, state: "disconnected" }`.
The frontend reconnect manager listens for this event and runs a per-volume backoff cycle (timer-driven, calling the
`reconnect_smb_volume(volumeId)` Tauri command on each tick).

`SmbVolume::do_attempt_reconnect` is the single source of truth for re-establishing the session:

1. Acquires `reconnect_lock` (single-flight: concurrent FE-cycle and lazy-nav callers wait here).
2. If state is already `Direct`, returns Ok cheaply.
3. Tries `build_session()` with the cached `SmbConnectionParams` (the credentials that worked at original connect).
   Every wait in it is bounded (§ "deadline and keepalive defaults"), so a server that accepts TCP and never answers
   fails it in ~10 s and the lock is released; no caller queued on `reconnect_lock` waits forever.
4. If that fails with an auth error, calls `refresh_credentials_from_store` (which re-reads through
   `host.credentials()`, share-level first then server-level) and retries once with the fresh creds. On success, the new
   credentials replace the cached ones via `params.write()`.
5. On success: installs the new client + tree under the next session generation (bumped in the same critical section, §
   "Which session a loss is about"), restarts the watcher with `spawn_watcher` for that generation (the prior watcher is
   cancelled via `stop_watcher` first), then `transition_to_direct` flips state and emits
   `volume-connection-changed { state: "connected" }`. Doing the state flip last means observers wake up to a
   fully-installed session, and no call gets a lease on the new session before it.
6. On failure: state stays `Disconnected`. The FE backoff cycle decides whether to retry. **Auth give-up is special**:
   when the failure is an auth error and the refreshed store creds also fail (or there are none), `do_attempt_reconnect`
   emits `volume-connection-changed { state: "needs_credentials" }` before returning Err. `NeedsCredentials` is a
   transient signal for the frontend, not a `ConnectionState` variant: the backend state machine stays binary
   Direct/Disconnected, which is why `From<ConnectionState> for VolumeConnection` (in
   `crates/cmdr-smb/src/volume/state.rs`) only ever produces the other two and the give-up path names the variant
   directly. That `VolumeConnection` is the BACKEND-FACING enum, `cmdr_fs::volume::host::events::VolumeConnection`, not
   the frontend's wire enum: the backend hands it to `host.events().connection_changed(...)`, whose app-side answer is
   `events::volume_mapping::TauriVolumeEvents`, and that adapter alone widens it into `network::VolumeConnection`.
   Converting straight into a `network` type here is what welded the backend and `network/` into one module cycle for
   months; the invariant that keeps it apart, and how it hid from grep, is `network/CLAUDE.md` plus `network/DETAILS.md`
   § "The one edge that must not come back". The reconnect manager flips to `needs-auth`, stops the futile backoff, and
   FilePane shows a "Sign in" prompt (`SmbReauthView`) instead of the generic "unreachable" banner. The user signs in
   via `Volume::reconnect_with_credentials` (Tauri `reconnect_smb_volume_with_credentials`), which updates the in-memory
   params, runs the reconnect, and ❗ only once a session came up files the credentials as the sheet's Remember box
   (`SignIn::remember`) says (`session::refresh_remembered_credentials`), so the next reconnect is silent. If the new
   creds are also wrong, it re-emits `needs_credentials` — a bad retry re-prompts rather than dead-ending.
   - **Which entry**: the one `refresh_credentials_from_store` reads (the share's own, else the server's), with the
     credentials that actually connected. ❗ A server-level entry for ANOTHER account stays: every other share on the
     server signs in with it, so Remember on files this share its own entry instead.
   - **Remember off refreshes, ❌ never creates.** An empty store, or a server entry for another account, gets nothing
     (the sheet forgot the entry before it dialed). Remember on seeds the SERVER's entry in an empty store.
   - **Why the server's** (#530): it's what every sign-in sheet writes, and what "Forget saved password" and Remember
     off take back. A share-scoped seed outlived both and, read first, came back on every sign-in. The app's
     `has_server_secret` / `forget_server_secret` cover a share's own entry too.
   - **Why dial first** (#530): writing before the dial let a typo replace the server-level password every share falls
     back to, and wrote one back even with Remember off.

Credentials are kept in memory for the lifetime of the `SmbVolume` (no security concern: they're already in the
process's address space for every smb2 call). Only re-pulled from the secret store on auth failure, in case the user
updated them.

### Which session a loss is about

A late error from a connection a reconnect already replaced must never take the new session down: every sibling copy on
it would then take an outage it never had (the `ERR-VEZUB` shape, #489). So the state machine knows WHICH session a loss
is about.

- **One predicate, `state::ends_the_session`**, by `smb2::Error::kind()` alone: `ConnectionLost`, `SessionExpired`,
  `SignatureInvalid`. The main session's funnel, the scan pool's members (`is_pool_member_dead`), and a write's date
  stamp (`stamping.rs::stamp_taken`) all ask it; ❌ no other `matches!` on those kinds outside `mapping.rs`. Since smb2
  0.32.0 the request whose own send broke gets `SendFailed` (kind `ConnectionLost`; every other waiter still gets
  `Disconnected`), and `Io` on an established connection means a LOCAL failure. So ❌ no errno arm: an `EACCES` or
  `ENOSPC` must never end the session, and `map_smb_error` keeps a local `Io`'s errno in `IoError::raw_os_error`.
- **A session generation.** `SessionCell` packs the state and the installed session's generation in one `AtomicU64`.
  `do_attempt_reconnect` bumps it (`advance`) in the same critical section that installs the rebuilt `MainSession`,
  which carries it. `clone_session` hands out a `SessionLease { tree, conn, generation }`, and every error goes through
  the funnel with the generation ITS call ran on (`handle_smb_result(op, path, generation, result)`), ❌ never a fresh
  read of the current one, which would silently disable the guard. The installed generation is also what
  `Volume::connection_generation` reports (`SessionCell::generation`), so a transfer whose call failed on an older
  session sees the share live on a newer one and runs the call again (`cmdr-operations`'s `transfer/volume_outage.rs`).
  A reconnect that finds the session already `Direct` installs nothing and moves nothing.
- **One guarded transition.** `transition_to_disconnected(ran_on)` is a CAS (`SessionCell::lose`) that flips only while
  `ran_on` is still installed, so a reconnect that installs between a check and the flip can't be overwritten.
  `transition_to_direct(generation)` is the same for the way up; `on_unmount` stores `Disconnected` whichever session is
  installed (`shut_down`: none will serve again).
- **Per-site generation**: the streaming-read producer and the random reader hold the share (`Arc<SmbVolumeInner>`) and
  their lease's generation; the watcher records the generation it was spawned for and its death marks only that one
  (`spawn_watcher_death_reconnect(share, generation)`; a watcher outliving a reconnect marks nothing, and the loop finds
  the share `Direct` and stops). The watcher doesn't classify its error by kind: any death of its dedicated session
  counts as the link failing, as before. The scan pool's members never touch the main state, so they carry no
  generation. Probes whose failure only means "no" (`exists`, the rename's directory and destination stats) report
  nothing to the funnel.
- Pinned by `state_test.rs` (the cell, the predicate) and `reconnect_test.rs` (`a_late_loss_from_a_replaced_session_*`,
  `a_send_failure_and_a_disconnect_*`, `the_reported_generation_moves_only_when_a_session_is_installed`), through both
  `handle_smb_result` and `note_smb_error`.

### Backend-autonomous reconnect and index resume

The FE reconnect manager only runs its backoff while a `FilePane` subscribes to the volume, so before this a background
disconnect (no pane open, or a restart) left an enabled NAS index dark until the user manually re-enabled. Two backend
hooks close that, both funneling through the ONE reconnect path (`do_attempt_reconnect`):

- **`spawn_watcher_death_reconnect(share, generation)`** (in `crates/cmdr-smb/src/volume/reconnect.rs`, kicked from the
  watcher's fatal-error exit). The watcher runs on its own dedicated smb2 session; that session erroring proves the
  server connection broke. A background disconnect may not have touched the MAIN session yet, so it can still read
  `Direct` — meaning `do_attempt_reconnect` would no-op. So the kick FIRST marks the main session the watcher was
  spawned for `Disconnected` (a no-op once a reconnect replaced it), then drives `do_attempt_reconnect` on a bounded,
  growing backoff (`WATCHER_DEATH_RECONNECT_BACKOFF`: ~6 tries over ~4 min, then gives up quietly — never hammering a
  truly-down server). It re-upgrades its share handle each iteration (a retirement or an unmount can land inside any
  sleep) and stops early on unmount, on a race back to `Direct` (an FE reconnect won), or on an auth failure
  (`PermissionDenied` — the FE "Sign in" flow owns that; retrying risks locking the account). Single-flight
  `reconnect_lock` coalesces it with any concurrent FE reconnect.
- **`indexing::resume_smb_index_if_enabled(volume_id)`** fires at every session-install success — `do_attempt_reconnect`
  (in-place reconnect), `register_smb_volume` (launch/auto-upgrade), and `try_smb_upgrade` (manual "Connect directly").
  It's fire-and-forget (spawns, so it never starts the async indexer under `reconnect_lock` / a registry lock), a no-op
  if the index is already active, and gated on the PERSISTED per-volume state — resume ONLY when the share carries the
  user's enable AND they haven't turned indexing off (the sticky `user_disabled` marker; `disable_drive_index` keeps the
  DB for fast re-enable but records intent). Registering flows through the indexing lifecycle registration bus, so the
  media scheduler resumes enrichment with no scheduler changes. The resumed index loads Stale (we weren't watching while
  disconnected); a rescan restores Fresh. Canonical detail lives in `indexing/DETAILS.md` § "SMB indexing and the
  freshness model"; this bullet is the volume-side trigger map.

## Scanning: where the copy scan asks whether to carry on

`scan_recursive` threads a `cmdr_fs::volume::ScanBoundary` and asks it at three places: once before the top-level stat,
`dir()` before every directory listing, and `file()` on every leaf. It walks in share-relative paths but hands the
boundary each entry under the CALLER's path (`caller_path`, carried down beside `smb_path`), which is what the transfer
tree's recorder places entries by (`crates/cmdr-fs/DETAILS.md` § "The recorder"). That is what a person's Cancel and
Pause reach, and before it existed the whole batch scan was one uninterruptible call — a wrong-share scan over a NAS
whose disks had spun down ran to completion whatever the user pressed.

❗ **`dir()` goes BEFORE the listing, never after.** A listing is the round trip, seconds of it on a cold share, so a
boundary on its far side is a Cancel the user waits out.

❌ **Don't ask inside the pipelined top-level stat drain.** Those futures each hold a cloned `Connection`, and parking
mid-drain would hold every one of them idle for as long as a person is thinking. The sizes it resolves are reported to
the boundary once the drain is over; the drain itself is O(top-level paths) and pipelined, so the wait it can add is
bounded, and the recursion after it is the long part.

A stopped scan comes back as `VolumeError::Cancelled`, ❌ never a short total — the shared contract, with the reasoning
in `crates/cmdr-fs/DETAILS.md` § "`ScanBoundary`". `conformance::assert_batch_scan_stops_when_told` and
`assert_batch_scan_asks_inside_the_walk` both run against the Docker share (`volume/conformance_test.rs`), because a
walk that ignores its boundary still returns the right numbers and nothing else in the suite would notice.

**What the batch scan owns, and what it borrows.** The oracle short-circuit is this backend's: the watcher is what earns
an `authoritative_listing` shortcut, so only a backend with one can take it. The conflict matcher and the batch fold are
`cmdr_fs::volume::scan_walk`'s, deliberately, so every backend hands the conflict dialog the same shape rather than each
growing its own near-miss.

## SMB scan-connection pool

Canonical home for the per-scan connection pool (`crates/cmdr-smb/src/volume/scan_pool.rs`).

A cold NAS index scan is metadata-read-bound, but the ceiling is **per-connection serialization in the server's ksmbd**,
not the disks: one SMB connection can't drive the server's read queue deep enough regardless of the SMB in-flight
window. NAS-side measurement (2026-07-22) held total in-flight depth constant and varied only the TCP connection count;
4 connections raised read IOPS ~1.75× at flat disk latency and lifted cold client throughput ~3.8×. Evidence:
`~/projects-git/vdavid/smb2/docs/benchmark-findings.md` §§ "Directory-listing throughput probe" and "NAS-side ground
truth" — link, don't restate.

So background bulk work opens `SCAN_POOL_SIZE` (4) EXTRA smb2 sessions (separate TCP connections) for its duration and
spreads across them; the pane's own session keeps serving browsing. Two users today: the index scan's directory
listings, and media enrichment's parallel prefetch reads.

- **Lifecycle, refcounted.** Opened LAZILY on `Volume::begin_scan_session` (`SmbVolume::open_scan_pool`, idempotent),
  closed when the LAST concurrent scan session ends (`scan_session_refs`, a saturating counter — an index rescan and an
  enrichment pass can overlap, and either one's `end_scan_session` must not tear the pool out from under the other);
  `on_unmount` tears it down synchronously regardless (`close_scan_pool_sync` flips the pool's `closed` flag so
  reconnect loops bail — a member must not keep walking an unmounted volume). `on_superseded` does NOT close it (an
  in-flight scan is still drawing from it); it only stops a retired volume opening a NEW one. Steady-state footprint
  between scans is unchanged (`scan_pool: RwLock<Option<Arc<ScanPool>>>` is `None`). The index-scan lifecycle brackets
  the spawned walk task (`indexing/lifecycle/manager/network_scan.rs`); the media pass brackets via a drop-guard in its
  scheduler — both run `end` on every outcome.
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
- **Failure handling.** A listing failing with an error that ends its session (`state::ends_the_session`) is retried on
  a sibling member, the dead member is dropped, and a single-flight background task reconnects it (`build_session`,
  bounded growing backoff `POOL_MEMBER_RECONNECT_BACKOFF`; gives up on auth — the MAIN session owns the
  credential-refresh / `needs_credentials` flow). A dead member NEVER transitions the main volume's connection state. A
  per-directory error (permission, not-found) is the same on any connection, so it's surfaced immediately, not retried.
  If every member is momentarily dead, the listing falls back to the main session, which keeps the scan progressing and,
  if it too is dead, yields the `DeviceDisconnected` the scanner's terminal-disconnect path expects. Members open
  STAGGERED at pool open; a rejected Nth session (server session cap) just means the pool runs with fewer.
- **Params are a snapshot.** If the main session refreshes credentials mid-scan (password change), members failing auth
  give up and listings fall back to the main session (documented degradation, not a correctness issue).
- **Reads: compound-only on members** (`open_read_stream_for_scan_impl`). Media enrichment's prefetch reads small HINTED
  files from pool members via the 1-RTT `read_file_compound` (dead member ⇒ sibling retry, exactly like a listing; size
  drift or a too-large file ⇒ main-session streaming). Members deliberately never serve STREAMING reads: a member dying
  mid-stream would surface as a transport error the pool can't transparently retry for the consumer — the main session,
  with its reconnect machinery and connection-state signaling, owns streaming. The pool's ceiling is a whole `max_read`,
  not the foreground read's `quick_read_limit()`: a member never streams, so it never measures its link and that limit
  stays at one chunk there, while enrichment fetches whole photos (1–8 MiB typically). That ceiling would push nearly
  all of them onto the main session the pane browses through. Its reasons (progress, not queuing ahead of the pane's
  listings) don't hold on a member.

## Listing a batch at a time

Every listing (the pane's, the scan's, and the pool members') goes through `query.rs::read_listing`, which reads smb2's
`DirectoryReader` one QUERY_DIRECTORY answer at a time (about 64 KiB of entries, a few hundred names). Three things fall
out of it, and the wire traffic is the same as smb2's own `list_directory`, which collects through the same reader:

- **Progress per batch.** `on_progress` ticks once per batch with the running tally, so the pane's "Loaded N entries"
  line climbs while a slow share lists a big folder. Per batch is per round trip, so it can't flood the IPC layer.
- **Cancel between batches.** A fired token ends the listing with `VolumeError::Cancelled` at the next batch boundary,
  so navigating away from a 100,000-entry folder stops reading it. The reader is closed on `host.runtime()` rather than
  awaited: its CLOSE has to wait for the batch in flight (smb2 orders them), and the caller has already moved on.
- **Mapped as it lands.** Each batch becomes `FileEntry` values before the next one is asked for, so the raw smb2
  entries of a big folder are never held whole.

Pinned by `smb_integration_a_listing_reports_progress_per_server_batch` and
`smb_integration_a_cancelled_listing_stops_between_batches`, both seeding 600 long names so the folder spans several
batches.

## SMB archive push-refresh

The recursive share watcher already refreshes the DIRECTORY listing showing a changed `.zip` (its new size/mtime). On
top of that, `process_event_batch`'s Modified and RenamedNewName handlers call
`maybe_refresh_archive_listings(volume_id, entry_path)`: when `entry_path`'s name is a supported archive
(`archive::has_supported_archive_extension`, the single-source predicate `format_for_name` backs), it fires the same
`refresh_archive_listings` the local `archive::watch` fires, pushing an out-of-band edit of the `.zip` to any open
archive-INNER listing.

Why this is the whole fix, cheaply:

- **Same consumer, same key.** `refresh_archive_listings` scans the listing cache for keys at/inside the archive path
  and re-reads them; `volume_id` here is the parent DRIVE id, which is exactly what archive listings key on, so no
  rekeying. It's a no-op when the path isn't an archive or no inner listing is open, and the watcher already runs for
  the whole volume lifetime — so the only added cost is a re-parse when a `.zip` actually changes AND an inner pane is
  open.
- **`entry_path` is already the cache-facing path.** It's the watcher's `to_display_path` result: the anchor stripped
  and the mount root joined on, in the server's own bytes, like every other cache-facing path in
  `crates/cmdr-smb/src/volume/watcher.rs`. Passing the raw share-relative event filename would miss the cache.
- **Fires independent of the stat.** The refresh runs even when the pre-refresh `get_metadata` fails (a mid-write,
  truncated `.zip`): `refresh_archive_listings` keeps the previous inner listing on an unreadable parse rather than
  blanking the pane, and the next change event retries.
- **NOT a freshness claim.** This is a visible-listing UX nicety, a SEPARATE consumer from the write-op fresh-listing
  oracle. `ArchiveVolume::listing_watch_coverage` stays `None` for a remote parent regardless (the SMB watcher is lossy
  under load, so the oracle must keep re-reading pre-flight scans honestly). The remote-archive freshness decision and
  the guardrail test are in `crates/cmdr-archive/src/watch/DETAILS.md` § "remote archives have NO live watch". MTP keeps
  manual refresh (F5) as its contract.

Tests, split along the seam: the ROUTING (which events reach `refresh_archive_listings`, with what path) is
`crates/cmdr-smb/src/volume/watcher/archive_refresh_test.rs`, a `RecordingListings` assertion with no archive and no
filesystem in it; what a refresh DOES to the cache is
`apps/desktop/src-tauri/src/listing_host/backends.rs::the_archive_refresh_re_reads_the_listings_under_its_path`.

## SMB names are opaque bytes

An SMB name is a byte string the server matches exactly: no Unicode folding, no case folding. One directory can hold its
own name composed (NFC) and its children's composed or decomposed (NFD) in any mix, and case is significant too.
Measured against David's QNAP (`naspi`) with the `smb2` CLI on 2026-09-22: in a Google Takeout album whose directory is
NFC and whose `retusált -185.jpg` is NFD, only the `(NFC directory, NFD file)` spelling opened; the all-NFC, all-NFD,
and `(NFD, NFC)` spellings answered `STATUS_OBJECT_NAME_NOT_FOUND` or `STATUS_OBJECT_PATH_NOT_FOUND`, and so did the
right bytes in the wrong case. The fixture's Samba stores names as sent too (verified on the `smb-consumer-guest`
container, `unicode_names_integration_test.rs`, 2026-09-23). So "SMB servers use form X" is false (ERR-VETBX). The rule,
in three parts: **use the bytes the server gave you; a foreign path resolves once, where it enters; a name Cmdr creates
goes out composed.** Scale on that NAS (a recursive listing of `Takeout/Google Photos`, 497,120 entries, 2026-09-22):
all 235 directory names NFC or ASCII, 712 file names NFD across six albums, and no two names colliding modulo form and
case, so a look-alike twin is rare but not impossible.

- **Server-derived paths go out byte-for-byte.** `to_smb_path` never normalizes, and `list_directory_impl` builds each
  entry's path from the listed directory's own path plus the entry's raw name, so every component of a path that came
  out of a listing is already exact.
- **The watcher keys on the same bytes.** Its event paths are the server's spelling, which is what a pane's path for the
  same directory carries, and the listing cache compares the two byte-for-byte (`ListingPath`). A fold on either side
  misses every directory not already in the folded form, so an outside change never reaches the open pane.
- **The one fold on a path is the mount anchor** (`MountAnchor::new`), because it never came out of a listing; its doc
  comment says why NFC.
- **Share and server names are a different namespace** (TreeConnect), and `SmbConnectionParams::new` keeps folding them.
- **A foreign path** (typed, restored, carried over from the macOS kernel mount, which decomposes every name on
  `readdir`) is not the server's spelling, and no single normalization spells a mixed-form path. Every `Volume` call
  here still sends it as given and misses; it becomes exact only through `Volume::find_stored_spelling`, § "Resolving a
  foreign path".
- **A NEW name Cmdr creates goes out composed** (`composes_new_names` answers `true`): a copy's free name, a new folder
  or file, a rename's target (bulk too), a new archive, a ` (N)` pick. The caller that knows it's creating respells it
  through `Volume::spell_new_name`; this crate never does, because a name that addresses an EXISTING entry (an
  overwrite, a merge, a same-share move) must keep the stored bytes or it plants a twin. Why composed: a decomposed name
  on a share is one Finder over the kernel mount (composes on lookup), Windows, and Linux clients list and can't open,
  and every name older Cmdr builds wrote to a share is composed. A policy, one override to revert:
  `crates/cmdr-operations/DETAILS.md` § "Look-alike names".
- **A name the share holds in another spelling is taken, never free**: a write that asked in the wrong spelling would
  stand a second, identical-looking entry beside it. The app's write layer finds it with one listing after a byte-exact
  miss (`crates/cmdr-operations/DETAILS.md` § "Look-alike names"). Docker-pinned by
  `operations_host/backend_suites/smb_look_alike_test.rs`.

### Resolving a foreign path

`volume/spelling.rs` implements `Volume::find_stored_spelling`: where a foreign path is, spelled the server's way.
`Ok(None)` when the path opens as given or nothing matches; `VolumeError::AmbiguousName` when it can't tell which.

- **Per component, localized by the error code.** One `stat` of the whole path: it opening ends the search, and
  `STATUS_OBJECT_NAME_NOT_FOUND` says only the leaf is wrong. `…_PATH_NOT_FOUND` steps up one ancestor at a time until
  one opens or names itself missing, bottom-up because the wrong component usually sits near the leaf (an accented album
  one level up). The first wrong component's parent is listed, matched, substituted, and the walk re-probes from there,
  so a path with several wrong components costs one listing each. The mount anchor is never resolved.
- **Listing: the oracle first.** `authoritative_listing` answers free when a pane shows the parent under a live watch;
  otherwise a real `list_directory`. A 58,440-entry directory took 17.2 s over Tailscale (spec evidence, 2026-09-22), so
  the resolve checks its `CancellationToken` between round trips and the pane seam runs it inside the cancelable listing
  task; one listing itself is a single call.
- **Matching: NFC-and-lowercase fold (`cmdr_fs::name_fold`, the app's one such key), exact wins.** An entry with the
  wanted bytes wins even beside look-alikes, because it IS what the caller named. One folded match is the answer. Two or
  more with no exact one is `AmbiguousName`: picking one is a wrong-file hazard on the next delete or overwrite.
- **Corrections are remembered per share** (`SpellingCache`, on `SmbVolumeInner`, keyed by share-relative parent and the
  foreign bytes, capped at 256, oldest out). A remembered one is only a guess: the next probe checks it, a miss drops it
  and lists again (`a_stale_remembered_spelling_heals`), and the watcher forgets a directory's corrections on any event
  there, all of them on `STATUS_NOTIFY_ENUM_DIR`. Keyed on the foreign BYTES, not the fold, so an exact path can never
  be steered onto a remembered look-alike.
- **Decision: a resolve, never a fold inside the operations.** **Why:** a blanket NFC fold in `to_smb_path` makes every
  NFD name unreachable (ERR-VETBX: listed, sized, and impossible to open), and a resolve built into `get_metadata` /
  `list_directory` / `delete` can't tell "another spelling of this path" from "a different entry sharing its folded
  name, the named one having vanished". The transfer layer's existence probes and post-move `exists` checks, and a
  delete walker's listings, all hold server-derived paths where a miss means gone. So the resolve is its own call, made
  only at the app's seams where a foreign path enters: the pane's directory open, a backend swap's respell, and a Finder
  drop or paste (`crates/cmdr-listing/DETAILS.md` § "A pane path the volume stores another way").
- **Rejected: a normalization retry ladder** (as given, then all-NFC, then all-NFD). Cheap, but a path with n accented
  components has 2ⁿ spellings on the server and the ladder tries three, none mixed, so `(NFC directory, NFD leaf)` from
  an all-NFD path never resolves; it can't see case; and on a share holding twins it opens whichever it reaches first,
  which for a delete or an overwrite is a wrong-file write.
- **The fixture folds case, David's QNAP doesn't.** The `smb-consumer-guest` Samba runs the default
  `case sensitive = auto`, so a case-only difference opens as given there; the Docker cells mix case with Unicode form
  to reach the fold (verified 2026-09-23, `a_foreign_path_in_another_case_reaches_its_directory`). Case-only matching is
  pinned by `spelling_test.rs`.

## A mount anchored inside the share

A mount is not always the share ROOT. macOS follows a DFS referral by making a SECOND mount underneath the namespace
root: `smb://example.com/SYSVOL` leaves `/Volumes/SYSVOL/example.com` beside `/Volumes/SYSVOL`, carrying
`//user@example.com/SYSVOL/example.com` in its `f_mntfromname`. A subdirectory mount (`mount_smbfs //server/share/sub`,
or `mount -t cifs` on Linux) makes the identical shape on purpose. Both address a DIRECTORY inside the share.

**A share is ONE path segment, and the anchor is everything below it.** `MountAnchor` (`volume/mod.rs`) pairs a mount
path with the `share_root` that mount sits at inside the share: `/`-separated, no leading or trailing separator, empty
for the ordinary mount. The two travel as one value because a caller holding one always holds the other, and splitting
them is what lets a mount be keyed as one place and addressed as another. Reported as ERR-48RZX: `SYSVOL/example.com`
reached TreeConnect as a share name, the server answered `STATUS_BAD_NETWORK_NAME`, so the share stayed on the slow
kernel mount, picked up a second volume ID, and warned the user about a share already connected directly one level up.
`MountAnchor::new` NFC-folds the anchor in one place, the one fold on a path in this crate (§ "SMB names are opaque
bytes").

**The anchor lives on the INSTANCE** (`SmbVolume::share_root`), not on the shared inner: it is a fact about one mount
root, and one share can be mounted at roots that sit at different depths.

**Both path directions carry it** (`paths.rs`). `to_smb_path` joins the anchor onto a mount-relative path on the way to
the wire; `to_display_path` takes it back off. Stripping matches WHOLE COMPONENTS, so a sibling that merely shares a
name prefix (`domain.old` beside `domain`) can't be mis-stripped onto a directory nobody mounted. A path that isn't
under the anchor is left alone, which is what an unanchored mount does with every path anyway.

**The watcher carries it too, and it is the one place the anchor also FILTERS.** The change watch is a single recursive
`CHANGE_NOTIFY` on the share root shared by every mount of that share, so its event paths are share-relative. An
anchored mount therefore hears about the whole share: `watcher.rs` strips the anchor to get the mount-relative path the
listing cache is keyed on, and skips an event that isn't under the anchor at all rather than joining it on. Joining it
on names a path no pane has open, which leaves the listing that did change stale while invalidating one that doesn't
exist. Nothing else about the event path changes: it keeps the server's bytes. One implementation of the rule,
`paths::below_share_root`, serves both this and `to_display_path`: two copies would drift, and the two directions
disagreeing is exactly what patches a cache key nothing is watching.

**An anchored mount is the SAME volume as its share.** The ID keys on `(server, port, share)` and leaves the anchor out,
so the nested mount and the namespace root share one ID, one session, one index, and one set of saved paths, and a
redundant upgrade attempt short-circuits on `is_already_direct` rather than opening a second connection. Where the
anchor is read off the mount: `apps/desktop/src-tauri/src/network/DETAILS.md`. How a mount source is parsed into share
plus subpath, on both platform twins: `apps/desktop/src-tauri/src/volumes/DETAILS.md`.

## A share that is a DFS namespace root

An Active Directory domain normally publishes a share as `\\<domain>\<namespace>`, which is MS-DFSC § 2.2.1.4's
preferred form and is **not a share on the server answering that name**: `TREE_CONNECT` on it is refused with
`STATUS_BAD_NETWORK_NAME`, and the share enumeration that would have found it comes back with zero disk shares. macOS
mounts it happily (`smbutil statshares -a` shows `DFS_SHARE TRUE` with `SERVER_NAME` still the domain), so the volume
exists and works, and before smb2 0.22 every direct-connect attempt on it failed and left it on the kernel mount at a
fraction of the speed. Reported as ERR-HYPZG, on a real corporate domain.

smb2 resolves it: on that refusal, from a server that advertised `SMB2_GLOBAL_CAP_DFS`, it asks for a root referral and
connects whichever target comes back, on whatever server that turns out to be.

**`dfs_enabled` is on wherever we open a share, and off where we only enumerate.** `volume::session::build_session`,
`volume::watcher`, and `connection::try_open_share` all open one and all have it on; the watcher especially, or a
namespace-backed volume would list fine and never notice a change. The two `try_list_shares_*` paths never leave `IPC$`,
so there is no share name for a referral to resolve and it stays off. It costs nothing where it's on: the referral only
goes out after a TreeConnect has already been refused.

**This composes with the anchor above rather than competing with it.** A root referral answers with the namespace root's
own targets, so the tree lands where the namespace root is, and walking into a link underneath it earns
`STATUS_PATH_NOT_COVERED`, which smb2's older reactive path resolves. The share stays ONE path segment either way, which
is the invariant the anchor is built on.

## Re-rooting a share

macOS mounts one share at several roots (`/Volumes/naspi` AND `/Volumes/naspi-1`) and they all derive one volume ID, so
the registry tracks the SET of roots and promotes a survivor when the active one dies (`volume/DETAILS.md` § "A volume
ID owns a set of mount roots"). `SmbVolume` implements `Volume::rerooted`, because the OS mount is only an addressing
prefix here: Cmdr's own I/O rides the smb2 session.

**Shape**: `SmbVolume` is a thin instance over an `Arc<SmbVolumeInner>`. The instance holds what belongs to ONE mount
root (`name`, `mount_path`, `mount_root_gone`); the inner holds the session and everything scoped to the SHARE (client,
tree, params, connection state, watcher handle, scan pool, the retirement flag, refcounts). `rerooted` is therefore one
allocation over the same inner: no re-auth, no transport rebuild, no session churn.

**Why the instance's root is immutable**: `Volume::root()` hands out a `&Path`, with ~115 call sites. Making the root
interior-mutable to reroot in place would either change that signature across the codebase or hand out a borrow that can
change under the caller. A new instance moves the root without either.

**A promotion looks the anchor up per root.** Two roots of one share need not sit at the same depth inside it
(`/Volumes/SYSVOL` is the share root while `/Volumes/SYSVOL/example.com` is a directory in; § "A mount anchored inside
the share"), and nothing about a path reveals its anchor. So `SmbVolumeInner::share_root_by_mount` records where each
known root sits, the app fills it in as it registers each root (`SmbVolume::note_mount_root`), and `rerooted` reads the
answer there:

- An UNANCHORED instance promotes to any root, recorded or not, treating an unknown one as another mount of the share
  root. That's every ordinary mount, and roots reach the registry from places that never touch the SMB upgrade (the
  FSEvents watcher registers a fallback volume too), so refusing an unrecorded root would strand a share on a dead mount
  in the common case.
- An ANCHORED instance refuses a root nobody recorded, returning `None`. Applying its own anchor to a root that may sit
  somewhere else addresses a real path on the share that nobody asked for. The registry reads the `None` as
  `Promotion::BackendCantReroot` and leaves the volume where it is (`RootRemoval::ActiveRootStranded`): still browsable
  over smb2, no longer claiming its paths are OS-openable.

A reconnect builds a whole new instance while the registry keeps the roots it already had for that ID, so
`register_replacing_predecessor` trades the roots in both directions before anyone is retired
(`SmbVolume::exchange_mount_roots_with`: the newcomer adopts every root the incumbent knew, and the incumbent learns
where the newcomer's own root sits). Both directions ship together because which instance survives is the registry's
call. Without it, whichever one it keeps would refuse a promotion the other would have allowed.

**The two instances overlap, briefly and by design.** For the moment between `rerooted` returning and the registry
dropping the old one, both address the same live session — and whoever grabbed the old one earlier (a running transfer,
an open viewer stream) keeps using it at the root it was handed, which is correct: its paths were built there. ❌ A
promotion must therefore NEVER call `on_superseded` or `on_unmount` on the old instance. Both act on the SHARED inner:
`on_superseded` stops the watcher and quiets the id, `on_unmount` drops the session outright — the teardown that once
killed a live NAS copy (§ "Supersede vs. unmount"). `manager/roots.rs::promote_to_best_root` swaps the volume without
either hook, which is what makes this safe.

**Honesty about the mount** (`mount_root_gone`): when the registry proves an instance's root is gone and has no live
sibling to promote to, it calls `Volume::note_root_mount_gone` (`manager/roots.rs::tell_volume_if_its_root_is_dead`, run
after every move of the active seat), and `paths_are_os_visible()` answers `false` from then on. Cmdr keeps browsing the
share over smb2 — which is why the bug was invisible — but a `file://` URL under a dead mount opens nowhere, so the
drag-out and Quick Look paths have to stop being told it does. A PUSH rather than a pull: the volume can't ask (nothing
may probe a mount), and a backend reaching into the registry from a capability flag would invert the dependency and risk
the registry's own lock. The flag lives on the INSTANCE, since it's a fact about one mount root, so a promotion onto a
live root starts honest again. ❌ Still not the same question as `supports_local_fs_access()`; see the Decision below.

**Known gap**: nothing detects a mount that dies WITHOUT an unmount event, and a NAS dropping off the network is exactly
that (macOS leaves the mount wedged). Probing is banned, and a direct `SmbVolume`'s lost connection carries no errno
(`map_smb_error` maps it to `DeviceDisconnected`), so `volume::note_root_failure` can't fire for one either. Until some
evidence arrives on its own, an SMB volume on a wedged mount still claims OS visibility.

**The watcher follows the active root.** It belongs to the session, not to one mount, and the absolute paths its
notifications carry decide which cached listing they patch. So `SmbVolumeInner::active_mount_path` (a std
`RwLock<PathBuf>`, shared with the watcher task and re-read once per event batch) is updated by a reroot; a watcher
pinned to the old root would keep feeding paths that no longer name anything.

## Dates on copies

The contract: `crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "Copies keep the source's metadata".

- **Source half: both foreground read paths report the server's `LastWriteTime` and `CreationTime`** (the streamed
  download and the hinted one-frame compound read), from the read's own CREATE response: `FileDownload::info()` on the
  streamed path (`ChannelReadStream::with_metadata`), `read_file_compound_sized_with_info` on the compound one
  (`InlineReadStream::with_created`). No extra frame; the wire-shape cells pin a hinted read at `(1, 3)`, so a stat
  creeping back in shows there as `(2, 8)`.
- **The scan pool's prefetch stays dateless**: enrichment never copies those bytes.
- **Destination half: `write_from_stream_impl` stamps `LastWriteTime` and `CreationTime`**, both in the one
  `FILE_BASIC_INFORMATION` (`stamping.rs::source_times`; access and change times go as 0, "don't change"), so the
  creation date costs no frame. It asks the stream for `CREATED` and the flags (§ "Hidden and Locked on copies"); a
  share has no mode, and xattrs wait for named streams. Best effort: a refused stamp is a `warn!` and the copy succeeds,
  and the write reports the creation date and Hidden refused (`stamping.rs::stamped`) for the copy's loss note. ❗ A
  stamp that meets the session ending fails the write with `DeviceDisconnected` through the funnel
  (`stamping.rs::stamp_taken`): a write never reports `Ok` past its own last step seeing the connection die, or the
  transfer lands the file or starts the next one on a dead session. The compound path deletes nothing on that error (its
  CLOSE already ran, at what may be the user's real name); the transfer settles the create by comparing bytes and puts
  the source's dates on by path (`transfer/interrupted_landing.rs::settle_unanswered_create`). The streaming path cleans
  up as for a failed chunk. Gotcha/Why: the server rewrites a file's date when the handle that WROTE it closes, so the
  moment matters. The streaming writer stamps its own handle before `finish()` (`stamp_writer`, which
  `FileWriter::set_times` orders after the pending writes); the one-frame compound write has already closed its handle,
  so it stamps by path right after, one more compound frame (CREATE+SET_INFO+CLOSE). ❌ Never stamp by path while a
  writer is still open: its close wins. A one-shot write to the user's real name (unstaged, `write_is_single_shot`)
  shows the server's dates for that one frame.
- **The landing rename keeps `CreationTime`** (verified on the Docker guest share, Samba 4.20.6 with
  `store dos attributes = yes`,
  `conformance_test.rs::smb_integration_a_copy_keeps_the_creation_date_per_the_shared_contract`, 2026-10-11):
  `FileRenameInformation` doesn't touch `FILE_BASIC_INFORMATION`, and Samba keeps birth in its `user.DOSATTRIB` xattr,
  which the rename carries. A share with `store dos attributes = no` can't keep it; QNAP's answer is unverified.
- **Folding the one-frame write's stamp into its compound would save a round trip per small file, and Samba allows it**
  (verified on the Docker guest share, Samba 4.20.6, 2026-10-11: CREATE+WRITE+FLUSH+SET_INFO+CLOSE in one compound, five
  of five files listed the SET_INFO's dates, not the CLOSE's). Not adopted as such: a FLUSH must end its chain (Windows
  refuses one mid-chain, and Samba before 4.18 crashes on it). Since smb2 0.31.1 the CLOSE is already its own frame, so
  the natural fold is a SET_INFO+CLOSE second frame on the open handle, which needs `smb2` to take `FileTimes` in
  `write_file_compound`. Until then the stamp is a third frame.
- **Pinned by** `conformance_test.rs`: `smb_integration_a_copy_keeps_the_source_date_per_the_shared_contract` and
  `smb_integration_a_copy_keeps_the_creation_date_per_the_shared_contract` (the shared assertions, compound path,
  through a staging-shaped temp and its rename), `smb_integration_a_streamed_write_keeps_the_source_date` (the writer
  path, SMB to SMB, both dates), and `smb_integration_a_read_stream_reports_the_listed_date_on_both_read_paths` (the
  read half on a file aged inside the fixture container with `touch -d`, both dates on both paths);
  `wire_shape_integration_test.rs::smb_integration_a_dated_single_shot_write_adds_one_frame_for_its_date` counts the
  stamp's frame. Through the engine: `smb_transfer_semantics_test.rs`'s `..._keeps_the_creation_dates` cells.
- **Folders: `apply_metadata` stamps `CreationTime` and `LastWriteTime` by path** (`mutation.rs::set_times_impl`,
  `Tree::set_times`, which opens with no create options so a directory opens like a file; one frame for both), then
  patches the parent listing with a `Modified`. The copy engine calls it on each folder it created once the contents
  landed. Mode and xattrs answer `NotApplicable`. Pinned by `smb_integration_apply_metadata_dates_a_folder`, the
  creation-date conformance cell, and the engine's `smb_integration_folders_copied_*` cells.

## Hidden and Locked on copies

The contract: `crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "Copies keep the source's metadata". Finder's
Locked is the DOS `READONLY` attribute, its Hidden is `HIDDEN` (`mapping.rs`: `user_flags_of`, `attributes_of`).
Append-only and no-dump have no attribute and drop.

- **Reads ride the CREATE response a read already gets**: `FileInfo.attributes` on both read paths (`read_stream.rs`),
  so a file's flags cost no frame. A folder has no read, so `folder_metadata` spends one `stat` (`attributes.rs`); the
  app asks only when its destination keeps a folder's flags.
- **A dotfile's Hidden is its name's, never a flag.** Samba reports every dotfile `HIDDEN` (`hide dot files`, on by
  default), and the name already hides it; a local copy wearing `UF_HIDDEN` too would stay hidden after a rename.
- **Hidden rides the write's dates frame** (`source_times`: `FileTimes::set_attributes` in the same
  `FILE_BASIC_INFORMATION`), so a Hidden file costs no frame. `METADATA_CAPS.staged_flags` says so to the engine.
- **Read-only goes on only after the landing**, through `apply_metadata` with the whole set and the dates in one
  compound (one frame for a Locked file alone). Gotcha/Why: Samba RENAMES a Read-only file, as Windows does, but refuses
  to DELETE one (`STATUS_CANNOT_DELETE`, files and folders alike), so a staged temp wearing it couldn't be taken away by
  a cancel or a failed landing.
- **Sending attributes REPLACES the settable set**: an empty set goes as `NORMAL`, which clears them. So
  `apply_metadata` sends nothing for a set with neither flag, and a Hidden or Locked copy doesn't wear the `ARCHIVE` a
  fresh Samba file gets (nothing on a Mac reads it).
- **`STATUS_CANNOT_DELETE` maps to `VolumeError::PermissionDenied`** (by status, `map_smb_error`), which is what lets a
  reversal clear and retry: `Volume::clear_blockers` reads the attributes (one `stat`) and drops `READONLY`, keeping the
  rest. Only the app's reversals call it, on an item they verified as their own.
- **Evidence** (verified on the Docker guest share, Samba 4.20.6, default `map readonly = no` and
  `store dos attributes = yes`, 2026-10-12): `attributes_integration_test.rs`, which pins Hidden on the write (one-frame
  and streamed), Read-only after it, the rename and the refused delete, the unlock, and the dotfile rule. Through the
  app's pipeline: `apps/desktop/src-tauri/src/operations_host/backend_suites/smb_flags_test.rs` (onto and off the share,
  and a rollback of Locked copies). The QNAP answer is unverified.

## Finder tags and other xattrs on copies (open gap)

A direct-SMB copy keeps no xattrs either way: Finder tags, comments, custom icons (FinderInfo), and resource forks stay
behind, and the loss note says so (`METADATA_CAPS` claims no `XATTRS`). Tracked as
[#405](https://github.com/vdavid/cmdr/issues/405), blocked on `smb2` named streams
([vdavid/smb2#9](https://github.com/vdavid/smb2/issues/9)); Finder tags alone on a share are
[#289](https://github.com/vdavid/cmdr/issues/289), which shares the trait capability.

- **What it has to write**: what `smbfs` writes, so Finder on any Mac sees it: `AFP_AfpInfo`, `AFP_Resource`, and each
  other xattr as a stream whose name carries the `U+F022` mapping for `:` (#289's verified QNAP finding).
- **The caps come from probing each share**: a server without `vfs_fruit` or streams refuses them, so a backend-wide
  claim would lie, and a refusal counts as a reported loss.
- **Ordering**: written as streams on the staged temp before its landing rename, like any staged xattr; read off the
  CREATE plus a stream enumeration the read can compound.
- **The Finder-mounted share already keeps them** (the native chunked arm through `smbfs`; on the Docker fixture, which
  has no `vfs_fruit`, as `._` sidecars, and a resource fork is lost: `crates/cmdr-operations/src/transfer/DETAILS.md` §
  "Local copies stage"). The QNAP-shaped `vfs_fruit` answer is unverified until a `fruit` fixture exists, which comes
  with smb2#9.

## Server-side copy (copychunk)

`copy.rs` implements `Volume::copy_within` and `copy_on_server` with `FSCTL_SRV_COPYCHUNK`, so duplicating a file inside
one share sends no bytes over the link. The engine asks only when both sides are the very same volume (the trait's
default `may_copy_on_server_from`), so a copy between two shares, or two mount roots of one share, streams. The
engine-side contract (staging, fallback, cancel): `crates/cmdr-fs/DETAILS.md` § "Server-side work".

- **Shape**: `stat` and the source open in one round trip (the dates come from the stat), then the resume key, then the
  destination open, then 16 MiB slices (`COPY_SLICE_BYTES`, one copychunk request at smb2's opening limits) through
  `Tree::server_side_copy_range`, then the dates and Hidden on the copy's own handle before it closes (as `stamp_writer`
  does). ❌ Never smb2's whole-file `server_side_copy_file`: one call has no progress and no place to stop.
- **Cancel and pause**: `copy_on_server` asks `ServerCopyProgress::checkpoint` before each slice (a pause parks there,
  holding both handles; a session lost meanwhile fails the copy, and the engine streams the file instead) and `advanced`
  after it. `copy_within` wraps its bare callback as `ProgressOnly`, with no pause. Any failure or cancel closes the
  handle and deletes the partial before answering.
- **A server without copychunk refuses the resume key** (`NOT_SUPPORTED`, `INVALID_DEVICE_REQUEST`, smb2's
  `ErrorKind::Unsupported`), before `to` is touched. `handle_copy_result` maps that to `NotSupported`, the engine's
  "stream it", ❌ never an `IoError`. `map_smb_error` keeps `Unsupported` as `IoError` for every other operation.
- **The destination is created or truncated**, per the trait: smb2's public read-write open (`open_file_readwrite`,
  `FileOpenIf`) keeps what's there, so a non-empty one is emptied in place first (an empty `write_file_compound`, which
  keeps the file's identity) and reopened. The engine's fresh staging temp never pays that.
- **No FLUSH before the close**: smb2's `flush_handle` is crate-private, so the copy relies on the server committing
  copychunk's writes, as SFTP's `copy-data` does. A streamed write's `finish()` does flush.
- **Evidence** (verified on the Docker guest share, Samba 4.20.6, 2026-10-14): `copy_test.rs` pins byte-for-byte across
  slices, one progress report and one checkpoint per slice, cancel and a stopped checkpoint leaving nothing, a longer
  destination truncated, an empty file, both dates kept, and the shared `copy_within` handshake. No fixture lacks
  copychunk, so the fallback is pinned by the refusal mapping's unit cells, plus the engine's
  `strategy_server_side_copy_tests.rs` fallback cell with a double. The QNAP answer is unverified.

## Copy concurrency and the credit window

Every SMB2 request spends credits from a budget the server grants (smb2 steers toward a ~512-credit window), and the
charge follows the size of the RESPONSE the request asks for, one credit per 64 KB. Credits and in-flight bytes are
therefore the same quantity seen twice, which decides both halves of how a copy is sized. The incident that established
all of this, with the log extract and the arithmetic: `docs/notes/smb-credit-stall-2026-09-01.md`.

**The hinted read asks for the file's size, never `max_read`.** `open_read_stream_with_hint` takes the compound
fast-path (CREATE+READ+CLOSE in one frame) for any hint that fits the connection's `quick_read_limit()`, and drives
`Tree::read_file_compound_sized(conn, path, size)`. The unsized `read_file_compound` asks for `max_read` whatever the
file weighs, which is what stalled a 300 GB copy of 4 MB files: 130 credits each, three of them in a 512-credit window,
seven of ten copy slots parked. The scan pool's prefetch (`scan_pool.rs::open_read_stream_for_scan_impl`) reads the same
way, for the same reason.

`expected_size` is a HARD bound, not a hint: smb2 compares the server's authoritative size (it rides the same compound
frame, in the CREATE response) against the length asked for and refuses with `ErrorKind::TooLarge` rather than hand back
a prefix. That splits the two size-drift arms in the fast path cleanly, and both are load-bearing. A file that GREW
since the scan comes back `TooLarge` and falls through to streaming, which serves it whole; one that SHRANK comes back
short of the hint (`data.len() != size`) and falls through the same way. Neither ever hands the copy a prefix under the
file's final name. ❌ Never "simplify" either arm away, and never call the unsized `read_file_compound` from a path that
knows the size: an 8 MB request against a 4 MB file is both the over-charge and a guard that can't fire.

**A window too small to carry ONE compound frame: every fast path sizes to the credit ceiling.** A server granting a
small window (embedded NAS firmware, a router's USB share, Samba built with a low `smb2 max credits`) paired with a
LARGE `max_read` / `max_write` can leave a 5 MiB file's 83-credit chain unfundable (a small `max_read` is safe, since
`read_file_compound_sized` clamps to it). Both reference servers grant 513. smb2 (0.24.4+) learns the server's window
ceiling once it declines to grow it (`Connection::credit_ceiling`), refuses a request wider than that at once with
`Error::CreditStarvation` before anything reaches the wire, and sizes everything it picks the size of to half the
ceiling. So each fast path here decides up front and keeps a reactive arm for the ceiling arriving or shrinking between
the check and the send:

- **The foreground read**: `quick_read_limit()` counts the ceiling, so an unfundable hinted read streams from the start.
  The `CreditStarvation` arm in `open_read_stream_with_hint` catches the race. A server that stopped granting altogether
  starves the stream too, which then reports it, so the arm can't hide a dead server.
- **The scan pool's prefetch** (up to `max_read`, on purpose) matches `CreditStarvation` and falls through to
  main-session streaming, like a file too big for one READ, instead of skipping the photo as unreadable.
- **The write**: `write_is_single_shot` answers on smb2's `quick_write_limit()`, which never exceeds
  `compound_write_limit()`, so the transfer stages a write the window can't fund, and the `FileWriter` streams it in
  chunks the window carries. `write_from_stream` branches on `write_stream::one_frame_write_limit`: a `.cmdr-tmp-*` dest
  follows today's limit (and streams if the frame is refused), the user's real name keeps `max_write`, the most any
  promise could have covered, and a refused frame there fails the write with nothing at the name. ❌ Never let a refused
  frame to a non-scratch name fall back to streaming: that name only gets a frame because the transfer skipped staging
  on a single-shot promise.
- **The drained buffer must be exactly the promised size, or nothing is opened** (`source_broke_its_length`). A source
  still growing (a log, a recording, a download finishing) is refused the moment it outruns `Known(size)`, before any
  CREATE and without buffering the rest; one that came up short is refused after the drain. Before this, a buffer that
  outgrew one frame fell through to the multi-round-trip streaming writer AT THE REAL NAME, so a drop or a quit in those
  round trips left a truncated file wearing the user's filename (#520). So the only drained buffer that ever streams is
  a `.cmdr-tmp-*` temp's, after a `CreditStarvation`. The copy engine already holds its streams to their length
  (`crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "A source is held to its declared length"); this guard is
  the backend's own, for every other caller. Pinned by
  `smb_integration_a_single_shot_source_that_breaks_its_length_creates_nothing` against `smb-consumer-maxreadsize`.
- Pinned by the `credit_cap_proxy.rs` cells in `wire_shape_integration_test.rs` (hinted read, staged write, refused
  final-name write in both `WriteMode`s, refused frame to a taken name, `CreateNew` temp streaming without clobbering,
  prefetch), which cap the guest fixture's window at 64 credits. smb2 pins the same against a real 64-credit Samba (its
  `smb-smallcredits` fixture).
- ❌ Don't retry `CreditStarvation` generically: it reports `is_retryable() == true` and classifies as
  `ErrorKind::TimedOut`, though an unfundable request never succeeds.

**`max_concurrent_ops` is clamped by what the window can carry.** The answer is
`min(setting, credit_capacity_for(512 KB))`, floored at 1 and never raised above what the user chose.
`Connection::credit_capacity_for` is sync but the connection lives behind an async mutex, so `clone_session` stores the
figure in `SmbVolumeInner::credit_copy_capacity` on its way through (every read and write passes there) and the sync
method reads the atomic; `0` means nothing has measured it yet and the setting stands.

`credit_capacity_for` divides the smaller of smb2's `CREDIT_TARGET` (512) and the server's known credit ceiling by the
request's charge. On a generous server (both reference servers grant 513, and Samba's default maximum is 8,192) the
ceiling is never seen, the answer is 51 (an 8 MB `max_read`) to 170 (a 64 KB one), and `min(setting, capacity)` is the
setting for every `network.smbConcurrency` value (1..=32). It binds only on a small-window server: 64 credits carry six
512 KB copies at once. ❌ Don't tune `REPRESENTATIVE_COPY_REQUEST_BYTES` to make it bite on generous servers: a value
small enough to bind there would throttle them on a guess.

The 512 KB question is the judgement call in it. There's no per-file knowledge at that seam, and the risk isn't
symmetric: because charge and in-flight bytes are the same quantity, extra slots beyond what the window carries only
park until a peer finishes (the wire is already full), while too FEW slots leave the window half empty and cost real
throughput no retry gets back. So the figure has to sit at or below the typical copy request, not at the worst one:
`max_read` would answer three or four slots and throttle a queue of 4 MB files the window carries seven of. One
pipelined streaming READ (smb2 chunks a download at ~512 KB) is the smallest request a copy slot actually sends, so the
cap stays clear of a healthy session and bites only where it must, on a server whose whole window can't carry the slots
the setting asks for. Per-file sizing would be the accurate answer, and it belongs at the read, not here: it would need
the transfer driver to budget slots by size rather than by count.

Only the transfer driver reads `max_concurrent_ops` (`crates/cmdr-operations/src/transfer/volume/copy.rs`), so this
clamp never slows a listing, a stat, or the watcher; those share the same window and are better off for the headroom.

## Junctions and symlinks

A Windows share holds junctions (`mklink /J`, relocated game libraries, Dropbox folders, build caches) and symlinks; a
Samba share can serve a Unix symlink. Each NAMES another place, so anything that walks through one acts on its target: a
delete that descended into a junction deleted the target's files, a folder tree outside the selection (#516).

- **A link is `is_link()`, never the `REPARSE_POINT` bit.** `mapping.rs` sets `FileEntry.is_symlink` from smb2's
  `DirectoryEntry::is_link` (listings) and `FileInfo::is_link` (`get_metadata`, through `file_info_to_file_entry`): the
  tag's name-surrogate bit, so mount points, symlinks, and any other link. Deduplicated files and OneDrive or
  cloud-files placeholders carry the reparse bit too, and they're real files and folders: as a "link", a delete would
  refuse their folder and a copy would skip its contents. Pinned by `mapping_test.rs`.
- **A link keeps the `is_directory` its listing gives it**, like a local symlink to a folder (`cmdr-local`'s
  `reading.rs`), so a pane opens it as a folder, while every walk that asks `EntryKind` (delete, the index's
  `index_walk`, a rename's tally, a merge's dir-vs-dir test) treats it as one leaf. `BackendKind::Smb` is in
  `has_symlinks`, so a delete's top-level probe asks `entry_kind`. A cross-volume COPY materializes a link, the same
  stance it takes on every backend (`crates/cmdr-operations/src/transfer/DETAILS.md` § "Symlinks are opaque to a move").
- **Every delete is smb2's `delete_link`**: `delete_impl` and a forced rename's `clear_rename_destination`. It opens the
  reparse point itself with no file-or-directory condition, so ONE frame takes a file, an empty folder, or a link AS the
  link (a junction is a folder on Windows, a Samba symlink a file), and refuses a non-empty folder as the trait
  promises. A cross-volume move off the share sweeps its sources through the same `Volume::delete`. Pinned by
  `wire_shape_integration_test.rs`'s delete cell (one frame for a file and a folder; the old file-then-folder pair cost
  a folder two) and `links_integration_test.rs`.

**What smb2 0.34 changed under each call site.** `delete_file`, `delete_directory`, `rename`, `stat` (and siblings), and
`set_times` open the link itself (`FILE_OPEN_REPARSE_POINT`), where 0.33 and earlier opened its target. Audited
2026-10-20:

- `rename_impl` (both arms) renames the link, never the target folder it used to move on Windows. Right as is.
- `set_basic_info_impl` and `write_stream.rs`'s by-path stamp date the link itself, the `lutimes` stance; Cmdr's own
  writes are never links. `stamping.rs` stamps an open handle, unaffected.
- `get_metadata`, `is_directory`, `exists`, `attributes_impl`, the copy scans' top-level stats (`scan.rs`), the rename's
  `either_end_is_a_directory`, and `copy_on_server`'s date stat describe the link. On Windows a junction or `mklink /D`
  link keeps its `DIRECTORY` attribute, so nothing that asks "folder?" changes. `spelling.rs`'s probe only asks whether
  a prefix opens, which a link does.
- `write_stream.rs` and `copy.rs` delete only a partial they wrote, never a link.

**Samba** (verified on 4.23.8, the fixtures' `alpine:3.24`, by `links_integration_test.rs` on smb2 0.34.0, 2026-10-20;
4.20.6 and 4.23.10 by
[smb2's Samba note](https://github.com/vdavid/smb2/blob/main/docs/notes/samba-symlinks-and-reparse-points.md), same
date):

- **Listings never mark a Unix symlink**, on any version: it lists as what it points to. So a delete of a FOLDER that
  holds a symlink to another folder still descends it and deletes the target's files through it, if that target sits
  inside the share (`wide links = no`, the default, keeps it there). Open gap: closing it on 4.23+ needs an `entry_kind`
  stat per folder before the walker descends, a round trip per folder that 4.20 can't answer anyway.
- **4.23 honors `FILE_OPEN_REPARSE_POINT`**: a `stat` reports the link (`ReparseTag::SYMLINK`), so a SELECTED symlink is
  a leaf and deleting it removes the link alone. The same stat calls a symlink to a folder a NON-directory, so
  `get_metadata` and `is_directory` of one say file: copying a selected folder symlink off the share fails to stream it,
  and a typed path into one isn't taken for a folder. A pane's listing still shows it as a folder and opens it.
- **4.20 ignores the flag**: a stat follows the link, and deleting a link deletes its target, whatever the client sends.

**Windows** follows MS-SMB2 § 2.2.13 and MS-FSCC § 2.1.2.1 (spec-based, not run against a Windows server, 2026-10-20).
No fixture serves a junction.

## Decisions

**Decision**: `SmbVolume::to_smb_path` returns `Result<String, VolumeError>` and refuses a path outside the mount root
**Why**: it turns a path the frontend sent into the share-relative string that goes on the wire, and every way of
GUESSING an answer for an out-of-root path put a real request at a real, wrong place. It compared the root as a raw
STRING, so with root `/Volumes/naspi` a path under the sibling mount `/Volumes/naspi-1/x` stripped to `-1/x` — a legal
file name on the share, which the server would happily create or delete. Anything that matched neither fell through to
"strip the leading slash", so `/Users/me/notes.txt` went out as the share-relative `Users/me/notes.txt`. Matching whole
path COMPONENTS (`Path::strip_prefix`) kills the first, and `VolumeError::NotFound` for the rest kills the second: a
path that isn't on this volume genuinely isn't found there, and the caller surfaces that instead of acting elsewhere.
`exists` maps the error to `false` (the honest answer to the question it was asked), and the post-mutation listing-cache
patches go through `display_path_for`, which returns an `Option` so a write that already succeeded is never reported as
failed because its parent path didn't convert. The Docker integration tests address the fixture share the same way
production does, through `volume::testing::share_path` (and, app-side, `smb_index_scan_test::unique_base`), so a test
path is either relative or absolute under `TEST_MOUNT_ROOT`; ❌ never a bare `format!("/{name}")`, which reads as a real
absolute path somewhere else entirely.

**Decision**: `map_smb_error` maps `ErrorKind::InvalidName` to its own `VolumeError::InvalidName`, never the `IoError`
catch-all **Why**: `STATUS_OBJECT_NAME_INVALID` means the server refused the NAME, so it never looked for the file and
the identical request can only fail the identical way. As an `IoError` it inherited the wrong behavior: the dialog would
have offered "couldn't copy the file" plus a Retry button instead of the one thing that works (rename it). The typed
variant carries end to end: `friendly_error::kinds::invalid_name` on the listing path (`NeedsAction`, ❌ no retry hint)
and `WriteOperationError::InvalidName` on the write path, which names the failing file so a 5,000-item transfer says
WHICH one to rename. smb2 ≥ 0.18 maps the characters SMB2 forbids outright (`"`, `*`, `:`, `<`, `>`, `?`, `\`, `|`,
control characters, trailing space or period) into the Unicode private-use area, so those copy through fine
(`forbidden_chars_integration_test.rs`); what still reaches this arm is a reserved Windows device name (`CON`, `NUL`,
`LPT1`), a name past the server's own length limit, or a character its filesystem can't store. The status is also in
smb2's table now, so the technical-details line reads `STATUS_OBJECT_NAME_INVALID` rather than bare `0xC0000033`.

**Decision**: `SmbVolume::supports_local_fs_access()` returns `false`, but `paths_are_os_visible()` answers for the
mount **Why**: `SmbVolume` handles listing updates via `notify_mutation` using its own smb2 `get_metadata`. A
`std::fs`-based synthetic diff path (`emit_synthetic_entry_diff`) would be redundant and would go through the slow OS
mount. Returning `false` skips it. But "Cmdr shouldn't use `std::fs` here" is a different claim from "no other app can
open these paths": the sneaky mount keeps the share at `mount_path` and every path this volume hands out is an absolute
path under it. The macOS drag-out path needs the second answer, so it reads `paths_are_os_visible()`. While it read the
first one, a drag out of an SMB pane published `NSFilePromiseProvider` items with an empty pasteboard, which Finder
accepts and every other drop target (browser upload widget, mail composer, editor) rejects — so dragging NAS files into
an email did nothing while the same drag from Finder's mount worked. ❌ Don't collapse the two flags: five write/caching
call sites read `supports_local_fs_access()` as "is this remote?", where `false` stays the honest answer.
`paths_are_os_visible()` is `true` only while the mount is actually there — see § "Re-rooting a share" for how the
registry tells the volume otherwise.

**Decision**: the main session is one `Mutex<Option<MainSession>>` (`SmbClient` + `Arc<Tree>`), held only to clone
**Why**: `smb2::Connection` is `Clone` (cheap `Arc::clone`, all clones multiplex frames over one SMB session) and the
tree is an `Arc`, so `clone_session` locks just long enough to clone both out, then drives `Tree::download` /
`Tree::read_file_compound` / `Tree::write_file_compound` on the clones with no lock held. N concurrent copies on one
`SmbVolume` pipeline N operations over the single session instead of queuing on the mutex. The streaming-write path uses
the same clone-and-release shape (see the `write_from_stream` Decision below), so the mutex is never held across I/O.
The client and the tree share ONE lock because a tree only works on the connection it was connected on: they used to sit
behind two, `clone_session` read the tree before it locked the client, and a reconnect landing between the two handed
out the old tree with the new connection. smb2 0.30 refuses that pair with `HandleInvalidated`, which classifies as
`ConnectionLost`, so the volume read a healthy session as dead and reconnected again. Pinned by
`smb_integration_a_session_swapped_mid_clone_is_handed_out_whole`.

**Decision**: `SmbVolume::local_path()` returns `None` **Why**: `local_path()` is checked in `volume/copy.rs` to decide
whether to use native OS copy APIs. If SmbVolume returned `Some(mount_path)`, copies would go through the slow OS mount,
which is exactly what we're trying to avoid. `root()` still returns the mount path for frontend path resolution.

**Decision**: SmbVolume background watcher runs on a dedicated smb2 session, not a clone of the volume's main connection
**Why**: smb2 0.10 made `Watcher` `'static` (owns a `Connection` clone), so technically the watcher could share the
volume's session via `clone_session`. Empirically it can't: stacking the watcher's CHANGE_NOTIFY long-polls on the same
TCP session as heavy concurrent writes wedges Samba — `smb_integration_concurrent_streaming_writes_no_deadlock` hangs
against `smb-consumer-maxreadsize` (64 KB max read/write, 8 concurrent writers, 200 × 1 MB files). The dedicated session
keeps the watcher's traffic out of the writers' way at the cost of a separate TCP+auth. What we _do_ keep from the new
API: the watcher is `'static` (no borrow on the watcher task's `client`), and the pipelining (one CHANGE_NOTIFY
pre-issued so events during consumer processing don't fall in a re-arm gap). Stat calls for new/modified files still go
through the share's own `get_metadata` at its active root, reached by a `SelfHandle` (the main session, never the
watcher's), so the cmdr-side `notify_mutation` cache patch from our own writes lands first regardless.

**Decision**: A change batch's follow-up stats run together (`WATCHER_STAT_CONCURRENCY`, 8), answered in event order
**Why**: a busy QNAP holds a single `stat` for up to 6 s while its session stays healthy, and asked one after another
that one stall delayed every later change in the batch by the same 6 s. `process_event_batch_with` queues every stat the
batch needs up front and drains them through `buffered`, which yields in queue order, so the listing hears the changes
exactly as before (a removal keeps its place between two slow additions). ❗ The queue mirrors the emit loop (same map,
same order, one `stat_for` per `needs_stat` event whose path is under the anchor); change one and change the other.
Pinned by `watcher/stat_concurrency_test.rs`.

**Decision**: Watcher task is not stored on `SmbVolume`, only the cancel sender is **Why**: The spawned task owns its
own `Watcher` and `SmbClient`. Storing them on the struct alongside the cancel sender would just duplicate ownership
without buying anything — `watcher.next_events()` is `&mut self`, so the task is the only thing that can drive it
anyway. The `watcher_cancel: Mutex<Option<oneshot::Sender<()>>>` on the struct provides clean shutdown.

**Decision**: Watcher doesn't reconnect itself; on death it KICKS the one reconnect path **Why**: When `next_events`
errors with anything but `NOTIFY_ENUM_DIR`, the watcher's task returns. It must NOT run its own reconnect-with-backoff
loop: two state machines tracking the same "is the session alive" question diverge — the watcher's internal retries
would swallow real disconnections the FE reconnect manager surfaces. So the watcher still owns no reconnect logic; it
just calls `spawn_watcher_death_reconnect(volume_id)`, which drives `do_attempt_reconnect` (the single source of truth)
on a bounded backoff. One reconnect path, one source of truth — now triggered on watcher death too, not only by the next
hot-path op / FE backoff tick. See § "Backend-autonomous reconnect and index resume" for why the kick marks the volume
`Disconnected` first.

**Decision**: we run `smb2`'s deadline and keepalive defaults unchanged (the handshake aside, below), and read none of
them as a liveness verdict **Why**: every wait a request can make is bounded by the crate, so Cmdr needs no timeout
layer of its own. A frame gets 20 s to reach the socket (`Error::SendTimeout`); once out, the server gets 30 s of
SILENCE (not elapsed time — every interim `STATUS_PENDING` restarts the clock, so a multi-minute write to a loaded NAS
is never cut off), stretched to 6× that on a connection an ECHO probe has just proven alive. A breach tears the
connection down, which is why `retry.rs` sees a typed `DeviceDisconnected` / `ConnectionTimeout` instead of a hang. The
ECHO keepalive (5 s, on by default) only probes when the wire has gone quiet with work outstanding, so a busy transfer
pays nothing for it. ❌ **A missed probe is NOT evidence of death** and nothing here may treat it as such: a QNAP TS-464
drops probes precisely while it writes (measured 2026-08-02: 1 of 3 dropped under write load, 0 of 3 idle). The crate
agrees — its only death verdict, `Error::ServerUnresponsive`, needs a request to burn its whole deadline AND the
connection to have put nothing at all on the wire meanwhile, the bytes of a response still arriving counted as they land
(0.23.0+).

**The handshake is the one place we pick a number** (`connection::HANDSHAKE_RESPONSE_TIMEOUT`, 10 s, via
`connect_volume_client`, used by `build_session` and the watcher's session). smb2 bounds NEGOTIATE only through
`ClientConfig::response_timeout` (0.29.0+; before that a server that accepts TCP and never answers, like a NAS whose
`smbd` is still starting or a middlebox swallowing the frame, held `build_session` and its `reconnect_lock` forever:
`#406`, verified on 0.28.0 against a silent loopback listener, 2026-10-14). That knob is per connection, so we set 10 s
for NEGOTIATE and SESSION_SETUP, then restore smb2's 30 s default before TreeConnect and everything after.

- **Why 10 s**: both are answered from the server's memory, so 10 s is generous for a slow or busy NAS; a spun-down disk
  is woken by TreeConnect or the first CREATE, which keep the full 30 s. A give-up that was too eager costs one retry
  against a server that's warm by then.
- **Why it has to be under 15 s**: the transfer's outage wait drops any attempt past `OutagePolicy::attempt_timeout` (15
  s). With the TCP connect at 10 s and NEGOTIATE at 10 s, a dead host and a silent server each end the attempt on their
  own with a typed error, so the outer bound is a safety net (a slow TCP accept followed by silence can still reach it).
  A test in `connection.rs` pins the 10 s under that 15 s.
- **What it closes**: the frontend's reconnect tick and the watcher-death loop both go through `do_attempt_reconnect`,
  which had no outer bound of its own. Its worst case is now bounded (TCP 10 s, NEGOTIATE and each SESSION_SETUP leg 10
  s of silence, TreeConnect 30 s), so `reconnect_lock` can no longer be held forever. Pinned by `connection.rs`'s
  `a_volume_connect_to_a_silent_server_gives_up_at_the_handshake_deadline`.
- The one-shot probes (`try_list_shares_*`, `try_open_share`) pass their caller's `timeout` as the response deadline, so
  every step there ends in a typed `Error::Timeout`, under the caller's own outer bound.

`SmbVolume::connection_liveness()` reads the `ServerUnresponsive` verdict as pollable state (`Connection::liveness()`),
before any request has paid for it, off a handle swapped in lockstep with the client (`src/volume/liveness.rs`, which
owns the mapping). What the transfer watchdog does with it: `crates/cmdr-operations/src/transfer/DETAILS.md` § "The
watchdog ACTS".

**The numbers stay as they are, and the case for widening them is closed.** The base deadline no longer has to be sized
for the slowest healthy case, because the ECHO stretch already covers a connection that is alive but busy; and a breach
that does land reaches the per-file retry as a typed transport fault (`VolumeError::transport_fault`), so a retry on a
rebuilt session absorbs it as a blip rather than a failed file. Nothing measured says the defaults hurt.

**Read `sent_age` before drawing any conclusion from a stall.** `None` means the request never reached the wire, so the
server has not been asked yet and none of the deadlines above are even running; a `Some` age is the only number that
says how long the server has actually been silent.

**Silence measured across a frozen Cmdr is discounted, not counted** (`smb2` 0.18.1+). Every clock in the crate measures
wall time, so a stretch where this process was not scheduled at all — a laptop sleep, an App Nap, a machine starved by a
parallel build — used to read as the server going quiet, and the reconnect that followed was against a NAS that had been
answering the whole time (2026-08-08: three freezes of 62 s, 175 s, and 355 s in twelve minutes). The crate now
recognizes the gap from its own loop cadence and shifts every liveness clock forward by it. Two consequences for Cmdr:
an `SmbVolume` reconnect after a wake is now evidence about the _network_, not about the sleep, and
`MetricsSnapshot::scheduling_stalls` (surfaced through `commands/smb_diagnostics.rs`) is the counter that says whether
the app stopped running. ❌ Don't add a Cmdr-side sleep/wake hook for this — the crate handles any stall from any cause,
and a hook would only cover the one macOS reports.

**Decision**: the watcher's dedicated session is probed like any other, and a watcher death stays cheap **Why**:
CHANGE_NOTIFY is exempt from the request deadline by design (it waits for an event that may never come), so that
connection is bounded by connection-wide silence instead — which is the only thing that lets a watcher on a dead session
ever find out, and it's what feeds `spawn_watcher_death_reconnect`. The cost of a false one is small by construction:
the kick marks the volume `Disconnected` and rebuilds the session, while an in-flight transfer holds its own
`Arc<Tree>` + `Connection` clone and runs on. ⚠️ Unverified: whether a NAS busy enough to drop 6 consecutive probes (30
s of total silence on that session) can trigger this during a large copy. It has not been observed; if watcher deaths
ever cluster with heavy transfers, that is the mechanism to suspect, and the fix is `Connection::set_response_timeout`
on the watcher's session alone.

**Decision**: Watcher debounces 200ms per batch, `FullRefresh` above 50 events per directory **Why**: Prevents 1000
individual stat calls when 1000 files are copied. The 200ms window collects events that arrive in rapid succession. The
50-event threshold for `FullRefresh` avoids O(n) stat calls for bulk operations.

**Decision**: `write_from_stream` uses a cloned `Connection` + `Arc<Tree>` (owned `FileWriter`) **Why**: `FileWriter`
owns its `Connection` (cheap `Arc::clone`) and `Arc<Tree>` rather than borrowing `&'a mut Connection`.
`write_from_stream` calls `clone_session` once up front and drives both the compound fast-path AND the streaming
fallback on the same owned `Connection` clone. The client mutex is held only for the few microseconds of
`clone_session()`, never across I/O. **Don't switch back to a borrowed `FileWriter<'a>` that holds the client mutex
across the upload**: that shape deadlocks under sustained concurrent pressure (the two-phase brief-clone-then-long-hold
pattern is the QNAP deadlock reproducer). The regression is pinned by
`smb_integration_concurrent_streaming_writes_no_deadlock`. The architectural property we get from owned `FileWriter`: N
concurrent streaming writes on one `SmbVolume` pipeline N WRITE chains over a single SMB session, multiplexed by
`MessageId` in smb2's receiver task. No external locking, no mutex contention on the hot copy path.

**Decision**: `write_from_stream` ERROR paths delete the partial file, mirroring the cancel branch **Why**: Once the
streaming `FileWriter` is open and bytes have streamed into it, an early error (mid-stream source-read error,
`write_chunk` failure, `finish` failure, the compound-fallback writer's `write_chunk`/`finish`) would otherwise leave a
half-written file at the user's intended destination name — corrupt bytes presented as a real file (violates AGENTS.md
principle #4). The cancel branch already cleaned up (`writer.abort()` + best-effort `delete_file` on a fresh cloned
session); every owned-writer error site now does the same. **`abort()` before delete is load-bearing**: dropping a
`FileWriter` without `finish()`/`abort()` leaks the SMB handle (smb2's `FileWriter::Drop` only logs, never sends CLOSE),
so a fresh-session `delete_file` (CREATE-with-delete-on-close) hits a sharing violation against the still-open handle
and the partial lingers. So: `write_chunk`/source-read errors `writer.abort().await` first (writer still owned), then
`delete_partial()`. `finish()` consumes the writer, so on its failure the handle is already gone — best-effort
`delete_partial()` only. The compound FAST-path (`write_file_compound`) carries the whole file in one CREATE+WRITE+FLUSH
frame (smb2 closes the handle itself, on success and failure alike) and the compound DRAIN loop buffers in memory before
any handle opens, so neither leaves a streamed partial — those propagate their error unchanged. The original error
always propagates (never `Cancelled`); cleanup is best-effort and never masks it. Pinned by
`smb_integration_write_from_stream_source_error_deletes_partial` (source errors after the first chunk; asserts the
propagated `IoError` and that no file remains at the destination). Don't refactor the owned-writer error sites into a
post-block catch-all that loses the writer — you'd lose the `abort()` and the delete would no-op against the leaked
handle.

**Decision**: `SmbVolume` overrides `scan_for_copy_batch` to pipeline per-path stats over a single SMB session **Why**:
A naive scan phase that loops `scan_for_copy` per top-level source costs N sequential RTTs before the copy phase can
start. For a 100-file copy over a ~60 ms Tailscale link that's ~5 s of serial stats. The override takes one
`clone_session`, clones its `smb2::Connection` per path (cheap `Arc::clone`, all clones multiplex over the same SMB
session), then drives `tree.stat(&mut conn, path)` on each clone inside a `FuturesUnordered`. Empty root paths skip the
stat. Single-path batches fall through to `scan_recursive` so one-file drag-drops don't pay the batch machinery cost. ⚠️
A single DIRECTORY path takes that same branch and walks its whole subtree: a batch of one is not a stat. A caller that
only wants "what is this path?" stats it directly (the model:
`apps/desktop/src-tauri/src/commands/file_system/volume_copy.rs`'s `stat_source_paths`, one `get_metadata` per top-level
path). Directories found during the stat phase recurse sequentially afterward; parallel directory recursion is a future
enhancement. Measured 6.5× wall-clock win at 100 × 10 KB: 6.11 s → 947 ms. See `docs/notes/phase4-rtt-investigation.md`
for the wire trace. **Oracle layered on top**: before the pipelined-stat block runs, every input path's parent is
checked against the fresh-listing oracle (`host.listings().authoritative_listing(volume_id, parent)`). Oracle-served
paths get their size + `is_directory` from the cached `FileEntry` and are removed from the leftover set; only the
leftover paths go through the pipelined stat. Decision is per-parent: one batch can mix oracle-served and pipelined-stat
paths, and if every path resolves via the oracle the stat pipeline is skipped entirely.

**Decision**: `SmbVolume` has a compound fast-path in `open_read_stream_with_hint` for files up to the connection's
`quick_read_limit()` (`read_stream::fits_one_compound_read`) and in `write_from_stream` for files that fit one compound
frame (`write_stream::fits_one_compound_write`) **Why**: The streaming open+read+close sequence costs 3 RTTs per file.
For small files (typical 10 KB copies on a NAS) that dominates wall-clock at high-latency links (~60 ms RTT → ~180
ms/file just for protocol overhead, not data). `smb2` already exposes `Tree::read_file_compound` (CREATE+READ+CLOSE in a
single compound frame = 1 RTT) and `Tree::write_file_compound` (CREATE+WRITE+FLUSH in one frame, then CLOSE = 2 RTTs),
with `write_file_compound_exclusive` (`FileCreate`) for `WriteMode::CreateNew`, the same pair as `create_file_writer` /
`create_file_writer_exclusive` on the streaming path, so every branch of `write_from_stream` (the frame, a staging
temp's streamed fallback, a too-big write) honors the mode (why the mode matters:
`crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "The single-shot exemption"). The copy pipeline feeds per-file
size hints from the pre-copy scan; when the size is known and fits the threshold, we take the compound path. The read
side stops at what the link moves in 250 ms (smb2's `quick_read_limit`: one 512 KiB chunk until a download of two or
more chunks has measured the link, then `rate × 250 ms`, capped at `max_read_size`; the rate expires after 30 s and a
reconnect clears it), because a bigger compound READ carries the whole file with no progress, queued ahead of every
listing on the connection (cmdr-reports#15: 23 s for 8 MiB at 375 KB/s), while `Tree::download` streams it through an
adaptive read-ahead. Under the limit, the compound saves the stream's CREATE round trip. The rate is the link's capacity
(timed from READ answers that queued behind each other), not the last download's pace, so on a fast link the limit rises
well past what a small window's pace would give; it still errs low. The 250 ms is a fixed budget for how long a listing
may wait behind one frame, deliberately apart from the read-ahead window's own headroom, which smb2 learns per
connection (30–500 ms) and which shrinks to tens of milliseconds on a quiet LAN, where a cut-off tied to it would stream
small files at an extra round trip or two each. The arithmetic stays in smb2. The numbers sit on
`fits_one_compound_read`. The scan pool's prefetch keeps `max_read_size` on purpose (§ "SMB scan-connection pool", the
reads bullet). Falls back cleanly to the streaming reader/writer when the hint is missing or the file is too big. Small
compound reads return a `Vec<u8>` wrapped as a single-chunk `InlineReadStream` so the consumer API stays shaped the
same. See `docs/notes/phase4-rtt-investigation.md` for the measurement. The write side stops at the same 250 ms, the
other way round: smb2's `quick_write_limit` (one 512 KiB upload chunk until an upload of two or more WRITEs has measured
the uplink's capacity, then `rate × 250 ms`, capped at `compound_write_limit`, the upload rate kept apart from the
download one because home links are asymmetric), since one compound WRITE carries the whole file up the link with
nothing else moving: at 375 KB/s a `stat` waited 23 s behind an 8 MiB upload and 1.5 s behind a streamed one
([smb2's upload benchmark](https://github.com/vdavid/smb2/blob/main/benchmarks/read-ahead/results/adaptive-uploads.md),
smb2 0.25.0, 2026-09-23). The numbers sit on `fits_one_compound_write`. The compound write is two round trips on every
server: smb2 ends the chain on the FLUSH and closes separately, because a server may fail an op that goes async
mid-chain (MS-SMB2 § 3.3.5.2.7; Windows answers `STATUS_INTERNAL_ERROR`), and Samba 4.4–4.17 (bug 15172, common in NAS
firmware) then crashes its `smbd` and resets the connection (smb2 0.31.1, reproduced on Samba 4.15.13, 2026-10-19; a
user's NAS dropped six times in 30 minutes on it). The data is flushed when the call returns, and a refused FLUSH is an
error. The WRITE side's condition is also a DATA-SAFETY contract: `write_is_single_shot` answers with
`fits_one_compound_write` on that limit, the fast path takes the frame for every size such a promise could cover
(`one_frame_write_limit`: a staging temp takes today's limit, the real name keeps `max_write`, because the limit moves
with every upload and expires after 30 s), and the transfer layer skips its `.cmdr-tmp-*` staging on the strength of
that answer. What the backend owes in return (short sources stay on the compound path, a post-CREATE failure cleans up
after itself): `crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "The single-shot exemption".

**Decision**: a streamed read (`open_smb_download_stream`) ends the consumer's stream at its last byte, before the
CLOSE's answer **Why**: smb2's `FileDownload::next_chunk` puts the CLOSE on the wire before it hands out the last chunk,
and only the call after it waits for the answer. Waiting for that `None` before dropping `chunk_tx` cost every streamed
file one round trip (+200 ms: 4 MiB about 830 → 630 ms). The producer breaks on `bytes_received() == total_size`, drops
`chunk_tx`, then collects the CLOSE's answer, logging an error at debug and feeding a dead connection to the volume's
state. A file that shrank mid-download still ends on `None`, which smb2 sends its CLOSE for inline. **Move safety**: a
move deletes the source after the copy's stream ends. That's safe without the answer: `send_and_count` returns only once
the per-socket FIFO writer task has written the CLOSE, so any delete issued afterwards on any clone of the connection
sits behind it on the wire, and `Tree::open_file` opens with `FILE_SHARE_DELETE`, so even a delete overtaking it would
only mark the file delete-pending until the CLOSE lands (verified against smb2 0.24.2's `stream.rs` and `connection.rs`,
2026-09-23). Pinned by `smb_integration_a_streamed_read_ends_before_the_close_is_answered` on the `slow` fixture.

**Decision**: streaming-write progress reports the SERVER-CONFIRMED byte count (`FileWriter::bytes_written()`), never
the count handed to the pipeline **Why**: `write_chunk` returns as soon as a chunk's WRITEs are on the wire, inside
smb2's write-behind window, so accepted and acknowledged bytes differ by up to a full window per file (up to 4 MiB
adaptive; the fixed 32-WRITE window smb2 had before 0.25 made it far more), and by `concurrency x window` across an
operation. In ERR-9WZRR (a 10-wide copy of 66 MB files to a NAS over a 6.7 MB/s link) that was ~320 MiB of pre-credited
bytes: the bar ran ~48 s of wire time ahead, then flatlined while the queue drained, the ETA was built on bytes nothing
had committed, and the transfer watchdog read the flatline as "no byte movement for 20s" on a healthy copy. `finish()`
drains the window and returns the confirmed total, so the loop reports the acknowledged count per chunk and one final
call credits the last window. The per-chunk call still happens even when the count hasn't moved, because it doubles as
the cancel poll.

❗ **Size any test that means to reach this path off `negotiated_max_write()`.** A payload that fits one compound write
silently takes the fast path instead, so the test passes without ever touching the pipelined loop it was written for.

The compound fast-path reports differently on purpose: it has no acknowledgement to report until the one all-or-nothing
frame returns, so it reports bytes buffered during the source drain. That lie is bounded by `max_write_size` and the
frame either lands whole or creates nothing.

An unknown-length generated stream bypasses the compound drain. It feeds bounded chunks through the owned streaming
writer, reports `StreamLength::Unknown` with server-confirmed bytes, and succeeds only after `FileWriter::finish` has
drained, flushed, and closed the handle. Known-length decisions stay unchanged.

**Decision**: `rename(force = true)` is smb2's `rename_with(replace_if_exists)`, so SMB answers
`supports_atomic_replace_rename()` true **Why**: the server swaps the source in under the taken name in the same
SET_INFO that renames it (`ReplaceIfExists`, MS-FSCC § 2.4.42.2), so the old file is never removed in a step of its own
and a refusal leaves it intact. That's the promise a validated-file publisher needs to land generated output with one
force rename rather than the tracked aside/restore. It also costs one round trip where the stat-delete-rename it
replaced cost three (pinned on the wire by `smb_integration_a_forced_rename_replaces_the_file_in_one_frame`).

- **A directory at either end is the one fallback.** smb2 replaces only a file: Samba refuses a directory with
  `AlreadyExists`, Windows with `AccessDenied`. On either refusal `replacing_rename` stats both ends, and only when one
  is a directory does it clear the destination (`clear_rename_destination`: delete the file, or the empty directory) and
  rename plainly. That path isn't atomic and doesn't have to be: the capability promises a FILE. Any other refusal
  stands as it came, with the original untouched.
- ❌ **Never clear a destination that names the source under another case.** On a case-insensitive share the destination
  stat finds the SOURCE, and the delete-first force path deleted it before the rename, so a forced `report.txt` →
  `Report.txt` lost the file. The replacing rename handles a case-only change in place; `same_name_ignoring_case` keeps
  the fallback from ever reaching it. Pinned by `smb_integration_a_forced_case_only_rename_keeps_the_file` (red before:
  `NotFound` after the source was deleted).
- **A plain rename sends no stat first.** The server refuses a taken name itself (smb2 sends `ReplaceIfExists = false`,
  answered `STATUS_OBJECT_NAME_COLLISION`), so the wire is the no-clobber guarantee and a stat beside it bought nothing
  but a round trip and one bug: on a case-insensitive share it found the source under a case-only new name, so renaming
  `report.txt` to `Report.txt` in a pane was refused as taken. Pinned by
  `smb_integration_a_case_only_rename_isnt_refused_as_taken` (red before) and the shared no-clobber conformance cell.

**Gotcha/Why**: a test that means to exercise the streaming writer MUST size its file off `negotiated_max_write()`, not
a literal. The fixture Samba negotiates a `max_write` far above any round number you would reach for, so a "4 MiB,
surely multi-chunk" file takes the compound path and the test silently exercises nothing. Pinned by
`smb_integration_write_progress_reports_confirmed_bytes_not_queued_ones`, which asserts the acknowledged count STALLS
while the window fills (a strictly increasing sequence is the signature of counting queued bytes) and asserts
`write_is_single_shot` is false up front.

## What a `VolumeError` payload carries

`VolumeError::NotFound`, `PermissionDenied`, `AlreadyExists`, `IsADirectory`, and `DeletePending` are DEFINED to carry
the PATH (`cmdr-fs/src/volume/types.rs`), and the transfer layer takes that literally:
`transfer_error.rs::map_volume_error` forwards the payload straight into `SourceNotFound { path }` /
`DestinationNotFound { path }`, which the frontend renders as the name of the file the user is missing.

So `mapping.rs::map_smb_error` takes the path it is mapping a failure for, and `session.rs::handle_smb_result` takes the
share-relative path beside its `op_name` and hands `map_smb_error` the display form. The server's own NTSTATUS wording
goes to the log line in `handle_smb_result`, never into the payload. The variants that carry a DIAGNOSTIC rather than a
path (`DeviceDisconnected`, `ConnectionTimeout`, `StorageFull`, `InvalidName`, `IoError`) still get `err.to_string()`:
that string is the technical-details text, not something the frontend reads a filename out of. An `IoError` from smb2's
`Io` (a local failure) also keeps the OS errno off the typed `io::Error` (`mapping.rs::local_errno`), ❌ never parsed
from the message.

- **Held to it by** `cmdr_fs::volume::conformance::assert_not_found_carries_the_path`, wired into
  `conformance_test.rs`'s `smb_integration_not_found_honors_the_shared_path_payload_contract` (Docker-gated like the
  rest of the lane), plus the payload assertions in `mapping_test.rs`.
- **Which path, for the two-path operations.** A `rename` refusal names the DESTINATION on `AlreadyExists` and the
  SOURCE on everything else, the same split `local_posix.rs::rename_error` makes. `get_space_info` is about the share,
  so it names the share root.

## Gotchas

**Gotcha**: Watcher filenames from SMB use backslashes; must normalize to forward slashes **Why**: SMB servers send
paths like `papers\new-file.txt`. The watcher normalizes these to `papers/new-file.txt` before extracting parent
directories and constructing display paths.

**Gotcha**: a guest session is asked for with the username `Guest`, ❌ never an empty (anonymous) one **Why**: some
servers accept the named `Guest` login but refuse an anonymous one (Windows with anonymous access restricted, some NAS
configurations), so switching would break guest access for real users, and nothing here can verify it against real
hardware. smb2 0.25.0 refused a guest session to any named login, `Guest` included, which failed every guest volume;
0.25.1 and later treat `Guest` (any ASCII case) as a guest login on purpose, like an empty name. ❌ So don't pin smb2 to
0.25.0.

**Gotcha**: a share name reaches the wire NFC, so `SmbConnectionParams` must be built with `new` **Why**: `new` runs the
NFC normalization; a struct literal filled from a raw `statfs` mount name carries macOS's NFD spelling straight to the
server, which answers `STATUS_BAD_NETWORK_NAME` for a share whose name has any composed character in it. The failure is
loud and immediate, which is the only reason this doesn't sit in `CLAUDE.md`.

**Gotcha**: a successful delete doesn't mean the name is free **Why**: smb2 deletes by opening with `FILE_SHARE_DELETE`
and setting the delete disposition, which succeeds while ANOTHER open still holds the file (another client, or macOS's
own mount of the same share). Until that handle closes, every CREATE on the name answers `STATUS_DELETE_PENDING`.
`get_metadata` waits that out across three tries (`DELETE_PENDING_BACKOFF`, 0.8 s) before reporting `DeletePending`,
because a move's destination probe used to end the whole move on it (cmdr-reports#17). Pinned by
`smb_integration_get_metadata_{waits_out_a_brief,reports_a}_delete_pending*`.

**Gotcha**: `specta` is pinned to the app's exact version, and a bump has to move both **Why**: `tauri-specta` collects
the app's commands transitively, so the `Type` impls this crate derives have to come from the SAME `specta` crate the
app links. Two `specta` nodes in one graph make them different traits, and the collection stops compiling.

## The suites

Which side each one lives on, and why: § "Which side a test lives on" above.

- **Server-free, colocated with the module each covers**: `mapping_test.rs` (`DirectoryEntry`→`FileEntry`,
  `FsInfo`→`SpaceInfo`, `smb2::Error`→`VolumeError`), `state_test.rs` (the binary state machine and how it widens),
  `paths_test.rs` (path translation both ways), `spelling_test.rs` (the foreign-path fold, match rule, and correction
  cache), `volume_impl_test.rs` (re-rooting and every capability flag), `reconnect_test.rs` (the reconnect early-exits,
  the transitions and the events they suppress, the watch-coverage gate, both retirement paths), `read_stream_test.rs`
  (the channel-backed read-stream consumer and the compound read's limit), `write_stream_test.rs` (the one-frame write's
  limit and the single-shot write promise), `scan_test.rs` (the progress ticker), `retirement_test.rs`,
  `watcher/archive_refresh_test.rs`, and the inline `mod tests` in `foreground_yield.rs` and `scan_pool.rs`. These run
  by default.
- `host_seam_test.rs` — the PACE of what this backend tells the listing seam, which no type can hold: one call per
  mutation, none per directory entry. Its server-free cells pin the addressing and that an un-stattable creation patches
  nothing; its Docker cell seeds a directory, walks it with a listing and a copy scan, and asserts
  `RecordingListings::change_count` doesn't move, which is what would catch a `notify_mutation` drifting into an entry
  loop. The rule and the instrument: `crates/cmdr-fs/src/volume/host/DETAILS.md`.
- `integration_test.rs` — what a share does with FILES against a real server: core CRUD, single-chunk streaming smoke,
  the copy and conflict scans, space info, and an anchored mount (`make_docker_volume_anchored`) listing, writing, and
  deleting inside its own directory rather than at the top of the share.
- `unicode_names_integration_test.rs` — names in both Unicode forms, seeded through a raw smb2 session so they sit on
  disk exactly as spelled: every operation on a listed NFD name inside an NFC directory (read, hinted read, scan, copy
  off and within the share, rename, delete, open), and the watcher reporting an outside change under the pane's own
  spelling of an accented directory (a 20 s delivery budget, because the fixture's `notifyd` lags under a shared lane:
  `docs/testing.md` § "Sanctioned slow-test exceptions"). Then the foreign-path resolve: an all-NFD path to an NFC file
  and to ERR-VETBX's mixed shape, a case-and-form-differing directory, look-alike twins refusing, a pane path carried
  over from the kernel mount, and a stale remembered correction healing (§ "Resolving a foreign path").
- `forbidden_chars_integration_test.rs` — names carrying the characters SMB2 forbids (`?`, `*`, `:`, `\`, trailing space
  or period, and so on), which the `unicode` fixture container seeds in the private-use bytes macOS smbfs writes: they
  list as the characters themselves with no private-use code point left, open and read (also through a directory whose
  own name carries one), and a name Cmdr writes round-trips.
- `session_integration_test.rs` — what the SESSION does: the connection gate the fresh-listing oracle reads, the
  reconnect cycle, the refcounted scan pool, and what a supersede leaves alone.
- `src/connection_integration_test.rs` — the three answers `try_open_share` hears from the `both` fixture: a guest
  refused at TreeConnect, the allowed account let in, and a missing share's bad network name. What the app makes of them
  is `apps/desktop/src-tauri/src/network/share_access_test.rs`.
- **The byte path is three files split by contract**, all declared from `volume/mod.rs`. A new byte-path cell adds
  itself to the matching contract rather than growing one file; a cell that straddles goes where its ASSERTION lives,
  not where its setup does.
  - `read_stream_integration_test.rs` — what `open_read_stream` / `open_read_stream_with_hint` hand back: a plain read,
    a multi-MB read across chunk boundaries, cancel-by-drop, and both size-drift arms of the hinted fast path (a file
    that grew, a file that shrank) serving the file as it is now.
  - `write_stream_integration_test.rs` — what `write_from_stream` does with a source: the bytes that land, progress
    shape (server-confirmed, never queued), cancel and mid-write cancel, multi-chunk sources, and the error /
    partial-cleanup path with the `ErroringReadStream` double. The cross-volume streaming copy is here because every
    assertion it makes is about what arrived on the share, even though its source is an `InMemoryVolume`.
  - `wire_shape_integration_test.rs` — what a byte-path op COSTS, which neither of the above asks: the hinted read's ONE
    compound frame and the `quick_read_limit` boundary where it gives way to streaming (on a cold connection `(1, 3)` at
    512 KiB, `(0, 4)` one byte over, in two chunks; after a measuring download, `(1, 3)` for 2 MiB), the streamed read
    ending at its last byte rather than at the CLOSE's answer (timed on the `slow` fixture, where that answer is 200 ms
    away), the single-shot write promise the transfer layer skips `.cmdr-tmp-*` staging on (the wire proof and the
    `write_is_single_shot` predicate behind it, kept together, and where that promise stops: one upload chunk on a cold
    connection, what a measured uplink moves in 250 ms after, with a staged write over it streaming and a promised one
    still `(2, 9)`), the copy-slot clamp, and a delete's one frame (§ "Junctions and symlinks"). It owns
    `request_counts`, the diagnostics-metric reader those frame assertions run on. **Every cell here asserts on the PAIR
    `(compound_requests_sent, requests_sent)`, because `requests_sent` alone reads like a frame count and is not one**:
    smb2 ticks it once per sub-op of a chain (`allocate_msg_id` is the funnel every send path goes through), while
    `compound_requests_sent` counts the chain, and `execute_compound` hands the whole chain to one `send_and_count`. So
    a hinted read is `(1, 3)` for one frame carrying CREATE+READ+CLOSE, and the single-shot write is `(2, 9)` for two
    frames, its own four ops and the post-write stat's five (CREATE, three QUERY_INFOs, the last the reparse tag, CLOSE;
    verified against Samba 4.23.8 in the `smb-consumer` container on smb2 0.34.0, 2026-10-20). Reading the second number
    as round trips costs an afternoon: it makes `requests == 1` look like the fast path's proof, and that assertion is
    unsatisfiable by construction. The pair also asserts more than either half: a streaming open reads as `(0, 3)`, a
    loose round trip beside the compound as `(1, 4)`. The copy-concurrency cell exercises neither byte path; it sits
    here because its subject is the credit window of § "Copy concurrency and the credit window", which the sized read
    and the slot clamp are the two halves of.
- `links_integration_test.rs` — a Unix symlink made inside the guest container (`run_in_the_guest_container`): a stat
  reports it, deleting it leaves its target, and Samba's listing doesn't mark it (§ "Junctions and symlinks").
- `conformance_test.rs` — the `cmdr_fs::volume::conformance` promises, answered by a real server rather than an
  in-process double (SMB has none): `STATUS_DIRECTORY_NOT_EMPTY`, `STATUS_OBJECT_NAME_COLLISION`,
  `STATUS_OBJECT_NAME_NOT_FOUND`. That last one is the conflict scan's: `scan_for_conflicts_impl` keeps its own
  cache-aware listing but reads a `NotFound` from it as an empty conflict list, which is the trait's contract for every
  backend (`Volume::scan_for_conflicts`), so pasting into a folder the transfer is about to create isn't refused.
- `credit_cap_proxy.rs`: a test-only SMB-aware TCP proxy that rewrites response headers so a connection's credit window
  (unspent plus in flight, as Samba's `smb2 max credits` bounds it) never exceeds a set number, the small-window server
  no fixture plays (unsigned guest traffic only). ❌ Don't cap only the unspent balance: the window would swell with
  every request in flight, and smb2 reads the server's ceiling off that window.
- `test_support.rs` — the session-free builders (a struct-literal `SmbVolumeInner` with no client and no tree), the
  vocabulary every suite globs, a re-export of `volume::testing` so one `use` covers all three, and `drain`, which lives
  here rather than in one suite because the read and wire-shape files both drain a hinted read.

Every Docker cell is `#[ignore]`d, so a default run skips it. Start the containers with
`apps/desktop/test/smb-servers/start.sh` and run `cargo nextest run -E 'package(cmdr-smb)' --run-ignored only` — select
by PACKAGE, not by the `smb_integration` name, or a cell named for what it asserts drops out of your run. A new Docker
cell here can be named that way precisely because `desktop-rust-integration-tests` selects this whole package's ignored
tests: every `#[ignore]` in a crate with no app around it IS a Docker cell. The app's SMB cells have no such luxury and
still need the `smb_integration_` prefix the same lane filters them by, which `fixture-lane-coverage` enforces rather
than asks for. The fixture ports come from the environment (`SMB_CONSUMER_GUEST_PORT` and friends, defaulting to smb2's
own 10480 / 10481 / 10488 / 10493); Cmdr's stack publishes 11480+ so both harnesses coexist, and the check runner
exports the override. The full container list: `apps/desktop/test/smb-servers/README.md`.

## Diagnostic privacy

Every log line marks its share (`LogPrivate::volume`), share-relative or mount path (`file_path` / `dir_path`, `path`
where the site can't tell), and account by kind with `cmdr_fs::log_private::LogPrivate`, under its typed key (`{:?}`, so
a share's own quote or newline can't escape the field). The slow-call line is a `Display` (`slow_calls::SlowCallLine`)
so its marked share never becomes a `String`. Operation failures log `error_kind=` (`smb2::ErrorKind`), `nt_status=`
(`smb2::Error::status`), and the backend's `Display` whole in `detail=`, marking the operation's path in it. Reports
redact and cap that field, scrubbing the line's own `share=` from it: `apps/desktop/src-tauri/src/redact/DETAILS.md` §
"External-text fields". Error mapping still receives the original `smb2::Error`. `smb2`'s own lines key and quote every
path, share, name, host, and user, so reports keep its DEBUG (`apps/desktop/src-tauri/src/logging/provenance.rs`).

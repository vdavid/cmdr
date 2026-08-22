# `cmdr-smb` details

## Where the boundary runs, and why

Two boundaries run through this crate, and they answer different questions.

**The backend / app boundary** is the `Volume` trait plus the `VolumeHost` seams: `SmbVolume` implements one and asks
everything else through the other, so nothing here names the app. What stayed up there is what genuinely needs the app —
finding a share and mounting it, deciding when to replace a kernel mount with a direct session, and driving transfers.
`apps/desktop/src-tauri/src/file_system/volume/backends/smb.rs` lists it.

**The protocol / app boundary** is older and sits inside `network/`, which grew as one pile: mDNS discovery, share
listing, mounting, the keychain, the auto-upgrade passes, and the Tauri events, plus a handful of pure functions over
`smb2`'s own types that ended up there for no reason beyond proximity. `src/{types,errors,connection}.rs` is those pure
functions, plus the vocabulary they speak.

The test for the second one is a single question: **can the protocol and its own types answer this?**

Here, because the answer is yes:

- `build_smb_addr` — an `smb2` address string. Strips a `.local` suffix, because `smb2` puts the addr's host component
  into UNC paths (`\\server\IPC$`) and some servers reject `.local` there.
- `is_auth_error` / `classify_error` / `classify_authenticated_error` — reading an `smb2::Error`. Every retry, fallback,
  and sign-in path branches on the first one, and it's the reason the `UpgradeFailure` log can name a rejected password
  even though its own enum has no auth variant.
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
- **Every event and every word** (`network/mod.rs`'s `VolumeConnectionChanged`, `SmbFellBackToOsMount`,
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

`index-crate-isolation` holds this crate to 15 root promises, 4 public modules, and 18 public items inside them, set on
2026-08-22 to exactly what the crate exposed the day the extraction finished — no headroom, so the first addition has to
be argued for.

**A backend's API is the `Volume` trait it implements**, which is `cmdr-fs`'s promise rather than this crate's, so none
of its methods are counted here. Everything that IS counted exists because something outside has to build a share or ask
after one, and a new item should name which of two audiences it serves:

- **The protocol layer**, for `network/`'s discovery and share-listing passes: `build_smb_addr`, the two
  `try_list_shares_*` calls, the three `classify_*` / `is_auth_error` readers, `convert_shares`, and the four vocabulary
  types that cross IPC.
- **Constructing and asking after a share**, for `network/smb_upgrade.rs` and the debug window's diagnostics dashboard:
  `connect_smb_volume`, `SmbVolume` with `new` / `volume_id` / `connection_state` / `diagnostics`, `SmbConnectionParams`
  with its five fields, and `ConnectionState`.

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
  `reconnect`, `state`, `streams`, `scan`, `scan_pool`, `watcher`, `foreground_yield`, and the stateless `mapping`.
  Splitting further needs a responsibility you can name the same way; a line count is not one. `e5ea10d02` reverted four
  splits invented to satisfy the counter, and every one of them had widened a visibility or torn a struct from its trait
  impl to do it. These carry the same `pub(super)` the crate already used and leave no module reaching into another's
  internals.
- **`volume/foreground_yield.rs` answers "should a background transfer stand aside?" WITHOUT a per-device gate.** MTP
  has an explicit holder for its single scarce USB pipe; SMB frames just interleave over one connection, so the signal
  here is time-based instead: the share counts as busy for `TRANSFER_FOREGROUND_IDLE_THRESHOLD` after the last
  navigation on it. Scope is PER VOLUME on purpose, so browsing a local folder never slows a NAS copy.
  `CheckpointStream`'s auto-yield parks on these two functions and `SmbVolume`'s `Volume` foreground-yield methods
  delegate to them.
- **`SmbVolumeInner` is private to `volume/`**, and nothing outside it may name the type. What the app gets is
  `SmbVolume`, `SmbConnectionParams`, `ConnectionState`, and `connect_smb_volume`.

## Which side a test lives on

The suites split by what a cell ASSERTS, never by what it connects to. Both sides connect to the same containers through
`volume::testing`.

- **Here**, if the assertion is about this backend: the `Volume` contract against a real server, the byte path, the
  shared conformance promises, the retirement wiring, the watcher's archive-refresh routing, and every session-free unit
  case. § "The suites" below has the file-by-file map.
- **In the app**, if the assertion is about what the APP does with a share: every cell driving `write_operations`, the
  volume registry, the listing cache, archive routing, or media enrichment. The app's `smb_app_integration_test.rs`
  holds the two that don't fit either heading — a pane close must not kill the watcher (the pane-close IPC is the
  app's), and a local file streams onto the share (`LocalPosixVolume` is the app's).

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

When a hot-path op hits `ConnectionLost` / `SessionExpired`, `handle_smb_result` flips state to `Disconnected` and
`transition_to_disconnected` emits `volume-connection-changed { volumeId, state: "disconnected" }`. The frontend
reconnect manager listens for this event and runs a per-volume backoff cycle (timer-driven, calling the
`reconnect_smb_volume(volumeId)` Tauri command on each tick).

`SmbVolume::do_attempt_reconnect` is the single source of truth for re-establishing the session:

1. Acquires `reconnect_lock` (single-flight: concurrent FE-cycle and lazy-nav callers wait here).
2. If state is already `Direct`, returns Ok cheaply.
3. Tries `build_session()` with the cached `SmbConnectionParams` (the credentials that worked at original connect).
4. If that fails with an auth error, calls `refresh_credentials_from_store` (which re-reads through
   `host.credentials()`, share-level first then server-level) and retries once with the fresh creds. On success, the new
   credentials replace the cached ones via `params.write()`.
5. On success: installs the new client + tree, restarts the watcher with `spawn_watcher` (the prior watcher is cancelled
   via `stop_watcher` first), then `transition_to_direct` flips state and emits
   `volume-connection-changed { state: "connected" }`. Doing the state flip last means observers wake up to a
   fully-installed session.
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
   via `Volume::reconnect_with_credentials` (Tauri `reconnect_smb_volume_with_credentials`), which persists the new
   password server-level (so the next reconnect is silent), updates the in-memory params, and runs
   `do_attempt_reconnect`. If the new creds are also wrong, it re-emits `needs_credentials` — a bad retry re-prompts
   rather than dead-ending.

Credentials are kept in memory for the lifetime of the `SmbVolume` (no security concern: they're already in the
process's address space for every smb2 call). Only re-pulled from the secret store on auth failure, in case the user
updated them.

### Backend-autonomous reconnect and index resume

The FE reconnect manager only runs its backoff while a `FilePane` subscribes to the volume, so before this a background
disconnect (no pane open, or a restart) left an enabled NAS index dark until the user manually re-enabled. Two backend
hooks close that, both funneling through the ONE reconnect path (`do_attempt_reconnect`):

- **`spawn_watcher_death_reconnect(share)`** (in `crates/cmdr-smb/src/volume/reconnect.rs`, kicked from the watcher's
  fatal-error exit). The watcher runs on its own dedicated smb2 session; that session erroring proves the server
  connection broke. A background disconnect may not have touched the MAIN session yet, so it can still read `Direct` —
  meaning `do_attempt_reconnect` would no-op. So the kick FIRST marks the volume `Disconnected`, then drives
  `do_attempt_reconnect` on a bounded, growing backoff (`WATCHER_DEATH_RECONNECT_BACKOFF`: ~6 tries over ~4 min, then
  gives up quietly — never hammering a truly-down server). It re-upgrades its share handle each iteration (a retirement
  or an unmount can land inside any sleep) and stops early on unmount, on a race back to `Direct` (an FE reconnect won),
  or on an auth failure (`PermissionDenied` — the FE "Sign in" flow owns that; retrying risks locking the account).
  Single-flight `reconnect_lock` coalesces it with any concurrent FE reconnect.
- **`indexing::resume_smb_index_if_enabled(volume_id)`** fires at every session-install success — `do_attempt_reconnect`
  (in-place reconnect), `register_smb_volume` (launch/auto-upgrade), and `try_smb_upgrade` (manual "Connect directly").
  It's fire-and-forget (spawns, so it never starts the async indexer under `reconnect_lock` / a registry lock), a no-op
  if the index is already active, and gated on the PERSISTED per-volume state — resume ONLY when the share carries the
  user's enable AND they haven't turned indexing off (the sticky `user_disabled` marker; `disable_drive_index` keeps the
  DB for fast re-enable but records intent). Registering flows through the indexing lifecycle registration bus, so the
  media scheduler resumes enrichment with no scheduler changes. The resumed index loads Stale (we weren't watching while
  disconnected); a rescan restores Fresh. Canonical detail lives in `indexing/DETAILS.md` § "SMB indexing and the
  freshness model"; this bullet is the volume-side trigger map.

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
  the spawned walk task (`indexing/lifecycle/network_scan.rs`); the media pass brackets via a drop-guard in its
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
- **Failure handling.** A listing failing with a typed `ConnectionLost`/`SessionExpired` is retried on a sibling member,
  the dead member is dropped, and a single-flight background task reconnects it (`build_session`, bounded growing
  backoff `POOL_MEMBER_RECONNECT_BACKOFF`; gives up on auth — the MAIN session owns the credential-refresh /
  `needs_credentials` flow). A dead member NEVER transitions the main volume's connection state. A per-directory error
  (permission, not-found) is the same on any connection, so it's surfaced immediately, not retried. If every member is
  momentarily dead, the listing falls back to the main session, which keeps the scan progressing and, if it too is dead,
  yields the `DeviceDisconnected` the scanner's terminal-disconnect path expects. Members open STAGGERED at pool open; a
  rejected Nth session (server session cap) just means the pool runs with fewer.
- **Params are a snapshot.** If the main session refreshes credentials mid-scan (password change), members failing auth
  give up and listings fall back to the main session (documented degradation, not a correctness issue).
- **Reads: compound-only on members** (`open_read_stream_for_scan_impl`). Media enrichment's prefetch reads small HINTED
  files from pool members via the 1-RTT `read_file_compound` (dead member ⇒ sibling retry, exactly like a listing; size
  drift or a too-large file ⇒ main-session streaming). Members deliberately never serve STREAMING reads: a member dying
  mid-stream would surface as a transport error the pool can't transparently retry for the consumer — the main session,
  with its reconnect machinery and connection-state signaling, owns streaming.

## SMB archive push-refresh

The recursive share watcher already refreshes the DIRECTORY listing showing a changed `.zip` (its new size/mtime). On
top of that, `process_event_batch`'s Modified and RenamedNewName handlers call
`maybe_refresh_archive_listings(volume_id, entry_path)`: when `entry_path`'s name is a supported archive
(`archive::has_supported_archive_extension`, the single-source predicate `format_for_name` backs), it fires the same
`caching::refresh_archive_listings` the local `archive::watch` fires, pushing an out-of-band edit of the `.zip` to any
open archive-INNER listing.

Why this is the whole fix, cheaply:

- **Same consumer, same key.** `refresh_archive_listings` scans `LISTING_CACHE` for keys at/inside the archive path and
  re-reads them; `volume_id` here is the parent DRIVE id, which is exactly what archive listings key on, so no rekeying.
  It's a no-op when the path isn't an archive or no inner listing is open, and the watcher already runs for the whole
  volume lifetime — so the only added cost is a re-parse when a `.zip` actually changes AND an inner pane is open.
- **`entry_path` is already normalized.** It's the `to_nfd_display_path` result, so it went through the same
  backslash→slash + NFC→NFD normalization every other cache-facing path in `crates/cmdr-smb/src/volume/watcher.rs` uses.
  Passing the raw event filename would miss the cache.
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
`listing/listing_host.rs::the_archive_refresh_re_reads_the_listings_under_its_path`.

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
that (macOS leaves the mount wedged). Probing is banned, and a direct `SmbVolume`'s own errors carry no errno
(`map_smb_error` sets `raw_os_error: None`), so `volume::note_root_failure` can't fire for one either. Until some
evidence arrives on its own, an SMB volume on a wedged mount still claims OS visibility.

**The watcher follows the active root.** It belongs to the session, not to one mount, and the absolute paths its
notifications carry decide which cached listing they patch. So `SmbVolumeInner::active_mount_path` (a std
`RwLock<PathBuf>`, shared with the watcher task and re-read once per event batch) is updated by a reroot; a watcher
pinned to the old root would keep feeding paths that no longer name anything.

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
the identical request can only fail the identical way. As an `IoError` it inherited the wrong behavior twice over:
`retry.rs::is_retryable` would have burned the full backoff re-sending a hopeless write, and the dialog would have
offered "couldn't copy the file" plus a Retry button instead of the one thing that works (rename it). The typed variant
carries end to end: `friendly_error::kinds::invalid_name` on the listing path (`NeedsAction`, ❌ no retry hint) and
`WriteOperationError::InvalidName` on the write path, which names the failing file so a 5,000-item transfer says WHICH
one to rename. smb2 ≥ 0.18 maps the characters SMB2 forbids outright (`"`, `*`, `:`, `<`, `>`, `?`, `\`, `|`, control
characters, trailing space or period) into the Unicode private-use area, so those copy through fine; what still reaches
this arm is a reserved Windows device name (`CON`, `NUL`, `LPT1`), a name past the server's own length limit, or a
character its filesystem can't store. The status is also in smb2's table now, so the technical-details line reads
`STATUS_OBJECT_NAME_INVALID` rather than bare `0xC0000033`.

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

**Decision**: `SmbVolume` splits session storage: `Arc<Mutex<Option<SmbClient>>>` + `Arc<RwLock<Option<Arc<Tree>>>>`
**Why**: Keeping the session in one `Mutex<Option<(SmbClient, Tree)>>` would force the streaming-read producer and the
compound read/write fast-paths to hold the mutex for the entire transfer, serializing every concurrent copy through it.
`smb2::Connection` is `Clone` (cheap `Arc::clone`, all clones multiplex frames over one SMB session), so splitting the
Tree out lets us briefly lock the client, clone its `Connection`, and release the lock, then drive `Tree::download` /
`Tree::read_file_compound` / `Tree::write_file_compound` on the cloned `Connection` with no lock held. N concurrent
copies on one `SmbVolume` pipeline N operations over the single session instead of queuing on the mutex. Tree lives in a
`RwLock` because we only take read locks in the hot path (cloning an `Arc<Tree>`) and only write on disconnect. The
streaming-write path uses the same clone-and-release shape (see the `write_from_stream` Decision below), so the client
mutex is never held across I/O.

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

**Decision**: we run `smb2`'s deadline and keepalive defaults unchanged, and read none of them as a liveness verdict
**Why**: every wait a request can make is bounded by the crate, so Cmdr needs no timeout layer of its own. A frame gets
20 s to reach the socket (`Error::SendTimeout`); once out, the server gets 30 s of SILENCE (not elapsed time — every
interim `STATUS_PENDING` restarts the clock, so a multi-minute write to a loaded NAS is never cut off), stretched to 6×
that on a connection an ECHO probe has just proven alive. A breach tears the connection down, which is why `retry.rs`
sees a typed `DeviceDisconnected` / `ConnectionTimeout` instead of a hang. The ECHO keepalive (5 s, on by default) only
probes when the wire has gone quiet with work outstanding, so a busy transfer pays nothing for it. ❌ **A missed probe
is NOT evidence of death** and nothing here may treat it as such: a QNAP TS-464 drops probes precisely while it writes
(measured 2026-08-02: 1 of 3 dropped under write load, 0 of 3 idle). The crate agrees — its only death verdict,
`Error::ServerUnresponsive`, needs a request to burn its whole deadline AND the connection to have put nothing at all on
the wire meanwhile. That is also why `SmbVolume::connection_liveness()` stays unimplemented; the full argument and what
`smb2` would have to expose to change it: `write_operations/transfer/DETAILS.md` § "The watchdog ACTS".

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
`delete_partial()` only. The compound FAST-path (`write_file_compound`) is atomic CREATE+WRITE+FLUSH+CLOSE and the
compound DRAIN loop buffers in memory before any handle opens, so neither leaves a streamed partial — those propagate
their error unchanged. The original error always propagates (never `Cancelled`); cleanup is best-effort and never masks
it. Pinned by `smb_integration_write_from_stream_source_error_deletes_partial` (source errors after the first chunk;
asserts the propagated `IoError` and that no file remains at the destination). Don't refactor the owned-writer error
sites into a post-block catch-all that loses the writer — you'd lose the `abort()` and the delete would no-op against
the leaked handle.

**Decision**: `SmbVolume` overrides `scan_for_copy_batch` to pipeline per-path stats over a single SMB session **Why**:
A naive scan phase that loops `scan_for_copy` per top-level source costs N sequential RTTs before the copy phase can
start. For a 100-file copy over a ~60 ms Tailscale link that's ~5 s of serial stats. The override clones
`smb2::Connection` per path under a brief client-mutex acquire (cheap `Arc::clone`, all clones multiplex over the same
SMB session), releases the lock, then drives `tree.stat(&mut conn, path)` on each clone inside a `FuturesUnordered`.
Empty root paths skip the stat. Single-path batches fall through to `scan_recursive` so one-file drag-drops don't pay
the batch machinery cost. Directories found during the stat phase recurse sequentially afterward; parallel directory
recursion is a future enhancement. Measured 6.5× wall-clock win at 100 × 10 KB: 6.11 s → 947 ms. See
`docs/notes/phase4-rtt-investigation.md` for the wire trace. **Oracle layered on top**: before the pipelined-stat block
runs, every input path's parent is checked against the fresh-listing oracle
(`host.listings().authoritative_listing(volume_id, parent)`). Oracle-served paths get their size + `is_directory` from
the cached `FileEntry` and are removed from the leftover set; only the leftover paths go through the pipelined stat.
Decision is per-parent: one batch can mix oracle-served and pipelined-stat paths, and if every path resolves via the
oracle the stat pipeline is skipped entirely.

**Decision**: `SmbVolume` has a compound fast-path in `open_read_stream_with_hint` and `write_from_stream` for files ≤
`max_read_size` / `max_write_size` **Why**: The streaming open+read+close sequence costs 3 RTTs per file. For small
files (typical 10 KB copies on a NAS) that dominates wall-clock at high-latency links (~60 ms RTT → ~180 ms/file just
for protocol overhead, not data). `smb2` already exposes `Tree::read_file_compound` (CREATE+READ+CLOSE in a single
compound frame = 1 RTT) and `Tree::write_file_compound` (CREATE+WRITE+FLUSH+CLOSE = 1 RTT). The copy pipeline feeds
per-file size hints from the pre-copy scan; when the size is known and fits in one READ/WRITE, we take the compound
path. Falls back cleanly to the streaming reader/writer when the hint is missing or the file is too big. Small compound
reads return a `Vec<u8>` wrapped as a single-chunk `InlineReadStream` so the consumer API stays shaped the same. See
`docs/notes/phase4-rtt-investigation.md` for the measurement. The WRITE side's condition is also a DATA-SAFETY contract:
`write_is_single_shot` answers with the same `fits_one_compound_write` the fast path branches on, and the transfer layer
skips its `.cmdr-tmp-*` staging on the strength of that answer. What the backend owes in return (short sources stay on
the compound path, a post-CREATE failure cleans up after itself): `write_operations/transfer/DETAILS.md` § "The
single-shot exemption".

## Gotchas

**Gotcha**: Watcher filenames from SMB use backslashes; must normalize to forward slashes **Why**: SMB servers send
paths like `papers\new-file.txt`. The watcher normalizes these to `papers/new-file.txt` before extracting parent
directories and constructing display paths.

**Gotcha**: Watcher filenames are NFC (from server) but macOS mount paths are NFD **Why**: SMB servers return
NFC-normalized filenames. macOS filesystem paths use NFD. The watcher NFD-normalizes filenames before constructing
display paths used for cache lookups.

## The suites

Which side each one lives on, and why: § "Which side a test lives on" above.

- **Server-free, colocated with the module each covers**: `mapping_test.rs` (`DirectoryEntry`→`FileEntry`,
  `FsInfo`→`SpaceInfo`, `smb2::Error`→`VolumeError`), `state_test.rs` (the binary state machine and how it widens),
  `paths_test.rs` (path translation both ways), `volume_impl_test.rs` (re-rooting and every capability flag),
  `reconnect_test.rs` (the reconnect early-exits, the transitions and the events they suppress, the watch-coverage gate,
  both retirement paths), `streams_test.rs` (the channel-backed `SmbReadStream` consumer and the single-shot write
  promise), `scan_test.rs` (the progress ticker), `retirement_test.rs`, `watcher/archive_refresh_test.rs`, and the
  inline `mod tests` in `foreground_yield.rs` and `scan_pool.rs`. These run by default.
- `host_seam_test.rs` — the PACE of what this backend tells the listing seam, which no type can hold: one call per
  mutation, none per directory entry. Its server-free cells pin the addressing and that an un-stattable creation patches
  nothing; its Docker cell seeds a directory, walks it with a listing and a copy scan, and asserts
  `RecordingListings::change_count` doesn't move, which is what would catch a `notify_mutation` drifting into an entry
  loop. The rule and the instrument: `crates/cmdr-fs/src/volume/host/DETAILS.md`.
- `integration_test.rs` — what a share does with FILES against a real server: core CRUD, single-chunk streaming smoke,
  the copy and conflict scans, space info.
- `session_integration_test.rs` — what the SESSION does: the connection gate the fresh-listing oracle reads, the
  reconnect cycle, the refcounted scan pool, and what a supersede leaves alone.
- `streaming_integration_test.rs` — the whole byte path: `open_read_stream` / `write_from_stream` across progress,
  cancel, cancel-by-drop, multi-chunk files, and the error / partial-cleanup paths with the `ErroringReadStream` double,
  plus the two compound-frame shape assertions.
- `conformance_test.rs` — the `cmdr_fs::volume::conformance` promises, answered by a real server rather than an
  in-process double (SMB has none): `STATUS_DIRECTORY_NOT_EMPTY`, `STATUS_OBJECT_NAME_COLLISION`.
- `test_support.rs` — the session-free builders (a struct-literal `SmbVolumeInner` with no client and no tree), the
  vocabulary every suite globs, and a re-export of `volume::testing` so one `use` covers all three.

Every Docker cell is `#[ignore]`d, so a default run skips it. Start the containers with
`apps/desktop/test/smb-servers/start.sh` and run `cargo nextest run -E 'package(cmdr-smb)' --run-ignored only` — select
by PACKAGE, not by the `smb_integration` name, or a cell named for what it asserts drops out of your run. A new Docker
cell here can be named that way precisely because `desktop-rust-integration-tests` selects this whole package's ignored
tests: every `#[ignore]` in a crate with no app around it IS a Docker cell. The app's SMB cells have no such luxury and
still need the `smb_integration_` prefix the same lane filters them by, which `fixture-lane-coverage` enforces rather
than asks for. The fixture ports come from the environment (`SMB_CONSUMER_GUEST_PORT` and friends, defaulting to smb2's own
10480 / 10481 / 10488 / 10493); Cmdr's stack publishes 11480+ so both harnesses coexist, and the check runner exports
the override. The full container list: `apps/desktop/test/smb-servers/README.md`.

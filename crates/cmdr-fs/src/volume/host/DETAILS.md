# Volume host seam details

## Why the seams exist

`Volume` has always been the API between Cmdr and a storage backend, and it has always lived below the app. What it
never had was exclusivity: a backend could implement the trait and ALSO reach sideways into the listing cache, the
keychain, the volume registry, the analytics client, and the settings module. `local_posix` and MTP still do, and
nothing but this boundary stops the next one; SMB's retrofit had to unpick roughly two dozen such sites.

Turning each reach into a named seam is what makes `cargo check -p cmdr-sftp` a complete verification loop: after the
boundary, a sideways reach is either one of the traits here or a compile error. That's the whole payoff, and it's
overwhelmingly for the backends not yet written. A new backend gets it for free; retrofitting an old one is where the
cost sits.

## Where the seams live, and why not their own crate

They're a module inside `cmdr-fs`, not a `cmdr-volume-host` crate.

Every backend already depends on `cmdr-fs`: it's where `Volume` lives, and where every type a backend speaks in lives
(`FileEntry`, `DirectoryChange`, `MutationEvent`, `CancellationToken`, `ArchiveFormat`). A separate crate would depend
on `cmdr-fs` for all of them, so it would add a hop without adding independence, and it would create a recurring
judgment call — "is this new type vocabulary or seam?" — with the answer split across two manifests. It would also need
its own entry in `index-crate-isolation`'s guard list, while a module inside `cmdr-fs` inherits that crate's existing
"no `tauri`, no `cmdr`" guarantee unchanged.

The one real cost is that `cmdr-fs` now carries `tokio` with `rt` + `rt-multi-thread`, for the runtime seam. That's
accepted: the `Volume` trait is already async top to bottom, so every consumer already has a runtime; what's new is
naming its handle.

The precedent points the same way. `cmdr-index` put its host seams inside the crate that needs them
(`crates/cmdr-index/src/indexing/host/`), not beside it.

## The shape every seam takes, and why it's a value

The app builds ONE `VolumeHost` at startup and hands a clone to every backend it constructs. A backend stores it as a
field.

`cmdr-index` does the opposite — process-wide statics behind accessors — and its own docs are explicit that this is a
concession to ~50 pre-existing globals rather than the shape it would choose fresh. Backends have no such legacy: the
app already constructs every one of them (`connect_smb_volume`, `manager/archive_routing.rs`), so there's a constructor
to thread a value through.

What that buys:

- **No install-and-restore guards.** A test builds a `VolumeHost` with fakes and passes it in. Nothing is process-wide,
  so nothing has to be serialized against the rest of the test binary and no `TestSinkGuard` has to exist.
- **No `cfg(test)` trap in the seam itself.** A test-only global slot is exactly the kind of item that silently
  disappears when a crate becomes someone's dependency. There isn't one.
- **Two hosts can coexist**, which is what a test that drives two backends against different fakes needs.

The fakes are still `#[cfg(any(test, feature = "testing"))]` — the crate rule, for the reason `cmdr-fs/CLAUDE.md` gives
— but they're plain `pub` structs, not machinery for reaching a global.

## The dispatch rule

**A seam trait object may be called per mutation, never per directory entry.**

Every seam is `dyn`-dispatched. `Volume` already is, so no NEW dynamic dispatch appears at the volume level, and thin
LTO at the workspace root is what kept the comparable index extraction's hot paths within ±2%. That result holds for a
call made when a file lands, a session drops, or a scan starts. It does not hold for a call made once per entry while
walking a directory with a quarter of a million of them, where the indirect call, the `Arc` traffic, and the lost
inlining are all real.

So: take one answer before the loop and carry it. A directory of 250 000 entries produces ONE `FullRefresh`, not 250 000
`Added`s. A concurrency limit is read once per batch dispatch, not once per file in the batch.

**The instrument is a counter, not a review.** `RecordingListings::change_count` counts every call, so a backend test
that walks four directories of 250 files and asserts a handful of changes fails loudly at 1 000. `cmdr-index` pins its
equivalent the same way (`pace_tests::the_policy_is_consulted_per_listing_not_per_entry`), and
`host_test.rs::the_change_counter_separates_per_mutation_from_per_entry` shows the shape to copy.

**When a per-entry answer really is needed**, that's a signal the seam is the wrong shape, not that the rule should
bend. Hoist it (one snapshot per batch), or push the loop across the seam so the host does the per-entry work behind a
single call.

## Seam by seam: what each one replaces

Measured against the SMB backend (`crates/cmdr-smb/src/volume/**` plus `crates/cmdr-smb/src/volume/watcher.rs`), the
most entangled one, with archive, MTP, and local POSIX checked for anything SMB doesn't exercise.

### `ListingHost`

- `directory_changed` ⇐ `listing::caching::notify_directory_changed`. SMB calls it from three arms of
  `mutation.rs::notify_mutation_impl` and from ten of the watcher's (nine in the event batch, one for an overflow
  `FullRefresh`); MTP calls it from four. One call covers three host concerns — the panes, the file index, the
  cloud-badge cache — so a backend never learns which of them exist.
- `authoritative_listing` ⇐ `listing::caching::try_get_authoritative_listing`. The fresh-listing oracle, consulted per
  unique parent directory by SMB's and MTP's batch scans.
- `refresh_archive_listings` ⇐ `listing::caching::refresh_archive_listings`. Two callers, both watching the drive that
  HOLDS an archive rather than the archive itself: the local archive content watch and the SMB share watcher.
- `volumes_with_open_listings` ⇐ `listing::volume_ids_with_listings`. For a DEVICE backend, one level above the volumes
  it serves: an MTP phone's event names a bare PTP handle, and resolving it costs a round trip per storage, so the
  backend asks which storages a pane is showing and searches only those.

**`DirectoryChange::Replaced` is the variant a device backend reports.** It carries the directory's new contents, so the
host sorts them the way each pane sorts, diffs, and patches. `FullRefresh` asks the host to do the read instead; report
`Replaced` when the entries are already in hand. Both are ONE call however many entries came back, which is what keeps a
device event out of a per-entry loop.

**A `directory_changed` call answers nothing**, `Replaced` included, so a backend can't learn whether a targeted refresh
found a pane to land on. That's deliberate: the seam is fire-and-forget in both directions, and a backend that wants a
safety net reports `FullRefresh` for the volume, which the host fans out to every listing on it.

**Two of the five listing functions the survey listed are NOT seams.** `find_listings_for_path_on_volume` and
`patch_listing_after_local_mutation` have exactly one caller between them, `local_posix.rs`, which is permanently
app-resident. `patch_listing_after_local_mutation` is by definition local: it `std::fs`-stats the changed entry, which a
backend on a protocol can neither do nor want (it can build the entry from a cheaper protocol reply). And SMB's own
`listing_watch_coverage` answers from its connection state and watcher handle, not from the listing cache, so it needs
neither. Adding them would have meant two trait methods no extractable backend could ever call.

**`FullRefresh` re-enters the backend, but never synchronously.** The host answers a `FullRefresh` by re-reading the
directory through `Volume::list_directory` — the backend that reported it. That dispatch is spawned, not inline, so a
backend can report a change while holding its own lock. Don't rely on that being an accident: report the change, then
release.

### The runtime handle

⇐ `tauri::async_runtime::spawn`, which is `tokio::spawn` on the app's runtime. Backends use it rather than
`tokio::spawn` because their watchers run on OS threads with no reactor: `notify`'s watch thread (archive), the SMB
watcher's own thread, and the app's synchronous `setup()` hook all panic under `tokio::spawn`. That constraint is real
and documented in `apps/desktop/src-tauri/src/file_system/CLAUDE.md`; the seam preserves it rather than fixing it.

A `Handle` rather than a trait, because a backend needs the `JoinHandle` back: SMB's `stop_watcher` aborts the task it
spawned. Wrapping that in a trait would be rebuilding tokio's API, worse.

### `VolumeEventSink`

⇐ a global `AppHandle` the SMB backend used to hold, emitting `network::VolumeConnectionChanged` through
`tauri_specta::Event`. The seam carries `VolumeConnection`, a three-variant enum; the payload struct, its derives, and
the wire enum it serializes stay app-side. The two enums meet in exactly one match, in `events/volume_mapping.rs`.

`NeedsCredentials` is worth its own variant even though SMB's internal state machine is binary
(`Direct ⇄ Disconnected`): it's the one transition the backend must NOT retry its way out of, and a string would let a
UI copy edit change what a backend means.

### `CredentialStore`

⇐ `network::keychain::{get_credentials, save_credentials}`. Three sites: the session builder's narrow-then-wide lookup
(`session.rs::refresh_credentials_from_store`, which tries share-level then server-level) and the sign-in path's save
(`reconnect.rs::do_reconnect_with_credentials`, which saves server-level so one password covers every share).

`(service, scope)` generalizes that pair: `(hostname, share)` for SMB, `("host:port", username)` for SFTP, and
`(endpoint, bucket)` for S3. `secret` rather than `password` because S3's is an access key.

### `HostKeys`

⇐ nothing that existed before: SFTP is the first backend whose security depends on recognizing a server across sessions,
and SMB had no equivalent. The app answers it from a durably-written `known-sftp-hosts.json`
(`network::sftp_host_keys::AppHostKeys`), mirroring `CredentialStore`'s shape for the same reason —
`config::durable_write_json` is app-side, and `index-crate-isolation` forbids naming `cmdr` from a guarded crate.

**Why the lookup is keyed by `(host, port, algorithm)`, and why that alone would be a hole.** A healthy server may hold
several host keys and present whichever the negotiation lands on, so a store keyed by host alone reports a CHANGED key
on a working server — training people to click through the one alarm that matters. Keying by the triple fixes that and
opens a worse hole: an attacker offering a type we hold no entry for lands on the UNKNOWN path and collects a one-click
approval. So the seam also answers `trusted_algorithms(host, port)`, which the backend pins its key-exchange preferences
to. Both halves, or neither. This is what OpenSSH does.

**Fingerprints rather than keys.** The seam speaks the OpenSSH `SHA256:…` string, so no SSH crate reaches into `cmdr-fs`
and the value the store holds is the one a human compares against `ssh-keygen -lf`.

**Rejected: resolving trust app-side and handing the backend a verdict** through its connection parameters, which would
have needed no seam at all. It works exactly once. A verdict is decided before the dial, and the moment that matters
most is the one the app isn't in: a reconnect hours later, from a backoff loop, against a server whose key changed while
nobody was watching. A backend that can't re-ask has to either refuse every reconnect or trust whatever answers, and
both are wrong. So the seam is a QUESTION the backend asks whenever it dials, not an answer it is handed.

**Detached means trust-nothing**, and that asymmetry is deliberate: every other seam degrades to a no-op, but a
credential-shaped seam that degraded to "yes" would make a security regression invisible in exactly the tests meant to
catch it. The cost is that a no-op `record` leaves an approve-then-reconnect harness looping forever on "unknown →
approve → still unknown", which is why `InMemoryHostKeys` exists alongside and actually remembers. A fixture uses that
one; a bench uses the detached default.

### `IndexNotifier`

⇐ `index_host::index().on_watch_gap` (three sites in `crates/cmdr-smb/src/volume/watcher.rs`: both setup failures, the
overflow arm, and the fatal-error exit) and `.resume_after_reconnect` (one, in `reconnect.rs`, fired while the reconnect
lock is held).

**Why a trait rather than a `cmdr-index` dependency.** A backend crate could import the index handle — both are
Tauri-free crates, and `crates/cmdr-smb/src/volume/watcher.rs` already imports `cmdr_index::{WatchGap, WatchScope}`
today. It shouldn't: depending on the index would put a quarter of the codebase inside `cargo check -p cmdr-sftp` for
the sake of two method calls, which is the exact inner-loop win the crate boundary is being built to get. So `WatchGap`
here is the seam's own, and the app's adapter maps it.

**`WatchScope` isn't here**, but both of its shapes are. A volume backend reports per volume id through `watch_gap`; a
DEVICE backend, whose one session carries several volumes, reports per device id through `device_watch_gap`, and the
adapter wraps each in the scope the index spells. MTP is the case: one PTP session per phone, so a reset invalidates
every storage on it at once, and looping `watch_gap` over the device's volumes would need a list the backend doesn't
have and can't read from a session that just died. `device_watch_gap` defaults to a no-op, so only a device backend ever
names it.

`device_object_changed` / `device_object_removed` ⇐ `index_host::index().on_device_object_changed / _removed`, the MTP
event loop's two index reaches. Keyed by DEVICE rather than by volume, because one PTP session carries every storage on
the phone and the handle namespace spans them all. They carry the bare protocol handle and nothing else: resolving it
first would be a device round trip per event, and the index may be mid-walk and about to read the object anyway, so it
owns the routing. Both default to no-ops, so a host with no device index answers them by existing.

The `#[cfg(any(target_os = "macos", target_os = "linux"))]` guards that surround the app's own index call sites don't
cross the seam: `Index::on_watch_gap` compiles on every platform and gates its own MTP arm, so both the adapter and a
backend call unconditionally.

### `BackendSettings`

⇐ `file_system::smb_concurrency()`, read on every batch-copy dispatch by `SmbVolume::max_concurrent_ops()`. Live by
design: the user moves a slider and the next batch picks it up without remounting, which is why it's a seam call and not
a constructor argument.

The `backend` argument is a settings namespace (`"smb"`, `"sftp"`, `"s3"`), not a classification — ❌ nothing branches
on it, on either side; the app resolves it through a namespace-keyed table (§ "Where the app answers each seam").
Connection parameters (address, port, region, passive mode) go through the backend's own constructor, typed as that
backend wants them.

### `UserActivity`

⇐ `priority::foreground::global()`, via `crates/cmdr-smb/src/volume/foreground_yield.rs`. The seam reports three raw
signals and no decisions: how many foreground operations are in flight on the volume, how long it has been quiet, and a
subscription that fires whenever either moves. The threshold stays at the call site, because how long counts as "busy"
belongs to the work standing aside — a transfer that parks outright wants 500 ms, an index scan that merely narrows
wants far longer, and each writes its constant beside its reasoning.

The scope is per volume, and that's the whole point: browsing a local folder must never slow a copy off a NAS.

The decisions are free functions over the seam, so no consumer can compose its own and be wrong in a case it never
tests: `volume_busy_for_user` (the rule), `volume_idle_for` (the decaying half alone), and `wait_until_volume_free`.
That wait is the shape worth knowing: a lease has an owner, so its end is an event to sleep on; a timestamp going stale
has nobody to announce it, so the leftover window is ONE sleep to a computed deadline. It takes its subscription BEFORE
reading the signals, which is what makes a release landing in between impossible to lose (the subscription carries a
version, not a permit), and it re-reads both halves on every wake, so a second listing starting or a navigation landing
mid-window simply re-parks with a new deadline.

### `AnalyticsSink`

⇐ `analytics::posthog::capture`, one site (`connect_smb_volume` records that a direct SMB session came up). The
`&[(&str, &str)]` shape is deliberate: consent, dev/CI suppression, and batching are host business, and there's no way
to hand the seam a struct and hope its serialization is PII-free.

## What deliberately isn't a seam

- **Registration.** Backends never register themselves; the app's wiring modules do (`mtp/volume_wiring.rs`,
  `network/smb_upgrade.rs`). Already solved, no seam needed.
- **Cancellation, progress, and error mapping.** `CancellationToken`, `ListingProgress`, and `friendly_error/` are
  already in `cmdr-fs`, spoken by the `Volume` trait itself.
- **Anything computable from a `&str`.** Volume id vocabulary (`smb_volume_id`, `mtp_ids`), archive detection
  (`archive_format::has_supported_archive_extension`), firmlink normalization. If you can compute the answer without
  asking anyone, it's vocabulary — move it down, don't make it a method.
- **Protocol helpers that happen to live in the app.** `build_smb_addr` and `is_auth_error` read like host reaches and
  aren't: they're pure functions over SMB's own types, and they now live in `crates/cmdr-smb/` with the share-listing
  vocabulary. Discovery, upgrade, mounts, the keychain, and the UI wiring stayed in `network/`. The boundary test and
  the full split: `crates/cmdr-smb/DETAILS.md`.
- **User-facing prose.** A backend emits typed values; the host renders every word. Diagnostic strings for `log::` are
  fine and stay English.

## What doesn't fit the seam set

The honest list, and the most useful thing in this document.

### The two registry reach-backs need no seam: `Retirement` plus a `SelfHandle`

`reconnect.rs` and `crates/cmdr-smb/src/volume/watcher.rs` used to call `get_volume_manager().get(volume_id)` from
inside the backend, and a `VolumeRegistry` seam looks like the obvious replacement. It isn't: both are the backend
asking about ITSELF, and going out through the registry to ask makes the answer wrong in two directions.

- An id resolves to whatever holds it NOW, so a watcher dying in the window around a supersede drove the SUCCESSOR's
  state. That is why an `instance_id` counter had to exist, and why the resolve had to downcast to compare it.
- A volume the registry has DROPPED still resolves to nothing only if nobody holds it. A running copy holds an `Arc` for
  its whole duration, so a dropped share kept its reconnect loop alive.

**The shape.** `Retirement` is a one-way flag the volume publishes through `Volume::retirement`. `SelfHandle<T>` is a
`Weak<T>` to the state a backend's background work hangs off, where `T: Retires` carries the same flag, and `live()`
answers only while the state is both allocated and unretired. The handle holds nothing but the `Weak`, so it cannot be
built over a flag other than the one the state publishes. Identity becomes a pointer, so the counter and both downcasts
are gone, and this removes the last thing that would have forced a seam to hand back an `Arc<dyn Volume>` for a backend
to downcast to its own concrete struct.

**Who writes the flag, and why that split.** The registry is the only writer of "you left", set at the two ways out of
it (`VolumeManager::unregister` and `roots::remove_root`'s last-mount arm). A backend writes it only for a hand-over it
performs itself, from `Volume::on_superseded`, because the id lives on under the successor and the registry sees a
replace rather than a removal.

**Never on a replace, and pinned by tests rather than by a rule.** A re-root hands the id to another instance of a share
that is still live and still watching, so retiring there stands a healthy volume down. `manager.rs`'s
`replacing_a_volume_at_its_own_root_retires_nobody` and `roots.rs`'s `promoting_a_surviving_mount_retires_nobody` both
go red on an over-eager fix.

**Scope it to the state, not to the instance.** SMB keeps the flag on `SmbVolumeInner`, the share-scoped half, because a
re-rooted instance is the same share, the same session, and the same watcher. A per-instance flag would retire the
share's watcher on a promotion that was supposed to save it.

**Retirement is one-way, so a comeback is a fresh instance.** Every re-register path already builds one. A backend that
cached and re-registered the same `Arc` would hand the registry a volume that is permanently retired.

### Visibility that has no cross-crate equivalent

`pub(in crate::file_system::volume)` has no cross-crate spelling, so an item wearing it faces one of two answers when
its backend moves: it becomes `#[cfg(any(test, feature = "testing"))] pub`, a real widening of the public surface, or
the test that uses it moves into the crate with it.

**Moving the test is the default, and widening is the exception that has to be argued.** SMB granted exactly one:
`detach_session_for_test`, because the app's scan-oracle cell that calls it asserts on the app's fresh-listing oracle
and belongs on that side. Everything else went the other way, `SmbVolumeInner` included, which is now private.

MTP's two, `test_hooks` and `test_window`, are one grant with the same argument. They're a `volume::testing` module of
three functions (`list_directory_call_count`, `reset_list_directory_call_count`, `set_read_window`), and the cell that
needs them across the boundary is the app's fresh-listing oracle again: it asserts the ORACLE issued no listing, which
is an app claim, and no wrapper `Volume` can see the call because the scan reaches `MtpVolume::list_directory` by static
dispatch. The module hands out two numbers and takes one; ❌ it must not grow into a way to read the backend's state,
which is the same shape `cmdr_smb::volume::testing` holds to.

The `cfg` half is not optional: `cfg(test)` is set only for a crate's own test target, so leaving it would make the item
vanish from a consumer's test build. This project has been bitten by that three times.

### Test modules reached through `use super::*`

A backend whose `mod.rs` doubles as its suites' prelude glob makes the move hard to plan: what the glob pulls in isn't
determinable without building, so the split can't be sized in advance. It's the biggest unknown MTP still carries.

SMB's answer, and the one to copy: the prelude moves to a `test_support.rs` beside the suites, and `mod.rs` goes back to
importing what it uses. Deleting the `#![allow(unused_imports)]` the glob needed is what makes a dead import in the
backend a finding again.

### The archive rustdoc link

`cmdr-archive`'s `volume.rs` carried an intra-doc link to `VolumeManager::resolve`, an app symbol no backend crate can
name, and it would have failed `desktop-rust-rustdoc`. It's prose now. Cheap to fix, but the kind of thing that only
surfaces at the end of a move: check every `[\`Type::method\`]` link for an app-side target before moving a backend.

## Writing a new backend

1. Depend on `cmdr-fs`. Implement `Volume`. Take a `VolumeHost` in your constructor and store it.
2. Declare your settings namespace once: `const BACKEND: BackendName = "webdav";`.
3. Every mutation you perform reports itself through `listings().directory_changed`, including writes that arrive via
   `write_from_stream`. A watcher event is not a substitute: watchers on network protocols are lossy under load.
4. If you have a watcher: report every exit through `indexing().watch_gap`, including the ones that look like setup
   failures rather than deaths. A never-connected watcher leaves the index just as blind as a dead one.
5. If you have no watcher (S3 has none), leave `Volume::listing_watch_coverage` at its `None` default and never call
   `authoritative_listing`. Claiming freshness you can't keep is how a pre-flight scan reuses a stale cache.
6. Spawn through `host.runtime()`, never `tokio::spawn`.
7. Read `settings().max_concurrent_operations(BACKEND)` per batch dispatch, not once at construction. Until your
   namespace has a row in `file_system::backend_settings`, you get a conservative built-in rather than someone else's
   number, so a forgotten row costs speed and nothing else.
8. Report connection transitions through `events()`, comparing against your previous state so a server that's down
   doesn't emit one per failed operation.
9. If anything you spawn outlives one call (a watcher, a reconnect loop), keep a `Retirement`, publish it from
   `Volume::retirement`, and reach your own state through a `SelfHandle` rather than an id you look up. Without it the
   registry has nowhere to write "you left", and your background work keeps running against a volume the app has
   forgotten. § "The two registry reach-backs" has the full rationale.
10. Write your tests against the fakes here. Assert on `change_count` as well as contents: that's what keeps a seam call
    from drifting into a per-entry loop. Three backends carry a `host_seam_test.rs` to copy from —
    `crates/cmdr-smb/src/volume/host_seam_test.rs` and `crates/cmdr-sftp/src/volume/host_seam_test.rs` both seed a real
    directory, walk it every way the backend can (listing, copy scan, conflict scan), and assert the counter stays put,
    and `crates/cmdr-archive/src/watch/host_seam_test.rs` does the archive-refresh half. The SFTP one is the sharpest
    instrument of the three: that backend has no watcher, so `notify_mutation` is the counter's ONLY producer.

## Where the app answers each seam

`apps/desktop/src-tauri/src/volume_host.rs` builds the one host and hands it out; each answer is a small adapter next to
the subsystem that can actually give it.

- `ListingHost` ⇒ `file_system::listing::listing_host::AppListings`
- `VolumeEventSink` ⇒ `events::volume_mapping::TauriVolumeEvents`
- `CredentialStore` ⇒ `network::credential_store::KeychainCredentials`
- `HostKeys` ⇒ `network::sftp_host_keys::AppHostKeys`
- `IndexNotifier` ⇒ `index_host::VolumeIndexNotifier`
- `UserActivity` ⇒ `priority::host_policy::AppUserActivity`
- `AnalyticsSink` ⇒ `analytics::volume_sink::PostHogVolumeAnalytics`
- `BackendSettings` ⇒ `file_system::backend_settings::AppBackendSettings`
- the runtime ⇒ the app's own `tauri::async_runtime` handle, so there's one thread pool

Both signatures the design left open resolved to "no change" against the real app. `authoritative_listing`'s owned
`Vec<FileEntry>` is what the cache can give: it clones the entries under its read lock and drops the lock before
anything crosses a volume boundary, so a borrow-shaped variant would have to hold that lock across the caller's work.
And `CredentialStore`'s blocking contract matches the keychain wrapper exactly, which is synchronous down to the OS
call, so an async variant would only wrap a blocking call in a future.

Three things the adapters found that aren't trait shape, and matter to whoever wires a backend up:

- **The connection event is backend-neutral.** `network::VolumeConnectionChanged` / `volume-connection-changed` is what
  the frontend's reconnect manager subscribes to, and every backend's transitions ride it. A second connecting backend
  reuses the channel and inherits the banner, the backoff, and the sign-in prompt; ❌ don't add a parallel backend-named
  event.
- **`AppBackendSettings` resolves through a table keyed by the namespace**, never a `match` on it. Two rows today, and a
  row says where a backend's number COMES FROM rather than promising a user can change it: `"smb"` ⇒ the
  `network.smbConcurrency` setting, which is SMB's alone (label, help text, and table row all say so), and `"sftp"` ⇒ a
  constant, because four operations share one SSH connection there and a second connection means a second
  authentication. A namespace with no row gets a conservative built-in (2), because the day someone adds a backend and
  forgets its row is the day that number ships, and an FTP server capped at four connections answers the fifth with a
  ban. Adding a backend means adding its row, exposed knob or not; which row a backend may read is settled in
  `file_system::backend_settings`' own doc comment.
- **`IndexNotifier` needs no platform fork.** `Index::on_watch_gap` compiles everywhere and cfg-gates its own MTP arm,
  so the adapter is unconditional; only the app's own call sites carry
  `#[cfg(any(target_os = "macos", target_os = "linux"))]`.

`cmdr-archive`, SMB, and SFTP are all fully on the seams: SMB takes a `VolumeHost` in `connect_smb_volume` and keeps it
on the share-scoped `SmbVolumeInner`, SFTP takes one in `connect_sftp_volume`, and neither reaches anything in the app
directly. `local_posix` and MTP still call `listing::caching`, `network::keychain`, and the rest, and stay app-resident
on purpose. Which backends move and which don't: § "Which backends move" below.

**Only the event sink needs a running app.** `volume_host::host()` hands out the app's real adapters even before
`install()`, leaving only the frontend channel (and the app's runtime) unwired, because the listing cache, secret store,
index handle, priority tracker, and settings are all process-global. That's what lets an app-side backend test drive a
real volume and assert on the real listing cache without standing a Tauri app up.

## What a backend crate buys, and the two things it doesn't

Worth stating plainly, because both non-wins are the ones people expect first and the real win is easy to miss.

**`pnpm check` will NOT get faster, and no amount of extraction changes that.** The cargo lanes run `--workspace` and
stay there, and per-package `-p` lanes are closed for reasons that have nothing to do with where the code lives:
`scripts/check/DETAILS.md` § "The Rust input blocks" carries the contract and the measurements. ❗ Read it before
offering a check-runner win as an extraction payoff: re-measured, a per-package split nets a LOSS on the Rust budget,
and it resolves features differently from the workspace build it shares `target/` with. What a crate boundary does buy
the runner is narrower cache scope for the source SCANNERS, whose input blocks are per-member: code that leaves the app
tree stops busting the app-tree lanes.

**Full app builds get slightly SLOWER after a backend edit**, because the app still relinks:
`docs/notes/index-extraction-baseline.md` measured +11% for "index edit, then `cargo build`". Expect the same sign for a
backend.

**The win is the scoped inner loop, and it comes from not compiling the app** — so it transfers whole to a small crate.
The index extraction measured `cargo check --lib` −83% (4.35 s → 0.75 s) and `cargo test --lib --no-run` −85% (23–30 s →
3.55 s). `cargo check -p cmdr-archive` compiles ~8k lines where it used to compile 332k. That structural ratio is what
makes `cargo check -p cmdr-sftp` a complete verification loop for an agent that never opens the app crate, and it's the
whole reason the seams exist. Release builds may also gain modestly (the index took a clean release build 214 s → 188 s,
−12%, because more crates give cargo more codegen units), but archive plus SMB are ~7% of the tree against the index's
28%, so don't expect that figure again.

The archive pilot's own timings are in `docs/notes/archive-extraction-baseline.md`, and most of them are **withdrawn**:
the machine was running several concurrent workspace builds on a near-full data volume. What survives is the inner-loop
ratio, as an order-of-magnitude reading.

## Which backends move

`cmdr-archive` and `cmdr-sftp` shipped. Every backend written from here (S3, WebDAV, whatever follows) is a crate from
day one, which costs almost nothing extra and gets the full benefit. FTP is not on that list: the protocol was weighed
and parked, and `docs/notes/ftp-crate-evaluation-2026-08-22.md` is the argument. The retrofits are judged one at a time,
because retrofitting is where all the cost sits:

- **SMB shipped too**, in `crates/cmdr-smb/`, and it's the worked example for a RETROFIT the way archive is for a
  greenfield crate. All four structural problems this document enumerates came up and all four are answered: the
  `pub(in …)` visibility, the `use super::*` test modules, the `network/` split, and the registry reach-backs. It cost
  what the archive move predicted: the coupling was roughly two dozen sites against archive's three, and the real work
  was SPLITTING the ~5,300 lines of suites that had grown to cover both sides of the boundary, not rewriting paths.
  Which side each cell landed on, and the rule that decided it: `crates/cmdr-smb/DETAILS.md` § "Which side a test lives
  on".
- **`cmdr-sftp` is the greenfield proof that the seam set isn't SMB-shaped**, which is the thing a second protocol was
  ever needed for. It was a crate from day one against a protocol sharing nothing with SMB, and **not one existing seam
  signature moved to fit it**. What it needed that didn't exist became a NEW seam, `HostKeys` (§ "Seam by seam") —
  growth rather than a break, because no earlier backend's security depended on recognizing a server across sessions.
  The backend itself: `crates/cmdr-sftp/DETAILS.md`.
- **MTP is the last retrofit, and it is under way.** It is the one backend still reaching sideways (the listing cache at
  four sites, the index handle, `tokio::spawn`, and a `tauri::AppHandle` that emits seven frontend events from inside
  the session layer). The three things that once read as permanent refusals each have an answer, and they are the same
  three answers SMB gave: a crate-local typed event trait for the derives, `any(test, feature = "testing")` for the
  `cfg(test)` gates, and one argued visibility widening for `test_hooks`. Reasons in full:
  `apps/desktop/src-tauri/src/file_system/volume/backends/DETAILS.md` § "Per-backend decisions"; the plan and its
  milestones: `docs/specs/mtp-crate-extraction.md`.
- **`local_posix` stays app-resident permanently**, and that refusal is not the same shape as MTP's: the git portal is
  implemented as `LocalPosixVolume` hooks, so extracting the backend means extracting git or inventing a seam with one
  implementor forever. The reasons are in the same section, because that's where someone proposing "let's complete the
  set" will be standing.

**Expect an extraction to surface latent defects**, and treat that as the point rather than a surprise. Archive's move
found two: seven `.unwrap()`s that were legal only while the file was `cfg(test)` and became clippy `unwrap_used`
violations as a `testing`-feature lib item, and a rustdoc intra-doc link to a function that no longer existed, which
`desktop-rust-rustdoc` had never seen while the item was buried in the app. Check every ``[`Type::method`]`` link for an
app-side target before moving a backend; an intra-doc link to an app symbol is unnameable from a backend crate and has
to become prose.

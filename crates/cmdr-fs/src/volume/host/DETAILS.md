# Volume host seam details

## Why the seams exist

`Volume` has always been the API between Cmdr and a storage backend, and it has always lived below the app. What it
never had was exclusivity: a backend could implement the trait and ALSO reach sideways into the listing cache, the
keychain, the volume registry, the analytics client, and the settings module. The mature SMB backend does that across
roughly two dozen sites, and nothing stops the next one.

Turning each reach into a named seam is what makes `cargo check -p cmdr-ftp` a complete verification loop: after the
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

Measured against the SMB backend (`file_system/volume/backends/smb/**` plus `smb_watcher.rs`), the most entangled one,
with archive, MTP, and local POSIX checked for anything SMB doesn't exercise.

### `ListingHost`

- `directory_changed` ⇐ `listing::caching::notify_directory_changed`. SMB calls it from four arms of
  `volume_impl.rs::notify_mutation` and from nine arms of the watcher's event batch; MTP calls it from four. One call
  covers three host concerns — the panes, the file index, the cloud-badge cache — so a backend never learns which of
  them exist.
- `authoritative_listing` ⇐ `listing::caching::try_get_authoritative_listing`. The fresh-listing oracle, consulted per
  unique parent directory by SMB's and MTP's batch scans.
- `refresh_archive_listings` ⇐ `listing::caching::refresh_archive_listings`. Two callers, both watching the drive that
  HOLDS an archive rather than the archive itself: the local archive content watch and the SMB share watcher.

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

⇐ `smb/events.rs`, which today holds a `OnceLock<Mutex<Option<AppHandle>>>` set from `lib.rs::setup` and emits
`network::VolumeConnectionChanged` through `tauri_specta::Event`. The seam carries `VolumeConnection`, a three-variant
enum; the payload struct, its derives, and the wire enum it serializes stay app-side. The two enums meet in exactly one
match, in `events/volume_mapping.rs`.

`NeedsCredentials` is worth its own variant even though SMB's internal state machine is binary
(`Direct ⇄ Disconnected`): it's the one transition the backend must NOT retry its way out of, and a string would let a
UI copy edit change what a backend means.

### `CredentialStore`

⇐ `network::keychain::{get_credentials, save_credentials}`. Three sites: the session builder's narrow-then-wide lookup
(`session.rs::refresh_credentials_from_store`, which tries share-level then server-level) and the sign-in path's save
(`reconnect.rs::do_reconnect_with_credentials`, which saves server-level so one password covers every share).

`(service, scope)` generalizes that pair: `(hostname, share)` for SMB, `(endpoint, bucket)` for S3, `(hostname, None)`
for FTP. `secret` rather than `password` because S3's is an access key.

### `IndexNotifier`

⇐ `index_host::index().on_watch_gap` (three sites in `smb_watcher.rs`: both setup failures, the overflow arm, and the
fatal-error exit) and `.resume_after_reconnect` (one, in `reconnect.rs`, fired while the reconnect lock is held).

**Why a trait rather than a `cmdr-index` dependency.** A backend crate could import the index handle — both are
Tauri-free crates, and `smb_watcher.rs` already imports `cmdr_index::{WatchGap, WatchScope}` today. It shouldn't:
depending on the index would put a quarter of the codebase inside `cargo check -p cmdr-ftp` for the sake of two method
calls, which is the exact inner-loop win the crate boundary is being built to get. So `WatchGap` here is the seam's own,
and the app's adapter maps it.

**`WatchScope` isn't here.** Its `Device` variant exists for MTP, where one PTP session carries several volumes and a
reset invalidates all of them at once. That's the transport layer's shape, and MTP is app-resident. A volume backend
reports per volume id; the adapter wraps it.

The `#[cfg(any(target_os = "macos", target_os = "linux"))]` guards that surround the app's own index call sites don't
cross the seam: `Index::on_watch_gap` compiles on every platform and gates its own MTP arm, so both the adapter and a
backend call unconditionally.

### `BackendSettings`

⇐ `file_system::smb_concurrency()`, read on every batch-copy dispatch by `SmbVolume::max_concurrent_ops()`. Live by
design: the user moves a slider and the next batch picks it up without remounting, which is why it's a seam call and not
a constructor argument.

The `backend` argument is a settings namespace (`"smb"`, `"ftp"`, `"s3"`), not a classification — ❌ nothing branches on
it, on either side; the app resolves it through a namespace-keyed table (§ "Where the app answers each seam").
Connection parameters (address, port, region, passive mode) go through the backend's own constructor, typed as that
backend wants them.

### `UserActivity`

⇐ `priority::foreground::global().idle_for_volume(...)`, via `smb/foreground_yield.rs`. The seam reports the raw signal;
the threshold stays at the call site, because how long counts as "busy" belongs to the work standing aside — a transfer
that parks outright wants 500 ms, an index scan that merely narrows wants far longer, and each writes its constant
beside its reasoning.

The scope is per volume, and that's the whole point: browsing a local folder must never slow a copy off a NAS.

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

### The two registry reach-backs, which need no seam after all

`reconnect.rs::still_the_same_volume` and `smb_watcher.rs::stat_via_volume` both call
`get_volume_manager().get(volume_id)`, and a `VolumeRegistry` seam looks like the obvious answer. It isn't. Both are the
backend asking about ITSELF:

- `still_the_same_volume` resolves the id, downcasts to `SmbVolume`, and compares `instance_id` — it's asking "am I
  still the live instance whose watcher died?", so a supersede can't make a healthy volume mark itself disconnected.
- `stat_via_volume` resolves the id to reach the MAIN session's `get_metadata`, deliberately not the watcher's dedicated
  session.

A `Weak` handle to the volume answers both without a registry: upgrade per iteration (the loop already re-resolves every
iteration for exactly the reason `Weak` handles for free), and identity is then a pointer, not an id plus a counter plus
a downcast. That deletes the awkwardness rather than wrapping it, and it removes the last thing that would have forced a
seam to hand back an `Arc<dyn Volume>` for a backend to downcast to its own concrete struct.

The residual gap is real and worth stating: a volume can be REMOVED from the registry without being superseded or
unmounted, and no flag on the volume records that. Whatever shape wins, the host has to tell a volume it's been retired,
or "am I still live?" stays unanswerable from inside.

### Visibility that has no cross-crate equivalent

`smb/mod.rs::detach_session_for_test` is `#[cfg(test)] pub(in crate::file_system::volume)`, and MTP's `test_hooks`
module is the same. There is no cross-crate spelling of "visible to this module subtree", so each becomes
`#[cfg(any(test, feature = "testing"))] pub` — a real widening of the public surface — or the test that uses it moves
into the backend crate with it. The `cfg` half is not optional: `cfg(test)` is set only for a crate's own test target,
so leaving it would make the item vanish from a consumer's test build. This project has been bitten by that three times.

### Test modules reached through `use super::*`

SMB's `#[cfg(test)] mod smb_*` children close over `smb/mod.rs`'s prelude glob, which re-exports submodule internals
specifically so they resolve. What that glob actually pulls in wasn't determinable without building. It's the biggest
unknown in moving SMB, and it's test-only, which is why the pilot backend is one with no `cfg(test)` behavior at all.

### The archive rustdoc link

`cmdr-archive`'s `volume.rs` carried an intra-doc link to `VolumeManager::resolve`, an app symbol no backend crate can
name, and it would have failed `desktop-rust-rustdoc`. It's prose now. Cheap to fix, but the kind of thing that only
surfaces at the end of a move: check every `[\`Type::method\`]` link for an app-side target before moving a backend.

## Writing a new backend

1. Depend on `cmdr-fs`. Implement `Volume`. Take a `VolumeHost` in your constructor and store it.
2. Declare your settings namespace once: `const BACKEND: BackendName = "ftp";`.
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
9. Write your tests against the fakes here. Assert on `change_count` as well as contents: that's what keeps a seam call
   from drifting into a per-entry loop.

## Where the app answers each seam

`apps/desktop/src-tauri/src/volume_host.rs` builds the one host and hands it out; each answer is a small adapter next to
the subsystem that can actually give it.

- `ListingHost` ⇒ `file_system::listing::listing_host::AppListings`
- `VolumeEventSink` ⇒ `events::volume_mapping::TauriVolumeEvents`
- `CredentialStore` ⇒ `network::credential_store::KeychainCredentials`
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
- **`AppBackendSettings` resolves through a table keyed by the namespace**, not a `match` on it. One row exists, `"smb"`
  ⇒ the `network.smbConcurrency` setting, which is SMB's alone: label, help text, and table row all say so. A namespace
  with no row gets a conservative built-in (2), because the day someone adds a backend and forgets its row is the day
  that number ships, and an FTP server capped at four connections answers the fifth with a ban. Adding a backend's knob
  is adding a row.
- **`IndexNotifier` needs no platform fork.** `Index::on_watch_gap` compiles everywhere and cfg-gates its own MTP arm,
  so the adapter is unconditional; only the app's own call sites carry
  `#[cfg(any(target_os = "macos", target_os = "linux"))]`.

`cmdr-archive` is the only backend on the seams so far, with one exception: SMB already reports connection transitions
through `VolumeConnection` and `events::volume_mapping`, because converting straight to the app's wire enum welded the
backend and `network/` into one cycle. It still reaches `listing::caching`, `network::keychain`, and the rest directly,
as do the other app-resident backends; each switches over when it moves into its own crate. Which ones will and won't: §
"Which backends move" below.

## What a backend crate buys, and the two things it doesn't

Worth stating plainly, because both non-wins are the ones people expect first and the real win is easy to miss.

**`pnpm check` will NOT get faster, and no amount of extraction changes that.** `scripts/check/checks/inputs.go` defines
one `rustInputs` set (`apps/desktop/src-tauri/**` + `crates/**` + the manifests) shared by every Rust check, so any Rust
edit anywhere busts every Rust check's cache; `desktop-rust-clippy` and `desktop-rust-tests` then run `--workspace`, so
the work doesn't shrink either. Getting a check-runner win takes new per-crate check lanes with narrowed `Inputs`, which
is deliberate check-runner work and not a side effect of moving code.

**Full app builds get slightly SLOWER after a backend edit**, because the app still relinks:
`docs/notes/index-extraction-baseline.md` measured +11% for "index edit, then `cargo build`". Expect the same sign for a
backend.

**The win is the scoped inner loop, and it comes from not compiling the app** — so it transfers whole to a small crate.
The index extraction measured `cargo check --lib` −83% (4.35 s → 0.75 s) and `cargo test --lib --no-run` −85% (23–30 s →
3.55 s). `cargo check -p cmdr-archive` compiles ~8k lines where it used to compile 332k. That structural ratio is what
makes `cargo check -p cmdr-ftp` a complete verification loop for an agent that never opens the app crate, and it's the
whole reason the seams exist. Release builds may also gain modestly (the index took a clean release build 214 s → 188 s,
−12%, because more crates give cargo more codegen units), but archive plus SMB are ~7% of the tree against the index's
28%, so don't expect that figure again.

The archive pilot's own timings are in `docs/notes/archive-extraction-baseline.md`, and most of them are **withdrawn**:
the machine was running several concurrent workspace builds on a near-full data volume. What survives is the inner-loop
ratio, as an order-of-magnitude reading.

## Which backends move

`cmdr-archive` shipped. Every backend written from here (FTP, S3, SFTP) is a crate from day one, which costs almost
nothing extra and gets the full benefit. The retrofits are judged one at a time, because retrofitting is where all the
cost sits:

- **SMB is the one worth retrofitting**, and it's underway in `crates/cmdr-smb/` (the protocol layer landed; the
  `Volume` impl hasn't). Its coupling is roughly two dozen sites against archive's three, plus the four structural
  problems this document enumerates: the `pub(in …)` visibility, the `use super::*` test modules, the `network/` split
  (done), and the registry reach-backs. Its test surface is 5,343 lines against archive's 3,376, and the archive move
  showed the real cost is app-side test re-homing rather than path rewriting — budget for SPLITTING tests that grew to
  cover both sides of the boundary, not moving them.
- **`local_posix` and MTP are permanently app-resident.** Both refusals are written out with their reasons in
  `apps/desktop/src-tauri/src/file_system/volume/backends/DETAILS.md` § "Per-backend decisions", because that's where
  someone proposing "let's complete the set" will be standing.

**Expect an extraction to surface latent defects**, and treat that as the point rather than a surprise. Archive's move
found two: seven `.unwrap()`s that were legal only while the file was `cfg(test)` and became clippy `unwrap_used`
violations as a `testing`-feature lib item, and a rustdoc intra-doc link to a function that no longer existed, which
`desktop-rust-rustdoc` had never seen while the item was buried in the app. Check every ``[`Type::method`]`` link for an
app-side target before moving a backend; an intra-doc link to an app symbol is unnameable from a backend crate and has
to become prose.

# `cmdr-fs` details

## Why the crate exists

`file_system/` sat on both sides of a cycle: the index subsystems referenced it (~70 `crate::file_system` refs), and
`file_system/*` referenced them back. A crate can't be carved out of one end of a cycle, so the shared vocabulary had to
become its own thing that both ends depend on and neither owns.

The boundary is enforceable rather than aspirational: nothing here can reach `tauri`, the index, or a real-storage
backend, because none of them are in the dependency graph. That's the property the whole extraction rests on, so treat
"just add a small dependency" as a design change, not a convenience.

## Module map

`CLAUDE.md` carries the must-knows; this is where each file sits. Why each one belongs here rather than in the app is
the next section.

- `volume/`: the `Volume` trait and its types; `connection.rs` (the REMOTE vocabulary: `ConnectionState`,
  `DeviceReadiness`, `BackendKind`, `SignInShape`, and `DeviceUnavailableReason`); `ids` + `canonical_root` + `mtp_ids`
  (the ID funnel and double-mount collapse); `published_locations` (the switcher list's dedupe both platforms share,
  where a favorite never claims a volume's path); `smb_mount_source` (the `user:password@host:port` split both
  platforms' SMB mount-source parsers share, bracketed IPv6 included); `capabilities.rs`; `share_link.rs` (`ShareLink`,
  a URL that's a credential and so prints nothing in `Debug`, and `ShareLinkExpiry`); `entry_kind.rs` (`File` /
  `Directory` / `Symlink`, the answer `Volume::entry_kind` gives, a link reported as the link, where `is_directory` may
  follow it); `child_name.rs` (`ChildName`, a listed name proven to be one plain path component, the only way a source
  listing's name may join a destination path; a refusal is `VolumeError::InvalidName`. Why and who calls it:
  `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "Listed names are untrusted");
  `server_side.rs` (`RenameWork`, `SubtreeTally`, `ServerCopyProgress`: work a server does on its own, § "Server-side
  work"); `retirement.rs` (how background work learns it stopped being the live volume); `channel_stream.rs` (a network
  backend's read path, consumer half); `scan_boundary.rs` + `scan_stop.rs` (the one seam a copy scan touches per entry:
  counts, Cancel, and Pause); `scan_walk.rs`, `mkdir_all.rs`, `patching.rs`, and `secret_store.rs` (the bodies a
  stat-and-listing backend gets for free); `liveness.rs` (the HTTP backends' silence watch, which tells a server gone
  silent from one that's only slow: `crates/cmdr-webdav/DETAILS.md` § "Silent or slow") and `tls.rs` (their connect
  probes' TLS-refusal test, by the typed `io::ErrorKind`); `app_paths.rs` (whether an app path sits under a volume root,
  segment-wise, and rebasing it onto a moved root); `remote_paths.rs` (a server tree's `<scheme>://user@host:port` app
  spelling, and the ONE translation); `friendly_error/` (typed, word-free classification); `usb_speed.rs` (❗ its doc
  comment reaches `bindings.ts`); `in_memory.rs` (the store and its knobs; `in_memory/volume_impl.rs` is its
  `impl Volume`); `conformance.rs` (+ `conformance/directory_creation.rs`, the three `create_directory_all` promises,
  and `conformance/modification_date.rs`, the two date ones); and `host/` (what a backend needs from the app, as named
  traits; read `src/volume/host/CLAUDE.md` before writing a backend).
- `entry.rs` + `icons/`: `FileEntry` and the classifiers behind `get_icon_id`.
- `sqlite_util.rs`: the ONE process-wide page-cache slab, the connection factories every store opens through, and the
  one way a database file is deleted. `src/sqlite_util/thread_conn_cache.rs` is the per-thread read-connection cache and
  its retirement, re-exported from it.
- `staging.rs`: `StagingTemp`, the ONLY way to name a scratch file.
- Leaves: `archive_format.rs` (sole source of truth for archive detection), `firmlinks.rs` (`normalize_path`; the index
  and the app's watchers have to agree on it), `file_provider.rs` (the cloud-domain marker), `filesystem_kind.rs`,
  `path_locations.rs` (how many PLACES a set of directories amounts to), `path_hash.rs` (the fixed 64-bit folder-path
  hash two resident maps key on instead of the path: search's importance weights and the media coverage score cache),
  `git_meta` (what a git portal row's Size cell states), `name_fold.rs` (the ONE "same name, spelled another way" key:
  NFC + lowercase, shared by share IDs, transfer conflict buckets, the SMB spelling resolve, and cursor placement),
  `log_rollup`, `log_detail` (`LogDetail`: external OS/server/CLI text as one Debug-quoted, 1 KiB-capped log field that
  the app's report pass redacts; see `apps/desktop/src-tauri/src/redact/DETAILS.md`), `tcc_paths`, `ignore_poison`,
  `pluralize`, `thread_qos`, `thread_cpu`, `process_memory`, `utility_pool.rs` (`UtilityPool`, a keep-alive pool of
  `Utility`-QoS threads, so background work that comes in streams, like index walks, reuses threads instead of creating
  one per job).
- `testing/`: behind the `testing` feature. `TestDir` and the two waits in `mod.rs`; `tcp_proxy.rs` (`TcpProxy`, a
  loopback proxy a network backend's test puts in front of a shared Docker fixture to cut the connection, refused or
  silent, without touching a container other runs lease; its header has the usage); on macOS, `disk_images/` (the
  synthetic APFS/HFS+ image harness and its guarded runner, § "`testing::disk_images`").

## Why each thing is here rather than in the app

❗ The membership of this crate is a `cargo check` closure, not a reading of `use` lines. An import census misses a call
through a local helper, a fully-qualified call inline in an expression, a `use` inside a function body, and
`#[cfg(test)]` items, so weigh any "just move this down" against the compiler, not a grep.

- **`Volume` + `types` + `ids`.** The trait is the crate's centrepiece: the index walks network volumes through
  `Volume::list_directory_for_scan`, so it needs the trait and every type the trait mentions.
- **`friendly_error/` + `git.rs`'s `FriendlyGitError`.** `VolumeError::FriendlyGit` carries the git error, and the git
  error maps onto `friendly_error::ErrorCategory`. A genuine two-way pair; neither can move without the other. The git
  type's only other non-`std` dependency is `serde`, so no git internals came along, and the boxed-trait fallback (keep
  the payload app-side behind a `cmdr-fs`-owned trait) wasn't needed.
- **`tcc_paths`.** `friendly_error/volume_error.rs` asks it whether a permission denial is really macOS TCC (it answers
  by probing the gate that covers the path, not by matching the path alone). Its app-side parent
  `restricted_paths/mod.rs` imports `tauri::AppHandle`, so only the child is here.
- **`FileEntry`.** 11 of the ~70 `crate::file_system` references from the index are this type; it isn't skippable. Its
  constructor brings three more predicates with it (below).
- **`InMemoryVolume`.** The one `Volume` impl that needs no host. It rides with the trait so a test in any crate can
  build a volume without the app.
- **`ignore_poison`, `pluralize`, `thread_qos`, `thread_cpu`, `process_memory`.** Host primitives with 8–49 references
  from the index trees, none of which can sensibly be injected. `thread_qos` in particular is the property that kept
  indexing in-process at all: a `tokio::runtime::Handle` does nothing for thread scheduling class. `thread_cpu` reads
  `CLOCK_THREAD_CPUTIME_ID` for the CALLING thread, which is the only way to attribute CPU to one thread on macOS:
  `ps -M` reports per-thread cumulative CPU but no thread names, so a thread has to report its own. It's cumulative on
  purpose, so a window is the difference of two readings; the index writer's heartbeat is its one consumer today
  (`../cmdr-index/src/indexing/writer/probe_stats.rs`).
- **`utility_pool`.** The pooled half of `thread_qos`: a thread that runs only `Utility` work for its whole life can be
  lowered without leaking the class onto anything else, which is what makes pooling background threads legal. It's here
  beside `thread_qos` for that reason; its consumers today are the index walker and the rescan drain in `cmdr-index`.
- **`path_hash`.** The folder-path hash both resident score maps key on: search's `ImportanceWeights` app-side and the
  media coverage score cache in `cmdr-index`. Neither side can own it without the other reaching across, and two copies
  of a hash that has to agree with a streamed variant (`PathHasher`) would drift.
- **`sqlite_util`.** A leaf over `std` + `rusqlite`, whose only two in-crate calls are `pluralize` and `ignore_poison`,
  both already here. It belongs here because the stores that share it sit on both sides of the boundary: the three index
  DBs live in `cmdr-index`, while the agent's and the operation log's stay app-side. Putting it in `cmdr-index` would
  make `agent/` and `operation_log/` depend on the index for connection plumbing, and there is only one
  `SQLITE_CONFIG_PAGECACHE` slab per process, so it genuinely has to be one instance both sides see.
- **`staging`.** The two file markers, the `.cmdr-staging-<op>` directory prefix, the `StagingTemp` mint, and the
  in-flight registry. `is_staging_dir_name` is a STRICT parse (the prefix plus exactly a hyphenated UUID, nothing around
  it) because a sweep acts on its answer, where `is_staging_temp_name` is a substring test on a name whose answer only
  hides a row; `is_cmdr_scratch_name` is the union the listing gate asks. A mutating backend has to be able to stage a
  write, and the archive mutator already does; leaving the mint in the app would mean the first backend crate either
  reaches upward for it or grows a seam for something with no per-backend variation. The mint's only tie to write-op
  state is an `Option<Weak<()>>` liveness token the CALLER hands over, which names no app type. The two visibility
  settings stay app-side (below).
- **`wait_until` / `wait_until_async`.** Behind the `testing` feature. The rest of the app's `test_support.rs` can't
  follow: `COUNTING_ALLOCATOR` is a `#[global_allocator]`, and a second one in any binary linking this crate is a hard
  compile error.

## `volume::ids`: why an ID is a digest and not a rendering

A volume ID is identity, not a label. It keys `index-{id}.db` and its `importance-`/`media-` siblings, `lastUsedPaths`,
tab `volumeId` fields, the `VolumeManager` registry, and therefore which disk an operation acts on. Two consequences run
in opposite directions, and both have bitten:

- Two volumes sharing one ID cross-wire their state, and route reads (and deletes) to the wrong disk.
- One volume getting two IDs over its lifetime orphans its index and saved paths, so a full rescan runs for a disk that
  was already indexed.

The retired scheme did both, because it built an ID by DELETING everything outside `[a-z0-9-]` and lowercasing the rest,
from the MOUNT PATH. Deleting characters is a many-to-one map (`/Volumes/My Disk` and `/Volumes/My_Disk` → one ID), and
a mount path isn't stable (macOS mounts a second same-named disk at `/Volumes/Backup 1`).

So the funnel here mints `{scheme}-{slug}-{digest}`:

- **`digest`** is 64 bits of BLAKE3 over the scheme followed by each canonical part, LENGTH-PREFIXED. The prefixing is
  what keeps the map injective across component boundaries: without it `("nas", "polyashare")` and
  `("naspolya", "share")` feed the hasher identical bytes. The scheme goes in first as domain separation. Cryptographic
  rather than a fast hash because volume names are user-controlled, so a _chosen_ collision (name a USB stick to steal
  another volume's index) has to cost ~2^64, not a few seconds.
- **`slug`** is lossy, capped, and purely so a data dir and a log line stay readable. ❌ Nothing may parse or key off
  it.
- **`scheme`** records which identity source the ID came from, best first: `vol-` (filesystem UUID), `smb-` (server,
  port, share), `mtp-` (device serial), `path-` (mount path, the fallback). `root` stays a bare literal, being unique by
  definition and special-cased across the app.

Case folding happens only where the protocol says two spellings ARE one thing (DNS hostnames, SMB share names, hex
UUIDs). That's canonicalization; everywhere else, folding would be exactly the information loss that caused the bug.

`smb_volume_id` NFC-folds its server and share before case-folding them, for the same reason and against the
one-volume-two-IDs direction above. macOS `statfs` spells an accented name decomposed while mDNS and the server's share
list spell it composed, so the two SMB upgrade paths would mint two IDs for one share and split its index and saved
paths down whichever path registered it first (ERR-ABXW4). `path_volume_id` deliberately does NOT fold: it hashes a
kernel-supplied mount path, and the kernel is self-consistent about how it spells one. The full list of places an SMB
name is folded lives in `apps/desktop/src-tauri/src/network/DETAILS.md` § "One SMB name, two spellings".

The 64-bit digest also bounds the length, which is load-bearing: an ID is a filename component, and macOS and Linux both
stop at 255 bytes. It's the reason a fully-injective escaping scheme (percent-encode the path) was rejected: reversible
and elegant, but unbounded, and it renders a mount path with spaces unreadable anyway.

Every remote volume's APP-ROOT PREFIX (`sftp_app_root`, `webdav_app_root`, `adb_app_root`, and the splits back,
`server_of_path` and `adb_serial_of_path`) is minted from the same tuple as its id, in `remote_paths.rs` beside the
translation it feeds, and re-exported from `volume` like the ids. That is what makes a path and an id agree by
construction: a `sftp://ada@nas:22/...` path resolves to exactly the id `sftp_volume_id` mints for that account, so a
saved row and the volume it becomes share one identity across a dial and a restored tab finds its way home. The prefix,
the translation between an app path and a server path, and why a bare server-absolute path is REFUSED rather than
anchored: `src/volume/remote_paths.rs`'s module doc, which is canonical for all of it.

Nothing enforces the funnel in the type system. An ID crosses IPC as a `String` in ~3,600 Rust and ~1,600 TypeScript
sites, so a `VolumeId` newtype would be a very large refactor for a property one module already guarantees; the
guardrail is instead the never-build-an-ID-by-hand rule in each caller's `CLAUDE.md`, plus `VolumeManager::register`
logging an error whenever one ID does end up covering two mount roots.

## Three questions a remote volume gets asked, and three types that answer them

Conflating any two of these is the bug `src/volume/connection.rs` exists to prevent, and its module doc is canonical for
what each variant means. The split itself is worth stating here, because the types are small enough to look
interchangeable:

- **How live is the SESSION?** `ConnectionState`, via `Volume::connection_state()`. The switcher dot, the pane's connect
  views, and the reconnect manager read it. Every connecting backend answers; a local disk, an archive, and the git
  portal answer `None`.
- **WHICH BACKEND serves it?** `BackendKind`, via `Volume::backend_kind()`. ❗ The one an "is this SMB?" test must ask.
  A `connection_state().is_some()` used that way hands an SFTP volume to the SMB indexer, which is exactly what happened
  when one field answered both questions. Backend-side only: the frontend classifies a pane off `fsType` and category,
  because an OS-mounted SMB share is served by `LocalPosixVolume` and this would answer `Local` for it.
- **Is the DEVICE there?** `DeviceReadiness`, set by the device providers alone and carried on `LocationInfo`, never on
  the `Volume`. A phone waiting for its "Allow USB debugging?" tap has no session, so putting it on the session field
  would enrol it in a backoff loop that dials nothing forever.

## Three boundary decisions, and why they hold

A naive reading of what `FileEntry` and `Volume` "need" pulls in 89 files and ~36,200 lines of the app, including
~15,600 lines of `indexing/`, which is the cycle coming straight back. Three decisions keep the closure at the ~25 files
and ~7,600 lines here, and each is worth defending against a tidy-up pass.

### 1. `Volume::notify_mutation` has a no-op default

A default that patched the local listing cache would open with `use crate::file_system::listing::caching::…` and
`use crate::file_system::listing::reading::…` inside the function body, and those two edges alone drag the app's listing
cache and listing I/O into the graph. So the default is a documented no-op, and the local-FS behavior lives app-side in
`file_system::listing::mutation::patch_listing_after_local_mutation`.

Why this over the alternatives:

- **A required method** (no default body) would touch ~45 `impl Volume for` sites, most of them test doubles, for no
  gain: every one would write `Box::pin(async {})`.
- **A `MutationObserver` trait** would need an injection point the signature doesn't have, so it would land as a
  `OnceLock` global inside this crate, exactly the shape this boundary exists to avoid.
- The no-op is also more correct. All four real backends (`LocalPosixVolume`, `SmbVolume`, `MtpVolume`, and the
  read-only `ArchiveVolume`, which never calls it) override the method; the only callers of a stat-the-real-filesystem
  default would be `InMemoryVolume` and the test doubles, for which it was never right.
- Keeping one helper app-side rather than a default plus `LocalPosixVolume`'s own copy leaves ~50 fewer lines and no
  duplicated cache-patching routine to rot apart.

**Guardrail this leaves behind**: a new mutable backend that forgets to override `notify_mutation` gets a silently stale
pane instead of a free correct one. That's why the trait doc says so and why the backends checklist repeats it.

### 2. `FileEntry::new`'s pure predicates live here

`FileEntry::new` sets `icon_id` through a local `get_icon_id` helper (which calls `icons::special_folders` and
`icons::per_path`), `is_archive` from a fully-qualified inline call into the archive vocabulary, and `is_hidden` from
`is_hidden_by_name` (the dot check), defined right beside it. None of these appear in any header `use` line, which is
exactly why the closure is derived by `cargo check` rather than by grepping imports: a call through a local helper, a
fully-qualified inline call, a `use` inside a function body, and `#[cfg(test)]` items are all invisible to a header
grep.

Every one is a pure name and path predicate with no I/O, a hard requirement since they run for every entry of a
100k-entry listing, so they live here rather than being stripped or injected (stripping the fields was never viable:
`FileEntry::new` has 83 call sites):

- `icons/special_folders.rs` is whole here; its only non-`std` dependency is `dirs`.
- `icons/packages.rs` holds the package half of the per-path classification. The custom-icon half stays app-side: it
  needs a `getxattr` syscall, which is why it never runs during a listing in the first place.
- `has_supported_archive_extension` delegates to `format_for_name`, so the whole name → format vocabulary is here
  (`ArchiveFormat`, `TarCodec`, `format_for_name`, `format_for_path`, `is_sequential`). The decoders that unwrap a tar's
  outer compression stay with the archive reading core. The split line is "naming vs machinery", and it keeps
  `format_for_name` the single source of truth rather than forking a second suffix table.
- `is_hidden_by_name` needs no file of its own (a one-line dot check), but it's the same shape: local POSIX later ORs in
  two I/O-cheap macOS mechanisms (`UF_HIDDEN`, root `/.hidden`) app-side, in `reading.rs`, where the `stat` and the
  volume-root context already are. See `FileEntry::is_hidden`'s doc for the full split.

  One consequence is worth following: because the format a name maps to decides WRITABILITY, the enum carries a variant
  whose whole reason for existing lives in the consumer crate. `ArchiveFormat::Ooxml` is a zip in every respect the
  reader cares about, and separate only so the app's write guard refuses it. The rationale is in
  `crates/cmdr-archive/DETAILS.md` § "Why a document container is its own format"; don't restate it here, and read it
  before touching the variant or the suffix table.

### 3. `filesystem_kind` is split: classification here, detection app-side

`detect_filesystem_for_path` reaches `crate::volumes::get_mount_point` (macOS) or `crate::file_system::linux_mounts`
(Linux). Classification is platform-free and detection is thin platform wiring, so only the classification is here.
Callers that want detection are app-side anyway.

## Gotcha: `cfg(test)`-conditioned BEHAVIOR stops meaning anything in a dependency

**Any `cfg(test)` that gates BEHAVIOR — not just a test module — silently changes meaning the moment its code moves into
a crate somebody else depends on.** `cfg(test)` is set only while compiling a crate's OWN test target. A consumer's
`#[test]`s compile this crate as a plain dependency, where it is NOT set, so the "test build" arm quietly stops being
taken and production behavior starts running inside everyone's test suite.

**Grep for `cfg(test)` and `cfg(not(test))` outside `mod tests` declarations before moving any code down here.** The
replacement is `any(test, feature = "testing")` (or `not(any(…))`), with consumers switching the feature on through a
**dev-dependency** so it stays off in shipped builds.

**Why this rates a gotcha rather than a footnote**: it is invisible at the call site, it compiles clean, and it fails as
a _timing_ symptom in unrelated tests. `thread_qos::set_current_thread_qos` was
`#[cfg(all(target_os = "macos", not(test)))]`; moving it here started applying the real `QOS_CLASS_UTILITY` to the app's
background threads during the app's own test run. Under nextest's one-process-per-core parallelism that starved a walker
test past its stall watchdog and `indexing::scanner::walker::tests::a_read_that_keeps_delivering_is_never_abandoned`
began failing — a genuine failure that reads exactly like flakiness. Nothing about the diff suggested thread scheduling.

**This applies to `cmdr-index` too, and harder.** `set_current_thread_qos` is called from seven sites across `scanner/`,
`writer/`, and `reconcile/` — all code that moves into that crate. When it does, the same question has to be asked of
every `cfg(test)` in the moving trees, and the QoS no-op has to keep working from a _third_ crate. It is the property
that kept indexing in-process at all.

It bit again on the way in, exactly as predicted: `sqlite_util`'s open counter and its test-only accessors
(`page_cache_kib`, `open_count_for`, `ThreadConnCache::len`) were bare `cfg(test)`, and five app-side test modules
import them. As a dependency they were configured out and the build broke loudly; had they merely gated behavior, the
counter would have gone quiet inside every consumer's suite instead. All of them are `any(test, feature = "testing")`
now.

The same shape bit once more, harmlessly: the `Volume::inject_error` E2E hook is `#[cfg(feature = "playwright-e2e")]`, a
feature that lived only on the app. This crate now declares its own, and the app's enables it via
`cmdr-fs/playwright-e2e`. A feature name that isn't declared in the crate you move code into doesn't error — it warns
about an "unexpected `cfg` condition value" and takes the false branch forever.

## Retiring cached read connections, and deleting a database (`sqlite_util`)

Both read paths keep their connections in a thread-local `ThreadConnCache` so enrichment never takes a lock. The price
is that nobody can close another thread's connection, and that mattered twice once databases could go away:

- **An unlinked file keeps its blocks until the last handle closes.** A forgotten share's database was deleted while
  blocking threads still cached connections to it, so the disk didn't come back until those threads died.
- **A stale connection still answers.** It reads the unlinked file, so a database recreated under the same path reads as
  the old one for as long as the slot survives. The index's `ReadPool` defends with a fresh generation per pool;
  importance reads carry no generation at all.

`retire_read_connections(db_path)` is the mechanism for both. It bumps a process-wide epoch and records the epoch the
path was retired at; every `ThreadConnCache::with` call compares the epoch to the one it last swept at (one atomic load
when nothing changed) and closes each entry opened before its path's retirement. So the owning thread closes its own
connections, on its next use of that cache for ANY database.

⚠️ **It is a request, not a close.** A thread that never reads again keeps its connections until it exits, which tokio
does to an idle blocking thread after ten seconds. The honest bound is "the owning thread's next read, or its death",
and there is one cache per read path (`THREAD_CONNS`, `READ_CONNS`), each swept on its own next use. ❌ Don't document
or rely on "closed when this returns". Closing sooner would need a lock on the hot read path, which is what the
thread-local exists to avoid.

A connection opened AFTER the retirement is kept, which is what lets a recreated database be cached normally. The
comparison is per entry (`opened_at` against the path's retirement epoch), not a path denylist.

`delete_database(db_path)` is the one way a database file is removed: retire, unlink the main file and its `-wal` and
`-shm`, retire again. The second retirement covers a reader that opened between the first one and the unlink, which
would otherwise be left holding the unlinked file. A missing file is fine, every file is attempted, and the first
refusal is returned. ⚠️ It only reaches cached READ connections: the caller stops whatever writes the database first.
`cmdr-index` builds its per-volume removal on it (`crates/cmdr-index/DETAILS.md` § "A volume's files, and the one door
they leave by"), and the three stores' schema-mismatch wipes call it too.

## What the app kept, and why

- **Every real-storage backend** (`LocalPosixVolume`, `SmbVolume`, `MtpVolume`, `ArchiveVolume`) with their `smb2` /
  `mtp-rs` / `gix` / mount-detection dependencies. Only their shared trait moved.
- **`VolumeManager`** — the process-wide registry. The index reaches it through an injected provider, not by importing
  it.
- **`file_system::listing::mutation::patch_listing_after_local_mutation`** — see cut 1.
- **`detect_filesystem_for_path`** — see cut 3. The kind → network mapping over it stayed app-side too
  (`file_system::index_provider::path_is_on_network_mount`), which is why `WatchCoverage::ThisMachineOnly` is a variant
  this crate can NAME but never decide: the vocabulary is portable, the mount probe isn't. A backend here answers
  `Volume::listing_watch_coverage` from what it knows; the app answers for OS-mounted shares. Capability model and the
  per-backend answers: `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md`.
- **Every answer to "is this path visible outside Cmdr?"** The trait NAMES the question (`Volume::paths_are_os_visible`,
  defaulting to `supports_local_fs_access`), because "can another app open a `file://` URL for this?" is portable
  vocabulary. Which backends split the two isn't: only the app knows an SMB share stays OS-mounted beside its own smb2
  session, so `SmbVolume` is where the override lives and `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` is
  where the per-backend answers are listed. `Volume::note_root_mount_gone` splits the same way: the trait names the fact
  ("the mount you're anchored to is gone"), and only the app's registry can establish it, since a mount is something you
  may never probe. Same shape as `listing_watch_coverage` above.
- **The listing cache that decides which spellings of a directory are one pane.** The trait names the answer
  (`Volume::listing_path`, identity by default; MTP folds its `mtp://` URL and inner paths together), and the app's
  listing cache keys every store and lookup on it. Why: `apps/desktop/src-tauri/src/file_system/listing/DETAILS.md`.
- **When a foreign path gets resolved.** The trait names the question (`Volume::find_stored_spelling`: where is this
  path, spelled the way you store it? `Ok(None)` by default), and SMB answers it against real listings
  (`crates/cmdr-smb/DETAILS.md` § "Resolving a foreign path"). Which callers may ask is app policy: only a pane opening
  a directory and a backend swap's respell, never an operation's own calls, where a resolve could land a delete on a
  look-alike twin (`apps/desktop/src-tauri/src/file_system/listing/DETAILS.md` § "A pane path the volume stores another
  way").
- **What spelling a NEW name takes, and whether a look-alike needs a listing to find.** The trait names both facts
  (`Volume::composes_new_names`, read through the provided `spell_new_name`;
  `Volume::matches_names_in_any_unicode_form`), `name_fold` holds the two comparisons (`composed`,
  `differ_only_in_form`), and the app decides which writes create a name and which address an existing entry
  (`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Look-alike names").
- **`icons/per_path.rs`'s custom-folder-icon half**, the NSWorkspace fetch, and the icon disk cache.
- **The scratch-visibility settings** (`advanced.showStagingTempFiles`, `advanced.showSafeSaveFiles`) and the listing
  read-path filter over them. "Is this ours, and does a live operation own it?" is vocabulary; "does the user see it?"
  is product policy, and the safe-save half is about other apps' files entirely.
- **The archive tar decoders**, and everything else in `cmdr-archive`.
- **The allocation-counting harness**, because a `#[global_allocator]` has to be per-binary. It now sits in
  `indexing/test_support.rs`, next to the memory guards that use it.

## The one place prose is produced here

The API contract says this crate emits no user-facing strings. Two things look like exceptions and aren't:

- **`pluralize`** formats "1 file" / "2 files". All 49 of its call sites build log lines. It lives here because it's a
  leaf with no dependencies, not because copy generation belongs in a filesystem crate. One of its outputs does reach a
  UI: `PhaseRecord.trigger` renders in the developer debug panel, which is diagnostics, not product copy. Anyone
  grepping `String` in this crate and concluding the bar was abandoned should read this paragraph first.
- **`git_meta`** (`src/git_meta.rs`) looks like the second, and isn't: `FileEntry::git_meta` is a typed `GitEntryMeta`
  that states a fact, and the host words the Size column from its own catalog.

## `ScanBoundary`: one seam, so a walk can't count without asking

`Volume::scan_for_copy_batch_with_boundary` promises counts that are **cumulative for the call** — callers shift by
their own baseline across several calls, so a per-path reset makes the scan dialog's counters jump backwards. Every
remote backend needs exactly that, and two hand-rolled copies would drift on precisely the part that matters. So the
boundary lives here and every backend counts through it.

⚠️ The counting exists at all because a recursive scan over a network backend reports nothing until it returns: the
transfer dialog sits on "0 files" for the length of the walk, and `write_operations/scan_watchdog.rs` — which bounds a
preview by INACTIVITY — can't tell a slow tree from a server that stopped answering.

**Reporting and stopping are one call, deliberately.** `boundary.file(size).await?` and `boundary.dir().await?` count
the entry AND answer the user's Cancel and Pause, so a backend author reaches the stop by writing `?` on a call they
were already making. The alternative — a separate stop parameter beside the callback — is a thing a backend can be
handed and quietly not use, and the only symptom of that is a Cancel button doing nothing on one backend while the
numbers stay perfectly correct. ❗ Ask BEFORE a round trip, never after: a boundary on the far side of a listing is a
boundary the user waits out, which over a sleeping NAS is seconds per directory.

**What the owner implements** is `ScanStopSignal` (`scan_stop.rs`), not anything in this crate: `crates/` carry no
`tauri` and can't name a write operation. The handle is `ScanStop`, `Arc`-held so a backend walking inside
`spawn_blocking` can carry one across the closure, cloned once per scan and never per entry. Cost is ~2 ns per entry
against the ~1–3 µs syscall or round trip the entry already costs.

**The copy hands the same signal to its destination** through `VolumeReadStream::stop_signal`, for an upload that
buffers ahead of the wire (S3's parts): the pause that parks the source stream can't reach bytes already read. The
engine's checkpoint stream answers it; every other wrapper forwards its inner stream's answer.

**A stopped scan returns `VolumeError::Cancelled`, ❌ never a partial `BatchScanResult`.** Callers read a scan's totals
as the size of the transfer they're about to run, so a truncated total that looks successful is a progress bar finishing
at 30% and a free-space check passing when it shouldn't.

**Granularity is per backend, and uniformity would be a lie.** A backend that walks a real tree (local, SMB, MTP, and
everything on `scan_walk`) asks per entry; one whose per-path scan is a bounded walk over an already-loaded index
(in-memory, archive) asks per source path, which is the smallest unit of waiting it has. `volume::conformance`'s
`assert_batch_scan_stops_when_told` and `assert_batch_scan_asks_inside_the_walk` pin which is which.

## Server-side work: `copy_on_server`, `copy_within`, and a rename that copies

`server_side.rs` holds the vocabulary for work a server does on its own.

**`Volume::copy_on_server(source, from, to, mode, progress)`** is the transfer engine's one server-side copy, asked
before every streamed file (`write_operations/transfer/volume/server_side_copy.rs::try_server_side_copy`). Some
protocols copy a file without the bytes travelling through Cmdr (SFTP's `copy-data@openssh.com`, S3's `CopyObject` and
`UploadPartCopy`; SMB's `FSCTL_SRV_COPYCHUNK` is unimplemented today). Contract points, all load-bearing:

- ❗ **The backend decides which sources it can copy from**, from `source`'s concrete type and identity
  (`as_any().downcast_ref`), ❌ never a path: two S3 places of one account are two instances that can copy between them,
  while a copy on a server the source path doesn't belong to copies the wrong file, silently. The default answers only
  for the very same instance (`std::ptr::addr_eq`), through `copy_within`.
- **`NotSupported` means "stream it"**, never a failure: a backend with no such operation, a server lacking the
  extension, or a source it doesn't recognize.
- **`to` is written under `mode`, as `write_from_stream` writes it.** A whole-publishing backend
  (`publishes_writes_whole`) publishes the copy whole too, so the caller writes its final name, `CreateNew` when it
  expected the name free; every other backend's destination holds a byte-incomplete file while the copy runs, so the
  caller stages it exactly like a streamed write, ❌ never single-shot. The default refuses `CreateNew`, because
  `copy_within` truncates.
- **Cancel and pause arrive through `ServerCopyProgress`**: `advanced` answers `Break` to stop (the backend removes its
  partial, then answers `Cancelled`), and `checkpoint` is where a backend that copies in pieces (S3's parts) waits out a
  pause before starting each one.

**`Volume::copy_within(from, to, on_progress)`** is the older, same-volume-only form that SFTP and ADB implement; the
default `copy_on_server` calls it, so they need nothing more.

**`Volume::rename_work(path)`** answers whether renaming one ENTRY is one call (`RenameWork::OneCall`, the default, with
no I/O) or a copy plus a delete (`CopyThenDelete`: an object store's folder or big file). Every `rename` caller asks it
first and sends a copying entry through the transfer engine; ❌ never inferred from a backend kind, because the answer
is per entry. Its two helpers: `tally_subtree(path, cap)`, a bounded count of what such a rename carries, keeping each
file's size and date for the rename's cost estimate (default `server_side::tally_by_listing`, one listing per folder),
and `delete_files(paths)`, the batch delete a move's source sweep calls per folder level (default: `delete` per path, a
gone path answering `Ok`).

## `root_anchored`: the one rule for turning a caller's path into a backend's

Cmdr's UI speaks two path dialects — a pane sends the absolute path it displays, the transfer dialog's destination box
sends a volume-relative one — and a leading `/` doesn't tell them apart. `volume::root_anchored(root, path)` is the
single rule that folds both into the absolute, root-anchored form: root spellings (empty, `.`, `/`) are the root; a path
already under the root passes through, matched by whole COMPONENTS so a sibling mount (`/Volumes/naspi-1`) can't pass as
being under `/Volumes/naspi`; anything else hangs off the root, minus its leading `/`.

It lives here rather than in the app because the ambiguity is a property of the TRAIT's path contract, and because four
app-side sites plus `LocalPosixVolume::resolve` have to agree on the answer byte for byte (an O_EXCL reservation and the
write that later lands on it; a pending-write registration and the writer it's meant to match). It's idempotent, so a
caller anchors without knowing which dialect it holds, and it never asks `is_absolute`, so a scheme-shaped MTP root
works the same.

**Anchoring is the CALLER's job, and that's deliberate.** A backend that guesses at the dialect addresses real files at
the wrong place: `SmbVolume::to_smb_path` answers `NotFound` for an out-of-mount absolute path instead, which is correct
and is exactly why the anchoring has to happen upstream. Consumers:
`commands/file_system/volume_copy.rs::resolve_dest_path` (every copy / move / compress / scan destination),
`path_exists`, and the transfer engine's local shortcuts.

**A typed path that repeats the root folder is anchored anyway, and the dialog warns (`volume::root_echo`).** On a place
rooted at a server folder (`sftp://ada@nas:22/srv/data`), a user typing the server path they know (`/srv/data/photos`)
gets `/srv/data/srv/data/photos`. Decision: keep the anchoring and warn, ❌ never strip the prefix on our own. Why: the
doubled folder can be real (a place rooted at `/home/bob` holding `/home/bob/home/bob/folder`, which F5 prefills as
`/home/bob/folder`), so a silent strip would send that copy somewhere else. `root_echo(root, path)` answers both
readings (`resolved`, server-side, and `stripped`, volume-relative) for the transfer dialog's warning and its "Use
shorter path" button. It only fires for a scheme-shaped root with a folder below the server's `/`: a mounted root
(`/Volumes/naspi`) is matched by `root_anchored`'s pass-through arm, and a root at `/` reads the same both ways. Folders
match by whole components.

## `InMemoryVolume` honors the contracts data safety leans on

The double is the oracle: these `Volume` contracts have to hold in it, not just on the happy path.

- **`delete` refuses a NON-EMPTY directory** (`ENOTEMPTY`). The same-volume rename-merge preserves a skipped child's
  source purely by letting its parent's cleanup delete FAIL. A permissive `delete` disarms that whole test class.
- **`rename` of a directory carries its whole subtree.** A same-volume move IS directory renames, so a `rename` that
  moved only the dir node made those tests pass over the exact data-loss shape they existed to catch.
- **`rename(force = false)` refuses an existing destination**, and **`create_file` refuses an existing path**. Both are
  no-clobber promises the real backends make, so a double that overwrote would let a clobbering caller look correct.

❌ Never relax a contract to make a test green.

### The shared assertions in `volume::conformance`

The contracts above that are CROSS-BACKEND live as shared assertions rather than as per-backend tests, so a backend
can't quietly opt out of one. Each takes an already-seeded fixture, because seeding is the one part that can't be shared
(a local volume needs a temp dir, MTP a backing dir plus a rescan, SMB a share); what the assertion checks is identical
everywhere, which is the point.

- `assert_delete_leaves_a_non_empty_dir_intact` — the refusal that data-safety logic leans on rather than re-checking.
  This is the one MTP broke for years: it claimed the contract by implementing the trait, and nothing looked.
- `assert_rename_refuses_an_existing_destination` — `force` is the only thing between a move and the file it would
  replace, and each backend earns the refusal differently (`renamex_np(RENAME_EXCL)`, an SMB `stat` plus the server's
  `ReplaceIfExists == false`, an MTP `exists` probe, a map lookup). No shared mechanism to trust, only a shared promise.
  MTP's is the one that can't be atomic, and that's a property of the protocol rather than of the code:
  `crates/cmdr-mtp/DETAILS.md` § "The no-clobber rename is check-then-act".
- `assert_create_file_refuses_to_clobber` — the New File command renders the refusal as "that name is taken", so a
  clobbering backend silently empties a file and reports success.
- `assert_delete_files_removes_exactly_what_it_names` — a move's source sweep hands a whole folder level to
  `delete_files`, and a backend may delete by key with no folder check (S3's `DeleteObjects`), so a wrong answer per
  path, or a neighbour taken along, would misreport or over-delete a move's sources. Run by S3, local, and the in-memory
  double; the other backends take the trait's default (`delete` per path).
- `assert_unknown_write_is_refused_before_io` — a backend without unknown-length support returns `NotSupported` before
  polling the source or touching an existing destination, and never calls that write single-shot. This is the safety
  boundary that lets callers ask the capability question without racing it against destructive I/O.
- `assert_create_directory_all_reports_an_existing_dir_honestly` — `Created` promises the leaf was empty, and the
  transfer driver spends it by skipping the per-file destination conflict probe inside. Only the dangerous direction is
  pinned; answering `AlreadyExisted` for a leaf you did create is merely slower, which is what the trait means by "when
  in doubt, answer `AlreadyExisted`". MTP is the backend this matters most for: it answers
  `create_directory_errors_on_existing_dir() == false`, so the default walk learns "already there" from its probe rather
  than from a collision error.
- `assert_create_directory_all_refuses_a_file_in_the_way` — a FILE at the path, or at an ancestor of it, is refused with
  `VolumeError::NotADirectory` carrying the file's path, and stays untouched. A taken name reads the same whatever holds
  it on most protocols, so a walk that never asks answers `AlreadyExisted` for the file itself (and the transfer goes on
  to write into it) and the `NotFound` of the level below it for anything deeper, naming a folder the user asked Cmdr to
  create as the thing that's missing. Before this cell, every mutable backend did one or both. Run by local, SMB, MTP,
  SFTP, WebDAV (Apache and Nextcloud), ADB, and the in-memory double.
- `assert_create_directory_all_goes_through_a_link_to_a_folder` — the other side of that refusal: a link that LEADS to a
  folder is a folder to a `mkdir -p`, so the link's own path answers `AlreadyExisted` and a path below it is created
  inside the target. Run by the backends that have links (local, SFTP, ADB). Why this differs from the merge engines'
  "not a directory in its own right": `src/volume/mkdir_all.rs` § "A link to a folder is a folder".
- `assert_conflict_scan_reads_a_missing_destination_as_empty` — a destination that isn't there yet holds nothing, so
  `scan_for_conflicts` answers an empty list rather than the `NotFound` its listing hit. `scan_volume_copy` propagates
  what comes back, so the wrong answer isn't an odd conflict entry: it's the whole copy preview refusing to open, on the
  ordinary act of pasting into a folder the transfer would have created moments later. Three backends kept it by
  accident (a per-item `exists()` that finds nothing, `scan_walk::scan_conflicts`' match arm, a double that lists a
  missing directory as empty) and two forwarded their listing error, because every other conflict-scan test seeds the
  destination first.
- `assert_writability_matches_the_mutations_offered` and `assert_export_matches_the_bytes_offered` — the two capability
  DECLARATIONS that reach the user as UI state. Nothing but a test stops either drifting from the methods it speaks for.
- `assert_not_found_carries_the_path` — the payload the frontend renders as the missing file's name.
- `assert_set_modified_dates_a_folder` — a folder `Volume::set_modified` dated after a file landed in it lists that
  date. Run by local, SMB, SFTP, and the in-memory double; every other backend keeps the `NotSupported` default.
- `assert_write_from_stream_keeps_the_source_date` and `assert_read_stream_reports_the_listed_date` — the two halves of
  "a copy keeps the source's date" (`src/volume/conformance/modification_date.rs`). Every copy suite checksums bytes,
  and a destination stamping its own date passes all of them. Run by local, S3, ADB, MTP (on the virtual device), SMB,
  SFTP, and the in-memory double green, WebDAV's both on Nextcloud (Apache stores no date, so it runs the read half),
  and the read half by archive. The contract:
  `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "Copies keep the source's
  date".

`InMemoryVolume`, `LocalPosixVolume`, `AdbVolume`, and the Docker-gated `SmbVolume`, `SftpVolume`, and `WebdavVolume`
run every one (InMemory's writability cell sits in `capabilities_test.rs`, next to the predicate it speaks for).
`MtpVolume` runs all but `create_file`, which it doesn't implement (an upload there is `write_from_stream`, one
`SendObject` transaction); its `delete` cell lives in `cmdr-mtp`'s `volume/delete_test.rs` for the scaffolding that
contract needs. `ArchiveVolume` is read-only: it runs the three that don't mutate and pins the rest of the ground with
`every_mutation_is_unsupported`, and it is deliberately outside the conflict-scan one, since nothing copies INTO an
archive through the volume. A backend that adds a mutation adds the matching call.

### Stream lengths and write progress

`StreamLength::{Known, Unknown}` is shared by `VolumeReadStream::total_size`, `Volume::write_is_single_shot`, and
`Volume::write_from_stream`. Real-file streams remain `Known`; generated streams use `Unknown` because zero means an
empty stream, not an absent length. Keeping the distinction at the writer boundary lets a backend decide before I/O
whether its protocol can frame the body. `supports_unknown_length_writes()` defaults to `false`, and every backend keeps
that answer until its writer can consume to EOF without a declared total. An unsupported `Unknown` is refused before
opening or truncating the destination and before polling the source. Unknown writes are never single-shot.

`VolumeReadStream::modified_at` carries the source file's own date beside its bytes, from whatever the opening already
read (a local `stat`, an S3 GET's headers), so a destination that stores dates as data (S3's `x-amz-meta-mtime`) keeps
it without a second request. A required method, so no backend skips it by omission; `None` means "store no date", ❌
never a made-up one; a wrapper stream (`CheckpointStream`) forwards its inner stream's answer.

`Volume::publishes_writes_whole` is the third write promise beside single-shot and atomic replacement: the protocol
publishes every write whole (an object store's PUT). Only the transfer engine's staging decision reads it; the contract
and what it does NOT promise are on the trait method, the engine side in
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "Whole-publish destinations".

Write callbacks receive one `StreamWriteProgress { bytes_written, expected_length }` value. The structured payload keeps
cumulative progress and its denominator named rather than relying on two confusable positional `u64`s. For a known
write, every update carries the original `Known` length and existing mismatch and cancellation behavior remains
unchanged.

`supports_atomic_replace_rename()` is a separate publication capability. It means `rename(from, to, force = true)`
replaces `to` in one atomic filesystem operation, not that rename merely accepts a force flag. Local POSIX opts in. SMB
deliberately does not: its force implementation clears the destination before the server rename. Generated-file
publishers therefore use one rename locally and the tracked displaced-destination aside/restore flow everywhere else.
The conservative default prevents a new backend from turning delete-then-rename into an accidental data-loss window.

## The faults `InMemoryVolume` can be told to have

Everything above is what the double gets RIGHT unconditionally. On top of that it can be told to misbehave in specific,
named ways, so a caller's defense against a hostile backend is testable rather than assumed. Each is test-only, and each
models something a real backend genuinely does:

- **`with_delete_failing()`** — `delete` returns an `IoError` instead of removing the entry. A backend that can't remove
  a path (a permission, a lock, a dead session).
- **`set_stat_failing(path)`** — `is_directory` and `get_metadata` (so `entry_kind` too) FAIL for that path rather than
  reporting it missing. The distinction is the whole point: `NotFound` is an ANSWER, and code that turns an unanswered
  stat into a confident "not a directory" routes a folder into a file-shaped, destructive branch.
- **`set_reported_type(path, is_directory)`** — the stat and the listing report a type the entry doesn't have, while its
  real contents stay put. A stale or racy directory entry, and the exact lie the original cross-volume copy bug rode in
  on.
- **`set_reported_size(path, bytes)`** — the listed size disagrees with the real streamed byte count. A remote source
  whose directory entry is stale; a transfer planning against the real stream still lands correct bytes.
- **`set_reported_name(path, name)`** — the listing reports a name the entry doesn't have, such as `../x` or `/x`. A
  hostile server, device, or archive; the transfer engines must refuse it through `ChildName`, not join it.
- **`set_modified_at(path, secs)`** — ages a file into the past or clears its mtime, for the conditional policies
  (`OverwriteOlder`).
- **`with_sibling_duplicates_allowed()`** — `create_directory_errors_on_existing_dir()` reports `false`, modeling MTP,
  which can't signal a same-name collision at all.
- **`with_read_range_unsupported()`** — positioned reads return `NotSupported`, modeling a remote backend without the
  primitive.
- **`with_read_chunk_delay(d)`** — each read chunk takes `d` to arrive. Not a fault so much as the passage of time: an
  in-memory read otherwise completes without ever yielding, so a whole file lands inside one poll and a test that has to
  CATCH a transfer mid-file (a cancel that must leave no partial, a pause that must park mid-stream) is racing something
  that never gives it a turn.

A fault the caller wants to arm on a call COUNT rather than on a path belongs one layer up, in the app's `FaultyVolume`
wrapper (`file_system/write_operations/transfer/volume/faulty_volume_test_support.rs`): it wraps any volume and fails
the Nth call to a named operation. ❌ Don't grow this list with fault shapes that aren't about what a real backend does.

## `process_memory`: the allocators, their accountants, and the one reader that spans them

### Which global allocator

**Decision**: macOS runs on the system allocator, Linux on mimalloc, and the `mimalloc` feature (the app's forwards to
this crate's) puts macOS on mimalloc too. `build.rs` folds that rule into one cfg, `cmdr_mimalloc`, and
`process_memory/allocator.rs` turns it into the `GLOBAL_ALLOCATOR` constant plus the `GlobalAlloc` type and
`GLOBAL_ALLOC` value the app's `main.rs` installs. Every allocator-specific line asks `cmdr_mimalloc` (or, outside this
crate, the constant); ❌ never `feature = "mimalloc"`, which is false on Linux where mimalloc is the default.

**Why macOS left mimalloc**: on David's dev Mac, 15 min after a search-and-listing burst, the system allocator settles
at a median 241 MiB against mimalloc's 403, and under the 300 MiB target in five of seven runs against two of eight
(`../../docs/notes/performance/allocator-slack-release-2026-09-27.md`). mimalloc's post-burst slack is fragmentation,
not retention: sparse pages pinned by a few long-lived blocks, which no allocator call returns. It used to cost search
speed to leave it, but the search loop no longer allocates per row, so that penalty is gone
(`../../docs/notes/performance/search-loop-allocations-2026-09-27.md`). What macOS pays: burst peaks ~500 MiB higher
(median 1,976 against 1,458 MiB), a transient that sometimes lasts past a minute after a burst, and an occasional bad
settle (382 MiB, mostly swapped malloc-zone memory). Idle footprint drops ~95 MiB
(`../../docs/notes/performance/allocator-comparison-2026-09-23.md`).

**Why Linux keeps mimalloc**: glibc malloc is unmeasured under Cmdr's load, and its per-thread arenas behave very
differently from macOS malloc under thread churn, so the macOS numbers say nothing about it. Measure it before flipping
Linux; a feature to force the system allocator there is a one-line addition to `build.rs`.

**Why mimalloc's VM tag stays at its default (100, `IOAccelerator`)**: `os_tag` is a Mach tag, so it does nothing on
Linux, where mimalloc already names its mappings `mimalloc` through `PR_SET_VMA_ANON_NAME`. On macOS it only reaches a
`--features mimalloc` build, which doesn't ship, and `process_memory/vm_regions.rs` reads that build's heap off tag 100
(`MIMALLOC_ARENA_TAG`), so a new tag means moving that reader with it. How each build's heap reads from outside:
`../../docs/tooling/memory-debugging.md` § "First: which allocator holds the Rust heap".

**Why the choice lives here and not in the app**: every memory reader has to know which heap it's reading, and the
readers live here. With the choice beside them, the app's `main.rs` needs no cfg at all, and a reading can't disagree
with the allocator that's installed. The one exception: only a binary that installs `GLOBAL_ALLOC` runs on it, and test
binaries install their own counting allocator over `System`, so the mimalloc readers' tests allocate through `mi_malloc`
directly and run only in a `--features mimalloc` test build.

**What keeps the unused path alive**: the `clippy-mimalloc` check lane (slow, macOS) clippies the workspace with
`cmdr/mimalloc` in its own target dir, and `rustdoc`'s `--all-features` documents it. `THIRD-PARTY-NOTICES.md` covers
the default macOS build, so it doesn't credit mimalloc: shipping macOS on mimalloc again means making it that build's
default (the rule in `build.rs`), which brings the credit back with it.

### The readers

Under **mimalloc**, `query_rust_heap` reads `mi_process_info` (committed bytes), and mimalloc is not a registered malloc
zone, so `query_system_malloc_zones` sees every zone and none of the Rust heap. Under the **system allocator**, the Rust
heap IS the default malloc zone, shared with Objective-C and C code, so `query_rust_heap` reads that zone's statistics
(in use and reserved; macOS keeps no high-water mark) and `query_system_malloc_zones` skips it. Either way the two never
overlap, which the watchdog's `untracked` remainder relies on. The skip leans on libmalloc keeping the default zone
first in its registry; `a_malloc_block_lands_in_the_rust_heap_and_not_in_the_other_zones` pins it.

Neither can say what SHAPE the bytes are in, and that gap is what left a 643 MB block unnamed across three memory
investigations (`../../docs/notes/performance/idle-memory-profile-2026-07-28.md`).

`query_vm_regions` closes it. It walks the task's own VM map with `mach_vm_region_recurse` and folds the entries by
`user_tag`, so it produces the same rows `vmmap -summary` prints — in-process, with no `vmmap` to spawn and no
`MallocStackLogging` relaunch, and covering every allocator because every allocator ultimately takes its pages from the
kernel. mimalloc's arenas sit under tag 100 (`MIMALLOC_ARENA_TAG`, which macOS names `IOAccelerator`); the zones' pages
sit under `MALLOC_TAGS`.

The per-tag histogram of distinct region sizes is the part that names things. macOS routes any allocation past its 127
KB large-zone threshold to a VM region of exactly the requested size, so a repeated exact size under `MALLOC_LARGE` is a
fingerprint of whatever asked for that many bytes. That is how the CLIP Core ML towers were identified from a region
table alone: 101,187,584 bytes is the text tower's `49,408 × 512` fp32 token embedding and nothing else in the process
(`../cmdr-index/src/media_index/clip/DETAILS.md` § "What holding the towers costs"). The mechanism is asserted, not
assumed: `a_big_system_zone_block_becomes_a_malloc_large_region_sized_to_its_request`.

⚠️ **`<mach/vm_region.h>` lives inside `#pragma pack(push, 4)`, so `VmRegionSubmapInfo64` must be
`#[repr(C, packed(4))]`.** With plain `#[repr(C)]` the `u64` `offset` field gets 4 bytes of padding the kernel didn't
write, every field after it reads 4 bytes late, and the walk returns plausible-looking nonsense rather than an error:
tags above 255 (the tag space only goes to 255), a region count an order of magnitude short, and a freshly allocated 9
MiB block absent from the map entirely (verified on macOS 26.5, 2026-08-21).

Cost is one syscall per map entry, so it is snapshot-only — never per watchdog tick or per log line, unlike the
`task_info` readers beside it.

`query_rust_heap_snapshot` answers the question the others can't: of what the heap holds, how much is live data. Under
mimalloc it runs `heap_census.rs`, which walks every page of the main heap (in mimalloc v3 one heap spans every thread's
pages) and sums blocks in use against block space, with an allocation-free visitor and a page ceiling, and reads that
against tag 100's resident bytes. Its blind spots and why it's safe to run in a live app: the module header. Under the
system allocator there's no census to take (nothing in the zone tells a Rust block from an Objective-C one), and none is
needed for live bytes: the zones count them exactly. The snapshot weighs every zone's live bytes against every malloc
tag's resident bytes, because the VM tags can't say which zone a page belongs to.

## Bodies a backend gets for free

A backend whose only tools are a stat and a listing (SFTP, WebDAV) writes almost no `Volume` body of its own. Four
modules under `volume/` carry the arithmetic, each behind a trait the backend implements in a handful of lines:

- **`scan_walk.rs`** (`ScanSource`: `scan_stat` + `scan_list`) answers `scan_for_copy`, `scan_for_copy_batch`, and
  `scan_for_conflicts`. A backend whose operations bill per object answers `keeps_files` true (S3), and `scan_trees`
  hands back every file's size and date in `BatchScanResult::files` for a cost estimate; everyone else gets `None` and
  holds no list. The walk lists and ❌ never stats a child, so a 1,000-file folder costs one round trip per DIRECTORY;
  `dedup_bytes` tracks `total_bytes` because a backend reaching this walk has no link count. ❌ Nothing here consults
  `authoritative_listing`: that shortcut needs a watcher behind it, and these backends have none. ❗ A symlinked
  directory counts as the ONE entry it is and is never walked: following one double-counts its target (Android's
  `/sdcard` and `/storage/emulated/0` are the same bytes) and a link aimed at an ancestor never terminates.
  `scan_preview.rs` makes the same promise app-side, so a copy estimate reads the same whichever walker produced it.
- **`mkdir_all.rs`** (`MakesDirectories`) answers `create_directory_all`, leaf first so the common case costs one
  request. ❗ Its `DirectoryCreation` answer is the load-bearing part: the transfer driver spends a `Created` by
  skipping its per-file destination conflict probe, so anything short of certainty (a lost race included) answers
  `AlreadyExisted`. It also reports the SHALLOWEST directory it created, which is the one listing worth patching. A
  taken name isn't a folder until somebody looks, so the trait's third method (`leads_to`, one stat that follows links)
  is what turns a file in the way into `NotADirectory`. ❗ It is asked only after a create was refused, so a walk that
  works pays nothing for it; when each look happens, and why an unclassified answer means "look further up", is in the
  module docs. The trait's DEFAULT `create_directory_all` (local, SMB, MTP, the double) keeps the same promise through
  `volume_path_leads_to`, which replaces its per-ancestor `exists()` at the same cost.
- **`patching.rs`** (`PatchSource`) answers `notify_mutation` and the created / deleted / renamed patches around it. A
  patch is a courtesy and ❌ never fails the mutation that earned it, so every function returns `()`. A rename across
  directories is two changes, ❗ never one `Renamed`.
- **`secret_store.rs`** is the only place a backend reads or refreshes a stored secret, always on a blocking task: the
  store can put a Keychain prompt in front of a call, and a modal dialog on the async runtime holds every other volume.
  It REFRESHES and ❌ never seeds, because an empty store is the user having declined to remember.

SMB and MTP keep their own cache-aware batch scans (their watchers back the `authoritative_listing` shortcut) and borrow
only the pure halves, `conflicts_against` and `fold_batch`, so every backend hands a conflict dialog the same shape and
folds a batch the same way.

## `testing::disk_images`: a real removable volume, through one guarded runner

A test that needs a real removable volume (an eject `diskutil` can refuse, a drive that vanishes mid-scan) gets a
synthetic APFS or HFS+ disk image from `testing::disk_images` (macOS, behind `testing`). It lives here because the app's
eject pins and `cmdr-index`'s vanish pin both need it, and two copies of a runner whose whole value is its safety rules
would drift apart.

❌ Never a physical disk, and never a new FAT or exFAT image: an FSKit `msdos` unmount kernel-panicked a Mac
(`crates/cmdr-index/src/indexing/tests/CLAUDE.md`). `CreateFs::LegacyFat32` / `LegacyExFat` exist only for the hand-run
`external_drive_fixture`, through `DiskImage::attach_legacy_fat_fixture`.

- **The session.** `DiskImageSession::acquire()` takes an exclusive `flock` on `$TMPDIR/cmdr-disk-image-tests.lock` and
  hands back an `Arc`; every `DiskImage` holds a clone, so no image exists without the lock. The lock sits on its own
  open file description, so it serializes threads of one process as well as processes: two worktrees running real-image
  tests take turns, and one can't unmount under another. A second `acquire` on a thread that already holds one panics
  rather than waiting on itself forever.
- **The runner (`runner.rs`).** The closed `Call` enum IS the verb list: `hdiutil create`, `attach -plist -nobrowse`,
  `info -plist`, `detach` and `detach -force`; `diskutil info -plist`, `apfs addVolume … -nomount`,
  `partitionDisk … JHFS+ … R`, `mount -mountOptions nobrowse`, `unmount`, `unmountDisk`, `renameVolume`, and `eject`;
  plus `/sbin/umount`. `unmount` and `unmountDisk` are what a DiskArbitration approval test drives, since both go
  through DA. `/sbin/umount` is the opposite case and the one verb that isn't a disk tool: it goes straight to the
  kernel, so DA asks nobody and a session hears about it only afterwards, which is how a vanished drive is driven
  without pulling a real cable. It's ownership-checked like every other change. A verb gets added with its first caller.
  Every call is SIGKILLed past 30 s and reaped. Output goes to anonymous temp files rather than pipes, so a large plist
  can't stall the tool and a helper holding a pipe can't block a read past the kill.
- **Ownership before every change (`facts.rs`).** A call that changes a disk runs only after `diskutil info -plist`
  walks its target to the physical whole disk (a partition's `ParentWholeDisk`; for an APFS volume or container, through
  `APFSPhysicalStores` to the store's parent) and `hdiutil info -plist` lists BOTH the node and that whole disk under
  this image's `image-path`. A mount-point target must still be that volume's `MountPoint`. It's read fresh for every
  call because DiskArbitration hands a freed BSD unit to the next disk at once, so a node stored a second ago can name
  someone's Time Machine drive. `detach -force` also refuses an image stored on another attached image, and an image
  another of the session's images is stored on (decided on the `statfs` host node of the session's OWN backing files, ❌
  never a stat of someone else's image).
- **`image-path` is compared exactly, never canonicalized.** `hdiutil info` spells the backing file the way
  `hdiutil attach` was given it (`/var/folders/…`, while its text-mode alias reads `/private/var/folders/…`), so the
  harness compares against the path it attached with. Canonicalizing another image's path would stat a file that can sit
  behind TCC or on a hung share.
- **Specs** (verified on macOS 26.6.2, a hand probe plus the `real_images` tests, 2026-09-14). `Apfs` (64 MB) and `Hfs`
  (256 MB) are one GPT volume. `ApfsTwoVolumes` is a 1.1 GB SPARSE image plus `addVolume -nomount` and a nobrowse mount:
  `addVolume` answers -69493 on a small container. `HfsTwoPartitions` is `partitionDisk` into 60 MB plus the rest, which
  mounts both browsable, so each is unmounted and remounted nobrowse. A volume `addVolume` or `partitionDisk` made is
  found by its unique name among the nodes `hdiutil info` lists under the image (it lists them at once), ❌ never parsed
  from the tool's text. Names are `CMDR<pid><n>`.
- **Teardown.** `DiskImage`'s `Drop` resolves the whole disk fresh, detaches, falls back to a guarded `-force`, and then
  its `TestDir` deletes the file. An image that's already gone (an eject detached it) is fine; one that won't detach
  panics the test unless it's already panicking, so a leak never passes silently.
- **Leftovers are reclaimed at `acquire`**, because no `Drop` runs when the test process is SIGKILLed — which is what
  nextest does to a test past its cap, and what left six images attached on 2026-09-16. `reclaim_orphans` detaches only
  what `facts::orphan_verdict` proves is ours on EVERY count: the backing file under this process's temp dir, in a
  `cmdr_disk_image_*` directory, named `image.dmg` / `image.sparseimage`, with every mounted volume carrying the
  `CMDR<digits>` shape — then through the normal ownership check, and ❌ never `-force`. Anything failing a check is
  left alone, and logged when it looked like ours. ❗ Safe only at `acquire`: the machine-wide `flock` is held, and the
  kernel releases a dead process's `flock`, so no live session can own a matching attachment. The attach itself is
  covered separately: a failed or killed `hdiutil attach` re-checks and detaches what it left, since the guard that
  would own it doesn't exist yet.
- **`FileHolder`** holds a file open from a child `/bin/sleep`, so its volume refuses to unmount. The child descends
  from the test process, so anything classifying holders by ancestry reads it as the test's own: identify it by `pid()`.
- **`serving_pid()`** is the `hdid` process behind an attached image, which holds that image's backing FILE open for as
  long as the image stays attached. What a test needs to ask "who holds the volume the `.dmg` sits on".
- **Tests.** The pure decisions and the runner are default-suite unit tests. `real_images` attaches each spec for real,
  `#[ignore]`d in the `disk-image` nextest group, and runs with every real-image pin built on the harness in the opt-in
  `pnpm check disk-images` lane (`scripts/check/checks/DETAILS.md` § "The disk-image lane").

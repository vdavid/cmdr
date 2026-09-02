# `cmdr-fs` details

## Why the crate exists

`file_system/` sat on both sides of a cycle: the index subsystems referenced it (~70 `crate::file_system` refs), and
`file_system/*` referenced them back. A crate can't be carved out of one end of a cycle, so the shared vocabulary had to
become its own thing that both ends depend on and neither owns.

The boundary is enforceable rather than aspirational: nothing here can reach `tauri`, the index, or a real-storage
backend, because none of them are in the dependency graph. That's the property the whole extraction rests on, so treat
"just add a small dependency" as a design change, not a convenience.

## What's in, and why each thing had to be

The set was derived by the compiler, not by reading `use` lines: create the crate, move the hypothesised set, and let
`cargo check` enumerate the real closure. Three earlier attempts at an import census each missed something, always the
same way — a call through a local helper, a fully-qualified call inline in an expression, a `use` inside a function
body, and `#[cfg(test)]` items are all invisible to a header grep.

- **`Volume` + `types` + `ids`.** The trait is the crate's centrepiece: the index walks network volumes through
  `Volume::list_directory_for_scan`, so it needs the trait and every type the trait mentions.
- **`friendly_error/` + `git.rs`'s `FriendlyGitError`.** `VolumeError::FriendlyGit` carries the git error, and the git
  error maps onto `friendly_error::ErrorCategory`. A genuine two-way pair; neither can move without the other. The git
  type's only other non-`std` dependency is `serde`, so no git internals came along, and the boxed-trait fallback (keep
  the payload app-side behind a `cmdr-fs`-owned trait) wasn't needed.
- **`tcc_paths`.** `friendly_error/volume_error.rs` asks it whether a permission denial is really macOS TCC (it answers
  by probing the gate that covers the path, not by matching the path alone). Its parent `restricted_paths/mod.rs`
  imports `tauri::AppHandle`, so the child was split out and moved alone.
- **`FileEntry`.** 11 of the ~70 `crate::file_system` references from the index are this type; it isn't skippable. Its
  constructor pulled three more things down with it (below).
- **`InMemoryVolume`.** The one `Volume` impl that needs no host. It rides with the trait so a test in any crate can
  build a volume without the app.
- **`ignore_poison`, `pluralize`, `thread_qos`, `thread_cpu`, `process_memory`.** Host primitives with 8–49 references
  from the index trees, none of which can sensibly be injected. `thread_qos` in particular is the property that kept
  indexing in-process at all: a `tokio::runtime::Handle` does nothing for thread scheduling class. `thread_cpu` reads
  `CLOCK_THREAD_CPUTIME_ID` for the CALLING thread, which is the only way to attribute CPU to one thread on macOS:
  `ps -M` reports per-thread cumulative CPU but no thread names, so a thread has to report its own. It's cumulative on
  purpose, so a window is the difference of two readings; the index writer's heartbeat is its one consumer today
  (`../cmdr-index/src/indexing/writer/probe_stats.rs`).
- **`sqlite_util`.** A leaf over `std` + `rusqlite`, whose only two in-crate calls are `pluralize` and `ignore_poison`,
  both already here. It had to come down because five stores share it and they sit on both sides of the boundary: the
  three index DBs move into `cmdr-index`, while the agent's and the operation log's stay app-side. Putting it in
  `cmdr-index` would have made `agent/` and `operation_log/` depend on the index for connection plumbing, and there is
  only one `SQLITE_CONFIG_PAGECACHE` slab per process, so it genuinely has to be one instance both sides see. (The plan
  budgeted this as "move `run_incremental_vacuum` down", counted when the module was two references; the process-wide
  page-cache work landed four days later and took it to 33.)
- **`staging`.** The markers, the `StagingTemp` mint, and the in-flight registry. A mutating backend has to be able to
  stage a write, and the archive mutator already does; leaving the mint in the app would mean the first backend crate
  either reaches upward for it or grows a seam for something with no per-backend variation. What made the move
  mechanical is that the mint's only tie to write-op state is an `Option<Weak<()>>` liveness token the CALLER hands
  over, which names no app type. The two visibility settings stayed behind (below).
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

Nothing enforces the funnel in the type system. An ID crosses IPC as a `String` in ~3,600 Rust and ~1,600 TypeScript
sites, so a `VolumeId` newtype would be a very large refactor for a property one module already guarantees; the
guardrail is instead the never-build-an-ID-by-hand rule in each caller's `CLAUDE.md`, plus `VolumeManager::register`
logging an error whenever one ID does end up covering two mount roots.

## The four cuts that made the closure finite

Measured at file granularity over transitive `crate::` references, the hypothesised seed set dragged in **89 files and
~36,200 lines** — `network/`, `secrets/`, `volumes/`, `mtp/`, `settings/`, and ~15,600 lines of `indexing/`, which is
the cycle coming straight back. Four cuts took it to the 25 files and ~7,600 lines that are here.

### 1. `Volume::notify_mutation` lost its default body

The default opened with `use crate::file_system::listing::caching::…` and `use crate::file_system::listing::reading::…`
**inside the function body**, so the marquee type dragged the app's listing cache and listing I/O. Essentially the whole
blow-up ran through those two edges.

**Disposition: the default is now a documented no-op**, and the local-FS behavior lives app-side in
`file_system::listing::mutation::patch_listing_after_local_mutation`.

Why this over the alternatives:

- **Making it a required method** (no default body) would touch ~45 `impl Volume for` sites, most of them test doubles,
  which contradicts "no other app file changes" for no gain: every one of them would write `Box::pin(async {})`.
- **A `MutationObserver` trait** would need an injection point the signature doesn't have, so it would land as a
  `OnceLock` global inside this crate — exactly the shape the extraction is trying to get rid of.
- The no-op is also a correctness improvement. All four real backends (`LocalPosixVolume`, `SmbVolume`, `MtpVolume`, and
  the read-only `ArchiveVolume`, which never calls it) already override the method, so **nothing changed behaviorally**.
  The only consumers of the old default were `InMemoryVolume` and the test doubles — for which "stat the real filesystem
  through `std::fs`" was never right.
- `LocalPosixVolume` already carried a verbatim copy of the default body, so extracting the helper **removed** ~50 lines
  of duplication rather than adding any. Two copies of a cache-patching routine is exactly the thing that rots apart.

**Guardrail this leaves behind**: a new mutable backend that forgets to override `notify_mutation` gets a silently stale
pane instead of a free correct one. That's why the trait doc says so and why the backends checklist repeats it.

### 2. `FileEntry::new`'s three predicates came down

`FileEntry::new` sets `icon_id` through a local `get_icon_id` helper (which calls `icons::special_folders` and
`icons::per_path`) and `is_archive` from a fully-qualified inline call into the archive backend. None of the three
appear in any header `use` line.

All three are pure name/path predicates with no I/O — that's a hard requirement, since they run for every entry of a
100k-entry listing — so **they moved down** rather than being stripped or injected:

- `icons/special_folders.rs` moved whole (its only non-`std` dependency is `dirs`).
- The package half of `icons/per_path.rs` split into `icons/packages.rs`. The custom-icon half stayed: it needs a
  `getxattr` syscall, which is why it never runs during a listing in the first place.
- `has_supported_archive_extension` delegates to `format_for_name`, so the whole name → format vocabulary moved
  (`ArchiveFormat`, `TarCodec`, `format_for_name`, `format_for_path`, `is_sequential`). The decoders that unwrap a tar's
  outer compression stayed with the archive reading core. The split line is "naming vs machinery", and it keeps
  `format_for_name` the single source of truth rather than forking a second suffix table.

Stripping the two fields instead was never viable: `FileEntry::new` has 83 call sites.

### 3. `filesystem_kind` split

`detect_filesystem_for_path` reaches `crate::volumes::get_mount_point` (macOS) or `crate::file_system::linux_mounts`
(Linux). The module's own doc already drew the line — classification is platform-free, detection is thin platform wiring
— so the classification moved and detection stayed. Callers that want detection stay app-side anyway.

### 4. The dead scanner/watcher apparatus

Already deleted before this crate existed: `VolumeScanner`, `VolumeWatcher`, `Volume::scanner()`, `Volume::watcher()`,
and their `LocalPosixVolume` implementations. They were the `file_system → indexing` half of the cycle and had zero
callers.

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
  UI: `PhaseRecord.trigger` renders in the developer debug panel, which is diagnostics, not product copy.
- **`FileEntry::display_size` / `display_size_tooltip`** are `String` fields rendered verbatim in the Size column. They
  are _written_ by the app-side git module; this crate only carries them. The bar is about production, not presence.

Anyone grepping `String` in this crate and concluding the bar was abandoned should read this paragraph first.

## `ScanTicker`: one ticker, so the promise can't drift

`Volume::scan_for_copy_batch_with_progress` promises counts that are **cumulative for the call** — callers shift by
their own baseline across several calls, so a per-path reset makes the scan dialog's counters jump backwards. Every
remote backend needs exactly that, and two hand-rolled copies would drift on precisely the part that matters. So the
ticker lives here and both `cmdr-smb` and `cmdr-sftp` count through it.

⚠️ It exists at all because a recursive scan over a network backend reports nothing until it returns: the transfer
dialog sits on "0 files" for the length of the walk, and `write_operations/scan_watchdog.rs` — which bounds a preview by
INACTIVITY — can't tell a slow tree from a server that stopped answering.

## `Volume::copy_within`: letting a server copy for itself

Some protocols can copy a file from one path to another without the bytes travelling through Cmdr (SFTP's
`copy-data@openssh.com`; SMB has `FSCTL_SRV_COPYCHUNK`, unimplemented today). Duplicating a large file inside one server
otherwise sends it down the link and straight back up.

The default is `NotSupported`, and ❗ a caller must read that as "do it the ordinary way" rather than as a failure: it
is the answer both for a backend with no such operation and for one whose SERVER simply lacks the extension, which is
only knowable at runtime. `write_operations/transfer/volume/strategy.rs::try_server_side_copy` is the one caller, and it
asks only when `Arc::ptr_eq` says both sides of the copy are the same volume.

Two contract points are load-bearing:

- ❗ **Never single-shot.** The destination genuinely holds a byte-incomplete file while this runs, so the caller stages
  it exactly as it stages a streamed write. A backend answering otherwise would be asking for a partial at the user's
  chosen filename.
- **`to` is created or TRUNCATED**, matching `write_from_stream`, so a caller-minted safe-replace temp works unchanged.

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
- `assert_create_file_refuses_to_clobber` — the New File command renders the refusal as "that name is taken", so a
  clobbering backend silently empties a file and reports success.
- `assert_create_directory_all_reports_an_existing_dir_honestly` — `Created` promises the leaf was empty, and the
  transfer driver spends it by skipping the per-file destination conflict probe inside. Only the dangerous direction is
  pinned; answering `AlreadyExisted` for a leaf you did create is merely slower, which is what the trait means by "when
  in doubt, answer `AlreadyExisted`". MTP is the backend this matters most for: it answers
  `create_directory_errors_on_existing_dir() == false`, so the default walk learns "already there" from its `exists`
  probe rather than from a collision error.
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

`InMemoryVolume`, `LocalPosixVolume`, `AdbVolume`, and the Docker-gated `SmbVolume`, `SftpVolume`, and `WebdavVolume`
run every one (InMemory's writability cell sits in `capabilities_test.rs`, next to the predicate it speaks for).
`MtpVolume` runs all but `create_file` (which it doesn't implement: an upload there is `write_from_stream`, one
`SendObject` transaction) and `rename` (which it DOES implement, so that one is an open gap rather than a
not-applicable), and its `delete` cell lives in `mtp_delete_test.rs` for the scaffolding that contract needs.
`ArchiveVolume` is read-only: it runs the three that don't mutate and pins the rest of the ground with
`every_mutation_is_unsupported`, and it is deliberately outside the conflict-scan one, since nothing copies INTO an
archive through the volume. A backend that adds a mutation adds the matching call.

## The faults `InMemoryVolume` can be told to have

Everything above is what the double gets RIGHT unconditionally. On top of that it can be told to misbehave in specific,
named ways, so a caller's defense against a hostile backend is testable rather than assumed. Each is test-only, and each
models something a real backend genuinely does:

- **`with_delete_failing()`** — `delete` returns an `IoError` instead of removing the entry. A backend that can't remove
  a path (a permission, a lock, a dead session).
- **`set_stat_failing(path)`** — `is_directory` and `get_metadata` FAIL for that path rather than reporting it missing.
  The distinction is the whole point: `NotFound` is an ANSWER, and code that turns an unanswered stat into a confident
  "not a directory" routes a folder into a file-shaped, destructive branch.
- **`set_reported_type(path, is_directory)`** — the stat and the listing report a type the entry doesn't have, while its
  real contents stay put. A stale or racy directory entry, and the exact lie the original cross-volume copy bug rode in
  on.
- **`set_reported_size(path, bytes)`** — the listed size disagrees with the real streamed byte count. A remote source
  whose directory entry is stale; a transfer planning against the real stream still lands correct bytes.
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

## `process_memory`: three accountants, and the one reader that spans them

`query_mimalloc_heap` sees only our Rust heap. `query_system_malloc_zones` sees only the registered macOS zones, which
mimalloc never joins. Neither can say what SHAPE the bytes are in, and that gap is what left a 643 MB block unnamed
across three memory investigations (`../../docs/notes/idle-memory-profile-2026-07-28.md`).

`query_vm_regions` closes it. It walks the task's own VM map with `mach_vm_region_recurse` and folds the entries by
`user_tag`, so it produces the same rows `vmmap -summary` prints — in-process, with no `vmmap` to spawn and no
`MallocStackLogging` relaunch, and covering BOTH allocators because every allocator ultimately takes its pages from the
kernel.

The per-tag histogram of distinct region sizes is the part that names things. macOS routes any allocation past its 127
KB large-zone threshold to a VM region of exactly the requested size, so a repeated exact size under `MALLOC_LARGE` is a
fingerprint of whatever asked for that many bytes. That is how the CLIP Core ML towers were identified from a region
table alone: 101,187,584 bytes is the text tower's `49,408 × 512` fp32 token embedding and nothing else in the process
(`../cmdr-index/src/media_index/clip/DETAILS.md` § "What holding the towers costs"). The mechanism is asserted, not
assumed: `a_big_system_zone_block_becomes_a_malloc_large_region_of_exactly_its_size`.

⚠️ **`<mach/vm_region.h>` lives inside `#pragma pack(push, 4)`, so `VmRegionSubmapInfo64` must be
`#[repr(C, packed(4))]`.** With plain `#[repr(C)]` the `u64` `offset` field gets 4 bytes of padding the kernel didn't
write, every field after it reads 4 bytes late, and the walk returns plausible-looking nonsense rather than an error:
tags above 255 (the tag space only goes to 255), a region count an order of magnitude short, and a freshly allocated 9
MiB block absent from the map entirely (verified on macOS 26.5, 2026-08-21).

Cost is one syscall per map entry, so it is snapshot-only — never per watchdog tick or per log line, unlike the
`task_info` readers beside it.

## Bodies a backend gets for free

A backend whose only tools are a stat and a listing (SFTP, WebDAV) writes almost no `Volume` body of its own. Four
modules under `volume/` carry the arithmetic, each behind a trait the backend implements in a handful of lines:

- **`scan_walk.rs`** (`ScanSource`: `scan_stat` + `scan_list`) answers `scan_for_copy`, `scan_for_copy_batch`, and
  `scan_for_conflicts`. The walk lists and ❌ never stats a child, so a 1,000-file folder costs one round trip per
  DIRECTORY; `dedup_bytes` tracks `total_bytes` because a backend reaching this walk has no link count. ❌ Nothing here
  consults `authoritative_listing`: that shortcut needs a watcher behind it, and these backends have none. ❗ A
  symlinked directory counts as the ONE entry it is and is never walked: following one double-counts its target
  (Android's `/sdcard` and `/storage/emulated/0` are the same bytes) and a link aimed at an ancestor never terminates.
  `scan_preview.rs` makes the same promise app-side, so a copy estimate reads the same whichever walker produced it.
- **`mkdir_all.rs`** (`MakesDirectories`) answers `create_directory_all`, leaf first so the common case costs one
  request. ❗ Its `DirectoryCreation` answer is the load-bearing part: the transfer driver spends a `Created` by
  skipping its per-file destination conflict probe, so anything short of certainty (a lost race included) answers
  `AlreadyExisted`. It also reports the SHALLOWEST directory it created, which is the one listing worth patching.
- **`patching.rs`** (`PatchSource`) answers `notify_mutation` and the created / deleted / renamed patches around it. A
  patch is a courtesy and ❌ never fails the mutation that earned it, so every function returns `()`. A rename across
  directories is two changes, ❗ never one `Renamed`.
- **`secret_store.rs`** is the only place a backend reads or refreshes a stored secret, always on a blocking task: the
  store can put a Keychain prompt in front of a call, and a modal dialog on the async runtime holds every other volume.
  It REFRESHES and ❌ never seeds, because an empty store is the user having declined to remember.

SMB and MTP keep their own cache-aware batch scans (their watchers back the `authoritative_listing` shortcut) and borrow
only the pure halves, `conflicts_against` and `fold_batch`, so every backend hands a conflict dialog the same shape and
folds a batch the same way.

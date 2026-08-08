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
- **`detect_filesystem_for_path`** — see cut 3.
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

## `InMemoryVolume` honors the contracts data safety leans on

The double is the oracle: two `Volume` contracts have to hold in it, not just on the happy path.

- **`delete` refuses a NON-EMPTY directory** (`ENOTEMPTY`). The same-volume rename-merge preserves a skipped child's
  source purely by letting its parent's cleanup delete FAIL. A permissive `delete` disarms that whole test class.
- **`rename` of a directory carries its whole subtree.** A same-volume move IS directory renames, so a `rename` that
  moved only the dir node made those tests pass over the exact data-loss shape they existed to catch.

❌ Never relax a contract to make a test green.

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

A fault the caller wants to arm on a call COUNT rather than on a path belongs one layer up, in the app's `FaultyVolume`
wrapper (`file_system/write_operations/transfer/volume/faulty_volume_test_support.rs`): it wraps any volume and fails
the Nth call to a named operation. ❌ Don't grow this list with fault shapes that aren't about what a real backend does.

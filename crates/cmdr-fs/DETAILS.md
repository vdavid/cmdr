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
  type's only other non-`std` dependency is `serde`, so no git internals came along — the plan's boxed-trait fallback
  wasn't needed.
- **`tcc_paths`.** `friendly_error/volume_error.rs` asks it whether a permission denial is really macOS TCC. Its parent
  `restricted_paths/mod.rs` imports `tauri::AppHandle`, so the child was split out and moved alone.
- **`FileEntry`.** 11 of the ~70 `crate::file_system` references from the index are this type; it isn't skippable. Its
  constructor pulled three more things down with it (below).
- **`InMemoryVolume`.** The one `Volume` impl that needs no host. It rides with the trait so a test in any crate can
  build a volume without the app.
- **`ignore_poison`, `pluralize`, `thread_qos`, `process_memory`.** Host primitives with 8–49 references from the index
  trees, none of which can sensibly be injected. `thread_qos` in particular is the property that kept indexing
  in-process at all: a `tokio::runtime::Handle` does nothing for thread scheduling class.
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

## The trap that only showed up at runtime

`thread_qos::set_current_thread_qos` was `#[cfg(all(target_os = "macos", not(test)))]`. That `not(test)` silently stops
meaning anything the moment the code becomes a dependency: `cfg(test)` is set only while compiling a crate's _own_ test
target, so the app's tests started applying the real `QOS_CLASS_UTILITY` to their background threads. Under nextest's
one-process-per-core parallelism that starved a walker test past its stall watchdog, and
`walker::tests::a_read_that_keeps_delivering_is_never_abandoned` failed for a completely real reason.

The condition is now `not(any(test, feature = "testing"))`. The lesson generalizes: **any `cfg(test)`-conditioned
BEHAVIOR (not just a test module) changes meaning when its code moves into a dependency.** Grep for it before moving
anything else down.

The same shape bit once more, harmlessly: the `Volume::inject_error` E2E hook is `#[cfg(feature = "playwright-e2e")]`, a
feature that lived only on the app. This crate now declares its own, and the app's enables it via
`cmdr-fs/playwright-e2e`.

## What the app kept, and why

- **Every real-storage backend** (`LocalPosixVolume`, `SmbVolume`, `MtpVolume`, `ArchiveVolume`) with their `smb2` /
  `mtp-rs` / `gix` / mount-detection dependencies. Only their shared trait moved.
- **`VolumeManager`** — the process-wide registry. The index reaches it through an injected provider, not by importing
  it.
- **`file_system::listing::mutation::patch_listing_after_local_mutation`** — see cut 1.
- **`detect_filesystem_for_path`** — see cut 3.
- **`icons/per_path.rs`'s custom-folder-icon half**, the NSWorkspace fetch, and the icon disk cache.
- **The archive tar decoders**, and everything else under `backends/archive/`.
- **`test_support.rs`'s allocation-counting harness**, because a `#[global_allocator]` has to be per-binary.

## The one place prose is produced here

The API contract says this crate emits no user-facing strings. Two things look like exceptions and aren't:

- **`pluralize`** formats "1 file" / "2 files". All 49 of its call sites build log lines. It lives here because it's a
  leaf with no dependencies, not because copy generation belongs in a filesystem crate. One of its outputs does reach a
  UI: `PhaseRecord.trigger` renders in the developer debug panel, which is diagnostics, not product copy.
- **`FileEntry::display_size` / `display_size_tooltip`** are `String` fields rendered verbatim in the Size column. They
  are _written_ by the app-side git module; this crate only carries them. The bar is about production, not presence.

Anyone grepping `String` in this crate and concluding the bar was abandoned should read this paragraph first.

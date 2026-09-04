# `cmdr-index` details

## Why the crate exists

Three reasons, in priority order. When a decision here is ambiguous, resolve it toward the higher one.

1. **Encapsulate the hardest code in the codebase behind a boundary you can reason about.** These three subsystems are
   about 28% of the backend and hold the gnarliest concurrency and lifecycle logic in the product, over ~50 process-wide
   mutable statics. While they lived in the app there was no line between "what the app may rely on" and "internals", so
   every app change could reach into index internals and every index change had an unbounded blast radius. A crate makes
   that line real and compiler-enforced.
2. **Build-time separation.** Backend work that doesn't touch the index no longer rebuilds the index, and vice versa.
3. **The index could one day be a product of its own.** "Cmdr, plus a smart file+image index any agent can tap into"
   needs a documented, stable, self-contained API. This is that API. It is NOT a daemon; there's no separate process
   here, and the deferred escalation lives in `docs/specs/later/indexing/out-of-process-indexing.md`.

## The contract this crate is held to

1. **No `tauri` in the dependency tree.** The load-bearing property, and exactly the kind that erodes one convenient
   import at a time.
2. **No user-facing strings produced here.** The index emits typed values; the host renders every word. Two deliberate
   exceptions, both from `cmdr-fs` rather than this crate: `FileEntry::display_size` (written app-side) and `pluralize`,
   which builds log lines. `PhaseRecord.trigger` carries pluralized text that the developer debug panel renders, which
   is acceptable but means "pluralize is purely log-only" isn't a claim to make.
3. **Typed errors everywhere.** No `Box<dyn Error>`, no stringly-typed failure, so a host never string-matches. Two
   named exceptions rather than zero: `IndexError::Internal(Diagnostic)` is the log-only residue for causes no caller
   acts on, and `ReadPool::with_conn` still returns `Result<T, String>`. Both are in `src/indexing/handle/DETAILS.md` §
   "The three exceptions, named". Typing the causes inside `lifecycle/state.rs` and `read/queries.rs` is still open
   work. The house error style is in `apps/desktop/src-tauri/CLAUDE.md`.
4. **Long-running work is cancelable** through one primitive (`tokio_util::sync::CancellationToken`), with cancellation
   observable from outside: a cancelled operation returns a distinct error variant, never a silent early return. One
   subsystem doesn't meet this yet: an `importance` recompute holds no token and no stop hook, so nothing stops it
   mid-walk. That gap and its entry point are in `src/indexing/host/DETAILS.md` § Cancellation.
5. **Everything long-running reports progress** as structured values through a caller-supplied sink.
6. **A handle, not a global.** The public API is methods on an `Index` the host constructs and owns.
7. **The house lint set is replicated, not weakened**, via `lints.workspace = true`, plus `#![deny(missing_docs)]`.
8. **Ingest and query are equal citizens.** `observe_listing` and `size_of` are designed and compiled, returning
   `NotImplemented` until they're built. They exist so the two features that need them can't force a redesign.

## What the host has to answer

`indexing/host/` declares five seams and nothing else may cross the boundary. In Cmdr, `src/index_host.rs` answers all
five in one function, at the top of `setup()`.

- **`EventSink`** — where the index reports. The host maps `IndexEvent` to its own wire format; error reporting rides
  the same channel, because a crate can't invoke the app's `log_error!` macro across the boundary and dropping it
  silently would be a feedback-loop regression.
- **`VolumeProvider`** — which volumes exist, volume identity, and mount classification ("is this a network fs?"). The
  index never touches a volume manager, a platform mount probe, or an MTP session layer directly.
- **`HostPolicy`** — "may I do background work right now?", composed from the host's own priority signals. Consulted
  inside scan loops, so it returns a cheap `Copy` value and callers cache it per batch. ❌ No trait may be introduced on
  a per-entry path; wanting one is a signal to restructure the call.
- **The runtime** — a `tokio::runtime::Handle`. See the `CLAUDE.md` must-know.
- **`IndexConfig`** — plain values passed at build time and updatable through `reconfigure`. The crate never reads a
  settings file, an env var, or the full-disk-access choice for itself: policy belongs to the product. The `CMDR_*`
  debug knobs are the deliberate exception, and stay `std::env::var` reads.

Their rationale, and what each one replaced, is in `src/indexing/host/DETAILS.md`.

## Why `specta` is an unconditional dependency

58 data types derive `specta::Type`, plus `FileEntry` and `TagRef` down in `cmdr-fs`, and `FolderSignals`'s serde shape
is load-bearing. Making that an optional feature reads like the tidier choice and is the worse one: the app is the only
consumer and always enables it, and nothing in the check runner builds `--no-default-features`, so the specta-off
configuration would be compiled zero times and rot on the first edit. Unconditional costs nothing now that
`tauri-specta` is out of the tree, and it means the crate has one shape rather than two, one of which is never tested.

Bindings collection is unaffected: the app's `ipc.rs` collects types transitively through command signatures, and a
cross-crate `specta::Type` impl collects normally. What DOES break is two `specta` versions in one graph, which is why
the pin is exact and identical to the app's.

## Why `indexing` is private and the other two aren't

`lib.rs` re-exports the file index's promises item by item, so the crate root is the one place that answers "what may a
host rely on?". Keeping `mod indexing` private buys two things: `cmdr_index::indexing::…` never appears in a caller (the
handle and its vocabulary sit at the root, where they read cleanly), and `pub(in crate::indexing)` keeps meaning
"internal to the file index" rather than silently widening to `media_index` and `importance`, which are siblings in the
same crate.

`media_index` and `importance` stay public under their own names because each carries a curated surface of its own (see
each `mod.rs`) and because their names don't stutter against the crate's.

## The two gated surfaces

Both are `#[doc(hidden)]`, both are turned on through a **dev-dependency** so they stay out of shipped builds, and
neither is a promise.

- **`testing`** — what a test outside this crate needs to drive the index: fake volumes, a recording sink, a
  controllable priority policy, a temp data dir, a reserved registry slot, the disk-image fixture, and the direct
  scan/writer/store entry points a host-side test uses to prove its own backend works with the index's scanner. Reached
  through `cmdr_index::testing`, grouped by what a test is trying to do. `tempfile` is a normal optional dependency
  rather than a dev-dependency because `reserve_initializing_index_for_test` hands a `TempDir` back to its caller.
- **`tooling`** — the importance evaluation corpus and measurement entry points that `crates/index-query`'s three
  importance binaries drive. Separate from `testing` because they answer different questions: a test needs fakes and
  guards, a tool needs the real scoring pipeline plus a corpus. And because those consumers are BINARIES in another
  crate, which `cfg(test)` can never reach.

## The `cfg(test)` trap

`cfg(test)` is set only while a crate compiles its OWN test target. It is NOT set when a consumer compiles this crate as
a dependency, even from that consumer's test build. So a `#[cfg(test)]` item with a consumer outside the crate simply
isn't there, and the failure is a missing symbol at best.

This fired four separate times across the extraction, and the last batch surfaced only when the code actually became a
dependency: `one_of_every_kind` (the host's event-mapping completeness test), the disk-image fixture,
`ScanPacer::unpaced`, `IndexStore::list_children`, and `handle::test_lock`. Each is now on the `testing` surface.

**The rule, in both directions:** `#[cfg(test)]` while every consumer is inside the crate; a feature the moment one
isn't. And a feature for an item with only in-crate callers isn't a harmless over-approximation, because the app enables
`testing` for every dev target, so the item exists in the non-test lib build with nothing calling it and
`#[deny(unused)]` turns that into a hard error.

## The counting allocator is duplicated, on purpose

`indexing/test_support.rs` installs a `#[global_allocator]` so the memory-shape guards can assert what a hot path
allocates. A binary gets exactly ONE global allocator, so it can't live in `cmdr-fs` (every binary linking that crate,
including the shipped app, would get a second one) and it can't ride a feature (dev-dependency features unify with
normal ones for the same package).

That makes it per test BINARY, and the host has its own. Cmdr keeps a trimmed copy in
`apps/desktop/src-tauri/src/test_support.rs` for `search/ranking/memory_tests.rs`. **This fails by measuring zero, not
by failing to compile**, so that test asserts a non-zero measurement before it asserts a budget. Note for anyone
comparing memory numbers: Rust test runs are measured under the counting allocator, not mimalloc.

## One fingerprint helper, two policy stamps (`fingerprint.rs`)

All three databases here are disposable caches, and two of them persist a stamp saying which compile-time policy their
rows were written under: the index's scan exclusions (`indexing/scanner/exclusions.rs::exclusion_policy_fingerprint`,
gating `index_predates_exclusion_policy`) and importance's classification rules
(`importance/classify.rs::scoring_policy_fingerprint`, gating `store::needs_full_pass`). Both hash their constant lists
through `fingerprint::fingerprint_of`, so editing a list re-arms every existing DB with no version number for anyone to
forget to bump.

The mixing is FNV-1a rather than `DefaultHasher` because the value goes to disk and must not shift with a toolchain
upgrade. It's crate-internal and shared rather than copied per subsystem, so ONE golden test
(`the_fingerprint_mixes_its_input`) covers both: a hash that collided two policies into one value would pass every
symmetric stamp-and-compare test while silently skipping the work the stamp exists to trigger.

## What deliberately stayed with the host

- **Every real-storage `Volume` backend** (local POSIX, SMB, MTP, archive) with its `smb2` / `mtp-rs` / git /
  mount-detection dependencies, plus the volume manager. The index reaches them only through `VolumeProvider`.
- **The 15 event payload structs.** The crate's events are a plain Rust enum with no `serde` and no `tauri_specta`; the
  host owns the wire format and every word in it. Schema derives on DATA are fine (58 types derive `specta::Type`);
  presentation decisions are not, and events are where presentation lives.
- **Every `#[tauri::command]`**, in the app's `commands/`.
- **`search/`.** A product surface with ranking choices and UI copy. It reads the index database directly, which is why
  `store` and `ReadPool` are public: that reach is deliberate and documented, not an accident.
- **`operation_log/`** and the agent's store, which share `cmdr-fs`'s one process-wide SQLite page-cache slab with the
  index's three databases. That's why the connection factories live in `cmdr-fs` rather than either end.

## Related

- `crates/cmdr-fs/DETAILS.md` — the layer below: the vocabulary this crate indexes, and why each piece is down there.
- `src/indexing/handle/DETAILS.md` — the public-surface audit, item by item.
- `src/indexing/host/DETAILS.md` — the seams and their rationale.
- `docs/specs/later/indexing/out-of-process-indexing.md` — the deferred daemon escalation this boundary makes cheaper.

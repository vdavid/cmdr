# `cmdr-index`

Everything Cmdr knows about what's on a volume, what's inside its images, and which of its folders matter, behind one
`Index` handle a host builds and holds. **No `tauri` in the dependency tree, and no reach into the app**: whatever the
index needs from an application arrives through the traits in `indexing/host/`, and whatever it reports leaves through
an `EventSink`. That's what lets the same code run under Cmdr, under a test with an `InMemoryVolume`, and under a bench
with no host at all.

## Module map

- `lib.rs`: the crate's whole public surface, in one file. If it isn't re-exported here, a host can't rely on it.
- `indexing/`: the file index. Scans volumes into per-volume SQLite databases, keeps them fresh against filesystem
  events, and answers recursive-size and freshness questions. Private at the root: everything it promises is re-exported
  from `lib.rs`. Its `CLAUDE.md` routes to the twelve areas inside.
- `media_index/`: OCR, Vision tags, and CLIP embeddings over the images the file index found, with ANN search on top.
- `importance/`: a deterministic, cheap "which folders matter" score that expensive features consult before spending.
- `benches/index_benchmarks.rs`: the enrichment and dir-stats hot paths, plus the aggregate roll-up. They compile
  against this crate as an EXTERNAL one, so they only reach what a host can.

## Must-knows

- **`#![deny(missing_docs)]` holds.** A new `pub` item, field, or enum variant needs a doc comment, on both platforms.
  Several of these types cross IPC through `specta::Type`, so the comment lands in `bindings.ts` too.
- **A `pub` in `lib.rs` is a promise, not a compile fix.** Reach for a facade method on `Index`, a fold into an existing
  call, or the `testing` / `tooling` gate before widening the surface. The item-by-item audit behind today's shape is
  `src/indexing/handle/DETAILS.md` § "The public surface".
- **❌ Never gate a test reach-through on `cfg(test)` when the consumer is a HOST.** `cfg(test)` is set only while this
  crate compiles its own test target, so a consumer's test build sees the item vanish. Rule: `#[cfg(test)]` while every
  consumer is inside the crate, `#[cfg(any(test, feature = "testing"))] pub` the moment one isn't. This has bitten four
  times. `DETAILS.md` § "The `cfg(test)` trap".
- **A feature for an item with only in-crate callers is NOT harmless.** The app turns `testing` on for every dev target,
  so the item exists in the non-test lib build with nothing calling it, and `#[deny(unused)]` makes that a hard error.
- **`specta` is pinned to `=2.0.0-rc.24`, identical to the app's.** Two `specta` crates in one graph and these `Type`
  impls stop satisfying `tauri-specta`, which breaks bindings generation.
- **Nothing here produces user-facing prose.** The index emits typed values; the host renders every word a human reads.
  Diagnostic strings for `log::` are fine and stay English. `DETAILS.md` names the two deliberate exceptions.
- **We never build a runtime.** The host injects a `tokio::runtime::Handle`. A second thread pool would compete for the
  same cores and split the thread-QoS story that lets indexing run in-process at all.
- **Rebuild, don't migrate.** All three databases are disposable caches; a format or scope change invalidates and
  rescans. ❌ Don't build machinery to preserve them without David's say-so for a specific case.

Why the crate exists, what each subsystem owes the others, the two gated surfaces, and the traps that have already
fired: `DETAILS.md`. Read it before moving anything across the boundary in either direction.

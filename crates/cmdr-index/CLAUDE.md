# `cmdr-index`

Everything Cmdr knows about what's on a volume, what's inside its images, and which of its folders matter, behind one
`Index` handle a host builds and holds. **No `tauri` in the dependency tree, and no reach into the app**: whatever the
index needs from an application arrives through the traits in `indexing/host/`, and whatever it reports leaves through
an `EventSink`. That's what lets the same code run under Cmdr, under a test with an `InMemoryVolume`, and under a bench
with no host at all.

## Module map

- `lib.rs`: the crate's whole public surface. If it isn't re-exported here, a host can't rely on it.
- `indexing/`: the file index. Scans volumes into per-volume SQLite databases, keeps them fresh against filesystem
  events, and answers recursive-size and freshness questions. Private at the root: everything it promises is re-exported
  from `lib.rs`. Its `CLAUDE.md` routes to the twelve areas inside.
- `media_index/`: OCR, Vision tags, and CLIP embeddings over the images the file index found, with ANN search on top.
- `importance/`: a deterministic, cheap "which folders matter" score that expensive features consult before spending.
- `volume_files.rs`: each volume's file names and folders; the ONE door they're removed by. `drive_index_relocation.rs`:
  adopts an older build's drive index at `build`.
- `benches/index_benchmarks.rs`: the enrichment, dir-stats, and roll-up hot paths, compiled as an EXTERNAL crate, so
  they reach only what a host can.

## Must-knows

- **`#![deny(missing_docs)]` holds.** A new `pub` item, field, or enum variant needs a doc comment, on both platforms.
  Several of these types cross IPC through `specta::Type`, so the comment lands in `bindings.ts` too.
- **A `pub` in `lib.rs` is a promise, not a compile fix.** `index-crate-isolation` caps root promises, `Index` methods,
  public modules, and their items, and asserts no `tauri` / `tauri-specta` / `cmdr` in either index crate's tree. Take
  one of the four dispositions (facade, fold, delete, gate); raise a ceiling only when the wider surface is genuinely
  better, said in the commit and `src/indexing/handle/DETAILS.md`. ❌ Never bump to pass.
- **❌ Never gate a test reach-through on `cfg(test)` when the consumer is a HOST**: a consumer's test build sees the
  item vanish. `#[cfg(test)]` while every consumer is in-crate, `#[cfg(any(test, feature = "testing"))] pub` the moment
  one isn't. Bitten four times. `DETAILS.md` § "The `cfg(test)` trap".
- **A feature for an item with only in-crate callers is NOT harmless.** The app turns `testing` on for every dev target,
  so the item exists in the non-test lib build with nothing calling it, and `#[deny(unused)]` makes that a hard error.
- **`specta` is pinned to `=2.0.0-rc.24`, identical to the app's.** Two `specta` crates in one graph and these `Type`
  impls stop satisfying `tauri-specta`, which breaks bindings generation.
- **Nothing here produces user-facing prose.** The index emits typed values; the host renders every word a human reads.
  Diagnostic strings for `log::` are fine and stay English. `DETAILS.md` names the two deliberate exceptions.
- **We never build a runtime.** The host injects a `tokio::runtime::Handle`; a second pool would compete for cores and
  split the thread-QoS story that lets indexing run in-process.
- **Rebuild, don't migrate.** All three databases are disposable caches; a format or scope change rescans. ❌ No
  machinery to preserve them without David's say-so.
- **Only the drive index lives in the cache dir**, out of backups. ❌ Never derive a store's folder from another store's
  path; ask `StoreDirs`. `DETAILS.md` § "Where the stores live".

Why the crate exists, what each subsystem owes the others, the two gated surfaces, and the traps that have already
fired: `DETAILS.md`. Read it before moving anything across the boundary in either direction.

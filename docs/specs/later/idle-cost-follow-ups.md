# What the idle-cost effort deliberately left

Cmdr's idle bill got two structural fixes: each CLIP tower loads on demand, so an enrichment pass stops holding ~246 MB
of text tower it never calls, and a storm of one-shot rescan anchors now costs one visible sweep a day instead of one
subtree walk each. Both live beside the code: `crates/cmdr-index/src/media_index/clip/DETAILS.md` § "What holding the
towers costs" and `crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/DETAILS.md` § "Anchor-cardinality
routing".

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

**Read first**: `docs/notes/idle-cpu-attribution-2026-08-03.md`. Four successive hypotheses about where this CPU goes
were refuted by measurement, and each refutation left a rule behind: never rank work off one `sample` window, never
report a share as CPU when the leaf frame is a syscall, never infer CPU from log volume.

## The item that isn't a tail: nobody knows where the bill stands

Every number this effort was ranked against came from **one v0.37.0 measurement on 2026-08-03**, on a machine that was
also running six cargo builds: 110 minutes of CPU over 9.1 hours, at a 1.78 GB footprint. It has never been re-taken.
Since then the sync-status poll, the live tick's folder-score re-read, the SQLite page-cache slab, the text tower, and
the rescan drain have all changed.

**So the next measurement is a fresh baseline on a quiet machine, not a fix.** Half a day, needs David's laptop and a
real prod build, and it is what tells anyone whether the items below are worth doing at all. ⚠️ Take it before ranking
anything here against the old numbers.

## Memory: two open calls and a spike, all on the CLIP towers

Scope is settled and the cheap win is banked. What is left carries a question that memory alone can't answer, so each
one is a decision before it is a task. The measured basis for all three is
`crates/cmdr-index/src/media_index/clip/DETAILS.md` § "What holding the towers costs".

- **Drop a tower that has gone idle.** A tower nobody has asked anything of in N minutes could unload and reload later.
  **The product question now has its number**: a first typed query costs **677 ms** cold against 8-10 ms warm
  (`DETAILS.md` § "The query path"), and an idle-unload pays that on every reload rather than once per launch. So the
  call is whether 677 ms is acceptable latency for a user who searches, searches again an hour later, and gets charged
  twice. David's.
- **Reconsider `MLComputeUnits`.** Worth roughly 400 MB: `CPUOnly` and `CPUAndNeuralEngine` measure 11.8 MB against
  `All`'s ~410 MB, because the GPU path materializes every weight matrix instead of reading the mmap'd `weight.bin`. ❌
  Don't touch it on the memory number alone; `crates/cmdr-index/src/media_index/clip/CLAUDE.md` carries that as an
  invariant. Enrichment throughput on the non-GPU path is unmeasured, and measuring both sides is its own task.
- **Convert the text tower to fp16.** A spike with a quality question attached. `install.rs` records why 8-bit was
  rejected (the text tower's 8-bit Core ML inference comes out all-NaN); fp16 sits between the two and was never tried.
  It would roughly halve the 245.9 MB the text tower costs a user who searches.

## The rescan threshold is a guess waiting on a week

`HIGH_CARDINALITY_ANCHORS` is 256, and it was picked with no distribution behind it. It is positioned against the only
anchor-cardinality data the repo holds (David's machine, 2026-07-19..23, while running six cargo builds): 5,876 distinct
anchors across a sampled day, 1,595 in the worst single window.

**What settles it**: an ordinary week on a quiet machine, collecting the INFO line `churn.rs` already emits, into a
`docs/notes/` note. Then re-set the constant from the real distribution, which is a one-line change. Half a day once the
week has passed; the week is the cost. Everything needed to collect it shipped, and is also what the router reads.

## One question for David, gating nothing

**May the rescan walk read `SYSTEM_DIR_EXCLUDES`?** (`crates/cmdr-index/src/indexing/scanner/exclusions.rs`.) It is a
shipped name denylist holding `target`, `node_modules`, and `Caches`, already read by search, the importance scorer, and
the folder-size tooltip, so the question is whether the rescan walk may be a fourth consumer, not whether we may have a
denylist.

The docstring's carve-out says the SCANNER must never be one, because skipping at walk time stamps coverage on parents
whose `dir_stats` then come out short. ⚠️ That reason doesn't obviously transfer to RE-walking already-indexed ground,
where the aggregates exist and the cost of skipping is staleness rather than a wrong count. Which is what makes it a
real question. Nothing depends on the answer: the shipped router bounds a rate and never needs to know which folders.

## Two smaller calls this effort surfaced

- **`ImportanceIndex::above_threshold(0.0)`'s `ORDER BY` may be dead weight.** Its cached consumer builds a `HashMap`
  and never reads the order, and `crates/cmdr-index/src/media_index/scheduler/mod.rs` already warns against reading it
  directly for exactly that reason. But the public API's ordering is asserted by a test and other callers exist, so this
  is a decision rather than a cleanup. It matters slightly more than it looks, because `cache_size` also sets SQLite's
  sorter budget (`crates/cmdr-fs/src/sqlite_util.rs`).
- **`derive-default-justified` scans only the filesystem trees**
  (`scripts/check/checks/desktop-rust-derive-default-justified.go`), so a `Default` derive on an IPC DTO goes
  unchallenged while its `cmdr-fs` twin needs a `DEFAULT-OK` line. Possibly unintentional; worth one look.

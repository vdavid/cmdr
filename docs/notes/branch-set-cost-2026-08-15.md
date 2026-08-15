# What the branch set cost, and what it costs now

The branch set (`crates/cmdr-index/src/indexing/watch/branches.rs`) tracks which ground a walk has covered, so live
filesystem events can be filtered and buffered correctly. Phased drive indexing registers an entry per covered frontier
root, so a mid-phase set runs to thousands of entries where it used to hold a handful.

Three separate efforts named it as a suspect and none of them measured it. This is the measurement, taken with
`crates/cmdr-index/src/indexing/watch/branches/bench.rs` (`branch_set_cost`, `#[ignore]`d) on 2026-08-15, release build,
M-series laptop, over synthetic frontier roots eight components deep sharing a long common prefix, which is the shape a
resumed phased index leaves behind.

## Registration: one `begin_covering` plus the matching `finish_covering`

- 100 roots: 2.18 ms before, 398.7 µs after (5x)
- 500 roots: 32.62 ms before, 1.16 ms after (28x)
- 1,000 roots: 85.70 ms before, 2.28 ms after (38x)
- 2,500 roots: 490.8 ms before, 5.65 ms after (87x)
- 5,000 roots: 1.97 s before, 10.42 ms after (189x)

The before numbers grow as the square of the width: 21.8 µs a root at 100, 393 µs a root at 5,000. The after numbers are
flat at ~2.3 µs a root. `finish_covering` was the expensive half by an order of magnitude, because its "is a settled
branch already covering this?" question asked every entry in turn.

## Admission: one `admit` per event against a settled set

Events landing INSIDE the covered branches, per event:

- 100 branches: 7.00 µs before, 0.26 µs after (27x)
- 500 branches: 34.40 µs before, 0.32 µs after (108x)
- 1,000 branches: 69.42 µs before, 0.35 µs after (198x)
- 2,500 branches: 171.21 µs before, 0.38 µs after (451x)
- 5,000 branches: 345.71 µs before, 0.39 µs after (886x)

Events landing OUTSIDE every branch, which on a branch-watched volume is the common case (most of a drive is ground no
walk went near), per event:

- 100 branches: 14.04 µs before, 0.36 µs after (39x)
- 500 branches: 68.88 µs before, 0.42 µs after (164x)
- 1,000 branches: 135.54 µs before, 0.52 µs after (261x)
- 2,500 branches: 338.74 µs before, 0.51 µs after (664x)
- 5,000 branches: 676.91 µs before, 0.55 µs after (1231x)

The outside case was the WORSE of the two, at twice the inside cost, because it paid two full scans: `deepest_containing`
found nothing, and then the coalesced-sweep re-anchoring collected every strict descendant before checking whether the
event was a sweep at all. Both scans are gone. The sweep flag is checked first, and the descendant collection is a range
query.

Read the before numbers as held-lock time on the live event path: at 2,500 branches a 20,000-event churn burst cost
6.8 s of it, and now costs 10 ms.

## What changed

The set went from a `Vec<Branch>` that scanned itself (and re-sorted on every insert) to a `BTreeMap<String, Branch>`
keyed by path, with both of its questions bounded by the PATH rather than by the set:

- "what holds this?" walks the path's own ancestors (`self_and_ancestors` in
  `crates/cmdr-index/src/indexing/paths/path_prefix.rs`), a handful of lookups however many branches exist.
- "what sits under this?" is a range scan bounded by `descendant_range_prefix`, so it costs what it yields.

Sorted order comes free from the container, so `branch_paths` (the persisted form) needs no sort at all.

## What did NOT reproduce

A report from the search work put 3.0 s on `Index::cover` for 2,503 frontier roots and attributed it to
`state::begin_branch_coverage` "registering them one at a time under the registry lock". Registration is already
batched: one `begin_branch_coverage` per group, taking the whole slice. The complete register-plus-release cycle at that
width measured 0.49 s, of which `begin_covering` was 31 ms. The set was genuinely costing that half-second across a
phase, plus the admission tax above, but the 3.0 s figure is not the branch set, and something else on that path still
owns it.
